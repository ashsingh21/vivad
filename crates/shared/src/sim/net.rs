//! Peer-to-peer play without a host. Every peer runs the whole simulation, but each warrior has
//! one owner: the peer whose player it is, or for a bot, the peer the roster assigns it to. Only
//! the owner moves it, fires its weapons and runs its brain; every other peer marks it `Remote`
//! and follows the state its owner broadcasts. The transport (iroh, or the in-process pipe in
//! `net_check`) only moves `NetMsg`s between the `Outbox` and the other peers' `Inbox`.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::combat::{KillMsg, Projectile, feed_line};
use super::flow::Match;
use super::loot::{Chest, Claims, Pickup, PickupId, open_chest, revoke, spawn_pickup};
use super::{
    Action, DamageCause, DamageMsg, Door, Eliminated, FeedMsg, Health, Inventory, Kinematic, MatchConfig, MoveMode, PlayerIntent, SimSet,
    Warrior,
};
use super::bots::BotBrain;
use crate::items::{AstraEffect, ItemDb, ItemKind, ItemStack, Rarity};

/// A peer's identity: the first eight bytes of its iroh endpoint key.
pub type PeerId = u64;

/// Who is in the match. `all` is sorted, so every peer derives the same roster from it.
#[derive(Resource, Clone, Debug)]
pub struct Peers {
    pub local: PeerId,
    pub all: Vec<PeerId>,
    /// The players' chosen names (a peer without one plays as "Warrior N").
    pub names: std::collections::BTreeMap<PeerId, String>,
}

impl Default for Peers {
    /// Single player: just us.
    fn default() -> Self {
        Self { local: 0, all: vec![0], names: default() }
    }
}

impl Peers {
    pub fn new(local: PeerId, mut all: Vec<PeerId>) -> Self {
        all.sort();
        all.dedup();
        assert!(all.contains(&local), "the local peer must be in the match");
        Self { local, all, names: default() }
    }

    /// Who runs bot `k` (counting bots from 0): dealt round the peers in turn.
    pub fn bot_owner(&self, k: u32) -> PeerId {
        self.all[k as usize % self.all.len()]
    }

    /// Who takes a bot over when `owner` goes: dealt round the others by roster index.
    pub fn heir(&self, owner: PeerId, id: NetId) -> PeerId {
        let rest: Vec<PeerId> = self.all.iter().copied().filter(|p| *p != owner).collect();
        if rest.is_empty() { owner } else { rest[id.0 as usize % rest.len()] }
    }
}

/// Names an entity the same way on every peer. Warriors: their roster index.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NetId(pub u32);

/// The peer that simulates this warrior.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner(pub PeerId);

/// Owned by another peer: the local simulation leaves it alone and applies its owner's updates.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Remote;

/// What peers tell each other.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetMsg {
    /// A warrior's state after its owner's tick (sent every tick; a lost one is simply replaced).
    State(WarriorState),
    /// An arrow or astra left a warrior's hand. Other peers fly an echo of it: an arrow's echo
    /// never hurts (its shooter decides what it hit), an astra's bursts on every peer and each
    /// applies it to its own warriors.
    Shot(Shot),
    /// The attacker's peer saw its hit land on a warrior owned by the receiver, who applies it.
    Hit { target: NetId, by: NetId, amount: f32, headshot: bool, item: String },
    /// A warrior fell on its owner's peer: everyone marks it, credits the kill and drops its loot.
    Down { id: NetId, by: Option<NetId>, placement: u32, at: Vec3, drops: Vec<ItemStack>, cause: DamageCause },
    /// A door took damage from one of the sender's warriors (index in `MapLayout::doors`).
    DoorHit { index: usize, amount: f32 },
    /// One of the sender's warriors opened a chest (index in `MapLayout::chests`).
    ChestOpened { index: usize },
    /// One of the sender's warriors took a pickup, at this match time.
    Took { pickup: PickupId, by: NetId, at: f32 },
    /// One of the sender's warriors put an item down to make room for another.
    Dropped { id: PickupId, at: Vec3, stack: ItemStack },
    /// What one of the sender's warriors carries now (sent when it changes), so the others draw
    /// the right weapon and can take the warrior over if its owner leaves.
    Inventory { id: NetId, inv: Inventory },
    /// The sender started the next match after match `generation` ended (see `flow::restart`).
    Restart { generation: u32 },
    /// The sender, heir to `from`, has taken bot `id` over because `from` went silent (see
    /// `stand_in`).
    TakeOver { id: NetId, from: PeerId },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shot {
    pub owner: NetId,
    pub from: Vec3,
    pub velocity: Vec3,
    pub gravity: f32,
    pub damage: f32,
    pub headshot_mult: f32,
    pub item: String,
    pub astra: Option<AstraEffect>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WarriorState {
    pub id: NetId,
    /// The owner's tick count, so late or reordered updates are dropped.
    pub tick: u32,
    pub pos: Vec3,
    pub velocity: Vec3,
    pub grounded: bool,
    pub mode: MoveMode,
    pub yaw: f32,
    pub pitch: f32,
    pub held: u16,
    pub hp: f32,
    pub shield: f32,
    pub selected: u8,
    pub since_attack: f32,
    pub charge: f32,
}

/// Messages for the other peers, drained by the transport after each update.
#[derive(Resource, Default, Debug)]
pub struct Outbox(pub Vec<NetMsg>);

/// Messages from other peers, filled by the transport, applied at the start of the next tick.
#[derive(Resource, Default, Debug)]
pub struct Inbox(pub Vec<(PeerId, NetMsg)>);

/// Peers whose connection is gone, pushed by the transport and handled at the next tick.
#[derive(Resource, Default, Debug)]
pub struct Departures(pub Vec<PeerId>);

/// This peer's fixed-tick count.
#[derive(Resource, Default, Debug)]
pub struct NetTick(pub u32);

/// The newest update applied per remote warrior.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LastUpdate(pub u32);

/// Where a remote warrior's owner last had it, and how long ago. Between updates the copy carries
/// on at that velocity, and is eased onto each new one rather than snapped, so a late or lost
/// update doesn't make it stall and jump.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NetPose {
    pub pos: Vec3,
    pub velocity: Vec3,
    pub age: f32,
}

