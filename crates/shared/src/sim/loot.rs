//! Chests (peti, kalash), floor loot and pickups.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::flow::Match;
use super::net::Remote;
use super::{Eliminated, Inventory, PlayerIntent, SimSet, Warrior, btn, match_rng, solid_mask, stream};
use crate::items::{ItemDb, ItemKind, ItemStack, Rarity};
use crate::map::{ChestKind, MapLayout};

pub const INTERACT_RANGE: f32 = 2.4;

#[derive(Component, Clone, Debug)]
pub struct Chest {
    /// Index in `MapLayout::chests`: names the chest across peers and seeds its contents.
    pub index: usize,
    pub kind: ChestKind,
    pub opened: bool,
}

#[derive(Component, Clone, Debug)]
pub struct Pickup {
    pub stack: ItemStack,
    /// Can't be picked up until this reaches zero (avoids instantly re-grabbing a drop).
    pub delay: f32,
}

pub struct LootPlugin;

impl Plugin for LootPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (interact, auto_pickup_arrows, tick_pickups).in_set(SimSet::Loot));
    }
}

pub fn spawn_pickup(commands: &mut Commands, at: Vec3, stack: ItemStack) {
    commands.spawn((Pickup { stack, delay: 0.6 }, Transform::from_translation(at)));
}

fn roll_rarity(kind: Option<ChestKind>, rng: &mut impl rand::Rng) -> Rarity {
    let w: [f32; 5] = match kind {
        Some(ChestKind::Kalash) => [5.0, 25.0, 38.0, 24.0, 8.0],
        Some(ChestKind::Peti) => [42.0, 33.0, 17.0, 7.0, 1.0],
        None => [60.0, 28.0, 10.0, 2.0, 0.0],
    };
    let total: f32 = w.iter().sum();
    let mut r = rng.random_range(0.0..total);
    for (i, wi) in w.iter().enumerate() {
        if r < *wi {
            return Rarity::ALL[i];
        }
        r -= wi;
    }
    Rarity::Common
}

/// Pick an item of the wanted category at (or below) the rolled rarity.
fn roll_item(db: &ItemDb, rarity: Rarity, filter: impl Fn(&ItemKind) -> bool, rng: &mut impl rand::Rng) -> Option<ItemStack> {
    for r in Rarity::ALL.iter().rev().filter(|r| **r <= rarity) {
        let pool: Vec<(&String, f32)> = db
            .items
            .iter()
            .filter(|(_, d)| filter(&d.kind))
            .filter_map(|(id, d)| d.drops.iter().find(|(dr, _)| dr == r).map(|(_, w)| (id, *w)))
            .collect();
        let total: f32 = pool.iter().map(|p| p.1).sum();
        if total <= 0.0 {
            continue;
        }
        let mut x = rng.random_range(0.0..total);
        for (id, w) in &pool {
            if x < *w {
                return Some(make_stack(db, id, *r));
            }
            x -= w;
        }
    }
    None
}

pub fn make_stack(db: &ItemDb, id: &str, rarity: Rarity) -> ItemStack {
    let count = match &db.get(id).kind {
        ItemKind::Bow { magazine, .. } => *magazine,
        ItemKind::Astra { count, .. } => *count,
        ItemKind::Consumable { stack, .. } => (*stack / 3).max(1),
        ItemKind::Arrows { amount } => *amount,
        ItemKind::Melee { .. } => 1,
    };
    ItemStack { id: id.into(), rarity, count }
}

fn is_weapon(k: &ItemKind) -> bool {
    matches!(k, ItemKind::Bow { .. } | ItemKind::Astra { .. })
}
fn is_consumable(k: &ItemKind) -> bool {
    matches!(k, ItemKind::Consumable { .. })
}

/// Fresh chests and floor loot for a new match (the same on every peer for the same seed).
pub fn spawn_loot(commands: &mut Commands, layout: &MapLayout, db: &ItemDb, seed: u64) {
    let mut rng = match_rng(seed, stream::FLOOR_LOOT);
    for (index, c) in layout.chests.iter().enumerate() {
        commands.spawn((
            Chest { index, kind: c.kind, opened: false },
            Transform::from_translation(c.pos).with_rotation(Quat::from_rotation_y(c.yaw)),
        ));
    }
    for p in &layout.floor_loot {
        if !rng.random_bool(0.75) {
            continue;
        }
        let rarity = roll_rarity(None, &mut rng);
        let item = if rng.random_bool(0.55) {
            roll_item(db, rarity, is_weapon, &mut rng)
        } else {
            roll_item(db, rarity, |k| is_consumable(k) || matches!(k, ItemKind::Arrows { .. }), &mut rng)
        };
        if let Some(st) = item {
            // Bows on the floor come with a quiver beside them.
            if matches!(db.get(&st.id).kind, ItemKind::Bow { .. }) {
                commands.spawn((Pickup { stack: make_stack(db, "arrows", Rarity::Common), delay: 0.0 },
                                Transform::from_translation(*p + Vec3::new(0.7, 0.0, 0.3))));
            }
            commands.spawn((Pickup { stack: st, delay: 0.0 }, Transform::from_translation(*p)));
        }
    }
}

