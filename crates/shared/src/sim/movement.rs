//! Kinematic character movement: Vimana ride -> freefall -> glide -> on foot.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{CAPSULE_LENGTH, CAPSULE_RADIUS, DamageCause, DamageMsg, Eliminated, Kinematic, MoveMode, PlayerIntent, SimSet, btn, solid_mask};

pub const GRAVITY: f32 = 22.0;
pub const JUMP_SPEED: f32 = 7.6;
pub const WALK_SPEED: f32 = 5.6;
pub const SPRINT_SPEED: f32 = 8.6;
/// A jump at a full sprint goes this much faster forward (and a little higher: RUN_JUMP_LIFT);
/// never past SPRINT_SPEED plus this, so hopping can't snowball.
pub const RUN_JUMP_BOOST: f32 = 1.0;
pub const RUN_JUMP_LIFT: f32 = 0.08;
/// A jump still works this long after running off an edge, and a press this long before landing
/// jumps on landing.
pub const COYOTE: f32 = 0.12;
pub const JUMP_BUFFER: f32 = 0.12;
/// Falling pulls harder than rising: a snappier arc, the same height.
pub const FALL_GRAVITY: f32 = 1.3;
/// Slowing in the air with no stick held (momentum otherwise kept).
pub const AIR_DRAG: f32 = 2.0;
pub const AIM_SPEED: f32 = 3.6;
/// Crouched: slower, steadier (crate::sim::combat), and shorter.
pub const CROUCH_SPEED: f32 = 2.4;
/// A slide: crouching while running fast throws the warrior into it at at least SLIDE_SPEED,
/// slowing by SLIDE_FRICTION (m/s each second; downhill, gravity along the slope speeds it up),
/// steered only a little; it ends crouched under SLIDE_END, and the next can start SLIDE_COOLDOWN
/// later. Jumping out keeps its speed.
pub const SLIDE_SPEED: f32 = 10.5;
pub const SLIDE_MAX: f32 = 14.0;
pub const SLIDE_FRICTION: f32 = 6.5;
pub const SLIDE_END: f32 = 3.6;
pub const SLIDE_STEER: f32 = 1.4;
pub const SLIDE_COOLDOWN: f32 = 0.5;
pub const GROUND_ACCEL: f32 = 70.0;
pub const AIR_ACCEL: f32 = 12.0;
pub const FREEFALL_MAX: f32 = 42.0;
pub const FREEFALL_HORIZ: f32 = 20.0;
pub const GLIDE_FALL: f32 = 6.5;
pub const GLIDE_HORIZ: f32 = 14.0;
/// Glider opens automatically at this height above ground.
pub const AUTO_GLIDE_HEIGHT: f32 = 40.0;
/// Landing faster than this hurts: a drop of 6.5 m (two storeys) is safe.
pub const SAFE_LANDING: f32 = 17.0;
/// Damage per m/s of landing speed over SAFE_LANDING: off a haveli's top roof (11 m) ~30, into
/// the stepwell from its rim (21 m) ~80, off the Ajanta cliff (24 m) ~95.
pub const FALL_DAMAGE: f32 = 6.0;
/// Steepest walkable slope (cosine of ~48 degrees).
const MIN_GROUND_NORMAL_Y: f32 = 0.66;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (move_warriors, remote_crouch).in_set(SimSet::Movement));
    }
}

pub fn wish_dir(intent: &PlayerIntent) -> Vec3 {
    let rot = intent.body_rotation();
    let fwd = rot * Vec3::NEG_Z;
    let right = rot * Vec3::X;
    let v = right * intent.move_axis.x + fwd * intent.move_axis.y;
    if v.length_squared() > 1.0 { v.normalize() } else { v }
}

fn approach(current: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let d = target - current;
    if d.length() <= max_delta { target } else { current + d.normalize() * max_delta }
}

fn ground_probe(spatial: &SpatialQuery, center: Vec3, entity: Entity, max: f32) -> Option<(f32, Vec3)> {
    let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([entity]);
    spatial
        .cast_ray(center, Dir3::NEG_Y, max, true, &filter)
        .map(|hit| (hit.distance, hit.normal))
}

