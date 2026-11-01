//! The one NCR map: the Delhi districts, each generated exactly as it is on its own, laid out on a
//! single 4 km square. Districts keep their real size and compass direction from Connaught Place;
//! only the distance between them is compressed. Open land fills the rest, so there is room to
//! fight between them. Axes: x east, z south, metres from the middle of the map.

use bevy::prelude::*;
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use super::terrain::{Terrain, ground};
use super::{MapId, MapLayout, MapStyle, delhi, footprint};

pub const HALF: f32 = 2000.0;
/// Metres between terrain samples. Each district's own terrain is a metre apart.
const STEP: f32 = 2.0;
/// How far out from its middle a district is kept. Each is generated out to 250 m of play.
const CLIP: f32 = 250.0;
/// Boundary walls: the first blocks every district pushes, dropped here because a district has no
/// edge on the big map.
const TILE_WALLS: usize = 4;

/// A district and where its middle sits on the map.
pub struct Place {
    pub id: MapId,
    pub at: Vec2,
    /// Share of ordinary buildings kept. Big buildings always stay, as they are what the place is
    /// known by; the rest are thinned so the streets have open ground between them.
    pub keep: f32,
}

/// Whole metres, so terrain samples line up.
pub const PLACES: [Place; 4] = [
    Place { id: MapId::Cp, at: Vec2::new(-900.0, -250.0), keep: 1.0 },
    Place { id: MapId::Chandni, at: Vec2::new(-540.0, -1020.0), keep: 0.45 },
    Place { id: MapId::IndiaGate, at: Vec2::new(-630.0, 390.0), keep: 1.0 },
    Place { id: MapId::HauzKhas, at: Vec2::new(-1260.0, 1000.0), keep: 0.7 },
];

/// Footprints at least this big (square metres) are never thinned out.
const LANDMARK_AREA: f32 = 900.0;

fn area(pts: &[Vec2]) -> f32 {
    let n = pts.len();
    ((0..n).map(|k| pts[k].perp_dot(pts[(k + 1) % n])).sum::<f32>() / 2.0).abs()
}

fn inside(p: Vec2) -> bool {
    p.abs().max_element() <= CLIP
}

/// Copy one district into the big map at `place`.
fn add(map: &mut MapLayout, place: &Place, seed: u64) {
    let mut t = delhi::generate(place.id, seed);
    let at = Vec3::new(place.at.x, 0.0, place.at.y);
    let mut rng = SmallRng::seed_from_u64(seed ^ (place.id.index() as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    t.blocks.drain(..TILE_WALLS);
    for mut b in t.blocks.drain(..).filter(|b| inside(b.pos.xz())) {
        b.pos += at;
        map.blocks.push(b);
    }
    for mut b in t.doors.drain(..).filter(|b| inside(b.pos.xz())) {
        b.pos += at;
        map.doors.push(b);
    }
    for mut c in t.chests.drain(..).filter(|c| inside(c.pos.xz())) {
        c.pos += at;
        map.chests.push(c);
    }
    map.floor_loot.extend(t.floor_loot.drain(..).filter(|p| inside(p.xz())).map(|p| p + at));
    for mut p in t.pois.drain(..).filter(|p| inside(p.center.xz())) {
        p.center += at;
        map.pois.push(p);
    }
    for mut p in t.props.drain(..).filter(|p| inside(p.pos.xz())) {
        p.pos += at;
        map.props.push(p);
    }
    for mut o in t.ornaments.drain(..).filter(|o| inside(o.pos.xz())) {
        o.pos += at;
        map.ornaments.push(o);
    }
    for mut l in t.lamps.drain(..).filter(|l| inside(l.pos.xz())) {
        l.pos += at;
        map.lamps.push(l);
    }
    for mut k in t.kit.drain(..).filter(|k| inside(k.pos.xz())) {
        k.pos += at;
        map.kit.push(k);
    }
    for mut f in t.footprints.drain(..) {
        // Whole buildings only: one cut at the edge would show its hollow inside.
        if !f.pts.iter().all(|p| inside(*p)) {
            continue;
        }
        let water = f.mat == super::Mat::Water;
        if !water && area(&f.pts) < LANDMARK_AREA && !rng.random_bool(place.keep as f64) {
            continue;
        }
        f.pts.iter_mut().for_each(|p| *p += place.at);
        map.footprints.push(f);
    }
    stamp(&mut map.terrain, &t.terrain, place.at);
}

/// Copy a district's ground into the big map's terrain.
fn stamp(world: &mut Terrain, tile: &Terrain, at: Vec2) {
    let n = world.n;
    let idx = |c: f32| ((c + world.size * 0.5) / STEP).max(0.0) as usize;
    let (i0, i1) = (idx(at.x - CLIP), idx(at.x + CLIP).min(n - 1));
    let (j0, j1) = (idx(at.y - CLIP), idx(at.y + CLIP).min(n - 1));
    let local = |c: f32| (((c + tile.size * 0.5) / tile.step()).round() as usize).min(tile.n - 1);
    for i in i0..=i1 {
        let li = local(world.coord(i) - at.x);
        for j in j0..=j1 {
            let lj = local(world.coord(j) - at.y);
            let (w, l) = (i * n + j, li * tile.n + lj);
            world.h[w] = tile.h[l];
            world.road_d[w] = tile.road_d[l];
            world.ground[w] = tile.ground[l];
        }
    }
}

pub fn generate(seed: u64) -> MapLayout {
    let size = 2.0 * HALF + 60.0;
    let n = (size / STEP) as usize + 1;
    let terrain = Terrain { n, size, h: vec![0.0; n * n], road_d: vec![1e3; n * n], ground: vec![ground::PARK; n * n] };
    let mut map = MapLayout { name: "NCR".into(), half_extent: HALF, style: MapStyle::City, terrain, ..default() };
    for place in &PLACES {
        add(&mut map, place, seed);
    }
    let MapLayout { terrain, blocks, .. } = &mut map;
    terrain.settle_under(blocks);
    map
}

/// Whether a point is inside any kept building, for tools that draw the map.
pub fn built(map: &MapLayout, p: Vec2) -> bool {
    map.footprints.iter().any(|f| f.pts[0].distance(p) < 400.0 && footprint::contains(&f.pts, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_do_not_overlap() {
        for (k, a) in PLACES.iter().enumerate() {
            assert!(a.at.abs().max_element() + CLIP <= HALF, "{:?} leaves the map", a.id);
            assert_eq!(a.at, a.at.round(), "{:?} must sit on whole metres", a.id);
            for b in &PLACES[k + 1..] {
                assert!((a.at - b.at).abs().max_element() >= 2.0 * CLIP, "{:?} overlaps {:?}", a.id, b.id);
            }
        }
    }

    #[test]
    fn same_seed_same_map_and_all_in_bounds() {
        let (a, b) = (generate(7), generate(7));
        assert_eq!(a.blocks.len(), b.blocks.len());
        assert_eq!(a.footprints.len(), b.footprints.len());
        assert!(a.blocks.iter().zip(&b.blocks).all(|(p, q)| p.pos == q.pos));
        assert!(a.blocks.iter().all(|b| b.pos.xz().abs().max_element() <= HALF));
        assert!(a.chests.iter().all(|c| c.pos.xz().abs().max_element() <= HALF));
        // Thinning leaves the map with fewer buildings than its districts have between them.
        let whole: usize = PLACES.iter().map(|p| delhi::generate(p.id, 7).footprints.len()).sum();
        assert!(a.footprints.len() < whole);
    }
}
