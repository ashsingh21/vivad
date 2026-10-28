//! Procedural "studio" cubemap so metals have something to reflect before real skies exist.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

const SIZE: u32 = 128;

/// Builds a cube map: dark floor, mid-grey walls, a large top softbox and a side strip light.
pub fn studio_cubemap() -> Image {
    // Face order +X, -X, +Y, -Y, +Z, -Z; each maps (u, v) in [-1, 1] to a direction.
    let faces: [fn(f32, f32) -> Vec3; 6] = [
        |u, v| Vec3::new(1.0, -v, -u),
        |u, v| Vec3::new(-1.0, -v, u),
        |u, v| Vec3::new(u, 1.0, v),
        |u, v| Vec3::new(u, -1.0, -v),
        |u, v| Vec3::new(u, -v, 1.0),
        |u, v| Vec3::new(-u, -v, -1.0),
    ];
    let mut data = Vec::with_capacity((SIZE * SIZE * 4 * 6) as usize);
    for face in faces {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let u = (x as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                let c = radiance(face(u, v).normalize());
                let c = c.clamp(Vec3::ZERO, Vec3::ONE);
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
        Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 6 },
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

fn radiance(d: Vec3) -> Vec3 {
    let floor = Vec3::splat(0.04);
    let wall = Vec3::splat(0.16);
    let mut c = floor.lerp(wall, ((d.y + 0.15) / 0.4).clamp(0.0, 1.0));
    // Overhead softbox, slightly behind.
    let top = Vec3::new(0.0, 1.0, -0.35).normalize();
    c += Vec3::splat(1.0) * smooth(0.80, 0.90, d.dot(top));
    // Vertical strip light to one side.
    let strip = Vec3::new(1.0, 0.0, 0.4).normalize();
    let side = Vec3::new(d.x, 0.0, d.z).normalize_or_zero().dot(strip);
    c += Vec3::new(0.95, 0.97, 1.0) * smooth(0.96, 0.99, side) * smooth(0.7, 0.2, d.y.abs());
    c
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
