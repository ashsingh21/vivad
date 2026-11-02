//! Chests (peti, kalash), floor loot and pickups.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;

use super::flow::Match;
use super::net::{NetId, NetMsg, Outbox, Remote};
use super::{Eliminated, Inventory, PlayerIntent, SimSet, Warrior, btn, match_rng, solid_mask, stream};
use crate::items::{ItemDb, ItemKind, ItemStack, Rarity};
use crate::map::{ChestKind, ChestSpawn, MapLayout};
use crate::map::compose::{CHEST_SETS, PLACES};
use crate::map::place::Spot;

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
    pub id: PickupId,
    pub stack: ItemStack,
    /// Can't be picked up until this reaches zero (avoids instantly re-grabbing a drop).
    pub delay: f32,
}

/// Names a pickup the same on every peer, from where it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PickupId(pub u64);

impl PickupId {
    pub fn floor(k: usize) -> Self {
        Self(1 << 56 | k as u64)
    }
    pub fn chest(index: usize, i: usize) -> Self {
        Self(2 << 56 | (index as u64) << 8 | i as u64)
    }
    /// The `i`th belonging a fallen warrior dropped.
    pub fn fallen(victim: NetId, i: usize) -> Self {
        Self(3 << 56 | (victim.0 as u64) << 8 | i as u64)
    }
    /// The `n`th item a warrior put down to make room (only its owner makes these).
    pub fn swapped(by: NetId, n: u32) -> Self {
        Self(4 << 56 | (by.0 as u64) << 32 | n as u64)
    }
}

/// Pickups this peer's warriors took recently. If another peer's warrior took the same one,
/// the earlier take wins (by match time, then warrior id) and the later taker gives it back.
#[derive(Resource, Default, Debug)]
pub struct Claims(pub Vec<Claim>);

/// Pickups another peer took before this one saw them spawn: one peer opens a chest, a second
/// standing beside it takes the goli the moment it appears, and the third may hear of the take
/// before the opening. Spawning one of these would leave a box here that no one can take.
#[derive(Resource, Default, Debug)]
pub struct Gone(pub std::collections::HashSet<PickupId>);

#[derive(Clone, Debug)]
pub struct Claim {
    pub pickup: PickupId,
    pub by: NetId,
    pub at: f32,
    pub stack: ItemStack,
}

/// Items put down by this peer's warriors so far (numbers `PickupId::swapped`).
#[derive(Resource, Default, Debug)]
struct Swaps(u32);

pub struct LootPlugin;

impl Plugin for LootPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Claims>()
            .init_resource::<Gone>()
            .init_resource::<Swaps>()
            // In a chain, so what `interact` took is gone before `auto_pickup_arrows` looks: the
            // same goli must not be taken, and counted, twice.
            .add_systems(FixedUpdate, ((prune_gone, interact, auto_pickup_arrows).chain(), tick_pickups).in_set(SimSet::Loot));
    }
}

/// Street weapons come on top of the guns, never in their place: sharing the guns' roll left a
/// third fewer on the NCR, and one bot in four with none a minute after landing. They lie on half
/// the floor spots that hold nothing else, and in this share of petis, beside the gun.
const MELEE_ON_FLOOR: f64 = 0.5;
const MELEE_IN_PETI: f64 = 0.4;

