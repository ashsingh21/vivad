//! Combat feedback that isn't sound or HUD: tracers behind bullets, a muzzle flash, streaks behind
//! arrows and astras, a fading ribbon
//! behind a swinging blade, sparks where a hit lands and dust where an arrow sticks, a brief
//! hit-stop on the attacker's swing, and camera punches (bow kick, hit shake, the Pinaka's zoom
//! as it draws) for the local player.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::RngExt;
use shared::items::{ItemDb, ItemKind};
use shared::lines::{Lines, with};
use shared::sim::bots::BotBrain;
use shared::sim::combat::Projectile;
use shared::sim::melee::{BrokeMsg, GuardMsg};
use shared::sim::{Action, Eliminated, FeedMsg, HitMsg, Inventory, Kinematic, LocalPlayer, MatchConfig, MoveMode, PlayerIntent, Warrior, btn};

use crate::animation::WarriorAnim;
use crate::visuals::{Shot, shot_kind};
use crate::visuals::{WarriorVisual, WeaponSocket};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraPunch>()
            .add_systems(Startup, setup)
            .add_systems(Update, (add_streaks, muzzle_flash, misfire_smoke, blast_shake, hit_sparks, guard_sparks, broke_splinters, arrow_dust, landing_dust, fly_sparks, drift_puffs, scope_glints, punch_camera, climb, hit_stop))
            .add_systems(PostUpdate, (update_streaks, swing_ribbons).after(TransformSystems::Propagate));
    }
}

/// Camera offsets the follow camera adds on top of the aim: a pitch kick (radians), a shake
/// amplitude (metres) and a field-of-view change (radians). Kick and shake decay by themselves.
/// `kick` chases `kick_target`, which each shot pushes up and which falls back on its own
/// (`punch_camera`).
#[derive(Resource, Default)]
pub struct CameraPunch {
    pub kick: f32,
    pub kick_target: f32,
    pub shake: f32,
    pub fov: f32,
}

/// How fast the view chases a shot's kick, how fast the kick falls back, and the most it adds up
/// to: a shot rises over a few frames and settles, where it used to jump and fade.
const KICK_RISE: f32 = 42.0;
const KICK_FALL: f32 = 8.0;
const KICK_MOST: f32 = 0.09;

#[derive(Resource)]
struct FxAssets {
    sphere: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    streak: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
    flash: Handle<StandardMaterial>,
    fire_streak: Handle<StandardMaterial>,
    wind_streak: Handle<StandardMaterial>,
    ribbon: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
    shield_spark: Handle<StandardMaterial>,
    gold_spark: Handle<StandardMaterial>,
    gulal: Handle<StandardMaterial>,
    dust: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    /// Black powder's white cloud: a soft round sprite, at each stage of its thinning out.
    puff: Handle<Mesh>,
    powder: [Handle<StandardMaterial>; PUFF_STAGES],
    /// A scope's lens catching the sun.
    lens: Handle<StandardMaterial>,
}

