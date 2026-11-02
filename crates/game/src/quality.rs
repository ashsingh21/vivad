//! Quality levels: how much of the frame goes to resolution, shadows, ambient occlusion,
//! anti-aliasing and grass. Picked from the GPU at start (a discrete GPU or Apple Silicon gets
//! Ultra, or on a big map Apple Silicon gets Medium, another integrated one Medium, anything else Low;
//! in the browser, which won't say, by the frame rate measured on High), forced with
//! VIVAD_QUALITY=low|medium|high|ultra (in the browser ?quality=, without ultra), and cycled in play
//! with F4. Ultra is the desktop level.
//!
//! The 3D view renders into an image no taller than the level allows and is stretched over the
//! window, so a Retina screen doesn't shade four pixels for every one a 1080p screen does; the HUD
//! is drawn over it at full resolution. Sustained slow frames take a sixth off those rows
//! (`RowsGovernor`).

use bevy::anti_alias::contrast_adaptive_sharpening::ContrastAdaptiveSharpening;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, ShadowFilteringMethod};
use bevy::pbr::{DistanceFog, ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel};
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
    /// The desktop level.
    Ultra,
}

impl Quality {
    /// The physical sky's level (crate::ultra).
    pub fn is_ultra(self) -> bool {
        self == Quality::Ultra
    }

    /// Whether the real surfaces show (crate::surfaces). Not in the browser, whose WebGPU allows 16
    /// textures a shader stage, far fewer than the ground's material adds to Bevy's own.
    pub fn surfaces(self) -> bool {
        self != Quality::Low && !cfg!(target_arch = "wasm32")
    }

    /// Most rows of pixels the 3D view is rendered at (None: the window's own).
    fn max_rows(self, big_map: bool) -> Option<u32> {
        match self {
            Quality::Low => Some(720),
            // 1080 rows left 7 ms a frame spare on the NCR. The rows governor's sixth off lands on
            // 1080 when a street is slow: real match, M4, 8 to 16 ms a frame, 4.36 GB.
            Quality::Medium => Some(1296),
            // A big map's every pixel has more behind it. Measured on an M4: 1800 rows take 21 ms
            // a frame there and 600 MB of render targets, and 1440 rows hold 60 fps in most views.
            Quality::High if big_map => Some(1440),
            Quality::High => None,
            // Temporal AA and sharpening recover most of the rest; shading a Retina window's every
            // pixel costs an M4 about a third of its frame rate for little that shows.
            Quality::Ultra => Some(1080),
        }
    }

    /// The ambient occlusion setting (None: off).
    fn ssao(self) -> Option<ScreenSpaceAmbientOcclusion> {
        match self {
            // Measured on an M4 on the NCR's busiest roads, Low samples at radius 0.6: 7 to 9 ms a
            // frame, most of it the normal prepass it brings, for little a player would see in
            // this light, so not on Medium.
            Quality::Low | Quality::Medium => None,
            Quality::High => Some(ScreenSpaceAmbientOcclusion { radius: 0.6, ..default() }),
            // Medium: Ultra's samples cost an M4 about 3 ms a frame for little that shows at this radius.
            Quality::Ultra => Some(ScreenSpaceAmbientOcclusion { radius: 0.9, quality_level: ScreenSpaceAmbientOcclusionQualityLevel::Medium, ..default() }),
        }
    }

    fn cas(self) -> f32 {
        if self == Quality::Ultra { 0.3 } else { 0.45 }
    }

    /// How the sun's shadow edges are filtered; Temporal hides the banding of soft edges under TAA.
    fn shadow_filter(self) -> ShadowFilteringMethod {
        if self == Quality::Ultra { ShadowFilteringMethod::Temporal } else { ShadowFilteringMethod::default() }
    }

    /// Where the first shadow cascade ends: nearer keeps more of its resolution round the player.
    fn first_cascade(self) -> f32 {
        if self == Quality::Ultra { 14.0 } else { 18.0 }
    }

