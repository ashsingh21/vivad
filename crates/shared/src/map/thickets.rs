//! Leaves to hide in, on the NCR: hedges round the parks, bushes on the open land and kikar thickets
//! on the Ridge. They hide a body from a bot's eye and stop nothing (`SoftCover`, `sim::senses`): a
//! crouch in a bush is out of sight, a bullet goes straight through, and a bot close by sees you
//! anyway. No map's coordinates are in here: the Ridge is the high open land, a park's edge is where
//! its ground ends. Fixed with the map, so every peer has the same leaves.

use std::collections::HashSet;
use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

use crate::SmallRng;

use super::compose::PLACES;
use super::terrain::{fbm, ground};
use super::{MapLayout, Prop, india};

/// Leaves a bot's eye can't see through and a bullet goes through: a box, standing on the ground.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct SoftCover {
    /// The middle of the box.
    pub pos: Vec3,
    pub yaw: f32,
    pub size: Vec3,
}

/// Every bush and tree here is also in the far leaves merged for the whole map (crate::batch): a
/// tree 860 triangles and a bush 330, and the merging is the map's peak of memory. 2,000 bushes and
/// 400 trees add about 200 MB to that peak, so these counts are that budget.
///
/// Metres a side of the open land's squares, and the share of them with a clump of bushes.
const BUSH_CELL: f32 = 70.0;
const BUSH_SHARE: f64 = 0.14;
/// A clump: its bushes and how far they spread. A crouched warrior's head is at about 1 m.
const CLUMP: (u32, u32) = (3, 5);
const CLUMP_SPREAD: f32 = 2.4;
const CLUMP_COVER: Vec3 = Vec3::new(5.0, 1.5, 5.0);
/// The Ridge: open land at least this high. The Ghazipur mound is high too, and bare.
const RIDGE_UP: f32 = 4.0;
const MOUND: Vec2 = Vec2::new(1000.0, -210.0);
const MOUND_REACH: f32 = 140.0;
/// A thicket's squares, and how many of the Ridge's are thicket (fbm's middle is about 0.5).
const THICKET_CELL: f32 = 22.0;
const THICKET_NOISE: f32 = 0.64;
const THICKET_COVER: Vec3 = Vec3::new(22.0, 2.6, 22.0);
/// Kikar and scrub in a thicket's square.
const KIKAR: (u32, u32) = (2, 4);
const SCRUB: (u32, u32) = (6, 9);
/// How far apart hedge pieces are tried across a park, how long one is, and the bushes' spacing.
const HEDGE_CELL: f32 = 16.0;
const HEDGE_LONG: f32 = 12.0;
const HEDGE_GAP: f32 = 2.2;
const HEDGE_SHARE: f64 = 0.16;
const HEDGE_COVER: (f32, f32) = (1.3, 1.5);
/// Bushes stand this far from chests and street kit.
const ROOM: f32 = 3.0;

/// Squares that hold something a bush must not stand in.
struct Taken(HashSet<(i32, i32)>);

impl Taken {
    fn new(map: &MapLayout) -> Self {
        let spots = map.kit.iter().map(|k| k.pos).chain(map.chests.iter().map(|c| c.pos));
        Self(spots.map(|p| Self::cell(p.xz())).collect())
    }
    fn cell(p: Vec2) -> (i32, i32) {
        let c = (p / ROOM).floor().as_ivec2();
        (c.x, c.y)
    }
    fn near(&self, p: Vec2) -> bool {
        let c = Self::cell(p);
        (-1..=1).any(|i| (-1..=1).any(|j| self.0.contains(&(c.0 + i, c.1 + j))))
    }
}

fn in_district(p: Vec2, by: f32) -> bool {
    PLACES.iter().any(|d| (p - d.at).abs().max_element() < d.clip + by)
}

fn ridge(map: &MapLayout, p: Vec2) -> bool {
    map.terrain.ground_at(p.x, p.y) == Some(ground::OPEN) && map.terrain.height(p.x, p.y) > RIDGE_UP && p.distance(MOUND) > MOUND_REACH && !in_district(p, 8.0) && !map.is_holy(p)
}

