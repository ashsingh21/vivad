//! Trees and bushes merged into a few meshes per chunk instead of a scene each.
//!
//! A city map plants hundreds of trees, and each spawned glTF scene is about ten entities (its
//! nodes, three LODs, bark and leaves): thousands of entities to cull, extract and propagate every
//! frame, which on the browser's one thread was a fifth of the frame. Here each model is spawned
//! once, out of sight, as a template; its meshes (by LOD) and materials are taken from it; and
//! every placed copy is merged, per 32 m chunk, LOD and material, into one mesh with that chunk's
//! visibility range. Trunk colliders are the simulation's and don't change.
//!
//! A big map can't afford every LOD of every tree merged: there only the leaves of the last LOD are,
//! per mid cell of its tiers (crate::buildings::tiers), and near the camera each tree is entities
//! sharing its model's meshes (`near`).

use std::collections::HashMap;
use std::ops::Range;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use shared::map::MapLayout;
use shared::sim::{OfMap, map_reloaded};

use crate::buildings::tiers;
use crate::characters::LodRange;

pub mod near;
pub mod probe;
pub mod upload;

/// Chunk side: a tree is at most `CHUNK * 0.71` from its chunk's centre, the slack in its LOD
/// switch distances.
const CHUNK: f32 = 32.0;

/// Props merged rather than spawned one by one.
pub fn batched(model: &str) -> bool {
    model.contains("tree_") || model.contains("bush_")
}

/// One mesh of a template: its LOD, mesh, material and place in the model's frame.
struct Part {
    lod: usize,
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    local: Transform,
}

#[derive(Resource, Default)]
struct Templates {
    /// Model path -> its parts, once its template scene has spawned.
    parts: HashMap<&'static str, Vec<Part>>,
    asked: bool,
    built: bool,
}

pub struct BatchPlugin;

impl Plugin for BatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((probe::ProbePlugin, upload::UploadPlugin))
            .init_resource::<Templates>()
            .init_resource::<near::Near>()
            .add_systems(Update, ((forget.run_if(map_reloaded)).in_set(near::NearReset), (spawn_templates, build_batches, near::stream).chain().after(near::NearReset)));
    }
}

/// Another map: its trees are merged afresh (the old batches went with it, being `OfMap`).
fn forget(mut t: ResMut<Templates>, mut near: ResMut<near::Near>) {
    *t = Templates::default();
    near.clear();
}

fn spawn_templates(mut commands: Commands, mut t: ResMut<Templates>, layout: Res<MapLayout>, assets: Res<AssetServer>) {
    if t.asked {
        return;
    }
    t.asked = true;
    let mut models: Vec<&'static str> = layout.props.iter().map(|p| p.model).filter(|m| batched(m)).collect();
    models.sort();
    models.dedup();
    for model in models {
        commands
            .spawn((
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(model))),
                Transform::from_xyz(0.0, -10_000.0, 0.0),
                Visibility::Hidden,
            ))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, parents: Query<&ChildOf>, names: Query<&Name>,
                           tfs: Query<&Transform>, parts: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>, mut t: ResMut<Templates>,
                           mut commands: Commands| {
                let root = ready.entity;
                let mut found = vec![];
                for e in children.iter_descendants(root) {
                    let Ok((mesh, mat)) = parts.get(e) else { continue };
                    // Its LOD from the nearest ancestor named ..._LOD<n>; its transform up to the root.
                    let lod = std::iter::once(e).chain(parents.iter_ancestors(e)).find_map(|a| {
                        names.get(a).ok()?.as_str().rsplit("LOD").next()?.parse::<usize>().ok()
                    });
                    let mut local = Transform::IDENTITY;
                    for a in std::iter::once(e).chain(parents.iter_ancestors(e)).take_while(|a| *a != root) {
                        local = tfs.get(a).copied().unwrap_or_default() * local;
                    }
                    found.push(Part { lod: lod.unwrap_or(0), mesh: mesh.0.clone(), mat: mat.0.clone(), local });
                }
                t.parts.insert(model, found);
                commands.entity(root).despawn();
            });
    }
}

/// Where a model's LODs give way, and where its last one fades (as for unbatched props in
/// crate::characters).
fn switches(model: &str) -> ([f32; 2], f32) {
    if model.contains("bush") {
        ([35.0, 220.0], 220.0)
    } else if model.contains("banyan") {
        ([90.0, 260.0], 900.0)
    } else {
        ([60.0, 180.0], 700.0)
    }
}

/// A model's LODs as merged per chunk: `switches` plus the chunk slack.
fn ranges(model: &str, lods: usize) -> Vec<(Range<f32>, Range<f32>)> {
    let slack = CHUNK * 0.71;
    let (switch, far) = switches(model);
    let mut start = 0.0..0.0;
    (0..lods)
        .map(|k| {
            let end = if k + 1 == lods { far + slack..far + slack + 30.0 } else { switch[k] + slack..switch[k] + slack + 8.0 };
            (std::mem::replace(&mut start, end.clone()), end)
        })
        .collect()
}

/// A big map's trees near the camera: each LOD by its own distance, and the last with no end, as
/// `near` takes the tree away along with its cell.
fn near_parts(model: &str, parts: &[Part]) -> Vec<near::Part> {
    let (switch, _) = switches(model);
    let last = parts.iter().map(|p| p.lod).max().unwrap_or(0);
    parts
        .iter()
        .map(|p| {
            let start = if p.lod == 0 { 0.0..0.0 } else { switch[p.lod - 1]..switch[p.lod - 1] + 8.0 };
            let end = if p.lod == last { 1.0e5..1.0e5 } else { switch[p.lod]..switch[p.lod] + 8.0 };
            near::Part {
                mesh: p.mesh.clone(),
                material: p.mat.clone(),
                local: p.local,
                range: LodRange(VisibilityRange { start_margin: start, end_margin: end, use_aabb: false }, p.lod),
                tag: "near trees",
            }
        })
        .collect()
}

