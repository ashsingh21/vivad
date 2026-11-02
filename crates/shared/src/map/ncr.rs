//! Fictional, compressed NCR. North is -Z; dimensions are metres, not geographic coordinates.
//! Architecture and loot share a seeded recipe on every peer. No external map data is used.
use super::{
    build::{Builder, Frame},
    terrain::{Terrain, fbm},
    *,
};
use bevy::prelude::*;
use rand::{RngExt, SeedableRng};
use crate::SmallRng;
use std::f32::consts::{PI, TAU};

/// Horizontal world scale for the NCR production pass. The original blockout was 1 km
/// across; the playable arena is now 2.2 km across without increasing vertical traversal times.
///
/// The district and infrastructure recipes below are still written in the 1 km blockout's
/// coordinates ("raw"); `generate` scales every x and z by this at the end. Only the `pub`
/// constants and functions here speak world metres, for the simulation and the client.
pub const WORLD_SCALE: f32 = 2.2;
pub const HALF: f32 = 500.0 * WORLD_SCALE;
/// The three road bridges' z, in world metres.
pub const BRIDGES: [f32; 3] = [BRIDGES_RAW[0] * WORLD_SCALE, 0.0, BRIDGES_RAW[2] * WORLD_SCALE];
/// The same in the blockout's coordinates, for the recipes.
const BRIDGES_RAW: [f32; 3] = [-270.0, 0.0, 270.0];
/// Half the river's width in world metres: the water strips (`infrastructure`) and the terrain
/// channel are both this wide, scaled with the rest of the map.
const RIVER_HALF_WIDTH: f32 = 28.0 * WORLD_SCALE;
/// VIVAD_MAP names this map (for the tools that run before a layout exists).
pub fn enabled() -> bool {
    super::MapId::from_env() == super::MapId::Ncr
}
/// The river's centre line x at world z.
pub fn river_x(z: f32) -> f32 {
    river_x_raw(z / WORLD_SCALE) * WORLD_SCALE
}
/// The river's centre line in the blockout's coordinates.
fn river_x_raw(z: f32) -> f32 {
    16.0 * (z / 160.0).sin()
}
/// Distance (world metres) from the river's channel, 0 in it.
pub fn river_distance(p: Vec2) -> f32 {
    ((p.x - river_x(p.y)).abs() - RIVER_HALF_WIDTH).max(0.0)
}
fn world() -> Frame {
    Frame::new(Vec3::ZERO, 0.0)
}
fn poi(b: &mut Builder, name: &str, x: f32, z: f32, r: f32) {
    b.map.pois.push(Poi {
        name: name.into(),
        center: Vec3::new(x, 0.0, z),
        radius: r,
    });
}
fn loot(b: &mut Builder, p: Vec3, rare: bool) {
    b.map.chests.push(ChestSpawn {
        pos: p + Vec3::Y * 0.1,
        yaw: 0.0,
        kind: if rare {
            ChestKind::Kalash
        } else {
            ChestKind::Peti
        },
        set: None,
    });
}
fn line(b: &mut Builder, a: Vec3, c: Vec3, width: f32, height: f32, mat: Mat, solid: bool) {
    let d = c - a;
    b.push(
        (a + c) * 0.5,
        Quat::from_rotation_y((-d.z).atan2(d.x)),
        Shape::Box(Vec3::new(d.length(), height, width)),
        mat,
        solid,
        true,
    );
}
fn road(b: &mut Builder, a: Vec2, c: Vec2, width: f32) {
    let d = c - a;
    let dir = d.normalize_or(Vec2::X);
    let side = Vec2::new(-dir.y, dir.x);
    // 1. Roadway (Asphalt)
    line(b, a.extend(0.0).xzy(), c.extend(0.0).xzy(), width, 0.1, Mat::Asphalt, false);

    // 2. Road Markings (White Paint) - Center line
    line(b, a.extend(0.07).xzy(), c.extend(0.07).xzy(), 0.15, 0.01, Mat::PaintWhite, false);

    for sign in [-1.0, 1.0] {
        let s_dir = side * sign;
        let roadway_edge = s_dir * (width / 2.0);

        // Curb (Concrete) - 0.15m high, 0.2m wide
        line(b, (a + roadway_edge).extend(0.075).xzy(), (c + roadway_edge).extend(0.075).xzy(), 0.2, 0.15, Mat::Concrete, false);

        // Sidewalk - Clear Path (Concrete) - 2.0m wide
        let path_offset = roadway_edge + s_dir * (0.2 + 1.0);
        line(b, (a + path_offset).extend(0.15).xzy(), (c + path_offset).extend(0.15).xzy(), 2.0, 0.1, Mat::Concrete, false);

        // Sidewalk - Furnishing Zone (Concrete) - 1.5m wide
        let furn_offset = roadway_edge + s_dir * (0.2 + 2.0 + 1.5 / 2.0);
        line(b, (a + furn_offset).extend(0.15).xzy(), (c + furn_offset).extend(0.15).xzy(), 1.5, 0.1, Mat::Concrete, false);

        // Sidewalk - Frontage Zone (Concrete) - 0.6m wide
        let front_offset = roadway_edge + s_dir * (0.2 + 2.0 + 1.5 + 0.6 / 2.0);
        line(b, (a + front_offset).extend(0.15).xzy(), (c + front_offset).extend(0.15).xzy(), 0.6, 0.1, Mat::Concrete, false);

        // Spawn Clutter in Furnishing Zone
        spawn_urban_clutter(b, a, c, s_dir, roadway_edge + s_dir * (0.2 + 2.0));
    }
}

