//! Connaught Place's ring of blocks: white two-storey blocks round a park, cut by radial streets,
//! each colonnaded on both faces over a ring of shops, with a storey of windows above, stairs up
//! through a shop now and then, and a roof behind parapets. Built for the India map (scaled down)
//! and for Delhi (at the real radii, from OpenStreetMap: crate::map::delhi).

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use rand::RngExt;
use rand::rngs::SmallRng;

use super::build::{Builder, Frame, Opening};
use super::{ChestKind, Mat, SignKind};

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
                b.bx(radial, Vec3::new((r0 + r1) / 2.0, top - SLAB / 2.0, 0.0), Vec3::new(r1 - r0, SLAB, chord_at(r1) + 0.08), Mat::Stucco);
            };
            // Marble underfoot (3 cm proud of the levelled ground: no step to trip on, and no
            // grass through the verandahs), then the first floor (the verandah ceilings and the
            // shops'), with the stair well left open.
            b.bx(radial, Vec3::new((r.ro + r.ri) / 2.0, -0.1 + lift / 2.0, 0.0), Vec3::new(r.ro - r.ri + 1.2, 0.26, chord_at(r.ro + 0.6) + 0.08), Mat::Stone);
            slab(b, r.ri - 0.6, core0, r.g + lift);
            slab(b, core1, r.ro + 0.6, r.g + lift);
            let well_to = if split { mid } else { core1 };
            if stairs(i) {
                slab(b, well_to, core1, r.g + lift);
            } else {
                slab(b, core0, core1, r.g + lift);
            }
            // Roof, a stone-tiled terrace, its parapets and cornices (their tops 5 cm under the
            // slabs' they run along).
            slab(b, r.ri - 0.3, r.ro + 0.3, r.g + r.u + lift);
            // The terrace's tiles: drawn only, a hair over the slab and inside the parapets, so they
            // share no face with it.
            // In four strips across, each as wide as its outer edge, so the curve leaves no gaps
            // (neighbours' strips sit 2 cm apart in height: `lift`).
            for k in 0..4 {
                let (q0, q1) = (r.ri + (r.ro - r.ri) * k as f32 / 4.0, r.ri + (r.ro - r.ri) * (k + 1) as f32 / 4.0);
                b.deco(radial, Vec3::new((q0 + q1) / 2.0, r.g + r.u + lift + 0.02, 0.0), Vec3::new(q1 - q0 - 0.01, 0.02, chord_at(q1)), Mat::Stone);
            }
            for (rad, out) in [(r.ro + 0.15, 1.0f32), (r.ri - 0.15, -1.0)] {
                b.wall(w, xz(rad, s0), xz(rad, s1), r.g + r.u + lift, 1.0, 0.3, Mat::Stucco, &[]);
                b.deco(radial, Vec3::new(rad + out * 0.25, r.g + r.u - 0.2 + lift, 0.0), Vec3::new(0.5, 0.3, chord_at(rad) + 0.1), Mat::Stucco);
                b.deco(radial, Vec3::new(rad + out * 0.3, r.g - 0.25 + lift, 0.0), Vec3::new(0.6, 0.4, chord_at(rad) + 0.1), Mat::Stucco);
            }
            // Columns at the start of each segment (and the block's last), on both faces.
            for (col, last) in [(s0, false), (s1, true)] {
                if last && i + 1 != n {
                    continue;
                }
                // Fat Tuscan columns, weathered grey-white: a square plinth, a round moulding at the
                // foot and a collar under the square abacus.
                for rad in [r.ro - 0.5, r.ri + 0.5] {
                    let base = r.at(rad, col, 0.0);
                    b.push(base + Vec3::Y * 0.2, Quat::from_rotation_y(-col), super::Shape::Box(Vec3::new(1.0, 0.4, 1.0)), Mat::Plaster, false, true);
                    b.deco_cyl(w, base + Vec3::Y * 0.4, 0.5, 0.16, Mat::Plaster);
                    b.push(base + Vec3::Y * (r.g - SLAB) / 2.0, Quat::IDENTITY, super::Shape::Cylinder { radius: 0.4, height: r.g - SLAB }, Mat::Plaster, true, true);
                    b.deco_cyl(w, base + Vec3::Y * (r.g - SLAB - 0.55), 0.46, 0.12, Mat::Plaster);
                    b.push(base + Vec3::Y * (r.g - SLAB - 0.17), Quat::from_rotation_y(-col), super::Shape::Box(Vec3::new(1.0, 0.34, 1.0)), Mat::Plaster, false, true);
                }
            }
            // Shop fronts on both faces: a doorway, or a shop window over a stall board. Each set in
            // a frame of dark wood cladding up to the signboard; the windows glazed.
            let window = i % 2 == 1;
            let front = if window { Opening { at: 0.0, width: 1.6, bottom: 0.9, top: 3.0 } } else { Opening::arch(0.0, 1.8, 3.2) };
            let out_dir = |a: f32| Vec2::new(a.cos(), a.sin());
            for (rad, toward) in [(core1, 1.0f32), (core0, -1.0)] {
                let (p, q) = (xz(rad, s0), xz(rad, s1));
                let o = Opening { at: p.distance(q) / 2.0, ..front };
                b.wall(w, p, q, 0.0, r.g - SLAB, 0.3, Mat::Rustic, &[o]);
                // The wall is a chord: mid-segment its face sits inside the arc by the sagitta.
                let face = rad * ((s1 - s0) / 2.0).cos() + toward * 0.18;
                let half = front.width / 2.0;
                for side in [-1.0f32, 1.0] {
                    b.deco(radial, Vec3::new(face, 1.85, side * (half + 0.2)), Vec3::new(0.05, 3.7, 0.4), Mat::Wood);
                }
                b.deco(radial, Vec3::new(face, front.top + 0.25, 0.0), Vec3::new(0.05, 0.5, front.width + 0.8), Mat::Wood);
                if window {
                    b.deco(radial, Vec3::new(face, 0.45, 0.0), Vec3::new(0.05, 0.9, front.width), Mat::Wood);
                    b.map.signs.push(super::ShopSign {
                        pos: r.at(rad, am, (front.bottom + front.top) / 2.0),
                        facing: out_dir(am) * toward,
                        width: front.width,
                        height: front.top - front.bottom,
                        kind: SignKind::Glass,
                    });
                }
            }
            // The back-to-back wall between the two rows of shops.
            if split {
                b.wall(w, xz(mid, s0), xz(mid, s1), 0.0, r.g - SLAB, 0.3, Mat::Stucco, &[]);
            }
            // Each shop's signboard over its front, under the verandah ceiling; shelves of goods
            // on its back wall, seen through the arch. The ring's faces carry bigger boards over
            // the colonnade, under the upper windows, every other bay.
            let chord = chord_at(core0).min(chord_at(core1));
            // A small red board hung in every bay between the column tops, facing the street.
            for (rad, toward) in [(r.ro - 0.5, 1.0f32), (r.ri + 0.5, -1.0)] {
                b.map.signs.push(super::ShopSign { pos: r.at(rad, am, r.g - SLAB - 0.42), facing: out_dir(am) * toward, width: 1.0, height: 0.42, kind: SignKind::Hanging });
            }
            for (rad, toward) in [(core0 - 0.16, -1.0f32), (core1 + 0.16, 1.0)] {
                b.map.signs.push(super::ShopSign {
                    pos: r.at(rad, am, (3.35 + r.g - SLAB) / 2.0 + 0.05),
                    facing: out_dir(am) * toward,
                    width: (chord * 0.82).min(3.6),
                    height: 0.78,
                    kind: SignKind::Board,
                });
            }
            if split {
                // A wall of goods across the back, a tube light down the middle of the ceiling.
                for (rad, toward, front) in [(mid - 0.16, -1.0f32, core0), (mid + 0.16, 1.0, core1)] {
                    b.map.signs.push(super::ShopSign { pos: r.at(rad, am, 1.4), facing: out_dir(am) * toward, width: chord_at(rad) * 0.96, height: 2.5, kind: SignKind::Shelves });
                    b.map.signs.push(super::ShopSign {
                        pos: r.at((front + mid) / 2.0, am, r.g - SLAB - 0.03),
                        facing: out_dir(am),
                        width: 0.12,
                        height: (mid - front) * 0.55,
                        kind: SignKind::Light,
                    });
                }
            }
            if i % 2 == 0 {
                for (rad, toward) in [(r.ri + 0.06, -1.0f32), (r.ro - 0.06, 1.0)] {
                    b.map.signs.push(super::ShopSign {
                        pos: r.at(rad, am, r.g + lift + 0.5),
                        facing: out_dir(am) * toward,
                        width: (chord_at(rad) * 0.9).min(4.2),
                        height: 0.75,
                        kind: SignKind::Board,
                    });
                }
            }
            // Upper storey: a tall window to each segment, front and back, framed outside by a sill,
            // side mouldings and a hood.
            let tall = |at: f32| Opening { at, width: 1.4, bottom: 0.8, top: 3.0 };
            for (rad, out) in [(r.ro - 0.2, 1.0f32), (r.ri + 0.2, -1.0)] {
                let (p, q) = (xz(rad, s0), xz(rad, s1));
                b.wall(w, p, q, r.g + lift, r.u - SLAB, 0.3, Mat::Stucco, &[tall(p.distance(q) / 2.0)]);
                let face = rad + out * 0.15;
                let y0 = r.g + lift;
                b.deco(radial, Vec3::new(face + out * 0.06, y0 + 0.74, 0.0), Vec3::new(0.12, 0.1, 1.75), Mat::Stucco);
                b.deco(radial, Vec3::new(face + out * 0.05, y0 + 3.12, 0.0), Vec3::new(0.1, 0.16, 1.95), Mat::Stucco);
                b.deco(radial, Vec3::new(face + out * 0.09, y0 + 3.24, 0.0), Vec3::new(0.18, 0.08, 2.1), Mat::Stucco);
                for side in [-0.79f32, 0.79] {
                    b.deco(radial, Vec3::new(face + out * 0.04, y0 + 1.9, side), Vec3::new(0.08, 2.24, 0.16), Mat::Stucco);
                    // Louvred shutters folded back either side, the ring's green.
                    b.deco(radial, Vec3::new(face + out * 0.03, y0 + 1.9, side * 1.47), Vec3::new(0.06, 2.1, 0.58), Mat::Paint(7));
                }
            }
            // A party wall between every pair of shops; the block's ends close the shops and
            // the upper storey off from the street, each with a door.
            if i % 2 == 0 || i + 1 == n {
                let col = if i + 1 == n && i % 2 == 1 { s1 } else { s0 };
                let end = i == 0 || i + 1 == n;
                for (r0, r1) in if split { vec![(core0, mid), (mid, core1)] } else { vec![(core0, core1)] } {
                    // Butting against the walls at its ends, not into them (their faces would share).
                    let (p, q) = (xz(r0 + 0.15, col), xz(r1 - 0.15, col));
                    let door = if end { vec![Opening::door(p.distance(q) / 2.0)] } else { vec![] };
                    b.wall(w, p, q, 0.0, r.g - SLAB, 0.3, Mat::Stucco, &door);
                    // Shelves down both faces of the party wall, from the back halfway to the front.
                    if !end {
                        // The shop's back is the middle wall: the inner row's far end, the outer row's near one.
                        let rm = if r1 == mid || !split { r1 - (r1 - r0) * 0.3 } else { r0 + (r1 - r0) * 0.3 };
                        let span = (r1 - r0) * 0.5;
                        let tangent = Vec2::new(-col.sin(), col.cos());
                        for side in [-1.0f32, 1.0] {
                            let pos = r.at(rm, col, 1.4) + Vec3::new(tangent.x, 0.0, tangent.y) * side * 0.16;
                            b.map.signs.push(super::ShopSign { pos, facing: tangent * side, width: span, height: 2.5, kind: SignKind::Shelves });
                        }
                    }
                }
                if end {
                    let (p, q) = (xz(r.ri + 0.2, col), xz(r.ro - 0.2, col));
                    b.wall(w, p, q, r.g, r.u - SLAB, 0.3, Mat::Stucco, &[Opening::window(p.distance(q) / 2.0)]);
                }
            }
            // Stairs up through the shop, climbing outwards onto the floor beyond the well.
            if stairs(i) {
                b.stairs(radial, Vec3::new(core0 + 0.5, 0.0, 0.0), well_to - core0 - 0.5, r.g, 1.6, Mat::Stucco);
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
