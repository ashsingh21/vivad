//! Quality levels: how much of the frame goes to resolution, shadows, ambient occlusion,
//! anti-aliasing and grass. Picked from the GPU at start (a discrete GPU gets High, an integrated
//! one such as a MacBook's gets Medium, anything else Low), forced with VIVAD_QUALITY=low|medium|high,
//! and cycled in play with F4.
//!
//! The 3D view renders into an image no taller than the level allows and is stretched over the
//! window, so a Retina screen doesn't shade four pixels for every one a 1080p screen does; the HUD
//! is drawn over it at full resolution.

use bevy::anti_alias::contrast_adaptive_sharpening::ContrastAdaptiveSharpening;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::input::{Act, ActionState};
use crate::visuals::Sun;

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    /// Most rows of pixels the 3D view is rendered at (None: the window's own).
    fn max_rows(self) -> Option<u32> {
        match self {
            Quality::Low => Some(720),
            Quality::Medium => Some(1080),
            Quality::High => None,
        }
    }

    fn ssao(self) -> bool {
        self == Quality::High
    }

    /// TAA needs the depth and motion prepass; Low does without it and uses FXAA.
    fn taa(self) -> bool {
        self != Quality::Low
    }

    /// (cascades, how far shadows reach, shadow map size).
    fn shadows(self) -> (usize, f32, usize) {
        match self {
            Quality::Low => (2, 70.0, 1024),
            Quality::Medium => (2, 120.0, 2048),
            Quality::High => (3, 200.0, 2048),
        }
    }

    /// Share of the full blade count grown (crate::grass).
    pub fn grass(self) -> f32 {
        match self {
            Quality::Low => 0.12,
            Quality::Medium => 0.3,
            Quality::High => 0.5,
        }
    }

    fn next(self) -> Self {
        match self {
            Quality::Low => Quality::Medium,
            Quality::Medium => Quality::High,
            Quality::High => Quality::Low,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Some(Quality::Low),
            "medium" | "med" => Some(Quality::Medium),
            "high" => Some(Quality::High),
            _ => None,
        }
    }
}

/// Window points per pixel of the 3D view's image, for placing HUD marks over world positions.
#[derive(Resource)]
pub struct ViewScale(pub f32);

#[derive(Resource)]
struct View3d {
    image: Handle<Image>,
    size: UVec2,
}

#[derive(Component)]
struct ViewImage;

/// Set until the level has been chosen (the GPU's description may arrive after Startup).
#[derive(Resource)]
struct Undecided;

pub struct QualityPlugin;

impl Plugin for QualityPlugin {
    fn build(&self, app: &mut App) {
        let forced = std::env::var("VIVAD_QUALITY").ok().and_then(|s| Quality::parse(&s));
        app.insert_resource(forced.unwrap_or(Quality::High)).insert_resource(ViewScale(1.0));
        if forced.is_none() {
            app.insert_resource(Undecided);
        }
        app.add_systems(Startup, spawn_view.after(crate::camera::spawn_camera))
            .add_systems(Update, (choose.run_if(resource_exists::<Undecided>), cycle, fit_view, apply.run_if(resource_changed::<Quality>)).chain());
    }
}

fn choose(mut commands: Commands, info: Option<Res<RenderAdapterInfo>>, mut q: ResMut<Quality>) {
    let level = if cfg!(target_arch = "wasm32") {
        Quality::Low
    } else {
        let Some(info) = info else { return };
        match format!("{:?}", info.device_type).as_str() {
            "DiscreteGpu" => Quality::High,
            "IntegratedGpu" => Quality::Medium,
            _ => Quality::Low,
        }
    };
    info!("quality {level:?} (VIVAD_QUALITY=low|medium|high to choose, F4 to cycle)");
    *q = level;
    commands.remove_resource::<Undecided>();
}

fn cycle(state: Res<ActionState>, mut q: ResMut<Quality>) {
    if state.just_pressed(Act::CycleQuality) {
        *q = q.next();
        info!("quality {:?}", *q);
    }
}

