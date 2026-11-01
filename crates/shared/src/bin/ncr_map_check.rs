//! Build the one NCR map, report what it holds and draw it from above.
//! cargo run --release -p shared --bin ncr_map_check -- [out.ppm]
use shared::map::{compose, terrain::ground};
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    let map = compose::generate(7);
    println!("built in {:.1} s", t0.elapsed().as_secs_f32());
    let collide = map.blocks.iter().filter(|b| b.collide).count();
    println!("blocks {} ({collide} solid), doors {}, footprints {}", map.blocks.len(), map.doors.len(), map.footprints.len());
    println!("chests {}, floor loot {}, props {}, kit {}, lamps {}", map.chests.len(), map.floor_loot.len(), map.props.len(), map.kit.len(), map.lamps.len());
    let t = &map.terrain;
    println!("terrain {} x {} samples, {} MB", t.n, t.n, (t.h.len() * 8 + t.ground.len()) / 1_000_000);
    let open = t.ground.iter().filter(|g| **g == ground::PARK).count() as f32 / t.ground.len() as f32;
    let area: f32 = map.footprints.iter().filter(|f| f.mat != shared::map::Mat::Water).map(|f| {
        let n = f.pts.len();
        ((0..n).map(|k| f.pts[k].perp_dot(f.pts[(k + 1) % n])).sum::<f32>() / 2.0).abs()
    }).sum();
    println!("open land {:.0} %, buildings cover {:.1} % of the map", open * 100.0, area / (4000.0 * 4000.0) * 100.0);
    for p in &map.pois {
        println!("  {:>7.0} {:>7.0}  {}", p.center.x, p.center.z, p.name);
    }
    let Some(out) = std::env::args().nth(1) else { return };
    let side = t.n;
    let mut px = vec![0u8; side * side * 3];
    for i in 0..side {
        for j in 0..side {
            let c: [u8; 3] = match t.ground[i * side + j] {
                ground::ROAD => [70, 74, 80],
                ground::PARK => [176, 190, 140],
                ground::WATER => [80, 150, 170],
                ground::PATH => [190, 170, 130],
                ground::PARKING => [130, 130, 130],
                ground::PLAZA => [205, 200, 190],
                _ => [216, 210, 196],
            };
            px[(j * side + i) * 3..(j * side + i) * 3 + 3].copy_from_slice(&c);
        }
    }
    for f in &map.footprints {
        if f.mat == shared::map::Mat::Water {
            continue;
        }
        let (lo, hi) = f.pts.iter().fold((bevy::math::Vec2::MAX, bevy::math::Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let k = |c: f32| ((c + t.size * 0.5) / t.step()) as usize;
        for i in k(lo.x)..=k(hi.x).min(side - 1) {
            for j in k(lo.y)..=k(hi.y).min(side - 1) {
                if shared::map::footprint::contains(&f.pts, bevy::math::Vec2::new(t.coord(i), t.coord(j))) {
                    px[(j * side + i) * 3..(j * side + i) * 3 + 3].copy_from_slice(&[58, 64, 74]);
                }
            }
        }
    }
    let mut file = format!("P6\n{side} {side}\n255\n").into_bytes();
    file.extend(px);
    std::fs::write(&out, file).unwrap();
    println!("drew {out}");
}