/// A big map's trees: near the camera in `near`, and past that the leaves of the last LOD merged
/// per mid cell, with the trunks and crowns of crate::buildings::tiers under them.
fn build_big(commands: &mut Commands, t: &Templates, layout: &MapLayout, meshes: &mut Assets<Mesh>, materials: &Assets<StandardMaterial>, near: &mut near::Near, scale: f32) {
    let is_leaves = |p: &Part| materials.get(&p.mat).is_some_and(|m| !matches!(m.alpha_mode, AlphaMode::Opaque));
    let kinds: HashMap<&str, u32> = t.parts.iter().map(|(model, parts)| (*model, near.kind(near_parts(model, parts)))).collect();
    let mut cells: HashMap<(i32, i32), Mesh> = HashMap::new();
    // All the models' leaves are cards from one atlas: one material does for them all.
    let mut material = None;
    for p in layout.props.iter().filter(|p| batched(p.model)) {
        let tf = Transform::from_translation(p.pos).with_rotation(Quat::from_rotation_y(p.yaw)).with_scale(Vec3::splat(p.scale));
        near.place(kinds[p.model], tf);
        let parts = &t.parts[p.model];
        let last = parts.iter().map(|q| q.lod).max().unwrap_or(0);
        for part in parts.iter().filter(|q| q.lod == last && is_leaves(q)) {
            let Some(src) = meshes.get(&part.mesh).cloned() else { continue };
            material.get_or_insert_with(|| part.mat.clone());
            let placed = src.transformed_by(tf * part.local);
            match cells.entry(tiers::cell_at(p.pos.x, p.pos.z)) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(placed);
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if e.get_mut().merge(&placed).is_err() {
                        warn!("couldn't merge the leaves of {}", p.model);
                    }
                }
            }
        }
    }
    let Some(material) = material else { return };
    let n = cells.len();
    for (cell, mesh) in cells {
        let origin = tiers::cell_centre(cell);
        let mut mesh = mesh.translated_by(-origin);
        mesh.asset_usage = tiers::usage();
        tiers::pack(&mut mesh);
        let base = tiers::mid_range();
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(origin),
            base.scaled(scale),
            base,
            crate::quality::LodShadow,
            probe::Tagged("mid trees"),
            OfMap,
        ));
    }
    info!("big map: {} trees and bushes, their far leaves in {n} cells", layout.props.iter().filter(|p| batched(p.model)).count());
}

#[allow(clippy::too_many_arguments)]
fn build_batches(
    mut commands: Commands,
    mut t: ResMut<Templates>,
    layout: Res<MapLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    mut near: ResMut<near::Near>,
    scale: Res<crate::quality::LodScale>,
) {
    if t.built || !t.asked {
        return;
    }
    let wanted: Vec<&'static str> = layout.props.iter().map(|p| p.model).filter(|m| batched(m)).collect();
    if wanted.iter().any(|m| !t.parts.contains_key(m)) {
        return;
    }
    if t.parts.values().flatten().any(|p| meshes.get(&p.mesh).is_none()) {
        return;
    }
    if tiers::big_map(&layout) {
        build_big(&mut commands, &t, &layout, &mut meshes, &materials, &mut near, scale.0);
        t.built = true;
        return;
    }
    // Group the placed copies by model and chunk.
    let mut groups: HashMap<(&'static str, i32, i32), Vec<Transform>> = HashMap::new();
    for p in layout.props.iter().filter(|p| batched(p.model)) {
        let tf = Transform::from_translation(p.pos).with_rotation(Quat::from_rotation_y(p.yaw)).with_scale(Vec3::splat(p.scale));
        groups.entry((p.model, (p.pos.x / CHUNK).floor() as i32, (p.pos.z / CHUNK).floor() as i32)).or_default().push(tf);
    }
    let mut spawned = 0;
    for ((model, _, _), copies) in groups {
        let parts = &t.parts[model];
        let lods = parts.iter().map(|p| p.lod).max().unwrap_or(0) + 1;
        let rs = ranges(model, lods);
        for (pi, part) in parts.iter().enumerate() {
            let Some(src) = meshes.get(&part.mesh).cloned() else { continue };
            let mut merged: Option<Mesh> = None;
            for tf in &copies {
                let m = src.clone().transformed_by(*tf * part.local);
                match merged.as_mut() {
                    None => merged = Some(m),
                    Some(acc) => {
                        if acc.merge(&m).is_err() {
                            warn!("couldn't merge a copy of {model} (part {pi})");
                        }
                    }
                }
            }
            let Some(mut mesh) = merged else { continue };
            // Its origin at the copies' centre: LOD distances (and their crossfade) are measured
            // from there.
            let centre = copies.iter().map(|c| c.translation).sum::<Vec3>() / copies.len() as f32;
            mesh.translate_by(-centre);
            let base = LodRange(VisibilityRange { start_margin: rs[part.lod].0.clone(), end_margin: rs[part.lod].1.clone(), use_aabb: false }, part.lod);
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(part.mat.clone()),
                Transform::from_translation(centre),
                base.scaled(scale.0),
                base,
                crate::quality::LodShadow,
                OfMap,
            ));
            spawned += 1;
        }
    }
    t.built = true;
    info!("batched {} trees and bushes into {spawned} meshes", layout.props.iter().filter(|p| batched(p.model)).count());
}
