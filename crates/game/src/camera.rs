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
    commands.spawn((
        MainCamera { distance: CAMERA_DISTANCE },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, ..default() }),
        Tonemapping::AgX,
        // Contact shadows in corners, under eaves and between blocks.
        bevy::pbr::ScreenSpaceAmbientOcclusion { radius: 0.6, ..default() },
        Msaa::Off,
        AtmosphereSettings::default(),
        DistanceFog {
            color: Color::srgba(0.78, 0.74, 0.68, 1.0),
            directional_light_color: Color::srgba(1.0, 0.9, 0.7, 0.4),
            directional_light_exponent: 20.0,
            falloff: FogFalloff::from_visibility_colors(2600.0, Color::srgb(0.62, 0.60, 0.56), Color::srgb(0.85, 0.80, 0.70)),
        },
        Transform::from_xyz(0.0, 140.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

#[allow(clippy::type_complexity)]
fn follow(
    local: Query<(Entity, &PlayerIntent, &Kinematic), With<LocalPlayer>>,
    visuals: Query<(&WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<(&mut Transform, &mut Projection, &mut MainCamera)>,
    m: Res<Match>,
    spatial: SpatialQuery,
    time: Res<Time>,
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
    ct.translation = pos;
    ct.rotation = rot;
    if let Projection::Perspective(p) = proj.as_mut() {
        let target = fov.to_radians();
        p.fov += (target - p.fov) * (1.0 - (-dt * 14.0).exp());
    }
}
