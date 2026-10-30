//! Over-the-shoulder third-person camera. Mirrors `shared::sim::aim_ray` so the crosshair is
//! where shots go; pulls in when blocked by walls.

use avian3d::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{AtmosphereSettings, DistanceFog, FogFalloff};
use bevy::prelude::*;
use shared::sim::flow::Match;
use shared::sim::{CAMERA_DISTANCE, CAMERA_SHOULDER, EYE_OFFSET, Kinematic, LocalPlayer, MoveMode, PlayerIntent, btn, solid_mask};

use crate::visuals::WarriorVisual;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraSystems;

#[derive(Component)]
pub struct MainCamera {
    distance: f32,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, follow.in_set(CameraSystems).before(TransformSystems::Propagate));
    }
}

fn spawn_camera(mut commands: Commands) {
    let mut cam = commands.spawn((
        MainCamera { distance: CAMERA_DISTANCE },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, ..default() }),
        Tonemapping::AgX,
        // Most of a stop under Bevy's default (9.7): the afternoon sun and sky were washing out the
        // ground and plaster. VIVAD_EV overrides for tuning.
        bevy::camera::Exposure { ev100: std::env::var("VIVAD_EV").ok().and_then(|s| s.parse().ok()).unwrap_or(10.8) },
        // Contact shadows in corners, under eaves and between blocks.
        bevy::pbr::ScreenSpaceAmbientOcclusion { radius: 0.6, ..default() },
        Msaa::Off,
        grading(),
        AtmosphereSettings::default(),
        DistanceFog {
            color: Color::srgba(0.78, 0.74, 0.68, 1.0),
            directional_light_color: Color::srgba(1.0, 0.9, 0.7, 0.4),
            directional_light_exponent: 20.0,
            falloff: FogFalloff::from_visibility_colors(2600.0, Color::srgb(0.62, 0.60, 0.56), Color::srgb(0.85, 0.80, 0.70)),
        },
        Transform::from_xyz(0.0, 140.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Temporal anti-aliasing: without it, carved detail and normal-map highlights finer than a
    // pixel crawl and sparkle as the camera moves (MSAA can't be used alongside SSAO).
    // VIVAD_AA=smaa|none for comparison.
    match std::env::var("VIVAD_AA").as_deref() {
        Ok("none") => {}
        Ok("smaa") => {
            cam.insert(bevy::anti_alias::smaa::Smaa::default());
        }
        _ => {
            // Sharpening wins back the crispness TAA's resolve softens.
            cam.insert((
                bevy::anti_alias::taa::TemporalAntiAliasing::default(),
                bevy::anti_alias::contrast_adaptive_sharpening::ContrastAdaptiveSharpening {
                    enabled: true,
                    sharpening_strength: 0.45,
                    ..default()
                },
            ));
        }
    }
    // Render experiments (flicker hunting): VIVAD_NO_SSAO=1.
    if std::env::var("VIVAD_NO_SSAO").is_ok() {
        cam.remove::<bevy::pbr::ScreenSpaceAmbientOcclusion>();
    }
}

/// AgX alone leaves the sunlit ground chalky: a little more saturation, midtone and highlight
/// contrast (not in the shadows, which would crush the characters' brown skin), and highlights
/// pulled down. VIVAD_GRADE="saturation,contrast,highlight gain" overrides for tuning.
fn grading() -> bevy::render::view::ColorGrading {
    let v: Vec<f32> = std::env::var("VIVAD_GRADE").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (sat, con, hi) = (v.first().copied().unwrap_or(1.1), v.get(1).copied().unwrap_or(1.06), v.get(2).copied().unwrap_or(0.85));
    let mut g = bevy::render::view::ColorGrading::default();
    for s in [&mut g.shadows, &mut g.midtones, &mut g.highlights] {
        s.saturation = sat;
    }
    g.midtones.contrast = con;
    g.highlights.contrast = con;
    g.highlights.gain = hi;
    g
}

#[allow(clippy::type_complexity)]
fn follow(
    local: Query<(Entity, &PlayerIntent, &Kinematic), With<LocalPlayer>>,
    visuals: Query<(&WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<(&mut Transform, &mut Projection, &mut MainCamera)>,
    m: Res<Match>,
    spatial: SpatialQuery,
    time: Res<Time>,
    punch: Res<crate::fx::CameraPunch>,
) {
    let Ok((mut ct, mut proj, mut mc)) = cam.single_mut() else { return };
    let Ok((me, intent, kin)) = local.single() else { return };
    let Some(body) = visuals.iter().find(|(v, _)| v.sim == me).map(|(_, t)| t.translation) else { return };
    let rot = intent.aim_rotation();
    let aiming = intent.held(btn::AIM) && kin.mode == MoveMode::Ground;
    let dt = time.delta_secs();

    let (pivot, want_dist, shoulder, fov): (Vec3, f32, f32, f32) = match kin.mode {
        MoveMode::Vimana => (m.vimana_pos() + Vec3::Y * 4.0, 26.0, 0.0, 70.0),
        MoveMode::Freefall | MoveMode::Gliding => (body + Vec3::Y * EYE_OFFSET, 7.5, 0.0, 78.0),
        MoveMode::Ground if aiming => (body + Vec3::Y * EYE_OFFSET, 1.9, CAMERA_SHOULDER * 0.85, 48.0),
        MoveMode::Ground => (body + Vec3::Y * EYE_OFFSET, CAMERA_DISTANCE, CAMERA_SHOULDER, 70.0),
    };
    // Smooth zoom in/out.
    mc.distance += (want_dist - mc.distance) * (1.0 - (-dt * 12.0).exp());
    let offset = rot * Vec3::new(shoulder, 0.0, mc.distance);
    let mut pos = pivot + offset;
    // Pull the camera in front of walls.
    if let Ok((dir, len)) = Dir3::new_and_length(offset) {
        let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([me]);
        if let Some(hit) = spatial.cast_ray(pivot, dir, len, true, &filter) {
            pos = pivot + dir * (hit.distance - 0.25).max(0.2);
        }
    }
    // Punches (crate::fx): bow kick tips the view up, hits shake it, a charging bow zooms in.
    let t = time.elapsed_secs();
    let shake = Vec3::new((t * 53.0).sin() + (t * 31.0).sin() * 0.5, (t * 61.0).cos() + (t * 37.0).cos() * 0.5, 0.0) * punch.shake;
    ct.translation = pos + rot * shake;
    ct.rotation = rot * Quat::from_rotation_x(punch.kick);
    if let Projection::Perspective(p) = proj.as_mut() {
        let target = fov.to_radians() + if aiming { punch.fov } else { 0.0 };
        p.fov += (target - p.fov) * (1.0 - (-dt * 14.0).exp());
    }
}
