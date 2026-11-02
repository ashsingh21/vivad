//! Vehicles a warrior can drive: an auto, a scooter, a commuter bike, a heavy bike and a small car,
//! parked along the streets. The interact key gets in and out, the move keys drive (crate::sim::drive),
//! nobody shoots while driving (`MoveMode::Riding` is not on foot). Moved by collide-and-slide like a
//! warrior, never by forces, so walls stop a vehicle and it slides along them.
//!
//! Colliders are on a layer of their own, so the nav grid, bot sight and the camera's wall check never
//! see them; warriors walk round them, shots and blows hurt them. Bots do not drive. Vehicles are off
//! in online matches for now (`enabled`): nothing here is told to other peers.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;
use rand::seq::SliceRandom;

use super::bots::BotBrain;
use super::drive::{DriveInput, Law, step};
use super::flow::{Match, Rules};
use super::loot::{Chest, INTERACT_RANGE, Pickup};
use super::movement::{GRAVITY, wading};
use super::net::{Peers, Remote};
use super::{
    CAPSULE_LENGTH, CAPSULE_RADIUS, DamageCause, DamageMsg, Eliminated, HALF_HEIGHT, HitMsg, Kinematic, Layer, MatchConfig, MoveMode, PlayerIntent, SimSet,
    Warrior, btn, match_rng, solid_mask, stream, walk_mask,
};
use crate::map::MapLayout;
use crate::map::terrain::ground;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleKind {
    Auto,
    Scooter,
    Commuter,
    Bullet,
    Car,
}

/// One kind's numbers, sizes from its model (tools/blender/props/vehicles.py). Points are from the
/// spot on the ground under its middle, nose along -Z.
pub struct Spec {
    /// Its word on screen.
    pub name: &'static str,
    pub model: &'static str,
    pub law: Law,
    pub hp: f32,
    /// The collision box's width, height and length.
    pub size: Vec3,
    /// The box's floor above the ground: kerbs and low slabs pass under it.
    pub clear: f32,
    /// The box's rounded edges: they keep a corner from catching on a wall.
    pub round: f32,
    /// Where the driver's body centre sits.
    pub seat: Vec3,
    /// Where damage smokes from.
    pub engine: Vec3,
    pub wheel: f32,
    /// How far the body leans into a corner at speed, radians.
    pub lean: f32,
}

impl Spec {
    /// The Transform's height over the ground: it stands at the box's centre.
    pub fn centre(&self) -> f32 {
        self.clear + self.size.y / 2.0
    }

    pub fn collider(&self) -> Collider {
        let s = self.size - Vec3::splat(2.0 * self.round);
        Collider::round_cuboid(s.x, s.y, s.z, self.round)
    }
}

/// TukTuk.io's numbers: a sluggish three-wheeler, 45 km/h flat out.
const AUTO: Spec = Spec {
    name: "Auto",
    model: "models/vehicles/auto.glb",
    law: Law { top: 12.5, reverse: 3.2, pull: 3.6, brake: 6.5, drag: 0.0042, turn: 2.0, grip: 3.5, calm: 0.45 },
    hp: 260.0,
    size: Vec3::new(1.15, 1.45, 2.7),
    clear: 0.25,
    round: 0.2,
    seat: Vec3::new(0.0, 0.85, -0.2),
    engine: Vec3::new(0.0, 0.55, 1.05),
    wheel: 0.231,
    lean: 0.05,
};
const SCOOTER: Spec = Spec {
    name: "Scooter",
    model: "models/vehicles/scooter.glb",
    law: Law { top: 15.0, reverse: 2.0, pull: 4.6, brake: 8.0, drag: 0.0042, turn: 2.5, grip: 2.5, calm: 0.5 },
    hp: 110.0,
    size: Vec3::new(0.5, 0.85, 1.75),
    clear: 0.15,
    round: 0.2,
    seat: Vec3::new(0.0, 0.74, 0.3),
    engine: Vec3::new(0.08, 0.4, 0.45),
    wheel: 0.229,
    lean: 0.36,
};
const COMMUTER: Spec = Spec {
    name: "Bike",
    model: "models/vehicles/commuter.glb",
    law: Law { top: 19.0, reverse: 2.0, pull: 5.2, brake: 9.0, drag: 0.0035, turn: 2.4, grip: 2.5, calm: 0.5 },
    hp: 130.0,
    size: Vec3::new(0.5, 0.9, 2.0),
    clear: 0.18,
    round: 0.2,
    seat: Vec3::new(0.0, 0.88, 0.25),
    engine: Vec3::new(0.0, 0.45, -0.1),
    wheel: 0.307,
    lean: 0.42,
};
/// The heavy bike: not as quick off the line as the commuter, a little faster flat out.
const BULLET: Spec = Spec {
    name: "Phatphati",
    model: "models/vehicles/bullet.glb",
    law: Law { top: 21.0, reverse: 2.0, pull: 4.8, brake: 8.5, drag: 0.0032, turn: 2.0, grip: 2.5, calm: 0.5 },
    hp: 170.0,
    size: Vec3::new(0.55, 0.9, 2.1),
    clear: 0.18,
    round: 0.2,
    seat: Vec3::new(0.0, 0.92, 0.3),
    engine: Vec3::new(0.0, 0.45, -0.05),
    wheel: 0.345,
    lean: 0.38,
};
const CAR: Spec = Spec {
    name: "Gaadi",
    model: "models/vehicles/car.glb",
    law: Law { top: 20.0, reverse: 5.0, pull: 4.4, brake: 9.0, drag: 0.0034, turn: 1.9, grip: 3.0, calm: 0.5 },
    hp: 320.0,
    size: Vec3::new(1.5, 1.2, 3.38),
    clear: 0.18,
    round: 0.25,
    seat: Vec3::new(-0.32, 0.52, 0.1),
    engine: Vec3::new(0.0, 0.8, -1.25),
    wheel: 0.3,
    lean: 0.04,
};

