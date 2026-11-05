//! Panga's maps: Delhi at real scale, from OpenStreetMap (assets/data/maps/delhi_<area>.json,
//! written by tools/osm/delhi.py; map data (c) OpenStreetMap contributors, ODbL 1.0): Connaught
//! Place, Chandni Chowk, Hauz Khas and India Gate.
//!
//! Every mapped building is its real outline, extruded (crate::map::footprint), as are the houses
//! the converter fills the old city's and the village's gaps with; roads, lanes, car parks, parks
//! and water are painted onto flat ground (`Terrain::ground`), the water sunk for wading. Some
//! landmarks are built here: Connaught Place's inner circle (crate::map::cp, at its real radii,
//! round Central Park with its flag) and India Gate. Avenue trees line the main roads and fill the
//! parks. Axes: x east, z south, metres from the middle of the map.

use std::cmp::Reverse;
use std::f32::consts::PI;

use bevy::prelude::*;
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use super::build::{Builder, FLOOR_H, Frame, Opening};
use super::terrain::{self, Pad, TerrainSpec, ground};
use super::{ChestKind, Footprint, MapLayout, Mat, Poi, Prop};

fn data(map: super::MapId) -> &'static str {
    match map {
        super::MapId::Chandni => include_str!("../../../../assets/data/maps/delhi_chandni.json"),
        super::MapId::HauzKhas => include_str!("../../../../assets/data/maps/delhi_hauzkhas.json"),
        super::MapId::IndiaGate => include_str!("../../../../assets/data/maps/delhi_indiagate.json"),
        _ => include_str!("../../../../assets/data/maps/delhi_cp.json"),
    }
}
const HALF: f32 = 250.0;
/// Ground samples per side: about a metre apart, for roads' edges.
const SAMPLES: usize = 561;

#[derive(Deserialize)]
struct Data {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    south_shift: f32,
    #[serde(default)]
    landmarks: Vec<Landmark>,
    buildings: Vec<Building>,
    roads: Vec<Road>,
    areas: Vec<Area>,
    trees: Vec<[f32; 2]>,
    pois: Vec<Place>,
}

#[derive(Deserialize)]
struct Building {
    pts: Vec<[f32; 2]>,
    h: f32,
    kind: String,
    #[serde(default)]
    base: Option<f32>,
}

#[derive(Deserialize)]
struct Road {
    kind: String,
    w: f32,
    pts: Vec<[f32; 2]>,
}

#[derive(Deserialize)]
struct Area {
    kind: String,
    pts: Vec<[f32; 2]>,
}

#[derive(Deserialize)]
struct Landmark {
    kind: String,
    x: f32,
    z: f32,
}

#[derive(Deserialize)]
struct Place {
    name: String,
    x: f32,
    z: f32,
    r: f32,
}

fn v2(p: &[f32; 2]) -> Vec2 {
    Vec2::new(p[0], p[1])
}

/// The inner circle at its real size: the Inner Circle road at 125 m round Central Park, the
/// blocks from 136 to 168 m, the seven radial roads where they really run (bearings from
/// OpenStreetMap, degrees from east toward south).
fn ring(center: Vec3) -> super::cp::Ring {
    super::cp::Ring {
        center,
        park: 112.0,
        ri: 136.0,
        ro: 168.0,
        ver: 4.2,
        g: 5.5,
        u: 4.6,
        seg_len: 4.6,
        street: 13.0,
        streets: [34.0f32, 93.6, 150.6, 212.2, 274.6, 304.1, 332.2].iter().map(|d| d.to_radians()).collect(),
        flag: 45.0,
    }
}

pub fn generate(id: super::MapId, seed: u64) -> MapLayout {
    let data: Data = serde_json::from_str(data(id)).expect("assets/data/maps/delhi_*.json");
    let mut map = MapLayout { name: data.title.clone().unwrap_or_else(|| "Dilli".into()), half_extent: HALF, ..default() };
    map.terrain = terrain::generate(&TerrainSpec {
        size: 2.0 * HALF + 60.0,
        n: SAMPLES,
        seed: seed as u32,
        half_extent: HALF,
        // Flat as a city: one pad over everything.
        pads: vec![Pad { center: Vec2::ZERO, radius: 2000.0, falloff: 1.0 }],
        roads: vec![],
        river: (1.0e5, 1.0e5 + 1.0, 0.0),
        pits: vec![],
        mesas: vec![],
        banks: vec![],
    });
    paint_ground(&mut map, &data);
    let cp = Vec3::new(0.0, 0.0, -data.south_shift);
    // Only Connaught Place has the ring; elsewhere a ring of no size keeps nothing out.
    let ring = if id == super::MapId::Cp { ring(cp) } else { super::cp::Ring { ri: 0.0, ro: 0.0, ..self::ring(Vec3::new(1.0e5, 0.0, 1.0e5)) } };
    let made = if id == super::MapId::Cp { CP_HILLS.iter().map(|h| h.at(&ring)).collect() } else { vec![] };
    hills(&mut map, &data, &ring, &made, seed as u32);
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut b = Builder::new(&mut map);
    boundary(&mut b);
    if id == super::MapId::Cp {
        super::cp::build(&mut b, &mut rng, &ring);
        dress_hills(&mut b, &mut rng, &made);
    }
    for l in &data.landmarks {
        if l.kind == "india_gate" {
            india_gate(&mut b, Vec3::new(l.x, 0.0, l.z));
        }
    }
    water(&mut b, &data);
    let houses = footprints(&mut b, &mut rng, &data, &ring, id);
    // The ring's roads, now the buildings round it are known (the outer circle stops at them).
    let outer = super::cp::OuterClear::new(b.map, &ring);
    if id == super::MapId::Cp {
        super::cp::paint(&mut b.map.terrain, &ring, &outer);
        super::cp::markings(&mut b, &ring, &outer);
    }
    trees(&mut b, &mut rng, &data, &ring, &houses);
    // No trees on the made hills' tops: they're for seeing from.
    b.map.props.retain(|p| !made.iter().any(|h| p.pos.xz().distance(h.center) < h.plateau + 3.0));
    let roads: Vec<(Vec<Vec2>, f32, bool)> = data
        .roads
        .iter()
        .filter(|r| !matches!(r.kind.as_str(), "path" | "footway" | "steps" | "cycleway"))
        .map(|r| (r.pts.iter().map(v2).collect(), r.w, !matches!(r.kind.as_str(), "primary" | "secondary" | "tertiary" | "trunk")))
        .collect();
    super::kit::streets(&mut b, &mut rng, &roads, &houses, HALF);
    super::kit::open_spaces(&mut b, &mut rng, &houses, HALF);
    super::kit::roofs(&mut b, &mut rng);
    if id == super::MapId::Cp {
        super::cp::dress(&mut b, &mut rng, &ring, &outer);
    }
    street_loot(&mut b, &mut rng, &data, &ring, &houses);
    map.terrain.settle_under(&map.blocks);
    map.pois = data.pois.iter().map(|p| Poi { name: p.name.clone(), center: Vec3::new(p.x, 0.0, p.z), radius: p.r }).collect();
    if id == super::MapId::Cp {
        map.pois.push(Poi { name: "Connaught Place".into(), center: cp, radius: 175.0 });
    }
    map
}

/// A hill made in Central Park: where (degrees from east toward south, metres from the middle),
/// its flat top's radius, its height and the width of its slopes.
struct MadeHill {
    bearing: f32,
    r: f32,
    plateau: f32,
    top: f32,
    skirt: f32,
    /// What crowns it: a lookout pavilion, a pile of rocks, or nothing.
    crown: Crown,
}

#[derive(Clone, Copy, PartialEq)]
enum Crown {
    Lookout,
    Rocks,
    Bare,
}

