//! The goods: what makes a stall a stall and a shop front a shop. Heaps on the thelas, sacks and
//! crates at the feet of the walls, cloth on rails over head height, tarps out over the pavement,
//! wires strung from pole to pole, steam off the chai.
//!
//! None of it is on the map: it has no collider, the street grid never hears of it, and bullets
//! pass through it. It is worked out from the map's kit and footprints only for the patches of
//! street near the camera, and each patch is one mesh with one material (the colours are in the
//! vertices, as in the street kit), merged on a worker. A market is a handful of draws, and with
//! nothing near the player nothing exists at all.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, VisibilityRange};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use shared::map::terrain::ground;
use shared::map::{MapLayout, Mat};
use shared::sim::OfMap;

use super::{Market, busy_at};
use crate::batch::probe::Tagged;
use crate::buildings::tiers;
use crate::camera::MainCamera;

pub struct GoodsPlugin;

impl Plugin for GoodsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Goods>()
            .init_resource::<Steam>()
            .add_systems(Startup, load)
            .add_systems(Update, (forget.run_if(shared::sim::map_reloaded), stream, steam).chain());
    }
}

/// Metres a side of a patch of street: the mesh it all goes in.
const PATCH: f32 = 64.0;
/// How far from the camera a patch's middle may be for it to exist, and where it goes.
const REACH: f32 = 128.0;
const LET_GO: f32 = 148.0;
/// Where a patch fades out, from its middle.
const FADE_FROM: f32 = 104.0;
const FADE_TO: f32 = 120.0;
/// Above the ground, the camera is too far up for goods to show.
const TOO_HIGH: f32 = 90.0;
/// The middle of the atlas's white tile (tools/blender/life/goods_atlas.py).
const WHITE_TEXEL: [f32; 2] = [0.08, 0.08];
/// A shop's width along a wall, from the buildings' own window rule.
const BAY: f32 = crate::buildings::BAY;

/// One piece's mesh, read from the file.
#[derive(Default)]
struct Piece {
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

/// A hanging wire: its ends and how far it sags.
#[derive(Clone, Copy)]
struct Wire {
    a: Vec3,
    b: Vec3,
    sag: f32,
}

#[derive(Resource, Default)]
struct Goods {
    pieces: Option<Arc<HashMap<String, Piece>>>,
    /// The pieces' file being read, and the atlas they are all mapped into.
    reading: Option<Task<Option<(HashMap<String, Piece>, Image)>>>,
    material: Option<Handle<StandardMaterial>>,
    /// The patches that exist.
    live: HashMap<(i32, i32), Entity>,
    /// A patch being merged.
    job: Option<((i32, i32), Task<Option<(Mesh, Aabb)>>)>,
}

fn load(mut goods: ResMut<Goods>, mut materials: ResMut<Assets<StandardMaterial>>) {
    goods.material = Some(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.82,
        reflectance: 0.3,
        cull_mode: None,
        double_sided: true,
        ..default()
    }));
    if cfg!(not(target_arch = "wasm32")) {
        goods.reading = Some(AsyncComputeTaskPool::get().spawn(async move { read_pieces().map_err(|e| warn!("life: goods: {e}")).ok() }));
    }
}

/// Another map: its patches went with it.
fn forget(mut goods: ResMut<Goods>) {
    goods.live.clear();
    goods.job = None;
}

