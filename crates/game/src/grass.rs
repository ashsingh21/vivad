//! Ground paint and grass: the terrain's colours by slope, height and context (shared with the
//! terrain mesh in `visuals`), and blades of grass streamed in chunks around the camera.
//!
//! Blades sway in a vertex shader (`assets/shaders/grass*.wesl`) and thin out with distance per
//! blade (each has a rank in `uv.x`; higher ranks sink into the ground nearer the camera), so the
//! far edge dissolves into the painted ground instead of popping by chunk.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use shared::map::terrain::{Terrain, fbm};
use shared::map::{MapLayout, Shape};

use crate::camera::MainCamera;

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .add_systems(Startup, setup)
            .add_systems(Update, blow);
        // VIVAD_NO_GRASS=1 for render cost comparisons.
        if std::env::var("VIVAD_NO_GRASS").is_err() {
            app.add_systems(Update, stream_chunks);
        }
    }
}

/// Chunk side, metres.
const CHUNK: f32 = 8.0;
/// Chunks are built within this distance of the camera and dropped past `DROP`. The shader has
/// sunk every blade by `FADE_FAR` (keep in step with `grass.wesl`).
const BUILD: f32 = 46.0;
const DROP: f32 = 60.0;
/// The shader's distance thinning (grass.wesl): all blades out to NEAR, none past FAR.
const NEAR: f32 = 13.0;
const FAR: f32 = 40.0;
/// A thinned chunk is rebuilt denser once the camera is this much closer than when it was built.
const REBUILD: f32 = 3.0;
/// Clumps per square metre at full cover, and blades per clump; plus short single blades
/// scattered between the clumps (per square metre) so the ground between them isn't bare.
const CLUMPS: f32 = 10.0;
const BLADES: (u32, u32) = (10, 20);
const CARPET: f32 = 40.0;
/// Share of the full blade count grown: VIVAD_GRASS=<0..1>. Half looks the same (the painted
/// ground carries the colour) and saves a third of the frame where grass fills the view.
fn density() -> f32 {
    static D: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var("VIVAD_GRASS").ok().and_then(|s| s.parse().ok()).unwrap_or(0.5f32).clamp(0.05, 1.0))
}

/// Share of blades that are dry straw.
const DRY: f32 = 0.06;
const RIVER: (f32, f32) = shared::map::india::RIVER_X;

pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassWind>;

/// x: time, y: tip sway in metres, z: last frame's time (for motion vectors).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct GrassWind {
    #[uniform(100)]
    pub wind: Vec4,
}

impl MaterialExtension for GrassWind {
    fn vertex_shader() -> ShaderRef {
        "shaders/grass.wesl".into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/grass_prepass.wesl".into()
    }
}

// ------------------------------------------------------------------ ground paint

fn lin(c: [f32; 3]) -> LinearRgba {
    Color::srgb(c[0], c[1], c[2]).to_linear()
}

/// Ground colour and grass cover (0 bare .. 1 full) at a point, given its height, normal and
/// distance to the nearest road.
pub fn paint(x: f32, z: f32, h: f32, n: Vec3, road_d: f32) -> (LinearRgba, f32) {
    let mix = |a: LinearRgba, b: LinearRgba, t: f32| a.mix(&b, t.clamp(0.0, 1.0));
    let (grass, grass2) = (lin([0.30, 0.44, 0.16]), lin([0.23, 0.38, 0.13]));
    let (dry, rock, dirt, sand) = (lin([0.62, 0.56, 0.32]), lin([0.50, 0.45, 0.40]), lin([0.60, 0.47, 0.32]), lin([0.74, 0.66, 0.50]));
    let patch = fbm(x / 22.0, z / 22.0, 3, 91);
    let mut c = mix(grass, grass2, patch * 1.6 - 0.3);
    // Sun-dried grass on the high hills and in a few patches.
    let w_dry = ((h - 13.0) / 14.0 + (fbm(x / 60.0, z / 60.0, 2, 17) - 0.6) * 1.6).clamp(0.0, 1.0);
    c = mix(c, dry, w_dry);
    // Rock on steep slopes.
    let w_rock = ((0.86 - n.y) * 6.0).clamp(0.0, 1.0);
    c = mix(c, rock, w_rock);
    // Sandy river banks.
    let rd = if x < RIVER.0 { RIVER.0 - x } else if x > RIVER.1 { x - RIVER.1 } else { 0.0 };
    let w_sand = (1.0 - rd / 9.0).clamp(0.0, 1.0);
    c = mix(c, sand, w_sand);
    // Roads.
    let w_road = ((5.0 - road_d) / 2.5).clamp(0.0, 1.0);
    c = mix(c, dirt, w_road);
    let v = 0.92 + 0.16 * fbm(x / 3.0, z / 3.0, 2, 5);
    let cover = (1.0 - w_dry * 1.4).max(0.0) * (1.0 - w_rock * 1.5).max(0.0) * (1.0 - w_sand * 1.3).max(0.0) * (1.0 - w_road * 1.2).max(0.0);
    (LinearRgba::new(c.red * v, c.green * v, c.blue * v, 1.0), cover)
}

