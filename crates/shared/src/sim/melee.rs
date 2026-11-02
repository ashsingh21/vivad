//! Street fights: a swing with a wind-up, a blade that sweeps an arc, a guard, knock and stun, and
//! old things in hand that wear out and break.
//!
//! The peer that owns a warrior decides what its swing strikes; the peer that owns the warrior
//! struck decides what its guard stops (`Fights::guarded`, called from `combat::apply_damage`), and
//! takes the knock and the stun. No peer waits on another, and `NetMsg::Hit` carries all of it:
//! knock and stun are read from the item's row. A peer sees another's guard from its copy of that
//! warrior's buttons, in hand and facing, which is what its spark and clang go by.
//!
//! `SwingPhase`, `Swing::phase`, the swing's `struck` list, `in_front` and the order of the tests
//! in `guard_share` are adapted by hand from wow-souls (https://github.com/jackke88/wow-souls,
//! crates/wow-souls/src/combat/action.rs, hits.rs and defense.rs), used under this licence. Its
//! numbers are not used, and nothing comes from its GPL `dsr-*` crates.
//!
//! MIT License
//!
//! Copyright (c) 2026 the wow-souls contributors
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::net::Remote;
use super::{Action, DamageCause, DamageMsg, Inventory, Kinematic, Layer, MoveMode, PlayerIntent, SimSet, Warrior, btn};
use crate::items::{ItemDb, ItemKind};

/// The blade's thickness: a sphere this wide is swept along it.
const BLADE_RADIUS: f32 = 0.35;
/// How high up the body a swing travels, above the capsule's centre.
const SWING_HEIGHT: f32 = 0.35;
/// How much a knock lifts its target, as a share of its push.
const KNOCK_LIFT: f32 = 0.35;
/// Seconds a blow on a raised guard adds to the attacker's wait.
const RECOIL: f32 = 0.25;

pub struct MeleePlugin;

impl Plugin for MeleePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BrokeMsg>()
            .add_message::<GuardMsg>()
            .add_message::<BlowMsg>()
            .add_systems(FixedUpdate, (take_blows, recoil).in_set(SimSet::Loot));
    }
}

/// Where a swing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingPhase {
    Startup,
    Active,
    Recovery,
}

/// The blade's numbers from an item's row.
#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub windup: f32,
    pub active: f32,
    pub reach: f32,
    /// Radians swept, centred on the aim.
    pub arc: f32,
    pub max_hits: u32,
}

/// A swing in progress.
#[derive(Clone, Debug, Default)]
pub struct Swing {
    pub t: f32,
    /// The blade's tip and middle at the last tick (none before it starts moving).
    prev: Option<[Vec3; 2]>,
    /// Who this swing has already struck.
    struck: Vec<Entity>,
}

impl Swing {
    pub fn phase(&self, s: &Stroke) -> SwingPhase {
        if self.t < s.windup {
            SwingPhase::Startup
        } else if self.t < s.windup + s.active {
            SwingPhase::Active
        } else {
            SwingPhase::Recovery
        }
    }

