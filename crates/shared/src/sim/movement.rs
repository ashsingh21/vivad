//! Kinematic character movement: Vimana ride -> freefall -> glide -> on foot.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::flow::Rules;
use super::stance::{CROUCH_SPEED, size_bodies, update_stance};
use super::{Action, CAPSULE_LENGTH, CAPSULE_RADIUS, DamageCause, DamageMsg, Eliminated, HALF_HEIGHT, Kinematic, MoveMode, PlayerIntent, SimSet, btn, solid_mask, walk_mask};
use crate::map::MapLayout;
use crate::map::terrain::ground;

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
/// Dazed by a heavy blow.
pub const STUN_SPEED: f32 = 1.4;
/// Walking (btn::WALK): a stroll.
pub const STROLL_SPEED: f32 = 1.8;

/// A gait's ground speed by direction (forward, back, left, right; m/s), for a body-relative
/// direction blended as the animation blends its clips (each share squared).
#[derive(Clone, Copy)]
pub struct Gait(pub [f32; 4]);

impl Gait {
    /// The speed along `(forward, right)` (any length; its direction is what counts).
    pub fn speed(&self, forward: f32, right: f32) -> f32 {
        let len = forward.hypot(right);
        if len < 1e-4 {
            return self.0[0];
        }
        let (f, r) = (forward / len, right / len);
        f.max(0.0).powi(2) * self.0[0] + (-f).max(0.0).powi(2) * self.0[1] + (-r).max(0.0).powi(2) * self.0[2] + r.max(0.0).powi(2) * self.0[3]
    }
}

/// Moving with a long gun: the speeds MoCap Online's rifle pack was captured at, so its clips play
/// at their own pace (sped up, mocap reads as fast-forward). Must match RIFLE_GAIT in
/// tools/blender/characters/anims.py.
pub const RIFLE_WALK: Gait = Gait([1.35, 1.12, 1.23, 1.27]);
pub const RIFLE_RUN: Gait = Gait([5.16, 2.72, 4.36, 4.88]);
pub const RIFLE_CROUCH: Gait = Gait([0.93, 0.92, 1.12, 1.02]);
pub const RIFLE_AIM_WALK: Gait = Gait([1.29, 1.19, 1.14, 1.10]);
pub const RIFLE_AIM_RUN: Gait = Gait([3.02, 2.52, 3.50, 3.97]);
pub const RIFLE_AIM_CROUCH: Gait = Gait([0.88, 0.84, 1.09, 0.98]);
/// The pack has no sprint: its run, a fifth quicker.
pub const RIFLE_SPRINT: f32 = 6.2;

/// Guns held in both hands (and moved with like a rifle): the one list.
pub fn long_gun(item: &str) -> bool {
    matches!(item, "dunali" | "bandook" | "vajra9" | "trishul_carbine" | "saintalis" | "thaana_slr" | "thri_not_thri" | "toradar")
}

/// A long gun's gait for this intent, or None to sprint.
pub fn rifle_gait(intent: &PlayerIntent, crouched: bool) -> Option<Gait> {
    let walk = intent.held(btn::WALK) && !intent.held(btn::SPRINT);
    Some(match (intent.held(btn::AIM), crouched, walk) {
        (true, true, _) => RIFLE_AIM_CROUCH,
        (true, false, true) => RIFLE_AIM_WALK,
        (true, false, false) => RIFLE_AIM_RUN,
        (false, true, _) => RIFLE_CROUCH,
        (false, false, true) => RIFLE_WALK,
        _ if intent.held(btn::SPRINT) && intent.move_axis.y > 0.3 => return None,
        _ => RIFLE_RUN,
    })
}
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
/// A body has weight (m/s each second): up to a run in a fifth of a second, a stop a little quicker,
/// and past a run a sprint builds slowly. Turning stays quick at any speed (`on_foot`).
pub const GROUND_ACCEL: f32 = 30.0;
pub const GROUND_BRAKE: f32 = 40.0;
pub const SPRINT_RAMP: f32 = 8.0;
pub const AIR_ACCEL: f32 = 12.0;
pub const FREEFALL_MAX: f32 = 42.0;
pub const FREEFALL_HORIZ: f32 = 20.0;
pub const GLIDE_FALL: f32 = 6.5;
pub const GLIDE_HORIZ: f32 = 14.0;
/// Glider opens automatically at this height above ground.
pub const AUTO_GLIDE_HEIGHT: f32 = 40.0;
/// Above this height a warrior can open the glider by hand.
pub const OPEN_GLIDE_HEIGHT: f32 = 12.0;
/// Landing faster than this hurts: a drop of 6.5 m (two storeys) is safe.
pub const SAFE_LANDING: f32 = 17.0;
/// Damage per m/s of landing speed over SAFE_LANDING: off a haveli's top roof (11 m) ~30, into
/// the stepwell from its rim (21 m) ~80, off the Ajanta cliff (24 m) ~95.
pub const FALL_DAMAGE: f32 = 6.0;
/// Steepest walkable slope (cosine of ~48 degrees).
const MIN_GROUND_NORMAL_Y: f32 = 0.66;

