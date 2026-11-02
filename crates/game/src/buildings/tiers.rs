//! The distance tiers of a big map. A small map keeps all of itself in its chunks; a 4 km map can't
//! draw all of that from a roof or from a 500 m drop, so past its near chunks it is drawn as two
//! cheaper shells on Bevy's own `VisibilityRange`, one mesh per cell and material:
//!
//! | Tier | Shows, from its cell's centre | What                                             | Cell  |
//! |------|-------------------------------|--------------------------------------------------|-------|
//! | Near | 0 to 192 m                    | everything (crate::buildings, crate::batch::near) | 128 m |
//! | Mid  | 192 to 640 m                  | walls, roofs, parapets, blocks, cover, trees      | 128 m |
//! | Far  | past 420 m                    | what of that is big                               | 256 m |
//!
//! The distances are before the quality level's scale, like every other LOD's. Why no hole shows
//! between the tiers, at any scale (`tests::no_camera_sees_a_hole` walks it):
//!
//! - Near and mid swap exactly. Every near mesh of a mid cell has its origin at the cell's centre,
//!   as its mid meshes do, so they measure the same distance and cross-fade together. The chunks
//!   stay 64 m for culling; only their origin moves.
//! - Mid and far overlap instead. A far cell can't share a centre with the mid cells in it, so it
//!   comes in well before they go and is under them meanwhile. It is made of the same surfaces from
//!   the same code, in the same material, so where both draw, they draw the same pixels.
//!
//! Anything a player can hide behind is kept in the mid tier, at the size of its collider.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::ecs::system::EntityCommands;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, MeshAttributeCompressionFlags, MeshCompressionArgs, PrimitiveTopology};
use bevy::prelude::*;
use shared::map::{Block, Footprint, MapLayout, Mat, Shape, footprint};
use shared::sim::OfMap;

use super::{BAY, BLUE_TINT, MARBLE_TINT, MeshBuf, OCHRE_TINT, PARAPET, PLASTER_TINT, Paint, STOREY, TRIM, bare, bare_tint, is_static_visual, lin, roof_shade};
use crate::batch::batched;
use crate::batch::probe::{Tagged, reading_meshes};
use crate::characters::LodRange;
use crate::quality::LodShadow;
use crate::street::weathered;
use crate::surfaces::{Surface, UltraSurfaces};

/// Half a map wider than this, and it has tiers.
const BIG_MAP: f32 = 600.0;
/// A mid cell's side, and a far cell's: two mid cells across.
pub(crate) const MID_CELL: f32 = 128.0;
const FAR_CELL: f32 = 2.0 * MID_CELL;
/// Where the near tier gives way to the mid, over `FADE`.
const NEAR_END: f32 = 192.0;
const FADE: f32 = 16.0;
/// Where the mid tier ends, and where the far tier has started by (`tests` holds these apart).
const MID_END: f32 = 640.0;
const FAR_START: f32 = 420.0;
/// Past the far tier's start: the far plane and the fog end the map first.
const NO_END: f32 = 1.0e5;
/// The smallest block the mid tier keeps that isn't solid, and the smallest the far tier keeps.
const MID_BLOCK: f32 = 1.0;
const FAR_BLOCK: f32 = 4.0;
/// How thick a parapet's coping is.
const COPING: f32 = 0.14;

/// The street kit that is cover, with its collider's size (shared::map::kit) and its colours' mean
/// (linear, by area, from tools/blender/props/street_kit.py's model, before `weathered`). The rest is
/// too small or too thin to hide a player and stops with the near tier.
#[allow(clippy::approx_constant)] // Colours, not 1/pi.
const COVER: [(&str, [f32; 3], [f32; 3]); 8] = [
    ("auto", [1.4, 1.8, 2.6], [0.354, 0.326, 0.039]),
    ("scooter", [0.45, 1.1, 1.65], [0.453, 0.069, 0.066]),
    ("thela", [1.25, 1.0, 2.1], [0.455, 0.215, 0.064]),
    ("chai", [1.7, 1.0, 0.8], [0.318, 0.237, 0.221]),
    ("crates", [1.24, 0.94, 0.42], [0.205, 0.065, 0.087]),
    ("cooler", [0.9, 1.1, 0.64], [0.586, 0.468, 0.3]),
    ("mumty", [3.0, 2.75, 2.4], [0.835, 0.775, 0.629]),
    ("drum", [0.6, 0.95, 0.6], [0.006, 0.119, 0.579]),
];

