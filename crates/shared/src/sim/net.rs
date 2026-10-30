//! Peer-to-peer play without a host. Every peer runs the whole simulation, but each warrior has
//! one owner: the peer whose player it is, or for a bot, the peer the roster assigns it to. Only
//! the owner moves it, fires its weapons and runs its brain; every other peer marks it `Remote`
//! and follows the state its owner broadcasts. The transport (iroh, or the in-process pipe in
//! `net_check`) only moves `NetMsg`s between the `Outbox` and the other peers' `Inbox`.

use bevy::prelude::*;

use super::{Action, Eliminated, Health, Inventory, Kinematic, MoveMode, PlayerIntent, SimSet, Warrior};

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
#[derive(Clone, Debug, PartialEq)]
pub enum NetMsg {
    /// A warrior's state after its owner's tick (sent every tick; a lost one is simply replaced).
    State(WarriorState),
}

#[derive(Clone, Debug, PartialEq)]
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

#[allow(clippy::type_complexity)]
fn apply_inbox(
    mut inbox: ResMut<Inbox>,
    peers: Res<Peers>,
    mut q: Query<
        (&NetId, &Owner, &mut LastUpdate, &mut Transform, &mut Kinematic, &mut PlayerIntent, &mut Health, &mut Inventory, &mut Action),
        With<Remote>,
    >,
) {
    for (from, msg) in inbox.0.drain(..) {
        match msg {
            NetMsg::State(s) => {
                let Some((_, owner, mut last, mut tf, mut kin, mut intent, mut health, mut inv, mut act)) =
                    q.iter_mut().find(|(id, ..)| **id == s.id)
                else {
                    continue;
                };
                // Only the owner speaks for a warrior; stale updates lose to newer ones.
                if owner.0 != from || from == peers.local || (last.0 != 0 && s.tick <= last.0) {
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
        }
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