/// Sweeps a sphere a little smaller than the capsule's base down from its centre: returns how far
/// the capsule's base is above the surface it would land on, and that surface's normal. Unlike a
/// single ray this sees what the capsule actually stands on at stair feet, ridges and ledges.
fn feet_probe(spatial: &SpatialQuery, center: Vec3, entity: Entity, max: f32) -> Option<(f32, Vec3)> {
    let shrink = 0.9;
    let r = CAPSULE_RADIUS * shrink;
    let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([entity]);
    let config = ShapeCastConfig { max_distance: max, ignore_origin_penetration: true, ..default() };
    let origin = center - Vec3::Y * (CAPSULE_LENGTH / 2.0);
    spatial.cast_shape(&Collider::sphere(r), origin, Quat::IDENTITY, Dir3::NEG_Y, &config, &filter).map(|hit| {
        let n = hit.normal1.normalize_or(Vec3::Y);
        (hit.distance - (CAPSULE_RADIUS - r) / n.y.max(0.2), n)
    })
}

#[allow(clippy::type_complexity)]
fn move_warriors(
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Kinematic, &PlayerIntent, &Collider), (Without<Eliminated>, Without<super::net::Remote>)>,
    move_and_slide: MoveAndSlide,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    let config = MoveAndSlideConfig::default();
    for (entity, mut tf, mut kin, intent, collider) in &mut q {
        tf.rotation = intent.body_rotation();
        let wish = wish_dir(intent);
        match kin.mode {
            MoveMode::Vimana => continue, // positioned by the flow system
            MoveMode::Freefall => {
                let target = wish * FREEFALL_HORIZ;
                let h = approach(kin.velocity.with_y(0.0), target, AIR_ACCEL * 2.0 * dt);
                let vy = (kin.velocity.y - GRAVITY * dt).max(-FREEFALL_MAX);
                kin.velocity = h.with_y(vy);
                let height = ground_probe(&spatial, tf.translation, entity, 2000.0).map(|h| h.0).unwrap_or(1000.0);
                if height < AUTO_GLIDE_HEIGHT || (intent.pressed(btn::JUMP) && height > 12.0) {
                    kin.mode = MoveMode::Gliding;
                }
            }
            MoveMode::Gliding => {
                let target = wish * GLIDE_HORIZ;
                let h = approach(kin.velocity.with_y(0.0), target, AIR_ACCEL * dt);
                let vy = approach(Vec3::Y * kin.velocity.y, Vec3::Y * -GLIDE_FALL, GRAVITY * dt).y;
                kin.velocity = h.with_y(vy);
            }
            MoveMode::Ground => {
                let crouch = intent.held(btn::CROUCH);
                let mut speed = if intent.held(btn::AIM) {
                    AIM_SPEED
                } else if intent.held(btn::SPRINT) && intent.move_axis.y > 0.3 {
                    SPRINT_SPEED
                } else {
                    WALK_SPEED
                };
                if kin.crouched {
                    speed = speed.min(CROUCH_SPEED);
                }
                kin.jump_buffer = if intent.pressed(btn::JUMP) { JUMP_BUFFER } else { (kin.jump_buffer - dt).max(0.0) };
                kin.air_time = if kin.grounded { 0.0 } else { kin.air_time + dt };
                kin.slide_cooldown = (kin.slide_cooldown - dt).max(0.0);
                let cur = kin.velocity.with_y(0.0);
                // Crouching while running fast on the ground: a slide.
                if crouch && intent.pressed(btn::CROUCH) && kin.grounded && !kin.sliding && kin.slide_cooldown <= 0.0 && cur.length() > WALK_SPEED - 0.3 {
                    kin.sliding = true;
                    kin.velocity = (cur.normalize() * cur.length().max(SLIDE_SPEED)).with_y(kin.velocity.y);
                }
                let cur = kin.velocity.with_y(0.0);
                let mut h = if kin.sliding && kin.grounded {
                    // Downhill speeds it up, friction slows it, the stick only bends it.
                    let n = kin.ground_normal;
                    let mut v = cur + Vec3::new(n.x, 0.0, n.z) * GRAVITY * 0.9 * dt;
                    let s = (v.length() - SLIDE_FRICTION * dt).max(0.0);
                    let mut dir = v.normalize_or_zero();
                    if wish != Vec3::ZERO && dir != Vec3::ZERO {
                        let turn = dir.angle_between(wish).min(SLIDE_STEER * dt);
                        let side = dir.cross(wish).y.signum();
                        dir = Quat::from_rotation_y(turn * side) * dir;
                    }
                    v = dir * s.min(SLIDE_MAX);
                    if s < SLIDE_END || !crouch {
                        kin.sliding = false;
                        kin.slide_cooldown = SLIDE_COOLDOWN;
                    }
                    v
                } else if kin.grounded {
                    approach(cur, wish * speed, GROUND_ACCEL * dt)
                } else if wish == Vec3::ZERO {
                    approach(cur, Vec3::ZERO, AIR_DRAG * dt)
                } else {
                    // Steer in the air without braking: a running jump keeps its speed.
                    approach(cur, wish * cur.length().max(speed), AIR_ACCEL * dt)
                };
                let mut vy = kin.velocity.y;
                if kin.jump_buffer > 0.0 && (kin.grounded || kin.air_time < COYOTE) {
                    // How much of a sprint this is: a running jump carries further and a bit higher.
                    let run = ((cur.length() - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0);
                    vy = JUMP_SPEED * (1.0 + RUN_JUMP_LIFT * run);
                    // The boost never takes a jump past a sprint's plus it; a slide's own speed is kept.
                    h = (h + h.normalize_or_zero() * RUN_JUMP_BOOST * run).clamp_length_max((SPRINT_SPEED + RUN_JUMP_BOOST).max(h.length()));
                    kin.sliding = false;
                    kin.grounded = false;
                    kin.jump_buffer = 0.0;
                    // No second jump off the same edge.
                    kin.air_time = COYOTE;
                } else if kin.grounded {
                    // Follow the ground plane so slopes and stair ramps don't sap speed.
                    let n = kin.ground_normal;
                    vy = if n.y > 0.0 { -(h.x * n.x + h.z * n.z) / n.y } else { 0.0 };
                } else {
                    vy -= GRAVITY * if vy < 0.0 { FALL_GRAVITY } else { 1.0 } * dt;
                }
                kin.velocity = h.with_y(vy);
            }
        }

        // Low while crouched or sliding; standing back up only with room overhead.
        let low = kin.mode == MoveMode::Ground && (intent.held(btn::CROUCH) || kin.sliding);
        let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([entity]);
        let want = low || (kin.crouched && !spatial.shape_intersections(&super::warrior_collider(false), tf.translation, Quat::IDENTITY, &filter).is_empty());
        if want != kin.crouched {
            // Swapped once this tick's queries are done (the spatial queries read every collider).
            kin.crouched = want;
            commands.entity(entity).insert(super::warrior_collider(want));
        }
        let out = move_and_slide.move_and_slide(
            collider,
            tf.translation,
            tf.rotation,
            kin.velocity,
            time.delta(),
            &config,
            &filter,
            |_| MoveAndSlideHitResponse::Accept,
        );
        tf.translation = out.position;
        // Keep the vertical component we asked for unless a surface stopped it.
        kin.velocity = out.projected_velocity;

        // Ground check and snapping (keeps us glued to ramps and stairs going down).
        let was_grounded = kin.grounded;
        let probe = feet_probe(&spatial, tf.translation, entity, 0.4);
        kin.grounded = false;
        if let Some((gap, normal)) = probe {
            let walkable = normal.y >= MIN_GROUND_NORMAL_Y;
            // Leaving the surface only by jumping: momentum up a ramp shouldn't launch us off its top.
            if walkable && (was_grounded || kin.velocity.dot(normal) <= 0.5) && (gap < 0.06 || (was_grounded && gap < 0.3)) {
                kin.grounded = true;
                kin.ground_normal = normal;
                tf.translation.y -= gap.max(0.0) - 0.01;
                kin.velocity.y = 0.0;
            }
        }
        // Fall damage by the height actually dropped (a warrior wedged on a ledge can build up
        // speed without moving), as the speed that drop would give. Gliding lands softly.
        let y = tf.translation.y;
        let peak = kin.fall_from.map(|f| f.max(y));
        if let (true, Some(peak)) = (kin.grounded, peak) {
            let landing = (2.0 * GRAVITY * (peak - y)).sqrt();
            if !was_grounded && landing > SAFE_LANDING {
                damage.write(DamageMsg { target: entity, amount: (landing - SAFE_LANDING) * FALL_DAMAGE, source: None, headshot: false, cause: DamageCause::Fall });
            }
        }
        kin.fall_from = if kin.grounded || kin.mode != MoveMode::Ground { Some(y) } else { peak };
        if kin.grounded && matches!(kin.mode, MoveMode::Freefall | MoveMode::Gliding) {
            kin.mode = MoveMode::Ground;
        }
        // Safety net: never fall through the world.
        if tf.translation.y < -40.0 {
            tf.translation.y = 30.0;
            kin.velocity = Vec3::ZERO;
            kin.mode = MoveMode::Gliding;
        }
    }
}

/// Other peers' warriors crouch with their crouch button (it comes over the network), so they're
/// as hard to hit behind low cover here as there.
fn remote_crouch(mut q: Query<(&PlayerIntent, &mut Kinematic, &mut Collider), With<super::net::Remote>>) {
    for (intent, mut kin, mut collider) in &mut q {
        let low = intent.held(btn::CROUCH);
        if low != kin.crouched {
            kin.crouched = low;
            *collider = super::warrior_collider(low);
        }
    }
}
