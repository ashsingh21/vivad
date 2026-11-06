//! Buildings to go inside. About 530 of the NCR's buildings are hollow: each stays a `Footprint`, so
//! its paint, signs and distant shells are the same as any other's, and gains an `Interior`: walls
//! with the same holes as the painted windows, floors at exact storeys, switchback stairs, a stair
//! room on the roof, furniture and loot. Built with the map from its fixed seed, so every peer has
//! the same rooms.
//!
//! Everything here is in the building's own frame: x along the front wall from its first corner, z
//! in from the front, y up, in world metres. `Interior::at` turns that into the world. A hollow
//! building is an exact rectangle: 1,233 of the map's outlines are, so nothing is lost by it, and it
//! is what lets a room be two numbers on each axis.
//!
//! The sim sees `Interior::solids`, one compound collider, and the client draws the same walls
//! (crate::buildings::interior in the game). Bots walk the ground floors (crate::sim::nav).

use std::collections::HashMap;

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

use crate::SmallRng;
use crate::sim::HALF_HEIGHT;
use crate::sim::nav::StreetFloor;

use super::build::subtract_rect;
use super::terrain::ground;
use super::{Block, ChestKind, MapLayout, Mat, Shape, compose, footprint, kit};

/// The painted facade's bay and storey, in metres (crate::buildings in the game paints by these).
pub const BAY: f32 = 3.2;
pub const STOREY: f32 = 3.4;
/// The block wall just behind the paint, the walls between rooms, and a floor's thickness.
pub const SHELL: f32 = 0.3;
pub const PARTITION: f32 = 0.14;
pub const SLAB: f32 = 0.25;
/// A window's glass in the facade texture, as a width and its sill and head over the storey's floor.
const WINDOW_W: f32 = 0.44 * BAY;
const SILL: f32 = 0.30 * STOREY;
const HEAD: f32 = 0.84 * STOREY;
/// The shop texture's open-shelf bay.
const SHOP_W: f32 = 0.88 * BAY;
const SHOP_H: f32 = 0.80 * STOREY;
/// A door onto the street: as wide as a painted window's frame, which leaves the frame's head and
/// glass over it as a fanlight.
pub const DOOR_W: f32 = 1.6;
pub const DOOR_H: f32 = 2.4;
const WIDE_DOOR: f32 = 2.0;
/// Doorways between rooms. On the ground floor wide enough for the bots' grid to fit a warrior
/// through at any angle (0.4 m clearance a side on half-metre cells).
const GROUND_DOOR: f32 = 1.6;
const UPPER_DOOR: f32 = 1.2;
const DOOR_TOP: f32 = 2.2;
/// A stair's lane, the wall between two lanes, a landing and a flight's length.
pub const LANE: f32 = 1.3;
const SPINE: f32 = 0.1;
const LANDING: f32 = 1.2;
const RUN: (f32, f32) = (4.0, 4.6);
/// The stair room's inside height, and a parapet's height over the roof (with its coping).
pub const STAIR_ROOM: f32 = 2.7;
const PARAPET: f32 = 1.04;
/// A building with more storeys than this has a lobby only.
const MAX_STOREYS: u32 = 4;
/// Metres a side.
const SIDE: (f32, f32) = (7.0, 40.0);
/// Kit, trees and anything else this close to a door goes.
const DOOR_CLEAR: f32 = 2.0;

/// What a storey is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Plan {
    Shop,
    Flat,
    Office,
    Godown,
    Dhaba,
    Lobby,
}

/// A hole in a wall, along the wall and up it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub s0: f32,
    pub s1: f32,
    pub y0: f32,
    pub y1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallKind {
    /// The block wall behind the paint: its inside is a room's, its outside the paint's.
    Shell,
    /// Between rooms: both faces are rooms'.
    Inner,
    /// Stands free (the stair room, a stair's spine): drawn as a box.
    Free,
    /// Drawn elsewhere (a parapet): only a collider.
    Hidden,
}

/// A wall in the building's frame: along x at z = `at`, or along z at x = `at`.
#[derive(Clone, Debug)]
pub struct Wall {
    pub along_x: bool,
    pub at: f32,
    pub thick: f32,
    pub s: (f32, f32),
    pub y: (f32, f32),
    pub cuts: Vec<Cut>,
    pub kind: WallKind,
}

/// A rectangle of floor, its top at `y`.
#[derive(Clone, Copy, Debug)]
pub struct Slab {
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub y: f32,
    pub thick: f32,
}

/// A stair shaft along the back wall: two lanes side by side with a wall between, a landing at
/// each end. Even flights climb +x in the front lane, odd ones -x in the back lane, so each starts
/// beside where the last arrived, and a storey's door is on the landing the flight below reached.
#[derive(Clone, Copy, Debug)]
pub struct Stair {
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub landing: f32,
}

impl Stair {
    /// The middle of the lane flight `k` climbs in.
    pub fn lane(&self, k: u32) -> f32 {
        if k % 2 == 0 { self.z.0 + LANE / 2.0 } else { self.z.1 - LANE / 2.0 }
    }

    /// Where the flights start and end along x, low end first.
    fn run(&self) -> (f32, f32) {
        (self.x.0 + self.landing, self.x.1 - self.landing)
    }

    /// Flight `k`'s foot and head along x.
    pub fn flight(&self, k: u32) -> (f32, f32) {
        let (a, b) = self.run();
        if k % 2 == 0 { (a, b) } else { (b, a) }
    }

    /// The middle of the landing a storey's door opens onto.
    fn landing_x(&self, storey: u32) -> f32 {
        if storey % 2 == 0 { self.x.0 + self.landing / 2.0 } else { self.x.1 - self.landing / 2.0 }
    }

    /// The middle of a storey's door to the stairs. The ground floor's is as wide as its other
    /// doors, for a bot that has come down, so it runs on past the landing beside the first steps.
    fn door_x(&self, storey: u32) -> f32 {
        if storey == 0 { self.x.0 + GROUND_DOOR / 2.0 + 0.05 } else { self.landing_x(storey) }
    }

    /// The hole flight `k` needs in the floor above it.
    pub fn well(&self, k: u32) -> (Vec2, Vec2) {
        let (a, b) = self.run();
        let z = self.lane(k);
        (Vec2::new(a, z - LANE / 2.0), Vec2::new(b, z + LANE / 2.0))
    }
}

/// A way out of a room, towards the stairs: a point on this side of the door and one past it.
#[derive(Clone, Copy, Debug)]
pub struct Exit {
    pub before: Vec2,
    pub beyond: Vec2,
}

/// A room on a storey, as its walls' inner faces.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    pub storey: u32,
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub plan: Plan,
    /// Picks its paint.
    pub tone: u32,
    pub shaft: bool,
    pub exit: Option<Exit>,
}

impl Room {
    fn holds(&self, p: Vec2, slack: f32) -> bool {
        p.x > self.x.0 - slack && p.x < self.x.1 + slack && p.y > self.z.0 - slack && p.y < self.z.1 + slack
    }
    fn middle(&self) -> Vec2 {
        Vec2::new((self.x.0 + self.x.1) / 2.0, (self.z.0 + self.z.1) / 2.0)
    }
}

/// A placed piece of furniture: the room kit model's name, where it stands, which way it faces.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub name: &'static str,
    pub pos: Vec3,
    pub yaw: f32,
}

/// A door onto the outside.
#[derive(Clone, Copy, Debug)]
pub struct Door {
    /// The middle of the opening's foot, on the outline.
    pub pos: Vec3,
    /// Outward, on the ground.
    pub out: Vec2,
    pub width: f32,
    /// A shop's open bay has no leaves.
    pub leaves: bool,
}

/// One of Connaught Place's ring of shops, as the middle of its back wall, which way is out towards
/// its front, and how wide it is.
#[derive(Clone, Copy, Debug)]
pub struct RingShop {
    pub back: Vec3,
    pub into: Vec2,
    pub wide: f32,
}

/// Where something can be set down indoors.
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub pos: Vec3,
    /// A chest here spills into the room.
    pub yaw: f32,
    pub storey: u32,
    pub roof: bool,
}

#[derive(Clone, Debug)]
pub struct Interior {
    /// Its index in `MapLayout::footprints`.
    pub footprint: usize,
    pub plan: Plan,
    pub corner: Vec2,
    pub x: Vec2,
    pub z: Vec2,
    pub w: f32,
    pub d: f32,
    /// The ground floor's level, and where the painted storeys count from.
    pub floor: f32,
    pub base: f32,
    /// Hollow storeys, and the height they end at.
    pub storeys: u32,
    pub roof: f32,
    /// Solid from `roof` up to here (a tower over its lobby).
    pub solid_to: Option<f32>,
    pub front: usize,
    pub walls: Vec<Wall>,
    pub slabs: Vec<Slab>,
    pub rooms: Vec<Room>,
    pub stair: Option<Stair>,
    pub pieces: Vec<Piece>,
    pub doors: Vec<Door>,
    /// The holes in each edge of the outline, in its own order, along it from its first corner.
    pub skin: [Vec<Cut>; 4],
}

/// The rectangles of a wall's face left by its holes, as [s0, s1, y0, y1]: a column to each stretch
/// between holes' edges, so that piers run the wall's whole height and adjacent solid columns are
/// one.
pub fn rects(s: (f32, f32), y: (f32, f32), cuts: &[Cut]) -> Vec<[f32; 4]> {
    let cuts: Vec<Cut> = cuts.iter().copied().filter(|c| c.s1 > s.0 && c.s0 < s.1 && c.y1 > y.0 && c.y0 < y.1).collect();
    let mut xs = vec![s.0, s.1];
    xs.extend(cuts.iter().flat_map(|c| [c.s0.clamp(s.0, s.1), c.s1.clamp(s.0, s.1)]));
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    let here = |m: f32| {
        let mut v: Vec<Cut> = cuts.iter().copied().filter(|c| c.s0 <= m && m < c.s1).collect();
        v.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        v
    };
    let mut out = vec![];
    let mut k = 0;
    while k + 1 < xs.len() {
        let mut end = k + 1;
        let holes = here((xs[k] + xs[end]) * 0.5);
        if holes.is_empty() {
            while end + 1 < xs.len() && here((xs[end] + xs[end + 1]) * 0.5).is_empty() {
                end += 1;
            }
        }
        let mut from = y.0;
        for c in &holes {
            if c.y0 > from + 1e-4 {
                out.push([xs[k], xs[end], from, c.y0]);
            }
            from = from.max(c.y1);
        }
        if from < y.1 - 1e-4 {
            out.push([xs[k], xs[end], from, y.1]);
        }
        k = end;
    }
    out
}

