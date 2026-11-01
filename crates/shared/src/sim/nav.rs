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
use std::collections::BinaryHeap;
use std::sync::Mutex;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{HALF_HEIGHT, solid_mask};
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
        let t = std::time::Instant::now();
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

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavGrid>().add_systems(FixedUpdate, sample.before(super::SimSet::Brains));
    }
}

fn sample(mut grid: ResMut<NavGrid>, layout: Res<MapLayout>, spatial: SpatialQuery) {
    if layout.is_changed() {
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