    /// Moves the blade on to this tick and returns who it newly struck: warriors, doors and
    /// vehicles, not through a wall. It sweeps right to left across the aim, its first tick a thrust out from
    /// the body, and a sphere swept from the last tick's blade to this one's leaves no gaps.
    /// `body` is the swinger's centre with any crouch taken off, so a crouched swing starts low.
    pub fn sweep(&mut self, me: Entity, body: Vec3, aim: Vec3, s: &Stroke, spatial: &SpatialQuery) -> Vec<Entity> {
        let centre = body + Vec3::Y * SWING_HEIGHT;
        let k = ((self.t - s.windup) / s.active.max(1e-3)).clamp(0.0, 1.0);
        let dir = Quat::from_rotation_y((k - 0.5) * s.arc) * aim.with_y(0.0).normalize_or(Vec3::NEG_Z) + Vec3::Y * aim.y.clamp(-0.5, 0.5);
        let now = [centre + dir * s.reach, centre + dir * s.reach * 0.5];
        let from = self.prev.unwrap_or([centre; 2]);
        self.prev = Some(now);
        let filter = SpatialQueryFilter::from_mask(LayerMask::from([Layer::Character, Layer::Door, Layer::Vehicle])).with_excluded_entities([me]);
        let walls = SpatialQueryFilter::from_mask(LayerMask::from(Layer::World));
        let mut new = vec![];
        for (a, b) in from.into_iter().zip(now) {
            let Ok((path, len)) = Dir3::new_and_length(b - a) else { continue };
            let config = ShapeCastConfig::from_max_distance(len);
            for hit in spatial.shape_hits(&Collider::sphere(BLADE_RADIUS), a, Quat::IDENTITY, path, 8, &config, &filter) {
                if self.struck.len() as u32 >= s.max_hits {
                    return new;
                }
                if self.struck.contains(&hit.entity) {
                    continue;
                }
                let contact = a + path * hit.distance;
                let clear = Dir3::new_and_length(contact - centre).is_ok_and(|(to, d)| spatial.cast_ray(centre, to, d, true, &walls).is_none());
                if clear {
                    self.struck.push(hit.entity);
                    new.push(hit.entity);
                }
            }
        }
        new
    }
}

pub fn in_front(body: &Transform, from: Vec3) -> bool {
    let d = (from - body.translation).with_y(0.0).normalize_or_zero();
    d != Vec3::ZERO && d.dot(*body.forward()) > 0.0
}

/// The share of a blow from `from` that the target's guard stops: nothing unless it holds a melee
/// weapon raised, and faces the blow.
pub fn guard_share(db: &ItemDb, cause: &DamageCause, tf: &Transform, intent: &PlayerIntent, inv: &Inventory, from: Option<Vec3>) -> f32 {
    let (DamageCause::Weapon(by), Some(from)) = (cause, from) else { return 0.0 };
    if !intent.held(btn::AIM) {
        return 0.0;
    }
    let Some(ItemKind::Melee { guard, shot_guard, .. }) = inv.current().map(|s| &db.get(&s.id).kind) else { return 0.0 };
    if !in_front(tf, from) {
        return 0.0;
    }
    match db.get(by).kind {
        ItemKind::Melee { .. } => *guard,
        ItemKind::Bow { .. } => *shot_guard,
        _ => 0.0,
    }
}

/// A guard took (some of) a blow: a client's cue for the clang, and the attacker's for its recoil.
#[derive(Message, Clone, Debug)]
pub struct GuardMsg {
    pub target: Entity,
    pub attacker: Option<Entity>,
    pub at: Vec3,
    /// A bullet, not a blade.
    pub shot: bool,
}

/// A melee weapon took its last blow: a client's cue for the crack, and for telling the player.
#[derive(Message, Clone, Debug)]
pub struct BrokeMsg {
    pub who: Entity,
    pub at: Vec3,
    pub item: String,
}

/// What a blow does to the body it lands on, beyond its damage.
#[derive(Message, Clone, Debug)]
pub struct BlowMsg {
    target: Entity,
    from: Vec3,
    knock: f32,
    stun: f32,
}

/// What `combat::apply_damage` needs from here.
#[derive(SystemParam)]
pub struct Fights<'w, 's> {
    bodies: Query<'w, 's, &'static Transform, With<Warrior>>,
    guards: MessageWriter<'w, GuardMsg>,
    blows: MessageWriter<'w, BlowMsg>,
    broke: MessageWriter<'w, BrokeMsg>,
}