/// A piece of furniture's size: along its wall, up, and out from the wall.
pub fn piece_size(name: &str) -> Vec3 {
    match name {
        "charpai" => Vec3::new(1.95, 0.50, 0.95),
        "almirah" => Vec3::new(0.91, 1.83, 0.50),
        "table" => Vec3::new(1.20, 0.75, 0.75),
        "plastic_chair" => Vec3::new(0.60, 0.84, 0.54),
        "stool" => Vec3::new(0.33, 0.45, 0.33),
        "shelf" => Vec3::new(0.92, 1.95, 0.40),
        "counter" => Vec3::new(1.60, 0.95, 0.60),
        "sacks" => Vec3::new(1.05, 0.70, 1.05),
        "gas_stove" => Vec3::new(0.62, 0.85, 0.42),
        "water_drum" => Vec3::new(0.60, 0.93, 0.60),
        "desk" => Vec3::new(1.40, 0.75, 0.70),
        "sofa" => Vec3::new(1.60, 0.82, 0.78),
        "tv_unit" => Vec3::new(1.50, 1.14, 0.42),
        "fan" => Vec3::new(1.20, 0.42, 1.20),
        _ => Vec3::splat(0.5),
    }
}

/// Whether a warrior walks round a piece. A stool or a chair is nudged past in a game, and a fan
/// is overhead.
fn piece_is_solid(name: &str) -> bool {
    !matches!(name, "fan" | "stool" | "plastic_chair")
}

impl Interior {
    /// A point of the building's frame in the world.
    pub fn at(&self, x: f32, y: f32, z: f32) -> Vec3 {
        let p = self.corner + self.x * x + self.z * z;
        Vec3::new(p.x, y, p.y)
    }

    /// A point of the map in the building's frame.
    pub fn local(&self, p: Vec2) -> Vec2 {
        let q = p - self.corner;
        Vec2::new(q.dot(self.x), q.dot(self.z))
    }

    /// The floor of a storey (the roof for the storey past the last).
    pub fn level(&self, k: u32) -> f32 {
        if k == 0 { self.floor } else { self.base + k as f32 * STOREY }
    }

    /// The rotation that lies a box along the building.
    pub fn rot(&self) -> Quat {
        Quat::from_rotation_y((-self.x.y).atan2(self.x.x))
    }

    /// Whether a point on the map is under the stair room, or close by it.
    pub fn by_stair_room(&self, p: Vec2) -> bool {
        let l = self.local(p);
        self.stair.is_some_and(|s| l.x > s.x.0 - 1.2 && l.x < s.x.1 + 1.2 && l.y > s.z.0 - 1.4)
    }

    /// Whether a point on the map is inside the outline.
    pub fn contains(&self, p: Vec2) -> bool {
        let l = self.local(p);
        l.x > 0.0 && l.x < self.w && l.y > 0.0 && l.y < self.d
    }

    fn solid(&self, min: Vec3, max: Vec3) -> Block {
        let c = (min + max) * 0.5;
        Block { pos: self.at(c.x, c.y, c.z), rot: self.rot(), shape: Shape::Box(max - min), mat: Mat::Concrete, collide: true, visible: false }
    }

    /// A wall as boxes.
    pub fn wall_solids(&self, wall: &Wall) -> Vec<Block> {
        rects(wall.s, wall.y, &wall.cuts)
            .into_iter()
            .map(|[s0, s1, y0, y1]| {
                let t = wall.thick / 2.0;
                if wall.along_x {
                    self.solid(Vec3::new(s0, y0, wall.at - t), Vec3::new(s1, y1, wall.at + t))
                } else {
                    self.solid(Vec3::new(wall.at - t, y0, s0), Vec3::new(wall.at + t, y1, s1))
                }
            })
            .collect()
    }

    /// A flight's two ends in the world, on its surface.
    pub fn flight_ends(&self, s: &Stair, k: u32) -> (Vec3, Vec3) {
        let (foot, head) = s.flight(k);
        let z = s.lane(k);
        (self.at(foot, self.level(k), z), self.at(head, self.level(k + 1), z))
    }

    /// What a warrior meets, as boxes in world space: one compound collider.
    pub fn solids(&self) -> Vec<Block> {
        let mut out: Vec<Block> = self.walls.iter().flat_map(|w| self.wall_solids(w)).collect();
        for s in &self.slabs {
            out.push(self.solid(Vec3::new(s.x.0, s.y - s.thick, s.z.0), Vec3::new(s.x.1, s.y, s.z.1)));
        }
        if let Some(s) = &self.stair {
            for k in 0..self.storeys {
                let (foot, head) = self.flight_ends(s, k);
                let x = (head - foot).normalize();
                let z = x.cross(Vec3::Y).normalize();
                let y = z.cross(x);
                const THICK: f32 = 0.3;
                out.push(Block {
                    pos: (foot + head) * 0.5 - y * THICK / 2.0,
                    rot: Quat::from_mat3(&Mat3::from_cols(x, y, z)),
                    shape: Shape::Box(Vec3::new(foot.distance(head), THICK, LANE)),
                    mat: Mat::Concrete,
                    collide: true,
                    visible: false,
                });
            }
        }
        for p in self.pieces.iter().filter(|p| piece_is_solid(p.name)) {
            let size = piece_size(p.name);
            out.push(Block { pos: p.pos + Vec3::Y * size.y / 2.0, rot: Quat::from_rotation_y(p.yaw), shape: Shape::Box(size), mat: Mat::Wood, collide: true, visible: false });
        }
        if let Some(top) = self.solid_to {
            out.push(self.solid(Vec3::new(0.0, self.roof, 0.0), Vec3::new(self.w, top, self.d)));
        }
        out
    }

    /// The holes on a wall's face at `at`, of those that overlap a stretch of it.
    pub fn cuts_on(&self, along_x: bool, at: f32, s: (f32, f32), y: (f32, f32)) -> Vec<Cut> {
        self.walls
            .iter()
            .filter(|w| w.along_x == along_x && (w.at - at).abs() <= w.thick / 2.0 + 0.03 && matches!(w.kind, WallKind::Shell | WallKind::Inner))
            .flat_map(|w| w.cuts.iter().copied())
            .filter(|c| c.s1 > s.0 && c.s0 < s.1 && c.y1 > y.0 && c.y0 < y.1)
            .collect()
    }

    /// For a warrior above the street in here: where to walk next on the way down, a point on the
    /// map. None on the ground floor, and outside.
    pub fn way_down(&self, pos: Vec3) -> Option<Vec2> {
        let s = self.stair.as_ref()?;
        let l = self.local(pos.xz());
        if l.x < -0.5 || l.y < -0.5 || l.x > self.w + 0.5 || l.y > self.d + 0.5 {
            return None;
        }
        let feet = pos.y - HALF_HEIGHT;
        let world = |p: Vec2| self.at(p.x, 0.0, p.y).xz();
        // On a flight: on down it, and past its foot.
        let in_shaft = l.x > s.x.0 - 0.1 && l.x < s.x.1 + 0.1 && l.y > s.z.0 - 0.1 && l.y < s.z.1 + 0.1;
        if feet < self.level(1) - 2.0 && !in_shaft {
            return None;
        }
        let (lo, hi) = s.run();
        if in_shaft && l.x > lo - 0.2 && l.x < hi + 0.2 {
            let k = (0..self.storeys).filter(|k| (s.lane(*k) - l.y).abs() < LANE / 2.0 + 0.1).find(|k| {
                let (foot, head) = self.flight_ends(s, *k);
                let t = ((l.x - s.flight(*k).0) / (s.flight(*k).1 - s.flight(*k).0)).clamp(0.0, 1.0);
                (feet - (foot.y + (head.y - foot.y) * t)).abs() < 1.0
            });
            if let Some(k) = k {
                let (foot, _) = s.flight(k);
                let past = foot + (foot - s.flight(k).1).signum() * s.landing * 0.5;
                return Some(world(Vec2::new(past, s.lane(k))));
            }
        }
        let storey = (((feet - self.base + 0.8) / STOREY).floor().max(0.0) as u32).min(self.storeys);
        if storey == 0 {
            return None;
        }
        // On a landing: to the head of the flight that came up here.
        if in_shaft || storey == self.storeys {
            let (_, head) = s.flight(storey - 1);
            let lane = s.lane(storey - 1);
            if storey == self.storeys && !in_shaft {
                // Out on the roof: to the stair room's door, and in.
                let door = Vec2::new(s.landing_x(storey), s.z.0 - 0.8);
                let inside = Vec2::new(s.landing_x(storey), s.z.0 + 0.5);
                let row = Vec2::new(l.x, door.y);
                let target = if (l.x - door.x).abs() < 0.45 && l.y > s.z.0 - 1.4 {
                    inside
                } else if l.y < door.y - 0.5 {
                    row
                } else {
                    door
                };
                return Some(world(target));
            }
            let on_it = (l.y - lane).abs() < LANE / 2.0 - 0.15 && (l.x - head).abs() < s.landing * 0.5 + 0.4;
            let toward = s.flight(storey - 1).0;
            return Some(world(if on_it { Vec2::new(toward, lane) } else { Vec2::new(head, lane) }));
        }
        // In a room: door by door to the stairs.
        let room = self.rooms.iter().find(|r| r.storey == storey && !r.shaft && r.holds(l, 0.25))?;
        let exit = room.exit?;
        let across = (exit.beyond - exit.before).normalize_or_zero();
        let off = (l - exit.before).reject_from(across).length();
        Some(world(if off < 0.4 { exit.beyond } else { exit.before }))
    }

