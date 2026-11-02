//! Match lifecycle: Pushpaka Vimana flight, the battle, the last warrior standing, restart.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::bots::BotBrain;
use super::net::{Inbox, LastUpdate, NetId, NetMsg, NetPose, Outbox, Owner, Peers, Remote};
use super::combat::{Burst, FireZone, Projectile};
use super::loot::{Chest, Pickup, spawn_loot};
use super::stance::{Stance, standing_capsule};
use super::zone::Zone;
use super::{
    match_rng, stream, Action, Eliminated, FeedMsg, Health, Inventory, Kinematic, Layer, LocalPlayer,
    MoveMode, PlayerIntent, PosHistory, RestartMatch, SimSet, Warrior, btn,
};
use crate::items::ItemDb;
use crate::map::compose::{PLACES, Tier};
use crate::map::{MapId, MapLayout, MapStyle};

pub const VIMANA_HEIGHT: f32 = 120.0;
pub const VIMANA_SPEED: f32 = 22.0;
/// The drop on the 4 km NCR: high enough to see where you are going, fast enough to cross the map
/// in about 70 seconds.
pub const NCR_VIMANA_HEIGHT: f32 = 500.0;
pub const NCR_VIMANA_SPEED: f32 = 60.0;
/// Through the Yamuna, m/s: slow enough that a bridge beats it, and nobody is ever trapped.
pub const NCR_WADE_SPEED: f32 = 2.4;
/// Flight on the NCR: how far a line may pass from the disc's middle, by the size of the match,
/// and how far it starts ahead of the disc so a rider can get their bearings.
const FLIGHT_OFFSET: [(u32, f32); 3] = [(16, 250.0), (32, 375.0), (u32::MAX, 800.0)];
const FLIGHT_LEAD_IN: f32 = 250.0;
/// Rerolls of a flight line that passes too few big districts.
const FLIGHT_TRIES: usize = 12;

/// How a match runs on a map, where the maps differ. The small maps keep the numbers they always
/// had; everything here is the same on every peer, as it comes from the map alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub vimana_height: f32,
    pub vimana_speed: f32,
    /// Walking pace through water; None: water doesn't slow anyone.
    pub wade_speed: Option<f32>,
    /// A warrior falling far off a roof gets its glider back.
    pub roof_rescue: bool,
    /// The press that leaves the ride starts a dive; a second press opens the glider. On the small
    /// maps the one press does both.
    pub dive_first: bool,
    /// How far from where it leaps a bot will make for.
    pub glide_reach: f32,
    /// How far off a bot on foot goes for a chest.
    pub chest_sight: f32,
    /// How near a find on the ground must be for a bot with no bow to take it before a chest.
    pub find_sight: f32,
}

impl Rules {
    pub fn of(style: MapStyle) -> Self {
        match style {
            MapStyle::City => Self { vimana_height: VIMANA_HEIGHT, vimana_speed: VIMANA_SPEED, wade_speed: None, roof_rescue: false, dive_first: false, glide_reach: 110.0, chest_sight: 120.0, find_sight: 14.0 },
            MapStyle::Ncr => Self {
                vimana_height: NCR_VIMANA_HEIGHT,
                vimana_speed: NCR_VIMANA_SPEED,
                wade_speed: Some(NCR_WADE_SPEED),
                roof_rescue: true,
                dive_first: true,
                glide_reach: 0.9 * super::movement::glide_reach(NCR_VIMANA_HEIGHT),
                chest_sight: 250.0,
                // Chests are a few hundred metres apart here: anything in sight comes first.
                find_sight: 45.0,
            },
        }
    }
}

/// The share of the line from `a` to `b` that lies inside the square of half-size `half`, if any.
fn clip_to_square(a: Vec2, b: Vec2, half: f32) -> Option<(Vec2, Vec2)> {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    let d = b - a;
    for (p, v) in [(a.x, d.x), (a.y, d.y)] {
        if v.abs() < 1e-6 {
            if p.abs() > half {
                return None;
            }
            continue;
        }
        let (u, w) = ((-half - p) / v, (half - p) / v);
        t0 = t0.max(u.min(w));
        t1 = t1.min(u.max(w));
    }
    (t0 < t1).then(|| (a + d * t0, a + d * t1))
}

