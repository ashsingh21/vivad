//! Panga's map: Connaught Place and the head of Janpath, New Delhi, at real scale, from
//! OpenStreetMap (assets/data/maps/delhi_cp.json, written by tools/osm/delhi.py; map data (c)
//! OpenStreetMap contributors, ODbL 1.0).
//!
//! The inner circle's blocks are built by crate::map::cp at their real radii, round Central Park
//! with its flag; every other mapped building is its real outline, extruded (crate::map::footprint);
//! roads, car parks and parks are painted onto flat ground (`Terrain::ground`). Avenue trees line
//! the main roads and fill the parks. Axes: x east, z south, metres from the middle of the map.

use std::f32::consts::PI;

use bevy::prelude::*;
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
use serde::Deserialize;

use super::build::{Builder, Frame};
use super::terrain::{self, Pad, TerrainSpec, ground};
use super::{ChestKind, Footprint, MapLayout, Mat, Poi, Prop};

const DATA: &str = include_str!("../../../../assets/data/maps/delhi_cp.json");
const HALF: f32 = 250.0;
/// Ground samples per side: about a metre apart, for roads' edges.
const SAMPLES: usize = 561;

#[derive(Deserialize)]
struct Data {
    south_shift: f32,
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

pub fn generate(seed: u64) -> MapLayout {
    let data: Data = serde_json::from_str(DATA).expect("assets/data/maps/delhi_cp.json");
    let mut map = MapLayout { name: "Dilli".into(), half_extent: HALF, ..default() };
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
    let ring = ring(cp);
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut b = Builder::new(&mut map);
    boundary(&mut b);
    super::cp::build(&mut b, &mut rng, &ring);
    footprints(&mut b, &mut rng, &data, &ring);
    trees(&mut b, &mut rng, &data, &ring);
    street_loot(&mut b, &mut rng, &data, &ring);
    map.terrain.settle_under(&map.blocks);
    map.pois = data
        .pois
        .iter()
        .map(|p| Poi { name: p.name.clone(), center: Vec3::new(p.x, 0.0, p.z), radius: p.r })
        .chain([Poi { name: "Connaught Place".into(), center: cp, radius: 175.0 }])
        .collect();
    map
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
    for a in &data.areas {
        let pts: Vec<Vec2> = a.pts.iter().map(v2).collect();
        let class = match a.kind.as_str() {
            "park" | "pitch" => ground::PARK,
            "parking" => ground::PARKING,
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
        let class = if matches!(r.kind.as_str(), "pedestrian" | "living_street") { ground::PLAZA } else { ground::ROAD };
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
                    if inside {
                        t.ground[i * n + j] = class;
                        t.road_d[i * n + j] = 0.0;
                    }
                }
            }
        }
    }
}

/// Every mapped building but those where the inner circle's blocks stand, as its real outline.
/// White for the colonial blocks and offices, the rest in Delhi's plasters and red sandstone.
fn footprints(b: &mut Builder, rng: &mut SmallRng, data: &Data, ring: &super::cp::Ring) {
    let palette = [Mat::Plaster, Mat::Plaster, Mat::PlasterOchre, Mat::Sandstone, Mat::RedSandstone, Mat::Brick];
    for bld in &data.buildings {
        let pts: Vec<Vec2> = bld.pts.iter().map(v2).collect();
        let c = pts.iter().sum::<Vec2>() / pts.len() as f32;
        let r = c.distance(ring.center.xz());
        if r > ring.ri - 10.0 && r < ring.ro + 8.0 {
            continue;
        }
        let mat = match bld.kind.as_str() {
            "commercial" | "retail" | "cp" => Mat::Plaster,
            "mosque" => Mat::RedSandstone,
            "temple" => Mat::Sandstone,
            _ => palette[rng.random_range(0..palette.len())],
        };
        b.map.footprints.push(Footprint { pts, base: bld.base.unwrap_or(0.0), top: bld.h, mat });
        // A chest at the foot of every other building inside the play area, on the pavement.
        if c.abs().max_element() < HALF - 8.0 && rng.random_bool(0.5) {
            let k = rng.random_range(0..bld.pts.len());
            let (p, q) = (v2(&bld.pts[k]), v2(&bld.pts[(k + 1) % bld.pts.len()]));
            let mid = (p + q) * 0.5;
            let out = (mid - c).normalize_or(Vec2::X);
            let at = mid + out * 1.4;
            let yaw = (-out.y).atan2(out.x) + PI / 2.0;
            b.chest(Frame::new(Vec3::ZERO, 0.0), Vec3::new(at.x, 0.0, at.y), yaw, ChestKind::Peti);
        }
    }
}

