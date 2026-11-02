//! The sky: a photograph on a dome, with the haze of a Delhi day baked up from its horizon and its
//! sun turned onto the game's (tools/textures/sky.py). It rides with the camera, so it is always
//! "at infinity". One draw of a few thousand triangles at any quality: nothing is simulated, which
//! suits the tile-based phone GPUs (docs/rendering-guidelines.md section 4).
//!
//! The photograph takes a moment to load, so the dome starts on one pixel of the haze's colour, in
//! the same material: bringing the image in then needs no new shader, which would leave the dome
//! undrawn for as long as that took to compile. If the image never comes (a GPU without BC formats
//! can't read it), the dome is painted per vertex instead: warm pale haze at the horizon to blue
//! overhead, a glow round the sun, a disc for the bloom to catch. VIVAD_PAINTED_SKY=1 paints it from
//! the start, for comparisons.
//!
//! Ultra is the exception: it brings the physical atmosphere back (crate::ultra), which hides the
//! dome. The atmosphere draws its own sun where the clock puts it.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::camera::MainCamera;
use crate::visuals::Sun;

pub struct SkyPlugin;

/// The dome moving to the camera: a tool that places the camera itself runs before this, or the
/// sky stays behind with the player.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkyFollow;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::ultra::UltraSky>()
            .add_systems(Startup, spawn_sky.after(crate::visuals::setup_sky))
            .add_systems(Update, show_photo.run_if(resource_exists::<Photo>))
            .add_systems(PostUpdate, follow_camera.in_set(SkyFollow).after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
    }
}

/// Inside the camera's far plane (Bevy's default 1000 m).
const RADIUS: f32 = 900.0;
/// The photograph is an 8-bit image: its values times this, in linear light, are what's drawn. Any
/// less and the tone mapper leaves the brightest cloud grey.
const PHOTO_GAIN: f32 = 1.6;
/// Sky colours (sRGB, over 1 where the sky is brighter than paper): the haze at the horizon, the
/// blue above it, and overhead. The distance fog is HORIZON too, and tools/textures/sky.py bakes it
/// into the photograph's last row: keep its number in step.
pub const HORIZON: [f32; 3] = [0.90, 0.875, 0.83];
const MID: [f32; 3] = [0.62, 0.77, 0.93];
const ZENITH: [f32; 3] = [0.34, 0.56, 0.88];
const SUN_GLOW: [f32; 3] = [1.0, 0.86, 0.62];

/// The sky rig's root, kept on the camera's position.
#[derive(Component)]
struct SkyRig;

/// The parts of the rig the physical atmosphere replaces (crate::ultra hides them).
#[derive(Component)]
#[require(Visibility)]
pub struct PaintedSky;

/// The dome.
#[derive(Component)]
struct Dome;

/// The photograph on its way, and what to fall back on. Gone once it has arrived, or failed.
#[derive(Resource)]
struct Photo {
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
    painted: Handle<Mesh>,
    rig: Entity,
    to_sun: Vec3,
}

fn lin(c: [f32; 3]) -> Vec3 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

