//! A Delhi market that feels alive, without a crowd that would break a battle royale. All of it is
//! the client's own: no sim code, no network message, no collider, nothing that bends the street
//! grid or the map's seed, nothing that can stop a bullet or be taken for a warrior.
//!
//! Four layers make "busy": goods on the stalls and shop fronts (`goods`), harmless people and
//! street animals near the player only (`people`), and two hooks for sound: `busy_at` says how
//! busy a spot is and `Scattered` fires when people run. VIVAD_NO_LIFE=1 switches all of it off.
//!
//! The market is alive when you land; the fight is what empties it.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::map::MapLayout;
use shared::map::terrain::ground;
use shared::sim::nav::StreetFloor;

pub mod flipbook;
mod goods;
mod people;
mod stage;

/// People ran for it: when the market's sound should drop away. Sent even if no one listens.
#[derive(Message, Clone, Copy, Debug)]
pub struct Scattered {
    /// What set them off.
    pub at: Vec3,
    /// How many ran.
    pub people: u32,
}

pub struct LifePlugin;

impl Plugin for LifePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Scattered>().init_resource::<Market>();
        if std::env::var("VIVAD_NO_LIFE").is_ok() {
            return;
        }
        app.add_plugins((people::PeoplePlugin, goods::GoodsPlugin, stage::StagePlugin)).add_systems(Startup, index_market).add_systems(Update, index_market.run_if(shared::sim::map_reloaded));
    }
}

/// Metres a side of the squares the market is indexed by.
const SQUARE: f32 = 32.0;
/// How busy a place is at most, by name. Anywhere else keeps to `QUIET`.
const PLACES: [(&str, f32); 26] = [
    ("Chandni Chowk", 1.0),
    ("Karol Bagh", 1.0),
    ("Lajpat Nagar", 1.0),
    ("Connaught Place", 0.9),
    ("Kamla Nagar", 0.95),
    ("Laxmi Nagar", 0.85),
    ("Shahdara", 0.8),
    ("Raj Nagar", 0.7),
    ("Lal Kuan", 0.7),
    ("Mayur Vihar", 0.7),
    ("Surajkund Mela Ground", 0.8),
    ("Sector 18", 0.75),
    ("Nehru Place", 0.7),
    ("Anand Vihar", 0.7),
    ("Ghaziabad", 0.65),
    ("Hauz Khas", 0.55),
    ("Azadpur Mandi", 0.6),
    ("New Delhi Station", 0.6),
    ("Kashmere Gate", 0.55),
    ("Sarai Kale Khan", 0.45),
    ("MG Road", 0.4),
    ("Saket", 0.4),
    ("Indirapuram", 0.3),
    ("Cyber City", 0.25),
    ("Rajouri Garden", 0.4),
    ("City Centre", 0.4),
];
const QUIET: f32 = 0.06;
/// Stalls in a square at which it is as full as its place gets.
const FULL_SQUARE: f32 = 2.0;

/// What a kind of stall is, for who stands at it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StallKind {
    Thela,
    Chai,
    Crates,
    Chairs,
    /// A spread on the pavement: goods only, worked out here, and no kit under it.
    Patri,
}

/// A stall from the street kit.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stall {
    pub kind: StallKind,
    pub pos: Vec3,
    pub yaw: f32,
}

/// The map read for life: where the stalls are and how busy each square is. Built from the map's
/// own kit, ground classes and place names, so it works in every district.
#[derive(Resource, Default)]
pub struct Market {
    half: f32,
    side: usize,
    busy: Vec<f32>,
    pub(crate) stalls: Vec<Stall>,
    near: HashMap<(i32, i32), Vec<u32>>,
    /// The electricity poles (kind means nothing).
    pub(crate) poles: Vec<Stall>,
    /// Footprints by the squares they touch.
    buildings: HashMap<(i32, i32), Vec<u32>>,
}

