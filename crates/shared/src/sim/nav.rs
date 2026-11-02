//! Where a warrior can walk, and the way there around buildings and up stairs.
//!
//! Bots used to walk straight at their goal, which on a city map meant pressing into Connaught
//! Place's ring (or a lane's dead end) until the Aandhi caught them. Here the map is sampled once
//! into a half-metre grid (a cell is free when a box wider than a warrior there touches nothing
//! solid and it isn't inside a building's outline), a few rows a tick so no single tick stalls,
//! and A* on it gives each bot a path, trimmed to its corners. The box is wider than the warrior
//! so that a free cell is one the warrior truly fits through, and the cells are half a metre so
//! that a 1.4 m door still has a cell centred where the warrior fits.
//!
//! Stairs make the grid layered: every long ramp in the map brings the floor it climbs to as a
//! level of its own, sampled the same way, and the ramp joins the two levels. The street is level
//! 0. Connected regions are labelled once the sampling ends, so a goal nobody can reach is known
//! at once instead of by flooding the map.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Mutex;

use avian3d::prelude::*;
use bevy::platform::time::Instant;
use bevy::prelude::*;

use super::{HALF_HEIGHT, solid_mask};
use crate::map::compose::{SPANS, Span};
use crate::map::{Block, Mat, MapLayout, Shape};

pub const CELL: f32 = 0.5;
/// Rows sampled per tick: a 500 m map takes four seconds of match time, while the vimana is still
/// flying, at about 5 ms a tick.
const ROWS_PER_TICK: usize = 4;
/// Nodes A* may expand before it gives up; regions already rule out the unreachable.
const BUDGET: usize = 800_000;
/// The probe box: a little wider than the warrior's capsule (0.76 m), from just over ankle height
/// to over the head, so that a stair's underside counts as well as a wall.
const PROBE_W: f32 = 0.8;
const PROBE_FROM: f32 = 0.3;
const PROBE_TO: f32 = 1.9;

/// Metres as a count of cells.
fn cells(m: f32) -> isize {
    (m / CELL).round() as isize
}
/// A ramp rising this much climbs to another floor; less is part of the street.
const STAIR_RISE: f32 = 3.0;
const NO_LINK: u32 = u32::MAX;
/// Marks a node A* has expanded, in its best-cost slot: with the weighted heuristic a node could
/// otherwise be reopened again and again.
const CLOSED: u32 = 1 << 31;

/// A ramp up to a floor: its top surface's ends, and the nodes it joins.
#[derive(Clone, Copy, Debug)]
struct Stair {
    foot: Vec3,
    head: Vec3,
    width: f32,
    /// None until the sampling has found free cells at both ends.
    nodes: Option<(u32, u32)>,
}

impl Stair {
    fn len(&self) -> f32 {
        self.foot.distance(self.head)
    }

    /// Whether a warrior's centre at `p` is standing on this ramp.
    fn under(&self, p: Vec3) -> bool {
        let axis = (self.head - self.foot).xz();
        let t = (p.xz() - self.foot.xz()).dot(axis) / axis.length_squared();
        if !(-0.05..=1.05).contains(&t) {
            return false;
        }
        let side = (p.xz() - self.foot.xz() - axis * t).length();
        let surface = self.foot.y + (self.head.y - self.foot.y) * t;
        side < self.width / 2.0 + 0.5 && (p.y - HALF_HEIGHT - surface).abs() < 0.8
    }
}

#[derive(Resource, Default)]
pub struct NavGrid {
    n: usize,
    half: f32,
    /// Per level (street first), n × n cells.
    free: Vec<Vec<bool>>,
    /// Height of each level above the street's; the street's is unused.
    floors: Vec<f32>,
    stairs: Vec<Stair>,
    /// By node: the stair starting here, if any.
    link: Vec<u32>,
    /// By node: its connected region (0 for a blocked cell), and the largest street region.
    region: Vec<u32>,
    main: u32,
    rows_done: usize,
    /// Ticks waited for the physics world to settle before sampling.
    wait: u32,
    /// A*'s working memory, kept between searches: zeroing a fresh page per node every search
    /// cost more than the search.
    scratch: Mutex<Scratch>,
    /// Searches run, their seconds in all, the longest, and nodes expanded in all: for the checks.
    stats: Mutex<(u32, f32, f32, u64)>,
}

#[derive(Default)]
struct Scratch {
    /// By node: best cost + 1, 0 for unvisited; reset after each search by the touched list.
    best: Vec<u32>,
    came: Vec<u32>,
    touched: Vec<u32>,
}

/// The long ramps of a map, foot first.
fn find_stairs(blocks: &[Block]) -> Vec<Stair> {
    let mut out = vec![];
    for b in blocks.iter().filter(|b| b.collide) {
        let Shape::Box(s) = b.shape else { continue };
        let up = b.rot * Vec3::Y;
        if up.y > 0.98 || up.y < 0.6 {
            continue;
        }
        let along = b.rot * Vec3::X * s.x / 2.0;
        let top = b.pos + up * s.y / 2.0;
        let (foot, head) = if along.y > 0.0 { (top - along, top + along) } else { (top + along, top - along) };
        if head.y - foot.y > STAIR_RISE {
            out.push(Stair { foot, head, width: s.z, nodes: None });
        }
    }
    out
}

impl NavGrid {
    pub fn ready(&self) -> bool {
        self.n > 0 && self.rows_done == self.n
    }

