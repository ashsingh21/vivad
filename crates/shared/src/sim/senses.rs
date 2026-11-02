//! What a bot knows, as against what is so. A bot sees from its head, hears, and remembers: where
//! somebody went out of sight is all it has to go on.
//!
//! - **Sight.** Within its range, inside a 120 degree cone (or close by, whatever the facing), with a
//!   clear ray from its eye to a warrior's head or chest: a wall lower than the eye hides nothing. A
//!   crouched warrior is noticed at 60% of the range. Seeing takes a moment: a meter fills while
//!   somebody is in view, faster the nearer they are, and drains when they aren't.
//! - **Hearing.** Steps by speed, and every `NoiseMsg`: shots, startled birds (`birds`), and later
//!   anything else loud. Noise gives a place, not a sight line.
//! - **Memory.** What was sensed stays known for `MEMORY` seconds at the place it was last sensed. A
//!   bot that loses its target, is hit from out of sight or hears somebody close walks to that place,
//!   looks round, and gives up.
//!
//! `sense` keeps `BotBrain::senses`; `drive_bots` reads it. Nothing here knows any map's grid: the
//! physics world, the warriors, and whatever `drive_bots` is told about the ground underfoot.
//!
//! The rules follow shards-engine's `shards_npc/src/perception.rs`, Copyright (c) 2026 larelgit,
//! MIT OR Apache-2.0, and the detection meter saddle-ai-fov's `update_stimulus_entry`, Copyright (c)
//! 2025 Julien Blanchon, MIT-0 (docs/references.md).

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::bots::{ANSWER_RANGE, BotBrain};
use super::combat::Projectile;
use super::movement::WALK_SPEED;
use super::stance::Stance;
use super::{EYE_OFFSET, Eliminated, HALF_HEIGHT, Kinematic, MoveMode, PlayerIntent, Warrior, btn, sight_mask, solid_mask};
use crate::map::MapLayout;

/// The whole cone of sight.
pub const FOV_DEG: f32 = 120.0;
/// Inside this a bot notices anyone, whatever way it faces.
pub const NEAR: f32 = 4.0;
/// A crouched warrior is noticed at this share of a bot's range, and of `NEAR`.
pub const CROUCH_SIGHT: f32 = 0.6;
/// Leaves and washing hide nobody from this close.
pub const SOFT_NEAR: f32 = 5.0;
/// Seconds to notice somebody at the end of a bot's range, and right by it.
const NOTICE_FAR: f32 = 0.9;
const NOTICE_NEAR: f32 = 0.12;
/// Seconds for a full meter to drain out of view.
const FORGET_SIGHT: f32 = 2.0;
/// What a bot watching somebody keeps in view past its range.
const KEEP: f32 = 1.3;
/// How far a grudge sees, as far as a bot answers (past that it runs for cover), and the grudge time
/// left (of 6 s) while the blow still gives its maker away.
const GRUDGE_SIGHT: f32 = ANSWER_RANGE;
const GRUDGE_FRESH: f32 = 5.5;
/// Seconds a place stays known once nothing new has come from it.
pub const MEMORY: f32 = 8.0;
/// Seconds between looks.
const LOOK_EVERY: f32 = 0.2;
/// How far steps carry: a sprint, a walk, and a crouch or anything as slow.
pub const RUN_NOISE: f32 = 30.0;
pub const WALK_NOISE: f32 = 15.0;
pub const CROUCH_NOISE: f32 = 8.0;
/// How far an engine carries, flat out or ticking over.
pub const ENGINE_NOISE: f32 = 60.0;
/// How far a shot carries, a gulel's twang, and a matchlock's boom and cloud of smoke.
pub const SHOT_NOISE: f32 = 110.0;
const TWANG_NOISE: f32 = 28.0;
const TORADAR_NOISE: f32 = 260.0;
/// Close enough to the place to start looking round; seconds given to the walk, and to the looking.
const ARRIVE: f32 = 2.2;
const SEARCH_WALK: f32 = 16.0;
const SEARCH_LOOK: f32 = 3.5;
/// How far the head turns each way while looking round.
const SWEEP: f32 = 1.4;
/// Seconds after a search before a mere sound sends the bot off on another, and how near a sound has
/// to be to be worth the walk: a fight across town is heard and left alone, or every shot would
/// gather the lobby and a match would be over in five minutes.
const BORED: f32 = 20.0;
const CURIOUS: f32 = 45.0;
/// Most points one hiding search tries.
const HIDE_RAYS: usize = 48;