    /// The camera's far plane: Ultra shows the city out to the skyline's last blocks. On a big map
    /// it goes by what the level can afford to draw, and the haze closes just short of it
    /// (crate::camera::big_map_fog), so nothing shows the line where drawing stops.
    fn far_plane(self, big_map: bool) -> f32 {
        match (self, big_map) {
            (Quality::Ultra, false) => 3000.0,
            (_, false) => 1000.0,
            (Quality::Low, true) => 1500.0,
            (Quality::Medium, true) => 2200.0,
            (Quality::High | Quality::Ultra, true) => 3000.0,
        }
    }

    /// The far plane from the air on a big map, where the haze can thin out and the city shows.
    fn far_plane_aloft(self) -> f32 {
        if self == Quality::Low { 2600.0 } else { 4600.0 }
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
            // Measured on an M4: 4 cascades at 4096 cost over 7 ms a frame against these.
            Quality::Ultra => (3, 250.0, 2048),
        }
    }

    /// Multiplies every prop's LOD switch and fade-out distance (crate::characters).
    fn lod_scale(self) -> f32 {
        match self {
            Quality::Low => 0.55,
            Quality::Medium => 0.75,
            Quality::High => 1.0,
            Quality::Ultra => 1.0,
        }
    }

    /// Whether a prop's far LODs (LOD1 and up) cast shadows.
    fn far_lod_shadows(self) -> bool {
        self != Quality::Low
    }

    /// Share of the full blade count grown (crate::grass).
    pub fn grass(self) -> f32 {
        match self {
            Quality::Low => 0.12,
            Quality::Medium => 0.3,
            Quality::High => 0.5,
            Quality::Ultra => 0.5,
        }
    }

    /// Whether characters show their densest LOD up close.
    pub fn character_lod0(self) -> bool {
        self != Quality::Low
    }

    fn next(self) -> Self {
        match self {
            Quality::Low => Quality::Medium,
            Quality::Medium => Quality::High,
            // The browser tops out at High.
            Quality::High if cfg!(target_arch = "wasm32") => Quality::Low,
            Quality::High => Quality::Ultra,
            Quality::Ultra => Quality::Low,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Some(Quality::Low),
            "medium" | "med" => Some(Quality::Medium),
            "high" => Some(Quality::High),
            "ultra" if !cfg!(target_arch = "wasm32") => Some(Quality::Ultra),
            _ => None,
        }
    }
}

/// Marks a prop LOD node whose shadow casting follows the level (`Quality::far_lod_shadows`).
#[derive(Component)]
pub struct LodShadow;

/// The quality level's LOD distance multiplier, for props spawned after it is chosen.
#[derive(Resource)]
pub struct LodScale(pub f32);

/// The safety net under a slow street: from Medium up, a sustained slow frame takes the 3D view down
/// a sixth of its rows, which sheds a third of its pixels, and steady frames bring them back. Only
/// in real play: VIVAD_ROWS, VIVAD_TOUR and VIVAD_NO_VSYNC (which every measurement uses) leave it
/// off, so that numbers mean what they say.
#[derive(Resource)]
struct RowsGovernor {
    /// Whether the rows are cut.
    cut: bool,
    /// Seconds of slow frames, or of steady ones.
    run: f32,
    /// Seconds of steady frames it takes to try the full rows again: doubles on each failure.
    patience: f32,
    /// Seconds since the rows came back.
    since_back: f32,
    /// The running mean of the frame time, seconds.
    mean: f32,
    on: bool,
}

impl Default for RowsGovernor {
    fn default() -> Self {
        let off = ["VIVAD_ROWS", "VIVAD_TOUR", "VIVAD_NO_VSYNC"].iter().any(|k| std::env::var(k).is_ok());
        Self { cut: false, run: 0.0, patience: RowsGovernor::PATIENCE, since_back: 1e6, mean: 1.0 / 60.0, on: !off }
    }
}

