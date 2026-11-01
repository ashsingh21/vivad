//! The Indian house kit shared by the city maps: plastered houses and havelis with jharokhas,
//! friezes and barsati rooms, and the tree models they stand among.

use std::f32::consts::PI;

use bevy::prelude::*;
use rand::rngs::SmallRng;
use rand::RngExt;

use super::build::{Builder, Frame, Opening, FLOOR_H, JHAROKHA, WALL_T};

pub const FRESCOES: [&str; 2] = ["models/props/fresco_a.gltf", "models/props/fresco_b.gltf"];
pub const BUNTING: &str = "models/props/bunting.gltf";

/// Painted frieze panels (2 m repeats, 0.44 m tall, bottom at `y`) along wall a -> c, on its
/// outer face (local +z) or, with `inner`, on the other; gaps of +-1.4 m round each of `skip`
/// (distances along the wall: balconies, doors).
fn frieze(b: &mut Builder, f: Frame, a: Vec2, c: Vec2, y: f32, thick: f32, inner: bool, skip: &[f32], model: &'static str) {
    let dir = c - a;
    let len = dir.length();
    let wf = f.child(Vec3::new(a.x, 0.0, a.y), (-dir.y).atan2(dir.x));
    let mut cuts: Vec<f32> = skip.iter().copied().filter(|&s| s > -1.4 && s < len + 1.4).collect();
    cuts.sort_by(f32::total_cmp);
    let mut runs = vec![];
    let mut s = 0.25;
    for at in cuts {
        runs.push((s, at - 1.4));
        s = at + 1.4;
    }
    runs.push((s, len - 0.25));
    let z = if inner { -(thick / 2.0 + 0.012) } else { thick / 2.0 + 0.012 };
    let yaw = if inner { PI } else { 0.0 };
    for (s, e) in runs {
        if e - s < 1.2 {
            continue;
        }
        let n = ((e - s) / 2.0).round().max(1.0);
        let piece = (e - s) / n;
        for i in 0..n as usize {
            b.ornament(wf, Vec3::new(s + (i as f32 + 0.5) * piece, y, z), yaw, model, Vec3::new(piece / 2.0, 1.0, 1.0));
        }
    }
}
use super::{ChestKind, Mat};

pub const TREE_NEEM: &str = "models/props/tree_neem.gltf";
pub const TREE_PEEPAL: &str = "models/props/tree_peepal.gltf";
pub const TREE_PALM: &str = "models/props/tree_palm.gltf";
pub const TREE_BANYAN: &str = "models/props/tree_banyan.gltf";
pub const BUSH_LANTANA: &str = "models/props/bush_lantana.gltf";
pub const BUSH_BOUGAINVILLEA: &str = "models/props/bush_bougainvillea.gltf";

/// Multi-storey house / haveli. Frame origin at the ground-floor centre; door faces local +Z.
pub fn house(b: &mut Builder, rng: &mut SmallRng, f: Frame, w: f32, d: f32, floors: u32, mat: Mat) {
    house_with(b, rng, f, w, d, floors, mat, false);
}

/// `palace`: a riverfront palace, every upper-floor front window a jharokha and a chhatri on
/// each front corner of the roof.
fn house_with(b: &mut Builder, rng: &mut SmallRng, f: Frame, w: f32, d: f32, floors: u32, mat: Mat, palace: bool) {
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
    let mut top_balconies = vec![];
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
        // A jharokha (projecting balcony window) replaces the central upper-floor window of wide
        // havelis, or every upper-floor front window of a palace.
        let balconies: Vec<f32> = if k == 0 {
            vec![]
        } else if palace {
            front.iter().map(|o| o.at).collect()
        } else if w >= 9.0 {
            vec![w / 2.0]
        } else {
            vec![]
        };
        if !balconies.is_empty() {
            front.retain(|o| balconies.iter().all(|&at| (o.at - at).abs() > 1.4));
            front.extend(balconies.iter().map(|&at| Opening { at, width: 1.2, bottom: 0.9, top: 2.4 }));
        }
        b.wall(f, corners[0], corners[1], y0, FLOOR_H, t, mat, &front);
        b.wall(f, corners[1], corners[2], y0, FLOOR_H, t, mat, &win(d));
        b.wall(f, corners[2], corners[3], y0, FLOOR_H, t, mat, &[]);
        b.wall(f, corners[3], corners[0], y0, FLOOR_H, t, mat, &win(d));
        let trim = trim_for(mat);
        let arched: Vec<Opening> = front.iter().copied().filter(|o| !balconies.iter().any(|&at| (o.at - at).abs() < 0.1)).collect();
        b.arch_openings(f, corners[0], corners[1], y0, t, &arched);
        b.arch_openings(f, corners[1], corners[2], y0, t, &win(d));
        b.arch_openings(f, corners[3], corners[0], y0, t, &win(d));
        for &at in &balconies {
            // Balcony floor level with the window sill.
            b.ornament(f, Vec3::new(at - hw, y0 + 0.26, hd), 0.0, JHAROKHA, Vec3::ONE);
        }
        if k + 1 == floors {
            top_balconies = balconies.clone();
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
    // A painted frieze under the front cornice (palaces always, most houses).
    if palace || rng.random_bool(0.6) {
        let model = FRESCOES[rng.random_range(0..FRESCOES.len())];
        frieze(b, f, Vec2::new(-hw, hd), Vec2::new(hw, hd), ry - 1.16, t, false, &top_balconies, model);
    }
    // Barsati: a rooftop room over the stair head, opening onto the terrace. It breaks up the
    // box silhouette and roofs the last flight.
    let barsati = rng.random_bool(if floors <= 2 { 0.6 } else { 0.35 });
    if barsati {
        let bw = (run + 1.4).max(w * 0.5).min(w - 2.0 * t - 1.0);
        barsati_room(b, f, Vec2::new(-hw + t, -hd + t), bw, 2.0 * lane_w + 1.0, ry, mat, false);
    }
    if palace {
        for sx in [-1.0, 1.0] {
            b.chhatri(f, Vec3::new(sx * (hw - 1.7), ry, hd - 1.7), 2.3, mat);
        }
    } else if w > 9.5 && rng.random_bool(if barsati { 0.25 } else { 0.5 }) {
        b.chhatri(f, Vec3::new(hw - 2.2, ry, hd - 2.2), 2.8, mat);
    }
    if rng.random_bool(0.3) {
        b.loot(f, Vec3::new(0.0, ry + 0.3, 0.0));
    }
}

/// A rooftop room (barsati) with its back-left corner at `corner`: a door and a window onto the
/// terrace (the door in the +Z face, or the +X face with `door_east`), merloned eaves on both.
fn barsati_room(b: &mut Builder, f: Frame, corner: Vec2, bw: f32, bd: f32, ry: f32, mat: Mat, door_east: bool) {
    let t = WALL_T;
    let trim = trim_for(mat);
    let (x0, z0) = (corner.x, corner.y);
    let (x1, z1) = (x0 + bw, z0 + bd);
    let rh = 2.8;
    let (mut front, mut side) = ([Opening::door(bw / 2.0)], [Opening::window(bd / 2.0)]);
    if door_east {
        (front, side) = ([Opening::window(bw / 2.0)], [Opening::door(bd / 2.0)]);
    }
    b.wall(f, Vec2::new(x0, z1), Vec2::new(x1, z1), ry, rh, t, mat, &front);
    b.arch_openings(f, Vec2::new(x0, z1), Vec2::new(x1, z1), ry, t, &front);
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
