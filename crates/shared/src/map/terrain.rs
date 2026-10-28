//! Heightfield terrain: rolling hills between flattened pads (POIs, villages, roads), mountains
//! around the edge, a river valley, and the fort's hill. Deterministic from the map seed.

use bevy::prelude::*;

#[derive(Clone, Debug, Default, Reflect)]
pub struct Terrain {
    /// Samples per side.
    pub n: usize,
    /// Side length in metres (centred on the origin).
    pub size: f32,
    /// Heights, index `i * n + j` with `i` along X and `j` along Z (avian/parry layout).
    pub h: Vec<f32>,
    /// Distance to the nearest road centreline, per sample (for painting roads).
    pub road_d: Vec<f32>,
}

impl Terrain {
    pub fn step(&self) -> f32 {
        self.size / (self.n - 1) as f32
    }

    pub fn coord(&self, i: usize) -> f32 {
        -self.size * 0.5 + i as f32 * self.step()
    }

    pub fn at(&self, i: usize, j: usize) -> f32 {
        self.h[i.min(self.n - 1) * self.n + j.min(self.n - 1)]
    }

    /// Bilinear height at a world XZ position.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let fx = ((x + self.size * 0.5) / self.step()).clamp(0.0, (self.n - 1) as f32 - 1e-3);
        let fz = ((z + self.size * 0.5) / self.step()).clamp(0.0, (self.n - 1) as f32 - 1e-3);
        let (i, j) = (fx as usize, fz as usize);
        let (tx, tz) = (fx - i as f32, fz - j as f32);
        let a = self.at(i, j) * (1.0 - tx) + self.at(i + 1, j) * tx;
        let b = self.at(i, j + 1) * (1.0 - tx) + self.at(i + 1, j + 1) * tx;
        a * (1.0 - tz) + b * tz
    }

    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = self.step();
        let dx = self.height(x + e, z) - self.height(x - e, z);
        let dz = self.height(x, z + e) - self.height(x, z - e);
        Vec3::new(-dx, 2.0 * e, -dz).normalize()
    }

    /// Rows for `Collider::heightfield` (outer = X, inner = Z).
    pub fn rows(&self) -> Vec<Vec<f32>> {
        (0..self.n).map(|i| self.h[i * self.n..(i + 1) * self.n].to_vec()).collect()
    }
}

// ------------------------------------------------------------------ noise

fn hash(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (z as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let (xi, zi) = (x.floor() as i32, z.floor() as i32);
    let (tx, tz) = (x - xi as f32, z - zi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sz) = (s(tx), s(tz));
    let a = hash(xi, zi, seed) * (1.0 - sx) + hash(xi + 1, zi, seed) * sx;
    let b = hash(xi, zi + 1, seed) * (1.0 - sx) + hash(xi + 1, zi + 1, seed) * sx;
    a * (1.0 - sz) + b * sz
}

pub fn fbm(x: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += value_noise(x * freq, z * freq, seed + o * 31) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

// ------------------------------------------------------------------ shaping

/// A flat area: full flatten inside `radius`, blending back to hills over `falloff`.
#[derive(Clone, Copy, Debug)]
pub struct Pad {
    pub center: Vec2,
    pub radius: f32,
    pub falloff: f32,
}

pub struct TerrainSpec {
    pub size: f32,
    pub n: usize,
    pub seed: u32,
    pub half_extent: f32,
    pub pads: Vec<Pad>,
    pub roads: Vec<(Vec2, Vec2)>,
    /// River channel (x0, x1, bed height): flat banks around it, a sunken bed inside.
    pub river: (f32, f32, f32),
    /// Rectangular pits (min, max, floor height): the stepwell.
    pub pits: Vec<(Vec2, Vec2, f32)>,
    /// Hills with flat summits: (center, plateau half extents, summit height, skirt width).
    pub mesas: Vec<(Vec2, Vec2, f32, f32)>,
}

pub fn generate(spec: &TerrainSpec) -> Terrain {
    let n = spec.n;
    let mut t = Terrain { n, size: spec.size, h: vec![0.0; n * n], road_d: vec![1e3; n * n] };
    let seed = spec.seed;
    for i in 0..n {
        for j in 0..n {
            let x = t.coord(i);
            let z = t.coord(j);
            let p = Vec2::new(x, z);
            // Rolling hills: broad swells with smaller lumps on top.
            let broad = fbm(x / 150.0, z / 150.0, 4, seed);
            let mut hills = (broad - 0.30).max(0.0).powf(1.15) * 58.0 + fbm(x / 35.0, z / 35.0, 3, seed + 7) * 2.2;
            // Mountains rising toward the arena edge.
            // Craggy ridges, not a smooth wall: the onset wobbles along the edge.
            let wob = (fbm(x / 70.0, z / 70.0, 2, seed + 41) - 0.5) * 30.0;
            let edge = smoothstep(spec.half_extent - 30.0 + wob, spec.half_extent + 25.0, x.abs().max(z.abs()));
            let crag = fbm(x / 22.0, z / 22.0, 4, seed + 13);
            hills += edge * edge * 26.0 + edge * (crag - 0.35).max(0.0) * 30.0;

            // Flatten: pads, road corridors, river corridor.
            let mut w: f32 = 0.0;
            for pad in &spec.pads {
                let d = p.distance(pad.center);
                w = w.max(1.0 - smoothstep(pad.radius, pad.radius + pad.falloff, d));
            }
            let mut rd = 1e3f32;
            for (a, b) in &spec.roads {
                rd = rd.min(seg_dist(p, *a, *b));
            }
            t.road_d[i * n + j] = rd;
            w = w.max(1.0 - smoothstep(5.0, 28.0, rd) * 1.0);
            let (rx0, rx1, bed) = spec.river;
            let river_d = if x < rx0 { rx0 - x } else if x > rx1 { x - rx1 } else { 0.0 };
            w = w.max(1.0 - smoothstep(18.0, 50.0, river_d));
            let mut h = hills * (1.0 - w);

            // Mesas (the fort hill): flat top, smooth skirt.
            for (c, half, top, skirt) in &spec.mesas {
                let q = (p - *c).abs() - *half;
                let outside = q.max(Vec2::ZERO).length();
                let m = *top * (1.0 - smoothstep(0.0, *skirt, outside));
                // Gentle ripples on the skirt so it doesn't look turned on a lathe.
                let rip = if outside > 2.0 { fbm(x / 18.0, z / 18.0, 2, seed + 29) * 2.0 * smoothstep(0.0, 10.0, outside) * (1.0 - smoothstep(*skirt * 0.6, *skirt, outside)) } else { 0.0 };
                h = h.max(m + rip);
            }
            if x > rx0 && x < rx1 {
                h = bed;
            }
            for (mn, mx, floor) in &spec.pits {
                if x > mn.x && x < mx.x && z > mn.y && z < mx.y {
                    h = *floor;
                }
            }
            t.h[i * n + j] = h;
        }
    }
    t
}
