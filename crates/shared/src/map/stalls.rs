//! The market's rows: trestle stalls along the kerbs of a busy place, handcarts and scooters parked
//! in front of them, banners over the road. A table is a solid thing, so where it stands is map data:
//! worked out here from the map alone, the same on every peer, and only then dressed by the client
//! (`life` in the game). The goods on it, the tarp over it, the wires and the cloth stay the client's.
//!
//! Each table is one box, and the boxes of a patch of street are one compound collider
//! (`Rows::patches`): the rows are thousands of tables and a few hundred colliders. The street floor
//! draws them (`StreetFloor::new`), so bots walk round them; bullets, sight and vehicles meet them as
//! they meet any wall. A table is a little over a crouched warrior's head from where it counts.
//!
//! What keeps a row from shutting anything: nothing stands on a door's way out, near a bay (a parked
//! vehicle has to get out again), on a bridge or its ramps, or on the loot, a row leaves the
//! carriageway clear, and no unbroken wall of tables runs longer than `WALL`: `cut_walls` opens a way
//! through it, across both rows, whatever line the road takes. `report` measures all of that on the
//! finished map, and `ncr_map_check` and a test fail if any of it stops being so.

use std::collections::{BTreeMap, HashMap, HashSet};

use bevy::prelude::*;

use super::compose::{SPANS, Span};
use super::interiors::Door;
use super::terrain::{Terrain, ground};
use super::{Block, MapLayout, Mat, Shape, kit};
use crate::sim::nav::StreetFloor;

/// Metres a side of the squares business is measured on.
const SQUARE: f32 = 32.0;
/// How busy a place is at most, by name. Anywhere else keeps to `QUIET`.
const PLACES: [(&str, f32); 26] = [
    ("Chandni Chowk", 1.0),
    ("Karol Bagh", 1.0),
    ("Lajpat Nagar", 1.0),
    ("Connaught Place", 0.9),
    ("Kamla Nagar", 0.95),
    ("Laxmi Nagar", 0.85),
    ("Shahdara", 0.8),
    ("Raj Nagar", 0.7),
    ("Lal Kuan", 0.7),
    ("Mayur Vihar", 0.7),
    ("Surajkund Mela Ground", 0.8),
    ("Sector 18", 0.75),
    ("Nehru Place", 0.7),
    ("Anand Vihar", 0.7),
    ("Ghaziabad", 0.65),
    ("Hauz Khas", 0.55),
    ("Azadpur Mandi", 0.6),
    ("New Delhi Station", 0.6),
    ("Kashmere Gate", 0.55),
    ("Sarai Kale Khan", 0.45),
    ("MG Road", 0.4),
    ("Saket", 0.4),
    ("Indirapuram", 0.3),
    ("Cyber City", 0.25),
    ("Rajouri Garden", 0.4),
    ("City Centre", 0.4),
];
const QUIET: f32 = 0.06;
/// Stalls in a square at which it is as full as its place gets.
const FULL_SQUARE: f32 = 2.0;
/// How far out from a door the way is kept clear, the door's own width and this much each side.
const WALK_LINE: f32 = 9.0;
const SHOP_FRONT: f32 = 2.4;
const LANE_SIDE: f32 = 0.5;
/// Metres a side of a patch of street: the tables in one are one collider.
pub const PATCH: f32 = 64.0;
/// A table's solid part: wide, high and deep. It stops at the top of what is heaped on it, a little
/// over the head of a warrior crouched behind it, and goes on under the ground.
pub const TABLE: Vec3 = Vec3::new(2.5, 1.05, 1.1);
const BURIED: f32 = 0.25;
/// Tables closer than this are one wall: a warrior's 0.76 m and room to run, as the street grid
/// sees it, with its half-metre cells and every edge grown by 0.4 m.
const PASS: f32 = 1.6;
/// The longest unbroken wall of tables, and the way cut through one. Together: a way through every
/// 15 m, as a market leaves them.
const WALL: f32 = 12.5;
const WAY: f32 = 2.4;
/// How far a table keeps from a bay's middle: a vehicle parked there pulls out at an angle, and the
/// long ones are 4.3 m.
const BAY_CLEAR: f32 = 7.0;
/// How far a table keeps from a bridge, beyond its ramps and to each side of its deck.
const BRIDGE_CLEAR: (f32, f32) = (12.0, 5.0);
/// How much open road a row leaves in front of it.
const LANE: f32 = 4.5;
/// Fewer free cells than this between tables is a pocket no one gets into: about 4 m by 3.
const POCKET: usize = 48;