/// A crown's mean colour, from the leaf atlas as crate::batch shades it, and the bark's (linear).
const LEAVES: [f32; 3] = [0.036, 0.061, 0.017];
const BARK: [f32; 3] = [0.063, 0.052, 0.039];

/// Whether this map is drawn in tiers.
pub(crate) fn big_map(layout: &MapLayout) -> bool {
    layout.half_extent > BIG_MAP
}

/// The mid cell a point on the map is in.
pub(crate) fn cell_at(x: f32, z: f32) -> (i32, i32) {
    ((x / MID_CELL).floor() as i32, (z / MID_CELL).floor() as i32)
}

/// The mid cell a 64 m chunk is in.
fn cell_of_chunk(chunk: (i32, i32)) -> (i32, i32) {
    (chunk.0.div_euclid(2), chunk.1.div_euclid(2))
}

/// The middle of a mid cell, at street level: where all its tiers measure from.
pub(crate) fn cell_centre(cell: (i32, i32)) -> Vec3 {
    Vec3::new((cell.0 as f32 + 0.5) * MID_CELL, 0.0, (cell.1 as f32 + 0.5) * MID_CELL)
}

/// The far cell a mid cell is in, and its middle.
fn far_cell(cell: (i32, i32)) -> (i32, i32) {
    (cell.0.div_euclid(2), cell.1.div_euclid(2))
}

fn far_centre(cell: (i32, i32)) -> Vec3 {
    Vec3::new((cell.0 as f32 + 0.5) * FAR_CELL, 0.0, (cell.1 as f32 + 0.5) * FAR_CELL)
}

/// How far from a mid cell's centre its near things must still be there, at a quality's scale.
pub(crate) fn near_reach(scale: f32) -> f32 {
    (NEAR_END + FADE) * scale
}

fn near_range() -> LodRange {
    LodRange(VisibilityRange { start_margin: 0.0..0.0, end_margin: NEAR_END..NEAR_END + FADE, use_aabb: false }, 0)
}

/// The range of a mid tier's mesh.
pub(crate) fn mid_range() -> LodRange {
    LodRange(VisibilityRange { start_margin: NEAR_END..NEAR_END + FADE, end_margin: MID_END..MID_END + FADE, use_aabb: false }, 1)
}

fn far_range() -> LodRange {
    LodRange(VisibilityRange { start_margin: FAR_START..FAR_START + FADE, end_margin: NO_END..NO_END, use_aabb: false }, 2)
}

/// How a tier's meshes are kept: only on the GPU once uploaded, unless a report reads them back.
pub(crate) fn usage() -> RenderAssetUsages {
    if reading_meshes() { RenderAssetUsages::default() } else { RenderAssetUsages::RENDER_WORLD }
}

/// Packs a static mesh into under half its bytes, in main memory and on the GPU: normals and
/// tangents to two numbers, UVs to 16 bits, colours to a byte a channel, indices to 16 bits where
/// they fit. Bevy's shaders unpack it. Positions stay as they are: a packed position needs its
/// bounds, and Bevy 0.20 loses those for a mesh kept only on the GPU (`Mesh::take_gpu_data`
/// recomputes them from the positions, and can't from packed ones), which leaves it undrawn.
pub(crate) fn pack(mesh: &mut Mesh) {
    let args = MeshCompressionArgs {
        compress_attributes: MeshAttributeCompressionFlags::COMPRESS_NORMAL | MeshAttributeCompressionFlags::COMPRESS_TANGENT | MeshAttributeCompressionFlags::COMPRESS_UV0,
        ..MeshCompressionArgs::regular()
    };
    if let Err(problems) = mesh.compress_mesh(&args) {
        warn!("a mesh stays unpacked: {problems:?}");
    }
}