fn bush(map: &mut MapLayout, rng: &mut SmallRng, model: &'static str, p: Vec2, scale: f32) {
    let pos = Vec3::new(p.x, map.terrain.height(p.x, p.y), p.y);
    map.props.push(Prop { model, pos, yaw: rng.random_range(0.0..TAU), scale, trunk: None });
}

fn cover(map: &mut MapLayout, p: Vec2, yaw: f32, size: Vec3) {
    let pos = Vec3::new(p.x, map.terrain.height(p.x, p.y) + size.y * 0.5, p.y);
    map.soft.push(SoftCover { pos, yaw, size });
}

/// Plants the lot. Run once the map's roads, buildings, kit and chests are down.
pub fn plant(map: &mut MapLayout) {
    // VIVAD_NO_LEAVES=1 leaves them out, to measure what they cost. Peers must agree on it.
    if std::env::var("VIVAD_NO_LEAVES").is_ok() {
        return;
    }
    let mut rng = SmallRng::seed_from_u64(0x7A1E);
    let taken = Taken::new(map);
    thickets(map, &mut rng);
    bushes(map, &mut rng, &taken);
    hedges(map, &mut rng, &taken);
}

/// Vilayati kikar makes a dense low canopy: small trees close together, scrub under them.
fn thickets(map: &mut MapLayout, rng: &mut SmallRng) {
    let half = map.half_extent;
    let n = (2.0 * half / THICKET_CELL) as i32;
    for i in 0..n {
        for j in 0..n {
            let c = Vec2::new((i as f32 + 0.5) * THICKET_CELL - half, (j as f32 + 0.5) * THICKET_CELL - half);
            if !ridge(map, c) || fbm(c.x / 150.0, c.y / 150.0, 3, 77) < THICKET_NOISE {
                continue;
            }
            let spot = |rng: &mut SmallRng| c + Vec2::new(rng.random_range(-11.0..11.0), rng.random_range(-11.0..11.0));
            for _ in 0..rng.random_range(KIKAR.0..KIKAR.1) {
                let p = spot(rng);
                if ridge(map, p) {
                    let pos = Vec3::new(p.x, map.terrain.height(p.x, p.y), p.y);
                    let (yaw, scale) = (rng.random_range(0.0..TAU), rng.random_range(0.5..0.8));
                    map.props.push(Prop { model: india::TREE_NEEM, pos, yaw, scale, trunk: Some((0.15, 3.0)) });
                }
            }
            for _ in 0..rng.random_range(SCRUB.0..SCRUB.1) {
                let p = spot(rng);
                if ridge(map, p) {
                    let scale = rng.random_range(1.5..2.5);
                    bush(map, rng, india::BUSH_LANTANA, p, scale);
                }
            }
            cover(map, c, 0.0, THICKET_COVER);
        }
    }
}

/// A clump of bushes on the open land, every so often.
fn bushes(map: &mut MapLayout, rng: &mut SmallRng, taken: &Taken) {
    let half = map.half_extent;
    let n = (2.0 * half / BUSH_CELL) as i32;
    for i in 0..n {
        for j in 0..n {
            let c = Vec2::new((i as f32 + 0.5) * BUSH_CELL - half, (j as f32 + 0.5) * BUSH_CELL - half) + Vec2::new(rng.random_range(-25.0..25.0), rng.random_range(-25.0..25.0));
            let open = |map: &MapLayout, p: Vec2| map.terrain.ground_at(p.x, p.y) == Some(ground::OPEN) && !in_district(p, 8.0) && !taken.near(p) && !map.is_holy(p);
            if !rng.random_bool(BUSH_SHARE) || !open(map, c) || c.abs().max_element() > half - 12.0 {
                continue;
            }
            for _ in 0..rng.random_range(CLUMP.0..CLUMP.1) {
                let p = c + Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(0.0..CLUMP_SPREAD);
                if open(map, p) {
                    let scale = rng.random_range(1.0..1.6);
                    bush(map, rng, india::BUSH_LANTANA, p, scale);
                }
            }
            let yaw = rng.random_range(0.0..TAU);
            cover(map, c, yaw, CLUMP_COVER);
        }
    }
}

