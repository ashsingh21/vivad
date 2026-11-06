//! Static architecture rendering: all non-dynamic map blocks are merged into chunked meshes (one
//! per 64 m chunk per material) with metric world-space UVs, so tiling building textures (plaster,
//! ashlar, brick, granite, wood, thatch) line up across blocks without stretching. Colliders stay
//! on the simulation's per-block entities.
//!
//! Each chunk is built twice: in full, and for past `FAR_CHUNK` with coarser domes and columns and
//! without the smallest trim blocks.
//!
//! Only what's seen from afar stays loaded: the coarse blocks, and the city's plain walls and roofs.
//! The rest (the full blocks, and the city's trim, window dressing, clutter and signs) is drawn near
//! only, so it's built per chunk as the camera comes within reach of it and freed when it leaves
//! (`stream`): on the NCR that's some 24 M vertices (1.5 GB) of which a few dozen chunks' worth are
//! ever in reach.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor, ImageAddressMode, ImageFilterMode};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use shared::map::interiors::{Cut, rects};
use shared::map::{Block, Footprint, MapLayout, Mat, Shape, ShopSign};
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
        app.init_resource::<PendingMips>()
            .add_systems(Startup, build_chunks.run_if(not(resource_exists::<shared::sim::DeferMap>)))
            .add_systems(Update, (build_chunks.run_if(map_reloaded), stream, queue_model_mips, mipmap_textures).chain());
    }
}

/// Textures that still need a CPU mip chain once loaded.
#[derive(Resource, Default)]
struct PendingMips(Vec<Handle<Image>>);

struct TexSet {
    base: Handle<Image>,
    normal: Handle<Image>,
}

/// A texture set under assets/textures/: `name` is its folder and prefix, e.g. "india/plaster".
fn load_set(assets: &AssetServer, pending: &mut PendingMips, name: &str) -> TexSet {
    let base = assets.load(crate::tex(&format!("textures/{name}_basecolor.png")));
    let normal = load_linear(assets, pending, &format!("textures/{name}_normal.png"));
    pending.0.push(base.clone());
    TexSet { base, normal }
}

/// A texture of data rather than colour (normals, roughness and metal): read as linear.
fn load_linear(assets: &AssetServer, pending: &mut PendingMips, path: &str) -> Handle<Image> {
    let h = assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| s.is_srgb = false).load(crate::tex(path));
    pending.0.push(h.clone());
    h
}

/// Which blocks the batcher draws (doors are dynamic and drawn per entity).
pub fn is_static_visual(b: &Block) -> bool {
    b.visible && !matches!(b.mat, Mat::Water)
}

/// Limewash and distemper hardly reflect: the default reflectance gives painted walls a sheen at
/// grazing angles, banded and blotchy on strong colours.
fn matte(h: Handle<StandardMaterial>, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    if let Some(mut m) = materials.get_mut(&h) {
        m.reflectance = 0.12;
    }
    h
}