/// A made hill placed on the map.
struct Hill {
    center: Vec2,
    plateau: f32,
    top: f32,
    skirt: f32,
    crown: Crown,
}

impl MadeHill {
    fn at(&self, ring: &super::cp::Ring) -> Hill {
        Hill { center: ring.at(self.r, self.bearing.to_radians(), 0.0).xz(), plateau: self.plateau, top: self.top, skirt: self.skirt, crown: self.crown }
    }
}

/// Central Park's hills, each in a wedge between two radial paths: the big one (higher than the
/// blocks' roofs, to see over them) with a lookout on top, a rocky one across the park from it,
/// and a pair of low mounds to the west. Slopes stay under 35 degrees (walkable to 48).
const CP_HILLS: [MadeHill; 4] = [
    MadeHill { bearing: 243.0, r: 72.0, plateau: 7.0, top: 11.0, skirt: 24.0, crown: Crown::Lookout },
    MadeHill { bearing: 3.0, r: 72.0, plateau: 4.0, top: 7.0, skirt: 20.0, crown: Crown::Rocks },
    MadeHill { bearing: 178.0, r: 52.0, plateau: 3.0, top: 4.5, skirt: 14.0, crown: Crown::Bare },
    MadeHill { bearing: 186.0, r: 88.0, plateau: 3.0, top: 4.0, skirt: 13.0, crown: Crown::Rocks },
];