/// A cheap hash to 0..1, for choices that follow from where a thing stands.
pub fn hash(a: u32, b: u32, c: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca77) ^ c.wrapping_mul(0xc2b2_ae3d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

fn road_at(t: &Terrain, p: Vec2) -> bool {
    t.ground_at(p.x, p.y) == Some(ground::ROAD)
}

/// How much of a square round `c` is ground people walk on.
fn street_share(layout: &MapLayout, c: Vec2) -> f32 {
    let t = &layout.terrain;
    let mut on = 0;
    for j in 0..4 {
        for i in 0..4 {
            let p = c + (Vec2::new(i as f32, j as f32) - 1.5) * SQUARE / 4.0;
            if matches!(t.ground_at(p.x, p.y), Some(ground::ROAD | ground::PAVEMENT | ground::PATH | ground::PLAZA | ground::PARKING)) {
                on += 1;
            }
        }
    }
    (on as f32 / 8.0).min(1.0)
}

/// How busy each square of a map is, 0 (nobody about) to 1 (the thick of Chandni Chowk), from the
/// map's own kit, ground classes and place names, so it works in every district.
#[derive(Clone, Debug, Default)]
pub struct Busy {
    half: f32,
    side: usize,
    cells: Vec<f32>,
}

/// Whether a kit piece stands on the street as a stall: not on a roof or a bridge deck, and never in
/// a holy site.
pub fn street_stall(layout: &MapLayout, k: &kit::KitPiece) -> bool {
    matches!(k.name, "thela" | "chai" | "crates" | "chairs") && k.pos.y - layout.terrain.height(k.pos.x, k.pos.z) <= 0.5 && !layout.is_holy(k.pos.xz())
}

impl Busy {
    fn square(&self, p: Vec2) -> Option<usize> {
        let (i, j) = (((p.x + self.half) / SQUARE).floor(), ((p.y + self.half) / SQUARE).floor());
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.side && (j as usize) < self.side).then(|| j as usize * self.side + i as usize)
    }

    /// How busy a spot is.
    pub fn at(&self, p: Vec2) -> f32 {
        self.square(p).map_or(0.0, |k| self.cells[k])
    }

    pub fn of(layout: &MapLayout) -> Self {
        let half = layout.half_extent;
        let side = (2.0 * half / SQUARE).ceil() as usize;
        let mut busy = Busy { half, side, cells: vec![0.0; side * side] };
        let mut stalls = vec![0.0f32; side * side];
        for k in layout.kit.iter().filter(|k| street_stall(layout, k)) {
            if let Some(s) = busy.square(k.pos.xz()) {
                stalls[s] += 1.0;
            }
        }
        let places: Vec<(Vec2, f32, f32)> = layout.pois.iter().filter_map(|p| Some((p.center.xz(), p.radius, PLACES.iter().find(|(n, _)| *n == p.name)?.1))).collect();
        for j in 0..side {
            for i in 0..side {
                let c = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * SQUARE - Vec2::splat(half);
                let place = places
                    .iter()
                    .map(|(at, r, weight)| {
                        let d = (c - *at).abs().max_element() / r.max(1.0);
                        weight * (1.0 - ((d - 0.7) / 0.5).clamp(0.0, 1.0))
                    })
                    .fold(QUIET, f32::max);
                // A stall nearby counts towards this square too.
                let mut round = 0.0;
                for dj in -1i32..=1 {
                    for di in -1i32..=1 {
                        let (x, z) = (i as i32 + di, j as i32 + dj);
                        if x >= 0 && z >= 0 && (x as usize) < side && (z as usize) < side {
                            round += stalls[z as usize * side + x as usize] / if di == 0 && dj == 0 { 1.0 } else { 3.0 };
                        }
                    }
                }
                busy.cells[j * side + i] = place * (0.6 + 0.4 * (round / FULL_SQUARE).min(1.0)) * street_share(layout, c);
            }
        }
        busy
    }
}

/// What stands in a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A trestle stall: solid.
    Table,
    /// A handcart at the kerb: dressing.
    Cart,
    /// Scooters parked in a bunch at the road's edge: dressing.
    Scooters,
}

#[derive(Clone, Copy, Debug)]
pub struct Stall {
    pub kind: RowKind,
    pub pos: Vec3,
    pub yaw: f32,
    /// In the row on the road's side, its front to the traffic.
    pub first: bool,
}

impl Stall {
    /// Half its width and depth.
    pub fn half(&self) -> Vec2 {
        match self.kind {
            RowKind::Table => Vec2::new(TABLE.x, TABLE.z) / 2.0,
            RowKind::Cart => Vec2::new(0.65, 1.1),
            RowKind::Scooters => Vec2::new(2.2, 0.9),
        }
    }

    /// Which way it faces, on the ground.
    pub fn facing(&self) -> Vec2 {
        (Quat::from_rotation_y(self.yaw) * Vec3::NEG_Z).xz()
    }

    /// The solid box of a table.
    pub fn block(&self) -> Block {
        let size = TABLE + Vec3::Y * BURIED;
        Block { pos: self.pos + Vec3::Y * (TABLE.y - BURIED) / 2.0, rot: Quat::from_rotation_y(self.yaw), shape: Shape::Box(size), mat: Mat::Wood, collide: true, visible: false }
    }

    /// Whether `p` is on its footprint.
    pub fn covers(&self, p: Vec2) -> bool {
        let d = p - self.pos.xz();
        let f = self.facing();
        d.dot(f).abs() <= self.half().y && d.dot(f.perp()).abs() <= self.half().x
    }
}

/// The map's rows. Empty on the small maps.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    pub stalls: Vec<Stall>,
    /// Banners over the road, kerb to kerb.
    pub banners: Vec<(Vec3, Vec3)>,
    /// The metre squares in front of a building's doors, kept clear: what stands here would look
    /// like it bars the way in.
    pub lanes: HashSet<(i32, i32)>,
}

