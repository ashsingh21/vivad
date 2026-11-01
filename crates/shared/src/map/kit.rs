//! The street kit on the city maps: parked autos and scooters, thelas, chai stalls, poles, drums,
//! crates and chairs along the roads; stair rooms, coolers, dishes, solar panels and washing lines on
//! the roofs. Placed from the map seed (every peer the same); drawn by the client from one model
//! file, merged per chunk (tools/blender/props/street_kit.py, crate::street in the game). The bigger
//! pieces stand behind invisible colliders, so a parked auto is cover.

use bevy::prelude::*;
use rand::RngExt;
use rand::rngs::SmallRng;

use super::build::Builder;
use super::terrain::ground;
use super::{MapLayout, Mat, Shape};

/// A placed piece: the kit model's name (its meshes are `<name>_LOD<n>`), where it stands and which
/// way it faces (a yaw about +Y; the model faces -Z).
#[derive(Clone, Debug, Reflect)]
pub struct KitPiece {
    pub name: &'static str,
    pub pos: Vec3,
    pub yaw: f32,
}

/// A piece's collider in its own frame, from the model's sizes: a box (width, height, depth) or an
/// upright cylinder (radius, height), centred over the origin; None for things to walk through.
fn collider(name: &str) -> Option<Shape> {
    Some(match name {
        "auto" => Shape::Box(Vec3::new(1.4, 1.8, 2.6)),
        "scooter" => Shape::Box(Vec3::new(0.45, 1.1, 1.65)),
        // Motorcycles parked on their side stands (leaning), and the roadster bicycle.
        "thumper" | "commuter" | "discover" | "twostroke" | "sport150" => Shape::Box(Vec3::new(0.7, 1.1, 2.0)),
        "roadster" => Shape::Box(Vec3::new(0.5, 1.0, 1.75)),
        "thela" => Shape::Box(Vec3::new(1.25, 1.0, 2.1)),
        "chai" => Shape::Box(Vec3::new(1.7, 1.0, 0.8)),
        "crates" => Shape::Box(Vec3::new(1.24, 0.94, 0.42)),
        "cooler" => Shape::Box(Vec3::new(0.9, 1.1, 0.64)),
        "mumty" => Shape::Box(Vec3::new(3.0, 2.75, 2.4)),
        "pole" => Shape::Cylinder { radius: 0.14, height: 9.0 },
        "drum" => Shape::Cylinder { radius: 0.3, height: 0.95 },
        _ => return None,
    })
}

/// Footprint radius kept clear round a piece when placing.
fn reach(name: &str) -> f32 {
    match name {
        "auto" | "thela" | "chai" => 1.6,
        "mumty" => 2.1,
        "clothesline" => 2.2,
        "scooter" | "crates" | "solar" | "thumper" | "commuter" | "discover" | "twostroke" | "sport150" | "roadster" => 1.0,
        _ => 0.6,
    }
}

fn place(b: &mut Builder, name: &'static str, pos: Vec3, yaw: f32) {
    if let Some(shape) = collider(name) {
        let h = match shape {
            Shape::Box(s) => s.y,
            Shape::Cylinder { height, .. } => height,
            Shape::Sphere(r) => 2.0 * r,
        };
        b.push(pos + Vec3::Y * h * 0.5, Quat::from_rotation_y(yaw), shape, Mat::Wood, true, false);
    }
    b.map.kit.push(KitPiece { name, pos, yaw });
}

/// The yaw that faces the model (-Z forward) along `dir` (x, z).
fn facing(dir: Vec2) -> f32 {
    (-dir.x).atan2(-dir.y)
}

/// Distance from `p` to the nearest edge of a closed outline.
fn to_outline(pts: &[Vec2], p: Vec2) -> f32 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, c) = (pts[i], pts[(i + 1) % n]);
            let e = c - a;
            let t = ((p - a).dot(e) / e.length_squared().max(1e-6)).clamp(0.0, 1.0);
            p.distance(a + e * t)
        })
        .fold(f32::MAX, f32::min)
}

/// Free street for a piece `r` across at `p`: off the buildings and the enterable houses, out of the
/// water, clear of every block, tree and piece already placed.
fn street_free(map: &MapLayout, houses: &[Vec<Vec2>], p: Vec2, r: f32, half: f32) -> bool {
    if p.x.abs() > half - 6.0 || p.y.abs() > half - 6.0 {
        return false;
    }
    if matches!(map.terrain.ground_at(p.x, p.y), Some(ground::WATER)) {
        return false;
    }
    let near_outline = |pts: &[Vec2]| {
        let c = pts.iter().sum::<Vec2>() / pts.len() as f32;
        c.distance(p) < 90.0 && (super::footprint::contains(pts, p) || to_outline(pts, p) < r + 0.1)
    };
    if map.footprints.iter().any(|f| near_outline(&f.pts)) || houses.iter().any(|h| near_outline(h) || to_outline(h, p) < r + 2.0) {
        return false;
    }
    if map.blocks.iter().filter(|b| b.collide || b.visible).any(|b| {
        let ext = match b.shape {
            Shape::Box(s) => s.x.max(s.z) * 0.5,
            Shape::Cylinder { radius, .. } => radius,
            Shape::Sphere(r) => r,
        };
        b.pos.xz().distance(p) < ext + r + 0.3 && b.pos.y < 6.0
    }) {
        return false;
    }
    !map.props.iter().any(|q| q.pos.xz().distance(p) < r + 1.0) && !map.kit.iter().any(|k| k.pos.xz().distance(p) < r + reach(k.name) + 0.4)
}