/// A per-sample terrain field (heights, road distances) at a point, interpolated on the same
/// triangles the terrain mesh uses (split along the (i, j)-(i+1, j+1) diagonal), so roots sit on
/// the rendered surface.
fn on_mesh(t: &Terrain, v: &[f32], x: f32, z: f32) -> f32 {
    let s = t.step();
    let fx = ((x + t.size * 0.5) / s).clamp(0.0, (t.n - 1) as f32 - 1e-3);
    let fz = ((z + t.size * 0.5) / s).clamp(0.0, (t.n - 1) as f32 - 1e-3);
    let (i, j) = (fx as usize, fz as usize);
    let (tx, tz) = (fx - i as f32, fz - j as f32);
    let at = |i: usize, j: usize| v[i * t.n + j];
    let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
    if tx > tz { a + tx * (b - a) + tz * (c - b) } else { a + tz * (d - a) + tx * (c - d) }
}

// ------------------------------------------------------------------ blockers

/// Footprints where grass must not grow (floors, walls, plinths, water), bucketed on a grid.
struct Blockers {
    boxes: Vec<(Vec3, Quat, Vec3)>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

const CELL: f32 = 8.0;
/// Clearance kept round a footprint, metres.
const MARGIN: f32 = 0.25;

impl Blockers {
    fn new(layout: &MapLayout) -> Self {
        let mut boxes = Vec::new();
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for b in layout.blocks.iter().chain(&layout.doors) {
            let half = match b.shape {
                Shape::Box(s) => s * 0.5,
                Shape::Cylinder { radius, height } => Vec3::new(radius, height * 0.5, radius),
                Shape::Sphere(r) => Vec3::splat(r),
            };
            // XZ bounds of the rotated box.
            let ext = [Vec3::X, Vec3::Y, Vec3::Z].iter().fold(Vec3::ZERO, |acc, &ax| acc + (b.rot * (ax * half)).abs());
            let (lo, hi) = ((b.pos.xz() - ext.xz() - MARGIN) / CELL, (b.pos.xz() + ext.xz() + MARGIN) / CELL);
            let k = boxes.len() as u32;
            boxes.push((b.pos, b.rot.inverse(), half));
            for cx in lo.x.floor() as i32..=hi.x.floor() as i32 {
                for cz in lo.y.floor() as i32..=hi.y.floor() as i32 {
                    cells.entry((cx, cz)).or_default().push(k);
                }
            }
        }
        Self { boxes, cells }
    }

    /// Is ground point `p` under or inside something built?
    fn blocked(&self, p: Vec3) -> bool {
        let Some(list) = self.cells.get(&((p.x / CELL).floor() as i32, (p.z / CELL).floor() as i32)) else { return false };
        list.iter().any(|&k| {
            let (pos, inv, half) = self.boxes[k as usize];
            let l = inv * (p - pos);
            // Anything reaching down to within a metre of the ground (floors, plinths, walls),
            // not roofs and balconies overhead.
            l.x.abs() < half.x + MARGIN && l.z.abs() < half.z + MARGIN && l.y > -half.y - 0.3 && l.y < half.y + 1.0
        })
    }
}

// ------------------------------------------------------------------ chunks

#[derive(Resource)]
struct Grass {
    material: Handle<GrassMaterial>,
    blockers: Blockers,
    /// Built chunks: entity and the share of blades kept.
    live: HashMap<(i32, i32), (Entity, f32)>,
}

fn setup(mut commands: Commands, layout: Res<MapLayout>, mut materials: ResMut<Assets<GrassMaterial>>) {
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.8,
            reflectance: 0.25,
            // Blades are single quads seen from both sides, lit with the same (mostly upward)
            // normal either way.
            cull_mode: None,
            ..default()
        },
        extension: GrassWind { wind: Vec4::new(0.0, 0.12, 0.0, 0.0) },
    });
    commands.insert_resource(Grass { material, blockers: Blockers::new(&layout), live: HashMap::new() });
}

/// Small deterministic generator, seeded per chunk so a chunk rebuilds identically.
struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