/// A sound in the world, heard by every bot within `radius`.
#[derive(Message, Clone, Copy, Debug)]
pub struct NoiseMsg {
    pub at: Vec3,
    pub radius: f32,
    /// Who made it; None for a sound from nobody in particular.
    pub source: Option<Entity>,
}

/// One warrior a bot knows of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sensed {
    pub who: Entity,
    /// Where it was last seen or heard.
    pub pos: Vec3,
    /// In sight at the last look (the meter is full and the way is clear).
    pub seen: bool,
    /// The sight meter: 0 to 1.
    pub signal: f32,
    /// Seconds since it was last seen or heard.
    pub age: f32,
}

/// Going to look: where, and whom for (None: a sound without a known source).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Search {
    pub spot: Vec3,
    pub who: Option<Entity>,
    /// Seconds left for this stage.
    pub left: f32,
    /// At the spot and looking round: from this yaw.
    pub looking: Option<f32>,
}

/// What a bot's senses have made it do so far, for `sim_check`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tally {
    /// Times it noticed somebody by sight.
    pub noticed: u32,
    /// Searches set off on; reached, to look round; broken off by finding somebody to fight.
    pub searches: u32,
    pub arrived: u32,
    pub found: u32,
    /// Times it ran from a fight, and of those, to somewhere out of its enemy's sight, or to
    /// somewhere that hides it crouched.
    pub flights: u32,
    pub hid: u32,
    pub low: u32,
}

/// What one bot knows. Part of `BotBrain`, so it goes where the brain goes.
#[derive(Clone, Debug, Default)]
pub struct Senses {
    pub known: Vec<Sensed>,
    /// A sound from nobody it can name, and how old.
    pub noise: Option<(Vec3, f32)>,
    /// The target it last fought: losing that one is worth a search.
    pub fought: Option<Entity>,
    pub search: Option<Search>,
    bored: f32,
    /// Seconds until the next look.
    timer: f32,
    pub tally: Tally,
}

impl Senses {
    /// A bot's first look comes at a moment of its own: all looking on one tick, 63 bots cast their
    /// rays together every `LOOK_EVERY`.
    pub fn staggered(rng: &mut impl rand::Rng) -> Self {
        Self { timer: rng.random_range(0.0..LOOK_EVERY), ..default() }
    }

    pub fn sees(&self, who: Entity) -> bool {
        self.known.iter().any(|k| k.who == who && k.seen)
    }

    fn get_mut(&mut self, who: Entity) -> &mut Sensed {
        let k = self.known.iter().position(|k| k.who == who).unwrap_or_else(|| {
            self.known.push(Sensed { who, pos: Vec3::ZERO, seen: false, signal: 0.0, age: 0.0 });
            self.known.len() - 1
        });
        &mut self.known[k]
    }

    /// Somebody was just sensed at `pos`.
    fn sensed(&mut self, who: Entity, pos: Vec3) {
        let k = self.get_mut(who);
        k.pos = pos;
        k.age = 0.0;
    }
}

/// The points of a warrior that can give him away, for a body centred at `center`.
pub fn body_points(center: Vec3, drop: f32) -> [Vec3; 2] {
    let head = center + Vec3::Y * (EYE_OFFSET - drop);
    [head, head - Vec3::Y * 0.45]
}