/// High ground to fight over (creative liberty: Delhi's lawns are near flat). Every park rolls in
/// low swells, rising away from its edges, the buildings in it and its paths, to hide behind;
/// on Connaught Place, Central Park's fountain and radial paths stay level, and its made hills
/// rise between them.
fn hills(map: &mut MapLayout, data: &Data, ring: &super::cp::Ring, made: &[Hill], seed: u32) {
    let t = &mut map.terrain;
    let n = t.n;
    let step = t.step();
    // Where hills may rise: park, not under a building.
    let mut open: Vec<bool> = t.ground.iter().map(|&g| g == ground::PARK).collect();
    let idx = |c: f32| (((c + t.size * 0.5) / step).floor().max(0.0) as usize).min(n - 1);
    for bld in &data.buildings {
        let pts: Vec<Vec2> = bld.pts.iter().map(v2).collect();
        let (lo, hi) = pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        for i in idx(lo.x - 2.0)..=idx(hi.x + 2.0) {
            for j in idx(lo.y - 2.0)..=idx(hi.y + 2.0) {
                let p = Vec2::new(t.coord(i), t.coord(j));
                if super::footprint::contains(&pts, p) || pts.iter().any(|q| q.distance(p) < 2.0) {
                    open[i * n + j] = false;
                }
            }
        }
    }
    // Distance to the nearest sample that must stay level (two-pass chamfer).
    let mut d: Vec<f32> = open.iter().map(|&o| if o { 1.0e4 } else { 0.0 }).collect();
    let diag = step * std::f32::consts::SQRT_2;
    for i in 0..n {
        for j in 0..n {
            let mut v = d[i * n + j];
            if i > 0 { v = v.min(d[(i - 1) * n + j] + step); }
            if j > 0 { v = v.min(d[i * n + j - 1] + step); }
            if i > 0 && j > 0 { v = v.min(d[(i - 1) * n + j - 1] + diag); }
            if i > 0 && j + 1 < n { v = v.min(d[(i - 1) * n + j + 1] + diag); }
            d[i * n + j] = v;
        }
    }
    for i in (0..n).rev() {
        for j in (0..n).rev() {
            let mut v = d[i * n + j];
            if i + 1 < n { v = v.min(d[(i + 1) * n + j] + step); }
            if j + 1 < n { v = v.min(d[i * n + j + 1] + step); }
            if i + 1 < n && j + 1 < n { v = v.min(d[(i + 1) * n + j + 1] + diag); }
            if i + 1 < n && j > 0 { v = v.min(d[(i + 1) * n + j - 1] + diag); }
            d[i * n + j] = v;
        }
    }
    let c = ring.center.xz();
    let radials: Vec<(Vec2, Vec2)> = ring.streets.iter().map(|&a| (c, ring.at(ring.park, a, 0.0).xz())).collect();
    for i in 0..n {
        for j in 0..n {
            let e = d[i * n + j];
            let p = Vec2::new(t.coord(i), t.coord(j));
            // Only park ground rises (or, in Central Park, its footpaths over a made hill).
            if e <= 0.0 && !(t.ground[i * n + j] == ground::PATH && p.distance(c) < ring.park) {
                continue;
            }
            // Swells: broad and mostly low (waist-high cover), now and then a knoll of 4 to 6 m.
            let swell = (terrain::fbm(p.x / 48.0, p.y / 48.0, 3, seed + 101) - 0.42).max(0.0) * 16.0 + terrain::fbm(p.x / 12.0, p.y / 12.0, 2, seed + 103) * 0.8;
            let mut h = swell * smooth(5.0, 24.0, e);
            // The made hills rise over the park's footpaths (which would cut gullies through them),
            // fading out only toward the park's edge.
            let inside = 1.0 - smooth(ring.park - 10.0, ring.park - 2.0, p.distance(c));
            for hill in made {
                let out = (p.distance(hill.center) - hill.plateau).max(0.0);
                h = h.max(hill.top * (1.0 - smooth(0.0, hill.skirt, out)) * inside);
            }
            // Central Park's fountain and the paths out from it stay level.
            if p.distance(c) < ring.park + 4.0 {
                let path = radials.iter().map(|(a, b)| seg_dist(p, *a, *b)).fold(f32::MAX, f32::min);
                h *= smooth(24.0, 36.0, p.distance(c)) * smooth(4.5, 12.0, path);
            }
            t.h[i * n + j] = h;
            // A footpath over a made hill gives out at its foot: the hill's grass climbs over it.
            if t.ground[i * n + j] == ground::PATH && h > 0.6 {
                t.ground[i * n + j] = ground::PARK;
            }
        }
    }
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// What's on the made hills: a chhatri to look out from on the big one, with a tijori (worth the
/// climb) under it; boulders to hide behind on the rocky ones' tops and down every hill's slopes.
fn dress_hills(b: &mut Builder, rng: &mut SmallRng, made: &[Hill]) {
    let w = Frame::new(Vec3::ZERO, 0.0);
    let ground = |b: &Builder, p: Vec2| b.map.terrain.height(p.x, p.y);
    let rock = |b: &mut Builder, rng: &mut SmallRng, p: Vec2, r: f32| {
        let at = Vec3::new(p.x, ground(b, p) + r * 0.3, p.y);
        b.push(at, Quat::IDENTITY, super::Shape::Sphere(r * 0.92), Mat::Granite, true, false);
        let model = super::india::ROCKS[rng.random_range(0..super::india::ROCKS.len())];
        b.ornament(w, at, rng.random_range(0.0..std::f32::consts::TAU), model, Vec3::splat(r));
    };
    for h in made {
        let top = Vec3::new(h.center.x, ground(b, h.center), h.center.y);
        match h.crown {
            Crown::Lookout => {
                // Its platform sunk into the hilltop, the floor a step (12 cm) above the ground: drawn
                // above it, the floor hid the feet of anyone standing there (it has no collider,
                // and is a high step). A flat floor to stand on (and to keep the grass off it).
                let lift = 0.12;
                b.chhatri(w, top - Vec3::Y * (Builder::chhatri_floor(4.2) - lift), 4.2, Mat::Sandstone);
                b.push(top + Vec3::Y * (lift - 0.15), Quat::IDENTITY, super::Shape::Box(Vec3::new(4.2 * 1.1, 0.3, 4.2 * 1.1)), Mat::Sandstone, true, false);
                b.chest(w, top + Vec3::Y * lift, rng.random_range(0.0..std::f32::consts::TAU), ChestKind::Tijori);
            }
            Crown::Rocks => {
                for k in 0..4 {
                    let a = std::f32::consts::TAU * (k as f32 + rng.random_range(0.0..0.5)) / 4.0;
                    let p = h.center + Vec2::from_angle(a) * h.plateau * rng.random_range(0.4..0.9);
                    let r = rng.random_range(1.1..1.8);
                    rock(b, rng, p, r);
                }
                b.loot(w, top + Vec3::Y * 0.3);
            }
            Crown::Bare => b.loot(w, top + Vec3::Y * 0.3),
        }
        // Outcrops: big boulders sunk into the slopes in twos and threes, a ledge to stand on
        // and a wall to hide behind (the higher hills only).
        if h.top > 5.0 {
            for k in 0..3 {
                let a = std::f32::consts::TAU * (k as f32 + rng.random_range(0.0..0.6)) / 3.0;
                let c = h.center + Vec2::from_angle(a) * (h.plateau + h.skirt * rng.random_range(0.3..0.5));
                for _ in 0..rng.random_range(2..4) {
                    let p = c + Vec2::new(rng.random_range(-2.5..2.5), rng.random_range(-2.5..2.5));
                    let r = rng.random_range(2.2..3.4);
                    let at = Vec3::new(p.x, ground(b, p) - r * 0.15, p.y);
                    b.push(at, Quat::IDENTITY, super::Shape::Sphere(r * 0.92), Mat::Granite, true, false);
                    let model = super::india::ROCKS[rng.random_range(0..super::india::ROCKS.len())];
                    b.ornament(w, at, rng.random_range(0.0..std::f32::consts::TAU), model, Vec3::splat(r));
                }
            }
        }
        // Boulders half way down, scattered round.
        for _ in 0..(h.skirt / 4.0) as usize {
            let a = rng.random_range(0.0..std::f32::consts::TAU);
            let p = h.center + Vec2::from_angle(a) * (h.plateau + h.skirt * rng.random_range(0.25..0.6));
            let r = rng.random_range(0.8..1.5);
            rock(b, rng, p, r);
        }
    }
}

/// The water's depth below the street, and where its surface stands.
const WATER_BED: f32 = -1.4;
const WATER_TOP: f32 = -0.35;

/// Ponds, the tank and the canals: the ground under them sunk to a wading depth, and a surface on
/// top (no collider: you wade).
fn water(b: &mut Builder, data: &Data) {
    let t = &mut b.map.terrain;
    let n = t.n;
    for (i, j) in (0..n).flat_map(|i| (0..n).map(move |j| (i, j))) {
        if t.ground[i * n + j] == ground::WATER {
            t.h[i * n + j] = WATER_BED;
        }
    }
    for a in data.areas.iter().filter(|a| a.kind == "water") {
        b.map.footprints.push(Footprint { pts: a.pts.iter().map(v2).collect(), base: WATER_BED, top: WATER_TOP, mat: Mat::Water });
    }
}

/// India Gate (Lutyens, 1931): a 42 m arch of buff sandstone on a red base, its 9 m opening on the
/// east-west axis down Kartavya Path; an attic with cornices above, a stepped crown and a shallow
/// bowl at the top. On a marble plaza.
/// India Gate, stylised (docs/style-guide.md: chunky, soft, big shadow lines) to its own
/// proportions: broad for its 42 m, a great arch through it and a smaller one through each pier,
/// deep cornices, the attic with its inscription panel, the stepped crown and the bowl. Pale buff
/// stone over a red sandstone base. Its faces look along x; it spans z.
fn india_gate(b: &mut Builder, c: Vec3) {
    let f = Frame::new(c, 0.0);
    // Half depth, half width, the great arch's half opening, its springing and crown; the side
    // arches' half opening and height.
    let (d, hw, open, spring, crown) = (5.2f32, 15.0f32, 4.8f32, 18.0f32, 23.4f32);
    let (side_open, side_h) = (2.3f32, 14.0f32);
    let attic = 34.0;
    b.bx(f, Vec3::new(0.0, -0.1, 0.0), Vec3::new(40.0, 0.26, 52.0), Mat::Marble);
    for s in [-1.0f32, 1.0] {
        let (z0, z1) = (open, hw);
        let zc = (z0 + z1) / 2.0;
        // The red base, a step proud all round.
        b.bx(f, Vec3::new(0.0, 1.3, s * zc), Vec3::new(2.0 * d + 0.8, 2.6, z1 - z0 + 0.8), Mat::RedSandstone);
        b.deco(f, Vec3::new(0.0, 2.7, s * zc), Vec3::new(2.0 * d + 1.0, 0.3, z1 - z0 + 1.0), Mat::Buff);
        // The pier, cut through from end to end (along z) by its side arch, as the real one's ends are.
        for x in [-1.0f32, 1.0] {
            b.bx(f, Vec3::new(x * (d + side_open) / 2.0, 2.6 + (attic - 2.6) / 2.0, s * zc), Vec3::new(d - side_open, attic - 2.6, z1 - z0), Mat::Buff);
        }
        b.bx(f, Vec3::new(0.0, side_h + (attic - side_h) / 2.0, s * zc), Vec3::new(2.0 * side_open, attic - side_h, z1 - z0), Mat::Buff);
        arch(b, Frame::new(c, std::f32::consts::FRAC_PI_2), Vec3::new(s * zc, side_h - 2.3, 0.0), z1 - z0, side_open, 2.3, 10);
        // The impost band where the great arch springs, and a pilaster strip up each pier's outer corners.
        b.deco(f, Vec3::new(0.0, spring - 0.3, s * zc), Vec3::new(2.0 * d + 0.7, 0.6, z1 - z0 + 0.2), Mat::Buff);
        for x in [-1.0f32, 1.0] {
            b.deco(f, Vec3::new(x * (d + 0.15), (2.9 + attic) / 2.0, s * (z1 - 0.7)), Vec3::new(0.3, attic - 2.9, 1.2), Mat::Buff);
        }
    }
    // The great arch's head: steps corbelling in from each pier to the crown, the attic over it.
    arch(b, f, Vec3::new(0.0, spring, 0.0), 2.0 * d, open, crown - spring, 16);
    b.bx(f, Vec3::new(0.0, (crown + attic) / 2.0, 0.0), Vec3::new(2.0 * d, attic - crown, 2.0 * open), Mat::Buff);
    // An archivolt round the opening on both faces, and its keystone.
    for x in [-1.0f32, 1.0] {
        b.deco(f, Vec3::new(x * (d + 0.12), crown + 0.45, 0.0), Vec3::new(0.24, 0.9, 2.0 * open + 1.4), Mat::Buff);
        b.deco(f, Vec3::new(x * (d + 0.2), crown - 0.2, 0.0), Vec3::new(0.4, 1.6, 1.1), Mat::Buff);
        // The inscription panel on the attic, a slab proud of it, lettered.
        b.deco(f, Vec3::new(x * (d + 0.1), crown + 4.6, 0.0), Vec3::new(0.2, 3.6, 2.0 * hw - 7.0), Mat::Buff);
        letters(b, c, x, d + 0.2, crown + 3.6, "INDIA");
    }
    // Deep cornices: under the attic and at its top, overhanging well (their shadows draw it).
    b.deco(f, Vec3::new(0.0, crown + 1.4, 0.0), Vec3::new(2.0 * d + 1.0, 0.5, 2.0 * hw + 1.0), Mat::Buff);
    b.deco(f, Vec3::new(0.0, attic + 0.35, 0.0), Vec3::new(2.0 * d + 1.6, 0.7, 2.0 * hw + 1.6), Mat::Buff);
    b.deco(f, Vec3::new(0.0, attic - 0.25, 0.0), Vec3::new(2.0 * d + 0.9, 0.5, 2.0 * hw + 0.9), Mat::Buff);
    // The stepped crown and the bowl on top.
    b.bx(f, Vec3::new(0.0, attic + 0.7 + 1.5, 0.0), Vec3::new(2.0 * d - 1.0, 3.0, 2.0 * hw * 0.62), Mat::Buff);
    b.deco(f, Vec3::new(0.0, attic + 3.85, 0.0), Vec3::new(2.0 * d - 0.4, 0.3, 2.0 * hw * 0.62 + 0.6), Mat::Buff);
    b.bx(f, Vec3::new(0.0, attic + 4.0 + 1.1, 0.0), Vec3::new(2.0 * d - 3.0, 2.2, 2.0 * hw * 0.36), Mat::Buff);
    b.cyl(f, Vec3::new(0.0, attic + 6.2, 0.0), 2.6, 0.9, Mat::Buff);
    b.cyl(f, Vec3::new(0.0, attic + 7.1, 0.0), 3.4, 0.9, Mat::Buff);
    b.map.pois.push(Poi { name: "India Gate".into(), center: c, radius: 34.0 });
}

/// Chunky raised letters on a face of India Gate's attic (the face toward `side` x, at `face` out
/// from the middle), their foot at `y`, centred: strokes 28 cm across, 12 cm proud, 2 m tall,
/// in the base's red sandstone so they read even with the face in shade (the real ones are cut
/// deep; a painted world shows that as contrast). Only the capitals used are drawn.
fn letters(b: &mut Builder, c: Vec3, side: f32, face: f32, y: f32, text: &str) {
    const H: f32 = 2.0;
    const ADVANCE: f32 = 1.75;
    const STROKE: f32 = 0.28;
    // Strokes on a 1 wide by 1.6 high cell (u right, v up).
    let strokes = |ch: char| -> &'static [((f32, f32), (f32, f32))] {
        match ch {
            'I' => &[((0.5, 0.0), (0.5, 1.6))],
            'N' => &[((0.1, 0.0), (0.1, 1.6)), ((0.9, 0.0), (0.9, 1.6)), ((0.1, 1.6), (0.9, 0.0))],
            'D' => &[((0.1, 0.0), (0.1, 1.6)), ((0.1, 1.6), (0.6, 1.6)), ((0.1, 0.0), (0.6, 0.0)), ((0.6, 1.6), (0.9, 1.2)), ((0.9, 1.2), (0.9, 0.4)), ((0.9, 0.4), (0.6, 0.0))],
            'A' => &[((0.05, 0.0), (0.5, 1.6)), ((0.5, 1.6), (0.95, 0.0)), ((0.27, 0.62), (0.73, 0.62))],
            _ => &[],
        }
    };
    let k = H / 1.6;
    let width = ADVANCE * (text.chars().count() as f32 - 1.0) + k;
    // Read left to right from in front: on the +x face that runs toward -z, on the -x face toward +z.
    let along = -side;
    for (i, ch) in text.chars().enumerate() {
        let u0 = -width / 2.0 + i as f32 * ADVANCE;
        for &((u1, v1), (u2, v2)) in strokes(ch) {
            let (p1, p2) = (Vec3::new(0.0, y + v1 * k, along * (u0 + u1 * k)), Vec3::new(0.0, y + v2 * k, along * (u0 + u2 * k)));
            let dir = (p2 - p1).normalize();
            let len = p1.distance(p2) + STROKE;
            let rot = Quat::from_mat3(&Mat3::from_cols(dir, Vec3::X.cross(dir), Vec3::X));
            let mid = (p1 + p2) * 0.5 + Vec3::X * side * (face + 0.06);
            b.push(c + mid, rot, super::Shape::Box(Vec3::new(len, STROKE, 0.12)), Mat::RedSandstone, false, true);
        }
    }
}

