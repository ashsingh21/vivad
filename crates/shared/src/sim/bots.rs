//! Bot warriors. They only write `PlayerIntent`, exactly like a human controller would.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::aim::{self, AimSkill, Seen};
use super::flow::{Match, Rules};
use super::loot::{Chest, INTERACT_RANGE, Pickup};
use super::zone::Zone;
use super::{Action, BotSkill, 
    EYE_OFFSET, Eliminated, HALF_HEIGHT, Health, HitMsg, Inventory, Kinematic, MoveMode, PlayerIntent, SimSet, Warrior, btn, solid_mask,
};
use super::movement::{FALL_DAMAGE, GLIDE_FALL, GLIDE_HORIZ, GRAVITY, OPEN_GLIDE_HEIGHT, RIFLE_SPRINT, SAFE_LANDING, SPRINT_SPEED, WALK_SPEED, dive_reach, long_gun};
use super::nav::{NavGrid, NoPath, StreetFloor, on_bridge};
use super::senses::{self, Senses};
use super::stance::Stance;
use crate::items::{AstraEffect, ItemDb, ItemKind};
use crate::map::MapLayout;

/// Whole-map routes worked out in one tick: every bot wanting a new one at the same moment (the
/// ring moving on) must not stall the tick.
const ROUTES_PER_TICK: u32 = 2;
/// Seconds before a route is worked out afresh, the goal being the same.
const ROUTE_EVERY: f32 = 15.0;
/// Metres between a route's points.
const ROUTE_STEP: f32 = 8.0;
/// Street searches in one tick: a stretch of route is cheap, but a whole lobby at once isn't.
const STRETCHES_PER_TICK: u32 = 6;
/// Route points ahead that a stretch aims for, and the shorter stretch tried when that fails.
const STRETCH: [usize; 2] = [5, 2];
/// Route points a bot looks ahead of itself to find where it has got to.
const ROUTE_LOOK: usize = 12;
/// Seconds before a stretch is planned afresh, though it isn't finished.
const STRETCH_EVERY: f32 = 2.5;
/// How much longer the way through the streets is than the straight line, near enough.
const STREET_DETOUR: f32 = 1.3;
/// The same on the NCR, most of it open land: `nav_check` measures 1.11 to 1.14 to a ring's centre.
const OPEN_DETOUR: f32 = 1.15;
/// How far inside the coming circle's edge a bot on the NCR means to be when the ring gets there,
/// metres. A share of the radius gave a bot a minute in hand at the big rings, and they never
/// touched anyone.
const OPEN_MARGIN: f32 = 15.0;
/// Bots leaping from the vimana spread out: none makes for a chest this close to another's.
const CLAIM: f32 = 45.0;
/// How far through the flight a bot still waits for loot to come within a dive.
const LAST_LEAP: f32 = 0.92;
/// Seconds after landing that a bot loots before it hunts: it still fights whoever hits it.
/// A chill bot takes its time, a sharp one less.
const LOOT_GRACE: (f32, f32) = (75.0, 30.0);
/// Seconds a bot keeps the same aim error before it drifts to another.
const AIM_HOLD: (f32, f32) = (0.3, 0.9);
/// Below this much health a bot breaks off a fight and runs, unless the ring is at its back:
/// a fight to the death every time emptied the lobby in four minutes.
const FLEE_HP: f32 = 40.0;
/// Seconds to spare at which a bot sets off for the coming circle, and at which one on its way
/// turns back to looting: apart, so that walking in doesn't undo the decision to walk in.
const LEAVE_AT: (f32, f32) = (12.0, 30.0);
/// How much finer than that a bot may cut it (`BotBrain::nerve`), the most careful first.
const NERVE: (f32, f32) = (-6.0, 24.0);
/// How far out in the coming circle, as a share of its radius, a bot sets off for its middle, and
/// how far in it stops: apart for the same reason.
const CIRCLE_LINE: (f32, f32) = (0.85, 0.7);
/// Seconds to spare at which the ring is at a bot's back, and at which it no longer is.
const PRESSED_AT: (f32, f32) = (4.0, 10.0);
/// Seconds a bot keeps in hand when it fetches something on the way in.
const FETCH_SPARE: f32 = 8.0;
/// A bot with no bow takes one this close (metres) whatever the ring is doing: on the NCR a gun in
/// reach is worth more than a few seconds of the early rings' fire.
const GRAB_GUN: f32 = 20.0;
/// How near a door must be for a walled-in bot to leave by it, metres, and for how many seconds it
/// tries.
const DOOR_NEAR: f32 = 16.0;
const DOOR_TRIES: f32 = 8.0;
/// How far off a bot with no bow makes for one on the NCR, metres: chests are hundreds of metres
/// apart there, and a bot that landed between them had nothing else to look for.
const GUN_SIGHT: f32 = 220.0;
/// How far a bot with no bow goes for a street weapon, metres.
const STOOP: f32 = 6.0;
/// A bot shoots back at whoever hit it out to here; from farther off it runs for cover instead.
pub(super) const ANSWER_RANGE: f32 = 120.0;
/// Seconds after a foe in reach starts a swing that a bot keeps its guard up.
const GUARD_READ: f32 = 0.4;

#[derive(Component, Clone, Debug)]
pub struct BotBrain {
    /// Fraction of the flight at which this bot leaps.
    pub drop_at: f32,
    pub goal: Option<Vec3>,
    pub target: Option<Entity>,
    pub think: f32,
    pub stuck_check: (Vec3, f32),
    pub unstick: f32,
    pub strafe: f32,
    pub strafe_timer: f32,
    /// Aim error in radians; reaction delay before firing at a new target.
    pub aim_error: f32,
    pub react: f32,
    /// How this bot turns and sees; what it has seen of its target; where its view points; the
    /// error it is aiming with, and for how much longer.
    pub aim: AimSkill,
    pub memory: Seen,
    pub view: (f32, f32),
    pub aim_off: Vec2,
    pub aim_hold: f32,
    /// How far an archer (or a swordsman) picks a fight, metres.
    pub sight: (f32, f32),
    /// Share of the time spent attacking once engaged (the rest is drawing breath), and this
    /// bot's phase in that rhythm.
    pub tempo: (f32, f32),
    /// Chill bots walk rather than sprint, sidestep lazily and stop a while at each spot.
    pub calm: bool,
    /// Seconds left standing about before wandering on; whether the zone is pushing us.
    pub rest: f32,
    pub urgent: bool,
    pub seen_for: f32,
    /// Whoever hit us recently, and for how much longer we hold the grudge.
    pub provoked: Option<(Entity, f32)>,
    /// Goals we failed to reach (blocked); ignored from now on.
    pub give_up: Vec<Vec3>,
    pub goal_time: f32,
    /// The way to the goal around buildings (corners, in x/z), the goal it was planned for, and
    /// seconds until it is planned afresh.
    pub path: Vec<Vec2>,
    pub path_for: Option<Vec3>,
    pub replan: f32,
    /// On the NCR arena: the street router's way to the goal (next waypoint last), the goal it
    /// was routed for, and seconds until it is routed afresh.
    pub route: Vec<Vec2>,
    pub route_goal: Option<Vec3>,
    pub route_timer: f32,
    /// The planned path's length, metres: how long a goal may fairly take.
    pub path_len: f32,
    /// Whether we asked to walk last tick (standing still on purpose isn't being stuck), and how
    /// many checks running we have failed to move.
    pub walking: bool,
    pub stuck_for: u32,
    /// Where in the coming circle we are heading, and that circle's centre.
    pub refuge: Option<(Vec2, Vec3)>,
    /// The ring is at our back: fight on the move, or just run.
    pub pressed: bool,
    /// We have set off for the coming circle.
    pub heading_in: bool,
    /// Seconds this bot cuts the ring finer than it should, or leaves earlier if negative: some
    /// stay for one more chest, and the ring catches them.
    pub nerve: f32,
    /// On a roof the grid doesn't cover: the edge we are making for.
    pub way_down: Option<Vec2>,
    /// Inside a house the floor shows as walled in: the door we are leaving by, and for how much
    /// longer we try it.
    pub way_out: Option<(Vec2, f32)>,
    /// Match time we first stood on the ground.
    pub landed: Option<f32>,
    /// Seconds left running from a fight that was going badly.
    pub flee: f32,
    /// We have arrived at the goal and taken our rest for it: no second helping.
    pub arrived: bool,
    /// What we have seen, heard and remember (`senses`).
    pub senses: Senses,
}