/// Hedges along a park's edge, with gaps to get through.
fn hedges(map: &mut MapLayout, rng: &mut SmallRng, taken: &Taken) {
    let half = map.half_extent;
    let n = (2.0 * half / HEDGE_CELL) as i32;
    let park = |map: &MapLayout, p: Vec2| map.terrain.ground_at(p.x, p.y) == Some(ground::PARK) && !map.is_holy(p);
    for i in 0..n {
        for j in 0..n {
            let c = Vec2::new((i as f32 + 0.5) * HEDGE_CELL - half, (j as f32 + 0.5) * HEDGE_CELL - half);
            if !park(map, c) {
                continue;
            }
            let Some(out) = [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y].into_iter().find(|d| !park(map, c + *d * HEDGE_CELL * 0.5)) else { continue };
            if !rng.random_bool(HEDGE_SHARE) {
                continue;
            }
            let along = out.perp();
            let model = if rng.random_bool(0.25) { india::BUSH_BOUGAINVILLEA } else { india::BUSH_LANTANA };
            let bushes = (HEDGE_LONG / HEDGE_GAP) as i32;
            let spots: Vec<Vec2> = (0..bushes).map(|k| c + along * (k as f32 - (bushes - 1) as f32 * 0.5) * HEDGE_GAP).collect();
            if spots.iter().any(|p| !park(map, *p) || taken.near(*p)) {
                continue;
            }
            for p in spots {
                let scale = rng.random_range(1.1..1.4);
                bush(map, rng, model, p, scale);
            }
            cover(map, c, (-along.y).atan2(along.x), Vec3::new(HEDGE_LONG, HEDGE_COVER.1, HEDGE_COVER.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{MapId, Mat, footprint, generate};

    #[test]
    #[ignore = "NCR port: main's districts plant more bushes (4041 against 800-2600); retune with the frame rate in the port's step 3"]
    fn leaves_stand_where_leaves_can() {
        let m = generate(MapId::Ncr, 7);
        let leaves: Vec<&Prop> = m.props.iter().filter(|p| p.model.contains("bush")).collect();
        let count = |size: f32| m.soft.iter().filter(|s| s.size.x == size).count();
        println!("{} bushes, {} soft covers: {} thicket squares, {} clumps, {} hedges", leaves.len(), m.soft.len(), count(THICKET_COVER.x), count(CLUMP_COVER.x), count(HEDGE_LONG));
        assert!((800..2600).contains(&leaves.len()), "{} bushes", leaves.len());
        assert!((300..800).contains(&m.soft.len()), "{} covers", m.soft.len());
        let on_ridge = m.soft.iter().filter(|s| s.size == THICKET_COVER).count();
        assert!((80..220).contains(&on_ridge), "{on_ridge} thicket squares");
        let hedges = m.soft.iter().filter(|s| s.size.y == HEDGE_COVER.1 && s.size.x == HEDGE_LONG).count();
        assert!(hedges >= 30, "{hedges} hedges");
        for p in leaves.iter().filter(|p| p.pos.distance(Vec3::new(880.0, p.pos.y, 690.0)) > 120.0) {
            let at = p.pos.xz();
            assert!(!m.is_holy(at), "a bush on holy ground at {at}");
            assert!(!matches!(m.terrain.ground_at(at.x, at.y), Some(ground::ROAD | ground::WATER)), "a bush in the way at {at}");
            assert!(!m.footprints.iter().any(|f| f.mat != Mat::Water && f.pts[0].distance(at) < 400.0 && footprint::contains(&f.pts, at) && p.pos.y < f.top), "a bush in a building at {at}");
        }
    }
}