    fn cell(&self, p: Vec2) -> Option<(usize, usize)> {
        let i = ((p.x + self.half) / CELL).floor();
        let j = ((p.y + self.half) / CELL).floor();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.n && (j as usize) < self.n).then_some((i as usize, j as usize))
    }

    fn centre(&self, (i, j): (usize, usize)) -> Vec2 {
        Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * CELL - Vec2::splat(self.half)
    }

    fn node(&self, level: usize, (i, j): (usize, usize)) -> u32 {
        ((level * self.n + j) * self.n + i) as u32
    }

    /// A node's level and cell.
    fn unpack(&self, node: u32) -> (usize, (usize, usize)) {
        let (node, n) = (node as usize, self.n);
        (node / (n * n), (node % n, node / n % n))
    }

    fn is_free(&self, level: usize, i: isize, j: isize) -> bool {
        i >= 0 && j >= 0 && (i as usize) < self.n && (j as usize) < self.n && self.free[level][j as usize * self.n + i as usize]
    }

    /// Free at street level.
    pub fn walkable(&self, p: Vec2) -> bool {
        self.walkable_on(0, p)
    }

    fn walkable_on(&self, level: usize, p: Vec2) -> bool {
        self.cell(p).is_some_and(|(i, j)| self.free[level][j * self.n + i])
    }

    /// The level whose floor is at about `y`: the height of a goal, or a warrior's centre.
    fn level_at(&self, y: f32) -> usize {
        (1..self.floors.len()).find(|&l| (self.floors[l] - 0.6..self.floors[l] + 2.4).contains(&y)).unwrap_or(0)
    }

    /// Whether the grid covers someone at `p` with the ground at `ground`: on the street, on a
    /// sampled floor with footing, or on a stair; not on a roof, and not on a ledge outside a
    /// floor's wall (the ring's slabs stick 0.65 m past its upper walls).
    pub fn knows(&self, p: Vec3, ground: f32) -> bool {
        p.y < ground + HALF_HEIGHT + 1.5 || self.level_at(p.y) != 0 && self.footing(self.level_at(p.y), p.xz()).is_some() || self.stairs.iter().any(|s| s.under(p))
    }

    /// The free cell under us or within a metre, reached without crossing anything: where we
    /// truly stand on a level. A free cell further off, or one across the 0.3 m wall we are
    /// pressed against, may be the room beyond a ledge.
    fn footing(&self, level: usize, p: Vec2) -> Option<(usize, usize)> {
        let (ci, cj) = self.cell(p).map(|(i, j)| (i as isize, j as isize))?;
        let r = cells(1.0);
        let mut near: Vec<(isize, isize)> = (-r..=r).flat_map(|dj| (-r..=r).map(move |di| (ci + di, cj + dj))).filter(|&(i, j)| self.is_free(level, i, j)).collect();
        near.sort_by_key(|&(i, j)| (i - ci).pow(2) + (j - cj).pow(2));
        near.into_iter().map(|(i, j)| (i as usize, j as usize)).find(|&c| self.open_to(level, p, self.centre(c)))
    }

    /// Whether the straight line from `p` to `q` crosses no blocked cell beyond the body's own
    /// clearance (the first 0.45 m: standing close to a wall, our own cell is blocked).
    fn open_to(&self, level: usize, p: Vec2, q: Vec2) -> bool {
        let len = p.distance(q);
        let steps = (len / (CELL * 0.25)).ceil().max(1.0) as usize;
        (0..=steps).all(|s| {
            let t = s as f32 / steps as f32;
            t * len <= 0.45 || self.walkable_on(level, p.lerp(q, t))
        })
    }

    /// The street cell nearest `p` that the main body of the map can reach, within 24 m.
    pub fn snap(&self, p: Vec2) -> Option<Vec2> {
        let c = self.cell(p.clamp(Vec2::splat(-self.half), Vec2::splat(self.half - 0.01)))?;
        self.nearest_free(0, c, cells(24.0), Some(self.main)).map(|c| self.centre(c))
    }

    /// The free cell nearest `c` on a level, within `r` cells, in `region` if one is named.
    fn nearest_free(&self, level: usize, c: (usize, usize), r: isize, region: Option<u32>) -> Option<(usize, usize)> {
        let (ci, cj) = (c.0 as isize, c.1 as isize);
        let fits = |i: isize, j: isize| {
            self.is_free(level, i, j) && region.is_none_or(|g| self.region[self.node(level, (i as usize, j as usize)) as usize] == g)
        };
        (0..=r)
            .find_map(|k| {
                let ring = (-k..=k).flat_map(|d| [(ci + d, cj - k), (ci + d, cj + k), (ci - k, cj + d), (ci + k, cj + d)]);
                ring.filter(|&(i, j)| fits(i, j)).min_by_key(|&(i, j)| (i - ci).pow(2) + (j - cj).pow(2))
            })
            .map(|(i, j)| (i as usize, j as usize))
    }

    /// Whether the straight line between two cells crosses only free cells.
    fn clear(&self, level: usize, a: (usize, usize), b: (usize, usize)) -> bool {
        let (pa, pb) = (self.centre(a), self.centre(b));
        let steps = (pa.distance(pb) / (CELL * 0.25)).ceil() as usize;
        // The warrior's width: either side of the line too.
        let side = (pb - pa).normalize_or_zero().perp() * (PROBE_W / 2.0);
        (0..=steps).all(|s| {
            let p = pa.lerp(pb, s as f32 / steps.max(1) as f32);
            self.walkable_on(level, p) && self.walkable_on(level, p + side) && self.walkable_on(level, p - side)
        })
    }

    /// Where A* starts: the stair we are on (both its ends), else our footing, whatever it
    /// connects to (a ledge is a ledge: a start in the room beyond its wall would send us through
    /// the wall), else, on the street only, the nearest cell of the main body within 3 m.
    fn starts(&self, from: Vec3) -> Vec<(u32, u32)> {
        if let Some(s) = self.stairs.iter().find(|s| s.under(from)) {
            if let Some((foot, head)) = s.nodes {
                let cost = |end: Vec3| (end.distance(from) / CELL * 10.0) as u32;
                return vec![(foot, cost(s.foot)), (head, cost(s.head))];
            }
        }
        let level = self.level_at(from.y);
        let found = self.footing(level, from.xz()).or_else(|| {
            let c = self.cell(from.xz()).filter(|_| level == 0)?;
            self.nearest_free(0, c, cells(3.0), Some(self.main)).or_else(|| self.nearest_free(0, c, cells(3.0), None))
        });
        found.map(|c| vec![(self.node(level, c), 0)]).unwrap_or_default()
    }

    /// A walk from `from` to (near) `to` along free cells and up or down stairs, as corner points
    /// ending at the goal.
    pub fn path(&self, from: Vec3, to: Vec3) -> Result<Vec<Vec2>, NoPath> {
        if !self.ready() {
            return Err(NoPath::Here);
        }
        let t = Instant::now();
        let (r, expanded) = self.search(from, to);
        let s = t.elapsed().as_secs_f32();
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.0 += 1;
        stats.1 += s;
        stats.2 = stats.2.max(s);
        stats.3 += expanded as u64;
        r
    }

    /// For the checks: searches run so far, their mean and longest milliseconds, mean expansions.
    pub fn stats(&self) -> (u32, f32, f32, f32) {
        let s = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        (s.0, 1000.0 * s.1 / s.0.max(1) as f32, 1000.0 * s.2, s.3 as f32 / s.0.max(1) as f32)
    }

    fn search(&self, from: Vec3, to: Vec3) -> (Result<Vec<Vec2>, NoPath>, usize) {
        let starts = self.starts(from);
        let Some(&(first, _)) = starts.first() else { return (Err(NoPath::Here), 0) };
        let here = self.region[first as usize];
        // The nearest cell to the goal that we can reach.
        let level = self.level_at(to.y);
        let reachable = self.cell(to.xz()).and_then(|c| self.nearest_free(level, c, cells(12.0), Some(here)));
        let Some(goal_cell) = reachable else {
            // Off in a pocket of our own: it is where we stand that's wrong, not the goal.
            return (Err(if here == self.main { NoPath::There } else { NoPath::Here }), 0);
        };
        let goal = self.node(level, goal_cell);
        let (n, total) = (self.n as isize, self.region.len());
        let h = |c: (usize, usize)| {
            let (dx, dy) = (c.0.abs_diff(goal_cell.0) as u32, c.1.abs_diff(goal_cell.1) as u32);
            // Octile distance in tenths of a cell, weighted by 1.4: the search then heads for the
            // goal instead of sweeping every cell of the park, and a bot's way may be a little
            // longer than the shortest, which no one can tell.
            14 * dx.max(dy) + 6 * dx.min(dy)
        };
        let mut scratch = self.scratch.lock().unwrap_or_else(|e| e.into_inner());
        if scratch.best.len() != total {
            *scratch = Scratch { best: vec![0; total], came: vec![0; total], touched: vec![] };
        }
        let Scratch { best, came, touched } = &mut *scratch;
        // Stored as cost + 1, so a zero means unvisited.
        let mut open = BinaryHeap::new();
        for &(node, cost) in &starts {
            if best[node as usize] == 0 {
                touched.push(node);
            }
            best[node as usize] = cost + 1;
            came[node as usize] = node + 1;
            open.push(Reverse((cost + h(self.unpack(node).1), Reverse(cost), node)));
        }
        let mut expanded = 0;
        let mut found = false;
        while let Some(Reverse((_, Reverse(g), node))) = open.pop() {
            if node == goal {
                found = true;
                break;
            }
            let slot = best[node as usize];
            if slot & CLOSED != 0 || slot != g + 1 {
                continue;
            }
            best[node as usize] = slot | CLOSED;
            expanded += 1;
            if expanded > BUDGET {
                break;
            }
            let (lv, c) = self.unpack(node);
            let (ci, cj) = (c.0 as isize, c.1 as isize);
            let mut visit = |next: u32, ng: u32, cell: (usize, usize)| {
                let slot = best[next as usize];
                if slot & CLOSED == 0 && (slot == 0 || ng + 1 < slot) {
                    if slot == 0 {
                        touched.push(next);
                    }
                    best[next as usize] = ng + 1;
                    came[next as usize] = node + 1;
                    open.push(Reverse((ng + h(cell), Reverse(ng), next)));
                }
            };
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (ni, nj) = (ci + di, cj + dj);
                if !self.is_free(lv, ni, nj) {
                    continue;
                }
                // No cutting a wall's corner diagonally.
                if di != 0 && dj != 0 && !(self.is_free(lv, ci + di, cj) && self.is_free(lv, ci, cj + dj)) {
                    continue;
                }
                let next = ((lv as isize * n + nj) * n + ni) as u32;
                visit(next, g + if di != 0 && dj != 0 { 14 } else { 10 }, (ni as usize, nj as usize));
            }
            if let Some(stair) = self.stairs.get(self.link[node as usize] as usize) {
                let (foot, head) = stair.nodes.expect("linked stairs have both ends");
                let other = if node == foot { head } else { foot };
                visit(other, g + (stair.len() / CELL * 11.0) as u32, self.unpack(other).1);
            }
        }
        let mut nodes = vec![goal];
        if found {
            while let Some(&prev) = came.get(*nodes.last().unwrap() as usize) {
                let prev = prev - 1;
                if prev == *nodes.last().unwrap() {
                    break;
                }
                nodes.push(prev);
            }
            nodes.reverse();
        }
        for &n in touched.iter() {
            best[n as usize] = 0;
        }
        touched.clear();
        drop(scratch);
        if !found {
            return (Err(NoPath::There), expanded);
        }
        let mut out = self.corners(&nodes);
        if let Some(last) = out.last_mut() {
            // End on the goal itself when it is walkable (a chest's front, not its cell's middle).
            if self.cell(to.xz()) == Some(goal_cell) {
                *last = to.xz();
            }
        }
        if out.is_empty() {
            out.push(self.centre(goal_cell));
        }
        (Ok(out), expanded)
    }

    /// Keep only the corners: from each kept node, the farthest one still in a straight line.
    /// A change of level is a stair, and always a corner.
    fn corners(&self, nodes: &[u32]) -> Vec<Vec2> {
        let mut out = vec![];
        let mut at = 0;
        while at + 1 < nodes.len() {
            let (level, from) = self.unpack(nodes[at]);
            let (next_level, next) = self.unpack(nodes[at + 1]);
            if next_level != level {
                out.push(self.centre(next));
                at += 1;
                continue;
            }
            // The end of this level's run.
            let run = nodes[at..].iter().position(|&m| self.unpack(m).0 != level).map_or(nodes.len(), |k| at + k) - 1;
            let in_line = |far: usize| self.clear(level, from, self.unpack(nodes[far]).1);
            // In long strides first: a line across the park is hundreds of cells.
            let mut far = at + 1;
            let mut stride = 16;
            while stride > 0 {
                if far + stride <= run && in_line(far + stride) {
                    far += stride;
                } else {
                    stride /= 2;
                }
            }
            out.push(self.centre(self.unpack(nodes[far]).1));
            at = far;
        }
        out
    }

    /// With every row sampled: joins the levels at the stairs and labels the connected regions.
    fn finish(&mut self) {
        let count = self.free.len() * self.n * self.n;
        self.link = vec![NO_LINK; count];
        for s in 0..self.stairs.len() {
            let stair = self.stairs[s];
            let dir = (stair.head - stair.foot).xz().normalize_or_zero();
            let level = (1..self.floors.len()).find(|&l| (self.floors[l] - stair.head.y).abs() < 0.3).unwrap_or(0);
            let foot = self.cell(stair.foot.xz() + dir * 0.3).and_then(|c| self.nearest_free(0, c, cells(2.0), None));
            let head = self.cell(stair.head.xz() + dir * 0.8).and_then(|c| self.nearest_free(level, c, cells(2.0), None));
            if let (Some(foot), Some(head), true) = (foot, head, level != 0) {
                let (foot, head) = (self.node(0, foot), self.node(level, head));
                self.stairs[s].nodes = Some((foot, head));
                self.link[foot as usize] = s as u32;
                self.link[head as usize] = s as u32;
            }
        }
        // Flood the free nodes, crossing stairs.
        self.region = vec![0; count];
        let mut sizes = vec![0u32];
        let mut stack = vec![];
        for seed in 0..count as u32 {
            let (level, (i, j)) = self.unpack(seed);
            if self.region[seed as usize] != 0 || !self.free[level][j * self.n + i] {
                continue;
            }
            let id = sizes.len() as u32;
            let mut size = 0;
            self.region[seed as usize] = id;
            stack.push(seed);
            while let Some(node) = stack.pop() {
                size += 1;
                let (lv, c) = self.unpack(node);
                let (ci, cj) = (c.0 as isize, c.1 as isize);
                for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    let (ni, nj) = (ci + di, cj + dj);
                    if !self.is_free(lv, ni, nj) || (di != 0 && dj != 0 && !(self.is_free(lv, ci + di, cj) && self.is_free(lv, ci, cj + dj))) {
                        continue;
                    }
                    let next = self.node(lv, (ni as usize, nj as usize));
                    if self.region[next as usize] == 0 {
                        self.region[next as usize] = id;
                        stack.push(next);
                    }
                }
                if let Some((foot, head)) = self.stairs.get(self.link[node as usize] as usize).and_then(|s| s.nodes) {
                    let other = if node == foot { head } else { foot };
                    if self.region[other as usize] == 0 {
                        self.region[other as usize] = id;
                        stack.push(other);
                    }
                }
            }
            sizes.push(size);
        }
        self.main = (1..sizes.len()).max_by_key(|&r| sizes[r]).unwrap_or(0) as u32;
    }

    /// For the checks: the stairs that join two levels, and the ones that were found at all.
    pub fn stair_count(&self) -> (usize, usize) {
        (self.stairs.iter().filter(|s| s.nodes.is_some()).count(), self.stairs.len())
    }

    /// For the checks: one level's free cells in a rectangle, a row of `#`/`.` per z.
    pub fn sketch(&self, level: usize, min: Vec2, max: Vec2) -> String {
        let mut out = String::new();
        let mut z = min.y;
        while z < max.y {
            let mut x = min.x;
            while x < max.x {
                let on = self.stairs.iter().any(|s| s.nodes.is_some() && (s.foot.xz().distance(Vec2::new(x, z)) < 0.8));
                out.push(if on { 'S' } else if self.walkable_on(level, Vec2::new(x, z)) { '.' } else { '#' });
                x += CELL;
            }
            out.push('\n');
            z += CELL;
        }
        out
    }

    /// For the checks: each joined stair's ends, foot first.
    pub fn stair_ends(&self) -> Vec<(Vec3, Vec3)> {
        self.stairs.iter().filter(|s| s.nodes.is_some()).map(|s| (s.foot, s.head)).collect()
    }
}

