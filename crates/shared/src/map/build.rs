//! Construction helpers: walls with openings, stairs (visual steps + ramp collider), rooms, domes.

use bevy::prelude::*;

use super::{Block, ChestKind, ChestSpawn, Mat, MapLayout, Shape};

pub const FLOOR_H: f32 = 3.4;
pub const WALL_T: f32 = 0.35;

/// A local placement frame: origin plus yaw (Y rotation).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Vec3,
    pub yaw: f32,
}

impl Frame {
    pub fn new(origin: Vec3, yaw: f32) -> Self {
        Self { origin, yaw }
    }
    pub fn rot(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }
    pub fn p(&self, local: Vec3) -> Vec3 {
        self.origin + self.rot() * local
    }
    pub fn child(&self, local: Vec3, yaw: f32) -> Frame {
        Frame { origin: self.p(local), yaw: self.yaw + yaw }
    }
}

/// An opening in a wall, measured along the wall from its start.
#[derive(Clone, Copy, Debug)]
pub struct Opening {
    pub at: f32,
    pub width: f32,
    pub bottom: f32,
    pub top: f32,
}

impl Opening {
    pub fn door(at: f32) -> Self {
        Self { at, width: 1.4, bottom: 0.0, top: 2.4 }
    }
    pub fn window(at: f32) -> Self {
        Self { at, width: 1.2, bottom: 1.0, top: 2.2 }
    }
    pub fn arch(at: f32, width: f32, top: f32) -> Self {
        Self { at, width, bottom: 0.0, top }
    }
}

pub struct Builder<'a> {
    pub map: &'a mut MapLayout,
}

impl<'a> Builder<'a> {
    pub fn new(map: &'a mut MapLayout) -> Self {
        Self { map }
    }

    pub fn push(&mut self, pos: Vec3, rot: Quat, shape: Shape, mat: Mat, collide: bool, visible: bool) {
        self.map.blocks.push(Block { pos, rot, shape, mat, collide, visible });
    }

    /// Box with its centre at `local` in frame `f`.
    pub fn bx(&mut self, f: Frame, local: Vec3, size: Vec3, mat: Mat) {
        if size.min_element() <= 0.001 {
            return;
        }
        self.push(f.p(local), f.rot(), Shape::Box(size), mat, true, true);
    }

    /// Box spanning `min..max` corners in frame `f`.
    pub fn span(&mut self, f: Frame, min: Vec3, max: Vec3, mat: Mat) {
        self.bx(f, (min + max) * 0.5, max - min, mat);
    }

    pub fn cyl(&mut self, f: Frame, base: Vec3, radius: f32, height: f32, mat: Mat) {
        self.push(f.p(base + Vec3::Y * height * 0.5), f.rot(), Shape::Cylinder { radius, height }, mat, true, true);
    }

    pub fn sphere(&mut self, f: Frame, center: Vec3, radius: f32, mat: Mat, collide: bool) {
        self.push(f.p(center), f.rot(), Shape::Sphere(radius), mat, collide, true);
    }

    /// Straight wall from `a` to `b` (local XZ at base height `y0`) with openings.
    pub fn wall(&mut self, f: Frame, a: Vec2, b: Vec2, y0: f32, height: f32, thick: f32, mat: Mat, openings: &[Opening]) {
        let dir = b - a;
        let len = dir.length();
        if len < 0.01 {
            return;
        }
        let yaw = (-dir.y).atan2(dir.x);
        let wf = f.child(Vec3::new(a.x, y0, a.y), yaw);
        let mut ops: Vec<Opening> = openings.to_vec();
        ops.sort_by(|p, q| p.at.total_cmp(&q.at));
        let mut cursor = 0.0;
        for o in &ops {
            let s = (o.at - o.width / 2.0).max(cursor);
            let e = (o.at + o.width / 2.0).min(len);
            if s > cursor {
                self.span(wf, Vec3::new(cursor, 0.0, -thick / 2.0), Vec3::new(s, height, thick / 2.0), mat);
            }
            if o.bottom > 0.0 {
                self.span(wf, Vec3::new(s, 0.0, -thick / 2.0), Vec3::new(e, o.bottom, thick / 2.0), mat);
            }
            if o.top < height {
                self.span(wf, Vec3::new(s, o.top, -thick / 2.0), Vec3::new(e, height, thick / 2.0), mat);
            }
            cursor = e;
        }
        if cursor < len {
            self.span(wf, Vec3::new(cursor, 0.0, -thick / 2.0), Vec3::new(len, height, thick / 2.0), mat);
        }
    }

    /// Stairs climbing along local +X from `start` (bottom front centre) by `rise` over `run`.
    /// Visual steps without collision, plus one invisible ramp collider for smooth movement.
    pub fn stairs(&mut self, f: Frame, start: Vec3, run: f32, rise: f32, width: f32, mat: Mat) {
        let steps = ((rise / 0.22).round() as i32).max(1);
        let step_run = run / steps as f32;
        let step_rise = rise / steps as f32;
        for i in 0..steps {
            let x0 = start.x + i as f32 * step_run;
            let top = start.y + (i + 1) as f32 * step_rise;
            let size = Vec3::new(step_run, top - start.y, width);
            let c = Vec3::new(x0 + step_run / 2.0, start.y + size.y / 2.0, start.z);
            self.push(f.p(c), f.rot(), Shape::Box(size), mat, false, true);
        }
        let len = (run * run + rise * rise).sqrt();
        let pitch = rise.atan2(run);
        let thick = 0.3;
        let mid = start + Vec3::new(run / 2.0, rise / 2.0, 0.0);
        // Offset the ramp down so its top surface runs along the step nosings.
        let normal = Vec3::new(-pitch.sin(), pitch.cos(), 0.0);
        let rot = f.rot() * Quat::from_rotation_z(pitch);
        self.push(f.p(mid - normal * thick / 2.0), rot, Shape::Box(Vec3::new(len, thick, width)), mat, true, false);
    }