fn stream_chunks(
    mut commands: Commands,
    mut grass: ResMut<Grass>,
    layout: Res<MapLayout>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(cam) = cam.single() else { return };
    let eye = cam.translation();
    let reach = |k: (i32, i32)| Vec2::new((k.0 as f32 + 0.5) * CHUNK, (k.1 as f32 + 0.5) * CHUNK).distance(eye.xz());
    // Up close only (high above the map, e.g. on the vimana, nothing is near enough to need it).
    let ground = layout.terrain.height(eye.x, eye.z);
    let near = eye.y - ground < BUILD;
    let gone: Vec<_> = grass.live.keys().copied().filter(|&k| !near || reach(k) > DROP).collect();
    for k in gone {
        if let Some((e, _)) = grass.live.remove(&k) {
            commands.entity(e).despawn();
        }
    }
    if !near {
        return;
    }
    // The share of blades the shader still shows at the chunk's nearest point.
    let keep_at = |k: (i32, i32)| {
        let d = (reach(k) - CHUNK * std::f32::consts::FRAC_1_SQRT_2).max(0.0);
        ((FAR - d) / (FAR - NEAR) + 0.03).clamp(0.0, 1.0)
    };
    let lim = layout.half_extent + 20.0;
    let r = (BUILD / CHUNK).ceil() as i32 + 1;
    let (ci, cj) = ((eye.x / CHUNK).floor() as i32, (eye.z / CHUNK).floor() as i32);
    let mut want: Vec<(i32, i32)> = (-r..=r)
        .flat_map(|i| (-r..=r).map(move |j| (ci + i, cj + j)))
        .filter(|&k| reach(k) < BUILD)
        .filter(|&k| match grass.live.get(&k) {
            None => true,
            // Come closer since it was built: it needs more blades.
            Some(&(_, kept)) => kept < 1.0 && keep_at(k) > kept + REBUILD / (FAR - NEAR),
        })
        .filter(|&k| ((k.0 as f32 + 0.5) * CHUNK).abs() < lim && ((k.1 as f32 + 0.5) * CHUNK).abs() < lim)
        .collect();
    want.sort_by(|a, b| reach(*a).total_cmp(&reach(*b)));
    // A few per frame, nearest first.
    for k in want.into_iter().take(6) {
        let keep = keep_at(k);
        let e = match build_chunk(k, &layout.terrain, &grass.blockers, keep) {
            Some((mesh, origin)) => commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(grass.material.clone()),
                    Transform::from_translation(origin),
                    bevy::light::NotShadowCaster,
                ))
                .id(),
            // Bare ground: remember it so it isn't rebuilt every frame.
            None => commands.spawn(Transform::default()).id(),
        };
        if let Some((old, _)) = grass.live.insert(k, (e, keep)) {
            commands.entity(old).despawn();
        }
    }
}

