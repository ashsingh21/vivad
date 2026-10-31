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
        app.add_systems(Startup, build_chunks).add_systems(Update, (queue_model_mips, mipmap_textures).chain());
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
        let _ = m.generate_tangents();
        (m, c)
    }
}

/// One repeat of a city facade: a bay `BAY` m wide and a storey `STOREY` m tall, a window in a
/// pale frame over a sill, a thin band at the floor line. Near-white, tinted per material.
const BAY: f32 = 3.2;
const STOREY: f32 = 3.4;

fn facade_image() -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let n = 128usize;
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let window = (0.28..0.72).contains(&u) && (0.18..0.72).contains(&v);
            let frame = (0.25..0.75).contains(&u) && (0.15..0.75).contains(&v);
            let sill = (0.22..0.78).contains(&u) && (0.75..0.8).contains(&v);
            let band = v > 0.95;
            let px: [u8; 3] = if window {
                // Glass: dark, a little bluer toward the top where it catches the sky.
                let k = 1.0 - (v - 0.18) / 0.54;
                [(38.0 + 22.0 * k) as u8, (46.0 + 26.0 * k) as u8, (56.0 + 34.0 * k) as u8]
            } else if frame || sill {
                [208, 204, 196]
            } else if band {
                [196, 190, 180]
            } else {
                [238, 234, 226]
            };
            data.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data,
               TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
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
    pending.0.push(facade.clone());
    let tint = |m: Mat| match m {
        Mat::PlasterOchre => Color::srgb(0.95, 0.76, 0.48),
        Mat::Sandstone => Color::srgb(0.85, 0.68, 0.50),
        Mat::RedSandstone => Color::srgb(0.78, 0.45, 0.36),
        Mat::Brick => Color::srgb(0.70, 0.40, 0.30),
        _ => Color::srgb(0.97, 0.95, 0.90),
    };
    let mut walls: HashMap<(i32, i32, Mat), MeshBuf> = HashMap::new();
    let mut roofs: HashMap<(i32, i32), MeshBuf> = HashMap::new();
    for fp in &layout.footprints {
        let c = fp.pts.iter().sum::<Vec2>() / fp.pts.len() as f32;
        let (cx, cz) = ((c.x / CHUNK).floor() as i32, (c.y / CHUNK).floor() as i32);
        let w = walls.entry((cx, cz, fp.mat)).or_default();
        let n = fp.pts.len();
        let mut along = 0.0;
        let ccw = fp.pts.iter().zip(fp.pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() > 0.0;
        for i in 0..n {
            let (a, b) = (fp.pts[i], fp.pts[(i + 1) % n]);
            let e = b - a;
            let len = e.length();
            if len < 1e-3 {
                continue;
            }
            let out2 = if ccw { Vec2::new(e.y, -e.x) } else { Vec2::new(-e.y, e.x) } / len;
            let out = Vec3::new(out2.x, 0.0, out2.y);
            let (a0, b0) = (Vec3::new(a.x, fp.base, a.y), Vec3::new(b.x, fp.base, b.y));
            let (a1, b1) = (a0.with_y(fp.top), b0.with_y(fp.top));
            // Windows run from the ground up in whole storeys; bays from each corner.
            let (u0, u1) = (along / BAY, (along + len) / BAY);
            let (v0, v1) = (-fp.base / STOREY, -fp.top / STOREY);
            let quad = [a0, b0, b1, a1];
            let uv = [Vec2::new(u0, v0), Vec2::new(u1, v0), Vec2::new(u1, v1), Vec2::new(u0, v1)];
            if (b0 - a0).cross(b1 - a0).dot(out) >= 0.0 {
                w.quad(quad, out, uv);
            } else {
                w.quad([b0, a0, a1, b1], out, [uv[1], uv[0], uv[3], uv[2]]);
            }
            along += len;
        }
        let r = roofs.entry((cx, cz)).or_default();
        for [i, j, k] in shared::map::footprint::triangulate(&fp.pts) {
            let p = [fp.pts[i as usize], fp.pts[j as usize], fp.pts[k as usize]].map(|q| Vec3::new(q.x, fp.top, q.y));
            let p = if (p[1] - p[0]).cross(p[2] - p[0]).y >= 0.0 { p } else { [p[0], p[2], p[1]] };
            let base = r.pos.len() as u32;
            for q in p {
                r.pos.push(q.to_array());
                r.nrm.push([0.0, 1.0, 0.0]);
                r.uv.push([q.x / TILE, q.z / TILE]);
            }
            r.idx.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
    let mut facade_mats: HashMap<Mat, Handle<StandardMaterial>> = HashMap::new();
    for ((_, _, mat), buf) in walls {
        let m = facade_mats
            .entry(mat)
            .or_insert_with(|| materials.add(StandardMaterial { base_color: tint(mat), base_color_texture: Some(facade.clone()), perceptual_roughness: 0.85, ..default() }))
            .clone();
        let (mesh, centre) = buf.into_mesh();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(m), Transform::from_translation(centre)));
    }
    for (_, buf) in roofs {
        let (mesh, centre) = buf.into_mesh();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(roof.clone()), Transform::from_translation(centre)));
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
        let (w, hgt) = (img.width() as usize, img.height() as usize);
        let Some(data) = img.data.clone() else { return false };
        if data.len() != w * hgt * 4 {
            return false; // only RGBA8 sources are handled
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
        false
    });
}
