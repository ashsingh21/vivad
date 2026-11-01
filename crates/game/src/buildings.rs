//! Static architecture rendering: all non-dynamic map blocks are merged into chunked meshes (one
//! per 64 m chunk per material) with metric world-space UVs, so tiling building textures (plaster,
//! ashlar, brick, granite, wood, thatch) line up across blocks without stretching. Colliders stay
//! on the simulation's per-block entities.
//!
//! Each chunk is built twice: in full, and for past `FAR_CHUNK` with coarser domes and columns and
//! without the smallest trim blocks.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor, ImageAddressMode, ImageFilterMode};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use shared::map::{Block, MapLayout, Mat, Shape};
use shared::sim::{OfMap, map_reloaded};

const CHUNK: f32 = 64.0;
/// Where a chunk's coarse LOD takes over, from its centre (scaled by quality: crate::quality::LodScale).
const FAR_CHUNK: f32 = 150.0;
/// Blocks no larger than this in any direction are left out of the coarse LOD.
const TRIM: f32 = 0.35;
/// Metres per texture repeat.
const TILE: f32 = 2.0;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_chunks)
            .add_systems(Update, (build_chunks.run_if(map_reloaded), queue_model_mips, mipmap_textures).chain());
    }
}

/// Textures that still need a CPU mip chain once loaded.
#[derive(Resource, Default)]
struct PendingMips(Vec<Handle<Image>>);

struct TexSet {
    base: Handle<Image>,
    normal: Handle<Image>,
}

fn load_set(assets: &AssetServer, pending: &mut PendingMips, name: &str) -> TexSet {
    let base = assets.load(format!("textures/india/{name}_basecolor.png"));
    let normal = assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| s.is_srgb = false)
        .load(format!("textures/india/{name}_normal.png"));
    pending.0.push(base.clone());
    pending.0.push(normal.clone());
    TexSet { base, normal }
}

/// Which blocks the batcher draws (doors are dynamic and drawn per entity).
pub fn is_static_visual(b: &Block) -> bool {
    b.visible && !matches!(b.mat, Mat::Water)
}

fn material_for(mat: Mat, sets: &HashMap<&'static str, TexSet>, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    let textured = |set: &str, tint: Color, rough: f32, materials: &mut Assets<StandardMaterial>| {
        let s = &sets[set];
        materials.add(StandardMaterial {
            base_color: tint,
            base_color_texture: Some(s.base.clone()),
            normal_map_texture: Some(s.normal.clone()),
            perceptual_roughness: rough,
            ..default()
        })
    };
    match mat {
        Mat::Sandstone => textured("sandstone", Color::WHITE, 0.85, materials),
        Mat::RedSandstone => textured("sandstone", Color::srgb(0.86, 0.50, 0.40), 0.85, materials),
        Mat::Granite => textured("granite", Color::WHITE, 0.8, materials),
        Mat::Plaster => textured("plaster", Color::srgb(0.97, 0.95, 0.90), 0.9, materials),
        Mat::PlasterOchre => textured("plaster", Color::srgb(0.95, 0.74, 0.44), 0.9, materials),
        Mat::PlasterBlue => textured("plaster", Color::srgb(0.55, 0.72, 0.95), 0.9, materials),
        Mat::Marble => textured("plaster", Color::srgb(1.0, 0.99, 0.97), 0.35, materials),
        Mat::Brick => textured("brick", Color::WHITE, 0.9, materials),
        Mat::Wood => textured("wood", Color::WHITE, 0.75, materials),
        Mat::Thatch => textured("thatch", Color::WHITE, 0.95, materials),
        Mat::Dirt => textured("sandstone", Color::srgb(0.75, 0.62, 0.48), 0.95, materials),
        Mat::Gold => materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.76, 0.34), metallic: 1.0, perceptual_roughness: 0.3, ..default() }),
        Mat::Cloth => materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.22, 0.32), perceptual_roughness: 0.9, double_sided: true, cull_mode: None, ..default() }),
        Mat::Foliage => materials.add(StandardMaterial { base_color: Color::srgb(0.26, 0.42, 0.18), perceptual_roughness: 0.95, ..default() }),
        Mat::Bark => textured("wood", Color::srgb(0.7, 0.65, 0.6), 0.95, materials),
        Mat::Grass => materials.add(StandardMaterial { base_color: Color::srgb(0.42, 0.50, 0.25), perceptual_roughness: 0.95, ..default() }),
        Mat::Water => materials.add(StandardMaterial { base_color: Color::srgba(0.10, 0.26, 0.28, 0.85), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.06, ..default() }),
    }
}

