//! Vehicles on screen and in the ears (crate::vehicle_feel for how they move): the models drawn
//! between ticks, the driver seated and posed, the camera swinging round behind, engine and horn,
//! the health panel and the get-in prompt, smoke from a hurt vehicle, a wreck's blast and fire.

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::audio::Volume;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::world_serialization::WorldInstanceReady;
use rand::RngExt;
use shared::map::MapLayout;
use shared::map::terrain::ground;
use shared::sim::vehicle::{CrashMsg, Riding, Vehicle, VehicleKind, WreckMsg, in_reach};
use shared::sim::{Eliminated, Kinematic, LocalPlayer, MoveMode, PlayerIntent};

use crate::arms::Body;
use crate::camera::{CameraSystems, MainCamera};
use crate::front::Screen;
use crate::fx::CameraPunch;
use crate::input::LocalInput;
use crate::vehicle_feel::{Feel, engine as engine_note};
use crate::visuals::{WarriorVisual, WeaponSocket};

pub struct VehiclesPlugin;

impl Plugin for VehiclesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoardPrompt>()
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, swing_camera.after(LocalInput))
            .add_systems(Update, (dress, clear_orphans, board_prompt, panel, engine, passing, crashes, smoke, fly_puffs, wrecks, fade_blasts))
            .add_systems(Last, count_drawn.run_if(|| std::env::var("VIVAD_PERF_LOG").is_ok()))
            .add_systems(Update, horn.run_if(in_state(Screen::Match)))
            .add_systems(PostUpdate, (place, seat_riders).chain().before(CameraSystems))
            .add_systems(PostUpdate, pose_riders.after(bevy::app::AnimationSystems).before(TransformSystems::Propagate));
    }
}

/// The key's meaning at the wheel (in `crate::hud`'s prompt).
pub const GET_OUT: &str = "[E] Utro";

/// What the interact key would get into here, for `crate::hud`'s prompt.
#[derive(Resource, Default)]
pub struct BoardPrompt(pub Option<String>);

/// How long a wreck burns, then smoulders, seconds.
const BURN: f32 = 14.0;
const SMOULDER: f32 = 40.0;
/// Dust: the speed that raises it, the puffs a second flat out, and how far off it is worth a card.
const DUST_SPEED: f32 = 5.0;
const DUST_RATE: f32 = 24.0;
const DUST_SEEN: f32 = 150.0;
/// How far off another's engine is heard, metres, how many at a time, and how loud at its nearest.
const HEARD_TO: f32 = 160.0;
const PASSING: usize = 3;
const PASSING_LOUD: f32 = 0.6;

#[derive(Resource)]
struct Kit {
    engines: [Handle<AudioSource>; 3],
    horns: [Handle<AudioSource>; 3],
    crash: Handle<AudioSource>,
    scrape: Handle<AudioSource>,
    blast: Handle<AudioSource>,
    card: Handle<Mesh>,
    /// Fading steps of grey smoke, of black smoke, of flame, and of dust.
    grey: Vec<Handle<StandardMaterial>>,
    black: Vec<Handle<StandardMaterial>>,
    flame: Vec<Handle<StandardMaterial>>,
    dust: Vec<Handle<StandardMaterial>>,
}

impl Kit {
    /// A puff's fading steps, by `Puff::steps`.
    fn steps(&self, steps: u8) -> &[Handle<StandardMaterial>] {
        match steps {
            0 => &self.grey,
            1 => &self.black,
            2 => &self.flame,
            _ => &self.dust,
        }
    }
}

/// The engine recording each kind plays, and at what rate: tools/audio/vehicles.py has three.
fn engine_of(kind: VehicleKind) -> (usize, f32) {
    match kind {
        VehicleKind::Auto => (0, 1.0),
        VehicleKind::Scooter => (0, 1.35),
        VehicleKind::Commuter => (1, 1.15),
        VehicleKind::Bullet => (2, 1.0),
        VehicleKind::Car => (1, 0.8),
        VehicleKind::Khuli => (2, 0.85),
        VehicleKind::Sarkari => (1, 0.7),
        VehicleKind::ChhotaTruck => (2, 0.72),
        VehicleKind::Tractor => (2, 0.55),
    }
}