/// How far a warrior carries on the glider alone, opened at `height` above the ground.
pub fn glide_reach(height: f32) -> f32 {
    (height - AUTO_GLIDE_HEIGHT).max(0.0) / GLIDE_FALL * GLIDE_HORIZ
}

/// How far a warrior carries in a dive from `height`, the glider opening by itself at the last.
pub fn dive_reach(height: f32) -> f32 {
    let fall = (height - AUTO_GLIDE_HEIGHT).max(0.0);
    let to_top_speed = FREEFALL_MAX / GRAVITY;
    let run_up = 0.5 * GRAVITY * to_top_speed * to_top_speed;
    let seconds = if fall > run_up { to_top_speed + (fall - run_up) / FREEFALL_MAX } else { (2.0 * fall / GRAVITY).sqrt() };
    seconds * FREEFALL_HORIZ + height.min(AUTO_GLIDE_HEIGHT) / GLIDE_FALL * GLIDE_HORIZ
}

/// Whether a warrior at `center` has its feet on a river's bed, not on a bridge over it.
pub(super) fn wading(layout: &MapLayout, center: Vec3) -> bool {
    layout.terrain.ground_at(center.x, center.z) == Some(ground::WATER) && center.y - HALF_HEIGHT < layout.terrain.height(center.x, center.z) + 0.5
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (update_stance, size_bodies, move_warriors).chain().in_set(SimSet::Movement));
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

/// One tick of a warrior's feet, from `cur` toward `want`, `run` being its running pace.
fn on_foot(cur: Vec3, want: Vec3, run: f32, dt: f32) -> Vec3 {
    let (now, then) = (cur.length(), want.length());
    if then > now && now >= run {
        // Into a sprint: only the speed builds slowly, not the turn.
        return approach(cur, want.normalize() * (now + SPRINT_RAMP * dt).min(then), GROUND_BRAKE * dt);
    }
    let rate = if then <= now || cur.dot(want) < 0.0 { GROUND_BRAKE } else { GROUND_ACCEL };
    approach(cur, want, rate * dt)
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
    let filter = SpatialQueryFilter::from_mask(walk_mask()).with_excluded_entities([entity]);
    let config = ShapeCastConfig { max_distance: max, ignore_origin_penetration: true, ..default() };
    let origin = center - Vec3::Y * (CAPSULE_LENGTH / 2.0);
    spatial.cast_shape(&Collider::sphere(r), origin, Quat::IDENTITY, Dir3::NEG_Y, &config, &filter).map(|hit| {
        let n = hit.normal1.normalize_or(Vec3::Y);
        (hit.distance - (CAPSULE_RADIUS - r) / n.y.max(0.2), n)
    })
}