/// Distance from a point to a straight line between two others.
fn to_line(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let u = b - a;
    p.distance(a + u * ((p - a).dot(u) / u.length_squared().max(1e-6)).clamp(0.0, 1.0))
}

/// The NCR's flight: a chord of the start disc, cut to the map, chosen by the seed. It ends where
/// it leaves the disc (the last riders are put out there), and passes a couple of the big
/// districts within a dive, so no line is a dull one.
fn ncr_flight(seed: u64, zone: &Zone, half: f32, height: f32, players: u32) -> (Vec2, Vec2) {
    let mut path = match_rng(seed, stream::VIMANA);
    let offset = FLIGHT_OFFSET.iter().find(|o| players <= o.0).map_or(0.0, |o| o.1);
    let reach = super::movement::dive_reach(height);
    let mut best: Option<(usize, Vec2, Vec2)> = None;
    for _ in 0..FLIGHT_TRIES {
        let dir = Vec2::from_angle(path.random_range(0.0..std::f32::consts::TAU));
        let off = path.random_range(-offset..offset);
        let mid = zone.center + dir.perp() * off;
        let h = (zone.radius * zone.radius - off * off).max(0.0).sqrt();
        let Some((from, to)) = clip_to_square(mid - dir * h, mid + dir * h, half - 30.0) else { continue };
        let big = PLACES.iter().filter(|p| matches!(p.tier, Tier::Hot | Tier::Warm) && to_line(p.at, from, to) < reach).count();
        if best.is_none_or(|b| big > b.0) {
            best = Some((big, from - dir * FLIGHT_LEAD_IN, to));
        }
        if big >= 2 {
            break;
        }
    }
    best.map_or((zone.center - Vec2::X * half, zone.center), |b| (b.1, b.2))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MatchState {
    #[default]
    Flight,
    InProgress,
    Ended,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct Match {
    pub state: MatchState,
    pub elapsed: f32,
    pub vimana_from: Vec3,
    pub vimana_to: Vec3,
    pub vimana_t: f32,
    pub alive: u32,
    pub total: u32,
    pub winner: Option<Entity>,
    /// Bumped on every restart so clients can rebuild.
    pub generation: u32,
    /// Everything random that peers must agree on comes from this (see `MatchConfig::match_seed`).
    pub seed: u64,
}

impl Match {
    pub fn vimana_pos(&self) -> Vec3 {
        self.vimana_from.lerp(self.vimana_to, self.vimana_t.clamp(0.0, 1.0))
    }
    pub fn vimana_dir(&self) -> Vec3 {
        (self.vimana_to - self.vimana_from).normalize_or(Vec3::X)
    }
}

/// Marker for the vimana itself (so clients can draw it).
#[derive(Component)]
pub struct Vimana;

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Match>()
            .init_resource::<StartedBy>()
            .add_systems(Startup, start_match_on_startup)
            .add_systems(FixedUpdate, (cast_votes, restart, fly_vimana, check_winner).chain().in_set(SimSet::Flow));
    }
}

/// The peer whose restart began this match (us, if we asked first): when two peers start the
/// next match at once on different maps, the lower one's stands.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct StartedBy(pub super::net::PeerId);

/// Our player's pick for the next match's map (between matches, or once out of this one).
#[derive(Message, Clone, Copy, Debug)]
pub struct VoteMap(pub MapId);

/// Who wants which map next: every peer's vote (`NetMsg::Vote`), ours included. Cleared each match.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapVotes(pub std::collections::BTreeMap<super::net::PeerId, MapId>);

impl MapVotes {
    pub fn count(&self, map: MapId) -> usize {
        self.0.values().filter(|m| **m == map).count()
    }
}

impl MapId {
    /// The maps a room can be on: its code names the map (`net::room_map`), so a host who picks
    /// any of these gets it.
    pub const ROOMS: [MapId; 5] = MapId::ALL;
}

