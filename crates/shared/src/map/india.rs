//! "Bharat" blockout: a 500 m square arena with eight points of interest.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

use super::build::{Builder, Frame, Opening, FLOOR_H, JHAROKHA, WALL_T};
use super::terrain::{self, Pad, TerrainSpec};
use super::{ChestKind, MapLayout, Mat, Poi, Prop};

const FORT_C: Vec2 = Vec2::new(125.0, -135.0);
/// The fort's hill summit height.
const FORT_TOP: f32 = 18.0;
const VILLAGES: [(Vec3, u32); 5] = [
    (Vec3::new(-40.0, 0.0, 195.0), 7),
    (Vec3::new(30.0, 0.0, -190.0), 7),
    (Vec3::new(-205.0, 0.0, 185.0), 6),
    (Vec3::new(222.0, 0.0, -10.0), 5),
    (Vec3::new(-30.0, 0.0, -95.0), 5),
];
/// Packed-earth roads linking the POIs (painted into the terrain, which is flattened along them).
const ROADS: [(Vec2, Vec2); 6] = [
    (Vec2::new(-60.0, 0.0), Vec2::new(165.0, 0.0)),
    (Vec2::new(0.0, -180.0), Vec2::new(0.0, 190.0)),
    (Vec2::new(-200.0, 5.0), Vec2::new(-60.0, 0.0)),
    (Vec2::new(0.0, 60.0), Vec2::new(-110.0, 80.0)),
    (Vec2::new(0.0, -60.0), Vec2::new(-140.0, -120.0)),
    (Vec2::new(40.0, 0.0), Vec2::new(105.0, -82.0)),
];

fn terrain_spec(seed: u64) -> TerrainSpec {
    let pad = |x: f32, z: f32, r: f32, f: f32| Pad { center: Vec2::new(x, z), radius: r, falloff: f };
    let mut pads = vec![
        pad(0.0, 0.0, 66.0, 30.0),
        pad(STEPWELL_C.x, STEPWELL_C.z, 34.0, 25.0),
        pad(-140.0, -130.0, 42.0, 30.0),
        pad(95.0, 170.0, 50.0, 25.0),
        pad(-200.0, 0.0, 62.0, 25.0),
        // The cliff's ends: the north one holds the spur stair's foot.
        pad(-180.0, 62.0, 24.0, 20.0),
        pad(-180.0, -62.0, 24.0, 20.0),
        pad(-158.0, 8.0, 24.0, 20.0),
        pad(138.0, 45.0, 42.0, 25.0), // Kashi: ghat town and its temples
    ];
    for (c, _) in VILLAGES {
        pads.push(pad(c.x, c.z, 42.0, 25.0));
    }
    TerrainSpec {
        size: 2.0 * HALF + 60.0,
        n: 311,
        seed: seed as u32,
        half_extent: HALF,
        pads,
        roads: ROADS.to_vec(),
        river: (RIVER_X.0, RIVER_X.1, RIVER_BED),
        pits: vec![(
            Vec2::new(STEPWELL_C.x - STEPWELL_L / 2.0, STEPWELL_C.z - STEPWELL_W / 2.0),
            Vec2::new(STEPWELL_C.x + STEPWELL_L / 2.0, STEPWELL_C.z + STEPWELL_W / 2.0),
            -STEPWELL_DEPTH - 2.0,
        )],
        mesas: vec![(FORT_C, Vec2::new(34.0, 28.0), FORT_TOP, 62.0)],
    }
}

pub const TREE_NEEM: &str = "models/props/tree_neem.glb";
pub const TREE_PEEPAL: &str = "models/props/tree_peepal.glb";
pub const TREE_PALM: &str = "models/props/tree_palm.glb";
pub const TREE_BANYAN: &str = "models/props/tree_banyan.glb";
pub const BUSH_LANTANA: &str = "models/props/bush_lantana.glb";
pub const BUSH_BOUGAINVILLEA: &str = "models/props/bush_bougainvillea.glb";
pub const TEMPLE_NAGARA: &str = "models/props/temple_nagara.glb";
pub const TEMPLE_DRAVIDA: &str = "models/props/temple_dravida.glb";
pub const GOPURAM: &str = "models/props/gopuram.glb";
pub const STONE_CHARIOT: &str = "models/props/stone_chariot.glb";
pub const ROCKS: [&str; 3] = ["models/props/rock_a.glb", "models/props/rock_b.glb", "models/props/rock_c.glb"];
pub const ROCKS_BASALT: [&str; 3] = ["models/props/rock_basalt_1.glb", "models/props/rock_basalt_2.glb", "models/props/rock_basalt_3.glb"];
pub const HUT_MUD: &str = "models/props/hut_mud.glb";
pub const HUT_BRICK: &str = "models/props/hut_brick.glb";
pub const STALLS: [&str; 3] = ["models/props/stall_spice.glb", "models/props/stall_brass.glb", "models/props/stall_cloth.glb"];
pub const STUPA: &str = "models/props/stupa.glb";
pub const AJANTA: &str = "models/props/ajanta.glb";
pub const MANDAPA_RUIN: &str = "models/props/mandapa_ruin.glb";
pub const STEPWELL: &str = "models/props/stepwell.glb";

/// Run `build` and hide everything it made: colliders whose art comes from a model placed separately.
fn hidden(b: &mut Builder, build: impl FnOnce(&mut Builder)) {
    let first = b.map.blocks.len();
    build(b);
    for blk in &mut b.map.blocks[first..] {
        blk.visible = false;
    }
}

/// A sloped slab collider whose top surface runs from `a` to `c` (local (z, y) in frame `f`),
/// `width` along local X.
fn slope(b: &mut Builder, f: Frame, a: Vec2, c: Vec2, width: f32, thick: f32) {
    let d = c - a;
    let len = d.length();
    // Rotation about X that takes local +Z onto the a->c direction.
    let ang = (-d.y).atan2(d.x);
    let rot = f.rot() * Quat::from_rotation_x(ang);
    let mut up = Quat::from_rotation_x(ang) * Vec3::Y;
    if up.y < 0.0 {
        up = -up;
    }
    let mid = Vec3::new(0.0, (a.y + c.y) / 2.0, (a.x + c.x) / 2.0) - up * thick / 2.0;
    b.push(f.p(mid), rot, super::Shape::Box(Vec3::new(width, thick, len)), Mat::Thatch, true, false);
}

/// A ring of wall segments (radius `r`, base `y0`), with openings `gap_half` metres either side
/// of each angle in `gaps` (angles measured from +X toward -Z).
fn ring_wall(b: &mut Builder, f: Frame, r: f32, y0: f32, h: f32, thick: f32, gaps: &[f32], gap_half: f32) {
    let da = gap_half / r;
    let mut g: Vec<f32> = gaps.to_vec();
    g.sort_by(f32::total_cmp);
    for (i, &a0) in g.iter().enumerate() {
        let a1 = if i + 1 < g.len() { g[i + 1] } else { g[0] + std::f32::consts::TAU };
        let (s0, s1) = (a0 + da, a1 - da);
        let n = ((s1 - s0) * r / 2.5).ceil().max(1.0) as i32;
        for k in 0..n {
            let (t0, t1) = (s0 + (s1 - s0) * k as f32 / n as f32, s0 + (s1 - s0) * (k + 1) as f32 / n as f32);
            let p0 = Vec2::new(r * t0.cos(), -r * t0.sin());
            let p1 = Vec2::new(r * t1.cos(), -r * t1.sin());
            let mid = (p0 + p1) / 2.0;
            let dir = p1 - p0;
            let yaw = (-dir.y).atan2(dir.x);
            let wf = f.child(Vec3::new(mid.x, y0, mid.y), yaw);
            b.bx(wf, Vec3::new(0.0, h / 2.0, 0.0), Vec3::new(dir.length() + 0.05, h, thick), Mat::Sandstone);
        }
    }
}

/// Run `build`, hide everything it made (gameplay colliders only), and show `model` instead.
fn art_over(b: &mut Builder, model: &'static str, f: Frame, scale: f32, build: impl FnOnce(&mut Builder)) {
    let first = b.map.blocks.len();
    build(b);
    for blk in &mut b.map.blocks[first..] {
        blk.visible = false;
    }
    b.map.props.push(Prop { model, pos: f.origin, yaw: f.yaw, scale, trunk: None });
}

