//! Bot warriors. They only write `PlayerIntent`, exactly like a human controller would.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::flow::Match;
use super::loot::{Chest, INTERACT_RANGE, Pickup};
use super::zone::Zone;
use super::{
    EYE_OFFSET, Eliminated, Health, HitMsg, Inventory, Kinematic, MoveMode, PlayerIntent, SimSet, Warrior, btn, solid_mask,
};
use crate::items::{AstraEffect, ItemDb, ItemKind};

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
    pub seen_for: f32,
    /// Whoever hit us recently, and for how much longer we hold the grudge.
    pub provoked: Option<(Entity, f32)>,
    /// Goals we failed to reach (blocked); ignored from now on.
    pub give_up: Vec<Vec3>,
    pub goal_time: f32,
}

impl BotBrain {
    pub fn new(rng: &mut impl rand::Rng) -> Self {
        Self {
            drop_at: rng.random_range(0.15..0.85),
            goal: None,
            target: None,
            think: rng.random_range(0.0..0.3),
            stuck_check: (Vec3::ZERO, 0.0),
            unstick: 0.0,
            strafe: 1.0,
            strafe_timer: 0.0,
            aim_error: rng.random_range(0.025..0.06),
            react: rng.random_range(0.35..0.8),
            seen_for: 0.0,
            provoked: None,
            give_up: vec![],
            goal_time: 0.0,
        }
    }
}

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, drive_bots.in_set(SimSet::Brains));
    }
}

fn yaw_pitch_to(from: Vec3, to: Vec3) -> (f32, f32) {
    let d = to - from;
    let yaw = (-d.x).atan2(-d.z);
    let pitch = d.y.atan2(d.xz().length());
    (yaw, pitch)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn drive_bots(
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
) {
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    let hits: Vec<HitMsg> = hits.read().cloned().collect();
    let los_filter = SpatialQueryFilter::from_mask(solid_mask());
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
        let engage_range = if has_ranged { 55.0 } else { 9.0 };

        match kin.mode {
            MoveMode::Vimana => {
                if m.vimana_t >= brain.drop_at && pos.x.abs() < 230.0 && pos.z.abs() < 230.0 {
                    intent.pressed |= btn::JUMP;
                    // Land near a random chest.
                    let n = chests.iter().count().max(1);
                    let pick = rng.random_range(0..n);
                    brain.goal = chests.iter().nth(pick).map(|(t, _)| t.translation);
                }
                continue;
            }
            MoveMode::Freefall | MoveMode::Gliding => {
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
            let new_target = best.map(|b| b.0);
            if new_target != brain.target {
                brain.seen_for = 0.0;
            }
            brain.target = new_target;

            // Goals when not fighting: zone first, then chests, then pickups, then wander.
            let (left, shrinking) = zone.countdown();
            let travel_time = (zone.target_center.distance(pos.xz()) - zone.target_radius).max(0.0) / 6.0;
            let must_move = shrinking || left < travel_time + 15.0;
            let zone_goal = if zone.outside(pos) || (must_move && zone.target_center.distance(pos.xz()) > zone.target_radius * 0.85) {
                Some(zone.target_center.extend(0.0).xzy())
            } else {
                None
            };
            let skip = brain.give_up.clone();
            let chest_goal = chests
                .iter()
                .filter(|(t, c)| !c.opened && t.translation.distance(pos) < 120.0)
                .filter(|(t, _)| !skip.iter().any(|g| g.distance(t.translation) < 0.5))
                .min_by(|a, b| a.0.translation.distance(pos).total_cmp(&b.0.translation.distance(pos)))
                .map(|(t, _)| t.translation);
            let free_slot = inv.slots.iter().any(|s| s.is_none());
            let pickup_goal = pickups
                .iter()
                .filter(|(t, p)| {
                    let kind = &db.get(&p.stack.id).kind;
                    let stackable = matches!(kind, ItemKind::Arrows { .. } | ItemKind::Consumable { .. } | ItemKind::Astra { .. });
                    // Weapons: only new kinds, or a strictly better rarity of one we hold.
                    let wanted = stackable
                        || inv.slots.iter().flatten().all(|s| s.id != p.stack.id || s.rarity < p.stack.rarity);
                    t.translation.distance(pos) < 45.0 && wanted && (free_slot || p.stack.id == "arrows")
                })
                .filter(|(t, _)| !skip.iter().any(|g| g.distance(t.translation) < 0.5))
                .min_by(|a, b| a.0.translation.distance(pos).total_cmp(&b.0.translation.distance(pos)))
                .map(|(t, _)| t.translation);
            let previous = brain.goal;
            brain.goal = zone_goal.or(pickup_goal).or(chest_goal).or_else(|| {
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

        // ---- stuck detection: if we barely moved while trying, jump and sidestep.
        brain.stuck_check.1 += dt;
        if brain.stuck_check.1 > 1.0 {
            if brain.stuck_check.0.distance(pos) < 0.6 && brain.goal.is_some() {
                brain.unstick = 0.9;
                brain.strafe = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
            }
            brain.stuck_check = (pos, 0.0);
        }

        // ---- combat
        let target = brain.target.and_then(|t| others.get(t).ok());
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
            if brain.seen_for > brain.react {
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
                brain.strafe_timer = rng.random_range(0.6..1.6);
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
            intent.move_axis = Vec2::new(brain.strafe * 0.8, forward);
            if rng.random_bool(0.01) {
                intent.pressed |= btn::JUMP;
            }
            continue;
        }

        // ---- out of combat: heal, reload, loot, travel
        if let Some(slot) = heal_slot(inv, &db, health) {
            if inv.selected != slot {
                intent.select = Some(slot as u8);
            } else {
                intent.pressed |= btn::FIRE;
            }
            continue;
        }
        if let Some(goal) = brain.goal {
            // Can't reach it in reasonable time (wrong floor, blocked): abandon it.
            brain.goal_time += dt;
            if brain.goal_time > 20.0 {
                brain.give_up.push(goal);
                brain.goal = None;
                brain.goal_time = 0.0;
            }
            let (yaw, _) = yaw_pitch_to(pos, goal);
            intent.yaw = yaw;
            intent.pitch = 0.0;
            let d = pos.xz().distance(goal.xz());
            intent.move_axis = if d > 1.4 { Vec2::Y } else { Vec2::ZERO };
            if d > 25.0 {
                intent.held |= btn::SPRINT;
            }
            if d < INTERACT_RANGE - 0.4 && (goal.y - pos.y).abs() < 2.5 {
                intent.pressed |= btn::INTERACT;
            }
        }
        if brain.unstick > 0.0 {
            brain.unstick -= dt;
            intent.move_axis = Vec2::new(brain.strafe, 0.4);
            if brain.unstick > 0.75 {
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