/// Why there's no path: where we stand isn't on the grid's streets (a roof, a pocket it mistook
/// for walled in), or the goal can't be reached from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoPath {
    Here,
    There,
}

// ------------------------------------------------------------------ the NCR's street level

/// Cells a side of the square a street search runs in: 128 m, wider than any stretch of a route.
const WINDOW: usize = 256;
/// A warrior's radius, near enough, as the clearance every obstacle is grown by.
const CLEARANCE: f32 = PROBE_W / 2.0;
/// How far from a point `StreetFloor::path` looks for a free cell to stand for it, metres.
const NEAR_START: f32 = 2.0;
const NEAR_GOAL: f32 = 3.0;
/// Cells one street search may expand: a stretch of route is short, so a search this long has met
/// a wall with no way round, and the bot does better to walk on and ask again later.
const MAX_EXPANDED: usize = 6000;
/// Steepest ground a warrior can climb, as rise over run (the 48 degrees in `movement`).
const STEEPEST: f32 = 1.05;
/// Metres between route points, and how far one shifts to find floor, so that a lane narrower than
/// the grid still has points down it.
const LANE: f32 = 8.0;
const LANE_SNAP: f32 = 5.0;
/// What a metre of route costs.
const COST_PER_M: f32 = 10.0 / LANE;
/// The eight ways out of a route point and what each costs, in opposite pairs.
const WAYS: [(isize, isize, u32); 8] = [(1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10), (1, 1, 14), (-1, -1, 14), (1, -1, 14), (-1, 1, 14)];
/// How far past its end a deck is reached from, metres: the ramp's foot, on a bridge that has one.
const DECK_APPROACH: f32 = 6.0;
/// In route points: how far off a warrior looks for a route point it can walk straight to, and how
/// far for any at all (the middle of the Yamuna is the worst of it).
const ENTRY_NEAR: isize = 2;
const ENTRY_FAR: isize = 24;