impl Market {
    fn square(&self, p: Vec2) -> Option<usize> {
        let (i, j) = (((p.x + self.half) / SQUARE).floor(), ((p.y + self.half) / SQUARE).floor());
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.side && (j as usize) < self.side).then(|| j as usize * self.side + i as usize)
    }

    /// The stalls round `p`, by id into `stalls`.
    pub(crate) fn stalls_near(&self, p: Vec2, reach: f32) -> impl Iterator<Item = u32> + '_ {
        let (x0, x1) = (((p.x - reach) / SQUARE).floor() as i32, ((p.x + reach) / SQUARE).floor() as i32);
        let (z0, z1) = (((p.y - reach) / SQUARE).floor() as i32, ((p.y + reach) / SQUARE).floor() as i32);
        (z0..=z1).flat_map(move |j| (x0..=x1).map(move |i| (i, j))).filter_map(|k| self.near.get(&k)).flatten().copied()
    }

    /// The buildings round `p`, by index into the map's footprints, each once.
    pub(crate) fn buildings_near(&self, p: Vec2, reach: f32) -> Vec<u32> {
        let (x0, x1) = (((p.x - reach) / SQUARE).floor() as i32, ((p.x + reach) / SQUARE).floor() as i32);
        let (z0, z1) = (((p.y - reach) / SQUARE).floor() as i32, ((p.y + reach) / SQUARE).floor() as i32);
        let mut out: Vec<u32> = (z0..=z1).flat_map(|j| (x0..=x1).map(move |i| (i, j))).filter_map(|k| self.buildings.get(&k)).flatten().copied().collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether a building stands at `p`.
    pub(crate) fn built(&self, layout: &MapLayout, p: Vec2) -> bool {
        let key = ((p.x / SQUARE).floor() as i32, (p.y / SQUARE).floor() as i32);
        self.buildings.get(&key).is_some_and(|ids| ids.iter().any(|i| shared::map::footprint::contains(&layout.footprints[*i as usize].pts, p)))
    }

    fn build(layout: &MapLayout, floor: &StreetFloor) -> Self {
        let half = layout.half_extent;
        let side = (2.0 * half / SQUARE).ceil() as usize;
        let mut market = Market { half, side, busy: vec![0.0; side * side], ..default() };
        for (i, f) in layout.footprints.iter().enumerate() {
            let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
            for j in (lo.y / SQUARE).floor() as i32..=(hi.y / SQUARE).floor() as i32 {
                for k in (lo.x / SQUARE).floor() as i32..=(hi.x / SQUARE).floor() as i32 {
                    market.buildings.entry((k, j)).or_default().push(i as u32);
                }
            }
        }
        for k in &layout.kit {
            if k.name == "pole" && !layout.is_holy(k.pos.xz()) {
                market.poles.push(Stall { kind: StallKind::Crates, pos: k.pos, yaw: k.yaw });
            }
            let kind = match k.name {
                "thela" => StallKind::Thela,
                "chai" => StallKind::Chai,
                "crates" => StallKind::Crates,
                "chairs" => StallKind::Chairs,
                _ => continue,
            };
            let at = k.pos.xz();
            // On the street, not on a roof or a bridge deck; never in a holy site.
            if k.pos.y - layout.terrain.height(at.x, at.y) > 0.5 || layout.is_holy(at) {
                continue;
            }
            let id = market.stalls.len() as u32;
            market.stalls.push(Stall { kind, pos: k.pos, yaw: k.yaw });
            market.near.entry(((at.x / SQUARE).floor() as i32, (at.y / SQUARE).floor() as i32)).or_default().push(id);
        }
        let mut stalls = vec![0.0f32; side * side];
        for s in &market.stalls {
            if let Some(k) = market.square(s.pos.xz()) {
                stalls[k] += 1.0;
            }
        }
        let places: Vec<(Vec2, f32, f32)> = layout.pois.iter().filter_map(|p| Some((p.center.xz(), p.radius, PLACES.iter().find(|(n, _)| *n == p.name)?.1))).collect();
        for j in 0..side {
            for i in 0..side {
                let c = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * SQUARE - Vec2::splat(half);
                let place = places
                    .iter()
                    .map(|(at, r, weight)| {
                        let d = (c - *at).abs().max_element() / r.max(1.0);
                        weight * (1.0 - ((d - 0.7) / 0.5).clamp(0.0, 1.0))
                    })
                    .fold(QUIET, f32::max);
                // A stall nearby counts towards this square too.
                let mut round = 0.0;
                for dj in -1i32..=1 {
                    for di in -1i32..=1 {
                        let (x, z) = (i as i32 + di, j as i32 + dj);
                        if x >= 0 && z >= 0 && (x as usize) < side && (z as usize) < side {
                            round += stalls[z as usize * side + x as usize] / if di == 0 && dj == 0 { 1.0 } else { 3.0 };
                        }
                    }
                }
                let street = street_share(layout, c);
                market.busy[j * side + i] = place * (0.6 + 0.4 * (round / FULL_SQUARE).min(1.0)) * street;
            }
        }
        market.lay_spreads(layout, floor);
        market
    }

    /// Pavement sellers: where the kerb and the shop fronts leave a bit of paved ground in a busy
    /// place, a sheet on the ground facing the road. Flat enough to walk over, so they stand in
    /// the open where a solid stall could not.
    fn lay_spreads(&mut self, layout: &MapLayout, floor: &StreetFloor) {
        const STEP: f32 = 6.0;
        let t = &layout.terrain;
        let cells = (2.0 * self.half / STEP) as i32;
        for j in 0..cells {
            for i in 0..cells {
                let p0 = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * STEP - Vec2::splat(self.half);
                let busy = busy_at(self, p0);
                if busy < 0.15 {
                    continue;
                }
                let (a, b) = (i as u32, j as u32);
                let p = p0 + Vec2::new(goods::hash(a, b, 21) - 0.5, goods::hash(a, b, 22) - 0.5) * 3.0;
                if goods::hash(a, b, 23) > busy * 0.8 || !matches!(t.ground_at(p.x, p.y), Some(ground::PAVEMENT | ground::PLAZA | ground::PATH)) {
                    continue;
                }
                // Nothing under it or quite against it.
                let room = [Vec2::ZERO, Vec2::new(1.4, 0.0), Vec2::new(-1.4, 0.0), Vec2::new(0.0, 1.4), Vec2::new(0.0, -1.4)];
                if !room.iter().all(|d| floor.walkable(p + *d)) || layout.is_holy(p) {
                    continue;
                }
                // Facing the nearest road, within a few steps of it.
                let mut facing = None;
                'looking: for reach in [3.0, 5.0, 7.0, 9.0] {
                    for k in 0..12 {
                        let dir = Vec2::from_angle(k as f32 / 12.0 * std::f32::consts::TAU);
                        let q = p + dir * reach;
                        if t.ground_at(q.x, q.y) == Some(ground::ROAD) {
                            facing = Some(dir);
                            break 'looking;
                        }
                    }
                }
                let Some(dir) = facing else { continue };
                let id = self.stalls.len() as u32;
                self.stalls.push(Stall { kind: StallKind::Patri, pos: Vec3::new(p.x, t.height(p.x, p.y), p.y), yaw: (-dir.x).atan2(-dir.y) });
                self.near.entry(((p.x / SQUARE).floor() as i32, (p.y / SQUARE).floor() as i32)).or_default().push(id);
            }
        }
    }
}

