//! A Delhi market that feels alive, without a crowd that would break a battle royale. Where the
//! rows of tables stand is the map's (`shared::map::stalls`), and a table is as solid as a wall.
//! All the rest is the client's own: no sim code, no network message, no collider, nothing that
//! bends the street grid or the map's seed, nothing that can stop a bullet or be taken for a warrior.
//!
//! Four layers make "busy": goods on the stalls and shop fronts (`goods`), harmless people and
//! street animals near the player only (`people`), and two hooks for sound: `busy_at` says how
//! busy a spot is and `Scattered` fires when people run. VIVAD_NO_LIFE=1 switches all of it off.
//!
//! The market is alive when you land; the fight is what empties it.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use shared::map::MapLayout;
use shared::map::stalls::{self, Busy, RowKind};
use shared::map::terrain::ground;
use shared::sim::nav::StreetFloor;

mod ab;
pub mod flipbook;
mod goods;
mod paint;
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
        app.add_message::<Scattered>().init_resource::<Market>().add_systems(Update, spent::report);
        if std::env::var("VIVAD_NO_LIFE").is_ok() {
            return;
        }
        // VIVAD_LIFE_ONLY=people or goods, to price one layer alone.
        let only = std::env::var("VIVAD_LIFE_ONLY").unwrap_or_default();
        if only != "goods" {
            app.add_plugins(people::PeoplePlugin);
        }
        if only != "people" {
            app.add_plugins(goods::GoodsPlugin);
        }
        app.add_plugins((stage::StagePlugin, paint::PaintPlugin)).add_systems(Startup, index_market).add_systems(Update, index_market.run_if(shared::sim::map_reloaded));
        if std::env::var("VIVAD_LIFE_AB").is_ok() {
            app.add_systems(Update, ab::step);
        }
    }
}

/// Metres a side of the squares the market is indexed by.
const SQUARE: f32 = 32.0;
/// What a kind of stall is, for who stands at it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StallKind {
    Thela,
    Chai,
    Crates,
    Chairs,
    /// A spread on the pavement: goods only, worked out here, and no kit under it.
    Patri,
    /// A trestle stall in a row along a kerb: solid, from the map.
    Row,
    /// A handcart at the kerb: dressing, no collider stands behind it.
    Cart,
    /// Scooters parked in a bunch at the road's edge, the same.
    Scooters,
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
    busy: Busy,
    pub(crate) stalls: Vec<Stall>,
    near: HashMap<(i32, i32), Vec<u32>>,
    /// The electricity poles (kind means nothing).
    pub(crate) poles: Vec<Stall>,
    /// Footprints by the squares they touch.
    buildings: HashMap<(i32, i32), Vec<u32>>,
    /// The metre squares what stands in a row stands on: nobody here walks through a handcart, and
    /// a table's are the floor's too.
    solid: HashSet<(i32, i32)>,
    /// The metre squares in front of a building's doors, as the map keeps them clear.
    lanes: HashSet<(i32, i32)>,
    /// Banners over the road, kerb to kerb.
    pub(crate) banners: Vec<(Vec3, Vec3)>,
    /// Whether this map has the street grid all of this leans on: the NCR's.
    pub(crate) ready: bool,
    /// Where loot lies, by 3 m squares, only while laying out: nothing may stand over it.
    loot: HashSet<(i32, i32)>,
    /// Half the map's side.
    half: f32,
}

impl Market {
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

    /// Whether a stall of the rows stands at `p`.
    pub(crate) fn stall_at(&self, p: Vec2) -> bool {
        self.solid.contains(&(p.x.floor() as i32, p.y.floor() as i32))
    }

    /// Whether a building stands at `p`.
    pub(crate) fn built(&self, layout: &MapLayout, p: Vec2) -> bool {
        let key = ((p.x / SQUARE).floor() as i32, (p.y / SQUARE).floor() as i32);
        self.buildings.get(&key).is_some_and(|ids| ids.iter().any(|i| shared::map::footprint::contains(&layout.footprints[*i as usize].pts, p)))
    }