#[derive(Default)]
struct MeshBuf {
    /// Built for distance: fewer segments round curves.
    coarse: bool,
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    /// Per-vertex colour (linear), when the buffer has one: tint and baked shading.
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl MeshBuf {
    fn quad(&mut self, p: [Vec3; 4], n: Vec3, uv: [Vec2; 4]) {
        let base = self.pos.len() as u32;
        for k in 0..4 {
            self.pos.push(p[k].to_array());
            self.nrm.push(n.to_array());
            self.uv.push(uv[k].to_array());
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A quad with a colour per corner.
    fn quad_col(&mut self, p: [Vec3; 4], n: Vec3, uv: [Vec2; 4], col: [[f32; 4]; 4]) {
        self.quad(p, n, uv);
        self.col.extend_from_slice(&col);
    }

    /// A box of one colour, for clutter: centre, rotation about Y, full size.
    fn solid_box(&mut self, c: Vec3, rot: Quat, size: Vec3, col: [f32; 4]) {
        let h = size * 0.5;
        for (n, u, v) in [(Vec3::X, Vec3::NEG_Z, Vec3::Y), (Vec3::NEG_X, Vec3::Z, Vec3::Y), (Vec3::Y, Vec3::X, Vec3::NEG_Z), (Vec3::NEG_Y, Vec3::X, Vec3::Z), (Vec3::Z, Vec3::X, Vec3::Y), (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y)] {
            let (cn, eu, ev) = (n * h, u * u.abs().dot(h), v * v.abs().dot(h));
            let p = [cn - eu - ev, cn + eu - ev, cn + eu + ev, cn - eu + ev].map(|q| c + rot * q);
            self.quad_col(p, rot * n, [Vec2::ZERO; 4], [col; 4]);
        }
    }

    /// A box of one colour with no bottom face, for trim that sits on or against something.
    fn open_box(&mut self, c: Vec3, rot: Quat, size: Vec3, col: [f32; 4]) {
        let n0 = self.idx.len();
        self.solid_box(c, rot, size, col);
        // solid_box's fourth face is the bottom: drop its six indices.
        self.idx.drain(n0 + 18..n0 + 24);
    }

    /// A bar of one colour along its local X with its four long edges chamfered `bev` deep, so they
    /// catch a line of light: trim's rounded look for a handful more triangles than a box.
    fn prism(&mut self, c: Vec3, rot: Quat, size: Vec3, bev: f32, col: [f32; 4]) {
        let (l, a, b) = (size.x * 0.5, size.z * 0.5, size.y * 0.5);
        let e = bev.min(a * 0.9).min(b * 0.9);
        // The cross-section as (z, y), anticlockwise from the +z face's foot.
        let sec = [(a, -b + e), (a, b - e), (a - e, b), (-a + e, b), (-a, b - e), (-a, -b + e), (-a + e, -b), (a - e, -b)];
        let at = |x: f32, (z, y): (f32, f32)| c + rot * Vec3::new(x, y, z);
        for k in 0..8 {
            let (s0, s1) = (sec[k], sec[(k + 1) % 8]);
            let n = rot * Vec3::new(0.0, -(s1.0 - s0.0), s1.1 - s0.1).normalize();
            // Undersides in their own shadow (baked occlusion): a ledge reads as a ledge.
            let k = if n.y < -0.5 { 0.6 } else if n.y < -0.1 { 0.8 } else { 1.0 };
            let c = [col[0] * k, col[1] * k, col[2] * k, col[3]];
            self.quad_col([at(-l, s0), at(l, s0), at(l, s1), at(-l, s1)], n, [Vec2::ZERO; 4], [c; 4]);
        }
        for (x, n) in [(l, Vec3::X), (-l, Vec3::NEG_X)] {
            let base = self.pos.len() as u32;
            for q in sec {
                self.pos.push(at(x, q).to_array());
                self.nrm.push((rot * n).to_array());
                self.uv.push([0.0, 0.0]);
                self.col.push(col);
            }
            for k in 1..7u32 {
                // Seen from +x the section runs clockwise, from -x anticlockwise.
                let t = if x > 0.0 { [base, base + k + 1, base + k] } else { [base, base + k, base + k + 1] };
                self.idx.extend_from_slice(&t);
            }
        }
    }

    /// A flat piece seen from both sides (cloth): a polygon fanned from its first corner, drawn
    /// twice, once each way round.
    fn cloth(&mut self, p: &[Vec3], col: [f32; 4]) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        for (side, flip) in [(n, false), (-n, true)] {
            let base = self.pos.len() as u32;
            for q in p {
                self.pos.push(q.to_array());
                self.nrm.push(side.to_array());
                self.uv.push([0.0, 0.0]);
                self.col.push(col);
            }
            for k in 1..p.len() as u32 - 1 {
                let t = if flip { [base, base + k + 1, base + k] } else { [base, base + k, base + k + 1] };
                self.idx.extend_from_slice(&t);
            }
        }
    }

    /// An upright cylinder of one colour (sides and top), for clutter.
    fn solid_cylinder(&mut self, c: Vec3, r: f32, h: f32, col: [f32; 4]) {
        let seg = 10;
        for k in 0..seg {
            let (a0, a1) = (std::f32::consts::TAU * k as f32 / seg as f32, std::f32::consts::TAU * (k + 1) as f32 / seg as f32);
            let (d0, d1) = (Vec3::new(a0.cos(), 0.0, -a0.sin()), Vec3::new(a1.cos(), 0.0, -a1.sin()));
            let (lo, hi) = (c - Vec3::Y * h * 0.5, c + Vec3::Y * h * 0.5);
            let base = self.pos.len() as u32;
            for (p, n) in [(lo + d0 * r, d0), (lo + d1 * r, d1), (hi + d1 * r, d1), (hi + d0 * r, d0)] {
                self.pos.push(p.to_array());
                self.nrm.push(n.to_array());
                self.uv.push([0.0, 0.0]);
                self.col.push(col);
            }
            self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            let base = self.pos.len() as u32;
            for p in [hi, hi + d0 * r, hi + d1 * r] {
                self.pos.push(p.to_array());
                self.nrm.push([0.0, 1.0, 0.0]);
                self.uv.push([0.0, 0.0]);
                self.col.push(col);
            }
            self.idx.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }

    fn add_box(&mut self, b: &Block, size: Vec3) {
        let h = size * 0.5;
        // (normal axis, u axis, v axis) per face, all in local space; quads wound CCW from outside.
        let faces = [
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ];
        for (n, u, v) in faces {
            let c = n * h;
            let eu = u * (u.abs().dot(h));
            let ev = v * (v.abs().dot(h));
            let corners = [c - eu - ev, c + eu - ev, c + eu + ev, c - eu + ev];
            let world: [Vec3; 4] = corners.map(|p| b.pos + b.rot * p);
            let wu = b.rot * u;
            let wv = b.rot * v;
            let uv = world.map(|p| Vec2::new(p.dot(wu), p.dot(wv)) / TILE);
            self.quad(world, b.rot * n, uv);
        }
    }

    fn add_cylinder(&mut self, b: &Block, radius: f32, height: f32) {
        let seg = match (self.coarse, radius > 1.5, radius > 0.4) {
            (false, true, _) => 32,
            (false, _, true) => 20,
            (false, ..) => 10,
            (true, true, _) => 12,
            (true, _, true) => 8,
            (true, ..) => 6,
        };
        let hh = height * 0.5;
        let circ = std::f32::consts::TAU * radius;
        // Whole number of texture repeats around so the seam is invisible.
        let reps = (circ / TILE).round().max(1.0);
        for k in 0..seg {
            let a0 = std::f32::consts::TAU * k as f32 / seg as f32;
            let a1 = std::f32::consts::TAU * (k + 1) as f32 / seg as f32;
            let d0 = Vec3::new(a0.cos(), 0.0, -a0.sin());
            let d1 = Vec3::new(a1.cos(), 0.0, -a1.sin());
            let p = [d0 * radius - Vec3::Y * hh, d1 * radius - Vec3::Y * hh, d1 * radius + Vec3::Y * hh, d0 * radius + Vec3::Y * hh];
            let world = p.map(|q| b.pos + b.rot * q);
            let (u0, u1) = (reps * k as f32 / seg as f32, reps * (k + 1) as f32 / seg as f32);
            let (v0, v1) = ((b.pos.y - hh) / TILE, (b.pos.y + hh) / TILE);
            let uv = [Vec2::new(u0, v0), Vec2::new(u1, v0), Vec2::new(u1, v1), Vec2::new(u0, v1)];
            let base = self.pos.len() as u32;
            for (i, w) in world.iter().enumerate() {
                let d = if i == 0 || i == 3 { d0 } else { d1 };
                self.pos.push(w.to_array());
                self.nrm.push((b.rot * d).to_array());
                self.uv.push(uv[i].to_array());
            }
            self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            // Caps as fan wedges.
            for (y, n, flip) in [(hh, Vec3::Y, false), (-hh, Vec3::NEG_Y, true)] {
                let c = b.pos + b.rot * (Vec3::Y * y);
                let e0 = b.pos + b.rot * (d0 * radius + Vec3::Y * y);
                let e1 = b.pos + b.rot * (d1 * radius + Vec3::Y * y);
                let base = self.pos.len() as u32;
                for w in [c, e0, e1] {
                    self.pos.push(w.to_array());
                    self.nrm.push((b.rot * n).to_array());
                    self.uv.push([w.x / TILE, w.z / TILE]);
                }
                if flip {
                    self.idx.extend_from_slice(&[base, base + 2, base + 1]);
                } else {
                    self.idx.extend_from_slice(&[base, base + 1, base + 2]);
                }
            }
        }
    }

    fn add_sphere(&mut self, b: &Block, r: f32) {
        let (seg, rings) = match (self.coarse, r > 3.0) {
            (false, true) => (36, 18),
            (false, false) => (20, 12),
            (true, true) => (14, 7),
            (true, false) => (8, 5),
        };
        let reps = (std::f32::consts::TAU * r / TILE).round().max(1.0);
        let vert = |i: usize, k: usize| -> (Vec3, Vec2) {
            let lat = std::f32::consts::PI * (i as f32 / rings as f32 - 0.5);
            let lon = std::f32::consts::TAU * k as f32 / seg as f32;
            let d = Vec3::new(lat.cos() * lon.cos(), lat.sin(), -lat.cos() * lon.sin());
            (d, Vec2::new(reps * k as f32 / seg as f32, (lat * r + b.pos.y) / TILE))
        };
        for i in 0..rings {
            for k in 0..seg {
                let c = [vert(i, k), vert(i, k + 1), vert(i + 1, k + 1), vert(i + 1, k)];
                let base = self.pos.len() as u32;
                for (d, uv) in c {
                    self.pos.push((b.pos + b.rot * (d * r)).to_array());
                    self.nrm.push((b.rot * d).to_array());
                    self.uv.push(uv.to_array());
                }
                self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }

    /// The mesh, with its vertices relative to the centre of its bounds, and that centre (the
    /// LOD crossfade measures distance from the mesh's origin, so it must sit in the middle).
    fn into_mesh(self) -> (Mesh, Vec3) {
        let (lo, hi) = self.pos.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
        let c = (lo + hi) / 2.0;
        let pos: Vec<[f32; 3]> = self.pos.iter().map(|p| (Vec3::from(*p) - c).to_array()).collect();
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_indices(Indices::U32(self.idx));
        if !self.col.is_empty() {
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        }
        let _ = m.generate_tangents();
        (m, c)
    }
}

/// A city facade's bay width and storey height, in metres.
pub(crate) const BAY: f32 = 3.2;
/// How far signs, cables and roof tanks are drawn (scaled by quality like the chunks' LODs).
const CLUTTER_FAR: f32 = 130.0;
/// Festival colours, kept for small things so they pop off the pastel walls: signboards,
/// awnings, bunting. Marigold, rani pink, peacock blue, parrot green, sindoor red, haldi yellow,
/// jamun purple, white.
const ACCENTS: [[f32; 3]; 8] = [
    [0.98, 0.58, 0.06],
    [0.90, 0.12, 0.46],
    [0.04, 0.46, 0.62],
    [0.18, 0.66, 0.24],
    [0.86, 0.12, 0.10],
    [0.99, 0.82, 0.10],
    [0.46, 0.16, 0.58],
    [0.97, 0.95, 0.90],
];
pub(crate) const STOREY: f32 = 3.4;
/// Lime-washed walls, sun-faded: Old Delhi's dusty pinks, ochres, mints and blues, and cream.
pub(crate) const WALLS: [[f32; 3]; 9] = [
    [0.93, 0.62, 0.58],
    [0.96, 0.72, 0.38],
    [0.60, 0.82, 0.70],
    [0.56, 0.72, 0.90],
    [0.96, 0.91, 0.80],
    [0.87, 0.50, 0.38],
    [0.77, 0.66, 0.85],
    [0.97, 0.84, 0.48],
    [0.48, 0.75, 0.78],
];
/// Off-white trim: parapet coping, cornices, pilasters, sills.
const TRIM_WHITE: [f32; 3] = [0.97, 0.94, 0.88];
/// How high a roof's parapet stands, and how thick it is.
const PARAPET: (f32, f32) = (0.9, 0.3);
/// Half the width of a corner pilaster: clear of the parapet and coping behind it, so no faces meet.
const PILASTER: f32 = 0.38;

/// A cheap hash of a few integers to 0..1, for texture detail that doesn't repeat by the pixel.
fn hash01(a: u32, b: u32, c: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca77) ^ c.wrapping_mul(0xc2b2_ae3d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// Pixels of an image built cell by cell, written as sRGB bytes.
fn image_from(w: usize, h: usize, px: impl Fn(usize, usize) -> [f32; 3]) -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let c = px(x, y);
            data.extend_from_slice(&[(c[0].clamp(0.0, 1.0) * 255.0) as u8, (c[1].clamp(0.0, 1.0) * 255.0) as u8, (c[2].clamp(0.0, 1.0) * 255.0) as u8, 255]);
        }
    }
    Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data,
               TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// The upper storeys: two bays by two storeys, each window different (plain, shuttered, with an AC
/// unit under it, behind a balcony rail), with rain stains running down from the sills. Near-white,
/// tinted per material and per building.
pub(crate) fn facade_image() -> Image {
    let n = 128usize;
    image_from(2 * n, 2 * n, |x, y| {
        let (cx, cy) = (x / n, y / n);
        let kind = [[0, 2], [3, 1]][cy][cx];
        let (u, v) = ((x % n) as f32 / n as f32, (y % n) as f32 / n as f32);
        // Plaster with blotches, a little darker in streaks under the sill.
        let grain = 0.94 + 0.06 * hash01(x as u32 / 3, y as u32 / 3, 1);
        let stain = if (0.30..0.70).contains(&u) && v > 0.80 { 1.0 - 0.18 * (1.0 - (v - 0.80) / 0.20) * (0.5 + hash01(x as u32 / 2, 0, 2)) } else { 1.0 };
        let wall = [0.93 * grain * stain, 0.91 * grain * stain, 0.88 * grain * stain];
        let window = (0.28..0.72).contains(&u) && (0.16..0.70).contains(&v);
        let frame = (0.25..0.75).contains(&u) && (0.13..0.73).contains(&v);
        let sill = (0.22..0.78).contains(&u) && (0.73..0.78).contains(&v);
        let band = v > 0.95;
        match kind {
            // Shutters either side, painted green.
            1 if ((0.10..0.25).contains(&u) || (0.75..0.90).contains(&u)) && (0.13..0.73).contains(&v) => {
                let slat = if (y % 5) < 2 { 0.8 } else { 1.0 };
                return [0.30 * slat, 0.50 * slat, 0.38 * slat];
            }
            // An AC unit hung under the window: a grey box with a grille, and its drip stain.
            2 if (0.34..0.66).contains(&u) && (0.79..0.94).contains(&v) => {
                let grille = (0.38..0.55).contains(&u) && (x % 3 == 0);
                return if grille { [0.45, 0.46, 0.47] } else { [0.78, 0.78, 0.76] };
            }
            // A balcony rail across the bottom of a door-height opening.
            3 if (0.20..0.80).contains(&u) && (0.52..0.80).contains(&v) => {
                let bar = (x % 7) < 2 || (0.52..0.56).contains(&v);
                if bar {
                    return [0.22, 0.20, 0.19];
                }
            }
            _ => {}
        }
        if window || (kind == 3 && (0.28..0.72).contains(&u) && (0.16..0.80).contains(&v)) {
            // Glass: dark, bluer toward the top where it catches the sky; a curtain in some.
            let k = 1.0 - (v - 0.16) / 0.54;
            let curtain = kind == 0 && u > 0.5;
            if curtain {
                return [0.55, 0.32, 0.26];
            }
            [0.15 + 0.09 * k, 0.18 + 0.10 * k, 0.22 + 0.13 * k]
        } else if frame || sill {
            [0.82, 0.80, 0.77]
        } else if band {
            [0.77, 0.75, 0.71]
        } else {
            wall
        }
    })
}

/// The ground storey: two bays of shopfront under a beam, one shuttered, one open onto shelves.
fn shop_image() -> Image {
    let n = 128usize;
    image_from(2 * n, n, |x, y| {
        let bay = x / n;
        let (u, v) = ((x % n) as f32 / n as f32, y as f32 / n as f32);
        let grain = 0.94 + 0.06 * hash01(x as u32 / 3, y as u32 / 3, 7);
        // Piers between shops, and the beam the signboards hang on.
        if !(0.06..0.94).contains(&u) || v < 0.20 {
            let dirt = if v > 0.8 { 0.75 + 0.25 * (1.0 - (v - 0.8) / 0.2) } else { 1.0 };
            return [0.86 * grain * dirt, 0.84 * grain * dirt, 0.80 * grain * dirt];
        }
        if bay == 0 {
            // A rolling shutter: ribbed grey steel, rustier toward the bottom.
            let rib = if (y % 4) == 0 { 0.72 } else { 1.0 };
            let rust = ((v - 0.2) / 0.8).max(0.0);
            [(0.58 + 0.10 * rust) * rib * grain, (0.58 - 0.05 * rust) * rib * grain, (0.58 - 0.12 * rust) * rib * grain]
        } else {
            // An open shop: dark inside, shelves of goods in every colour.
            let shelf = ((v - 0.20) / 0.80 * 4.0).fract();
            if shelf < 0.08 {
                return [0.35, 0.28, 0.22];
            }
            if shelf > 0.35 && v < 0.85 {
                let item = hash01(x as u32 / 6, (((v - 0.20) / 0.80) * 4.0) as u32, 3);
                let c = [[0.75, 0.20, 0.18], [0.90, 0.70, 0.20], [0.20, 0.45, 0.70], [0.85, 0.85, 0.80], [0.25, 0.55, 0.30]][(item * 4.99) as usize];
                let gap = (x % 6) == 0;
                if !gap {
                    return c.map(|k| k * 0.6);
                }
            }
            [0.10, 0.09, 0.08]
        }
    })
}

/// A sagging cable from `a` to `b`: a thin square tube in a few straight pieces.
fn wire(buf: &mut MeshBuf, a: Vec3, b: Vec3, sag: f32) {
    let col = [0.02, 0.02, 0.02, 1.0];
    let pieces = 6;
    let at = |t: f32| a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t);
    for k in 0..pieces {
        let (p, q) = (at(k as f32 / pieces as f32), at((k + 1) as f32 / pieces as f32));
        let Ok(dir) = Dir3::new(q - p) else { continue };
        let side = dir.cross(Vec3::Y).normalize_or(Vec3::X) * 0.018;
        let up = side.cross(*dir).normalize() * 0.018;
        for (u, v) in [(side, up), (up, -side), (-side, -up), (-up, side)] {
            let n = (u + v).normalize();
            buf.quad_col([p + u, q + u, q + v, p + v], n, [Vec2::ZERO; 4], [col; 4]);
        }
    }
}

/// A string of triangular paper flags across a lane, in festival colours.
fn bunting(buf: &mut MeshBuf, a: Vec3, b: Vec3, sag: f32, seed: u32) {
    let at = |t: f32| a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t);
    let count = ((b - a).length() / 0.55) as u32;
    wire(buf, a, b, sag);
    for k in 1..count {
        let (t0, t1) = ((k as f32 - 0.35) / count as f32, (k as f32 + 0.35) / count as f32);
        let (p, q) = (at(t0), at(t1));
        let tip = (p + q) * 0.5 - Vec3::Y * 0.34;
        let c = ACCENTS[(hash01(seed, k, 9) * ACCENTS.len() as f32) as usize % ACCENTS.len()];
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        buf.cloth(&[p, q, tip], [l.red, l.green, l.blue, 1.0]);
    }
}

/// A city map's buildings (MapLayout::footprints), merged per chunk and material: walls in the
/// facade, roofs in plain plaster.
fn build_footprints(
    commands: &mut Commands,
    layout: &MapLayout,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    pending: &mut PendingMips,
    roof: Handle<StandardMaterial>,
) {
    if layout.footprints.is_empty() {
        return;
    }
    let facade = images.add(facade_image());
    let shopfront = images.add(shop_image());
    pending.0.push(facade.clone());
    pending.0.push(shopfront.clone());
    // Where buildings stand, a metre a cell: whether a wall faces the street or a neighbour, and
    // how far across the lane the building opposite is.
    let span = layout.terrain.size.max(2.0 * layout.half_extent);
    let cells = span.ceil() as usize;
    // The tallest roof over each cell, 0 where nothing stands.
    let mut solid = vec![0.0f32; cells * cells];
    let to_cell = |p: Vec2| -> Option<usize> {
        let (i, j) = ((p.x + span * 0.5).floor(), (p.y + span * 0.5).floor());
        (i >= 0.0 && j >= 0.0 && (i as usize) < cells && (j as usize) < cells).then(|| j as usize * cells + i as usize)
    };
    for fp in layout.footprints.iter().filter(|f| f.mat != Mat::Water) {
        let (lo, hi) = fp.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        for j in lo.y.floor() as i32..=hi.y.ceil() as i32 {
            for i in lo.x.floor() as i32..=hi.x.ceil() as i32 {
                let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5);
                if shared::map::footprint::contains(&fp.pts, p)
                    && let Some(k) = to_cell(p)
                {
                    solid[k] = solid[k].max(fp.top.max(0.1));
                }
            }
        }
    }
    let roof_at = |p: Vec2| to_cell(p).map_or(0.0, |k| solid[k]);
    let occupied = |p: Vec2| roof_at(p) > 0.0;
    let lin = |c: [f32; 3]| {
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        [l.red, l.green, l.blue, 1.0]
    };
    let mut clutter: HashMap<(i32, i32), MeshBuf> = HashMap::new();
    let mut trims: HashMap<(i32, i32), MeshBuf> = HashMap::new();
    let tint = |m: Mat| match m {
        Mat::PlasterOchre => Color::srgb(0.95, 0.76, 0.48),
        Mat::Sandstone => Color::srgb(0.85, 0.68, 0.50),
        Mat::RedSandstone => Color::srgb(0.78, 0.45, 0.36),
        Mat::Brick => Color::srgb(0.70, 0.40, 0.30),
        _ => Color::srgb(0.97, 0.95, 0.90),
    };
    let mut walls: HashMap<(i32, i32, Mat, bool), MeshBuf> = HashMap::new();
    let mut roofs: HashMap<(i32, i32), MeshBuf> = HashMap::new();
    let mut water = MeshBuf::default();
    for fp in &layout.footprints {
        let c = fp.pts.iter().sum::<Vec2>() / fp.pts.len() as f32;
        let (cx, cz) = ((c.x / CHUNK).floor() as i32, (c.y / CHUNK).floor() as i32);
        // Water is only its surface.
        if fp.mat == Mat::Water {
            for [i, j, k] in shared::map::footprint::triangulate(&fp.pts) {
                let p = [fp.pts[i as usize], fp.pts[j as usize], fp.pts[k as usize]].map(|q| Vec3::new(q.x, fp.top, q.y));
                let p = if (p[1] - p[0]).cross(p[2] - p[0]).y >= 0.0 { p } else { [p[0], p[2], p[1]] };
                let base = water.pos.len() as u32;
                for q in p {
                    water.pos.push(q.to_array());
                    water.nrm.push([0.0, 1.0, 0.0]);
                    water.uv.push([q.x / 8.0, q.z / 8.0]);
                }
                water.idx.extend_from_slice(&[base, base + 1, base + 2]);
            }
            continue;
        }
        // Each building a little different: lighter or darker, warmer or cooler.
        let seed = (c.x * 7.3 + c.y * 13.1).to_bits();
        let r = |k: u32| hash01(seed, k, 11);
        // Plastered walls take a colour from the palette; brick and stone keep their own (the
        // material's tint).
        let painted = !matches!(fp.mat, Mat::Brick | Mat::RedSandstone | Mat::Sandstone);
        let wall = if painted { WALLS[(r(4) * WALLS.len() as f32) as usize % WALLS.len()] } else { [1.0; 3] };
        let v = 0.96 + 0.06 * r(1);
        // Darker toward the street, and cooler there, as if in the lane's blue shade.
        let tint_at = |shade: f32| {
            let cool = (1.0 - shade).max(0.0) * 0.25;
            let l = Color::srgb(wall[0] * v * shade, wall[1] * v * shade, wall[2] * v * (shade + cool)).to_linear();
            [l.red, l.green, l.blue, 1.0]
        };
        // Trim mostly white; now and then the wall's own colour, deeper.
        let trim = if r(5) < 0.75 || !painted { lin(TRIM_WHITE) } else { lin(wall.map(|c| c * 0.78)) };
        let height = fp.top - fp.base;
        // A row of shops along the street when there's a storey to spare above them.
        let shops = height > STOREY * 1.6 && r(3) < 0.8;
        let split = (fp.base + STOREY).min(fp.top);
        let n = fp.pts.len();
        let mut along = 0.0;
        let ccw = fp.pts.iter().zip(fp.pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() > 0.0;
        // Which corners stick out (and turn enough to be a corner, not a bend in a curve).
        let corner: Vec<i8> = (0..n)
            .map(|i| {
                let (prev, a, b) = (fp.pts[(i + n - 1) % n], fp.pts[i], fp.pts[(i + 1) % n]);
                let (e0, e1) = (a - prev, b - a);
                if e0.length() < 1.0 || e1.length() < 1.0 || e0.normalize().dot(e1.normalize()) > 0.9 {
                    return 0;
                }
                if ((e0.x * e1.y - e0.y * e1.x) > 0.0) == ccw { 1 } else { -1 }
            })
            .collect();
        for i in 0..n {
            let (a, b) = (fp.pts[i], fp.pts[(i + 1) % n]);
            let e = b - a;
            let len = e.length();
            if len < 1e-3 {
                continue;
            }
            let out2 = if ccw { Vec2::new(e.y, -e.x) } else { Vec2::new(-e.y, e.x) } / len;
            let out = Vec3::new(out2.x, 0.0, out2.y);
            let (u0, u1) = (along / (2.0 * BAY), (along + len) / (2.0 * BAY));
            let mid = (a + b) * 0.5;
            let street = !occupied(mid + out2 * 1.0) && !occupied(mid + out2 * 2.5);
            // How high the neighbour's wall stands against this one: above it, this wall is seen.
            let screened = roof_at(mid + out2 * 1.0).max(roof_at(mid + out2 * 2.5));
            let dir = Vec3::new(e.x, 0.0, e.y) / len;
            let rot = Quat::from_rotation_arc(Vec3::X, dir);
            let at = |s: f32, y: f32| Vec3::new(a.x, y, a.y) + dir * s;
            {
                let t = trims.entry((cx, cz)).or_default();
                // The parapet round the roof, and its coping.
                let (ph, pt) = PARAPET;
                t.open_box(at(len * 0.5, fp.top + ph * 0.5) - out * pt * 0.5, rot, Vec3::new(len, ph, pt), tint_at(1.06));
                t.prism(at(len * 0.5, fp.top + ph + 0.07) - out * 0.15, rot, Vec3::new(len, 0.14, 0.4), 0.05, trim);
                // A chhajja over the ground storey: a deep sunshade ledge, run on round outside
                // corners to meet the next wall's and stopped short of inside ones.
                if fp.top - split > 1.0 {
                    const DEEP: f32 = 0.6;
                    let ext = |c: i8| c as f32 * DEEP;
                    let (s0, s1) = (-ext(corner[i]), len + ext(corner[(i + 1) % n]));
                    t.prism(at((s0 + s1) * 0.5, split + 0.04) + out * (DEEP - 0.1) * 0.5, rot, Vec3::new(s1 - s0, 0.2, DEEP + 0.1), 0.06, trim);
                }
                // A pilaster up each outside corner.
                if corner[i] > 0 {
                    let h = fp.top - fp.base + ph + 0.3;
                    let up = rot * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                    t.prism(Vec3::new(a.x, fp.base + h * 0.5, a.y), up, Vec3::new(h, 2.0 * PILASTER, 2.0 * PILASTER), 0.08, trim);
                }
            }
            // Sills and lintels at each window of the upper storeys, where they can be seen,
            // where the window texture puts them (crate::buildings::facade_image): a window a bay,
            // its sill 0.75-0.92 m up a storey, its head at 2.9-3.06 m; the balcony's opening goes
            // down to a slab at 0.62 m.
            if fp.top > screened + STOREY {
                let k = clutter.entry((cx, cz)).or_default();
                let first = ((along + 0.6) / BAY).ceil() as i32;
                let last = ((along + len - 0.6) / BAY).floor() as i32;
                let storeys = ((fp.top - fp.base) / STOREY + 0.01).floor() as i32;
                for bay in first..last {
                    let s = (bay as f32 + 0.5) * BAY - along;
                    for st in 1..storeys {
                        let floor = fp.base + st as f32 * STOREY;
                        if floor < screened {
                            continue;
                        }
                        // The texture's two-by-two repeat: which of its windows this is.
                        let kind = [[0, 2], [3, 1]][(st % 2 == 0) as usize][bay.rem_euclid(2) as usize];
                        if kind == 3 {
                            k.prism(at(s, floor + 0.62) + out * 0.27, rot, Vec3::new(2.1, 0.14, 0.6), 0.04, trim);
                        } else {
                            k.prism(at(s, floor + 0.84) + out * 0.08, rot, Vec3::new(1.75, 0.14, 0.24), 0.04, trim);
                        }
                        k.prism(at(s, floor + 3.0) + out * 0.06, rot, Vec3::new(1.75, 0.16, 0.2), 0.04, trim);
                    }
                }
            }
            // Two bands, each with its own texture and shading baked into the vertex colour: the
            // ground storey dark with street grime at its foot, the rest lighter toward the top.
            for (lo, hi, shop, (s0, s1)) in [(fp.base, split, shops, (0.66, 0.84)), (split, fp.top, false, (0.9, 1.06))] {
                if hi - lo < 0.05 {
                    continue;
                }
                let (a0, b0) = (Vec3::new(a.x, lo, a.y), Vec3::new(b.x, lo, b.y));
                let b1 = b0.with_y(hi);
                // Storeys counted from the building's foot; a shop band is one storey of its texture.
                let rows = if shop { STOREY } else { 2.0 * STOREY };
                let (v0, v1) = (-(lo - fp.base) / rows, -(hi - fp.base) / rows);
                let w = walls.entry((cx, cz, fp.mat, shop)).or_default();
                // Inside corners in shadow (baked occlusion): the wall is cut 1.5 m from each one
                // and darkened toward it.
                const DIM: f32 = 1.5;
                let cut0 = if corner[i] < 0 { DIM.min(len * 0.5) } else { 0.0 };
                let cut1 = if corner[(i + 1) % n] < 0 { DIM.min(len * 0.5) } else { 0.0 };
                let mut stops = vec![(0.0, if cut0 > 0.0 { 0.62 } else { 1.0 })];
                if cut0 > 0.0 {
                    stops.push((cut0, 1.0));
                }
                if cut1 > 0.0 {
                    stops.push((len - cut1, 1.0));
                }
                stops.push((len, if cut1 > 0.0 { 0.62 } else { 1.0 }));
                stops.dedup_by(|x, y| (x.0 - y.0).abs() < 1e-3);
                let flip = (b0 - a0).cross(b1 - a0).dot(out) < 0.0;
                for pair in stops.windows(2) {
                    let ((d0, k0), (d1, k1)) = (pair[0], pair[1]);
                    let (p0, p1) = (a0 + (b0 - a0) * (d0 / len), a0 + (b0 - a0) * (d1 / len));
                    let (q0, q1) = (p0.with_y(hi), p1.with_y(hi));
                    let (e0, e1) = (u0 + (u1 - u0) * d0 / len, u0 + (u1 - u0) * d1 / len);
                    let uv = [Vec2::new(e0, v0), Vec2::new(e1, v0), Vec2::new(e1, v1), Vec2::new(e0, v1)];
                    let dim = |c: [f32; 4], k: f32| [c[0] * k, c[1] * k, c[2] * k, c[3]];
                    let col = [dim(tint_at(s0), k0), dim(tint_at(s0), k1), dim(tint_at(s1), k1), dim(tint_at(s1), k0)];
                    if !flip {
                        w.quad_col([p0, p1, q1, q0], out, uv, col);
                    } else {
                        w.quad_col([p1, p0, q0, q1], out, [uv[1], uv[0], uv[3], uv[2]], [col[1], col[0], col[3], col[2]]);
                    }
                }
            }
            // Clutter on walls that face the street (not a neighbour's wall).
            if street {
                let k = clutter.entry((cx, cz)).or_default();
                if shops {
                    // A signboard over each shop: a coloured board with a pale panel of "lettering".
                    let pair = 2.0 * BAY;
                    let first = (along / pair).ceil() as i32;
                    let last = ((along + len) / pair).floor() as i32;
                    for s in first..last {
                        let h = hash01(seed, s as u32, i as u32);
                        if h > 0.8 {
                            continue;
                        }
                        let at = a + e / len * ((s as f32 + 0.5) * pair - along);
                        let y = fp.base + STOREY - 0.5;
                        let board = ACCENTS[(h * 41.0) as usize % ACCENTS.len()];
                        let p = Vec3::new(at.x, y, at.y) + out * 0.07;
                        k.solid_box(p, rot, Vec3::new(pair * 0.86, 0.66, 0.1), lin(board));
                        let letters = if board[0] + board[1] + board[2] > 2.0 { [0.12, 0.10, 0.10] } else { [0.95, 0.93, 0.86] };
                        k.solid_box(p + out * 0.055, rot, Vec3::new(pair * 0.86 * (0.4 + 0.3 * hash01(seed, s as u32, 5)), 0.26, 0.02), lin(letters));
                        // Under some, a striped cloth awning with a valance.
                        if hash01(seed, s as u32, 6) < 0.45 {
                            let c = lin(ACCENTS[(hash01(seed, s as u32, 7) * 7.0) as usize]);
                            let w = pair * 0.86;
                            let stripes = (w / 0.45).round().max(1.0) as i32;
                            let foot = Vec3::new(at.x, fp.base, at.y);
                            for k2 in 0..stripes {
                                let (x0, x1) = (w * (k2 as f32 / stripes as f32 - 0.5), w * ((k2 + 1) as f32 / stripes as f32 - 0.5));
                                let col = if k2 % 2 == 0 { c } else { lin(TRIM_WHITE) };
                                let (d0, d1) = (dir * x0, dir * x1);
                                let (top, lip) = (foot + Vec3::Y * 2.45 + out * 0.05, foot + Vec3::Y * 2.05 + out * 1.05);
                                k.cloth(&[top + d0, top + d1, lip + d1, lip + d0], col);
                                k.cloth(&[lip + d0, lip + d1, lip + d1 - Vec3::Y * 0.2, lip + d0 - Vec3::Y * 0.2], col);
                            }
                        }
                    }
                }
                // Cables slung across the lane to the building opposite.
                if len > 4.0 && r(20 + i as u32) < 0.35 {
                    let from = Vec3::new(mid.x, split + 1.2 + 1.5 * r(40 + i as u32), mid.y) + out * 0.1;
                    let reach = (2..18).find(|&d| occupied(mid + out2 * d as f32)).map(|d| d as f32 - 0.6);
                    if let Some(d) = reach.filter(|d| *d > 3.0) {
                        let to = from + out * d + Vec3::Y * (r(60 + i as u32) - 0.5);
                        if r(90 + i as u32) < 0.4 {
                            bunting(k, from, to, 0.25 + 0.05 * d, seed ^ i as u32);
                        } else {
                            for w in 0..2 + (r(80 + i as u32) * 2.0) as usize {
                                let sag = 0.4 + 0.25 * w as f32 + 0.1 * d;
                                let shift = dir * (w as f32 * 0.12);
                                wire(k, from + shift, to + shift, sag);
                            }
                        }
                    }
                }
            }
            along += len;
        }
        // Black water tanks on the roof.
        if r(30) < 0.55 && height > 3.0 {
            let k = clutter.entry((cx, cz)).or_default();
            let (lo, hi) = fp.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            for t in 0..1 + (r(31) * 2.5) as u32 {
                let p = c.lerp(lo + (hi - lo) * Vec2::new(r(32 + t), r(36 + t)), 0.5);
                if shared::map::footprint::contains(&fp.pts, p) {
                    let (radius, h) = (0.55 + 0.2 * r(33 + t), 1.0 + 0.4 * r(34 + t));
                    k.solid_cylinder(Vec3::new(p.x, fp.top + h * 0.5, p.y), radius, h, lin([0.07, 0.07, 0.08]));
                }
            }
        }
        let roof = roofs.entry((cx, cz)).or_default();
        let g = 0.84 + 0.1 * r(6);
        let roof_col = lin([g, g * 0.97, g * 0.92]);
        for [i, j, k] in shared::map::footprint::triangulate(&fp.pts) {
            let p = [fp.pts[i as usize], fp.pts[j as usize], fp.pts[k as usize]].map(|q| Vec3::new(q.x, fp.top, q.y));
            let p = if (p[1] - p[0]).cross(p[2] - p[0]).y >= 0.0 { p } else { [p[0], p[2], p[1]] };
            let base = roof.pos.len() as u32;
            for q in p {
                roof.pos.push(q.to_array());
                roof.nrm.push([0.0, 1.0, 0.0]);
                roof.uv.push([q.x / TILE, q.z / TILE]);
                roof.col.push(roof_col);
            }
            roof.idx.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
    let mut facade_mats: HashMap<(Mat, bool), Handle<StandardMaterial>> = HashMap::new();
    for ((_, _, mat, shop), buf) in walls {
        let texture = if shop { shopfront.clone() } else { facade.clone() };
        let m = facade_mats
            .entry((mat, shop))
            .or_insert_with(|| materials.add(StandardMaterial { base_color: if matches!(mat, Mat::Brick | Mat::RedSandstone | Mat::Sandstone) { tint(mat) } else { Color::WHITE }, base_color_texture: Some(texture), perceptual_roughness: 0.85, ..default() }))
            .clone();
        let (mesh, centre) = buf.into_mesh();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(m), Transform::from_translation(centre), OfMap));
    }
    // Signs, cables and tanks are small: drawn near only.
    let plain = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.7, ..default() });
    let near = bevy::camera::visibility::VisibilityRange { start_margin: 0.0..0.0, end_margin: CLUTTER_FAR..CLUTTER_FAR + 10.0, use_aabb: false };
    for (_, buf) in clutter {
        if buf.idx.is_empty() {
            continue;
        }
        let range = crate::characters::LodRange(near.clone(), 0);
        let (mesh, centre) = buf.into_mesh();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(plain.clone()), Transform::from_translation(centre), range.0.clone(), range, OfMap));
    }
    let mut trim_tris = 0;
    for (_, buf) in trims {
        trim_tris += buf.idx.len() / 3;
        let (mesh, centre) = buf.into_mesh();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(plain.clone()), Transform::from_translation(centre), OfMap));
    }
    info!("city: trim {trim_tris} triangles");
    for (_, buf) in roofs {
        let (mesh, centre) = buf.into_mesh();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(roof.clone()), Transform::from_translation(centre), OfMap));
    }
    if !water.idx.is_empty() {
        let (mesh, centre) = water.into_mesh();
        let m = materials.add(StandardMaterial {
            base_color: Color::srgba(0.16, 0.32, 0.30, 0.82),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.05,
            reflectance: 0.6,
            ..default()
        });
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(m), Transform::from_translation(centre), bevy::light::NotShadowCaster, OfMap));
    }
    info!("city: {} buildings from footprints", layout.footprints.len());
}