/// Pieces along the roads: on each side of each stretch, a piece every 6-14 m just off the
/// carriageway, and a pole every ~30 m.
pub fn streets(b: &mut Builder, rng: &mut SmallRng, roads: &[(Vec<Vec2>, f32, bool)], houses: &[Vec<Vec2>], half: f32) {
    const STREET: [(&str, f32); 14] = [
        ("auto", 0.14),
        ("scooter", 0.08),
        ("thumper", 0.04),
        ("commuter", 0.05),
        ("discover", 0.03),
        ("twostroke", 0.02),
        ("sport150", 0.03),
        ("roadster", 0.04),
        ("thela", 0.11),
        ("chai", 0.07),
        ("drum", 0.09),
        ("crates", 0.11),
        ("chairs", 0.07),
        ("pot", 0.12),
    ];
    for (pts, width, minor) in roads {
        for seg in pts.windows(2) {
            let (a, c) = (seg[0], seg[1]);
            let len = a.distance(c);
            if len < 8.0 {
                continue;
            }
            let d = (c - a) / len;
            let n = Vec2::new(-d.y, d.x);
            for side in [-1.0f32, 1.0] {
                // Poles on one side of main roads.
                if side > 0.0 && !minor {
                    let mut t = rng.random_range(4.0..20.0);
                    while t < len - 4.0 {
                        let p = a + d * t + n * side * (width * 0.5 + 0.5);
                        if street_free(b.map, houses, p, 0.3, half) {
                            place(b, "pole", Vec3::new(p.x, b.map.terrain.height(p.x, p.y), p.y), facing(n));
                        }
                        t += rng.random_range(26.0..34.0);
                    }
                }
                let mut t = rng.random_range(2.0..8.0);
                while t < len - 3.0 {
                    let mut roll = rng.random_range(0.0..1.0);
                    let name = STREET.iter().find(|(_, w)| {
                        roll -= w;
                        roll < 0.0
                    }).map_or("pot", |(n, _)| *n);
                    let r = reach(name);
                    // On the lane's edge (Old Delhi parks and trades in the lane itself), never nearer its
                    // middle than 1.2 m, so a way stays open down it.
                    let p = a + d * t + n * side * (width * 0.5 - r * 0.4).max(1.2 + r * 0.5);
                    if street_free(b.map, houses, p, r, half) {
                        let yaw = match name {
                            // Parked along the kerb, either way round.
                            "auto" | "scooter" | "thela" | "thumper" | "commuter" | "discover" | "twostroke" | "sport150" | "roadster" => facing(d) + if rng.random_range(0.0..1.0) < 0.5 { 0.0 } else { std::f32::consts::PI },
                            // Stalls and chairs face the road.
                            "chai" | "chairs" => facing(-n * side),
                            _ => rng.random_range(0.0..std::f32::consts::TAU),
                        };
                        place(b, name, Vec3::new(p.x, b.map.terrain.height(p.x, p.y), p.y), yaw);
                    }
                    t += rng.random_range(4.0..10.0);
                }
            }
        }
    }
}

/// Roofs: a stair room on most bigger ones, and one to three things from roof life.
pub fn roofs(b: &mut Builder, rng: &mut SmallRng) {
    const ROOF: [&str; 8] = ["cooler", "dish", "solar", "clothesline", "pot", "chairs", "drum", "crates"];
    let roofs: Vec<(Vec<Vec2>, f32)> = b.map.footprints.iter().filter(|f| f.mat != Mat::Water && f.top - f.base > 3.0).map(|f| (f.pts.clone(), f.top)).collect();
    for (pts, top) in roofs {
        let area = (0..pts.len()).map(|i| pts[i].perp_dot(pts[(i + 1) % pts.len()])).sum::<f32>().abs() * 0.5;
        if area < 30.0 {
            continue;
        }
        let c = pts.iter().sum::<Vec2>() / pts.len() as f32;
        // Along the outline's longest edge.
        let (ea, eb) = (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1))).expect("outline");
        let along = (eb - ea).normalize_or(Vec2::X);
        let fits = |p: Vec2, r: f32| super::footprint::contains(&pts, p) && to_outline(&pts, p) > r + 0.5;
        let mut taken: Vec<(Vec2, f32)> = vec![];
        if area > 60.0 && rng.random_range(0.0..1.0) < 0.6 {
            for _ in 0..6 {
                let corner = pts[rng.random_range(0..pts.len())];
                let p = c.lerp(corner, rng.random_range(0.35..0.6));
                if fits(p, 2.1) {
                    place(b, "mumty", Vec3::new(p.x, top, p.y), facing(along) + if rng.random_range(0.0..1.0) < 0.5 { 0.0 } else { std::f32::consts::PI });
                    taken.push((p, 2.1));
                    break;
                }
            }
        }
        let count = 1 + (rng.random_range(0.0..1.0) * (area / 80.0).min(2.0)) as usize;
        for _ in 0..count {
            let name = ROOF[rng.random_range(0..ROOF.len())];
            let r = reach(name);
            for _ in 0..6 {
                let corner = pts[rng.random_range(0..pts.len())];
                let p = c.lerp(corner, rng.random_range(0.1..0.75));
                if fits(p, r) && !taken.iter().any(|(q, qr)| q.distance(p) < r + qr + 0.4) {
                    let yaw = if name == "clothesline" || name == "solar" { facing(along) } else { rng.random_range(0.0..std::f32::consts::TAU) };
                    place(b, name, Vec3::new(p.x, top, p.y), yaw);
                    taken.push((p, r));
                    break;
                }
            }
        }
    }
}
