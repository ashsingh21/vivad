//! Where a warrior can walk at street level, and the way there around buildings.
//!
//! Bots used to walk straight at their goal, which on a city map meant pressing into Connaught
//! Place's ring (or a lane's dead end) until the Aandhi caught them. Here the map is sampled once
//! into a 1 m grid (a cell is free when a warrior-width box at chest height there touches nothing
//! solid and it isn't inside a building's outline), a few rows a tick so no single tick stalls,
//! and A* on it gives each bot a path, trimmed to its corners.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::solid_mask;
use crate::map::{Mat, MapLayout};

pub const CELL: f32 = 1.0;
/// Rows sampled per tick: a 500 m map takes about 4 s, while the vimana is still flying.
const ROWS_PER_TICK: usize = 8;
/// Nodes A* may expand before it gives up (an unreachable goal floods its whole region).
const BUDGET: usize = 40_000;

#[derive(Resource, Default)]
pub struct NavGrid {
    n: usize,
    half: f32,
    free: Vec<bool>,
    rows_done: usize,
    /// Ticks waited for the physics world to settle before sampling.
    wait: u32,
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

    fn is_free(&self, i: isize, j: isize) -> bool {
        i >= 0 && j >= 0 && (i as usize) < self.n && (j as usize) < self.n && self.free[j as usize * self.n + i as usize]
    }

    pub fn walkable(&self, p: Vec2) -> bool {
        self.cell(p).is_some_and(|(i, j)| self.free[j * self.n + i])
    }

    /// The free cell nearest `c`, within `r` cells.
    fn nearest_free(&self, c: (usize, usize), r: isize) -> Option<(usize, usize)> {
        let (ci, cj) = (c.0 as isize, c.1 as isize);
        (0..=r).find_map(|k| {
            let ring = (-k..=k).flat_map(|d| [(ci + d, cj - k), (ci + d, cj + k), (ci - k, cj + d), (ci + k, cj + d)]);
            ring.filter(|&(i, j)| self.is_free(i, j)).min_by_key(|&(i, j)| (i - ci).pow(2) + (j - cj).pow(2))
        })
        .map(|(i, j)| (i as usize, j as usize))
    }

    /// Whether the straight line between two cells crosses only free cells.
    fn clear(&self, a: (usize, usize), b: (usize, usize)) -> bool {
        let (pa, pb) = (self.centre(a), self.centre(b));
        let steps = (pa.distance(pb) / (CELL * 0.25)).ceil() as usize;
        (0..=steps).all(|s| {
            let p = pa.lerp(pb, s as f32 / steps.max(1) as f32);
            // The warrior's width: either side of the line too.
            let side = (pb - pa).normalize_or_zero().perp() * 0.3;
            self.walkable(p) && self.walkable(p + side) && self.walkable(p - side)
        })
    }

    /// A walk from `from` to (near) `to` along free cells, as corner points ending at the goal.
    pub fn path(&self, from: Vec2, to: Vec2) -> Result<Vec<Vec2>, NoPath> {
        if !self.ready() {
            return Err(NoPath::Here);
        }
        let start = self.cell(from).and_then(|c| self.nearest_free(c, 3)).ok_or(NoPath::Here)?;
        let goal = self.cell(to).and_then(|c| self.nearest_free(c, 12)).ok_or(NoPath::There)?;
        let n = self.n;
        let idx = |(i, j): (usize, usize)| j * n + i;
        let h = |(i, j): (usize, usize)| {
            let (dx, dy) = ((i as isize - goal.0 as isize).abs(), (j as isize - goal.1 as isize).abs());
            // Octile distance, in tenths of a cell.
            (10 * dx.max(dy) + 4 * dx.min(dy)) as u32
        };
        let mut cost = std::collections::HashMap::<usize, u32>::new();
        let mut came = std::collections::HashMap::<usize, usize>::new();
        let mut open = BinaryHeap::new();
        cost.insert(idx(start), 0);
        open.push(Reverse((h(start), 0u32, start)));
        let mut expanded = 0;
        let mut found = false;
        while let Some(Reverse((_, g, c))) = open.pop() {
            if c == goal {
                found = true;
                break;
            }
            if cost.get(&idx(c)).is_some_and(|&best| best < g) {
                continue;
            }
            expanded += 1;
            if expanded > BUDGET {
                return Err(NoPath::There);
            }
            let (ci, cj) = (c.0 as isize, c.1 as isize);
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (ni, nj) = (ci + di, cj + dj);
                if !self.is_free(ni, nj) {
                    continue;
                }
                // No cutting a wall's corner diagonally.
                if di != 0 && dj != 0 && !(self.is_free(ci + di, cj) && self.is_free(ci, cj + dj)) {
                    continue;
                }
                let next = (ni as usize, nj as usize);
                let ng = g + if di != 0 && dj != 0 { 14 } else { 10 };
                if cost.get(&idx(next)).is_none_or(|&best| ng < best) {
                    cost.insert(idx(next), ng);
                    came.insert(idx(next), idx(c));
                    open.push(Reverse((ng + h(next), ng, next)));
                }
            }
        }
        if !found {
            // A pocket this small is the grid's mistake about where we stand, not the goal's.
            return Err(if expanded < 400 { NoPath::Here } else { NoPath::There });
        }
        let mut cells = vec![goal];
        let mut k = idx(goal);
        while let Some(&p) = came.get(&k) {
            cells.push((p % n, p / n));
            k = p;
        }
        cells.reverse();
        // Keep only the corners: from each kept cell, the farthest one still in a straight line.
        let mut out = vec![];
        let mut at = 0;
        while at + 1 < cells.len() {
            let mut far = at + 1;
            while far + 1 < cells.len() && self.clear(cells[at], cells[far + 1]) {
                far += 1;
            }
            out.push(self.centre(cells[far]));
            at = far;
        }
        if let Some(last) = out.last_mut() {
            // End on the goal itself when it is walkable (a chest's front, not its cell's middle).
            if self.cell(to) == Some(goal) {
                *last = to;
            }
        }
        Ok(out)
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
    // Colliders reach the query pipeline after a physics step or two.
    if grid.wait < 3 {
        grid.wait += 1;
        return;
    }
    if grid.n == 0 {
        grid.half = layout.half_extent;
        grid.n = (2.0 * layout.half_extent / CELL).ceil() as usize;
        grid.free = vec![false; grid.n * grid.n];
    }
    let probe = Collider::cuboid(0.6, 0.9, 0.6);
    let filter = SpatialQueryFilter::from_mask(solid_mask());
    let n = grid.n;
    let first = grid.rows_done;
    let last = (first + ROWS_PER_TICK).min(n);
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
            let inside = row.iter().any(|f| f.base < ground + 1.0 && crate::map::footprint::contains(&f.pts, p));
            let free = !inside && spatial.shape_intersections(&probe, Vec3::new(x, ground + 1.0, z), Quat::IDENTITY, &filter).is_empty();
            grid.free[j * n + i] = free;
        }
    }
    grid.rows_done = last;
    if grid.ready() {
        let open = grid.free.iter().filter(|f| **f).count();
        info!("nav grid {n}x{n}: {:.0}% walkable", 100.0 * open as f32 / (n * n) as f32);
    }
}