impl VehicleKind {
    pub const ALL: [VehicleKind; 5] = [VehicleKind::Auto, VehicleKind::Scooter, VehicleKind::Commuter, VehicleKind::Bullet, VehicleKind::Car];

    pub fn spec(self) -> &'static Spec {
        match self {
            VehicleKind::Auto => &AUTO,
            VehicleKind::Scooter => &SCOOTER,
            VehicleKind::Commuter => &COMMUTER,
            VehicleKind::Bullet => &BULLET,
            VehicleKind::Car => &CAR,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct Vehicle {
    pub kind: VehicleKind,
    /// Along the nose, m/s, minus in reverse.
    pub speed: f32,
    /// A yaw about +Y.
    pub heading: f32,
    /// Nose up, radians, from the ground under it.
    pub pitch: f32,
    pub input: DriveInput,
    pub hp: f32,
    pub driver: Option<Entity>,
    /// Burnt out: cover, and no more.
    pub wrecked: bool,
    /// Who hurt it last: the blast is theirs.
    pub hurt_by: Option<Entity>,
    grounded: bool,
    fall: f32,
    /// Who it hit lately, and until when (match time) they can't be hit again.
    rammed: Vec<(Entity, f32)>,
    /// Its pose at the last two ticks, for clients to draw between.
    pub before: (Vec3, Quat),
    pub now: (Vec3, Quat),
}

/// On a warrior at the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Riding(pub Entity);

/// A vehicle hit something hard enough to feel.
#[derive(Message, Clone, Copy, Debug)]
pub struct CrashMsg {
    pub at: Vec3,
    /// Speed lost, m/s.
    pub force: f32,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct WreckMsg {
    pub vehicle: Entity,
    pub at: Vec3,
}

/// Speed lost in one hit below this does nothing.
const CRASH: f32 = 3.5;
/// A vehicle's health lost per m/s of a crash, past `CRASH`.
const CRASH_HURT: f32 = 5.0;
/// The driver feels a crash past this.
const HARD_CRASH: f32 = 9.0;
const HARD_CRASH_HURT: f32 = 3.0;
/// Below this speed a vehicle hurts no one.
const RAM: f32 = 3.0;
/// Health a warrior loses per m/s of a hit, past `RAM`: an auto flat out takes about two thirds of
/// it, a bike flat out all of it.
const RAM_HURT: f32 = 7.0;
/// How near a warrior's middle has to be to a vehicle's box to get in.
const BOARD_REACH: f32 = 1.4;
/// A wreck's blast: reach and damage at its heart.
const BLAST: (f32, f32) = (4.0, 45.0);
/// What is parked, in turn: the roads' own mix.
const ROSTER: [VehicleKind; 8] = [
    VehicleKind::Commuter,
    VehicleKind::Auto,
    VehicleKind::Scooter,
    VehicleKind::Commuter,
    VehicleKind::Scooter,
    VehicleKind::Auto,
    VehicleKind::Bullet,
    VehicleKind::Car,
];

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CrashMsg>().add_message::<WreckMsg>().add_systems(
            FixedUpdate,
            (
                (park, board).chain().in_set(SimSet::Flow),
                (drive, carry, ram).chain().in_set(SimSet::Movement).after(super::movement::move_warriors),
                (hurt, wreck, drop_fallen).chain().in_set(SimSet::Damage),
            ),
        );
    }
}

/// Single player only for now: a peer would need the vehicles' state from their owners. VIVAD_VEHICLES=0 turns them off.
pub fn enabled(config: &MatchConfig, peers: &Peers) -> bool {
    !config.online && peers.all.len() == 1 && std::env::var("VIVAD_VEHICLES").as_deref() != Ok("0")
}

/// A vehicle standing at `ground` (a point on the road) facing `heading`.
pub fn spawn_vehicle(commands: &mut Commands, kind: VehicleKind, ground: Vec3, heading: f32) -> Entity {
    let spec = kind.spec();
    let tf = Transform::from_translation(ground + Vec3::Y * spec.centre()).with_rotation(Quat::from_rotation_y(heading));
    commands
        .spawn((
            Vehicle {
                kind,
                speed: 0.0,
                heading,
                pitch: 0.0,
                input: DriveInput::default(),
                hp: spec.hp,
                driver: None,
                wrecked: false,
                hurt_by: None,
                grounded: true,
                fall: 0.0,
                rammed: vec![],
                before: (tf.translation, tf.rotation),
                now: (tf.translation, tf.rotation),
            },
            tf,
            RigidBody::Kinematic,
            spec.collider(),
            // As with warriors: found by queries, never paired for contacts.
            CollisionLayers::new(Layer::Vehicle, LayerMask::NONE),
        ))
        .id()
}

/// Where vehicles may stand: along the kerbs, either side of the street kit's parked ones; on a map
/// with no street kit, anywhere on open ground. Shuffled by the match seed.
fn spots(layout: &MapLayout, parked: usize, rng: &mut crate::SmallRng) -> Vec<(Vec3, f32)> {
    let mut v = vec![];
    for k in layout.kit.iter().filter(|k| matches!(k.name, "auto" | "scooter")) {
        let along = Quat::from_rotation_y(k.yaw) * Vec3::NEG_Z;
        v.push((k.pos + along * 4.6, k.yaw));
        v.push((k.pos - along * 4.6, k.yaw));
    }
    v.shuffle(rng);
    if v.len() < parked {
        let reach = layout.half_extent * 0.9;
        for _ in 0..800 {
            let (x, z) = (rng.random_range(-reach..reach), rng.random_range(-reach..reach));
            v.push((Vec3::new(x, layout.terrain.height(x, z), z), rng.random_range(0.0..std::f32::consts::TAU)));
        }
    }
    v
}

/// Once per match, when the world's colliders can be asked about: this match's vehicles.
#[allow(clippy::too_many_arguments)]
fn park(
    mut commands: Commands,
    m: Res<Match>,
    config: Res<MatchConfig>,
    peers: Res<Peers>,
    layout: Res<MapLayout>,
    spatial: SpatialQuery,
    parked: Query<Entity, With<Vehicle>>,
    mut done: Local<u32>,
) {
    if *done == m.generation || m.elapsed < 0.3 {
        return;
    }
    *done = m.generation;
    for e in &parked {
        commands.entity(e).despawn();
    }
    if !enabled(&config, &peers) {
        return;
    }
    let mut rng = match_rng(m.seed, stream::VEHICLES);
    let filter = SpatialQueryFilter::from_mask(solid_mask());
    let parked = Rules::of(layout.style).parked;
    let mut taken: Vec<Vec2> = vec![];
    for (at, yaw) in spots(&layout, parked, &mut rng) {
        if taken.len() == parked {
            break;
        }
        if taken.iter().any(|p| p.distance(at.xz()) < 22.0) || matches!(layout.terrain.ground_at(at.x, at.z), Some(ground::WATER | ground::PARK)) {
            continue;
        }
        let kind = ROSTER[taken.len() % ROSTER.len()];
        let spec = kind.spec();
        // The real ground here: a slab or the terrain, and nothing in the way above it.
        let Some(hit) = spatial.cast_ray(at + Vec3::Y * 2.5, Dir3::NEG_Y, 4.0, true, &filter) else { continue };
        let foot = at + Vec3::Y * (2.5 - hit.distance);
        let rot = Quat::from_rotation_y(yaw);
        if hit.normal.y < 0.95 || !spatial.shape_intersections(&spec.collider(), foot + Vec3::Y * spec.centre(), rot, &filter).is_empty() {
            continue;
        }
        taken.push(at.xz());
        spawn_vehicle(&mut commands, kind, foot, yaw);
    }
    info!("vehicles: {} parked", taken.len());
}

/// Near enough to get in, from `me` (a warrior's middle)?
pub fn in_reach(me: Vec3, tf: &Transform, v: &Vehicle) -> bool {
    let local = tf.rotation.inverse() * (me - tf.translation);
    (local.abs() - v.kind.spec().size / 2.0).max(Vec3::ZERO).length() < BOARD_REACH
}

/// Beside the vehicle where a warrior fits: the left, the right, behind, in front, else on top.
fn step_out(spatial: &SpatialQuery, tf: &Transform, v: &Vehicle, driver: Entity) -> Vec3 {
    let spec = v.kind.spec();
    let foot = tf.translation - Vec3::Y * spec.centre();
    let filter = SpatialQueryFilter::from_mask(walk_mask()).with_excluded_entities([driver]);
    let body = Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH);
    let yaw = Quat::from_rotation_y(v.heading);
    let (x, z) = (spec.size.x / 2.0 + CAPSULE_RADIUS + 0.15, spec.size.z / 2.0 + CAPSULE_RADIUS + 0.15);
    for side in [Vec3::NEG_X * x, Vec3::X * x, Vec3::Z * z, Vec3::NEG_Z * z] {
        let at = foot + yaw * side + Vec3::Y * (HALF_HEIGHT + 0.08);
        if spatial.shape_intersections(&body, at, Quat::IDENTITY, &filter).is_empty() {
            return at;
        }
    }
    foot + Vec3::Y * (spec.clear + spec.size.y + HALF_HEIGHT + 0.1)
}