/// An arch's head: from `at` (the springing, the opening's middle), steps corbelling in from both
/// sides, `half` the opening, to a round crown `rise` up, `depth` through (along x), in `steps`.
fn arch(b: &mut Builder, f: Frame, at: Vec3, depth: f32, half: f32, rise: f32, steps: usize) {
    let h = rise / steps as f32;
    for k in 0..steps {
        // The opening's half width at the top of this step, on an ellipse.
        let t = (k as f32 + 1.0) / steps as f32;
        let w = half * (1.0 - t * t).max(0.0).sqrt();
        let fill = half - w;
        if fill < 0.02 {
            continue;
        }
        for s in [-1.0f32, 1.0] {
            b.bx(f, at + Vec3::new(0.0, (k as f32 + 0.5) * h, s * (half - fill / 2.0)), Vec3::new(depth, h, fill), Mat::Buff);
        }
    }
}

/// Invisible walls round the playable square (the city carries on beyond them).
fn boundary(b: &mut Builder) {
    for (c, s) in [
        (Vec3::new(HALF + 2.0, 60.0, 0.0), Vec3::new(4.0, 160.0, 2.0 * HALF + 8.0)),
        (Vec3::new(-HALF - 2.0, 60.0, 0.0), Vec3::new(4.0, 160.0, 2.0 * HALF + 8.0)),
        (Vec3::new(0.0, 60.0, HALF + 2.0), Vec3::new(2.0 * HALF + 8.0, 160.0, 4.0)),
        (Vec3::new(0.0, 60.0, -HALF - 2.0), Vec3::new(2.0 * HALF + 8.0, 160.0, 4.0)),
    ] {
        b.push(c, Quat::IDENTITY, super::Shape::Box(s), Mat::Granite, true, false);
    }
}

/// What covers each ground sample: pavement by default, parks and car parks from their outlines,
/// then roads (and pedestrian plazas) at their widths over everything.
fn paint_ground(map: &mut MapLayout, data: &Data) {
    let t = &mut map.terrain;
    let n = t.n;
    t.ground = vec![ground::PAVEMENT; n * n];
    let step = t.step();
    let half = t.size * 0.5;
    let idx = |c: f32| (((c + half) / step).floor().max(0.0) as usize).min(n - 1);
    // Water last of the areas: a lake inside a park is water.
    let mut areas: Vec<&Area> = data.areas.iter().collect();
    areas.sort_by_key(|a| a.kind == "water");
    for a in areas {
        let pts: Vec<Vec2> = a.pts.iter().map(v2).collect();
        let class = match a.kind.as_str() {
            "park" | "pitch" => ground::PARK,
            "parking" => ground::PARKING,
            "water" => ground::WATER,
            _ => continue,
        };
        let (lo, hi) = pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        for i in idx(lo.x)..=idx(hi.x) {
            for j in idx(lo.y)..=idx(hi.y) {
                if super::footprint::contains(&pts, Vec2::new(t.coord(i), t.coord(j))) {
                    t.ground[i * n + j] = class;
                }
            }
        }
    }
    for r in &data.roads {
        let class = match r.kind.as_str() {
            "pedestrian" | "living_street" => ground::PLAZA,
            "path" => ground::PATH,
            _ => ground::ROAD,
        };
        for s in r.pts.windows(2) {
            let (a, c) = (v2(&s[0]), v2(&s[1]));
            let hw = r.w * 0.5;
            let (lo, hi) = (a.min(c) - Vec2::splat(hw), a.max(c) + Vec2::splat(hw));
            for i in idx(lo.x)..=idx(hi.x) {
                for j in idx(lo.y)..=idx(hi.y) {
                    let p = Vec2::new(t.coord(i), t.coord(j));
                    let d = (p - a).reject_from(c - a).length();
                    let along = (p - a).dot((c - a).normalize_or_zero());
                    let inside = if along < 0.0 { p.distance(a) < hw } else if along > a.distance(c) { p.distance(c) < hw } else { d < hw };
                    // Paths don't cross water (a bridge's path would sink it).
                    if inside && !(class == ground::PATH && t.ground[i * n + j] == ground::WATER) {
                        t.ground[i * n + j] = class;
                        if class == ground::ROAD {
                            t.road_d[i * n + j] = 0.0;
                        }
                    }
                }
            }
        }
    }
}

