//! Match lifecycle: Pushpaka Vimana flight, the battle, the last warrior standing, restart.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::bots::BotBrain;
use super::combat::{Burst, FireZone, Projectile};
use super::loot::{Chest, Pickup, spawn_loot};
use super::zone::Zone;
use super::{
    Action, CAPSULE_LENGTH, CAPSULE_RADIUS, Eliminated, FeedMsg, Health, Inventory, Kinematic, Layer, LocalPlayer,
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
    "Abhimanyu", "Bhima", "Draupadi", "Eklavya", "Ghatotkacha", "Karna", "Lakshmana", "Nakula", "Sahadeva",
    "Satyaki", "Shikhandi", "Sugriva", "Angada", "Jambavan", "Vibhishana", "Rani Durgavati", "Prithviraj",
    "Hadi Rani", "Chand Bibi", "Velu Nachiyar", "Rudrama Devi", "Banda", "Tara",
];

fn start_match_on_startup(world: &mut World) {
    start_match(world);
}

/// Spawns warriors, loot and resets the zone. Everything from a previous match must already be gone.
pub fn start_match(world: &mut World) {
    let mut rng = rand::rng();
    let half = world.resource::<MapLayout>().half_extent;
    let bots = world.resource::<super::MatchConfig>().bots;
    let autopilot = world.resource::<super::MatchConfig>().autopilot;
    // Flight path crosses the map through a random chord.
    let a = rng.random_range(0.0..std::f32::consts::TAU);
    let off = rng.random_range(-60.0..60.0);
    let dir = Vec2::new(a.cos(), a.sin());
    let perp = Vec2::new(-dir.y, dir.x) * off;
    let from = (-dir * (half + 40.0) + perp).extend(VIMANA_HEIGHT);
    let to = (dir * (half + 40.0) + perp).extend(VIMANA_HEIGHT);
    let generation = world.resource::<Match>().generation + 1;
    *world.resource_mut::<Match>() = Match {
        state: MatchState::Flight,
        vimana_from: Vec3::new(from.x, from.z, from.y),
        vimana_to: Vec3::new(to.x, to.z, to.y),
        total: bots + 1,
        alive: bots + 1,
        generation,
        ..default()
    };
    world.insert_resource(Zone::new());
    world.spawn((Vimana, Transform::default()));

    let melee = world.resource::<ItemDb>().default_melee.clone();
    let mut names: Vec<&str> = BOT_NAMES.to_vec();
    for i in 0..=bots {
        let is_bot = i > 0;
        let name = if is_bot { names.remove(rng.random_range(0..names.len())).to_string() } else { "You".into() };
        let mut e = world.spawn((
            Warrior { name, is_bot, look: i, kills: 0 },
            Health::default(),
            Inventory::new(&melee),
            Action::default(),
            Kinematic { mode: MoveMode::Vimana, ..default() },
            PlayerIntent::default(),
            PosHistory::default(),
            Transform::from_translation(Vec3::new(0.0, VIMANA_HEIGHT, 0.0)),
            RigidBody::Kinematic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
            CollisionLayers::new(Layer::Character, [Layer::World, Layer::Door]),
        ));
        if is_bot || autopilot {
            e.insert(BotBrain::new(&mut rng));
        }
        if !is_bot {
            e.insert(LocalPlayer);
        }
    }
    let layout = world.resource::<MapLayout>().clone();
    let db = world.resource::<ItemDb>().clone();
    let mut commands = world.commands();
    spawn_loot(&mut commands, &layout, &db);
    world.flush();
}

fn restart(world: &mut World) {
    let requested = world.resource_mut::<Messages<RestartMatch>>().drain().count() > 0;
    if !requested {
        return;
    }
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
    mut riders: Query<(&mut Transform, &mut Kinematic, &PlayerIntent), (With<Warrior>, Without<Vimana>)>,
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
            feed.write(FeedMsg(format!("{} is the last warrior standing. Vijayi bhava!", w.name)));
        }
    }
}