fn prop(b: &mut Builder, model: &'static str, pos: Vec3, yaw: f32, scale: f32) {
    let trunk = match model {
        TREE_NEEM => Some((0.28, 3.0)),
        TREE_PEEPAL => Some((0.36, 3.6)),
        TREE_PALM => Some((0.2, 9.0)),
        TREE_BANYAN => Some((1.4, 5.5)),
        _ => None, // bushes are soft cover: you can hide in them, not stand on them
    }
    .map(|(r, h)| (r * scale, h * scale));
    // Sit on the terrain, sunk a little so roots/flare meet slopes.
    let pos = Vec3::new(pos.x, b.map.terrain.height(pos.x, pos.z) - 0.15, pos.z);
    b.map.props.push(Prop { model, pos, yaw, scale, trunk });
}

const HALF: f32 = 250.0;
const RIVER_X: (f32, f32) = (172.0, 196.0);
const RIVER_BED: f32 = -1.6;
const STEPWELL_C: Vec3 = Vec3::new(-110.0, 0.0, 95.0);
const STEPWELL_L: f32 = 46.0;
const STEPWELL_W: f32 = 18.0;
const STEPWELL_DEPTH: f32 = 9.0;

pub fn generate(seed: u64) -> MapLayout {
    let mut map = MapLayout { name: "Bharat".into(), half_extent: HALF, ..default() };
    map.terrain = terrain::generate(&terrain_spec(seed));
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut b = Builder::new(&mut map);

    ground(&mut b);
    river_and_ghats(&mut b, &mut rng);
    bazaar(&mut b, &mut rng);
    stepwell(&mut b, &mut rng);
    hampi(&mut b, &mut rng);
    fort(&mut b, &mut rng);
    banyan_grove(&mut b, &mut rng);
    caves_and_stupa(&mut b, &mut rng);
    for (c, n) in VILLAGES {
        village(&mut b, &mut rng, c, n);
    }
    scatter_trees(&mut b, &mut rng, 230);

    map.pois = vec![
        poi("Chandni Bazaar", 0.0, 0.0, 55.0),
        poi("Rani ki Baoli", STEPWELL_C.x, STEPWELL_C.z, 32.0),
        poi("Hampi Khandahar", -140.0, -130.0, 45.0),
        poi("Garh Qila", 125.0, -135.0, 50.0),
        poi("Kashi Ghat", 150.0, 40.0, 50.0),
        poi("Bargad Van", 95.0, 170.0, 35.0),
        poi("Ajanta Gufa", -200.0, 0.0, 50.0),
        poi("Gaon", -40.0, 195.0, 30.0),
    ];
    map
}

fn poi(name: &str, x: f32, z: f32, r: f32) -> Poi {
    Poi { name: name.into(), center: Vec3::new(x, 0.0, z), radius: r }
}

fn world() -> Frame {
    Frame::new(Vec3::ZERO, 0.0)
}

// ------------------------------------------------------------------ terrain

fn ground(b: &mut Builder) {
    // The terrain itself is the heightfield in `map.terrain`; only the arena boundary is built here.
    for (c, s) in [
        (Vec3::new(HALF + 2.0, 60.0, 0.0), Vec3::new(4.0, 160.0, 2.0 * HALF + 8.0)),
        (Vec3::new(-HALF - 2.0, 60.0, 0.0), Vec3::new(4.0, 160.0, 2.0 * HALF + 8.0)),
        (Vec3::new(0.0, 60.0, HALF + 2.0), Vec3::new(2.0 * HALF + 8.0, 160.0, 4.0)),
        (Vec3::new(0.0, 60.0, -HALF - 2.0), Vec3::new(2.0 * HALF + 8.0, 160.0, 4.0)),
    ] {
        b.push(c, Quat::IDENTITY, super::Shape::Box(s), Mat::Granite, true, false);
    }
}

// ------------------------------------------------------------------ Kashi Ghat

fn river_and_ghats(b: &mut Builder, rng: &mut SmallRng) {
    let (x0, x1) = RIVER_X;
    b.push(Vec3::new((x0 + x1) / 2.0, -0.55, 0.0), Quat::IDENTITY,
           super::Shape::Box(Vec3::new(x1 - x0, 0.02, 2.0 * HALF + 60.0)), Mat::Water, false, true);

    // Ghats: broad stone steps down to the water on the west bank, with shrines on top.
    for zc in [-10.0, 30.0, 70.0, 110.0] {
        let f = Frame::new(Vec3::new(x0, 0.0, zc), PI); // +X points west, away from the river
        let run = 7.0;
        b.stairs(f, Vec3::new(-run, RIVER_BED, 0.0), run, -RIVER_BED, 24.0, Mat::Sandstone);
        b.span(f, Vec3::new(0.0, -0.4, -12.0), Vec3::new(4.0, 0.0, 12.0), Mat::Sandstone);
        b.chhatri(f, Vec3::new(2.0, 0.0, -9.0), 3.2, Mat::Sandstone);
        b.chhatri(f, Vec3::new(2.0, 0.0, 9.0), 3.2, Mat::Sandstone);
        b.loot(f, Vec3::new(-3.0, -0.8, 0.0));
    }
    // East bank ghats so the river is never a trap.
    for zc in [-90.0, 20.0, 140.0] {
        let f = Frame::new(Vec3::new(x1, 0.0, zc), 0.0);
        b.stairs(f, Vec3::new(-6.0, RIVER_BED, 0.0), 6.0, -RIVER_BED, 14.0, Mat::Sandstone);
    }
    // Stone bridge.
    let bf = Frame::new(Vec3::new((x0 + x1) / 2.0, 0.0, -45.0), 0.0);
    b.span(bf, Vec3::new(-(x1 - x0) / 2.0 - 2.0, -0.8, -3.5), Vec3::new((x1 - x0) / 2.0 + 2.0, 0.0, 3.5), Mat::Sandstone);
    for s in [-1.0, 1.0] {
        b.span(bf, Vec3::new(-(x1 - x0) / 2.0 - 2.0, 0.0, s * 3.5 - 0.25), Vec3::new((x1 - x0) / 2.0 + 2.0, 1.0, s * 3.5 + 0.25), Mat::Sandstone);
    }
    for i in 0..3 {
        b.cyl(bf, Vec3::new(-8.0 + 8.0 * i as f32, RIVER_BED, 0.0), 1.6, -RIVER_BED - 0.8, Mat::Sandstone);
    }
    // Boats for cover.
    for _ in 0..7 {
        let z = rng.random_range(-200.0..200.0);
        let x = rng.random_range(x0 + 4.0..x1 - 4.0);
        let f = Frame::new(Vec3::new(x, RIVER_BED, z), rng.random_range(-0.3..0.3) + FRAC_PI_2);
        b.bx(f, Vec3::new(0.0, 0.6, 0.0), Vec3::new(7.0, 1.2, 2.2), Mat::Wood);
        b.bx(f, Vec3::new(0.0, 1.7, 0.0), Vec3::new(2.4, 0.1, 2.4), Mat::Cloth);
    }
    // Riverside town: houses and temples along the ghats.
    let mut z = -40.0;
    while z < 130.0 {
        let x = rng.random_range(140.0..158.0);
        let wdt = rng.random_range(8.0..11.0);
        let dep = rng.random_range(7.0..10.0);
        let floors = rng.random_range(1..=3);
        let f = Frame::new(Vec3::new(x, 0.0, z), FRAC_PI_2); // door faces the river (east)
        let mat = pick_plaster(rng);
        house(b, rng, f, wdt, dep, floors, mat);
        z += wdt + rng.random_range(3.0..7.0);
    }
    temple(b, rng, Frame::new(Vec3::new(125.0, 0.0, 15.0), FRAC_PI_2), 1.0);
    temple(b, rng, Frame::new(Vec3::new(128.0, 0.0, 85.0), FRAC_PI_2), 0.8);
}

// ------------------------------------------------------------------ Chandni Bazaar

