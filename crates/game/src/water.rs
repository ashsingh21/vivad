//! Water surfaces (the river, the stepwell's pool, the fort's channels): a murky green-brown
//! surface with a generated ripple normal map that drifts slowly, instead of a glassy slab that
//! mirrors the sky. On Ultra crate::surfaces gives each one a finer ripple map and a cleaner
//! surface, which then does mirror the sky: alpha-blended, so forward, with no screen-space
//! reflections of the city (crate::surfaces says why).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use shared::map::{Mat, Shape};
use shared::sim::MapBlock;

use crate::surfaces::{Surface, UltraSurfaces, WATER_TILE};

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        // After the buildings, whose rebuild on a new map starts crate::surfaces' list afresh.
        app.add_systems(Startup, setup).add_systems(Update, (dress_water.after(crate::buildings::build_chunks), flow));
    }
}

/// Metres per repeat of the ripple map.
pub(crate) const TILE: f32 = 7.0;
const SIZE: usize = 256;

#[derive(Resource)]
struct Ripples(Handle<Image>);

/// Each water surface's material and how many ripple tiles (of `TILE` metres) it spans per UV.
#[derive(Component)]
pub(crate) struct Flowing(pub(crate) Handle<StandardMaterial>, pub(crate) Vec2);

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(Ripples(images.add(ripple_map())));
}

/// A tileable normal map of crossing ripples (integer frequencies so it wraps), mip-mapped.
fn ripple_map() -> Image {
    let waves: [(f32, f32, f32, f32); 6] =
        [(3.0, 1.0, 0.0, 1.0), (-2.0, 4.0, 1.3, 0.7), (5.0, -3.0, 2.1, 0.45), (1.0, 7.0, 0.4, 0.3), (-8.0, 5.0, 2.7, 0.2), (11.0, 9.0, 1.9, 0.12)];
    let tau = std::f32::consts::TAU;
    let n = SIZE;
    let mut h = vec![0.0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            h[y * n + x] = waves.iter().map(|&(fx, fy, ph, a)| a * (tau * (fx * u + fy * v) + ph).sin()).sum();
        }
    }
    let mut level = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let at = |dx: i32, dy: i32| h[((y as i32 + dy).rem_euclid(n as i32) as usize) * n + (x as i32 + dx).rem_euclid(n as i32) as usize];
            let (gx, gy) = (at(1, 0) - at(-1, 0), at(0, 1) - at(0, -1));
            let nrm = Vec3::new(-gx * 1.6, -gy * 1.6, 1.0).normalize();
            let i = (y * n + x) * 4;
            level[i..i + 4].copy_from_slice(&[((nrm.x * 0.5 + 0.5) * 255.0) as u8, ((nrm.y * 0.5 + 0.5) * 255.0) as u8, ((nrm.z * 0.5 + 0.5) * 255.0) as u8, 255]);
        }
    }
    // Box-filtered mips, so distant water doesn't sparkle.
    let mut data = level.clone();
    let (mut w, mut prev, mut count) = (n, level, 1u32);
    while w > 1 {
        let nw = w / 2;
        let mut next = vec![0u8; nw * nw * 4];
        for y in 0..nw {
            for x in 0..nw {
                for c in 0..4 {
                    let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|&(dx, dy)| prev[((2 * y + dy) * w + 2 * x + dx) * 4 + c] as u32).sum();
                    next[(y * nw + x) * 4 + c] = (s / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        (w, prev, count) = (nw, next, count + 1);
    }
    let mut img = Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        Vec::new(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = count;
    img.data = Some(data);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    img
}

fn dress_water(
    layout: Res<shared::map::MapLayout>,
    mut commands: Commands,
    q: Query<(Entity, &MapBlock), Added<MapBlock>>,
    ripples: Option<Res<Ripples>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ultra: Option<ResMut<UltraSurfaces>>,
) {
    let Some(ripples) = ripples else { return };
    for (e, MapBlock(b)) in &q {
        if b.mat != Mat::Water || !b.visible {
            continue;
        }
        let Shape::Box(s) = b.shape else { continue };
        let mut mesh = Plane3d::default().mesh().size(s.x, s.z).build();
        let _ = mesh.generate_tangents();
        let mat = materials.add(StandardMaterial {
            // Ganga green-brown: silty, not see-through far, a soft sheen rather than a mirror.
            base_color: if layout.style == shared::map::MapStyle::Ncr {Color::srgba(0.05,0.30,0.34,0.98)} else {Color::srgba(0.13, 0.17, 0.12, 0.9)},
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.14,
            reflectance: 0.35,
            normal_map_texture: Some(ripples.0.clone()),
            ..default()
        });
        if let Some(ultra) = &mut ultra {
            ultra.add(mat.clone(), Surface::Water);
        }
        let tiles = Vec2::new(s.x, s.z) / TILE;
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(mat.clone()),
            Transform::from_xyz(0.0, s.y / 2.0, 0.0),
            bevy::light::NotShadowCaster,
            Flowing(mat, tiles),
            ChildOf(e),
        ));
    }
}

/// The ripples drift downstream (+Z) and a little across.
fn flow(time: Res<Time>, q: Query<&Flowing>, mut materials: ResMut<Assets<StandardMaterial>>, ultra: Option<Res<UltraSurfaces>>) {
    let t = time.elapsed_secs();
    // Ultra's ripple map covers more water a repeat, and moves at the same speed in metres.
    let span = if ultra.is_some_and(|u| u.showing()) { TILE / WATER_TILE } else { 1.0 };
    for Flowing(h, tiles) in &q {
        if let Some(mut m) = materials.get_mut(h) {
            m.uv_transform = Affine2::from_scale_angle_translation(*tiles * span, 0.0, Vec2::new(t * 0.004, t * 0.012) * span);
        }
    }
}