/// The bridge whose deck a warrior at `pos` is standing on.
fn deck_under(pos: Vec3) -> Option<usize> {
    let on = |s: &Span| {
        let along = (s.c - s.a).normalize_or_zero();
        let t = (pos.xz() - s.a).dot(along);
        (-2.0..s.a.distance(s.c) + 2.0).contains(&t)
            && (pos.xz() - s.a - along * t).length() < s.width / 2.0 + 1.0
            && (pos.y - HALF_HEIGHT - s.top).abs() < 1.5
    };
    SPANS.iter().position(on)
}

/// Whether a warrior at `pos` is up on one of the NCR's bridges.
pub fn on_bridge(pos: Vec3) -> bool {
    deck_under(pos).is_some()
}

/// What stands in a warrior's way at street level across the whole NCR, as half-metre cells of one
/// bit each. The 0.5 m grid above would need a gigabyte and minutes of physics probes for 4 km, so
/// this one is drawn straight from the map's own shapes when the map loads, at about 8 MB. It has
/// one level, the street: roofs are left by their edges.
///
/// Over it lie the routes, for the long walks: a point every 8 m, joined to a neighbour only where
/// a straight walk between them is clear, so they know walls and cliffs, and a chain of points
/// along each bridge's deck, joined to the ground only at its ends, so a route goes over a river
/// and never along the bed under the deck. `route` says which way to go; `path` says how to get
/// along the next stretch of that, round the stalls and trees between two route points.
#[derive(Resource, Default)]
pub struct StreetFloor {
    n: usize,
    half: f32,
    /// A bit per cell, row by row: set where a warrior can't stand.
    blocked: Vec<u64>,
    /// A bit per cell: the hollow buildings' ground floors, flooded in from their doors. The routes
    /// don't go indoors, so these are the cells a warrior can walk out from without one.
    indoors: Vec<u64>,
    scratch: Mutex<FloorScratch>,
    /// Searches run, seconds in all, the longest, nodes expanded in all: for the checks.
    stats: Mutex<(u32, f32, f32, u64)>,
    /// Route points a side.
    m: usize,
    /// Where each route point stands (NaN: nowhere): the grid's first, then the decks'.
    points: Vec<Vec2>,
    /// By grid point: a bit for each of `WAYS` that can be walked straight.
    ways: Vec<u8>,
    /// Steps other than to a grid neighbour, with their costs: along a deck, and on and off it.
    joins: HashMap<u32, Vec<(u32, u32)>>,
    /// By bridge: its first deck point, and how many it has.
    decks: Vec<(u32, u32)>,
    routing: Mutex<FloorScratch>,
    route_stats: Mutex<(u32, f32, f32, u64)>,
}

#[derive(Default)]
struct FloorScratch {
    /// By window cell or route point: best cost + 1 (0 for unvisited), and where it was reached
    /// from + 1.
    best: Vec<u32>,
    came: Vec<u32>,
    touched: Vec<u32>,
}

impl StreetFloor {
    /// Empty on the small maps, which have the `NavGrid`.
    pub fn new(map: &MapLayout) -> Self {
        if map.style != crate::map::MapStyle::Ncr {
            return Self::default();
        }
        let n = (2.0 * map.half_extent / CELL).ceil() as usize;
        let mut floor = Self { n, half: map.half_extent, blocked: vec![0; (n * n).div_ceil(64)], ..default() };
        floor.draw_water(map);
        for (k, f) in map.footprints.iter().enumerate().filter(|(_, f)| f.mat != Mat::Water) {
            if map.rooms_of(k).is_none() {
                floor.draw_building(map, f);
            }
        }
        for b in map.blocks.iter().filter(|b| b.collide).chain(&map.doors) {
            floor.draw_block(map, b);
        }
        for p in map.props.iter() {
            if let Some((r, h)) = p.trunk {
                floor.draw_block(map, &Block { pos: p.pos + Vec3::Y * h / 2.0, rot: Quat::IDENTITY, shape: Shape::Cylinder { radius: r, height: h }, mat: Mat::Bark, collide: true, visible: false });
            }
        }
        floor.draw_steep(map);
        // Routes go round a hollow building as round any other, so they are the same with or
        // without rooms: it is drawn solid for them, and then its rooms take its place.
        let open = floor.blocked.clone();
        for r in &map.rooms {
            floor.draw_building(map, &map.footprints[r.footprint]);
        }
        floor.lay_routes();
        if !map.rooms.is_empty() {
            floor.blocked = open;
            floor.draw_rooms(map);
        }
        floor
    }

