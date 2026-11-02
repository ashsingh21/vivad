//! The Ultra level's sky and finish: Bevy's physical atmosphere with a sky-generated environment
//! light, the sun on the clock and an exposure set by the sun's height. Every other level keeps the
//! dome of crate::sky, the fixed sun and fixed exposure, and F4 moves cleanly between the two
//! (`apply`, from crate::quality).
//!
//! Switches, for comparisons:
//!   VIVAD_NO_ATMO=1      Ultra on the painted sky and fixed sun, with distance fog.
//!   VIVAD_AUTOEXP=1      adds metering on top, for comparisons. Off by default: it costs about 3 ms a
//!                        frame on an M4, and once on, Bevy 0.20 keeps metering after the component
//!                        is removed, so leaving Ultra with F4 would dim the lower levels.
//!   VIVAD_EV=<ev100>     one fixed exposure, no metering.
//!   VIVAD_TONEMAP=agx    AgX, to compare with TonyMcMapface.
//!   VIVAD_HOUR=<h>       pins the clock (`18.5` or `18:30`).
//!   VIVAD_ATMO_FULL=1    Bevy's full-size lookup tables instead of the lean set.
//!   VIVAD_ATMO_ENV=<n>   environment map size in pixels, a power of two; 0 for none.
//!   VIVAD_ATMO_DENSITY=<k>  haze: multiplies the air's density (default 1.8).
//!
//! How Bevy's parts behave, read from its 0.20 sources; each shapes something here:
//! - With an atmosphere, the sun's direct light is multiplied by the air's transmittance towards
//!   it and by how much of it clears the horizon, and N·L does the rest. So `illuminance` is the
//!   light above the air (`RAW_SUNLIGHT`) and stays constant through the day; scaling it by
//!   elevation or tinting it for sunset would count both twice.
//! - `AutoExposure::range` is in log2 of the pre-exposed main image, not in EV100, and metering
//!   aims the scene's mean at log2 0 (a linear 1.0). So the camera's `Exposure` still sets the
//!   working level, the range sits round 0, and a compensation curve sets the real target.
//! - The sun disc's radiance is `illuminance / solid angle`: at full strength it overflows the
//!   Rgba16Float main image (past 65504) once exposed, and an infinity feeds TAA and bloom.
//! - `GeneratedEnvironmentMapLight` only builds its maps on a camera without an
//!   `EnvironmentMapLight`, and removing `AtmosphereEnvironmentMapLight` strips any
//!   `EnvironmentMapLight` the camera has: NCR's hemisphere light has to come off first and be put
//!   back after (`CommandQueue` flushes the removal's observer before the next command).
//! - The atmosphere pass reads the depth texture. `Camera3d` asks only for RENDER_ATTACHMENT, but
//!   Bevy adds TEXTURE_BINDING in the render world every frame (`Camera3d` is re-extracted each
//!   frame, so its `Changed` filter passes), which covers switching on in play.
//! - The main pass is Rgba16Float whatever the camera targets (`Bloom` and `AtmosphereSettings`
//!   require `Hdr`), and the atmosphere draws into that, so the Rgba8 view image in crate::quality
//!   only ever receives the tonemapped result. Metering reads the same HDR image.
//! - All four lookup tables are rebuilt every frame and the environment map is refiltered every
//!   frame, however still the sun: the table sizes below and the 64 px map are the cost controls
//!   Bevy offers. `AtmosphereMode::LookupTexture` is the cheap mode.

use std::f32::consts::PI;

use bevy::camera::Exposure;
use bevy::curve::cubic_splines::LinearSpline;
use bevy::light::atmosphere::ScatteringMedium;
use bevy::light::light_consts::lux;
use bevy::light::{Atmosphere, AtmosphereEnvironmentMapLight, SunDisk};
use bevy::pbr::{AtmosphereMode, AtmosphereSettings, DistanceFog};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve};
use bevy::prelude::*;
use shared::map::MapLayout;

use crate::camera::MainCamera;
use crate::sky::PaintedSky;
use crate::visuals::Sun;

