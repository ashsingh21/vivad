//! Map lamps: oil flames that flicker and light their surroundings, and unseen fill lights that
//! stand in for daylight bounced into dark interiors (the caves).

use std::f32::consts::TAU;

use bevy::prelude::*;
use shared::map::MapLayout;

pub struct LampsPlugin;

impl Plugin for LampsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_lamps).add_systems(Update, flicker);
    }
}

#[derive(Component)]
struct Flicker {
    base: f32,
    seed: f32,
}

#[derive(Component)]
struct Flame {
    seed: f32,
}

fn spawn_lamps(mut commands: Commands, layout: Res<MapLayout>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let flame_mesh = meshes.add(Sphere::new(1.0).mesh().uv(10, 8));
    let flame_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.75, 0.35),
        emissive: LinearRgba::rgb(14.0, 6.0, 1.6),
        unlit: true,
        ..default()
    });
    for (i, lamp) in layout.lamps.iter().enumerate() {
        let seed = i as f32 * 7.31;
        let light = PointLight {
            color: Color::linear_rgb(lamp.color.x, lamp.color.y, lamp.color.z),
            intensity: lamp.lumens,
            range: lamp.range,
            shadow_maps_enabled: false,
            ..default()
        };
        let mut e = commands.spawn((light, Transform::from_translation(lamp.pos + Vec3::Y * 0.08)));
        if let Some((r, n)) = lamp.flames {
            e.insert(Flicker { base: lamp.lumens, seed });
            commands.spawn((Transform::from_translation(lamp.pos), Visibility::default())).with_children(|c| {
                for k in 0..n {
                    let a = TAU * k as f32 / n as f32;
                    c.spawn((
                        Mesh3d(flame_mesh.clone()),
                        MeshMaterial3d(flame_mat.clone()),
                        // A teardrop-ish flame: a thin, tall ellipsoid sitting on the wick.
                        Transform::from_xyz(r * a.cos(), 0.025, -r * a.sin()).with_scale(Vec3::new(0.011, 0.028, 0.011)),
                        bevy::light::NotShadowCaster,
                        Flame { seed: seed + k as f32 * 1.7 },
                    ));
                }
            });
        }
    }
}

/// Two incommensurate sines per lamp: a slow breathing plus a quick shiver.
fn wobble(t: f32, seed: f32) -> f32 {
    (t * 2.3 + seed).sin() * 0.6 + (t * 11.7 + seed * 3.1).sin() * 0.3 + (t * 23.0 + seed * 5.7).sin() * 0.1
}

fn flicker(time: Res<Time>, mut lights: Query<(&mut PointLight, &Flicker)>, mut flames: Query<(&mut Transform, &Flame)>) {
    let t = time.elapsed_secs();
    for (mut l, f) in &mut lights {
        l.intensity = f.base * (0.88 + 0.12 * wobble(t, f.seed));
    }
    for (mut tf, f) in &mut flames {
        let w = wobble(t, f.seed);
        tf.scale.y = 0.028 * (1.0 + 0.18 * w);
    }
}
