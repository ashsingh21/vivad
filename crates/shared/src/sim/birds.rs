//! Birds as an alarm. Pigeons sit on the poles and the roof tanks. A warrior going by on his feet, or
//! a shot close by, puts them up, and every bot in earshot learns somebody is there; a crouch passes
//! under them. Each peer works this out for itself from the warriors it knows of: the flocks are not
//! sent, and only that peer's bots hear them.

use bevy::prelude::*;

use super::combat::Projectile;
use super::senses::NoiseMsg;
use super::stance::Stance;
use super::{Eliminated, Kinematic, MoveMode, OfMap, Warrior};
use crate::map::MapLayout;

/// How near (in the flat) a walk and a sprint put birds up, and a shot.
const STARTLE_WALK: f32 = 6.0;
const STARTLE_RUN: f32 = 10.0;
const STARTLE_SHOT: f32 = 30.0;
/// How far the clatter carries.
pub const FLOCK_NOISE: f32 = 40.0;
/// Seconds before they come back.
pub const AWAY: f32 = 25.0;
/// Each flock looks for walkers every this many ticks: a fifth of a second, in which no warrior
/// crosses what puts birds up. Every flock against every warrior each tick is what this saves.
const LOOK_TICKS: usize = 12;

/// A few birds on a perch.
#[derive(Component, Clone, Copy, Debug)]
pub struct Flock {
    pub at: Vec3,
    /// Seconds until they are back; 0 while they sit.
    pub away: f32,
}

/// Where flocks sit on a map: on every third pole's top, every fourth tank's lid.
pub fn perches(layout: &MapLayout) -> Vec<Vec3> {
    let on = |name: &'static str, every: usize, height: f32| {
        layout.kit.iter().filter(move |k| k.name == name).step_by(every).map(move |k| k.pos + Vec3::Y * height)
    };
    on("pole", 3, 9.0).chain(on("tanki", 4, 1.56)).collect()
}

pub fn spawn_flocks(commands: &mut Commands, layout: &MapLayout) {
    for at in perches(layout) {
        commands.spawn((Flock { at, away: 0.0 }, OfMap));
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn startle(
    mut flocks: Query<&mut Flock>,
    warriors: Query<(Entity, &Transform, &Kinematic, &Stance), (With<Warrior>, Without<Eliminated>)>,
    shots: Query<(&Transform, &Projectile), Added<Projectile>>,
    time: Res<Time>,
    mut turn: Local<usize>,
    mut noise: MessageWriter<NoiseMsg>,
) {
    let dt = time.delta_secs();
    *turn = (*turn + 1) % LOOK_TICKS;
    // Who can put birds up, worked out once a tick and not once a flock.
    let walkers: Vec<(Entity, Vec3, f32)> = warriors
        .iter()
        .filter_map(|(who, tf, kin, stance)| {
            let speed = kin.velocity.xz().length();
            (kin.mode == MoveMode::Ground && !stance.crouched && speed > 3.0).then_some((who, tf.translation, if speed > 6.0 { STARTLE_RUN } else { STARTLE_WALK }))
        })
        .collect();
    let shots: Vec<(Vec3, Entity)> = shots.iter().map(|(tf, p)| (tf.translation, p.owner)).collect();
    for (k, mut flock) in flocks.iter_mut().enumerate() {
        if flock.away > 0.0 {
            flock.away = (flock.away - dt).max(0.0);
            continue;
        }
        let at = flock.at;
        let near = |p: Vec3, r: f32| p.xz().distance(at.xz()) < r && (p.y - at.y).abs() < 12.0;
        let walker = if k % LOOK_TICKS == *turn { walkers.iter().find(|(_, p, r)| near(*p, *r)).map(|w| w.0) } else { None };
        // A shot is seen by every flock, in the one tick it is new: there are few of those.
        let shooter = shots.iter().find(|(p, _)| near(*p, STARTLE_SHOT)).map(|s| s.1);
        if let Some(source) = walker.or(shooter) {
            flock.away = AWAY;
            noise.write(NoiseMsg { at, radius: FLOCK_NOISE, source: Some(source) });
        }
    }
}