fn spawn_urban_clutter(b: &mut Builder, a: Vec2, c: Vec2, side: Vec2, offset_start: Vec2) {
    let d = c - a;
    let len = d.length();
    let seed = a.x.to_bits() as u64
        ^ (a.y.to_bits() as u64).rotate_left(13)
        ^ (c.x.to_bits() as u64).rotate_left(27)
        ^ (c.y.to_bits() as u64).rotate_left(41);
    let mut rng = SmallRng::seed_from_u64(seed);

    // Place clutter every 20-40m
    let mut curr = 0.0;
    while curr < len {
        curr += rng.random_range(20.0..40.0);
        if curr >= len { break; }

        let p = a + d.normalize() * curr;
        let clutter_pos = p + offset_start + side * rng.random_range(0.0..1.5);

        // Build the street furniture from the shared kit; these props have no glTF assets.
        let kind = rng.random_range(0..4);
        let frame = Frame::new(clutter_pos.extend(0.2).xzy(), rng.random_range(0.0..TAU));
        match kind {
            0 => {
                b.cyl(frame, Vec3::ZERO, 0.12, 0.9, Mat::Steel);
                b.deco_cyl(frame, Vec3::Y * 0.68, 0.125, 0.1, Mat::PaintWhite);
            }
            1 => {
                b.bx(frame, Vec3::Y * 0.45, Vec3::new(0.55, 0.9, 0.55), Mat::Steel);
                b.deco(frame, Vec3::Y * 0.92, Vec3::new(0.6, 0.06, 0.6), Mat::Rubber);
            }
            2 => {
                b.cyl(frame, Vec3::ZERO, 0.09, 5.0, Mat::Steel);
                b.deco(frame, Vec3::new(0.4, 4.9, 0.0), Vec3::new(0.9, 0.08, 0.08), Mat::Steel);
                b.deco(frame, Vec3::new(0.8, 4.85, 0.0), Vec3::new(0.3, 0.08, 0.22), Mat::PaintWhite);
            }
            _ => {
                b.bx(frame, Vec3::Y * 0.3, Vec3::splat(0.6), Mat::Wood);
            }
        }
    }
}
fn tree(b: &mut Builder, x: f32, z: f32, scale: f32) {
    // The terrain is already in world metres; the recipe's x and z are not yet.
    let y = b.map.terrain.height(x * WORLD_SCALE, z * WORLD_SCALE);
    b.map.props.push(Prop {
        model: "models/props/tree_neem.gltf",
        pos: Vec3::new(x, y, z),
        yaw: x * 0.2,
        scale,
        trunk: None,
    });
}
/// Solid shell towers use a single collider. Window ribbons, ACs and trims are visual only.
fn tower(b: &mut Builder, f: Frame, w: f32, d: f32, h: f32, glass: bool) {
    super::ncr_art::tower(b,f,w,d,h,glass);
    let mat = if glass { Mat::Glass } else { Mat::PlasterOchre };
    b.bx(f, Vec3::new(0.0, h / 2.0, 0.0), Vec3::new(w, h, d), mat);
    for floor in 1..(h / 3.8) as usize {
        let y = floor as f32 * 3.8;
        for z in [-d / 2.0 - 0.04, d / 2.0 + 0.04] {
            b.deco(
                f,
                Vec3::new(0.0, y, z),
                Vec3::new(w + 0.15, if glass { 0.18 } else { 1.3 }, 0.08),
                if glass { Mat::Steel } else { Mat::Glass },
            );
        }
        for x in [-w / 2.0 - 0.04, w / 2.0 + 0.04] {
            b.deco(
                f,
                Vec3::new(x, y, 0.0),
                Vec3::new(0.08, if glass { 0.18 } else { 1.3 }, d + 0.15),
                if glass { Mat::Steel } else { Mat::Glass },
            );
        }
    }
    for i in 1..(w / 5.0) as usize {
        let x = -w / 2.0 + i as f32 * 5.0;
        for z in [-d / 2.0 - 0.09, d / 2.0 + 0.09] {
            b.deco(
                f,
                Vec3::new(x, h / 2.0, z),
                Vec3::new(0.18, h, 0.12),
                Mat::Steel,
            );
        }
    }
    b.bx(
        f,
        Vec3::new(0.0, h + 0.15, 0.0),
        Vec3::new(w + 0.5, 0.3, d + 0.5),
        Mat::Concrete,
    );
    b.bx(
        f,
        Vec3::new(w * 0.25, h + 0.8, d * 0.2),
        Vec3::new(3.0, 1.3, 2.0),
        Mat::Steel,
    );
    b.deco_cyl(
        f,
        Vec3::new(-w * 0.22, h + 0.3, -d * 0.2),
        1.3,
        1.8,
        Mat::PlasterBlue,
    );
    loot(b, f.p(Vec3::new(0.0, h + 0.35, 0.0)), glass);
}
/// Two open sides, a roof with a stairwell, and a wide external stair: a legible enterable kit.
fn pavilion(b: &mut Builder, f: Frame, w: f32, d: f32, h: f32, mat: Mat) {
    super::ncr_art::pavilion(b,f,w,d,h,mat);
    b.bx(
        f,
        Vec3::new(0.0, -0.1, 0.0),
        Vec3::new(w, 0.2, d),
        Mat::Concrete,
    );
    for x in [-w / 2.0 + 0.3, w / 2.0 - 0.3] {
        b.bx(f, Vec3::new(x, h / 2.0, 0.0), Vec3::new(0.5, h, d), mat);
    }
    for x in [-w * 0.3, w * 0.3] {
        for z in [-d / 2.0 + 0.5, d / 2.0 - 0.5] {
            b.bx(
                f,
                Vec3::new(x, h / 2.0, z),
                Vec3::new(0.5, h, 0.5),
                Mat::Concrete,
            );
        }
    }
    b.bx(
        f,
        Vec3::new(0.0, h, 0.0),
        Vec3::new(w, 0.3, d),
        Mat::Concrete,
    );
    // Cyan lintels consistently identify traversable interiors.
    for z in [-d / 2.0 - 0.06, d / 2.0 + 0.06] {
        b.deco(
            f,
            Vec3::new(0.0, 2.8, z),
            Vec3::new(w - 1.0, 0.25, 0.1),
            Mat::TransitBlue,
        );
    }
    b.stairs(
        f,
        Vec3::new(-w / 2.0 - h * 1.8, 0.0, d / 2.0 - 2.0),
        h * 1.8,
        h + 0.15,
        3.0,
        Mat::Concrete,
    );
    loot(b, f.p(Vec3::new(0.0, 0.25, 0.0)), false);
    loot(b, f.p(Vec3::new(w * 0.25, h + 0.2, 0.0)), true);
}
fn terrain(seed: u64) -> Terrain {
    let n = 257;
    let mut t = Terrain {
        n,
        size: 1040.0 * WORLD_SCALE,
        h: vec![0.0; n * n],
        road_d: vec![100.0; n * n],
        ground: vec![],
    };
    // The samples are in world metres (the terrain is never scaled afterwards), so the recipe's
    // features are matched in the blockout's coordinates. Heights stay in metres.
    for i in 0..n {
        for j in 0..n {
            let (x, z) = (t.coord(i) / WORLD_SCALE, t.coord(j) / WORLD_SCALE);
            let edge = ((-x - 405.0) / 100.0).clamp(0.0, 1.0);
            let mut h = edge
                * edge
                * (22.0 + 28.0 * fbm(x / 70.0, z / 70.0, 3, (seed as u32) & 0x7fff_ffff));
            let rd = (x - river_x_raw(z)).abs();
            if rd < 32.0 {
                h = -3.5 * (1.0 - ((rd - 24.0) / 8.0).clamp(0.0, 1.0));
            }
            if x > 82.0 && x < 136.0 && z > 335.0 && z < 455.0 {
                h = -0.7;
            }
            // Continuous excavated metro trench; a roof slab restores the street above.
            if x.abs() < 170.0 && z.abs() < 13.0 {
                h = -8.6;
            }
            t.h[i * n + j] = h;
        }
    }
    t
}
fn delhi(b: &mut Builder, rng: &mut SmallRng) {
    let c = Vec3::new(-235.0, 0.0, -235.0);
    poi(b, "Delhi / Connaught Place", c.x, c.z, 100.0);
    // Two segmented circular carriageways and twelve arcades, with radial escape routes.
    for radius in [46.0, 87.0] {
        for i in 0..64 {
            let a = TAU * i as f32 / 64.0;
            let d = TAU * (i + 1) as f32 / 64.0;
            road(
                b,
                c.xz() + Vec2::new(a.cos(), a.sin()) * radius,
                c.xz() + Vec2::new(d.cos(), d.sin()) * radius,
                8.0,
            );
        }
    }
    for i in 0..12 {
        let a = TAU * i as f32 / 12.0;
        let f = Frame::new(c + Vec3::new(a.cos(), 0.0, a.sin()) * 66.0, -a + PI / 2.0);
        pavilion(b, f, 24.0, 16.0, 7.0, Mat::Plaster);
    }
    for i in 0..12 {
        let a = TAU * i as f32 / 12.0;
        tree(b, c.x + a.cos() * 32.0, c.z + a.sin() * 32.0, 0.8);
    }
    poi(b, "Old Delhi / Rooftop Lanes", -255.0, -395.0, 65.0);
    for ix in 0..7 {
        for iz in 0..3 {
            let (x, z) = (-360.0 + ix as f32 * 32.0, -440.0 + iz as f32 * 29.0);
            let h = rng.random_range(6.0..13.0);
            tower(
                b,
                Frame::new(Vec3::new(x, 0.0, z), 0.0),
                22.0,
                18.0,
                h,
                false,
            );
            if iz == 1 {
                b.stairs(
                    world(),
                    Vec3::new(x - 11.0 - h * 1.7, 0.0, z + 7.0),
                    h * 1.7,
                    h + 0.3,
                    2.0,
                    Mat::Concrete,
                );
            }
            loot(b, Vec3::new(x + 13.0, 0.2, z), false);
        }
    }
    poi(b, "Delhi / Civic Market", -280.0, -75.0, 70.0);
    for ix in 0..7 {
        let x = -380.0 + ix as f32 * 39.0;
        pavilion(
            b,
            Frame::new(Vec3::new(x, 0.0, -70.0), 0.0),
            20.0,
            26.0,
            4.5,
            Mat::PlasterOchre,
        );
        tree(b, x, -115.0, 0.7);
    }
    road(
        b,
        Vec2::new(-390.0, -100.0),
        Vec2::new(-100.0, -100.0),
        10.0,
    );
    road(b, Vec2::new(-100.0, -355.0), Vec2::new(-100.0, -25.0), 12.0);
    road(b, Vec2::new(-390.0, -355.0), Vec2::new(-90.0, -355.0), 10.0);
}
fn gurugram(b: &mut Builder, rng: &mut SmallRng) {
    poi(b, "Gurugram / Cyber Hub", -245.0, 190.0, 100.0);
    for ix in 0..4 {
        for iz in 0..3 {
            let (x, z) = (-350.0 + ix as f32 * 63.0, 95.0 + iz as f32 * 65.0);
            tower(
                b,
                Frame::new(Vec3::new(x, 0.0, z), 0.0),
                29.0,
                30.0,
                rng.random_range(32.0..72.0),
                true,
            );
        }
    }
    for z in [58.0, 130.0, 195.0, 260.0] {
        road(b, Vec2::new(-390.0, z), Vec2::new(-90.0, z), 14.0);
    }
    for x in [-390.0, -315.0, -252.0, -189.0, -90.0] {
        road(b, Vec2::new(x, 40.0), Vec2::new(x, 460.0), 12.0);
    }
    poi(b, "Skycourt Mall", -225.0, 330.0, 45.0);
    pavilion(
        b,
        Frame::new(Vec3::new(-225.0, 0.0, 330.0), 0.0),
        65.0,
        38.0,
        9.0,
        Mat::Glass,
    );
    pavilion(
        b,
        Frame::new(Vec3::new(-335.0, 0.0, 330.0), 0.0),
        35.0,
        35.0,
        6.0,
        Mat::Concrete,
    );
    poi(b, "Ridge Golf / Launch Terrace", -265.0, 425.0, 65.0);
    for i in 0..7 {
        let x = -360.0 + i as f32 * 38.0;
        tree(b, x, 470.0, 1.2);
        b.deco_cyl(world(), Vec3::new(x, 0.03, 425.0), 12.0, 0.05, Mat::Grass);
    }
}
fn noida(b: &mut Builder, rng: &mut SmallRng) {
    poi(b, "Noida / Sector 18", 245.0, 150.0, 100.0);
    for x in [90.0, 170.0, 250.0, 330.0, 410.0] {
        road(b, Vec2::new(x, 45.0), Vec2::new(x, 350.0), 16.0);
    }
    for z in [60.0, 140.0, 220.0, 300.0] {
        road(b, Vec2::new(75.0, z), Vec2::new(460.0, z), 16.0);
    }
    for ix in 0..4 {
        for iz in 0..3 {
            let (x, z) = (130.0 + ix as f32 * 80.0, 100.0 + iz as f32 * 80.0);
            if (ix + iz) % 3 == 0 {
                pavilion(
                    b,
                    Frame::new(Vec3::new(x, 0.0, z), 0.0),
                    30.0,
                    24.0,
                    5.0,
                    Mat::Plaster,
                );
            } else {
                tower(
                    b,
                    Frame::new(Vec3::new(x, 0.0, z), 0.0),
                    32.0,
                    26.0,
                    rng.random_range(9.0..23.0),
                    ix % 2 == 0,
                );
            }
            tree(b, x + 23.0, z + 18.0, 0.8);
            loot(b, Vec3::new(x - 22.0, 0.2, z), false);
        }
    }
    poi(b, "Film City / Backlots", 350.0, 395.0, 70.0);
    for x in [290.0, 360.0, 430.0] {
        pavilion(
            b,
            Frame::new(Vec3::new(x, 0.0, 400.0), 0.0),
            40.0,
            55.0,
            8.0,
            Mat::Brick,
        );
    }
    poi(b, "Okhla / Wetland Walk", 135.0, 400.0, 65.0);
    for i in 0..5 {
        let z = 345.0 + i as f32 * 25.0;
        b.deco(
            world(),
            Vec3::new(110.0, -0.01, z),
            Vec3::new(55.0, 0.04, 20.0),
            Mat::Water,
        );
        tree(b, 155.0, z, 0.9);
    }
    line(
        b,
        Vec3::new(138.0, 0.35, 330.0),
        Vec3::new(138.0, 0.35, 470.0),
        4.0,
        0.3,
        Mat::Wood,
        true,
    );
}
fn ghaziabad(b: &mut Builder, rng: &mut SmallRng) {
    poi(b, "Ghaziabad / Vaishali", 180.0, -365.0, 80.0);
    for ix in 0..4 {
        for iz in 0..2 {
            tower(
                b,
                Frame::new(
                    Vec3::new(115.0 + ix as f32 * 55.0, 0.0, -435.0 + iz as f32 * 62.0),
                    0.0,
                ),
                25.0,
                27.0,
                rng.random_range(23.0..46.0),
                false,
            );
        }
    }
    poi(b, "Sahibabad / Freight Yard", 295.0, -165.0, 120.0);
    for x in [110.0, 145.0, 180.0] {
        for dx in [-1.0, 1.0] {
            line(
                b,
                Vec3::new(x + dx, 0.1, -300.0),
                Vec3::new(x + dx, 0.1, -45.0),
                0.12,
                0.2,
                Mat::Steel,
                false,
            );
        }
        for i in 0..35 {
            b.deco(
                world(),
                Vec3::new(x, 0.02, -300.0 + i as f32 * 7.2),
                Vec3::new(3.0, 0.12, 0.5),
                Mat::Wood,
            );
        }
    }
    for ix in 0..12 {
        for iz in 0..14 {
            if iz == 4 || iz == 9 || ix == 5 {
                continue;
            }
            let (x, z) = (215.0 + ix as f32 * 20.0, -295.0 + iz as f32 * 16.0);
            let h = if (ix + iz) % 3 == 0 { 5.4 } else { 2.7 };
            b.bx(
                world(),
                Vec3::new(x, h / 2.0, z),
                Vec3::new(12.0, h, 5.0),
                if ix % 2 == 0 {
                    Mat::TransitBlue
                } else {
                    Mat::ContainerRed
                },
            );
            for j in 0..6 {
                b.deco(
                    world(),
                    Vec3::new(x - 5.0 + j as f32 * 2.0, h / 2.0, z - 2.55),
                    Vec3::new(0.12, h, 0.1),
                    Mat::Steel,
                );
            }
            if iz % 3 == 0 {
                loot(b, Vec3::new(x + 8.0, 0.15, z), false);
            }
        }
    }
    for x in [245.0, 350.0] {
        pavilion(
            b,
            Frame::new(Vec3::new(x, 0.0, -40.0), 0.0),
            65.0,
            30.0,
            9.0,
            Mat::Concrete,
        );
    }
    road(b, Vec2::new(75.0, -330.0), Vec2::new(465.0, -330.0), 16.0);
}
fn infrastructure(b: &mut Builder) {
    // Thin water strips follow the channel. Non-colliding: the shallow bed remains walkable.
    for i in 0..100 {
        let z = -495.0 + i as f32 * 10.0;
        b.deco(
            world(),
            Vec3::new(river_x_raw(z), -1.05, z),
            Vec3::new(2.0 * RIVER_HALF_WIDTH / WORLD_SCALE, 0.03, 10.05),
            Mat::Water,
        );
    }
    for z in BRIDGES_RAW {
        if z == 0.0 {
            road(b, Vec2::new(-450.0, z), Vec2::new(-180.0, z), 18.0);
            road(b, Vec2::new(180.0, z), Vec2::new(465.0, z), 18.0);
        } else {
            road(
                b,
                Vec2::new(if z < 0.0 { -100.0 } else { -450.0 }, z),
                Vec2::new(-75.0, z),
                18.0,
            );
            road(b, Vec2::new(75.0, z), Vec2::new(465.0, z), 18.0);
        }
        // Keep the visual deck continuous, but use short collision panels. A 130 m thin
        // collider can stop the capsule's tangent sweep despite its unobstructed top.
        b.deco(
            world(),
            Vec3::new(0.0, 1.35, z),
            Vec3::new(130.0, 0.5, 18.0),
            Mat::Concrete,
        );
        for panel in 0..13 {
            b.push(
                Vec3::new(-60.0 + panel as f32 * 10.0, 1.35, z),
                Quat::IDENTITY,
                Shape::Box(Vec3::new(10.0, 0.5, 18.0)),
                Mat::Concrete,
                true,
                false,
            );
        }
        b.ramp(
            world(),
            Vec3::new(-85.0, 0.12, z),
            20.0,
            1.48,
            18.0,
            0.3,
            Mat::Asphalt,
        );
        b.ramp(
            Frame::new(Vec3::new(85.0, 0.12, z), PI),
            Vec3::ZERO,
            20.0,
            1.48,
            18.0,
            0.3,
            Mat::Asphalt,
        );
        for x in [-48.0, -16.0, 16.0, 48.0] {
            b.bx(
                world(),
                Vec3::new(x, -1.0, z),
                Vec3::new(2.5, 4.0, 13.0),
                Mat::Concrete,
            );
            for dz in [-7.0, 7.0] {
                b.bx(
                    world(),
                    Vec3::new(x, 2.15, z + dz),
                    Vec3::new(4.0, 1.1, 0.7),
                    Mat::Concrete,
                );
            }
        }
        poi(
            b,
            if z < 0.0 {
                "North Bridge"
            } else if z > 0.0 {
                "South Bridge"
            } else {
                "Central Interchange"
            },
            0.0,
            z,
            35.0,
        );
    }
    // Blue Line: excavated walkable underpass with daylight entrances and a continuous canopy.
    let f = world();
    b.bx(
        f,
        Vec3::new(0.0, -8.25, 0.0),
        Vec3::new(340.0, 0.5, 24.0),
        Mat::Concrete,
    );
    for z in [-11.5, 11.5] {
        b.bx(
            f,
            Vec3::new(0.0, -4.3, z),
            Vec3::new(340.0, 8.0, 0.6),
            Mat::Concrete,
        );
    }
    b.bx(
        f,
        Vec3::new(0.0, -2.8, 0.0),
        Vec3::new(260.0, 0.5, 24.0),
        Mat::Concrete,
    );
    // Fill between tunnel roof and ground on land; the river is allowed above the submerged roof.
    for x in [-95.0, 95.0] {
        b.bx(
            f,
            Vec3::new(x, -1.25, 0.0),
            Vec3::new(70.0, 2.5, 24.0),
            Mat::Concrete,
        );
    }
    for s in [-1.0, 1.0] {
        let f = Frame::new(
            Vec3::new(s * 130.0, -8.0, 0.0),
            if s > 0.0 { 0.0 } else { PI },
        );
        b.stairs(f, Vec3::ZERO, 40.0, 8.4, 8.0, Mat::Concrete);
        b.bx(
            world(),
            Vec3::new(s * 175.0, 0.15, 0.0),
            Vec3::new(10.0, 0.5, 8.0),
            Mat::Concrete,
        );
        b.deco(
            f,
            Vec3::new(0.0, 2.5, -10.9),
            Vec3::new(8.0, 0.4, 0.15),
            Mat::TransitBlue,
        );
        loot(b, Vec3::new(s * 115.0, -7.9, 7.0), true);
    }
    for x in [-110.0, -70.0, -30.0, 30.0, 70.0, 110.0] {
        b.map.lamps.push(Lamp {
            pos: Vec3::new(x, -4.0, 0.0),
            color: Vec3::new(0.65, 0.82, 1.0),
            lumens: 1600.0,
            range: 23.0,
            flames: None,
        });
        b.deco(
            f,
            Vec3::new(x, -3.1, 0.0),
            Vec3::new(5.0, 0.08, 0.3),
            Mat::TransitBlue,
        );
    }
    poi(b, "Blue Line / River Underpass", -125.0, 0.0, 20.0);
    // Yellow Line viaduct: static platforms, with stairs and cover; no train simulation yet.
    line(
        b,
        Vec3::new(-80.0, 8.0, 65.0),
        Vec3::new(-80.0, 8.0, 455.0),
        8.0,
        0.6,
        Mat::Concrete,
        true,
    );
    for z in (75..455).step_by(30) {
        b.bx(
            world(),
            Vec3::new(-80.0, 4.0, z as f32),
            Vec3::new(2.0, 8.0, 2.0),
            Mat::Concrete,
        );
    }
    for z in [85.0, 310.0, 440.0] {
        b.bx(
            world(),
            Vec3::new(-80.0, 8.0, z),
            Vec3::new(17.0, 0.6, 25.0),
            Mat::Concrete,
        );
        b.stairs(
            world(),
            Vec3::new(-112.0, 0.0, z),
            24.0,
            8.7,
            4.0,
            Mat::Concrete,
        );
        b.deco(
            world(),
            Vec3::new(-80.0, 9.5, z + 12.0),
            Vec3::new(17.0, 0.4, 0.3),
            Mat::TransitYellow,
        );
        loot(b, Vec3::new(-80.0, 8.4, z), false);
    }
}
pub fn generate(seed: u64) -> MapLayout {
    let mut map = MapLayout {
        name: "Dilli NCR".into(),
        style: MapStyle::Ncr,
        river: River::Sine { x0: 0.0, amp: 16.0 * WORLD_SCALE, wavelength: 160.0 * WORLD_SCALE, half_width: RIVER_HALF_WIDTH },
        half_extent: HALF,
        terrain: terrain(seed),
        ..default()
    };
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut b = Builder::new(&mut map);
    delhi(&mut b, &mut rng);
    gurugram(&mut b, &mut rng);
    noida(&mut b, &mut rng);
    ghaziabad(&mut b, &mut rng);
    infrastructure(&mut b);
    super::ncr_art::streets(&mut b);
    for x in [-370.0, -110.0, 60.0, 475.0] {
        for z in (-460..461).step_by(35) {
            let z = z as f32;
            if z.abs() < 30.0 {
                continue;
            }
            tree(&mut b, x, z, 0.65);
            let f = Frame::new(Vec3::new(x + 4.0, 0.0, z), 0.0);
            b.deco(
                f,
                Vec3::new(0.0, 3.5, 0.0),
                Vec3::new(0.12, 7.0, 0.12),
                Mat::Steel,
            );
            b.deco(
                f,
                Vec3::new(1.0, 7.0, 0.0),
                Vec3::new(2.1, 0.15, 0.4),
                Mat::Concrete,
            );
        }
    }
    for x in [183.0, 343.0, 423.0] {
        for z in [82.0, 162.0, 242.0] {
            tree(&mut b, x, z, 0.65);
        }
    }

    // Public-space cover makes rotations readable without obstructing road centre lines.
    for x in [-390.0, -100.0, 75.0, 465.0] {
        for z in (-450..451).step_by(45) {
            if BRIDGES_RAW
                .iter()
                .any(|bridge| (z as f32 - bridge).abs() < 15.0)
            {
                continue;
            }
            b.bx(
                world(),
                Vec3::new(x, 0.6, z as f32),
                Vec3::new(3.5, 1.2, 1.8),
                Mat::Concrete,
            );
            b.map.floor_loot.push(Vec3::new(x + 3.0, 0.3, z as f32));
        }
    }
    for (p, s) in [
        (Vec3::new(-502.0, 80.0, 0.0), Vec3::new(4.0, 240.0, 1008.0)),
        (Vec3::new(502.0, 80.0, 0.0), Vec3::new(4.0, 240.0, 1008.0)),
        (Vec3::new(0.0, 80.0, -502.0), Vec3::new(1008.0, 240.0, 4.0)),
        (Vec3::new(0.0, 80.0, 502.0), Vec3::new(1008.0, 240.0, 4.0)),
    ] {
        b.push(p, Quat::IDENTITY, Shape::Box(s), Mat::Concrete, true, false);
    }
    scale_horizontal(&mut map);
    map
}