impl BotBrain {
    pub fn new(rng: &mut impl rand::Rng, skill: BotSkill) -> Self {
        let chill = skill == BotSkill::Chill;
        Self {
            drop_at: rng.random_range(0.15..0.85),
            goal: None,
            target: None,
            think: rng.random_range(0.0..0.3),
            stuck_check: (Vec3::ZERO, 0.0),
            unstick: 0.0,
            strafe: 1.0,
            strafe_timer: 0.0,
            aim_error: if chill { rng.random_range(0.06..0.11) } else { rng.random_range(0.025..0.06) },
            react: if chill { rng.random_range(1.0..1.8) } else { rng.random_range(0.35..0.8) },
            aim: if chill { AimSkill::EASY } else { AimSkill::MEDIUM },
            memory: Seen::default(),
            view: (0.0, 0.0),
            aim_off: Vec2::ZERO,
            aim_hold: 0.0,
            sight: if chill { (35.0, 7.0) } else { (55.0, 9.0) },
            tempo: (if chill { 0.5 } else { 1.0 }, rng.random_range(0.0..1.0)),
            calm: chill,
            rest: 0.0,
            urgent: false,
            seen_for: 0.0,
            provoked: None,
            give_up: vec![],
            goal_time: 0.0,
            path: vec![],
            path_for: None,
            replan: 0.0,
            route: vec![],
            route_goal: None,
            route_timer: 0.0,
            path_len: 0.0,
            walking: false,
            stuck_for: 0,
            refuge: None,
            pressed: false,
            heading_in: false,
            nerve: rng.random_range(NERVE.0..NERVE.1),
            way_down: None,
            way_out: None,
            landed: None,
            flee: 0.0,
            arrived: false,
            senses: Senses::staggered(rng),
        }
    }
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (senses::shot_noise, super::birds::startle, senses::sense, drive_bots, senses::look_about, mind_the_drop).chain().in_set(SimSet::Brains));
    }
}

/// Marks where a bot is making for at once, not next tick: two bots thinking in step each gave way
/// to the other, and swapped chests for as long as they stayed in step.
fn claim(claimed: &mut Vec<(Entity, Vec3)>, me: Entity, goal: Option<Vec3>) {
    claimed.retain(|c| c.0 != me);
    claimed.extend(goal.map(|g| (me, g)));
}

/// The longest fall that costs no more than `damage`.
fn drop_for(damage: f32) -> f32 {
    let speed = SAFE_LANDING + damage.max(0.0) / FALL_DAMAGE;
    speed * speed / (2.0 * GRAVITY)
}

/// Bots on foot don't walk off a drop that would hurt badly (cliff tops, the stepwell's rim): with
/// no ground within the drop they'd take below the next step (on foot or mid-jump), they stop. A
/// healthy bot will take a storey's roof (up to 13 m, about 35 damage) rather than be stranded up
/// there, and with the Aandhi coming anyone takes what they would live through.
#[allow(clippy::type_complexity)]
fn mind_the_drop(
    mut bots: Query<(Entity, &Transform, &mut PlayerIntent, &Kinematic, &Health, &BotBrain), Without<Eliminated>>,
    spatial: SpatialQuery,
    layout: Res<MapLayout>,
) {
    let rules = Rules::of(layout.style);
    for (me, tf, mut intent, kin, health, brain) in &mut bots {
        let wish = super::movement::wish_dir(&intent);
        if kin.mode != MoveMode::Ground || wish == Vec3::ZERO {
            continue;
        }
        let ahead = tf.translation + wish * 1.2;
        let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([me]);
        let drop = drop_for(health.hp - if brain.urgent { 15.0 } else { 60.0 });
        // A long way down is fine where the glider comes back.
        let drop = if rules.roof_rescue { drop.min(OPEN_GLIDE_HEIGHT + 1.0) } else { drop };
        let safe = if rules.roof_rescue {
            let below = spatial.cast_ray(ahead, Dir3::NEG_Y, HALF_HEIGHT + OPEN_GLIDE_HEIGHT + 1.0, true, &filter);
            below.is_none_or(|hit| hit.distance <= HALF_HEIGHT + drop)
        } else {
            spatial.cast_ray(ahead, Dir3::NEG_Y, HALF_HEIGHT + drop, true, &filter).is_some()
        };
        if !safe {
            intent.move_axis = Vec2::ZERO;
        }
    }
}

fn yaw_pitch_to(from: Vec3, to: Vec3) -> (f32, f32) {
    let d = to - from;
    let yaw = (-d.x).atan2(-d.z);
    let pitch = d.y.atan2(d.xz().length());
    (yaw, pitch)
}

/// The nearest point to `at` a warrior can come down on: on the grid's streets, with open sky above
/// and the ground, not a roof, underneath.
fn open_ground(at: Vec2, reach: f32, walkable: &dyn Fn(Vec2) -> bool, layout: &MapLayout, spatial: &SpatialQuery, filter: &SpatialQueryFilter) -> Option<Vec3> {
    let bearings = 12;
    let mut r: f32 = 0.0;
    while r <= reach {
        for k in 0..if r == 0.0 { 1 } else { bearings } {
            let a = k as f32 / bearings as f32 * std::f32::consts::TAU;
            let p = at + Vec2::from_angle(a) * r;
            if p.abs().max_element() > layout.half_extent - 8.0 || !walkable(p) {
                continue;
            }
            let ground = layout.terrain.height(p.x, p.y);
            let start = Vec3::new(p.x, ground + 0.8, p.y);
            if spatial.cast_ray(start, Dir3::Y, 60.0, true, filter).is_none() {
                return Some(start.with_y(ground));
            }
        }
        r += 4.0;
    }
    None
}

