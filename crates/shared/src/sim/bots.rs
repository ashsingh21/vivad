//! Bot warriors. They only write `PlayerIntent`, exactly like a human controller would.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::flow::Match;
use super::loot::{Chest, INTERACT_RANGE, Pickup};
use super::zone::Zone;
use super::{BotSkill, 
    EYE_OFFSET, Eliminated, HALF_HEIGHT, Health, HitMsg, Inventory, Kinematic, MoveMode, PlayerIntent, SimSet, Warrior, btn, solid_mask,
};
use super::movement::{FALL_DAMAGE, GLIDE_FALL, GLIDE_HORIZ, GRAVITY, SAFE_LANDING, WALK_SPEED};
use super::nav::{NavGrid, NoPath};
use crate::items::{AstraEffect, ItemDb, ItemKind};
use crate::map::MapLayout;

/// How far from the vimana a leap can carry a warrior.
const GLIDE_REACH: f32 = 110.0;
/// How much longer the way through the streets is than the straight line, near enough.
const STREET_DETOUR: f32 = 1.3;
/// Bots leaping from the vimana spread out: none makes for a chest this close to another's.
const CLAIM: f32 = 45.0;
/// Seconds after landing that a bot loots before it hunts: it still fights whoever hits it.
/// A chill bot takes its time, a sharp one less.
const LOOT_GRACE: (f32, f32) = (75.0, 30.0);
/// Below this much health a bot breaks off a fight and runs, unless the ring is at its back:
/// a fight to the death every time emptied the lobby in four minutes.
const FLEE_HP: f32 = 40.0;

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
    /// On a roof the grid doesn't cover: the edge we are making for.
    pub way_down: Option<Vec2>,
    /// Match time we first stood on the ground.
    pub landed: Option<f32>,
    /// Seconds left running from a fight that was going badly.
    pub flee: f32,
    /// We have arrived at the goal and taken our rest for it: no second helping.
    pub arrived: bool,
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
            way_down: None,
            landed: None,
            flee: 0.0,
            arrived: false,
        }
    }
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (drive_bots, mind_the_drop).chain().in_set(SimSet::Brains));
    }
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
) {
    for (me, tf, mut intent, kin, health, brain) in &mut bots {
        let wish = super::movement::wish_dir(&intent);
        if kin.mode != MoveMode::Ground || wish == Vec3::ZERO {
            continue;
        }
        let ahead = tf.translation + wish * 1.2;
        let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([me]);
        let drop = drop_for(health.hp - if brain.urgent { 15.0 } else { 60.0 });
        if spatial.cast_ray(ahead, Dir3::NEG_Y, HALF_HEIGHT + drop, true, &filter).is_none() {
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
fn open_ground(at: Vec2, reach: f32, nav: &NavGrid, layout: &MapLayout, spatial: &SpatialQuery, filter: &SpatialQueryFilter) -> Option<Vec3> {
    let bearings = 12;
    let mut r: f32 = 0.0;
    while r <= reach {
        for k in 0..if r == 0.0 { 1 } else { bearings } {
            let a = k as f32 / bearings as f32 * std::f32::consts::TAU;
            let p = at + Vec2::from_angle(a) * r;
            if p.abs().max_element() > layout.half_extent - 8.0 || !nav.walkable(p) {
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
    street: &crate::map::navigation::StreetNav,
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
) -> Option<Vec3> {
    let d = pos.xz().distance(goal.xz());
    if street.active() && pos.y < 4.0 && pos.y > -2.0 && goal.y > -2.0 && goal.y < 4.0 {
        // The NCR arena: by its street router, over the bridges rather than into the Yamuna.
        brain.route_timer -= dt;
        if brain.route_timer <= 0.0 || brain.route_goal.is_none_or(|g| g.distance(goal) > 6.0) {
            brain.route = street.route(pos, goal);
            brain.route_goal = Some(goal);
            brain.route_timer = 3.0;
        }
        while brain.route.last().is_some_and(|p| p.distance(pos.xz()) < 2.0) {
            brain.route.pop();
        }
        return Some(brain.route.last().map_or(goal, |p| Vec3::new(p.x, pos.y, p.y)));
    }
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
    street: Res<crate::map::navigation::StreetNav>,
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    let hits: Vec<HitMsg> = hits.read().cloned().collect();
    let los_filter = SpatialQueryFilter::from_mask(solid_mask());
    // Where each bot is making for, in the air or on the ground: its patch.
    let claimed: Vec<(Entity, Vec3)> = bots.iter().filter_map(|(e, _, b, ..)| b.goal.map(|g| (e, g))).collect();
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
        let has_ranged = inv.slots.iter().flatten().any(|s| {
            matches!(db.get(&s.id).kind, ItemKind::Bow { .. }) && (s.count > 0 || inv.arrows > 0)
        });
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
                    intent.pressed |= btn::JUMP;
                    if nav.ready() {
                        // Make for a random chest within a glide, setting down on the open ground
                        // nearest it: straight at one under a roof would land us on that roof.
                        let near: Vec<Vec3> = chests
                            .iter()
                            .map(|(t, _)| t.translation)
                            .filter(|p| p.xz().distance(pos.xz()) < GLIDE_REACH)
                            .collect();
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
                            let Some(spot) = open_ground(aim, 40.0, &nav, &layout, &spatial, &los_filter) else { continue };
                            let clear = claimed.iter().all(|(e, c)| *e == me || c.xz().distance(spot.xz()) > CLAIM);
                            landing = Some(spot);
                            if clear {
                                break;
                            }
                        }
                        brain.goal = landing.or_else(|| open_ground(pos.xz(), 40.0, &nav, &layout, &spatial, &los_filter));
                        continue;
                    }
                    // Land near a random chest under open sky: one under a roof (a temple's
                    // sanctum) would set us down on the spire, stranded.
                    let open: Vec<Vec3> = chests
                        .iter()
                        .map(|(t, _)| t.translation)
                        .filter(|p| !street.active() || p.y < 2.0)
                        .filter(|p| spatial.cast_ray(*p + Vec3::Y * 0.6, Dir3::Y, 40.0, true, &los_filter).is_none())
                        .collect();
                    brain.goal = (!open.is_empty()).then(|| open[rng.random_range(0..open.len())]);
                }
                continue;
            }
            MoveMode::Freefall | MoveMode::Gliding => {
                brain.think -= dt;
                if brain.think <= 0.0 && nav.ready() {
                    brain.think = 0.25;
                    // A roof coming up with the goal beyond reach: slip off to open ground.
                    if let Some(hit) = spatial.cast_ray(pos, Dir3::NEG_Y, 60.0, true, &los_filter) {
                        let below = layout.terrain.height(pos.x, pos.z);
                        let reach = (pos.y - below) / GLIDE_FALL * GLIDE_HORIZ;
                        let far = brain.goal.is_none_or(|g| g.xz().distance(pos.xz()) > reach);
                        if pos.y - hit.distance > below + 2.5 && far {
                            brain.goal = open_ground(pos.xz(), reach.min(60.0), &nav, &layout, &spatial, &los_filter).or(brain.goal);
                        }
                    }
                }
                if let Some(g) = brain.goal {
                    let (yaw, _) = yaw_pitch_to(pos, g);
                    intent.yaw = yaw;
                    intent.move_axis = if pos.xz().distance(g.xz()) > 3.0 { Vec2::Y } else { Vec2::ZERO };
                }
                continue;
            }
            MoveMode::Ground => {}
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
                if (d > engage_range && !(grudge && d < 70.0)) || best.is_some_and(|b| b.1 < d) {
                    continue;
                }
                let to = ot.translation + Vec3::Y * 0.4 - eye;
                let Ok((dir, len)) = Dir3::new_and_length(to) else { continue };
                if spatial.cast_ray(eye, dir, len, true, &los_filter).is_none() {
                    best = Some((oe, d));
                }
            }
            let new_target = if brain.flee > 0.0 { None } else { best.map(|b| b.0) };
            if new_target != brain.target {
                brain.seen_for = 0.0;
            }
            brain.target = new_target;

            // Goals when not fighting: zone first, then chests, then pickups, then wander.
            // Leave in time to walk in, streets being longer than the straight line.
            let (left, shrinking) = zone.countdown();
            let centre = zone.target_center.distance(pos.xz());
            let travel_time = (centre - zone.target_radius * 0.8).max(0.0) * STREET_DETOUR / WALK_SPEED;
            // With slack once we have set off: deciding afresh each tick flipped bots between the
            // refuge and the chest behind them, and they looted nothing while they dithered.
            let must_move = shrinking || left < travel_time + if brain.urgent { 30.0 } else { 12.0 };
            let outside = zone.outside(pos);
            let zone_goal = (outside || (must_move && centre > zone.target_radius * 0.85)).then(|| {
                if brain.refuge.is_none_or(|(c, _)| c != zone.target_center) {
                    // A spot of our own well inside, on our side of the circle.
                    let side = Vec2::from_angle(rng.random_range(-0.7..0.7)).rotate((pos.xz() - zone.target_center).normalize_or(Vec2::X));
                    let spot = zone.target_center + side * zone.target_radius * rng.random_range(0.2..0.6);
                    let spot = nav.ready().then(|| nav.snap(spot)).flatten().unwrap_or(spot);
                    brain.refuge = Some((zone.target_center, spot.extend(0.0).xzy()));
                }
                brain.refuge.map_or(Vec3::ZERO, |r| r.1)
            });
            brain.pressed = outside || (zone_goal.is_some() && left < travel_time + 4.0);
            // Nothing outside the ring, or outside the coming one once it is nearly on its way.
            let in_time = |p: Vec3| {
                !zone.outside(p) && ((left > 20.0 && !shrinking) || zone.target_center.distance(p.xz()) < zone.target_radius)
            };
            let skip = brain.give_up.clone();
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
                .filter(|(t, c)| !c.opened && t.translation.distance(pos) < 120.0 && in_time(t.translation))
                .filter(|(t, _)| (!street.active() && !looting) || (t.translation.y - pos.y).abs() < 2.5)
                .filter(|(t, _)| !skip.iter().any(|g| g.distance(t.translation) < 0.5) && !theirs(t.translation))
                .min_by(|a, b| a.0.translation.distance(pos).total_cmp(&b.0.translation.distance(pos)))
                .map(|(t, _)| t.translation);
            let free_slot = inv.slots.iter().any(|s| s.is_none());
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
                    t.translation.distance(pos) < 45.0 && in_time(t.translation) && wanted && (free_slot || p.stack.id == "arrows")
                })
                .filter(|(t, _)| !skip.iter().any(|g| g.distance(t.translation) < 0.5))
                .min_by(|a, b| a.0.translation.distance(pos).total_cmp(&b.0.translation.distance(pos)))
                .map(|(t, _)| t.translation);
            brain.urgent = zone_goal.is_some();
            let previous = brain.goal;
            // Running from a fight: keep running, unless the ring says otherwise.
            let flee_goal = (brain.flee > 0.0 && zone_goal.is_none()).then_some(brain.goal).flatten();
            // A find at our feet is taken on the way; otherwise the chest comes before the
            // find across the street: chests are what move a warrior through the blocks.
            let near_find = pickup_goal.filter(|p| p.distance(pos) < 14.0);
            brain.goal = zone_goal.or(flee_goal).or(near_find).or(chest_goal).or(pickup_goal).or_else(|| {
                if brain.goal.is_none_or(|g| g.distance(pos) < 4.0) {
                    let c = zone.target_center;
                    let r = zone.target_radius.max(8.0) * 0.7;
                    Some(Vec3::new(c.x + rng.random_range(-r..r), 0.0, c.y + rng.random_range(-r..r)))
                } else {
                    brain.goal
                }
            });
            if brain.goal.map(|g| g.round()) != previous.map(|g| g.round()) {
                brain.goal_time = 0.0;
            }
        }

        // ---- stuck detection: if we barely moved while trying, jump and sidestep; if that keeps
        // failing, back right off and think again.
        brain.stuck_check.1 += dt;
        if brain.stuck_check.1 > 1.0 {
            if brain.stuck_check.0.distance(pos) < 0.6 && brain.walking {
                brain.stuck_for += 1;
                brain.unstick = if brain.stuck_for % 3 == 0 { 1.8 } else { 0.9 };
                brain.replan = 0.0;
                brain.strafe = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
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
        // Losing: break off and run for the far side of a block, then drink and think again.
        // Not out of the ring, and not at all once the ring is too small to run anywhere.
        if zone.outside(pos) {
            brain.flee = 0.0;
        }
        if let Some((_, ttf, _)) = target.filter(|_| health.hp < FLEE_HP && !brain.pressed && brain.flee <= 0.0 && zone.radius > 60.0) {
            let away = (pos - ttf.translation).xz().normalize_or(Vec2::X);
            let spot = pos.xz() + Vec2::from_angle(rng.random_range(-0.6..0.6)).rotate(away) * 45.0;
            let spot = nav.ready().then(|| nav.snap(spot)).flatten().unwrap_or(spot);
            if zone.target_center.distance(spot) < zone.target_radius * 0.9 && zone.center.distance(spot) < zone.radius * 0.9 {
                brain.goal = Some(Vec3::new(spot.x, 0.0, spot.y));
                brain.goal_time = 0.0;
                brain.target = None;
                brain.flee = rng.random_range(6.0..10.0);
            }
        }
        let target = target.filter(|_| brain.flee <= 0.0);
        if let Some((_, ttf, tkin)) = target {
            brain.seen_for += dt;
            let dist = ttf.translation.distance(pos);
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
            let mut aim_at = ttf.translation + Vec3::Y * 0.35;
            if speed > 0.0 {
                let t = dist / speed;
                aim_at += tkin.velocity * t + Vec3::Y * 0.5 * 5.0 * t * t;
            }
            // Shots follow the over-the-shoulder aim ray (like a player's crosshair), not the eye
            // line, so solve the angles from that ray's origin (two fixed-point steps suffice).
            let (mut yaw, mut pitch) = yaw_pitch_to(eye, aim_at);
            for _ in 0..2 {
                let probe = PlayerIntent { yaw, pitch, ..default() };
                let (origin, _) = super::aim_ray(pos, &probe);
                (yaw, pitch) = yaw_pitch_to(origin, aim_at);
            }
            let wobble = brain.aim_error * (1.0 + dist / 40.0);
            intent.yaw = yaw + rng.random_range(-wobble..wobble);
            intent.pitch = pitch + rng.random_range(-wobble..wobble) * 0.6;
            let on_beat = (m.elapsed * 0.4 + brain.tempo.1).fract() < brain.tempo.0;
            if brain.seen_for > brain.react && on_beat {
                match held {
                    Some(ItemKind::Melee { .. }) => {
                        if dist < 2.6 {
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
            // Movement: melee rushes, archers keep a band and strafe.
            brain.strafe_timer -= dt;
            if brain.strafe_timer <= 0.0 {
                brain.strafe_timer = if brain.calm { rng.random_range(1.5..3.5) } else { rng.random_range(0.6..1.6) };
                brain.strafe = -brain.strafe;
            }
            let forward = if matches!(held, Some(ItemKind::Melee { .. })) {
                1.0
            } else if dist > 35.0 {
                0.7
            } else if dist < 10.0 {
                -0.6
            } else {
                0.0
            };
            let (side, ahead) = if brain.calm { (0.3, forward * 0.6) } else { (0.8, forward) };
            intent.move_axis = Vec2::new(brain.strafe * side, ahead);
            if !brain.calm && rng.random_bool(0.01) {
                intent.pressed |= btn::JUMP;
            }
            // The ring won't wait for the fight: keep shooting, but keep walking out of it.
            if brain.pressed {
                if let Some(goal) = brain.goal {
                    if let Some(toward) = heading(&mut brain, pos, goal, dt, &nav, &layout, &street, &spatial, &los_filter) {
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
            let patience = if street.active() { 90.0 } else { 20.0 + brain.path_len * 0.3 };
            if brain.goal_time > patience && !brain.urgent {
                brain.give_up.push(goal);
                brain.goal = None;
                brain.goal_time = 0.0;
            }
            let d = pos.xz().distance(goal.xz());
            let near = d <= 1.4 && (goal.y - pos.y).abs() < 2.6;
            // Around buildings and up stairs by the grid's path; down from a roof by its edge.
            let Some(toward) = heading(&mut brain, pos, goal, dt, &nav, &layout, &street, &spatial, &los_filter) else { continue };
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
        let score = match &db.get(&s.id).kind {
            ItemKind::Bow { pellets, charge, .. } => {
                let usable = s.count > 0 || inv.arrows > 0;
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