/// Further than this from its update, a copy is put straight there (a respawn, a long stall).
const SNAP: f32 = 4.0;
/// How long a copy keeps going on an old update's velocity before it waits for the next.
const COAST: f32 = 0.25;
/// Time constant of easing a copy onto where its owner has it, seconds.
const EASE: f32 = 0.05;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Peers>()
            .init_resource::<Outbox>()
            .init_resource::<Inbox>()
            .init_resource::<NetTick>()
            .init_resource::<Departures>()
            .init_resource::<Heard>()
            .add_systems(
                FixedUpdate,
                (depart, listen, apply_loot, apply_inbox, stand_in).chain().in_set(SimSet::Brains).before(super::bots::drive_bots),
            )
            .add_systems(FixedUpdate, follow_remote.in_set(SimSet::Movement))
            .add_systems(FixedUpdate, (send_state, send_inventory).in_set(SimSet::Cleanup).before(super::clear_latched_input));
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply_inbox(
    mut commands: Commands,
    mut inbox: ResMut<Inbox>,
    peers: Res<Peers>,
    db: Res<ItemDb>,
    ids: Query<(Entity, &NetId, &Owner, Has<Remote>, Has<Eliminated>)>,
    mut q: Query<
        (&mut LastUpdate, &mut Transform, &mut Kinematic, &mut PlayerIntent, &mut Health, &mut Inventory, &mut Action, &mut NetPose),
        With<Remote>,
    >,
    names: Query<&Warrior>,
    mut doors: Query<(Entity, &mut Door)>,
    mut damage: MessageWriter<DamageMsg>,
    mut feed: MessageWriter<FeedMsg>,
    mut kills: MessageWriter<KillMsg>,
) {
    let find = |id: NetId| ids.iter().find(|(_, i, ..)| **i == id).map(|(e, _, o, remote, down)| (e, o.0, remote, down));
    let mut msgs = std::mem::take(&mut inbox.0).into_iter();
    while let Some((from, msg)) = msgs.next() {
        if from == peers.local {
            continue;
        }
        match msg {
            // The rest belongs to the next match: leave it for after the restart.
            NetMsg::Restart { .. } => {
                inbox.0.push((from, msg));
                inbox.0.extend(msgs);
                break;
            }
            NetMsg::State(s) => {
                // Only the owner speaks for a warrior; stale updates lose to newer ones.
                let Some((e, owner, true, false)) = find(s.id) else { continue };
                let Ok((mut last, mut tf, mut kin, mut intent, mut health, mut inv, mut act, mut pose)) = q.get_mut(e) else { continue };
                if owner != from || (last.0 != 0 && s.tick <= last.0) {
                    continue;
                }
                if last.0 == 0 || tf.translation.distance(s.pos) > SNAP {
                    tf.translation = s.pos;
                }
                last.0 = s.tick;
                *pose = NetPose { pos: s.pos, velocity: s.velocity, age: 0.0 };
                tf.rotation = Quat::from_rotation_y(s.yaw);
                kin.velocity = s.velocity;
                kin.grounded = s.grounded;
                kin.mode = s.mode;
                intent.yaw = s.yaw;
                intent.pitch = s.pitch;
                intent.held = s.held;
                health.hp = s.hp;
                health.shield = s.shield;
                inv.selected = s.selected as usize;
                act.since_attack = s.since_attack;
                act.charge = s.charge;
            }
            NetMsg::Shot(shot) => {
                let Some((owner, peer, true, _)) = find(shot.owner) else { continue };
                if peer != from {
                    continue;
                }
                let dir = shot.velocity.normalize_or(Vec3::NEG_Z);
                commands.spawn((
                    Projectile {
                        owner,
                        velocity: shot.velocity,
                        gravity: shot.gravity,
                        damage: shot.damage,
                        headshot_mult: shot.headshot_mult,
                        ttl: if shot.astra.is_some() { 6.0 } else { 4.0 },
                        item: shot.item,
                        astra: shot.astra,
                        stuck: false,
                    },
                    Transform::from_translation(shot.from).looking_to(dir, Vec3::Y),
                ));
            }
            NetMsg::Hit { target, by, amount, headshot, item } => {
                // Ours, still standing, hit by a warrior still standing here, by no more than
                // the weapon can do.
                let Some((victim, _, false, false)) = find(target) else { continue };
                let Some((attacker, peer, _, false)) = find(by) else { continue };
                if peer != from || amount <= 0.0 || amount > max_hit(&db, &item) {
                    continue;
                }
                damage.write(DamageMsg { target: victim, amount, source: Some(attacker), headshot, cause: DamageCause::Weapon(item) });
            }
            NetMsg::Down { id, by, placement, at, drops, cause } => {
                let Some((e, owner, true, false)) = find(id) else { continue };
                if owner != from {
                    continue;
                }
                let killer = by.and_then(find).map(|(k, ..)| k);
                commands.entity(e).insert(Eliminated { placement, by: killer }).remove::<Collider>();
                drop_loot(&mut commands, id, at, drops);
                let name = |e: Entity| names.get(e).map(|w| w.name.as_str()).unwrap_or_default();
                feed.write(FeedMsg(feed_line(name(e), &cause, killer.map(name), &db)));
                if let Some(k) = killer.filter(|k| *k != e) {
                    kills.write(KillMsg { killer: k });
                }
            }
            NetMsg::DoorHit { index, amount } => {
                if let Some((e, mut door)) = doors.iter_mut().find(|(_, d)| d.index == index) {
                    door.hp -= amount;
                    if door.hp <= 0.0 {
                        commands.entity(e).despawn();
                    }
                }
            }
            NetMsg::Inventory { id, inv: new } => {
                let Some((e, owner, true, _)) = find(id) else { continue };
                if owner != from {
                    continue;
                }
                if let Ok((.., mut inv, _, _)) = q.get_mut(e) {
                    *inv = new;
                }
            }
            // Handled by `apply_loot` and `listen`.
            NetMsg::ChestOpened { .. } | NetMsg::Took { .. } | NetMsg::Dropped { .. } | NetMsg::TakeOver { .. } => {}
        }
    }
}