fn read_pieces() -> Result<(HashMap<String, Piece>, Image), String> {
    let path = std::path::Path::new(crate::ASSET_DIR).join("models/life/goods.glb");
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let atlas = std::fs::read(std::path::Path::new(crate::ASSET_DIR).join("models/life/goods_atlas.png")).map_err(|e| e.to_string())?;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let mut atlas = Image::from_buffer(&atlas, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, tiers::usage()).map_err(|e| e.to_string())?;
    crate::buildings::mip_chain(&mut atlas);
    let file = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
    let blob = file.blob.as_deref().ok_or("no binary chunk")?;
    let mut out = HashMap::new();
    for node in file.document.nodes() {
        let (Some(mesh), Some(name)) = (node.mesh(), node.name()) else { continue };
        let mut piece = Piece::default();
        for prim in mesh.primitives() {
            let reader = prim.reader(|_| Some(blob));
            let base = piece.pos.len() as u32;
            piece.pos.extend(reader.read_positions().ok_or("no positions")?.map(Vec3::from));
            piece.nrm.extend(reader.read_normals().ok_or("no normals")?.map(Vec3::from));
            piece.uv.extend(reader.read_tex_coords(0).ok_or("no uvs")?.into_f32());
            piece.col.extend(reader.read_colors(0).ok_or("no colours")?.into_rgba_f32());
            piece.idx.extend(reader.read_indices().ok_or("no indices")?.into_u32().map(|i| i + base));
        }
        out.insert(name.to_owned(), piece);
    }
    Ok((out, atlas))
}

pub(super) fn hash(a: u32, b: u32, c: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b1) ^ b.wrapping_mul(0x85eb_ca77) ^ c.wrapping_mul(0xc2b2_ae3d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

fn pick<'a>(from: &[&'a str], roll: f32) -> &'a str {
    from[((roll * from.len() as f32) as usize).min(from.len() - 1)]
}

fn patch_at(p: Vec2) -> (i32, i32) {
    ((p.x / PATCH).floor() as i32, (p.y / PATCH).floor() as i32)
}

fn patch_centre(k: (i32, i32)) -> Vec2 {
    (Vec2::new(k.0 as f32, k.1 as f32) + 0.5) * PATCH
}

fn on_street(layout: &MapLayout, p: Vec2) -> bool {
    matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::ROAD | ground::PAVEMENT | ground::PATH | ground::PLAZA | ground::PARKING))
}

