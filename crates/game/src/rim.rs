//! Rim light on the characters: their glTF materials swapped, once each, for the same material
//! extended with a silhouette glow (assets/shaders/rim.wesl), so players read against busy, colourful
//! streets. A few shader instructions on the character pixels only: on at every quality level.

use std::collections::HashMap;

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct RimPlugin;

impl Plugin for RimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<RimMaterial>::default()).init_resource::<Converted>().add_systems(PostUpdate, convert);
    }
}

pub type RimMaterial = ExtendedMaterial<StandardMaterial, Rim>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct Rim {
    /// rgb: colour (linear), a: strength.
    #[uniform(100)]
    pub rim: Vec4,
}

impl MaterialExtension for Rim {
    fn fragment_shader() -> ShaderRef {
        "shaders/rim.wesl".into()
    }
}

/// Put on an entity (a character's scene root's descendants) to have its material rimmed.
#[derive(Component)]
pub struct RimTarget;

/// The rimmed copy of each material, made once.
#[derive(Resource, Default)]
struct Converted(HashMap<AssetId<StandardMaterial>, Handle<RimMaterial>>);

/// A pale sky-warm white: reads as light, not as a coloured outline.
const RIM: Vec4 = Vec4::new(1.0, 0.95, 0.88, 0.22);

fn convert(
    mut commands: Commands,
    q: Query<(Entity, &MeshMaterial3d<StandardMaterial>), With<RimTarget>>,
    standard: Res<Assets<StandardMaterial>>,
    mut rimmed: ResMut<Assets<RimMaterial>>,
    mut done: ResMut<Converted>,
) {
    for (e, m) in &q {
        let Some(base) = standard.get(&m.0) else { continue };
        let handle = done.0.entry(m.0.id()).or_insert_with(|| rimmed.add(ExtendedMaterial { base: base.clone(), extension: Rim { rim: RIM } })).clone();
        commands.entity(e).remove::<(MeshMaterial3d<StandardMaterial>, RimTarget)>().insert(MeshMaterial3d(handle));
    }
}