/// A style guide colour token (assets/style/palette.json) as a colour.
fn tok(name: &str) -> Color {
    Color::srgb_from_array(shared::style::srgb(name))
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
        Mat::Buff => textured("ashlar", tok("architecture.buff"), 0.85, materials),
        // Coursed blocks, as the buff (the old grainy sandstone read as noise at a distance).
        Mat::RedSandstone => textured("ashlar", tok("architecture.red_sandstone"), 0.85, materials),
        Mat::Granite => textured("granite", Color::WHITE, 0.8, materials),
        Mat::Plaster => textured("plaster", Color::srgb(0.97, 0.95, 0.90), 0.9, materials),
        Mat::PlasterOchre => textured("plaster", Color::srgb(0.95, 0.74, 0.44), 0.9, materials),
        Mat::PlasterBlue => textured("plaster", Color::srgb(0.55, 0.72, 0.95), 0.9, materials),
        Mat::Marble => textured("plaster", Color::srgb(1.0, 0.99, 0.97), 0.35, materials),
        Mat::Brick => textured("brick", Color::WHITE, 0.9, materials),
        Mat::Wood => textured("panga_wood", tok("wood.cladding"), 0.75, materials),
        Mat::Thatch => textured("thatch", Color::WHITE, 0.95, materials),
        // Connaught Place: the style guide's tokens (docs/style-guide.md) over its painted textures.
        Mat::Stucco => matte(textured("stucco", tok("architecture.stucco"), 0.92, materials), materials),
        Mat::Stone => textured("stone", tok("architecture.floor_tile"), 0.6, materials),
        Mat::Rustic => matte(textured("rustic", tok("architecture.stucco"), 0.9, materials), materials),
        Mat::Plinth => textured("plinth", Color::WHITE, 0.85, materials),
        // The NCR's (crate::map::compose), plain for now: textured when the NCR is drawn properly.
        Mat::Concrete => materials.add(StandardMaterial { base_color: Color::srgb(0.64, 0.66, 0.64), perceptual_roughness: 0.92, ..default() }),
        Mat::Asphalt => materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.15, 0.17), perceptual_roughness: 0.95, ..default() }),
        Mat::Glass => materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.32, 0.40), metallic: 0.65, perceptual_roughness: 0.22, ..default() }),
        Mat::Steel => materials.add(StandardMaterial { base_color: Color::srgb(0.35, 0.41, 0.44), metallic: 0.7, perceptual_roughness: 0.4, ..default() }),
        Mat::Pavers => textured("stone", Color::srgb(0.72, 0.70, 0.66), 0.85, materials),
        Mat::ContainerRed => materials.add(StandardMaterial { base_color: Color::srgb(0.44, 0.22, 0.17), metallic: 0.35, perceptual_roughness: 0.65, ..default() }),
        Mat::Rubber => materials.add(StandardMaterial { base_color: Color::srgb(0.035, 0.04, 0.045), perceptual_roughness: 0.94, ..default() }),
        Mat::PaintWhite => materials.add(StandardMaterial { base_color: Color::srgb(0.74, 0.76, 0.74), metallic: 0.25, perceptual_roughness: 0.32, ..default() }),
        Mat::ShopSign | Mat::MetroSign => materials.add(StandardMaterial { base_color: Color::srgb(0.07, 0.2, 0.25), perceptual_roughness: 0.7, ..default() }),
        Mat::TransitBlue => materials.add(StandardMaterial { base_color: Color::srgb(0.14, 0.38, 0.52), ..default() }),
        Mat::TransitYellow => materials.add(StandardMaterial { base_color: Color::srgb(0.76, 0.60, 0.20), ..default() }),
        Mat::Paint(k) => {
            // Over the limewash's near-white mottle (tools/textures/city.py).
            let c = shared::map::paint(k);
            matte(textured("limewash", Color::srgb(c[0], c[1], c[2]), 0.95, materials), materials)
        }
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
            // Metric, from the face's own axes, so a tiling texture lines up along a run of them.
            let (wu, wv) = (rot * u, rot * v);
            let uv = p.map(|q| Vec2::new(q.dot(wu), -q.dot(wv)) / TILE);
            self.quad_col(p, rot * n, uv, [col; 4]);
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
        // Metric UVs: along the bar, and round its section.
        let x0 = c.dot(rot * Vec3::X);
        let mut round = 0.0;
        for k in 0..8 {
            let (s0, s1) = (sec[k], sec[(k + 1) % 8]);
            let n = rot * Vec3::new(0.0, -(s1.0 - s0.0), s1.1 - s0.1).normalize();
            // Undersides in their own shadow (baked occlusion): a ledge reads as a ledge.
            let k = if n.y < -0.5 { 0.6 } else if n.y < -0.1 { 0.8 } else { 1.0 };
            // Warmed a touch as it darkens: lit only by the blue sky, it would read as blue paint.
            let w = 1.0 - k;
            let c = [col[0] * k * (1.0 + 0.3 * w), col[1] * k, col[2] * k * (1.0 - 0.4 * w), col[3]];
            let step = Vec2::new(s1.0 - s0.0, s1.1 - s0.1).length();
            let (ua, ub, va, vb) = ((x0 - l) / TILE, (x0 + l) / TILE, round / TILE, (round + step) / TILE);
            round += step;
            self.quad_col([at(-l, s0), at(l, s0), at(l, s1), at(-l, s1)], n, [Vec2::new(ua, va), Vec2::new(ub, va), Vec2::new(ub, vb), Vec2::new(ua, vb)], [c; 4]);
        }
        for (x, n) in [(l, Vec3::X), (-l, Vec3::NEG_X)] {
            let base = self.pos.len() as u32;
            for q in sec {
                self.pos.push(at(x, q).to_array());
                self.nrm.push((rot * n).to_array());
                self.uv.push([q.0 / TILE, -q.1 / TILE]);
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
        // Soft edges near (docs/style-guide.md section 3): a bevel the light catches. On the trim
        // only (what's drawn and not walked into: cornices, plinths, beams, frames, slats): a
        // bevel cuts a groove where two boxes meet, and the walls' and slabs' grooves let the sun
        // through into the rooms in the shadow map.
        let bev = BEVEL.min(size.min_element() * 0.3);
        if !self.coarse && !b.collide && bev > 0.004 {
            self.bevelled_box(b, size, bev);
            return;
        }
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

    /// A box with its edges cut back by `bev`, the cut shaded round: each face inset, a strip along
    /// every edge whose normals run from one face's to the other's, a triangle at every corner.
    /// 44 triangles to the plain box's 12.
    fn bevelled_box(&mut self, b: &Block, size: Vec3, bev: f32) {
        let h = size * 0.5;
        let axis = |a: usize| [Vec3::X, Vec3::Y, Vec3::Z][a];
        // The texture's axes for a face (as `add_box`'s), by its normal.
        let frame = |n: Vec3| match (n.x, n.y, n.z) {
            (x, ..) if x > 0.5 => (Vec3::NEG_Z, Vec3::Y),
            (x, ..) if x < -0.5 => (Vec3::Z, Vec3::Y),
            (_, y, _) if y > 0.5 => (Vec3::X, Vec3::NEG_Z),
            (_, y, _) if y < -0.5 => (Vec3::X, Vec3::Z),
            (.., z) if z > 0.5 => (Vec3::X, Vec3::Y),
            _ => (Vec3::NEG_X, Vec3::Y),
        };
        // The vertex on face (axis `a`, side `sa`) toward corner signs `s`.
        let vert = |a: usize, sa: f32, s: [f32; 3]| -> (Vec3, Vec3) {
            let mut p = Vec3::ZERO;
            for k in 0..3 {
                p[k] = if k == a { sa * h[k] } else { s[k] * (h[k] - bev) };
            }
            (p, axis(a) * sa)
        };
        let mut emit = |verts: &[(Vec3, Vec3)]| {
            let out: Vec3 = verts.iter().map(|v| v.1).sum();
            let world: Vec<(Vec3, Vec3)> = verts.iter().map(|(p, n)| (b.pos + b.rot * *p, b.rot * *n)).collect();
            let base = self.pos.len() as u32;
            for (p, n) in &world {
                let (u, v) = frame(b.rot.inverse() * *n);
                let (wu, wv) = (b.rot * u, b.rot * v);
                self.pos.push(p.to_array());
                self.nrm.push(n.to_array());
                self.uv.push((Vec2::new(p.dot(wu), p.dot(wv)) / TILE).to_array());
            }
            let wrong = (verts[1].0 - verts[0].0).cross(verts[2].0 - verts[0].0).dot(out) < 0.0;
            for k in 1..verts.len() as u32 - 1 {
                if wrong {
                    self.idx.extend_from_slice(&[base, base + k + 1, base + k]);
                } else {
                    self.idx.extend_from_slice(&[base, base + k, base + k + 1]);
                }
            }
        };
        let sides = [-1.0f32, 1.0];
        for a in 0..3 {
            let (j, k) = ((a + 1) % 3, (a + 2) % 3);
            for sa in sides {
                // The face, inset.
                let at = |sj: f32, sk: f32| {
                    let mut s = [0.0; 3];
                    s[j] = sj;
                    s[k] = sk;
                    vert(a, sa, s)
                };
                emit(&[at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]);
                // The edges this face shares with the faces on axis j (the strip runs along k).
                for sj in sides {
                    let mut s0 = [0.0; 3];
                    s0[a] = sa;
                    s0[j] = sj;
                    let mut s1 = s0;
                    s0[k] = -1.0;
                    s1[k] = 1.0;
                    emit(&[vert(a, sa, s0), vert(a, sa, s1), vert(j, sj, s1), vert(j, sj, s0)]);
                }
            }
        }
        // The corners.
        for sx in sides {
            for sy in sides {
                for sz in sides {
                    let s = [sx, sy, sz];
                    emit(&[vert(0, sx, s), vert(1, sy, s), vert(2, sz, s)]);
                }
            }
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
        // Kept on the GPU only (nothing reads a building's mesh back; its bounds are cached): the
        // NCR's city is 2.5 GB of vertices, held twice otherwise.
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_indices(Indices::U32(self.idx));
        // The sky's occlusion baked in: a face turned down sees none of the sky, so the undersides of
        // cornices, sills, sunshades and ceilings come out darker even in shade, where the ambient
        // alone would light every face the same (docs/style-guide.md section 4: corner dark). Free
        // at run time.
        let sky = |n: &[f32; 3]| 1.0 - SKY_SHADE * (-n[1]).max(0.0);
        let col: Vec<[f32; 4]> = if self.col.is_empty() {
            self.nrm.iter().map(|n| { let k = sky(n); [k, k, k, 1.0] }).collect()
        } else {
            self.col.iter().zip(&self.nrm).map(|(c, n)| { let k = sky(n); [c[0] * k, c[1] * k, c[2] * k, c[3]] }).collect()
        };
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        let _ = m.generate_tangents();
        (m, c)
    }
}

/// How far the architecture's box edges are cut back near (metres; at most a third of a box's
/// thinnest side): docs/style-guide.md section 3's 4-8 cm.
const BEVEL: f32 = 0.05;

/// How much darker a face turned straight down is than one facing up or sideways (baked sky occlusion).
const SKY_SHADE: f32 = 0.26;

/// A city facade's bay width and storey height, in metres.
pub(crate) const BAY: f32 = 3.2;
/// How far signs, cables and roof tanks are drawn (scaled by quality like the chunks' LODs).
const CLUTTER_FAR: f32 = 130.0;
/// How far the city's trim (copings, cornices, ledges, pilasters) is drawn, from its chunk's middle.
const TRIM_FAR: f32 = 220.0;
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
/// Boards in the signs atlas (4 x 8), and a board's width (its height is a quarter and a bit).
const SIGN_BOARDS: usize = 32;
const SIGN_W: f32 = 3.6;
/// Off-white trim: parapet coping, cornices, pilasters, sills.
const TRIM_WHITE: [f32; 3] = [0.97, 0.94, 0.88];
/// How high a roof's parapet stands, and how thick it is.
const PARAPET: (f32, f32) = (0.9, 0.3);
/// Half the width of a corner pilaster: clear of the parapet and coping behind it, so no faces meet.
const PILASTER: f32 = 0.46;

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

/// A small painted stand-in for the facade texture (tools/textures/city.py), for the distant skyline:
/// two bays by two storeys, each window different (plain, shuttered, with an AC
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

/// The street base course on the city's buildings: its height, and how far it stands proud of
/// the wall (it's set as deep again into it).
const PLINTH_H: f32 = 0.55;
const PLINTH_DEEP: f32 = 0.08;

/// Where a city's buildings stand, a metre a cell: the tallest roof over each, 0 where nothing
/// stands. Over the whole map, or round one chunk's buildings (`Solid::around`).
struct Solid {
    span: f32,
    cells: i32,
    /// The cells held: from (i0, j0), w by h.
    i0: i32,
    j0: i32,
    w: i32,
    h: i32,
    roof: Vec<f32>,
}

impl Solid {
    fn span(layout: &MapLayout) -> f32 {
        layout.terrain.size.max(2.0 * layout.half_extent)
    }

    /// Cells from `lo` to `hi` of a map `span` across, with the buildings among `fps` that stand there.
    fn new<'a>(span: f32, lo: Vec2, hi: Vec2, fps: impl Iterator<Item = &'a Footprint>) -> Self {
        let cells = span.ceil() as i32;
        let cell = |p: Vec2| ((p.x + span * 0.5).floor() as i32, (p.y + span * 0.5).floor() as i32);
        let ((i0, j0), (i1, j1)) = (cell(lo), cell(hi));
        let (i0, j0, i1, j1) = (i0.max(0), j0.max(0), i1.min(cells - 1), j1.min(cells - 1));
        let (w, h) = ((i1 - i0 + 1).max(0), (j1 - j0 + 1).max(0));
        let mut solid = Self { span, cells, i0, j0, w, h, roof: vec![0.0; (w * h) as usize] };
        for fp in fps.filter(|f| f.mat != Mat::Water) {
            let (flo, fhi) = fp.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            for j in flo.y.floor().max(lo.y.floor() - 1.0) as i32..=fhi.y.ceil().min(hi.y.ceil() + 1.0) as i32 {
                for i in flo.x.floor().max(lo.x.floor() - 1.0) as i32..=fhi.x.ceil().min(hi.x.ceil() + 1.0) as i32 {
                    let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5);
                    if let Some(k) = solid.index(p)
                        && shared::map::footprint::contains(&fp.pts, p)
                    {
                        solid.roof[k] = solid.roof[k].max(fp.top.max(0.1));
                    }
                }
            }
        }
        solid
    }

    /// Round one chunk's buildings, as far out as a wall looks for the building across the lane.
    fn around(src: &Near, chunk: (i32, i32)) -> Self {
        const REACH: f32 = 20.0;
        let Some(own) = src.footprints.get(&chunk) else { return Self::new(src.span, Vec2::ZERO, Vec2::ZERO, std::iter::empty()) };
        let (lo, hi) = own.iter().flat_map(|&i| &src.all_footprints[i].pts).fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let (lo, hi) = (lo - Vec2::splat(REACH), hi + Vec2::splat(REACH));
        let (c0, c1) = ((lo / CHUNK).floor().as_ivec2(), (hi / CHUNK).floor().as_ivec2());
        let mut near: Vec<usize> = (c0.x..=c1.x).flat_map(|x| (c0.y..=c1.y).map(move |z| (x, z))).filter_map(|c| src.touching.get(&c)).flatten().copied().collect();
        near.sort_unstable();
        near.dedup();
        Self::new(src.span, lo, hi, near.into_iter().map(|i| &src.all_footprints[i]))
    }

    fn index(&self, p: Vec2) -> Option<usize> {
        let (i, j) = ((p.x + self.span * 0.5).floor() as i32, (p.y + self.span * 0.5).floor() as i32);
        let (x, y) = (i - self.i0, j - self.j0);
        (i >= 0 && j >= 0 && i < self.cells && j < self.cells && x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| (y * self.w + x) as usize)
    }

    fn at(&self, p: Vec2) -> f32 {
        self.index(p).map_or(0.0, |k| self.roof[k])
    }
}