/// The most one hit of `item` can do: its best rarity, on the head.
fn max_hit(db: &ItemDb, item: &str) -> f32 {
    let Some(def) = db.items.get(item) else { return 0.0 };
    let best = Rarity::ALL.iter().map(|r| r.damage_scale()).fold(1.0, f32::max);
    let most = match &def.kind {
        ItemKind::Melee { damage, .. } => damage * best,
        ItemKind::Bow { damage, headshot, .. } => damage * best * headshot.max(1.0),
        _ => 0.0,
    };
    most + 0.01
}

/// A fallen warrior's belongings, scattered round where it fell (the same on every peer).
pub fn drop_loot(commands: &mut Commands, victim: NetId, at: Vec3, drops: Vec<ItemStack>) {
    for (i, st) in drops.into_iter().enumerate() {
        let a = i as f32 * 1.3;
        spawn_pickup(commands, PickupId::fallen(victim, i), at - Vec3::Y * 0.8 + Vec3::new(a.cos(), 0.0, a.sin()) * 1.2, st);
    }
}

/// Chests, pickups and contested takes (taken out of the inbox before the rest is applied).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply_loot(
    mut commands: Commands,
    mut inbox: ResMut<Inbox>,
    db: Res<ItemDb>,
    m: Res<Match>,
    spatial: SpatialQuery,
    mut chests: Query<(&Transform, &mut Chest)>,
    pickups: Query<(Entity, &Pickup)>,
    mut ours: Query<(&NetId, &mut Inventory), Without<Remote>>,
    mut claims: ResMut<Claims>,
) {
    let mut rest = vec![];
    let mut msgs = std::mem::take(&mut inbox.0).into_iter();
    while let Some((from, msg)) = msgs.next() {
        match msg {
            NetMsg::Restart { .. } => {
                rest.push((from, msg));
                rest.extend(msgs);
                break;
            }
            NetMsg::ChestOpened { index } => {
                // Both peers opening it at once is harmless: the contents are the same.
                if let Some((ct, mut c)) = chests.iter_mut().find(|(_, c)| c.index == index && !c.opened) {
                    open_chest(&mut commands, &spatial, &db, m.seed, ct, &mut c);
                }
            }
            NetMsg::Took { pickup, by, at } => {
                if let Some((e, _)) = pickups.iter().find(|(_, p)| p.id == pickup) {
                    commands.entity(e).despawn();
                } else if let Some(k) = claims.0.iter().position(|c| c.pickup == pickup) {
                    // We took it too: the earlier take keeps it.
                    let mine = &claims.0[k];
                    if (at, by.0) < (mine.at, mine.by.0) {
                        if let Some((_, mut inv)) = ours.iter_mut().find(|(id, _)| **id == mine.by) {
                            revoke(&mut inv, &mine.stack);
                        }
                        claims.0.remove(k);
                    }
                }
            }
            NetMsg::Dropped { id, at, stack } => spawn_pickup(&mut commands, id, at, stack),
            other => rest.push((from, other)),
        }
    }
    inbox.0 = rest;
}

