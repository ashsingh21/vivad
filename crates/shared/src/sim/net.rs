//! Peer-to-peer play without a host. Every peer runs the whole simulation, but each warrior has
//! one owner: the peer whose player it is, or for a bot, the peer the roster assigns it to. Only
//! the owner moves it, fires its weapons and runs its brain; every other peer marks it `Remote`
//! and follows the state its owner broadcasts. The transport (iroh, or the in-process pipe in
//! `net_check`) only moves `NetMsg`s between the `Outbox` and the other peers' `Inbox`.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::combat::{KillMsg, Projectile, feed_line};
use super::loot::spawn_pickup;
use super::{
    Action, DamageCause, DamageMsg, Door, Eliminated, FeedMsg, Health, Inventory, Kinematic, MoveMode, PlayerIntent, SimSet, Warrior,
};
use crate::items::{AstraEffect, ItemDb, ItemKind, ItemStack, Rarity};

/// A peer's identity (later: derived from its iroh endpoint key).
pub type PeerId = u64;

/// Who is in the match. `all` is sorted, so every peer derives the same roster from it.
#[derive(Resource, Clone, Debug)]
pub struct Peers {
    pub local: PeerId,
    pub all: Vec<PeerId>,
}

impl Default for Peers {
    /// Single player: just us.
    fn default() -> Self {
        Self { local: 0, all: vec![0] }
    }
}

impl Peers {
    pub fn new(local: PeerId, mut all: Vec<PeerId>) -> Self {
        all.sort();
        all.dedup();
        assert!(all.contains(&local), "the local peer must be in the match");
        Self { local, all }
    }

    /// Who runs bot `k` (counting bots from 0): dealt round the peers in turn.
    pub fn bot_owner(&self, k: u32) -> PeerId {
        self.all[k as usize % self.all.len()]
    }
}

/// Names an entity the same way on every peer. Warriors: their roster index.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u32);

/// The peer that simulates this warrior.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner(pub PeerId);

/// Owned by another peer: the local simulation leaves it alone and applies its owner's updates.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Remote;

/// What peers tell each other.
#[derive(Clone, Debug)]
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
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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

/// This peer's fixed-tick count.
#[derive(Resource, Default, Debug)]
pub struct NetTick(pub u32);

/// The newest update applied per remote warrior.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LastUpdate(pub u32);

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Peers>()
            .init_resource::<Outbox>()
            .init_resource::<Inbox>()
            .init_resource::<NetTick>()
            .add_systems(FixedUpdate, apply_inbox.in_set(SimSet::Brains).before(super::bots::drive_bots))
            .add_systems(FixedUpdate, send_state.in_set(SimSet::Cleanup).before(super::clear_latched_input));
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
        (&mut LastUpdate, &mut Transform, &mut Kinematic, &mut PlayerIntent, &mut Health, &mut Inventory, &mut Action),
        With<Remote>,
    >,
    names: Query<&Warrior>,
    mut doors: Query<(Entity, &mut Door)>,
    mut damage: MessageWriter<DamageMsg>,
    mut feed: MessageWriter<FeedMsg>,
    mut kills: MessageWriter<KillMsg>,
) {
    let find = |id: NetId| ids.iter().find(|(_, i, ..)| **i == id).map(|(e, _, o, remote, down)| (e, o.0, remote, down));
    for (from, msg) in inbox.0.drain(..) {
        if from == peers.local {
            continue;
        }
        match msg {
            NetMsg::State(s) => {
                // Only the owner speaks for a warrior; stale updates lose to newer ones.
                let Some((e, owner, true, false)) = find(s.id) else { continue };
                let Ok((mut last, mut tf, mut kin, mut intent, mut health, mut inv, mut act)) = q.get_mut(e) else { continue };
                if owner != from || (last.0 != 0 && s.tick <= last.0) {
                    continue;
                }
                last.0 = s.tick;
                tf.translation = s.pos;
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
                let killer = by.and_then(|b| find(b)).map(|(k, ..)| k);
                commands.entity(e).insert(Eliminated { placement, by: killer }).remove::<Collider>();
                drop_loot(&mut commands, at, drops);
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
pub fn drop_loot(commands: &mut Commands, at: Vec3, drops: Vec<ItemStack>) {
    for (i, st) in drops.into_iter().enumerate() {
        let a = i as f32 * 1.3;
        spawn_pickup(commands, at - Vec3::Y * 0.8 + Vec3::new(a.cos(), 0.0, a.sin()) * 1.2, st);
    }
}

#[allow(clippy::type_complexity)]
fn send_state(
    mut tick: ResMut<NetTick>,
    mut outbox: ResMut<Outbox>,
    q: Query<(&NetId, &Transform, &Kinematic, &PlayerIntent, &Health, &Inventory, &Action), (With<Warrior>, Without<Remote>, Without<Eliminated>)>,
) {
    tick.0 += 1;
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
