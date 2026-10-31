//! Finds z-fighting in the map: pairs of visible box faces that lie in the same plane, face the
//! same way and overlap, with different materials (same-material overlaps shade identically, since
//! building textures are mapped in world space, so they can't be seen to fight).
//!
//! cargo run --release -p shared --bin zfight_check [-- --all] [--dump <file.json>]   (VIVAD_MAP and VIVAD_MAP_SEED pick the map)
//!
//! `--dump` also writes model placements, visible box faces and the terrain heights for
//! tools/zfight_models.py, which checks the models' own faces against both.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::map::{Block, Mat, Shape};

/// Planes closer than this (1 mm) count as the same: the layout's exact coincidences agree to far
/// less, and reversed-Z depth keeps surfaces a few millimetres apart at game distances.
const PLANE_TOL: f32 = 0.001;

struct Face {
    block: usize,
    n: Vec3,
    d: f32,
    c: Vec3,
    u: Vec3,
    v: Vec3,
    hu: f32,
    hv: f32,
}

fn faces(i: usize, b: &Block) -> Vec<Face> {
    let Shape::Box(size) = b.shape else { return vec![] };
    let h = size / 2.0;
    let ax = [b.rot * Vec3::X, b.rot * Vec3::Y, b.rot * Vec3::Z];
    let hs = [h.x, h.y, h.z];
    let mut out = vec![];
    for k in 0..3 {
        let (a1, a2) = ((k + 1) % 3, (k + 2) % 3);
        for s in [-1.0f32, 1.0] {
            let n = ax[k] * s;
            let c = b.pos + n * hs[k];
            out.push(Face { block: i, n, d: n.dot(c), c, u: ax[a1], v: ax[a2], hu: hs[a1], hv: hs[a2] });
        }
    }
    out
}

/// Overlap area of two coplanar rectangles, measured in `a`'s axes (exact when their edges are
/// parallel or perpendicular, which is how the map is built).
fn overlap(a: &Face, b: &Face) -> f32 {
    let (bu, bv) = if b.u.dot(a.u).abs() > 0.99 { (b.hu, b.hv) } else { (b.hv, b.hu) };
    let du = (b.c - a.c).dot(a.u);
    let dv = (b.c - a.c).dot(a.v);
    let ou = ((a.hu + bu) - du.abs()).min(2.0 * a.hu).min(2.0 * bu).max(0.0);
    let ov = ((a.hv + bv) - dv.abs()).min(2.0 * a.hv).min(2.0 * bv).max(0.0);
    ou * ov
}

/// Centre of the overlap of two coplanar rectangles (as `overlap`).
fn overlap_centre(a: &Face, b: &Face) -> Vec3 {
    let (bu, bv) = if b.u.dot(a.u).abs() > 0.99 { (b.hu, b.hv) } else { (b.hv, b.hu) };
    let (du, dv) = ((b.c - a.c).dot(a.u), (b.c - a.c).dot(a.v));
    let mid = |h: f32, d: f32, bh: f32| ((-h).max(d - bh) + h.min(d + bh)) / 2.0;
    a.c + a.u * mid(a.hu, du, bu) + a.v * mid(a.hv, dv, bv)
}