impl Fights<'_, '_> {
    /// What gets through the target's guard of this blow, as this peer sees that guard.
    pub fn guarded(&mut self, db: &ItemDb, msg: &DamageMsg, tf: &Transform, intent: &PlayerIntent, inv: &Inventory) -> f32 {
        let from = msg.source.and_then(|s| self.bodies.get(s).ok()).map(|t| t.translation);
        let share = guard_share(db, &msg.cause, tf, intent, inv, from);
        if share <= 0.0 {
            return msg.amount;
        }
        let shot = matches!(&msg.cause, DamageCause::Weapon(id) if matches!(db.get(id).kind, ItemKind::Bow { .. }));
        self.guards.write(GuardMsg { target: msg.target, attacker: msg.source, at: tf.translation + *tf.forward() * 0.5 + Vec3::Y * 0.4, shot });
        msg.amount * (1.0 - share)
    }

    /// A blow landed on one of this peer's warriors, `taken` of it through any guard: its push and
    /// daze, for `take_blows`.
    pub fn land(&mut self, db: &ItemDb, msg: &DamageMsg, taken: f32) {
        let (DamageCause::Weapon(id), Some(src)) = (&msg.cause, msg.source) else { return };
        let ItemKind::Melee { knock, stun, .. } = db.get(id).kind else { return };
        let Ok(from) = self.bodies.get(src).map(|t| t.translation) else { return };
        let scale = taken / msg.amount.max(1e-3);
        self.blows.write(BlowMsg { target: msg.target, from, knock: knock * scale, stun: stun * scale });
    }

    /// One more blow on the old thing `who` holds (a guard wears it too).
    pub fn wear(&mut self, db: &ItemDb, who: Entity, at: Vec3, inv: &mut Inventory) {
        let id = inv.current().map(|s| s.id.clone());
        if wear(inv, db) {
            self.broke.write(BrokeMsg { who, at, item: id.unwrap_or_default() });
        }
    }
}

/// One more blow on what's in hand, if it is old. True when that was its last, and the slot is empty.
pub fn wear(inv: &mut Inventory, db: &ItemDb) -> bool {
    let slot = inv.selected;
    let Some(s) = slot.checked_sub(1).and_then(|i| inv.slots[i].as_mut()) else { return false };
    if !matches!(db.get(&s.id).kind, ItemKind::Melee { durability: 1.., .. }) {
        return false;
    }
    s.count = s.count.saturating_sub(1);
    if s.count > 0 {
        return false;
    }
    inv.slots[slot - 1] = None;
    true
}

/// Pushed and dazed by a blow, on the peer that owns the body.
fn take_blows(mut blows: MessageReader<BlowMsg>, mut q: Query<(&Transform, &mut Kinematic, &mut Action), Without<Remote>>) {
    for b in blows.read() {
        let Ok((tf, mut kin, mut act)) = q.get_mut(b.target) else { continue };
        if kin.mode != MoveMode::Ground {
            continue;
        }
        if b.stun > 0.0 {
            act.stun = act.stun.max(b.stun);
            act.swing = None;
        }
        if b.knock != 0.0 {
            let away = (tf.translation - b.from).with_y(0.0).normalize_or(Vec3::X);
            kin.velocity = away * b.knock + Vec3::Y * b.knock.abs() * KNOCK_LIFT;
            kin.grounded = false;
            // Lands as a normal jump, without fall damage.
            kin.fall_from = None;
        }
    }
}

