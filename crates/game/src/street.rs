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
        app.add_systems(Startup, load_kit).add_systems(Update, (rebuild.run_if(map_reloaded), build_kit).chain());
    }
}

const CHUNK: f32 = 32.0;
/// Where LOD1 takes over, and where the pieces stop being drawn (from a chunk's centre, before the
/// quality level's scale): the chunk's slack added.
const SWITCH: f32 = 35.0;
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
    let uv: Vec<[f32; 2]> = pos
        .iter()
        .zip(nrm)
        .map(|(p, n)| {
            let (p, a) = (Vec3::from(*p), Vec3::from(*n).abs());
            let (u, v) = if a.x > a.y && a.x > a.z { (p.z, p.y) } else if a.z > a.y { (p.x, p.y) } else { (p.x, p.z) };
            [u / WEAR_TILE, -v / WEAR_TILE]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
}

#[derive(Resource)]
struct Kit {
    file: Handle<Gltf>,
    material: Handle<StandardMaterial>,
    /// Whether this map's pieces have been built.
    built: bool,
}

fn load_kit(mut commands: Commands, assets: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Kit {
        file: assets.load("models/props/street_kit.glb"),
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

fn rebuild(mut kit: ResMut<Kit>) {
    kit.built = false;
}

fn build_kit(mut commands: Commands, mut kit: ResMut<Kit>, layout: Res<MapLayout>, gltfs: Res<Assets<Gltf>>, gltf_meshes: Res<Assets<GltfMesh>>, mut meshes: ResMut<Assets<Mesh>>) {
    if kit.built {
        return;
    }
    let Some(file) = gltfs.get(&kit.file) else { return };
    kit.built = true;
    if layout.kit.is_empty() {
        return;
    }
    // Each piece's mesh by name and LOD.
    let mesh_of = |name: &str, lod: usize| -> Option<Mesh> {
        let gm = gltf_meshes.get(file.named_meshes.get(format!("{name}_LOD{lod}").as_str())?)?;
        meshes.get(&gm.primitives.first()?.mesh).cloned()
    };
    let mut templates: HashMap<(&str, usize), Option<Mesh>> = HashMap::new();
    let mut chunks: HashMap<(i32, i32, usize), Mesh> = HashMap::new();
    for piece in &layout.kit {
        let key = ((piece.pos.x / CHUNK).floor() as i32, (piece.pos.z / CHUNK).floor() as i32);
        for lod in 0..2 {
            let Some(template) = templates.entry((piece.name, lod)).or_insert_with(|| mesh_of(piece.name, lod)) else { continue };
            let mut placed = template.clone().transformed_by(Transform::from_translation(piece.pos).with_rotation(Quat::from_rotation_y(piece.yaw)));
            project_uvs(&mut placed);
            match chunks.get_mut(&(key.0, key.1, lod)) {
                Some(m) => {
                    if let Err(e) = m.merge(&placed) {
                        warn_once!("street kit: {} doesn't merge: {e}", piece.name);
                    }
                }
                None => {
                    chunks.insert((key.0, key.1, lod), placed);
                }
            }
        }
    }
    let slack = CHUNK * 0.71;
    let ranges = [(0.0..0.0, SWITCH + slack..SWITCH + slack + 6.0), (SWITCH + slack..SWITCH + slack + 6.0, FAR + slack..FAR + slack + 15.0)];
    let n = chunks.len();
    for ((cx, cz, lod), mesh) in chunks {
        // The origin at the chunk's centre: LOD distances are measured from it.
        let centre = Vec3::new((cx as f32 + 0.5) * CHUNK, 0.0, (cz as f32 + 0.5) * CHUNK);
        let mut mesh = mesh.translated_by(-centre);
        let _ = mesh.generate_tangents();
        let (s, e) = ranges[lod].clone();
        let range = LodRange(VisibilityRange { start_margin: s, end_margin: e, use_aabb: false }, lod);
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(kit.material.clone()),
            Transform::from_translation(centre),
            range.0.clone(),
            range,
            crate::quality::LodShadow,
            OfMap,
        ));
    }
    info!("street kit: {} pieces in {n} chunk meshes", layout.kit.len());
}