    /// The hollow buildings' ground floors: their walls and furniture, and the cells a warrior can
    /// reach from their doors. Stairs stay blocked: there is one level here, and upstairs is the
    /// player's edge.
    fn draw_rooms(&mut self, map: &MapLayout) {
        self.indoors = vec![0; self.blocked.len()];
        for r in &map.rooms {
            for b in r.solids().iter().chain(r.stair_block().iter()) {
                self.draw_block(map, b);
            }
        }
        let mut stack = vec![];
        for r in &map.rooms {
            for p in r.ways_in() {
                if let Some(cell) = self.cell(p).and_then(|c| self.nearest_free(c, 1.0)) {
                    stack.push(cell);
                }
            }
            while let Some((i, j)) = stack.pop() {
                let k = j * self.n + i;
                if self.indoors[k / 64] >> (k % 64) & 1 != 0 || !r.contains(self.centre((i, j))) {
                    continue;
                }
                self.indoors[k / 64] |= 1 << (k % 64);
                for (di, dj) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                    let next = ((i as isize + di) as usize, (j as isize + dj) as usize);
                    if !self.is_blocked(next) {
                        stack.push(next);
                    }
                }
            }
        }
    }

    /// Whether a point is on a hollow building's ground floor, reached from its door.
    pub fn is_indoors(&self, p: Vec2) -> bool {
        !self.indoors.is_empty()
            && self.cell(p).is_some_and(|(i, j)| {
                let k = j * self.n + i;
                self.indoors[k / 64] >> (k % 64) & 1 != 0
            })
    }

    pub fn ready(&self) -> bool {
        self.n > 0
    }

    fn cell(&self, p: Vec2) -> Option<(usize, usize)> {
        let i = ((p.x + self.half) / CELL).floor();
        let j = ((p.y + self.half) / CELL).floor();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.n && (j as usize) < self.n).then_some((i as usize, j as usize))
    }

    fn centre(&self, (i, j): (usize, usize)) -> Vec2 {
        Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * CELL - Vec2::splat(self.half)
    }

    fn set(&mut self, (i, j): (usize, usize)) {
        let k = j * self.n + i;
        self.blocked[k / 64] |= 1 << (k % 64);
    }

    fn is_blocked(&self, (i, j): (usize, usize)) -> bool {
        let k = j * self.n + i;
        self.blocked[k / 64] >> (k % 64) & 1 != 0
    }

    /// The cells of a box round two corners, cut to the grid.
    fn span(&self, lo: Vec2, hi: Vec2) -> impl Iterator<Item = (usize, usize)> + use<> {
        let index = |c: f32, n: usize| (((c + self.half) / CELL).floor().max(0.0) as usize).min(n - 1);
        let n = self.n;
        let (i0, i1, j0, j1) = (index(lo.x, n), index(hi.x, n), index(lo.y, n), index(hi.y, n));
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
    }

    /// Water is for wading, not for routes: the streets' way never goes through it.
    fn draw_water(&mut self, map: &MapLayout) {
        let t = &map.terrain;
        let half_step = Vec2::splat(t.step() / 2.0);
        for i in 0..t.n {
            for j in 0..t.n {
                if t.ground[i * t.n + j] == crate::map::terrain::ground::WATER {
                    let c = Vec2::new(t.coord(i), t.coord(j));
                    for cell in self.span(c - half_step, c + half_step) {
                        self.set(cell);
                    }
                }
            }
        }
    }

    /// A building: its outline filled, and the outline's edges grown by a warrior's clearance.
    fn draw_building(&mut self, map: &MapLayout, f: &crate::map::Footprint) {
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let on_the_ground = f.base < map.terrain.height((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0) + 1.0;
        if !on_the_ground {
            return;
        }
        for cell in self.span(lo, hi) {
            if crate::map::footprint::contains(&f.pts, self.centre(cell)) {
                self.set(cell);
            }
        }
        for (a, b) in f.pts.iter().zip(f.pts.iter().cycle().skip(1)) {
            let steps = (a.distance(*b) / CELL).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let p = a.lerp(*b, k as f32 / steps as f32);
                for cell in self.span(p - Vec2::splat(CLEARANCE), p + Vec2::splat(CLEARANCE)) {
                    if self.centre(cell).distance(p) < CLEARANCE {
                        self.set(cell);
                    }
                }
            }
        }
    }

    /// Ground too steep to climb stops a warrior as a wall does.
    fn draw_steep(&mut self, map: &MapLayout) {
        let t = &map.terrain;
        let step = t.step();
        let limit = STEEPEST * step;
        for i in 0..t.n - 1 {
            for j in 0..t.n - 1 {
                let h = t.h[i * t.n + j];
                if (t.h[(i + 1) * t.n + j] - h).abs() > limit || (t.h[i * t.n + j + 1] - h).abs() > limit {
                    let c = Vec2::new(t.coord(i), t.coord(j));
                    for cell in self.span(c, c + Vec2::splat(step)) {
                        self.set(cell);
                    }
                }
            }
        }
    }

    /// A solid block, if it stands in the height a warrior walks through: over a step and under a
    /// head, as the `NavGrid`'s probe has it. A tilted block is a ramp, and is walked up.
    fn draw_block(&mut self, map: &MapLayout, b: &Block) {
        if (b.rot * Vec3::Y).y < 0.99 {
            return;
        }
        let half = match b.shape {
            Shape::Box(s) => s / 2.0,
            Shape::Cylinder { radius, height } => Vec3::new(radius, height / 2.0, radius),
            Shape::Sphere(r) => Vec3::splat(r),
        };
        let reach = (b.rot * Vec3::X * half.x).abs() + (b.rot * Vec3::Y * half.y).abs() + (b.rot * Vec3::Z * half.z).abs();
        let inverse = b.rot.inverse();
        let grown = reach.xz() + Vec2::splat(CLEARANCE);
        for cell in self.span(b.pos.xz() - grown, b.pos.xz() + grown) {
            let p = self.centre(cell);
            let floor = map.terrain.height(p.x, p.y);
            if b.pos.y + reach.y < floor + PROBE_FROM || b.pos.y - reach.y > floor + PROBE_TO {
                continue;
            }
            let q = inverse * Vec3::new(p.x - b.pos.x, 0.0, p.y - b.pos.z);
            let inside = match b.shape {
                Shape::Box(_) => q.x.abs() < half.x + CLEARANCE && q.z.abs() < half.z + CLEARANCE,
                _ => q.xz().length() < half.x + CLEARANCE,
            };
            if inside {
                self.set(cell);
            }
        }
    }

    /// Whether a warrior can stand at `p` at street level.
    pub fn walkable(&self, p: Vec2) -> bool {
        self.cell(p).is_some_and(|c| !self.is_blocked(c))
    }

    /// Lays the routes over the finished floor.
    fn lay_routes(&mut self) {
        let m = (2.0 * self.half / LANE) as usize;
        self.m = m;
        let mut points = vec![Vec2::NAN; m * m];
        for j in 0..m {
            for i in 0..m {
                let c = (Vec2::new(i as f32, j as f32) + 0.5) * LANE - Vec2::splat(self.half);
                if let Some(cell) = self.cell(c).and_then(|c| self.nearest_free(c, LANE_SNAP)) {
                    points[j * m + i] = self.centre(cell);
                }
            }
        }
        let mut ways = vec![0u8; m * m];
        let side = m as isize;
        for j in 0..side {
            for i in 0..side {
                let k = (j * side + i) as usize;
                for (w, &(di, dj, _)) in WAYS.iter().enumerate().step_by(2) {
                    let (x, z) = (i + di, j + dj);
                    if x < 0 || z < 0 || x >= side || z >= side {
                        continue;
                    }
                    let v = (z * side + x) as usize;
                    // At half-metre steps: everything drawn is at least a warrior's width across.
                    let (a, b) = (points[k], points[v]);
                    let n = (a.distance(b) / CELL).ceil() as usize;
                    if !a.is_nan() && !b.is_nan() && (0..=n).all(|s| self.walkable(a.lerp(b, s as f32 / n.max(1) as f32))) {
                        ways[k] |= 1 << w;
                        ways[v] |= 1 << (w + 1);
                    }
                }
            }
        }
        self.points = points;
        self.ways = ways;
        for s in &SPANS {
            let (len, along) = (s.a.distance(s.c), (s.c - s.a).normalize_or_zero());
            let steps = (len / LANE).ceil().max(1.0) as usize;
            let first = self.points.len() as u32;
            self.points.extend((0..=steps).map(|t| s.a.lerp(s.c, t as f32 / steps as f32)));
            self.decks.push((first, steps as u32 + 1));
            let cost = (len / steps as f32 * COST_PER_M).ceil() as u32;
            for t in 0..steps as u32 {
                self.join(first + t, first + t + 1, cost);
            }
            self.join_to_ground(first, s.a - along * DECK_APPROACH);
            self.join_to_ground(first + steps as u32, s.c + along * DECK_APPROACH);
        }
        self.keep_main_region();
    }

    fn join(&mut self, a: u32, b: u32, cost: u32) {
        self.joins.entry(a).or_default().push((b, cost));
        self.joins.entry(b).or_default().push((a, cost));
    }

    /// Joins a deck's end to the ground points that can walk straight to `foot`, or the nearest.
    fn join_to_ground(&mut self, end: u32, foot: Vec2) {
        let mut near = self.grid_points(foot, ENTRY_NEAR + 1);
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let clear: Vec<u32> = near.iter().filter(|n| self.line_clear(self.points[n.1 as usize], foot)).map(|n| n.1).collect();
        let to = if clear.is_empty() { near.first().map(|n| n.1).into_iter().collect() } else { clear };
        let from = self.points[end as usize];
        for k in to {
            let cost = (self.points[k as usize].distance(from) * COST_PER_M).ceil() as u32;
            self.join(end, k, cost);
        }
    }

    /// The grid's route points within `reach` points of `p`, with their distances from it.
    fn grid_points(&self, p: Vec2, reach: isize) -> Vec<(f32, u32)> {
        let side = self.m as isize;
        let at = ((p + Vec2::splat(self.half)) / LANE).floor();
        let (ci, cj) = (at.x as isize, at.y as isize);
        let mut out = vec![];
        for j in (cj - reach).max(0)..=(cj + reach).min(side - 1) {
            for i in (ci - reach).max(0)..=(ci + reach).min(side - 1) {
                let k = (j * side + i) as usize;
                if !self.points[k].is_nan() {
                    out.push((self.points[k].distance(p), k as u32));
                }
            }
        }
        out
    }

    /// Calls `step` with each route point `k` joins, and what that step costs.
    fn each_way(&self, k: u32, mut step: impl FnMut(u32, u32)) {
        let side = self.m as isize;
        if (k as usize) < self.ways.len() {
            let (i, j) = (k as isize % side, k as isize / side);
            for (w, &(di, dj, cost)) in WAYS.iter().enumerate() {
                if self.ways[k as usize] >> w & 1 != 0 {
                    step(((j + dj) * side + i + di) as u32, cost);
                }
            }
        }
        if let Some(more) = self.joins.get(&k) {
            for &(next, cost) in more {
                step(next, cost);
            }
        }
    }

    /// Drops every route point that doesn't join up with the most of them: a yard with no way out
    /// is no place to send a warrior.
    fn keep_main_region(&mut self) {
        let n = self.points.len();
        let mut region = vec![u32::MAX; n];
        let mut sizes: Vec<u32> = vec![];
        let mut stack = vec![];
        for seed in 0..n {
            if region[seed] != u32::MAX || self.points[seed].is_nan() {
                continue;
            }
            let id = sizes.len() as u32;
            region[seed] = id;
            stack.push(seed as u32);
            let mut size = 0;
            while let Some(k) = stack.pop() {
                size += 1;
                self.each_way(k, |next, _| {
                    if region[next as usize] == u32::MAX {
                        region[next as usize] = id;
                        stack.push(next);
                    }
                });
            }
            sizes.push(size);
        }
        let main = (0..sizes.len()).max_by_key(|k| sizes[*k]).unwrap_or(0) as u32;
        for k in 0..n {
            if region[k] != main {
                self.points[k] = Vec2::NAN;
                if k < self.ways.len() {
                    self.ways[k] = 0;
                }
                self.joins.remove(&(k as u32));
            }
        }
    }

    /// The route points kept, of all laid: for the checks.
    pub fn route_points(&self) -> (usize, usize) {
        (self.points.iter().filter(|p| !p.is_nan()).count(), self.points.len())
    }

    /// The route point a warrior at `p` joins the routes by.
    fn entry(&self, p: Vec3) -> Option<u32> {
        if let Some(bridge) = deck_under(p) {
            let (first, count) = self.decks[bridge];
            let far = |k: &u32| self.points[*k as usize].distance_squared(p.xz());
            let nearest = (first..first + count).filter(|k| !self.points[*k as usize].is_nan()).min_by(|a, b| far(a).total_cmp(&far(b)));
            if nearest.is_some() {
                return nearest;
            }
        }
        let mut near = self.grid_points(p.xz(), ENTRY_NEAR);
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.iter()
            .find(|n| self.line_clear(p.xz(), self.points[n.1 as usize]))
            .or(near.first())
            .map(|n| n.1)
            .or_else(|| self.grid_points(p.xz(), ENTRY_FAR).into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).map(|n| n.1))
    }

    /// Whether a warrior at `p` could join the routes on foot: not walled up in a yard.
    pub fn reachable(&self, p: Vec2) -> bool {
        self.walkable(p) && (self.is_indoors(p) || self.grid_points(p, ENTRY_NEAR).iter().any(|n| self.line_clear(p, self.points[n.1 as usize])))
    }

    /// The way from one point of the map to another along the routes, as points with the next one
    /// last, so it is cheap to use up. Empty when there is nothing to walk or no way.
    pub fn route(&self, from: Vec3, to: Vec3) -> Vec<Vec2> {
        let t = Instant::now();
        let (out, expanded) = self.find_route(from, to);
        let s = t.elapsed().as_secs_f32();
        let mut stats = self.route_stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.0 += 1;
        stats.1 += s;
        stats.2 = stats.2.max(s);
        stats.3 += expanded as u64;
        out
    }

    /// For the checks: routes worked out so far, their mean and longest milliseconds, mean expansions.
    pub fn route_stats(&self) -> (u32, f32, f32, f32) {
        let s = self.route_stats.lock().unwrap_or_else(|e| e.into_inner());
        (s.0, 1000.0 * s.1 / s.0.max(1) as f32, 1000.0 * s.2, s.3 as f32 / s.0.max(1) as f32)
    }

    fn find_route(&self, from: Vec3, to: Vec3) -> (Vec<Vec2>, usize) {
        if !self.ready() {
            return (vec![], 0);
        }
        let (Some(start), Some(end)) = (self.entry(from), self.entry(to)) else { return (vec![], 0) };
        let goal = self.points[end as usize];
        // Weighted a little, as `search` is: it heads for the goal rather than sweeping.
        let h = |k: u32| {
            let d = (self.points[k as usize] - goal).abs();
            (1.1 * COST_PER_M * (d.max_element() + 0.4 * d.min_element())) as u32
        };
        let mut scratch = self.routing.lock().unwrap_or_else(|e| e.into_inner());
        if scratch.best.is_empty() {
            *scratch = FloorScratch { best: vec![0; self.points.len()], came: vec![0; self.points.len()], touched: vec![] };
        }
        let FloorScratch { best, came, touched } = &mut *scratch;
        let mut open = BinaryHeap::new();
        best[start as usize] = 1;
        came[start as usize] = start + 1;
        touched.push(start);
        open.push(Reverse((h(start), Reverse(0u32), start)));
        let (mut expanded, mut found) = (0, false);
        while let Some(Reverse((_, Reverse(g), node))) = open.pop() {
            if node == end {
                found = true;
                break;
            }
            let slot = best[node as usize];
            if slot & CLOSED != 0 || slot != g + 1 {
                continue;
            }
            best[node as usize] = slot | CLOSED;
            expanded += 1;
            self.each_way(node, |next, cost| {
                let ng = g + cost;
                let slot = best[next as usize];
                if slot & CLOSED == 0 && (slot == 0 || ng + 1 < slot) {
                    if slot == 0 {
                        touched.push(next);
                    }
                    best[next as usize] = ng + 1;
                    came[next as usize] = node + 1;
                    open.push(Reverse((ng + h(next), Reverse(ng), next)));
                }
            });
        }
        let mut out = vec![];
        if found {
            let mut at = end;
            while at != start {
                out.push(self.points[at as usize]);
                at = came[at as usize] - 1;
            }
        }
        for &k in touched.iter() {
            best[k as usize] = 0;
        }
        touched.clear();
        (out, expanded)
    }

    /// The free cell nearest `c`, within `reach` metres.
    fn nearest_free(&self, c: (usize, usize), reach: f32) -> Option<(usize, usize)> {
        let (ci, cj) = (c.0 as isize, c.1 as isize);
        let n = self.n as isize;
        let free = |i: isize, j: isize| i >= 0 && j >= 0 && i < n && j < n && !self.is_blocked((i as usize, j as usize));
        (0..=cells(reach)).find_map(|k| {
            let ring = (-k..=k).flat_map(|d| [(ci + d, cj - k), (ci + d, cj + k), (ci - k, cj + d), (ci + k, cj + d)]);
            ring.filter(|&(i, j)| free(i, j)).min_by_key(|&(i, j)| (i - ci).pow(2) + (j - cj).pow(2)).map(|(i, j)| (i as usize, j as usize))
        })
    }

    /// The street spot nearest `p`, within `reach` metres.
    pub fn snap(&self, p: Vec2, reach: f32) -> Option<Vec2> {
        let c = self.cell(p.clamp(Vec2::splat(-self.half), Vec2::splat(self.half - 0.01)))?;
        self.nearest_free(c, reach).map(|c| self.centre(c))
    }

    /// Whether a warrior can walk the straight line between two points.
    pub fn line_clear(&self, a: Vec2, b: Vec2) -> bool {
        let steps = (a.distance(b) / (CELL * 0.25)).ceil() as usize;
        (0..=steps).all(|s| self.walkable(a.lerp(b, s as f32 / steps.max(1) as f32)))
    }

    /// Whether the straight line between two cells crosses only free cells.
    fn clear(&self, a: (usize, usize), b: (usize, usize)) -> bool {
        self.line_clear(self.centre(a), self.centre(b))
    }

    /// A walk from `from` to (near) `to` round whatever is between them, as corner points, or None
    /// when there is no way within the window. Both must be within about 100 m of each other.
    pub fn path(&self, from: Vec2, to: Vec2) -> Option<Vec<Vec2>> {
        let t = Instant::now();
        let (out, expanded) = self.search(from, to);
        let s = t.elapsed().as_secs_f32();
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.0 += 1;
        stats.1 += s;
        stats.2 = stats.2.max(s);
        stats.3 += expanded as u64;
        out
    }

    /// For the checks: searches run so far, their mean and longest milliseconds, mean expansions.
    pub fn stats(&self) -> (u32, f32, f32, f32) {
        let s = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        (s.0, 1000.0 * s.1 / s.0.max(1) as f32, 1000.0 * s.2, s.3 as f32 / s.0.max(1) as f32)
    }

    fn search(&self, from: Vec2, to: Vec2) -> (Option<Vec<Vec2>>, usize) {
        if !self.ready() {
            return (None, 0);
        }
        let start = self.cell(from).and_then(|c| self.nearest_free(c, NEAR_START));
        let goal = self.cell(to).and_then(|c| self.nearest_free(c, NEAR_GOAL));
        let (Some(start), Some(goal)) = (start, goal) else { return (None, 0) };
        // The window, centred between the ends and kept on the map.
        let w = WINDOW as isize;
        let origin = |a: usize, b: usize| ((a + b) as isize / 2 - w / 2).clamp(0, self.n as isize - w);
        let (ox, oz) = (origin(start.0, goal.0), origin(start.1, goal.1));
        let local = |c: (usize, usize)| {
            let (x, z) = (c.0 as isize - ox, c.1 as isize - oz);
            (x >= 0 && z >= 0 && x < w && z < w).then_some((z * w + x) as u32)
        };
        let world = |k: u32| (((k as isize % w) + ox) as usize, ((k as isize / w) + oz) as usize);
        let (Some(first), Some(last)) = (local(start), local(goal)) else { return (None, 0) };
        let h = |c: (usize, usize)| {
            let (dx, dz) = (c.0.abs_diff(goal.0) as u32, c.1.abs_diff(goal.1) as u32);
            // Weighted, as in `NavGrid::search`: it heads for the goal rather than sweeping.
            14 * dx.max(dz) + 6 * dx.min(dz)
        };
        let mut scratch = self.scratch.lock().unwrap_or_else(|e| e.into_inner());
        if scratch.best.is_empty() {
            *scratch = FloorScratch { best: vec![0; WINDOW * WINDOW], came: vec![0; WINDOW * WINDOW], touched: vec![] };
        }
        let FloorScratch { best, came, touched } = &mut *scratch;
        let mut open = BinaryHeap::new();
        best[first as usize] = 1;
        came[first as usize] = first + 1;
        touched.push(first);
        open.push(Reverse((h(start), Reverse(0u32), first)));
        let mut expanded = 0;
        let mut found = false;
        while let Some(Reverse((_, Reverse(g), node))) = open.pop() {
            if node == last {
                found = true;
                break;
            }
            if expanded >= MAX_EXPANDED {
                break;
            }
            let slot = best[node as usize];
            if slot & CLOSED != 0 || slot != g + 1 {
                continue;
            }
            best[node as usize] = slot | CLOSED;
            expanded += 1;
            let (ci, cj) = world(node);
            let (ci, cj) = (ci as isize, cj as isize);
            let free = |i: isize, j: isize| {
                i >= 0 && j >= 0 && (i as usize) < self.n && (j as usize) < self.n && !self.is_blocked((i as usize, j as usize))
            };
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (ni, nj) = (ci + di, cj + dj);
                if !free(ni, nj) || (di != 0 && dj != 0 && !(free(ci + di, cj) && free(ci, cj + dj))) {
                    continue;
                }
                let cell = (ni as usize, nj as usize);
                let Some(next) = local(cell) else { continue };
                let ng = g + if di != 0 && dj != 0 { 14 } else { 10 };
                let slot = best[next as usize];
                if slot & CLOSED == 0 && (slot == 0 || ng + 1 < slot) {
                    if slot == 0 {
                        touched.push(next);
                    }
                    best[next as usize] = ng + 1;
                    came[next as usize] = node + 1;
                    open.push(Reverse((ng + h(cell), Reverse(ng), next)));
                }
            }
        }
        let mut way = vec![goal];
        if found {
            let mut at = last;
            while came[at as usize] != at + 1 {
                at = came[at as usize] - 1;
                way.push(world(at));
            }
            way.reverse();
        }
        for &k in touched.iter() {
            best[k as usize] = 0;
        }
        touched.clear();
        drop(scratch);
        if !found {
            return (None, expanded);
        }
        // Keep only the corners: from each, the farthest cell still in a straight line.
        let mut out = vec![];
        let mut at = 0;
        while at + 1 < way.len() {
            let mut far = at + 1;
            let mut stride = 16;
            while stride > 0 {
                if far + stride < way.len() && self.clear(way[at], way[far + stride]) {
                    far += stride;
                } else {
                    stride /= 2;
                }
            }
            out.push(self.centre(way[far]));
            at = far;
        }
        if out.is_empty() {
            out.push(self.centre(goal));
        }
        (Some(out), expanded)
    }
}