fn horn_of(kind: VehicleKind) -> usize {
    match kind {
        VehicleKind::Auto | VehicleKind::Tractor => 0,
        VehicleKind::Scooter | VehicleKind::Commuter | VehicleKind::Bullet => 1,
        VehicleKind::Car | VehicleKind::Khuli | VehicleKind::Sarkari | VehicleKind::ChhotaTruck => 2,
    }
}

const STEPS: usize = 6;

/// A soft blob with a torn edge, for smoke and flame cards.
fn blob(images: &mut Assets<Image>) -> Handle<Image> {
    let n = 64usize;
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / (n - 1) as f32 * 2.0 - 1.0, y as f32 / (n - 1) as f32 * 2.0 - 1.0);
            let a = v.atan2(u);
            let edge = 0.78 + 0.09 * (a * 3.0).sin() + 0.06 * (a * 7.0 + 1.3).sin() + 0.04 * (a * 13.0).cos();
            let r = (u * u + v * v).sqrt() / edge;
            let alpha = (1.0 - r * r).clamp(0.0, 1.0).powf(1.4);
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    images.add(Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let tex = blob(&mut images);
    let mut fade = |rgb: (f32, f32, f32), alpha: f32, add: bool| -> Vec<Handle<StandardMaterial>> {
        (0..STEPS)
            .map(|i| {
                let a = alpha * (1.0 - i as f32 / STEPS as f32);
                materials.add(StandardMaterial {
                    base_color: Color::srgba(rgb.0, rgb.1, rgb.2, a),
                    base_color_texture: Some(tex.clone()),
                    alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Blend },
                    unlit: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .collect()
    };
    let (grey, black, flame) = (fade((0.62, 0.61, 0.6), 0.5, false), fade((0.07, 0.065, 0.06), 0.8, false), fade((1.0, 0.45, 0.1), 0.95, true));
    // Paler than the ground it rises from: the colour of the sand itself doesn't show against it.
    let dust = fade((0.90, 0.84, 0.72), 0.5, false);
    commands.insert_resource(Kit {
        engines: ["engine_auto", "engine_bike", "engine_thump"].map(|n| assets.load(format!("audio/{n}.ogg"))),
        horns: ["horn_auto", "horn_bike", "horn_car"].map(|n| assets.load(format!("audio/{n}.ogg"))),
        crash: assets.load("audio/sword_hit_stone.ogg"),
        scrape: assets.load("audio/bullet_impact_stone.ogg"),
        blast: assets.load("audio/brahmastra_blast.ogg"),
        card: meshes.add(Rectangle::new(1.0, 1.0)),
        grey,
        black,
        flame,
        dust,
    });
    if std::env::var("VIVAD_CLEAN_CAPTURE").is_err() {
        commands
            .spawn((
                Panel,
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50.0),
                    margin: UiRect::left(px(-150.0)),
                    bottom: px(40.0),
                    width: px(300.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5.0),
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|p| {
                p.spawn((PanelText, Text::new(""), TextFont { font_size: FontSize::Px(18.0), ..default() }, TextColor(Color::WHITE)));
                p.spawn((Node { width: percent(100.0), height: px(12.0), ..default() }, BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.55)))).with_children(|b| {
                    b.spawn((PanelBar, Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(Color::srgb(0.35, 0.85, 0.35))));
                });
            });
    }
}

// ------------------------------------------------------------------ the model

/// The drawn vehicle: its root follows the sim between ticks, a child on the ground under it leans.
#[derive(Component)]
pub struct VehicleVisual {
    pub sim: Entity,
    pub feel: Feel,
    pivot: Entity,
    /// Seconds of smoke owed, and puffs of dust.
    puff: f32,
    dust: f32,
    /// How long ago it blew.
    burnt: Option<f32>,
}

/// One of a vehicle's meshes, and whose it is.
#[derive(Component)]
struct VehiclePart(Entity);

/// With VIVAD_PERF_LOG, how many of the match's vehicles are on screen, every 10 s.
fn count_drawn(time: Res<Time>, mut next: Local<f32>, parts: Query<(&VehiclePart, &ViewVisibility)>, vehicles: Query<(), With<Vehicle>>) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 10.0;
    let mut drawn: Vec<Entity> = parts.iter().filter(|(_, v)| v.get()).map(|(p, _)| p.0).collect();
    drawn.sort();
    drawn.dedup();
    info!("vehicles drawn: {} of {}", drawn.len(), vehicles.iter().count());
}

