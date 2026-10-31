//! Life that costs nothing to simulate: soft dark patches under warriors that keep them on the
//! ground where real shadows stop, and Delhi's sky: kites on their strings and pigeons wheeling
//! over the rooftops.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use shared::map::MapLayout;
use shared::sim::{OfMap, map_reloaded, solid_mask};

use crate::visuals::WarriorVisual;

pub struct AmbientPlugin;

impl Plugin for AmbientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup, spawn_sky.after(setup)))
            .add_systems(Update, (spawn_sky.run_if(map_reloaded), add_blobs, place_blobs, fly_kites, fly_pigeons));
    }
}

#[derive(Resource)]
struct AmbientAssets {
    blob_mesh: Handle<Mesh>,
    blob_mat: Handle<StandardMaterial>,
}

/// The patch under a warrior.
#[derive(Component)]
struct Blob {
    of: Entity,
}

#[derive(Component)]
struct Kite {
    anchor: Vec3,
    phase: f32,
    speed: f32,
}

#[derive(Component)]
struct Pigeon {
    centre: Vec3,
    radius: f32,
    phase: f32,
    speed: f32,
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    // Soft round falloff, darkest in the middle.
    let n = 64u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32 * 2.0 - Vec2::ONE;
            let a = (1.0 - d.length()).clamp(0.0, 1.0).powf(1.6);
            data.extend_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
        }
    }
    let tex = images.add(Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default()));
    commands.insert_resource(AmbientAssets {
        blob_mesh: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
        blob_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 0.55),
            base_color_texture: Some(tex),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            depth_bias: 50.0,
            ..default()
        }),
    });
}

fn add_blobs(mut commands: Commands, q: Query<Entity, Added<WarriorVisual>>, assets: Res<AmbientAssets>) {
    for e in &q {
        commands.spawn((
            Blob { of: e },
            Mesh3d(assets.blob_mesh.clone()),
            MeshMaterial3d(assets.blob_mat.clone()),
            Transform::from_scale(Vec3::ZERO),
            bevy::light::NotShadowCaster,
            bevy::light::NotShadowReceiver,
        ));
    }
}

/// Each patch on whatever is under its warrior, smaller and gone as they rise off it.
#[allow(clippy::type_complexity)]
fn place_blobs(mut commands: Commands, mut blobs: Query<(Entity, &Blob, &mut Transform)>, vis: Query<(&Transform, &Visibility), (With<WarriorVisual>, Without<Blob>)>, spatial: SpatialQuery) {
    let filter = SpatialQueryFilter::from_mask(solid_mask());
    for (e, b, mut t) in &mut blobs {
        let Ok((wt, v)) = vis.get(b.of) else {
            commands.entity(e).despawn();
            continue;
        };
        let from = wt.translation + Vec3::Y * 0.5;
        let hit = (*v != Visibility::Hidden).then(|| spatial.cast_ray(from, Dir3::NEG_Y, 8.0, true, &filter)).flatten();
        match hit {
            Some(h) => {
                let up = h.distance - 0.5;
                let k = (1.0 - up / 6.0).clamp(0.0, 1.0);
                t.translation = from - Vec3::Y * h.distance + h.normal * 0.02;
                t.rotation = Quat::from_rotation_arc(Vec3::Y, h.normal);
                t.scale = Vec3::splat(1.3 * k.max(0.0001));
            }
            None => t.scale = Vec3::ZERO,
        }
    }
}

/// A kite: a diamond with a tail, both faces drawn.
fn kite_mesh() -> Mesh {
    let (w, h) = (0.45, 0.55);
    let mut pos = vec![[0.0, h, 0.0], [w, 0.0, 0.0], [0.0, -h, 0.0], [-w, 0.0, 0.0]];
    // A short tail, a thin triangle hanging from the bottom corner.
    pos.extend_from_slice(&[[0.0, -h, 0.0], [0.08, -h - 0.9, 0.0], [-0.08, -h - 0.9, 0.0]]);
    let n = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; n])
        .with_inserted_indices(Indices::U32(vec![0, 3, 1, 1, 3, 2, 4, 6, 5]))
}

