//! The street kit drawn: every placed piece (shared::map::kit) copied out of one model file
//! (tools/blender/props/street_kit.py) and merged, per 32 m chunk and LOD, into one mesh with one
//! shared material (the colours are in the vertices). A whole lane of autos, stalls and drums is a
//! couple of draws (docs/rendering-guidelines.md section 3).
//!
//! The pieces have no UVs of their own: each gets them projected in the world (along whichever axis
//! a face looks most) when merged, for a shared wear texture of grime and scratches with its normal
//! map (tools/textures/city.py). Mobile cost: two texture samples (512 px) per prop pixel, tangents
//! in the vertices; no extra draws. On at every quality level.

use std::collections::HashMap;

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{Gltf, GltfMesh};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use shared::map::MapLayout;
use shared::sim::{OfMap, map_reloaded};

use crate::characters::LodRange;

pub struct StreetPlugin;

impl Plugin for StreetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Furniture>()
            .add_systems(Startup, load_kit)
            .add_systems(Update, (rebuild.run_if(map_reloaded), build_kit.run_if(crate::loading::settled), stream_furniture.run_if(crate::loading::settled)).chain());
    }
}

const CHUNK: f32 = 32.0;
/// Where LOD1 takes over, where the far band (the heavy pieces' LOD2, the rest's LOD1 again) does,
/// and where the pieces stop being drawn (from a chunk's centre, before the quality level's
/// scale): the chunk's slack added.
const SWITCH: f32 = 35.0;
const MID: f32 = 55.0;
const FAR: f32 = 140.0;

/// The wear textures (tools/textures/city.py), and how many metres one repeat covers.
pub const WEAR: &str = "textures/panga/wear_basecolor.png";
pub const WEAR_NORMAL: &str = "textures/panga/wear_normal.png";
const WEAR_TILE: f32 = 1.5;

/// UVs projected in the world, along the axis each vertex's normal leans to most.
fn project_uvs(mesh: &mut Mesh) {
    let (Some(VertexAttributeValues::Float32x3(pos)), Some(VertexAttributeValues::Float32x3(nrm))) =
        (mesh.attribute(Mesh::ATTRIBUTE_POSITION), mesh.attribute(Mesh::ATTRIBUTE_NORMAL))
    else {
        return;
    };
    // The tangents too, from the projection (u runs along `du`, v against `dv`; `w` as
    // mikktspace gives it, bar at the seams of the projection): mikktspace on
    // the merged chunks took ten seconds at launch.
    let (uv, tangent): (Vec<[f32; 2]>, Vec<[f32; 4]>) = pos
        .iter()
        .zip(nrm)
        .map(|(p, n)| {
            let (p, n) = (Vec3::from(*p), Vec3::from(*n));
            let a = n.abs();
            let (u, v, du, dv) = if a.x > a.y && a.x > a.z { (p.z, p.y, Vec3::Z, Vec3::Y) } else if a.z > a.y { (p.x, p.y, Vec3::X, Vec3::Y) } else { (p.x, p.z, Vec3::X, Vec3::Z) };
            let t = (du - n * n.dot(du)).normalize_or(Vec3::X);
            let w = if n.cross(t).dot(dv) < 0.0 { -1.0 } else { 1.0 };
            ([u / WEAR_TILE, -v / WEAR_TILE], [t.x, t.y, t.z, w])
        })
        .unzip();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangent);
}

#[derive(Resource)]
struct Kit {
    file: Handle<Gltf>,
    /// Pieces with a baked texture of their own (tools/blender/props/auto.py, furniture.py),
    /// merged per chunk apart from the rest, with their material.
    textured: Vec<Handle<Gltf>>,
    material: Handle<StandardMaterial>,
    /// Whether this map's pieces have been built.
    built: bool,
}

fn load_kit(mut commands: Commands, assets: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Kit {
        file: assets.load("models/props/street_kit.glb"),
        textured: vec![assets.load("models/props/auto.glb"), assets.load("models/props/furniture.glb"), assets.load("models/props/racks.glb")],
        material: materials.add(StandardMaterial {
            // The wear texture is 0.8 where clean.
            base_color: Color::linear_rgb(1.25, 1.25, 1.25),
            base_color_texture: Some(crate::ground::load_detail(&assets, WEAR, true)),
            normal_map_texture: Some(crate::ground::load_detail(&assets, WEAR_NORMAL, false)),
            perceptual_roughness: 0.75,
            ..default()
        }),
        built: false,
    });
}

fn rebuild(mut kit: ResMut<Kit>, mut furniture: ResMut<Furniture>) {
    kit.built = false;
    *furniture = Furniture::default();
}