/// On a roof the grid knows nothing of: the nearest place the surface falls away, as a point just
/// past the edge. Short drops beat long ones, and edges towards the goal win ties.
fn roof_edge(pos: Vec3, goal: Vec3, spatial: &SpatialQuery, filter: &SpatialQueryFilter) -> Option<Vec2> {
    let feet = pos.y - HALF_HEIGHT;
    let bearings = 16;
    let mut best: Option<(f32, Vec2)> = None;
    for k in 0..bearings {
        let dir = Vec2::from_angle(k as f32 / bearings as f32 * std::f32::consts::TAU);
        for d in [1.5, 3.0, 5.0, 7.5, 10.0, 14.0, 19.0, 25.0, 32.0, 40.0] {
            let p = pos.xz() + dir * d;
            // From above the parapets, so they read as ground and the edge beyond them as the edge.
            let from = Vec3::new(p.x, feet + 3.0, p.y);
            let height = spatial.cast_ray(from, Dir3::NEG_Y, 40.0, true, filter).map_or(feet - 40.0, |h| from.y - h.distance);
            if height < feet - 1.6 {
                let score = d + (feet - height) * 0.7 + (p.distance(goal.xz()) - pos.xz().distance(goal.xz())) * 0.1;
                if best.is_none_or(|b| score < b.0) {
                    best = Some((score, p + dir * 1.5));
                }
                break;
            }
        }
    }
    best.map(|b| b.1)
}

/// What a tick may still spend on finding the way.
struct Budget {
    routes: u32,
    stretches: u32,
}

/// The way across the NCR: along the floor's routes and over the bridges, the next stretch of it
/// walked round whatever the floor shows in the way. Off a roof first: the floor doesn't know
/// roofs. None when the goal is close and can't be reached: it has been dropped.
#[allow(clippy::too_many_arguments)]
fn street_heading(
    brain: &mut BotBrain,
    pos: Vec3,
    goal: Vec3,
    dt: f32,
    layout: &MapLayout,
    floor: &StreetFloor,
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    budget: &mut Budget,
) -> Option<Vec3> {
    let ground = layout.terrain.height(pos.x, pos.z);
    brain.replan -= dt;
    if let Some((door, left)) = brain.way_out {
        brain.way_out = (left > dt && !floor.reachable(pos.xz())).then_some((door, left - dt));
        if brain.way_out.is_some() {
            // At the door and on through it: its middle is a step short of the street.
            let past = door + (door - pos.xz()).normalize_or_zero() * 3.0;
            return Some(Vec3::new(past.x, pos.y, past.y));
        }
    }
    let deck = on_bridge(pos);
    if pos.y > ground + HALF_HEIGHT + 1.5 && !deck {
        if brain.way_down.is_none_or(|p| p.distance(pos.xz()) < 0.8) || brain.replan <= 0.0 {
            brain.way_down = roof_edge(pos, goal, spatial, filter);
            brain.replan = 2.0;
        }
        brain.route_goal = None;
        brain.path.clear();
        return Some(brain.way_down.map_or(goal, |p| Vec3::new(p.x, pos.y, p.y)));
    }
    brain.way_down = None;
    brain.route_timer -= dt;
    if (brain.route_timer <= 0.0 || brain.route_goal.is_none_or(|g| g.distance(goal) > 6.0)) && budget.routes > 0 {
        budget.routes -= 1;
        brain.route = floor.route(pos, goal);
        brain.route_goal = Some(goal);
        brain.route_timer = ROUTE_EVERY;
        brain.path_len = brain.route.len() as f32 * ROUTE_STEP;
        brain.path.clear();
        brain.replan = 0.0;
    }
    // Where we have got to on the route: the point of it nearest us, of those just ahead.
    let n = brain.route.len();
    let nearest = (1..=ROUTE_LOOK.min(n)).min_by(|a, b| brain.route[n - a].distance_squared(pos.xz()).total_cmp(&brain.route[n - b].distance_squared(pos.xz())));
    if let Some(k) = nearest {
        brain.route.truncate(n - k + 1);
    }
    while brain.route.last().is_some_and(|p| p.distance(pos.xz()) < 2.0) {
        brain.route.pop();
    }
    let next = brain.route.last().map_or(goal.xz(), |p| *p);
    // On a deck there is only its lane to walk.
    if deck {
        brain.path.clear();
        return Some(Vec3::new(next.x, pos.y, next.y));
    }
    while brain.path.first().is_some_and(|p| p.distance(pos.xz()) < 1.0) {
        brain.path.remove(0);
        if brain.path.is_empty() {
            brain.replan = 0.0;
        }
    }
    // Not every tick: a search that found nothing waits for the timer like any other.
    if brain.replan <= 0.0 && budget.stretches > 0 {
        budget.stretches -= 1;
        brain.replan = STRETCH_EVERY;
        let n = brain.route.len();
        let aim = |k: usize| if n > k { brain.route[n - 1 - k] } else { goal.xz() };
        // On open land, which is most of the map, the straight line is the way and needs no search.
        let far = aim(STRETCH[0]);
        let found = if floor.line_clear(pos.xz(), far) { Some(vec![far]) } else { STRETCH.iter().find_map(|&k| floor.path(pos.xz(), aim(k))) };
        // The goal itself is in reach and walled off: leave it.
        if found.is_none() && !brain.urgent && n <= STRETCH[1] && !floor.walkable(goal.xz()) {
            brain.give_up.push(goal);
            brain.goal = None;
            return None;
        }
        brain.path = found.unwrap_or_default();
    }
    let p = brain.path.first().copied().unwrap_or(next);
    Some(Vec3::new(p.x, pos.y, p.y))
}