/// The city's materials, made once a map.
struct CityMats {
    facade: HashMap<(Mat, bool), Handle<StandardMaterial>>,
    roof: Handle<StandardMaterial>,
    /// Trim, in the clean stucco.
    trim: Handle<StandardMaterial>,
    plinth: Handle<StandardMaterial>,
    plain: Handle<StandardMaterial>,
    lettered: Handle<StandardMaterial>,
    pane: Handle<StandardMaterial>,
    glow: Handle<StandardMaterial>,
    warm: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
}

fn city_mats(
    layout: &MapLayout,
    materials: &mut Assets<StandardMaterial>,
    images: &AssetServer,
    pending: &mut PendingMips,
    trim: Handle<StandardMaterial>,
    plinth: Handle<StandardMaterial>,
) -> CityMats {
    let assets = images;
    // The facades' painted relief (tools/textures/city.py): colour, normals, and roughness and metal
    // so glass and steel catch the sun.
    let facade = (load_set(assets, pending, "panga/facade"), load_linear(assets, pending, "textures/panga/facade_mr.png"));
    let shopfront = (load_set(assets, pending, "panga/shop"), load_linear(assets, pending, "textures/panga/shop_mr.png"));
    let roof_tex = (load_set(assets, pending, "panga/roof"), load_linear(assets, pending, "textures/panga/roof_mr.png"));
    let signs_tex: Handle<Image> = assets.load(crate::tex("textures/panga/signs.png"));
    pending.0.push(signs_tex.clone());
    let tint = |m: Mat| match m {
        Mat::PlasterOchre => Color::srgb(0.95, 0.76, 0.48),
        Mat::Sandstone => Color::srgb(0.85, 0.68, 0.50),
        Mat::RedSandstone => Color::srgb(0.78, 0.45, 0.36),
        Mat::Brick => Color::srgb(0.70, 0.40, 0.30),
        _ => Color::srgb(0.97, 0.95, 0.90),
    };
    let mut facade_mats = HashMap::new();
    for mat in layout.footprints.iter().map(|f| f.mat).filter(|m| *m != Mat::Water) {
        for shop in [false, true] {
            if facade_mats.contains_key(&(mat, shop)) {
                continue;
            }
            let (set, mr) = if shop { &shopfront } else { &facade };
            facade_mats.insert(
                (mat, shop),
                materials.add(StandardMaterial {
                    base_color: if matches!(mat, Mat::Brick | Mat::RedSandstone | Mat::Sandstone) { tint(mat) } else { Color::WHITE },
                    base_color_texture: Some(set.base.clone()),
                    normal_map_texture: Some(set.normal.clone()),
                    metallic_roughness_texture: Some(mr.clone()),
                    // Both scaled by the texture's.
                    perceptual_roughness: 1.0,
                    metallic: 1.0,
                    // Plaster hardly reflects (matte()): at the default, the sky's sheen off the
                    // normal map's trowel waves blotched shaded walls a deep blue-violet.
                    reflectance: 0.12,
                    ..default()
                }),
            );
        }
    }
    let (set, mr) = roof_tex;
    let roof = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(set.base),
        normal_map_texture: Some(set.normal),
        metallic_roughness_texture: Some(mr),
        perceptual_roughness: 1.0,
        ..default()
    });
    let plain = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.7, ..default() });
    let lettered = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(signs_tex), perceptual_roughness: 0.6, ..default() });
    // Shop windows: faintly tinted, glossy, see-through to the lit shop behind.
    let pane = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.66, 0.70, 0.22),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.06,
        reflectance: 0.6,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    // Warm white, bright enough to bloom: shops lit inside, seen from the verandah. They light
    // nothing (no light sources): a glow only.
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.97, 0.9),
        emissive: LinearRgba::new(6.0, 5.6, 4.8, 1.0),
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    // A round falloff, added over what's there: no light sources, no shadows, one draw a chunk.
    let n = 64usize;
    let fall = image_from(n, n, |x, y| {
        let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32 * 2.0 - Vec2::ONE;
        let k = (1.0 - d.length()).clamp(0.0, 1.0);
        [k * k * (3.0 - 2.0 * k); 3]
    });
    let warm = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.76, 0.48, 0.5),
        base_color_texture: Some(images.add(fall)),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.16, 0.32, 0.30, 0.82),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.05,
        reflectance: 0.6,
        ..default()
    });
    CityMats { facade: facade_mats, roof, trim, plinth, plain, lettered, pane, glow, warm, water }
}