impl Rows {
    pub fn tables(&self) -> impl Iterator<Item = &Stall> {
        self.stalls.iter().filter(|s| s.kind == RowKind::Table)
    }

    /// Each patch of street's tables, as boxes: each patch is one collider. In a fixed order.
    pub fn patches(&self) -> Vec<Vec<Block>> {
        let mut by_patch: BTreeMap<(i32, i32), Vec<Block>> = BTreeMap::new();
        for s in self.tables() {
            by_patch.entry(((s.pos.x / PATCH).floor() as i32, (s.pos.z / PATCH).floor() as i32)).or_default().push(s.block());
        }
        by_patch.into_values().collect()
    }

    /// Whether `p` is on the way out of a door.
    pub fn on_lane(&self, p: Vec2) -> bool {
        self.lanes.contains(&(p.x.floor() as i32, p.y.floor() as i32))
    }

    /// The way out of a door: a walker's line to the road from a real door, and from a shop's open
    /// bay as far as a customer stands.
    fn keep_clear(&mut self, door: &Door) {
        let out_to = if door.leaves { WALK_LINE } else { SHOP_FRONT };
        let half = door.width * 0.5 + LANE_SIDE;
        let across = door.out.perp();
        let (mut s, foot) = (0.0, door.pos.xz());
        while s <= out_to {
            let mut t = -half;
            while t <= half {
                let p = foot + door.out * s + across * t;
                self.lanes.insert((p.x.floor() as i32, p.y.floor() as i32));
                t += 0.5;
            }
            s += 0.5;
        }
    }
}

/// How `p` sits to a bridge: metres past its ends, ramps included, and off to the side of its deck.
fn off_bridge(s: &Span, p: Vec2) -> (f32, f32) {
    let along = (p - s.a).dot(s.dir());
    let past = (-s.ramp() - along).max(along - s.len() - s.ramp());
    (past, (p - s.at(along, 0.0)).length() - s.width / 2.0)
}

/// Whether `p` stands in a bridge's way.
fn on_bridge(p: Vec2) -> bool {
    SPANS.iter().any(|s| {
        let (past, aside) = off_bridge(s, p);
        past < BRIDGE_CLEAR.0 && aside < BRIDGE_CLEAR.1
    })
}

/// What a row has to keep off, gathered once.
struct Ground<'a> {
    map: &'a MapLayout,
    floor: &'a StreetFloor,
    busy: Busy,
    /// Footprints by the squares they touch.
    buildings: HashMap<(i32, i32), Vec<u32>>,
    /// Where loot lies, by 3 m squares: nothing may stand over it.
    loot: HashSet<(i32, i32)>,
}

impl Ground<'_> {
    fn built(&self, p: Vec2) -> bool {
        let key = ((p.x / SQUARE).floor() as i32, (p.y / SQUARE).floor() as i32);
        self.buildings.get(&key).is_some_and(|ids| ids.iter().any(|i| super::footprint::contains(&self.map.footprints[*i as usize].pts, p)))
    }

    /// Whether loot lies round `p`: a stall over it would hide it, and no one could tell.
    fn over_loot(&self, p: Vec2) -> bool {
        let (i, j) = ((p.x / 3.0).floor() as i32, (p.y / 3.0).floor() as i32);
        (-1..=1).any(|a| (-1..=1).any(|b| self.loot.contains(&(i + a, j + b))))
    }

    /// Whether a bay or a bridge needs the spot.
    fn traffic(&self, p: Vec2) -> bool {
        on_bridge(p) || self.map.parking.iter().any(|b| b.pos.xz().distance_squared(p) < BAY_CLEAR * BAY_CLEAR)
    }

    /// Whether a stall can stand here: open paved ground, or the edge of a wide road if `spilt`, no
    /// kit or tree, no building against it.
    fn room_for(&self, rows: &Rows, p: Vec2, along: Vec2, reach: f32, spilt: bool) -> bool {
        let t = &self.map.terrain;
        [0.0, reach, -reach].iter().all(|k| {
            let q = p + along * *k;
            let deep = along.perp() * 0.7;
            self.floor.walkable(q) && (spilt || !road_at(t, q)) && !matches!(t.ground_at(q.x, q.y), Some(ground::WATER | ground::OPEN | ground::SAND)) && !self.built(q + along.perp() * 1.5) && !self.built(q - along.perp() * 1.5) && !rows.on_lane(q) && !rows.on_lane(q + deep) && !rows.on_lane(q - deep)
        }) && !self.map.is_holy(p) && !self.over_loot(p)
            && !self.traffic(p)
    }
}

