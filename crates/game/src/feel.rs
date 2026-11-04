//! How movement feels, by the numbers: the local player's motion traced frame by frame, for
//! `tools/feel/feel.py` to measure (response, stops, landings, foot slide, lean).
//!
//! - Trace: VIVAD_TRACE=<file.csv> writes every frame of the match: input, sim velocity, the drawn
//!   body's place and facing (t is match time, which steps with the sim's ticks; rt the frame's
//!   own clock, for rates), the clips playing and their weights, and the world position of the
//!   bones in `TRACE_BONES`.
//! - Mark: F9 at any time saves the last `MARK_SECONDS` of the same trace and a screenshot to
//!   marks/ ("this felt wrong here").
//! - Scenes: VIVAD_FEEL="name:t:item,..." stands the player on open ground with a long clear run
//!   ahead at each match time t, still, holding that item (or the bat), the scene's name in the
//!   trace; "end:t:" quits. The keys come from VIVAD_KEYS (`just feelcheck` writes both).
//! - Tunables: assets/data/feel.ron (blend rate, turning, lean), reloaded while the game runs.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write as _;

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use serde::Deserialize;
use shared::items::{ItemDb, ItemKind, ItemStack, Rarity};
use shared::map::MapLayout;
use shared::sim::flow::Match;
use shared::sim::{Action, Health, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, PosHistory, Warrior, HALF_HEIGHT};

use crate::animation::{AnimSets, WarriorAnim};
use crate::visuals::WarriorVisual;

/// Bones traced, in this order (each x, y, z in the CSV).
pub const TRACE_BONES: [&str; 18] = [
    "pelvis", "spine_03", "neck_01", "head", "thigh_l", "calf_l", "foot_l", "ball_l", "thigh_r", "calf_r", "foot_r", "ball_r",
    "upperarm_l", "lowerarm_l", "hand_l", "upperarm_r", "lowerarm_r", "hand_r",
];
/// How much a mark keeps.
const MARK_SECONDS: f32 = 4.0;

/// The local player's traced bones (`TRACE_BONES`), found when its rig loads.
#[derive(Component)]
pub struct TraceBones(pub Vec<Entity>);

/// Movement feel tunables (assets/data/feel.ron).
#[derive(Resource, Deserialize, Clone, Copy, Debug)]
pub struct Feel {
    /// How fast clip weights move to their targets: every blend eases in and out, 90% done in
    /// about 3.9 / blend_rate seconds (`crate::animation::ease`).
    pub blend_rate: f32,
    /// How fast the legs' own speed eases down when the body slows (per second): a stop's
    /// strides run out over about 2.3 / stop_ease seconds.
    pub stop_ease: f32,
    /// How fast the drawn body turns to its facing (per second), fighting and not.
    pub turn_rate_fight: f32,
    pub turn_rate: f32,
    /// Lean into turns: radians per (rad/s of turn x m/s of speed), and the most.
    pub lean_gain: f32,
    pub lean_max: f32,
}

#[cfg(not(target_arch = "wasm32"))]
fn feel_path() -> String {
    format!("{}/data/feel.ron", crate::ASSET_DIR)
}

impl Default for Feel {
    fn default() -> Self {
        ron::from_str(include_str!("../../../assets/data/feel.ron")).expect("assets/data/feel.ron")
    }
}

pub struct FeelPlugin;

impl Plugin for FeelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Feel>().init_resource::<Trace>().add_systems(Last, (trace, mark, reload_feel));
        if let Ok(list) = std::env::var("VIVAD_FEEL") {
            let scenes = list
                .split(',')
                .filter_map(|s| {
                    let mut p = s.split(':');
                    let (name, t, item) = (p.next()?.trim(), p.next()?.trim().parse().ok()?, p.next().unwrap_or("").trim());
                    Some(Scene { name: name.into(), at: t, item: item.into() })
                })
                .collect();
            app.insert_resource(Scenes { list: scenes, index: None, spot: None, title: None })
                .add_systems(PreUpdate, stage_scenes.after(crate::input::LocalInput));
        }
    }
}

#[derive(Default, Resource)]
struct Trace {
    /// The last `MARK_SECONDS` of rows: (match time, row).
    ring: VecDeque<(f32, String)>,
    file: Option<std::io::BufWriter<std::fs::File>>,
    opened: bool,
    since_flush: f32,
}