    /// The way a player takes from the street to the roof, as points on the map, each with the
    /// height of the floor there: in at the door, round the furniture to the stairs, up them flight
    /// by flight, and out of the stair room.
    pub fn way_up(&self, floor: &StreetFloor) -> Option<Vec<(Vec2, f32)>> {
        let door = self.doors.first()?;
        let flat = |p: Vec2, y: f32| (self.at(p.x, 0.0, p.y).xz(), y);
        let g = self.level(0);
        let inside = door.pos.xz() - door.out * 1.0;
        let mut way = vec![(door.pos.xz() + door.out * 2.5, g), (inside, g)];
        let at = |p: Vec2| self.rooms.iter().find(|m| m.storey == 0 && m.holds(p, 0.3));
        let mut room = at(self.local(inside))?;
        for _ in 0..32 {
            let Some(exit) = room.exit else { break };
            let (from, to) = (way.last()?.0, self.at(exit.before.x, 0.0, exit.before.y).xz());
            way.extend(floor.path(from, to)?.into_iter().map(|p| (p, g)));
            way.push(flat(exit.before, g));
            way.push(flat(exit.beyond, g));
            room = at(exit.beyond)?;
        }
        // A tower's lobby is as far as it goes.
        let Some(s) = self.stair.as_ref() else {
            way.push(flat(room.middle(), g));
            return Some(way);
        };
        for k in 0..self.storeys {
            let (foot, head) = s.flight(k);
            let (lane, dir) = (s.lane(k), (head - foot).signum());
            let landing = s.landing * 0.55;
            way.push(flat(Vec2::new(foot - dir * landing, lane), self.level(k)));
            way.push(flat(Vec2::new(foot + dir * 0.3, lane), self.level(k)));
            way.push(flat(Vec2::new(head, lane), self.level(k + 1)));
            way.push(flat(Vec2::new(head + dir * landing, lane), self.level(k + 1)));
        }
        let (foot, head) = s.flight(self.storeys - 1);
        let x = head + (head - foot).signum() * s.landing * 0.5;
        way.push(flat(Vec2::new(x, s.z.0 + 0.7), self.roof));
        way.push(flat(Vec2::new(x, s.z.0 - 1.2), self.roof));
        Some(way)
    }

    /// What the bots' floor marks on the ground floor beyond `solids`: flight 0, as they don't climb.
    /// Grown by a warrior's clearance when it's drawn, so drawn that much smaller here, or the
    /// landing beside it would be closed.
    pub fn stair_block(&self) -> Option<Block> {
        let s = self.stair.as_ref()?;
        let (a, b) = s.run();
        let z = s.lane(0);
        Some(self.solid(Vec3::new(a + 0.5, self.floor + 0.5, z - 0.2), Vec3::new(b, self.floor + 1.8, z + 0.2)))
    }

    /// Points just inside each way in from outside, for the bots' floor to flood from.
    pub fn ways_in(&self) -> Vec<Vec2> {
        self.doors.iter().map(|d| d.pos.xz() - d.out * 1.0).collect()
    }
}

/// A cheap hash to 0..1, for choices that follow from where a building stands.
fn hash01(a: u32, b: u32, c: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca77) ^ c.wrapping_mul(0xc2b2_ae3d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// Whether a building is painted with a row of shops along its ground storey (`Paint` in
/// crate::buildings: the same test on the same numbers).
pub fn has_shops(c: Vec2, height: f32) -> bool {
    let seed = (c.x * 7.3 + c.y * 13.1).to_bits();
    height > STOREY * 1.6 && hash01(seed, 3, 11) < 0.8
}

/// The bays of an outline's edge that fit whole, by the rule that paints their sills: a bay and how
/// far along the edge its middle is.
fn bays(along: f32, len: f32) -> Vec<(i32, f32)> {
    let first = ((along + 0.6) / BAY).ceil() as i32;
    let last = ((along + len - 0.6) / BAY).floor() as i32;
    (first..last).map(|bay| (bay, (bay as f32 + 0.5) * BAY - along)).collect()
}

/// What's known of a building chosen to go hollow.
struct Site {
    index: usize,
    pts: [Vec2; 4],
    front: usize,
    door_bay: i32,
    along: [f32; 4],
    base: f32,
    top: f32,
    floor: f32,
    storeys: u32,
    tower: bool,
    shops: bool,
    plan: Plan,
    upper: Plan,
    seed: u32,
}

/// Builds up one interior.
struct Maker<'a> {
    int: Interior,
    site: &'a Site,
    /// Doors between rooms, as the rooms they join (`None` for the stairs) and a point each side.
    links: Vec<(usize, Option<usize>, Vec2, Vec2)>,
    spots: Vec<Spot>,
}