/// The maps that take turns: only the NCR, the one final map. A small map picked with VIVAD_MAP
/// is not in the turn, so `next_map` keeps it until the game closes.
pub const ROTATION: [MapId; 1] = [MapId::Ncr];

/// The next match's map: the most voted for, ties going to whichever comes up soonest in turn;
/// with no votes, the next in turn.
pub fn next_map(current: MapId, votes: &MapVotes) -> MapId {
    let pool = &ROTATION;
    let Some(at) = pool.iter().position(|m| *m == current) else { return current };
    // Turns from now until each map comes up: the current one last.
    let wait = |m: MapId| (pool.iter().position(|p| *p == m).unwrap_or(at) + pool.len() - at - 1) % pool.len();
    pool.iter().copied().max_by_key(|m| (votes.count(*m), std::cmp::Reverse(wait(*m)))).unwrap_or(current)
}

fn cast_votes(
    mut asked: MessageReader<VoteMap>,
    mut votes: ResMut<MapVotes>,
    peers: Res<Peers>,
    config: Res<super::MatchConfig>,
    mut outbox: ResMut<Outbox>,
) {
    for VoteMap(map) in asked.read() {
        if !config.rotate || !ROTATION.contains(map) || votes.0.get(&peers.local) == Some(map) {
            continue;
        }
        votes.0.insert(peers.local, *map);
        outbox.0.push(NetMsg::Vote { map: map.index() });
    }
}

const BOT_NAMES: [&str; 23] = [
    "Pappu", "Bunty", "Chintu", "Golu", "Munna", "Raju", "Babloo", "Guddu", "Sonu", "Monu", "Tinku", "Rinku",
    "Pinky", "Dolly", "Sweety", "Bittu", "Chotu", "Lallan", "Babli", "Chikki", "Laddoo", "Jhumri", "Bobby",
];

pub fn tick_simulation_pause(mut pause: ResMut<super::SimulationPause>, time: Res<Time>) {
    if pause.timer > 0.0 {
        pause.timer -= time.delta_secs();
    }
}

fn start_match_on_startup(world: &mut World) {
    let config = world.resource::<super::MatchConfig>();
    if !config.online && !config.menu {
        start_match(world);
    }
}

/// Online, the next match starts for everyone at once: when this one is over, or once no player is
/// left in it (bots alone could fight on for minutes). `standing`: the warriors still in it.
pub fn next_match_open<'a>(m: &Match, mut standing: impl Iterator<Item = &'a Warrior>) -> bool {
    m.state == MatchState::Ended || !standing.any(|w| !w.is_bot)
}

/// Sweeps the match away: warriors, loot and whatever is in the air. The map stays.
fn clear_match(world: &mut World) {
    world.resource_mut::<MapVotes>().0.clear();
    world.resource_mut::<super::loot::Claims>().0.clear();
    world.resource_mut::<super::loot::Gone>().0.clear();
    let mut doomed: Vec<Entity> = vec![];
    for e in world.query_filtered::<Entity, Or<(With<Warrior>, With<Chest>, With<Pickup>, With<Projectile>, With<FireZone>, With<Burst>, With<Vimana>)>>().iter(world) {
        doomed.push(e);
    }
    for e in doomed {
        world.despawn(e);
    }
}

/// Out of the match and back to the empty map, for the menu; `RestartMatch` starts the next one.
pub fn leave_match(world: &mut World) {
    clear_match(world);
    let generation = world.resource::<Match>().generation;
    *world.resource_mut::<Match>() = Match { generation, ..default() };
}