#[derive(Component)]
struct Wheel {
    rest: Quat,
    steers: bool,
    radius: f32,
    /// Its turn, radians: each wheel's own, as a tractor's two sizes run at two rates.
    spin: f32,
}

fn dress(mut commands: Commands, q: Query<(Entity, &Vehicle, &Transform), Added<Vehicle>>, assets: Res<AssetServer>) {
    for (e, v, t) in &q {
        let spec = v.kind.spec();
        let pivot = commands.spawn((Transform::from_xyz(0.0, -spec.centre(), 0.0), Visibility::default())).id();
        commands.spawn((VehicleVisual { sim: e, feel: Feel::default(), pivot, puff: 0.0, dust: 0.0, burnt: None }, *t, Visibility::default())).add_child(pivot);
        let steers = v.kind.steers();
        let (front, rear) = (spec.wheel, spec.rear);
        commands
            .spawn((WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(spec.model))), Transform::default(), ChildOf(pivot)))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, tf: Query<&Transform>, parents: Query<&ChildOf>, drawn: Query<(), With<Mesh3d>>, mut commands: Commands| {
                let mut wheels = 0;
                let wheel = |e: Entity| names.get(e).is_ok_and(|n| n.starts_with("wheel_"));
                for d in children.iter_descendants(ready.entity) {
                    if drawn.contains(d) {
                        commands.entity(d).insert(VehiclePart(e));
                    }
                    let Ok(t) = tf.get(d) else { continue };
                    // The node, not the mesh parts under it, which share its name.
                    if wheel(d) && !parents.get(d).is_ok_and(|p| wheel(p.parent())) {
                        let steering = names.get(d).is_ok_and(|n| n.starts_with("wheel_f"));
                        commands.entity(d).insert(Wheel { rest: t.rotation, steers: steers && steering, radius: if steering { front } else { rear }, spin: 0.0 });
                        wheels += 1;
                    }
                }
                debug!("vehicle ready: {wheels} wheels");
            });
    }
}

fn clear_orphans(mut commands: Commands, vis: Query<(Entity, &VehicleVisual)>, sims: Query<(), With<Vehicle>>) {
    for (e, v) in &vis {
        if !sims.contains(v.sim) {
            commands.entity(e).despawn();
        }
    }
}

/// Between the last two ticks, with the lean, the nose and the wheels eased over them.
#[allow(clippy::type_complexity)]
fn place(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sims: Query<&Vehicle>,
    mut vis: Query<(&mut VehicleVisual, &mut Transform, &Children)>,
    mut parts: Query<&mut Transform, Without<VehicleVisual>>,
    kids: Query<&Children>,
    mut wheels: Query<&mut Wheel>,
) {
    let (dt, a) = (time.delta_secs(), fixed.overstep_fraction());
    for (mut vv, mut t, _) in &mut vis {
        let Ok(v) = sims.get(vv.sim) else { continue };
        t.translation = v.before.0.lerp(v.now.0, a);
        t.rotation = v.before.1.slerp(v.now.1, a);
        let spec = v.kind.spec();
        let steer = if v.wrecked { 0.0 } else { v.input.steer };
        vv.feel.update(v.speed, steer, spec.law.top, spec.lean, dt);
        let Ok(mut pivot) = parts.get_mut(vv.pivot) else { continue };
        pivot.rotation = vv.feel.body();
        for d in kids.iter_descendants(vv.pivot) {
            let Ok(mut w) = wheels.get_mut(d) else { continue };
            let Ok(mut wt) = parts.get_mut(d) else { continue };
            w.spin = (w.spin - v.speed / w.radius * dt) % TAU;
            wt.rotation = if w.steers { Quat::from_rotation_y(vv.feel.steer) * w.rest } else { w.rest } * Quat::from_rotation_x(w.spin);
        }
    }
}

// ------------------------------------------------------------------ the driver