/// A block's longest side.
pub(super) fn extent(b: &Block) -> f32 {
    match b.shape {
        Shape::Box(s) => s.max_element(),
        Shape::Cylinder { radius, height } => (radius * 2.0).max(height),
        Shape::Sphere(r) => r * 2.0,
    }
}

/// A chunk's mesh in the near tier, in the mid cell it belongs to.
pub(super) fn spawn_near<'a>(commands: &'a mut Commands, meshes: &mut Assets<Mesh>, buf: MeshBuf, material: Handle<StandardMaterial>, chunk: (i32, i32), scale: f32) -> EntityCommands<'a> {
    let origin = cell_centre(cell_of_chunk(chunk));
    let mut mesh = buf.into_mesh_at(origin);
    pack(&mut mesh);
    let base = near_range();
    commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::from_translation(origin), base.scaled(scale), base, LodShadow, Tagged("near"), OfMap))
}

/// What a material looks like from a distance, as a vertex colour: its texture's mean (measured
/// from the files) times its tint in `material_for`.
#[allow(clippy::approx_constant)] // Colours, not 1/pi.
fn flat_colour(mat: Mat) -> [f32; 4] {
    const PLASTER: [f32; 3] = [0.753, 0.727, 0.686];
    const SANDSTONE: [f32; 3] = [0.439, 0.244, 0.111];
    let times = |tex: [f32; 3], tint: [f32; 3]| {
        let t = lin(tint);
        [tex[0] * t[0], tex[1] * t[1], tex[2] * t[2], 1.0]
    };
    let plastered = |tint: Color| {
        let t = tint.to_linear();
        [PLASTER[0] * t.red, PLASTER[1] * t.green, PLASTER[2] * t.blue, 1.0]
    };
    let raw = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
    match mat {
        Mat::Plaster => plastered(PLASTER_TINT),
        Mat::PlasterOchre => plastered(OCHRE_TINT),
        Mat::PlasterBlue => plastered(BLUE_TINT),
        Mat::Marble => plastered(MARBLE_TINT),
        Mat::Sandstone => raw(SANDSTONE),
        Mat::RedSandstone => times(SANDSTONE, [0.86, 0.50, 0.40]),
        Mat::Dirt => times(SANDSTONE, [0.75, 0.62, 0.48]),
        Mat::Granite => raw([0.318, 0.26, 0.213]),
        Mat::Brick => raw([0.351, 0.108, 0.062]),
        Mat::Wood => raw([0.133, 0.058, 0.02]),
        Mat::Bark => times([0.133, 0.058, 0.02], [0.7, 0.65, 0.6]),
        Mat::Thatch => raw([0.348, 0.238, 0.079]),
        Mat::Concrete => raw([0.398, 0.385, 0.356]),
        Mat::Asphalt => raw([0.043, 0.049, 0.052]),
        Mat::Glass => raw([0.026, 0.059, 0.077]),
        Mat::Steel => raw([0.249, 0.279, 0.298]),
        Mat::Pavers => raw([0.312, 0.298, 0.266]),
        Mat::ShopSign => raw([0.084, 0.111, 0.091]),
        Mat::MetroSign => raw([0.053, 0.104, 0.16]),
        Mat::ContainerRed => lin([0.55, 0.16, 0.10]),
        Mat::Rubber => lin([0.035, 0.04, 0.045]),
        Mat::PaintWhite => lin([0.74, 0.76, 0.74]),
        Mat::TransitBlue => lin([0.14, 0.38, 0.52]),
        Mat::TransitYellow => lin([0.76, 0.60, 0.20]),
        Mat::Gold => lin([1.0, 0.76, 0.34]),
        Mat::Cloth => lin([0.85, 0.22, 0.32]),
        Mat::Foliage => lin([0.26, 0.42, 0.18]),
        Mat::Grass => lin([0.42, 0.50, 0.25]),
        Mat::Water => lin([0.10, 0.26, 0.28]),
    }
}