fn set_down(tf: &mut Transform, kin: &mut Kinematic, at: Vec3, velocity: Vec3) {
    tf.translation = at;
    kin.mode = MoveMode::Ground;
    kin.grounded = false;
    kin.velocity = velocity;
    kin.fall_from = Some(at.y);
}

/// The interact key at the wheel gets out; on foot, it gets in, if there is no loot in reach to
/// take first. Either way the key is used up, so the loot system doesn't also act on it.
#[allow(clippy::type_complexity)]
fn board(
    mut commands: Commands,
    mut warriors: Query<
        (Entity, &mut Transform, &mut Kinematic, &mut PlayerIntent, Option<&Riding>),
        (With<Warrior>, Without<Eliminated>, Without<Remote>, Without<BotBrain>, Without<Vehicle>),
    >,
    mut vehicles: Query<(Entity, &Transform, &mut Vehicle), Without<Warrior>>,
    chests: Query<(&Transform, &Chest), (Without<Warrior>, Without<Vehicle>)>,
    pickups: Query<&Transform, (With<Pickup>, Without<Warrior>, Without<Vehicle>)>,
    spatial: SpatialQuery,
) {
    for (e, mut tf, mut kin, mut intent, riding) in &mut warriors {
        if !intent.pressed(btn::INTERACT) {
            continue;
        }
        if let Some(&Riding(ride)) = riding {
            intent.pressed &= !btn::INTERACT;
            let Ok((_, vt, mut v)) = vehicles.get_mut(ride) else { continue };
            v.driver = None;
            let at = step_out(&spatial, vt, &v, e);
            set_down(&mut tf, &mut kin, at, vt.rotation * Vec3::NEG_Z * v.speed * 0.5);
            commands.entity(e).remove::<Riding>();
            continue;
        }
        if kin.mode != MoveMode::Ground || !kin.grounded {
            continue;
        }
        let me = tf.translation;
        let loot = chests.iter().any(|(t, c)| !c.opened && t.translation.distance(me) < INTERACT_RANGE) || pickups.iter().any(|t| t.translation.distance(me) < INTERACT_RANGE);
        if loot {
            continue;
        }
        let near = vehicles
            .iter_mut()
            .filter(|(_, t, v)| v.driver.is_none() && !v.wrecked && in_reach(me, t, v))
            .min_by(|a, b| a.1.translation.distance(me).total_cmp(&b.1.translation.distance(me)));
        if let Some((ve, _, mut v)) = near {
            v.driver = Some(e);
            kin.mode = MoveMode::Riding;
            kin.velocity = Vec3::ZERO;
            intent.pressed &= !btn::INTERACT;
            commands.entity(e).insert(Riding(ve));
        }
    }
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Every vehicle one tick: the law, then collide-and-slide, then the ground under it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn drive(
    mut vehicles: Query<(Entity, &mut Transform, &mut Vehicle, &Collider)>,
    drivers: Query<&PlayerIntent, (With<Warrior>, Without<Eliminated>)>,
    move_and_slide: MoveAndSlide,
    spatial: SpatialQuery,
    time: Res<Time>,
    layout: Res<MapLayout>,
    mut crashes: MessageWriter<CrashMsg>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    let config = MoveAndSlideConfig::default();
    let road = SpatialQueryFilter::from_mask(solid_mask());
    let wade = Rules::of(layout.style).wade_speed;
    for (e, mut tf, mut v, collider) in &mut vehicles {
        v.before = v.now;
        let held = v.driver.and_then(|d| drivers.get(d).ok()).filter(|_| !v.wrecked);
        if v.driver.is_some() && held.is_none() {
            v.driver = None;
        }
        if held.is_none() && v.speed == 0.0 && v.grounded {
            v.input = DriveInput::default();
            continue;
        }
        v.input = held.map_or(DriveInput::default(), |i| DriveInput {
            throttle: i.move_axis.y.max(0.0),
            brake: (-i.move_axis.y).max(0.0),
            steer: i.move_axis.x.clamp(-1.0, 1.0),
        });
        let spec = v.kind.spec();
        let (mut speed, mut heading) = (v.speed, v.heading);
        step(&spec.law, &mut speed, &mut heading, v.input, dt);
        // Water takes a vehicle down to a warrior's wading pace.
        if let Some(w) = wade.filter(|_| wading(&layout, tf.translation + Vec3::Y * (HALF_HEIGHT - spec.centre()))) {
            speed = speed.clamp(-w, w);
        }
        let yaw = Quat::from_rotation_y(heading);
        let flat = yaw * Vec3::NEG_Z;
        let rot = yaw * Quat::from_rotation_x(v.pitch);
        let mut want = rot * Vec3::NEG_Z * speed;
        if !v.grounded {
            v.fall -= GRAVITY * dt;
            want.y = v.fall;
        }
        let filter = SpatialQueryFilter::from_mask(walk_mask()).with_excluded_entities([e]);
        let mut struck = None;
        let out = move_and_slide.move_and_slide(collider, tf.translation, rot, want, time.delta(), &config, &filter, |hit| {
            struck = Some(hit.entity);
            MoveAndSlideHitResponse::Accept
        });
        tf.translation = out.position;
        let got = out.projected_velocity;
        let lost = (want - got).with_y(0.0).length();
        if struck.is_some() && lost > 0.02 {
            // What is left of the speed runs along the wall, and the nose comes round to follow it.
            speed = got.with_y(0.0).length() * speed.signum();
            if got.length() > 1.0 {
                let along = got * speed.signum();
                heading += wrap((-along.x).atan2(-along.z) - heading) * (dt * 6.0).min(1.0);
            }
        }
        if lost > CRASH {
            if let Some(hit) = struck {
                let amount = (lost - CRASH) * CRASH_HURT;
                v.hp -= amount;
                crashes.write(CrashMsg { at: tf.translation + flat * speed.signum() * spec.size.z / 2.0, force: lost });
                // Another vehicle takes the same; a door gives way in the end.
                damage.write(DamageMsg { target: hit, amount, source: v.driver, headshot: false, cause: DamageCause::Ram });
                if let Some(d) = v.driver.filter(|_| lost > HARD_CRASH) {
                    damage.write(DamageMsg { target: d, amount: (lost - HARD_CRASH) * HARD_CRASH_HURT, source: None, headshot: false, cause: DamageCause::Fall });
                }
            }
        }
        // The ground under the axles: the height to ride at, and the slope.
        let foot = tf.translation - Vec3::Y * spec.centre();
        let reach = spec.size.z * 0.35;
        let probe = |at: Vec3| {
            spatial.cast_ray(at + Vec3::Y, Dir3::NEG_Y, 1.8, true, &road).filter(|h| h.distance > 0.4 && h.normal.y > 0.6).map(|h| at.y + 1.0 - h.distance)
        };
        let under = match (probe(foot + flat * reach), probe(foot - flat * reach)) {
            (Some(a), Some(b)) => Some(((a + b) / 2.0, (a - b) / (2.0 * reach))),
            (Some(a), None) | (None, Some(a)) => Some((a, 0.0)),
            (None, None) => None,
        };
        match under {
            Some((y, slope)) if foot.y - y < 0.3 => {
                if !v.grounded && v.fall < -9.0 {
                    crashes.write(CrashMsg { at: foot, force: -v.fall });
                    v.hp -= (-v.fall - 9.0) * CRASH_HURT;
                }
                tf.translation.y += (y - foot.y) * (1.0 - (-dt * 28.0).exp());
                v.pitch += (slope.atan() - v.pitch) * (1.0 - (-dt * 14.0).exp());
                v.grounded = true;
                v.fall = 0.0;
            }
            _ => {
                if v.grounded {
                    v.fall = got.y;
                }
                v.grounded = false;
                v.pitch *= 1.0 - (dt * 1.5).min(1.0);
            }
        }
        // A wreck left rolling, or a vehicle driven off the map's floor, comes to rest.
        if tf.translation.y < -40.0 {
            v.hp = 0.0;
            v.grounded = true;
            speed = 0.0;
        }
        v.speed = speed;
        v.heading = heading;
        tf.rotation = Quat::from_rotation_y(heading) * Quat::from_rotation_x(v.pitch);
        v.now = (tf.translation, tf.rotation);
    }
}

/// Drivers sit where their vehicle has gone.
#[allow(clippy::type_complexity)]
fn carry(
    mut commands: Commands,
    vehicles: Query<(&Transform, &Vehicle), Without<Warrior>>,
    mut riders: Query<(Entity, &mut Transform, &mut Kinematic, &Riding), (With<Warrior>, Without<Eliminated>)>,
) {
    for (e, mut tf, mut kin, ride) in &mut riders {
        let Ok((vt, v)) = vehicles.get(ride.0) else {
            let at = tf.translation;
            set_down(&mut tf, &mut kin, at, Vec3::ZERO);
            commands.entity(e).remove::<Riding>();
            continue;
        };
        let spec = v.kind.spec();
        tf.translation = vt.translation + vt.rotation * (spec.seat - Vec3::Y * spec.centre());
        tf.rotation = Quat::from_rotation_y(v.heading);
        kin.velocity = vt.rotation * Vec3::NEG_Z * v.speed;
        kin.grounded = true;
        kin.fall_from = Some(tf.translation.y);
    }
}

/// A moving vehicle hits the warriors in its way, and throws them.
#[allow(clippy::type_complexity)]
fn ram(
    mut vehicles: Query<(&Transform, &mut Vehicle)>,
    mut warriors: Query<(Entity, &Transform, &mut Kinematic), (With<Warrior>, Without<Eliminated>, Without<Riding>, Without<Vehicle>)>,
    m: Res<Match>,
    mut damage: MessageWriter<DamageMsg>,
) {
    for (tf, mut v) in &mut vehicles {
        v.rammed.retain(|r| r.1 > m.elapsed);
        let moving = v.speed.abs();
        if moving < RAM {
            continue;
        }
        let spec = v.kind.spec();
        let half = spec.size / 2.0 + Vec3::new(CAPSULE_RADIUS, HALF_HEIGHT, CAPSULE_RADIUS);
        for (e, wt, mut kin) in &mut warriors {
            let local = tf.rotation.inverse() * (wt.translation - tf.translation);
            if local.abs().cmplt(half).all() && !v.rammed.iter().any(|r| r.0 == e) {
                v.rammed.push((e, m.elapsed + 1.0));
                damage.write(DamageMsg { target: e, amount: (moving - RAM) * RAM_HURT, source: v.driver, headshot: false, cause: DamageCause::Ram });
                let ahead = tf.rotation * Vec3::NEG_Z * v.speed.signum();
                let aside = (wt.translation - tf.translation).with_y(0.0).normalize_or(ahead);
                kin.velocity = (ahead + aside * 0.6) * moving * 0.8 + Vec3::Y * (3.0 + moving * 0.25);
                kin.grounded = false;
                // Thrown: lands without fall damage, as from a wind astra.
                kin.fall_from = None;
                v.speed *= 0.75;
            }
        }
    }
}

/// Blows and shots on vehicles (the warriors' own damage is `combat::apply_damage`).
fn hurt(mut msgs: MessageReader<DamageMsg>, mut vehicles: Query<(&Transform, &mut Vehicle)>, mut hits: MessageWriter<HitMsg>) {
    for msg in msgs.read() {
        let Ok((tf, mut v)) = vehicles.get_mut(msg.target) else { continue };
        if v.wrecked {
            continue;
        }
        v.hp -= msg.amount;
        v.hurt_by = msg.source.or(v.hurt_by);
        if let Some(src) = msg.source {
            hits.write(HitMsg { attacker: src, target: msg.target, amount: msg.amount, headshot: false, shield: true, at: tf.translation + Vec3::Y * 0.8 });
        }
    }
}

/// A vehicle with no health left blows: the driver is thrown out, those near it hurt, and it stays as a burnt shell.
#[allow(clippy::type_complexity)]
fn wreck(
    mut commands: Commands,
    mut vehicles: Query<(Entity, &Transform, &mut Vehicle), Without<Warrior>>,
    mut warriors: Query<(Entity, &mut Transform, &mut Kinematic), (With<Warrior>, Without<Eliminated>, Without<Remote>)>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageMsg>,
    mut wrecks: MessageWriter<WreckMsg>,
) {
    for (e, tf, mut v) in &mut vehicles {
        if v.wrecked || v.hp > 0.0 {
            continue;
        }
        v.wrecked = true;
        v.hp = 0.0;
        let at = tf.translation;
        if let Some(d) = v.driver.take() {
            if let Ok((_, mut wt, mut kin)) = warriors.get_mut(d) {
                let out = step_out(&spatial, tf, &v, d);
                set_down(&mut wt, &mut kin, out, Vec3::Y * 5.0);
            }
            commands.entity(d).remove::<Riding>();
        }
        for (we, wt, _) in &warriors {
            let near = 1.0 - wt.translation.distance(at) / BLAST.0;
            if near > 0.0 {
                damage.write(DamageMsg { target: we, amount: BLAST.1 * near, source: v.hurt_by, headshot: false, cause: DamageCause::Ram });
            }
        }
        wrecks.write(WreckMsg { vehicle: e, at });
    }
}

/// A driver who falls at the wheel drops to the ground beside it.
#[allow(clippy::type_complexity)]
fn drop_fallen(
    mut commands: Commands,
    mut fallen: Query<(Entity, &mut Transform, &Riding), (With<Warrior>, Added<Eliminated>)>,
    spatial: SpatialQuery,
) {
    let filter = SpatialQueryFilter::from_mask(solid_mask());
    for (e, mut tf, _) in &mut fallen {
        if let Some(hit) = spatial.cast_ray(tf.translation, Dir3::NEG_Y, 30.0, true, &filter) {
            tf.translation.y -= hit.distance - HALF_HEIGHT;
        }
        commands.entity(e).remove::<Riding>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_seat_inside_its_length_and_a_faster_top_than_a_sprint() {
        for k in VehicleKind::ALL {
            let s = k.spec();
            assert!(s.seat.z.abs() < s.size.z / 2.0, "{k:?}");
            assert!(s.law.top > super::super::movement::SPRINT_SPEED * 1.4, "{k:?}");
            assert!(s.size.min_element() >= 2.0 * s.round, "{k:?}");
        }
    }

    #[test]
    fn off_in_online_matches() {
        let alone = Peers::default();
        assert!(enabled(&MatchConfig { online: false, ..default() }, &alone));
        assert!(!enabled(&MatchConfig { online: true, ..default() }, &alone));
        assert!(!enabled(&MatchConfig { online: false, ..default() }, &Peers::new(1, vec![1, 2])));
    }

    #[test]
    fn wrap_stays_within_half_a_turn() {
        assert!((wrap(3.5 * std::f32::consts::PI) + 0.5 * std::f32::consts::PI).abs() < 1e-4);
        assert!((wrap(-0.2) + 0.2).abs() < 1e-6);
    }
}