fn bazaar(b: &mut Builder, rng: &mut SmallRng) {
    // A long street (along X) of havelis, a cross street, and a chowk with a temple.
    for side in [-1.0f32, 1.0] {
        let mut x = -58.0;
        while x < 58.0 {
            let wdt = rng.random_range(9.0..13.0);
            if x < 16.0 && x + wdt > -16.0 {
                x = 16.0; // leave the chowk open
                continue;
            }
            let dep = rng.random_range(9.0..12.0);
            let floors = rng.random_range(2..=3);
            let zc = side * (7.0 + dep / 2.0);
            let yaw = if side > 0.0 { PI } else { 0.0 }; // doors face the street
            let f = Frame::new(Vec3::new(x + wdt / 2.0, 0.0, zc), yaw);
            let mat = pick_plaster(rng);
        house(b, rng, f, wdt, dep, floors, mat);
            x += wdt + rng.random_range(1.5..4.0);
        }
    }
    // Market stalls with cloth awnings down the street.
    for i in 0..14 {
        let x = -55.0 + i as f32 * 8.5 + rng.random_range(-1.0..1.0);
        if x.abs() < 15.0 {
            continue;
        }
        let z = if i % 2 == 0 { -4.5 } else { 4.5 };
        // The counter faces the street.
        let yaw = if z > 0.0 { PI } else { 0.0 };
        stall(b, Frame::new(Vec3::new(x, 0.0, z), yaw), STALLS[i % STALLS.len()]);
        if rng.random_bool(0.4) {
            b.loot(world(), Vec3::new(x, 0.3, z * 0.6));
        }
    }
    // Chowk: raised platform, small temple, well.
    b.span(world(), Vec3::new(-10.0, 0.0, -10.0), Vec3::new(10.0, 0.4, 10.0), Mat::Sandstone);
    temple(b, rng, Frame::new(Vec3::new(0.0, 0.4, -2.0), 0.0), 0.65);
    b.cyl(world(), Vec3::new(6.5, 0.4, 6.5), 1.3, 0.9, Mat::Sandstone);
    b.chest(world(), Vec3::new(-6.5, 0.4, 6.5), 0.3, ChestKind::Peti);
}

/// Market stall: bamboo posts, a cloth awning and a counter of goods (`model`).
fn stall(b: &mut Builder, f: Frame, model: &'static str) {
    hidden(b, |b| {
        for (x, z) in [(-1.6, -1.2), (1.6, -1.2), (1.6, 1.2), (-1.6, 1.2)] {
            b.cyl(f, Vec3::new(x, 0.0, z), 0.07, 2.7, Mat::Wood);
        }
        b.bx(f, Vec3::new(0.0, 2.68, 0.0), Vec3::new(3.6, 0.06, 2.8), Mat::Cloth);
        b.bx(f, Vec3::new(0.0, 0.45, 0.0), Vec3::new(3.0, 0.9, 1.6), Mat::Wood);
    });
    b.ornament(f, Vec3::ZERO, 0.0, model, Vec3::ONE);
}

// ------------------------------------------------------------------ buildings

fn pick_plaster(rng: &mut SmallRng) -> Mat {
    [Mat::Plaster, Mat::PlasterOchre, Mat::PlasterBlue, Mat::Sandstone, Mat::Plaster][rng.random_range(0..5)]
}

/// Multi-storey house / haveli. Frame origin at the ground-floor centre; door faces local +Z.
pub fn house(b: &mut Builder, rng: &mut SmallRng, f: Frame, w: f32, d: f32, floors: u32, mat: Mat) {
    let hw = w / 2.0;
    let hd = d / 2.0;
    let t = WALL_T;
    // Two stair lanes along the back wall, used on alternate floors.
    let run = (w - 3.4).clamp(4.0, 6.5);
    let lane_w = 1.15;
    let sx = -hw + t + 0.4;
    let lane_z = |k: u32| -hd + t + 0.15 + lane_w / 2.0 + (k % 2) as f32 * (lane_w + 0.1);

    // Ground-floor paving (visual; the terrain below is the collider).
    b.push(f.p(Vec3::new(0.0, 0.03, 0.0)), f.rot(), super::Shape::Box(Vec3::new(w - 0.1, 0.06, d - 0.1)), Mat::Sandstone, false, true);
    for k in 0..floors {
        let y0 = k as f32 * FLOOR_H;
        let win = |len: f32| -> Vec<Opening> {
            let n = ((len - 2.0) / 3.2).floor().max(1.0) as i32;
            (0..n).map(|i| Opening::window((i as f32 + 0.5) * len / n as f32)).collect()
        };
        let mut front = win(w);
        if k == 0 {
            front.retain(|o| (o.at - w / 2.0).abs() > 1.6);
            front.push(Opening::door(w / 2.0));
        }
        let corners = [Vec2::new(-hw, hd), Vec2::new(hw, hd), Vec2::new(hw, -hd), Vec2::new(-hw, -hd)];
        // A jharokha (projecting balcony window) replaces the central upper-floor window of wide havelis.
        let jharokha = k > 0 && w >= 9.0;
        if jharokha {
            front.retain(|o| (o.at - w / 2.0).abs() > 1.4);
            front.push(Opening { at: w / 2.0, width: 1.2, bottom: 0.9, top: 2.4 });
        }
        b.wall(f, corners[0], corners[1], y0, FLOOR_H, t, mat, &front);
        b.wall(f, corners[1], corners[2], y0, FLOOR_H, t, mat, &win(d));
        b.wall(f, corners[2], corners[3], y0, FLOOR_H, t, mat, &[]);
        b.wall(f, corners[3], corners[0], y0, FLOOR_H, t, mat, &win(d));
        let trim = trim_for(mat);
        let arched: Vec<Opening> = front.iter().copied().filter(|o| !(jharokha && (o.at - w / 2.0).abs() < 0.1)).collect();
        b.arch_openings(f, corners[0], corners[1], y0, t, &arched);
        b.arch_openings(f, corners[1], corners[2], y0, t, &win(d));
        b.arch_openings(f, corners[3], corners[0], y0, t, &win(d));
        if jharokha {
            // Balcony floor level with the window sill.
            b.ornament(f, Vec3::new(0.0, y0 + 0.26, hd), 0.0, JHAROKHA, Vec3::ONE);
        }
        // String course marking each floor.
        if k > 0 {
            for (a, c) in [(corners[0], corners[1]), (corners[1], corners[2]), (corners[2], corners[3]), (corners[3], corners[0])] {
                band(b, f, a, c, y0 - 0.08, 0.16, t + 0.12, trim);
            }
        }
        if k == 0 {
            b.door(f, corners[0], corners[1], w / 2.0, 0.0);
        }
        // Stairs from this floor up to the next (or to the roof).
        // Switchback: even floors climb +X in lane A, odd floors climb -X in lane B, so each
        // flight starts right beside where the previous one arrived.
        let lz = lane_z(k);
        if k % 2 == 0 {
            b.stairs(f, Vec3::new(sx, y0, lz), run, FLOOR_H, lane_w, Mat::Wood);
        } else {
            b.stairs(f.child(Vec3::new(sx + run, 0.0, 0.0), PI), Vec3::new(0.0, y0, -lz), run, FLOOR_H, lane_w, Mat::Wood);
        }
        // Slab above, with the whole flight's footprint open for headroom.
        let hole = (Vec2::new(sx - 0.05, lz - lane_w / 2.0 - 0.05), Vec2::new(sx + run + 0.05, lz + lane_w / 2.0 + 0.05));
        b.slab_with_hole(f, Vec2::new(-hw, -hd), Vec2::new(hw, hd), y0 + FLOOR_H, 0.25, Some(hole), mat);
        // Loot: a chest in a front corner, floor loot mid-room.
        if rng.random_bool(0.55) {
            let cx = if rng.random_bool(0.5) { -hw + 1.2 } else { hw - 1.2 };
            b.chest(f, Vec3::new(cx, y0, hd - 1.1), PI, ChestKind::Peti);
        }
        if rng.random_bool(0.5) {
            b.loot(f, Vec3::new(rng.random_range(-hw + 1.5..hw - 1.5), y0 + 0.3, 0.8));
        }
    }
    // Roof: parapet with kangura merlons over a bracketed chhajja eave; plinth band at the base.
    let ry = floors as f32 * FLOOR_H;
    let trim = trim_for(mat);
    for (a, c) in [
        (Vec2::new(-hw, hd), Vec2::new(hw, hd)),
        (Vec2::new(hw, hd), Vec2::new(hw, -hd)),
        (Vec2::new(hw, -hd), Vec2::new(-hw, -hd)),
        (Vec2::new(-hw, -hd), Vec2::new(-hw, hd)),
    ] {
        b.wall(f, a, c, ry, 1.0, t, mat, &[]);
        band(b, f, a, c, ry - 0.22, 0.12, t + 0.9, trim);
        corbels(b, f, a, c, ry - 0.62, t, trim);
        b.merlons(f, a, c, ry + 1.0, t, 0.9, Vec2::new(0.34, 0.32), mat);
        band(b, f, a, c, 0.0, 0.42, t + 0.16, trim);
    }
    // Barsati: a rooftop room over the stair head, opening onto the terrace. It breaks up the
    // box silhouette and roofs the last flight.
    let barsati = rng.random_bool(if floors <= 2 { 0.6 } else { 0.35 });
    if barsati {
        let bw = (run + 1.4).max(w * 0.5).min(w - 2.0 * t - 1.0);
        let bd = 2.0 * lane_w + 1.0;
        let (x0, z0) = (-hw + t, -hd + t);
        let (x1, z1) = (x0 + bw, z0 + bd);
        let rh = 2.8;
        let door = [Opening::door(bw / 2.0)];
        let side = [Opening::window(bd / 2.0)];
        b.wall(f, Vec2::new(x0, z1), Vec2::new(x1, z1), ry, rh, t, mat, &door);
        b.arch_openings(f, Vec2::new(x0, z1), Vec2::new(x1, z1), ry, t, &door);
        b.wall(f, Vec2::new(x1, z1), Vec2::new(x1, z0), ry, rh, t, mat, &side);
        b.arch_openings(f, Vec2::new(x1, z1), Vec2::new(x1, z0), ry, t, &side);
        b.wall(f, Vec2::new(x1, z0), Vec2::new(x0, z0), ry, rh, t, mat, &[]);
        b.wall(f, Vec2::new(x0, z0), Vec2::new(x0, z1), ry, rh, t, mat, &[]);
        b.slab_with_hole(f, Vec2::new(x0 - t / 2.0, z0 - t / 2.0), Vec2::new(x1 + t / 2.0, z1 + t / 2.0), ry + rh + 0.2, 0.2, None, mat);
        for (a, c) in [(Vec2::new(x0, z1), Vec2::new(x1, z1)), (Vec2::new(x1, z1), Vec2::new(x1, z0))] {
            band(b, f, a, c, ry + rh - 0.1, 0.12, t + 0.8, trim);
            corbels(b, f, a, c, ry + rh - 0.5, t, trim);
            b.merlons(f, a, c, ry + rh + 0.2, t, 0.7, Vec2::new(0.26, 0.26), mat);
        }
    }
    if w > 9.5 && rng.random_bool(if barsati { 0.25 } else { 0.5 }) {
        b.chhatri(f, Vec3::new(hw - 2.2, ry, hd - 2.2), 2.8, mat);
    }
    if rng.random_bool(0.3) {
        b.loot(f, Vec3::new(0.0, ry + 0.3, 0.0));
    }
}