/// A tree's or bush's crown: the height of its middle, half its height and its radius.
fn crown(model: &str) -> (f32, f32, f32) {
    if model.contains("bush") {
        (0.7, 0.7, 1.0)
    } else if model.contains("banyan") {
        (7.0, 5.0, 12.0)
    } else if model.contains("palm") {
        (7.0, 2.0, 3.0)
    } else if model.contains("peepal") {
        (5.6, 4.0, 6.5)
    } else {
        (5.0, 3.4, 5.0)
    }
}

/// One cell's meshes: walls in the window texture, everything else in its vertices' colour.
struct Shell {
    walls: MeshBuf,
    plain: MeshBuf,
}

impl Default for Shell {
    fn default() -> Self {
        // Round things come out in fewer segments.
        Shell { walls: MeshBuf::default(), plain: MeshBuf { coarse: true, ..default() } }
    }
}

impl MeshBuf {
    /// A quad of one normal, whichever way round its corners were given.
    fn quad_facing(&mut self, p: [Vec3; 4], n: Vec3, uv: [Vec2; 4], col: [[f32; 4]; 4]) {
        if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) >= 0.0 {
            self.quad_col(p, n, uv, col);
        } else {
            self.quad_col([p[1], p[0], p[3], p[2]], n, [uv[1], uv[0], uv[3], uv[2]], [col[1], col[0], col[3], col[2]]);
        }
    }

    /// A tier's mesh, its vertices relative to `origin`: no tangents, as no map needs them this far
    /// off, and UVs only if it is textured.
    fn into_tier_mesh(self, origin: Vec3, textured: bool) -> Mesh {
        let pos: Vec<[f32; 3]> = self.pos.iter().map(|p| (Vec3::from(*p) - origin).to_array()).collect();
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, usage())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx));
        if textured {
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        }
        pack(&mut m);
        m
    }

    /// A mesh whose material has no maps, with its vertices relative to the middle of its bounds,
    /// and that middle.
    pub(super) fn into_plain_mesh(self) -> (Mesh, Vec3) {
        let (lo, hi) = self.pos.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
        let c = (lo + hi) / 2.0;
        (self.into_tier_mesh(c, false), c)
    }

    /// A block in its material's colour.
    fn add_flat(&mut self, b: &Block) {
        match b.shape {
            Shape::Box(s) => self.add_box(b, s),
            Shape::Cylinder { radius, height } => self.add_cylinder(b, radius, height),
            Shape::Sphere(r) => self.add_sphere(b, r),
        }
        self.col.resize(self.pos.len(), flat_colour(b.mat));
    }

    /// A far tree: a six-sided double cone that reads as round.
    fn add_crown(&mut self, c: Vec3, r: f32, h: f32, col: [f32; 4]) {
        let base = self.pos.len() as u32;
        for k in 0..6 {
            let a = std::f32::consts::TAU * k as f32 / 6.0;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            self.pos.push((c + d * r).to_array());
            self.nrm.push(d.to_array());
        }
        for y in [h, -h] {
            self.pos.push((c + Vec3::Y * y).to_array());
            self.nrm.push([0.0, y.signum(), 0.0]);
        }
        self.uv.resize(self.pos.len(), [0.0, 0.0]);
        self.col.resize(self.pos.len(), col);
        let (top, bottom) = (base + 6, base + 7);
        for k in 0..6 {
            let (this, next) = (base + k, base + (k + 1) % 6);
            self.idx.extend_from_slice(&[top, next, this, bottom, this, next]);
        }
    }
}