/// A city's meshes, per chunk (and material), before they're made into meshes.
#[derive(Default)]
struct CityBufs {
    walls: HashMap<(i32, i32, Mat, bool), MeshBuf>,
    /// The windows', ACs' and shops' faces over the walls (drawn near only).
    overlays: HashMap<(i32, i32, Mat, bool), MeshBuf>,
    roofs: HashMap<(i32, i32), MeshBuf>,
    trims: HashMap<(i32, i32), MeshBuf>,
    clutter: HashMap<(i32, i32), MeshBuf>,
    plinths: HashMap<(i32, i32), MeshBuf>,
    signs: HashMap<(i32, i32), MeshBuf>,
    lights: HashMap<(i32, i32), MeshBuf>,
    glass: HashMap<(i32, i32), MeshBuf>,
    pools: HashMap<(i32, i32), MeshBuf>,
    water: MeshBuf,
}

/// A city map's buildings (MapLayout::footprints, those numbered `fps`), merged per chunk and
/// material: walls in the facade, roofs in clay tile, trim in plaster. What's seen from afar (the
/// plain walls, roofs and water), or else the rest, drawn near only (`detail`). `solid` must hold
/// every building within reach of these.
/// A hollow building's holes (shared::map::interiors::Interior::skin): on each edge of its outline,
/// along the edge from its first corner; and whether its rooms draw its roof (it has no tower
/// standing on them).
type Skin = ([Vec<Cut>; 4], bool);