fn build_chunks(
    mut commands: Commands,
    layout: Res<MapLayout>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut pending = PendingMips::default();
    let mut sets = HashMap::new();
    for name in ["sandstone", "plaster", "brick", "granite", "wood", "thatch"] {
        sets.insert(name, load_set(&assets, &mut pending, name));
    }
    let mut groups: HashMap<(i32, i32, Mat, bool), MeshBuf> = HashMap::new();
    for b in layout.blocks.iter().filter(|b| is_static_visual(b)) {
        for coarse in [false, true] {
            let small = match b.shape {
                Shape::Box(s) => s.max_element() <= TRIM,
                Shape::Cylinder { radius, height } => (radius * 2.0).max(height) <= TRIM,
                Shape::Sphere(r) => r * 2.0 <= TRIM,
            };
            if coarse && small {
                continue;
            }
            let key = ((b.pos.x / CHUNK).floor() as i32, (b.pos.z / CHUNK).floor() as i32, b.mat, coarse);
            let buf = groups.entry(key).or_insert_with(|| MeshBuf { coarse, ..default() });
            match b.shape {
                Shape::Box(s) => buf.add_box(b, s),
                Shape::Cylinder { radius, height } => buf.add_cylinder(b, radius, height),
                Shape::Sphere(r) => buf.add_sphere(b, r),
            }
        }
    }
    let mut mat_cache: HashMap<Mat, Handle<StandardMaterial>> = HashMap::new();
    let n = groups.len();
    // Distances from the chunk's centre, where its mesh's origin is.
    let near = bevy::camera::visibility::VisibilityRange { start_margin: 0.0..0.0, end_margin: FAR_CHUNK..FAR_CHUNK + 10.0, use_aabb: false };
    let far = bevy::camera::visibility::VisibilityRange { start_margin: FAR_CHUNK..FAR_CHUNK + 10.0, end_margin: 1.0e5..1.0e5, use_aabb: false };
    for ((_, _, mat, coarse), buf) in groups {
        let handle = mat_cache.entry(mat).or_insert_with(|| material_for(mat, &sets, &mut materials)).clone();
        let range = crate::characters::LodRange(if coarse { far.clone() } else { near.clone() }, coarse as usize);
        let (mesh, centre) = buf.into_mesh();
        let mut e = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(handle),
            Transform::from_translation(centre),
            range.0.clone(),
            range,
            OfMap,
        ));
        if matches!(mat, Mat::Foliage | Mat::Cloth) {
            e.insert(bevy::light::NotShadowCaster);
        } else {
            e.insert(crate::quality::LodShadow);
        }
    }
    let roof = mat_cache.entry(Mat::Plaster).or_insert_with(|| material_for(Mat::Plaster, &sets, &mut materials)).clone();
    build_footprints(&mut commands, &layout, &mut meshes, &mut materials, &mut images, &mut pending, roof);
    info!("architecture: {} static blocks merged into {} chunk meshes", layout.blocks.iter().filter(|b| is_static_visual(b)).count(), n);
    commands.insert_resource(pending);
}