/// A driver sits on the drawn vehicle, leaning with it, weapon put away.
fn seat_riders(
    riders: Query<&Riding>,
    sims: Query<&Vehicle>,
    vehicles: Query<(&VehicleVisual, &Transform), Without<WarriorVisual>>,
    mut bodies: Query<(&WarriorVisual, &WeaponSocket, &mut Transform), Without<VehicleVisual>>,
    mut sockets: Query<&mut Visibility>,
) {
    for (wv, socket, mut t) in &mut bodies {
        let ride = riders.get(wv.sim).ok();
        if let Ok(mut vis) = sockets.get_mut(socket.0) {
            vis.set_if_neq(if ride.is_some() { Visibility::Hidden } else { Visibility::Inherited });
        }
        let Some(ride) = ride else { continue };
        let Some((vv, vt)) = vehicles.iter().find(|(vv, _)| vv.sim == ride.0) else { continue };
        let Ok(v) = sims.get(ride.0) else { continue };
        let spec = v.kind.spec();
        let body = vv.feel.body();
        // An auto's canopy is low: from the sim's seat the head came through it and the hips hung
        // over the cushion. Only the drawn body sits down; where the warrior is doesn't change.
        let sunk = if v.kind == VehicleKind::Auto { AUTO_SEAT_SINK } else { 0.0 };
        t.translation = vt.translation + vt.rotation * (Vec3::NEG_Y * spec.centre() + body * (spec.seat - Vec3::Y * sunk));
        t.rotation = vt.rotation * body;
    }
}

/// How far a drawn auto driver sits below the sim's seat, metres.
const AUTO_SEAT_SINK: f32 = 0.09;

/// Bones a seated pose bends.
#[derive(Component)]
struct Limbs {
    thigh: [Entity; 2],
    calf: [Entity; 2],
    upper: [Entity; 2],
    lower: [Entity; 2],
}

/// (Thigh raised, knee bent, arms forward, elbows, back forward), radians.
fn pose_of(kind: VehicleKind) -> (f32, f32, f32, f32, f32) {
    match kind {
        VehicleKind::Auto => (1.45, 1.5, 0.6, 0.6, 0.38),
        VehicleKind::Scooter => (1.4, 1.45, 0.55, 0.6, 0.05),
        VehicleKind::Commuter => (1.2, 1.2, 0.75, 0.45, 0.28),
        VehicleKind::Bullet => (1.1, 1.1, 0.62, 0.45, 0.12),
        VehicleKind::Car | VehicleKind::Sarkari => (1.5, 1.35, 0.8, 0.7, -0.05),
        VehicleKind::Khuli => (1.4, 1.3, 0.8, 0.65, 0.0),
        VehicleKind::ChhotaTruck => (1.35, 1.3, 0.8, 0.7, 0.0),
        VehicleKind::Tractor => (1.15, 1.2, 0.75, 0.5, 0.22),
    }
}

/// Turn a bone by `by`, a turn in the character's own space (+Z forward, +X its left), after the clips.
fn turn(bone: Entity, by: Quat, root: Entity, parents: &Query<&ChildOf>, tf: &mut Query<&mut Transform, Without<WarriorVisual>>) {
    let mut parent = Quat::IDENTITY;
    let mut cur = parents.get(bone).map(|p| p.parent());
    while let Ok(c) = cur {
        if c == root {
            break;
        }
        parent = tf.get(c).map_or(Quat::IDENTITY, |t| t.rotation) * parent;
        cur = parents.get(c).map(|p| p.parent());
    }
    if let Ok(mut t) = tf.get_mut(bone) {
        t.rotation = parent.inverse() * by * parent * t.rotation;
    }
}