/// Spawns warriors, loot and resets the zone. Everything from a previous match must already be gone.
pub fn start_match(world: &mut World) {
    // Bot personalities stay local: a bot is only ever run by one peer.
    let mut rng = rand::rng();
    let (half, style) = {
        let layout = world.resource::<MapLayout>();
        (layout.half_extent, layout.style)
    };
    let rules = Rules::of(style);
    let config = *world.resource::<super::MatchConfig>();
    let (bots, skill, autopilot) = (config.bots, config.bot_skill, config.autopilot);
    let peers = world.resource::<Peers>().clone();
    let humans = peers.all.len() as u32;
    let generation = world.resource::<Match>().generation + 1;
    // A rematch with a fixed seed still differs from the last, the same way on every peer.
    let seed = config.match_seed.map(|s| s.wrapping_add(generation as u64)).unwrap_or_else(|| rng.random());
    let zone = match style {
        MapStyle::City => Zone::for_extent(seed, half),
        MapStyle::Ncr => Zone::for_ncr(seed, humans + bots, world.resource::<MapLayout>(), world.resource::<super::nav::StreetFloor>()),
    };
    let (from, to) = match style {
        // Flight path crosses the map through a random chord.
        MapStyle::City => {
            let mut path = match_rng(seed, stream::VIMANA);
            let a = path.random_range(0.0..std::f32::consts::TAU);
            let off = path.random_range(-half * 0.24..half * 0.24);
            let dir = Vec2::new(a.cos(), a.sin());
            let perp = Vec2::new(-dir.y, dir.x) * off;
            (-dir * (half + 40.0) + perp, dir * (half + 40.0) + perp)
        }
        MapStyle::Ncr => ncr_flight(seed, &zone, half, rules.vimana_height, humans + bots),
    };
    *world.resource_mut::<Match>() = Match {
        state: MatchState::Flight,
        vimana_from: Vec3::new(from.x, rules.vimana_height, from.y),
        vimana_to: Vec3::new(to.x, rules.vimana_height, to.y),
        total: humans + bots,
        alive: humans + bots,
        generation,
        seed,
        ..default()
    };
    let disc = (zone.center, zone.radius);
    world.insert_resource(zone);
    world.spawn((Vimana, Transform::default()));

    let melee = world.resource::<ItemDb>().default_melee.clone();
    let mut names: Vec<&str> = BOT_NAMES.to_vec();
    let mut name_rng = match_rng(seed, stream::NAMES);
    let mut round = 0;
    // VIVAD_LOOK=1 swaps every warrior's look (the player's body) for captures.
    let look_shift = std::env::var("VIVAD_LOOK").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    // The roster, the same on every peer: a warrior per peer (in peer order), then the bots.
    for i in 0..humans + bots {
        let is_bot = i >= humans;
        let owner = if is_bot { peers.bot_owner(i - humans) } else { peers.all[i as usize] };
        let local = owner == peers.local;
        let name = if is_bot {
            // More bots than names: deal the names out again, numbered.
            if names.is_empty() {
                names = BOT_NAMES.to_vec();
                round += 1;
            }
            let n = names.remove(name_rng.random_range(0..names.len()));
            if round == 0 { n.to_string() } else { format!("{n} {}", round + 1) }
        } else if local {
            "You".into()
        } else {
            peers.names.get(&owner).cloned().unwrap_or_else(|| crate::lines::with(world.resource::<crate::lines::Lines>().text("lobby.player"), &[("n", &(i + 1).to_string())]))
        };
        let mut e = world.spawn((
            NetId(i),
            Owner(owner),
            Warrior { name, is_bot, look: i + look_shift, kills: 0 },
            Health::default(),
            Inventory::new(&melee),
            Action::default(),
            Kinematic { mode: MoveMode::Vimana, ..default() },
            PlayerIntent::default(),
            PosHistory::default(),
            Transform::from_translation(Vec3::new(0.0, rules.vimana_height, 0.0)),
            RigidBody::Kinematic,
            standing_capsule(),
            Stance::default(),
            // Found by spatial queries (shots, sweeps, movement), but never paired for contacts:
            // movement is our own collide-and-slide, and nothing reads contacts.
            CollisionLayers::new(Layer::Character, LayerMask::NONE),
        ));
        if !local {
            e.insert((Remote, LastUpdate::default(), NetPose::default()));
        } else if is_bot || autopilot {
            e.insert(BotBrain::new(&mut rng, skill));
        }
        if local && !is_bot {
            e.insert(LocalPlayer);
        }
    }
    // By reference: the NCR's layout is far too big to copy for every match.
    world.resource_scope(|world, layout: Mut<MapLayout>| {
        world.resource_scope(|world, db: Mut<ItemDb>| {
            let mut commands = world.commands();
            spawn_loot(&mut commands, &layout, &db, seed, disc);
            world.flush();
        });
    });
}