/// A warrior's eye.
pub fn eye(center: Vec3, drop: f32) -> Vec3 {
    center + Vec3::Y * (EYE_OFFSET - drop)
}

/// Whether `at` is inside the cone of an eye facing along `yaw`, in the flat.
pub fn in_cone(eye: Vec3, yaw: f32, at: Vec3) -> bool {
    let to = (at - eye).xz().normalize_or_zero();
    to.dot(Vec2::new(-yaw.sin(), -yaw.cos())) >= (FOV_DEG * 0.5).to_radians().cos() - 1e-5
}

/// Whether a warrior `distance` away is inside what a bot takes in: `range` ahead, `NEAR` all round.
pub fn in_view(eye: Vec3, yaw: f32, at: Vec3, range: f32, crouched: bool) -> bool {
    let share = if crouched { CROUCH_SIGHT } else { 1.0 };
    let distance = eye.xz().distance(at.xz());
    distance <= NEAR * share || (distance <= range * share && in_cone(eye, yaw, at))
}

/// The sight meter after `dt` seconds with somebody in view or not, `near` being 1 right by the bot
/// and 0 at the end of its range.
pub fn meter(signal: f32, visible: bool, near: f32, dt: f32) -> f32 {
    let change = if visible { dt / (NOTICE_FAR + (NOTICE_NEAR - NOTICE_FAR) * near.clamp(0.0, 1.0)) } else { -dt / FORGET_SIGHT };
    (signal + change).clamp(0.0, 1.0)
}

/// How far the steps of a warrior going at `speed` carry.
pub fn step_noise(speed: f32, crouched: bool) -> f32 {
    if speed < 0.8 {
        0.0
    } else if crouched || speed < 3.0 {
        CROUCH_NOISE
    } else if speed > WALK_SPEED + 0.5 {
        RUN_NOISE
    } else {
        WALK_NOISE
    }
}

/// How far off a warrior can be heard: by steps on foot, by the engine at the wheel, not at all in the air.
pub fn noise_of(kin: &Kinematic, crouched: bool) -> f32 {
    match kin.mode {
        MoveMode::Ground => step_noise(kin.velocity.xz().length(), crouched),
        MoveMode::Riding => ENGINE_NOISE,
        _ => -1.0,
    }
}

pub fn clear_line(spatial: &SpatialQuery, filter: &SpatialQueryFilter, eye: Vec3, point: Vec3) -> bool {
    let Ok((dir, len)) = Dir3::new_and_length(point - eye) else { return true };
    spatial.cast_ray(eye, dir, len, true, filter).is_none()
}

/// Whether an eye at `from` has a clear line to the head or chest of a warrior centred at `center`.
pub fn in_sight(spatial: &SpatialQuery, from: Vec3, center: Vec3, drop: f32) -> bool {
    let filter = SpatialQueryFilter::from_mask(if from.distance(center) < SOFT_NEAR { solid_mask() } else { sight_mask() });
    body_points(center, drop).iter().any(|p| clear_line(spatial, &filter, from, *p))
}

/// Where somebody at `from` can get out of sight of an eye at `threat`: the walkable point within
/// `within` metres farthest from the threat that it cannot see, on a warrior `drop` lower than
/// standing. None when each of the `HIDE_RAYS` points tried is in sight. `walkable` is whatever
/// knows this map's ground, and is asked only as far as the search gets: on the NCR it isn't cheap.
pub fn hiding_spot(spatial: &SpatialQuery, walkable: &impl Fn(Vec2) -> bool, layout: &MapLayout, threat: Vec3, from: Vec2, within: f32, drop: f32) -> Option<Vec2> {
    let mut points: Vec<(f32, Vec2)> = vec![];
    let mut r = 3.0;
    while r <= within {
        for k in 0..16 {
            let p = from + Vec2::from_angle(k as f32 / 16.0 * TAU) * r;
            points.push((p.distance(threat.xz()), p));
        }
        r += 3.0;
    }
    points.sort_by(|a, b| b.0.total_cmp(&a.0));
    points.into_iter().map(|(_, p)| p).filter(|p| walkable(*p)).take(HIDE_RAYS).find(|p| !in_sight(spatial, threat, Vec3::new(p.x, layout.terrain.height(p.x, p.y) + HALF_HEIGHT, p.y), drop))
}

