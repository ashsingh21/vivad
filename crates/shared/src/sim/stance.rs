//! Crouching: a warrior's stance and the rules that move it.
//!
//! - **Feet stay.** The capsule shrinks from the top, so the Transform, and all that measures from
//!   it, stays where it was. Every height on the body (the eye, the muzzle, the head) drops by
//!   `CROUCH_DROP`: `PlayerIntent::drop` and `Stance::drop`.
//! - **Standing up waits for room.** A warrior under something stays down (the crouched head cast up
//!   by what it lacks: the check in bevy_ahoy's `handle_crouching`).
//! - **The stance travels in `held`.** After this system runs, `btn::CROUCH` in `held` is the stance
//!   itself (a warrior who can't stand yet has it held for him), so `send_state` carries it and
//!   every peer sizes its copy from it, with no new field on the wire.
//!
//! The rules follow shards-engine's `shards_motion/src/controller/stance.rs`,
//! Copyright (c) 2026 larelgit, MIT OR Apache-2.0 (docs/references.md).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::net::Remote;
use super::{CAPSULE_LENGTH, CAPSULE_RADIUS, Eliminated, Kinematic, MoveMode, PlayerIntent, Warrior, btn, solid_mask};

/// How much shorter a crouched warrior is: 1.76 m standing, 1.10 m down.
pub const CROUCH_DROP: f32 = 0.66;
pub const CROUCH_SPEED: f32 = 2.6;

#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
pub struct Stance {
    pub crouched: bool,
}

impl Stance {
    /// How far every height on the body has dropped.
    pub fn drop(&self) -> f32 {
        if self.crouched { CROUCH_DROP } else { 0.0 }
    }
}

pub fn standing_capsule() -> Collider {
    Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH)
}

/// The standing capsule's bottom end, in the same place, with the top brought down.
pub fn crouched_capsule() -> Collider {
    let bottom = -CAPSULE_LENGTH / 2.0;
    Collider::capsule_endpoints(CAPSULE_RADIUS, Vec3::Y * bottom, Vec3::Y * (bottom + CAPSULE_LENGTH - CROUCH_DROP))
}

/// Whether a crouched warrior at `center` has room to stand. The sweep is a little narrower than the
/// body, so a wall beside him is no ceiling.
fn room_to_stand(spatial: &SpatialQuery, center: Vec3, me: Entity) -> bool {
    let r = CAPSULE_RADIUS * 0.9;
    let head = center + Vec3::Y * (CAPSULE_LENGTH / 2.0 - CROUCH_DROP);
    let config = ShapeCastConfig { max_distance: CROUCH_DROP + CAPSULE_RADIUS - r, ignore_origin_penetration: true, ..default() };
    let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([me]);
    spatial.cast_shape(&Collider::sphere(r), head, Quat::IDENTITY, Dir3::Y, &config, &filter).is_none()
}

/// Before anyone moves this tick: warriors go down as asked and up when there is room.
#[allow(clippy::type_complexity)]
pub(crate) fn update_stance(
    mut q: Query<(Entity, &Transform, &mut PlayerIntent, &mut Stance, &Kinematic, Has<Remote>), (With<Warrior>, Without<Eliminated>)>,
    spatial: SpatialQuery,
) {
    for (entity, tf, mut intent, mut stance, kin, remote) in &mut q {
        let mut down = intent.held(btn::CROUCH) && kin.mode == MoveMode::Ground;
        // A copy of another peer's warrior is as its owner says.
        if stance.crouched && !down && !remote && kin.mode == MoveMode::Ground && !room_to_stand(&spatial, tf.translation, entity) {
            down = true;
        }
        if down {
            intent.held |= btn::CROUCH;
        } else {
            intent.held &= !btn::CROUCH;
        }
        if down != stance.crouched {
            stance.crouched = down;
        }
    }
}

/// Right after: the body takes the stance's shape (apart, as `SpatialQuery` reads colliders).
pub(crate) fn size_bodies(mut q: Query<(&Stance, &mut Collider), Changed<Stance>>) {
    for (stance, mut collider) in &mut q {
        *collider = if stance.crouched { crouched_capsule() } else { standing_capsule() };
    }
}