fn header() -> String {
    let mut h = String::from("t,rt,dt,scene,keys,move_x,move_y,yaw,pitch,held,mode,grounded,crouched,sliding,vx,vy,vz,px,py,pz,facing,lean,phase,legs,action,clips");
    for b in TRACE_BONES {
        let _ = write!(h, ",{b}_x,{b}_y,{b}_z");
    }
    h
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn trace(
    m: Option<Res<Match>>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    sets: Option<Res<AnimSets>>,
    scenes: Option<Res<Scenes>>,
    me: Query<(Entity, &PlayerIntent, &Kinematic), With<LocalPlayer>>,
    vis: Query<(&WarriorVisual, &Transform, &WarriorAnim, &TraceBones)>,
    bones: Query<&GlobalTransform>,
    mut tr: ResMut<Trace>,
) {
    let (Some(m), Some(sets)) = (m, sets) else { return };
    let Ok((me, intent, kin)) = me.single() else { return };
    let Some((_, tf, anim, tb)) = vis.iter().find(|v| v.0.sim == me) else { return };
    if !tr.opened {
        tr.opened = true;
        if let Ok(path) = std::env::var("VIVAD_TRACE") {
            match std::fs::File::create(&path) {
                Ok(f) => {
                    let mut w = std::io::BufWriter::new(f);
                    let _ = writeln!(w, "{}", header());
                    tr.file = Some(w);
                }
                Err(e) => warn!("[feel] can't write {path}: {e}"),
            }
        }
    }
    let mut held = String::new();
    for (k, n) in [(KeyCode::KeyW, "W"), (KeyCode::KeyA, "A"), (KeyCode::KeyS, "S"), (KeyCode::KeyD, "D"), (KeyCode::ShiftLeft, "Sh"),
                   (KeyCode::KeyC, "C"), (KeyCode::ControlLeft, "Ct"), (KeyCode::Space, "Sp"), (KeyCode::AltLeft, "Alt")] {
        if keys.pressed(k) {
            held.push_str(n);
            held.push(' ');
        }
    }
    for (b, n) in [(MouseButton::Left, "LMB"), (MouseButton::Right, "RMB")] {
        if mouse.pressed(b) {
            held.push_str(n);
            held.push(' ');
        }
    }
    let (yaw, _, lean) = tf.rotation.to_euler(EulerRot::YXZ);
    let scene = scenes.as_ref().and_then(|s| s.index.map(|i| s.list[i].name.as_str())).unwrap_or("");
    let v = kin.velocity;
    let p = tf.translation;
    let mode = match kin.mode {
        MoveMode::Ground => "ground",
        MoveMode::Vimana => "vimana",
        MoveMode::Freefall => "freefall",
        MoveMode::Gliding => "glide",
    };
    let mut row = format!(
        "{:.4},{:.4},{:.4},{scene},{},{:.2},{:.2},{:.4},{:.4},{},{mode},{},{},{},{:.3},{:.3},{:.3},{:.4},{:.4},{:.4},{yaw:.4},{lean:.4},{:.4},{},{},",
        m.elapsed, time.elapsed_secs(), time.delta_secs(), held.trim_end(), intent.move_axis.x, intent.move_axis.y, intent.yaw, intent.pitch, intent.held,
        kin.grounded as u8, kin.crouched as u8, kin.sliding as u8, v.x, v.y, v.z, p.x, p.y, p.z, anim.phase(), anim.showing.0, anim.showing.1,
    );
    for (i, (n, masked, w)) in anim.blend(&sets).into_iter().enumerate() {
        let _ = write!(row, "{}{n}{}={w:.3}", if i > 0 { "|" } else { "" }, if masked { "~" } else { "" });
    }
    for &b in &tb.0 {
        let g = bones.get(b).map(|g| g.translation()).unwrap_or(Vec3::NAN);
        let _ = write!(row, ",{:.4},{:.4},{:.4}", g.x, g.y, g.z);
    }
    if let Some(f) = tr.file.as_mut() {
        let _ = writeln!(f, "{row}");
    }
    tr.since_flush += time.delta_secs();
    if tr.since_flush > 1.0 {
        tr.since_flush = 0.0;
        if let Some(f) = tr.file.as_mut() {
            let _ = f.flush();
        }
    }
    let t = m.elapsed;
    tr.ring.push_back((t, row));
    while tr.ring.front().is_some_and(|(t0, _)| t - *t0 > MARK_SECONDS || *t0 > t) {
        tr.ring.pop_front();
    }
}

/// F9: the last few seconds of trace and a screenshot, to marks/, and a word on screen.
#[allow(unused_variables, unused_mut)]
fn mark(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, tr: Res<Trace>, mut toast: Local<Option<(Entity, f32)>>, time: Res<Time>) {
    if let Some((e, left)) = toast.as_mut() {
        *left -= time.delta_secs();
        if *left <= 0.0 {
            commands.entity(*e).despawn();
            *toast = None;
        }
    }
    if !keys.just_pressed(KeyCode::F9) {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = format!("{}/../marks", crate::ASSET_DIR);
        let _ = std::fs::create_dir_all(&dir);
        let dir = std::fs::canonicalize(&dir).map(|p| p.display().to_string()).unwrap_or(dir);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = format!("{dir}/mark_{stamp}");
        let mut out = header();
        for (_, row) in &tr.ring {
            out.push('\n');
            out.push_str(row);
        }
        out.push('\n');
        match std::fs::write(format!("{path}.csv"), out) {
            Ok(()) => println!("[feel] marked {path}.csv ({} frames)", tr.ring.len()),
            Err(e) => warn!("[feel] can't write {path}.csv: {e}"),
        }
        commands.spawn(bevy::render::view::window::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::window::screenshot::save_to_disk(format!("{path}.png")));
        if let Some((e, _)) = toast.take() {
            commands.entity(e).despawn();
        }
        let e = commands
            .spawn((
                Text::new("Marked"),
                TextFont { font_size: bevy::text::FontSize::Px(28.0), ..default() },
                TextColor(Color::srgb(1.0, 0.92, 0.6)),
                bevy::ui::widget::TextShadow::default(),
                Node { position_type: PositionType::Absolute, top: px(110.0), left: px(24.0), ..default() },
            ))
            .id();
        *toast = Some((e, 1.5));
    }
}

/// Reread assets/data/feel.ron when it changes (checked twice a second).
#[allow(unused_variables, unused_mut)]
fn reload_feel(mut feel: ResMut<Feel>, time: Res<Time>, mut wait: Local<f32>, mut seen: Local<Option<std::time::SystemTime>>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        *wait -= time.delta_secs();
        if *wait > 0.0 {
            return;
        }
        *wait = 0.5;
        let Ok(at) = std::fs::metadata(feel_path()).and_then(|m| m.modified()) else { return };
        if seen.replace(at).is_none_or(|s| s == at) {
            return;
        }
        match std::fs::read_to_string(feel_path()).map_err(|e| e.to_string()).and_then(|s| ron::from_str::<Feel>(&s).map_err(|e| e.to_string())) {
            Ok(f) => {
                *feel = f;
                println!("[feel] reloaded {f:?}");
            }
            Err(e) => warn!("[feel] feel.ron: {e}"),
        }
    }
}

