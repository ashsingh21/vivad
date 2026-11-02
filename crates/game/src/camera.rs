//! Over-the-shoulder third-person camera. Mirrors `shared::sim::aim_origin` so the crosshair is
//! where shots go; pulls in when blocked by walls. At a scope (crate::scope) it goes to the eye.

use avian3d::prelude::*;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use shared::items::{ItemDb, ItemKind};
use shared::sim::flow::Match;
use shared::sim::{ADS_DISTANCE, CAMERA_DISTANCE, CAMERA_SHOULDER, EYE_OFFSET, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, btn, camera_distance, solid_mask};

use crate::visuals::{WarriorVisual, TimeOfDay};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraSystems;

#[derive(Component)]
pub struct MainCamera {
    distance: f32,
    /// The eye's height over the body, eased as the player crouches and stands.
    eye: f32,
    /// How far out along its line the camera is now: in at once at a wall, easing back out.
    boom: f32,
    /// How far the view's height trails the body's, and how fast that's changing: a spring, so
    /// stairs and landings don't jolt the view, sideways it stays rigid and the crosshair true.
    lag: f32,
    lag_speed: f32,
    /// The body's height last frame.
    body_y: f32,
    /// The fall speed while in the air, for the dip on landing.
    falling: f32,
}

/// The camera's radius against walls.
const CAMERA_RADIUS: f32 = 0.18;
/// How fast the camera moves back out once a wall has gone by (m/s).
const BOOM_OUT: f32 = 4.0;
/// The height spring's stiffness (radians a second, critically damped), and how far it can trail.
const SPRING: f32 = 22.0;
const MOST_LAG: f32 = 0.3;
/// The view dips on landing at this share of the fall speed, for an instant, up to the most.
const DIP: f32 = 0.16;
const MOST_DIP: f32 = 1.6;

/// How far a sphere gets out from `pivot` along `dir`, up to `len`.
fn boom_room(spatial: &SpatialQuery, pivot: Vec3, dir: Dir3, len: f32, me: Entity) -> f32 {
    let config = ShapeCastConfig { max_distance: len, ignore_origin_penetration: true, ..default() };
    let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([me]);
    spatial.cast_shape(&Collider::sphere(CAMERA_RADIUS), pivot, Quat::IDENTITY, dir, &config, &filter).map_or(len, |hit| hit.distance.max(0.2))
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
        MainCamera { distance: CAMERA_DISTANCE, eye: EYE_OFFSET, boom: CAMERA_DISTANCE, lag: 0.0, lag_speed: 0.0, body_y: 0.0, falling: 0.0 },
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
            EnvironmentMapLight::hemispherical_gradient(&mut images, Color::srgb(0.66, 0.70, 0.74), Color::srgb(0.78, 0.78, 0.76), Color::srgb(0.36, 0.33, 0.29))
                .with_intensity(3600.0),
        );
    }
}

/// VIVAD_EV, for tuning.
pub fn ev_override() -> Option<f32> {
    std::env::var("VIVAD_EV").ok()?.parse().ok()
}

