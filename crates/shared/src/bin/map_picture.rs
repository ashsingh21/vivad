//! A map seen from above, as a picture: its ground (pavement, road, park...), the buildings'
//! footprints, trees and bushes, street kit and chests. For seeing where a map is empty.
//!
//! cargo run --release -p shared --bin map_picture -- <map> <out.ppm> [metres a pixel, default 2]

use bevy::math::Vec2;
use shared::map::terrain::ground;
use shared::map::{MapId, generate};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = args.first().and_then(|s| MapId::parse(s)).expect("map name");
    let out = args.get(1).expect("output .ppm");
    let mpp: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let m = generate(id, shared::sim::map_seed());
    let half = m.half_extent;
    let n = (2.0 * half / mpp) as usize;
    let mut px = vec![[0u8; 3]; n * n];
    let to = |p: Vec2| ((p.x + half) / mpp, (p.y + half) / mpp);
    for j in 0..n {
        for i in 0..n {
            let (x, z) = (i as f32 * mpp - half, j as f32 * mpp - half);
            px[j * n + i] = match m.terrain.ground_at(x, z) {
                Some(ground::PAVEMENT) => [200, 190, 170],
                Some(ground::ROAD) => [90, 90, 95],
                Some(ground::PARK) => [110, 170, 80],
                Some(ground::PARKING) => [140, 140, 150],
                Some(ground::PLAZA) => [220, 210, 190],
                Some(ground::WATER) => [70, 130, 200],
                Some(ground::PATH) => [190, 160, 120],
                Some(ground::OPEN) => [170, 150, 100],
                Some(ground::SAND) => [235, 215, 160],
                Some(_) => [255, 0, 255],
                None => [40, 40, 40],
            };
        }
    }
    let mut dot = |p: Vec2, r: f32, c: [u8; 3]| {
        let (cx, cy) = to(p);
        let r = (r / mpp).max(0.5);
        for y in (cy - r).floor() as i64..=(cy + r).ceil() as i64 {
            for x in (cx - r).floor() as i64..=(cx + r).ceil() as i64 {
                if x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n && ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)) <= r * r {
                    px[y as usize * n + x as usize] = c;
                }
            }
        }
    };
    for f in &m.footprints {
        // Filled by scanline over its bounding box (even-odd).
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let mut y = lo.y;
        while y <= hi.y {
            let mut x = lo.x;
            while x <= hi.x {
                let p = Vec2::new(x, y);
                let mut inside = false;
                for k in 0..f.pts.len() {
                    let (a, b) = (f.pts[k], f.pts[(k + 1) % f.pts.len()]);
                    if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                        inside = !inside;
                    }
                }
                if inside {
                    dot(p, 0.0, [150, 60, 50]);
                }
                x += mpp;
            }
            y += mpp;
        }
    }
    for b in &m.blocks {
        dot(Vec2::new(b.pos.x, b.pos.z), 0.0, [120, 70, 60]);
    }
    for p in &m.props {
        dot(Vec2::new(p.pos.x, p.pos.z), 1.5, [30, 110, 30]);
    }
    for k in &m.kit {
        dot(Vec2::new(k.pos.x, k.pos.z), 0.0, [60, 60, 200]);
    }
    for c in &m.chests {
        dot(Vec2::new(c.pos.x, c.pos.z), 4.0, [255, 30, 30]);
    }
    let mut bytes = format!("P6\n{n} {n}\n255\n").into_bytes();
    px.iter().for_each(|c| bytes.extend_from_slice(c));
    std::fs::write(out, bytes).unwrap();
    println!("{}: {} footprints, {} blocks, {} props, {} kit, {} chests", id.name(), m.footprints.len(), m.blocks.len(), m.props.len(), m.kit.len(), m.chests.len());
}