/// Where to walk this tick to get to `goal`: the next corner of the way there, down from a roof
/// first. None when there is no reaching it and it isn't worth forcing: the goal has been dropped.
#[allow(clippy::too_many_arguments)]
fn heading(
    brain: &mut BotBrain,
    pos: Vec3,
    goal: Vec3,
    dt: f32,
    nav: &NavGrid,
    layout: &MapLayout,
    floor: &StreetFloor,
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    budget: &mut Budget,
) -> Option<Vec3> {
    if floor.ready() {
        return street_heading(brain, pos, goal, dt, layout, floor, spatial, filter, budget);
    }
    let d = pos.xz().distance(goal.xz());
    if !nav.ready() || (d <= 3.0 && (goal.y - pos.y).abs() < 2.6) {
        return Some(goal);
    }
    brain.replan -= dt;
    let ground = layout.terrain.height(pos.x, pos.z);
    let mut lost = !nav.knows(pos, ground);
    if !lost && (brain.path_for != Some(goal) || brain.replan <= 0.0) {
        brain.path_for = Some(goal);
        brain.replan = 4.0;
        brain.path_len = 0.0;
        match nav.path(pos, goal) {
            Ok(path) if !brain.urgent && path.last().is_none_or(|e| e.distance(goal.xz()) > INTERACT_RANGE - 0.4) => {
                // We'd only get near it, and it is something to reach: pick something else.
                brain.path.clear();
                let _ = path;
                brain.give_up.push(goal);
                brain.goal = None;
                return None;
            }
            Ok(path) => {
                let mut at = pos.xz();
                for p in &path {
                    brain.path_len += at.distance(*p);
                    at = *p;
                }
                brain.path = path;
                brain.way_down = None;
            }
            Err(NoPath::There) if !brain.urgent => {
                // No way there: pick something else.
                brain.give_up.push(goal);
                brain.goal = None;
                brain.path.clear();
                return None;
            }
            // The zone won't wait: straight at it.
            Err(NoPath::There) => brain.path.clear(),
            // Lost to the grid.
            Err(NoPath::Here) => {
                brain.path.clear();
                lost = pos.y > ground + HALF_HEIGHT + 1.5;
            }
        }
    }
    if lost {
        brain.path.clear();
        if brain.way_down.is_none_or(|p| p.distance(pos.xz()) < 0.8) || brain.replan <= 0.0 {
            brain.way_down = roof_edge(pos, goal, spatial, filter);
            brain.replan = 2.0;
        }
        return Some(brain.way_down.map_or(goal, |p| Vec3::new(p.x, pos.y, p.y)));
    }
    while brain.path.len() > 1 && brain.path[0].distance(pos.xz()) < 1.2 {
        brain.path.remove(0);
    }
    Some(brain.path.first().map_or(goal, |w| Vec3::new(w.x, goal.y, w.y)))
}