fn spawn_sky(mut commands: Commands, sun: Query<&Transform, With<Sun>>, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Toward the sun: the light shines along its -Z.
    let to_sun = sun.single().map(|t| t.back().as_vec3()).unwrap_or(Vec3::new(0.7, 0.5, 0.3).normalize());
    let rig = commands.spawn((SkyRig, Transform::default(), Visibility::default())).id();
    let (painted, photo) = dome(to_sun);
    let painted = meshes.add(painted);
    if std::env::var("VIVAD_PAINTED_SKY").is_ok() {
        let material = materials.add(sky_material(None));
        commands.spawn((PaintedSky, Dome, Mesh3d(painted), MeshMaterial3d(material), sky_flags(), ChildOf(rig)));
        spawn_sun(&mut commands, rig, to_sun, &mut meshes, &mut materials);
        return;
    }
    let [r, g, b] = HORIZON;
    let haze = Image::new_fill(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &Srgba::new(r, g, b, 1.0).to_u8_array(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let material = materials.add(sky_material(Some(images.add(haze))));
    commands.spawn((PaintedSky, Dome, Mesh3d(meshes.add(photo)), MeshMaterial3d(material.clone()), sky_flags(), ChildOf(rig)));
    let image = assets.load_builder().with_settings(|s: &mut ImageLoaderSettings| {
        s.is_srgb = true;
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            // Round and round the sky, but never from the zenith to the horizon.
            address_mode_u: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
    });
    commands.insert_resource(Photo { image: image.load("textures/sky/sky_basecolor.ktx2"), material, painted, rig, to_sun });
}

fn sky_material(image: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial { base_color: Color::WHITE, base_color_texture: image, unlit: true, fog_enabled: false, ..default() }
}

/// The painted sky's sun: a disc bright enough to bloom.
fn spawn_sun(commands: &mut Commands, rig: Entity, to_sun: Vec3, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let disc = Circle::new(26.0).mesh().resolution(32).build();
    let glow = StandardMaterial { emissive: LinearRgba::rgb(14.0, 12.0, 9.0), ..sky_material(None) };
    commands.spawn((
        PaintedSky,
        Mesh3d(meshes.add(disc)),
        MeshMaterial3d(materials.add(glow)),
        Transform::from_translation(to_sun * RADIUS * 0.95).looking_to(-to_sun, Vec3::Y),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
        ChildOf(rig),
    ));
}

/// Puts the photograph on the dome once it's in, or paints the dome if it won't come.
fn show_photo(
    mut commands: Commands,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    photo: Res<Photo>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut dome: Query<&mut Mesh3d, With<Dome>>,
) {
    match assets.get_load_state(&photo.image) {
        Some(LoadState::Failed(_)) => {
            warn!("sky: the photograph would not load; the dome is painted instead");
            if let Some(mut m) = materials.get_mut(&photo.material) {
                m.base_color_texture = None;
            }
            for mut mesh in &mut dome {
                mesh.0 = photo.painted.clone();
            }
            spawn_sun(&mut commands, photo.rig, photo.to_sun, &mut meshes, &mut materials);
        }
        Some(LoadState::Loaded) if images.contains(&photo.image) => {
            if let Some(mut m) = materials.get_mut(&photo.material) {
                m.base_color = Color::LinearRgba(LinearRgba::rgb(PHOTO_GAIN, PHOTO_GAIN, PHOTO_GAIN));
                m.base_color_texture = Some(photo.image.clone());
            }
        }
        _ => return,
    }
    commands.remove_resource::<Photo>();
}

fn sky_flags() -> (Transform, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver) {
    (Transform::default(), bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver)
}

/// A sphere seen from inside: painted, its colour the sky's at each vertex, and for the photograph,
/// whose rows run from the zenith down to the horizon and whose middle column is toward the sun.
fn dome(to_sun: Vec3) -> (Mesh, Mesh) {
    let (rings, segs) = (48u32, 96u32);
    let (horizon, mid, zenith, glow) = (lin(HORIZON), lin(MID), lin(ZENITH), lin(SUN_GLOW));
    let sun_az = to_sun.z.atan2(to_sun.x);
    let (mut pos, mut nrm, mut col, mut uv, mut idx) = (vec![], vec![], vec![], vec![], vec![]);
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32;
        for s in 0..=segs {
            let theta = TAU * s as f32 / segs as f32;
            let d = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            let h = d.y;
            let c = if h >= 0.0 {
                // Haze sits low: most of the change in the first 15 degrees.
                let a = (h / 0.25).clamp(0.0, 1.0).powf(0.7);
                let b = ((h - 0.25) / 0.75).clamp(0.0, 1.0);
                horizon.lerp(mid, a).lerp(zenith, b)
            } else {
                // Below the horizon (seen past the map's edge): the haze, a little darker.
                horizon * (1.0 + h * 0.3)
            };
            let s = d.dot(to_sun).max(0.0);
            let c = c + glow * (0.55 * s.powf(48.0) + 0.18 * s.powf(6.0));
            pos.push((d * RADIUS).to_array());
            nrm.push((-d).to_array());
            col.push([c.x, c.y, c.z, 1.0]);
            // Under the horizon the image's last row, by the sampler's clamp: the haze.
            uv.push([0.5 + (theta - sun_az) / TAU, phi / FRAC_PI_2]);
        }
    }
    let w = segs + 1;
    for r in 0..rings {
        for s in 0..segs {
            let (a, b, c, d) = (r * w + s, r * w + s + 1, (r + 1) * w + s + 1, (r + 1) * w + s);
            // Wound to face inward.
            idx.extend_from_slice(&[a, c, b, a, d, c]);
        }
    }
    let plain = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_indices(Indices::U32(idx));
    (plain.clone().with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(col)), plain.with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv))
}

fn follow_camera(cam: Query<&Transform, (With<MainCamera>, Without<SkyRig>)>, mut rig: Query<&mut Transform, With<SkyRig>>) {
    let (Ok(c), Ok(mut r)) = (cam.single(), rig.single_mut()) else { return };
    r.translation = c.translation;
}