/// Trim stone that contrasts with a wall finish.
fn trim_for(wall: Mat) -> Mat {
    match wall {
        Mat::Sandstone => Mat::RedSandstone,
        Mat::RedSandstone => Mat::Sandstone,
        _ => Mat::Sandstone,
    }
}

/// Continuous horizontal band along a wall run (string course, cornice, plinth).
fn band(b: &mut Builder, f: Frame, a: Vec2, c: Vec2, y: f32, height: f32, depth: f32, mat: Mat) {
    let dir = c - a;
    let len = dir.length();
    let yaw = (-dir.y).atan2(dir.x);
    let wf = f.child(Vec3::new(a.x, y, a.y), yaw);
    b.deco(wf, Vec3::new(len / 2.0, height / 2.0, 0.0), Vec3::new(len + depth - 0.2, height, depth), mat);
}

/// Stepped stone brackets under an eave, on both faces of the wall.
fn corbels(b: &mut Builder, f: Frame, a: Vec2, c: Vec2, y: f32, thick: f32, mat: Mat) {
    let dir = c - a;
    let len = dir.length();
    let yaw = (-dir.y).atan2(dir.x);
    let wf = f.child(Vec3::new(a.x, y, a.y), yaw);
    let n = (len / 1.4).floor() as i32;
    for i in 0..n {
        let x = (i as f32 + 0.5) * len / n as f32;
        b.deco(wf, Vec3::new(x, 0.28, 0.0), Vec3::new(0.16, 0.2, thick + 0.75), mat);
        b.deco(wf, Vec3::new(x, 0.12, 0.0), Vec3::new(0.14, 0.16, thick + 0.45), mat);
    }
}

/// Nagara-style temple: plinth, pillared mandapa, sanctum with a shikhara tower.
/// The blocks below are the gameplay colliders; the visible art is the `TEMPLE_NAGARA` model.
fn temple(b: &mut Builder, rng: &mut SmallRng, f: Frame, scale: f32) {
    art_over(b, TEMPLE_NAGARA, f, scale, |b| temple_blocks(b, rng, f, scale));
}

/// The same footprint in the Vijayanagara (Dravidian) style, for Hampi.
fn temple_dravida(b: &mut Builder, rng: &mut SmallRng, f: Frame, scale: f32) {
    art_over(b, TEMPLE_DRAVIDA, f, scale, |b| temple_blocks(b, rng, f, scale));
}

fn temple_blocks(b: &mut Builder, rng: &mut SmallRng, f: Frame, scale: f32) {
    let s = scale;
    let plinth_h = 1.2 * s;
    b.span(f, Vec3::new(-7.0 * s, 0.0, -9.0 * s), Vec3::new(7.0 * s, plinth_h, 9.0 * s), Mat::Sandstone);
    // Front steps (climbing toward -Z into the temple).
    b.stairs(f.child(Vec3::new(0.0, 0.0, 9.0 * s + 3.0 * s), FRAC_PI_2), Vec3::ZERO, 3.0 * s, plinth_h, 4.0 * s, Mat::Sandstone);
    // Mandapa pillars and roof.
    for x in [-4.5, -1.5, 1.5, 4.5] {
        for z in [2.0, 5.0, 8.0] {
            b.cyl(f, Vec3::new(x * s, plinth_h, z * s), 0.35 * s, 4.0 * s, Mat::Sandstone);
        }
    }
    b.span(f, Vec3::new(-6.0 * s, plinth_h + 4.0 * s, 0.5 * s), Vec3::new(6.0 * s, plinth_h + 4.6 * s, 9.0 * s), Mat::Sandstone);
    b.tower(f, Vec3::new(0.0, plinth_h + 4.6 * s, 4.7 * s), 7.0 * s, 4.0 * s, 2.5 * s, 3, 0.5, Mat::Sandstone);
    // Sanctum (garbhagriha) with doorway.
    let sf = f.child(Vec3::new(0.0, plinth_h, -4.0 * s), 0.0);
    let h = 5.0 * s;
    let r = 4.5 * s;
    b.wall(sf, Vec2::new(-r, r), Vec2::new(r, r), 0.0, h, 0.6, Mat::Sandstone, &[Opening::arch(r, 2.0 * s.max(0.8), 3.0 * s.max(0.8))]);
    b.wall(sf, Vec2::new(r, r), Vec2::new(r, -r), 0.0, h, 0.6, Mat::Sandstone, &[]);
    b.wall(sf, Vec2::new(r, -r), Vec2::new(-r, -r), 0.0, h, 0.6, Mat::Sandstone, &[]);
    b.wall(sf, Vec2::new(-r, -r), Vec2::new(-r, r), 0.0, h, 0.6, Mat::Sandstone, &[]);
    b.span(sf, Vec3::new(-r, h, -r), Vec3::new(r, h + 0.5, r), Mat::Sandstone);
    b.tower(sf, Vec3::new(0.0, h + 0.5, 0.0), 2.0 * r, 2.0 * r, 14.0 * s, 7, 0.65, Mat::Sandstone);
    let kind = if rng.random_bool(0.6) { ChestKind::Kalash } else { ChestKind::Peti };
    b.chest(sf, Vec3::new(0.0, 0.0, -r + 1.4), 0.0, kind);
}

// ------------------------------------------------------------------ Rani ki Baoli