/// Every mapped building but those where the inner circle's blocks stand, as its real outline.
/// White for the colonial blocks and offices, the rest in Delhi's plasters and red sandstone.
/// In the old city and the village some of the houses (square ones of up to three storeys) are
/// built to go inside instead: rooms, stairs to each floor and the roof, a door onto the street
/// (crate::map::india::house). Their outlines are returned, to keep trees and finds out of them.
fn footprints(b: &mut Builder, rng: &mut SmallRng, data: &Data, ring: &super::cp::Ring, id: super::MapId) -> Vec<Vec<Vec2>> {
    let palette = [Mat::Plaster, Mat::Plaster, Mat::PlasterOchre, Mat::Sandstone, Mat::RedSandstone, Mat::Brick];
    // The old city's and the village's houses: limewashed in white, ochre and blue, or bare brick.
    let houses = [Mat::Plaster, Mat::PlasterOchre, Mat::PlasterBlue, Mat::Brick, Mat::PlasterOchre];
    // About 110 chests a map, however many buildings it has.
    let in_play = data.buildings.iter().filter(|b| b.pts.iter().map(v2).sum::<Vec2>().abs().max_element() < (HALF - 8.0) * b.pts.len() as f32).count();
    // Where houses are entered, their own chests (about 1.4 a house) make up the rest.
    let lived_in = matches!(id, super::MapId::Chandni | super::MapId::HauzKhas);
    let chest_odds = ((if lived_in { 60.0 } else { 110.0 }) / in_play.max(1) as f64).min(0.5);
    let mut entered = vec![];
    let outlines: Vec<Vec<Vec2>> = data.buildings.iter().map(|b| b.pts.iter().map(v2).collect()).collect();
    let mut halls = 0;
    for bld in &data.buildings {
        let pts: Vec<Vec2> = bld.pts.iter().map(v2).collect();
        let c = pts.iter().sum::<Vec2>() / pts.len() as f32;
        let r = c.distance(ring.center.xz());
        if r > ring.ri - 10.0 && r < ring.ro + 8.0 {
            continue;
        }
        let floors = (bld.h / FLOOR_H).round() as u32;
        if lived_in
            && matches!(bld.kind.as_str(), "infill" | "house" | "residential" | "apartments" | "yes")
            && (1..=3).contains(&floors)
            && c.abs().max_element() < HALF - 6.0
            && entered.len() < MAX_HOUSES
            && rng.random_bool(0.4)
            && let Some((front, w, d)) = enterable(&pts, b.map)
        {
            let yaw = front.x.atan2(front.y);
            let mat = [Mat::Plaster, Mat::PlasterOchre, Mat::PlasterBlue, Mat::PlasterOchre][rng.random_range(0..4)];
            super::india::house(b, rng, Frame::new(Vec3::new(c.x, 0.0, c.y), yaw), w, d, floors, mat);
            entered.push(pts);
            continue;
        }
        // Ruins (Hauz Khas's madrasa and tombs): their own arcades, not a city block's shops.
        if bld.kind == "ruins" {
            let h = ruin(b, rng, &pts, bld.h);
            b.map.footprints.push(Footprint { pts, base: h, top: h + 0.55, mat: Mat::Granite });
            continue;
        }
        let mat = match bld.kind.as_str() {
            "commercial" | "retail" | "cp" => Mat::Plaster,
            "mosque" => Mat::RedSandstone,
            "temple" => Mat::Sandstone,
            "ruins" => Mat::Granite,
            "infill" => houses[rng.random_range(0..houses.len())],
            _ => palette[rng.random_range(0..palette.len())],
        };
        // A kiosk or a guard hut mapped with a tall building's levels would stand as a thin
        // tower: anything under 4.5 m across is one storey.
        let top = if narrow(&pts) < 4.5 { bld.h.min(3.6) } else { bld.h };
        let mut base = bld.base.unwrap_or(0.0);
        // Most bigger buildings open at the street: a hall on the ground floor, the building's
        // mass standing on it.
        if base == 0.0
            && halls < MAX_HALLS
            && top >= HALL + 3.0
            && narrow(&pts) >= 6.0
            && c.abs().max_element() < HALF - 10.0
            && !matches!(bld.kind.as_str(), "mosque" | "temple" | "ruins")
        {
            hall(b, rng, &pts, &outlines);
            base = HALL;
            halls += 1;
        }
        b.map.footprints.push(Footprint { pts, base, top, mat });
        // A chest at the foot of every other building inside the play area, on the pavement.
        if c.abs().max_element() < HALF - 8.0 && rng.random_bool(chest_odds) {
            let k = rng.random_range(0..bld.pts.len());
            let (p, q) = (v2(&bld.pts[k]), v2(&bld.pts[(k + 1) % bld.pts.len()]));
            let mid = (p + q) * 0.5;
            let out = (mid - c).normalize_or(Vec2::X);
            let at = mid + out * 1.4;
            let yaw = (-out.y).atan2(out.x) + PI / 2.0;
            b.chest(Frame::new(Vec3::ZERO, 0.0), Vec3::new(at.x, 0.0, at.y), yaw, ChestKind::Peti);
        }
    }
    entered
}

/// The ground-floor hall's height, to the underside of the building above it.
const HALL: f32 = 4.2;
/// Halls a map, at most: each is a hundred-odd pieces.
const MAX_HALLS: usize = 60;