impl RowsGovernor {
    /// Frame times that count as slow and as steady (a 60 Hz display's, a touch under), seconds.
    const SLOW: f32 = 1.0 / 48.0;
    const STEADY: f32 = 1.0 / 57.0;
    /// Seconds of slow frames that cut the rows.
    const CUT_AFTER: f32 = 3.0;
    /// Seconds of steady frames before trying the full rows, at first and at the most.
    const PATIENCE: f32 = 20.0;
    const MOST: f32 = 160.0;
    /// Coming back and going slow again within this many seconds counts as a failure.
    const TRIAL: f32 = 12.0;

    /// One frame of `dt` seconds. True when the rows change.
    fn step(&mut self, dt: f32) -> bool {
        // A hitch is something loading, not the level's cost.
        if dt > 0.1 {
            return false;
        }
        self.mean += (dt - self.mean) * (dt * 2.0).min(1.0);
        self.since_back += dt;
        if self.cut {
            self.run = if self.mean < Self::STEADY { self.run + dt } else { 0.0 };
            if self.run >= self.patience {
                (self.cut, self.run, self.since_back) = (false, 0.0, 0.0);
                return true;
            }
        } else {
            self.run = if self.mean > Self::SLOW { self.run + dt } else { 0.0 };
            if self.run >= Self::CUT_AFTER {
                self.patience = if self.since_back < Self::TRIAL { (self.patience * 2.0).min(Self::MOST) } else { Self::PATIENCE };
                (self.cut, self.run) = (true, 0.0);
                return true;
            }
        }
        false
    }
}

/// Window points per pixel of the 3D view's image, for placing HUD marks over world positions.
#[derive(Resource)]
pub struct ViewScale(pub f32);

/// The image the main camera draws the 3D view into; the window shows it under the HUD.
#[derive(Resource)]
pub struct View3d {
    pub image: Handle<Image>,
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
        // VIVAD_QUALITY natively; in the browser ?quality= in the page's address.
        #[cfg(not(target_arch = "wasm32"))]
        let forced = std::env::var("VIVAD_QUALITY").ok().and_then(|s| Quality::parse(&s));
        #[cfg(target_arch = "wasm32")]
        let forced = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok()?.get("quality"))
            .and_then(|s| Quality::parse(&s));
        let first = forced.unwrap_or(Quality::High);
        app.insert_resource(first).insert_resource(ViewScale(1.0)).insert_resource(LodScale(first.lod_scale())).init_resource::<RowsGovernor>();
        if forced.is_none() {
            app.insert_resource(Undecided);
        }
        app.add_systems(Startup, spawn_view.after(crate::camera::spawn_camera))
            .add_systems(Update, (choose.run_if(resource_exists::<Undecided>), settle.run_if(resource_exists::<Settle>), cycle, fit_view, apply.run_if(|q: Res<Quality>, swaps: Res<shared::sim::MapSwaps>| q.is_changed() || shared::sim::map_reloaded(swaps)), far_shadows, haze_by_height).chain());
        if std::env::var("VIVAD_TRIS").is_ok() {
            app.add_systems(Last, (tri_report, entity_report));
        }
    }
}

/// Whether the levels are those of a big map.
fn is_big(layout: &shared::map::MapLayout) -> bool {
    layout.terrain.n >= crate::visuals::BIG_TERRAIN
}

fn choose(mut commands: Commands, info: Option<Res<RenderAdapterInfo>>, layout: Res<shared::map::MapLayout>, mut q: ResMut<Quality>) {
    // A browser doesn't say what GPU it has: start high and let `settle` step down if it's slow.
    let level = if cfg!(target_arch = "wasm32") {
        commands.insert_resource(Settle::default());
        Quality::High
    } else {
        let Some(info) = info else { return };
        // Apple Silicon reports itself as integrated, but with the render height capped it
        // carries the top level. Not on a big map: with 64 warriors an M4 at High takes about 20 ms
        // a frame and 4.7 GB on the NCR, where Medium takes 8 to 15 ms and 4.3 GB.
        match (format!("{:?}", info.device_type).as_str(), format!("{:?}", info.backend).as_str()) {
            ("DiscreteGpu", _) => Quality::Ultra,
            ("IntegratedGpu", "Metal") if is_big(&layout) => Quality::Medium,
            ("IntegratedGpu", "Metal") => Quality::Ultra,
            ("IntegratedGpu", _) => Quality::Medium,
            _ => Quality::Low,
        }
    };
    info!("quality {level:?} (VIVAD_QUALITY=low|medium|high|ultra to choose, F4 to cycle)");
    *q = level;
    commands.remove_resource::<Undecided>();
}

