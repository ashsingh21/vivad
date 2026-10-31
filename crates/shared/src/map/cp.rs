//! Connaught Place's ring of blocks: white two-storey blocks round a park, cut by radial streets,
//! each colonnaded on both faces over a ring of shops, with a storey of windows above, stairs up
//! through a shop now and then, and a roof behind parapets. Built for the India map (scaled down)
//! and for Delhi (at the real radii, from OpenStreetMap: crate::map::delhi).

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use rand::RngExt;
use rand::rngs::SmallRng;

use super::build::{Builder, Frame, Opening};
use super::{ChestKind, Mat};

/// Slab thickness.
const SLAB: f32 = 0.35;

pub struct Ring {
    pub center: Vec3,
    /// The park's radius (its flag and fountain at the centre).
    pub park: f32,
    /// Inner (park side) and outer column rows.
    pub ri: f32,
    pub ro: f32,
    /// Verandah depth behind each column row, to the shop fronts.
    pub ver: f32,
    /// Ground storey (to the top of the first-floor slab) and upper storey.
    pub g: f32,
    pub u: f32,
    /// About how wide one shop front is, along the inner row.
    pub seg_len: f32,
    /// The radial streets' width at the inner row, and the bearings of their middles (angles from
    /// +X towards +Z): a block stands between each street and the next.
    pub street: f32,
    pub streets: Vec<f32>,
    /// Height of the flagpole in the park.
    pub flag: f32,
}

impl Ring {
    /// A point on the circle of radius `r` at angle `a` (from +X towards +Z), height `y`.
    pub fn at(&self, r: f32, a: f32, y: f32) -> Vec3 {
        self.center + Vec3::new(r * a.cos(), y, r * a.sin())
    }

    /// The radial streets, from the park out past the blocks (for painting and levelling).
    pub fn street_lines(&self) -> Vec<(Vec2, Vec2)> {
        self.streets.iter().map(|&a| (self.at(self.park, a, 0.0).xz(), self.at(self.ro + 8.0, a, 0.0).xz())).collect()
    }
}