struct Scene {
    name: String,
    at: f32,
    item: String,
}

#[derive(Resource)]
struct Scenes {
    list: Vec<Scene>,
    index: Option<usize>,
    /// The scene's name on screen (for recording a run).
    title: Option<Entity>,
    /// Where each scene starts and the way it faces (yaw).
    spot: Option<(Vec3, f32)>,
}

/// Open ground with a long clear run ahead and room either side: the first spot, out from the
/// park's edge, whose corridor (40 m ahead, 12 m either side, inside the map) is level and where
/// rays along and across it at knee and head height hit nothing.
fn run_spot(layout: &MapLayout, spatial: &SpatialQuery, filter: &SpatialQueryFilter) -> (Vec3, f32) {
    let (len, half) = (40.0, 12.0);
    let edge = layout.half_extent - 4.0;
    let ray = |from: Vec3, to: Vec3| {
        let d = to - from;
        Dir3::new(d).map_or(true, |dir| spatial.cast_ray(from, dir, d.length(), true, filter).is_none())
    };
    for r in (0..160).step_by(4) {
        let n = (r / 2).max(1);
        for k in 0..n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            let o = Vec2::new(38.0, 104.0) + Vec2::new(a.cos(), a.sin()) * r as f32;
            let h0 = layout.terrain.height(o.x, o.y);
            for d in 0..8 {
                let yaw = d as f32 * std::f32::consts::FRAC_PI_4;
                let (fwd, side) = (Vec2::new(-yaw.sin(), -yaw.cos()), Vec2::new(yaw.cos(), -yaw.sin()));
                let at = |s: f32, w: f32, up: f32| {
                    let p = o + fwd * s + side * w;
                    Vec3::new(p.x, h0 + up, p.y)
                };
                let level = (-2..=20).all(|s| (-6..=6).all(|w| {
                    let p = o + fwd * (s as f32 * 2.0) + side * (w as f32 * 2.0);
                    p.x.abs() < edge && p.y.abs() < edge && (layout.terrain.height(p.x, p.y) - h0).abs() < 0.5
                }));
                let open = level
                    && [0.5, 1.6].iter().all(|&up| {
                        (-4..=4).all(|w| ray(at(-4.0, w as f32 * 3.0, up), at(len, w as f32 * 3.0, up)))
                            && (0..=8).all(|s| ray(at(s as f32 * 5.0, -half, up), at(s as f32 * 5.0, half, up)))
                    });
                if open {
                    info!("[feel] scenes at ({:.0}, {:.0}) facing {:.0} deg", o.x, o.y, yaw.to_degrees());
                    return (Vec3::new(o.x, h0 + HALF_HEIGHT + 0.05, o.y), yaw);
                }
            }
        }
    }
    warn!("[feel] no clear run found; using the park's edge");
    (Vec3::new(38.0, layout.terrain.height(38.0, 104.0) + HALF_HEIGHT + 0.05, 104.0), 0.0)
}