/// How much of a square round `c` is ground people walk on.
fn street_share(layout: &MapLayout, c: Vec2) -> f32 {
    let t = &layout.terrain;
    let mut on = 0;
    for j in 0..4 {
        for i in 0..4 {
            let p = c + (Vec2::new(i as f32, j as f32) - 1.5) * SQUARE / 4.0;
            if matches!(t.ground_at(p.x, p.y), Some(ground::ROAD | ground::PAVEMENT | ground::PATH | ground::PLAZA | ground::PARKING)) {
                on += 1;
            }
        }
    }
    (on as f32 / 8.0).min(1.0)
}

fn index_market(mut market: ResMut<Market>, layout: Res<MapLayout>, floor: Res<StreetFloor>) {
    *market = Market::build(&layout, &floor);
    info!("market: {} stalls", market.stalls.len());
    if std::env::var("VIVAD_LIFE_LOG").is_ok() {
        // The busiest spot in each place, to go and look at.
        for p in &layout.pois {
            let (mut best, mut at) = (0.0, Vec2::ZERO);
            for s in &market.stalls {
                let q = s.pos.xz();
                if (q - p.center.xz()).abs().max_element() < p.radius {
                    let around = market.stalls_near(q, 24.0).count() as f32;
                    if around > best {
                        (best, at) = (around, q);
                    }
                }
            }
            info!("  {}: {best} stalls round {at:.0}, busy {:.2}", p.name, busy_at(&market, at));
        }
    }
}

/// How busy a spot is, 0 (nobody about) to 1 (the thick of Chandni Chowk). The hook for street
/// sound: it reads the map, not who happens to be there, so a market a fight has emptied still
/// says what it is.
pub fn busy_at(market: &Market, p: Vec2) -> f32 {
    market.square(p).map_or(0.0, |k| market.busy[k])
}

/// A small steady random stream, for what no peer has to agree on. Nothing here may touch the
/// match's own random numbers.
#[derive(Clone, Copy)]
pub(crate) struct Dice(pub u32);

impl Dice {
    pub fn next(&mut self) -> f32 {
        let mut s = self.0;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.0 = s;
        (s % 100_000) as f32 / 100_000.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

#[cfg(test)]
mod tests {
    use shared::map::{MapId, generate};

    use super::*;

    /// The real NCR map, once: the named markets read busy, the empty land doesn't, no stall stands
    /// in a holy circle, and the hook answers for anywhere.
    #[test]
    fn the_market_comes_from_the_map() {
        let layout = generate(MapId::Ncr, 7);
        let market = Market::build(&layout, &StreetFloor::new(&layout));
        assert!(market.stalls.len() > 1000, "{}", market.stalls.len());
        assert!(market.stalls.iter().all(|s| !layout.is_holy(s.pos.xz())));
        let around = |name: &str| {
            let p = layout.pois.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("{name}"));
            let c = p.center.xz();
            let mut best = 0.0f32;
            for i in -6..=6 {
                for j in -6..=6 {
                    best = best.max(busy_at(&market, c + Vec2::new(i as f32, j as f32) * 24.0));
                }
            }
            best
        };
        for name in ["Chandni Chowk", "Karol Bagh", "Lajpat Nagar", "Connaught Place"] {
            let busy = around(name);
            assert!(busy > 0.7, "{name} only {busy}");
        }
        assert!(around("Expressway") < 0.2);
        assert_eq!(busy_at(&market, Vec2::splat(1.0e5)), 0.0);
        assert!(busy_at(&market, Vec2::new(30.0, -300.0)) < 0.1, "the river bank");
    }

    #[test]
    fn the_dice_stay_in_range_and_keep_rolling() {
        let mut dice = Dice(1);
        let rolls: Vec<f32> = (0..1000).map(|_| dice.range(2.0, 3.0)).collect();
        assert!(rolls.iter().all(|r| (2.0..=3.0).contains(r)));
        assert!(rolls.windows(2).any(|w| w[0] != w[1]));
    }
}
