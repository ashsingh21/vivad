//! Match lifecycle: Pushpaka Vimana flight, the battle, the last warrior standing, restart.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::bots::BotBrain;
use super::net::{Inbox, LastUpdate, NetId, NetMsg, NetPose, Outbox, Owner, Peers, Remote};
use super::combat::{Burst, FireZone, Projectile};
use super::loot::{Chest, Pickup, spawn_loot};
use super::zone::Zone;
use super::{
    match_rng, stream, Action, CAPSULE_LENGTH, CAPSULE_RADIUS, Eliminated, FeedMsg, Health, Inventory, Kinematic, Layer, LocalPlayer,
    MoveMode, PlayerIntent, PosHistory, RestartMatch, SimSet, Warrior, btn,
};
use crate::items::ItemDb;
use crate::map::MapLayout;

pub const VIMANA_HEIGHT: f32 = 120.0;
pub const VIMANA_SPEED: f32 = 22.0;

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
            .add_systems(Startup, start_match_on_startup)
            .add_systems(FixedUpdate, (restart, fly_vimana, check_winner).chain().in_set(SimSet::Flow));
    }
}

const BOT_NAMES: [&str; 23] = [
    "Pappu", "Bunty", "Chintu", "Golu", "Munna", "Raju", "Babloo", "Guddu", "Sonu", "Monu", "Tinku", "Rinku",
    "Pinky", "Dolly", "Sweety", "Bittu", "Chotu", "Lallan", "Babli", "Chikki", "Laddoo", "Jhumri", "Bobby",
];

fn start_match_on_startup(world: &mut World) {
    if !world.resource::<super::MatchConfig>().online {
        start_match(world);
    }
}