    /// Plain ramp (visible) from `start` along +X.
    pub fn ramp(&mut self, f: Frame, start: Vec3, run: f32, rise: f32, width: f32, thick: f32, mat: Mat) {
        let len = (run * run + rise * rise).sqrt();
        let pitch = rise.atan2(run);
        let normal = Vec3::new(-pitch.sin(), pitch.cos(), 0.0);
        let mid = start + Vec3::new(run / 2.0, rise / 2.0, 0.0);
        let rot = f.rot() * Quat::from_rotation_z(pitch);
        self.push(f.p(mid - normal * thick / 2.0), rot, Shape::Box(Vec3::new(len, thick, width)), mat, true, true);
    }

    /// Slab with a rectangular hole (for stair wells). Rects are (min, max) in local XZ.
    pub fn slab_with_hole(&mut self, f: Frame, min: Vec2, max: Vec2, y: f32, thick: f32, hole: Option<(Vec2, Vec2)>, mat: Mat) {
        let rects = match hole {
            None => vec![(min, max)],
            Some((hmin, hmax)) => subtract_rect((min, max), (hmin, hmax)),
        };
        for (a, b) in rects {
            self.span(f, Vec3::new(a.x, y - thick, a.y), Vec3::new(b.x, y, b.y), mat);
        }
    }

    pub fn door(&mut self, f: Frame, a: Vec2, b: Vec2, at: f32, y0: f32) {
        let dir = (b - a).normalize();
        let yaw = (-dir.y).atan2(dir.x);
        let c = a + dir * at;
        let df = f.child(Vec3::new(c.x, y0, c.y), yaw);
        self.map.doors.push(Block {
            pos: df.p(Vec3::new(0.0, 1.2, 0.0)),
            rot: df.rot(),
            shape: Shape::Box(Vec3::new(1.36, 2.38, 0.12)),
            mat: Mat::Wood,
            collide: true,
            visible: true,
        });
    }

    pub fn chest(&mut self, f: Frame, local: Vec3, yaw: f32, kind: ChestKind) {
        self.map.chests.push(ChestSpawn { pos: f.p(local), yaw: f.yaw + yaw, kind });
    }

    pub fn loot(&mut self, f: Frame, local: Vec3) {
        self.map.floor_loot.push(f.p(local));
    }

    /// Chhatri: four pillars and a dome, the domed rooftop pavilion of Rajasthan.
    pub fn chhatri(&mut self, f: Frame, base: Vec3, size: f32, mat: Mat) {
        let h = size * 1.1;
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            self.cyl(f, base + Vec3::new(sx * size * 0.42, 0.0, sz * size * 0.42), size * 0.07, h, mat);
        }
        self.bx(f, base + Vec3::new(0.0, h + 0.12, 0.0), Vec3::new(size * 1.05, 0.24, size * 1.05), mat);
        self.sphere(f, base + Vec3::new(0.0, h + 0.24, 0.0), size * 0.48, mat, true);
        self.cyl(f, base + Vec3::new(0.0, h + 0.24 + size * 0.46, 0.0), size * 0.05, size * 0.25, Mat::Gold);
    }

    /// Tapered tower of stacked slabs (shikhara / gopuram silhouette).
    pub fn tower(&mut self, f: Frame, base: Vec3, w: f32, d: f32, height: f32, tiers: u32, taper: f32, mat: Mat) {
        let th = height / tiers as f32;
        for i in 0..tiers {
            let s = 1.0 - taper * i as f32 / tiers as f32;
            self.bx(f, base + Vec3::new(0.0, th * (i as f32 + 0.5), 0.0), Vec3::new(w * s, th, d * s), mat);
        }
        let top = base + Vec3::Y * height;
        let r = w * (1.0 - taper) * 0.5;
        self.cyl(f, top, r * 1.1, r * 0.45, mat);
        self.cyl(f, top + Vec3::Y * r * 0.45, r * 0.2, r * 0.6, Mat::Gold);
    }
}

/// Rect minus hole -> up to four rects (all in the same plane).
pub fn subtract_rect(r: (Vec2, Vec2), h: (Vec2, Vec2)) -> Vec<(Vec2, Vec2)> {
    let (rmin, rmax) = r;
    let hmin = h.0.max(rmin);
    let hmax = h.1.min(rmax);
    if hmin.x >= hmax.x || hmin.y >= hmax.y {
        return vec![r];
    }
    let mut out = vec![];
    if hmin.y > rmin.y {
        out.push((rmin, Vec2::new(rmax.x, hmin.y)));
    }
    if hmax.y < rmax.y {
        out.push((Vec2::new(rmin.x, hmax.y), rmax));
    }
    if hmin.x > rmin.x {
        out.push((Vec2::new(rmin.x, hmin.y), Vec2::new(hmin.x, hmax.y)));
    }
    if hmax.x < rmax.x {
        out.push((Vec2::new(hmax.x, hmin.y), Vec2::new(rmax.x, hmax.y)));
    }
    out
}

/// Subtract many holes from one rect.
pub fn subtract_all(r: (Vec2, Vec2), holes: &[(Vec2, Vec2)]) -> Vec<(Vec2, Vec2)> {
    let mut rects = vec![r];
    for h in holes {
        rects = rects.into_iter().flat_map(|rr| subtract_rect(rr, *h)).collect();
    }
    rects
}
