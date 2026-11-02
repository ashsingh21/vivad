//! Ground routes across the NCR map: a grid over the whole square, built from its buildings, its
//! water and its bridges, for the long walk between districts. Roofs are human routes.
use super::terrain::ground;
use super::{MapLayout, MapStyle, Mat, Shape, compose};
use bevy::prelude::*;
use std::{cmp::Reverse, collections::BinaryHeap};
/// Metres between cells: a route, not a footpath. Inside a street the bot steers for itself.
const STEP: f32 = 8.0;
#[derive(Resource, Default)]
pub struct StreetNav {
    open: Vec<bool>,
    n: usize,
    origin: f32,
}
impl StreetNav {
    pub fn new(map: &MapLayout) -> Self {
        if map.style != MapStyle::Ncr {
            return Self::default();
        }
        let n = (2.0 * map.half_extent / STEP) as usize;
        let origin = -map.half_extent + STEP / 2.0;
        let mut nav = Self { open: vec![true; n * n], n, origin };
        // Water is waded by people in a hurry, not routed through.
        for k in 0..n * n {
            let p = nav.point(k);
            nav.open[k] = map.terrain.ground_at(p.x, p.y) != Some(ground::WATER);
        }
        for f in map.footprints.iter().filter(|f| f.mat != Mat::Water) {
            let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
            let (min, max) = (nav.cell(lo), nav.cell(hi));
            for i in min.0..=max.0 {
                for j in min.1..=max.1 {
                    let k = i * n + j;
                    if super::footprint::contains(&f.pts, nav.point(k)) {
                        nav.open[k] = false;
                    }
                }
            }
        }
        for b in map.blocks.iter().filter(|b| b.collide) {
            // Slopes, terrain and structures wholly overhead aren't ground obstructions.
            if (b.rot * Vec3::Y).y < 0.99 {
                continue;
            }
            let h = match b.shape {
                Shape::Box(s) => s * 0.5,
                Shape::Cylinder { radius, height } => Vec3::new(radius, height / 2.0, radius),
                Shape::Sphere(r) => Vec3::splat(r),
            };
            let floor = map.terrain.height(b.pos.x, b.pos.z);
            if b.pos.y + h.y < floor + 0.7 || b.pos.y - h.y > floor + 2.0 {
                continue;
            }
            // A thin wall does not fill a cell: only what a walker could not slip round.
            if h.x.max(h.z) < STEP || h.x.min(h.z) < 0.6 {
                continue;
            }
            let ext = (b.rot * Vec3::X * h.x).abs() + (b.rot * Vec3::Z * h.z).abs();
            let (min, max) = (nav.cell(b.pos.xz() - ext.xz()), nav.cell(b.pos.xz() + ext.xz()));
            for i in min.0..=max.0 {
                for j in min.1..=max.1 {
                    let k = i * n + j;
                    let q = b.rot.inverse() * (nav.point(k).extend(b.pos.y).xzy() - b.pos);
                    if q.x.abs() < h.x + 0.8 && q.z.abs() < h.z + 0.8 {
                        nav.open[k] = false;
                    }
                }
            }
        }
        // Bridges are the way over: their centre lanes stay open whatever lies under them.
        for s in &compose::SPANS {
            let steps = (s.a.distance(s.c) / (STEP / 2.0)) as usize;
            for t in 0..=steps {
                let (i, j) = nav.cell(s.a.lerp(s.c, t as f32 / steps as f32));
                nav.open[i * n + j] = true;
            }
        }
        nav
    }
    fn cell(&self, p: Vec2) -> (usize, usize) {
        let v = ((p - Vec2::splat(self.origin)) / STEP).round().clamp(Vec2::ZERO, Vec2::splat((self.n - 1) as f32));
        (v.x as usize, v.y as usize)
    }
    fn point(&self, k: usize) -> Vec2 {
        Vec2::new(self.origin + (k / self.n) as f32 * STEP, self.origin + (k % self.n) as f32 * STEP)
    }
    fn nearest(&self, p: Vec2) -> Option<usize> {
        let (i, j) = self.cell(p);
        let n = self.n;
        for r in 0..12 {
            for x in i.saturating_sub(r)..=(i + r).min(n - 1) {
                for z in j.saturating_sub(r)..=(j + r).min(n - 1) {
                    let k = x * n + z;
                    if self.open[k] {
                        return Some(k);
                    }
                }
            }
        }
        None
    }
    pub fn active(&self) -> bool {
        !self.open.is_empty()
    }
    /// Reversed path: the next waypoint is the last element, allowing cheap consumption.
    pub fn route(&self, from: Vec3, to: Vec3) -> Vec<Vec2> {
        if !self.active() {
            return vec![];
        }
        let (Some(start), Some(end)) = (self.nearest(from.xz()), self.nearest(to.xz())) else {
            return vec![];
        };
        let n = self.n;
        let mut cost = vec![u32::MAX; n * n];
        let mut prev = vec![usize::MAX; n * n];
        let mut heap = BinaryHeap::new();
        cost[start] = 0;
        heap.push(Reverse((0, start)));
        while let Some(Reverse((_, k))) = heap.pop() {
            if k == end {
                break;
            }
            let (i, j) = (k / n, k % n);
            for (x, z) in [(i.wrapping_sub(1), j), (i + 1, j), (i, j.wrapping_sub(1)), (i, j + 1)] {
                if x >= n || z >= n {
                    continue;
                }
                let v = x * n + z;
                if !self.open[v] {
                    continue;
                }
                let next = cost[k] + 1;
                if next < cost[v] {
                    cost[v] = next;
                    prev[v] = k;
                    let h = x.abs_diff(end / n) + z.abs_diff(end % n);
                    heap.push(Reverse((next + h as u32, v)));
                }
            }
        }
        if cost[end] == u32::MAX {
            return vec![];
        }
        let mut out = vec![];
        let mut k = end;
        while k != start {
            out.push(self.point(k));
            k = prev[k];
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Every district is reachable from every other on foot without wading.
    #[test]
    fn districts_connected_without_wading() {
        let m = compose::generate(7);
        let nav = StreetNav::new(&m);
        let cp = compose::PLACES[0].at;
        for d in &compose::PLACES[1..] {
            let (a, b) = (Vec3::new(cp.x, 0.0, cp.y), Vec3::new(d.at.x, 0.0, d.at.y));
            let route = nav.route(a, b);
            assert!(!route.is_empty(), "no way to {}", d.name);
            let on_water = route.iter().filter(|p| m.terrain.ground_at(p.x, p.y) == Some(ground::WATER)).count();
            let bridge = route.iter().filter(|p| compose::SPANS.iter().any(|s| p.distance(s.a) + p.distance(s.c) < s.a.distance(s.c) + 12.0)).count();
            assert!(on_water <= bridge, "{} wades {on_water} cells with {bridge} on bridges", d.name);
        }
    }
}