/// What a chest holds: fixed by the match seed, whoever opens it.
pub fn chest_contents(db: &ItemDb, seed: u64, index: usize, kind: ChestKind) -> Vec<ItemStack> {
    let mut rng = match_rng(seed, stream::CHEST + index as u64);
    let mut out = vec![];
    let rarity = roll_rarity(Some(kind), &mut rng);
    out.extend(roll_item(db, rarity, is_weapon, &mut rng));
    out.extend(roll_item(db, roll_rarity(Some(kind), &mut rng), is_consumable, &mut rng));
    out.push(make_stack(db, "arrows", Rarity::Common));
    if kind == ChestKind::Kalash {
        out.extend(roll_item(db, Rarity::Legendary, is_consumable, &mut rng));
    }
    out
}

/// Try to add a stack to an inventory. Returns what could not be taken.
pub fn give(inv: &mut Inventory, db: &ItemDb, mut st: ItemStack) -> Option<ItemStack> {
    match &db.get(&st.id).kind {
        ItemKind::Arrows { .. } => {
            inv.arrows += st.count;
            return None;
        }
        ItemKind::Consumable { stack, .. } | ItemKind::Astra { count: stack, .. } => {
            let cap = if matches!(db.get(&st.id).kind, ItemKind::Astra { .. }) { stack * 3 } else { *stack };
            for s in inv.slots.iter_mut().flatten() {
                if s.id == st.id && s.count < cap {
                    let take = (cap - s.count).min(st.count);
                    s.count += take;
                    st.count -= take;
                    if st.count == 0 {
                        return None;
                    }
                }
            }
        }
        _ => {}
    }
    if let Some(slot) = inv.slots.iter_mut().find(|s| s.is_none()) {
        *slot = Some(st);
        return None;
    }
    Some(st)
}

#[allow(clippy::type_complexity)]
fn interact(
    mut commands: Commands,
    mut warriors: Query<(&Transform, &PlayerIntent, &mut Inventory), (With<Warrior>, Without<Eliminated>, Without<Remote>)>,
    mut chests: Query<(&Transform, &mut Chest), Without<Warrior>>,
    pickups: Query<(Entity, &Transform, &Pickup), Without<Warrior>>,
    db: Res<ItemDb>,
    m: Res<Match>,
    spatial: SpatialQuery,
) {
    for (tf, intent, mut inv) in &mut warriors {
        if !intent.pressed(btn::INTERACT) {
            continue;
        }
        let me = tf.translation;
        // Nearest chest or pickup in range wins.
        let chest = chests
            .iter_mut()
            .filter(|(t, c)| !c.opened && t.translation.distance(me) < INTERACT_RANGE)
            .min_by(|a, b| a.0.translation.distance(me).total_cmp(&b.0.translation.distance(me)));
        if let Some((ct, mut c)) = chest {
            c.opened = true;
            let fwd = ct.rotation * Vec3::Z;
            for (i, st) in chest_contents(&db, m.seed, c.index, c.kind).into_iter().enumerate() {
                let side = (i as f32 - 1.0) * 0.7;
                let mut at = ct.translation + fwd * 1.0 + ct.rotation * Vec3::X * side + Vec3::Y * 0.6;
                let filter = SpatialQueryFilter::from_mask(solid_mask());
                if let Some(hit) = spatial.cast_ray(at, Dir3::NEG_Y, 3.0, true, &filter) {
                    at.y -= hit.distance - 0.25;
                }
                commands.spawn((Pickup { stack: st, delay: 0.3 }, Transform::from_translation(at)));
            }
            continue;
        }
        let nearest = pickups
            .iter()
            .filter(|(_, t, p)| p.delay <= 0.0 && t.translation.distance(me) < INTERACT_RANGE)
            .min_by(|a, b| a.1.translation.distance(me).total_cmp(&b.1.translation.distance(me)));
        if let Some((pe, pt, p)) = nearest {
            commands.entity(pe).despawn();
            if let Some(rest) = give(&mut inv, &db, p.stack.clone()) {
                // Full: swap with the held slot (or slot 1 when holding the khanda).
                let slot = if inv.selected > 0 { inv.selected - 1 } else { 0 };
                let old = inv.slots[slot].replace(rest);
                if let Some(old) = old {
                    spawn_pickup(&mut commands, pt.translation, old);
                }
                inv.selected = slot + 1;
            }
        }
    }
}

fn auto_pickup_arrows(
    mut commands: Commands,
    mut warriors: Query<(&Transform, &mut Inventory), (With<Warrior>, Without<Eliminated>, Without<Remote>)>,
    pickups: Query<(Entity, &Transform, &Pickup), Without<Warrior>>,
) {
    for (pe, pt, p) in &pickups {
        if p.stack.id != "arrows" || p.delay > 0.0 {
            continue;
        }
        for (wt, mut inv) in &mut warriors {
            if wt.translation.distance(pt.translation) < 1.6 {
                inv.arrows += p.stack.count;
                commands.entity(pe).despawn();
                break;
            }
        }
    }
}

fn tick_pickups(mut q: Query<&mut Pickup>, time: Res<Time>) {
    for mut p in &mut q {
        p.delay = (p.delay - time.delta_secs()).max(0.0);
    }
}