/// Model textures (the shared `models/textures/` set the props' glTFs point at) arrive without
/// mips; queue each one as it loads.
fn queue_model_mips(mut events: MessageReader<AssetEvent<Image>>, assets: Res<AssetServer>, mut pending: ResMut<PendingMips>) {
    for ev in events.read() {
        if let AssetEvent::Added { id } = ev
            && assets.get_path(*id).is_some_and(|p| p.path().starts_with("models/textures"))
            && let Some(h) = assets.get_id_handle(*id)
        {
            pending.0.push(h);
        }
    }
}

/// Give tiling textures a full mip chain (box-filtered on the CPU) and a repeating, anisotropic sampler.
///
/// Cut-out textures (leaf cards, with alpha tested at 0.5) are filtered so they keep their look at
/// a distance: colour is averaged by alpha, so the empty background doesn't darken the leaves, and
/// each level's alpha is scaled until the same share of texels passes the cutoff as at full size.
/// Plain averaging thins the alpha below the cutoff, so canopies go sparse and leaves blink.
fn mipmap_textures(mut pending: ResMut<PendingMips>, mut images: ResMut<Assets<Image>>) {
    pending.0.retain(|h| {
        let Some(mut img) = images.get_mut(h) else { return true };
        mip_chain(&mut img);
        false
    });
}

