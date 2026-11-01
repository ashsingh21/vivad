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

pub fn spawn_camera(mut commands: Commands) {
    // Ambient occlusion and anti-aliasing are the quality level's (crate::quality).
    commands.spawn((
        MainCamera { distance: CAMERA_DISTANCE },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, ..default() }),
        Tonemapping::AgX,
        // Most of a stop under Bevy's default (9.7): the afternoon sun and sky were washing out the
        // ground and plaster. VIVAD_EV overrides for tuning.
        bevy::camera::Exposure { ev100: std::env::var("VIVAD_EV").ok().and_then(|s| s.parse().ok()).unwrap_or(10.8) },
        Msaa::Off,
        grading(),
        // A soft glow round the sun's glints, lamps and muzzle flashes, and the frame's edges
        // darkened a little to draw the eye to the middle: cheap, and most of what reads as a
        // "finished" look. VIVAD_NO_POST=1 turns both off, for comparisons.
        post(),
        AtmosphereSettings::default(),
        DistanceFog {
            // Neutral, a touch of sky blue: a warm tint here turned the whole distant city orange.
            color: Color::srgba(0.78, 0.80, 0.84, 1.0),
            directional_light_color: Color::srgba(1.0, 0.93, 0.82, 0.25),
            directional_light_exponent: 20.0,
            falloff: FogFalloff::from_visibility_colors(2600.0, Color::srgb(0.60, 0.60, 0.60), Color::srgb(0.80, 0.84, 0.90)),
        },
        Transform::from_xyz(0.0, 140.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn post() -> (bevy::post_process::bloom::Bloom, bevy::post_process::effect_stack::Vignette) {
    use bevy::post_process::{bloom::Bloom, effect_stack::Vignette};
    if std::env::var("VIVAD_NO_POST").is_ok() {
        return (Bloom { intensity: 0.0, ..Bloom::NATURAL }, Vignette { intensity: 0.0, ..default() });
    }
    (Bloom { intensity: 0.07, ..Bloom::NATURAL }, Vignette { intensity: 0.22, radius: 0.9, smoothness: 2.5, ..default() })
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
    // A touch of afternoon warmth over the whole frame.
    g.global.temperature = 0.015;
    g
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow(
    local: Query<(Entity, &PlayerIntent, &Kinematic), With<LocalPlayer>>,
    visuals: Query<(&WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<(&mut Transform, &mut Projection, &mut MainCamera)>,
    m: Res<Match>,
    spatial: SpatialQuery,
    time: Res<Time>,
    punch: Res<crate::fx::CameraPunch>,
    ride: Res<crate::visuals::DropRide>,
) {
    let Ok((mut ct, mut proj, mut mc)) = cam.single_mut() else { return };
    let Ok((me, intent, kin)) = local.single() else { return };
    let Some(body) = visuals.iter().find(|(v, _)| v.sim == me).map(|(_, t)| t.translation) else { return };
    let rot = intent.aim_rotation();
    let aiming = intent.held(btn::AIM) && kin.mode == MoveMode::Ground;
    let dt = time.delta_secs();

    let (pivot, want_dist, shoulder, fov): (Vec3, f32, f32, f32) = match kin.mode {
        // The auto's fans stand 14 m over its floor: framed from farther back and higher.
        MoveMode::Vimana if *ride == crate::visuals::DropRide::Jugaad => (m.vimana_pos() + Vec3::Y * 8.0, 40.0, 0.0, 70.0),
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
