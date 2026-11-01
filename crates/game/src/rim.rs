//! The characters' material: their glTF materials swapped, once per warrior, for the same material
//! extended (assets/shaders/rim.wesl, rim_prepass.wesl) with
//! - a rim light, a silhouette glow, so players read against busy, colourful streets;
//! - cloth that moves: loose garments (the kurti's skirt and panels, the gamchha's ends, shirt
//!   hems, and the hair; their second UV set says how loose, tools/blender/characters/finish.py) trail
//!   behind the warrior's motion, lift when it falls and flutter, from its velocity each frame
//!   (packed into each part's `MeshTag`: the materials themselves never change, and are shared).
//!
//! A few vertex and fragment instructions on the characters only: on at every quality level.

use std::collections::HashMap;

use bevy::mesh::MeshTag;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct RimPlugin;

impl Plugin for RimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<RimMaterial>::default())
            .init_resource::<Converted>()
            .init_resource::<Cloth>()
            .add_systems(PostUpdate, (convert, sway).chain());
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
    fn vertex_shader() -> ShaderRef {
        "shaders/rim.wesl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/rim.wesl".into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/rim_prepass.wesl".into()
    }
}

/// Put on a character's parts: its material is rimmed, and its cloth moves with this entity (the
/// warrior's visual, whose motion it follows).
#[derive(Component)]
pub struct RimTarget(pub Entity);

/// A converted part: its cloth follows this warrior's motion, through its `MeshTag`.
#[derive(Component)]
struct Sways(Entity);

/// The rimmed copy of each material, made once and shared by every warrior wearing it (the
/// motion goes per mesh, in its tag, so the materials never change and batch together).
#[derive(Resource, Default)]
struct Converted(HashMap<AssetId<StandardMaterial>, Handle<RimMaterial>>);

/// Each warrior's last position and smoothed velocity.
#[derive(Resource, Default)]
struct Cloth(HashMap<Entity, (Option<Vec3>, Vec3)>);

/// A pale sky-warm white: reads as light, not as a coloured outline.
const RIM: Vec4 = Vec4::new(1.0, 0.95, 0.88, 0.22);

fn convert(
    mut commands: Commands,
    q: Query<(Entity, &MeshMaterial3d<StandardMaterial>, &RimTarget)>,
    standard: Res<Assets<StandardMaterial>>,
    mut rimmed: ResMut<Assets<RimMaterial>>,
    mut done: ResMut<Converted>,
) {
    for (e, m, target) in &q {
        let Some(base) = standard.get(&m.0) else { continue };
        let handle = done.0.entry(m.0.id()).or_insert_with(|| rimmed.add(ExtendedMaterial { base: base.clone(), extension: Rim { rim: RIM } })).clone();
        commands
            .entity(e)
            .remove::<(MeshMaterial3d<StandardMaterial>, RimTarget)>()
            .insert((MeshMaterial3d(handle), Sways(target.0), MeshTag::new(pack(Vec3::ZERO))));
    }
}

/// A velocity packed for the shader (cloth.wesl's unpack_velocity): 10 bits a component, x and z
/// over +-16 m/s, y over +-48.
fn pack(v: Vec3) -> u32 {
    let q = |c: f32, range: f32| (((c / range).clamp(-1.0, 1.0) * 0.5 + 0.5) * 1023.0).round() as u32;
    q(v.x, 16.0) | q(v.y, 48.0) << 10 | q(v.z, 16.0) << 20
}

/// Each warrior's velocity, from how far it moved since the last frame (smoothed, so cloth eases
/// rather than snapping), into its parts' tags (only when it changes); warriors gone are forgotten.
fn sway(time: Res<Time>, warriors: Query<&GlobalTransform>, mut cloth: ResMut<Cloth>, mut parts: Query<(&Sways, &mut MeshTag)>) {
    let dt = time.delta_secs().max(1e-4);
    cloth.0.retain(|e, _| warriors.contains(*e));
    for (sways, _) in &parts {
        cloth.0.entry(sways.0).or_default();
    }
    for (e, (prev, vel)) in cloth.0.iter_mut() {
        let Ok(tf) = warriors.get(*e) else { continue };
        let p = tf.translation();
        let v = prev.map_or(Vec3::ZERO, |q| (p - q) / dt);
        // Teleports (respawn, a new match) aren't motion.
        let v = if v.length() > 60.0 { Vec3::ZERO } else { v };
        *vel = vel.lerp(v, (dt * 8.0).min(1.0));
        *prev = Some(p);
    }
    for (sways, mut tag) in &mut parts {
        if let Some((_, vel)) = cloth.0.get(&sways.0) {
            tag.set_if_neq(MeshTag::new(pack(*vel)));
        }
    }
}