/// The browser's level, found by measuring: frames are timed for a few seconds once the game
/// has settled (shaders compiled, map built), and the level steps down while they average under
/// `MIN_FPS`.
#[derive(Resource, Default)]
struct Settle {
    /// Seconds into the current measurement (negative: still waiting for things to settle).
    t: f32,
    frames: u32,
}

const MIN_FPS: f32 = 45.0;
/// Seconds to wait after a change before measuring, then to measure for.
const SETTLE_WAIT: f32 = 6.0;
const SETTLE_MEASURE: f32 = 4.0;

fn settle(mut commands: Commands, time: Res<Time<Real>>, mut s: ResMut<Settle>, mut q: ResMut<Quality>, state: Res<ActionState>) {
    // Choosing by hand ends it.
    if state.just_pressed(Act::CycleQuality) {
        commands.remove_resource::<Settle>();
        return;
    }
    // A long hitch is something loading, not the level's cost: measure again from scratch.
    if time.delta_secs() > 0.25 && s.t > SETTLE_WAIT {
        *s = Settle { t: SETTLE_WAIT - 1.0, frames: 0 };
    }
    s.t += time.delta_secs();
    if s.t < SETTLE_WAIT {
        return;
    }
    s.frames += 1;
    let measured = s.t - SETTLE_WAIT;
    if measured < SETTLE_MEASURE {
        return;
    }
    let fps = s.frames as f32 / measured;
    if fps < MIN_FPS && *q != Quality::Low {
        *q = match *q {
            Quality::Ultra | Quality::High => Quality::Medium,
            _ => Quality::Low,
        };
        info!("{fps:.0} fps: quality down to {:?}", *q);
        *s = Settle::default();
    } else {
        info!("{fps:.0} fps: quality stays {:?}", *q);
        commands.remove_resource::<Settle>();
    }
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
        // The main camera is no longer the one drawing to the window, which is where Bevy measures
        // lamp shadows' LOD distances from unless told.
        commands.entity(e).insert((RenderTarget::Image(ImageRenderTarget::from(image.clone())), bevy::camera::ShadowLodOrigin));
    }
    // Bevy treats every camera as a view of the 3D world: it culls for it, measures LOD distances
    // from it and fits sun-shadow cascades round it. On a render layer nothing is on, and far
    // below the map, this one sees none of it (the UI it draws doesn't care where it stands).
    commands.spawn((
        Camera2d,
        Camera { order: 1, ..default() },
        IsDefaultUiCamera,
        bevy::camera::visibility::RenderLayers::layer(31),
        Transform::from_xyz(0.0, -100_000.0, 0.0),
    ));
    commands.spawn((
        ViewImage,
        ImageNode::new(image.clone()),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        GlobalZIndex(i32::MIN),
    ));
    commands.insert_resource(View3d { image, size: UVec2::new(16, 9) });
}