/// Stall rows along both kerbs of the roads of a busy place: a wall of goods and tarps either side of
/// the road, and a second row back to back with the first where the place is busier still, facing the
/// shops, with a keeper's gap between them. The map's roads are far wider than a market leaves them,
/// so on a wide road the rows step out onto it the way stalls do, up to `SPILL`, and the road narrows
/// to about `CARRIAGEWAY` of open tarmac: what the eye reads from the road is a lane, and the pavement
/// behind is a way along the shop fronts, clear of every table. Handcarts and parked scooters stand
/// along the row's front, and banners go over the road.
fn lay_rows(g: &Ground, rows: &mut Rows) {
    const ROW_BUSY: f32 = 0.3;
    const SECOND_BUSY: f32 = 0.5;
    const SPACING: f32 = 2.75;
    /// From the first row's middle to the second's, back to back.
    const BACK_ROW: f32 = 2.5;
    const LOOK: isize = 6;
    /// The open road left between the rows of a wide road, and how far a row steps out onto it.
    const CARRIAGEWAY: f32 = 10.0;
    const SPILL: f32 = 4.5;
    let t = &g.map.terrain;
    let n = t.n as isize;
    let road = |i: isize, j: isize| i >= 0 && j >= 0 && i < n && j < n && t.ground[(i * n + j) as usize] == ground::ROAD;
    // The kerb: paved samples next to road, each with which way the road lies.
    let mut kerb: Vec<(Vec2, Vec2)> = vec![];
    for j in 1..n - 1 {
        for i in 1..n - 1 {
            if road(i, j) || !(road(i + 1, j) || road(i - 1, j) || road(i, j + 1) || road(i, j - 1)) {
                continue;
            }
            let p = Vec2::new(t.coord(i as usize), t.coord(j as usize));
            if g.busy.at(p) < ROW_BUSY {
                continue;
            }
            let mut to = Vec2::ZERO;
            for dj in -LOOK..=LOOK {
                for di in -LOOK..=LOOK {
                    if di * di + dj * dj <= LOOK * LOOK && road(i + di, j + dj) {
                        to += Vec2::new(di as f32, dj as f32);
                    }
                }
            }
            if to.length() > 8.0 {
                kerb.push((p, to.normalize()));
            }
        }
    }
    // The road's width from each sample to the far kerb, a junction no wider than a wide road.
    let widths: Vec<f32> = kerb
        .iter()
        .map(|(p, to)| {
            let mut wide = 2.0;
            while wide < 28.0 && road_at(t, *p + *to * wide) {
                wide += 1.0;
            }
            wide
        })
        .collect();
    // Smoothed: the grid makes a kerb a staircase, a row should stand in a line.
    let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (k, (p, _)) in kerb.iter().enumerate() {
        cells.entry(((p.x / 8.0).floor() as i32, (p.y / 8.0).floor() as i32)).or_default().push(k as u32);
    }
    let mut taken: HashMap<(i32, i32), Vec<Vec2>> = HashMap::new();
    let close = |taken: &HashMap<(i32, i32), Vec<Vec2>>, p: Vec2, gap: f32| {
        let (ci, cj) = ((p.x / 4.0).floor() as i32, (p.y / 4.0).floor() as i32);
        (-1..=1).any(|a| (-1..=1).any(|b| taken.get(&(ci + a, cj + b)).is_some_and(|c| c.iter().any(|q| q.distance_squared(p) < gap * gap))))
    };
    let mark = |taken: &mut HashMap<(i32, i32), Vec<Vec2>>, p: Vec2| taken.entry(((p.x / 4.0).floor() as i32, (p.y / 4.0).floor() as i32)).or_default().push(p);
    for (k, &(p, to)) in kerb.iter().enumerate() {
        let (a, b) = ((p.x * 3.0) as i32 as u32, (p.y * 3.0) as i32 as u32);
        let busy = g.busy.at(p);
        // The same side's kerb round here, averaged.
        let (ci, cj) = ((p.x / 8.0).floor() as i32, (p.y / 8.0).floor() as i32);
        let (mut sum_n, mut sum_off, mut sum_wide, mut count, mut far) = (Vec2::ZERO, 0.0, 0.0, 0.0, 0.0);
        for u in -2..=2 {
            for v in -2..=2 {
                for q in cells.get(&(ci + u, cj + v)).into_iter().flatten() {
                    let (qp, qn) = kerb[*q as usize];
                    if qn.dot(to) <= 0.8 {
                        continue;
                    }
                    let d = qp.distance_squared(p);
                    if d < 64.0 {
                        sum_n += qn;
                        sum_off += (qp - p).dot(to);
                        count += 1.0;
                    }
                    // The road's width goes by a longer stretch: a row keeps its line along it.
                    if d < 400.0 {
                        sum_wide += widths[*q as usize];
                        far += 1.0;
                    }
                }
            }
        }
        if count < 2.0 {
            continue;
        }
        let normal = sum_n.normalize_or(to);
        let edge = p + to * (sum_off / count);
        let along = normal.perp();
        let run = edge.dot(along);
        let yaw = (-normal.x).atan2(-normal.y);
        let y = |q: Vec2| t.height(q.x, q.y);
        // How far the row steps out onto the road, only along a straight run of kerb: stepping out at
        // a corner would close the junction.
        let straight = [-7.0, 7.0].iter().all(|k| !road_at(t, edge - normal * 1.5 + along * *k) && road_at(t, edge + normal * 2.5 + along * *k));
        let spill = if straight { ((sum_wide / far - CARRIAGEWAY) * 0.5).clamp(0.0, SPILL) } else { 0.0 };
        // The first row, its front at the kerb or out on the road.
        let first = edge + normal * (spill - 1.0);
        // A row leaves a way along the shop fronts behind it: where the pavement is too thin for that,
        // a warrior would have to run through the tables, so there's no row.
        let way_behind = |p: Vec2| !g.built(p - normal * 3.2);
        if !close(&taken, first, SPACING) && hash(a, b, 31) < busy * 1.9 && (spill > 1.2 || way_behind(first)) && g.room_for(rows, first, along, 1.1, spill > 0.0) {
            mark(&mut taken, first);
            rows.stalls.push(Stall { kind: RowKind::Table, pos: Vec3::new(first.x, y(first), first.y), yaw, first: true });
        }
        // The second, back to back with it, facing the shops.
        let second = first - normal * BACK_ROW;
        if busy >= SECOND_BUSY && !close(&taken, second, SPACING) && hash(a, b, 32) < busy * 1.5 && way_behind(second) && g.room_for(rows, second, along, 1.1, spill > 0.0) {
            mark(&mut taken, second);
            rows.stalls.push(Stall { kind: RowKind::Table, pos: Vec3::new(second.x, y(second), second.y), yaw: yaw + std::f32::consts::PI, first: false });
        }
        // Parked along the row's front, on the road: bunches of scooters, handcarts.
        let verge = edge + normal * (spill + 1.3);
        if road_at(t, verge) && !close(&taken, verge, 5.0) && g.floor.walkable(verge) && !rows.on_lane(verge) && !g.map.is_holy(verge) && hash(a, b, 33) < 0.16 + busy * 0.26 {
            mark(&mut taken, verge);
            let at = Vec3::new(verge.x, y(verge), verge.y);
            if hash(a, b, 34) < 0.3 {
                rows.stalls.push(Stall { kind: RowKind::Cart, pos: at, yaw: yaw + hash(a, b, 35) * 0.3 + std::f32::consts::FRAC_PI_2, first: false });
            } else {
                rows.stalls.push(Stall { kind: RowKind::Scooters, pos: at, yaw, first: false });
            }
        }
        // A banner over the road, once for each crossing.
        if k % 3 == 0 && (normal.x + normal.y * 0.37) > 0.0 && (run / 26.0).rem_euclid(1.0) < 2.0 / 26.0 {
            let mut d = 2.0;
            while d < 34.0 && road_at(t, edge + normal * d) {
                d += 1.0;
            }
            let far = edge + normal * d;
            if (8.0..33.0).contains(&d) && !g.map.is_holy(edge) && !g.map.is_holy(far) && !rows.banners.iter().rev().take(40).any(|b| b.0.xz().distance_squared(edge) < 400.0) {
                rows.banners.push((Vec3::new(edge.x, y(edge), edge.y), Vec3::new(far.x, y(far), far.y)));
            }
        }
    }
}