/// A building's ground floor as a hall to go into (the building's mass stands on it, from `HALL`
/// up): rusticated stone walls along its outline, with arches and shop windows where it faces the
/// street (blank where it backs onto a neighbour), a signboard over each; inside, a grid of
/// columns, tube lights, counters and clutter to take cover behind, a chest and finds.
fn hall(b: &mut Builder, rng: &mut SmallRng, pts: &[Vec2], others: &[Vec<Vec2>]) {
    const T: f32 = 0.3;
    let w = Frame::new(Vec3::ZERO, 0.0);
    let n = pts.len();
    let ccw = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() > 0.0;
    let outward = |e: Vec2| if ccw { Vec2::new(e.y, -e.x) } else { Vec2::new(-e.y, e.x) }.normalize_or_zero();
    // The wall's middle line: the outline moved in by half a wall and 5 cm (its face just inside
    // the building's above, so the two never share a plane), mitred at the corners.
    let inset = T * 0.5 + 0.05;
    let normals: Vec<Vec2> = (0..n).map(|i| outward(pts[(i + 1) % n] - pts[i])).collect();
    let line: Vec<Vec2> = (0..n)
        .map(|i| {
            let (n0, n1) = (normals[(i + n - 1) % n], normals[i]);
            let m = (n0 + n1).normalize_or(n1);
            pts[i] - m * (inset / m.dot(n1).max(0.5))
        })
        .collect();
    let built = |p: Vec2| others.iter().any(|o| o[0].distance(p) < 150.0 && super::footprint::contains(o, p));
    let c = pts.iter().sum::<Vec2>() / n as f32;
    // Inside, the walls limewashed one of the warm light paints (docs/style-guide.md: rooms light and
    // warm; lilac and powder blue go a deep violet in a hall's mostly sky-blue light).
    let wash = Mat::Paint([0u8, 1, 2, 4][rng.random_range(0..4)]);
    // Which sides face the street; with none (a building in a courtyard), its longest side does.
    let side_len = |i: usize| pts[i].distance(pts[(i + 1) % n]);
    let mut street: Vec<bool> = (0..n)
        .map(|i| {
            let mid = (pts[i] + pts[(i + 1) % n]) * 0.5;
            !built(mid + normals[i] * 1.5) && !built(mid + normals[i] * 3.0)
        })
        .collect();
    if !(0..n).any(|i| street[i] && side_len(i) > 3.4) {
        let longest = (0..n).max_by(|&i, &j| side_len(i).total_cmp(&side_len(j))).unwrap_or(0);
        street[longest] = true;
    }
    for i in 0..n {
        let (a, q) = (line[i], line[(i + 1) % n]);
        let len = a.distance(q);
        if len < 0.05 {
            continue;
        }
        let dir = (q - a) / len;
        let out = normals[i];
        // Run on past each end, so neighbouring walls overlap at the corners (no notch).
        let (a, q) = (a - dir * (T * 0.5 + 0.015), q + dir * (T * 0.5 + 0.015));
        let len = a.distance(q);
        let mut ops = vec![];
        if street[i] && len > 3.4 {
            let k = ((len / 5.5).floor() as usize).max(1);
            for j in 0..k {
                let at = len * (j as f32 + 0.5) / k as f32;
                let door = j % 2 == 0;
                ops.push(if door { Opening::arch(at, 2.2, 3.1) } else { Opening { at, width: 2.4, bottom: 0.9, top: 3.0 } });
                let p = a + dir * at;
                let face = p + out * (T * 0.5 + 0.06);
                b.map.signs.push(super::ShopSign { pos: Vec3::new(face.x, 3.62, face.y), facing: out, width: 2.6, height: 0.7, kind: super::SignKind::Board });
                if door {
                    b.map.entrances.push((p, out));
                } else {
                    b.map.signs.push(super::ShopSign { pos: Vec3::new(p.x, 1.95, p.y), facing: out, width: 2.4, height: 2.1, kind: super::SignKind::Glass });
                }
            }
        }
        // Up into the building above, its top hidden there.
        b.wall(w, a, q, 0.0, HALL + 0.1, T, Mat::Rustic, &ops);
        // The paint inside, with the wall's openings. (The wall runs on past its corners by half its
        // thickness; the skin is measured from the corners themselves.)
        let run = T * 0.5 + 0.015;
        let lops: Vec<Opening> = ops.iter().map(|o| Opening { at: o.at - run, ..*o }).collect();
        b.line_inside(w, a + dir * run, q - dir * run, 0.0, HALL, T, (a + q) * 0.5 - out, wash, &lops);
        // A blank wall inside is lined with shelves of goods, a bay every 3.5 m.
        if ops.is_empty() && len > 4.0 {
            let k = ((len - 1.0) / 3.5).floor() as usize;
            for j in 0..k {
                let p = a + dir * (len * 0.5 + (j as f32 - (k as f32 - 1.0) * 0.5) * 3.5) - out * (T * 0.5 + 0.02);
                let floor = b.map.terrain.height(p.x, p.y);
                super::kit::shelving(b, Vec3::new(p.x, floor, p.y), -out, 3.2, (p.x * 7.0 + p.y * 13.0) as i32 as u32);
            }
        }
    }
    // Inside: along the longest side's direction, a grid of square columns, wider apart in the
    // bigger halls (about six across the hall's breadth, never under 8 m), and a tube light by
    // every other one.
    let (i, _) = (0..n).map(|i| (i, pts[i].distance(pts[(i + 1) % n]))).fold((0, 0.0), |m, (i, l)| if l > m.1 { (i, l) } else { m });
    let u = (pts[(i + 1) % n] - pts[i]).normalize_or(Vec2::X);
    let v = u.perp();
    let clear = |p: Vec2, r: f32| super::footprint::contains(pts, p) && (0..n).all(|k| seg_dist(p, pts[k], pts[(k + 1) % n]) > r);
    let (lo, hi) = pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| {
        let l = Vec2::new((*p - c).dot(u), (*p - c).dot(v));
        (lo.min(l), hi.max(l))
    });
    let grid = ((hi - lo).min_element() / 6.0).max(8.0);
    let at = |x: f32, y: f32| c + u * x + v * y;
    let yaw_u = (-u.y).atan2(u.x);
    let mut free = vec![];
    let mut x = lo.x + grid * 0.5;
    let mut k = 0;
    while x < hi.x - 1.0 {
        let mut y = lo.y + grid * 0.5;
        while y < hi.y - 1.0 {
            let p = at(x, y);
            if clear(p, 2.0) {
                b.bx(Frame::new(Vec3::new(p.x, 0.0, p.y), yaw_u), Vec3::new(0.0, HALL * 0.5, 0.0), Vec3::new(0.6, HALL, 0.6), Mat::Stucco);
                let l = at(x + grid * 0.5, y);
                if k % 2 == 1 && clear(l, 1.0) {
                    super::kit::place(b, "fan", Vec3::new(l.x, HALL, l.y), rng.random_range(0.0..std::f32::consts::TAU));
                }
                if k % 2 == 0 && clear(l, 1.0) {
                    b.map.signs.push(super::ShopSign { pos: Vec3::new(l.x, HALL - 0.03, l.y), facing: v, width: 0.14, height: 1.2, kind: super::SignKind::Light });
                    let y = b.map.terrain.height(l.x, l.y) + 0.03;
                    b.map.signs.push(super::ShopSign { pos: Vec3::new(l.x, y, l.y), facing: v, width: 3.6, height: 3.4, kind: super::SignKind::Pool });
                }
                let f = at(x + grid * 0.5, y + grid * 0.5);
                if clear(f, 1.6) {
                    free.push(f);
                }
                k += 1;
            }
            y += grid;
        }
        x += grid;
    }
    // Warm light inside, as the style guide's rooms (the hall is shut but for its street side, and
    // the tube lights are drawn, not lit): an unseen fill under the ceiling.
    let reach = (hi - lo).max_element();
    let warm = crate::style::linear("light.room");
    b.map.lamps.push(super::Lamp {
        pos: Vec3::new(c.x, HALL - 0.8, c.y),
        color: Vec3::new(warm[0], warm[1], warm[2]),
        lumens: 600_000.0,
        range: (reach * 0.75).clamp(9.0, 18.0),
        flames: None,
    });
    // A few things to take cover behind, not a warehouse of them.
    while free.len() > 10 {
        free.swap_remove(rng.random_range(0..free.len()));
    }
    // Between the columns: counters, racks of goods, clutter, one chest a hall, finds.
    for (k, p) in free.iter().enumerate() {
        match (k + rng.random_range(0..3)) % 5 {
            0 => b.bx(Frame::new(Vec3::new(p.x, 0.0, p.y), yaw_u), Vec3::new(0.0, 0.5, 0.0), Vec3::new(2.4, 1.0, 0.8), Mat::Wood),
            // Racks back to back, stocked both sides: cover to duck round.
            3 => {
                let floor = b.map.terrain.height(p.x, p.y);
                for side in [-1.0f32, 1.0] {
                    super::kit::shelving(b, Vec3::new(p.x, floor, p.y), v * side, 2.5, k as u32 * 31 + side as u32);
                }
            }
            1 => super::kit::place(b, ["crates", "drum", "cooler", "chairs"][rng.random_range(0..4)], Vec3::new(p.x, 0.0, p.y), rng.random_range(0.0..std::f32::consts::TAU)),
            2 => b.loot(w, Vec3::new(p.x, 0.3, p.y)),
            _ => {}
        }
    }
    if let Some(p) = free.get(rng.random_range(0..free.len().max(1))).or(free.first()) {
        let p = *p + v * 1.2;
        if clear(p, 1.0) {
            b.chest(w, Vec3::new(p.x, 0.0, p.y), yaw_u, ChestKind::Peti);
        }
    }
}