fn stepwell(b: &mut Builder, rng: &mut SmallRng) {
    // Local frame: long axis X. Entrance at +X (ground level), descending west to the well.
    let f = Frame::new(STEPWELL_C, 0.0);
    let hl = STEPWELL_L / 2.0;
    let hw = STEPWELL_W / 2.0;
    let dp = STEPWELL_DEPTH;
    // Retaining walls.
    b.span(f, Vec3::new(-hl - 1.0, -dp - 2.0, -hw - 1.0), Vec3::new(hl + 1.0, 0.0, -hw), Mat::Sandstone);
    b.span(f, Vec3::new(-hl - 1.0, -dp - 2.0, hw), Vec3::new(hl + 1.0, 0.0, hw + 1.0), Mat::Sandstone);
    b.span(f, Vec3::new(-hl - 1.0, -dp - 2.0, -hw), Vec3::new(-hl, 0.0, hw), Mat::Sandstone);
    b.span(f, Vec3::new(-hl, -dp - 2.0, -hw), Vec3::new(hl, -dp, hw), Mat::Sandstone);
    // Three flights with pillared pavilions over the landings (their roofs are walkable bridges).
    let lvl = dp / 3.0;
    let flight = 9.0;
    let landing = 4.0;
    let mut x = hl; // top of the first flight
    for i in 0..3 {
        let top = -(i as f32) * lvl;
        let bottom = top - lvl;
        b.stairs(f.child(Vec3::new(x - flight, 0.0, 0.0), 0.0), Vec3::new(0.0, bottom, 0.0), flight, lvl, 7.0, Mat::Sandstone);
        // Fill under the stairs.
        b.span(f, Vec3::new(x - flight, -dp, -3.5), Vec3::new(x, bottom, 3.5), Mat::Sandstone);
        x -= flight;
        // Landing platform.
        b.span(f, Vec3::new(x - landing, -dp, -hw), Vec3::new(x, bottom, hw), Mat::Sandstone);
        if i < 2 {
            // Pavilion: pillars from the landing up to ground level, roof slab = bridge across the
            // well. The carved pillars and the bridge's fascia are in the stepwell model.
            hidden(b, |b| {
                for px in [x - landing + 0.6, x - 0.6] {
                    for pz in [-hw + 0.8, -3.0, 3.0, hw - 0.8] {
                        b.cyl(f, Vec3::new(px, bottom, pz), 0.3, -bottom - 0.5, Mat::Sandstone);
                    }
                }
                b.span(f, Vec3::new(x - landing, -0.5, -hw), Vec3::new(x, 0.0, hw), Mat::Sandstone);
            });
            b.chest(f, Vec3::new(x - landing / 2.0, bottom, hw - 1.6), PI, ChestKind::Peti);
        }
        // Side galleries along the long walls at this level.
        b.span(f, Vec3::new(x, bottom, -hw), Vec3::new(x + flight, bottom + 0.3, -3.5), Mat::Sandstone);
        b.span(f, Vec3::new(x, bottom, 3.5), Vec3::new(x + flight, bottom + 0.3, hw), Mat::Sandstone);
        x -= landing;
    }
    // The well shaft at the west end.
    b.cyl(f, Vec3::new(-hl + 4.5, -dp, 0.0), 3.6, 1.0, Mat::Sandstone);
    b.push(f.p(Vec3::new(-hl + 4.5, -dp + 0.6, 0.0)), Quat::IDENTITY,
           super::Shape::Cylinder { radius: 3.0, height: 0.05 }, Mat::Water, false, true);
    b.chest(f, Vec3::new(-hl + 9.0, -dp, -hw + 1.5), 0.0, ChestKind::Kalash);
    for _ in 0..4 {
        b.loot(f, Vec3::new(rng.random_range(-hl + 3.0..hl - 3.0), 0.3, rng.random_range(-hw - 6.0..-hw - 2.0)));
    }
    // Carved lining: tiers of niches on the walls (stepwell.py).
    b.map.props.push(Prop { model: STEPWELL, pos: f.origin, yaw: f.yaw, scale: 1.0, trunk: None });
    // Surrounding chhatris on the rim.
    for (cx, cz) in [(hl + 3.0, hw + 3.0), (hl + 3.0, -hw - 3.0), (-hl - 3.0, hw + 3.0), (-hl - 3.0, -hw - 3.0)] {
        b.chhatri(f, Vec3::new(cx, 0.0, cz), 3.5, Mat::Sandstone);
    }
}

// ------------------------------------------------------------------ Hampi Khandahar

fn hampi(b: &mut Builder, rng: &mut SmallRng) {
    let c = Vec3::new(-140.0, 0.0, -130.0);
    let f = Frame::new(c, 0.25);
    // Granite boulder piles.
    let rock = |b: &mut Builder, rng: &mut SmallRng, p: Vec3, r: f32| {
        b.push(f.p(p), f.rot(), super::Shape::Sphere(r * 0.92), Mat::Granite, true, false);
        let model = ROCKS[rng.random_range(0..ROCKS.len())];
        b.ornament(f, p, rng.random_range(0.0..std::f32::consts::TAU), model, Vec3::splat(r));
    };
    for _ in 0..40 {
        let r = rng.random_range(1.6..6.5);
        let p = Vec3::new(rng.random_range(-55.0..55.0), r * 0.35, rng.random_range(-50.0..50.0));
        // Keep the mandapa, chariot, gopuram passage and temple clear.
        let keep_clear = [(Vec2::ZERO, 18.0), (Vec2::new(0.0, 22.0), 9.0), (Vec2::new(0.0, -32.0), 14.0), (Vec2::new(30.0, 10.0), 16.0)];
        if keep_clear.iter().any(|(c, rad)| p.xz().distance(*c) < rad + r * 0.6) {
            continue;
        }
        rock(b, rng, p, r);
        // Hampi's famous balancing boulders.
        if r > 3.5 && rng.random_bool(0.55) {
            let r2 = r * rng.random_range(0.45..0.65);
            let dz = rng.random_range(-0.5..0.5);
            rock(b, rng, p + Vec3::new(r * 0.25, r * 0.6 + r2 * 0.55, dz), r2);
        }
    }
    // Ruined mandapa: a grid of pillars (some snapped, some gone), roof over the western half.
    art_over(b, MANDAPA_RUIN, f, 1.0, |b| {
        for (i, col) in MANDAPA_RUIN_PILLARS.iter().enumerate() {
            for (j, st) in col.iter().enumerate() {
                if let Some(h) = st {
                    b.bx(f, Vec3::new(-10.0 + i as f32 * 4.0, h / 2.0 + 0.6, -6.0 + j as f32 * 4.0), Vec3::new(0.66, *h, 0.66), Mat::Granite);
                }
            }
        }
        b.span(f, Vec3::new(-12.0, 0.0, -8.0), Vec3::new(12.0, 0.6, 8.0), Mat::Granite);
        b.span(f, Vec3::new(-11.0, 4.8, -7.0), Vec3::new(1.0, 5.4, 7.0), Mat::Granite);
        // Steps up the plinth, front and back.
        b.stairs(f.child(Vec3::new(4.0, 0.0, 9.9), FRAC_PI_2), Vec3::ZERO, 1.8, 0.6, 3.0, Mat::Granite);
        b.stairs(f.child(Vec3::new(-4.0, 0.0, -9.9), -FRAC_PI_2), Vec3::ZERO, 1.8, 0.6, 3.0, Mat::Granite);
        // Fallen shafts.
        b.bx(f.child(Vec3::new(7.5, 0.0, 3.5), 0.3), Vec3::new(0.0, 0.92, 0.0), Vec3::new(3.4, 0.64, 0.64), Mat::Granite);
        b.bx(f.child(Vec3::new(14.0, 0.0, -3.0), 1.2), Vec3::new(0.0, 0.32, 0.0), Vec3::new(3.0, 0.64, 0.64), Mat::Granite);
    });
    b.chest(f, Vec3::new(-6.0, 0.6, 0.0), 0.0, ChestKind::Peti);
    // Stone chariot.
    let cf = f.child(Vec3::new(0.0, 0.0, 22.0), 0.0);
    let first_chariot = b.map.blocks.len();
    b.span(cf, Vec3::new(-3.0, 0.0, -2.2), Vec3::new(3.0, 2.2, 2.2), Mat::Granite);
    for (x, z) in [(-1.6, -2.6), (1.6, -2.6), (-1.6, 2.6), (1.6, 2.6)] {
        b.push(cf.p(Vec3::new(x, 1.3, z)), cf.rot() * Quat::from_rotation_x(FRAC_PI_2),
               super::Shape::Cylinder { radius: 1.3, height: 0.4 }, Mat::Granite, true, true);
    }
    b.tower(cf, Vec3::new(0.0, 2.2, 0.0), 4.0, 3.0, 4.5, 3, 0.55, Mat::Granite);
    for blk in &mut b.map.blocks[first_chariot..] {
        blk.visible = false;
    }
    b.map.props.push(Prop { model: STONE_CHARIOT, pos: cf.origin, yaw: cf.yaw, scale: 1.0, trunk: None });
    // Gopuram gateway: two massive piers and a stepped tower above the passage.
    let gf = f.child(Vec3::new(0.0, 0.0, -32.0), 0.0);
    let first_gate = b.map.blocks.len();
    b.span(gf, Vec3::new(-9.0, 0.0, -3.0), Vec3::new(-2.5, 7.0, 3.0), Mat::Granite);
    b.span(gf, Vec3::new(2.5, 0.0, -3.0), Vec3::new(9.0, 7.0, 3.0), Mat::Granite);
    b.span(gf, Vec3::new(-9.0, 7.0, -3.0), Vec3::new(9.0, 8.0, 3.0), Mat::Granite);
    b.tower(gf, Vec3::new(0.0, 8.0, 0.0), 16.0, 5.5, 14.0, 6, 0.6, Mat::Brick);
    for blk in &mut b.map.blocks[first_gate..] {
        blk.visible = false;
    }
    b.map.props.push(Prop { model: GOPURAM, pos: gf.origin, yaw: gf.yaw, scale: 1.0, trunk: None });
    b.chest(gf, Vec3::new(-1.8, 0.0, 1.5), 0.0, ChestKind::Kalash); // in the gateway passage
    for _ in 0..6 {
        b.loot(f, Vec3::new(rng.random_range(-40.0..40.0), 0.3, rng.random_range(-40.0..40.0)));
    }
    temple_dravida(b, rng, f.child(Vec3::new(30.0, 0.0, 10.0), -FRAC_PI_2), 0.8);
}

