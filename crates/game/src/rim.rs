//! The characters' material: their glTF materials swapped, once per warrior, for the same material
//! extended (assets/shaders/rim.wesl, rim_prepass.wesl) with
//! - a rim light, a silhouette glow, so players read against busy, colourful streets;
//! - cloth that moves: loose garments (the kurti's skirt and panels, the gamchha's ends, shirt
//!   hems, and the hair; their second UV set says how loose, tools/blender/characters/finish.py) trail
//!   behind the warrior's motion, lift when it falls and flutter, from its velocity each frame.
//!
//! A few vertex and fragment instructions on the characters only: on at every quality level.

use std::collections::HashMap;

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
    /// xyz: the warrior's velocity (m/s, world), w: time (s): for the cloth.
    #[uniform(101)]
    pub motion: Vec4,
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

/// Each warrior's rimmed copy of each material, made once.
#[derive(Resource, Default)]
struct Converted(HashMap<(Entity, AssetId<StandardMaterial>), Handle<RimMaterial>>);

/// A warrior's materials, its last position and its smoothed velocity.
type Swaying = (Vec<Handle<RimMaterial>>, Option<Vec3>, Vec3);

#[derive(Resource, Default)]
struct Cloth(HashMap<Entity, Swaying>);

/// A pale sky-warm white: reads as light, not as a coloured outline.
const RIM: Vec4 = Vec4::new(1.0, 0.95, 0.88, 0.22);

fn convert(
    mut commands: Commands,
    q: Query<(Entity, &MeshMaterial3d<StandardMaterial>, &RimTarget)>,
    standard: Res<Assets<StandardMaterial>>,
    mut rimmed: ResMut<Assets<RimMaterial>>,
    mut done: ResMut<Converted>,
    mut cloth: ResMut<Cloth>,
) {
    for (e, m, target) in &q {
        let Some(base) = standard.get(&m.0) else { continue };
        let handle = done
            .0
            .entry((target.0, m.0.id()))
            .or_insert_with(|| {
                let h = rimmed.add(ExtendedMaterial { base: base.clone(), extension: Rim { rim: RIM, motion: Vec4::ZERO } });
                cloth.0.entry(target.0).or_default().0.push(h.clone());
                h
            })
            .clone();
        commands.entity(e).remove::<(MeshMaterial3d<StandardMaterial>, RimTarget)>().insert(MeshMaterial3d(handle));
    }
}

/// Each warrior's velocity, from how far it moved since the last frame (smoothed, so cloth eases
/// rather than snapping), into its materials; warriors gone are forgotten.
fn sway(time: Res<Time>, warriors: Query<&GlobalTransform>, mut cloth: ResMut<Cloth>, mut done: ResMut<Converted>, mut rimmed: ResMut<Assets<RimMaterial>>) {
    let dt = time.delta_secs().max(1e-4);
    let t = time.elapsed_secs_wrapped();
    cloth.0.retain(|e, _| warriors.contains(*e));
    done.0.retain(|(e, _), _| warriors.contains(*e));
    for (e, (mats, prev, vel)) in cloth.0.iter_mut() {
        let Ok(tf) = warriors.get(*e) else { continue };
        let p = tf.translation();
        let v = prev.map_or(Vec3::ZERO, |q| (p - q) / dt);
        // Teleports (respawn, a new match) aren't motion.
        let v = if v.length() > 60.0 { Vec3::ZERO } else { v };
        *vel = vel.lerp(v, (dt * 8.0).min(1.0));
        *prev = Some(p);
        for h in mats.iter() {
            if let Some(mut m) = rimmed.get_mut(h) {
                m.extension.motion = vel.extend(t);
            }
        }
    }
}