fn scale_horizontal(map: &mut MapLayout) {
    let scale = WORLD_SCALE;
    let xz = |p: &mut Vec3| {
        p.x *= scale;
        p.z *= scale;
    };
    for block in map.blocks.iter_mut().chain(map.doors.iter_mut()) {
        xz(&mut block.pos);
        match &mut block.shape {
            Shape::Box(size) => {
                let up = block.rot * Vec3::Y;
                if up.y > 0.999 {
                    // Upright: its local x and z lie in the ground plane.
                    size.x *= scale;
                    size.z *= scale;
                } else {
                    // A ramp or a stair's collider (build::ramp): local x runs up the slope, local
                    // z is horizontal. Stretching the run and keeping the rise gives a longer,
                    // shallower slope; the thickness (local y) is left alone.
                    let along = block.rot * Vec3::X;
                    let stretched = Vec3::new(along.x * scale, along.y, along.z * scale);
                    size.x *= stretched.length();
                    size.z *= scale;
                    let across = block.rot * Vec3::Z;
                    let x = stretched.normalize();
                    let y = across.cross(x).normalize();
                    block.rot = Quat::from_mat3(&Mat3::from_cols(x, y, across));
                }
            }
            Shape::Cylinder { radius, .. } | Shape::Sphere(radius) => *radius *= scale,
        }
    }
    for chest in &mut map.chests {
        xz(&mut chest.pos);
    }
    for position in &mut map.floor_loot {
        xz(position);
    }
    for poi in &mut map.pois {
        xz(&mut poi.center);
        poi.radius *= scale;
    }
    for prop in &mut map.props {
        xz(&mut prop.pos);
        prop.scale *= scale;
    }
    for ornament in &mut map.ornaments {
        xz(&mut ornament.pos);
        ornament.scale.x *= scale;
        ornament.scale.z *= scale;
    }
    for lamp in &mut map.lamps {
        xz(&mut lamp.pos);
        lamp.range *= scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_and_bounded() {
        for seed in [0, 7, 42, u64::MAX] {
            let a = generate(seed);
            let b = generate(seed);
            assert_eq!(a.blocks.len(), b.blocks.len());
            for (a, b) in a.blocks.iter().zip(&b.blocks) {
                assert_eq!(a.pos, b.pos);
                assert_eq!(a.shape, b.shape);
                assert!(a.pos.is_finite());
            }
            assert!(a.chests.len() > 100);
            assert!(a.blocks.len() < 25000);
            for p in a
                .chests
                .iter()
                .map(|c| c.pos)
                .chain(a.floor_loot.iter().copied())
            {
                assert!(p.x.abs() < HALF && p.z.abs() < HALF);
            }
            assert!(a.pois.iter().any(|p| p.name.starts_with("Delhi")));
            assert!(a.pois.iter().any(|p| p.name.starts_with("Gurugram")));
            assert!(a.pois.iter().any(|p| p.name.starts_with("Noida")));
            assert!(a.pois.iter().any(|p| p.name.starts_with("Ghaziabad")));
            // Every place and every solid is inside the walls (the backdrop past them is
            // visual only).
            for p in &a.pois {
                assert!(p.center.x.abs() < HALF && p.center.z.abs() < HALF, "{} is outside the map", p.name);
            }
            for b in a.blocks.iter().filter(|b| b.collide) {
                assert!(b.pos.x.abs() <= HALF + 5.0 && b.pos.z.abs() <= HALF + 5.0, "solid outside the map at {:?}", b.pos);
            }
        }
    }

    #[test]
    fn scaled_layout_matches_terrain() {
        let m = generate(7);
        // The bridges are built where the simulation and the router expect them.
        for z in BRIDGES {
            assert!(m.blocks.iter().any(|b| b.collide && b.pos.x.abs() < 1.0 && (b.pos.z - z).abs() < 1.0), "no bridge at z {z}");
        }
        // The districts stand on flat ground; only the western ridge rises.
        for (x, z) in [(-235.0, -235.0), (-245.0, 190.0), (245.0, 150.0), (180.0, -365.0), (-390.0, 58.0)] {
            let h = m.terrain.height(x * WORLD_SCALE, z * WORLD_SCALE);
            assert!(h.abs() < 0.5, "ground at ({x}, {z}) is {h} m");
        }
        assert!(m.terrain.height(-480.0 * WORLD_SCALE, 0.0) > 10.0);
        // The river channel is where the water is drawn and where `river_distance` says (away
        // from the metro trench at z = 0).
        for z in [-400.0, -100.0, 150.0, 400.0] {
            let x = river_x(z);
            assert!(m.terrain.height(x, z) < -3.0);
            assert_eq!(river_distance(Vec2::new(x, z)), 0.0);
            assert!(m.terrain.height(x + RIVER_HALF_WIDTH + 30.0, z) >= -0.01);
            assert!(m.blocks.iter().any(|b| b.mat == Mat::Water && (b.pos.z - z).abs() < 12.0 && (b.pos.x - x).abs() < 2.0));
        }
        // The metro trench lies under the tunnel floor, and the street is level beside it.
        assert!(m.terrain.height(0.0, 0.0) < -8.0);
        assert!(m.terrain.height(100.0 * WORLD_SCALE, 0.0) < -8.0);
        assert!(m.terrain.height(100.0 * WORLD_SCALE, 40.0 * WORLD_SCALE).abs() < 0.5);
    }
}