pub fn spawn_pickup(commands: &mut Commands, id: PickupId, at: Vec3, stack: ItemStack) {
    commands.spawn((Pickup { id, stack, delay: 0.6 }, Transform::from_translation(at)));
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

/// Pick an item of the wanted category at (or below) the rolled rarity. Weapons go by the spot they
/// are found at, if given (`ItemDb::drops`); medicine and goli are the same everywhere.
fn roll_item(db: &ItemDb, rarity: Rarity, filter: impl Fn(&ItemKind) -> bool, at: Option<Spot>, rng: &mut impl rand::Rng) -> Option<ItemStack> {
    for r in Rarity::ALL.iter().rev().filter(|r| **r <= rarity) {
        let pool: Vec<(&String, f32)> = db
            .items
            .iter()
            .filter(|(_, d)| filter(&d.kind))
            .filter_map(|(id, d)| {
                let w = match at {
                    Some(at) => db.drops.weight(at, id, d, *r),
                    None => d.drops.iter().find(|(dr, _)| dr == r).map(|(_, w)| *w),
                };
                w.map(|w| (id, w))
            })
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
        ItemKind::Melee { durability, .. } => (*durability).max(1),
    };
    ItemStack { id: id.into(), rarity, count }
}

fn is_weapon(k: &ItemKind) -> bool {
    matches!(k, ItemKind::Bow { .. } | ItemKind::Astra { .. })
}
fn is_melee(k: &ItemKind) -> bool {
    matches!(k, ItemKind::Melee { .. })
}
fn is_consumable(k: &ItemKind) -> bool {
    matches!(k, ItemKind::Consumable { .. })
}

/// Which of its sets of chests each district has out in this match: every peer draws the same, and
/// a player who knows the map still has to look.
pub fn chest_sets(seed: u64) -> [u8; PLACES.len()] {
    let mut rng = match_rng(seed, stream::CHEST_SETS);
    std::array::from_fn(|_| rng.random_range(0..CHEST_SETS))
}

/// Whether a chest is out in a match with these sets.
pub fn chest_is_out(c: &ChestSpawn, sets: &[u8]) -> bool {
    c.set.is_none_or(|(district, set)| sets[district as usize] == set)
}

/// Where a floor bow's quiver lies: beside it, on whichever side isn't in a holy site's circle. One
/// of the two isn't, a circle being round and the bow outside it; two circles can take both.
pub fn quiver_spot(layout: &MapLayout, bow: Vec3) -> Option<Vec3> {
    let beside = Vec3::new(0.7, 0.0, 0.3);
    [bow + beside, bow - beside].into_iter().find(|p| !layout.is_holy(p.xz()))
}

/// Where item `i` falls from a chest at `ct` (height aside): in front of it, or behind it or at its
/// feet if the front is in a holy site's circle.
pub fn spill_spot(layout: &MapLayout, ct: &Transform, i: usize) -> Vec3 {
    let (fwd, side) = (ct.rotation * Vec3::Z, ct.rotation * Vec3::X * (i as f32 - 1.0) * 0.7);
    let spots = [ct.translation + fwd + side, ct.translation - fwd + side];
    spots.into_iter().find(|p| !layout.is_holy(p.xz())).unwrap_or(ct.translation)
}

/// Fresh chests and floor loot for a new match (the same on every peer for the same seed). Only
/// inside `disc`, the circle the match opens on: nobody fights beyond it, and loot past it would
/// only be entities every bot looks over.
pub fn spawn_loot(commands: &mut Commands, layout: &MapLayout, db: &ItemDb, seed: u64, disc: (Vec2, f32)) {
    let mut rng = match_rng(seed, stream::FLOOR_LOOT);
    let mut street = match_rng(seed, stream::STREET_WEAPONS);
    let inside = |p: Vec3| p.xz().distance(disc.0) <= disc.1;
    let sets = chest_sets(seed);
    for (index, c) in layout.chests.iter().enumerate() {
        if !inside(c.pos) || !chest_is_out(c, &sets) {
            continue;
        }
        commands.spawn((
            Chest { index, kind: c.kind, opened: false },
            Transform::from_translation(c.pos).with_rotation(Quat::from_rotation_y(c.yaw)),
        ));
    }
    for (k, p) in layout.floor_loot.iter().enumerate() {
        if !inside(*p) {
            continue;
        }
        let here = Some(layout.place_at(*p));
        if !rng.random_bool(0.75) {
            if street.random_bool(MELEE_ON_FLOOR) && let Some(st) = roll_item(db, roll_rarity(None, &mut street), is_melee, here, &mut street) {
                commands.spawn((Pickup { id: PickupId::floor(k), stack: st, delay: 0.0 }, Transform::from_translation(*p)));
            }
            continue;
        }
        let rarity = roll_rarity(None, &mut rng);
        let item = if rng.random_bool(0.55) {
            roll_item(db, rarity, is_weapon, here, &mut rng)
        } else {
            roll_item(db, rarity, |k| is_consumable(k) || matches!(k, ItemKind::Arrows { .. }), None, &mut rng)
        };
        if let Some(st) = item {
            // Bows on the floor come with a quiver beside them.
            if matches!(db.get(&st.id).kind, ItemKind::Bow { .. }) && let Some(at) = quiver_spot(layout, *p) {
                let id = PickupId::floor(layout.floor_loot.len() + k);
                commands.spawn((Pickup { id, stack: make_stack(db, "arrows", Rarity::Common), delay: 0.0 }, Transform::from_translation(at)));
            }
            commands.spawn((Pickup { id: PickupId::floor(k), stack: st, delay: 0.0 }, Transform::from_translation(*p)));
        }
    }
}

/// What a chest holds: fixed by the match seed and where it stands, whoever opens it.
pub fn chest_contents(db: &ItemDb, layout: &MapLayout, seed: u64, index: usize, kind: ChestKind) -> Vec<ItemStack> {
    let at = layout.chests.get(index).map(|c| layout.place_at(c.pos));
    chest_at(db, at, seed, index, kind)
}

fn chest_at(db: &ItemDb, at: Option<Spot>, seed: u64, index: usize, kind: ChestKind) -> Vec<ItemStack> {
    let mut rng = match_rng(seed, stream::CHEST + index as u64);
    let mut out = vec![];
    let rarity = roll_rarity(Some(kind), &mut rng);
    out.extend(roll_item(db, rarity, is_weapon, at, &mut rng));
    out.extend(roll_item(db, roll_rarity(Some(kind), &mut rng), is_consumable, None, &mut rng));
    out.push(make_stack(db, "arrows", Rarity::Common));
    if kind == ChestKind::Kalash {
        out.extend(roll_item(db, Rarity::Legendary, is_consumable, None, &mut rng));
    } else if rng.random_bool(MELEE_IN_PETI) {
        out.extend(roll_item(db, roll_rarity(Some(kind), &mut rng), is_melee, at, &mut rng));
    }
    out
}

/// What lies out at the start of a match, by place: for each spot, how many weapons of each kind
/// (floor pickups and chests' first finds). `sim_check` and VIVAD_LOOT_LOG=1 in the game print it.
pub fn census(layout: &MapLayout, db: &ItemDb, seed: u64, floor: impl Iterator<Item = (Vec3, String)>, chests: impl Iterator<Item = (usize, ChestKind)>) -> Vec<(Spot, Vec<(String, u32)>)> {
    let mut by: std::collections::BTreeMap<Spot, std::collections::BTreeMap<String, u32>> = Default::default();
    let is_arm = |id: &str| matches!(db.get(id).kind, ItemKind::Bow { .. } | ItemKind::Astra { .. } | ItemKind::Melee { .. });
    for (p, id) in floor {
        if is_arm(&id) {
            *by.entry(layout.place_at(p)).or_default().entry(id).or_default() += 1;
        }
    }
    for (index, kind) in chests {
        let here = layout.place_at(layout.chests[index].pos);
        for s in chest_contents(db, layout, seed, index, kind).iter().filter(|s| is_arm(&s.id)) {
            *by.entry(here).or_default().entry(s.id.clone()).or_default() += 1;
        }
    }
    by.into_iter()
        .map(|(spot, items)| {
            let mut items: Vec<(String, u32)> = items.into_iter().collect();
            items.sort_by_key(|i| std::cmp::Reverse(i.1));
            (spot, items)
        })
        .collect()
}

/// `census` as lines of text.
pub fn census_lines(census: &[(Spot, Vec<(String, u32)>)]) -> Vec<String> {
    census
        .iter()
        .map(|(spot, items)| {
            let total: u32 = items.iter().map(|i| i.1).sum();
            let top: Vec<String> = items.iter().take(5).map(|(id, n)| format!("{id} {n}")).collect();
            format!("{:?}/{}: {total} ({})", spot.place, spot.era.map_or("-".to_string(), |e| format!("{e:?}")), top.join(", "))
        })
        .collect()
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

/// Opens a chest: its contents spill out in front of it (the same on every peer).
pub fn open_chest(commands: &mut Commands, spatial: &SpatialQuery, db: &ItemDb, layout: &MapLayout, seed: u64, ct: &Transform, c: &mut Chest, gone: &mut Gone) {
    c.opened = true;
    for (i, st) in chest_contents(db, layout, seed, c.index, c.kind).into_iter().enumerate() {
        let id = PickupId::chest(c.index, i);
        if gone.0.remove(&id) {
            continue;
        }
        let mut at = spill_spot(layout, ct, i) + Vec3::Y * 0.6;
        let filter = SpatialQueryFilter::from_mask(solid_mask());
        if let Some(hit) = spatial.cast_ray(at, Dir3::NEG_Y, 3.0, true, &filter) {
            at.y -= hit.distance - 0.25;
        }
        commands.spawn((Pickup { id, stack: st, delay: 0.3 }, Transform::from_translation(at)));
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn interact(
    mut commands: Commands,
    mut warriors: Query<(&Transform, &PlayerIntent, &mut Inventory, Option<&NetId>), (With<Warrior>, Without<Eliminated>, Without<Remote>)>,
    mut chests: Query<(&Transform, &mut Chest), Without<Warrior>>,
    pickups: Query<(Entity, &Transform, &Pickup), Without<Warrior>>,
    db: Res<ItemDb>,
    layout: Res<MapLayout>,
    m: Res<Match>,
    spatial: SpatialQuery,
    mut outbox: ResMut<Outbox>,
    mut claims: ResMut<Claims>,
    mut swaps: ResMut<Swaps>,
    mut gone: ResMut<Gone>,
) {
    claims.0.retain(|c| m.elapsed - c.at < 3.0);
    let mut taken = vec![];
    for (tf, intent, mut inv, net_id) in &mut warriors {
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
            open_chest(&mut commands, &spatial, &db, &layout, m.seed, ct, &mut c, &mut gone);
            outbox.0.push(NetMsg::ChestOpened { index: c.index });
            continue;
        }
        let nearest = pickups
            .iter()
            .filter(|(e, t, p)| p.delay <= 0.0 && !taken.contains(e) && t.translation.distance(me) < INTERACT_RANGE)
            .min_by(|a, b| a.1.translation.distance(me).total_cmp(&b.1.translation.distance(me)));
        if let Some((pe, pt, p)) = nearest {
            taken.push(pe);
            // The same tick's quiver can go to someone walking over it.
            commands.entity(pe).try_despawn();
            if let Some(&by) = net_id {
                outbox.0.push(NetMsg::Took { pickup: p.id, by, at: m.elapsed });
                claims.0.push(Claim { pickup: p.id, by, at: m.elapsed, stack: p.stack.clone() });
            }
            if let Some(rest) = give(&mut inv, &db, p.stack.clone()) {
                // Full: swap with the held slot (or slot 1 when holding the bat).
                let slot = if inv.selected > 0 { inv.selected - 1 } else { 0 };
                let old = inv.slots[slot].replace(rest);
                if let (Some(old), Some(&by)) = (old, net_id) {
                    swaps.0 += 1;
                    let id = PickupId::swapped(by, swaps.0);
                    outbox.0.push(NetMsg::Dropped { id, at: pt.translation, stack: old.clone() });
                    spawn_pickup(&mut commands, id, pt.translation, old);
                }
                inv.selected = slot + 1;
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn auto_pickup_arrows(
    mut commands: Commands,
    mut warriors: Query<(&Transform, &mut Inventory, Option<&NetId>), (With<Warrior>, Without<Eliminated>, Without<Remote>)>,
    pickups: Query<(Entity, &Transform, &Pickup), Without<Warrior>>,
    m: Res<Match>,
    mut outbox: ResMut<Outbox>,
    mut claims: ResMut<Claims>,
) {
    for (pe, pt, p) in &pickups {
        if p.stack.id != "arrows" || p.delay > 0.0 {
            continue;
        }
        for (wt, mut inv, net_id) in &mut warriors {
            if wt.translation.distance(pt.translation) < 1.6 {
                inv.arrows += p.stack.count;
                commands.entity(pe).try_despawn();
                if let Some(&by) = net_id {
                    outbox.0.push(NetMsg::Took { pickup: p.id, by, at: m.elapsed });
                    claims.0.push(Claim { pickup: p.id, by, at: m.elapsed, stack: p.stack.clone() });
                }
                break;
            }
        }
    }
}

/// Gives back a pickup another peer's warrior took first.
pub fn revoke(inv: &mut Inventory, stack: &ItemStack) {
    if stack.id == "arrows" {
        inv.arrows = inv.arrows.saturating_sub(stack.count);
        return;
    }
    if let Some(slot) = inv.slots.iter_mut().find(|s| s.as_ref().is_some_and(|s| s.id == stack.id && s.rarity == stack.rarity)) {
        let s = slot.as_mut().unwrap();
        if s.count > stack.count {
            s.count -= stack.count;
        } else {
            *slot = None;
        }
    }
}

/// A pickup that another peer took before its spawn reached us, however the two messages were
/// ordered (the spawn may have been queued in the same batch as the take): gone.
fn prune_gone(mut commands: Commands, q: Query<(Entity, &Pickup)>, mut gone: ResMut<Gone>) {
    if gone.0.is_empty() {
        return;
    }
    for (e, p) in &q {
        if gone.0.remove(&p.id) {
            commands.entity(e).despawn();
        }
    }
}

fn tick_pickups(mut q: Query<&mut Pickup>, time: Res<Time>) {
    for mut p in &mut q {
        p.delay = (p.delay - time.delta_secs()).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::place::{Era, Place};
    use crate::map::{MapId, generate};

    /// Nothing a chest or a floor bow drops lands in a holy site's circle, on any map.
    #[test]
    fn no_drop_lands_at_a_holy_site() {
        for (map, seeds) in [(MapId::Ncr, &[7, 90001][..]), (MapId::Cp, &[7][..]), (MapId::Chandni, &[7, 23][..]), (MapId::HauzKhas, &[7][..]), (MapId::IndiaGate, &[7][..])] {
            for &seed in seeds {
                let m = generate(map, seed);
                for c in &m.chests {
                    let ct = Transform::from_translation(c.pos).with_rotation(Quat::from_rotation_y(c.yaw));
                    for i in 0..4 {
                        let at = spill_spot(&m, &ct, i);
                        assert!(!m.is_holy(at.xz()), "{map:?}, seed {seed}: chest at {} spills to {at}", c.pos);
                    }
                }
                for p in &m.floor_loot {
                    assert!(quiver_spot(&m, *p).is_none_or(|q| !m.is_holy(q.xz())), "{map:?}, seed {seed}: quiver by {p}");
                }
            }
        }
    }

    /// A peti's first find is a gun or an astra, as ever: a street weapon only ever joins it.
    #[test]
    fn street_weapons_leave_the_guns_alone() {
        let db = ItemDb::load_from_str(include_str!("../../../../assets/data/items.ron")).unwrap();
        let melee = |s: &ItemStack| matches!(db.get(&s.id).kind, ItemKind::Melee { .. });
        let mut extra = 0;
        for index in 0..500 {
            let items = chest_at(&db, None, 7, index, ChestKind::Peti);
            assert!(!melee(&items[0]), "{items:?}");
            assert!(items.len() <= 4, "{items:?}");
            extra += items.iter().filter(|s| melee(s)).count();
        }
        assert!((100..300).contains(&extra), "{extra} street weapons in 500 petis");
    }

    fn db() -> ItemDb {
        ItemDb::load_from_str(include_str!("../../../../assets/data/items.ron")).unwrap()
    }

    /// `n` rolls of a kind of find at a spot, most found first.
    fn rolls(db: &ItemDb, at: Spot, filter: fn(&ItemKind) -> bool, n: u32) -> Vec<(String, u32)> {
        let mut rng = match_rng(7, stream::FLOOR_LOOT);
        let mut seen: std::collections::BTreeMap<String, u32> = Default::default();
        for _ in 0..n {
            let rarity = roll_rarity(None, &mut rng);
            let st = roll_item(db, rarity, filter, Some(at), &mut rng).unwrap();
            *seen.entry(st.id).or_default() += 1;
        }
        let mut seen: Vec<_> = seen.into_iter().collect();
        seen.sort_by_key(|s| std::cmp::Reverse(s.1));
        seen
    }

    /// Each place's main gun leads its finds, out of 1,000 floor rolls.
    #[test]
    fn the_main_gun_leads_each_place() {
        let db = db();
        for (place, main) in [(Place::Room, "dunali"), (Place::Lane, "vajra9"), (Place::Road, "saintalis"), (Place::Roof, "thaana_slr"), (Place::Open, "thri_not_thri")] {
            let seen = rolls(&db, Spot { place, era: None }, is_weapon, 1000);
            assert_eq!(seen[0].0, main, "{place:?}: {seen:?}");
            assert!(seen[0].1 > seen[1].1 * 3 / 2, "{place:?} is led by a whisker: {seen:?}");
        }
    }

    /// The old city raises the old twin and the new city the modern gun: a Chandni Chowk lane, a
    /// Cyber City road and the Ridge hand out different guns.
    #[test]
    fn eras_raise_their_own_guns() {
        let db = db();
        let lead = |place, era| rolls(&db, Spot { place, era }, is_weapon, 1000)[0].0.clone();
        assert_eq!(lead(Place::Lane, Some(Era::Old)), "dunali");
        assert_eq!(lead(Place::Lane, Some(Era::New)), "vajra9");
        assert_eq!(lead(Place::Road, Some(Era::Old)), "thaana_slr");
        assert_eq!(lead(Place::Road, Some(Era::New)), "saintalis");
        assert_eq!(lead(Place::Road, Some(Era::Raj)), "saintalis");
        assert_eq!(lead(Place::Open, Some(Era::Raj)), "thri_not_thri");
        let street = |place, era| rolls(&db, Spot { place, era }, is_melee, 1000)[0].0.clone();
        assert_eq!(street(Place::Lane, Some(Era::Old)), "talwar");
        assert_eq!(street(Place::Lane, Some(Era::New)), "hockey_stick");
        assert_eq!(street(Place::Room, None), "tawa");
    }

    /// Nothing falls below a quarter of its weight, anywhere: a Talwar still turns up in Cyber City.
    #[test]
    fn nothing_is_ruled_out_anywhere() {
        let db = db();
        for place in Place::ALL {
            for era in [None, Some(Era::Old), Some(Era::Raj), Some(Era::New)] {
                let at = Spot { place, era };
                for (id, d) in db.items.iter().filter(|(_, d)| !d.drops.is_empty() && (is_weapon(&d.kind) || is_melee(&d.kind))) {
                    assert!(db.drops.odds(at, id) >= 0.25, "{id} at {at:?}");
                    let rolled: f32 = Rarity::ALL.iter().filter_map(|r| db.drops.weight(at, id, d, *r)).sum();
                    assert!(rolled > 0.0, "{id} at {at:?}");
                }
            }
        }
        let cyber = Spot { place: Place::Road, era: Some(Era::New) };
        assert!(rolls(&db, cyber, is_melee, 4000).iter().any(|(id, n)| id == "talwar" && *n >= 4));
    }

    /// The rarest guns stay the rarest: a grey roll can't turn up a Toradar, wherever it is.
    #[test]
    fn divya_items_stay_divya() {
        let db = db();
        let at = Spot { place: Place::Open, era: Some(Era::Old) };
        for tier in [Rarity::Common, Rarity::Rare, Rarity::Epic, Rarity::Legendary] {
            assert!(db.drops.weight(at, "toradar", db.get("toradar"), tier).is_none());
        }
    }

    /// Peers agree: a chest's contents come from the seed and the map alone.
    #[test]
    fn a_chest_holds_the_same_on_every_peer() {
        let (db, map) = (db(), generate(MapId::Cp, 7));
        for index in 0..map.chests.len() {
            let c = &map.chests[index];
            assert_eq!(chest_contents(&db, &map, 5, index, c.kind), chest_contents(&db, &generate(MapId::Cp, 7), 5, index, c.kind));
        }
    }

    /// A match opens one set of chests in each district, the same on every peer, and the sets move
    /// from one match to the next.
    #[test]
    fn chests_move_between_matches() {
        assert_eq!(chest_sets(5), chest_sets(5));
        let picks: Vec<_> = (0..6).map(chest_sets).collect();
        assert!(picks.windows(2).all(|p| p[0] != p[1]), "{picks:?}");
        let m = generate(MapId::Ncr, 7);
        let out = |seed: u64| {
            let sets = chest_sets(seed);
            m.chests.iter().filter(|c| chest_is_out(c, &sets)).map(|c| c.pos).collect::<Vec<_>>()
        };
        let (a, b) = (out(1), out(2));
        assert!((130..=160).contains(&a.len()), "{}", a.len());
        let same = a.iter().filter(|p| b.contains(p)).count();
        assert!(same < a.len() * 3 / 4, "{same} of {} stayed put", a.len());
    }
}