/// Every blade in chunk `k`, or None if nothing grows there. Vertices are relative to the
/// returned origin (chunk centre on the ground). Per vertex: uv = (rank, height fraction), color
/// alpha = blade height (the shader sinks blades by it).
/// Blade geometry for one chunk. Per vertex: uv = (rank, height fraction), colour alpha = blade
/// height (see grass.wesl).
#[derive(Default)]
struct Blades {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Blades {
    /// A tapering blade of four segments curving over along `lean_dir`.
    #[allow(clippy::too_many_arguments)]
    fn blade(&mut self, root: Vec3, height: f32, width: f32, side: Vec3, lean_dir: Vec3, lean: f32, normal: Vec3, (root_c, tip_c): (LinearRgba, LinearRgba), rank: f32) {
        let base = self.pos.len() as u32;
        let at = |f: f32| root + Vec3::Y * (height * f * (1.0 - 0.25 * f * lean / height.max(1e-3))) + lean_dir * (lean * f * f);
        for (f, w) in [(0.0, 1.0), (0.3, 0.85), (0.6, 0.6), (0.85, 0.3)] {
            let c = root_c.mix(&tip_c, f);
            for s in [-0.5, 0.5] {
                self.pos.push((at(f) + side * (width * w * s)).to_array());
                self.nrm.push(normal.to_array());
                self.uv.push([rank, f]);
                self.col.push([c.red, c.green, c.blue, height]);
            }
        }
        self.pos.push(at(1.0).to_array());
        self.nrm.push(normal.to_array());
        self.uv.push([rank, 1.0]);
        self.col.push([tip_c.red, tip_c.green, tip_c.blue, height]);
        self.idx.extend([0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4, 4, 5, 7, 4, 7, 6, 6, 7, 8].map(|i| base + i));
    }
}

/// Builds chunk `k` with only the blades of rank below `keep` (the rest would be sunk by the
/// shader at the chunk's distance).
fn build_chunk(k: (i32, i32), t: &Terrain, blockers: &Blockers, keep: f32) -> Option<(Mesh, Vec3)> {
    let x0 = Vec2::new(k.0 as f32, k.1 as f32) * CHUNK;
    let c = x0 + Vec2::splat(CHUNK * 0.5);
    let origin = Vec3::new(c.x, on_mesh(t, &t.h, c.x, c.y), c.y);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((k.0 as u64) << 32 | (k.1 as u32 as u64)).wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
    let tip_tint = lin([0.55, 0.68, 0.28]);
    let (cool, warm, straw) = (lin([0.17, 0.33, 0.15]), lin([0.47, 0.54, 0.2]), lin([0.72, 0.64, 0.4]));
    let mut mesh = Blades::default();
    let k = density();
    let clumps = (CHUNK * CHUNK * CLUMPS * k) as usize;
    let carpet = (CHUNK * CHUNK * CARPET * k) as usize;
    for i in 0..clumps + carpet {
        let p = x0 + Vec2::new(rng.f(), rng.f()) * CHUNK;
        let (h, rd) = (on_mesh(t, &t.h, p.x, p.y), on_mesh(t, &t.road_d, p.x, p.y));
        let n = t.normal(p.x, p.y);
        let (ground, cover) = paint(p.x, p.y, h, n, rd);
        // Thicker in the lush patches, thin at the edges of dry ground and paths.
        let lush = fbm(p.x / 9.0, p.y / 9.0, 2, 33);
        if rng.f() > cover * (0.55 + 0.9 * lush) || blockers.blocked(Vec3::new(p.x, h, p.y)) {
            continue;
        }
        let is_clump = i < clumps;
        let (blades, spread) = if is_clump { (BLADES.0 + (rng.f() * (BLADES.1 - BLADES.0 + 1) as f32) as u32, 0.13) } else { (1, 0.0) };
        let tall = (0.6 + 0.5 * lush) * (0.7 + 0.3 * cover) * if is_clump { 1.0 } else { 0.45 };
        // Each clump its own shade: some deep and bluish, some yellower in the sun.
        let tint = if rng.f() < 0.5 { cool } else { warm };
        let clump_c = ground.mix(&tint, rng.range(0.0, 0.4));
        for _ in 0..blades {
            let q = p + Vec2::new(rng.range(-spread, spread), rng.range(-spread, spread));
            let root = Vec3::new(q.x, on_mesh(t, &t.h, q.x, q.y) - 0.02, q.y) - origin;
            let height = rng.range(0.16, 0.42) * tall;
            let width = rng.range(0.014, 0.03);
            let yaw = rng.f() * TAU;
            let side = Vec3::new(yaw.cos(), 0.0, yaw.sin());
            let face = Vec3::new(-yaw.sin(), 0.0, yaw.cos());
            let lean_dir = (face * if rng.f() < 0.5 { 1.0 } else { -1.0 } + side * rng.range(-0.5, 0.5)).normalize();
            let lean = rng.range(0.15, 0.6) * height;
            let normal = (n + face * rng.range(-0.35, 0.35)).normalize();
            // Darker at the root (the clump shades itself), sunlit and yellower at the tip; now
            // and then a dry straw blade.
            let shade = rng.range(0.85, 1.12);
            let (root_c, tip_c) = if rng.f() < DRY {
                (straw * 0.55, straw * shade)
            } else {
                (clump_c * (0.42 * shade), clump_c.mix(&tip_tint, rng.range(0.1, 0.3)) * (1.05 * shade))
            };
            // Every draw is made whatever `keep` is, so a rebuild at another density puts the
            // same blades in the same places.
            let rank = rng.f();
            if rank < keep {
                mesh.blade(root, height, width, side, lean_dir, lean, normal, (root_c, tip_c), rank);
            }
        }
    }
    let Blades { pos, nrm, uv, col, idx } = mesh;
    if idx.is_empty() {
        return None;
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx));
    Some((mesh, origin))
}

/// Advance the wind.
fn blow(time: Res<Time>, grass: Res<Grass>, mut materials: ResMut<Assets<GrassMaterial>>) {
    if let Some(mut m) = materials.get_mut(&grass.material) {
        let now = time.elapsed_secs_wrapped();
        let last = m.extension.wind.x;
        m.extension.wind.z = if now >= last { last } else { now };
        m.extension.wind.x = now;
    }
}