/// A bird: two wings in a shallow V, both faces drawn.
fn bird_mesh() -> Mesh {
    let pos = vec![[0.0, 0.0, 0.12], [0.0, 0.0, -0.12], [-0.35, 0.08, 0.0], [0.35, 0.08, 0.0]];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 1, 0, 3]))
}

const KITE_COLOURS: [(f32, f32, f32); 6] = [(0.95, 0.20, 0.25), (0.98, 0.75, 0.10), (0.20, 0.55, 0.95), (0.25, 0.75, 0.30), (0.95, 0.45, 0.10), (0.85, 0.25, 0.75)];

fn spawn_sky(mut commands: Commands, layout: Res<MapLayout>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Delhi's sky only.
    if layout.river.is_some() || layout.footprints.is_empty() {
        return;
    }
    let half = layout.half_extent;
    let kite = meshes.add(kite_mesh());
    let bird = meshes.add(bird_mesh());
    let mut s = 0x1234_5678u32 ^ layout.name.len() as u32;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s % 10_000) as f32 / 10_000.0
    };
    for k in 0..18 {
        let (r, g, b) = KITE_COLOURS[k % KITE_COLOURS.len()];
        let mat = materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), double_sided: true, cull_mode: None, perceptual_roughness: 0.8, ..default() });
        let anchor = Vec3::new((rnd() * 2.0 - 1.0) * half, 30.0 + rnd() * 30.0, (rnd() * 2.0 - 1.0) * half);
        commands.spawn((
            Kite { anchor, phase: rnd() * TAU, speed: 0.25 + rnd() * 0.3 },
            Mesh3d(kite.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(anchor).with_scale(Vec3::splat(2.4)),
            bevy::light::NotShadowCaster,
            OfMap,
        ));
    }
    let grey = materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.33, 0.36), double_sided: true, cull_mode: None, perceptual_roughness: 0.9, ..default() });
    for _ in 0..3 {
        let centre = Vec3::new((rnd() * 2.0 - 1.0) * half * 0.7, 22.0 + rnd() * 12.0, (rnd() * 2.0 - 1.0) * half * 0.7);
        let (radius, speed) = (14.0 + rnd() * 10.0, 0.35 + rnd() * 0.15);
        for _ in 0..14 {
            commands.spawn((
                Pigeon { centre: centre + Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5) * 4.0, radius: radius + rnd() * 3.0, phase: rnd() * 1.2, speed },
                Mesh3d(bird.clone()),
                MeshMaterial3d(grey.clone()),
                Transform::from_translation(centre),
                bevy::light::NotShadowCaster,
                OfMap,
            ));
        }
    }
}

/// Kites drift in slow loops round where their string holds them, tilting into the wind and shivering.
fn fly_kites(time: Res<Time>, mut q: Query<(&Kite, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (k, mut tf) in &mut q {
        let a = t * k.speed + k.phase;
        tf.translation = k.anchor + Vec3::new(a.sin() * 6.0, (a * 2.0).sin() * 2.5, (a * 0.7).cos() * 3.0);
        tf.rotation = Quat::from_rotation_y(0.6 + (a * 0.5).sin() * 0.3) * Quat::from_rotation_z((a * 1.3).cos() * 0.5 + (t * 9.0 + k.phase).sin() * 0.05);
    }
}

/// Pigeons wheel in a loose flock, flapping.
fn fly_pigeons(time: Res<Time>, mut q: Query<(&Pigeon, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (p, mut tf) in &mut q {
        let a = t * p.speed + p.phase;
        let pos = p.centre + Vec3::new(a.cos() * p.radius, (a * 3.0).sin() * 1.5, a.sin() * p.radius);
        let ahead = Vec3::new(-a.sin(), 0.0, a.cos());
        tf.translation = pos;
        tf.rotation = Transform::IDENTITY.looking_to(ahead, Vec3::Y).rotation * Quat::from_rotation_z(0.3);
        tf.scale = Vec3::new(1.0, 1.0 + (t * 14.0 + p.phase * 9.0).sin() * 1.6, 1.0);
    }
}
