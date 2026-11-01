//! Conservative ground-layer routing for the NCR blockout. Roofs and metro are human routes.
use super::{MapLayout, MapStyle, Shape, ncr};
use bevy::prelude::*;
use std::{cmp::Reverse, collections::BinaryHeap};
const STEP: f32 = 5.0 * ncr::WORLD_SCALE;
const N: usize = 199;
const ORIGIN: f32 = -ncr::HALF + STEP;
#[derive(Resource, Default)]
pub struct StreetNav {
    open: Vec<bool>,
}
impl StreetNav {
    pub fn new(map: &MapLayout) -> Self {
        if map.style != MapStyle::Ncr {
            return Self::default();
        }
        let mut open = vec![true; N * N];
        for (k, v) in open.iter_mut().enumerate() {
            let p = Self::point(k);
            // Route over bridges; the shallow water remains a slower player alternative.
            *v = ncr::river_distance(p) > 8.0 || ncr::BRIDGES.iter().any(|z| (p.y - z).abs() < 5.5);
            if p.x.abs() > 130.0 * ncr::WORLD_SCALE
                && p.x.abs() < 175.0 * ncr::WORLD_SCALE
                && p.y.abs() < 17.0 * ncr::WORLD_SCALE
            {
                *v = false;
            }
            // Central bridge is above the metro excavation.
            if p.x.abs() < 88.0 * ncr::WORLD_SCALE && p.y.abs() < 5.5 * ncr::WORLD_SCALE {
                *v = true;
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
            if b.pos.y + h.y < 0.7 || b.pos.y - h.y > 2.0 {
                continue;
            }
            let ext = (b.rot * Vec3::X * h.x).abs() + (b.rot * Vec3::Z * h.z).abs();
            let min = Self::cell(b.pos.xz() - ext.xz() - Vec2::splat(1.0));
            let max = Self::cell(b.pos.xz() + ext.xz() + Vec2::splat(1.0));
            for i in min.0..=max.0 {
                for j in min.1..=max.1 {
                    let k = i * N + j;
                    let p = Self::point(k);
                    let q = b.rot.inverse() * (p.extend(b.pos.y).xzy() - b.pos);
                    if q.x.abs() < h.x + 0.8 && q.z.abs() < h.z + 0.8 {
                        open[k] = false;
                    }
                }
            }
        }
        // Bridge slabs are support, not walls. Only central 8 m are used for routing.
        for z in ncr::BRIDGES {
            for i in 0..N {
                for j in 0..N {
                    let p = Self::point(i * N + j);
                    if p.x.abs() < 90.0 * ncr::WORLD_SCALE && (p.y - z).abs() < 4.0 * ncr::WORLD_SCALE {
                        open[i * N + j] = true;
                    }
                }
            }
        }
        Self { open }
    }
    fn cell(p: Vec2) -> (usize, usize) {
        let v = ((p - Vec2::splat(ORIGIN)) / STEP)
            .round()
            .clamp(Vec2::ZERO, Vec2::splat((N - 1) as f32));
        (v.x as usize, v.y as usize)
    }
    fn point(k: usize) -> Vec2 {
        Vec2::new(
            ORIGIN + (k / N) as f32 * STEP,
            ORIGIN + (k % N) as f32 * STEP,
        )
    }
    fn nearest(&self, p: Vec2) -> Option<usize> {
        let (i, j) = Self::cell(p);
        for r in 0..9 {
            for x in i.saturating_sub(r)..=(i + r).min(N - 1) {
                for z in j.saturating_sub(r)..=(j + r).min(N - 1) {
                    let k = x * N + z;
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
        let mut cost = vec![u32::MAX; N * N];
        let mut prev = vec![usize::MAX; N * N];
        let mut heap = BinaryHeap::new();
        cost[start] = 0;
        heap.push(Reverse((0, start)));
        while let Some(Reverse((_, k))) = heap.pop() {
            if k == end {
                break;
            }
            let (i, j) = (k / N, k % N);
            for (x, z) in [
                (i.wrapping_sub(1), j),
                (i + 1, j),
                (i, j.wrapping_sub(1)),
                (i, j + 1),
            ] {
                if x >= N || z >= N {
                    continue;
                }
                let v = x * N + z;
                if !self.open[v] {
                    continue;
                }
                let next = cost[k] + 1;
                if next < cost[v] {
                    cost[v] = next;
                    prev[v] = k;
                    let h = x.abs_diff(end / N) + z.abs_diff(end % N);
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
            out.push(Self::point(k));
            k = prev[k];
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn districts_connected_without_wading() {
        let m = ncr::generate(7);
        let nav = StreetNav::new(&m);
        for (a, b) in [
            (Vec3::new(-100.0, 0.0, -355.0), Vec3::new(90.0, 0.0, 140.0)),
            (Vec3::new(-390.0, 0.0, 58.0), Vec3::new(90.0, 0.0, -330.0)),
        ] {
            let route = nav.route(a, b);
            assert!(!route.is_empty());
            for p in route {
                if ncr::river_distance(p) < 1.0 {
                    assert!(ncr::BRIDGES.iter().any(|z| (p.y - z).abs() < 6.0 * ncr::WORLD_SCALE));
                }
            }
        }
    }
}