/// A new match, when our player asks for one or another peer has started it. Online, only once
/// `next_match_open` says so, and the peers tell each other so they all move on together. With
/// rotation on, the next match is on the map most voted for (`next_map`), and the restart names
/// it, so a peer that is told follows onto the same map.
fn restart(world: &mut World) {
    let requested = world.resource_mut::<Messages<RestartMatch>>().drain().count() > 0;
    let generation = world.resource::<Match>().generation;
    let peers = world.resource::<Peers>().clone();
    let online = peers.all.len() > 1;
    let config = *world.resource::<super::MatchConfig>();
    // `apply_inbox` stops at a restart and leaves it first in the inbox.
    let mut told = None;
    let started_by = world.resource::<StartedBy>().0;
    let mut inbox = world.resource_mut::<Inbox>();
    if let Some(&(from, NetMsg::Restart { generation: g, map })) = inbox.0.first() {
        inbox.0.remove(0);
        let map = MapId::from_index(map);
        if g == generation {
            told = Some((from, map));
        } else if g + 1 == generation && map != config.map && from < started_by {
            // We both started the next match, on different maps: the lower peer's stands, so
            // start it again on theirs.
            world.resource_mut::<Match>().generation = g;
            told = Some((from, map));
        }
        // Otherwise an older one: we both started the same next match.
    }
    // Online, until the first match is built there is nothing to restart (the empty `Match` reads as
    // over), and starting one here would put a second match under the one the lobby begins.
    let waiting = config.online && generation == 0;
    let mut standing = world.query_filtered::<&Warrior, Without<Eliminated>>();
    let asked = requested && !waiting && (!online || next_match_open(world.resource::<Match>(), standing.iter(world)));
    if told.is_none() && !asked {
        return;
    }
    let (by, map) = told.unwrap_or_else(|| (peers.local, if config.rotate { next_map(config.map, world.resource::<MapVotes>()) } else { config.map }));
    if told.is_none() {
        world.resource_mut::<Outbox>().0.push(NetMsg::Restart { generation, map: map.index() });
    }
    world.resource_mut::<StartedBy>().0 = by;
    clear_match(world);
    if map != config.map {
        super::load_map(world, map);
    }
    start_match(world);
}

#[allow(clippy::type_complexity)]
fn fly_vimana(
    mut m: ResMut<Match>,
    layout: Res<MapLayout>,
    mut riders: Query<(&mut Transform, &mut Kinematic, &mut PlayerIntent), (With<Warrior>, Without<Vimana>, Without<Remote>)>,
    mut vimana: Query<&mut Transform, With<Vimana>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let rules = Rules::of(layout.style);
    m.elapsed += dt;
    if m.state != MatchState::Flight {
        // The NCR's flight ends over the map: fly on and out of sight, not hang in the sky.
        if layout.style == MapStyle::Ncr {
            for mut t in &mut vimana {
                let next = t.translation + m.vimana_dir() * rules.vimana_speed * dt;
                if next.xz().abs().max_element() < layout.half_extent + 40.0 {
                    t.translation = next;
                }
            }
        }
        return;
    }
    let len = m.vimana_from.distance(m.vimana_to);
    m.vimana_t += rules.vimana_speed * dt / len;
    let pos = m.vimana_pos();
    let dir = m.vimana_dir();
    for mut t in &mut vimana {
        t.translation = pos;
        t.look_to(dir, Vec3::Y);
    }
    let done = m.vimana_t >= 1.0;
    // Players may only leap once the vimana is over the arena.
    let over_land = pos.x.abs() < layout.half_extent - 10.0 && pos.z.abs() < layout.half_extent - 10.0;
    for (mut tf, mut kin, mut intent) in &mut riders {
        if kin.mode != MoveMode::Vimana {
            continue;
        }
        tf.translation = pos - Vec3::Y * 3.0;
        if done || (over_land && intent.pressed(btn::JUMP)) {
            kin.mode = MoveMode::Freefall;
            kin.velocity = dir * rules.vimana_speed * 0.5;
            if rules.dive_first {
                intent.pressed &= !btn::JUMP;
            }
        }
    }
    if done {
        m.state = MatchState::InProgress;
    }
}