/// A second camera draws the 3D view's image over the whole window, with the HUD on top.
fn spawn_view(mut commands: Commands, mut images: ResMut<Assets<Image>>, cam: Query<Entity, With<MainCamera>>) {
    let image = images.add(Image::new_target_texture(16, 9, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    if let Ok(e) = cam.single() {
        commands.entity(e).insert(RenderTarget::Image(ImageRenderTarget::from(image.clone())));
    }
    commands.spawn((Camera2d, Camera { order: 1, ..default() }, IsDefaultUiCamera));
    commands.spawn((
        ViewImage,
        ImageNode::new(image.clone()),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        GlobalZIndex(i32::MIN),
    ));
    commands.insert_resource(View3d { image, size: UVec2::new(16, 9) });
}

/// Keeps the 3D view's image the window's shape, at most as many rows as the level allows.
fn fit_view(
    q: Res<Quality>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View3d>,
    mut images: ResMut<Assets<Image>>,
    mut cam: Query<&mut RenderTarget, With<MainCamera>>,
    mut node: Query<&mut ImageNode, With<ViewImage>>,
    mut scale: ResMut<ViewScale>,
) {
    let Ok(w) = window.single() else { return };
    let full = w.physical_size().max(UVec2::ONE);
    let rows = q.max_rows().map_or(full.y, |m| m.min(full.y));
    let size = UVec2::new(((full.x as u64 * rows as u64) / full.y as u64).max(1) as u32, rows);
    scale.0 = w.width() / size.x as f32;
    if size == view.size {
        return;
    }
    let image = images.add(Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    for mut t in &mut cam {
        *t = RenderTarget::Image(ImageRenderTarget::from(image.clone()));
    }
    for mut n in &mut node {
        n.image = image.clone();
    }
    images.remove(&view.image);
    *view = View3d { image, size };
}

/// Puts the level's settings on the camera, the sun and the grass. VIVAD_AA, VIVAD_NO_SSAO,
/// VIVAD_CASCADES and VIVAD_SHADOW_FAR still override them, for comparisons.
fn apply(
    mut commands: Commands,
    q: Res<Quality>,
    cam: Query<Entity, With<MainCamera>>,
    sun: Query<Entity, With<Sun>>,
    mut grass: ResMut<crate::grass::Grass>,
) {
    let env = |k: &str| std::env::var(k).ok();
    for e in &cam {
        let mut c = commands.entity(e);
        c.remove::<(ScreenSpaceAmbientOcclusion, TemporalAntiAliasing, ContrastAdaptiveSharpening, Smaa, Fxaa)>();
        if q.ssao() && env("VIVAD_NO_SSAO").is_none() {
            // Contact shadows in corners, under eaves and between blocks.
            c.insert(ScreenSpaceAmbientOcclusion { radius: 0.6, ..default() });
        }
        match env("VIVAD_AA").as_deref() {
            Some("none") => {}
            Some("smaa") => {
                c.insert(Smaa::default());
            }
            Some("fxaa") => {
                c.insert(Fxaa::default());
            }
            _ if q.taa() => {
                // Without TAA, carved detail and normal-map highlights finer than a pixel crawl and
                // sparkle as the camera moves; sharpening wins back the crispness its resolve softens.
                c.insert((TemporalAntiAliasing::default(), ContrastAdaptiveSharpening { enabled: true, sharpening_strength: 0.45, ..default() }));
            }
            _ => {
                c.insert(Fxaa::default());
            }
        }
    }
    let (cascades, far, map) = q.shadows();
    let cascades = env("VIVAD_CASCADES").and_then(|s| s.parse().ok()).unwrap_or(cascades);
    let far = env("VIVAD_SHADOW_FAR").and_then(|s| s.parse().ok()).unwrap_or(far);
    for e in &sun {
        commands.entity(e).insert(CascadeShadowConfigBuilder { num_cascades: cascades, maximum_distance: far, first_cascade_far_bound: 18.0, ..default() }.build());
    }
    commands.insert_resource(DirectionalLightShadowMap { size: map });
    grass.set_density(q.grass());
}