/// There is no seated clip: the idle clip's legs and arms are bent to a seat and a handlebar.
#[allow(clippy::type_complexity)]
fn pose_riders(
    mut commands: Commands,
    vis: Query<(Entity, &WarriorVisual, &Body, Option<&Limbs>)>,
    riders: Query<&Riding, Without<Eliminated>>,
    sims: Query<&Vehicle>,
    kids: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    mut tf: Query<&mut Transform, Without<WarriorVisual>>,
) {
    for (e, wv, body, limbs) in &vis {
        let Ok(ride) = riders.get(wv.sim) else { continue };
        let Ok(v) = sims.get(ride.0) else { continue };
        let Some(limbs) = limbs else {
            let find = |n: &str| kids.iter_descendants(body.root).find(|d| names.get(*d).is_ok_and(|x| x.as_str() == n));
            let pair = |n: &str| Some([find(&format!("{n}_l"))?, find(&format!("{n}_r"))?]);
            if let (Some(thigh), Some(calf), Some(upper), Some(lower)) = (pair("thigh"), pair("calf"), pair("upperarm"), pair("lowerarm")) {
                commands.entity(e).insert(Limbs { thigh, calf, upper, lower });
            }
            continue;
        };
        let (thigh, knee, arm, elbow, back) = pose_of(v.kind);
        turn(body.spine[0], Quat::from_rotation_x(back), body.root, &parents, &mut tf);
        for i in 0..2 {
            turn(limbs.thigh[i], Quat::from_rotation_x(-thigh), body.root, &parents, &mut tf);
            turn(limbs.calf[i], Quat::from_rotation_x(knee), body.root, &parents, &mut tf);
            turn(limbs.upper[i], Quat::from_rotation_x(-arm), body.root, &parents, &mut tf);
            turn(limbs.lower[i], Quat::from_rotation_x(-elbow), body.root, &parents, &mut tf);
        }
    }
}