/// For a warrior on an upper floor or a roof of a hollow building: where to walk next, on the way
/// down its stairs. `roof_edge` would strand a bot here: a parapet stops it, and a window's sill.
pub fn way_down(layout: &MapLayout, pos: Vec3) -> Option<Vec2> {
    if pos.y - HALF_HEIGHT - layout.terrain.height(pos.x, pos.z) < 0.35 {
        return None;
    }
    layout.rooms.iter().find(|r| r.corner.distance_squared(pos.xz()) < 3600.0 && r.contains(pos.xz())).and_then(|r| r.way_down(pos))
}

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavGrid>().add_systems(FixedUpdate, sample.before(super::SimSet::Brains));
    }
}

fn sample(mut grid: ResMut<NavGrid>, swaps: Res<super::MapSwaps>, layout: Res<MapLayout>, spatial: SpatialQuery) {
    // By the swap count, not `layout.is_changed()`: every rematch looks like a change to the layout
    // (`MapSwaps` says why), and the grid would be thrown away for nothing.
    if swaps.is_changed() && !swaps.is_added() {
        *grid = NavGrid::default();
    }
    if grid.ready() {
        return;
    }
    // The NCR arena is 2.2 km across and has its own street router (crate::map::navigation).
    if layout.style == crate::map::MapStyle::Ncr {
        return;
    }
    // Colliders reach the query pipeline after a physics step or two.
    if grid.wait < 3 {
        grid.wait += 1;
        return;
    }
    if grid.n == 0 {
        grid.half = layout.half_extent;
        grid.n = (2.0 * layout.half_extent / CELL).ceil() as usize;
        grid.stairs = find_stairs(&layout.blocks);
        // The street, and a level for each height the stairs climb to.
        grid.floors = vec![0.0];
        for k in 0..grid.stairs.len() {
            let y = grid.stairs[k].head.y;
            if !grid.floors.iter().skip(1).any(|f| (f - y).abs() < 0.3) {
                grid.floors.push(y);
            }
        }
        grid.free = vec![vec![false; grid.n * grid.n]; grid.floors.len()];
    }
    let probe = Collider::cuboid(PROBE_W, PROBE_TO - PROBE_FROM, PROBE_W);
    let lift = (PROBE_TO + PROBE_FROM) / 2.0;
    let filter = SpatialQueryFilter::from_mask(solid_mask());
    let n = grid.n;
    let first = grid.rows_done;
    let last = (first + ROWS_PER_TICK).min(n);
    let floors = grid.floors.clone();
    for j in first..last {
        let z = (j as f32 + 0.5) * CELL - grid.half;
        // Buildings standing on the ground whose outline spans this row: a box inside a mesh
        // collider touches nothing, so their insides are ruled out by outline.
        let row: Vec<&crate::map::Footprint> = layout
            .footprints
            .iter()
            .filter(|f| f.mat != Mat::Water && f.pts.iter().any(|p| p.y <= z) && f.pts.iter().any(|p| p.y >= z))
            .collect();
        for i in 0..n {
            let x = (i as f32 + 0.5) * CELL - grid.half;
            let ground = layout.terrain.height(x, z);
            let p = Vec2::new(x, z);
            let touches = |y: f32| !spatial.shape_intersections(&probe, Vec3::new(x, y + lift, z), Quat::IDENTITY, &filter).is_empty();
            let inside = row.iter().any(|f| f.base < ground + 1.0 && crate::map::footprint::contains(&f.pts, p));
            grid.free[0][j * n + i] = !inside && !touches(ground);
            for (level, &y) in floors.iter().enumerate().skip(1) {
                // A floor to stand on (the stair well has none), with room above it.
                let floor = spatial.cast_ray(Vec3::new(x, y + 1.0, z), Dir3::NEG_Y, 1.3, true, &filter).is_some_and(|hit| hit.distance > 0.0);
                grid.free[level][j * n + i] = floor && !inside && !touches(y);
            }
        }
    }
    grid.rows_done = last;
    if grid.ready() {
        grid.finish();
        let open = grid.free[0].iter().filter(|f| **f).count();
        let (joined, stairs) = grid.stair_count();
        info!("nav grid {n}x{n}: {:.0}% walkable, {joined}/{stairs} stairs", 100.0 * open as f32 / (n * n) as f32);
    }
}