/// Pillar heights of the ruined mandapa's 6 x 4 grid (None = gone); must match
/// `hampi.py::RUIN` (4.2 = standing to the beams).
const MANDAPA_RUIN_PILLARS: [[Option<f32>; 4]; 6] = [
    [Some(4.2), Some(4.2), Some(4.2), Some(4.2)],
    [Some(4.2), Some(4.2), Some(4.2), Some(4.2)],
    [Some(4.2), Some(4.2), Some(4.2), Some(4.2)],
    [Some(4.2), Some(2.2), Some(4.2), None],
    [Some(1.4), Some(4.2), None, Some(2.8)],
    [None, Some(1.0), Some(4.2), Some(2.0)],
];

// ------------------------------------------------------------------ Garh Qila

fn fort(b: &mut Builder, rng: &mut SmallRng) {
    let f = Frame::new(Vec3::new(FORT_C.x, 0.0, FORT_C.y), 0.0);
    // The fort crowns a natural hill (a terrain mesa); its summit is flat at FORT_TOP.
    let top = FORT_TOP;
    let (w, d) = (56.0, 44.0);
    // Curtain walls with a gate on the south side, corner bastions.
    let hw = w / 2.0 - 1.0;
    let hd = d / 2.0 - 1.0;
    let wall_h = 7.0;
    b.wall(f, Vec2::new(-hw, hd), Vec2::new(hw, hd), top, wall_h, 2.0, Mat::RedSandstone, &[Opening::arch(hw - 8.0, 5.0, 5.0)]);
    b.arch_openings(f, Vec2::new(-hw, hd), Vec2::new(hw, hd), top, 2.0, &[Opening::arch(hw - 8.0, 5.0, 5.0)]);
    b.arch_openings(f, Vec2::new(-hw, -hd), Vec2::new(-hw, hd), top, 2.0, &[Opening::arch(hd, 4.0, 4.0)]);
    b.wall(f, Vec2::new(hw, hd), Vec2::new(hw, -hd), top, wall_h, 2.0, Mat::RedSandstone, &[]);
    b.wall(f, Vec2::new(hw, -hd), Vec2::new(-hw, -hd), top, wall_h, 2.0, Mat::RedSandstone, &[]);
    b.wall(f, Vec2::new(-hw, -hd), Vec2::new(-hw, hd), top, wall_h, 2.0, Mat::RedSandstone, &[Opening::arch(hd, 4.0, 4.0)]);
    for (a, c) in [(Vec2::new(-hw, hd), Vec2::new(hw, hd)), (Vec2::new(hw, hd), Vec2::new(hw, -hd)),
                   (Vec2::new(hw, -hd), Vec2::new(-hw, -hd)), (Vec2::new(-hw, -hd), Vec2::new(-hw, hd))] {
        b.merlons(f, a, c, top + wall_h, 2.0, 2.2, Vec2::new(1.1, 1.0), Mat::RedSandstone);
        band(b, f, a, c, top + wall_h - 0.5, 0.3, 2.5, Mat::Sandstone);
        band(b, f, a, c, top, 0.8, 2.4, Mat::Sandstone);
    }
    for (x, z) in [(-hw, -hd), (hw, -hd), (hw, hd), (-hw, hd)] {
        b.deco_cyl(f, Vec3::new(x, top + wall_h + 2.6, z), 4.4, 0.4, Mat::Sandstone);
        b.deco_cyl(f, Vec3::new(x, top, z), 4.3, 1.0, Mat::Sandstone);
        b.cyl(f, Vec3::new(x, top, z), 4.0, wall_h + 3.0, Mat::RedSandstone);
        b.chhatri(f, Vec3::new(x, top + wall_h + 3.0, z), 4.5, Mat::RedSandstone);
    }
    // Wall walk: stairs up inside the north-east corner.
    b.stairs(f.child(Vec3::new(hw - 14.0, top, -hd + 2.5), 0.0), Vec3::ZERO, 10.0, wall_h, 2.0, Mat::RedSandstone);
    b.span(f, Vec3::new(hw - 4.0, top + wall_h - 0.3, -hd + 1.0), Vec3::new(hw - 1.0, top + wall_h, -hd + 4.0), Mat::RedSandstone);
    // Palace (mahal) around a courtyard: four houses facing inward.
    let pf = f.child(Vec3::new(0.0, top, -4.0), 0.0);
    for (pos, yaw) in [
        (Vec3::new(0.0, 0.0, -11.0), 0.0),
        (Vec3::new(0.0, 0.0, 11.0), PI),
        (Vec3::new(-13.0, 0.0, 0.0), FRAC_PI_2),
        (Vec3::new(13.0, 0.0, 0.0), -FRAC_PI_2),
    ] {
        house(b, rng, pf.child(pos, yaw), 12.0, 7.0, 2, Mat::RedSandstone);
    }
    b.chest(pf, Vec3::ZERO, 0.0, ChestKind::Kalash);
    b.cyl(pf, Vec3::new(0.0, 0.0, 3.0), 2.0, 0.6, Mat::Marble);
    for _ in 0..5 {
        b.loot(f, Vec3::new(rng.random_range(-20.0..20.0), top + 0.3, rng.random_range(10.0..18.0)));
    }
}

// ------------------------------------------------------------------ Bargad Van

fn banyan_grove(b: &mut Builder, rng: &mut SmallRng) {
    let c = Vec3::new(95.0, 0.0, 170.0);
    // Three hero banyans whose prop roots make a maze of cover.
    for (dx, dz, sc) in [(0.0, 0.0, 1.0), (30.0, -16.0, 0.8), (-26.0, 18.0, 0.85)] {
        prop(b, TREE_BANYAN, c + Vec3::new(dx, 0.0, dz), rng.random_range(0.0..std::f32::consts::TAU), sc);
    }
    // Machans: hunters' watch platforms on stilts under the canopy, reached by ladders.
    for (dx, dz) in [(7.0, 6.0), (24.0, -6.0), (-20.0, 10.0)] {
        let f = Frame::new(c + Vec3::new(dx, 0.0, dz), rng.random_range(0.0..PI));
        for (x, z) in [(-1.8, -1.8), (1.8, -1.8), (1.8, 1.8), (-1.8, 1.8)] {
            b.cyl(f, Vec3::new(x, 0.0, z), 0.12, 4.6, Mat::Wood);
        }
        b.span(f, Vec3::new(-2.0, 4.2, -2.0), Vec3::new(2.0, 4.5, 2.0), Mat::Wood);
        b.wall(f, Vec2::new(-2.0, 2.0), Vec2::new(2.0, 2.0), 4.5, 0.8, 0.12, Mat::Wood, &[]);
        b.wall(f, Vec2::new(2.0, -2.0), Vec2::new(-2.0, -2.0), 4.5, 0.8, 0.12, Mat::Wood, &[]);
        b.stairs(f.child(Vec3::new(-7.5, 0.0, 0.0), 0.0), Vec3::ZERO, 5.5, 4.5, 1.2, Mat::Wood);
        b.chest(f, Vec3::new(0.8, 4.5, 0.8), 0.0, ChestKind::Peti);
    }
    // Shrine under the main tree with offerings.
    b.span(world(), c + Vec3::new(3.0, 0.0, -3.0), c + Vec3::new(6.0, 0.8, 0.0), Mat::Sandstone);
    b.loot(world(), c + Vec3::new(4.5, 1.0, -1.5));
    for _ in 0..3 {
        let p = c + Vec3::new(rng.random_range(-30.0..30.0), 0.0, rng.random_range(-25.0..30.0));
        let yaw = rng.random_range(0.0..PI);
        hut(b, rng, Frame::new(p, yaw));
    }
    for _ in 0..30 {
        let p = c + Vec3::new(rng.random_range(-40.0..40.0), 0.0, rng.random_range(-35.0..40.0));
        let m = if rng.random_bool(0.7) { BUSH_LANTANA } else { BUSH_BOUGAINVILLEA };
        prop(b, m, p, rng.random_range(0.0..std::f32::consts::TAU), rng.random_range(0.7..1.2));
    }
}