/// The nearest two tables' footprints come to each other, near enough: the largest gap on any of
/// their edges' axes, which is never more than the true one.
fn apart(a: &Stall, b: &Stall) -> f32 {
    let d = b.pos.xz() - a.pos.xz();
    let (fa, fb) = (a.facing(), b.facing());
    let reach = |axis: Vec2, s: &Stall, f: Vec2| s.half().y * axis.dot(f).abs() + s.half().x * axis.dot(f.perp()).abs();
    [fa, fa.perp(), fb, fb.perp()].iter().map(|axis| d.dot(*axis).abs() - reach(*axis, a, fa) - reach(*axis, b, fb)).fold(f32::MIN, f32::max).max(0.0)
}

/// The walls of tables: groups of them too close to pass between, by index into `stalls`, each with
/// its length. The rows back to back are one wall: what the gap between them gives is a way along.
fn walls(stalls: &[Stall]) -> Vec<(Vec<usize>, f32, Vec2, Vec2)> {
    let tables: Vec<usize> = (0..stalls.len()).filter(|k| stalls[*k].kind == RowKind::Table).collect();
    let mut parent: Vec<usize> = (0..stalls.len()).collect();
    fn root(parent: &mut [usize], mut k: usize) -> usize {
        while parent[k] != k {
            parent[k] = parent[parent[k]];
            k = parent[k];
        }
        k
    }
    let bucket = |p: Vec3| ((p.x / 6.0).floor() as i32, (p.z / 6.0).floor() as i32);
    let mut buckets: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for &k in &tables {
        buckets.entry(bucket(stalls[k].pos)).or_default().push(k);
    }
    for &k in &tables {
        let (ci, cj) = bucket(stalls[k].pos);
        for j in cj - 1..=cj + 1 {
            for i in ci - 1..=ci + 1 {
                for &q in buckets.get(&(i, j)).into_iter().flatten() {
                    if q > k && apart(&stalls[k], &stalls[q]) < PASS {
                        let (x, y) = (root(&mut parent, k), root(&mut parent, q));
                        parent[x] = y;
                    }
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &k in &tables {
        groups.entry(root(&mut parent, k)).or_default().push(k);
    }
    groups
        .into_values()
        .map(|wall| {
            let at: Vec<Vec2> = wall.iter().map(|k| stalls[*k].pos.xz()).collect();
            let mean = at.iter().sum::<Vec2>() / at.len() as f32;
            let (mut xx, mut xz, mut zz) = (0.0, 0.0, 0.0);
            for p in &at {
                let d = *p - mean;
                xx += d.x * d.x;
                xz += d.x * d.y;
                zz += d.y * d.y;
            }
            let axis = Vec2::from_angle(0.5 * (2.0 * xz).atan2(xx - zz));
            let (lo, hi) = at.iter().map(|p| (*p - mean).dot(axis)).fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s), b.max(s)));
            (wall, hi - lo + TABLE.x, mean, axis)
        })
        .collect()
}

/// Opens a way through every wall of tables longer than `WALL`, at even spaces along it and across
/// both rows, by taking out the tables that stand in the way. Done on the tables themselves and not by
/// the road's line, as a kerb that bends or runs aslant gives a line no one can count along.
fn cut_walls(stalls: &mut Vec<Stall>) {
    for _ in 0..8 {
        let mut gone = HashSet::new();
        for (wall, long, mean, axis) in walls(stalls) {
            if long <= WALL {
                continue;
            }
            let along = |k: usize| (stalls[k].pos.xz() - mean).dot(axis);
            let lo = wall.iter().map(|k| along(*k)).fold(f32::MAX, f32::min);
            let cuts = ((long + WAY) / (WALL + WAY)).ceil() as usize - 1;
            for c in 1..=cuts {
                let aim = lo + (long - TABLE.x) * c as f32 / (cuts + 1) as f32;
                let Some(&mid) = wall.iter().min_by(|a, b| (along(**a) - aim).abs().total_cmp(&(along(**b) - aim).abs())) else { continue };
                let at = along(mid);
                gone.extend(wall.iter().copied().filter(|k| (along(*k) - at).abs() < (WAY + TABLE.x) / 2.0));
            }
        }
        if gone.is_empty() {
            return;
        }
        let mut k = 0;
        stalls.retain(|_| {
            k += 1;
            !gone.contains(&(k - 1))
        });
    }
}

/// Takes out a table on the road with another too close before it: a kerb that bends can bring two
/// rows face to face.
fn keep_lanes(map: &MapLayout, stalls: &mut Vec<Stall>) {
    let t = &map.terrain;
    let bucket = |p: Vec2| ((p.x / 4.0).floor() as i32, (p.y / 4.0).floor() as i32);
    let mut near: HashMap<(i32, i32), Vec<Stall>> = HashMap::new();
    for s in stalls.iter().filter(|s| s.kind == RowKind::Table) {
        near.entry(bucket(s.pos.xz())).or_default().push(*s);
    }
    let in_the_way = |s: &Stall| {
        let f = s.facing();
        (0..=(LANE * 2.0) as i32).any(|k| {
            let p = s.pos.xz() + f * (s.half().y + 0.2 + k as f32 * 0.5);
            let (ci, cj) = bucket(p);
            (cj - 1..=cj + 1).any(|j| (ci - 1..=ci + 1).any(|i| near.get(&(i, j)).is_some_and(|c| c.iter().any(|o| o.covers(p)))))
        })
    };
    stalls.retain(|s| s.kind != RowKind::Table || !s.first || !road_at(t, s.pos.xz()) || !in_the_way(s));
}

/// Whether the two sides of a table are one street: walkers get from before it to behind it, within
/// the street search's reach. Where either side is a pocket or indoors, there is nothing to shut.
fn sides_meet(floor: &StreetFloor, s: &Stall) -> bool {
    let f = s.facing();
    let spot = |p: Vec2| floor.snap(p, 1.0).filter(|q| !floor.is_indoors(*q) && floor.room_round(*q, POCKET) >= POCKET);
    let (Some(a), Some(b)) = (spot(s.pos.xz() + f * 1.3), spot(s.pos.xz() - f * 1.3)) else { return true };
    floor.path(a, b).is_some()
}

/// Takes out the walls of tables that close a lane from building to building, or the mouth of a yard:
/// those with no way round in reach. A row is a side of a street.
fn open_lanes(map: &MapLayout, floor: &mut StreetFloor, stalls: &mut Vec<Stall>) {
    let blocks: Vec<Block> = stalls.iter().filter(|s| s.kind == RowKind::Table).map(|s| s.block()).collect();
    floor.draw_tables(map, &blocks);
    let mut gone = HashSet::new();
    for (wall, ..) in walls(stalls) {
        if !sides_meet(floor, stalls_mid(stalls, &wall)) {
            gone.extend(wall);
        }
    }
    let mut k = 0;
    stalls.retain(|_| {
        k += 1;
        !gone.contains(&(k - 1))
    });
}

/// Lays out the map's rows, last of all: they keep off everything else.
pub(super) fn lay(map: &mut MapLayout) {
    let mut floor = StreetFloor::without_stalls(map);
    let mut rows = Rows::default();
    for door in map.rooms.iter().flat_map(|r| &r.doors) {
        rows.keep_clear(door);
    }
    let mut buildings: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, f) in map.footprints.iter().enumerate() {
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        for j in (lo.y / SQUARE).floor() as i32..=(hi.y / SQUARE).floor() as i32 {
            for k in (lo.x / SQUARE).floor() as i32..=(hi.x / SQUARE).floor() as i32 {
                buildings.entry((k, j)).or_default().push(i as u32);
            }
        }
    }
    let spots = map.floor_loot.iter().copied().chain(map.chests.iter().map(|c| c.pos));
    let loot = spots.map(|p| ((p.x / 3.0).floor() as i32, (p.z / 3.0).floor() as i32)).collect();
    let ground = Ground { map, floor: &floor, busy: Busy::of(map), buildings, loot };
    lay_rows(&ground, &mut rows);
    keep_lanes(map, &mut rows.stalls);
    cut_walls(&mut rows.stalls);
    open_lanes(map, &mut floor, &mut rows.stalls);
    map.stalls = rows;
}