/// The exposure below Ultra: sunlit mid grey at about 0.25 before tone mapping. Anything brighter
/// clips the sunlit ground to chalk.
pub fn fixed_ev() -> f32 {
    ev_override().unwrap_or(12.0)
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

/// The picture's finish: TonyMcMapface and a near-neutral grade on every level. Also a soft glow round the sun's glints, lamps and muzzle flashes, and
/// the frame's edges darkened a little to draw the eye to the middle: cheap, and most of what reads
/// as a "finished" look. VIVAD_NO_POST=1 turns those two off, for comparisons.
pub fn look(_ncr: bool, ultra: bool) -> impl Bundle {
    // VIVAD_TONEMAP=agx: compare the looks.
    let tony = std::env::var("VIVAD_TONEMAP").as_deref() != Ok("agx");
    (if tony { Tonemapping::TonyMcMapface } else { Tonemapping::AgX }, grading(), post(ultra))
}

fn post(ultra: bool) -> (bevy::post_process::bloom::Bloom, bevy::post_process::effect_stack::Vignette) {
    use bevy::post_process::{bloom::Bloom, effect_stack::Vignette};
    if std::env::var("VIVAD_NO_POST").is_ok() {
        return (Bloom { intensity: 0.0, ..Bloom::NATURAL }, Vignette { intensity: 0.0, ..default() });
    }
    let (bloom, vignette) = if ultra { (0.05, 0.12) } else { (0.07, 0.22) };
    (Bloom { intensity: bloom, ..Bloom::NATURAL }, Vignette { intensity: vignette, radius: 0.9, smoothness: 2.5, ..default() })
}

/// Only a touch of saturation: the light and the surfaces carry the picture.
/// VIVAD_GRADE="saturation,contrast,highlight gain" overrides for tuning.
fn grading() -> bevy::render::view::ColorGrading {
    let v: Vec<f32> = std::env::var("VIVAD_GRADE").unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect();
    let (dsat, dcon, dhi) = (1.05, 1.0, 1.0);
    let (sat, con, hi) = (v.first().copied().unwrap_or(dsat), v.get(1).copied().unwrap_or(dcon), v.get(2).copied().unwrap_or(dhi));
    let mut g = bevy::render::view::ColorGrading::default();
    for s in [&mut g.shadows, &mut g.midtones, &mut g.highlights] {
        s.saturation = sat;
    }
    g.midtones.contrast = con;
    g.highlights.contrast = con;
    g.highlights.gain = hi;
    g
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow(
    local: Query<(Entity, &PlayerIntent, &Kinematic, &Inventory), With<LocalPlayer>>,
    visuals: Query<(&WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<(&mut Transform, &mut Projection, &mut MainCamera)>,
    db: Res<ItemDb>,
    m: Res<Match>,
    spatial: SpatialQuery,
    time: Res<Time>,
    punch: Res<crate::fx::CameraPunch>,
    ride: Res<crate::visuals::DropRide>,
    scope: Res<crate::scope::Scope>,
) {
    let Ok((mut ct, mut proj, mut mc)) = cam.single_mut() else { return };
    let Ok((me, intent, kin, inv)) = local.single() else { return };
    let Some(body) = visuals.iter().find(|(v, _)| v.sim == me).map(|(_, t)| t.translation) else { return };
    let rot = intent.aim_rotation();
    // A raised guard keeps the wide view: a fight needs it.
    let guard = inv.current().is_some_and(|s| matches!(db.get(&s.id).kind, ItemKind::Melee { .. }));
    let aiming = intent.held(btn::AIM) && kin.mode == MoveMode::Ground && !guard;
    let dt = time.delta_secs();
    mc.eye += (shared::sim::eye_offset(intent) - mc.eye) * (1.0 - (-dt * 10.0).exp());
    let eye = body + Vec3::Y * mc.eye;

    let (pivot, want_dist, shoulder, fov): (Vec3, f32, f32, f32) = match kin.mode {
        // The auto's fans stand 14 m over its floor: framed from farther back and higher.
        MoveMode::Vimana if *ride == crate::visuals::DropRide::Jugaad => (m.vimana_pos() + Vec3::Y * 8.0, 40.0, 0.0, 70.0),
        MoveMode::Vimana => (m.vimana_pos() + Vec3::Y * 4.0, 26.0, 0.0, 70.0),
        MoveMode::Freefall | MoveMode::Gliding => (body + Vec3::Y * EYE_OFFSET, 7.5, 0.0, 78.0),
        MoveMode::Ground if scope.active() => (eye, 0.0, 0.0, scope.fov()),
        MoveMode::Ground if aiming => (eye, ADS_DISTANCE, CAMERA_SHOULDER, 48.0),
        MoveMode::Ground => (eye, camera_distance(&spatial, eye, me), CAMERA_SHOULDER, 70.0),
        MoveMode::Riding => (body + Vec3::Y * 1.0, 5.4, 0.0, 72.0),
    };
    // Smooth zoom in/out.
    mc.distance += (want_dist - mc.distance) * (1.0 - (-dt * 12.0).exp());
    if scope.active() {
        mc.distance = 0.0;
    }
    // The height on a spring, on foot and not aiming: the shot's line wants it true then. A landing
    // gives it a push down.
    let on_foot = kin.mode == MoveMode::Ground;
    if on_foot && !kin.grounded {
        mc.falling = mc.falling.max(-kin.velocity.y);
    } else {
        let fell = std::mem::take(&mut mc.falling);
        if fell > 3.0 {
            mc.lag_speed -= (fell * DIP).min(MOST_DIP);
        }
    }
    let rise = body.y - std::mem::replace(&mut mc.body_y, body.y);
    if on_foot && !aiming && !scope.active() {
        let step = dt.min(1.0 / 30.0);
        let (lag, trailing) = (mc.lag - rise, mc.lag_speed);
        mc.lag_speed += (-lag * SPRING * SPRING - trailing * 2.0 * SPRING) * step;
        mc.lag = (lag + mc.lag_speed * step).clamp(-MOST_LAG, MOST_LAG);
    } else {
        mc.lag = 0.0;
        mc.lag_speed = 0.0;
    }
    let pivot = pivot + Vec3::Y * mc.lag;
    // Out from the eye along the boom: in at once in front of a wall, easing back out, so a pillar
    // going by doesn't pop the view in and out.
    let offset = rot * Vec3::new(shoulder, 0.0, mc.distance);
    let pos = match Dir3::new_and_length(offset) {
        Ok((dir, len)) => {
            let room = boom_room(&spatial, pivot, dir, len, me);
            let out = if on_foot { BOOM_OUT } else { len };
            mc.boom = if room < mc.boom { room } else { (mc.boom + out * dt).min(room) };
            pivot + dir * mc.boom
        }
        Err(_) => pivot,
    };
    // Punches (crate::fx): bow kick tips the view up, hits shake it, a charging bow zooms in.
    let t = time.elapsed_secs();
    // Organic shake: sum of multiple prime-frequency sines for a non-repetitive feel.
    let shake = Vec3::new(
        (t * 41.0).sin() * 0.6 + (t * 23.0).sin() * 0.3 + (t * 11.0).sin() * 0.1,
        (t * 37.0).cos() * 0.6 + (t * 17.0).cos() * 0.3 + (t * 7.0).cos() * 0.1,
        (t * 13.0).sin() * 0.2,
    ) * punch.shake;

    let speed = kin.velocity.length();
    ct.translation = pos + rot * shake;
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