/// Keeps the 3D view's image the window's shape, at most as many rows as the level allows.
#[allow(clippy::too_many_arguments)]
fn fit_view(
    q: Res<Quality>,
    time: Res<Time<Real>>,
    mut governor: ResMut<RowsGovernor>,
    layout: Res<shared::map::MapLayout>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View3d>,
    mut images: ResMut<Assets<Image>>,
    mut cam: Query<&mut RenderTarget, With<MainCamera>>,
    mut node: Query<&mut ImageNode, With<ViewImage>>,
    mut scale: ResMut<ViewScale>,
) {
    let Ok(w) = window.single() else { return };
    let full = w.physical_size().max(UVec2::ONE);
    if governor.on && *q != Quality::Low && governor.step(time.delta_secs()) {
        info!("3D view at {} of its rows", if governor.cut { "five sixths" } else { "all" });
    }
    let level_rows = q.max_rows(is_big(&layout)).map(|m| if governor.cut { m / 6 * 5 } else { m });
    let rows = std::env::var("VIVAD_ROWS").ok().and_then(|s| s.parse().ok()).or(level_rows).map_or(full.y, |m: u32| m.min(full.y));
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
/// VIVAD_CASCADES, VIVAD_SHADOW_FAR, VIVAD_SHADOW_MAP, VIVAD_GRASS, VIVAD_LOD (and VIVAD_ROWS, in
/// `fit_view`) still override them, for comparisons.
#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    q: Res<Quality>,
    layout: Res<shared::map::MapLayout>,
    mut cam: Query<(Entity, &mut Projection), With<MainCamera>>,
    sun: Query<Entity, With<Sun>>,
    mut grass: ResMut<crate::grass::Grass>,
    mut lods: Query<(Entity, &crate::characters::LodRange, &mut bevy::camera::visibility::VisibilityRange, Has<LodShadow>)>,
    painted: Query<&ChildOf, With<crate::sky::PaintedSky>>,
    mut transforms: Query<&mut Transform>,
) {
    let env = |k: &str| std::env::var(k).ok();
    let big = is_big(&layout);
    let far_plane = q.far_plane(big);
    // The dome is a mesh, drawn in front of anything past its radius (a tenth inside Bevy's default
    // 1000 m far plane), so it has to stand back with the far plane.
    if let Some(rig) = painted.iter().next().map(|c| c.parent())
        && let Ok(mut t) = transforms.get_mut(rig)
    {
        t.scale = Vec3::splat(if big { far_plane / 1000.0 } else { 1.0 });
    }
    for (e, mut projection) in &mut cam {
        if let Projection::Perspective(p) = projection.as_mut() {
            p.far = far_plane;
        }
        let mut c = commands.entity(e);
        c.remove::<(ScreenSpaceAmbientOcclusion, TemporalAntiAliasing, ContrastAdaptiveSharpening, Smaa, Fxaa)>();
        c.insert(q.shadow_filter());
        // Contact shadows in corners, under eaves and between blocks.
        if let Some(mut ssao) = q.ssao().filter(|_| env("VIVAD_NO_SSAO").is_none()) {
            // VIVAD_SSAO=low|medium|high|ultra picks the sample count, for comparisons.
            match env("VIVAD_SSAO").as_deref() {
                Some("low") => ssao.quality_level = ScreenSpaceAmbientOcclusionQualityLevel::Low,
                Some("medium") => ssao.quality_level = ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                Some("high") => ssao.quality_level = ScreenSpaceAmbientOcclusionQualityLevel::High,
                Some("ultra") => ssao.quality_level = ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
                _ => {}
            }
            c.insert(ssao);
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
                c.insert((TemporalAntiAliasing::default(), ContrastAdaptiveSharpening { enabled: true, sharpening_strength: q.cas(), ..default() }));
            }
            _ => {
                // Sharpening matters most here: the low level's view is the most stretched.
                c.insert((Fxaa::default(), ContrastAdaptiveSharpening { enabled: true, sharpening_strength: 0.6, ..default() }));
            }
        }
    }
    let (cascades, far, map) = q.shadows();
    let cascades = env("VIVAD_CASCADES").and_then(|s| s.parse().ok()).unwrap_or(cascades);
    let far = env("VIVAD_SHADOW_FAR").and_then(|s| s.parse().ok()).unwrap_or(far);
    for e in &sun {
        commands.entity(e).insert(CascadeShadowConfigBuilder { num_cascades: cascades, maximum_distance: far, first_cascade_far_bound: q.first_cascade(), ..default() }.build());
    }
    let map = env("VIVAD_SHADOW_MAP").and_then(|s| s.parse().ok()).unwrap_or(map);
    commands.insert_resource(DirectionalLightShadowMap { size: map });
    grass.set_density(env("VIVAD_GRASS").and_then(|s| s.parse().ok()).unwrap_or(q.grass()));
    let k = env("VIVAD_LOD").and_then(|s| s.parse().ok()).unwrap_or(q.lod_scale());
    commands.insert_resource(LodScale(k));
    for (e, base, mut range, shadow) in &mut lods {
        *range = base.scaled(k);
        if shadow && base.1 > 0 {
            if q.far_lod_shadows() {
                commands.entity(e).remove::<bevy::light::NotShadowCaster>();
            } else {
                commands.entity(e).insert(bevy::light::NotShadowCaster);
            }
        }
    }
    crate::ultra::apply(&mut commands, q.is_ultra());
    // After that, which takes the fog off the camera when the atmosphere comes on.
    if big {
        for (e, _) in &cam {
            commands.entity(e).insert(crate::camera::big_map_fog(far_plane));
        }
    }
}

