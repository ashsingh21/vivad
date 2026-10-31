//! Trees and bushes merged into a few meshes per chunk instead of a scene each.
//!
//! A city map plants hundreds of trees, and each spawned glTF scene is about ten entities (its
//! nodes, three LODs, bark and leaves): thousands of entities to cull, extract and propagate every
//! frame, which on the browser's one thread was a fifth of the frame. Here each model is spawned
//! once, out of sight, as a template; its meshes (by LOD) and materials are taken from it; and
//! every placed copy is merged, per 32 m chunk, LOD and material, into one mesh with that chunk's
//! visibility range. Trunk colliders are the simulation's and don't change.

use std::collections::HashMap;
use std::ops::Range;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use shared::map::MapLayout;
use shared::sim::{OfMap, map_reloaded};

use crate::characters::LodRange;

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
        app.init_resource::<Templates>().add_systems(Update, (forget.run_if(map_reloaded), spawn_templates, build_batches).chain());
    }
}

/// Another map: its trees are merged afresh (the old batches went with it, being `OfMap`).
fn forget(mut t: ResMut<Templates>) {
    *t = Templates::default();
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

/// Where a model's LODs give way (as for unbatched props in crate::characters), plus the chunk slack.
fn ranges(model: &str, lods: usize) -> Vec<(Range<f32>, Range<f32>)> {
    let slack = CHUNK * 0.71;
    let (switch, far): ([f32; 2], f32) = if model.contains("bush") {
        ([35.0, 220.0], 220.0)
    } else if model.contains("banyan") {
        ([90.0, 260.0], 900.0)
    } else {
        ([60.0, 180.0], 700.0)
    };
    let mut start = 0.0..0.0;
    (0..lods)
        .map(|k| {
            let end = if k + 1 == lods { far + slack..far + slack + 30.0 } else { switch[k] + slack..switch[k] + slack + 8.0 };
            (std::mem::replace(&mut start, end.clone()), end)
        })
        .collect()
}

fn build_batches(
    mut commands: Commands,
    mut t: ResMut<Templates>,
    layout: Res<MapLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
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