pub fn build(b: &mut Builder, rng: &mut SmallRng, r: &Ring) {
    let w = Frame::new(Vec3::ZERO, 0.0);
    let tau = std::f32::consts::TAU;
    let half_gap = (r.street / 2.0 / r.ri).asin();
    // The park: a fountain basin and the tall flag at its heart.
    let c = r.center;
    b.cyl(w, c, 3.2, 0.55, Mat::Marble);
    b.deco_cyl(w, c + Vec3::Y * 0.55, 0.6, 1.2, Mat::Marble);
    b.cyl(w, c, 0.22 * (r.flag / 32.0).max(1.0), r.flag, Mat::Granite);
    let k = r.flag / 32.0;
    for (i, mat) in [Mat::PlasterOchre, Mat::Plaster, Mat::Grass].into_iter().enumerate() {
        b.push(c + Vec3::new(3.6 * k, r.flag - 1.0 * k - i as f32 * 1.3 * k, 0.0), Quat::IDENTITY,
               super::Shape::Box(Vec3::new(6.6 * k, 1.3 * k, 0.06)), mat, false, true);
    }
    for i in 0..4 {
        b.loot(w, r.at(r.park * 0.6, tau * (i as f32 + 0.5) / 4.0, 0.3));
    }
    let mut streets = r.streets.clone();
    streets.sort_by(f32::total_cmp);
    // Deep blocks are two rows of shops back to back, one facing each colonnade.
    let (core0, core1) = (r.ri + r.ver, r.ro - r.ver);
    let split = core1 - core0 > 14.0;
    let mid = (core0 + core1) / 2.0;
    for blk in 0..streets.len() {
        let a0 = streets[blk] + half_gap;
        let a1 = streets[(blk + 1) % streets.len()] + if blk + 1 == streets.len() { tau } else { 0.0 } - half_gap;
        let n = (((a1 - a0) * r.ri) / r.seg_len).round().max(4.0) as usize;
        let ang = |t: f32| a0 + (a1 - a0) * t;
        let segs: Vec<(f32, f32)> = (0..n).map(|i| (ang(i as f32 / n as f32), ang((i + 1) as f32 / n as f32))).collect();
        let xz = |rad: f32, a: f32| r.at(rad, a, 0.0).xz();
        // Stairs every ten shops or so (at least one per block), in the middle of each run.
        let stairs = |i: usize| i % 10 == 5 || (n <= 10 && i == n / 2);
        for (i, &(s0, s1)) in segs.iter().enumerate() {
            let am = (s0 + s1) / 2.0;
            // Alternate segments sit 2 cm apart in height, so where neighbouring slabs overlap
            // (boxes on a curve) their faces never share a plane.
            let lift = if i % 2 == 0 { 0.0 } else { 0.02 };
            let radial = Frame::new(c, -am);
            let chord_at = |rad: f32| 2.0 * rad * ((s1 - s0) / 2.0).sin();
            let slab = |b: &mut Builder, r0: f32, r1: f32, top: f32| {
                b.bx(radial, Vec3::new((r0 + r1) / 2.0, top - SLAB / 2.0, 0.0), Vec3::new(r1 - r0, SLAB, chord_at(r1) + 0.08), Mat::Plaster);
            };
            // Marble underfoot (3 cm proud of the levelled ground: no step to trip on, and no
            // grass through the verandahs), then the first floor (the verandah ceilings and the
            // shops'), with the stair well left open.
            b.bx(radial, Vec3::new((r.ro + r.ri) / 2.0, -0.1 + lift / 2.0, 0.0), Vec3::new(r.ro - r.ri + 1.2, 0.26, chord_at(r.ro + 0.6) + 0.08), Mat::Marble);
            slab(b, r.ri - 0.6, core0, r.g + lift);
            slab(b, core1, r.ro + 0.6, r.g + lift);
            let well_to = if split { mid } else { core1 };
            if stairs(i) {
                slab(b, well_to, core1, r.g + lift);
            } else {
                slab(b, core0, core1, r.g + lift);
            }
            // Roof, its parapets and cornices (their tops 5 cm under the slabs' they run along).
            slab(b, r.ri - 0.3, r.ro + 0.3, r.g + r.u + lift);
            for (rad, out) in [(r.ro + 0.15, 1.0f32), (r.ri - 0.15, -1.0)] {
                b.wall(w, xz(rad, s0), xz(rad, s1), r.g + r.u + lift, 1.0, 0.3, Mat::Plaster, &[]);
                b.deco(radial, Vec3::new(rad + out * 0.25, r.g + r.u - 0.2 + lift, 0.0), Vec3::new(0.5, 0.3, chord_at(rad) + 0.1), Mat::Plaster);
                b.deco(radial, Vec3::new(rad + out * 0.3, r.g - 0.25 + lift, 0.0), Vec3::new(0.6, 0.4, chord_at(rad) + 0.1), Mat::Plaster);
            }
            // Columns at the start of each segment (and the block's last), on both faces.
            for (col, last) in [(s0, false), (s1, true)] {
                if last && i + 1 != n {
                    continue;
                }
                for rad in [r.ro - 0.45, r.ri + 0.45] {
                    let base = r.at(rad, col, 0.0);
                    b.push(base + Vec3::Y * 0.2, Quat::from_rotation_y(-col), super::Shape::Box(Vec3::new(0.8, 0.4, 0.8)), Mat::Plaster, false, true);
                    b.push(base + Vec3::Y * (r.g - SLAB) / 2.0, Quat::IDENTITY, super::Shape::Cylinder { radius: 0.3, height: r.g - SLAB }, Mat::Plaster, true, true);
                    b.push(base + Vec3::Y * (r.g - SLAB - 0.15), Quat::from_rotation_y(-col), super::Shape::Box(Vec3::new(0.78, 0.3, 0.78)), Mat::Plaster, false, true);
                }
            }
            // Shop fronts on both faces: an arched opening, or a window over a stall board.
            let front = if i % 2 == 0 { Opening::arch(0.0, 1.8, 3.2) } else { Opening { at: 0.0, width: 1.6, bottom: 0.9, top: 3.0 } };
            for rad in [core1, core0] {
                let (p, q) = (xz(rad, s0), xz(rad, s1));
                let o = Opening { at: p.distance(q) / 2.0, ..front };
                b.wall(w, p, q, 0.0, r.g - SLAB, 0.3, Mat::Plaster, &[o]);
            }
            // The back-to-back wall between the two rows of shops.
            if split {
                b.wall(w, xz(mid, s0), xz(mid, s1), 0.0, r.g - SLAB, 0.3, Mat::Plaster, &[]);
            }
            // Upper storey: a window to each segment, front and back.
            for rad in [r.ro - 0.2, r.ri + 0.2] {
                let (p, q) = (xz(rad, s0), xz(rad, s1));
                b.wall(w, p, q, r.g + lift, r.u - SLAB, 0.3, Mat::Plaster, &[Opening::window(p.distance(q) / 2.0)]);
            }
            // A party wall between every pair of shops; the block's ends close the shops and
            // the upper storey off from the street, each with a door.
            if i % 2 == 0 || i + 1 == n {
                let col = if i + 1 == n && i % 2 == 1 { s1 } else { s0 };
                let end = i == 0 || i + 1 == n;
                for (r0, r1) in if split { vec![(core0, mid), (mid, core1)] } else { vec![(core0, core1)] } {
                    let (p, q) = (xz(r0, col), xz(r1, col));
                    let door = if end { vec![Opening::door(p.distance(q) / 2.0)] } else { vec![] };
                    b.wall(w, p, q, 0.0, r.g - SLAB, 0.3, Mat::Plaster, &door);
                }
                if end {
                    let (p, q) = (xz(r.ri + 0.2, col), xz(r.ro - 0.2, col));
                    b.wall(w, p, q, r.g, r.u - SLAB, 0.3, Mat::Plaster, &[Opening::window(p.distance(q) / 2.0)]);
                }
            }
            // Stairs up through the shop, climbing outwards onto the floor beyond the well.
            if stairs(i) {
                b.stairs(radial, Vec3::new(core0 + 0.5, 0.0, 0.0), well_to - core0 - 0.5, r.g, 1.6, Mat::Plaster);
            }
            // Loot: a chest in every third shop (the back row too, in deep blocks), an urn
            // upstairs now and then, loose finds on the verandahs.
            if i % 3 == 1 {
                let at = if split { (core0 + mid) / 2.0 } else { (core0 + core1) / 2.0 };
                b.chest(radial, Vec3::new(at, 0.0, 0.0), FRAC_PI_2, ChestKind::Peti);
                if split {
                    b.chest(radial, Vec3::new((mid + core1) / 2.0, 0.0, 0.0), -FRAC_PI_2, ChestKind::Peti);
                }
            }
            if i % 9 == 2 || i + 3 == n {
                let rad = if blk % 2 == 0 { r.ro - 2.0 } else { r.ri + 2.0 };
                b.chest(radial, Vec3::new(rad, r.g, 0.0), -FRAC_PI_2, ChestKind::Kalash);
            }
            if rng.random_bool(0.3) {
                let rad = if rng.random_bool(0.5) { r.ro - r.ver / 2.0 } else { r.ri + r.ver / 2.0 };
                b.loot(radial, Vec3::new(rad, 0.3, 0.0));
            }
        }
    }
}