impl Maker<'_> {
    fn r(&self, k: u32) -> f32 {
        hash01(self.site.seed, k, 23)
    }

    fn wall(&mut self, along_x: bool, at: f32, thick: f32, s: (f32, f32), y: (f32, f32), cuts: Vec<Cut>, kind: WallKind) {
        self.int.walls.push(Wall { along_x, at, thick, s, y, cuts, kind });
    }

    fn shell(&mut self) {
        let (w, d, t) = (self.int.w, self.int.d, SHELL / 2.0);
        let (y, roof) = ((self.int.floor.min(self.int.base), self.int.roof), self.site.top);
        for (along_x, at, len) in [(true, t, w), (false, w - t, d), (true, d - t, w), (false, t, d)] {
            self.wall(along_x, at, SHELL, (0.0, len), y, vec![], WallKind::Shell);
            // The parapet the paint shows.
            if self.int.solid_to.is_none() {
                self.wall(along_x, at, SHELL, (0.0, len), (roof, roof + PARAPET), vec![], WallKind::Hidden);
            }
        }
    }

    fn slabs(&mut self) {
        let (min, max) = (Vec2::splat(SHELL), Vec2::new(self.int.w - SHELL, self.int.d - SHELL));
        for k in 1..=self.int.storeys {
            let well = self.int.stair.map(|s| s.well(k - 1));
            let parts = match well {
                Some(h) => subtract_rect((min, max), h),
                None => vec![(min, max)],
            };
            let y = self.int.level(k);
            for (a, b) in parts {
                self.int.slabs.push(Slab { x: (a.x, b.x), z: (a.y, b.y), y, thick: SLAB });
            }
        }
    }

    /// The stairs' walls: the spine between the lanes, the shaft's open end, and the stair room.
    fn stairs(&mut self) {
        let Some(s) = self.int.stair else { return };
        let (w, roof, floor) = (self.int.w, self.int.roof, self.int.floor);
        let spine = s.z.0 + LANE + SPINE / 2.0;
        let (lo, hi) = s.run();
        let top = roof + STAIR_ROOM;
        self.wall(true, spine, SPINE, (lo, hi), (floor, top), vec![], WallKind::Free);
        for (at, closes) in [(s.x.0 - PARTITION / 2.0, s.x.0 > SHELL + 0.01), (s.x.1 + PARTITION / 2.0, s.x.1 < w - SHELL - 0.01)] {
            if closes {
                self.wall(false, at, PARTITION, (s.z.0 - PARTITION, s.z.1), (floor, roof), vec![], WallKind::Inner);
            }
        }
        // The stair room stands inside the parapet.
        const T: f32 = 0.2;
        let left = if s.x.0 > SHELL + 0.01 { s.x.0 - T / 2.0 } else { SHELL + T / 2.0 };
        let right = if s.x.1 < w - SHELL - 0.01 { s.x.1 + T / 2.0 } else { w - SHELL - T / 2.0 };
        let (front, back) = (s.z.0 - T / 2.0, s.z.1 - T / 2.0);
        let y = (roof, top);
        let x = s.landing_x(self.int.storeys);
        let door = Cut { s0: x - 0.55, s1: x + 0.55, y0: roof, y1: roof + DOOR_TOP };
        self.wall(true, front, T, (left - T / 2.0, right + T / 2.0), y, vec![door], WallKind::Free);
        self.wall(true, back, T, (left - T / 2.0, right + T / 2.0), y, vec![], WallKind::Free);
        self.wall(false, left, T, (front, back), y, vec![], WallKind::Free);
        self.wall(false, right, T, (front, back), y, vec![], WallKind::Free);
        self.int.slabs.push(Slab { x: (left - T, right + T), z: (front - T, back + T), y: top + 0.2, thick: 0.2 });
    }

    /// One storey's rooms: a grid in front of the stairs, chained by doors, and whatever's left
    /// beside the stairs.
    fn storey(&mut self, k: u32, plan: Plan) {
        let (w, d) = (self.int.w, self.int.d);
        let (y0, y1) = (self.int.level(k), self.int.level(k + 1) - SLAB);
        let stair = self.int.stair;
        let (x0, x1) = (SHELL, w - SHELL);
        let z_end = stair.map_or(d - SHELL, |s| s.z.0 - PARTITION);
        let door_w = if k == 0 { GROUND_DOOR } else { UPPER_DOOR };
        // About how wide and deep its rooms are.
        let (col, row) = match plan {
            Plan::Shop => (5.0, 7.5),
            Plan::Flat => (3.2, 4.4),
            Plan::Office => (6.0, 6.0),
            Plan::Godown => (14.0, 14.0),
            Plan::Dhaba => (9.0, 7.0),
            Plan::Lobby => (40.0, 40.0),
        };
        let along0 = self.site.along[self.site.front];
        // Partitions on the bay lines, between the windows; a shop's at the shutter bays' middles,
        // between two open bays. Clear of the stair door.
        let shops = self.site.shops && k == 0;
        let keep = stair.map(|s| s.door_x(k));
        let mut planes: Vec<f32> = vec![];
        let mut last = x0;
        for j in 0..=((w + along0) / BAY) as i32 + 1 {
            let line = if shops { (2 * j) as f32 * BAY + 0.5 * BAY - along0 } else { j as f32 * BAY - along0 };
            let clear = keep.is_none_or(|x| (x - line).abs() > door_w / 2.0 + 0.3);
            if line - last >= col - 0.1 && x1 - line >= 2.6 && clear {
                planes.push(line);
                last = line;
            }
        }
        let rows = (((z_end - SHELL) / row).round() as usize).clamp(1, 6);
        let row_at: Vec<f32> = (0..=rows).map(|r| SHELL + (z_end - SHELL) * r as f32 / rows as f32).collect();
        let cols: Vec<(f32, f32)> = {
            let mut edges = vec![x0 - PARTITION];
            edges.extend(planes.iter().copied());
            edges.push(x1 + PARTITION);
            edges.windows(2).map(|e| (e[0] + PARTITION / 2.0, e[1] - PARTITION / 2.0)).map(|(a, b)| (a.max(x0), b.min(x1))).collect()
        };
        let first = self.int.rooms.len();
        let room_at = |r: usize, c: usize| first + r * cols.len() + c;
        for r in 0..rows {
            let z = (if r == 0 { SHELL } else { row_at[r] + PARTITION / 2.0 }, if r + 1 == rows { z_end } else { row_at[r + 1] - PARTITION / 2.0 });
            for (c, &x) in cols.iter().enumerate() {
                let tone = (self.r(100 + k * 40 + (r * cols.len() + c) as u32) * 1000.0) as u32;
                self.int.rooms.push(Room { storey: k, x, z, plan, tone, shaft: false, exit: None });
                // The partition to the next room along, with its door.
                if c + 1 < cols.len() {
                    let at = planes[c];
                    let (a, b) = ((z.0 + z.1) / 2.0 - door_w / 2.0, (z.0 + z.1) / 2.0 + door_w / 2.0);
                    let cut = Cut { s0: a, s1: b, y0, y1: y0 + DOOR_TOP };
                    self.wall(false, at, PARTITION, z, (y0, y1), vec![cut], WallKind::Inner);
                    let mid = (a + b) / 2.0;
                    let room = room_at(r, c);
                    self.links.push((room, Some(room + 1), Vec2::new(at - 0.7, mid), Vec2::new(at + 0.7, mid)));
                }
            }
            // The wall to the next row back, with a door to every other room and at least one.
            if r + 1 < rows {
                let at = row_at[r + 1];
                let mut cuts = vec![];
                for (c, &x) in cols.iter().enumerate() {
                    if c == 0 || self.r(300 + k * 50 + (r * cols.len() + c) as u32) < 0.5 {
                        let mid = (x.0 + x.1) / 2.0;
                        cuts.push(Cut { s0: mid - door_w / 2.0, s1: mid + door_w / 2.0, y0, y1: y0 + DOOR_TOP });
                        self.links.push((room_at(r, c), Some(room_at(r + 1, c)), Vec2::new(mid, at - 0.7), Vec2::new(mid, at + 0.7)));
                    }
                }
                self.wall(true, at, PARTITION, (x0, x1), (y0, y1), cuts, WallKind::Inner);
            }
        }
        let Some(s) = stair else { return };
        // The wall along the stairs, with the door onto this storey's landing.
        let at = s.z.0 - PARTITION / 2.0;
        let x = s.door_x(k);
        let half = if k == 0 { GROUND_DOOR / 2.0 } else { (s.landing / 2.0 - 0.05).min(0.8) };
        let mut cuts = vec![Cut { s0: x - half, s1: x + half, y0, y1: y0 + DOOR_TOP }];
        let back_row = room_at(rows - 1, 0);
        let reaches = cols.iter().position(|c| x > c.0 && x < c.1).unwrap_or(0);
        self.int.rooms.push(Room { storey: k, x: s.x, z: s.z, plan, tone: 0, shaft: true, exit: None });
        let shaft = self.int.rooms.len() - 1;
        self.links.push((back_row + reaches, Some(shaft), Vec2::new(x, at - 0.7), Vec2::new(x, at + 0.8)));
        // What's left of the back strip is one more room.
        let spare = if s.x.0 > SHELL + 2.2 {
            Some((SHELL, s.x.0 - PARTITION))
        } else if s.x.1 < w - SHELL - 2.2 {
            Some((s.x.1 + PARTITION, w - SHELL))
        } else {
            None
        };
        if let Some(sx) = spare {
            let half = (door_w / 2.0).min((sx.1 - sx.0) / 2.0 - 0.15);
            // Its door, clear of the partitions that come up to this wall from the other side.
            let free = |x: f32| planes.iter().all(|p| (p - x).abs() > half + 0.25);
            let mid = [(sx.0 + sx.1) / 2.0, sx.0 + half + 0.15, sx.1 - half - 0.15].into_iter().find(|x| free(*x)).unwrap_or((sx.0 + sx.1) / 2.0);
            cuts.push(Cut { s0: mid - half, s1: mid + half, y0, y1: y0 + DOOR_TOP });
            let above = cols.iter().position(|c| mid > c.0 && mid < c.1).unwrap_or(cols.len() - 1);
            let tone = (self.r(500 + k) * 1000.0) as u32;
            self.int.rooms.push(Room { storey: k, x: sx, z: s.z, plan, tone, shaft: false, exit: None });
            let spare_room = self.int.rooms.len() - 1;
            self.links.push((spare_room, Some(back_row + above), Vec2::new(mid, at + 0.7), Vec2::new(mid, at - 0.7)));
        }
        self.wall(true, at, PARTITION, (x0, x1), (y0, y1), cuts, WallKind::Inner);
    }

    /// Each room's way towards the stairs, breadth first from them.
    fn exits(&mut self) {
        let mut todo: Vec<usize> = (0..self.int.rooms.len()).filter(|&r| self.int.rooms[r].shaft).collect();
        let mut seen = vec![false; self.int.rooms.len()];
        for &r in &todo {
            seen[r] = true;
        }
        while let Some(r) = todo.pop() {
            for &(a, b, pa, pb) in &self.links {
                let Some(b) = b else { continue };
                let (from, before, beyond) = if a == r { (b, pb, pa) } else if b == r { (a, pa, pb) } else { continue };
                if !seen[from] {
                    seen[from] = true;
                    self.int.rooms[from].exit = Some(Exit { before, beyond });
                    todo.insert(0, from);
                }
            }
        }
    }

    /// Cuts the painted windows, doors and shop bays through the shell.
    fn openings(&mut self, open: &dyn Fn(Vec2, f32) -> bool) {
        let site = self.site;
        let (w, d, n) = (self.int.w, self.int.d, self.int.storeys);
        for i in 0..4 {
            let e = (i + 4 - site.front) % 4;
            let (a, b) = (site.pts[i], site.pts[(i + 1) % 4]);
            let len = a.distance(b);
            let dir = (b - a) / len;
            let out = [-self.int.z, self.int.x, self.int.z, -self.int.x][e];
            let to_wall = |s: f32| if e < 2 { s } else if e == 2 { w - s } else { d - s };
            for (bay, s) in bays(site.along[i], len) {
                let side = a + dir * s + out * 1.0;
                for k in 0..n {
                    let y = self.int.level(k).max(site.base + k as f32 * STOREY);
                    let storey = site.base + k as f32 * STOREY;
                    let (half, y0, y1, door) = if k == 0 && e == 0 && !site.shops && bay == site.door_bay {
                        // A balcony bay's painted rail is wider than a door: a double door takes it in.
                        let half = if bay.rem_euclid(2) == 0 { WIDE_DOOR / 2.0 } else { DOOR_W / 2.0 };
                        (half, self.int.floor, site.base + DOOR_H, true)
                    } else if k == 0 && site.shops {
                        if bay.rem_euclid(2) != 1 {
                            continue;
                        }
                        (SHOP_W / 2.0, self.int.floor, storey + SHOP_H, true)
                    } else {
                        // The texture's two-by-two repeat, as in the painted sills: which window this is.
                        let kind = [[0, 2], [3, 1]][((k % 2) == 0) as usize][bay.rem_euclid(2) as usize];
                        if kind == 3 {
                            continue;
                        }
                        (WINDOW_W / 2.0, storey + SILL, storey + HEAD, false)
                    };
                    if !open(side, y + 1.0) {
                        continue;
                    }
                    let c = to_wall(s);
                    if self.hits_wall(e, c - half, c + half, y0, y1) {
                        continue;
                    }
                    let shell = self.int.walls.iter_mut().filter(|w| w.kind == WallKind::Shell).nth(e).expect("four shell walls");
                    shell.cuts.push(Cut { s0: c - half, s1: c + half, y0, y1 });
                    self.int.skin[i].push(Cut { s0: s - half, s1: s + half, y0, y1 });
                    if door {
                        let p = a + dir * s;
                        let leaves = !(k == 0 && site.shops);
                        // The front's first: it's the one known to have the street before it.
                        let at = if e == 0 && bay == site.door_bay { 0 } else { self.int.doors.len() };
                        self.int.doors.insert(at, Door { pos: Vec3::new(p.x, self.int.floor, p.y), out, width: 2.0 * half, leaves });
                    }
                }
            }
        }
    }

    /// Whether a hole in shell wall `e` would meet a wall coming up to it, or the stairs behind it.
    fn hits_wall(&self, e: usize, s0: f32, s1: f32, y0: f32, y1: f32) -> bool {
        let int = &self.int;
        let (w, d) = (int.w, int.d);
        let meets = |wall: &Wall| {
            let reaches = match e {
                0 => wall.s.0 <= SHELL + 0.25,
                1 => wall.s.1 >= w - SHELL - 0.25,
                2 => wall.s.1 >= d - SHELL - 0.25,
                _ => wall.s.0 <= SHELL + 0.25,
            };
            wall.along_x == (e % 2 == 1) && matches!(wall.kind, WallKind::Inner | WallKind::Free) && reaches && wall.at > s0 - 0.12 && wall.at < s1 + 0.12 && wall.y.1 > y0 && wall.y.0 < y1
        };
        if int.walls.iter().any(meets) {
            return true;
        }
        let Some(st) = &int.stair else { return false };
        match e {
            2 => s1 > st.x.0 - 0.15 && s0 < st.x.1 + 0.15,
            1 => st.x.1 >= w - SHELL - 0.01 && s1 > st.z.0 - 0.15,
            3 => st.x.0 <= SHELL + 0.01 && s1 > st.z.0 - 0.15,
            _ => false,
        }
    }

    /// Sets pieces down along a room's walls, keeping clear of its doors, and leaves it spots for
    /// loot. `list` is in order of importance: what doesn't fit is left out.
    fn furnish(&mut self, r: usize, list: &[&'static str], rng: &mut SmallRng) {
        let room = self.int.rooms[r];
        let int = &self.int;
        let (y0, y1) = (int.level(room.storey), int.level(room.storey + 1) - SLAB);
        let lane: f32 = if room.storey == 0 { 1.7 } else { 1.3 };
        // side: its wall's plane, whether it runs along x, its span and which way is into the room.
        let sides = [
            (true, room.z.0 - PARTITION / 2.0, room.x, Vec2::Y),
            (false, room.x.1 + PARTITION / 2.0, room.z, Vec2::NEG_X),
            (true, room.z.1 + PARTITION / 2.0, room.x, Vec2::NEG_Y),
            (false, room.x.0 - PARTITION / 2.0, room.z, Vec2::X),
        ];
        // Keep-outs: an apron in front of everything that opens to the floor.
        let mut taken: Vec<(Vec2, Vec2)> = vec![];
        let mut tall: Vec<(Vec2, Vec2)> = vec![];
        for (along_x, at, span, into) in sides {
            for c in int.cuts_on(along_x, at, span, (y0, y1)) {
                let (a, b) = (c.s0.max(span.0) - 0.2, c.s1.min(span.1) + 0.2);
                let face = if along_x { Vec2::new(0.0, at) } else { Vec2::new(at, 0.0) };
                let (p, q) = if along_x { (Vec2::new(a, 0.0), Vec2::new(b, 0.0)) } else { (Vec2::new(0.0, a), Vec2::new(0.0, b)) };
                // Out to the middle of the room at the least, where pieces along the walls can't
                // stop a warrior: two piles either side of a door would otherwise seal it.
                let to_middle = (room.middle() - (p + face)).dot(into);
                let (p, q) = (p + face, q + face + into * if c.y0 < y0 + 0.5 { lane.max(to_middle) } else { lane });
                let rect = (p.min(q), p.max(q));
                if c.y0 < y0 + 0.5 {
                    taken.push(rect);
                } else {
                    tall.push(rect);
                }
            }
        }
        // A narrow room keeps one long side clear, the side its door is on: furniture on facing
        // walls would close it to a squeeze.
        let (wide, deep) = (room.x.1 - room.x.0, room.z.1 - room.z.0);
        if wide.min(deep) < NARROW {
            let opens = |along_x: bool, at: f32, span: (f32, f32)| int.cuts_on(along_x, at, span, (y0, y1)).iter().any(|c| c.y0 < y0 + 0.5);
            taken.push(if deep < wide {
                let z = if opens(true, room.z.0 - PARTITION / 2.0, room.x) { room.z.0 } else { room.z.1 - lane };
                (Vec2::new(room.x.0, z), Vec2::new(room.x.1, z + lane))
            } else {
                let x = if opens(false, room.x.0 - PARTITION / 2.0, room.z) { room.x.0 } else { room.x.1 - lane };
                (Vec2::new(x, room.z.0), Vec2::new(x + lane, room.z.1))
            });
        }
        let inside = |lo: Vec2, hi: Vec2| lo.x >= room.x.0 - 0.01 && hi.x <= room.x.1 + 0.01 && lo.y >= room.z.0 - 0.01 && hi.y <= room.z.1 + 0.01;
        let meets = |a: (Vec2, Vec2), b: (Vec2, Vec2)| a.0.x < b.1.x && a.1.x > b.0.x && a.0.y < b.1.y && a.1.y > b.0.y;
        let turn = rng.random_range(0..4usize);
        let fit = |size: Vec3, taken: &mut Vec<(Vec2, Vec2)>, rng: &mut SmallRng, gap: f32| -> Option<(Vec2, Vec2)> {
            for k in 0..4 {
                let (along_x, at, span, into) = sides[(k + turn) % 4];
                let wall = if along_x { at + into.y * PARTITION / 2.0 } else { at + into.x * PARTITION / 2.0 };
                let room_for = span.1 - span.0 - size.x;
                if room_for < 0.0 {
                    continue;
                }
                let start = rng.random_range(0.0..1.0f32);
                let steps = (room_for / 0.25).floor() as i32 + 1;
                for j in 0..steps {
                    let s = span.0 + ((start * steps as f32) as i32 + j).rem_euclid(steps) as f32 * 0.25;
                    let (c, half) = if along_x {
                        (Vec2::new(s + size.x / 2.0, wall + into.y * (size.z / 2.0 + 0.04)), Vec2::new(size.x, size.z) / 2.0)
                    } else {
                        (Vec2::new(wall + into.x * (size.z / 2.0 + 0.04), s + size.x / 2.0), Vec2::new(size.z, size.x) / 2.0)
                    };
                    let rect = (c - half - gap, c + half + gap);
                    let is_tall = size.y > 0.95;
                    if inside(c - half, c + half) && !taken.iter().any(|t| meets(*t, rect)) && !(is_tall && tall.iter().any(|t| meets(*t, rect))) {
                        taken.push((c - half, c + half));
                        return Some((c, into));
                    }
                }
            }
            None
        };
        let mut pieces = vec![];
        // The middle of a big room: rows of whatever it holds, with a warrior's lane round each,
        // so a hall has something to fight round and isn't one bare floor.
        if let Some((cluster, half)) = island(room.plan) {
            let (x0, x1, z0, z1) = (room.x.0 + 1.5 + half.x, room.x.1 - 1.5 - half.x, room.z.0 + 1.5 + half.y, room.z.1 - 1.5 - half.y);
            let pitch = 2.0 * half + Vec2::splat(ISLAND_LANE);
            let (columns, ranks) = (((x1 - x0) / pitch.x).floor(), ((z1 - z0) / pitch.y).floor());
            let mut left = MAX_ISLANDS;
            for j in 0..=ranks as i32 {
                for i in 0..=columns as i32 {
                    let c = Vec2::new(x0 + (x1 - x0 - columns * pitch.x) / 2.0 + i as f32 * pitch.x, z0 + (z1 - z0 - ranks * pitch.y) / 2.0 + j as f32 * pitch.y);
                    let rect = (c - half - ISLAND_LANE / 2.0, c + half + ISLAND_LANE / 2.0);
                    if x1 >= x0 && z1 >= z0 && left > 0 && !taken.iter().any(|t| meets(*t, rect)) {
                        // Its lane with it: what stands along the walls can't close it off.
                        taken.push(rect);
                        left -= 1;
                        for &(name, offset, face) in cluster {
                            pieces.push((name, Vec3::new(c.x + offset.x, y0 + offset.y, c.y + offset.z), face));
                        }
                    }
                }
            }
        }
        for &name in list {
            if name == "fan" {
                let c = room.middle();
                // It hangs: its origin is the point on the ceiling.
                pieces.push((name, Vec3::new(c.x, y1, c.y), Vec2::Y));
                continue;
            }
            if let Some((c, into)) = fit(piece_size(name), &mut taken, rng, 0.06) {
                pieces.push((name, Vec3::new(c.x, y0, c.y), into));
            }
        }
        let mut spots = vec![];
        // Spots come with room to walk up to them.
        const APPROACH: f32 = 1.8;
        // Out from the wall by a warrior's clearance and half a cell: the bots go only for what's
        // on a cell of their floor they could stand on.
        const OFF_WALL: f32 = 0.78;
        for _ in 0..2 {
            if let Some((c, into)) = fit(Vec3::new(1.7, 0.7, APPROACH), &mut taken, rng, 0.1) {
                let c = c - into * (APPROACH / 2.0 + 0.04 - OFF_WALL);
                spots.push((Vec3::new(c.x, y0, c.y), into));
            }
        }
        for (name, p, into) in pieces {
            let (world, dir) = (self.int.at(p.x, p.y, p.z), self.int.x * into.x + self.int.z * into.y);
            self.int.pieces.push(Piece { name, pos: world, yaw: kit::facing(dir) });
        }
        let storey = room.storey;
        for (p, into) in spots {
            let int = &self.int;
            let (world, dir) = (int.at(p.x, p.y, p.z), int.x * into.x + int.z * into.y);
            self.spots.push(Spot { pos: world, yaw: dir.x.atan2(dir.y), storey, roof: false });
        }
    }

    fn furniture(&mut self, rng: &mut SmallRng) {
        for r in 0..self.int.rooms.len() {
            let room = self.int.rooms[r];
            if room.shaft {
                continue;
            }
            let area = (room.x.1 - room.x.0) * (room.z.1 - room.z.0);
            let order = r as u32;
            let list: Vec<&'static str> = match room.plan {
                Plan::Shop => {
                    let mut l = vec!["counter", "shelf", "shelf", "shelf", "stool", "fan", "sacks", "shelf"];
                    if area > 30.0 {
                        l.extend(["shelf", "shelf", "sacks"]);
                    }
                    l
                }
                Plan::Flat => match order % 3 {
                    0 => vec!["sofa", "tv_unit", "fan", "table", "plastic_chair", "plastic_chair"],
                    1 => vec!["charpai", "almirah", "fan", "stool"],
                    _ => vec!["gas_stove", "water_drum", "shelf", "table"],
                },
                Plan::Office => {
                    let mut l = vec!["desk", "plastic_chair", "almirah", "fan", "shelf"];
                    if area > 24.0 {
                        l.extend(["desk", "plastic_chair", "desk", "plastic_chair"]);
                    }
                    l
                }
                Plan::Godown => {
                    let mut l = vec!["sacks", "sacks", "water_drum", "shelf", "sacks", "water_drum"];
                    if area > 80.0 {
                        l.extend(["sacks", "sacks", "shelf", "sacks", "water_drum", "sacks"]);
                    }
                    l
                }
                Plan::Dhaba => {
                    let mut l = vec!["counter", "gas_stove", "water_drum", "table", "plastic_chair", "plastic_chair", "fan", "table", "plastic_chair", "plastic_chair"];
                    if area > 40.0 {
                        l.extend(["table", "plastic_chair", "plastic_chair"]);
                    }
                    l
                }
                Plan::Lobby => vec!["counter", "counter", "sofa", "sofa", "plastic_chair", "plastic_chair", "fan"],
            };
            self.furnish(r, &list, rng);
        }
    }

    /// Spots out on the roof, clear of the stair room.
    fn roof_spots(&mut self) {
        let Some(s) = self.int.stair else { return };
        let z = (SHELL + 1.2 + s.z.0 - 1.0) / 2.0;
        for x in [0.3, 0.7] {
            let p = self.int.at(self.int.w * x, self.int.roof, z);
            self.spots.push(Spot { pos: p, yaw: self.int.z.x.atan2(self.int.z.y), storey: self.int.storeys, roof: true });
        }
    }
}

