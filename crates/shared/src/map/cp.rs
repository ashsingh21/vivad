//! Connaught Place's ring of blocks: white two-storey blocks round a park, cut by radial streets,
//! each colonnaded on both faces over a ring of shops, with a storey of windows above, stairs up
//! through a shop now and then, and a roof behind parapets. Built for the India map (scaled down)
//! and for Delhi (at the real radii, from OpenStreetMap: crate::map::delhi).

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use rand::RngExt;
use crate::SmallRng;

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

/// The base course's height: dressed stone along the walls' foot, to a stall board's top.
const PLINTH: f32 = 0.9;

pub fn build(b: &mut Builder, rng: &mut SmallRng, r: &Ring) {
    let w = Frame::new(Vec3::ZERO, 0.0);
    let tau = std::f32::consts::TAU;
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
        let p = r.at(r.park * 0.6, tau * (i as f32 + 0.5) / 4.0, 0.0);
        b.loot(w, p + Vec3::Y * (b.map.terrain.height(p.x, p.z) + 0.3));
    }
    park(b, rng, r);
    // Deep blocks are two rows of shops back to back, one facing each colonnade.
    let (core0, core1) = (r.ri + r.ver, r.ro - r.ver);
    let split = core1 - core0 > 14.0;
    let mid = (core0 + core1) / 2.0;
    for (blk, segs) in blocks(r).into_iter().enumerate() {
        let n = segs.len();
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
            // The rooms upstairs floored in the verandahs' tiles, drawn only, a hair over the slab (as
            // the terrace's below), in strips round the curve; the stair well left open.
            let well = if stairs(i) { Some((core0, well_to)) } else { None };
            for k in 0..4 {
                let (q0, q1) = (r.ri + (r.ro - r.ri) * k as f32 / 4.0, r.ri + (r.ro - r.ri) * (k + 1) as f32 / 4.0);
                let spans: Vec<(f32, f32)> = match well {
                    Some((w0, w1)) => [(q0, q1.min(w0)), (q0.max(w1), q1)].into_iter().filter(|(a, b)| b - a > 0.05).collect(),
                    None => vec![(q0, q1)],
                };
                for (a0, a1) in spans {
                    b.deco(radial, Vec3::new((a0 + a1) / 2.0, r.g + lift + 0.02, 0.0), Vec3::new(a1 - a0 - 0.01, 0.02, chord_at(a1)), Mat::Stone);
                }
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
                // A chunky cornice in two steps (docs/style-guide.md section 3), and a coping on the
                // parapet: their shadow lines are what draw the block at a distance.
                b.deco(radial, Vec3::new(rad + out * 0.4, r.g + r.u - 0.23 + lift, 0.0), Vec3::new(0.8, 0.36, chord_at(rad + 0.8) + 0.1), Mat::Stucco);
                b.deco(radial, Vec3::new(rad + out * 0.24, r.g + r.u - 0.495 + lift, 0.0), Vec3::new(0.48, 0.19, chord_at(rad + 0.5) + 0.1), Mat::Stucco);
                b.deco(radial, Vec3::new(rad, r.g + r.u + 1.03 + lift, 0.0), Vec3::new(0.5, 0.14, chord_at(rad + 0.25) + 0.08), Mat::Stucco);
                b.deco(radial, Vec3::new(rad + out * 0.3, r.g - 0.25 + lift, 0.0), Vec3::new(0.6, 0.4, chord_at(rad) + 0.1), Mat::Stucco);
            }
            // A beam across each verandah from every column to the shop wall, under the ceiling: the
            // colonnade's rhythm of light and shade; and a light in the ceiling of every bay.
            for (c0, c1) in [(r.ri + 0.5, core0 - 0.15), (core1 + 0.15, r.ro - 0.5)] {
                b.deco(Frame::new(c, -s0), Vec3::new((c0 + c1) / 2.0, r.g - SLAB - 0.25 + lift, 0.0), Vec3::new(c1 - c0, 0.5, 0.42), Mat::Stucco);
                b.map.signs.push(super::ShopSign {
                    pos: r.at((c0 + c1) / 2.0, am, r.g - SLAB - 0.03 + lift),
                    facing: Vec2::new(am.cos(), am.sin()),
                    width: 0.14,
                    height: (c1 - c0) * 0.5,
                    kind: SignKind::Light,
                });
                // Its light on the marble below (the floor's top is 3 cm up).
                b.map.signs.push(super::ShopSign {
                    pos: r.at((c0 + c1) / 2.0, am, 0.045 + lift / 2.0),
                    facing: Vec2::new(am.cos(), am.sin()),
                    width: 3.6,
                    height: c1 - c0 + 1.0,
                    kind: SignKind::Pool,
                });
            }
            // Columns at the start of each segment (and the block's last), on both faces.
            for (col, last) in [(s0, false), (s1, true)] {
                if last && i + 1 != n {
                    continue;
                }
                // Fat Tuscan columns, white: a square plinth of dressed stone (the base course's,
                // docs/style-guide.md section 4), a round moulding at the foot and a collar under
                // the square abacus.
                for rad in [r.ro - 0.5, r.ri + 0.5] {
                    let base = r.at(rad, col, 0.0);
                    b.push(base + Vec3::Y * 0.3, Quat::from_rotation_y(-col), super::Shape::Box(Vec3::new(1.0, 0.6, 1.0)), Mat::Plinth, false, true);
                    b.deco_cyl(w, base + Vec3::Y * 0.6, 0.5, 0.16, Mat::Stucco);
                    b.push(base + Vec3::Y * (r.g - SLAB) / 2.0, Quat::IDENTITY, super::Shape::Cylinder { radius: 0.4, height: r.g - SLAB }, Mat::Stucco, true, true);
                    b.deco_cyl(w, base + Vec3::Y * (r.g - SLAB - 0.55), 0.46, 0.12, Mat::Stucco);
                    b.push(base + Vec3::Y * (r.g - SLAB - 0.17), Quat::from_rotation_y(-col), super::Shape::Box(Vec3::new(1.0, 0.34, 1.0)), Mat::Stucco, false, true);
                }
            }
            // Every second segment starts a shop, which the NCR may furnish.
            if i % 2 == 0 && i + 1 < n && !stairs(i) && !stairs(i + 1) {
                let middle = s1;
                let wide = 2.0 * mid.max(core0 + 1.0) * (s1 - s0).sin();
                let back = if split { mid } else { core1 };
                b.map.ring_shops.push(super::interiors::RingShop { back: r.at(back - 0.15, middle, 0.0), into: -r.at(1.0, middle, 0.0).xz() + r.at(0.0, middle, 0.0).xz(), wide });
                if split {
                    b.map.ring_shops.push(super::interiors::RingShop { back: r.at(back + 0.15, middle, 0.0), into: r.at(1.0, middle, 0.0).xz() - r.at(0.0, middle, 0.0).xz(), wide });
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
                let wall_mid = rad * ((s1 - s0) / 2.0).cos();
                let face = wall_mid + toward * 0.18;
                let half = front.width / 2.0;
                // Cladding: a dark backing either side and above, vertical slats over it with
                // gaps between (the relief catches the light), a slatted fascia up top;
                // the opening lined in wood too; a base course of dressed stone along the wall's
                // foot, under a capping ledge.
                let slat = face + toward * 0.035;
                for side in [-1.0f32, 1.0] {
                    b.deco(radial, Vec3::new(face, 1.85, side * (half + 0.22)), Vec3::new(0.03, 3.7, 0.44), Mat::Wood);
                    for k in 0..4 {
                        b.deco(radial, Vec3::new(slat, 1.85, side * (half + 0.075 + 0.105 * k as f32)), Vec3::new(0.04, 3.68, 0.07), Mat::Wood);
                    }
                    b.deco(radial, Vec3::new(wall_mid, front.top / 2.0 + front.bottom / 2.0, side * (half + 0.01)), Vec3::new(0.37, front.top - front.bottom, 0.035), Mat::Wood);
                }
                b.deco(radial, Vec3::new(face, front.top + 0.25, 0.0), Vec3::new(0.03, 0.5, front.width + 0.88), Mat::Wood);
                for k in 0..3 {
                    b.deco(radial, Vec3::new(slat, front.top + 0.08 + 0.17 * k as f32, 0.0), Vec3::new(0.04, 0.12, front.width + 0.86), Mat::Wood);
                }
                b.deco(radial, Vec3::new(wall_mid, front.top + 0.01, 0.0), Vec3::new(0.37, 0.035, front.width + 0.02), Mat::Wood);
                let chord_mid = chord_at(rad);
                for side in [-1.0f32, 1.0] {
                    let (z0, z1) = (half + 0.45, chord_mid / 2.0 - 0.02);
                    if z1 > z0 {
                        b.deco(radial, Vec3::new(face + toward * 0.01, PLINTH / 2.0, side * (z0 + z1) / 2.0), Vec3::new(0.06, PLINTH, z1 - z0), Mat::Plinth);
                        // The ledge 1 cm short of the course's ends (their end faces would share).
                        b.deco(radial, Vec3::new(face + toward * 0.025, PLINTH + 0.03, side * (z0 + z1) / 2.0), Vec3::new(0.09, 0.06, z1 - z0 - 0.02), Mat::Plinth);
                    }
                }
                if window {
                    b.deco(radial, Vec3::new(face, 0.45, 0.0), Vec3::new(0.03, 0.9, front.width), Mat::Wood);
                    for k in 0..5 {
                        b.deco(radial, Vec3::new(slat, 0.1 + 0.17 * k as f32, 0.0), Vec3::new(0.04, 0.12, front.width - 0.02), Mat::Wood);
                    }
                    b.map.signs.push(super::ShopSign {
                        pos: r.at(rad, am, (front.bottom + front.top) / 2.0),
                        facing: out_dir(am) * toward,
                        width: front.width,
                        height: front.top - front.bottom,
                        kind: SignKind::Glass,
                    });
                    // A rack of goods just behind the glass, seen from the verandah (not on the
                    // foot of a flight of stairs).
                    if !(stairs(i) && rad == core0) {
                        super::kit::shelving(b, r.at(rad - toward * 0.62, am, 0.03 + lift / 2.0), out_dir(am) * toward, front.width, (blk * 97 + i * 13) as u32 + toward as u32);
                    }
                }
            }
            // The back-to-back wall between the two rows of shops, limewashed a shop's colour (Delhi's
            // faded pinks, ochres, mints and blues) behind its shelves.
            let wash = |k: usize| Mat::Paint(((blk * 7 + k * 3) % 7) as u8);
            if split {
                b.wall(w, xz(mid, s0), xz(mid, s1), 0.0, r.g - SLAB, 0.3, wash(i / 2), &[]);
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
                    // Big, the bay's width (docs/style-guide.md section 6).
                    width: (chord * 0.94).min(4.2),
                    height: 1.0,
                    kind: SignKind::Board,
                });
            }
            if split {
                // A wall of goods across the back, a tube light down the middle of the ceiling.
                for (rad, toward, front) in [(mid - 0.16, -1.0f32, core0), (mid + 0.16, 1.0, core1)] {
                    super::kit::shelving(b, r.at(rad, am, 0.03 + lift / 2.0), out_dir(am) * toward, chord_at(rad) * 0.96, (blk * 97 + i * 13 + 5) as u32 + toward as u32);
                    // The inner shop's ceiling is the stair well, where it has the stairs.
                    if stairs(i) && front == core0 {
                        continue;
                    }
                    b.map.signs.push(super::ShopSign {
                        pos: r.at((front + mid) / 2.0, am, r.g - SLAB - 0.03),
                        facing: out_dir(am),
                        width: 0.12,
                        height: (mid - front) * 0.55,
                        kind: SignKind::Light,
                    });
                    b.map.signs.push(super::ShopSign {
                        pos: r.at((front + mid) / 2.0, am, 0.045 + lift / 2.0),
                        facing: out_dir(am),
                        width: 3.4,
                        height: mid - front,
                        kind: SignKind::Pool,
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
            // Upper storey: a big window to each segment, front and back, in a deep surround with a
            // sill and a chhajja (the sunshade slab) over it; a sill band along the whole front and
            // pilasters between the bays (docs/style-guide.md section 3: chunky, soft, big shadow lines).
            let (bot, top, hw) = (1.15, 3.3, 0.9);
            let tall = |at: f32| Opening { at, width: 2.0 * hw, bottom: bot, top };
            for (rad, out) in [(r.ro - 0.2, 1.0f32), (r.ri + 0.2, -1.0)] {
                let (p, q) = (xz(rad, s0), xz(rad, s1));
                b.wall(w, p, q, r.g + lift, r.u - SLAB, 0.3, Mat::Stucco, &[tall(p.distance(q) / 2.0)]);
                b.inside_frame(w, p, q, r.g + lift, 0.3, r.at((r.ri + r.ro) / 2.0, (s0 + s1) / 2.0, 0.0).xz(), &tall(p.distance(q) / 2.0), Mat::Wood);
                // The wall is a chord: in the segment's middle frame its face is a plane.
                let face = rad * ((s1 - s0) / 2.0).cos() + out * 0.15;
                let y0 = r.g + lift;
                // Trim from `o0` to `o1` proud of the face (from a centimetre inside it, so no face is shared).
                let trim = |b: &mut Builder, o1: f32, ya: f32, yb: f32, z: f32, zw: f32| {
                    b.deco(radial, Vec3::new(face + out * (o1 - 0.01) / 2.0, y0 + (ya + yb) / 2.0, z), Vec3::new(o1 + 0.01, yb - ya, zw), Mat::Stucco);
                };
                let len = p.distance(q);
                trim(b, 0.1, bot - 0.22, bot - 0.1, 0.0, len + 0.02);
                trim(b, 0.24, bot - 0.16, bot, 0.0, 2.0 * hw + 0.56);
                for side in [-1.0f32, 1.0] {
                    trim(b, 0.15, bot, top + 0.2, side * (hw + 0.1), 0.2);
                }
                trim(b, 0.15, top, top + 0.2, 0.0, 2.0 * hw + 0.4);
                trim(b, 0.62, top + 0.3, top + 0.42, 0.0, 2.0 * hw + 0.9);
                for side in [-1.0f32, 1.0] {
                    trim(b, 0.34, top + 0.12, top + 0.3, side * (hw + 0.3), 0.14);
                }
                // Glazed, a blind or curtain drawn some way down behind the glass in a shop's colour,
                // and now and then a window AC in the bottom of it (half out over the street).
                let am = (s0 + s1) / 2.0;
                let (mid_y, h) = ((bot + top) / 2.0, top - bot);
                b.map.signs.push(super::ShopSign { pos: r.at(rad + out * 0.02, am, y0 + mid_y), facing: Vec2::new(am.cos(), am.sin()) * out, width: 2.0 * hw, height: h, kind: SignKind::Glass });
                let ac = rng.random_bool(0.2);
                let drop = rng.random_range(0.5..if ac { 1.4 } else { 1.9 });
                b.deco(radial, Vec3::new(rad - out * 0.12, y0 + top - drop / 2.0, 0.0), Vec3::new(0.03, drop, 2.0 * hw - 0.04), Mat::Paint(rng.random_range(0..7)));
                if ac {
                    b.deco(radial, Vec3::new(rad + out * 0.05, y0 + bot + 0.3, 0.0), Vec3::new(0.62, 0.55, 0.8), Mat::Plaster);
                }
                for side in [-1.0f32, 1.0] {
                    // Louvred shutters folded back either side, the ring's green.
                    trim(b, 0.07, bot + 0.05, top - 0.05, side * (hw + 0.48), 0.54);
                    b.deco(radial, Vec3::new(face + out * 0.065, y0 + mid_y, side * (hw + 0.48)), Vec3::new(0.03, h - 0.14, 0.5), Mat::Paint(7));
                }
            }
            // Pilasters between the bays upstairs (and at the block's ends), on the sill band up to the cornice.
            for (col, last) in [(s0, false), (s1, true)] {
                if last && i + 1 != n {
                    continue;
                }
                for (rad, out) in [(r.ro - 0.05, 1.0f32), (r.ri + 0.05, -1.0)] {
                    let (ya, yb) = (r.g + lift + bot - 0.1, r.g + r.u - 0.58 + lift);
                    b.deco(Frame::new(c, -col), Vec3::new(rad + out * 0.07, (ya + yb) / 2.0, 0.0), Vec3::new(0.16, yb - ya, 0.42), Mat::Stucco);
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
                    b.wall(w, p, q, 0.0, r.g - SLAB, 0.3, if end { Mat::Stucco } else { wash(i / 2 + 1) }, &door);
                    // The end walls (on the radial streets) carry the base course too, both faces,
                    // either side of the door.
                    if end {
                        let (a0, a1, dm) = (r0 + 0.15, r1 - 0.15, (r0 + r1) / 2.0);
                        for (x0, x1) in [(a0, dm - 0.75), (dm + 0.75, a1)] {
                            for out in [-1.0f32, 1.0] {
                                let f = Frame::new(c, -col);
                                b.deco(f, Vec3::new((x0 + x1) / 2.0, PLINTH / 2.0, out * 0.175), Vec3::new(x1 - x0, PLINTH, 0.05), Mat::Plinth);
                                b.deco(f, Vec3::new((x0 + x1) / 2.0, PLINTH + 0.03, out * 0.19), Vec3::new(x1 - x0 - 0.02, 0.06, 0.08), Mat::Plinth);
                            }
                        }
                    }
                    // Shelves down both faces of the party wall, from the back halfway to the front.
                    if !end {
                        // The shop's back is the middle wall: the inner row's far end, the outer row's near one.
                        let rm = if r1 == mid || !split { r1 - (r1 - r0) * 0.3 } else { r0 + (r1 - r0) * 0.3 };
                        let span = (r1 - r0) * 0.5;
                        let tangent = Vec2::new(-col.sin(), col.cos());
                        for side in [-1.0f32, 1.0] {
                            let back = r.at(rm, col, 0.03) + Vec3::new(tangent.x, 0.0, tangent.y) * side * 0.15;
                            super::kit::shelving(b, back, tangent * side, span, (blk * 97 + i * 13 + 9) as u32 + side as u32);
                        }
                    }
                }
                if end {
                    let (p, q) = (xz(r.ri + 0.2, col), xz(r.ro - 0.2, col));
                    let o = Opening::window(p.distance(q) / 2.0);
                    b.wall(w, p, q, r.g, r.u - SLAB, 0.3, Mat::Stucco, &[o]);
                    let into = if i == 0 { s0 + 0.05 } else { s1 - 0.05 };
                    b.inside_frame(w, p, q, r.g, 0.3, r.at((r.ri + r.ro) / 2.0, into, 0.0).xz(), &o, Mat::Wood);
                }
            }
            // Upstairs, the long floor divided into rooms: a limewashed partition every two segments
            // with a doorway, and a few things in each room to take cover behind (not where the
            // stairs come up).
            if i % 2 == 0 && i > 0 {
                let (p, q) = (xz(r.ri + 0.36, s0), xz(r.ro - 0.36, s0));
                // The lighter paints only (not terracotta or the old green: rooms read dark in them).
                let room = [0u8, 1, 2, 3, 4, 6][(blk * 5 + i) % 6];
                // Beside a stair well its doorways go where the floor is: one on the verandah's side
                // of the well, one past its far end.
                let doors = if stairs(i) || stairs(i - 1) {
                    let well_to = if split { mid } else { core1 };
                    [Opening::door((core0 - r.ri - 0.36) / 2.0), Opening::door((well_to + r.ro - 0.36) / 2.0 - r.ri - 0.36)]
                } else {
                    [Opening::door(p.distance(q) * 0.35), Opening::door(p.distance(q) * 0.65)]
                };
                b.wall(w, p, q, r.g + lift, r.u - SLAB, 0.2, Mat::Paint(room), &doors);
                // A storeroom's racks on both faces, between the doorways (not over a stair well:
                // each face stands in the bay on its side).
                let tangent = Vec2::new(-s0.sin(), s0.cos());
                let at = p.lerp(q, 0.5);
                for side in [-1.0f32, 1.0].into_iter().filter(|_| !stairs(i) && !stairs(i - 1)) {
                    let f = at + tangent * side * 0.1;
                    super::kit::shelving(b, Vec3::new(f.x, r.g + lift, f.y), tangent * side, p.distance(q) * 0.22, (blk * 97 + i * 13 + 3) as u32 + side as u32);
                }
            }
            // Upstairs, a ceiling fan in the bays between the lit ones.
            if i % 2 == 0 && !stairs(i) {
                for rad in [(r.ri * 2.0 + r.ro) / 3.0, (r.ri + r.ro * 2.0) / 3.0] {
                    super::kit::place(b, "fan", r.at(rad, am, r.g + r.u - SLAB + lift), rng.random_range(0.0..tau));
                }
            }
            // Upstairs, a tube light down the middle of every other bay's ceiling, its glow below
            // (not over a stair well).
            if i % 2 == 1 && !stairs(i) {
                for (rad, len) in [((r.ri * 2.0 + r.ro) / 3.0, 1.2), ((r.ri + r.ro * 2.0) / 3.0, 1.2)] {
                    b.map.signs.push(super::ShopSign { pos: r.at(rad, am, r.g + r.u - SLAB - 0.03 + lift), facing: out_dir(am), width: 0.14, height: len, kind: SignKind::Light });
                    b.map.signs.push(super::ShopSign { pos: r.at(rad, am, r.g + lift + 0.045), facing: out_dir(am), width: 3.4, height: 3.4, kind: SignKind::Pool });
                }
            }
            // Each room's clutter (docs/style-guide.md section 6), on and by the partition it
            // starts at: a calendar, a clock and a god's picture on the wall over the racks, a
            // matka, a bucket, slippers and a plant on the floor (clear of the doorways).
            if i % 2 == 0 && i > 0 && !stairs(i) && !stairs(i - 1) {
                let (p, q) = (xz(r.ri + 0.36, s0), xz(r.ro - 0.36, s0));
                let tangent = Vec2::new(-s0.sin(), s0.cos());
                let on = |t: f32, side: f32, y: f32, out: f32| {
                    let f = p.lerp(q, t) + tangent * side * (0.1 + out);
                    Vec3::new(f.x, r.g + lift + y, f.y)
                };
                for side in [-1.0f32, 1.0] {
                    let face = super::kit::facing(tangent * side);
                    let k = rng.random_range(0..3);
                    let (w0, w1) = [("calendar", "god_photo"), ("clock", "calendar"), ("god_photo", "clock")][k];
                    super::kit::place(b, w0, on(0.2, side, 1.7, 0.0), face);
                    super::kit::place(b, w1, on(0.5, side, 2.45, 0.0), face);
                    super::kit::place(b, "clock", on(0.8, side, 2.1, 0.0), face);
                    let floor = [("matka", 0.12), ("bucket", 0.88), ("pot", 0.2), ("slippers", 0.42)];
                    for (name, t) in floor {
                        if rng.random_bool(0.7) {
                            super::kit::place(b, name, on(t, side, 0.0, 0.35), face + rng.random_range(-0.3..0.3));
                        }
                    }
                }
            }
            if i % 2 == 0 && !stairs(i) && !stairs(i + 1) && i + 1 < n {
                // An office upstairs: down the room's depth every 5 m a desk mostly, else a
                // cupboard, a sofa for visitors or stock, in the middle of the room's width.
                let mut rad = r.ri + 3.0;
                while rad < r.ro - 2.5 {
                    let roll = rng.random_range(0.0..1.0);
                    let name = if roll < 0.55 { "desk" } else if roll < 0.7 { "almirah" } else if roll < 0.8 { "sofa" } else if roll < 0.9 { "crates" } else { "cooler" };
                    let ang = s1 + rng.random_range(-0.6..0.6) / rad;
                    // Square to the room (desks and sofas set at odd angles read as tipped over).
                    super::kit::place(b, name, r.at(rad, ang, r.g + lift), -ang + FRAC_PI_2 * rng.random_range(0..4) as f32);
                    rad += 5.0;
                }
            }
            // Stairs up through the shop, climbing outwards onto the floor beyond the well.
            if stairs(i) {
                // As wide as the shop leaves beside the racks on its party wall (2.4 m on the real
                // ring), handrails up both sides, and a guard rail round the well upstairs where no
                // partition closes it.
                let wide = (chord_at(core0) - 2.3).clamp(1.6, 2.4);
                let (x0, x1, up) = (core0 + 0.5, well_to, r.g + lift);
                b.stairs(radial, Vec3::new(x0, 0.0, 0.0), x1 - x0, r.g, wide, Mat::Stucco);
                for side in [-1.0f32, 1.0] {
                    let z = side * (wide / 2.0 - 0.06);
                    b.railing(radial.p(Vec3::new(x0, 0.0, z)), radial.p(Vec3::new(x1, r.g, z)), 0.9, Mat::Wood);
                    // Across the top of the well either side of the flight, on the floor past it.
                    let edge = chord_at(x1) / 2.0 - 0.1;
                    b.railing(radial.p(Vec3::new(x1 + 0.06, up, z)), radial.p(Vec3::new(x1 + 0.06, up, side * edge)), 1.0, Mat::Wood);
                }
                let edge = chord_at(core0) / 2.0 - 0.1;
                b.railing(radial.p(Vec3::new(core0 - 0.06, up, -edge)), radial.p(Vec3::new(core0 - 0.06, up, edge)), 1.0, Mat::Wood);
                // The well's sides on the segment's edges: open where no partition stands on the
                // edge (one starts each even segment past the first; the block's ends are walled).
                for (j, col, out) in [(i, s0, -1.0f32), (i + 1, s1, 1.0)] {
                    if j % 2 == 1 && j < n {
                        let f = Frame::new(c, -col);
                        b.railing(f.p(Vec3::new(core0, up, out * 0.06)), f.p(Vec3::new(x1, up, out * 0.06)), 1.0, Mat::Wood);
                    }
                }
            }
            // Loot: a chest in every third shop (the back row too, in deep blocks), an urn
            // upstairs now and then, loose finds on the verandahs. A chest spills what it holds
            // 1 m along its +Z, and in `radial` +X points away from the park, so yaw -FRAC_PI_2
            // faces the park and FRAC_PI_2 faces outward: each chest faces its own shop front,
            // and its contents land between it and the arch. The stairs climb through the inner
            // shop (the only one, in a shallow block): no chest there, under them.
            if i % 3 == 1 {
                if !stairs(i) {
                    let at = if split { (core0 + mid) / 2.0 } else { (core0 + core1) / 2.0 };
                    b.chest(radial, Vec3::new(at, 0.0, 0.0), if split { -FRAC_PI_2 } else { FRAC_PI_2 }, ChestKind::Peti);
                }
                if split {
                    b.chest(radial, Vec3::new((mid + core1) / 2.0, 0.0, 0.0), FRAC_PI_2, ChestKind::Peti);
                }
            }
            if i % 9 == 2 || i + 3 == n {
                let rad = if blk % 2 == 0 { r.ro - 2.0 } else { r.ri + 2.0 };
                b.chest(radial, Vec3::new(rad, r.g, 0.0), -FRAC_PI_2, ChestKind::Tijori);
            }
            if rng.random_bool(0.2) {
                let rad = if rng.random_bool(0.5) { r.ro - r.ver / 2.0 } else { r.ri + r.ver / 2.0 };
                b.loot(radial, Vec3::new(rad, 0.3, 0.0));
            }
        }
    }
}


/// The inner circle's carriageway, between the park's pavement and the colonnade's (radii).
pub fn inner_road(r: &Ring) -> (f32, f32) {
    (r.park + 4.0, r.ri - 5.0)
}

/// The outer circle's, beyond the colonnade's pavement and its neem avenue (the buildings round
/// it start some 20 m out).
pub fn outer_road(r: &Ring) -> (f32, f32) {
    (r.ro + 9.5, r.ro + 18.5)
}

/// The radial streets' carriageway, its half-width.
const RADIAL_HALF: f32 = 4.5;

/// Bins round the circle for `OuterClear`.
const BINS: usize = 1440;

/// Where round the ring the outer circle can run: in places the buildings beyond stand right up
/// to the colonnade's pavement, and there the road stops (built after the buildings are).
pub struct OuterClear(Vec<bool>);

impl OuterClear {
    pub fn new(map: &super::MapLayout, r: &Ring) -> Self {
        let (r0, r1) = outer_road(r);
        let c = r.center.xz();
        let near: Vec<&super::Footprint> = map.footprints.iter().filter(|f| f.pts.iter().any(|p| p.distance(c) < r1 + 30.0)).collect();
        let free: Vec<bool> = (0..BINS)
            .map(|k| {
                let a = std::f32::consts::TAU * (k as f32 + 0.5) / BINS as f32;
                let mut rad = r0 - 1.5;
                while rad < r1 + 1.5 {
                    let p = r.at(rad, a, 0.0).xz();
                    if near.iter().any(|f| super::footprint::contains(&f.pts, p)) {
                        return false;
                    }
                    rad += 0.75;
                }
                true
            })
            .collect();
        // Kept 3 m clear of a building's side, too.
        let pad = (3.0 / r0 / std::f32::consts::TAU * BINS as f32).ceil() as i64;
        Self((0..BINS as i64).map(|k| (-pad..=pad).all(|d| free[(k + d).rem_euclid(BINS as i64) as usize])).collect())
    }

    /// Whether the outer circle runs at bearing `a`.
    pub fn at(&self, a: f32) -> bool {
        self.0[((a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * BINS as f32) as usize).min(BINS - 1)]
    }
}

/// Tarmac where the ring's roads run: both circles round (the outer where it's `clear`), and
/// each radial street between them (the mapped roads come in pieces that miss the ring's
/// geometry).
pub fn paint(t: &mut super::terrain::Terrain, r: &Ring, clear: &OuterClear) {
    let roads = [inner_road(r), outer_road(r)];
    let n = t.n;
    for i in 0..n {
        for j in 0..n {
            let p = Vec2::new(t.coord(i), t.coord(j)) - r.center.xz();
            let d = p.length();
            if d < r.park + 2.0 || d > roads[1].1 {
                continue;
            }
            let a = p.y.atan2(p.x);
            // Beyond the colonnade's pavement, only where the outer circle runs.
            let open = d < r.ro + 6.0 || clear.at(a);
            let radial = r.streets.iter().any(|&s| {
                let da = (a - s + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                (da * d).abs() < RADIAL_HALF && d > roads[0].1 - 1.0 && d < roads[1].0 + 1.0
            });
            if open && (roads.iter().any(|&(r0, r1)| d > r0 && d < r1) || radial) {
                t.ground[i * n + j] = super::terrain::ground::ROAD;
                t.road_d[i * n + j] = 0.0;
            }
        }
    }
}

/// Both circles' markings, drawn a hair over the tarmac: edge lines, a dashed centre line, a
/// zebra crossing each side of every radial street's mouth, dashes down the radial streets;
/// granite kerbs along both edges (ankle high, drawn only: kerbs that collided would trip).
pub fn markings(b: &mut Builder, r: &Ring, clear: &OuterClear) {
    let tau = std::f32::consts::TAU;
    let ground = |b: &Builder, rad: f32, a: f32| {
        let p = r.at(rad, a, 0.0);
        b.map.terrain.height(p.x, p.z)
    };
    // How far round from the nearest radial street's middle (radians at radius `rad`, in metres).
    let off_street = |a: f32, rad: f32| {
        r.streets.iter().map(|&s| (((a - s + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI) * rad).abs()).fold(f32::MAX, f32::min)
    };
    let paint = |b: &mut Builder, rad: f32, a: f32, along: f32, across: f32| {
        let y = ground(b, rad, a) + 0.012;
        b.deco(Frame::new(r.center, -a), Vec3::new(rad, y, 0.0), Vec3::new(across, 0.02, along), Mat::Plaster);
    };
    // Crossings span the mouths, RADIAL_HALF + 4 m out from each street's middle.
    let zebra = RADIAL_HALF + 4.0;
    // Each circle, and the edge of it the radial streets join (the blocks' side).
    let (inner, outer) = (inner_road(r), outer_road(r));
    for ((r0, r1), joins) in [(inner, inner.1), (outer, outer.0)] {
        let mid = (r0 + r1) / 2.0;
        let runs = |a: f32| r0 < r.ro || clear.at(a);
        for (rad, len, gap, width) in [(r0 + 0.4, 2.8, 0.2, 0.12), (r1 - 0.4, 2.8, 0.2, 0.12), (mid, 3.0, 3.0, 0.15)] {
            let n = (tau * rad / (len + gap)) as usize;
            for k in 0..n {
                let a = tau * (k as f32 + 0.5) / n as f32;
                if off_street(a, rad) > zebra + 2.0 && runs(a - len / 2.0 / rad) && runs(a + len / 2.0 / rad) {
                    paint(b, rad, a, len, width);
                }
            }
        }
        for rad in [r0 - 0.18, r1 + 0.18] {
            // Chunky (docs/style-guide.md section 3), in Delhi's yellow and black stripes.
            let h = 0.18;
            let n = (tau * rad / 1.2) as usize;
            for k in 0..n {
                let a = tau * (k as f32 + 0.5) / n as f32;
                // The kerb dips at the crossings and where the radial streets join.
                if off_street(a, rad) < zebra + 1.6 || ((rad - joins).abs() < 0.5 && off_street(a, rad) < RADIAL_HALF + 0.5) || !runs(a) {
                    continue;
                }
                let y = ground(b, rad, a);
                let stripe = if k % 2 == 0 { super::KERB_YELLOW } else { super::KERB_BLACK };
                b.deco(Frame::new(r.center, -a), Vec3::new(rad, y + h / 2.0 - 0.02, 0.0), Vec3::new(0.36, h + 0.04, tau * rad / n as f32 - 0.005), Mat::Paint(stripe));
            }
        }
        // Where the road stops at a building, a kerb across its end.
        if r0 > r.ro {
            for k in 0..BINS {
                let (a, next) = (tau * (k as f32 + 0.5) / BINS as f32, tau * (k as f32 + 1.5) / BINS as f32);
                if runs(a) != runs(next) {
                    let at = if runs(a) { a } else { next };
                    let y = ground(b, mid, at);
                    b.deco(Frame::new(r.center, -at), Vec3::new(mid, y + 0.07, 0.0), Vec3::new(r1 - r0 + 0.6, 0.22, 0.36), Mat::Paint(super::KERB_YELLOW));
                }
            }
        }
        for &s in &r.streets {
            // Zebras: stripes along the road, 0.5 m wide a metre apart across it, 3 m to cross on.
            for side in [-1.0f32, 1.0] {
                let a = s + side * (zebra - 1.5) / mid;
                if !runs(a - 2.0 / mid) || !runs(a + 2.0 / mid) {
                    continue;
                }
                let mut rad = r0 + 0.9;
                while rad < r1 - 0.9 {
                    paint(b, rad, a, 3.0, 0.5);
                    rad += 1.0;
                }
            }
        }
    }
    // Dashes down each radial street, from one circle to the other.
    for &s in &r.streets {
        let mut rad = inner.1 + 2.0;
        while rad < outer.0 - 1.5 && (rad < r.ro + 4.0 || clear.at(s)) {
            b.deco(Frame::new(r.center, -s), Vec3::new(rad, ground(b, rad, s) + 0.012, 0.0), Vec3::new(2.4, 0.02, 0.15), Mat::Plaster);
            rad += 5.0;
        }
    }
}


/// Each block's shop segments (start and end bearings), between one radial street and the next.
fn blocks(r: &Ring) -> Vec<Vec<(f32, f32)>> {
    let tau = std::f32::consts::TAU;
    let half_gap = (r.street / 2.0 / r.ri).asin();
    let mut streets = r.streets.clone();
    streets.sort_by(f32::total_cmp);
    (0..streets.len())
        .map(|blk| {
            let a0 = streets[blk] + half_gap;
            let a1 = streets[(blk + 1) % streets.len()] + if blk + 1 == streets.len() { tau } else { 0.0 } - half_gap;
            let n = (((a1 - a0) * r.ri) / r.seg_len).round().max(4.0) as usize;
            let ang = |t: f32| a0 + (a1 - a0) * t;
            (0..n).map(|i| (ang(i as f32 / n as f32), ang((i + 1) as f32 / n as f32))).collect()
        })
        .collect()
}

/// The ring lived in, after the street kit: roof life on the terraces (stair rooms, coolers,
/// dishes, solar panels, tanks of drums), two-wheelers parked in rows along both circles' kerbs
/// with autos between, a chai stall or thela by each block's ends, and a few planters, crates and
/// coolers by the colonnade's columns.
pub fn dress(b: &mut Builder, rng: &mut SmallRng, r: &Ring, clear: &OuterClear) {
    use super::kit::place;
    let tau = std::f32::consts::TAU;
    const ROOF: [&str; 7] = ["cooler", "dish", "solar", "drum", "crates", "pot", "clothesline"];
    const BIKES: [&str; 6] = ["scooter", "scooter", "commuter", "thumper", "discover", "sport150"];
    // Facing along the circle at bearing `a` (the kit's models face -Z).
    let along = |a: f32| (a.sin()).atan2(-a.cos());
    let radial_yaw = |a: f32| (-a.cos()).atan2(-a.sin());
    // A tree's trunk (the avenue's, planted first) within `d` of `p`.
    let by_tree = |b: &Builder, p: Vec3, d: f32| b.map.props.iter().any(|q| q.trunk.is_some() && q.pos.xz().distance(p.xz()) < d);
    for segs in blocks(r) {
        let n = segs.len();
        for (i, &(s0, s1)) in segs.iter().enumerate() {
            let lift = if i % 2 == 0 { 0.0 } else { 0.02 };
            let roof = r.g + r.u + lift + 0.04;
            // A stair room every eight or so shops, else one or two things, kept off the parapets.
            if i % 8 == 3 {
                place(b, "mumty", r.at((r.ri + r.ro) / 2.0 + rng.random_range(-4.0..4.0), (s0 + s1) / 2.0, roof), along(s0) + if rng.random_bool(0.5) { 0.0 } else { tau / 2.0 });
            } else {
                for _ in 0..rng.random_range(1..5) {
                    let name = ROOF[rng.random_range(0..ROOF.len())];
                    let rad = rng.random_range(r.ri + 2.0..r.ro - 2.0);
                    let yaw = if name == "solar" || name == "clothesline" { along(s0) } else { rng.random_range(0.0..tau) };
                    place(b, name, r.at(rad, s0 + (s1 - s0) * rng.random_range(0.2..0.8), roof), yaw);
                }
            }
            // The kerbs: the inner circle's against the colonnade, the outer circle's beyond it.
            for (kerb, out) in [(r.ri - 2.6, -1.0f32), (r.ro + 2.6, 1.0)] {
                // Not across the radial streets' mouths, nor at the block's ends (where the stalls are).
                if i <= 1 || i + 2 >= n {
                    continue;
                }
                let roll = rng.random_range(0.0..1.0);
                if roll < 0.2 {
                    // A row of two-wheelers nosed in, angled, side by side down the bay.
                    let chord = (s1 - s0) * kerb;
                    let count = ((chord / 0.95) as usize).min(3);
                    for k in 0..count {
                        if rng.random_bool(0.2) {
                            continue;
                        }
                        let a = s0 + (s1 - s0) * (k as f32 + 0.5) / count as f32;
                        let name = BIKES[rng.random_range(0..BIKES.len())];
                        if by_tree(b, r.at(kerb + out * 0.4, a, 0.0), 1.4) {
                            continue;
                        }
                        place(b, name, r.at(kerb + out * 0.4, a, 0.0), radial_yaw(a) + if out > 0.0 { tau / 2.0 } else { 0.0 } + 0.5 + rng.random_range(-0.08..0.08));
                    }
                } else if roll < 0.35 {
                    let a = (s0 + s1) / 2.0;
                    place(b, "auto", r.at(kerb - out * 0.6, a, 0.0), along(a) + if rng.random_bool(0.5) { 0.0 } else { tau / 2.0 });
                }
            }
            // Cars parked a bay at a time along both circles' edges on the colonnade's side.
            if i > 1 && i + 2 < n {
                const CARS: [&str; 4] = ["car_white", "car_red", "car_silver", "car_blue"];
                let a = (s0 + s1) / 2.0;
                for (rad, odds) in [(inner_road(r).1 - 1.1, 0.35), (outer_road(r).0 + 1.1, 0.3)] {
                    if rng.random_bool(odds) && (rad < r.ro || clear.at(a - 2.5 / rad) && clear.at(a + 2.5 / rad)) {
                        place(b, CARS[rng.random_range(0..CARS.len())], r.at(rad, a, 0.0), along(a) + if rng.random_bool(0.5) { 0.0 } else { tau / 2.0 });
                    }
                }
            }
            // A stall at each block's ends, by the street corner, facing the passers-by.
            if i == 1 || i + 2 == n {
                for (kerb, out) in [(r.ri - 1.6, -1.0f32), (r.ro + 3.0, 1.0)] {
                    let name = if rng.random_bool(0.5) { "chai" } else { "thela" };
                    let a = (s0 + s1) / 2.0;
                    let yaw = if name == "chai" { radial_yaw(a) + if out > 0.0 { 0.0 } else { tau / 2.0 } } else { along(a) };
                    if !by_tree(b, r.at(kerb + out * 1.5, a, 0.0), 2.0) {
                        place(b, name, r.at(kerb + out * 1.5, a, 0.0), yaw);
                    }
                }
            }
            // By a column now and then, in the verandah: out of the bay's middle, where loot lies.
            if rng.random_bool(0.18) {
                let (rad, name) = if rng.random_bool(0.5) { (r.ri + 1.3, "planter") } else { (r.ro - 1.3, ["crates", "cooler", "pot"][rng.random_range(0..3)]) };
                let a = s0 + 1.3 / rad;
                place(b, name, r.at(rad, a, 0.0), rng.random_range(0.0..tau));
            }
        }
    }
}


/// Central Park dressed: stone paths from the fountain out along each radial road and a ring path
/// round it, flower beds and lamps along them, benches facing in (all on the grass, drawn only;
/// the benches, lamps and beds take colliders as kit pieces do).
fn park(b: &mut Builder, rng: &mut SmallRng, r: &Ring) {
    let w = Frame::new(Vec3::ZERO, 0.0);
    let ground = |b: &Builder, p: Vec3| b.map.terrain.height(p.x, p.z);
    // Radial paths, in 4 m lengths so they follow the ground.
    for &a in &r.streets {
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        let mut d = 8.0;
        while d < r.park - 4.0 {
            let p = r.center + dir * (d + 2.0);
            b.deco(Frame::new(Vec3::new(p.x, ground(b, p), p.z), -a), Vec3::new(0.0, 0.015, 0.0), Vec3::new(4.0, 0.03, 3.2), Mat::Stone);
            d += 4.0;
        }
        // Benches and lamps either side, every 14 m.
        let mut d = 18.0;
        while d < r.park - 10.0 {
            for side in [-1.0f32, 1.0] {
                let n = Vec3::new(-a.sin(), 0.0, a.cos()) * side;
                let at = r.center + dir * d + n * 2.6;
                let pos = Vec3::new(at.x, ground(b, at), at.z);
                let name = if (d as i32 / 14) % 2 == 0 { "bench" } else { "lamp" };
                super::kit::place(b, name, pos, (-n.x).atan2(-n.z) + std::f32::consts::PI);
            }
            d += 14.0;
        }
    }
    // A ring path at two thirds out, in short chords; flower beds round it between the roads. It
    // stops at the foot of any hill in its way (flat pieces would float on a slope).
    let ring_r = r.park * 0.62;
    let n = (std::f32::consts::TAU * ring_r / 3.5) as usize;
    let level = |b: &Builder, p: Vec3, reach: f32| {
        let hs = [Vec3::ZERO, Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z].map(|o| ground(b, p + o * reach));
        hs.iter().cloned().fold(f32::MIN, f32::max) - hs.iter().cloned().fold(f32::MAX, f32::min) < 0.03
    };
    for k in 0..n {
        let a = std::f32::consts::TAU * (k as f32 + 0.5) / n as f32;
        let p = r.at(ring_r, a, 0.0);
        if !level(b, p, 2.0) {
            continue;
        }
        b.deco(Frame::new(Vec3::new(p.x, ground(b, p), p.z), -a), Vec3::new(0.0, 0.016 + 0.002 * (k % 2) as f32, 0.0), Vec3::new(3.0, 0.03, 3.7), Mat::Stone);
        if k % 4 == 2 && rng.random_bool(0.7) && level(b, r.at(ring_r + 4.0, a, 0.0), 1.0) {
            let q = r.at(ring_r + 4.0, a, 0.0);
            super::kit::place(b, "planter", Vec3::new(q.x, ground(b, q), q.z), 0.0);
        }
    }
    let _ = w;
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;
    use crate::map::{Block, MapLayout, Shape};

    /// The ring at Connaught Place's real size (crate::map::delhi::ring), round the origin.
    fn ring() -> Ring {
        Ring {
            center: Vec3::ZERO,
            park: 112.0,
            ri: 136.0,
            ro: 168.0,
            ver: 4.2,
            g: 5.5,
            u: 4.6,
            seg_len: 4.6,
            street: 13.0,
            streets: [34.0f32, 93.6, 150.6, 212.2, 274.6, 304.1, 332.2].iter().map(|d| d.to_radians()).collect(),
            flag: 45.0,
        }
    }

    fn built(r: &Ring) -> MapLayout {
        // Flat ground under it (main's ring reads the ground: its park's hills, its kerbs).
        let (size, n) = (400.0, 401);
        let terrain = crate::map::terrain::Terrain { n, size, h: vec![0.0; n * n], road_d: vec![1e3; n * n], ground: vec![0; n * n] };
        let mut map = MapLayout { terrain, ..Default::default() };
        build(&mut Builder::new(&mut map), &mut SmallRng::seed_from_u64(7), r);
        map
    }

    /// A chest spills what it holds 1 m along its +Z (sim::loot::open_chest). Every ground chest
    /// in the ring must spill toward the arch of its own shop: the inner row toward the park, the
    /// outer row away from it, never into the party wall between them.
    #[test]
    fn ring_chests_face_their_shop_front() {
        let r = ring();
        let map = built(&r);
        let mid = (r.ri + r.ro) / 2.0;
        let ground: Vec<_> = map.chests.iter().filter(|c| c.kind == ChestKind::Peti && c.pos.y < 0.5).collect();
        assert!(ground.len() > 100, "{} ground chests", ground.len());
        let mut shown = [false; 2];
        for c in ground {
            let spill = c.pos + Quat::from_rotation_y(c.yaw) * Vec3::Z;
            let inner = c.pos.xz().length() < mid;
            // The arch is on the chest's bearing, in the shop front its row opens through.
            let arch = c.pos.xz().normalize() * if inner { r.ri + r.ver } else { r.ro - r.ver };
            let (dc, ds) = (c.pos.xz().distance(arch), spill.xz().distance(arch));
            if !std::mem::replace(&mut shown[inner as usize], true) {
                let row = if inner { "inner" } else { "outer" };
                println!("{row} row: chest {:.2?} yaw {:.3} spills at {:.2?}; arch {arch:.2?}: {dc:.2} m from the chest, {ds:.2} m from the spill", c.pos, c.yaw, spill);
            }
            assert!(ds < dc - 0.99, "chest at {:.1?} yaw {:.2} spills at {:.1?}, {ds:.2} m from its arch at {arch:.1?} ({dc:.2} m)", c.pos, c.yaw, spill);
        }
    }

    /// No chest stands inside the ring's masonry: the stairs climb through the inner shops.
    #[test]
    fn ring_chests_stand_clear_of_the_masonry() {
        let map = built(&ring());
        let inside = |b: &Block, p: Vec3| {
            let l = b.rot.inverse() * (p - b.pos);
            match b.shape {
                Shape::Box(s) => l.abs().cmplt(s / 2.0).all(),
                Shape::Cylinder { radius, height } => l.xz().length() < radius && l.y.abs() < height / 2.0,
                Shape::Sphere(r) => l.length() < r,
            }
        };
        for c in &map.chests {
            let p = c.pos + Vec3::Y * 0.4;
            let hit = map.blocks.iter().find(|b| (b.visible || b.collide) && inside(b, p));
            assert!(hit.is_none(), "chest at {:.1?} stands in {:?}", c.pos, hit.map(|b| (b.shape, b.pos)));
        }
    }
}