fn city_bufs(footprints: &[Footprint], fps: &[usize], solid: &Solid, detail: bool, skins: &HashMap<usize, Skin>) -> CityBufs {
    let roof_at = |p: Vec2| solid.at(p);
    let occupied = |p: Vec2| roof_at(p) > 0.0;
    let lin = |c: [f32; 3]| {
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        [l.red, l.green, l.blue, 1.0]
    };
    let mut out_bufs = CityBufs::default();
    let CityBufs { walls, overlays, roofs, trims, clutter, plinths, signs, water, .. } = &mut out_bufs;
    // (The base course along the street, Mat::Plinth's dressed stone, has its own buffers and
    // material.)
    for (fi, fp) in fps.iter().map(|&i| (i, &footprints[i])) {
        let skin = skins.get(&fi);
        let c = fp.pts.iter().sum::<Vec2>() / fp.pts.len() as f32;
        let (cx, cz) = ((c.x / CHUNK).floor() as i32, (c.y / CHUNK).floor() as i32);
        // Water is only its surface.
        if fp.mat == Mat::Water {
            if detail {
                continue;
            }
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
        let painted = !matches!(fp.mat, Mat::Brick | Mat::RedSandstone | Mat::Sandstone | Mat::Granite | Mat::Buff);
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
        // Standing on a hall (crate::map::delhi's real ground floor): no painted shops, the
        // sunshade over the hall, windows from the first storey up, and a plaster ceiling under.
        let raised = fp.base > 0.5;
        // A row of shops along the street when there's a storey to spare above them.
        let shops = !raised && height > STOREY * 1.6 && r(3) < 0.8;
        let split = if raised { fp.base } else { (fp.base + STOREY).min(fp.top) };
        if raised && detail {
            let t = trims.entry((cx, cz)).or_default();
            // The ceiling token, made up for the sky shading every face turned down gets (into_mesh).
            let l = tok("architecture.ceiling").to_linear();
            let k = 1.0 / (1.0 - SKY_SHADE);
            let ceiling = [l.red * k, l.green * k, l.blue * k, 1.0];
            for [i, j, k] in shared::map::footprint::triangulate(&fp.pts) {
                let p = [fp.pts[i as usize], fp.pts[j as usize], fp.pts[k as usize]].map(|q| Vec3::new(q.x, fp.base, q.y));
                let p = if (p[1] - p[0]).cross(p[2] - p[0]).y <= 0.0 { p } else { [p[0], p[2], p[1]] };
                let base = t.pos.len() as u32;
                for q in p {
                    t.pos.push(q.to_array());
                    t.nrm.push([0.0, -1.0, 0.0]);
                    t.uv.push([q.x / 4.0, q.z / 4.0]);
                    t.col.push(ceiling);
                }
                t.idx.extend_from_slice(&[base, base + 1, base + 2]);
            }
        }
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
            // What's left of a stretch of this wall round a hollow building's holes.
            let cuts: &[Cut] = skin.and_then(|k| k.0.get(i)).map_or(&[], |c| c.as_slice());
            let holed = |s: (f32, f32), y: (f32, f32)| if cuts.is_empty() { vec![[s.0, s.1, y.0, y.1]] } else { rects(s, y, cuts) };
            if detail {
                let t = trims.entry((cx, cz)).or_default();
                // The parapet round the roof, and its coping.
                let (ph, pt) = PARAPET;
                t.open_box(at(len * 0.5, fp.top + ph * 0.5) - out * pt * 0.5, rot, Vec3::new(len, ph, pt), tint_at(1.06));
                // Chunky, as a stylised city's trim is: a thick rounded coping overhanging both
                // faces of the parapet...
                t.prism(at(len * 0.5, fp.top + ph + 0.11) - out * 0.17, rot, Vec3::new(len, 0.22, 0.56), 0.08, trim);
                // ...and a fat cornice under the roofline, run on round outside corners and stopped
                // short of inside ones.
                {
                    let ext = |c: i8| c as f32 * 0.3;
                    let (s0, s1) = (-ext(corner[i]), len + ext(corner[(i + 1) % n]));
                    t.prism(at((s0 + s1) * 0.5, fp.top - 0.17) + out * 0.11, rot, Vec3::new(s1 - s0, 0.3, 0.32), 0.11, trim);
                }
                // A chhajja over the ground storey: a deep sunshade ledge, run on round outside
                // corners to meet the next wall's and stopped short of inside ones.
                if fp.top - split > 1.0 {
                    const DEEP: f32 = 0.75;
                    let ext = |c: i8| c as f32 * DEEP;
                    let (s0, s1) = (-ext(corner[i]), len + ext(corner[(i + 1) % n]));
                    t.prism(at((s0 + s1) * 0.5, split + 0.04) + out * (DEEP - 0.1) * 0.5, rot, Vec3::new(s1 - s0, 0.3, DEEP + 0.1), 0.1, trim);
                }
                // A base course of dressed stone along the street at a building's foot, under a
                // capping ledge (as Connaught Place's, crate::map::cp), half set into the wall;
                // run round outside corners and stopped short of inside ones.
                if street && fp.base < 0.5 {
                    let ext = |c: i8| c as f32 * PLINTH_DEEP;
                    let (s0, s1) = (-ext(corner[i]), len + ext(corner[(i + 1) % n]));
                    let p = plinths.entry((cx, cz)).or_default();
                    let stone = [v, v, v, 1.0];
                    // Broken at a hollow building's doorways (5 cm clear of each side).
                    let mut runs = vec![];
                    let mut from = s0;
                    let mut doors: Vec<&Cut> = cuts.iter().filter(|c| c.y0 < fp.base + PLINTH_H + 0.1).collect();
                    doors.sort_by(|x, y| x.s0.total_cmp(&y.s0));
                    for c in doors {
                        runs.push((from, c.s0 - 0.05));
                        from = from.max(c.s1 + 0.05);
                    }
                    runs.push((from, s1));
                    for (r0, r1) in runs.into_iter().filter(|(r0, r1)| r1 - r0 > 0.1) {
                        p.prism(at((r0 + r1) * 0.5, fp.base + PLINTH_H * 0.5), rot, Vec3::new(r1 - r0, PLINTH_H, 2.0 * PLINTH_DEEP), 0.03, stone);
                        p.prism(at((r0 + r1) * 0.5, fp.base + PLINTH_H + 0.03), rot, Vec3::new(r1 - r0 + 0.04, 0.07, 2.0 * PLINTH_DEEP + 0.07), 0.025, stone);
                    }
                }
                // A pilaster up each outside corner.
                if corner[i] > 0 {
                    let h = fp.top - fp.base + ph + 0.3;
                    let up = rot * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                    t.prism(Vec3::new(a.x, fp.base + h * 0.5, a.y), up, Vec3::new(h, 2.0 * PILASTER, 2.0 * PILASTER), 0.13, trim);
                }
            }
            // Sills and lintels at each window of the upper storeys, where they can be seen,
            // where the window texture puts them (tools/textures/city.py): a window a bay,
            // its sill 0.75-0.92 m up a storey, its head at 2.9-3.06 m; the balcony's opening goes
            // down to a slab at 0.62 m.
            if detail && fp.top > screened + STOREY {
                let k = clutter.entry((cx, cz)).or_default();
                let first = ((along + 0.6) / BAY).ceil() as i32;
                let last = ((along + len - 0.6) / BAY).floor() as i32;
                let storeys = ((fp.top - fp.base) / STOREY + 0.01).floor() as i32;
                for bay in first..last {
                    let s = (bay as f32 + 0.5) * BAY - along;
                    for st in (!raised as i32)..storeys {
                        let floor = fp.base + st as f32 * STOREY;
                        if floor < screened {
                            continue;
                        }
                        // The texture's two-by-two repeat: which of its windows this is.
                        let kind = [[0, 2], [3, 1]][(st % 2 == 0) as usize][bay.rem_euclid(2) as usize];
                        if kind == 3 {
                            k.prism(at(s, floor + 0.62) + out * 0.32, rot, Vec3::new(2.3, 0.2, 0.7), 0.07, trim);
                        } else {
                            k.prism(at(s, floor + 0.84) + out * 0.12, rot, Vec3::new(1.95, 0.2, 0.34), 0.07, trim);
                        }
                        k.prism(at(s, floor + 3.02) + out * 0.1, rot, Vec3::new(2.0, 0.22, 0.28), 0.08, trim);
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
                let w = if detail { overlays.entry((cx, cz, fp.mat, shop)).or_default() } else { walls.entry((cx, cz, fp.mat, shop)).or_default() };
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
                for pair in stops.windows(2).filter(|_| !detail) {
                    let ((d0, k0), (d1, k1)) = (pair[0], pair[1]);
                    let dim = |c: [f32; 4], k: f32| [c[0] * k, c[1] * k, c[2] * k, c[3]];
                    // Each piece of the stretch left round any holes, its texture and shading
                    // where the whole stretch's would be.
                    for [sa, sb, ya, yb] in holed((d0, d1), (lo, hi)) {
                        let (p0, p1) = (a0 + (b0 - a0) * (sa / len), a0 + (b0 - a0) * (sb / len));
                        let (p0, p1) = (p0.with_y(ya), p1.with_y(ya));
                        let (q0, q1) = (p0.with_y(yb), p1.with_y(yb));
                        let (e0, e1) = (u0 + (u1 - u0) * sa / len, u0 + (u1 - u0) * sb / len);
                        let (va, vb) = (v0 + (v1 - v0) * (ya - lo) / (hi - lo), v0 + (v1 - v0) * (yb - lo) / (hi - lo));
                        let uv = [Vec2::new(e0, va), Vec2::new(e1, va), Vec2::new(e1, vb), Vec2::new(e0, vb)];
                        let k = |s: f32| k0 + (k1 - k0) * ((s - d0) / (d1 - d0).max(1e-4));
                        let tint = |y: f32| tint_at(s0 + (s1 - s0) * (y - lo) / (hi - lo));
                        let col = [dim(tint(ya), k(sa)), dim(tint(ya), k(sb)), dim(tint(yb), k(sb)), dim(tint(yb), k(sa))];
                        if !flip {
                            w.quad_col([p0, p1, q1, q0], out, uv, col);
                        } else {
                            w.quad_col([p1, p0, q0, q1], out, [uv[1], uv[0], uv[3], uv[2]], [col[1], col[0], col[3], col[2]]);
                        }
                    }
                }
                // The wall's colour is for its plaster: over each window, AC unit, balcony and
                // shop front (where tools/textures/city.py paints them in each one-bay, one-storey
                // cell) the same texture again, a hair proud of the wall, shaded but not tinted.
                // Only where it can be seen over the neighbour.
                const KINDS: [&[[f32; 4]]; 4] = [
                    &[[0.25, 0.75, 0.13, 0.73]],
                    &[[0.10, 0.90, 0.13, 0.73]],
                    &[[0.25, 0.75, 0.13, 0.73], [0.34, 0.66, 0.79, 0.94]],
                    &[[0.25, 0.75, 0.13, 0.73], [0.20, 0.80, 0.73, 0.80]],
                ];
                const SHOP: &[[f32; 4]] = &[[0.06, 0.94, 0.20, 1.0]];
                const PROUD: f32 = 0.015;
                let grey = |c: [f32; 4]| {
                    let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    [l, l, l, 1.0]
                };
                let storeys = if detail { ((hi - fp.base) / STOREY + 0.01).floor() as i32 } else { i32::MIN };
                for st in ((lo - fp.base) / STOREY + 0.01).floor() as i32..storeys {
                    let floor = fp.base + st as f32 * STOREY;
                    if floor + STOREY < screened {
                        continue;
                    }
                    for bay in (along / BAY).ceil() as i32..((along + len) / BAY).floor() as i32 {
                        let rects = if shop { SHOP } else { KINDS[[[0, 2], [3, 1]][(st % 2 == 0) as usize][bay.rem_euclid(2) as usize]] };
                        for &[ru0, ru1, rv0, rv1] in rects {
                            let (sa, sb) = ((bay as f32 + ru0) * BAY - along, (bay as f32 + ru1) * BAY - along);
                            // The cell's v runs from its top (0) to the floor (1).
                            let (ya, yb) = (floor + (1.0 - rv1) * STOREY, floor + (1.0 - rv0) * STOREY);
                            for [sa, sb, ya, yb] in holed((sa, sb), (ya, yb)) {
                            let at = |s: f32, y: f32| Vec3::new(a.x, y, a.y) + dir * s + out * PROUD;
                            let uv = |s: f32, y: f32| Vec2::new((along + s) / (2.0 * BAY), -(y - fp.base) / rows);
                            let shade = |y: f32| grey(tint_at(s0 + (s1 - s0) * ((y - lo) / (hi - lo)).clamp(0.0, 1.0)));
                            let (p0, p1, q1, q0) = (at(sa, ya), at(sb, ya), at(sb, yb), at(sa, yb));
                            let (t0, t1, t2, t3) = (uv(sa, ya), uv(sb, ya), uv(sb, yb), uv(sa, yb));
                            let (c0, c1) = (shade(ya), shade(yb));
                            if !flip {
                                w.quad_col([p0, p1, q1, q0], out, [t0, t1, t2, t3], [c0, c0, c1, c1]);
                            } else {
                                w.quad_col([p1, p0, q0, q1], out, [t1, t0, t3, t2], [c0, c0, c1, c1]);
                            }
                            }
                        }
                    }
                }
            }
            // Clutter on walls that face the street (not a neighbour's wall).
            if street && detail {
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
                        // One of the shops in the signs atlas (tools/textures/signs.py), on a board of
                        // its own colour: the face lettered, the edges plain.
                        let shop = (hash01(seed, s as u32, 5) * SIGN_BOARDS as f32) as usize % SIGN_BOARDS;
                        let board = ACCENTS[shop % ACCENTS.len()];
                        let p = Vec3::new(at.x, y, at.y) + out * 0.07;
                        let (bw, bh) = (SIGN_W, SIGN_W / 4.2);
                        k.solid_box(p, rot, Vec3::new(bw + 0.08, bh + 0.08, 0.1), lin(board));
                        let (col, row) = ((shop % 4) as f32, (shop / 4) as f32);
                        let (u0, u1, v0, v1) = (col / 4.0, (col + 1.0) / 4.0, row / 8.0, (row + 1.0) / 8.0);
                        let face = p + out * 0.051;
                        let right = (-out).cross(Vec3::Y) * (bw * 0.5);
                        let up = Vec3::Y * (bh * 0.5);
                        signs.entry((cx, cz)).or_default().quad(
                            [face - right - up, face + right - up, face + right + up, face - right + up],
                            out,
                            [Vec2::new(u0, v1), Vec2::new(u1, v1), Vec2::new(u1, v0), Vec2::new(u0, v0)],
                        );
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
        if detail && r(30) < 0.55 && height > 3.0 {
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
        if detail {
            continue;
        }
        // A hollow building's roof is its rooms' (the slab with the stair's way up through it).
        if skin.is_some_and(|k| k.1) {
            continue;
        }
        let roof = roofs.entry((cx, cz)).or_default();
        // Clay tiles left bare, grey cement over them, or lime-washed white, a little lighter or darker.
        let g = 0.9 + 0.12 * r(6);
        let roof_col = lin([[0.86, 0.56, 0.44], [0.72, 0.72, 0.70], [0.95, 0.93, 0.88]][(r(7) * 2.99) as usize].map(|c| c * g));
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
    out_bufs
}

/// The signboards, lights and shop windows numbered `idx` among MapLayout::signs.
fn sign_bufs(all: &[ShopSign], idx: &[usize], bufs: &mut CityBufs) {
    let lin = |c: [f32; 3]| {
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        [l.red, l.green, l.blue, 1.0]
    };
    // Signboards, lights and shop windows set into hand-built blocks (Connaught Place's ring):
    // boards in the festival colours lettered from the signs atlas, like the shops' above (the
    // shelves of goods are the street kit's racks).
    // The tube lights' glow where it falls: a soft warm pool on the ceiling round each tube and on
    // the floor under it (SignKind::Pool) (the verandahs and shops are lit by the sky alone, cool blue-grey).
    let CityBufs { lights, glass, pools, clutter, signs, .. } = bufs;
    for (n, sg) in idx.iter().map(|&n| (n, &all[n])) {
        let key = ((sg.pos.x / CHUNK).floor() as i32, (sg.pos.z / CHUNK).floor() as i32);
        let out = Vec3::new(sg.facing.x, 0.0, sg.facing.y).normalize_or(Vec3::X);
        let right = (-out).cross(Vec3::Y) * (sg.width * 0.5);
        let up = Vec3::Y * (sg.height * 0.5);
        if sg.kind == shared::map::SignKind::Light {
            // A tube under the ceiling, its face down, running along `facing`.
            let along = out * (sg.height * 0.5);
            let across = Vec3::Y.cross(out) * (sg.width * 0.5);
            let p = sg.pos;
            lights.entry(key).or_default().quad([p - along - across, p - along + across, p + along + across, p + along - across], Vec3::NEG_Y, [Vec2::ZERO; 4]);
            let uv = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)];
            let (l, a) = (out * (sg.height * 0.5 + 1.0), Vec3::Y.cross(out) * 1.4);
            let c = p + Vec3::Y * 0.02;
            pools.entry(key).or_default().quad([c - l - a, c - l + a, c + l + a, c + l - a], Vec3::NEG_Y, uv);
            continue;
        }
        if sg.kind == shared::map::SignKind::Pool {
            let (l, a, f) = (out * (sg.height * 0.5), Vec3::Y.cross(out) * (sg.width * 0.5), sg.pos);
            let uv = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)];
            pools.entry(key).or_default().quad([f - l + a, f - l - a, f + l - a, f + l + a], Vec3::Y, uv);
            continue;
        }
        if sg.kind == shared::map::SignKind::Glass {
            glass.entry(key).or_default().quad([sg.pos - right - up, sg.pos + right - up, sg.pos + right + up, sg.pos - right + up], out, [Vec2::ZERO; 4]);
            continue;
        }
        let mut shop = (hash01(n as u32, 17, 23) * SIGN_BOARDS as f32) as usize % SIGN_BOARDS;
        if sg.kind == shared::map::SignKind::Hanging {
            // One of the atlas's red boards (board k is ACCENTS[k % 8]; 4 is sindoor red).
            shop = shop / ACCENTS.len() * ACCENTS.len() + 4;
        }
        let p = sg.pos + out * 0.05;
        let rot = Quat::from_rotation_arc(Vec3::X, right.normalize());
        // Hung between columns: always red, as Connaught Place's are.
        let board = if sg.kind == shared::map::SignKind::Hanging { ACCENTS[4] } else { ACCENTS[shop % ACCENTS.len()] };
        clutter.entry(key).or_default().solid_box(p, rot, Vec3::new(sg.width + 0.08, sg.height + 0.08, 0.1), lin(board));
        let (col, row) = ((shop % 4) as f32, (shop / 4) as f32);
        let (u0, u1, v0, v1) = (col / 4.0, (col + 1.0) / 4.0, row / 8.0, (row + 1.0) / 8.0);
        let face = p + out * 0.051;
        // The atlas boards are 4:1; a squatter board shows the middle of its lettering.
        let crop = ((sg.width / sg.height) / 4.0).min(1.0);
        let (cu0, cu1) = (u0 + (u1 - u0) * (1.0 - crop) * 0.5, u1 - (u1 - u0) * (1.0 - crop) * 0.5);
        signs.entry(key).or_default().quad(
            [face - right - up, face + right - up, face + right + up, face - right + up],
            out,
            [Vec2::new(cu0, v1), Vec2::new(cu1, v1), Vec2::new(cu1, v0), Vec2::new(cu0, v0)],
        );
    }
}

/// Which of the city's materials a mesh is drawn in.
#[derive(Clone, Copy)]
enum Look {
    Facade(Mat, bool),
    Block(Mat),
    Roof,
    Trim,
    Plinth,
    Plain,
    Lettered,
    Pane,
    Glow,
    Warm,
    Water,
}

/// A mesh made, not yet spawned (made off the main thread for the streamed chunks): its middle,
/// material, how far it's drawn (from its middle; none: always), and its shadow (`spawn_pieces`).
struct Piece {
    mesh: Mesh,
    centre: Vec3,
    look: Look,
    range: Option<VisibilityRange>,
    shadow: Option<bool>,
}

/// Makes a city's buffers into meshes.
fn city_pieces(bufs: CityBufs, out: &mut Vec<Piece>) {
    let mut put = |buf: MeshBuf, look: Look, range: Option<f32>, shadow: Option<bool>| {
        if buf.idx.is_empty() {
            return;
        }
        let (mesh, centre) = buf.into_mesh();
        let range = range.map(|end| VisibilityRange { start_margin: 0.0..0.0, end_margin: end..end + 10.0, use_aabb: false });
        out.push(Piece { mesh, centre, look, range, shadow });
    };
    for ((_, _, mat, shop), buf) in bufs.walls {
        put(buf, Look::Facade(mat, shop), None, None);
    }
    // The windows' faces: drawn as far as the trim.
    for ((_, _, mat, shop), buf) in bufs.overlays {
        put(buf, Look::Facade(mat, shop), Some(TRIM_FAR), None);
    }
    // Signs, cables and tanks are small: drawn near only.
    for (_, buf) in bufs.clutter {
        put(buf, Look::Plain, Some(CLUTTER_FAR), None);
    }
    for (_, buf) in bufs.signs {
        put(buf, Look::Lettered, Some(CLUTTER_FAR), None);
    }
    for (_, buf) in bufs.glass {
        put(buf, Look::Pane, Some(CLUTTER_FAR), Some(false));
    }
    for (_, buf) in bufs.lights {
        put(buf, Look::Glow, Some(CLUTTER_FAR), Some(false));
    }
    for (_, buf) in bufs.pools {
        put(buf, Look::Warm, Some(CLUTTER_FAR), Some(false));
    }
    // Trim is chunky and many-sided: drawn to TRIM_FAR (scaled by quality), past which the plain
    // walls and roofs read just as well.
    for (_, buf) in bufs.trims {
        put(buf, Look::Trim, Some(TRIM_FAR), None);
    }
    for (_, buf) in bufs.plinths {
        put(buf, Look::Plinth, Some(TRIM_FAR), Some(true));
    }
    for (_, buf) in bufs.roofs {
        put(buf, Look::Roof, None, None);
    }
    put(bufs.water, Look::Water, None, Some(false));
}

/// Spawns made meshes: `shadow` Some(false) never casts one, Some(true) as the quality level says
/// (crate::quality::LodShadow), none always. Returns what it spawned.
fn spawn_pieces(commands: &mut Commands, meshes: &mut Assets<Mesh>, city: &City, pieces: Vec<Piece>, k: f32) -> Vec<Entity> {
    pieces
        .into_iter()
        .filter_map(|p| {
            let mat = match p.look {
                Look::Block(m) => city.blocks.get(&m),
                look => city.mats.as_ref().map(|c| match look {
                    Look::Facade(m, shop) => &c.facade[&(m, shop)],
                    Look::Roof => &c.roof,
                    Look::Trim => &c.trim,
                    Look::Plinth => &c.plinth,
                    Look::Plain => &c.plain,
                    Look::Lettered => &c.lettered,
                    Look::Pane => &c.pane,
                    Look::Glow => &c.glow,
                    Look::Warm => &c.warm,
                    Look::Water | Look::Block(_) => &c.water,
                }),
            }?;
            let mut e = commands.spawn((Mesh3d(meshes.add(p.mesh)), MeshMaterial3d(mat.clone()), Transform::from_translation(p.centre), OfMap));
            if let Some(r) = p.range {
                let base = crate::characters::LodRange(r, 0);
                e.insert((base.scaled(k), base));
            }
            match p.shadow {
                Some(false) => {
                    e.insert(bevy::light::NotShadowCaster);
                }
                Some(true) => {
                    e.insert(crate::quality::LodShadow);
                }
                None => {}
            }
            Some(e.id())
        })
        .collect()
}

/// What the near-only meshes are made from, shared with the tasks that make them.
#[derive(Default)]
struct Near {
    span: f32,
    /// The full blocks in each chunk.
    blocks: HashMap<(i32, i32), Vec<(Block, bool)>>,
    all_footprints: Vec<Footprint>,
    all_signs: Vec<ShopSign>,
    /// The hollow buildings' holes, by `all_footprints` index.
    skins: HashMap<usize, Skin>,
    /// The buildings (by `all_footprints` index) whose middle is in each chunk...
    footprints: HashMap<(i32, i32), Vec<usize>>,
    /// ...and those reaching into it at all.
    touching: HashMap<(i32, i32), Vec<usize>>,
    signs: HashMap<(i32, i32), Vec<usize>>,
}

/// What's drawn near only, per chunk, built as the camera comes within reach (`stream`).
#[derive(Resource, Default)]
struct City {
    mats: Option<CityMats>,
    blocks: HashMap<Mat, Handle<StandardMaterial>>,
    src: std::sync::Arc<Near>,
    /// Every chunk with anything near-only in it.
    chunks: Vec<(i32, i32)>,
    /// Chunks built, and what they spawned...
    loaded: HashMap<(i32, i32), Vec<Entity>>,
    /// ...and those being made.
    making: HashMap<(i32, i32), bevy::tasks::Task<Vec<Piece>>>,
}

/// How far out a chunk's near-only meshes are built (from the chunk's square; they're drawn to
/// TRIM_FAR from their middle, scaled by quality), and how far beyond that they're freed again.
const STREAM_SLACK: f32 = 40.0;
const STREAM_KEEP: f32 = 60.0;
/// Chunks made at once (off the main thread), nearest first.
const STREAM_TASKS: usize = 6;

fn chunk_dist(c: (i32, i32), p: Vec2) -> f32 {
    let lo = Vec2::new(c.0 as f32, c.1 as f32) * CHUNK;
    (p - p.clamp(lo, lo + Vec2::splat(CHUNK))).length()
}

/// Makes the near-only meshes of the chunks the camera comes within reach of (off the main thread),
/// spawns them as they're done, and frees those it has left.
fn stream(
    mut commands: Commands,
    city: Option<ResMut<City>>,
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    scale: Res<crate::quality::LodScale>,
) {
    let Some(mut city) = city else { return };
    let Ok(cam) = cam.single() else { return };
    let at = cam.translation().xz();
    let reach = TRIM_FAR.max(FAR_CHUNK).max(CLUTTER_FAR) * scale.0 + STREAM_SLACK;
    let city = &mut *city;
    // Done ones spawned, unless the camera has left them meanwhile.
    let done: Vec<(i32, i32)> = city.making.iter_mut().filter(|(_, t)| t.is_finished()).map(|(c, _)| *c).collect();
    for c in done {
        let task = city.making.remove(&c).unwrap();
        let pieces = bevy::tasks::block_on(task);
        if chunk_dist(c, at) <= reach + STREAM_KEEP {
            let spawned = spawn_pieces(&mut commands, &mut meshes, city, pieces, scale.0);
            city.loaded.insert(c, spawned);
        }
    }
    let gone: Vec<(i32, i32)> = city.loaded.keys().filter(|c| chunk_dist(**c, at) > reach + STREAM_KEEP).copied().collect();
    for c in gone {
        for e in city.loaded.remove(&c).unwrap_or_default() {
            commands.entity(e).despawn();
        }
    }
    // Dropping a task cancels it.
    city.making.retain(|c, _| chunk_dist(*c, at) <= reach + STREAM_KEEP);
    if city.making.len() >= STREAM_TASKS {
        return;
    }
    let mut wanted: Vec<((i32, i32), f32)> = city.chunks.iter().filter(|c| !city.loaded.contains_key(c) && !city.making.contains_key(c)).map(|c| (*c, chunk_dist(*c, at))).filter(|(_, d)| *d < reach).collect();
    wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
    let pool = bevy::tasks::AsyncComputeTaskPool::get();
    for (c, _) in wanted.into_iter().take(STREAM_TASKS - city.making.len()) {
        let src = city.src.clone();
        city.making.insert(c, pool.spawn(async move { make_near(&src, c) }));
    }
}

/// One chunk's near-only meshes: its full blocks, and its buildings' trim, window dressing, clutter
/// and signs.
fn make_near(src: &Near, chunk: (i32, i32)) -> Vec<Piece> {
    let mut out = vec![];
    if let Some(blocks) = src.blocks.get(&chunk) {
        // A hollow building's rooms (`.1`) apart: they cast no sun shadow (the painted shell round
        // them casts the building's).
        let mut groups: HashMap<(Mat, bool), MeshBuf> = HashMap::new();
        for (b, room) in blocks {
            let buf = groups.entry((b.mat, *room)).or_default();
            match b.shape {
                Shape::Box(s) => buf.add_box(b, s),
                Shape::Cylinder { radius, height } => buf.add_cylinder(b, radius, height),
                Shape::Sphere(r) => buf.add_sphere(b, r),
            }
        }
        let range = VisibilityRange { start_margin: 0.0..0.0, end_margin: FAR_CHUNK..FAR_CHUNK + 10.0, use_aabb: false };
        for ((mat, room), buf) in groups {
            let (mesh, centre) = buf.into_mesh();
            let shadow = !room && !matches!(mat, Mat::Foliage | Mat::Cloth);
            out.push(Piece { mesh, centre, look: Look::Block(mat), range: Some(range.clone()), shadow: Some(shadow) });
        }
    }
    let own = src.footprints.get(&chunk).map_or(&[][..], |v| v.as_slice());
    let mut bufs = if own.is_empty() { CityBufs::default() } else { city_bufs(&src.all_footprints, own, &Solid::around(src, chunk), true, &src.skins) };
    if let Some(idx) = src.signs.get(&chunk) {
        sign_bufs(&src.all_signs, idx, &mut bufs);
    }
    city_pieces(bufs, &mut out);
    out
}

fn build_chunks(
    mut commands: Commands,
    layout: Res<MapLayout>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    scale: Res<crate::quality::LodScale>,
) {
    let mut city = City::default();
    let mut src = Near { span: Solid::span(&layout), ..default() };
    let key = |p: Vec2| ((p.x / CHUNK).floor() as i32, (p.y / CHUNK).floor() as i32);
    let mut pending = PendingMips::default();
    let mut sets = HashMap::new();
    for name in ["sandstone", "plaster", "brick", "granite", "wood", "thatch"] {
        sets.insert(name, load_set(&assets, &mut pending, &format!("india/{name}")));
    }
    for name in ["stucco", "stone", "limewash", "rustic", "ashlar", "plinth"] {
        sets.insert(name, load_set(&assets, &mut pending, &format!("panga/{name}")));
    }
    sets.insert("panga_wood", load_set(&assets, &mut pending, "panga/wood"));
    // The coarse blocks now, for good; the full ones as the camera nears (`stream`).
    let mut groups: HashMap<(i32, i32, Mat, bool), MeshBuf> = HashMap::new();
    let mut mat_cache: HashMap<Mat, Handle<StandardMaterial>> = HashMap::new();
    for (i, b) in layout.blocks.iter().enumerate().filter(|(_, b)| is_static_visual(b) && !crate::diag::skip("blocks")) {
        mat_cache.entry(b.mat).or_insert_with(|| material_for(b.mat, &sets, &mut materials));
        let (cx, cz) = key(b.pos.xz());
        let room = layout.rooms_drawn.contains(&i);
        src.blocks.entry((cx, cz)).or_default().push((b.clone(), room));
        let small = match b.shape {
            Shape::Box(s) => s.max_element() <= TRIM,
            Shape::Cylinder { radius, height } => (radius * 2.0).max(height) <= TRIM,
            Shape::Sphere(r) => r * 2.0 <= TRIM,
        };
        // A hollow building's rooms are drawn near only (past FAR_CHUNK a window is a few pixels:
        // their walls far off cost 10% of the frame over Karol Bagh, and 40 MB).
        let inside = room;
        if small || inside {
            continue;
        }
        let buf = groups.entry((cx, cz, b.mat, room)).or_insert_with(|| MeshBuf { coarse: true, ..default() });
        match b.shape {
            Shape::Box(s) => buf.add_box(b, s),
            Shape::Cylinder { radius, height } => buf.add_cylinder(b, radius, height),
            Shape::Sphere(r) => buf.add_sphere(b, r),
        }
    }
    let n = groups.len();
    // Distances from the chunk's centre, where its mesh's origin is.
    let far = VisibilityRange { start_margin: FAR_CHUNK..FAR_CHUNK + 10.0, end_margin: 1.0e5..1.0e5, use_aabb: false };
    for ((_, _, mat, room), buf) in groups {
        let handle = mat_cache[&mat].clone();
        let range = crate::characters::LodRange(far.clone(), 1);
        let (mesh, centre) = buf.into_mesh();
        let mut e = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(handle),
            Transform::from_translation(centre),
            range.scaled(scale.0),
            range,
            OfMap,
        ));
        if room || matches!(mat, Mat::Foliage | Mat::Cloth) {
            e.insert(bevy::light::NotShadowCaster);
        } else {
            e.insert(crate::quality::LodShadow);
        }
    }
    // City trim (parapets, ledges, pilasters) in the clean stucco.
    let roof = mat_cache.entry(Mat::Stucco).or_insert_with(|| material_for(Mat::Stucco, &sets, &mut materials)).clone();
    let plinth = mat_cache.entry(Mat::Plinth).or_insert_with(|| material_for(Mat::Plinth, &sets, &mut materials)).clone();
    if !crate::diag::skip("city") && !(layout.footprints.is_empty() && layout.signs.is_empty()) {
        let mats = city_mats(&layout, &mut materials, &assets, &mut pending, roof, plinth);
        for (i, fp) in layout.footprints.iter().enumerate().filter(|(_, f)| f.mat != Mat::Water) {
            let c = fp.pts.iter().sum::<Vec2>() / fp.pts.len() as f32;
            src.footprints.entry(key(c)).or_default().push(i);
            let (lo, hi) = fp.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            let (c0, c1) = (key(lo), key(hi));
            for x in c0.0..=c1.0 {
                for z in c0.1..=c1.1 {
                    src.touching.entry((x, z)).or_default().push(i);
                }
            }
        }
        for (i, sg) in layout.signs.iter().enumerate() {
            src.signs.entry(key(sg.pos.xz())).or_default().push(i);
        }
        if shared::map::compose::ROOMS_BUILT {
            src.skins = layout.rooms.iter().map(|r| (r.footprint, (r.skin.clone(), r.solid_to.is_none()))).collect();
        }
        // What's seen from afar, for good: the plain walls, the roofs and the water.
        let all: Vec<usize> = (0..layout.footprints.len()).collect();
        let span = Solid::span(&layout);
        let solid = Solid::new(span, Vec2::splat(-span * 0.5), Vec2::splat(span * 0.5), layout.footprints.iter());
        let mut pieces = vec![];
        city_pieces(city_bufs(&layout.footprints, &all, &solid, false, &src.skins), &mut pieces);
        city.mats = Some(mats);
        spawn_pieces(&mut commands, &mut meshes, &city, pieces, scale.0);
        src.all_footprints = layout.footprints.clone();
        src.all_signs = layout.signs.clone();
        info!("city: {} buildings from footprints", layout.footprints.len());
    }
    city.blocks = mat_cache;
    let mut chunks: Vec<(i32, i32)> = src.blocks.keys().chain(src.footprints.keys()).chain(src.signs.keys()).copied().collect();
    chunks.sort_unstable();
    chunks.dedup();
    city.chunks = chunks;
    city.src = std::sync::Arc::new(src);
    info!("architecture: {} static blocks, {} coarse chunk meshes; full ones streamed", layout.blocks.iter().filter(|b| is_static_visual(b)).count(), n);
    commands.insert_resource(city);
    commands.insert_resource(pending);
}