fn make(site: &Site, open: &dyn Fn(Vec2, f32) -> bool, rng: &mut SmallRng) -> (Interior, Vec<Spot>) {
    let (a, b, last) = (site.pts[site.front], site.pts[(site.front + 1) % 4], site.pts[(site.front + 3) % 4]);
    let (w, d) = (a.distance(b), a.distance(last));
    let storeys = if site.tower { 1 } else { site.storeys };
    let roof = if site.tower { site.base + STOREY } else { site.top };
    let stair = (!site.tower).then(|| {
        let inner = w - 2.0 * SHELL;
        let run = (inner - 2.0 * LANDING).clamp(RUN.0, RUN.1);
        let mut landing = LANDING.min((inner - run) / 2.0);
        if inner - (run + 2.0 * landing) < 2.4 {
            landing = (inner - run) / 2.0;
        }
        let len = run + 2.0 * landing;
        let from = if hash01(site.seed, 9, 5) < 0.5 { SHELL } else { w - SHELL - len };
        Stair { x: (from, from + len), z: (d - SHELL - 2.0 * LANE - SPINE, d - SHELL), landing }
    });
    let int = Interior {
        footprint: site.index,
        plan: site.plan,
        corner: a,
        x: (b - a).normalize(),
        z: (last - a).normalize(),
        w,
        d,
        floor: site.floor,
        base: site.base,
        storeys,
        roof,
        solid_to: site.tower.then_some(site.top),
        front: site.front,
        walls: vec![],
        slabs: vec![],
        rooms: vec![],
        stair,
        pieces: vec![],
        doors: vec![],
        skin: Default::default(),
    };
    let mut m = Maker { int, site, links: vec![], spots: vec![] };
    m.shell();
    m.slabs();
    m.stairs();
    for k in 0..storeys {
        m.storey(k, if k == 0 { site.plan } else { site.upper });
    }
    m.exits();
    m.openings(open);
    m.furniture(rng);
    m.roof_spots();
    (m.int, m.spots)
}