/// Delhi's latitude, which tilts the sun's path to the south.
const LATITUDE: f32 = 28.6 * PI / 180.0;
/// Haze over the plain: a Delhi sky, not a clean Earth one.
const AIR_DENSITY: f32 = 1.8;
/// As far as the camera sees (crate::quality's far plane).
const AERIAL_RANGE: f32 = 3000.0;

/// Sun elevations (degrees) between which the sun's light fades out through twilight; below the
/// lower one the same light comes back as the moon.
const SUN_FADE: (f32, f32) = (-14.0, -6.0);
/// Moon elevation is the mirror of the sun's: full moonlight once the sun is this far down.
const MOON_FULL: f32 = -20.0;
/// Far above the real 0.3 lux: the map has to stay playable at night.
const MOONLIGHT: f32 = 60.0;
/// A sliver of the physical disc: enough to catch the bloom, nowhere near overflowing.
const SUN_DISC_INTENSITY: f32 = 0.1;

/// Ambient (cd/m²) on top of the sky's environment light, which has nothing coming from below.
const AMBIENT_DAY: f32 = 4000.0;
const AMBIENT_NIGHT: f32 = 10.0;

/// The camera's EV100 by sun elevation in degrees: the working level, right on its own, that
/// metering then trims.
const EV_BY_SUN: [(f32, f32); 6] = [(-18.0, 5.5), (-12.0, 6.5), (-6.0, 8.5), (0.0, 11.5), (10.0, 13.5), (25.0, 14.5)];

/// Where metering puts the scene's mean, in log2 of the pre-exposed image; 0 would be far too
/// bright. The most it may move the exposure from the working level, in stops, each way.
const METER_TARGET: f32 = -2.0;
const METER_BRIGHTEN: f32 = 2.5;
const METER_DARKEN: f32 = 3.5;
/// What metering looks at. Wide: pixels under it drop out of the mean; the curve does the limiting.
const METER_RANGE: (f32, f32) = (-8.0, 4.0);

/// What's up on the current level.
#[derive(Resource, Default)]
pub struct UltraSky {
    atmosphere: bool,
    look: bool,
    medium: Option<Handle<ScatteringMedium>>,
    curve: Option<Handle<AutoExposureCompensationCurve>>,
    saved: Option<Painted>,
}

impl UltraSky {
    /// Whether the physical sky is what's showing: `visuals::update_sun` runs it if so.
    pub fn atmosphere(&self) -> bool {
        self.atmosphere
    }
}

/// How the fixed sky stood when the physical one took over, to put back.
struct Painted {
    sun: Transform,
    illuminance: f32,
    color: Color,
    ambient: f32,
    hemisphere: Option<EnvironmentMapLight>,
}

#[derive(Component)]
struct UltraOnly;

/// Moves everything to the level just chosen; the commands run straight after `quality::apply`.
pub fn apply(commands: &mut Commands, ultra: bool) {
    commands.run_system_cached_with(sync, ultra);
}