/// Model textures (the shared `models/textures/` set the props' glTFs point at) arrive without
/// mips; queue each one as it loads.
fn queue_model_mips(mut events: MessageReader<AssetEvent<Image>>, assets: Res<AssetServer>, mut pending: ResMut<PendingMips>, mut images: ResMut<Assets<Image>>) {
    for ev in events.read() {
        // A texture inside a model file (capped in size where the model is made:
        // tools/glb_textures.py) comes with its own mips or none and nothing reads it here: kept
        // on the GPU only.
        if let AssetEvent::Added { id } = ev
            && assets.get_path(*id).is_some_and(|p| p.label().is_some() && !p.path().starts_with("models/textures"))
            && let Some(mut img) = images.get_mut(*id)
        {
            img.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
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
/// The tiling textures' sampler: repeating, trilinear, anisotropic.
fn repeating() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    })
}

pub(crate) fn mip_chain(img: &mut Image) {
    let (w, hgt) = (img.width() as usize, img.height() as usize);
    let Some(data) = img.data.clone() else { return };
    if data.len() != w * hgt * 4 {
        // A packed KTX2 (crate::tex) brings its own mips: its sampler as below, and on the GPU only.
        if img.texture_descriptor.mip_level_count > 1 {
            img.sampler = repeating();
            img.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
        return; // only RGBA8 sources are mipped here
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
    img.sampler = repeating();
    // Its mips made, nothing reads it here again: kept on the GPU only (some 0.6 GB of textures
    // held twice otherwise). A second pass finds no data and leaves it be.
    img.asset_usage = RenderAssetUsages::RENDER_WORLD;
}