/// Each new arrow or astra (here or an echo of another peer's) is a noise where it left the hand.
pub(crate) fn shot_noise(shots: Query<(&Transform, &Projectile), Added<Projectile>>, mut noise: MessageWriter<NoiseMsg>) {
    for (tf, shot) in &shots {
        let radius = match shot.item.as_str() {
            "gulel" => TWANG_NOISE,
            "toradar" => TORADAR_NOISE,
            _ => SHOT_NOISE,
        };
        noise.write(NoiseMsg { at: tf.translation, radius, source: Some(shot.owner) });
    }
}

/// Every look: who is in view, who can be heard, and how old the rest has grown. A sound is taken in
/// the tick it is made.
#[allow(clippy::type_complexity)]
pub(crate) fn sense(
    mut bots: Query<(Entity, &Transform, &PlayerIntent, &mut BotBrain, &Kinematic), Without<Eliminated>>,
    others: Query<(Entity, &Transform, &Kinematic, &Stance), (With<Warrior>, Without<Eliminated>)>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut noises: MessageReader<NoiseMsg>,
) {
    let dt = time.delta_secs();
    let noises: Vec<NoiseMsg> = noises.read().copied().collect();
    for (me, tf, intent, mut brain, kin) in &mut bots {
        let pos = tf.translation;
        let grudge = brain.provoked.map(|p| p.0);
        // The blow itself gives away where it came from; after that the grudge only looks further.
        let hurt_by = brain.provoked.filter(|p| p.1 > GRUDGE_FRESH).map(|p| p.0);
        let range = brain.sight.0;
        let senses = &mut brain.senses;
        for n in noises.iter().filter(|n| n.source != Some(me) && n.at.distance(pos) <= n.radius) {
            match n.source.filter(|s| others.contains(*s)) {
                Some(who) => senses.sensed(who, n.at),
                None => senses.noise = Some((n.at, 0.0)),
            }
        }
        senses.timer -= dt;
        senses.bored -= dt;
        if senses.timer > 0.0 {
            continue;
        }
        let step = LOOK_EVERY - senses.timer;
        senses.timer = LOOK_EVERY;
        for k in &mut senses.known {
            k.age += step;
        }
        if let Some(n) = &mut senses.noise {
            n.1 += step;
        }
        if kin.mode != MoveMode::Ground {
            continue;
        }
        let from = eye(pos, intent.drop());
        for (who, ot, okin, stance) in &others {
            if who == me {
                continue;
            }
            let (at, crouched) = (ot.translation, stance.crouched);
            let was_seen = senses.sees(who);
            let reach = if grudge == Some(who) { GRUDGE_SIGHT } else if was_seen { range * KEEP } else { range };
            let visible = in_view(from, intent.yaw, at, reach, crouched) && in_sight(&spatial, from, at, stance.drop());
            let heard = from.distance(at) <= noise_of(okin, crouched);
            let hurt = hurt_by == Some(who);
            let signal = senses.known.iter().find(|k| k.who == who).map_or(0.0, |k| k.signal);
            let near = 1.0 - from.distance(at) / reach.max(1.0);
            let next = if hurt && visible { 1.0 } else { meter(signal, visible, near, step) };
            if visible || heard || hurt || signal > 0.0 {
                let k = senses.get_mut(who);
                k.signal = next;
                k.seen = visible && next >= 1.0;
                let noticed = k.seen && !was_seen;
                if visible || heard || hurt {
                    k.pos = at;
                    k.age = 0.0;
                }
                senses.tally.noticed += noticed as u32;
            }
        }
        senses.known.retain(|k| k.age <= MEMORY && others.contains(k.who));
        if senses.noise.is_some_and(|n| n.1 > MEMORY) {
            senses.noise = None;
        }
    }
}