/// A ruin's arcades: rubble-stone walls along its outline, an arch every few metres on every side
/// (open, as a ruined madrasa's are), piers down the middle of the wider wings, a little domed
/// chhatri now and then along the roof's longest side, and a find or two inside. Returns the
/// height its roof (the footprint, from there up) starts at.
fn ruin(b: &mut Builder, rng: &mut SmallRng, pts: &[Vec2], h: f32) -> f32 {
    const T: f32 = 0.6;
    let top = h.clamp(4.5, 6.5) - 0.55;
    let w = Frame::new(Vec3::ZERO, 0.0);
    let n = pts.len();
    let ccw = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() > 0.0;
    let outward = |e: Vec2| if ccw { Vec2::new(e.y, -e.x) } else { Vec2::new(-e.y, e.x) }.normalize_or_zero();
    let normals: Vec<Vec2> = (0..n).map(|i| outward(pts[(i + 1) % n] - pts[i])).collect();
    let inset = T * 0.5 + 0.05;
    let line: Vec<Vec2> = (0..n)
        .map(|i| {
            let (n0, n1) = (normals[(i + n - 1) % n], normals[i]);
            let m = (n0 + n1).normalize_or(n1);
            pts[i] - m * (inset / m.dot(n1).max(0.5))
        })
        .collect();
    let mut longest = (0, 0.0f32);
    for i in 0..n {
        let (a, q) = (line[i], line[(i + 1) % n]);
        let len = a.distance(q);
        if len < 0.05 {
            continue;
        }
        if len > longest.1 {
            longest = (i, len);
        }
        let dir = (q - a) / len;
        let (a, q) = (a - dir * (T * 0.5 + 0.015), q + dir * (T * 0.5 + 0.015));
        let len = a.distance(q);
        // Arches a bay of 3.6 m, piers 1.2 m between them.
        let k = ((len - 1.2) / 3.6).floor() as usize;
        let ops: Vec<Opening> = (0..k).map(|j| Opening::arch(len * 0.5 + (j as f32 - (k as f32 - 1.0) * 0.5) * 3.6, 2.4, (top - 1.2).min(3.6))).collect();
        b.wall(w, a, q, 0.0, top + 0.1, T, Mat::Granite, &ops);
        // A plinth along its foot and a moulding at the arches' springing, proud of the face.
        let out = normals[i];
        let mid = (a + q) * 0.5 + out * (T * 0.5 + 0.08);
        let yaw = (-dir.y).atan2(dir.x);
        b.deco(Frame::new(Vec3::new(mid.x, 0.0, mid.y), yaw), Vec3::new(0.0, top - 0.25, 0.0), Vec3::new(len, 0.3, 0.16), Mat::Buff);
    }
    // Piers down the middle of a wing wide enough to need them.
    let (i, len) = longest;
    let (a, q) = (line[i], line[(i + 1) % n]);
    let dir = (q - a) / len.max(1e-3);
    let inward = -normals[i];
    let breadth = (0..n).map(|k| (pts[k] - pts[i]).dot(inward)).fold(0.0f32, f32::max);
    if breadth > 8.0 {
        let k = (len / 5.0).floor() as usize;
        for j in 1..k {
            let p = a + dir * (j as f32 * len / k as f32) + inward * breadth * 0.5;
            if super::footprint::contains(pts, p) {
                b.bx(Frame::new(Vec3::new(p.x, 0.0, p.y), (-dir.y).atan2(dir.x)), Vec3::new(0.0, top * 0.5, 0.0), Vec3::new(0.9, top, 0.9), Mat::Granite);
                if j % 3 == 1 {
                    b.loot(w, Vec3::new(p.x + inward.x * 1.5, 0.3, p.y + inward.y * 1.5));
                }
            }
        }
    }
    // Chhatris along the roof over the longest side, every 18 m or so.
    let k = (len / 18.0).floor() as usize;
    for j in 0..k {
        let p = a + dir * (len * (j as f32 + 0.5) / k as f32) + inward * 2.2;
        if rng.random_bool(0.8) {
            b.chhatri(w, Vec3::new(p.x, top + 0.55 + Builder::chhatri_floor(2.6), p.y), 2.6, Mat::Buff);
        }
    }
    top
}

/// About how wide an outline is across its narrow side: the short side of the rectangle with its
/// area and perimeter.
fn narrow(pts: &[Vec2]) -> f32 {
    let n = pts.len();
    let area = (0..n).map(|k| pts[k].perp_dot(pts[(k + 1) % n])).sum::<f32>().abs() * 0.5;
    let perim: f32 = (0..n).map(|k| pts[k].distance(pts[(k + 1) % n])).sum();
    (perim - (perim * perim - 16.0 * area).max(0.0).sqrt()) / 4.0
}

/// Houses built to enter, at most, a map: each is a few hundred pieces and a chest or two.
const MAX_HOUSES: usize = 50;

/// A near-rectangular outline a house fits (7.5 to 16 m a side): the outward direction of its
/// side onto a street, lane or path (else the one with nothing built in front), that side's
/// length and the depth behind it.
fn enterable(pts: &[Vec2], map: &MapLayout) -> Option<(Vec2, f32, f32)> {
    if pts.len() != 4 {
        return None;
    }
    let edges: Vec<Vec2> = (0..4).map(|k| pts[(k + 1) % 4] - pts[k]).collect();
    if (0..4).any(|k| edges[k].normalize_or_zero().dot(edges[(k + 1) % 4].normalize_or_zero()).abs() > 0.08) {
        return None;
    }
    if edges.iter().any(|e| !(7.5..=16.0).contains(&e.length())) {
        return None;
    }
    let c = pts.iter().sum::<Vec2>() / 4.0;
    let built = |p: Vec2| map.footprints.iter().any(|f| f.pts[0].distance(p) < 60.0 && super::footprint::contains(&f.pts, p));
    let (k, _) = (0..4)
        .map(|k| {
            let mid = (pts[k] + pts[(k + 1) % 4]) / 2.0;
            let out = (mid - c).normalize_or_zero();
            let probe = mid + out * 3.0;
            let street = matches!(map.terrain.ground_at(probe.x, probe.y), Some(ground::ROAD | ground::PATH | ground::PAVEMENT));
            (k, street as u32 * 2 + !built(probe) as u32)
        })
        .max_by_key(|(k, score)| (*score, Reverse(*k)))?;
    let mid = (pts[k] + pts[(k + 1) % 4]) / 2.0;
    Some(((mid - c).normalize_or_zero(), edges[k].length(), edges[(k + 1) % 4].length()))
}

/// Delhi's trees and bushes, to the style guide (tools/blender/nature/stylised_trees.py).
pub const NEEM: &str = "models/props/stylised_tree_neem.gltf";
pub const PEEPAL: &str = "models/props/stylised_tree_peepal.gltf";
pub const GULMOHAR: &str = "models/props/stylised_tree_gulmohar.gltf";
pub const ASHOKA: &str = "models/props/stylised_tree_ashoka.gltf";
pub const LANTANA: &str = "models/props/stylised_bush_lantana.gltf";
pub const BOUGAINVILLEA: &str = "models/props/stylised_bush_bougainvillea.gltf";

/// A tree's trunk collider radius (tools/blender/nature/stylised_trees.py's trunks).
fn trunk(model: &str) -> f32 {
    match model {
        PEEPAL => 0.5,
        GULMOHAR => 0.38,
        ASHOKA => 0.26,
        _ => 0.42,
    }
}