/// The move axes that walk a warrior facing along `yaw` in the direction `dir`.
fn axes_towards(yaw: f32, dir: Vec2) -> Vec2 {
    let (sin, cos) = yaw.sin_cos();
    let (forward, right) = (Vec2::new(-sin, -cos), Vec2::new(cos, -sin));
    Vec2::new(dir.dot(right), dir.dot(forward))
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn drive_bots(
    mut bots: Query<
        (Entity, &Transform, &mut BotBrain, &mut PlayerIntent, &Kinematic, &Inventory, &Health),
        Without<Eliminated>,
    >,
    others: Query<(Entity, &Transform, &Kinematic), (With<Warrior>, Without<Eliminated>)>,
    stances: Query<&Stance>,
    swings: Query<&Action, With<Warrior>>,
    chests: Query<(&Transform, &Chest)>,
    pickups: Query<(&Transform, &Pickup)>,
    m: Res<Match>,
    zone: Res<Zone>,
    db: Res<ItemDb>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut hits: MessageReader<HitMsg>,
    nav: Res<NavGrid>,
    layout: Res<MapLayout>,
    floor: Res<StreetFloor>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    let rules = Rules::of(layout.style);
    let mut budget = Budget { routes: ROUTES_PER_TICK, stretches: STRETCHES_PER_TICK };
    // What the bots know of the ground underfoot, map by map.
    let mapped = nav.ready() || floor.ready();
    let walkable = |p: Vec2| if floor.ready() { floor.reachable(p) } else { nav.walkable(p) };
    let snap = |p: Vec2| if floor.ready() { floor.snap(p, 24.0) } else if nav.ready() { nav.snap(p) } else { None };
    // On the NCR bots walk the streets: loot upstairs or indoors is for the players who go in.
    let at_street = |p: Vec3| p.y - layout.terrain.height(p.x, p.z) < 2.5 && floor.walkable(p.xz());
    let hits: Vec<HitMsg> = hits.read().cloned().collect();
    let los_filter = SpatialQueryFilter::from_mask(solid_mask());
    // Where each bot is making for, in the air or on the ground: its patch.
    let mut claimed: Vec<(Entity, Vec3)> = bots.iter().filter_map(|(e, _, b, ..)| b.goal.map(|g| (e, g))).collect();
    for (me, tf, mut brain, mut intent, kin, inv, health) in &mut bots {
        let pos = tf.translation;
        let eye = pos + Vec3::Y * EYE_OFFSET;
        intent.held = 0;
        intent.move_axis = Vec2::ZERO;
        if let Some(h) = hits.iter().find(|h| h.target == me && h.attacker != me) {
            brain.provoked = Some((h.attacker, 6.0));
        }
        if let Some((_, t)) = brain.provoked.as_mut() {
            *t -= dt;
        }
        if brain.provoked.is_some_and(|p| p.1 <= 0.0) {
            brain.provoked = None;
        }
        let is_bow = |id: &str| matches!(db.get(id).kind, ItemKind::Bow { .. });
        let holds_bow = inv.slots.iter().flatten().any(|s| is_bow(&s.id));
        let has_ranged = inv.slots.iter().flatten().any(|s| is_bow(&s.id) && (s.count > 0 || inv.arrows >= db.get(&s.id).kind.round_cost()));
        if kin.mode == MoveMode::Ground && brain.landed.is_none() {
            brain.landed = Some(m.elapsed);
        }
        // Fresh off the vimana, loot first: the grudge still answers whoever shoots.
        let grace = if brain.calm { LOOT_GRACE.0 } else { LOOT_GRACE.1 };
        let looting = brain.landed.is_none_or(|t| m.elapsed - t < grace);
        // While looting, only a grudge picks a fight: two bots passing in a colonnade are
        // otherwise a cricket-bat duel, and that emptied half the lobby before the first ring.
        let engage_range = if looting { 0.0 } else if has_ranged { brain.sight.0 } else { brain.sight.1 };

        match kin.mode {
            MoveMode::Vimana => {
                if m.vimana_t >= brain.drop_at && pos.x.abs() < layout.half_extent - 20.0 && pos.z.abs() < layout.half_extent - 20.0 {
                    // The NCR's drop is from 500 m: a glide is over a minute in the sky, so ride
                    // on until there is loot within a dive. The ride's end puts everyone out. It
                    // is a long list to look through, so not every tick.
                    if floor.ready() {
                        brain.think -= dt;
                        if brain.think > 0.0 {
                            continue;
                        }
                        brain.think = 0.25;
                    }
                    let dive = 0.8 * dive_reach(pos.y - layout.terrain.height(pos.x, pos.z));
                    let dive_goals = |claimed: &[(Entity, Vec3)]| -> Vec<Vec3> {
                        chests
                            .iter()
                            .map(|(t, _)| t.translation)
                            .chain(pickups.iter().filter(|(_, p)| matches!(db.get(&p.stack.id).kind, ItemKind::Bow { .. })).map(|(t, _)| t.translation))
                            .filter(|p| p.xz().distance(pos.xz()) < dive && at_street(*p) && claimed.iter().all(|(e, c)| *e == me || c.xz().distance(p.xz()) > CLAIM))
                            .collect()
                    };
                    let dive_near = if floor.ready() { dive_goals(&claimed) } else { vec![] };
                    if floor.ready() && dive_near.is_empty() && m.vimana_t < LAST_LEAP {
                        continue;
                    }
                    intent.pressed |= btn::JUMP;
                    if mapped {
                        // Make for a random chest within a glide, setting down on the open ground
                        // nearest it: straight at one under a roof would land us on that roof.
                        let near: Vec<Vec3> = if !dive_near.is_empty() {
                            dive_near
                        } else {
                            chests.iter().map(|(t, _)| t.translation).filter(|p| p.xz().distance(pos.xz()) < rules.glide_reach).collect()
                        };
                        // Our own patch: the open ground by a chest that no one else is making
                        // for. Every chest in a block comes down on the same few street cells,
                        // so it is the landing that must be clear of the others, not the chest.
                        let mut landing = None;
                        for _ in 0..12 {
                            if near.is_empty() {
                                break;
                            }
                            // Not straight at the chest: every chest in a block comes down on
                            // the same street cell otherwise, and two bots on one cell is a brawl.
                            let aim = near[rng.random_range(0..near.len())].xz()
                                + Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(0.0..10.0);
                            let Some(spot) = open_ground(aim, 40.0, &walkable, &layout, &spatial, &los_filter) else { continue };
                            let clear = claimed.iter().all(|(e, c)| *e == me || c.xz().distance(spot.xz()) > CLAIM);
                            landing = Some(spot);
                            if clear {
                                break;
                            }
                        }
                        brain.goal = landing.or_else(|| open_ground(pos.xz(), 40.0, &walkable, &layout, &spatial, &los_filter));
                        claim(&mut claimed, me, brain.goal);
                        continue;
                    }
                    // Land near a random chest under open sky: one under a roof (a temple's
                    // sanctum) would set us down on the spire, stranded.
                    let open: Vec<Vec3> = chests
                        .iter()
                        .map(|(t, _)| t.translation)
                        .filter(|p| spatial.cast_ray(*p + Vec3::Y * 0.6, Dir3::Y, 40.0, true, &los_filter).is_none())
                        .collect();
                    brain.goal = (!open.is_empty()).then(|| open[rng.random_range(0..open.len())]);
                }
                continue;
            }
            MoveMode::Freefall | MoveMode::Gliding => {
                brain.think -= dt;
                if brain.think <= 0.0 && mapped {
                    brain.think = 0.25;
                    // A roof coming up with the goal beyond reach: slip off to open ground.
                    if let Some(hit) = spatial.cast_ray(pos, Dir3::NEG_Y, 60.0, true, &los_filter) {
                        let below = layout.terrain.height(pos.x, pos.z);
                        let reach = (pos.y - below) / GLIDE_FALL * GLIDE_HORIZ;
                        let far = brain.goal.is_none_or(|g| g.xz().distance(pos.xz()) > reach);
                        if pos.y - hit.distance > below + 2.5 && far {
                            brain.goal = open_ground(pos.xz(), reach.min(60.0), &walkable, &layout, &spatial, &los_filter).or(brain.goal);
                        }
                    }
                }
                if let Some(g) = brain.goal {
                    let (yaw, _) = yaw_pitch_to(pos, g);
                    intent.yaw = yaw;
                    let d = pos.xz().distance(g.xz());
                    intent.move_axis = if d > 3.0 { Vec2::Y } else { Vec2::ZERO };
                }
                continue;
            }
            MoveMode::Ground | MoveMode::Riding => {}
        }
        // Off a roof's edge with nothing near below: the glider, where the map has that.
        if rules.roof_rescue && !kin.grounded && kin.velocity.y < -8.0 {
            intent.pressed |= btn::JUMP;
        }

        // ---- perception (every think tick)
        brain.think -= dt;
        if brain.think <= 0.0 {
            brain.think = 0.25;
            let mut best: Option<(Entity, f32)> = None;
            for (oe, ot, _) in &others {
                if oe == me {
                    continue;
                }
                let d = ot.translation.distance(pos);
                let grudge = brain.provoked.is_some_and(|p| p.0 == oe);
                if (d > engage_range && !(grudge && d < ANSWER_RANGE)) || best.is_some_and(|b| b.1 < d) {
                    continue;
                }
                if brain.senses.sees(oe) {
                    best = Some((oe, d));
                }
            }
            let new_target = if brain.flee > 0.0 { None } else { best.map(|b| b.0) };
            if new_target != brain.target {
                brain.seen_for = 0.0;
            }
            brain.target = new_target;
            senses::consider_search(&mut brain, has_ranged, looting, pos, &snap);

            // Goals when not fighting: zone first, then chests, then pickups, then wander.
            // On the NCR a bot sprints and the ring takes minutes to close, so it stays out until
            // the last moment it can run in ahead of the ring. A small map's ring is on its heels
            // from the start: there it leaves in time to walk in before the ring moves. Streets
            // being longer than the straight line, either way.
            let (left, shrinking) = zone.countdown();
            let open_land = floor.ready();
            // A long gun in hand sprints slower, so the bot has to set off sooner.
            let sprint = if inv.current().is_some_and(|s| long_gun(&s.id)) { RIFLE_SPRINT } else { SPRINT_SPEED };
            let (pace, ahead, detour, reach) = if open_land { (sprint, zone.closes_in(), OPEN_DETOUR, zone.target_radius - OPEN_MARGIN) } else { (WALK_SPEED, left, STREET_DETOUR, zone.target_radius * 0.8) };
            let run_in = |from: f32| (from - reach).max(0.0) * detour / pace;
            let centre = zone.target_center.distance(pos.xz());
            let spare = ahead - run_in(centre);
            // With slack once we have set off: deciding afresh each tick flipped bots between the
            // refuge and the chest behind them, and they looted nothing while they dithered.
            let nerve = if open_land { brain.nerve } else { 0.0 };
            let must_move = (shrinking && !open_land) || spare < if brain.heading_in { LEAVE_AT.1 } else { LEAVE_AT.0 } - nerve;
            brain.heading_in = must_move && centre > zone.target_radius * if brain.heading_in { CIRCLE_LINE.1 } else { CIRCLE_LINE.0 };
            let outside = zone.outside(pos);
            let zone_goal = (outside || brain.heading_in).then(|| {
                if brain.refuge.is_none_or(|(c, _)| c != zone.target_center) {
                    // A spot of our own well inside, on our side of the circle.
                    let toward = (pos.xz() - zone.target_center).normalize_or(Vec2::X);
                    let mut draw = || zone.target_center + Vec2::from_angle(rng.random_range(-0.7..0.7)).rotate(toward) * zone.target_radius * rng.random_range(0.2..0.6);
                    // Somewhere to stand, not a river or a building, where the map has those.
                    let spot = (0..6).map(|_| draw()).find(|p| !open_land || floor.reachable(*p)).unwrap_or(zone.target_center);
                    let spot = snap(spot).unwrap_or(spot);
                    brain.refuge = Some((zone.target_center, spot.extend(0.0).xzy()));
                }
                brain.refuge.map_or(Vec3::ZERO, |r| r.1)
            });
            brain.pressed = outside || (zone_goal.is_some() && spare < if brain.pressed { PRESSED_AT.1 } else { PRESSED_AT.0 });
            // On the way in, a bot with no bow turns aside for what it sees, unless the ring is
            // at its back: on the NCR a long walk to the circle is most of the early game.
            let scavenging = open_land && !has_ranged && !brain.pressed;
            let grabs = |p: Vec3| open_land && !has_ranged && p.distance(pos) < GRAB_GUN;
            // What there is time for. On the NCR: anything inside this ring that leaves time to go
            // on into the coming circle afterwards. On a small map: anything inside the ring until
            // it is nearly on its way, then only what lies inside the coming one, short of the
            // line that sets a bot heading in, or the two would pull it back and forth.
            let in_time = |p: Vec3| {
                let there = zone.target_center.distance(p.xz());
                let fetch = p.distance(pos) * detour / pace + run_in(there);
                if open_land {
                    grabs(p) || (!zone.outside(p) && ahead - fetch > if scavenging { 0.0 } else { FETCH_SPARE - nerve })
                } else {
                    !zone.outside(p) && ((!must_move && left > 20.0 && !shrinking) || there < zone.target_radius * CIRCLE_LINE.0)
                }
            };
            let skip = brain.give_up.clone();
            let search_goal = brain.senses.search.map(|s| s.spot).filter(|g| !skip.iter().any(|b| b.distance(*g) < 0.5));
            // While looting, a chest someone else stands by or is making for is theirs: the
            // early game is spent apart, not in a cricket-bat brawl over the first peti.
            let theirs = |p: Vec3| {
                looting
                    && (others.iter().any(|(oe, ot, _)| oe != me && ot.translation.distance(p) < 18.0)
                        || claimed.iter().any(|(e, c)| *e != me && c.distance(p) < 12.0))
            };
            // Upstairs urns wait until the looting is done: the stairs are a long way round
            // for a bot with a minute to get armed.
            let chest_goal = chests
                .iter()
                .filter(|(t, c)| !c.opened && t.translation.distance(pos) < rules.chest_sight && in_time(t.translation))
                .filter(|(t, _)| if floor.ready() { at_street(t.translation) } else { !looting || (t.translation.y - pos.y).abs() < 2.5 })
                .filter(|(t, _)| !skip.iter().any(|g| g.distance(t.translation) < 0.5) && !theirs(t.translation))
                .min_by(|a, b| a.0.translation.distance(pos).total_cmp(&b.0.translation.distance(pos)))
                .map(|(t, _)| t.translation);
            let free_slot = inv.slots.iter().any(|s| s.is_none());
            // In a yard with no way in: not for us, and no looking at it again.
            let mut if_reachable = |goal: Option<Vec3>| {
                goal.filter(|p| {
                    let ok = !floor.ready() || floor.reachable(p.xz());
                    if !ok {
                        brain.give_up.push(*p);
                    }
                    ok
                })
            };
            let chest_goal = if_reachable(chest_goal);
            let pickup_goal = pickups
                .iter()
                .filter(|(t, p)| {
                    let kind = &db.get(&p.stack.id).kind;
                    // Goli only while we are short of it: a bot that wants every box hoovers
                    // the verandahs and never opens a chest.
                    let stackable = match kind {
                        ItemKind::Arrows { .. } => inv.arrows < 120,
                        ItemKind::Consumable { .. } | ItemKind::Astra { .. } => true,
                        _ => false,
                    };
                    // Weapons: only new kinds, or a strictly better rarity of one we hold.
                    let wanted = stackable
                        || inv.slots.iter().flatten().all(|s| s.id != p.stack.id || s.rarity < p.stack.rarity);
                    // A first bow is worth a slot, whatever has to go for it (`loot::interact` swaps).
                    let needed = !holds_bow && matches!(kind, ItemKind::Bow { .. });
                    // With no gun yet a street weapon is only worth stooping for.
                    let sight = if !holds_bow && matches!(kind, ItemKind::Melee { .. }) { STOOP } else if floor.ready() && needed { GUN_SIGHT } else { 45.0 };
                    let far = t.translation.distance(pos);
                    // As with a chest, a gun a long way off is one bot's: eight bots ran for one
                    // 200 m away. One close by goes to whoever gets there first.
                    let spoken_for = floor.ready() && needed && far > 45.0 && theirs(t.translation);
                    far < sight && !spoken_for && in_time(t.translation) && wanted && (free_slot || needed || p.stack.id == "arrows") && (!floor.ready() || at_street(t.translation))
                })
                .filter(|(t, _)| !skip.iter().any(|g| g.distance(t.translation) < 0.5))
                .min_by(|a, b| a.0.translation.distance(pos).total_cmp(&b.0.translation.distance(pos)))
                .map(|(t, _)| t.translation);
            let pickup_goal = if_reachable(pickup_goal);
            let search_goal = if_reachable(search_goal);
            let previous = brain.goal;
            // Running from a fight: keep running, unless the ring says otherwise.
            let flee_goal = (brain.flee > 0.0 && zone_goal.is_none()).then_some(brain.goal).flatten();
            // A find at our feet is taken on the way; otherwise the chest comes before the
            // find across the street: chests are what move a warrior through the blocks.
            // A bot with no bow on the NCR goes for whichever is nearer, the find or the chest:
            // with a fixed limit, a street that led away from a find flipped it to the chest and
            // back as it crossed the limit.
            let find_sight = if has_ranged {
                14.0
            } else if floor.ready() {
                rules.find_sight.max(chest_goal.map_or(GUN_SIGHT, |c| c.distance(pos)))
            } else {
                rules.find_sight
            };
            let near_find = pickup_goal.filter(|p| p.distance(pos) < find_sight);
            let aside = near_find.filter(|p| scavenging || grabs(*p)).or(chest_goal.filter(|p| grabs(*p)));
            brain.urgent = zone_goal.is_some() && aside.is_none();
            brain.goal = aside.or(zone_goal).or(flee_goal).or(search_goal).or(near_find).or(chest_goal).or(pickup_goal).or_else(|| {
                if brain.goal.is_none_or(|g| g.distance(pos) < 4.0) {
                    let c = zone.target_center;
                    let r = zone.target_radius.max(8.0) * 0.7;
                    let spot = Vec2::new(c.x + rng.random_range(-r..r), c.y + rng.random_range(-r..r));
                    let spot = if floor.ready() { floor.snap(spot, 24.0).unwrap_or(spot) } else { spot };
                    Some(Vec3::new(spot.x, 0.0, spot.y))
                } else {
                    brain.goal
                }
            });
            if brain.goal.map(|g| g.round()) != previous.map(|g| g.round()) {
                brain.goal_time = 0.0;
            }
            claim(&mut claimed, me, brain.goal);
        }

        // ---- stuck detection: if we barely moved while trying, jump and sidestep; if that keeps
        // failing, back right off and think again.
        brain.stuck_check.1 += dt;
        if brain.stuck_check.1 > 1.0 {
            if brain.stuck_check.0.distance(pos) < 0.6 && brain.walking {
                brain.stuck_for += 1;
                brain.unstick = if brain.stuck_for % 3 == 0 { 1.8 } else { 0.9 };
                brain.replan = 0.0;
                // Something the grids can't see: on to the next route point, and a fresh stretch.
                brain.route.pop();
                brain.path.clear();
                brain.strafe = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
                // A fight can carry a bot in through a broken door, and the floor knows no way out.
                if brain.stuck_for >= 2 && brain.way_out.is_none() && floor.ready() && !floor.reachable(pos.xz()) {
                    let far = |d: &&crate::map::Block| d.pos.xz().distance_squared(pos.xz());
                    let door = layout.doors.iter().min_by(|a, b| far(a).total_cmp(&far(b)));
                    brain.way_out = door.map(|d| d.pos.xz()).filter(|d| d.distance(pos.xz()) < DOOR_NEAR).map(|d| (d, DOOR_TRIES));
                }
                if brain.stuck_for >= 8 && !brain.urgent {
                    if let Some(g) = brain.goal.take() {
                        brain.give_up.push(g);
                    }
                    brain.stuck_for = 0;
                }
            } else if brain.stuck_check.0.distance(pos) > 2.0 {
                brain.stuck_for = 0;
            }
            brain.stuck_check = (pos, 0.0);
        }
        brain.walking = false;

        // ---- combat
        brain.flee -= dt;
        let target = brain.target.and_then(|t| others.get(t).ok());
        // Deep in the Aandhi with the fight out of reach, running beats shooting.
        let burning = zone.outside(pos) && pos.xz().distance(zone.center) > zone.radius + 8.0;
        let target = target.filter(|(_, t, _)| !(burning && t.translation.distance(pos) > 14.0));
        // Losing, or shot from too far to answer: break off and run for the far side of a block,
        // then drink and think again.
        // Not out of the ring, and not at all once the ring is too small to run anywhere.
        if zone.outside(pos) {
            brain.flee = 0.0;
        }
        let losing = target.filter(|_| health.hp < FLEE_HP).map(|(_, t, _)| t.translation);
        // The blow gave its maker's place away (`senses`): that is all a far rifle leaves to go on.
        let sniper = brain.provoked.and_then(|(who, _)| brain.senses.known.iter().find(|k| k.who == who)).map(|k| k.pos).filter(|at| at.distance(pos) >= ANSWER_RANGE);
        if let Some(from) = losing.or(sniper).filter(|_| !brain.pressed && brain.flee <= 0.0 && zone.radius > 60.0) {
            let away = (pos - from).xz().normalize_or(Vec2::X);
            let spot = pos.xz() + Vec2::from_angle(rng.random_range(-0.6..0.6)).rotate(away) * 45.0;
            let spot = snap(spot).unwrap_or(spot);
            // Out of its sight if there is such a place near; a long way off if not.
            let threat = senses::eye(from, 0.0);
            let hidden = mapped.then(|| senses::hiding_spot(&spatial, &walkable, &layout, threat, pos.xz(), 24.0, 0.0)).flatten();
            let spot = hidden.unwrap_or(spot);
            // A short dash from a far rifle may leave the coming ring; a longer fall-back may not.
            let coming = losing.is_none() || zone.target_center.distance(spot) < zone.target_radius * 0.9;
            if coming && zone.center.distance(spot) < zone.radius * 0.9 {
                brain.goal = Some(Vec3::new(spot.x, 0.0, spot.y));
                brain.goal_time = 0.0;
                brain.target = None;
                brain.rest = 0.0;
                brain.flee = rng.random_range(6.0..10.0);
                brain.senses.tally.flights += 1;
                brain.senses.tally.hid += hidden.is_some() as u32;
            }
        }
        let target = target.filter(|_| brain.flee <= 0.0);
        if let Some((foe, ttf, tkin)) = target {
            // A new target: from wherever we were looking, and nothing known of it yet.
            if brain.seen_for == 0.0 {
                brain.memory.clear();
                brain.view = (intent.yaw, intent.pitch);
            }
            brain.seen_for += dt;
            let dist = ttf.translation.distance(pos);
            let skill = brain.aim;
            brain.memory.push(m.elapsed, ttf.translation, tkin.velocity, &skill);
            let (seen_at, seen_moving) = brain.memory.perceive(&skill).unwrap_or((ttf.translation, tkin.velocity));
            // Weapon choice by range.
            let want = choose_weapon(inv, &db, dist);
            if want != inv.selected {
                intent.select = Some(want as u8);
            }
            let held = inv.current().map(|s| db.get(&s.id).kind.clone());
            let speed = match held {
                Some(ItemKind::Bow { speed, .. }) => speed,
                _ => 0.0,
            };
            // Lead the target and compensate drop.
            let mut aim_at = seen_at + Vec3::Y * (0.35 - stances.get(foe).map_or(0.0, Stance::drop));
            if speed > 0.0 {
                let t = dist / speed;
                aim_at += seen_moving * t + Vec3::Y * 0.5 * 5.0 * t * t;
            }
            // Shots follow the over-the-shoulder aim ray (like a player's crosshair), not the eye
            // line, so solve the angles from that ray's origin (two fixed-point steps suffice).
            // A swing goes where the body faces. Up close (a swing's range) the ray's origin is off
            // to the side of the target and the angle from it swings wildly: aim from the eye there.
            let (mut yaw, mut pitch) = yaw_pitch_to(eye, aim_at);
            let melee = matches!(held, Some(ItemKind::Melee { .. }));
            for _ in 0..if !melee && dist > 4.0 { 2 } else { 0 } {
                let probe = PlayerIntent { yaw, pitch, ..default() };
                let (origin, _) = super::aim_ray(pos, &probe);
                (yaw, pitch) = yaw_pitch_to(origin, aim_at);
            }
            // The aim is off by a little, for a little while: a hand drifting, not a shake.
            brain.aim_hold -= dt;
            if brain.aim_hold <= 0.0 {
                brain.aim_hold = rng.random_range(AIM_HOLD.0..AIM_HOLD.1);
                let wobble = brain.aim_error * (1.0 + dist / 40.0);
                brain.aim_off = Vec2::new(rng.random_range(-wobble..wobble), rng.random_range(-wobble..wobble) * 0.6);
            }
            brain.view = aim::turn(brain.view, (yaw + brain.aim_off.x, pitch + brain.aim_off.y), &skill, dt);
            (intent.yaw, intent.pitch) = brain.view;
            let on_beat = (m.elapsed * 0.4 + brain.tempo.1).fract() < brain.tempo.0;
            if brain.seen_for > brain.react && on_beat {
                match held {
                    Some(ItemKind::Melee { reach, .. }) => {
                        if dist < reach + 0.3 {
                            intent.held |= btn::FIRE;
                        }
                    }
                    Some(ItemKind::Bow { charge, .. }) if charge > 0.0 => {
                        // Draw, then release on alternate cycles.
                        if (m.elapsed * 0.9).fract() < 0.75 {
                            intent.held |= btn::FIRE;
                        }
                    }
                    Some(ItemKind::Bow { .. }) => intent.held |= btn::FIRE,
                    Some(ItemKind::Astra { .. }) => intent.pressed |= btn::FIRE,
                    _ => {}
                }
            }
            // A blow coming: up with whatever is in hand (a chill bot often misses it).
            if matches!(held, Some(ItemKind::Melee { .. })) && dist < 3.6 && (!brain.calm || on_beat) && swings.get(foe).is_ok_and(|a| a.since_attack < GUARD_READ) {
                intent.held = (intent.held & !btn::FIRE) | btn::AIM;
            }
            // Movement: melee rushes, archers keep a band and strafe.
            brain.strafe_timer -= dt;
            if brain.strafe_timer <= 0.0 {
                brain.strafe_timer = if brain.calm { rng.random_range(1.5..3.5) } else { rng.random_range(0.6..1.6) };
                brain.strafe = -brain.strafe;
            }
            let forward = if melee {
                // Close in, then stand and swing: running on past the target is what circled it.
                if dist > 1.8 { 1.0 } else { 0.0 }
            } else if dist > 35.0 {
                0.7
            } else if dist < 10.0 {
                -0.6
            } else {
                0.0
            };
            let (side, ahead) = if brain.calm { (0.3, forward * 0.6) } else { (0.8, forward) };
            // No sidestepping in a melee once within a few steps (it orbited the target).
            let side = if melee && dist < 4.0 { 0.0 } else { side };
            intent.move_axis = Vec2::new(brain.strafe * side, ahead);
            if !brain.calm && rng.random_bool(0.01) {
                intent.pressed |= btn::JUMP;
            }
            // The ring won't wait for the fight: keep shooting, but keep walking out of it.
            if brain.pressed {
                if let Some(goal) = brain.goal {
                    if let Some(toward) = heading(&mut brain, pos, goal, dt, &nav, &layout, &floor, &spatial, &los_filter, &mut budget) {
                        intent.move_axis = axes_towards(intent.yaw, (toward - pos).xz().normalize_or_zero());
                        brain.walking = true;
                    }
                }
            }
            continue;
        }

        // ---- out of combat: heal, reload, loot, travel
        if let Some(slot) = heal_slot(inv, &db, health).filter(|_| !zone.outside(pos)) {
            if inv.selected != slot {
                intent.select = Some(slot as u8);
            } else {
                intent.pressed |= btn::FIRE;
            }
            continue;
        }
        brain.rest -= dt;
        if brain.calm && brain.rest > 0.0 && !brain.urgent && brain.unstick <= 0.0 {
            continue;
        }
        if let Some(goal) = brain.goal {
            // Can't reach it in reasonable time (blocked): abandon it, unless it is the way out.
            brain.goal_time += dt;
            let patience = 20.0 + brain.path_len * 0.3;
            if brain.goal_time > patience && !brain.urgent {
                brain.give_up.push(goal);
                brain.goal = None;
                brain.goal_time = 0.0;
            }
            let d = pos.xz().distance(goal.xz());
            let near = d <= 1.4 && (goal.y - pos.y).abs() < 2.6;
            // Around buildings and up stairs by the grid's path; down from a roof by its edge.
            let Some(toward) = heading(&mut brain, pos, goal, dt, &nav, &layout, &floor, &spatial, &los_filter, &mut budget) else { continue };
            let (yaw, _) = yaw_pitch_to(pos, toward);
            intent.yaw = yaw;
            intent.pitch = 0.0;
            intent.move_axis = if near { Vec2::ZERO } else { Vec2::Y };
            brain.walking = !near;
            if near && brain.calm && !brain.arrived {
                // Arrived: a moment at a chest, none over a find on the ground, and a while
                // standing about wherever we wandered to. Once per arrival: set again every
                // tick, the rest never ran out and bots stood a quarter of their looting time.
                brain.arrived = true;
                let at_chest = chests.iter().any(|(t, _)| t.translation.distance(goal) < 0.6);
                let at_find = pickups.iter().any(|(t, _)| t.translation.distance(goal) < 0.6);
                brain.rest = if at_find { 0.0 } else if at_chest { rng.random_range(1.5..3.5) } else { rng.random_range(3.0..9.0) };
            }
            if !near {
                brain.arrived = false;
            }
            // Sprint when it is far, when the ring presses, when running from a fight, and
            // between the chests of the loot window: a jog, not a stroll, with the ring coming.
            if d > 25.0 && (!brain.calm || brain.urgent) || brain.flee > 0.0 || (looting && d > 12.0) {
                intent.held |= btn::SPRINT;
            }
            if d < INTERACT_RANGE - 0.4 && (goal.y - pos.y).abs() < 2.5 {
                intent.pressed |= btn::INTERACT;
            }
        }
        if brain.unstick > 0.0 {
            brain.unstick -= dt;
            // The long ones back away from whatever is holding us.
            intent.move_axis = Vec2::new(brain.strafe, if brain.unstick > 0.9 { -0.8 } else { 0.4 });
            if brain.unstick % 0.9 > 0.75 {
                intent.pressed |= btn::JUMP;
            }
        }
    }
}

fn choose_weapon(inv: &Inventory, db: &ItemDb, dist: f32) -> usize {
    let mut best = (0usize, if dist < 3.0 { 10.0 } else { 0.0 });
    for (i, s) in inv.slots.iter().enumerate() {
        let Some(s) = s else { continue };
        let kind = &db.get(&s.id).kind;
        let score = match kind {
            ItemKind::Bow { pellets, charge, .. } => {
                let usable = s.count > 0 || inv.arrows >= kind.round_cost();
                if !usable {
                    0.0
                } else if *pellets > 1 {
                    if dist < 14.0 { 9.0 } else { 2.0 }
                } else if *charge > 0.0 {
                    if dist > 35.0 { 9.0 } else { 3.0 }
                } else {
                    6.0
                }
            }
            // Whatever is in hand beats the bat up close, and is what a warrior with no gun carries.
            ItemKind::Melee { reach, .. } => if dist < reach + 1.0 { 11.0 } else { 0.5 },
            // Fire astras go to a mid-range target; the wind astra is left to players.
            ItemKind::Astra { effect: AstraEffect::Fire { .. }, .. } if s.count > 0 && (8.0..24.0).contains(&dist) => 7.0,
            _ => 0.0,
        } * s.rarity.damage_scale();
        if score > best.1 {
            best = (i + 1, score);
        }
    }
    best.0
}

fn heal_slot(inv: &Inventory, db: &ItemDb, h: &Health) -> Option<usize> {
    inv.slots.iter().enumerate().find_map(|(i, s)| {
        let s = s.as_ref()?;
        match db.get(&s.id).kind {
            ItemKind::Consumable { heal, heal_cap, shield, .. } => {
                let ok = (heal > 0.0 && h.hp < heal_cap.min(h.max_hp) - 10.0) || (shield > 0.0 && h.shield < h.max_shield - 20.0);
                ok.then_some(i + 1)
            }
            _ => None,
        }
    })
}