/// A building from outside, the same in the mid and far tiers: two bands of wall in the near
/// walls' colours and windows, a roof, and a parapet with its coping.
fn add_shell(s: &mut Shell, fp: &Footprint, paint: &Paint) {
    // Bare brick and stone have their colour from their material up close: here, from the vertices.
    let tint = if bare(fp.mat) { bare_tint(fp.mat).to_linear() } else { LinearRgba::WHITE };
    let wall_at = |shade: f32| {
        let c = paint.wall_at(shade);
        [c[0] * tint.red, c[1] * tint.green, c[2] * tint.blue, 1.0]
    };
    let (ph, _) = PARAPET;
    let split = (fp.base + STOREY).min(fp.top);
    let top = fp.top + ph + COPING;
    let n = fp.pts.len();
    let ccw = fp.pts.iter().zip(fp.pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() > 0.0;
    let mut along = 0.0;
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
        let (a3, b3) = (Vec3::new(a.x, 0.0, a.y), Vec3::new(b.x, 0.0, b.y));
        for (lo, hi, (s0, s1)) in [(fp.base, split, (0.66, 0.84)), (split, fp.top, (0.86, 1.0))] {
            if hi - lo < 0.05 {
                continue;
            }
            let rows = 2.0 * STOREY;
            let (v0, v1) = (-(lo - fp.base) / rows, -(hi - fp.base) / rows);
            let uv = [Vec2::new(u0, v0), Vec2::new(u1, v0), Vec2::new(u1, v1), Vec2::new(u0, v1)];
            let (c0, c1) = (wall_at(s0), wall_at(s1));
            s.walls.quad_facing([a3.with_y(lo), b3.with_y(lo), b3.with_y(hi), a3.with_y(hi)], out, uv, [c0, c0, c1, c1]);
        }
        // The parapet: its outside, its coping and its inside.
        let (c, t) = (wall_at(1.0), paint.trim);
        let (a_in, b_in) = (a3 - out * 0.4, b3 - out * 0.4);
        let no_uv = [Vec2::ZERO; 4];
        s.plain.quad_facing([a3.with_y(fp.top), b3.with_y(fp.top), b3.with_y(top), a3.with_y(top)], out, no_uv, [c; 4]);
        s.plain.quad_facing([a3.with_y(top), b3.with_y(top), b_in.with_y(top), a_in.with_y(top)], Vec3::Y, no_uv, [t; 4]);
        s.plain.quad_facing([a_in.with_y(fp.top), b_in.with_y(fp.top), b_in.with_y(top), a_in.with_y(top)], -out, no_uv, [c; 4]);
        along += len;
    }
    let plaster = flat_colour(Mat::Plaster);
    let shade = roof_shade(paint.r(6));
    let roof = [plaster[0] * shade[0], plaster[1] * shade[1], plaster[2] * shade[2], 1.0];
    for [i, j, k] in footprint::triangulate(&fp.pts) {
        let p = [fp.pts[i as usize], fp.pts[j as usize], fp.pts[k as usize]].map(|q| Vec3::new(q.x, fp.top, q.y));
        let p = if (p[1] - p[0]).cross(p[2] - p[0]).y >= 0.0 { p } else { [p[0], p[2], p[1]] };
        let base = s.plain.pos.len() as u32;
        for q in p {
            s.plain.pos.push(q.to_array());
            s.plain.nrm.push([0.0, 1.0, 0.0]);
            s.plain.uv.push([0.0, 0.0]);
            s.plain.col.push(roof);
        }
        s.plain.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }
}