// ------------------------------------------------------------------ Ajanta Gufa + stupa

fn caves_and_stupa(b: &mut Builder, rng: &mut SmallRng) {
    // Cliff face along Z at x = -192, caves cut westward into it. The art is the `AJANTA` model
    // (buddhist.py), authored at the face; everything below is its collider set.
    let face_x = -192.0;
    let back_x = -240.0;
    let hall_d = 20.0;
    let hall_w = 12.0;
    let hall_h = 6.5;
    let cliff_h = 24.0;
    let halls = [-48.0, -16.0, 16.0, 48.0];
    let z0 = -75.0;
    let z1 = 75.0;
    let f = world();
    // The rock spur at the north end with a stair cut up it (z 67..73, stair in z 68..72).
    let (spur_z0, spur_z1, spur_run) = (67.0, 73.0, 28.0);
    art_over(b, AJANTA, Frame::new(Vec3::new(face_x, 0.0, 0.0), 0.0), 1.0, |b| {
        // Solid rock behind the halls and above them.
        b.span(f, Vec3::new(back_x, 0.0, z0), Vec3::new(face_x - hall_d, cliff_h, z1), Mat::Granite);
        b.span(f, Vec3::new(face_x - hall_d, hall_h, z0), Vec3::new(face_x, cliff_h, z1), Mat::Granite);
        // Rock between the halls.
        let mut z = z0;
        for &hz in &halls {
            b.span(f, Vec3::new(face_x - hall_d, 0.0, z), Vec3::new(face_x, hall_h, hz - hall_w / 2.0), Mat::Granite);
            z = hz + hall_w / 2.0;
        }
        b.span(f, Vec3::new(face_x - hall_d, 0.0, z), Vec3::new(face_x, hall_h, z1), Mat::Granite);
        // Hall pillars (two rows inside, two pairs in the porch) and the chaitya stupas.
        for (i, &hz) in halls.iter().enumerate() {
            let hf = Frame::new(Vec3::new(face_x, 0.0, hz), 0.0);
            for xi in 0..4 {
                for s in [-1.0, 1.0] {
                    b.cyl(hf, Vec3::new(-3.0 - xi as f32 * 4.0, 0.0, s * 3.6), 0.38, hall_h, Mat::Granite);
                }
            }
            for s in [-1.0, 1.0] {
                for off in [2.0, 4.6] {
                    b.cyl(hf, Vec3::new(0.9, 0.0, s * off), 0.38, hall_h, Mat::Granite);
                }
            }
            if i % 2 == 0 {
                b.cyl(hf, Vec3::new(-hall_d + 4.0, 0.0, 0.0), 2.25, 1.6, Mat::Granite);
                b.sphere(hf, Vec3::new(-hall_d + 4.0, 1.6, 0.0), 2.1, Mat::Granite, true);
            } else {
                // The shrine's plinth in the back wall.
                b.span(hf, Vec3::new(-hall_d, 0.0, -1.5), Vec3::new(-hall_d + 1.2, 0.8, 1.5), Mat::Granite);
            }
        }
        // The spur: stair ramp, the rock filling under it, and parapets either side.
        let sf = Frame::new(Vec3::new(face_x + spur_run, 0.0, (spur_z0 + spur_z1) / 2.0), PI);
        b.stairs(sf, Vec3::ZERO, spur_run, cliff_h, 4.0, Mat::Granite);
        let n = 7;
        for k in 0..n {
            let (x0, x1) = (spur_run * k as f32 / n as f32, spur_run * (k + 1) as f32 / n as f32);
            let top = cliff_h * x0 / spur_run - 0.4;
            if top > 0.1 {
                b.span(sf, Vec3::new(x0, 0.0, -3.0), Vec3::new(x1, top, 3.0), Mat::Granite);
            }
        }
        for s in [-1.0f32, 1.0] {
            let pf = sf.child(Vec3::new(0.0, 0.0, s * 2.5), FRAC_PI_2);
            slope(b, pf, Vec2::new(0.0, 0.9), Vec2::new(spur_run, cliff_h + 0.9), 1.0, 1.2);
        }
    });
    // Chests and loot in the halls and on the cliff top.
    for (i, &hz) in halls.iter().enumerate() {
        let hf = Frame::new(Vec3::new(face_x, 0.0, hz), 0.0);
        if i % 2 == 0 {
            b.chest(hf, Vec3::new(-hall_d + 7.5, 0.0, 0.0), FRAC_PI_2, ChestKind::Kalash);
        } else {
            b.chest(hf, Vec3::new(-hall_d + 2.0, 0.0, 3.5), FRAC_PI_2, ChestKind::Peti);
            b.loot(hf, Vec3::new(-8.0, 0.3, 0.0));
        }
    }
    for _ in 0..4 {
        b.loot(f, Vec3::new(rng.random_range(back_x + 5.0..face_x - 25.0), cliff_h + 0.3, rng.random_range(z0..z1)));
    }
    // Talus: fallen basalt blocks along the foot of the cliff, clear of the cave mouths and spur.
    let rock = |b: &mut Builder, rng: &mut SmallRng, p: Vec3, r: f32| {
        b.push(p, Quat::IDENTITY, super::Shape::Sphere(r * 0.8), Mat::Granite, true, false);
        let model = ROCKS_BASALT[rng.random_range(0..ROCKS_BASALT.len())];
        b.ornament(world(), p, rng.random_range(0.0..std::f32::consts::TAU), model, Vec3::splat(r));
    };
    for _ in 0..60 {
        let z = rng.random_range(z0 + 2.0..z1 + 6.0);
        if halls.iter().any(|hz| (z - hz).abs() < 10.0) || (spur_z0 - 2.0..spur_z1 + 2.0).contains(&z) {
            continue;
        }
        let r = rng.random_range(0.7..2.2);
        let x = face_x + rng.random_range(0.0..5.0) + r * 0.5;
        rock(b, rng, Vec3::new(x, r * 0.25, z), r);
    }
    // Scrub and boulders on the cliff top.
    for _ in 0..24 {
        let p = Vec3::new(rng.random_range(back_x + 4.0..face_x - 6.0), cliff_h, rng.random_range(z0 + 3.0..z1 - 3.0));
        if rng.random_bool(0.35) {
            let r = rng.random_range(0.8..1.8);
            rock(b, rng, p + Vec3::Y * r * 0.25, r);
        } else {
            let m = if rng.random_bool(0.7) { BUSH_LANTANA } else { BUSH_BOUGAINVILLEA };
            b.map.props.push(Prop { model: m, pos: p - Vec3::Y * 0.1, yaw: rng.random_range(0.0..std::f32::consts::TAU), scale: rng.random_range(0.8..1.3), trunk: None });
        }
    }

    // Sanchi-style great stupa in front of the caves (art: `STUPA`, buddhist.py).
    let sc = Frame::new(Vec3::new(-158.0, 0.0, 8.0), 0.0);
    art_over(b, STUPA, sc, 1.0, |b| {
        b.cyl(sc, Vec3::ZERO, 11.0, 3.0, Mat::Sandstone); // medhi (drum terrace)
        b.sphere(sc, Vec3::new(0.0, 2.0, 0.0), 9.5, Mat::Plaster, true); // anda
        b.bx(sc, Vec3::new(0.0, 12.1, 0.0), Vec3::new(3.0, 1.8, 3.0), Mat::Sandstone); // harmika
        b.cyl(sc, Vec3::new(0.0, 13.0, 0.0), 0.15, 3.0, Mat::Sandstone);
        for (i, r) in [1.6, 1.2, 0.8].iter().enumerate() {
            b.cyl(sc, Vec3::new(0.0, 13.6 + i as f32 * 0.9, 0.0), *r, 0.12, Mat::Sandstone); // chhatravali
        }
        // Stair to the medhi on the west; the walkway railing round its top.
        b.stairs(sc.child(Vec3::new(-16.0, 0.0, 0.0), 0.0), Vec3::ZERO, 5.0, 3.0, 2.5, Mat::Sandstone);
        ring_wall(b, sc, 10.75, 3.0, 0.95, 0.3, &[PI], 1.4);
        // Ground railing (vedika) with four entrances, each faced by a torana.
        ring_wall(b, sc, 15.0, 0.0, 2.6, 0.5, &[0.0, FRAC_PI_2, PI, 1.5 * PI], 2.0);
        for q in 0..4 {
            let tf = sc.child(Vec3::ZERO, q as f32 * FRAC_PI_2).child(Vec3::new(17.0, 0.0, 0.0), 0.0);
            for s in [-1.0, 1.0] {
                b.bx(tf, Vec3::new(0.0, 2.95, s * 2.2), Vec3::new(0.7, 5.9, 0.7), Mat::Sandstone);
            }
            for (y, half) in [(5.9, 3.6), (6.85, 3.8), (7.8, 4.0)] {
                b.bx(tf, Vec3::new(0.0, y + 0.225, 0.0), Vec3::new(0.5, 0.45, 2.0 * half), Mat::Sandstone);
            }
        }
    });
    b.chest(sc, Vec3::new(12.5, 0.0, 6.0), 0.0, ChestKind::Peti);
    b.chest(sc, Vec3::new(-12.5, 0.0, -6.0), PI, ChestKind::Peti);
}