/// The hollow buildings' furniture (shared::map::interiors::Interior::kit_pieces: some 40,000 on
/// the NCR), drawn near the camera only: by cell, spawned as the camera comes within `NEAR` and
/// freed past `KEEP`, each piece an entity sharing the kit's mesh for it (drawn instanced). Their
/// colliders are the buildings' own (crate::sim).
#[derive(Resource, Default)]
struct Furniture {
    ready: bool,
    cells: HashMap<(i32, i32), Vec<(&'static str, Vec3, f32)>>,
    /// Each piece's mesh and material, by name and LOD (0 near, 1 past `SWITCH_IN`).
    meshes: HashMap<(&'static str, usize), (Handle<Mesh>, Handle<StandardMaterial>)>,
    loaded: HashMap<(i32, i32), Vec<Entity>>,
}

const ROOM_CELL: f32 = 16.0;
const NEAR: f32 = 50.0;
const KEEP: f32 = 70.0;
const SWITCH_IN: f32 = 22.0;

#[allow(clippy::too_many_arguments)]
fn stream_furniture(
    mut commands: Commands,
    mut f: ResMut<Furniture>,
    kit: Res<Kit>,
    layout: Res<MapLayout>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_mats: Res<Assets<bevy::gltf::GltfMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut frame: Local<u32>,
) {
    if layout.rooms.is_empty() || !shared::map::compose::ROOMS_BUILT || crate::diag::skip("kit") {
        return;
    }
    if !f.ready {
        let Some(file) = gltfs.get(&kit.file) else { return };
        let Some(textured) = kit.textured.iter().map(|h| gltfs.get(h)).collect::<Option<Vec<&Gltf>>>() else { return };
        f.ready = true;
        let mut cells: HashMap<(i32, i32), Vec<(&'static str, Vec3, f32)>> = HashMap::new();
        for p in layout.rooms.iter().flat_map(|r| r.kit_pieces()) {
            cells.entry(((p.1.x / ROOM_CELL).floor() as i32, (p.1.z / ROOM_CELL).floor() as i32)).or_default().push(p);
        }
        let names: std::collections::HashSet<&'static str> = cells.values().flatten().map(|p| p.0).collect();
        let mut own_mats: HashMap<AssetId<bevy::gltf::GltfMaterial>, Handle<StandardMaterial>> = HashMap::new();
        for name in names {
            for lod in 0..2 {
                let label = format!("{name}_LOD{lod}");
                let (gm, own) = match textured.iter().find_map(|t| t.named_meshes.get(label.as_str())) {
                    Some(h) => (gltf_meshes.get(h), true),
                    None => (file.named_meshes.get(label.as_str()).and_then(|h| gltf_meshes.get(h)), false),
                };
                let Some(prim) = gm.and_then(|g| g.primitives.first()) else { continue };
                let Some(mut mesh) = meshes.get(&prim.mesh).cloned() else { continue };
                let material = match prim.material.as_ref().filter(|_| own).and_then(|m| Some((m.id(), gltf_mats.get(m)?))) {
                    Some((id, gm)) => own_mats.entry(id).or_insert_with(|| materials.add(bevy::pbr::gltf::standard_material_from_gltf_material(gm))).clone(),
                    None => {
                        project_uvs(&mut mesh);
                        kit.material.clone()
                    }
                };
                if materials.get(&material).is_some_and(|m| m.normal_map_texture.is_some()) && !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
                    let _ = mesh.generate_tangents();
                }
                mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
                f.meshes.insert((name, lod), (meshes.add(mesh), material));
            }
        }
        f.cells = cells;
        info!("room furniture: {} pieces in {} cells, drawn near", f.cells.values().map(Vec::len).sum::<usize>(), f.cells.len());
    }
    // Every few frames is enough at a run's pace.
    *frame += 1;
    if *frame % 6 != 0 {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let at = cam.translation().xz();
    let dist = |c: (i32, i32)| {
        let lo = Vec2::new(c.0 as f32, c.1 as f32) * ROOM_CELL;
        (at.clamp(lo, lo + Vec2::splat(ROOM_CELL)) - at).length()
    };
    let gone: Vec<(i32, i32)> = f.loaded.keys().copied().filter(|c| dist(*c) > KEEP).collect();
    for c in gone {
        for e in f.loaded.remove(&c).unwrap_or_default() {
            commands.entity(e).despawn();
        }
    }
    let r = (NEAR / ROOM_CELL).ceil() as i32 + 1;
    let (cx, cz) = ((at.x / ROOM_CELL).floor() as i32, (at.y / ROOM_CELL).floor() as i32);
    for x in cx - r..=cx + r {
        for z in cz - r..=cz + r {
            let c = (x, z);
            if f.loaded.contains_key(&c) || dist(c) > NEAR {
                continue;
            }
            let Some(pieces) = f.cells.get(&c) else { continue };
            let mut spawned = vec![];
            for &(name, pos, yaw) in pieces {
                let tf = Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw));
                for lod in 0..2 {
                    let Some((mesh, material)) = f.meshes.get(&(name, lod)).cloned() else { continue };
                    let (s, e) = [(0.0..0.0, SWITCH_IN..SWITCH_IN + 4.0), (SWITCH_IN..SWITCH_IN + 4.0, KEEP..KEEP + 5.0)][lod].clone();
                    let range = LodRange(VisibilityRange { start_margin: s, end_margin: e, use_aabb: false }, lod);
                    spawned.push(commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), tf, range.0.clone(), range, bevy::light::NotShadowCaster, OfMap)).id());
                }
            }
            f.loaded.insert(c, spawned);
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Above this many pieces each is its own entity, drawn instanced, instead of merged per chunk, in
/// the browser too (crate::diag::instanced: the desktop always instances).
const INSTANCE_ABOVE: usize = 8000;

fn build_kit(
    mut commands: Commands,
    mut kit: ResMut<Kit>,
    layout: Res<MapLayout>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_mats: Res<Assets<bevy::gltf::GltfMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if kit.built {
        return;
    }
    if crate::diag::skip("kit") {
        kit.built = true;
        return;
    }
    let Some(file) = gltfs.get(&kit.file) else { return };
    let Some(textured) = kit.textured.iter().map(|h| gltfs.get(h)).collect::<Option<Vec<&Gltf>>>() else { return };
    kit.built = true;
    if layout.kit.is_empty() {
        return;
    }
    // Each piece's mesh by name and LOD: from a textured file when one has it (with its own
    // material, one per file's material however many pieces share it), else the kit's.
    let mut file_mats: HashMap<AssetId<bevy::gltf::GltfMaterial>, Handle<StandardMaterial>> = HashMap::new();
    let mut mesh_of = |name: &str, lod: usize| -> Option<(Mesh, Option<Handle<StandardMaterial>>)> {
        let label = format!("{name}_LOD{lod}");
        let (gm, own) = match textured.iter().find_map(|t| t.named_meshes.get(label.as_str())) {
            Some(h) => (gltf_meshes.get(h)?, true),
            None => (gltf_meshes.get(file.named_meshes.get(label.as_str())?)?, false),
        };
        let prim = gm.primitives.first()?;
        let own = if own {
            prim.material.as_ref().and_then(|m| {
                let gm = gltf_mats.get(m)?;
                Some(file_mats.entry(m.id()).or_insert_with(|| materials.add(bevy::pbr::gltf::standard_material_from_gltf_material(gm))).clone())
            })
        } else {
            None
        };
        Some((meshes.get(&prim.mesh)?.clone(), own))
    };
    let mut templates: HashMap<(&str, usize), Option<(Mesh, Option<Handle<StandardMaterial>>)>> = HashMap::new();
    let indoor = |name: &str| name.starts_with("rack_");
    // A big map's kit (the NCR's 9,000 pieces), each piece its own entity sharing its model's mesh,
    // drawn instanced: merged, every copy's vertices repeat at every LOD (2.7 GB on the NCR). The
    // wear's projected UVs are then the piece's own, not the world's.
    if crate::diag::instanced(layout.kit.len(), INSTANCE_ABOVE) {
        // First every piece's meshes (`mesh_of` holds the asset stores while it's in use), then
        // each made once into a shared mesh.
        for piece in &layout.kit {
            for lod in 0..3 {
                templates.entry((piece.name, lod)).or_insert_with(|| mesh_of(piece.name, lod));
            }
        }
        let mut shared: HashMap<(&str, usize), Option<(Handle<Mesh>, Handle<StandardMaterial>)>> = HashMap::new();
        let mut spawned = 0;
        for piece in &layout.kit {
            let tf = Transform::from_translation(piece.pos).with_rotation(Quat::from_rotation_y(piece.yaw));
            for lod in 0..if indoor(piece.name) { 2 } else { 3 } {
                let have = templates.get(&(piece.name, lod)).is_some_and(|t| t.is_some());
                let from = if lod == 2 && !have { 1 } else { lod };
                let made = shared.entry((piece.name, from)).or_insert_with(|| {
                    let (template, own) = templates.get(&(piece.name, from))?.clone()?;
                    let mut mesh = template;
                    if own.is_none() {
                        project_uvs(&mut mesh);
                    }
                    let material = own.unwrap_or_else(|| kit.material.clone());
                    if materials.get(&material).is_some_and(|m| m.normal_map_texture.is_some()) && !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
                        let _ = mesh.generate_tangents();
                    }
                    // On the GPU only, as the city's buildings are (crate::buildings).
                    mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
                    Some((meshes.add(mesh), material))
                });
                let Some((mesh, material)) = made.clone() else { continue };
                let (s, e) = [(0.0..0.0, SWITCH..SWITCH + 6.0), (SWITCH..SWITCH + 6.0, MID..MID + 8.0), (MID..MID + 8.0, FAR..FAR + 15.0)][lod].clone();
                let range = LodRange(VisibilityRange { start_margin: s, end_margin: e, use_aabb: false }, lod);
                let mut e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), tf, range.0.clone(), range, OfMap));
                if indoor(piece.name) {
                    e.insert(bevy::light::NotShadowCaster);
                } else {
                    e.insert(crate::quality::LodShadow);
                }
                spawned += 1;
            }
        }
        info!("street kit: {} pieces instanced as {spawned} entities", layout.kit.len());
        return;
    }
    let mut chunks: HashMap<(i32, i32, usize, Option<AssetId<StandardMaterial>>), Mesh> = HashMap::new();
    let mut own_materials: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>> = HashMap::new();
    // Indoor pieces the shops are lined with (`indoor`): drawn near only (no far band) and casting
    // no shadow (they stand under roofs), apart from the rest.
    let mut indoor_mats: std::collections::HashSet<AssetId<StandardMaterial>> = std::collections::HashSet::new();
    for piece in &layout.kit {
        let key = ((piece.pos.x / CHUNK).floor() as i32, (piece.pos.z / CHUNK).floor() as i32);
        for lod in 0..if indoor(piece.name) { 2 } else { 3 } {
            // The far band: LOD2 where the piece has one, else its LOD1.
            let have = templates.entry((piece.name, lod)).or_insert_with(|| mesh_of(piece.name, lod)).is_some();
            let from = if lod == 2 && !have { 1 } else { lod };
            let Some((template, own)) = templates.entry((piece.name, from)).or_insert_with(|| mesh_of(piece.name, from)) else { continue };
            let mut placed = template.clone().transformed_by(Transform::from_translation(piece.pos).with_rotation(Quat::from_rotation_y(piece.yaw)));
            let mat = own.as_ref().map(|h| {
                own_materials.insert(h.id(), h.clone());
                if indoor(piece.name) {
                    indoor_mats.insert(h.id());
                }
                h.id()
            });
            if mat.is_none() {
                project_uvs(&mut placed);
            }
            match chunks.get_mut(&(key.0, key.1, lod, mat)) {
                Some(m) => {
                    if let Err(e) = m.merge(&placed) {
                        warn_once!("street kit: {} doesn't merge: {e}", piece.name);
                    }
                }
                None => {
                    chunks.insert((key.0, key.1, lod, mat), placed);
                }
            }
        }
    }
    let slack = CHUNK * 0.71;
    let ranges = [
        (0.0..0.0, SWITCH + slack..SWITCH + slack + 6.0),
        (SWITCH + slack..SWITCH + slack + 6.0, MID + slack..MID + slack + 8.0),
        (MID + slack..MID + slack + 8.0, FAR + slack..FAR + slack + 15.0),
    ];
    let n = chunks.len();
    for ((cx, cz, lod, mat), mesh) in chunks {
        // The origin at the chunk's centre: LOD distances are measured from it.
        let centre = Vec3::new((cx as f32 + 0.5) * CHUNK, 0.0, (cz as f32 + 0.5) * CHUNK);
        let mut mesh = mesh.translated_by(-centre);
        // On the GPU only, as the city's buildings are (crate::buildings).
        mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
        let material = mat.and_then(|id| own_materials.get(&id).cloned()).unwrap_or_else(|| kit.material.clone());
        let bumpy = materials.get(&material).is_some_and(|m| m.normal_map_texture.is_some());
        if bumpy && !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
            let _ = mesh.generate_tangents();
        }
        let (s, e) = ranges[lod].clone();
        let range = LodRange(VisibilityRange { start_margin: s, end_margin: e, use_aabb: false }, lod);
        let mut piece = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::from_translation(centre), range.0.clone(), range, OfMap));
        if mat.is_some_and(|m| indoor_mats.contains(&m)) {
            piece.insert(bevy::light::NotShadowCaster);
        } else {
            piece.insert(crate::quality::LodShadow);
        }
    }
    let mut counts: Vec<(&str, usize)> = vec![];
    for piece in &layout.kit {
        match counts.iter_mut().find(|(k, _)| *k == piece.name) {
            Some((_, c)) => *c += 1,
            None => counts.push((piece.name, 1)),
        }
    }
    info!("street kit: {} pieces in {n} chunk meshes: {counts:?}", layout.kit.len());
}