/// Haze lies low over the city: on a big map the view opens up as the camera climbs out of it, and
/// the far plane, the dome and the fog's reach with it. Street level is untouched.
fn haze_by_height(q: Res<Quality>, layout: Res<shared::map::MapLayout>, mut cam: Query<(&GlobalTransform, &mut Projection, Option<&mut DistanceFog>), With<MainCamera>>, painted: Query<&ChildOf, With<crate::sky::PaintedSky>>, mut transforms: Query<&mut Transform>) {
    /// Metres up that the haze starts to thin, and is as thin as it gets.
    const THINS: (f32, f32) = (60.0, 140.0);
    if !is_big(&layout) {
        return;
    }
    let Ok((at, mut projection, fog)) = cam.single_mut() else { return };
    let Projection::Perspective(p) = projection.as_mut() else { return };
    let t = ((at.translation().y - THINS.0) / (THINS.1 - THINS.0)).clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    let far = q.far_plane(true).lerp(q.far_plane_aloft(), s);
    if (far - p.far).abs() < 0.5 {
        return;
    }
    p.far = far;
    if let Some(mut fog) = fog {
        // From the drop the city should read to the horizon: the haze is what lies under you, not
        // a wall a kilometre off. It still closes just past the far plane, so no line shows.
        *fog = crate::camera::big_map_fog(far);
        fog.falloff = bevy::pbr::FogFalloff::from_visibility_squared(far * (0.87 + 0.26 * s));
    }
    if let Some(rig) = painted.iter().next().map(|c| c.parent())
        && let Ok(mut t) = transforms.get_mut(rig)
    {
        t.scale = Vec3::splat(far / 1000.0);
    }
}

/// VIVAD_TRIS=1: every 5 s, log the triangles drawn in view and the models they come from.
pub fn tri_report(
    time: Res<Time>,
    mut next: Local<f32>,
    meshes: Res<Assets<Mesh>>,
    assets: Res<AssetServer>,
    q: Query<(&Mesh3d, &ViewVisibility, Has<bevy::light::NotShadowCaster>, Option<&ChildOf>)>,
    names: Query<(Option<&Name>, Has<shared::sim::MapBlock>, Has<shared::sim::Warrior>)>,
) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 5.0;
    let mut by: std::collections::HashMap<String, (usize, usize, usize)> = default();
    let (mut total, mut casters) = (0, 0);
    for (m, vis, no_shadow, parent) in &q {
        let Some(mesh) = meshes.get(&m.0) else { continue };
        let tris = mesh.indices().map_or(mesh.count_vertices(), |i| i.len()) / 3;
        if !no_shadow {
            casters += tris;
        }
        if !vis.get() {
            continue;
        }
        total += tris;
        let name = assets.get_path(m.0.id()).map(|p| p.path().display().to_string()).unwrap_or_else(|| {
            let what = match parent.and_then(|p| names.get(p.parent()).ok()) {
                Some((_, true, _)) => "map block".to_string(),
                Some((_, _, true)) => "warrior".to_string(),
                Some((Some(n), ..)) => n.as_str().to_string(),
                _ => format!("{tris}-tri mesh"),
            };
            format!("(code) {what}")
        });
        let e = by.entry(name).or_default();
        e.0 += tris;
        e.1 += 1;
        e.2 += if no_shadow { 0 } else { tris };
    }
    let mut v: Vec<_> = by.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1 .0));
    info!("[tris] {total} in view, {casters} in shadow casters anywhere");
    for (name, (t, n, c)) in v.into_iter().take(25) {
        info!("[tris]   {t:>8} in {n:>5} meshes ({c:>8} casting)  {name}");
    }
}