/// A soft round blot of white, opaque in the middle and gone at the edge, a little uneven.
fn soft_blot() -> Image {
    const SIDE: usize = 64;
    let mut data = Vec::with_capacity(SIDE * SIDE * 4);
    for j in 0..SIDE {
        for i in 0..SIDE {
            let p = (Vec2::new(i as f32, j as f32) + 0.5) / SIDE as f32 * 2.0 - 1.0;
            let lump = 0.1 * (p.x * 7.0 + 1.0).sin() * (p.y * 6.0 + 2.0).cos();
            let d = (p.length() + lump).clamp(0.0, 1.0);
            let a = (1.0 - d * d).powi(2);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    let size = Extent3d { width: SIDE as u32, height: SIDE as u32, depth_or_array_layers: 1 };
    Image::new(size, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let soft = images.add(soft_blot());
    let glow = |mats: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba, a: f32| {
        mats.add(StandardMaterial {
            base_color: c.with_alpha(a),
            emissive: e,
            alpha_mode: if a < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque },
            unlit: true,
            ..default()
        })
    };
    commands.insert_resource(FxAssets {
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(6)),
        streak: glow(&mut mats, Color::srgb(1.0, 0.93, 0.78), LinearRgba::rgb(0.6, 0.5, 0.3), 0.6),
        tracer: glow(&mut mats, Color::srgb(1.0, 0.8, 0.45), LinearRgba::rgb(8.0, 4.5, 1.2), 0.8),
        flash: glow(&mut mats, Color::srgb(1.0, 0.75, 0.35), LinearRgba::rgb(20.0, 11.0, 3.0), 1.0),
        fire_streak: glow(&mut mats, Color::srgb(1.0, 0.5, 0.15), LinearRgba::rgb(4.0, 1.2, 0.2), 0.55),
        wind_streak: glow(&mut mats, Color::srgb(0.75, 0.9, 1.0), LinearRgba::rgb(0.6, 1.0, 1.4), 0.4),
        ribbon: mats.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.96, 0.88, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        spark: glow(&mut mats, Color::srgb(1.0, 0.85, 0.6), LinearRgba::rgb(6.0, 3.5, 1.2), 1.0),
        shield_spark: glow(&mut mats, Color::srgb(0.6, 0.85, 1.0), LinearRgba::rgb(1.5, 4.0, 8.0), 1.0),
        gold_spark: glow(&mut mats, Color::srgb(1.0, 0.85, 0.3), LinearRgba::rgb(9.0, 6.0, 1.0), 1.0),
        gulal: mats.add(StandardMaterial {
            base_color: Color::srgba(0.88, 0.1, 0.2, 0.42),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        dust: mats.add(StandardMaterial {
            base_color: Color::srgba(0.72, 0.62, 0.48, 0.55),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        smoke: mats.add(StandardMaterial {
            base_color: Color::srgba(0.32, 0.31, 0.3, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        puff: meshes.add(Rectangle::new(1.0, 1.0)),
        powder: std::array::from_fn(|k| {
            mats.add(StandardMaterial {
                base_color: Color::srgba(0.9, 0.89, 0.85, 0.3 * (1.0 - k as f32 / PUFF_STAGES as f32)),
                base_color_texture: Some(soft.clone()),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })
        }),
        // Unlit draws base colour alone, so the light that blooms goes there.
        lens: mats.add(StandardMaterial {
            base_color: LinearRgba::rgb(9.0, 8.0, 6.0).into(),
            base_color_texture: Some(soft.clone()),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
    });
}

// ------------------------------------------------------------------ streaks

/// A pale streak trailing a projectile: from where it is back to where it was `time` ago.
#[derive(Component)]
struct Streak {
    of: Entity,
    trail: VecDeque<(f32, Vec3)>,
    time: f32,
}

const STREAK_TIME: f32 = 0.14;
/// A bullet's tracer: short, hot and thin (a dunali's pellets thinner still).
const TRACER_TIME: f32 = 0.05;

fn add_streaks(mut commands: Commands, q: Query<(Entity, &Projectile, &Transform), Added<Projectile>>, fx: Res<FxAssets>, time: Res<Time>) {
    let now = time.elapsed_secs();
    for (e, p, t) in &q {
        let (mat, width, time) = match p.astra.as_ref() {
            Some(_) if p.item == "agneyastra" => (fx.fire_streak.clone(), 0.09, STREAK_TIME),
            Some(_) if p.item == "chakra" => (fx.gold_spark.clone(), 0.06, STREAK_TIME * 1.5),
            Some(_) if p.item == "chappal" => (fx.streak.clone(), 0.03, STREAK_TIME * 0.6),
            Some(_) if p.item == "brahmastra" => (fx.gold_spark.clone(), 0.12, STREAK_TIME * 2.0),
            Some(_) => (fx.wind_streak.clone(), 0.08, STREAK_TIME),
            None if shot_kind(&p.item) == Shot::Bullet => (fx.tracer.clone(), if p.item == "dunali" { 0.018 } else { 0.03 }, TRACER_TIME),
            None => (fx.streak.clone(), 0.055, STREAK_TIME),
        };
        commands.spawn((
            Streak { of: e, trail: VecDeque::from([(now, t.translation)]), time },
            Mesh3d(fx.cylinder.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(t.translation).with_scale(Vec3::new(width, 0.0, width)),
            bevy::light::NotShadowCaster,
        ));
    }
}

fn update_streaks(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Streak, &mut Transform)>,
    projectiles: Query<(&Projectile, &GlobalTransform)>,
) {
    let now = time.elapsed_secs();
    for (e, mut s, mut t) in &mut q {
        // Follow the projectile while it flies; once it stops (or is gone) the tail catches up.
        let flying = match projectiles.get(s.of) {
            Ok((p, g)) if !p.stuck => {
                s.trail.push_back((now, g.translation()));
                true
            }
            _ => {
                if let Some(&(_, last)) = s.trail.back() {
                    s.trail.push_back((now, last));
                }
                false
            }
        };
        while s.trail.len() > 2 && s.trail[1].0 < now - s.time {
            s.trail.pop_front();
        }
        let (Some(&(_, head)), Some(&(_, tail))) = (s.trail.back(), s.trail.front()) else { continue };
        let d = head - tail;
        if !flying && d.length() < 0.05 {
            commands.entity(e).despawn();
            continue;
        }
        t.translation = (head + tail) * 0.5;
        t.rotation = Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y));
        t.scale.y = d.length();
    }
}

// ------------------------------------------------------------------ blade ribbon

/// Samples of the blade (base, tip) while it swings, drawn as a fading ribbon.
#[derive(Component)]
struct SwingRibbon {
    of: Entity,
    samples: VecDeque<(f32, Vec3, Vec3)>,
    mesh: Handle<Mesh>,
}

const RIBBON_TIME: f32 = 0.08;
/// How wide the ribbon is behind the tip, metres: a thin arc, not a sheet the length of the blade.
const RIBBON_WIDTH: f32 = 0.06;
/// Where a weapon's blade runs (its glTF +Y from the grip): the ribbon follows the end of it.
fn blade_of(item: &str) -> (f32, f32) {
    match item {
        "lathi" => (0.45, 1.15),
        "tawa" => (0.1, 0.38),
        "dhal" => (0.1, 0.22),
        "talwar" => (0.2, 0.85),
        "gada" => (0.45, 0.85),
        _ => (0.2, 0.7),
    }
}

#[allow(clippy::type_complexity)]
fn swing_ribbons(
    mut commands: Commands,
    time: Res<Time>,
    vis: Query<(Entity, &WarriorVisual, &WeaponSocket)>,
    sims: Query<(&Inventory, &Action)>,
    db: Res<ItemDb>,
    globals: Query<&GlobalTransform>,
    mut ribbons: Query<(Entity, &mut SwingRibbon, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    fx: Res<FxAssets>,
) {
    let now = time.elapsed_secs();
    for (e, wv, socket) in &vis {
        let Ok((inv, act)) = sims.get(wv.sim) else { continue };
        let Some((id, ItemKind::Melee { windup, active, .. })) = inv.current().map(|s| (s.id.as_str(), &db.get(&s.id).kind)) else { continue };
        // Only while the blade comes through, not on the way up.
        if !(act.since_attack > *windup && act.since_attack < windup + active + 0.03) {
            continue;
        }
        let Ok(g) = globals.get(socket.0) else { continue };
        let blade = blade_of(id);
        let (base, tip) = (g.transform_point(Vec3::Y * blade.0), g.transform_point(Vec3::Y * blade.1));
        let base = tip + (base - tip).normalize_or_zero() * RIBBON_WIDTH;
        if let Some((_, mut r, _)) = ribbons.iter_mut().find(|(_, r, _)| r.of == e) {
            r.samples.push_back((now, base, tip));
        } else {
            // One point to start with: Bevy warns about a mesh with no vertices. It draws nothing,
            // and the ribbon replaces it below.
            let empty = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()).with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]]);
            let mesh = meshes.add(empty);
            commands.spawn((
                SwingRibbon { of: e, samples: VecDeque::from([(now, base, tip)]), mesh: mesh.clone() },
                Mesh3d(mesh),
                MeshMaterial3d(fx.ribbon.clone()),
                Transform::default(),
                Visibility::default(),
                bevy::light::NotShadowCaster,
            ));
        }
    }
    for (e, mut r, mut v) in &mut ribbons {
        while r.samples.front().is_some_and(|s| s.0 < now - RIBBON_TIME) {
            r.samples.pop_front();
        }
        if r.samples.len() < 2 {
            *v = Visibility::Hidden;
            if r.samples.is_empty() && vis.get(r.of).is_err() {
                commands.entity(e).despawn();
            }
            continue;
        }
        *v = Visibility::Inherited;
        // Two vertices per sample; the edge nearest the tip is brightest, the newest sample widest
        // and most opaque, and it tapers to nothing behind.
        let (mut pos, mut col, mut idx) = (vec![], vec![], vec![]);
        for (i, &(t, b, tip)) in r.samples.iter().enumerate() {
            let a = (1.0 - (now - t) / RIBBON_TIME).clamp(0.0, 1.0);
            pos.push(tip.lerp(b, a).to_array());
            pos.push(tip.to_array());
            col.push([1.0, 1.0, 1.0, a * a * 0.1]);
            col.push([1.0, 1.0, 1.0, a * a * 0.5]);
            if i > 0 {
                let k = (i as u32 - 1) * 2;
                idx.extend_from_slice(&[k, k + 1, k + 3, k, k + 3, k + 2]);
            }
        }
        if let Some(mut m) = meshes.get_mut(&r.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; r.samples.len() * 2]);
            m.insert_indices(Indices::U32(idx));
        }
    }
}

// ------------------------------------------------------------------ sparks and dust

#[derive(Component)]
struct Spark {
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    gravity: f32,
}

/// Powder and dust grains billow (grow) as they drift, instead of shrinking like sparks.
#[derive(Component)]
struct Billow;

fn burst(commands: &mut Commands, fx: &FxAssets, mat: &Handle<StandardMaterial>, at: Vec3, toward: Vec3, n: usize, speed: f32, size: f32, gravity: f32) {
    let mut rng = rand::rng();
    for _ in 0..n {
        let r = Vec3::new(rng.random_range(-1.0..1.0), rng.random_range(-0.4..1.0), rng.random_range(-1.0..1.0));
        let vel = (toward * 0.8 + r * 0.7).normalize_or(Vec3::Y) * speed * rng.random_range(0.5..1.0);
        let max = rng.random_range(0.18..0.35);
        commands.spawn((
            Spark { vel, life: max, max, size: size * rng.random_range(0.6..1.2), gravity },
            Mesh3d(fx.sphere.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            bevy::light::NotShadowCaster,
        ));
    }
}

fn billow(commands: &mut Commands, fx: &FxAssets, mat: &Handle<StandardMaterial>, at: Vec3, toward: Vec3, n: usize, speed: f32, size: f32) {
    let mut rng = rand::rng();
    for _ in 0..n {
        let r = Vec3::new(rng.random_range(-1.0..1.0), rng.random_range(-0.3..1.0), rng.random_range(-1.0..1.0));
        let vel = (toward * 0.6 + r).normalize_or(Vec3::Y) * speed * rng.random_range(0.3..1.0);
        let max = rng.random_range(0.35..0.6);
        commands.spawn((
            Spark { vel, life: max, max, size: size * rng.random_range(0.7..1.3), gravity: -0.4 },
            Billow,
            Mesh3d(fx.sphere.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            bevy::light::NotShadowCaster,
        ));
    }
}

/// Where a hit lands: sparks off a ward (shield), a puff of crimson gulal off flesh, gold for a
/// headshot.
fn hit_sparks(mut commands: Commands, mut hits: MessageReader<HitMsg>, fx: Res<FxAssets>, tf: Query<&Transform>) {
    for h in hits.read() {
        let from = tf.get(h.attacker).map(|t| t.translation).unwrap_or(h.at);
        let back = (from - h.at).with_y(0.0).normalize_or(Vec3::Y);
        // `at` is a metre above the body's centre; bring it to the chest (or the head) on the
        // side facing the attacker.
        let at = h.at + back * 0.3 + Vec3::Y * if h.headshot { -0.15 } else { -0.6 };
        if h.shield {
            burst(&mut commands, &fx, &fx.shield_spark, at, back, 10, 5.0, 0.022, 9.0);
        } else {
            billow(&mut commands, &fx, &fx.gulal, at, back, 14, 2.4, 0.03);
            burst(&mut commands, &fx, &fx.spark, at, back, 4, 4.0, 0.014, 9.0);
        }
        if h.headshot {
            burst(&mut commands, &fx, &fx.gold_spark, at, Vec3::Y, 8, 3.5, 0.02, 6.0);
        }
    }
}

/// A blow on a raised guard rings off it in sparks.
fn guard_sparks(mut commands: Commands, mut guards: MessageReader<GuardMsg>, fx: Res<FxAssets>) {
    for g in guards.read() {
        burst(&mut commands, &fx, &fx.shield_spark, g.at, Vec3::Y, 12, 5.0, 0.02, 9.0);
    }
}

/// An old weapon flies apart, and its owner is told.
fn broke_splinters(mut commands: Commands, mut broke: MessageReader<BrokeMsg>, fx: Res<FxAssets>, me: Query<(), With<LocalPlayer>>, db: Res<ItemDb>, lines: Res<Lines>, mut feed: MessageWriter<FeedMsg>) {
    for b in broke.read() {
        let at = b.at + Vec3::Y * 0.4;
        burst(&mut commands, &fx, &fx.gold_spark, at, Vec3::Y, 10, 4.0, 0.02, 9.0);
        billow(&mut commands, &fx, &fx.dust, at, Vec3::Y, 8, 1.6, 0.04);
        if me.contains(b.who) {
            feed.write(FeedMsg(with(lines.text("hud.broke"), &[("item", &db.get(&b.item).name)])));
        }
    }
}

#[derive(Component)]
struct Puffed;

/// A ring of dust kicked up where a warrior lands hard (a jump off a roof, the end of a glide).
fn landing_dust(
    mut commands: Commands,
    q: Query<(Entity, &shared::sim::Kinematic, &Transform)>,
    mut was: Local<std::collections::HashMap<Entity, (bool, f32)>>,
    fx: Res<FxAssets>,
) {
    for (e, kin, t) in &q {
        let (grounded, vy) = was.get(&e).copied().unwrap_or((true, 0.0));
        if !grounded && kin.grounded && vy < -6.0 {
            let n = (6.0 + -vy).min(20.0) as usize;
            let feet = t.translation - Vec3::Y * shared::sim::HALF_HEIGHT + Vec3::Y * 0.1;
            billow(&mut commands, &fx, &fx.dust, feet, Vec3::NEG_Y * 0.5, n, 1.2 + -vy * 0.05, 0.12);
        }
        was.insert(e, (kin.grounded, kin.velocity.y));
    }
    was.retain(|e, _| q.contains(*e));
}

/// A puff of dust where an arrow sticks into the world; a bullet kicks up sparks with it.
fn arrow_dust(mut commands: Commands, q: Query<(Entity, &Projectile, &Transform), Without<Puffed>>, fx: Res<FxAssets>) {
    for (e, p, t) in &q {
        if p.stuck {
            commands.entity(e).insert(Puffed);
            let back = -(t.rotation * Vec3::NEG_Z);
            if shot_kind(&p.item) == Shot::Bullet {
                burst(&mut commands, &fx, &fx.spark, t.translation, back, 5, 5.0, 0.012, 9.0);
                billow(&mut commands, &fx, &fx.dust, t.translation, back, 4, 1.0, 0.025);
            } else {
                billow(&mut commands, &fx, &fx.dust, t.translation, back, 7, 1.2, 0.035);
            }
        }
    }
}

/// The Brahmastra shakes the view of everyone near it, harder the closer.
fn blast_shake(q: Query<(&shared::sim::combat::Burst, &Transform), Added<shared::sim::combat::Burst>>, me: Query<&Transform, With<LocalPlayer>>,
               mut punch: ResMut<CameraPunch>) {
    let Ok(me) = me.single() else { return };
    for (b, t) in &q {
        if b.blast {
            punch.shake += 0.35 * (1.0 - me.translation.distance(t.translation) / 80.0).max(0.0);
        }
    }
}

/// A katta that only clicked coughs a wisp of smoke; one that backfired a flash and a cloud.
fn misfire_smoke(mut commands: Commands, mut msgs: MessageReader<shared::sim::combat::MisfireMsg>, fx: Res<FxAssets>) {
    for m in msgs.read() {
        if m.backfire {
            burst(&mut commands, &fx, &fx.gold_spark, m.at, Vec3::Y, 10, 4.0, 0.016, 6.0);
            billow(&mut commands, &fx, &fx.smoke, m.at, Vec3::Y, 12, 1.4, 0.06);
        } else {
            billow(&mut commands, &fx, &fx.smoke, m.at, Vec3::Y, 3, 0.4, 0.03);
        }
    }
}

/// A gun going off: a hot flash at the muzzle with a moment of light round it, and a few sparks
/// thrown forward. Once per shot, however many pellets it fires.
fn muzzle_flash(mut commands: Commands, q: Query<(&Projectile, &Transform), Added<Projectile>>, shooters: Query<&Transform, With<Warrior>>, fx: Res<FxAssets>) {
    let mut fired: Vec<Entity> = vec![];
    for (p, t) in &q {
        if p.astra.is_some() || shot_kind(&p.item) != Shot::Bullet || fired.contains(&p.owner) {
            continue;
        }
        fired.push(p.owner);
        let ahead = p.velocity.normalize_or(Vec3::NEG_Z);
        let at = t.translation + ahead * 0.15;
        let size = if matches!(p.item.as_str(), "dunali" | "thri_not_thri" | "toradar") { 0.2 } else { 0.13 };
        commands.spawn((
            Spark { vel: Vec3::ZERO, life: 0.06, max: 0.06, size, gravity: 0.0 },
            Mesh3d(fx.sphere.clone()),
            MeshMaterial3d(fx.flash.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            bevy::light::NotShadowCaster,
            PointLight { color: Color::srgb(1.0, 0.75, 0.4), intensity: 150_000.0, range: 7.0, shadow_maps_enabled: false, ..default() },
        ));
        burst(&mut commands, &fx, &fx.gold_spark, at, ahead, 5, 7.0, 0.01, 4.0);
        // Black powder: the big white puff that gives a Toradar's shooter away.
        if p.item == "toradar" {
            // A fast bullet is metres gone by the time it is seen here: the smoke starts at the gun.
            let gun = shooters.get(p.owner).map_or(at, |s| s.translation + Vec3::Y * shared::sim::EYE_OFFSET * 0.7 + ahead * 0.9);
            powder_cloud(&mut commands, &fx, gun, ahead);
        }
    }
}

/// A blot of black powder's smoke, always facing the view: it blows out of the muzzle, slows,
/// swells and thins for as long as it takes a far shooter to be seen.
#[derive(Component)]
struct Puff {
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    stage: usize,
}

/// Blots in the cloud, how far along the shot they reach, their width at birth, how many times over
/// they swell, seconds they hang, and the steps of their thinning.
const PUFF_BLOTS: usize = 22;
const PUFF_REACH: f32 = 4.5;
const PUFF_SIZE: (f32, f32) = (1.0, 2.2);
const PUFF_SWELL: f32 = 2.8;
const PUFF_LIFE: (f32, f32) = (6.0, 9.5);
const PUFF_STAGES: usize = 6;

fn powder_cloud(commands: &mut Commands, fx: &FxAssets, at: Vec3, ahead: Vec3) {
    let mut rng = rand::rng();
    let wind = Vec3::new(rng.random_range(-0.4..0.4), 0.0, rng.random_range(-0.4..0.4));
    for k in 0..PUFF_BLOTS {
        // Strung out along the shot's first metres, as a matchlock blows it, thickest at the muzzle.
        let along = PUFF_REACH * ((k as f32 + rng.random_range(0.0..1.0)) / PUFF_BLOTS as f32).powf(1.5);
        let off = Vec3::new(rng.random_range(-0.3..0.3), rng.random_range(-0.15..0.35), rng.random_range(-0.3..0.3));
        let max = rng.random_range(PUFF_LIFE.0..PUFF_LIFE.1);
        let size = rng.random_range(PUFF_SIZE.0..PUFF_SIZE.1);
        let vel = ahead * rng.random_range(0.5..2.0) + wind + Vec3::Y * rng.random_range(0.05..0.3);
        commands.spawn((
            Puff { vel, life: max, max, size, stage: 0 },
            Mesh3d(fx.puff.clone()),
            MeshMaterial3d(fx.powder[0].clone()),
            Transform::from_translation(at + ahead * along + off).with_scale(Vec3::splat(size)),
            bevy::light::NotShadowCaster,
        ));
    }
}

fn drift_puffs(
    mut commands: Commands,
    time: Res<Time>,
    fx: Res<FxAssets>,
    camera: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut q: Query<(Entity, &mut Puff, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let dt = time.delta_secs();
    let facing = camera.single().map_or(Quat::IDENTITY, |c| c.rotation());
    for (e, mut p, mut t, mut look) in &mut q {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        p.vel *= 1.0 - 0.6 * dt;
        t.translation += p.vel * dt;
        t.rotation = facing;
        let age = 1.0 - p.life / p.max;
        t.scale = Vec3::splat(p.size * (1.0 + (PUFF_SWELL - 1.0) * age.sqrt()));
        let stage = ((age * PUFF_STAGES as f32) as usize).min(PUFF_STAGES - 1);
        if stage != p.stage {
            p.stage = stage;
            look.0 = fx.powder[stage].clone();
        }
    }
}

/// A scope seen from in front catches the sun: far rifles show where they are, more the more nearly
/// they point at you. A small point of light, the same size on the screen near or far. A player is
/// at the scope while holding aim; a bot, whose rifle is always up, while it has somebody to shoot.
#[derive(Component)]
struct Glint(Entity);

/// How near facing the viewer a scope must be to catch the light, 1 being dead on.
const GLINT_FROM: f32 = 0.55;
/// Its soft blot's width as a share of its distance, faintest and full.
const GLINT_SIZE: (f32, f32) = (0.003, 0.012);
/// The sun's hours: at night a lens has nothing to catch.
const SUN_UP: (f32, f32) = (6.0, 19.0);

#[allow(clippy::type_complexity)]
fn scope_glints(
    mut commands: Commands,
    warriors: Query<(Entity, &Transform, &PlayerIntent, &Inventory, &Kinematic, Option<&BotBrain>), (With<Warrior>, Without<LocalPlayer>, Without<Eliminated>)>,
    mut glints: Query<(Entity, &Glint, &mut Transform), Without<Warrior>>,
    camera: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    db: Res<ItemDb>,
    fx: Res<FxAssets>,
    tod: Res<crate::visuals::TimeOfDay>,
    time: Res<Time>,
    mut had: Local<Vec<Entity>>,
) {
    let Ok(cam) = camera.single() else { return };
    let (facing_view, cam) = (cam.rotation(), cam.translation());
    let hour = tod.hour.rem_euclid(24.0);
    let day = (SUN_UP.0..SUN_UP.1).contains(&hour);
    let t = time.elapsed_secs();
    let mut now: Vec<(Entity, Vec3, f32)> = vec![];
    for (e, tf, intent, inv, kin, brain) in &warriors {
        let looking = intent.held(btn::AIM) || brain.is_some_and(|b| b.target.is_some());
        if !day || !looking || kin.mode != MoveMode::Ground || inv.current().is_none_or(|s| db.get(&s.id).kind.scope().is_none()) {
            continue;
        }
        let aim = intent.aim_dir();
        let lens = tf.translation + Vec3::Y * (shared::sim::EYE_OFFSET - intent.drop()) + aim * 0.4;
        let facing = aim.dot((cam - lens).normalize_or_zero());
        if facing > GLINT_FROM {
            let k = ((facing - GLINT_FROM) / (1.0 - GLINT_FROM)).powi(2);
            let twinkle = 0.8 + 0.2 * (t * 11.0 + e.index().index() as f32).sin();
            now.push((e, lens, lens.distance(cam) * (GLINT_SIZE.0 + (GLINT_SIZE.1 - GLINT_SIZE.0) * k) * twinkle));
        }
    }
    for (e, g, mut tf) in &mut glints {
        match now.iter().find(|n| n.0 == g.0) {
            Some((_, at, size)) => {
                tf.translation = *at;
                tf.rotation = facing_view;
                tf.scale = Vec3::splat(*size);
            }
            None => commands.entity(e).despawn(),
        }
    }
    for (e, at, size) in &now {
        if !had.contains(e) {
            commands.spawn((Glint(*e), Mesh3d(fx.puff.clone()), MeshMaterial3d(fx.lens.clone()), Transform::from_translation(*at).with_rotation(facing_view).with_scale(Vec3::splat(*size)), bevy::light::NotShadowCaster));
        }
    }
    *had = now.iter().map(|n| n.0).collect();
}

fn fly_sparks(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Spark, &mut Transform, Has<Billow>)>) {
    let dt = time.delta_secs();
    for (e, mut s, mut t, billows) in &mut q {
        s.life -= dt;
        if s.life <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        s.vel.y -= s.gravity * dt;
        s.vel *= 1.0 - if billows { 5.0 } else { 2.5 } * dt;
        t.translation += s.vel * dt;
        let k = s.life / s.max;
        t.scale = Vec3::splat(if billows { s.size * (1.0 + 2.0 * (1.0 - k)) * (k * 4.0).min(1.0) } else { s.size * k.sqrt() });
    }
}

// ------------------------------------------------------------------ the Saintalis's climb

/// How a held-down Saintalis walks off the target, shot by shot: (up, sideways) in radians. The
/// first few stay close, then it climbs and swings. The Saintalis's lore says it: stop and start, or
/// the bullets go to the sky. The pattern's idea is bevy_fps_game's `spray_pattern` (MIT, see
/// docs/references.md); the numbers are ours. Past its end it keeps on with the last rows.
const CLIMB: [(f32, f32); 14] = [
    (0.003, 0.000), (0.004, 0.001), (0.007, -0.002), (0.010, -0.003), (0.012, 0.002), (0.013, 0.005), (0.013, 0.006),
    (0.012, 0.002), (0.011, -0.004), (0.010, -0.007), (0.009, -0.005), (0.008, 0.001), (0.008, 0.005), (0.008, 0.002),
];
/// Seconds without a shot that end a burst: it starts low again.
const BURST_GAP: f32 = 0.36;
/// The share of the climb that settles back once the trigger is let go, and how fast.
const CLIMB_BACK: f32 = 0.55;
const SETTLE: f32 = 7.0;

#[derive(Default)]
struct Climbing {
    shot: usize,
    last_shot: f32,
    /// What is still to settle back.
    owed: f32,
    /// The pitch as this left it, to see the player pulling down against it.
    pitch: f32,
}

/// The local aim itself climbs, so where the shots go moves with the view, and the player pulls
/// down against it with the mouse. This is the player's own aim, sent as any other: nothing here
/// goes through the sim, and the other peers see where the shots really went.
fn climb(mut local: Query<(Entity, &mut PlayerIntent), With<LocalPlayer>>, shots: Query<&Projectile, Added<Projectile>>, time: Res<Time>, config: Res<MatchConfig>, mut c: Local<Climbing>) {
    let Ok((me, mut intent)) = local.single_mut() else { return };
    if config.autopilot {
        return;
    }
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    // Pulling down has paid off what would have settled back.
    c.owed = (c.owed - (c.pitch - intent.pitch).max(0.0)).max(0.0);
    let mut fired = false;
    for p in &shots {
        if p.owner == me && p.item == "saintalis" {
            if now - c.last_shot > BURST_GAP {
                c.shot = 0;
            }
            let k = if c.shot < CLIMB.len() { c.shot } else { CLIMB.len() - 3 + (c.shot - CLIMB.len()) % 3 };
            let (up, side) = CLIMB[k];
            intent.pitch += up;
            intent.yaw += side;
            c.owed += up * CLIMB_BACK;
            c.shot += 1;
            c.last_shot = now;
            fired = true;
        }
    }
    if !fired && now - c.last_shot > BURST_GAP * 0.5 {
        let back = c.owed * (1.0 - (-dt * SETTLE).exp());
        intent.pitch -= back;
        c.owed -= back;
    }
    c.pitch = intent.pitch;
}

// ------------------------------------------------------------------ hit-stop and camera

/// A sword hit holds the attacker's swing for a few frames, so the blow lands with weight.
fn hit_stop(mut hits: MessageReader<HitMsg>, sims: Query<&Inventory>, db: Res<ItemDb>, mut vis: Query<(&WarriorVisual, &mut WarriorAnim)>) {
    for h in hits.read() {
        let melee = sims.get(h.attacker).ok().and_then(|i| i.current()).is_some_and(|s| matches!(db.get(&s.id).kind, ItemKind::Melee { .. }));
        if !melee {
            continue;
        }
        if let Some((_, mut a)) = vis.iter_mut().find(|(v, _)| v.sim == h.attacker) {
            a.hit_stop = 0.07;
        }
    }
}

/// One frame of the kick: the target falls back and the view chases it.
fn kick_step(kick: f32, target: f32, dt: f32) -> (f32, f32) {
    let target = target * (-dt * KICK_FALL).exp();
    (kick + (target - kick) * (1.0 - (-dt * KICK_RISE).exp()), target)
}

#[allow(clippy::too_many_arguments)]
fn punch_camera(
    mut punch: ResMut<CameraPunch>,
    time: Res<Time>,
    mut hits: MessageReader<HitMsg>,
    shots: Query<&Projectile, Added<Projectile>>,
    local: Query<(Entity, &Inventory, &Action, &PlayerIntent), With<LocalPlayer>>,
    db: Res<ItemDb>,
) {
    let dt = time.delta_secs();
    (punch.kick, punch.kick_target) = kick_step(punch.kick, punch.kick_target, dt);
    punch.shake *= (-dt * 16.0).exp();
    let Ok((me, inv, act, intent)) = local.single() else { return };
    let held = inv.current().map(|s| &db.get(&s.id).kind);
    for p in &shots {
        if p.owner == me && p.astra.is_none() {
            // Each bow's own kick (radians of pitch) and rattle (metres), once per volley: the
            // katta and the dunali wrench the view, the Ladi-9 chatters, the tamancha taps.
            let (kick, rattle) = match p.item.as_str() {
                "katta" => (0.040, 0.02),
                "dunali" => (0.045, 0.025),
                "gulel" => (0.030, 0.0),
                "tamancha" => (0.016, 0.004),
                "vajra9" => (0.011, 0.012),
                "trishul_carbine" => (0.022, 0.008),
                "saintalis" => (0.018, 0.012),
                "thaana_slr" => (0.03, 0.012),
                "thri_not_thri" => (0.05, 0.02),
                "toradar" => (0.07, 0.03),
                _ => (0.012, 0.0),
            };
            punch.kick_target = (punch.kick_target + kick).min(KICK_MOST.max(kick));
            punch.shake = punch.shake.max(rattle);
        }
    }
    for h in hits.read() {
        if h.target == me {
            punch.shake = punch.shake.max(0.03 + h.amount * 0.0008);
        } else if h.attacker == me && matches!(held, Some(ItemKind::Melee { .. })) {
            punch.shake = punch.shake.max(0.035);
        }
    }
    // Drawing a charged bow while aiming narrows the view as the draw comes up.
    let want = match held {
        Some(ItemKind::Bow { charge, .. }) if *charge > 0.0 && intent.held(btn::AIM) => -0.16 * (act.charge / charge).clamp(0.0, 1.0),
        _ => 0.0,
    };
    punch.fov += (want - punch.fov) * (dt * 8.0).min(1.0);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::{TimePlugin, TimeUpdateStrategy};

    use super::*;

    /// A kick rises over a few frames, tops out and is gone in a moment.
    #[test]
    fn a_kick_rises_then_falls_back() {
        let (dt, shot) = (1.0 / 60.0, 0.05);
        let (mut kick, mut target) = (0.0, shot);
        let mut seen = vec![];
        for _ in 0..40 {
            (kick, target) = kick_step(kick, target, dt);
            seen.push(kick);
        }
        let top = seen.iter().copied().fold(0.0, f32::max);
        let at = seen.iter().position(|k| *k == top).unwrap();
        assert!((2..8).contains(&at), "peaks on frame {at}: {seen:?}");
        assert!(top > shot * 0.55 && top < shot, "{top}");
        assert!(seen[1] > seen[0], "it builds");
        assert!(seen[39] < top * 0.2, "gone after two thirds of a second: {}", seen[39]);
    }

    fn firing_range() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(30)))
            .insert_resource(MatchConfig { autopilot: false, ..default() })
            .add_systems(Update, climb);
        let me = app.world_mut().spawn((LocalPlayer, PlayerIntent::default())).id();
        app.update();
        (app, me)
    }

    fn shoot(app: &mut App, owner: Entity, item: &str) {
        app.world_mut().spawn(Projectile { owner, velocity: Vec3::NEG_Z, gravity: 0.0, damage: 0.0, headshot_mult: 1.0, ttl: 1.0, item: item.into(), astra: None, stuck: false });
    }

    fn pitch(app: &App, me: Entity) -> f32 {
        app.world().get::<PlayerIntent>(me).unwrap().pitch
    }

    /// Run `frames` of 30 ms with a Saintalis shot every sixth, as its 0.18 s between shots does.
    fn hold_fire(app: &mut App, me: Entity, shots: usize) {
        for _ in 0..shots {
            shoot(app, me, "saintalis");
            for _ in 0..6 {
                app.update();
            }
        }
    }

    #[test]
    fn a_held_saintalis_climbs_and_bursts_stay_on_target() {
        let (mut app, me) = firing_range();
        hold_fire(&mut app, me, 3);
        let burst = pitch(&app, me);
        assert!(burst > 0.0 && burst < 0.02, "the first three stay close: {burst}");
        hold_fire(&mut app, me, 9);
        let held = pitch(&app, me);
        assert!(held > 0.08, "twelve shots go to the sky: {held}");
        for _ in 0..60 {
            app.update();
        }
        let settled = pitch(&app, me);
        assert!(settled < held * 0.6 && settled > held * 0.3, "part of it settles back: {held} to {settled}");
    }

    #[test]
    fn pulling_down_beats_the_climb() {
        let (mut app, me) = firing_range();
        hold_fire(&mut app, me, 12);
        let held = pitch(&app, me);
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().pitch = 0.0;
        for _ in 0..60 {
            app.update();
        }
        assert!(pitch(&app, me).abs() < held * 0.05, "the player's pull stands: {}", pitch(&app, me));
    }

    #[test]
    fn only_the_saintalis_climbs() {
        let (mut app, me) = firing_range();
        for _ in 0..6 {
            shoot(&mut app, me, "vajra9");
            app.update();
        }
        assert_eq!(pitch(&app, me), 0.0);
    }
}