/// Decides whether to go and look for somebody, at a think tick. `armed`: a bot with nothing to shoot
/// doesn't go after sounds; `looting`: nor does one still arming itself. Losing a fight or being hit
/// from out of sight always counts. `snap` finds the walkable spot nearest a point, on this map's
/// ground.
pub fn consider_search(brain: &mut BotBrain, armed: bool, looting: bool, pos: Vec3, snap: &impl Fn(Vec2) -> Option<Vec2>) {
    let grudge = brain.provoked.map(|p| p.0);
    let (fleeing, target) = (brain.flee > 0.0, brain.target);
    let senses = &mut brain.senses;
    if let Some(t) = target {
        senses.fought = Some(t);
        senses.tally.found += senses.search.take().is_some() as u32;
        return;
    }
    if fleeing {
        senses.search = None;
        return;
    }
    if senses.search.is_some() {
        return;
    }
    let curious = armed && !looting && senses.bored <= 0.0;
    let close = |at: Vec3| at.distance(pos) < CURIOUS;
    let wanted = |k: &&Sensed| !k.seen && (Some(k.who) == senses.fought || Some(k.who) == grudge || (curious && close(k.pos)));
    let mut found = senses.known.iter().filter(wanted).min_by(|a, b| a.age.total_cmp(&b.age)).map(|k| (k.pos, Some(k.who)));
    if found.is_none() && curious {
        found = senses.noise.filter(|n| close(n.0)).map(|n| (n.0, None));
    }
    let Some((at, who)) = found else { return };
    // Somewhere a warrior can walk to, however high up they were heard.
    let spot = snap(at.xz()).map_or(at, |p| Vec3::new(p.x, at.y.min(pos.y), p.y));
    senses.search = Some(Search { spot, who, left: SEARCH_WALK, looking: None });
    senses.tally.searches += 1;
    brain.rest = 0.0;
}