/// What the rows come to on a finished map. All but the first few should be zero: `failures`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Report {
    pub tables: usize,
    pub colliders: usize,
    pub most_in_one: usize,
    pub carts: usize,
    pub scooters: usize,
    pub walls: usize,
    pub longest_wall: f32,
    /// Tables on a door's way out, on a bay, on a bridge or over loot.
    pub on_doors: usize,
    pub on_bays: usize,
    pub on_bridges: usize,
    pub on_loot: usize,
    /// Doors whose way to the routes is shut, or rooms' routes that no longer start.
    pub doors_shut: usize,
    pub rooms_shut: usize,
    /// Bays with no way for a car out to the road.
    pub bays_shut: usize,
    /// Walls of tables whose front can't be walked round to their back.
    pub no_way_round: usize,
    /// Tables on the road with less than `LANE` of open road before them.
    pub lanes_narrow: usize,
    /// Walls too long, and the longest.
    pub too_long: usize,
}

impl Report {
    pub fn failures(&self) -> usize {
        self.on_doors + self.on_bays + self.on_bridges + self.on_loot + self.doors_shut + self.rooms_shut + self.bays_shut + self.no_way_round + self.lanes_narrow + self.too_long
    }
}

/// Measures the rows on a finished map, with the street floor that knows them.
pub fn report(map: &MapLayout, floor: &StreetFloor) -> Report {
    let rows = &map.stalls;
    let tables: Vec<&Stall> = rows.tables().collect();
    let mut near: HashMap<(i32, i32), Vec<&Stall>> = HashMap::new();
    let bucket = |p: Vec2| ((p.x / 4.0).floor() as i32, (p.y / 4.0).floor() as i32);
    for s in &tables {
        near.entry(bucket(s.pos.xz())).or_default().push(s);
    }
    let table_at = |p: Vec2| {
        let (ci, cj) = bucket(p);
        (cj - 1..=cj + 1).any(|j| (ci - 1..=ci + 1).any(|i| near.get(&(i, j)).is_some_and(|c| c.iter().any(|s| s.covers(p)))))
    };
    let patches = rows.patches();
    let mut out = Report {
        tables: tables.len(),
        colliders: patches.len(),
        most_in_one: patches.iter().map(Vec::len).max().unwrap_or(0),
        carts: rows.stalls.iter().filter(|s| s.kind == RowKind::Cart).count(),
        scooters: rows.stalls.iter().filter(|s| s.kind == RowKind::Scooters).count(),
        ..default()
    };
    let walled = walls(&rows.stalls);
    out.walls = walled.len();
    out.longest_wall = walled.iter().map(|w| w.1).fold(0.0, f32::max);
    out.too_long = walled.iter().filter(|w| w.1 > WALL + 2.0 * TABLE.x).count();
    // Doors: their ways out are free of tables, and they join the routes.
    for r in &map.rooms {
        for d in &r.doors {
            let out_to = if d.leaves { WALK_LINE } else { SHOP_FRONT };
            let across = d.out.perp();
            let (mut s, mut hit) = (0.0, false);
            while s <= out_to {
                let t = d.width * 0.5;
                hit |= [-t, 0.0, t].iter().any(|t| table_at(d.pos.xz() + d.out * s + across * *t));
                s += 0.25;
            }
            out.on_doors += hit as usize;
        }
        out.doors_shut += (!r.doors.is_empty() && !r.doors.iter().any(|d| floor.reachable(d.pos.xz() + d.out * 1.5))) as usize;
        out.rooms_shut += (!r.ways_in().iter().any(|p| floor.is_indoors(*p))) as usize;
    }
    // Bays: nothing on them, and a straight way for a car to open road.
    for b in &map.parking {
        let at = b.pos.xz();
        let (ci, cj) = bucket(at);
        let close = (cj - 3..=cj + 3).any(|j| (ci - 3..=ci + 3).any(|i| near.get(&(i, j)).is_some_and(|c| c.iter().any(|s| apart(s, &Stall { kind: RowKind::Cart, pos: b.pos, yaw: b.yaw, first: false }) < 1.0))));
        out.on_bays += close as usize;
        let lane_clear = |dir: Vec2| (0..=24).all(|k| (-2..=2).all(|w| !table_at(at + dir * k as f32 * 0.5 + dir.perp() * w as f32 * 0.5)));
        let free = (0..16).any(|k| lane_clear(Vec2::from_angle(k as f32 / 16.0 * std::f32::consts::TAU)));
        out.bays_shut += !free as usize;
    }
    out.on_bridges = tables.iter().filter(|s| on_bridge(s.pos.xz())).count();
    out.on_loot = map.floor_loot.iter().copied().chain(map.chests.iter().map(|c| c.pos)).filter(|p| table_at(p.xz())).count();
    // A wall's two sides are one street.
    for (wall, ..) in &walled {
        let s = stalls_mid(&rows.stalls, wall);
        if !sides_meet(floor, s) {
            out.no_way_round += 1;
            trace(&format!("no way round the wall of {} at {:.1}\n{}", wall.len(), s.pos.xz(), floor.sketch(s.pos.xz() - Vec2::splat(14.0), s.pos.xz() + Vec2::splat(14.0))));
        }
    }
    // The road: a first row leaves `LANE` clear in front of it.
    for s in tables.iter().filter(|s| s.first && road_at(&map.terrain, s.pos.xz())) {
        let f = s.facing();
        let open = (0..=(LANE * 2.0) as i32).all(|k| !table_at(s.pos.xz() + f * (s.half().y + 0.2 + k as f32 * 0.5)));
        out.lanes_narrow += !open as usize;
        if !open {
            trace(&format!("little road before the table at {:.1}\n{}", s.pos.xz(), floor.sketch(s.pos.xz() - Vec2::splat(14.0), s.pos.xz() + Vec2::splat(14.0))));
        }
    }
    out
}