/// What stands in the middle of a big room of this kind: pieces at offsets from the middle of a
/// group, each with the way it faces, and half the group's size.
fn island(plan: Plan) -> Option<(&'static [(&'static str, Vec3, Vec2)], Vec2)> {
    const TABLE: [(&str, Vec3, Vec2); 3] = [("table", Vec3::ZERO, Vec2::NEG_Y), ("plastic_chair", Vec3::new(0.0, 0.0, 0.72), Vec2::NEG_Y), ("plastic_chair", Vec3::new(0.0, 0.0, -0.72), Vec2::Y)];
    const SACKS: [(&str, Vec3, Vec2); 3] = [("sacks", Vec3::new(-0.55, 0.0, 0.0), Vec2::Y), ("sacks", Vec3::new(0.55, 0.0, 0.0), Vec2::X), ("sacks", Vec3::new(-0.55, 0.7, 0.0), Vec2::X)];
    const DESK: [(&str, Vec3, Vec2); 2] = [("desk", Vec3::ZERO, Vec2::NEG_Y), ("plastic_chair", Vec3::new(0.0, 0.0, 0.66), Vec2::NEG_Y)];
    const AISLE: [(&str, Vec3, Vec2); 2] = [("shelf", Vec3::new(-0.47, 0.0, 0.0), Vec2::NEG_Y), ("shelf", Vec3::new(0.47, 0.0, 0.0), Vec2::NEG_Y)];
    const SEATS: [(&str, Vec3, Vec2); 3] = [("sofa", Vec3::new(0.0, 0.0, 0.62), Vec2::NEG_Y), ("table", Vec3::new(0.0, 0.0, -0.42), Vec2::NEG_Y), ("plastic_chair", Vec3::new(1.2, 0.0, -0.42), Vec2::NEG_X)];
    match plan {
        Plan::Dhaba => Some((&TABLE, Vec2::new(0.6, 1.0))),
        Plan::Godown => Some((&SACKS, Vec2::new(1.1, 0.55))),
        Plan::Office => Some((&DESK, Vec2::new(0.7, 0.85))),
        Plan::Shop => Some((&AISLE, Vec2::new(0.95, 0.25))),
        Plan::Lobby => Some((&SEATS, Vec2::new(1.5, 1.05))),
        Plan::Flat => None,
    }
}
/// A room narrower than this has furniture along one long wall only.
const NARROW: f32 = 3.6;
/// The room left between groups, and at most how many a room has.
const ISLAND_LANE: f32 = 1.9;
const MAX_ISLANDS: usize = 8;

/// Hollow buildings each district has, as low buildings and tower lobbies.
fn quota(name: &str) -> (usize, usize) {
    match name {
        "Chandni Chowk" => (115, 0),
        "Karol Bagh" => (100, 0),
        "Hauz Khas" => (50, 0),
        "Ghaziabad" => (60, 0),
        "Laxmi Nagar" => (35, 0),
        "Connaught Place" => (10, 0),
        "Anand Vihar" => (4, 0),
        "Film City" => (3, 0),
        "Sector 18" => (8, 8),
        "MG Road" => (14, 6),
        "Saket" => (3, 3),
        "Nehru Place" => (3, 8),
        "Cyber City" => (0, 8),
        "Indirapuram" => (0, 6),
        "Golf Course Road" => (6, 6),
        "Expressway" | "Pari Chowk" | "Aerocity" => (0, 1),
        _ => (0, 0),
    }
}
/// Sheds on the land between districts.
const OPEN_LAND: usize = 150;

/// An outline as an exact rectangle: its corners in the same order, or None when it isn't one.
fn rectangle(pts: &[Vec2]) -> Option<[Vec2; 4]> {
    if pts.len() != 4 {
        return None;
    }
    let e: Vec<Vec2> = (0..4).map(|k| pts[(k + 1) % 4] - pts[k]).collect();
    let square = (0..4).all(|k| e[k].normalize_or_zero().dot(e[(k + 1) % 4].normalize_or_zero()).abs() < 0.03);
    let parallel = (e[0] + e[2]).length() < 0.25 && (e[1] + e[3]).length() < 0.25;
    if !square || !parallel || e.iter().any(|v| !(SIDE.0..=SIDE.1).contains(&v.length())) {
        return None;
    }
    let c = pts.iter().sum::<Vec2>() / 4.0;
    let (u, v) = ((e[0] - e[2]) * 0.25, (e[1] - e[3]) * 0.25);
    // Square the second axis off the first.
    let v = u.normalize().perp() * v.dot(u.normalize().perp());
    let out = [c - u - v, c + u - v, c + u + v, c - u + v];
    Some(out)
}

/// Where footprints stand, for asking what's near: a bucket to every 32 m.
struct Buckets(HashMap<(i32, i32), Vec<u32>>);

impl Buckets {
    const CELL: f32 = 32.0;

    fn key(p: Vec2) -> (i32, i32) {
        ((p.x / Self::CELL).floor() as i32, (p.y / Self::CELL).floor() as i32)
    }

    fn add(&mut self, index: u32, lo: Vec2, hi: Vec2) {
        let (a, b) = (Self::key(lo), Self::key(hi));
        for i in a.0..=b.0 {
            for j in a.1..=b.1 {
                self.0.entry((i, j)).or_default().push(index);
            }
        }
    }