/// `mipmap_textures` for one image at hand.
pub(crate) fn mip_chain(img: &mut Image) {
    let (w, hgt) = (img.width() as usize, img.height() as usize);
    let Some(data) = img.data.clone() else { return };
    if data.len() != w * hgt * 4 {
        return; // only RGBA8 sources are handled
    }
    let coverage = |px: &[u8], scale: f32| px.chunks_exact(4).filter(|p| p[3] as f32 * scale >= 127.5).count() as f32 / (px.len() / 4) as f32;
    let full = coverage(&data, 1.0);
    let cutout = full < 0.98 && data.chunks_exact(4).any(|p| p[3] == 0);
    // `chain` holds the plain filtered levels; `levels` the ones uploaded (alpha rescaled).
    let mut chain: Vec<Vec<u8>> = vec![data.clone()];
    let mut levels: Vec<Vec<u8>> = vec![data];
    let (mut cw, mut ch) = (w, hgt);
    while cw > 1 && ch > 1 {
        let prev = chain.last().unwrap();
        let (nw, nh) = (cw / 2, ch / 2);
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                let taps = [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)].map(|(xx, yy)| ((yy * cw) + xx) * 4);
                let alpha: u32 = taps.iter().map(|&i| prev[i + 3] as u32).sum();
                let o = (y * nw + x) * 4;
                for c in 0..3 {
                    next[o + c] = if cutout && alpha > 0 {
                        (taps.iter().map(|&i| prev[i + c] as u32 * prev[i + 3] as u32).sum::<u32>() / alpha) as u8
                    } else {
                        (taps.iter().map(|&i| prev[i + c] as u32).sum::<u32>() / 4) as u8
                    };
                }
                next[o + 3] = (alpha / 4) as u8;
            }
        }
        let mut out = next.clone();
        if cutout {
            // Bisect the alpha scale that restores the full-size coverage.
            let (mut lo, mut hi) = (0.25f32, 8.0f32);
            for _ in 0..14 {
                let mid = (lo + hi) * 0.5;
                if coverage(&next, mid) < full { lo = mid } else { hi = mid }
            }
            let scale = (lo + hi) * 0.5;
            for p in out.chunks_exact_mut(4) {
                p[3] = (p[3] as f32 * scale).min(255.0) as u8;
            }
        }
        chain.push(next);
        levels.push(out);
        cw = nw;
        ch = nh;
    }
    img.texture_descriptor.mip_level_count = levels.len() as u32;
    img.data = Some(levels.concat());
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
}