#[allow(clippy::type_complexity)]
fn send_state(
    mut tick: ResMut<NetTick>,
    mut outbox: ResMut<Outbox>,
    peers: Res<Peers>,
    q: Query<(&NetId, &Transform, &Kinematic, &PlayerIntent, &Health, &Inventory, &Action), (With<Warrior>, Without<Remote>, Without<Eliminated>)>,
) {
    tick.0 += 1;
    // Alone: nobody to tell, and no transport to empty the outbox.
    if peers.all.len() == 1 {
        outbox.0.clear();
        return;
    }
    for (id, tf, kin, intent, health, inv, act) in &q {
        outbox.0.push(NetMsg::State(WarriorState {
            id: *id,
            tick: tick.0,
            pos: tf.translation,
            velocity: kin.velocity,
            grounded: kin.grounded,
            mode: kin.mode,
            yaw: intent.yaw,
            pitch: intent.pitch,
            held: intent.held,
            hp: health.hp,
            shield: health.shield,
            selected: inv.selected as u8,
            since_attack: act.since_attack,
            charge: act.charge,
        }));
    }
}

/// What each of our warriors carries, sent whenever it changes.
#[allow(clippy::type_complexity)]
fn send_inventory(
    mut outbox: ResMut<Outbox>,
    peers: Res<Peers>,
    mut sent: Local<std::collections::BTreeMap<u32, Inventory>>,
    m: Res<Match>,
    mut generation: Local<u32>,
    q: Query<(&NetId, &Inventory), (With<Warrior>, Without<Remote>, Without<Eliminated>)>,
) {
    if *generation != m.generation {
        *generation = m.generation;
        sent.clear();
    }
    if peers.all.len() == 1 {
        return;
    }
    for (id, inv) in &q {
        if sent.get(&id.0) != Some(inv) {
            sent.insert(id.0, inv.clone());
            outbox.0.push(NetMsg::Inventory { id: *id, inv: inv.clone() });
        }
    }
}

/// A peer left: its player falls out of the match and its bots are dealt round those who remain
/// (the same way on every peer), the new owner's copy taking over where the last update left it.
#[allow(clippy::type_complexity)]
fn depart(
    mut commands: Commands,
    mut departures: ResMut<Departures>,
    mut peers: ResMut<Peers>,
    config: Res<MatchConfig>,
    m: Res<Match>,
    mut q: Query<(Entity, &NetId, &mut Owner, &Warrior, Has<Eliminated>)>,
    mut feed: MessageWriter<FeedMsg>,
) {
    let mut placement = m.alive;
    for gone in std::mem::take(&mut departures.0) {
        if gone == peers.local || !peers.all.contains(&gone) {
            continue;
        }
        for (e, id, mut owner, w, down) in &mut q {
            if owner.0 != gone {
                continue;
            }
            if w.is_bot {
                owner.0 = peers.heir(gone, *id);
                if owner.0 == peers.local {
                    commands.entity(e).remove::<(Remote, LastUpdate, NetPose)>().insert(BotBrain::new(&mut rand::rng(), config.bot_skill));
                } else {
                    commands.entity(e).insert(LastUpdate::default());
                }
            } else if !down {
                commands.entity(e).insert(Eliminated { placement, by: None }).remove::<Collider>();
                placement = placement.saturating_sub(1);
                feed.write(FeedMsg(format!("{} left the match", w.name)));
            }
        }
        peers.all.retain(|p| *p != gone);
    }
}

