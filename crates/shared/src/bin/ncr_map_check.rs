//! Build the one NCR map, report what it holds and draw it from above.
//! cargo run --release -p shared --bin ncr_map_check -- [out.ppm]
//! cargo run --release -p shared --bin ncr_map_check -- --at <x> <z> <radius>   what stands there
//! cargo run --release -p shared --bin ncr_map_check -- --line <x0> <z0> <x1> <z1>   the ground along a line
use bevy::prelude::Vec3Swizzles;
use shared::map::{compose, terrain::ground};
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    let map = compose::generate(7);
    println!("built in {:.1} s", t0.elapsed().as_secs_f32());
    let collide = map.blocks.iter().filter(|b| b.collide).count();
    println!("blocks {} ({collide} solid), doors {}, footprints {}", map.blocks.len(), map.doors.len(), map.footprints.len());
    let one_match = |c: &&shared::map::ChestSpawn| c.set.is_none_or(|s| s.1 == 0);
    println!("chests {} a match, of {} spots; floor loot {}, props {}, kit {}, lamps {}", map.chests.iter().filter(one_match).count(), map.chests.len(), map.floor_loot.len(), map.props.len(), map.kit.len(), map.lamps.len());
    let t = &map.terrain;
    println!("terrain {} x {} samples, {} MB", t.n, t.n, (t.h.len() * 8 + t.ground.len()) / 1_000_000);
    let share = |class: u8| t.ground.iter().filter(|g| **g == class).count() as f32 / t.ground.len() as f32 * 100.0;
    let area: f32 = map.footprints.iter().filter(|f| f.mat != shared::map::Mat::Water).map(|f| {
        let n = f.pts.len();
        ((0..n).map(|k| f.pts[k].perp_dot(f.pts[(k + 1) % n])).sum::<f32>() / 2.0).abs()
    }).sum();
    println!("open land {:.0} %, river sand {:.0} %, road {:.1} %, buildings cover {:.1} % of the map", share(ground::OPEN), share(ground::SAND), share(ground::ROAD), area / (4000.0 * 4000.0) * 100.0);
    let kalash = map.chests.iter().filter(one_match).filter(|c| c.kind == shared::map::ChestKind::Kalash).count();
    println!("kalash {kalash}, kit on roofs and decks {}", map.kit.iter().filter(|k| k.pos.y - t.height(k.pos.x, k.pos.z) > 1.5).count());
    println!("kit floating on nothing: {}", compose::floating(&map).len());
    let pieces = |name: &str| map.kit.iter().filter(|k| k.name == name).count();
    println!("hiding: {} gumti, {} tanks, {} washing lines, {} poles, {} flocks", pieces("gumti"), pieces("tanki"), pieces("clothesline"), pieces("pole"), shared::sim::birds::perches(&map).len());
    let (walls, short) = shared::map::kit::out_of_step(&map);
    println!("hidden colliders out of step with the kit: {walls} invisible walls, {short} pieces short of a collider");
    let mut squares = std::collections::HashMap::new();
    for k in &map.kit {
        *squares.entry(((k.pos.x / 100.0).floor() as i32, (k.pos.z / 100.0).floor() as i32)).or_insert(0) += 1;
    }
    println!("kit in the fullest 100 m square: {}", squares.values().max().unwrap());
    for p in &compose::PLACES {
        let (x, z) = (p.at.x, p.at.y);
        let kit = map.kit.iter().filter(|k| (k.pos.x - x).abs() <= p.clip && (k.pos.z - z).abs() <= p.clip).count();
        let chests = map.chests.iter().filter(one_match).filter(|c| (c.pos.x - x).abs() <= p.clip && (c.pos.z - z).abs() <= p.clip).count();
        println!("  {:<17} exits {:>2}  kit {kit:>4}  chests {chests:>2}", p.name, compose::exits(&map, p));
    }
    for p in map.pois.iter().skip(compose::PLACES.len()) {
        println!("  {:>7.0} {:>7.0}  {}", p.center.x, p.center.z, p.name);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--at") {
        let n: Vec<f32> = args[1..].iter().map(|a| a.parse().expect("--at x z radius")).collect();
        here(&map, bevy::math::Vec2::new(n[0], n[1]), n[2]);
        return;
    }
    if args.first().is_some_and(|a| a == "--line") {
        let n: Vec<f32> = args[1..].iter().map(|a| a.parse().expect("--line x0 z0 x1 z1")).collect();
        let (a, c) = (bevy::math::Vec2::new(n[0], n[1]), bevy::math::Vec2::new(n[2], n[3]));
        for k in 0..=(a.distance(c) / 4.0) as usize {
            let p = a.move_towards(c, k as f32 * 4.0);
            println!("{:>8.1} {:>8.1}  height {:>6.2}  ground {:?}", p.x, p.y, t.height(p.x, p.y), t.ground_at(p.x, p.y));
        }
        return;
    }
    let Some(out) = args.first().cloned() else { return };
    let side = t.n;
    let mut px = vec![0u8; side * side * 3];
    for i in 0..side {
        for j in 0..side {
            let c: [u8; 3] = match t.ground[i * side + j] {
                ground::ROAD => [70, 74, 80],
                ground::PARK => [150, 182, 120],
                ground::OPEN => [176, 190, 140],
                ground::SAND => [196, 178, 136],
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
    // The decks, in white, as the ground under them is only painted where it is land.
    for s in &compose::SPANS {
        for d in 0..s.a.distance(s.c) as usize {
            let p = s.a.lerp(s.c, d as f32 / s.a.distance(s.c));
            let k = |c: f32| ((c + t.size * 0.5) / t.step()) as usize;
            for o in 0..3 {
                px[((k(p.y) + o) * side + k(p.x)) * 3..((k(p.y) + o) * side + k(p.x)) * 3 + 3].copy_from_slice(&[250, 250, 250]);
            }
        }
    }
    let mut file = format!("P6\n{side} {side}\n255\n").into_bytes();
    file.extend(px);
    std::fs::write(&out, file).unwrap();
    println!("drew {out}");
}

/// Everything standing within `r` of a point, to find out what a screenshot is showing.
fn here(map: &shared::map::MapLayout, p: bevy::math::Vec2, r: f32) {
    for b in map.blocks.iter().filter(|b| b.pos.xz().distance(p) < r) {
        println!("block {:?} {:?} at {:.1?} collide {} visible {}", b.mat, b.shape, b.pos, b.collide, b.visible);
    }
    for k in map.kit.iter().filter(|k| k.pos.xz().distance(p) < r) {
        println!("kit {} at {:.1?}", k.name, k.pos);
    }
    for f in map.footprints.iter().filter(|f| f.pts[0].distance(p) < r) {
        println!("footprint {:?} {:.1}..{:.1} at {:.1?}", f.mat, f.base, f.top, f.pts[0]);
    }
}