/// Moving, the view swings round behind the vehicle, unless the mouse has just moved it.
fn swing_camera(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    mut idle: Local<f32>,
    mut me: Query<(&mut PlayerIntent, &Riding), With<LocalPlayer>>,
    vehicles: Query<&Vehicle>,
) {
    let dt = time.delta_secs();
    *idle = if motion.delta.length_squared() > 0.5 { 0.0 } else { *idle + dt };
    let Ok((mut intent, ride)) = me.single_mut() else { return };
    let Ok(v) = vehicles.get(ride.0) else { return };
    if *idle < 1.0 || v.speed.abs() < 1.5 {
        return;
    }
    let k = 1.0 - (-dt * 3.0).exp();
    let off = (v.heading - intent.yaw + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
    intent.yaw += off * k;
    intent.pitch += (-0.17 - intent.pitch) * k;
}

// ------------------------------------------------------------------ prompt and panel

fn board_prompt(me: Query<(&Transform, &Kinematic), With<LocalPlayer>>, vehicles: Query<(&Transform, &Vehicle)>, mut prompt: ResMut<BoardPrompt>) {
    prompt.0 = None;
    let Ok((tf, kin)) = me.single() else { return };
    if kin.mode != MoveMode::Ground || !kin.grounded {
        return;
    }
    let near = vehicles
        .iter()
        .filter(|(t, v)| v.driver.is_none() && !v.wrecked && in_reach(tf.translation, t, v))
        .min_by(|a, b| a.0.translation.distance(tf.translation).total_cmp(&b.0.translation.distance(tf.translation)));
    prompt.0 = near.map(|(_, v)| format!("[E] {} chalao", v.kind.spec().name));
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct PanelText;
#[derive(Component)]
struct PanelBar;

fn panel(
    me: Query<&Riding, With<LocalPlayer>>,
    vehicles: Query<&Vehicle>,
    mut root: Query<&mut Visibility, With<Panel>>,
    mut text: Query<&mut Text, With<PanelText>>,
    mut bar: Query<(&mut Node, &mut BackgroundColor), With<PanelBar>>,
) {
    let ride = me.single().ok().and_then(|r| vehicles.get(r.0).ok());
    for mut v in &mut root {
        v.set_if_neq(if ride.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    let Some(v) = ride else { return };
    let spec = v.kind.spec();
    let left = (v.hp / spec.hp).clamp(0.0, 1.0);
    for mut t in &mut text {
        let line = format!("{}   {:.0} km/h", spec.name, v.speed.abs() * 3.6);
        if t.0 != line {
            t.0 = line;
        }
    }
    for (mut n, mut c) in &mut bar {
        n.width = percent(100.0 * left);
        c.0 = if left > 0.55 { Color::srgb(0.35, 0.85, 0.35) } else if left > 0.25 { Color::srgb(0.95, 0.7, 0.2) } else { Color::srgb(0.9, 0.25, 0.15) };
    }
}

// ------------------------------------------------------------------ sound

#[derive(Component)]
struct EngineSound(VehicleKind);

/// The local driver's engine, its rate and volume following speed and throttle.
fn engine(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<Kit>,
    me: Query<&Riding, With<LocalPlayer>>,
    vehicles: Query<&Vehicle>,
    mut running: Query<(Entity, &EngineSound, Option<&mut AudioSink>)>,
    mut note: Local<(f32, f32)>,
    mut told: Local<f32>,
) {
    let ride = me.single().ok().and_then(|r| vehicles.get(r.0).ok()).filter(|v| !v.wrecked);
    for (e, sound, sink) in &mut running {
        let Some(v) = ride.filter(|v| v.kind == sound.0) else {
            commands.entity(e).despawn();
            continue;
        };
        let Some(mut sink) = sink else { continue };
        let (rate, volume) = engine_note((v.speed.abs() / v.kind.spec().law.top).min(1.0), v.input.throttle.max(v.input.brake));
        let dt = time.delta_secs();
        note.0 += (rate - note.0) * (dt * 2.5).min(1.0);
        note.1 += (volume - note.1) * (dt * 5.0).min(1.0);
        sink.set_speed(note.0 * engine_of(v.kind).1);
        sink.set_volume(Volume::Linear(note.1));
        if *told + 4.0 < time.elapsed_secs() && std::env::var("VIVAD_AUDIO_LOG").is_ok() {
            *told = time.elapsed_secs();
            info!("engine {:?}: {:.1} m/s, rate {:.2}, volume {:.2}", v.kind, v.speed, sink.speed(), note.1);
        }
    }
    if let Some(v) = ride {
        if running.is_empty() {
            *note = (0.8, 0.2);
            commands.spawn((EngineSound(v.kind), AudioPlayer(kit.engines[engine_of(v.kind).0].clone()), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.2))));
        }
    }
}

/// An engine that isn't the local driver's.
#[derive(Component)]
struct PassingEngine(Entity);

/// The nearest vehicles driven by somebody else, each with its own engine, fading with distance:
/// you hear them coming before you see them.
fn passing(
    mut commands: Commands,
    kit: Res<Kit>,
    me: Query<(&Transform, Option<&Riding>), With<LocalPlayer>>,
    vehicles: Query<(Entity, &Transform, &Vehicle)>,
    mut running: Query<(Entity, &PassingEngine, Option<&mut AudioSink>)>,
) {
    let Ok((ear, ride)) = me.single() else { return };
    let mut near: Vec<(f32, Entity, &Vehicle)> = vehicles
        .iter()
        .filter(|(e, _, v)| v.driver.is_some() && !v.wrecked && ride.is_none_or(|r| r.0 != *e))
        .map(|(e, t, v)| (t.translation.distance(ear.translation), e, v))
        .filter(|n| n.0 < HEARD_TO)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.truncate(PASSING);
    for (e, sound, sink) in &mut running {
        let Some(n) = near.iter().find(|n| n.1 == sound.0) else {
            commands.entity(e).despawn();
            continue;
        };
        let Some(mut sink) = sink else { continue };
        let (rate, _) = engine_note((n.2.speed.abs() / n.2.kind.spec().law.top).min(1.0), n.2.input.throttle);
        let fade = 1.0 - n.0 / HEARD_TO;
        sink.set_speed(rate * engine_of(n.2.kind).1);
        sink.set_volume(Volume::Linear(PASSING_LOUD * fade * fade));
    }
    for n in &near {
        if !running.iter().any(|(_, sound, _)| sound.0 == n.1) {
            if std::env::var("VIVAD_AUDIO_LOG").is_ok() {
                info!("engine passing: {:?} heard at {:.0} m", n.2.kind, n.0);
            }
            commands.spawn((PassingEngine(n.1), AudioPlayer(kit.engines[engine_of(n.2.kind).0].clone()), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0))));
        }
    }
}

fn horn(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, kit: Res<Kit>, me: Query<&Riding, With<LocalPlayer>>, vehicles: Query<&Vehicle>) {
    if !keys.just_pressed(KeyCode::KeyH) {
        return;
    }
    let Some(v) = me.single().ok().and_then(|r| vehicles.get(r.0).ok()).filter(|v| !v.wrecked) else { return };
    info!("horn: {:?}", v.kind);
    commands.spawn((AudioPlayer(kit.horns[horn_of(v.kind)].clone()), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.7))));
}

