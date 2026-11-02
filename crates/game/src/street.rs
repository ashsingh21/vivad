//! The street kit drawn: every placed piece (shared::map::kit) copied out of one model file
//! (tools/blender/props/street_kit.py) and merged, per 32 m chunk and LOD, into one mesh with one
//! shared material (the colours are in the vertices). A whole lane of autos, stalls and drums is a
//! couple of draws (docs/rendering-guidelines.md section 3).
//!
//! A big map's kit isn't merged: near the camera each piece is entities sharing the kit's meshes
//! (crate::batch::near), and past that its cover is boxes in the tiers (crate::buildings::tiers).

use std::collections::HashMap;

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{Gltf, GltfMesh};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use shared::map::MapLayout;
use shared::sim::{OfMap, map_reloaded};

use crate::batch::near::{self, Near, NearReset};
use crate::buildings::tiers;
use crate::characters::LodRange;

pub struct StreetPlugin;

impl Plugin for StreetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_kit).add_systems(Update, (rebuild.run_if(map_reloaded), build_kit).chain().after(NearReset));
    }
}

const CHUNK: f32 = 32.0;
/// Where LOD1 takes over, and where the pieces stop being drawn (from a chunk's centre, before the
/// quality level's scale): the chunk's slack added.
const SWITCH: f32 = 35.0;
const FAR: f32 = 140.0;
/// How much of a street kit colour the sun has taken, and what dust leaves of what remains: the model
/// is painted as bought, and nothing on a Delhi street stays that way.
const FADED: f32 = 0.5;
const DUSTY: f32 = 0.92;
/// Metres of a piece to one repeat of its wear map, and that map's mean in linear light (tools/textures/
/// kit_wear.py). The model has no UVs, so they are made here.
const WEAR_TILE: f32 = 1.2;
pub(crate) const WEAR_MEAN: f32 = 0.80;
/// How high the road's dust climbs a piece, in metres, its colour (linear) and how much of it there is
/// at the foot.
const DUST_HEIGHT: f32 = 0.55;
const DUST: [f32; 3] = [0.20, 0.165, 0.125];
const DUST_FOOT: f32 = 0.5;

#[derive(Resource)]
pub(crate) struct Kit {
    file: Handle<Gltf>,
    material: Handle<StandardMaterial>,
    /// Whether this map's pieces have been built.
    pub built: bool,
    /// Whether the model's colours have been weathered, which they need only once.
    weathered: bool,
}

fn load_kit(mut commands: Commands, assets: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Kit {
        file: assets.load("models/props/street_kit.glb"),
        material: materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(assets.load("models/textures/kit_wear.png")), perceptual_roughness: 0.75, ..default() }),
        built: false,
        weathered: false,
    });
}

fn rebuild(mut kit: ResMut<Kit>) {
    kit.built = false;
}

/// A colour of the kit (linear) after some years outdoors.
pub(crate) fn weathered(c: [f32; 3]) -> [f32; 3] {
    let grey = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    c.map(|x| (x + (grey - x) * FADED) * DUSTY)
}

/// Each vertex's UV, projected along its normal's main axis, so wear lies on a piece at one scale
/// whichever way its faces turn.
fn wear_uv(p: [f32; 3], n: [f32; 3]) -> [f32; 2] {
    let (x, y, z) = (n[0].abs(), n[1].abs(), n[2].abs());
    let flat = if y >= x && y >= z {
        [p[0], p[2]]
    } else if x >= z {
        [p[2], p[1]]
    } else {
        [p[0], p[1]]
    };
    [flat[0] / WEAR_TILE, flat[1] / WEAR_TILE]
}

/// A weathered colour with the road's dust on it, `y` metres up the piece.
fn dusted(c: [f32; 3], y: f32) -> [f32; 3] {
    let k = DUST_FOOT * (1.0 - (y / DUST_HEIGHT).clamp(0.0, 1.0)).powi(2);
    [c[0] + (DUST[0] - c[0]) * k, c[1] + (DUST[1] - c[1]) * k, c[2] + (DUST[2] - c[2]) * k]
}

