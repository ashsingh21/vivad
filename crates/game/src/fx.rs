//! Combat feedback that isn't sound or HUD: tracers behind bullets, a muzzle flash, streaks behind
//! arrows and astras, a fading ribbon
//! behind a swinging blade, sparks where a hit lands and dust where an arrow sticks, a brief
//! hit-stop on the attacker's swing, and camera punches (bow kick, hit shake, the Pinaka's zoom
//! as it draws) for the local player.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use rand::RngExt;
use shared::items::{ItemDb, ItemKind};
use shared::sim::combat::Projectile;
use shared::sim::{Action, HitMsg, Inventory, LocalPlayer, PlayerIntent, btn};

use crate::animation::WarriorAnim;
use crate::visuals::{Shot, shot_kind};
use crate::visuals::{WarriorVisual, WeaponSocket};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraPunch>()
            .add_systems(Startup, setup)
            .add_systems(Update, (add_streaks, muzzle_flash, hit_sparks, arrow_dust, fly_sparks, punch_camera, hit_stop))
            .add_systems(PostUpdate, (update_streaks, swing_ribbons).after(TransformSystems::Propagate));
    }
}

/// Camera offsets the follow camera adds on top of the aim: a pitch kick (radians), a shake
/// amplitude (metres) and a field-of-view change (radians). Kick and shake decay by themselves.
#[derive(Resource, Default)]
pub struct CameraPunch {
    pub kick: f32,
    pub shake: f32,
    pub fov: f32,
}

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
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
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
            Some(_) => (fx.wind_streak.clone(), 0.08, STREAK_TIME),
            None if shot_kind(&p.item) == Shot::Bullet => (fx.tracer.clone(), if p.item == "sharanga" { 0.018 } else { 0.03 }, TRACER_TIME),
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

const RIBBON_TIME: f32 = 0.13;
/// The swing ribbon spans this part of the khanda's blade (its glTF +Y from the grip).
const BLADE: (f32, f32) = (0.3, 0.92);

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
        let melee = inv.current().is_some_and(|s| matches!(db.get(&s.id).kind, ItemKind::Melee { .. }));
        if !(melee && act.since_attack > 0.06 && act.since_attack < 0.32) {
            continue;
        }
        let Ok(g) = globals.get(socket.0) else { continue };
        let (base, tip) = (g.transform_point(Vec3::Y * BLADE.0), g.transform_point(Vec3::Y * BLADE.1));
        if let Some((_, mut r, _)) = ribbons.iter_mut().find(|(_, r, _)| r.of == e) {
            r.samples.push_back((now, base, tip));
        } else {
            let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
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
        // Two vertices per sample; the edge nearest the tip is brightest and newest samples opaque.
        let (mut pos, mut col, mut idx) = (vec![], vec![], vec![]);
        for (i, &(t, b, tip)) in r.samples.iter().enumerate() {
            let a = (1.0 - (now - t) / RIBBON_TIME).clamp(0.0, 1.0);
            pos.push(b.to_array());
            pos.push(tip.to_array());
            col.push([1.0, 1.0, 1.0, a * a * 0.05]);
            col.push([1.0, 1.0, 1.0, a * a * 0.55]);
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

#[derive(Component)]
struct Puffed;

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

/// A gun going off: a hot flash at the muzzle with a moment of light round it, and a few sparks
/// thrown forward. Once per shot, however many pellets it fires.
fn muzzle_flash(mut commands: Commands, q: Query<(&Projectile, &Transform), Added<Projectile>>, fx: Res<FxAssets>) {
    let mut fired: Vec<Entity> = vec![];
    for (p, t) in &q {
        if p.astra.is_some() || shot_kind(&p.item) != Shot::Bullet || fired.contains(&p.owner) {
            continue;
        }
        fired.push(p.owner);
        let ahead = p.velocity.normalize_or(Vec3::NEG_Z);
        let at = t.translation + ahead * 0.15;
        let size = if p.item == "sharanga" { 0.2 } else { 0.13 };
        commands.spawn((
            Spark { vel: Vec3::ZERO, life: 0.06, max: 0.06, size, gravity: 0.0 },
            Mesh3d(fx.sphere.clone()),
            MeshMaterial3d(fx.flash.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size)),
            bevy::light::NotShadowCaster,
            PointLight { color: Color::srgb(1.0, 0.75, 0.4), intensity: 150_000.0, range: 7.0, shadow_maps_enabled: false, ..default() },
        ));
        burst(&mut commands, &fx, &fx.gold_spark, at, ahead, 5, 7.0, 0.01, 4.0);
    }
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
    punch.kick *= (-dt * 14.0).exp();
    punch.shake *= (-dt * 16.0).exp();
    let Ok((me, inv, act, intent)) = local.single() else { return };
    let held = inv.current().map(|s| &db.get(&s.id).kind);
    for p in &shots {
        if p.owner == me && p.astra.is_none() {
            // A heavier bow kicks harder (one kick per volley).
            let k = if let Some(ItemKind::Bow { damage, pellets, .. }) = held { (damage * *pellets as f32).sqrt() * 0.0028 } else { 0.012 };
            punch.kick = punch.kick.max(k);
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