/// A swing that met a raised guard jars the hand that swung it.
fn recoil(mut guards: MessageReader<GuardMsg>, mut q: Query<&mut Action, Without<Remote>>) {
    for g in guards.read() {
        if let Some(mut act) = g.attacker.filter(|_| !g.shot).and_then(|a| q.get_mut(a).ok()) {
            act.cooldown += RECOIL;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::items::{ItemStack, Rarity};

    fn db() -> ItemDb {
        ItemDb::load_from_str(include_str!("../../../../assets/data/items.ron")).unwrap()
    }

    fn holding(id: &str, count: u32) -> Inventory {
        let mut inv = Inventory::new("bat");
        inv.slots[1] = Some(ItemStack { id: id.into(), rarity: Rarity::Epic, count });
        inv.selected = 2;
        inv
    }

    #[test]
    fn an_old_weapon_breaks_on_its_last_blow() {
        let (db, mut inv) = (db(), holding("talwar", 2));
        assert!(!wear(&mut inv, &db));
        assert_eq!(inv.slots[1].as_ref().map(|s| s.count), Some(1));
        assert!(wear(&mut inv, &db));
        assert!(inv.slots[1].is_none());
    }

    #[test]
    fn new_things_never_wear() {
        let db = db();
        let mut bat = Inventory::new("bat");
        assert!(!wear(&mut bat, &db));
        assert_eq!(bat.melee.count, 1);
        let mut lathi = holding("lathi", 1);
        assert!(!wear(&mut lathi, &db));
        assert!(lathi.slots[1].is_some());
    }

    #[test]
    fn a_swing_goes_through_its_phases() {
        let s = Stroke { windup: 0.2, active: 0.1, reach: 2.0, arc: 1.0, max_hits: 1 };
        let at = |t| Swing { t, ..default() }.phase(&s);
        assert_eq!(at(0.1), SwingPhase::Startup);
        assert_eq!(at(0.25), SwingPhase::Active);
        assert_eq!(at(0.31), SwingPhase::Recovery);
    }

    /// The blade travels at the height of the body it is given: a crouched swing passes under what
    /// a standing one strikes.
    #[test]
    fn a_crouched_swing_goes_low() {
        let struck = |drop: f32| {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
                .add_plugins(PhysicsPlugins::default())
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
            let head = app.world_mut().spawn((RigidBody::Static, Collider::sphere(0.1), Transform::from_xyz(0.0, SWING_HEIGHT, -1.5), CollisionLayers::new(Layer::Character, LayerMask::ALL))).id();
            app.finish();
            app.cleanup();
            for _ in 0..5 {
                app.update();
            }
            let me = app.world_mut().spawn_empty().id();
            let stroke = Stroke { windup: 0.0, active: 0.1, reach: 2.0, arc: 0.5, max_hits: 1 };
            let hit = move |spatial: SpatialQuery| Swing { t: 0.05, ..default() }.sweep(me, Vec3::Y * -drop, Vec3::NEG_Z, &stroke, &spatial);
            app.world_mut().run_system_once(hit).unwrap().contains(&head)
        };
        assert!(struck(0.0));
        assert!(!struck(crate::sim::stance::CROUCH_DROP));
    }

    #[test]
    fn a_guard_stops_blows_only_from_the_front() {
        let db = db();
        let body = Transform::default();
        let aim = PlayerIntent { held: btn::AIM, ..default() };
        let inv = holding("tawa", 1);
        let cut = DamageCause::Weapon("bat".into());
        let front = Some(Vec3::new(0.0, 0.0, -2.0));
        assert!(guard_share(&db, &cut, &body, &aim, &inv, front) > 0.5);
        assert_eq!(guard_share(&db, &cut, &body, &aim, &inv, Some(Vec3::new(0.0, 0.0, 2.0))), 0.0);
        assert_eq!(guard_share(&db, &cut, &body, &PlayerIntent::default(), &inv, front), 0.0);
        assert_eq!(guard_share(&db, &DamageCause::Fall, &body, &aim, &inv, front), 0.0);
    }

    #[test]
    fn only_a_tawa_or_a_dhal_stops_a_bullet() {
        let db = db();
        let (body, aim) = (Transform::default(), PlayerIntent { held: btn::AIM, ..default() });
        let (shot, front) = (DamageCause::Weapon("tamancha".into()), Some(Vec3::new(0.0, 0.0, -9.0)));
        assert!(guard_share(&db, &shot, &body, &aim, &holding("tawa", 1), front) > 0.0);
        assert!(guard_share(&db, &shot, &body, &aim, &holding("dhal", 9), front) > 0.5);
        assert_eq!(guard_share(&db, &shot, &body, &aim, &holding("lathi", 1), front), 0.0);
    }
}