fn tree(b: &mut Builder, rng: &mut SmallRng, p: Vec2) {
    let model = [super::india::TREE_NEEM, super::india::TREE_PEEPAL, super::india::TREE_NEEM][rng.random_range(0..3)];
    let scale = rng.random_range(0.85..1.2);
    b.map.props.push(Prop { model, pos: Vec3::new(p.x, 0.0, p.y), yaw: rng.random_range(0.0..std::f32::consts::TAU), scale, trunk: Some((0.25, 4.0)) });
}

/// Free of buildings, roads and the blocks' ring: where a tree or a find can go.
fn open(map: &MapLayout, ring: &super::cp::Ring, p: Vec2, clear: f32) -> bool {
    let r = p.distance(ring.center.xz());
    if r > ring.ri - 6.0 && r < ring.ro + 3.0 || r < 12.0 {
        return false;
    }
    if map.terrain.ground_at(p.x, p.y) == Some(ground::ROAD) {
        return false;
    }
    !map.footprints.iter().any(|f| {
        let c = f.pts.iter().sum::<Vec2>() / f.pts.len() as f32;
        c.distance(p) < 80.0 && (super::footprint::contains(&f.pts, p) || f.pts.iter().any(|q| q.distance(p) < clear))
    }) && !map.props.iter().any(|q| q.pos.xz().distance(p) < 5.0)
}

/// OpenStreetMap's mapped trees, Central Park and Palika Park planted round their edges and in
/// groves, and neem avenues along the circles and main roads (on the pavement beside them).
fn trees(b: &mut Builder, rng: &mut SmallRng, data: &Data, ring: &super::cp::Ring) {
    for t in &data.trees {
        let p = v2(t);
        if open(b.map, ring, p, 1.5) {
            tree(b, rng, p);
        }
    }
    for a in data.areas.iter().filter(|a| a.kind == "park") {
        let pts: Vec<Vec2> = a.pts.iter().map(v2).collect();
        let (lo, hi) = pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let target = ((hi - lo).x * (hi - lo).y / 260.0) as usize;
        for _ in 0..target * 3 {
            let p = Vec2::new(rng.random_range(lo.x..hi.x), rng.random_range(lo.y..hi.y));
            if super::footprint::contains(&pts, p) && open(b.map, ring, p, 2.0) {
                tree(b, rng, p);
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
                    if p.abs().max_element() < HALF + 40.0 && open(b.map, ring, p, 2.5) {
                        tree(b, rng, p);
                    }
                }
                d += 14.0;
            }
        }
    }
}

/// Loose finds along the pavements and in the parks.
fn street_loot(b: &mut Builder, rng: &mut SmallRng, _data: &Data, ring: &super::cp::Ring) {
    let w = Frame::new(Vec3::ZERO, 0.0);
    let mut placed = 0;
    let mut tries = 0;
    while placed < 90 && tries < 5000 {
        tries += 1;
        let p = Vec2::new(rng.random_range(-HALF + 6.0..HALF - 6.0), rng.random_range(-HALF + 6.0..HALF - 6.0));
        if open(b.map, ring, p, 1.0) {
            b.loot(w, Vec3::new(p.x, 0.3, p.y));
            placed += 1;
        }
    }
}