fn crashes(mut commands: Commands, mut msgs: MessageReader<CrashMsg>, kit: Res<Kit>, me: Query<&Transform, With<LocalPlayer>>, mut punch: ResMut<CameraPunch>) {
    let ear = me.single().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    for c in msgs.read() {
        let near = (1.0 - ear.distance(c.at) / 40.0).max(0.0);
        if near == 0.0 {
            continue;
        }
        let k = (c.force / 14.0).min(1.0);
        punch.shake = punch.shake.max(0.12 * k * near);
        for (sound, rate) in [(&kit.crash, 0.55), (&kit.scrape, 0.7)] {
            commands.spawn((AudioPlayer(sound.clone()), PlaybackSettings::DESPAWN.with_speed(rate).with_volume(Volume::Linear((0.35 + 0.6 * k) * near))));
        }
    }
}

// ------------------------------------------------------------------ smoke and fire

/// A card of smoke or flame, always facing the camera.
#[derive(Component)]
struct Puff {
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    grow: f32,
    spin: f32,
    steps: u8,
}

fn puff(commands: &mut Commands, kit: &Kit, steps: u8, at: Vec3, vel: Vec3, life: f32, size: f32, grow: f32) {
    commands.spawn((
        Puff { vel, age: 0.0, life, size, grow, spin: rand::rng().random_range(-1.0..1.0), steps },
        Mesh3d(kit.card.clone()),
        MeshMaterial3d(kit.steps(steps)[0].clone()),
        Transform::from_translation(at).with_scale(Vec3::splat(size)),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
    ));
}

/// Grey smoke under 55% health, black under 25%; a wreck burns, then smoulders. Moving over bare
/// ground, dust behind the wheels.
fn smoke(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<Kit>,
    layout: Res<MapLayout>,
    cam: Query<&Transform, With<MainCamera>>,
    sims: Query<(&Vehicle, &Transform)>,
    mut vis: Query<&mut VehicleVisual>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    let eye = cam.single().map_or(Vec3::ZERO, |c| c.translation);
    for mut vv in &mut vis {
        let Ok((v, t)) = sims.get(vv.sim) else { continue };
        let spec = v.kind.spec();
        let left = v.hp / spec.hp;
        let foot = t.translation - Vec3::Y * spec.centre();
        let bare = matches!(layout.terrain.ground_at(foot.x, foot.z), Some(ground::OPEN | ground::SAND | ground::PATH));
        if bare && v.speed.abs() > DUST_SPEED && foot.distance(eye) < DUST_SEEN && foot.y - layout.terrain.height(foot.x, foot.z) < 0.6 {
            vv.dust += dt * DUST_RATE * (v.speed.abs() / spec.law.top).min(1.0);
            while vv.dust >= 1.0 {
                vv.dust -= 1.0;
                let side = rng.random_range(-0.5..0.5) * spec.size.x;
                let at = foot + t.rotation * Vec3::new(side, 0.2, spec.size.z * 0.42);
                let vel = t.rotation * Vec3::Z * v.speed * 0.12 + Vec3::new(rng.random_range(-0.4..0.4), rng.random_range(0.3..0.9), rng.random_range(-0.4..0.4));
                puff(&mut commands, &kit, 3, at, vel, rng.random_range(1.4..2.6), 0.8, 2.2);
            }
        }
        let at = t.translation + t.rotation * (spec.engine - Vec3::Y * spec.centre());
        let drift = t.rotation * Vec3::Z * v.speed * 0.4;
        if let Some(b) = vv.burnt.as_mut() {
            *b += dt;
        }
        let (rate, black, flames) = match vv.burnt {
            Some(b) if b < BURN => (26.0, true, true),
            Some(b) if b < SMOULDER => (7.0, true, false),
            Some(_) => continue,
            None if left < 0.25 => (20.0, true, false),
            None if left < 0.55 => (10.0, false, false),
            None => continue,
        };
        vv.puff += dt * rate;
        while vv.puff >= 1.0 {
            vv.puff -= 1.0;
            let r = Vec3::new(rng.random_range(-0.5..0.5), rng.random_range(0.9..1.5), rng.random_range(-0.5..0.5));
            puff(&mut commands, &kit, black as u8, at + r * 0.15, r * if black { 1.6 } else { 1.1 } + drift, rng.random_range(1.3..2.4), 0.45, 1.5);
            if flames {
                let up = t.rotation * Vec3::new(rng.random_range(-0.4..0.4), 0.2, rng.random_range(-0.7..0.7)) * spec.size.z.min(1.6);
                puff(&mut commands, &kit, 2, at + up, Vec3::Y * rng.random_range(1.2..2.6), rng.random_range(0.35..0.7), 0.7, 0.4);
            }
        }
    }
}