    fn build(layout: &MapLayout, floor: &StreetFloor) -> Self {
        let mut market = Market { busy: Busy::of(layout), ready: floor.ready(), lanes: layout.stalls.lanes.clone(), banners: layout.stalls.banners.clone(), half: layout.half_extent, ..default() };
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
            if !stalls::street_stall(layout, k) {
                continue;
            }
            let kind = match k.name {
                "thela" => StallKind::Thela,
                "chai" => StallKind::Chai,
                "crates" => StallKind::Crates,
                _ => StallKind::Chairs,
            };
            let at = k.pos.xz();
            let id = market.stalls.len() as u32;
            market.stalls.push(Stall { kind, pos: k.pos, yaw: k.yaw });
            market.near.entry(((at.x / SQUARE).floor() as i32, (at.y / SQUARE).floor() as i32)).or_default().push(id);
        }
        for s in &layout.stalls.stalls {
            let kind = match s.kind {
                RowKind::Table => StallKind::Row,
                RowKind::Cart => StallKind::Cart,
                RowKind::Scooters => StallKind::Scooters,
            };
            market.add_solid(kind, s.pos, s.yaw, s.half());
        }
        if market.ready {
            let spots = layout.floor_loot.iter().copied().chain(layout.chests.iter().map(|c| c.pos));
            market.loot = spots.map(|p| ((p.x / 3.0).floor() as i32, (p.z / 3.0).floor() as i32)).collect();
            market.lay_spreads(layout, floor);
            market.loot = HashSet::new();
        }
        market
    }

    /// Whether `p` is on the way out of a door.
    pub(crate) fn on_lane(&self, p: Vec2) -> bool {
        self.lanes.contains(&(p.x.floor() as i32, p.y.floor() as i32))
    }

    /// Whether loot lies round `p`: a stall over it would hide it, and no one could tell.
    fn over_loot(&self, p: Vec2) -> bool {
        let (i, j) = ((p.x / 3.0).floor() as i32, (p.y / 3.0).floor() as i32);
        (-1..=1).any(|a| (-1..=1).any(|b| self.loot.contains(&(i + a, j + b))))
    }

    /// Adds a stall with its footprint: half width, half depth about its middle.
    fn add_solid(&mut self, kind: StallKind, pos: Vec3, yaw: f32, half: Vec2) {
        let id = self.stalls.len() as u32;
        self.stalls.push(Stall { kind, pos, yaw });
        let at = pos.xz();
        self.near.entry(((at.x / SQUARE).floor() as i32, (at.y / SQUARE).floor() as i32)).or_default().push(id);
        self.solid.insert((at.x.floor() as i32, at.y.floor() as i32));
        let turn = Vec2::from_angle(-yaw);
        let reach = half.length().ceil() as i32 + 1;
        for j in -reach..=reach {
            for i in -reach..=reach {
                let cell = (at.x.floor() as i32 + i, at.y.floor() as i32 + j);
                let d = Vec2::new(cell.0 as f32 + 0.5, cell.1 as f32 + 0.5) - at;
                let q = turn.rotate(d);
                if q.x.abs() <= half.x + 0.1 && q.y.abs() <= half.y + 0.1 {
                    self.solid.insert(cell);
                }
            }
        }
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
                let p = p0 + Vec2::new(stalls::hash(a, b, 21) - 0.5, stalls::hash(a, b, 22) - 0.5) * 3.0;
                if stalls::hash(a, b, 23) > busy * 0.8 || !matches!(t.ground_at(p.x, p.y), Some(ground::PAVEMENT | ground::PLAZA | ground::PATH)) {
                    continue;
                }
                // Nothing under it or quite against it.
                let room = [Vec2::ZERO, Vec2::new(1.4, 0.0), Vec2::new(-1.4, 0.0), Vec2::new(0.0, 1.4), Vec2::new(0.0, -1.4)];
                if !room.iter().all(|d| floor.walkable(p + *d) && !self.stall_at(p + *d) && !self.on_lane(p + *d)) || layout.is_holy(p) || self.over_loot(p) {
                    continue;
                }
                // Facing the nearest road, within a few steps of it.
                let mut facing = None;
                'looking: for reach in [3.0, 5.0, 7.0, 9.0, 12.0, 15.0, 18.0] {
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

fn index_market(mut market: ResMut<Market>, layout: Res<MapLayout>, floor: Res<StreetFloor>) {
    let start = bevy::platform::time::Instant::now();
    *market = Market::build(&layout, &floor);
    info!("market: {} stalls in {} ms", market.stalls.len(), start.elapsed().as_millis());
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
    market.busy.at(p)
}

/// Time spent here, for VIVAD_LIFE_LOG=1: the frame budget is about a millisecond in a busy market.
pub(crate) mod spent {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use bevy::platform::time::Instant;
    use bevy::prelude::*;

    pub static PEOPLE: AtomicU64 = AtomicU64::new(0);
    pub static GOODS: AtomicU64 = AtomicU64::new(0);
    /// Those who stepped out of a vehicle's way, those who had to leap, and those a vehicle caught.
    pub static ASIDE: AtomicU64 = AtomicU64::new(0);
    pub static LEAPT: AtomicU64 = AtomicU64::new(0);
    pub static CAUGHT: AtomicU64 = AtomicU64::new(0);

    fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var("VIVAD_LIFE_LOG").is_ok())
    }

    /// Adds the time until it is dropped to `into`, if anyone is counting.
    pub struct Timed(Option<(Instant, &'static AtomicU64)>);

    impl Timed {
        pub fn start(into: &'static AtomicU64) -> Self {
            Self(on().then(|| (Instant::now(), into)))
        }
    }

    impl Drop for Timed {
        fn drop(&mut self) {
            if let Some((t, into)) = self.0 {
                into.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
            }
        }
    }

    /// Every 600 frames: the time spent here, and the frame's median and slow end.
    pub fn report(time: Res<Time<Real>>, mut frames: Local<Vec<f32>>) {
        if !on() {
            return;
        }
        frames.push(time.delta_secs() * 1000.0);
        if frames.len() >= 600 {
            let n = frames.len() as f32;
            let per = |c: &AtomicU64| c.swap(0, Ordering::Relaxed) as f32 / n;
            frames.sort_by(f32::total_cmp);
            info!("life spent: people {:.0} us, goods {:.0} us a frame; frame median {:.2} ms, 1% slowest from {:.2} ms", per(&PEOPLE), per(&GOODS), frames[frames.len() / 2], frames[frames.len() * 99 / 100]);
            info!("life traffic so far: {} stepped aside, {} leapt, {} caught", ASIDE.load(Ordering::Relaxed), LEAPT.load(Ordering::Relaxed), CAUGHT.load(Ordering::Relaxed));
            frames.clear();
        }
    }
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
        assert!(market.banners.iter().all(|(a, b)| !layout.is_holy(a.xz()) && !layout.is_holy(b.xz())));
        // The rows line the roads, on the roads' edges: a street is left between them, and every
        // row has its ways through.
        let on_road = |p: Vec2| layout.terrain.ground_at(p.x, p.y) == Some(ground::ROAD);
        let rows: Vec<&Stall> = market.stalls.iter().filter(|s| s.kind == StallKind::Row).collect();
        assert!(rows.len() > 5000, "{}", rows.len());
        // A row on the road stands at its edge: the kerb is a few steps off, whichever way it runs.
        for s in rows.iter().filter(|s| on_road(s.pos.xz())) {
            let kerb = (0..16).any(|k| !on_road(s.pos.xz() + Vec2::from_angle(k as f32 / 16.0 * std::f32::consts::TAU) * 5.0));
            assert!(kerb, "a stall in the middle of the road at {}", s.pos);
        }
        // Every door has its way out.
        for door in layout.rooms.iter().flat_map(|r| &r.doors) {
            let foot = door.pos.xz() + door.out * 1.2;
            assert!(market.on_lane(foot), "no lane at the door at {foot}");
            assert!(!market.stalls.iter().filter(|s| s.kind != StallKind::Thela && s.kind != StallKind::Crates && s.kind != StallKind::Chai && s.kind != StallKind::Chairs).any(|s| s.pos.xz().distance(foot) < 1.0), "a stall stands in the door at {foot}");
        }
        // Loot stays in plain sight.
        for spot in layout.floor_loot.iter() {
            assert!(!rows.iter().any(|s| s.pos.xz().distance(spot.xz()) < 1.5), "a stall stands over the loot at {spot}");
        }
        let solid = rows.iter().filter(|s| market.stall_at(s.pos.xz())).count();
        assert_eq!(solid, rows.len(), "people walk round every stall");
        for name in ["Chandni Chowk", "Karol Bagh", "Lajpat Nagar", "Connaught Place"] {
            let p = layout.pois.iter().find(|p| p.name == name).expect("place");
            let inside = rows.iter().filter(|s| (s.pos.xz() - p.center.xz()).abs().max_element() < p.radius).count();
            assert!(inside >= 30, "{name} has {inside} row stalls");
        }
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

    /// `cargo test -p game --lib where_the_streets_are -- --ignored --nocapture`: what each place
    /// holds, and the road with the most stalls along it, to go and look at.
    #[test]
    #[ignore]
    fn where_the_streets_are() {
        let layout = generate(MapId::Ncr, 7);
        let start = bevy::platform::time::Instant::now();
        let market = Market::build(&layout, &StreetFloor::new(&layout));
        println!("built in {} ms: {} stalls, {} banners", start.elapsed().as_millis(), market.stalls.len(), market.banners.len());
        let t = &layout.terrain;
        for poi in layout.pois.iter() {
            let c = poi.center.xz();
            let inside = |s: &&Stall| (s.pos.xz() - c).abs().max_element() < poi.radius;
            let count = |k: StallKind| market.stalls.iter().filter(inside).filter(|s| s.kind == k).count();
            let (rows, spreads, carts) = (count(StallKind::Row), count(StallKind::Patri), count(StallKind::Cart));
            let (mut best, mut at, mut dir) = (0, Vec2::ZERO, Vec2::X);
            for j in -24..=24 {
                for i in -24..=24 {
                    let p = c + Vec2::new(i as f32, j as f32) * poi.radius / 24.0;
                    if t.ground_at(p.x, p.y) != Some(ground::ROAD) {
                        continue;
                    }
                    let n = market.stalls_near(p, 36.0).filter(|id| market.stalls[*id as usize].kind == StallKind::Row && market.stalls[*id as usize].pos.xz().distance(p) < 36.0).count();
                    if n > best {
                        // The road's way: the turn of the longest run of road from here.
                        let turns = (0..24).map(|k| Vec2::from_angle(k as f32 / 24.0 * std::f32::consts::TAU));
                        let run = |d: &Vec2| (1..40).take_while(|s| t.ground_at(p.x + d.x * *s as f32 * 2.0, p.y + d.y * *s as f32 * 2.0) == Some(ground::ROAD)).count();
                        dir = turns.max_by_key(run).unwrap_or(Vec2::X);
                        (best, at) = (n, p);
                    }
                }
            }
            let yaw = (-dir.x).atan2(-dir.y).to_degrees();
            println!("{:<24} rows {rows:>4} spreads {spreads:>4} carts {carts:>3}  best road {at:.0} yaw {yaw:.0} with {best} stalls round it, busy {:.2}", poi.name, busy_at(&market, at));
        }
    }

    #[test]
    fn the_dice_stay_in_range_and_keep_rolling() {
        let mut dice = Dice(1);
        let rolls: Vec<f32> = (0..1000).map(|_| dice.range(2.0, 3.0)).collect();
        assert!(rolls.iter().all(|r| (2.0..=3.0).contains(r)));
        assert!(rolls.windows(2).any(|w| w[0] != w[1]));
    }
}