/// With ROWS_TRACE set, what `report` finds wrong, spelt out.
fn trace(what: &str) {
    if std::env::var("ROWS_TRACE").is_ok() {
        println!("{what}");
    }
}

/// The table of a wall nearest its middle.
fn stalls_mid<'a>(stalls: &'a [Stall], wall: &[usize]) -> &'a Stall {
    let mean = wall.iter().map(|k| stalls[*k].pos.xz()).sum::<Vec2>() / wall.len() as f32;
    let far = |k: &&usize| stalls[**k].pos.xz().distance_squared(mean);
    &stalls[*wall.iter().min_by(|a, b| far(a).total_cmp(&far(b))).expect("a wall has tables")]
}

#[cfg(test)]
mod tests {
    use super::super::{MapId, generate};
    use super::*;

    #[test]
    fn a_small_map_has_no_rows() {
        assert!(generate(MapId::Cp, 7).stalls.stalls.is_empty());
    }

    /// The rows stand clear of what must stay open, on the real map, and two builds agree.
    #[test]
    fn the_rows_shut_nothing() {
        let map = generate(MapId::Ncr, 7);
        let floor = StreetFloor::new(&map);
        let r = report(&map, &floor);
        println!("{r:?}");
        assert_eq!(r.failures(), 0, "{r:?}");
        assert!(r.tables > 3000 && r.colliders < 800, "{r:?}");
        let again = generate(MapId::Ncr, 7);
        assert_eq!(map.stalls.stalls.len(), again.stalls.stalls.len());
        assert!(map.stalls.stalls.iter().zip(&again.stalls.stalls).all(|(a, b)| a.pos == b.pos && a.yaw == b.yaw && a.kind == b.kind));
    }