fn fly_puffs(mut commands: Commands, time: Res<Time>, kit: Res<Kit>, cam: Query<&Transform, With<MainCamera>>, mut q: Query<(Entity, &mut Puff, &mut Transform, &mut MeshMaterial3d<StandardMaterial>), Without<MainCamera>>) {
    let dt = time.delta_secs();
    let Ok(cam) = cam.single() else { return };
    for (e, mut p, mut t, mut m) in &mut q {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        t.translation += p.vel * dt;
        p.vel *= 1.0 - 0.6 * dt;
        p.size += p.grow * dt;
        let k = ((p.age / p.life) * STEPS as f32) as usize;
        let set = kit.steps(p.steps);
        if m.0 != set[k.min(STEPS - 1)] {
            m.0 = set[k.min(STEPS - 1)].clone();
        }
        // Coming up from nothing, not popping in.
        let s = p.size * (p.age * 8.0).min(1.0);
        t.scale = Vec3::splat(s);
        t.rotation = cam.rotation * Quat::from_rotation_z(p.spin + p.age * p.spin);
    }
}

/// A wreck: the blast, a burnt shell where the model was.
#[allow(clippy::too_many_arguments)]
fn wrecks(
    mut commands: Commands,
    mut msgs: MessageReader<WreckMsg>,
    kit: Res<Kit>,
    mut vis: Query<(Entity, &mut VehicleVisual)>,
    kids: Query<&Children>,
    meshes: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut punch: ResMut<CameraPunch>,
    me: Query<&Transform, With<LocalPlayer>>,
) {
    let mut rng = rand::rng();
    for w in msgs.read() {
        let Some((e, mut vv)) = vis.iter_mut().find(|(_, v)| v.sim == w.vehicle) else { continue };
        vv.burnt = Some(0.0);
        for d in kids.iter_descendants(e) {
            let Ok(m) = meshes.get(d) else { continue };
            let Some(mut burnt) = materials.get(&m.0).cloned() else { continue };
            burnt.base_color = burnt.base_color.to_linear().with_alpha(1.0).into();
            let c = burnt.base_color.to_linear();
            burnt.base_color = Color::linear_rgb(c.red * 0.05 + 0.006, c.green * 0.05 + 0.005, c.blue * 0.05 + 0.005);
            burnt.alpha_mode = AlphaMode::Opaque;
            burnt.perceptual_roughness = 0.95;
            burnt.metallic = 0.0;
            commands.entity(d).insert(MeshMaterial3d(materials.add(burnt)));
        }
        commands.spawn((AudioPlayer(kit.blast.clone()), PlaybackSettings::DESPAWN.with_speed(1.3).with_volume(Volume::Linear(0.8))));
        for _ in 0..28 {
            let r = Vec3::new(rng.random_range(-1.0..1.0), rng.random_range(0.2..1.0), rng.random_range(-1.0..1.0));
            puff(&mut commands, &kit, 2, w.at + r * 0.5, r * 6.0, rng.random_range(0.3..0.7), 1.0, 2.0);
            puff(&mut commands, &kit, 1, w.at + r * 0.5, r * 3.5, rng.random_range(1.2..2.6), 0.9, 1.8);
        }
        commands.spawn((
            Blast(0.35),
            PointLight { color: Color::srgb(1.0, 0.6, 0.25), intensity: 4.0e6, range: 28.0, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(w.at + Vec3::Y),
        ));
        if let Ok(t) = me.single() {
            punch.shake = punch.shake.max(0.3 * (1.0 - t.translation.distance(w.at) / 40.0).max(0.0));
        }
    }
}

/// A wreck's flash, burning out: the seconds it has left.
#[derive(Component)]
struct Blast(f32);

fn fade_blasts(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Blast, &mut PointLight)>) {
    for (e, mut b, mut light) in &mut q {
        b.0 -= time.delta_secs();
        if b.0 <= 0.0 {
            commands.entity(e).despawn();
        } else {
            light.intensity *= 1.0 - (time.delta_secs() * 5.0).min(1.0);
        }
    }
}