/// When (by our tick count) we last heard from each peer.
#[derive(Resource, Default, Debug)]
pub struct Heard(pub std::collections::BTreeMap<PeerId, u32>);

/// A peer whose messages stop this long (seconds) loses its bots to their heirs. A live owner
/// sends every tick; the network itself notices a crash only after several seconds, and the bots
/// would stand frozen till then.
const SILENT: f32 = 0.75;

/// Note who we've heard from, and hand over bots that their heirs have taken (before the rest of
/// the inbox, so the new owner's updates that follow are taken).
#[allow(clippy::type_complexity)]
fn listen(
    mut commands: Commands,
    mut inbox: ResMut<Inbox>,
    tick: Res<NetTick>,
    mut heard: ResMut<Heard>,
    peers: Res<Peers>,
    mut q: Query<(Entity, &NetId, &mut Owner, &Transform, Has<Remote>)>,
) {
    let mut rest = vec![];
    let mut msgs = std::mem::take(&mut inbox.0).into_iter();
    while let Some((from, msg)) = msgs.next() {
        heard.0.insert(from, tick.0);
        match msg {
            NetMsg::Restart { .. } => {
                rest.push((from, msg));
                rest.extend(msgs);
                break;
            }
            NetMsg::TakeOver { id, from: was } => {
                let Some((e, _, mut owner, tf, remote)) = q.iter_mut().find(|(_, i, ..)| **i == id) else { continue };
                if owner.0 != was || peers.heir(was, id) != from {
                    continue;
                }
                owner.0 = from;
                if remote {
                    commands.entity(e).insert(LastUpdate::default());
                } else {
                    // Ours: we stalled, and it went on without us.
                    let pose = NetPose { pos: tf.translation, ..default() };
                    commands.entity(e).remove::<BotBrain>().insert((Remote, LastUpdate::default(), pose));
                }
            }
            other => rest.push((from, other)),
        }
    }
    inbox.0 = rest;
}

/// Take over the live bots we are heir to whose owner has gone silent, and say so.
#[allow(clippy::type_complexity)]
fn stand_in(
    mut commands: Commands,
    mut outbox: ResMut<Outbox>,
    tick: Res<NetTick>,
    mut heard: ResMut<Heard>,
    peers: Res<Peers>,
    config: Res<MatchConfig>,
    mut q: Query<(Entity, &NetId, &mut Owner, &Warrior), (With<Remote>, Without<Eliminated>)>,
) {
    let silent = (SILENT * super::TICK_HZ as f32) as u32;
    for &p in &peers.all {
        if p == peers.local {
            continue;
        }
        let last = *heard.0.entry(p).or_insert(tick.0);
        if tick.0.saturating_sub(last) < silent {
            continue;
        }
        for (e, id, mut owner, w) in &mut q {
            if owner.0 == p && w.is_bot && peers.heir(p, *id) == peers.local {
                info!("{} stands in for silent peer {p:016x}'s bot {}", peers.local, w.name);
                owner.0 = peers.local;
                commands.entity(e).remove::<(Remote, LastUpdate, NetPose)>().insert(BotBrain::new(&mut rand::rng(), config.bot_skill));
                outbox.0.push(NetMsg::TakeOver { id: *id, from: p });
            }
        }
    }
}

/// Move each remote copy on: carried by its last update's velocity, and eased onto where that
/// update puts it now.
fn follow_remote(time: Res<Time>, mut q: Query<(&mut Transform, &mut NetPose), (With<Remote>, Without<Eliminated>)>) {
    let dt = time.delta_secs();
    let ease = 1.0 - (-dt / EASE).exp();
    for (mut tf, mut pose) in &mut q {
        pose.age += dt;
        let coasting = pose.age <= COAST;
        let target = pose.pos + pose.velocity * pose.age.min(COAST);
        let step = if coasting { pose.velocity * dt } else { Vec3::ZERO };
        let at = tf.translation + step;
        tf.translation = at + (target - at) * ease;
    }
}