/// Everything in a patch of street, from the map.
fn dress(layout: &MapLayout, market: &Market, key: (i32, i32), out: &mut Vec<(&'static str, Vec3, f32)>, wires: &mut Vec<Wire>) {
    let inside = |p: Vec2| patch_at(p) == key;
    let ground_y = |p: Vec2| layout.terrain.height(p.x, p.y);
    let mid = patch_centre(key);

    // The stalls.
    for id in market.stalls_near(mid, PATCH * 0.5 + 1.0) {
        let s = market.stalls[id as usize];
        let at = s.pos.xz();
        if !inside(at) {
            continue;
        }
        let (a, b) = (at.x.to_bits(), at.y.to_bits());
        let busy = busy_at(market, at);
        let local = |x: f32, y: f32, z: f32| s.pos + Quat::from_rotation_y(s.yaw) * Vec3::new(x, y, z);
        match s.kind {
            super::StallKind::Thela => {
                out.push((pick(&["heap_orange_apple", "heap_banana_guava", "heap_tomato_onion", "heap_veg", "heap_cauli"], hash(a, b, 1)), s.pos, s.yaw));
                if hash(a, b, 2) < 0.4 + busy * 0.5 {
                    out.push((pick(&["canopy_blue", "canopy_orange", "canopy_white"], hash(a, b, 3)), s.pos, s.yaw));
                }
                // Stock under the cart, inside its collider.
                out.push((pick(&["crate_onion", "crate_tomato", "crate_coconut", "cartons"], hash(a, b, 4)), local(0.0, 0.0, 0.2), s.yaw + std::f32::consts::FRAC_PI_2));
            }
            super::StallKind::Crates => {
                out.push((pick(&["crate_tomato", "crate_onion", "crate_cauli", "crate_coconut"], hash(a, b, 1)), local(0.32, 0.3, 0.0), s.yaw));
            }
            super::StallKind::Patri => {
                out.push((pick(&["patri_veg", "patri_fruit", "patri_roots", "patri_clothes", "patri_shoes", "patri_steel", "greens"], hash(a, b, 1)), s.pos, s.yaw));
                if hash(a, b, 2) < 0.25 + busy * 0.35 {
                    out.push((pick(&["sheet_blue", "sheet_orange", "sheet_white"], hash(a, b, 3)), s.pos, s.yaw));
                }
            }
            _ => {}
        }
    }

    // The shop fronts.
    for i in market.buildings_near(mid, PATCH * 0.5 + 2.0) {
        let fp = &layout.footprints[i as usize];
        if fp.mat == Mat::Water || fp.top - fp.base < 4.5 {
            continue;
        }
        let theme = hash(i, 11, 0);
        let area: f32 = fp.pts.iter().zip(fp.pts.iter().cycle().skip(1)).map(|(a, b)| a.perp_dot(*b)).sum();
        let n = fp.pts.len();
        for e in 0..n {
            let (a, b) = (fp.pts[e], fp.pts[(e + 1) % n]);
            let len = a.distance(b);
            if len < 3.0 {
                continue;
            }
            let dir = (b - a) / len;
            let out2 = if area > 0.0 { Vec2::new(dir.y, -dir.x) } else { Vec2::new(-dir.y, dir.x) };
            let bays = (len / BAY).floor().max(1.0) as i32;
            for s in 0..bays {
                let at = a + dir * ((s as f32 + 0.5) * len / bays as f32);
                if !inside(at) || layout.is_holy(at) {
                    continue;
                }
                let front = at + out2 * 2.0;
                if !on_street(layout, front) || market.built(layout, at + out2 * 1.0) || market.built(layout, at + out2 * 2.5) {
                    continue;
                }
                let y = ground_y(front);
                if (y - fp.base).abs() > 0.7 {
                    continue;
                }
                let busy = busy_at(market, front);
                let yaw = (-out2.x).atan2(-out2.y);
                let seed = (e as u32) * 97 + s as u32;
                let at3 = |off: f32| Vec3::new(at.x + out2.x * off, y, at.y + out2.y * off);
                if hash(i, seed, 2) < (busy * 1.7).min(0.92) {
                    let roll = hash(i, seed, 3);
                    let (name, off) = if theme < 0.34 {
                        (pick(&["rack_cloth", "rack_cloth", "rack_short", "rack_high", "bolts"], roll), 0.03)
                    } else if theme < 0.66 {
                        (pick(&["sacks_spice", "sacks_grain", "sacks_stack", "stand_veg", "stand_fruit", "greens", "crate_cauli"], roll), 0.34)
                    } else if theme < 0.84 {
                        (pick(&["pots", "buckets", "cartons", "rack_high"], roll), 0.34)
                    } else {
                        (pick(&["cartons", "buckets", "sacks_stack", "rack_short", "stand_fruit"], roll), 0.3)
                    };
                    let off = if name.starts_with("rack") { 0.03 } else { off };
                    out.push((name, at3(off), yaw));
                }
                if s % 2 == 0 && hash(i, seed, 4) < busy * 0.6 {
                    out.push((pick(&["tarp_blue", "tarp_orange", "tarp_grey", "tarp_green"], hash(i, seed, 5)), at3(0.02), yaw));
                }
            }
        }
    }

    // The wires, from pole to pole, with a bundle dropping to the nearest wall.
    for (k, pole) in market.poles.iter().enumerate() {
        let at = pole.pos.xz();
        if !inside(at) {
            continue;
        }
        let busy = busy_at(market, at);
        let rot = Quat::from_rotation_y(pole.yaw);
        let arm = |x: f32, y: f32| pole.pos + rot * Vec3::new(x, y, 0.0);
        let mut near: Vec<&super::Stall> = market.poles.iter().filter(|q| { let d = q.pos.xz().distance(at); d > 8.0 && d < 46.0 }).collect();
        near.sort_by(|a, b| a.pos.xz().distance_squared(at).total_cmp(&b.pos.xz().distance_squared(at)));
        for (n, other) in near.iter().take(2).enumerate() {
            // Each span once.
            if other.pos.x + other.pos.z < pole.pos.x + pole.pos.z {
                continue;
            }
            let other_rot = Quat::from_rotation_y(other.yaw);
            for w in 0..4 {
                let side = -0.8 + 0.53 * w as f32;
                let a = arm(side, 8.5);
                let b = other.pos + other_rot * Vec3::new(side, 8.5, 0.0);
                wires.push(Wire { a, b, sag: 0.5 + hash(k as u32, n as u32 * 7 + w, 1) * 0.5 });
            }
            // The low bundle every street grows.
            for w in 0..(1.0 + busy * 5.0) as u32 {
                let (ha, hb) = (5.2 + hash(k as u32, w, 2) * 1.8, 5.2 + hash(k as u32, w, 3) * 1.8);
                wires.push(Wire { a: pole.pos + Vec3::Y * ha, b: other.pos + Vec3::Y * hb, sag: 0.5 + hash(k as u32, w, 4) * 1.1 });
            }
            if hash(k as u32, n as u32, 9) < 0.3 {
                let m = pole.pos.lerp(other.pos, 0.3 + hash(k as u32, n as u32, 10) * 0.4);
                out.push(("crow", Vec3::new(m.x, 8.3 + m.y, m.z), (other.pos - pole.pos).xz().to_angle()));
            }
        }
        // To the nearest wall.
        let mut best: Option<(f32, Vec2)> = None;
        for t in 0..12 {
            let dir = Vec2::from_angle(t as f32 / 12.0 * std::f32::consts::TAU);
            for d in 3..20 {
                let p = at + dir * d as f32;
                if market.built(layout, p) {
                    if best.is_none_or(|b| (d as f32) < b.0) {
                        best = Some((d as f32, p));
                    }
                    break;
                }
            }
        }
        if let Some((_, wall)) = best {
            for w in 0..(1.0 + busy * 3.5) as u32 {
                let h = 4.2 + hash(k as u32, w, 6) * 2.2;
                let shift = Vec3::new(hash(k as u32, w, 7) - 0.5, 0.0, hash(k as u32, w, 8) - 0.5);
                wires.push(Wire { a: pole.pos + Vec3::Y * (5.0 + hash(k as u32, w, 5) * 2.5), b: Vec3::new(wall.x, pole.pos.y + h, wall.y) + shift, sag: 0.3 + hash(k as u32, w, 9) * 0.5 });
            }
        }
    }
}

/// Merges a patch's pieces and wires into one mesh, its origin at the patch's middle.
fn merge(pieces: &HashMap<String, Piece>, placed: &[(&'static str, Vec3, f32)], wires: &[Wire], origin: Vec3) -> Option<(Mesh, Aabb)> {
    let (mut pos, mut nrm, mut uv, mut col, mut idx): (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>, Vec<u32>) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
    for (name, at, yaw) in placed {
        let Some(p) = pieces.get(*name) else { continue };
        let rot = Quat::from_rotation_y(*yaw);
        let base = pos.len() as u32;
        for (v, n) in p.pos.iter().zip(&p.nrm) {
            let q = rot * *v + *at - origin;
            lo = lo.min(q);
            hi = hi.max(q);
            pos.push(q.to_array());
            nrm.push((rot * *n).to_array());
        }
        uv.extend_from_slice(&p.uv);
        col.extend_from_slice(&p.col);
        idx.extend(p.idx.iter().map(|i| i + base));
    }
    const STEPS: usize = 7;
    const WIDTH: f32 = 0.011;
    for w in wires {
        // Two crossed ribbons along a sagging curve: it reads as a line from any side.
        let along = (w.b - w.a).normalize_or(Vec3::X);
        let flat = along.cross(Vec3::Y).normalize_or(Vec3::X);
        for ribbon in [flat, Vec3::Y] {
            let base = pos.len() as u32;
            for s in 0..=STEPS {
                let t = s as f32 / STEPS as f32;
                let p = w.a.lerp(w.b, t) - Vec3::Y * w.sag * 4.0 * t * (1.0 - t) - origin;
                for side in [-WIDTH, WIDTH] {
                    let q = p + ribbon * side;
                    lo = lo.min(q);
                    hi = hi.max(q);
                    pos.push(q.to_array());
                    nrm.push([0.0, 1.0, 0.0]);
                    uv.push(WHITE_TEXEL);
                    col.push([0.012, 0.012, 0.013, 1.0]);
                }
            }
            for s in 0..STEPS as u32 {
                let k = base + s * 2;
                idx.extend_from_slice(&[k, k + 1, k + 2, k + 1, k + 3, k + 2]);
            }
        }
    }
    if pos.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, tiers::usage());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    tiers::pack(&mut mesh);
    Some((mesh, Aabb::from_min_max(lo, hi)))
}

/// Keeps the patches near the camera in being, and only those.
#[allow(clippy::too_many_arguments)]
fn stream(
    mut commands: Commands,
    mut goods: ResMut<Goods>,
    time: Res<Time>,
    mut every: Local<f32>,
    market: Res<Market>,
    layout: Res<MapLayout>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let goods = &mut *goods;
    if let Some(task) = &mut goods.reading
        && let Some(done) = block_on(poll_once(task))
    {
        goods.reading = None;
        if let Some((pieces, atlas)) = done {
            info!("life: {} kinds of goods", pieces.len());
            goods.pieces = Some(Arc::new(pieces));
            if let Some(mut m) = goods.material.as_ref().and_then(|m| materials.get_mut(m)) {
                m.base_color_texture = Some(images.add(atlas));
            }
        }
    }
    // A merge finished.
    if let Some((key, task)) = &mut goods.job
        && let Some(done) = block_on(poll_once(task))
    {
        let key = *key;
        goods.job = None;
        if let (Some((mesh, bounds)), Some(material)) = (done, goods.material.clone()) {
            let c = patch_centre(key);
            let e = commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    Transform::from_xyz(c.x, 0.0, c.y),
                    bounds,
                    NoAutoAabb,
                    VisibilityRange { start_margin: 0.0..0.0, end_margin: FADE_FROM..FADE_TO, use_aabb: false },
                    Tagged("life goods"),
                    OfMap,
                ))
                .id();
            goods.live.insert(key, e);
        }
    }
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.2;
    let Ok(camera) = camera.single() else { return };
    let eye = camera.translation();
    let at = eye.xz();
    let aloft = eye.y - layout.terrain.height(at.x, at.y) > TOO_HIGH;
    goods.live.retain(|key, e| {
        let keep = !aloft && patch_centre(*key).distance(at) < LET_GO;
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    if aloft || goods.job.is_some() {
        return;
    }
    let Some(pieces) = goods.pieces.clone() else { return };
    // The nearest patch not yet there, one at a time.
    let reach = (REACH / PATCH).ceil() as i32;
    let (ci, cj) = patch_at(at);
    let mut best: Option<(f32, (i32, i32))> = None;
    for i in ci - reach..=ci + reach {
        for j in cj - reach..=cj + reach {
            let key = (i, j);
            let d = patch_centre(key).distance(at);
            if d < REACH && !goods.live.contains_key(&key) && best.is_none_or(|b| d < b.0) {
                best = Some((d, key));
            }
        }
    }
    let Some((_, key)) = best else { return };
    let (mut placed, mut wires) = (vec![], vec![]);
    dress(&layout, &market, key, &mut placed, &mut wires);
    if std::env::var("VIVAD_LIFE_LOG").is_ok() {
        info!("goods patch {key:?}: {} pieces, {} wires", placed.len(), wires.len());
    }
    let c = patch_centre(key);
    let origin = Vec3::new(c.x, 0.0, c.y);
    // An empty patch still counts as there, so it isn't looked at again.
    if placed.is_empty() && wires.is_empty() {
        let e = commands.spawn((Transform::from_xyz(c.x, 0.0, c.y), OfMap)).id();
        goods.live.insert(key, e);
        return;
    }
    goods.job = Some((key, AsyncComputeTaskPool::get().spawn(async move { merge(&pieces, &placed, &wires, origin) })));
}

// ------------------------------------------------------------------ steam

/// A wisp over a kettle.
#[derive(Component)]
struct Puff {
    age: f32,
    life: f32,
    from: Vec3,
    drift: Vec2,
}

#[derive(Resource, Default)]
struct Steam {
    mesh: Option<Handle<Mesh>>,
    /// By how far gone a wisp is.
    fades: Vec<Handle<StandardMaterial>>,
    /// Kettles near the camera.
    kettles: Vec<Vec3>,
    next: f32,
    look: f32,
    /// Which kettle's turn it is.
    turn: usize,
    dice: u32,
}

const NEAR_KETTLE: f32 = 26.0;
const FADES: usize = 5;

fn puff_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let n = 32u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let d = (Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32 * 2.0 - Vec2::ONE).length();
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.4);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn steam(
    mut commands: Commands,
    mut state: ResMut<Steam>,
    time: Res<Time>,
    market: Res<Market>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    let state = &mut *state;
    let Ok(camera) = camera.single() else { return };
    let eye = camera.translation();
    state.look -= dt;
    if state.look <= 0.0 {
        state.look = 0.4;
        state.kettles.clear();
        for id in market.stalls_near(eye.xz(), NEAR_KETTLE) {
            let s = market.stalls[id as usize];
            if s.kind == super::StallKind::Chai && s.pos.distance(eye) < NEAR_KETTLE {
                state.kettles.push(s.pos + Quat::from_rotation_y(s.yaw) * Vec3::new(-0.4, 1.44, 0.0));
            }
        }
        state.kettles.truncate(3);
    }
    if !state.kettles.is_empty() {
        if state.mesh.is_none() {
            state.mesh = Some(meshes.add(Rectangle::new(1.0, 1.0)));
            let texture = images.add(puff_image());
            state.fades = (0..FADES)
                .map(|k| {
                    let alpha = 0.34 * (1.0 - k as f32 / FADES as f32);
                    materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, alpha), base_color_texture: Some(texture.clone()), alpha_mode: AlphaMode::Blend, unlit: true, ..default() })
                })
                .collect();
        }
        state.next -= dt;
        if state.next <= 0.0 {
            state.next = 0.22;
            state.turn = state.turn.wrapping_add(1);
            state.dice = state.dice.wrapping_mul(1664525).wrapping_add(1013904223);
            let r = ((state.dice >> 16) & 0xff) as f32 / 255.0 - 0.5;
            let from = state.kettles[state.turn % state.kettles.len()];
            if let Some(mesh) = state.mesh.clone() {
                commands.spawn((
                    Puff { age: 0.0, life: 1.7 + r, from, drift: Vec2::new(r * 0.5, 0.18 - r * 0.2) },
                    Mesh3d(mesh),
                    MeshMaterial3d(state.fades[0].clone()),
                    Transform::from_translation(from).with_scale(Vec3::splat(0.1)),
                    bevy::light::NotShadowCaster,
                    bevy::light::NotShadowReceiver,
                    Tagged("life steam"),
                    OfMap,
                ));
            }
        }
    }
    let turn = camera.compute_transform().rotation;
    for (e, mut p, mut tf, mut mat) in &mut puffs {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        let k = p.age / p.life;
        let wobble = (p.age * 3.0 + p.life * 9.0).sin() * 0.06;
        tf.translation = p.from + Vec3::new(p.drift.x * p.age + wobble, 0.55 * p.age, p.drift.y * p.age);
        tf.rotation = turn;
        tf.scale = Vec3::splat(0.12 + 0.55 * k);
        let fade = ((k * FADES as f32) as usize).min(FADES - 1);
        if mat.0 != state.fades[fade] {
            mat.0 = state.fades[fade].clone();
        }
    }
}