#[allow(clippy::type_complexity)]
pub(super) fn move_warriors(
    mut q: Query<(Entity, &mut Transform, &mut Kinematic, &PlayerIntent, &Collider, &Action, Option<&super::Inventory>), (Without<Eliminated>, Without<super::net::Remote>)>,
    move_and_slide: MoveAndSlide,
    spatial: SpatialQuery,
    time: Res<Time>,
    layout: Res<MapLayout>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let dt = time.delta_secs();
    let config = MoveAndSlideConfig::default();
    let rules = Rules::of(layout.style);
    for (entity, mut tf, mut kin, intent, collider, act, inv) in &mut q {
        tf.rotation = intent.body_rotation();
        let wish = wish_dir(intent);
        match kin.mode {
            MoveMode::Vimana => continue, // positioned by the flow system
            MoveMode::Riding => continue, // carried by crate::sim::vehicle
            MoveMode::Freefall => {
                let target = wish * FREEFALL_HORIZ;
                let h = approach(kin.velocity.with_y(0.0), target, AIR_ACCEL * 2.0 * dt);
                let vy = (kin.velocity.y - GRAVITY * dt).max(-FREEFALL_MAX);
                kin.velocity = h.with_y(vy);
                let height = ground_probe(&spatial, tf.translation, entity, 2000.0).map(|h| h.0).unwrap_or(1000.0);
                if height < AUTO_GLIDE_HEIGHT || (intent.pressed(btn::JUMP) && height > OPEN_GLIDE_HEIGHT) {
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
                // Off a tall roof the glider comes back: by itself when the fall is long, by hand
                // otherwise. Without it a 60 m tower is a 200 damage death trap.
                if rules.roof_rescue && !kin.grounded && kin.velocity.y < 0.0 {
                    let height = ground_probe(&spatial, tf.translation, entity, 2000.0).map_or(1000.0, |h| h.0);
                    if height > AUTO_GLIDE_HEIGHT || (intent.pressed(btn::JUMP) && height > OPEN_GLIDE_HEIGHT) {
                        kin.mode = MoveMode::Gliding;
                    }
                }
                let crouch = intent.held(btn::CROUCH);
                let rifle = inv.and_then(|i| i.current()).is_some_and(|s| long_gun(&s.id));
                let mut speed = if act.stun > 0.0 {
                    STUN_SPEED
                } else if rifle {
                    // At the pace the rifle clips were captured at, by gait and direction.
                    match rifle_gait(intent, kin.crouched) {
                        Some(g) => g.speed(intent.move_axis.y, intent.move_axis.x),
                        None => RIFLE_SPRINT,
                    }
                } else if intent.held(btn::WALK) && !intent.held(btn::SPRINT) {
                    STROLL_SPEED
                } else if intent.held(btn::AIM) {
                    AIM_SPEED
                } else if intent.held(btn::SPRINT) && intent.move_axis.y > 0.3 {
                    SPRINT_SPEED
                } else {
                    WALK_SPEED
                };
                if kin.crouched && !rifle {
                    speed = speed.min(CROUCH_SPEED);
                }
                if let Some(wade) = rules.wade_speed.filter(|_| wading(&layout, tf.translation)) {
                    speed = speed.min(wade);
                }
                let run_speed = if rifle { RIFLE_RUN.0[0] } else { WALK_SPEED };
                kin.jump_buffer = if intent.pressed(btn::JUMP) { JUMP_BUFFER } else { (kin.jump_buffer - dt).max(0.0) };
                kin.air_time = if kin.grounded { 0.0 } else { kin.air_time + dt };
                kin.slide_cooldown = (kin.slide_cooldown - dt).max(0.0);
                let cur = kin.velocity.with_y(0.0);
                // Crouching while running fast on the ground: a slide.
                if crouch && intent.pressed(btn::CROUCH) && kin.grounded && !kin.sliding && kin.slide_cooldown <= 0.0 && cur.length() > run_speed - 0.3 {
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
                    on_foot(cur, wish * speed, run_speed, dt)
                } else if wish == Vec3::ZERO {
                    approach(cur, Vec3::ZERO, AIR_DRAG * dt)
                } else {
                    // Steer in the air without braking: a running jump keeps its speed.
                    approach(cur, wish * cur.length().max(speed), AIR_ACCEL * dt)
                };
                let mut vy = kin.velocity.y;
                if kin.jump_buffer > 0.0 && !crouch && (kin.grounded || kin.air_time < COYOTE) {
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

        let filter = SpatialQueryFilter::from_mask(walk_mask()).with_excluded_entities([entity]);
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