fn inside(b: &Block, p: Vec3) -> bool {
    let l = b.rot.inverse() * (p - b.pos);
    match b.shape {
        Shape::Box(s) => (l.abs() - s / 2.0).max_element() < 0.0,
        Shape::Cylinder { radius, height } => l.xz().length() < radius && l.y.abs() < height / 2.0,
        Shape::Sphere(r) => l.length() < r,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let all = args.iter().any(|a| a == "--all");
    let dump = args.iter().position(|a| a == "--dump").and_then(|i| args.get(i + 1)).cloned();
    let map = shared::map::generate(shared::map::MapId::from_env(), shared::sim::map_seed());
    let blocks: Vec<&Block> = map.blocks.iter().chain(map.doors.iter()).collect();
    let mut buckets: HashMap<(i32, i32, i32, i32), Vec<Face>> = HashMap::new();
    for (i, b) in blocks.iter().enumerate() {
        if !b.visible {
            continue;
        }
        for f in faces(i, b) {
            // Bucket by rounded normal and plane offset (neighbouring offsets checked below).
            let key = ((f.n.x * 100.0).round() as i32, (f.n.y * 100.0).round() as i32, (f.n.z * 100.0).round() as i32, (f.d * 200.0).round() as i32);
            buckets.entry(key).or_default().push(f);
        }
    }
    let mut pairs: HashMap<(Mat, Mat, &str), (u32, f32, Vec3)> = HashMap::new();
    let mut total = 0;
    for (key, fs) in &buckets {
        for dk in [0, 1] {
            let other = (key.0, key.1, key.2, key.3 + dk);
            let Some(gs) = buckets.get(&other) else { continue };
            for (ia, a) in fs.iter().enumerate() {
                for (ib, b) in gs.iter().enumerate() {
                    if (dk == 0 && ib <= ia) || a.block == b.block || (a.d - b.d).abs() > PLANE_TOL || a.n.dot(b.n) < 0.999 {
                        continue;
                    }
                    // Undersides resting on (or in) the ground are never seen.
                    if a.n.y < -0.9 && a.c.y < map.terrain.height(a.c.x, a.c.z) + 0.1 {
                        continue;
                    }
                    let (ma, mb) = (blocks[a.block].mat, blocks[b.block].mat);
                    if ma == mb && !all {
                        continue;
                    }
                    let area = overlap(a, b);
                    if area < 0.004 {
                        continue;
                    }
                    // Both faces buried in a third block (say, a corbel's side and a pier's end
                    // inside the lintel over an arch) can't be seen.
                    let front = overlap_centre(a, b) + a.n * 0.005;
                    if blocks.iter().enumerate().any(|(k, blk)| k != a.block && k != b.block && blk.visible && inside(blk, front)) {
                        continue;
                    }
                    total += 1;
                    let dir = if a.n.y > 0.9 { "top" } else if a.n.y < -0.9 { "underside" } else { "side" };
                    let k = if format!("{ma:?}") <= format!("{mb:?}") { (ma, mb, dir) } else { (mb, ma, dir) };
                    let e = pairs.entry(k).or_insert((0, 0.0, a.c));
                    e.0 += 1;
                    e.1 += area;
                }
            }
        }
    }
    if let Some(path) = dump {
        let v3 = |v: Vec3| format!("[{:.4},{:.4},{:.4}]", v.x, v.y, v.z);
        let q4 = |q: Quat| format!("[{:.6},{:.6},{:.6},{:.6}]", q.x, q.y, q.z, q.w);
        let mut out = String::from("{\"placements\":[");
        let mut first = true;
        for p in &map.props {
            out += &format!("{}{{\"model\":\"{}\",\"pos\":{},\"rot\":{},\"scale\":{}}}", if first { "" } else { "," }, p.model, v3(p.pos),
                            q4(Quat::from_rotation_y(p.yaw)), v3(Vec3::splat(p.scale)));
            first = false;
        }
        for o in &map.ornaments {
            out += &format!(",{{\"model\":\"{}\",\"pos\":{},\"rot\":{},\"scale\":{}}}", o.model, v3(o.pos), q4(o.rot), v3(o.scale));
        }
        out += "],\"faces\":[";
        let mut first = true;
        for fs in buckets.values() {
            for f in fs {
                out += &format!("{}[{},{},{},{:.4},{:.4},\"{:?}\",{}]", if first { "" } else { "," }, v3(f.c), v3(f.u), v3(f.v), f.hu, f.hv, blocks[f.block].mat, v3(f.n));
                first = false;
            }
        }
        // Visible cylinder tops (as the square inscribed in the disc), for the terrain check.
        for b in &blocks {
            if let (true, Shape::Cylinder { radius, height }) = (b.visible, b.shape) {
                let c = b.pos + b.rot * Vec3::Y * (height / 2.0);
                let r = radius * std::f32::consts::FRAC_1_SQRT_2;
                out += &format!(",[{},{},{},{r:.4},{r:.4},\"{:?}\",{}]", v3(c), v3(b.rot * Vec3::X), v3(b.rot * Vec3::Z), b.mat, v3(b.rot * Vec3::Y));
            }
        }
        // Solid boxes (visible or not), so a model face pressed against one counts as hidden.
        out += "],\"boxes\":[";
        let mut first = true;
        for b in &blocks {
            if let Shape::Box(size) = b.shape {
                out += &format!("{}[{},{},{}]", if first { "" } else { "," }, v3(b.pos), q4(b.rot), v3(size));
                first = false;
            }
        }
        let t = &map.terrain;
        out += &format!("],\"terrain\":{{\"n\":{},\"size\":{},\"h\":[{}]}}}}", t.n, t.size,
                        t.h.iter().map(|h| format!("{h:.3}")).collect::<Vec<_>>().join(","));
        std::fs::write(&path, out).unwrap();
        println!("dumped {} placements and the box faces to {path}", map.props.len() + map.ornaments.len());
    }
    let mut rows: Vec<_> = pairs.into_iter().collect();
    rows.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1));
    println!("{total} z-fighting face pairs ({} materials combinations)", rows.len());
    for ((a, b, dir), (n, area, at)) in rows.iter().take(25) {
        println!("  {:<14} vs {:<14} {dir:<9} {n:>5} pairs  {area:8.1} m2   e.g. at ({:.1}, {:.1}, {:.1})", format!("{a:?}"), format!("{b:?}"), at.x, at.y, at.z);
    }
}