/// Weathers the model where it is loaded, as every piece shares these meshes: its colours, and the
/// UVs its wear map needs.
fn weather(file: &Gltf, gltf_meshes: &Assets<GltfMesh>, meshes: &mut Assets<Mesh>) {
    for mesh in file.named_meshes.values().filter_map(|h| gltf_meshes.get(h)).flat_map(|m| &m.primitives) {
        let Some(mut mesh) = meshes.get_mut(&mesh.mesh) else { continue };
        let (Some(VertexAttributeValues::Float32x3(pos)), Some(VertexAttributeValues::Float32x3(nrm))) = (mesh.attribute(Mesh::ATTRIBUTE_POSITION), mesh.attribute(Mesh::ATTRIBUTE_NORMAL)) else { continue };
        let uv: Vec<[f32; 2]> = pos.iter().zip(nrm).map(|(p, n)| wear_uv(*p, *n)).collect();
        let heights: Vec<f32> = pos.iter().map(|p| p[1]).collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        if let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
            for (c, y) in colours.iter_mut().zip(heights) {
                let [r, g, b] = dusted(weathered([c[0], c[1], c[2]]), y);
                *c = [r, g, b, c[3]];
            }
        }
    }
}

/// A big map's kit near the camera: LOD1 has no end, as `near` takes the piece away with its cell.
fn near_kit(near: &mut Near, kit: &Kit, layout: &MapLayout, file: &Gltf, gltf_meshes: &Assets<GltfMesh>) {
    let mut kinds: HashMap<&str, Option<u32>> = HashMap::new();
    let ranges = [(0.0..0.0, SWITCH..SWITCH + 6.0), (SWITCH..SWITCH + 6.0, 1.0e5..1.0e5)];
    for piece in &layout.kit {
        let kind = *kinds.entry(piece.name).or_insert_with(|| {
            let parts: Vec<near::Part> = ranges
                .iter()
                .enumerate()
                .filter_map(|(lod, (s, e))| {
                    let gm = gltf_meshes.get(file.named_meshes.get(format!("{}_LOD{lod}", piece.name).as_str())?)?;
                    Some(near::Part {
                        mesh: gm.primitives.first()?.mesh.clone(),
                        material: kit.material.clone(),
                        local: Transform::IDENTITY,
                        range: LodRange(VisibilityRange { start_margin: s.clone(), end_margin: e.clone(), use_aabb: false }, lod),
                        tag: "near kit",
                    })
                })
                .collect();
            (!parts.is_empty()).then(|| near.kind(parts))
        });
        if let Some(kind) = kind {
            near.place(kind, Transform::from_translation(piece.pos).with_rotation(Quat::from_rotation_y(piece.yaw)));
        }
    }
    info!("street kit: {} pieces near the camera only", layout.kit.len());
}

#[allow(clippy::too_many_arguments)]
fn build_kit(mut commands: Commands, mut kit: ResMut<Kit>, layout: Res<MapLayout>, gltfs: Res<Assets<Gltf>>, gltf_meshes: Res<Assets<GltfMesh>>, mut meshes: ResMut<Assets<Mesh>>, mut near: ResMut<Near>) {
    if kit.built {
        return;
    }
    let Some(file) = gltfs.get(&kit.file) else { return };
    kit.built = true;
    if !kit.weathered {
        kit.weathered = true;
        weather(file, &gltf_meshes, &mut meshes);
    }
    if layout.kit.is_empty() {
        return;
    }
    if tiers::big_map(&layout) {
        near_kit(&mut near, &kit, &layout, file, &gltf_meshes);
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
            let placed = template.clone().transformed_by(Transform::from_translation(piece.pos).with_rotation(Quat::from_rotation_y(piece.yaw)));
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
        let mesh = mesh.translated_by(-centre);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dust_lies_at_a_piece_foot() {
        let c = [0.1, 0.3, 0.6];
        assert_eq!(dusted(c, DUST_HEIGHT), c);
        let foot = dusted(c, 0.0);
        assert!((foot[2] - DUST[2]).abs() < (c[2] - DUST[2]).abs(), "no dust at the foot: {foot:?}");
    }

    #[test]
    fn wear_lies_along_each_face() {
        // Up a wall v is height, whichever way the wall faces; on a top, it is plan.
        assert_eq!(wear_uv([1.2, 2.4, 0.0], [0.0, 0.0, 1.0]), [1.0, 2.0]);
        assert_eq!(wear_uv([0.0, 2.4, 1.2], [1.0, 0.0, 0.0]), [1.0, 2.0]);
        assert_eq!(wear_uv([1.2, 9.0, 2.4], [0.0, 1.0, 0.0]), [1.0, 2.0]);
    }

    #[test]
    fn weathering_calms_a_colour_and_leaves_grey_grey() {
        let blue = weathered([0.006, 0.119, 0.579]);
        assert!(blue[2] - blue[0] < 0.6 * (0.579 - 0.006), "still as saturated: {blue:?}");
        let grey = weathered([0.5, 0.5, 0.5]);
        assert!((grey[0] - grey[2]).abs() < 1e-5 && grey[0] < 0.5, "grey should only darken: {grey:?}");
    }
}
