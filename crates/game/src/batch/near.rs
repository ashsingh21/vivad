//! A big map's street kit and trees close to the camera. Merging them per chunk at every LOD, as a
//! small map does, would be gigabytes on a 4 km map (a tree's three LODs are half a megabyte, there
//! are thousands, and the kit is ten times that many pieces). So here each placed thing is entities
//! that share its model's meshes, and only those near the camera exist: the renderer draws all
//! the copies of one mesh as one batch, so however many there are, the whole of the near kit and
//! trees is a few dozen batches, with no new meshes to build or upload.
//!
//! Things are spawned and despawned a whole mid cell at a time, by the same distance from the same
//! point that swaps the cell's near and mid tiers (crate::buildings::tiers), a little before the
//! mid tier starts to fade out and a little after it has finished fading in. Within a cell each
//! thing picks its own LOD by its own `VisibilityRange`, the last of which never ends.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::sim::OfMap;
use shared::sim::loot::{Chest, Pickup};

use super::probe::Tagged;
use crate::buildings::tiers::{MID_CELL, cell_at, cell_centre, near_reach};
use crate::camera::MainCamera;
use crate::characters::LodRange;
use crate::quality::{LodScale, LodShadow};

/// Metres (at full quality) past the near tier's end that a cell's things are spawned, and past
/// that again that they are despawned, so a player on the line doesn't churn them.
const LEAD: f32 = 12.0;
const LINGER: f32 = 12.0;

/// How far a big map's floor loot and chests are drawn. A dropped gun is a whole model of up to 6,300
/// triangles with a shadow, a pool of light and a beam, 2 cm of it to a pixel past this, and there
/// are 1,600 on the floor: left to the frustum they were most of the triangles in every street view.
const LOOT_REACH: f32 = 110.0;
const CHEST_REACH: f32 = 180.0;
/// Seconds between looks: at a car's 20 m/s, 5 m.
const LOOT_EVERY: f32 = 0.25;

/// One entity of a placed thing: a mesh at a LOD.
pub(crate) struct Part {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    /// Its place in the model.
    pub local: Transform,
    pub range: LodRange,
    pub tag: &'static str,
}

#[derive(Default)]
struct Cell {
    placed: Vec<(u32, Transform)>,
    spawned: Vec<Entity>,
}

/// The kinds of thing, and where each stands.
#[derive(Resource, Default)]
pub(crate) struct Near {
    kinds: Vec<Vec<Part>>,
    cells: HashMap<(i32, i32), Cell>,
    /// The cells with their things spawned.
    live: Vec<(i32, i32)>,
}

impl Near {
    /// A kind of thing: its parts. Returns what to place it by.
    pub fn kind(&mut self, parts: Vec<Part>) -> u32 {
        self.kinds.push(parts);
        self.kinds.len() as u32 - 1
    }

    pub fn place(&mut self, kind: u32, at: Transform) {
        self.cells.entry(cell_at(at.translation.x, at.translation.z)).or_default().placed.push((kind, at));
    }

    /// Everything goes with its map (being `OfMap`): start again.
    pub fn clear(&mut self) {
        *self = Near::default();
    }
}

/// Runs before anything adds to `Near` for a new map.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NearReset;

/// Hides floor loot and chests past their reach. By the root, so what is hidden costs nothing a
/// frame, where a `VisibilityRange` on each of their thousands of parts would be looked at every frame.
pub(crate) fn loot_reach(time: Res<Time>, mut wait: Local<f32>, camera: Query<&GlobalTransform, With<MainCamera>>, mut loot: Query<(&Transform, &mut Visibility, Has<Chest>), Or<(With<Pickup>, With<Chest>)>>) {
    *wait -= time.delta_secs();
    if *wait > 0.0 {
        return;
    }
    *wait = LOOT_EVERY;
    let Ok(camera) = camera.single() else { return };
    let at = camera.translation();
    for (tf, mut visibility, chest) in &mut loot {
        let reach = if chest { CHEST_REACH } else { LOOT_REACH };
        let wanted = if tf.translation.distance_squared(at) < reach * reach { Visibility::Inherited } else { Visibility::Hidden };
        visibility.set_if_neq(wanted);
    }
}

pub(crate) fn stream(mut commands: Commands, mut near: ResMut<Near>, camera: Query<&GlobalTransform, With<MainCamera>>, scale: Res<LodScale>) {
    let Ok(camera) = camera.single() else { return };
    if near.cells.is_empty() {
        return;
    }
    let near = &mut *near;
    let at = camera.translation();
    let on = near_reach(scale.0) + LEAD * scale.0;
    let off = on + LINGER * scale.0;
    let cells = &mut near.cells;
    near.live.retain(|key| {
        if cell_centre(*key).distance(at) < off {
            return true;
        }
        if let Some(cell) = cells.get_mut(key) {
            for e in cell.spawned.drain(..) {
                commands.entity(e).despawn();
            }
        }
        false
    });
    // No higher than the near tier's reach can there be a cell in it.
    if at.y.abs() > on {
        return;
    }
    let reach = (on / MID_CELL).ceil() as i32;
    let (ci, cj) = cell_at(at.x, at.z);
    for i in ci - reach..=ci + reach {
        for j in cj - reach..=cj + reach {
            let key = (i, j);
            let Some(cell) = cells.get_mut(&key) else { continue };
            if !cell.spawned.is_empty() || cell_centre(key).distance(at) >= on {
                continue;
            }
            for (kind, tf) in &cell.placed {
                for part in &near.kinds[*kind as usize] {
                    let e = commands.spawn((
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(part.material.clone()),
                        *tf * part.local,
                        part.range.scaled(scale.0),
                        part.range.clone(),
                        LodShadow,
                        Tagged(part.tag),
                        OfMap,
                    ));
                    cell.spawned.push(e.id());
                }
            }
            near.live.push(key);
        }
    }
}
