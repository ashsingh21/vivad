//! Shared loading and collision for the small Delhi environment sample.
//! This is independent of the battle-royale match and the full NCR generator.
use avian3d::prelude::*;
use bevy::prelude::*;
use serde::Deserialize;
use shared::sim::{
    CAPSULE_LENGTH, CAPSULE_RADIUS, HALF_HEIGHT, Kinematic, Layer, MoveMode, PlayerIntent,
};

/// A small original daylight cubemap for both the visible sky and filtered environment light.
/// The same asset is used by the sample's desktop and mobile profiles.
pub fn daylight_cubemap() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    };
    const N: u32 = 128;
    let faces: [fn(f32, f32) -> Vec3; 6] = [
        |u, v| Vec3::new(1.0, -v, -u),
        |u, v| Vec3::new(-1.0, -v, u),
        |u, v| Vec3::new(u, 1.0, v),
        |u, v| Vec3::new(u, -1.0, -v),
        |u, v| Vec3::new(u, -v, 1.0),
        |u, v| Vec3::new(-u, -v, -1.0),
    ];
    fn noise(p: Vec3) -> f32 {
        let base = p.floor();
        let t = p - base;
        let t = t * t * (Vec3::splat(3.0) - t * 2.0);
        let hash = |x: i32, y: i32, z: i32| {
            let mut h = (x as u32).wrapping_mul(374761393)
                ^ (y as u32).wrapping_mul(668265263)
                ^ (z as u32).wrapping_mul(2246822519);
            h = (h ^ (h >> 13)).wrapping_mul(1274126177);
            ((h ^ (h >> 16)) & 65535) as f32 / 65535.0
        };
        let mut value = 0.0;
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let w = if x == 0 { 1.0 - t.x } else { t.x }
                        * if y == 0 { 1.0 - t.y } else { t.y }
                        * if z == 0 { 1.0 - t.z } else { t.z };
                    value += hash(base.x as i32 + x, base.y as i32 + y, base.z as i32 + z) * w;
                }
            }
        }
        value
    }
    let mut data = Vec::with_capacity((N * N * 24) as usize);
    for face in faces {
        for y in 0..N {
            for x in 0..N {
                let d = face(
                    (x as f32 + 0.5) / N as f32 * 2.0 - 1.0,
                    (y as f32 + 0.5) / N as f32 * 2.0 - 1.0,
                )
                .normalize();
                let elevation = d.y.max(0.0);
                let mut c = Vec3::new(0.69, 0.77, 0.84)
                    .lerp(Vec3::new(0.19, 0.43, 0.73), elevation.powf(0.45));
                if d.y < 0.0 {
                    c = c.lerp(Vec3::new(0.24, 0.23, 0.21), (-d.y * 7.0).min(1.0));
                } else {
                    let p = d * 4.4 + Vec3::new(3.4, 7.1, 1.8);
                    let n = noise(p) * 0.56 + noise(p * 2.1) * 0.28 + noise(p * 4.3) * 0.16;
                    let cloud = ((n - 0.50) / 0.20).clamp(0.0, 1.0);
                    let cloud =
                        cloud * cloud * (3.0 - 2.0 * cloud) * (elevation * 8.0).clamp(0.0, 1.0);
                    c = c.lerp(Vec3::new(0.93, 0.95, 0.97), cloud * 0.92);
                }
                data.extend_from_slice(&[
                    (c.x * 255.0) as u8,
                    (c.y * 255.0) as u8,
                    (c.z * 255.0) as u8,
                    255,
                ]);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: N,
            height: N,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

pub const MODEL: &str = "models/samples/delhi_street/street.gltf";

#[derive(Deserialize, Resource)]
pub struct SamplePlan {
    pub colliders: Vec<SampleCollider>,
    pub props: Vec<SampleProp>,
}

#[derive(Deserialize)]
pub struct SampleCollider {
    pub name: String,
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Deserialize)]
pub struct SampleProp {
    pub model: String,
    pub position: [f32; 3],
    pub scale: f32,
}

impl SamplePlan {
    pub fn load() -> Self {
        let path = format!(
            "{}/models/samples/delhi_street/collision.ron",
            crate::ASSET_DIR
        );
        ron::from_str(&std::fs::read_to_string(&path).expect("generate street sample assets first"))
            .expect("valid street sample collision data")
    }
}

#[derive(Component)]
pub struct SampleWalker;

pub fn spawn_colliders(mut commands: Commands, plan: Res<SamplePlan>) {
    for c in &plan.colliders {
        commands.spawn((
            Name::new(c.name.clone()),
            RigidBody::Static,
            Collider::cuboid(c.size[0], c.size[1], c.size[2]),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            Transform::from_translation(Vec3::from_array(c.center)).with_rotation(
                Quat::from_euler(EulerRot::XYZ, c.rotation[0], c.rotation[1], c.rotation[2]),
            ),
        ));
    }
}

pub fn walker_bundle(feet: Vec3) -> impl Bundle {
    (
        SampleWalker,
        Transform::from_translation(feet + Vec3::Y * (HALF_HEIGHT + 0.04)),
        RigidBody::Kinematic,
        Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
        CollisionLayers::new(Layer::Character, [Layer::World]),
        Kinematic {
            mode: MoveMode::Ground,
            ..default()
        },
        PlayerIntent::default(),
    )
}
