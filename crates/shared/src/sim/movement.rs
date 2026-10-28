//! Kinematic character movement: Vimana ride -> freefall -> glide -> on foot.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{Eliminated, HALF_HEIGHT, Kinematic, MoveMode, PlayerIntent, SimSet, btn, solid_mask};

pub const GRAVITY: f32 = 22.0;
pub const JUMP_SPEED: f32 = 7.6;
pub const WALK_SPEED: f32 = 5.6;
pub const SPRINT_SPEED: f32 = 7.4;
pub const AIM_SPEED: f32 = 3.6;
pub const GROUND_ACCEL: f32 = 70.0;
pub const AIR_ACCEL: f32 = 12.0;
pub const FREEFALL_MAX: f32 = 42.0;
pub const FREEFALL_HORIZ: f32 = 20.0;
pub const GLIDE_FALL: f32 = 6.5;
pub const GLIDE_HORIZ: f32 = 14.0;
/// Glider opens automatically at this height above ground.
pub const AUTO_GLIDE_HEIGHT: f32 = 40.0;
/// Steepest walkable slope (cosine of ~48 degrees).
const MIN_GROUND_NORMAL_Y: f32 = 0.66;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, move_warriors.in_set(SimSet::Movement));
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

#[allow(clippy::type_complexity)]
fn move_warriors(
    mut q: Query<(Entity, &mut Transform, &mut Kinematic, &PlayerIntent, &Collider), Without<Eliminated>>,
    move_and_slide: MoveAndSlide,
    spatial: SpatialQuery,
    time: Res<Time>,
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
                let speed = if intent.held(btn::AIM) {
                    AIM_SPEED
                } else if intent.held(btn::SPRINT) && intent.move_axis.y > 0.3 {
                    SPRINT_SPEED
                } else {
                    WALK_SPEED
                };
                let accel = if kin.grounded { GROUND_ACCEL } else { AIR_ACCEL };
                let h = approach(kin.velocity.with_y(0.0), wish * speed, accel * dt);
                let mut vy = kin.velocity.y;
                if kin.grounded {
                    vy = vy.max(0.0);
                    if intent.pressed(btn::JUMP) {
                        vy = JUMP_SPEED;
                        kin.grounded = false;
                    }
                } else {
                    vy -= GRAVITY * dt;
                }
                kin.velocity = h.with_y(vy);
            }
        }

        let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([entity]);
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
        let probe = ground_probe(&spatial, tf.translation, entity, HALF_HEIGHT + 0.35);
        kin.grounded = false;
        if let Some((dist, normal)) = probe {
            let gap = dist - HALF_HEIGHT;
            let walkable = normal.y >= MIN_GROUND_NORMAL_Y;
            if walkable && kin.velocity.y <= 0.5 && (gap < 0.06 || (was_grounded && gap < 0.3)) {
                kin.grounded = true;
                tf.translation.y -= gap.max(0.0) - 0.01;
                kin.velocity.y = 0.0;
            }
        }
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