/// After `drive_bots`: a bot at the place it came to look at stands there and turns its head, then
/// gives up and goes back to what it was doing.
#[allow(clippy::type_complexity)]
pub(crate) fn look_about(mut bots: Query<(&Transform, &mut BotBrain, &mut PlayerIntent), Without<Eliminated>>, time: Res<Time>) {
    let dt = time.delta_secs();
    for (tf, mut brain, mut intent) in &mut bots {
        let Some(mut search) = brain.senses.search else { continue };
        if brain.target.is_some() {
            continue;
        }
        if brain.urgent {
            brain.senses.search = None;
            continue;
        }
        search.left -= dt;
        if search.looking.is_none() && tf.translation.xz().distance(search.spot.xz()) < ARRIVE {
            search.looking = Some(intent.yaw);
            search.left = SEARCH_LOOK;
            brain.senses.tally.arrived += 1;
        }
        if let Some(yaw) = search.looking {
            let turned = 1.0 - search.left / SEARCH_LOOK;
            intent.yaw = yaw + SWEEP * (turned * TAU).sin();
            intent.move_axis = Vec2::ZERO;
            intent.held &= !btn::SPRINT;
            // Standing here on purpose: `drive_bots` must not take that for being stuck.
            brain.walking = false;
        }
        brain.senses.search = Some(search);
        if search.left <= 0.0 {
            let senses = &mut brain.senses;
            senses.search = None;
            senses.fought = None;
            senses.noise = None;
            senses.bored = BORED;
            senses.known.retain(|k| Some(k.who) != search.who);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHEAD: Vec3 = Vec3::new(0.0, 0.0, -20.0);

    #[test]
    fn the_cone_faces_along_yaw() {
        assert!(in_cone(Vec3::ZERO, 0.0, AHEAD));
        assert!(!in_cone(Vec3::ZERO, 0.0, -AHEAD), "behind");
        assert!(!in_cone(Vec3::ZERO, std::f32::consts::FRAC_PI_2, Vec3::new(20.0, 0.0, 0.0)), "turned away");
        assert!(in_cone(Vec3::ZERO, std::f32::consts::FRAC_PI_2, Vec3::new(-20.0, 0.0, 0.0)), "turned to it");
        let edge = Vec3::new(-(50f32.to_radians().sin()), 0.0, -(50f32.to_radians().cos()));
        assert!(in_cone(Vec3::ZERO, 0.0, edge * 10.0));
        let outside = Vec3::new(-(70f32.to_radians().sin()), 0.0, -(70f32.to_radians().cos()));
        assert!(!in_cone(Vec3::ZERO, 0.0, outside * 10.0));
    }

    #[test]
    fn a_crouch_cuts_what_is_taken_in() {
        assert!(in_view(Vec3::ZERO, 0.0, AHEAD * 1.5, 35.0, false));
        assert!(in_view(Vec3::ZERO, 0.0, AHEAD, 35.0, true), "20 m of 21");
        assert!(!in_view(Vec3::ZERO, 0.0, AHEAD * 1.5, 35.0, true), "30 m of 21");
        assert!(in_view(Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 3.0), 35.0, false), "close behind");
        assert!(!in_view(Vec3::ZERO, 0.0, Vec3::new(0.0, 0.0, 3.0), 35.0, true), "crouched past a back");
    }

    #[test]
    fn noticing_takes_a_moment() {
        let far = |n: u32| (0..n).fold(0.0, |s, _| meter(s, true, 0.0, LOOK_EVERY));
        assert!(far(3) < 1.0, "a far warrior isn't noticed at a glance");
        assert!(far(6) >= 1.0);
        assert!(meter(0.0, true, 1.0, LOOK_EVERY) >= 1.0, "one right by the bot is");
        assert!(meter(1.0, false, 1.0, LOOK_EVERY) < 1.0 && meter(1.0, false, 1.0, LOOK_EVERY) > 0.8, "it drains slowly");
    }

    /// 63 bots, as on the NCR: the look they all had on one tick spread over the ticks of a look.
    #[test]
    fn bots_look_on_ticks_of_their_own() {
        let mut rng = rand::rng();
        let mut on_tick = [0; 12];
        for _ in 0..63 {
            on_tick[((Senses::staggered(&mut rng).timer / LOOK_EVERY * 12.0) as usize).min(11)] += 1;
        }
        assert!(on_tick.iter().all(|n| *n < 24), "{on_tick:?}");
    }

    #[test]
    fn steps_carry_by_speed() {
        assert_eq!(step_noise(0.0, false), 0.0);
        assert_eq!(step_noise(2.6, true), CROUCH_NOISE);
        assert_eq!(step_noise(5.6, false), WALK_NOISE);
        assert_eq!(step_noise(7.4, false), RUN_NOISE);
    }

    #[test]
    fn an_engine_carries_past_a_sprint_and_the_air_is_silent() {
        let at = |mode, speed| Kinematic { mode, velocity: Vec3::X * speed, ..default() };
        assert_eq!(noise_of(&at(MoveMode::Ground, 7.4), false), RUN_NOISE);
        assert_eq!(noise_of(&at(MoveMode::Riding, 0.0), false), ENGINE_NOISE);
        assert!(ENGINE_NOISE > RUN_NOISE);
        assert!(noise_of(&at(MoveMode::Gliding, 14.0), false) < 0.0);
    }

    #[test]
    fn a_crouched_body_is_lower() {
        let [head, chest] = body_points(Vec3::ZERO, super::super::stance::CROUCH_DROP);
        assert!(head.y < 0.1 && chest.y < head.y);
    }
}