// ------------------------------------------------------------------ villages, nature

/// Thatched mud (or brick) hut. The art is the hut model scaled to this footprint; the walls,
/// roof slopes and door are the gameplay pieces. Door faces local +Z, window on the +X wall.
fn hut(b: &mut Builder, rng: &mut SmallRng, f: Frame) {
    const W0: f32 = 5.2;
    const D0: f32 = 4.5;
    let w = rng.random_range(5.0..5.6);
    let d = rng.random_range(4.2..4.8);
    let model = if rng.random_bool(0.6) { HUT_MUD } else { HUT_BRICK };
    let (hw, hd) = (w / 2.0, d / 2.0);
    let h = 2.8;
    let (sx, sz) = (w / W0, d / D0);
    hidden(b, |b| {
        b.wall(f, Vec2::new(-hw, hd), Vec2::new(hw, hd), 0.0, h, 0.4, Mat::Plaster, &[Opening::door(hw)]);
        b.wall(f, Vec2::new(hw, hd), Vec2::new(hw, -hd), 0.0, h, 0.4, Mat::Plaster, &[Opening::window(hd)]);
        b.wall(f, Vec2::new(hw, -hd), Vec2::new(-hw, -hd), 0.0, h, 0.4, Mat::Plaster, &[]);
        b.wall(f, Vec2::new(-hw, -hd), Vec2::new(-hw, hd), 0.0, h, 0.4, Mat::Plaster, &[]);
        // Thatch slopes (see vernacular.py: ridge 4.35, eave top 2.83, 0.6 m eave, 0.45 m gables).
        let eave = hd + 0.2 + 0.6 * sz;
        let span = w + 0.4 + 0.9 * sx;
        for s in [-1.0f32, 1.0] {
            slope(b, f, Vec2::new(0.0, 4.35), Vec2::new(s * eave, 2.83), span, 0.25);
        }
    });
    b.door(f, Vec2::new(-hw, hd), Vec2::new(hw, hd), hw, 0.0);
    b.ornament(f, Vec3::ZERO, 0.0, model, Vec3::new(sx, 1.0, sz));
    if rng.random_bool(0.6) {
        // Back-right corner: the hearth is in the back-left one.
        b.chest(f, Vec3::new(hw - 1.0, 0.0, -hd + 1.0), 0.0, ChestKind::Peti);
    } else {
        b.loot(f, Vec3::new(0.0, 0.3, 0.0));
    }
}

fn village(b: &mut Builder, rng: &mut SmallRng, c: Vec3, n: u32) {
    for _ in 0..n {
        let p = c + Vec3::new(rng.random_range(-26.0..26.0), 0.0, rng.random_range(-22.0..22.0));
        let yaw = rng.random_range(0.0..std::f32::consts::TAU);
        hut(b, rng, Frame::new(p, yaw));
    }
    // Haystacks, a cart, low walls, a field patch.
    for _ in 0..5 {
        let p = c + Vec3::new(rng.random_range(-35.0..35.0), 0.0, rng.random_range(-30.0..30.0));
        b.cyl(world(), p, rng.random_range(1.2..1.8), rng.random_range(1.6..2.4), Mat::Thatch);
    }
    let cf = Frame::new(c + Vec3::new(rng.random_range(-15.0..15.0), 0.0, 28.0), rng.random_range(0.0..PI));
    b.bx(cf, Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.2, 0.6, 1.8), Mat::Wood);
    for s in [-1.0, 1.0] {
        b.push(cf.p(Vec3::new(0.0, 0.75, s * 1.0)), cf.rot() * Quat::from_rotation_x(FRAC_PI_2),
               super::Shape::Cylinder { radius: 0.75, height: 0.15 }, Mat::Wood, true, true);
    }
    for _ in 0..3 {
        let wf = Frame::new(c + Vec3::new(rng.random_range(-40.0..40.0), 0.0, rng.random_range(-35.0..35.0)), rng.random_range(0.0..PI));
        b.bx(wf, Vec3::new(0.0, 0.55, 0.0), Vec3::new(rng.random_range(6.0..14.0), 1.1, 0.6), Mat::Granite);
    }
    b.push(c + Vec3::new(0.0, 0.015, -38.0), Quat::IDENTITY, super::Shape::Box(Vec3::new(40.0, 0.03, 18.0)), Mat::Thatch, false, true);
}

fn scatter_trees(b: &mut Builder, rng: &mut SmallRng, n: u32) {
    let pois = [
        (Vec2::new(0.0, 0.0), 66.0),
        (STEPWELL_C.xz(), 34.0),
        (Vec2::new(-140.0, -130.0), 30.0),
        (Vec2::new(125.0, -135.0), 52.0),
        (Vec2::new(150.0, 40.0), 30.0),
        (Vec2::new(-200.0, 0.0), 78.0),
        (Vec2::new(95.0, 170.0), 45.0),
    ];
    let clear = |p: Vec2| !pois.iter().any(|(c, r)| p.distance(*c) < *r) && !(p.x > RIVER_X.0 - 3.0 && p.x < RIVER_X.1 + 3.0);
    let tau = std::f32::consts::TAU;
    // Trees: neem and peepal across the plains, in small groves.
    let mut placed = 0;
    while placed < n {
        let g = Vec2::new(rng.random_range(-HALF + 8.0..HALF - 8.0), rng.random_range(-HALF + 8.0..HALF - 8.0));
        let grove = rng.random_range(1..=4);
        for _ in 0..grove {
            let p = g + Vec2::new(rng.random_range(-9.0..9.0), rng.random_range(-9.0..9.0));
            if !clear(p) {
                continue;
            }
            let m = if rng.random_bool(0.6) { TREE_NEEM } else { TREE_PEEPAL };
            prop(b, m, Vec3::new(p.x, 0.0, p.y), rng.random_range(0.0..tau), rng.random_range(0.8..1.25));
            placed += 1;
        }
    }
    // Coconut palms line both river banks and the ghats.
    for _ in 0..70 {
        let z = rng.random_range(-HALF + 5.0..HALF - 5.0);
        let x = if rng.random_bool(0.6) { rng.random_range(RIVER_X.0 - 14.0..RIVER_X.0 - 3.0) } else { rng.random_range(RIVER_X.1 + 3.0..RIVER_X.1 + 16.0) };
        if (x - 150.0).abs() < 12.0 && z > -45.0 && z < 135.0 {
            continue; // keep the ghat town's houses clear
        }
        prop(b, TREE_PALM, Vec3::new(x, 0.0, z), rng.random_range(0.0..tau), rng.random_range(0.85..1.15));
    }
    // Dense scrub: lantana thickets and bougainvillea, clustered so they make real cover.
    for _ in 0..170 {
        let g = Vec2::new(rng.random_range(-HALF + 5.0..HALF - 5.0), rng.random_range(-HALF + 5.0..HALF - 5.0));
        let n = rng.random_range(2..=6);
        for _ in 0..n {
            let p = g + Vec2::new(rng.random_range(-4.0..4.0), rng.random_range(-4.0..4.0));
            if !clear(p) {
                continue;
            }
            let m = if rng.random_bool(0.78) { BUSH_LANTANA } else { BUSH_BOUGAINVILLEA };
            prop(b, m, Vec3::new(p.x, 0.0, p.y), rng.random_range(0.0..tau), rng.random_range(0.7..1.3));
        }
    }
    // Bougainvillea spilling over village and bazaar walls.
    for c in [Vec2::new(-40.0, 195.0), Vec2::new(30.0, -190.0), Vec2::new(-205.0, 185.0), Vec2::new(222.0, -10.0), Vec2::new(-30.0, -95.0)] {
        for _ in 0..8 {
            let p = c + Vec2::new(rng.random_range(-30.0..30.0), rng.random_range(-28.0..28.0));
            prop(b, BUSH_BOUGAINVILLEA, Vec3::new(p.x, 0.0, p.y), rng.random_range(0.0..tau), rng.random_range(0.8..1.2));
        }
    }
}