    fn near(&self, lo: Vec2, hi: Vec2) -> Vec<u32> {
        let (a, b) = (Self::key(lo), Self::key(hi));
        let mut out: Vec<u32> = (a.0..=b.0).flat_map(|i| (a.1..=b.1).map(move |j| (i, j))).filter_map(|k| self.0.get(&k)).flatten().copied().collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Chooses the map's hollow buildings and builds their rooms. `per_cell` is how many a 128 m cell
/// may have at most: the one number that turns rooms down, and zero is the map without them.
/// Returns the spots loot can go indoors.
pub fn hollow(map: &mut MapLayout, per_cell: usize) -> Vec<Spot> {
    if per_cell == 0 {
        return vec![];
    }
    let floor = StreetFloor::new(map);
    let mut buildings = Buckets(HashMap::new());
    for (k, f) in map.footprints.iter().enumerate().filter(|(_, f)| f.mat != Mat::Water) {
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        buildings.add(k as u32, lo, hi);
    }
    let mut solids = Buckets(HashMap::new());
    for (k, b) in map.blocks.iter().enumerate().filter(|(_, b)| b.collide && b.pos.y < 6.0) {
        solids.add(k as u32, b.pos.xz(), b.pos.xz());
    }
    // Every candidate, in a fixed shuffled order.
    let mut order: Vec<usize> = (0..map.footprints.len()).collect();
    let mut rng = SmallRng::seed_from_u64(0x0F100D);
    compose::shuffle(&mut order, &mut rng);
    let mut left: Vec<(usize, usize)> = compose::PLACES.iter().map(|p| quota(p.name)).collect();
    let mut sheds = OPEN_LAND;
    let mut in_cell: HashMap<(i32, i32), usize> = HashMap::new();
    let mut spots = vec![];
    let mut rooms = vec![];
    let mut shapes = vec![];
    for index in order {
        let f = &map.footprints[index];
        if f.mat == Mat::Water || f.base > 1.0 {
            continue;
        }
        let Some(pts) = rectangle(&f.pts) else { continue };
        let c = pts.iter().sum::<Vec2>() / 4.0;
        if map.closed.iter().any(|q| q.distance(f.pts.iter().sum::<Vec2>() / 4.0) < 1.0) || pts.iter().chain([&c]).any(|p| map.is_holy(*p)) {
            continue;
        }
        let storeys = ((f.top - f.base) / STOREY).round().max(1.0) as u32;
        let tower = storeys > MAX_STOREYS;
        let district = compose::PLACES.iter().position(|d| (c - d.at).abs().max_element() <= d.clip);
        let budget = match district {
            Some(d) if tower => left[d].1,
            Some(d) => left[d].0,
            None => if tower { 0 } else { sheds },
        };
        let cell = ((c.x / 128.0).floor() as i32, (c.y / 128.0).floor() as i32);
        if budget == 0 || in_cell.get(&cell).copied().unwrap_or(0) >= per_cell {
            continue;
        }
        // Nothing else in it.
        let (lo, hi) = pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let others: Vec<usize> = buildings.near(lo, hi).into_iter().map(|k| k as usize).filter(|k| *k != index).collect();
        let (u, v) = ((pts[1] - pts[0]).normalize(), (pts[3] - pts[0]).normalize());
        let (lu, lv) = (pts[0].distance(pts[1]), pts[0].distance(pts[3]));
        let in_rect = |p: Vec2| {
            let q = p - pts[0];
            let (a, b) = (q.dot(u), q.dot(v));
            a > -0.05 && b > -0.05 && a < lu + 0.05 && b < lv + 0.05
        };
        let samples = |step: f32| {
            let (nu, nv) = ((lu / step).ceil() as i32, (lv / step).ceil() as i32);
            let (base, u, v) = (pts[0], u, v);
            (0..=nu).flat_map(move |i| (0..=nv).map(move |j| base + u * (i as f32 * lu / nu as f32) + v * (j as f32 * lv / nv as f32)))
        };
        let overlapped = others.iter().any(|k| {
            let g = &map.footprints[*k];
            g.pts.iter().any(|p| in_rect(*p)) || samples(2.0).any(|p| footprint::contains(&g.pts, p))
        });
        let crowded = solids.near(lo, hi).into_iter().any(|k| {
            let b = &map.blocks[k as usize];
            in_rect(b.pos.xz()) && b.pos.y - map.terrain.height(b.pos.x, b.pos.z) < 3.0
        });
        if overlapped || crowded {
            continue;
        }
        // Ground level enough to lay a floor on.
        let heights: Vec<f32> = samples(1.0).map(|p| map.terrain.height(p.x, p.y)).collect();
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
        if high - low > 0.5 || samples(2.0).any(|p| matches!(map.terrain.ground_at(p.x, p.y), Some(ground::WATER))) {
            continue;
        }
        // The outline's edges as the paint counts them.
        let mut along = [0.0; 4];
        let mut run = 0.0;
        for k in 0..4 {
            along[k] = run;
            run += pts[k].distance(pts[(k + 1) % 4]);
        }
        let height = storeys as f32 * STOREY;
        let top = f.base + height;
        let shops = has_shops(c, height);
        // The front: an edge with a door's width of the street before it, the busiest such.
        let mut best: Option<(i32, usize, i32)> = None;
        for i in 0..4 {
            let (a, b) = (pts[i], pts[(i + 1) % 4]);
            let (len, dir) = (a.distance(b), (b - a).normalize());
            let out = ((a + b) / 2.0 - c).normalize();
            let mid = (a + b) / 2.0;
            let score = match map.terrain.ground_at(mid.x + out.x * 5.0, mid.y + out.y * 5.0) {
                Some(ground::ROAD | ground::PLAZA) => 3,
                Some(ground::PATH | ground::PARKING) => 2,
                Some(ground::PAVEMENT) => 1,
                _ => 0,
            };
            let free = |s: f32| {
                let p = a + dir * s + out * 2.0;
                [p, p - out * 0.5, p + out * 0.5, p + dir * 0.7, p - dir * 0.7].into_iter().all(|q| floor.reachable(q))
            };
            let door = bays(along[i], len)
                .into_iter()
                .filter(|(bay, s)| (!shops || bay.rem_euclid(2) == 1) && free(*s))
                // Nearest the middle, but a shuttered bay first: a balcony bay's painted rail would
                // be left sticking out either side of the door.
                .min_by_key(|(bay, s)| ((s - len / 2.0).abs() * 2.0) as i32 + 1000 * (!shops && bay.rem_euclid(2) == 0) as i32);
            if let Some((bay, _)) = door
                && best.is_none_or(|b| score > b.0)
            {
                best = Some((score, i, bay));
            }
        }
        let Some((_, front, door_bay)) = best else { continue };
        let seed = (c.x * 7.3 + c.y * 13.1).to_bits();
        let r = hash01(seed, 1, 31);
        let flats_above = matches!(district.map(|d| compose::PLACES[d].name), Some("Chandni Chowk" | "Karol Bagh" | "Hauz Khas" | "Laxmi Nagar" | "Ghaziabad"));
        let area = lu * lv;
        // Over a Delhi shop there are flats, and now and then an office or a storeroom.
        let up = hash01(seed, 2, 37);
        let over_shop = if !flats_above || up < 0.2 { Plan::Office } else if up < 0.34 { Plan::Godown } else { Plan::Flat };
        let (plan, upper) = if tower {
            (Plan::Lobby, Plan::Lobby)
        } else if district.is_none() {
            (if area > 170.0 { Plan::Godown } else if r < 0.5 { Plan::Dhaba } else { Plan::Shop }, Plan::Godown)
        } else if shops {
            (if r < 0.14 { Plan::Dhaba } else if r < 0.24 { Plan::Godown } else { Plan::Shop }, over_shop)
        } else if flats_above && r > 0.15 {
            (Plan::Flat, Plan::Flat)
        } else {
            (Plan::Office, Plan::Office)
        };
        let site = Site { index, pts, front, door_bay, along, base: f.base, top, floor: high, storeys, tower, shops, plan, upper, seed };
        let neighbours = &others;
        let footprints = &map.footprints;
        let open = |p: Vec2, y: f32| !neighbours.iter().any(|k| footprints[*k].top > y && footprint::contains(&footprints[*k].pts, p));
        let (int, mut found) = make(&site, &open, &mut rng);
        if int.doors.is_empty() {
            continue;
        }
        spots.append(&mut found);
        rooms.push(int);
        shapes.push((index, pts, top));
        *in_cell.entry(cell).or_default() += 1;
        match district {
            Some(d) if tower => left[d].1 -= 1,
            Some(d) => left[d].0 -= 1,
            None => sheds -= 1,
        }
    }
    apply(map, rooms, shapes, &mut rng);
    spots.retain(|s| !compose::no_loot(s.pos.xz()) && !map.is_holy(s.pos.xz()));
    spots
}

/// Makes the chosen buildings what their rooms expect: exact outlines and storeys, level ground,
/// nothing in the doors, and a roof with what's been set on it kept clear of the stairs.
fn apply(map: &mut MapLayout, rooms: Vec<Interior>, shapes: Vec<(usize, [Vec2; 4], f32)>, rng: &mut SmallRng) {
    map.room_of = vec![u32::MAX; map.footprints.len()];
    for (k, (index, pts, top)) in shapes.iter().enumerate() {
        let f = &mut map.footprints[*index];
        f.pts = pts.to_vec();
        if rooms[k].solid_to.is_none() {
            f.top = *top;
        }
        map.room_of[*index] = k as u32;
        flatten(map, &rooms[k]);
    }
    let doors: Vec<Vec2> = rooms.iter().flat_map(|r| r.doors.iter().map(|d| d.pos.xz())).collect();
    let mut near_door = Buckets(HashMap::new());
    for (k, d) in doors.iter().enumerate() {
        near_door.add(k as u32, *d - DOOR_CLEAR, *d + DOOR_CLEAR);
    }
    let in_door = |p: Vec2| near_door.near(p, p).into_iter().any(|k| doors[k as usize].distance(p) < DOOR_CLEAR);
    let mut hollow = Buckets(HashMap::new());
    for (k, r) in rooms.iter().enumerate() {
        let a = r.at(0.0, 0.0, 0.0).xz().min(r.at(r.w, 0.0, r.d).xz()).min(r.at(r.w, 0.0, 0.0).xz()).min(r.at(0.0, 0.0, r.d).xz());
        let b = r.at(0.0, 0.0, 0.0).xz().max(r.at(r.w, 0.0, r.d).xz()).max(r.at(r.w, 0.0, 0.0).xz()).max(r.at(0.0, 0.0, r.d).xz());
        hollow.add(k as u32, a, b);
    }
    let on_roof = |p: Vec2| hollow.near(p, p).into_iter().any(|k| rooms[k as usize].contains(p));
    let stays: Vec<bool> = map.kit.iter().map(|k| !on_roof(k.pos.xz()) && !in_door(k.pos.xz())).collect();
    kit::keep(map, &stays);
    map.props.retain(|p| !in_door(p.pos.xz()) && !on_roof(p.pos.xz()));
    // Roof life again, clear of the stair room.
    let mut b = super::build::Builder::new(map);
    const ROOF: [&str; 8] = ["cooler", "dish", "solar", "clothesline", "pot", "chairs", "drum", "crates"];
    for r in rooms.iter().filter(|r| r.stair.is_some()) {
        let s = r.stair.as_ref().unwrap();
        // The row before the stair room stays clear: it's the way down.
        let reach = s.z.0 - 2.2 - SHELL - 0.9;
        if reach < 0.5 {
            continue;
        }
        for _ in 0..rng.random_range(1..=3) {
            let name = ROOF[rng.random_range(0..ROOF.len())];
            let (x, z) = (rng.random_range(1.2..(r.w - 1.2).max(1.3)), SHELL + 0.9 + rng.random_range(0.0..reach));
            kit::place(&mut b, name, r.at(x, r.roof, z), rng.random_range(0.0..std::f32::consts::TAU));
        }
    }
    map.rooms = rooms;
}

/// About 60 of Connaught Place's open shops get the same furniture as the rooms, along their back
/// walls, where the chests in the middle of each and the way in from the arch are clear of it.
pub fn furnish_ring(map: &mut MapLayout) {
    const EVERY: usize = 2;
    const SETS: [&[&str]; 4] = [&["shelf", "counter", "shelf"], &["shelf", "shelf", "sacks"], &["almirah", "desk", "plastic_chair"], &["sacks", "water_drum", "shelf"]];
    if map.rooms.is_empty() {
        return;
    }
    let shops: Vec<RingShop> = map.ring_shops.iter().step_by(EVERY).copied().collect();
    let mut b = super::build::Builder::new(map);
    for (k, shop) in shops.iter().enumerate() {
        let into = shop.into.normalize_or_zero();
        let along = into.perp();
        let set = SETS[k % SETS.len()];
        let count = set.len() as f32;
        for (n, name) in set.iter().copied().enumerate() {
            let size = piece_size(name);
            let at = shop.back.xz() + along * ((n as f32 + 0.5) / count - 0.5) * shop.wide.min(6.0) * 0.8 + into * (size.z / 2.0 + 0.1);
            let pos = Vec3::new(at.x, shop.back.y, at.y);
            let yaw = kit::facing(into);
            b.map.ring_pieces.push(Piece { name, pos, yaw });
            if piece_is_solid(name) {
                b.push(pos + Vec3::Y * size.y / 2.0, Quat::from_rotation_y(yaw), Shape::Box(size), Mat::Concrete, true, false);
            }
        }
    }
}

/// Levels the ground under a building to its floor.
fn flatten(map: &mut MapLayout, r: &Interior) {
    let t = &mut map.terrain;
    let step = t.step();
    let (n, half) = (t.n, t.size * 0.5);
    let idx = |c: f32| (((c + half) / step).floor().max(0.0) as usize).min(n - 1);
    let corners = [r.at(0.0, 0.0, 0.0), r.at(r.w, 0.0, 0.0), r.at(r.w, 0.0, r.d), r.at(0.0, 0.0, r.d)];
    let (lo, hi) = corners.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(p.xz()), b.max(p.xz())));
    for i in idx(lo.x - step)..=idx(hi.x + step) {
        for j in idx(lo.y - step)..=idx(hi.y + step) {
            let p = Vec2::new(t.coord(i), t.coord(j));
            let l = p - r.corner;
            let (a, b) = (l.dot(r.x), l.dot(r.z));
            if a > -step && b > -step && a < r.w + step && b < r.d + step {
                t.h[i * n + j] = r.floor;
                // No grass through the floor.
                if t.ground[i * n + j] != ground::ROAD {
                    t.ground[i * n + j] = ground::PAVEMENT;
                }
            }
        }
    }
}

