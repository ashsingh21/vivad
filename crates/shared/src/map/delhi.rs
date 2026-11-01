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

use super::build::{Builder, FLOOR_H, Frame};
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
            if e <= 0.0 {
                continue;
            }
            let p = Vec2::new(t.coord(i), t.coord(j));
            // Swells: broad and mostly low (waist-high cover), now and then a knoll of 4 to 6 m.
            let swell = (terrain::fbm(p.x / 48.0, p.y / 48.0, 3, seed + 101) - 0.42).max(0.0) * 16.0 + terrain::fbm(p.x / 12.0, p.y / 12.0, 2, seed + 103) * 0.8;
            let mut h = swell * smooth(5.0, 24.0, e);
            for hill in made {
                let out = (p.distance(hill.center) - hill.plateau).max(0.0);
                h = h.max(hill.top * (1.0 - smooth(0.0, hill.skirt, out)) * smooth(1.0, 6.0, e));
            }
            // Central Park's fountain and the paths out from it stay level.
            if p.distance(c) < ring.park + 4.0 {
                let path = radials.iter().map(|(a, b)| seg_dist(p, *a, *b)).fold(f32::MAX, f32::min);
                h *= smooth(24.0, 36.0, p.distance(c)) * smooth(4.5, 12.0, path);
            }
            t.h[i * n + j] = h;
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

/// What's on the made hills: a chhatri to look out from on the big one, with a kalash (worth the
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
                b.chhatri(w, top, 4.2, Mat::Sandstone);
                b.chest(w, top, rng.random_range(0.0..std::f32::consts::TAU), ChestKind::Kalash);
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
fn india_gate(b: &mut Builder, c: Vec3) {
    let f = Frame::new(c, 0.0);
    let (depth, half_w, open) = (4.6, 10.6, 4.55);
    b.bx(f, Vec3::new(0.0, -0.1, 0.0), Vec3::new(36.0, 0.26, 44.0), Mat::Marble);
    for s in [-1.0f32, 1.0] {
        let z = s * (open + half_w) / 2.0;
        let w = half_w - open;
        // The piers, a red plinth and the buff shaft, with a band at the springing.
        b.bx(f, Vec3::new(0.0, 1.2, z), Vec3::new(depth * 2.0 + 0.6, 2.4, w + 0.6), Mat::RedSandstone);
        b.bx(f, Vec3::new(0.0, 2.4 + 8.8, z), Vec3::new(depth * 2.0, 17.6, w), Mat::Sandstone);
        b.deco(f, Vec3::new(0.0, 15.6, z), Vec3::new(depth * 2.0 + 0.4, 0.6, w + 0.4), Mat::Sandstone);
        // The arch's curve: steps corbelling in from each pier to the crown of the opening.
        for k in 0..5 {
            let y = 16.0 + k as f32 * 0.8;
            let inset = open - (open * (1.0 - (1.0 - (k as f32 + 1.0) / 5.0).powi(2)).sqrt()).max(0.0);
            b.bx(f, Vec3::new(0.0, y + 0.4, s * (open - inset / 2.0)), Vec3::new(depth * 2.0, 0.8, inset.max(0.05)), Mat::Sandstone);
        }
    }
    // The attic over the arch, its cornices, the stepped crown and the bowl.
    b.bx(f, Vec3::new(0.0, 20.0 + 6.5, 0.0), Vec3::new(depth * 2.0, 13.0, half_w * 2.0), Mat::Sandstone);
    b.deco(f, Vec3::new(0.0, 33.2, 0.0), Vec3::new(depth * 2.0 + 0.8, 0.5, half_w * 2.0 + 0.8), Mat::Sandstone);
    b.deco(f, Vec3::new(0.0, 22.0, 0.0), Vec3::new(depth * 2.0 + 0.5, 0.4, half_w * 2.0 + 0.5), Mat::Sandstone);
    b.bx(f, Vec3::new(0.0, 33.45 + 1.6, 0.0), Vec3::new(depth * 1.6, 3.2, half_w * 1.4), Mat::Sandstone);
    b.bx(f, Vec3::new(0.0, 36.65 + 1.2, 0.0), Vec3::new(depth * 1.2, 2.4, half_w * 0.9), Mat::Sandstone);
    b.cyl(f, Vec3::new(0.0, 39.05, 0.0), 3.4, 1.0, Mat::Sandstone);
    b.cyl(f, Vec3::new(0.0, 40.05, 0.0), 4.2, 1.2, Mat::Sandstone);
    b.map.pois.push(Poi { name: "India Gate".into(), center: c, radius: 30.0 });
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
        let mat = match bld.kind.as_str() {
            "commercial" | "retail" | "cp" => Mat::Plaster,
            "mosque" => Mat::RedSandstone,
            "temple" => Mat::Sandstone,
            "ruins" => Mat::Granite,
            "infill" => houses[rng.random_range(0..houses.len())],
            _ => palette[rng.random_range(0..palette.len())],
        };
        b.map.footprints.push(Footprint { pts, base: bld.base.unwrap_or(0.0), top: bld.h, mat });
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

fn tree(b: &mut Builder, rng: &mut SmallRng, p: Vec2) {
    let model = [super::india::TREE_NEEM, super::india::TREE_PEEPAL, super::india::TREE_NEEM][rng.random_range(0..3)];
    let scale = rng.random_range(0.85..1.2);
    b.map.props.push(Prop { model, pos: Vec3::new(p.x, b.map.terrain.height(p.x, p.y), p.y), yaw: rng.random_range(0.0..std::f32::consts::TAU), scale, trunk: Some((0.25, 4.0)) });
}

/// Free of buildings, roads and the blocks' ring: where a tree or a find can go.
fn open(map: &MapLayout, ring: &super::cp::Ring, houses: &[Vec<Vec2>], p: Vec2, clear: f32) -> bool {
    let r = p.distance(ring.center.xz());
    if r > ring.ri - 6.0 && r < ring.ro + 3.0 || r < 12.0 {
        return false;
    }
    if matches!(map.terrain.ground_at(p.x, p.y), Some(ground::ROAD | ground::WATER | ground::PATH)) {
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