    #[test]
    fn walls_get_a_way_through() {
        let table = |x: f32, z: f32| Stall { kind: RowKind::Table, pos: Vec3::new(x, 0.0, z), yaw: 0.0, first: true };
        // Two rows back to back, 60 m long, a table every 2.75 m.
        let mut stalls: Vec<Stall> = (0..22).flat_map(|k| [table(k as f32 * 2.75, 0.0), table(k as f32 * 2.75, 2.5)]).collect();
        cut_walls(&mut stalls);
        let w = walls(&stalls);
        assert!(w.len() >= 4, "{} walls", w.len());
        assert!(w.iter().all(|w| w.1 <= WALL), "{:?}", w.iter().map(|w| w.1).collect::<Vec<_>>());
        // A wall short of the limit stays whole.
        let mut stalls: Vec<Stall> = (0..4).map(|k| table(k as f32 * 2.75, 0.0)).collect();
        cut_walls(&mut stalls);
        assert_eq!(stalls.len(), 4);
    }

    #[test]
    fn a_table_is_cover_and_goes_into_the_ground() {
        let s = Stall { kind: RowKind::Table, pos: Vec3::new(4.0, 2.0, -3.0), yaw: 0.7, first: true };
        let b = s.block();
        let Shape::Box(size) = b.shape else { panic!("a box") };
        assert!((b.pos.y + size.y / 2.0 - (2.0 + TABLE.y)).abs() < 1e-5, "its top");
        assert!(b.pos.y - size.y / 2.0 < 2.0 - 0.1, "its foot");
        assert!(s.covers(s.pos.xz() + s.facing() * 0.5) && !s.covers(s.pos.xz() + s.facing() * 0.7));
        assert!(s.covers(s.pos.xz() + s.facing().perp() * 1.2) && !s.covers(s.pos.xz() + s.facing().perp() * 1.3));
    }
}