#[allow(clippy::too_many_arguments)]
fn sync(
    In(ultra): In<bool>,
    mut commands: Commands,
    mut state: ResMut<UltraSky>,
    mut media: ResMut<Assets<ScatteringMedium>>,
    curves: Option<ResMut<Assets<AutoExposureCompensationCurve>>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    layout: Res<MapLayout>,
    mut cam: Query<(Entity, &mut Exposure, Option<&EnvironmentMapLight>), With<MainCamera>>,
    mut sun: Query<(Entity, &mut Transform, &mut DirectionalLight), With<Sun>>,
    mut painted: Query<&mut Visibility, With<PaintedSky>>,
    atmospheres: Query<Entity, With<UltraOnly>>,
) {
    let Ok((camera, mut exposure, hemisphere)) = cam.single_mut() else { return };
    let ncr = layout.style == shared::map::MapStyle::Ncr;

    let want_atmosphere = ultra && std::env::var("VIVAD_NO_ATMO").is_err();
    if want_atmosphere && !state.atmosphere {
        if let Ok((e, tf, light)) = sun.single() {
            state.saved = Some(Painted {
                sun: *tf,
                illuminance: light.illuminance,
                color: light.color,
                ambient: ambient.brightness,
                hemisphere: hemisphere.cloned(),
            });
            commands.entity(e).insert(SunDisk { intensity: SUN_DISC_INTENSITY, ..SunDisk::EARTH });
        }
        // Sky pixels the atmosphere draws over start from the clear colour; below Ultra the dome
        // covers every one, so black can stay.
        commands.insert_resource(ClearColor(Color::BLACK));
        let medium = state.medium.get_or_insert_with(|| media.add(ScatteringMedium::earth(256, 256).with_density_multiplier(env_f32("VIVAD_ATMO_DENSITY").unwrap_or(AIR_DENSITY)))).clone();
        commands.spawn((Atmosphere::earth(medium), UltraOnly));
        // The aerial perspective replaces the distance fog; the environment light, the hemisphere.
        let mut c = commands.entity(camera);
        c.remove::<(DistanceFog, EnvironmentMapLight)>();
        c.insert(lean_tables());
        if let Some(size) = env_map_size() {
            c.insert(AtmosphereEnvironmentMapLight { size: UVec2::splat(size), ..default() });
        }
        for mut v in &mut painted {
            *v = Visibility::Hidden;
        }
        state.atmosphere = true;
    } else if !want_atmosphere && state.atmosphere {
        for e in &atmospheres {
            commands.entity(e).despawn();
        }
        // These first: removing the environment light also strips any EnvironmentMapLight, so the
        // hemisphere goes back on after it.
        commands.entity(camera).remove::<(AtmosphereSettings, AtmosphereEnvironmentMapLight)>();
        commands.entity(camera).insert(crate::camera::distance_fog(ncr));
        let saved = state.saved.take();
        if let Some(saved) = saved {
            if let Ok((e, mut tf, mut light)) = sun.single_mut() {
                *tf = saved.sun;
                light.illuminance = saved.illuminance;
                light.color = saved.color;
                commands.entity(e).remove::<SunDisk>();
            }
            ambient.brightness = saved.ambient;
            if let Some(h) = saved.hemisphere {
                commands.entity(camera).insert(h);
            }
        }
        exposure.ev100 = crate::camera::fixed_ev();
        for mut v in &mut painted {
            *v = Visibility::Inherited;
        }
        state.atmosphere = false;
    }

    if ultra != state.look {
        commands.entity(camera).insert(crate::camera::look(ncr, ultra));
        state.look = ultra;
    }
    // VIVAD_EV means one exposure; the plugin itself is only added with VIVAD_AUTOEXP (main.rs).
    let metered = ultra && std::env::var("VIVAD_AUTOEXP").is_ok() && crate::camera::ev_override().is_none();
    match curves {
        Some(mut curves) if metered => {
            let compensation_curve = state.curve.get_or_insert_with(|| curves.add(meter_curve())).clone();
            let (lo, hi) = METER_RANGE;
            commands.entity(camera).insert(AutoExposure { range: lo..=hi, filter: 0.10..=0.90, speed_brighten: 2.0, speed_darken: 1.0, compensation_curve, ..default() });
        }
        _ => {
            commands.entity(camera).remove::<AutoExposure>();
        }
    }
}

/// The lookup tables at the sizes the Medium level used when this sky was on every level (they
/// looked right there), with thinner aerial and multiple-scattering tables on top: both hold
/// slow gradients. The cost is in those two and the sky view, so they are what shrank.
fn lean_tables() -> AtmosphereSettings {
    let full = AtmosphereSettings { aerial_view_lut_max_distance: AERIAL_RANGE, rendering_method: AtmosphereMode::LookupTexture, ..default() };
    if std::env::var("VIVAD_ATMO_FULL").is_ok() {
        return full;
    }
    AtmosphereSettings {
        transmittance_lut_size: UVec2::new(128, 64),
        transmittance_lut_samples: 20,
        multiscattering_lut_size: UVec2::splat(24),
        multiscattering_lut_samples: 10,
        sky_view_lut_size: UVec2::new(200, 100),
        sky_view_lut_samples: 8,
        aerial_view_lut_size: UVec3::new(24, 24, 16),
        aerial_view_lut_samples: 5,
        ..full
    }
}

/// None: no environment light at all.
fn env_map_size() -> Option<u32> {
    let size = std::env::var("VIVAD_ATMO_ENV").ok().and_then(|s| s.parse().ok()).unwrap_or(64u32);
    (size > 0).then(|| size.next_power_of_two().max(8))
}