/// Props that spawn after the level was applied: on Low their far LODs cast no shadows.
fn far_shadows(mut commands: Commands, q: Res<Quality>, added: Query<(Entity, &crate::characters::LodRange), Added<LodShadow>>) {
    if q.far_lod_shadows() {
        return;
    }
    for (e, base) in &added {
        if base.1 > 0 {
            commands.entity(e).insert(bevy::light::NotShadowCaster);
        }
    }
}

/// VIVAD_TRIS=1 also: how many entities of the kinds that cost CPU every frame.
#[allow(clippy::too_many_arguments)]
pub fn entity_report(
    time: Res<Time>,
    mut next: Local<f32>,
    all: Query<()>,
    meshes: Query<(), With<Mesh3d>>,
    pickups: Query<(), With<shared::sim::loot::Pickup>>,
    chests: Query<(), With<shared::sim::loot::Chest>>,
    targets: Query<(), With<bevy::animation::AnimationTargetId>>,
    blocks: Query<(), With<shared::sim::MapBlock>>,
    changed: Query<(), Changed<Transform>>,
    colliders: Query<(), With<avian3d::prelude::Collider>>,
) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 5.0;
    info!(
        "[ents] {} entities: {} meshes, {} pickups, {} chests, {} animated bones, {} map blocks, {} colliders; {} transforms changed this frame",
        all.iter().count(), meshes.iter().count(), pickups.iter().count(), chests.iter().count(), targets.iter().count(),
        blocks.iter().count(), colliders.iter().count(), changed.iter().count()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(g: &mut RowsGovernor, fps: f32, seconds: f32) -> u32 {
        let dt = 1.0 / fps;
        (0..(seconds * fps) as u32).filter(|_| g.step(dt)).count() as u32
    }

    fn governor() -> RowsGovernor {
        RowsGovernor { on: true, ..default() }
    }

    #[test]
    fn slow_frames_cut_the_rows_and_steady_ones_bring_them_back() {
        let mut g = governor();
        run(&mut g, 60.0, 30.0);
        assert!(!g.cut, "steady frames leave the rows alone");
        run(&mut g, 40.0, 6.0);
        assert!(g.cut, "sustained slow frames cut them");
        run(&mut g, 60.0, RowsGovernor::PATIENCE + 2.0);
        assert!(!g.cut, "steady frames bring them back");
    }

    #[test]
    fn a_short_slow_patch_changes_nothing() {
        let mut g = governor();
        run(&mut g, 60.0, 5.0);
        run(&mut g, 40.0, 1.0);
        run(&mut g, 60.0, 5.0);
        assert!(!g.cut);
    }

    #[test]
    fn a_street_that_stays_slow_doesnt_flip_back_and_forth() {
        let mut g = governor();
        run(&mut g, 40.0, 6.0);
        let first = g.patience;
        // Fine at the cut rows, slow at the full ones: each trial waits longer than the last.
        for _ in 0..3 {
            let wait = g.patience + 2.0;
            run(&mut g, 60.0, wait);
            assert!(!g.cut);
            run(&mut g, 40.0, 6.0);
            assert!(g.cut);
        }
        assert!(g.patience > first * 4.0, "patience {}", g.patience);
    }

    #[test]
    fn a_hitch_is_not_a_slow_frame() {
        let mut g = governor();
        for _ in 0..200 {
            g.step(0.5);
        }
        assert!(!g.cut);
    }
}