/// Each scene's start: the player still on the spot, facing down the run, the item in hand; kept
/// healthy, the bots' brains off and out of the way.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn stage_scenes(
    mut commands: Commands,
    mut sc: ResMut<Scenes>,
    m: Option<Res<Match>>,
    db: Res<ItemDb>,
    layout: Res<MapLayout>,
    mut me: Query<(&mut Transform, &mut Kinematic, &mut PosHistory, &mut Inventory, &mut PlayerIntent, &mut Health, &mut Action), With<LocalPlayer>>,
    mut bots: Query<(Entity, &mut Transform, &mut Kinematic, &mut Health), (With<Warrior>, Without<LocalPlayer>)>,
    mut exit: MessageWriter<AppExit>,
    mut texts: Query<&mut Text>,
    spatial: SpatialQuery,
    warriors: Query<Entity, With<Warrior>>,
) {
    // The map's colliders take a moment to come in.
    let Some(m) = m.filter(|m| m.elapsed > 3.0) else { return };
    let Ok((mut tf, mut kin, mut hist, mut inv, mut intent, mut health, mut act)) = me.single_mut() else { return };
    let filter = SpatialQueryFilter::from_excluded_entities(warriors.iter());
    let (spot, yaw) = *sc.spot.get_or_insert_with(|| run_spot(&layout, &spatial, &filter));
    let due = sc.list.iter().rposition(|s| m.elapsed >= s.at);
    health.hp = 100.0;
    health.shield = 100.0;
    for (bot, mut btf, mut bkin, mut bhp) in &mut bots {
        commands.entity(bot).remove::<shared::sim::bots::BotBrain>();
        // Behind the start, off the run.
        let at = spot + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * 12.0 + Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * 10.0;
        if btf.translation.distance(at) > 1.0 {
            btf.translation = at;
            bkin.velocity = Vec3::ZERO;
        }
        bkin.mode = MoveMode::Ground;
        bhp.hp = 100.0;
    }
    if kin.mode == MoveMode::Vimana && due.is_none() {
        // Waiting for the first scene: on the spot already, so the body has loaded and landed.
        kin.mode = MoveMode::Ground;
        tf.translation = spot;
        hist.prev = spot;
        hist.curr = spot;
        intent.yaw = yaw;
    }
    let Some(i) = due else { return };
    if sc.index == Some(i) {
        return;
    }
    sc.index = Some(i);
    let scene = &sc.list[i];
    println!("[feel] scene {} at {:.2}", scene.name, m.elapsed);
    let label = scene.name.replace('_', " ");
    match sc.title.and_then(|e| texts.get_mut(e).ok()) {
        Some(mut t) => t.0 = label,
        None => {
            let e = commands
                .spawn((
                    Text::new(label),
                    TextFont { font_size: bevy::text::FontSize::Px(40.0), ..default() },
                    TextColor(Color::srgb(1.0, 0.92, 0.6)),
                    bevy::ui::widget::TextShadow::default(),
                    Node { position_type: PositionType::Absolute, top: px(70.0), left: px(40.0), ..default() },
                ))
                .id();
            sc.title = Some(e);
        }
    }
    let scene = &sc.list[i];
    if scene.name == "end" {
        exit.write(AppExit::Success);
        return;
    }
    kin.mode = MoveMode::Ground;
    kin.velocity = Vec3::ZERO;
    tf.translation = spot;
    hist.prev = spot;
    hist.curr = spot;
    intent.yaw = yaw;
    intent.pitch = 0.0;
    let def = (!scene.item.is_empty() && scene.item != "bat").then(|| db.get(&scene.item));
    inv.slots[0] = def.map(|d| ItemStack {
        id: scene.item.clone(),
        rarity: Rarity::Epic,
        count: match &d.kind {
            ItemKind::Bow { magazine, .. } => *magazine,
            _ => 2,
        },
    });
    inv.arrows = 200;
    inv.selected = if def.is_some() { 1 } else { 0 };
    *act = Action::default();
    act.since_attack = 5.0;
}