fn env_f32(key: &str) -> Option<f32> {
    std::env::var(key).ok()?.parse().ok()
}

/// Exposure compensation by the scene's mean: flat at the target, then following the mean at
/// either end so that the correction it asks for (compensation minus mean) stops at the limits.
fn meter_curve() -> AutoExposureCompensationCurve {
    let (lo, hi) = METER_RANGE;
    let knee_dark = METER_TARGET - METER_BRIGHTEN;
    let knee_bright = METER_TARGET + METER_DARKEN;
    AutoExposureCompensationCurve::from_curve(LinearSpline::new([
        vec2(lo, lo + METER_BRIGHTEN),
        vec2(knee_dark, METER_TARGET),
        vec2(knee_bright, METER_TARGET),
        vec2(hi, hi - METER_DARKEN),
    ]))
    .expect("the points run left to right")
}

/// The sky at an hour of the day.
#[derive(Clone, Copy, Debug)]
pub struct SkyNow {
    /// Toward the light: the sun, or the moon once the sun has gone.
    pub to_light: Vec3,
    pub illuminance: f32,
    pub color: Color,
    /// `GlobalAmbientLight::brightness`.
    pub ambient: f32,
    pub ev100: f32,
}

/// Toward the sun at an hour: rises in the east (+X) at 6, sets in the west at 18, south (+Z) of
/// overhead at noon.
fn sun_dir(hour: f32) -> Vec3 {
    let a = (hour - 6.0) * PI / 12.0;
    Vec3::new(a.cos(), a.sin() * LATITUDE.cos(), a.sin() * LATITUDE.sin())
}

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One light in the sky, so it fades to nothing before turning into the other: the direction's
/// jump never shows.
pub fn sky_at(hour: f32) -> SkyNow {
    let sun = sun_dir(hour);
    let elevation = sun.y.asin().to_degrees();
    let (gone, fading) = SUN_FADE;
    let (to_light, illuminance, color) = if elevation > gone {
        (sun, lux::RAW_SUNLIGHT * smooth(gone, fading, elevation), Color::WHITE)
    } else {
        (-sun, MOONLIGHT * (1.0 - smooth(MOON_FULL, gone, elevation)), Color::srgb(0.62, 0.72, 1.0))
    };
    let ambient = AMBIENT_NIGHT + (AMBIENT_DAY - AMBIENT_NIGHT) * smooth(-8.0, 15.0, elevation);
    SkyNow { to_light, illuminance, color, ambient, ev100: ev_at(elevation) }
}

