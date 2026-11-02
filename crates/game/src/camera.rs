//! Over-the-shoulder third-person camera. Mirrors `shared::sim::aim_origin` so the crosshair is
//! where shots go; pulls in when blocked by walls. At a scope (crate::scope) it goes to the eye.

use avian3d::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use shared::sim::flow::Match;
use shared::sim::stance::Stance;
use shared::sim::{ADS_DISTANCE, CAMERA_DISTANCE, CAMERA_SHOULDER, EYE_OFFSET, Kinematic, LocalPlayer, MoveMode, PlayerIntent, btn, camera_spot};

use crate::visuals::{WarriorVisual, TimeOfDay};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraSystems;

#[derive(Component)]
pub struct MainCamera {
    distance: f32,
    /// How far a crouch has brought the view down, eased.
    drop: f32,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::scope::Scope>()
            .add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, (follow, update_fog.run_if(crate::visuals::daynight)).chain().in_set(CameraSystems).before(TransformSystems::Propagate));
    }
}

pub fn spawn_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>, layout: Res<shared::map::MapLayout>) {
    // The NCR arena: cooler, farther haze over the towers, and a sky-to-ground hemisphere light.
    let ncr = layout.style == shared::map::MapStyle::Ncr;
    // Ambient occlusion and anti-aliasing are the quality level's (crate::quality).
    let mut cam = commands.spawn((
        MainCamera { distance: CAMERA_DISTANCE, drop: 0.0 },
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, ..default() }),
        look(ncr, false),
        bevy::camera::Exposure { ev100: fixed_ev() },
        Msaa::Off,
        distance_fog(ncr),
        Transform::from_xyz(0.0, 140.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    if ncr {
        cam.insert(
            EnvironmentMapLight::hemispherical_gradient(&mut images, Color::srgb(0.42, 0.60, 0.80), Color::srgb(0.72, 0.77, 0.81), Color::srgb(0.25, 0.24, 0.22))
                .with_intensity(1800.0),
        );
    }
}

/// VIVAD_EV, for tuning.
pub fn ev_override() -> Option<f32> {
    std::env::var("VIVAD_EV").ok()?.parse().ok()
}

/// The exposure below Ultra. Most of a stop under Bevy's default (9.7): the afternoon sun and sky
/// were washing out the ground and plaster.
pub fn fixed_ev() -> f32 {
    ev_override().unwrap_or(10.8)
}

/// The haze below Ultra, where the atmosphere's aerial perspective takes over (crate::ultra).
pub fn distance_fog(ncr: bool) -> DistanceFog {
    DistanceFog {
        // The sky's horizon haze (crate::sky), so the far city fades into it. (A warm tint here
        // once turned the whole distant city orange.)
        color: if ncr { Color::srgb(0.68, 0.77, 0.83) } else { Color::srgb(crate::sky::HORIZON[0], crate::sky::HORIZON[1], crate::sky::HORIZON[2]) },
        directional_light_color: Color::srgba(1.0, 0.93, 0.82, 0.25),
        directional_light_exponent: 20.0,
        falloff: if ncr {
            FogFalloff::from_visibility_colors(4200.0, Color::srgb(0.66, 0.74, 0.80), Color::srgb(0.82, 0.85, 0.87))
        } else {
            FogFalloff::from_visibility_colors(2600.0, Color::srgb(0.60, 0.60, 0.60), Color::srgb(0.80, 0.84, 0.90))
        },
    }
}

/// The haze on a big map: it has to hide the line where the camera stops drawing (`far`) and the
/// map's own edge. The squared falloff keeps the first few hundred metres clear, thins the middle
/// distance and is all but solid at the far plane, in the sky's own horizon colour so the city
/// sinks into the sky with no line between them.
pub fn big_map_fog(far: f32) -> DistanceFog {
    let [r, g, b] = crate::sky::HORIZON;
    DistanceFog {
        color: Color::srgb(r, g, b),
        directional_light_color: Color::srgba(1.0, 0.93, 0.82, 0.25),
        directional_light_exponent: 20.0,
        falloff: big_map_haze(far),
    }
}

/// How thick `big_map_fog` is, for a camera whose far plane is `far`.
fn big_map_haze(far: f32) -> FogFalloff {
    const SHARE_OF_FAR: f32 = 0.87;
    FogFalloff::from_visibility_squared(far * SHARE_OF_FAR)
}

/// The picture's finish: AgX and a grade below Ultra; TonyMcMapface, near-neutral, on Ultra, where
/// the light itself is right. Also a soft glow round the sun's glints, lamps and muzzle flashes, and
/// the frame's edges darkened a little to draw the eye to the middle: cheap, and most of what reads
/// as a "finished" look. VIVAD_NO_POST=1 turns those two off, for comparisons.
pub fn look(ncr: bool, ultra: bool) -> impl Bundle {
    // VIVAD_TONEMAP=agx: compare the looks on Ultra.
    let tony = ultra && std::env::var("VIVAD_TONEMAP").as_deref() != Ok("agx");
    (if tony { Tonemapping::TonyMcMapface } else { Tonemapping::AgX }, grading(ncr, ultra), post(ultra))
}

fn post(ultra: bool) -> (bevy::post_process::bloom::Bloom, bevy::post_process::effect_stack::Vignette) {
    use bevy::post_process::{bloom::Bloom, effect_stack::Vignette};
    if std::env::var("VIVAD_NO_POST").is_ok() {
        return (Bloom { intensity: 0.0, ..Bloom::NATURAL }, Vignette { intensity: 0.0, ..default() });
    }
    let (bloom, vignette) = if ultra { (0.05, 0.12) } else { (0.07, 0.22) };
    (Bloom { intensity: bloom, ..Bloom::NATURAL }, Vignette { intensity: vignette, radius: 0.9, smoothness: 2.5, ..default() })
}

/// AgX alone leaves the sunlit ground chalky: a little more saturation, midtone and highlight
/// contrast (not in the shadows, which would crush the characters' brown skin), and highlights
/// pulled down. Ultra needs only a touch of saturation, and no painted warmth: the sun's colour
/// comes through the air. VIVAD_GRADE="saturation,contrast,highlight gain" overrides for tuning.
fn grading(ncr: bool, ultra: bool) -> bevy::render::view::ColorGrading {
    let v: Vec<f32> = std::env::var("VIVAD_GRADE").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (dsat, dcon, dhi) = if ultra { (1.05, 1.0, 1.0) } else { (1.1, 1.06, 0.85) };
    let (sat, con, hi) = (v.first().copied().unwrap_or(dsat), v.get(1).copied().unwrap_or(dcon), v.get(2).copied().unwrap_or(dhi));
    let mut g = bevy::render::view::ColorGrading::default();
    for s in [&mut g.shadows, &mut g.midtones, &mut g.highlights] {
        s.saturation = sat;
    }
    g.midtones.contrast = con;
    g.highlights.contrast = con;
    g.highlights.gain = hi;
    // A touch of afternoon warmth over the whole frame.
    g.global.temperature = if ultra { 0.0 } else { 0.015 };
    if ncr {
        // The NCR arena: cooler and a little drained, more contrast: a smoggy city, not golden Bharat.
        g.global.temperature = -0.01;
        g.midtones.saturation *= 0.8;
        g.highlights.saturation *= 0.85;
        g.midtones.contrast *= 1.06;
    }
    g
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow(
    local: Query<(Entity, &PlayerIntent, &Kinematic, Option<&Stance>), With<LocalPlayer>>,
    visuals: Query<(&WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<(&mut Transform, &mut Projection, &mut MainCamera)>,
    m: Res<Match>,
    spatial: SpatialQuery,
    time: Res<Time>,
    punch: Res<crate::fx::CameraPunch>,
    ride: Res<crate::visuals::DropRide>,
    scope: Res<crate::scope::Scope>,
) {
    let Ok((mut ct, mut proj, mut mc)) = cam.single_mut() else { return };
    let Ok((me, intent, kin, stance)) = local.single() else { return };
    let Some(body) = visuals.iter().find(|(v, _)| v.sim == me).map(|(_, t)| t.translation) else { return };
    let rot = intent.aim_rotation();
    let aiming = intent.held(btn::AIM) && kin.mode == MoveMode::Ground;
    let dt = time.delta_secs();
    mc.drop += (stance.map_or(0.0, Stance::drop) - mc.drop) * (1.0 - (-dt * 12.0).exp());
    let eye = body + Vec3::Y * (EYE_OFFSET - mc.drop);

    let (pivot, want_dist, shoulder, fov): (Vec3, f32, f32, f32) = match kin.mode {
        // The auto's fans stand 14 m over its floor: framed from farther back and higher.
        MoveMode::Vimana if *ride == crate::visuals::DropRide::Jugaad => (m.vimana_pos() + Vec3::Y * 8.0, 40.0, 0.0, 70.0),
        MoveMode::Vimana => (m.vimana_pos() + Vec3::Y * 4.0, 26.0, 0.0, 70.0),
        MoveMode::Freefall | MoveMode::Gliding => (body + Vec3::Y * EYE_OFFSET, 7.5, 0.0, 78.0),
        MoveMode::Ground if scope.active() => (eye, 0.0, 0.0, scope.fov()),
        MoveMode::Ground if aiming => (eye, ADS_DISTANCE, CAMERA_SHOULDER, 48.0),
        MoveMode::Ground => (eye, CAMERA_DISTANCE, CAMERA_SHOULDER, 70.0),
    };
    // Smooth zoom in/out.
    mc.distance += (want_dist - mc.distance) * (1.0 - (-dt * 12.0).exp());
    if scope.active() {
        mc.distance = 0.0;
    }
    // In front of walls, the same spot the sim fires from.
    let pos = camera_spot(&spatial, pivot, rot * Vec3::new(shoulder, 0.0, mc.distance), me);
    // Punches (crate::fx): bow kick tips the view up, hits shake it, a charging bow zooms in.
    let t = time.elapsed_secs();
    // Organic shake: sum of multiple prime-frequency sines for a non-repetitive feel.
    let shake = Vec3::new(
        (t * 41.0).sin() * 0.6 + (t * 23.0).sin() * 0.3 + (t * 11.0).sin() * 0.1,
        (t * 37.0).cos() * 0.6 + (t * 17.0).cos() * 0.3 + (t * 7.0).cos() * 0.1,
        (t * 13.0).sin() * 0.2,
    ) * punch.shake;

    // Head bob: periodic vertical offset based on movement speed.
    let speed = kin.velocity.length();
    let bob = if speed > 0.1 {
        let bob_freq = 10.0; // steps per second
        let bob_amp = 0.05 * (speed / 7.4).min(1.0);
        Vec3::Y * (t * bob_freq).sin() * bob_amp
    } else {
        Vec3::ZERO
    };

    ct.translation = pos + rot * (shake + bob);
    ct.rotation = rot * Quat::from_rotation_x(punch.kick);
    if let Projection::Perspective(p) = proj.as_mut() {
        // Dynamic FOV: scale the base FOV by velocity to increase sensation of speed.
        // The full kick at the small maps' ride speed, on every map: a sprint feels the same
        // everywhere, and the NCR's faster ride stops at the same kick.
        let velocity_factor = (speed / shared::sim::flow::VIMANA_SPEED).clamp(0.0, 1.0);
        let speed_fov = velocity_factor * 10.0 * (1.0 - (fov / 180.0));
        let target = fov.to_radians() + speed_fov.to_radians() + if aiming { punch.fov } else { 0.0 };
        p.fov += (target - p.fov) * (1.0 - (-dt * 14.0).exp());
        if scope.active() {
            p.fov = fov.to_radians();
        }
    }
}

fn update_fog(
    tod: Res<TimeOfDay>,
    layout: Res<shared::map::MapLayout>,
    mut cam_q: Query<(&mut DistanceFog, &Projection)>,
) {
    let Ok((mut fog, projection)) = cam_q.single_mut() else { return };

    let angle = (tod.hour - 6.0) * (std::f32::consts::PI / 12.0);
    let intensity = (angle.sin()).max(0.0);

    // Smog transitions: Yellow-Grey (Morning) -> Pale Blue (Noon) -> Orange-Purple (Evening) -> Dark (Night)
    if angle < 0.0 || angle > std::f32::consts::PI {
        fog.color = Color::srgb(0.1, 0.1, 0.2);
    } else if angle < std::f32::consts::PI / 4.0 {
        fog.color = Color::srgb(0.7, 0.65, 0.5); // Morning smog
    } else if angle < 3.0 * std::f32::consts::PI / 4.0 {
        fog.color = Color::srgb(0.75, 0.8, 0.85); // Day haze
    } else {
        fog.color = Color::srgb(0.6, 0.4, 0.3); // Evening haze
    }

    // A big map's haze closes before the map ends at every hour, or its edge would show.
    if layout.terrain.n >= crate::visuals::BIG_TERRAIN
        && let Projection::Perspective(p) = projection
    {
        fog.falloff = big_map_haze(p.far);
        return;
    }

    // Visibility drops during smoggy hours (morning/evening)
    let visibility = 2000.0 + intensity * 2000.0;
    fog.falloff = FogFalloff::from_visibility_colors(
        visibility,
        fog.color,
        Color::srgb(0.8, 0.8, 0.8),
    );
}