/// Builds a big map's mid and far tiers. The trees' leaves come later, once their models have
/// loaded (crate::batch).
#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    commands: &mut Commands,
    layout: &MapLayout,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    facade: Option<Handle<Image>>,
    ultra: &mut UltraSurfaces,
    scale: f32,
) {
    let mut mid: HashMap<(i32, i32), Shell> = HashMap::new();
    let mut far: HashMap<(i32, i32), Shell> = HashMap::new();
    for fp in layout.footprints.iter().filter(|f| f.mat != Mat::Water) {
        let c = fp.pts.iter().sum::<Vec2>() / fp.pts.len() as f32;
        let paint = Paint::of(c, fp.mat);
        let cell = cell_at(c.x, c.y);
        add_shell(mid.entry(cell).or_default(), fp, &paint);
        add_shell(far.entry(far_cell(cell)).or_default(), fp, &paint);
    }
    for b in layout.blocks.iter().filter(|b| is_static_visual(b)) {
        let (size, cell) = (extent(b), cell_at(b.pos.x, b.pos.z));
        if size > TRIM && (b.collide || size >= MID_BLOCK) {
            mid.entry(cell).or_default().plain.add_flat(b);
        }
        if size >= FAR_BLOCK {
            far.entry(far_cell(cell)).or_default().plain.add_flat(b);
        }
    }
    let cover: HashMap<&str, ([f32; 3], [f32; 3])> = COVER.iter().map(|(name, size, col)| (*name, (*size, *col))).collect();
    for piece in &layout.kit {
        let Some((size, col)) = cover.get(piece.name) else { continue };
        let size = Vec3::from(*size);
        let c = piece.pos + Vec3::Y * size.y * 0.5;
        let [r, g, b] = weathered(*col);
        mid.entry(cell_at(c.x, c.z)).or_default().plain.open_box(c, Quat::from_rotation_y(piece.yaw), size, [r, g, b, 1.0]);
    }
    for p in layout.props.iter().filter(|p| batched(p.model)) {
        let (y, h, r) = crown(p.model);
        let cell = cell_at(p.pos.x, p.pos.z);
        // The leaves' cards above the trunk are crate::batch's.
        let up = (y * p.scale).max(0.2);
        let trunk = Vec3::new(0.7, up, 0.7) * Vec3::new(p.scale, 1.0, p.scale);
        mid.entry(cell).or_default().plain.open_box(p.pos + Vec3::Y * up * 0.5, Quat::from_rotation_y(p.yaw), trunk, [BARK[0], BARK[1], BARK[2], 1.0]);
        far.entry(far_cell(cell)).or_default().plain.add_crown(p.pos + Vec3::Y * y * p.scale, r * p.scale, h * p.scale, [LEAVES[0], LEAVES[1], LEAVES[2], 1.0]);
    }
    let walls = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: facade, perceptual_roughness: 0.85, ..default() });
    // On Ultra the near walls' colours and windows, without their relief: these meshes have no
    // tangents, which is what its normal and depth maps need.
    ultra.add(walls.clone(), Surface::Facade);
    let plain = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.85, ..default() });
    let (mut mid_tris, mut far_tris) = (0, 0);
    let mut spawn = |commands: &mut Commands, shell: Shell, origin: Vec3, tris: &mut usize, tier: LodRange, tag: &'static str| {
        for (buf, material, textured) in [(shell.walls, &walls, true), (shell.plain, &plain, false)] {
            if buf.idx.is_empty() {
                continue;
            }
            *tris += buf.idx.len() / 3;
            let mesh = meshes.add(buf.into_tier_mesh(origin, textured));
            let mut e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material.clone()), Transform::from_translation(origin), tier.scaled(scale), tier.clone(), Tagged(tag), OfMap));
            if tier.1 > 1 {
                // Past any shadow's reach.
                e.insert(NotShadowCaster);
            } else {
                e.insert(LodShadow);
            }
        }
    };
    let (mid_cells, far_cells) = (mid.len(), far.len());
    for (cell, shell) in mid {
        spawn(commands, shell, cell_centre(cell), &mut mid_tris, mid_range(), "mid");
    }
    for (cell, shell) in far {
        spawn(commands, shell, far_centre(cell), &mut far_tris, far_range(), "far");
    }
    info!("tiers: mid {mid_tris} triangles in {mid_cells} cells, far {far_tris} in {far_cells}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The quality levels' LOD scales (`Quality::lod_scale`), and the farthest a mid cell's centre
    /// is from its far cell's.
    const SCALES: [f32; 3] = [0.55, 0.75, 1.0];
    const OFFSET: f32 = MID_CELL * 0.5 * std::f32::consts::SQRT_2;

    #[test]
    fn near_and_mid_meet_exactly() {
        let (near, mid) = (near_range().0, mid_range().0);
        assert_eq!(near.end_margin, mid.start_margin);
    }

    #[test]
    fn cells_nest() {
        for x in [-1999.0f32, -129.0, -128.0, -65.0, -1.0, 0.0, 63.9, 64.0, 127.9, 128.0, 1999.0] {
            let chunk = ((x / 64.0).floor() as i32, (x / 64.0).floor() as i32);
            assert_eq!(cell_of_chunk(chunk), cell_at(x, x), "at {x}");
            let d = (cell_centre(cell_at(x, x)) - Vec3::new(x, 0.0, x)).abs().max_element();
            assert!(d <= MID_CELL * 0.5, "at {x}");
            let d = (far_centre(far_cell(cell_at(x, x))) - Vec3::new(x, 0.0, x)).abs().max_element();
            assert!(d <= FAR_CELL * 0.5, "at {x}");
        }
    }

    /// Walks a camera round the corner of four far cells at every quality level. At each spot, for
    /// each mid cell: something of the cell must be solidly there unless near and mid are in their
    /// shared cross-fade, and no far shell may show while the near tier does, which it would
    /// reach through (the near tier has rooms, and the far shell is a solid block).
    #[test]
    fn no_camera_sees_a_hole() {
        let cells: Vec<(i32, i32)> = (-6..6).flat_map(|i| (-6..6).map(move |j| (i, j))).collect();
        for k in SCALES {
            let (near, mid, far) = (near_range().scaled(k), mid_range().scaled(k), far_range().scaled(k));
            for x in (-1000..=1000).step_by(16) {
                for z in (-1000..=1000).step_by(16) {
                    for y in [2.0, 40.0, 500.0] {
                        let cam = Vec3::new(x as f32, y, z as f32);
                        for &cell in &cells {
                            let d = cam.distance(cell_centre(cell));
                            let f = cam.distance(far_centre(far_cell(cell)));
                            let solid = d < near.end_margin.start || (d >= mid.start_margin.end && d < mid.end_margin.start) || f >= far.start_margin.end;
                            let fading = d >= near.end_margin.start && d < near.end_margin.end;
                            assert!(solid || fading, "hole at {cam} in {cell:?}, scale {k}");
                            assert!(!(near.is_visible_at_all(d) && far.is_visible_at_all(f)), "far shell in the near tier at {cam} in {cell:?}, scale {k}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_far_tier_comes_in_before_the_mid_goes() {
        for k in SCALES {
            assert!(MID_END * k - OFFSET >= (FAR_START + FADE) * k, "scale {k}");
            assert!(FAR_START * k - OFFSET >= (NEAR_END + FADE) * k, "scale {k}");
        }
    }

    fn square(ccw: bool) -> Footprint {
        let mut pts = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 8.0), Vec2::new(0.0, 8.0)];
        if !ccw {
            pts.reverse();
        }
        Footprint { pts, base: 0.0, top: 10.0, mat: Mat::Plaster }
    }

    /// Every wall of a shell has to face outward, whichever way its outline runs, or it vanishes
    /// from outside when it swaps with the near tier.
    #[test]
    fn a_shell_faces_outward() {
        for ccw in [true, false] {
            let mut s = Shell::default();
            let fp = square(ccw);
            add_shell(&mut s, &fp, &Paint::of(Vec2::new(5.0, 4.0), fp.mat));
            let middle = Vec3::new(5.0, 5.0, 4.0);
            for t in s.walls.idx.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|i| Vec3::from(s.walls.pos[i as usize]));
                let n = (b - a).cross(c - a);
                assert!(n.dot((a + b + c) / 3.0 - middle) > 0.0, "a wall faces in (ccw {ccw})");
                let stored = Vec3::from(s.walls.nrm[t[0] as usize]);
                assert!(n.normalize().dot(stored) > 0.99, "a wall's normal isn't its face's (ccw {ccw})");
            }
            assert_eq!(s.walls.col.len(), s.walls.pos.len());
            assert_eq!(s.plain.col.len(), s.plain.pos.len());
            assert_eq!(s.plain.uv.len(), s.plain.pos.len());
        }
    }

    #[test]
    fn a_crown_faces_outward() {
        let mut buf = MeshBuf::default();
        let c = Vec3::new(3.0, 5.0, 1.0);
        buf.add_crown(c, 4.0, 3.0, [1.0; 4]);
        for t in buf.idx.as_chunks::<3>().0 {
            let [a, b, d] = t.map(|i| Vec3::from(buf.pos[i as usize]));
            assert!((b - a).cross(d - a).dot((a + b + d) / 3.0 - c) > 0.0);
        }
    }
}