fn ev_at(elevation: f32) -> f32 {
    let first = EV_BY_SUN[0];
    if elevation <= first.0 {
        return first.1;
    }
    for pair in EV_BY_SUN.windows(2) {
        let ((e0, ev0), (e1, ev1)) = (pair[0], pair[1]);
        if elevation <= e1 {
            return ev0 + (ev1 - ev0) * (elevation - e0) / (e1 - e0);
        }
    }
    EV_BY_SUN[EV_BY_SUN.len() - 1].1
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::core_pipeline::tonemapping::Tonemapping;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::light::GeneratedEnvironmentMapLight;
    use bevy::post_process::auto_exposure::AutoExposurePlugin;
    use bevy::render::RenderPlugin;
    use bevy::render::settings::WgpuSettings;
    use bevy::window::ExitCondition;

    #[test]
    fn the_day_reads_as_a_day() {
        let (morning, noon, dusk, night) = (sky_at(8.0), sky_at(13.0), sky_at(18.5), sky_at(22.0));
        for up in [morning, noon] {
            assert_eq!(up.illuminance, lux::RAW_SUNLIGHT, "the air dims the sun, not the light");
            assert_eq!(up.color, Color::WHITE, "the air colours the sun, not the light");
            assert!(up.to_light.y > 0.3);
        }
        assert!(noon.to_light.z > 0.0 && noon.to_light.y > 0.8, "a high sun, to the south");
        assert!(morning.to_light.x > 0.0, "rises in the east");
        assert!(dusk.to_light.y < 0.0 && dusk.illuminance > 0.0, "the sun lights the sky from under the horizon");
        assert!(night.to_light.y > 0.3, "the moon is up");
        assert_eq!(night.illuminance, MOONLIGHT);
        assert!(noon.ev100 > morning.ev100 - 0.01 && morning.ev100 > dusk.ev100 && dusk.ev100 > night.ev100);
        assert!(noon.ambient > dusk.ambient && dusk.ambient > night.ambient);
    }

    #[test]
    fn the_light_never_jumps() {
        let steps = 24 * 200;
        let at = |i: usize| sky_at(i as f32 * 24.0 / steps as f32);
        for i in 0..steps {
            let (a, b) = (at(i), at((i + 1) % steps));
            assert!((a.ev100 - b.ev100).abs() < 0.1, "exposure at step {i}");
            assert!((a.ambient - b.ambient).abs() < AMBIENT_DAY * 0.02, "ambient at step {i}");
            assert!((a.illuminance - b.illuminance).abs() < lux::RAW_SUNLIGHT * 0.02, "light at step {i}");
            // The swap from sun to moon has to happen in the dark: a light under the horizon casts
            // nothing (the atmosphere's sun visibility), so the one above it must be next to zero.
            if a.to_light.dot(b.to_light) < 0.99 {
                let up = if a.to_light.y > 0.0 { a } else { b };
                assert!(up.illuminance < 1.0, "direction jumps in the light at step {i}");
            }
        }
    }

    #[test]
    fn hours_parse() {
        use crate::visuals::parse_hour;
        assert_eq!(parse_hour("18:30"), Some(18.5));
        assert_eq!(parse_hour(" 13 "), Some(13.0));
        assert_eq!(parse_hour("22.25"), Some(22.25));
        assert_eq!(parse_hour("25"), Some(1.0));
        assert_eq!(parse_hour("noon"), None);
        assert_eq!(parse_hour("nan"), None);
    }

    #[test]
    fn the_settings_build() {
        meter_curve();
        let lean = lean_tables();
        assert!(matches!(lean.rendering_method, AtmosphereMode::LookupTexture));
        assert_eq!(lean.aerial_view_lut_max_distance, AERIAL_RANGE);
    }

    /// Bevy's real plugins with no GPU, so the atmosphere's observers run. The render world doesn't
    /// exist here: this covers what happens to the main world's components, not the drawing.
    fn app(ncr: bool) -> App {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<bevy::log::LogPlugin>()
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::audio::AudioPlugin>()
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .set(RenderPlugin { render_creation: WgpuSettings { backends: None, ..default() }.into(), ..default() }),
        );
        app.add_plugins(AutoExposurePlugin);
        app.insert_resource(MapLayout { name: if ncr { "NCR".into() } else { "Connaught Place".into() }, style: if ncr { shared::map::MapStyle::Ncr } else { shared::map::MapStyle::City }, ..default() });
        app.init_resource::<UltraSky>();
        app.finish();
        app.cleanup();
        app.world_mut().run_system_once(crate::camera::spawn_camera).unwrap();
        app.world_mut().spawn((Sun, DirectionalLight { illuminance: 27_000.0, ..default() }, Transform::from_xyz(1.0, 0.75, 0.45).looking_at(Vec3::ZERO, Vec3::Y)));
        app.world_mut().spawn(PaintedSky);
        app.world_mut().resource_mut::<GlobalAmbientLight>().brightness = 1800.0;
        app.update();
        app
    }

    fn set_level(app: &mut App, ultra: bool) {
        app.world_mut().run_system_once(move |mut c: Commands| apply(&mut c, ultra)).unwrap();
        app.update();
    }

    fn camera(app: &mut App) -> Entity {
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap()
    }

    fn cycle(ncr: bool) {
        // VIVAD_NO_ATMO / VIVAD_AUTOEXP / VIVAD_EV: run the suite with them set to test the switches.
        let atmosphere = std::env::var("VIVAD_NO_ATMO").is_err();
        let metered = std::env::var("VIVAD_AUTOEXP").is_ok() && std::env::var("VIVAD_EV").is_err();
        let mut app = app(ncr);
        let cam = camera(&mut app);
        let (sun, sun_at) = {
            let mut q = app.world_mut().query_filtered::<(Entity, &Transform), With<Sun>>();
            let (e, t) = q.single(app.world()).unwrap();
            (e, *t)
        };
        let hemisphere = app.world().get::<EnvironmentMapLight>(cam).map(|h| h.diffuse_map.clone());
        assert_eq!(hemisphere.is_some(), ncr);
        // Below Ultra nothing of this has touched the camera.
        set_level(&mut app, false);
        assert!(app.world().get::<DistanceFog>(cam).is_some());

        for round in 0..3 {
            set_level(&mut app, true);
            {
                let w = app.world_mut();
                let hidden = w.query_filtered::<&Visibility, With<PaintedSky>>().iter(w).all(|v| *v == Visibility::Hidden);
                let n = w.query::<&Atmosphere>().iter(w).count();
                assert_eq!(n, atmosphere as usize, "round {round}");
                assert_eq!(hidden, atmosphere);
                if atmosphere {
                    let planet = w.query_filtered::<&GlobalTransform, With<Atmosphere>>().single(w).unwrap().translation();
                    assert!(planet.y < -6_000_000.0, "the planet is under the map");
                }
            }
            let w = app.world();
            assert_eq!(w.get::<AtmosphereSettings>(cam).is_some(), atmosphere);
            assert_eq!(w.get::<AtmosphereEnvironmentMapLight>(cam).is_some(), atmosphere);
            assert_eq!(w.get::<GeneratedEnvironmentMapLight>(cam).is_some(), atmosphere);
            assert_eq!(w.get::<DistanceFog>(cam).is_none(), atmosphere);
            assert_eq!(w.get::<SunDisk>(sun).is_some(), atmosphere);
            assert_eq!(w.get::<AutoExposure>(cam).is_some(), metered);
            assert_eq!(*w.get::<Tonemapping>(cam).unwrap(), Tonemapping::TonyMcMapface);
            if atmosphere {
                assert!(w.get::<EnvironmentMapLight>(cam).is_none(), "the hemisphere would block the generated map");
            }

            // update_sun's work, to be undone.
            if atmosphere {
                let w = app.world_mut();
                let mut s = w.entity_mut(sun);
                s.get_mut::<DirectionalLight>().unwrap().illuminance = 1.0;
                *s.get_mut::<Transform>().unwrap() = Transform::from_xyz(0.0, 9.0, 0.0);
                w.resource_mut::<GlobalAmbientLight>().brightness = 10.0;
                w.get_mut::<Exposure>(cam).unwrap().ev100 = 5.5;
            }

            set_level(&mut app, false);
            let w = app.world_mut();
            assert_eq!(w.query::<&Atmosphere>().iter(w).count(), 0, "round {round}");
            assert!(w.query_filtered::<&Visibility, With<PaintedSky>>().iter(w).all(|v| *v == Visibility::Inherited));
            let w = app.world();
            assert!(w.get::<AtmosphereSettings>(cam).is_none());
            assert!(w.get::<AtmosphereEnvironmentMapLight>(cam).is_none());
            assert!(w.get::<GeneratedEnvironmentMapLight>(cam).is_none());
            assert!(w.get::<AutoExposure>(cam).is_none());
            assert!(w.get::<SunDisk>(sun).is_none());
            assert!(w.get::<DistanceFog>(cam).is_some());
            assert_eq!(*w.get::<Tonemapping>(cam).unwrap(), Tonemapping::TonyMcMapface);
            assert_eq!(w.get::<Exposure>(cam).unwrap().ev100, crate::camera::fixed_ev());
            assert_eq!(*w.get::<Transform>(sun).unwrap(), sun_at);
            assert_eq!(w.get::<DirectionalLight>(sun).unwrap().illuminance, 27_000.0);
            assert_eq!(w.resource::<GlobalAmbientLight>().brightness, 1800.0);
            // The same hemisphere, put back after the observer's clean-up.
            assert_eq!(w.get::<EnvironmentMapLight>(cam).map(|h| h.diffuse_map.clone()), hemisphere, "round {round}");
        }
    }

    #[test]
    fn f4_cycles_on_a_city_map() {
        cycle(false);
    }

    #[test]
    fn f4_cycles_on_ncr_and_keeps_its_hemisphere() {
        cycle(true);
    }
}