fn tree(b: &mut Builder, rng: &mut SmallRng, p: Vec2) {
    // A mix, each its own shape: round neems, the big peepal's dome, the gulmohar's flat red-flecked
    // umbrella, the ashoka's narrow column.
    let model = [NEEM, NEEM, NEEM, PEEPAL, PEEPAL, GULMOHAR, GULMOHAR, GULMOHAR, ASHOKA, ASHOKA][rng.random_range(0..10)];
    let scale = rng.random_range(0.85..1.2);
    b.map.props.push(Prop { model, pos: Vec3::new(p.x, b.map.terrain.height(p.x, p.y), p.y), yaw: rng.random_range(0.0..std::f32::consts::TAU), scale, trunk: Some((trunk(model), 4.0)) });
}

/// In front of a hall's doorway (within 3 m out and 1.8 m either side, plus `r`): kept clear.
pub(super) fn before_door(map: &MapLayout, p: Vec2, r: f32) -> bool {
    map.entrances.iter().any(|(d, out)| {
        let v = p - *d;
        let ahead = v.dot(*out);
        ahead > -0.5 - r && ahead < 3.0 + r && v.perp_dot(*out).abs() < 1.8 + r
    })
}

/// Free of buildings, roads and the blocks' ring: where a tree or a find can go.
fn open(map: &MapLayout, ring: &super::cp::Ring, houses: &[Vec<Vec2>], p: Vec2, clear: f32) -> bool {
    let r = p.distance(ring.center.xz());
    if r > ring.ri - 6.0 && r < ring.ro + 3.0 || r < 12.0 {
        return false;
    }
    free(map, houses, p, clear)
}

/// Off the roads, water and paths, out of doorways, clear of buildings, trees and the street kit.
fn free(map: &MapLayout, houses: &[Vec<Vec2>], p: Vec2, clear: f32) -> bool {
    if matches!(map.terrain.ground_at(p.x, p.y), Some(ground::ROAD | ground::WATER | ground::PATH)) {
        return false;
    }
    if before_door(map, p, 0.0) {
        return false;
    }
    !map.footprints.iter().any(|f| {
        let c = f.pts.iter().sum::<Vec2>() / f.pts.len() as f32;
        c.distance(p) < 80.0 && (super::footprint::contains(&f.pts, p) || f.pts.iter().any(|q| q.distance(p) < clear))
    }) && !houses.iter().any(|h| h[0].distance(p) < 30.0 && (super::footprint::contains(h, p) || h.iter().any(|q| q.distance(p) < clear)))
        && !map.props.iter().any(|q| q.pos.xz().distance(p) < 5.0)
        && !map.kit.iter().any(|k| k.pos.xz().distance(p) < 2.5)
}

/// OpenStreetMap's mapped trees, Central Park and Palika Park planted round their edges and in
/// groves, and neem avenues along the circles and main roads (on the pavement beside them).
fn trees(b: &mut Builder, rng: &mut SmallRng, data: &Data, ring: &super::cp::Ring, houses: &[Vec<Vec2>]) {
    for t in &data.trees {
        let p = v2(t);
        if open(b.map, ring, houses, p, 1.5) {
            tree(b, rng, p);
        }
    }
    // Parks get groves, not forests (a lawn should stay a lawn, and a deer park would otherwise
    // be thousands): a tree to every 900 square metres, at most 150 a park and 500 a map.
    let mut planted = 0;
    for a in data.areas.iter().filter(|a| a.kind == "park") {
        let pts: Vec<Vec2> = a.pts.iter().map(v2).collect();
        let (lo, hi) = pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let lo = lo.max(Vec2::splat(-HALF - 40.0));
        let hi = hi.min(Vec2::splat(HALF + 40.0));
        if lo.x >= hi.x || lo.y >= hi.y {
            continue;
        }
        let target = (((hi - lo).x * (hi - lo).y / 900.0) as usize).min(150);
        let mut here = 0;
        for _ in 0..target * 3 {
            if here >= target || planted >= 500 {
                break;
            }
            let p = Vec2::new(rng.random_range(lo.x..hi.x), rng.random_range(lo.y..hi.y));
            if super::footprint::contains(&pts, p) && open(b.map, ring, houses, p, 2.0) {
                tree(b, rng, p);
                here += 1;
                planted += 1;
            }
        }
    }
    // Connaught Place's avenue: big neems (and ashokas) down the outer colonnade's pavement, between it and the
    // outer circle, shading the shops.
    if ring.ri > 0.0 {
        for (rad, step) in [(ring.ro + 7.0, 18.0)] {
            let n = (std::f32::consts::TAU * rad / step) as usize;
            for k in 0..n {
                let a = std::f32::consts::TAU * (k as f32 + rng.random_range(-0.15..0.15)) / n as f32;
                let p = ring.at(rad + rng.random_range(-0.6..0.6), a, 0.0).xz();
                if p.abs().max_element() < HALF - 4.0 && free(b.map, houses, p, 2.5) {
                    let scale = rng.random_range(0.95..1.2);
                    // Neems, every third an ashoka's tall column for the avenue's rhythm.
                    let model = if k % 3 == 1 { ASHOKA } else { NEEM };
                    b.map.props.push(Prop { model, pos: Vec3::new(p.x, b.map.terrain.height(p.x, p.y), p.y), yaw: rng.random_range(0.0..std::f32::consts::TAU), scale, trunk: Some((trunk(model), 4.0)) });
                }
            }
        }
    }
    for r in data.roads.iter().filter(|r| matches!(r.kind.as_str(), "secondary" | "tertiary")) {
        for s in r.pts.windows(2) {
            let (a, c) = (v2(&s[0]), v2(&s[1]));
            let len = a.distance(c);
            let side = (c - a).perp().normalize_or_zero() * (r.w * 0.5 + 2.5);
            let mut d = rng.random_range(0.0..6.0);
            while d < len {
                for sgn in [-1.0, 1.0] {
                    let p = a.lerp(c, d / len) + side * sgn;
                    if p.abs().max_element() < HALF + 40.0 && open(b.map, ring, houses, p, 2.5) {
                        tree(b, rng, p);
                    }
                }
                d += 14.0;
            }
        }
    }
}

/// Loose finds along the pavements and in the parks, and chests in the open where a map has too
/// few buildings to stand them by (India Gate's lawns).
fn street_loot(b: &mut Builder, rng: &mut SmallRng, _data: &Data, ring: &super::cp::Ring, houses: &[Vec<Vec2>]) {
    let w = Frame::new(Vec3::ZERO, 0.0);
    let mut tries = 0;
    while b.map.chests.len() < 70 && tries < 5000 {
        tries += 1;
        let p = Vec2::new(rng.random_range(-HALF + 8.0..HALF - 8.0), rng.random_range(-HALF + 8.0..HALF - 8.0));
        if open(b.map, ring, houses, p, 2.0) && !b.map.chests.iter().any(|c| c.pos.xz().distance(p) < 15.0) {
            b.chest(w, Vec3::new(p.x, b.map.terrain.height(p.x, p.y), p.y), rng.random_range(0.0..std::f32::consts::TAU), ChestKind::Peti);
        }
    }
    let mut placed = 0;
    let mut tries = 0;
    while placed < 90 && tries < 5000 {
        tries += 1;
        let p = Vec2::new(rng.random_range(-HALF + 6.0..HALF - 6.0), rng.random_range(-HALF + 6.0..HALF - 6.0));
        if open(b.map, ring, houses, p, 1.0) {
            b.loot(w, Vec3::new(p.x, b.map.terrain.height(p.x, p.y) + 0.3, p.y));
            placed += 1;
        }
    }
}
