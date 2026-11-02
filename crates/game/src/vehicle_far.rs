//! Vehicles a long way off. A vehicle's model is 5,000 to 14,000 triangles in up to a dozen meshes,
//! and alone of what's on the map it was drawn whole at any distance: on a busy street they were over
//! half the triangles in view, paid for again in the depth pass, for things a few pixels long. Past
//! `FULL_TO` each is a box the size of its collider in its kind's colour, so a car is still cover you
//! can see; all the boxes of a kind draw as one, whatever the number.
//!
//! This only watches what crate::vehicles spawns: nothing there knows of it.

use bevy::camera::visibility::VisibilityRange;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use shared::sim::vehicle::{Vehicle, VehicleKind};

use crate::characters::LodRange;
use crate::quality::LodScale;
use crate::vehicles::VehicleVisual;

pub struct VehicleFarPlugin;

impl Plugin for VehicleFarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Boxes>().add_systems(Update, (stand_ins, full_only_near));
    }
}

/// Where a model hands over to its box, at High, and over how far. A level's LOD scale moves it.
const FULL_TO: f32 = 320.0;
const FADE: f32 = 20.0;
/// However deep a model's meshes are under its visual.
const DEEPEST: usize = 10;

/// A model's meshes.
fn near_range() -> LodRange {
    LodRange(VisibilityRange { start_margin: 0.0..0.0, end_margin: FULL_TO..FULL_TO + FADE, use_aabb: false }, 0)
}

/// A box.
fn far_range() -> LodRange {
    LodRange(VisibilityRange { start_margin: FULL_TO..FULL_TO + FADE, end_margin: 1.0e5..1.0e5, use_aabb: false }, 1)
}

/// sRGB, about what each kind's model comes to from afar.
fn colour(kind: VehicleKind) -> Color {
    match kind {
        VehicleKind::Auto => Color::srgb(0.38, 0.46, 0.20),
        VehicleKind::Scooter => Color::srgb(0.34, 0.34, 0.36),
        VehicleKind::Commuter | VehicleKind::Bullet => Color::srgb(0.18, 0.18, 0.20),
        VehicleKind::Car => Color::srgb(0.70, 0.70, 0.68),
        VehicleKind::Khuli => Color::srgb(0.30, 0.33, 0.22),
        VehicleKind::Sarkari => Color::srgb(0.78, 0.78, 0.74),
        VehicleKind::ChhotaTruck => Color::srgb(0.56, 0.58, 0.60),
        VehicleKind::Tractor => Color::srgb(0.46, 0.17, 0.11),
    }
}

#[derive(Resource, Default)]
struct Boxes {
    mesh: Option<Handle<Mesh>>,
    materials: Vec<(VehicleKind, Handle<StandardMaterial>)>,
}

fn stand_ins(
    mut commands: Commands,
    mut boxes: ResMut<Boxes>,
    lod: Res<LodScale>,
    added: Query<(Entity, &VehicleVisual), Added<VehicleVisual>>,
    sims: Query<&Vehicle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, visual) in &added {
        let Ok(vehicle) = sims.get(visual.sim) else { continue };
        let kind = vehicle.kind;
        let mesh = boxes.mesh.get_or_insert_with(|| meshes.add(Cuboid::new(1.0, 1.0, 1.0))).clone();
        let known = boxes.materials.iter().find(|(k, _)| *k == kind).map(|(_, m)| m.clone());
        let material = known.unwrap_or_else(|| {
            let m = materials.add(StandardMaterial { base_color: colour(kind), perceptual_roughness: 0.6, ..default() });
            boxes.materials.push((kind, m.clone()));
            m
        });
        let range = far_range();
        // The visual's origin is its collider's middle.
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_scale(kind.spec().size), range.scaled(lod.0), range, NotShadowCaster, ChildOf(e)));
    }
}

/// Every mesh of a model as it comes in: none has a range of its own.
fn full_only_near(mut commands: Commands, lod: Res<LodScale>, added: Query<Entity, (Added<Mesh3d>, Without<VisibilityRange>)>, parents: Query<&ChildOf>, visuals: Query<(), With<VehicleVisual>>) {
    for e in &added {
        let mut at = e;
        for _ in 0..DEEPEST {
            let Ok(up) = parents.get(at) else { break };
            at = up.parent();
            if visuals.contains(at) {
                let range = near_range();
                commands.entity(e).insert((range.scaled(lod.0), range));
                break;
            }
        }
    }
}