/// Spawns warriors, loot and resets the zone. Everything from a previous match must already be gone.
pub fn start_match(world: &mut World) {
    // Bot personalities stay local: a bot is only ever run by one peer.
    let mut rng = rand::rng();
    let half = world.resource::<MapLayout>().half_extent;
    let config = *world.resource::<super::MatchConfig>();
    let (bots, skill, autopilot) = (config.bots, config.bot_skill, config.autopilot);
    let peers = world.resource::<Peers>().clone();
    let humans = peers.all.len() as u32;
    let generation = world.resource::<Match>().generation + 1;
    // A rematch with a fixed seed still differs from the last, the same way on every peer.
    let seed = config.match_seed.map(|s| s.wrapping_add(generation as u64)).unwrap_or_else(|| rng.random());
    // Flight path crosses the map through a random chord.
    let mut path = match_rng(seed, stream::VIMANA);
    let a = path.random_range(0.0..std::f32::consts::TAU);
    let off = path.random_range(-60.0..60.0);
    let dir = Vec2::new(a.cos(), a.sin());
    let perp = Vec2::new(-dir.y, dir.x) * off;
    let from = (-dir * (half + 40.0) + perp).extend(VIMANA_HEIGHT);
    let to = (dir * (half + 40.0) + perp).extend(VIMANA_HEIGHT);
    *world.resource_mut::<Match>() = Match {
        state: MatchState::Flight,
        vimana_from: Vec3::new(from.x, from.z, from.y),
        vimana_to: Vec3::new(to.x, to.z, to.y),
        total: humans + bots,
        alive: humans + bots,
        generation,
        seed,
        ..default()
    };
    world.insert_resource(Zone::new(seed));
    world.spawn((Vimana, Transform::default()));

    let melee = world.resource::<ItemDb>().default_melee.clone();
    let mut names: Vec<&str> = BOT_NAMES.to_vec();
    let mut name_rng = match_rng(seed, stream::NAMES);
    // VIVAD_LOOK=1 swaps every warrior's look (the player's body) for captures.
    let look_shift = std::env::var("VIVAD_LOOK").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    // The roster, the same on every peer: a warrior per peer (in peer order), then the bots.
    for i in 0..humans + bots {
        let is_bot = i >= humans;
        let owner = if is_bot { peers.bot_owner(i - humans) } else { peers.all[i as usize] };
        let local = owner == peers.local;
        let name = if is_bot {
            names.remove(name_rng.random_range(0..names.len())).to_string()
        } else if local {
            "You".into()
        } else {
            peers.names.get(&owner).cloned().unwrap_or_else(|| format!("Warrior {}", i + 1))
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
            Transform::from_translation(Vec3::new(0.0, VIMANA_HEIGHT, 0.0)),
            RigidBody::Kinematic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
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
    let layout = world.resource::<MapLayout>().clone();
    let db = world.resource::<ItemDb>().clone();
    let mut commands = world.commands();
    spawn_loot(&mut commands, &layout, &db, seed);
    world.flush();
}

/// A new match, when our player asks for one or another peer has started it. Online, only once
/// the match is over (for everyone), and the peers tell each other so they all move on together.
fn restart(world: &mut World) {
    let requested = world.resource_mut::<Messages<RestartMatch>>().drain().count() > 0;
    let generation = world.resource::<Match>().generation;
    let online = world.resource::<Peers>().all.len() > 1;
    // `apply_inbox` stops at a restart and leaves it first in the inbox.
    let mut told = false;
    let mut inbox = world.resource_mut::<Inbox>();
    if let Some((_, NetMsg::Restart { generation: g })) = inbox.0.first() {
        // An older one: we both started the same next match.
        told = *g == generation;
        inbox.0.remove(0);
    }
    let asked = requested && (!online || world.resource::<Match>().state == MatchState::Ended);
    if !told && !asked {
        return;
    }
    if asked && !told {
        world.resource_mut::<Outbox>().0.push(NetMsg::Restart { generation });
    }
    world.resource_mut::<super::loot::Claims>().0.clear();
    let mut doomed: Vec<Entity> = vec![];
    for e in world.query_filtered::<Entity, Or<(With<Warrior>, With<Chest>, With<Pickup>, With<Projectile>, With<FireZone>, With<Burst>, With<Vimana>)>>().iter(world) {
        doomed.push(e);
    }
    for e in doomed {
        world.despawn(e);
    }
    start_match(world);
}

#[allow(clippy::type_complexity)]
fn fly_vimana(
    mut m: ResMut<Match>,
    mut riders: Query<(&mut Transform, &mut Kinematic, &PlayerIntent), (With<Warrior>, Without<Vimana>, Without<Remote>)>,
    mut vimana: Query<&mut Transform, With<Vimana>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    m.elapsed += dt;
    if m.state != MatchState::Flight {
        return;
    }
    let len = m.vimana_from.distance(m.vimana_to);
    m.vimana_t += VIMANA_SPEED * dt / len;
    let pos = m.vimana_pos();
    let dir = m.vimana_dir();
    for mut t in &mut vimana {
        t.translation = pos;
        t.look_to(dir, Vec3::Y);
    }
    let done = m.vimana_t >= 1.0;
    // Players may only leap once the vimana is over the arena.
    let over_land = pos.x.abs() < 240.0 && pos.z.abs() < 240.0;
    for (mut tf, mut kin, intent) in &mut riders {
        if kin.mode != MoveMode::Vimana {
            continue;
        }
        tf.translation = pos - Vec3::Y * 3.0;
        if done || (over_land && intent.pressed(btn::JUMP)) {
            kin.mode = MoveMode::Freefall;
            kin.velocity = dir * VIMANA_SPEED * 0.5;
        }
    }
    if done {
        m.state = MatchState::InProgress;
    }
}

fn check_winner(
    mut m: ResMut<Match>,
    alive: Query<Entity, (With<Warrior>, Without<Eliminated>)>,
    names: Query<&Warrior>,
    mut feed: MessageWriter<FeedMsg>,
) {
    m.alive = alive.iter().count() as u32;
    if m.state == MatchState::InProgress && m.alive <= 1 {
        m.state = MatchState::Ended;
        m.winner = alive.iter().next();
        if let Some(w) = m.winner.and_then(|e| names.get(e).ok()) {
            feed.write(FeedMsg(if w.name == "You" { "Tumne Panga jeet liya!".to_string() } else { format!("{} ne Panga jeet liya!", w.name) }));
        }
    }
}