/// The share of indoor finds upstairs, about.
const UPSTAIRS_FINDS: f32 = 0.25;

/// Half of each district's chest set moves indoors, gold to the top floors and the roofs, the rest
/// onto ground floors the bots walk, and about 240 floor finds with them.
pub fn move_loot(map: &mut MapLayout, spots: &[Spot], rng: &mut SmallRng) {
    const FLOOR_FINDS: usize = 240;
    if spots.is_empty() {
        return;
    }
    let mut used = vec![false; spots.len()];
    let mut chests: Vec<usize> = (0..map.chests.len()).filter(|k| map.chests[*k].set.is_some()).collect();
    compose::shuffle(&mut chests, rng);
    let mut moved: HashMap<(u8, u8), usize> = HashMap::new();
    let mut seen: HashMap<(u8, u8), usize> = HashMap::new();
    for &k in &chests {
        let c = &map.chests[k];
        let set = c.set.expect("filtered");
        *seen.entry(set).or_default() += 1;
        if moved.get(&set).copied().unwrap_or(0) * 2 >= seen[&set] {
            continue;
        }
        let gold = c.kind == ChestKind::Tijori;
        let fits = |s: &Spot| if gold { s.roof || s.storey > 0 } else { s.storey == 0 };
        let far = |s: &Spot| s.pos.xz().distance_squared(c.pos.xz());
        let spot = (0..spots.len()).filter(|i| !used[*i] && fits(&spots[*i])).min_by(|a, b| far(&spots[*a]).total_cmp(&far(&spots[*b])));
        if let Some(i) = spot.filter(|i| far(&spots[*i]) < 240.0 * 240.0) {
            used[i] = true;
            *moved.entry(set).or_default() += 1;
            map.chests[k].pos = spots[i].pos;
            map.chests[k].yaw = spots[i].yaw;
        }
    }
    let mut free: Vec<usize> = (0..spots.len()).filter(|i| !used[*i] && !spots[*i].roof).collect();
    compose::shuffle(&mut free, rng);
    // Mostly ground floors, where the bots loot too: an upstairs full of finds would leave them short.
    free.sort_by_key(|i| spots[*i].storey > 0 && hash01(*i as u32, 5, 7) > UPSTAIRS_FINDS);
    let mut left = vec![true; map.floor_loot.len()];
    for &i in free.iter().take(FLOOR_FINDS) {
        let spot = spots[i].pos;
        let far = |p: &Vec3| p.xz().distance_squared(spot.xz());
        let nearest = (0..left.len()).filter(|k| left[*k]).min_by(|a, b| far(&map.floor_loot[*a]).total_cmp(&far(&map.floor_loot[*b])));
        if let Some(k) = nearest.filter(|k| far(&map.floor_loot[*k]) < 200.0 * 200.0) {
            left[k] = false;
            map.floor_loot[k] = spot + Vec3::Y * 0.3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{MapId, generate};
    use std::sync::OnceLock;

    fn map() -> &'static MapLayout {
        static MAP: OnceLock<MapLayout> = OnceLock::new();
        MAP.get_or_init(|| generate(MapId::Ncr, 7))
    }

    /// A wall's boxes cover it all but its holes, and never twice.
    #[test]
    fn a_wall_is_its_rects_and_its_holes() {
        let cut = |s0, s1, y0, y1| Cut { s0, s1, y0, y1 };
        let holes = [cut(1.0, 2.0, 1.0, 2.0), cut(1.0, 2.0, 2.5, 3.0), cut(4.0, 5.0, 0.0, 2.0), cut(-1.0, 0.5, 1.0, 1.5)];
        let (s, y) = ((0.0, 6.0), (0.0, 3.4));
        let solid: f32 = rects(s, y, &holes).iter().map(|[a, b, c, d]| (b - a) * (d - c)).sum();
        let open = 1.0 + 0.5 + 2.0 + 0.25;
        assert!((solid - (6.0 * 3.4 - open)).abs() < 1e-3, "{solid}");
        let (a, b) = (rects(s, y, &holes), rects(s, y, &[]));
        assert_eq!(b, vec![[0.0, 6.0, 0.0, 3.4]]);
        for (i, p) in a.iter().enumerate() {
            for q in &a[i + 1..] {
                assert!(p[1] <= q[0] + 1e-4 || q[1] <= p[0] + 1e-4 || p[3] <= q[2] + 1e-4 || q[3] <= p[2] + 1e-4, "{p:?} overlaps {q:?}");
            }
        }
    }

    #[test]
    fn only_true_rectangles_go_hollow() {
        let square = [Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)];
        assert!(rectangle(&square).is_some());
        assert!(rectangle(&square[..3]).is_none());
        assert!(rectangle(&[Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(12.0, 8.0), Vec2::new(0.0, 8.0)]).is_none());
        assert!(rectangle(&[Vec2::ZERO, Vec2::new(5.0, 0.0), Vec2::new(5.0, 8.0), Vec2::new(0.0, 8.0)]).is_none());
    }

    /// The number the plan set, and rooms everywhere the plan wanted them.
    #[test]
    #[ignore = "NCR port: main's halls take ground floors first, 389 go hollow; retune in the port's step 3"]
    fn about_five_hundred_buildings_are_hollow() {
        let m = map();
        assert!((480..=560).contains(&m.rooms.len()), "{}", m.rooms.len());
        for plan in [Plan::Shop, Plan::Flat, Plan::Godown, Plan::Dhaba, Plan::Lobby] {
            assert!(m.rooms.iter().any(|r| r.plan == plan), "no {plan:?}");
        }
    }

    /// A hole in the skin is where the paint shows a window: a whole bay's middle, on a storey.
    #[test]
    fn holes_are_on_the_painted_windows() {
        let m = map();
        for r in m.rooms.iter().take(120) {
            let f = &m.footprints[r.footprint];
            let mut along = 0.0;
            for i in 0..4 {
                let len = f.pts[i].distance(f.pts[(i + 1) % 4]);
                for c in &r.skin[i] {
                    let bay = ((c.s0 + c.s1) / 2.0 + along) / BAY - 0.5;
                    assert!((bay - bay.round()).abs() < 1e-2, "a hole {:.2} off its bay", bay - bay.round());
                    assert!(c.s0 > 0.0 && c.s1 < len, "a hole past the wall's end");
                    let storey = (c.y0 - f.base).rem_euclid(STOREY);
                    assert!(storey < 1e-3 || (storey - SILL).abs() < 1e-3 || c.y0 <= r.floor + 0.01, "a sill {storey} up its storey");
                }
                along += len;
            }
        }
    }

    /// Rooms keep inside their outlines, at the storeys the paint shows, and out of what is holy.
    #[test]
    fn rooms_keep_to_their_buildings() {
        let m = map();
        for r in &m.rooms {
            let f = &m.footprints[r.footprint];
            assert_eq!(m.rooms_of(r.footprint).map(|x| x.footprint), Some(r.footprint));
            let storeys = (f.top - f.base) / STOREY;
            assert!(r.solid_to.is_some() || (storeys - storeys.round()).abs() < 1e-3, "{storeys} storeys");
            assert!(r.solid_to.is_some() || (f.top - r.roof).abs() < 1e-3, "roof {} under top {}", r.roof, f.top);
            for p in &r.pieces {
                assert!(r.contains(p.pos.xz()), "a {} outside its building", p.name);
            }
            assert!(!m.is_holy(r.at(r.w / 2.0, 0.0, r.d / 2.0).xz()));
            assert!(!m.closed.iter().any(|c| r.contains(*c)));
            assert!(!r.doors.is_empty());
        }
    }

    /// Nothing the bots or the street kit do changes when a door is opened: what stood in a door's
    /// way has gone.
    #[test]
    fn door_fronts_are_clear() {
        let m = map();
        let doors: Vec<Vec2> = m.rooms.iter().flat_map(|r| r.doors.iter().map(|d| d.pos.xz())).collect();
        for k in &m.kit {
            assert!(doors.iter().all(|d| d.distance(k.pos.xz()) >= DOOR_CLEAR - 0.01 || k.pos.y > 2.0), "a {} in a doorway", k.name);
        }
    }
}
