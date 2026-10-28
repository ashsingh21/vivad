//! Finds z-fighting in the map: pairs of visible box faces that lie in the same plane, face the
//! same way and overlap, with different materials (same-material overlaps shade identically, since
//! building textures are mapped in world space, so they can't be seen to fight).
//!
//! cargo run --release -p shared --bin zfight_check [-- --all]

use std::collections::HashMap;

use bevy::prelude::*;
use shared::map::{Block, Mat, Shape};

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

fn main() {
    let all = std::env::args().any(|a| a == "--all");
    let map = shared::map::india::generate(7);
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
                    if (dk == 0 && ib <= ia) || a.block == b.block || (a.d - b.d).abs() > 0.004 || a.n.dot(b.n) < 0.999 {
                        continue;
                    }
                    // Undersides resting on the ground are never seen.
                    if a.n.y < -0.9 && a.c.y < 0.1 {
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
    let mut rows: Vec<_> = pairs.into_iter().collect();
    rows.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1));
    println!("{total} z-fighting face pairs ({} materials combinations)", rows.len());
    for ((a, b, dir), (n, area, at)) in rows.iter().take(25) {
        println!("  {:<14} vs {:<14} {dir:<9} {n:>5} pairs  {area:8.1} m2   e.g. at ({:.1}, {:.1}, {:.1})", format!("{a:?}"), format!("{b:?}"), at.x, at.y, at.z);
    }
}