fn check_winner(
    mut m: ResMut<Match>,
    alive: Query<Entity, (With<Warrior>, Without<Eliminated>)>,
    fallen: Query<(Entity, &Eliminated), With<Warrior>>,
    names: Query<(&Warrior, Has<LocalPlayer>)>,
    lines: Res<crate::lines::Lines>,
    mut feed: MessageWriter<FeedMsg>,
) {
    m.alive = alive.iter().count() as u32;
    if m.state == MatchState::InProgress && m.alive <= 1 {
        m.state = MatchState::Ended;
        // The last two can fall in the same tick, both burning in the ring: the one who fell
        // last (placement 1) stood the longest, and is crowned; every peer has the placements.
        m.winner = alive.iter().next().or_else(|| fallen.iter().find(|(_, e)| e.placement == 1).map(|(e, _)| e));
        if let Some((w, you)) = m.winner.and_then(|e| names.get(e).ok()) {
            let line = if you { lines.text("win.feed.you") } else { lines.pick("win.feed", m.seed, w.look as u64) };
            feed.write(FeedMsg(crate::lines::with(line, &[("k", &w.name)])));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::sim::{MatchConfig, SimPlugin};

    /// The NCR is the default and the only map in the turn; a small map picked by hand stays, and
    /// no vote moves a match on to one.
    #[test]
    fn the_ncr_is_the_one_map_in_the_turn() {
        assert_eq!(MapId::default(), MapId::Ncr);
        let mut votes = MapVotes::default();
        votes.0.insert(1, MapId::Chandni);
        assert_eq!(next_map(MapId::Ncr, &votes), MapId::Ncr);
        assert_eq!(next_map(MapId::Ncr, &MapVotes::default()), MapId::Ncr);
        for map in [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate] {
            assert_eq!(next_map(map, &votes), map);
        }
    }

    /// On a small map: every one of these would otherwise build the whole NCR.
    fn sim(config: MatchConfig) -> App {
        let mut app = App::new();
        app.insert_resource(MatchConfig { map: MapId::Cp, ..config })
            .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
            .add_plugins(PhysicsPlugins::default())
            .add_plugins(SimPlugin { items_ron: include_str!("../../../../assets/data/items.ron").to_string() })
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
        app.finish();
        app.cleanup();
        for _ in 0..5 {
            app.update();
        }
        app
    }

    fn restart_now(app: &mut App) {
        app.world_mut().write_message(RestartMatch);
        for _ in 0..5 {
            app.update();
        }
    }

    #[test]
    fn a_lobby_starts_no_match_of_its_own() {
        let mut app = sim(MatchConfig { online: true, bots: 1, ..default() });
        restart_now(&mut app);
        assert_eq!(app.world().resource::<Match>().generation, 0);
        let w = app.world_mut();
        assert_eq!(w.query::<&Warrior>().iter(w).count(), 0);
    }

    #[test]
    fn online_the_next_match_waits_only_for_players() {
        let m = Match { state: MatchState::InProgress, ..default() };
        let player = Warrior { name: "Player".into(), is_bot: false, look: 0, kills: 0 };
        let bot = Warrior { is_bot: true, ..player.clone() };
        assert!(!next_match_open(&m, [&player, &bot].into_iter()));
        assert!(next_match_open(&m, [&bot].into_iter()));
        assert!(next_match_open(&Match { state: MatchState::Ended, ..default() }, [&player].into_iter()));
    }

    #[test]
    fn the_next_match_stays_on_this_map() {
        let mut app = sim(MatchConfig { bots: 1, ..default() });
        let map = app.world().resource::<MatchConfig>().map;
        restart_now(&mut app);
        assert_eq!(app.world().resource::<Match>().generation, 2);
        assert_eq!(app.world().resource::<MatchConfig>().map, map);
        let w = app.world_mut();
        assert_eq!(w.query::<&Warrior>().iter(w).count(), 2);
    }
}
