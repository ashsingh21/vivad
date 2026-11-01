//! Inspect the small Delhi street in the real Bevy renderer. No match, bots or networking.
//! `VIVAD_SAMPLE_CAPTURE=<dir>` captures four views and exits; otherwise WASD walks the scene.
use std::time::Instant;

use avian3d::prelude::*;
use bevy::camera::{Exposure, Hdr};
use bevy::core_pipeline::Skybox;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::Gltf;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::{
    CascadeShadowConfigBuilder, DirectionalLightShadowMap, GeneratedEnvironmentMapLight,
};
use bevy::pbr::{DefaultOpaqueRendererMethod, DistanceFog, FogFalloff, ScreenSpaceReflections};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::window::{CursorGrabMode, CursorOptions, WindowResolution};
use bevy::world_serialization::WorldInstanceReady;
use game::street_sample::{
    MODEL, SamplePlan, SampleWalker, daylight_cubemap, spawn_colliders, walker_bundle,
};
use shared::sim::{DamageMsg, EYE_OFFSET, Kinematic, PlayerIntent, btn};

#[derive(Resource)]
struct Options {
    capture: Option<String>,
    mobile: bool,
    reflections: bool,
    only: Option<String>,
}

#[derive(Resource, Default)]
struct AssetsReady(Vec<Handle<Gltf>>);

#[derive(Component)]
struct ViewCamera;

#[derive(Resource, Default)]
struct Capture {
    frames: u32,
    ready: bool,
    view: usize,
    samples: Vec<f64>,
    results: Vec<ViewTiming>,
    last: Option<Instant>,
}

#[derive(serde::Serialize)]
struct ViewTiming {
    name: String,
    profile: String,
    physical_resolution: [u32; 2],
    frames: usize,
    mean_fps: f64,
    median_ms: f64,
    p95_ms: f64,
}

const VIEWS: [(&str, [f32; 3], [f32; 3]); 4] = [
    ("01_street", [-6.55, 1.76, 38.0], [1.5, 2.4, -35.0]),
    ("02_arcade", [-10.35, 1.76, 24.0], [-9.0, 2.1, -18.0]),
    ("03_metro", [2.0, 2.25, 27.0], [8.2, 0.2, 7.0]),
    ("04_overview", [22.0, 17.0, 43.0], [1.0, 3.0, -12.0]),
];

fn main() -> AppExit {
    let capture_dir = std::env::var("VIVAD_SAMPLE_CAPTURE").ok();
    if let Some(dir) = &capture_dir {
        std::fs::create_dir_all(dir).expect("capture directory");
    }
    let mobile = std::env::var("VIVAD_SAMPLE_PROFILE").as_deref() == Ok("mobile");
    let reflections = !mobile && std::env::var("VIVAD_SAMPLE_REFLECTIONS").as_deref() != Ok("0");
    let resolution = if mobile { (1280, 720) } else { (1600, 900) };
    let plan = SamplePlan::load();
    App::new()
        .insert_resource(Options {
            capture: capture_dir,
            mobile,
            reflections,
            only: std::env::var("VIVAD_SAMPLE_ONLY").ok(),
        })
        .insert_resource(plan)
        .insert_resource(if !reflections {
            DefaultOpaqueRendererMethod::forward()
        } else {
            DefaultOpaqueRendererMethod::deferred()
        })
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .insert_resource(Gravity(Vec3::ZERO))
        .insert_resource(DirectionalLightShadowMap {
            size: if mobile { 1024 } else { 2048 },
        })
        .init_resource::<AssetsReady>()
        .init_resource::<Capture>()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Vivad — Delhi street sample".into(),
                        resolution: WindowResolution::new(resolution.0, resolution.1)
                            .with_scale_factor_override(1.0),
                        resizable: false,
                        present_mode: if std::env::var("VIVAD_SAMPLE_UNCAPPED").is_ok() {
                            bevy::window::PresentMode::AutoNoVsync
                        } else {
                            default()
                        },
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: game::ASSET_DIR.into(),
                    ..default()
                }),
        )
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(game::buildings::MaterialMipsPlugin)
        .add_message::<DamageMsg>()
        .add_plugins(shared::sim::movement::MovementPlugin)
        .add_systems(Startup, (spawn_colliders, setup))
        .add_systems(PreUpdate, input.after(bevy::input::InputSystems))
        .add_systems(
            PostUpdate,
            (follow, capture)
                .chain()
                .before(TransformSystems::Propagate),
        )
        .run()
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    plan: Res<SamplePlan>,
    options: Res<Options>,
    mut ready: ResMut<AssetsReady>,
    mut images: ResMut<Assets<Image>>,
) {
    ready.0.push(server.load(MODEL));
    commands.spawn((
        WorldAssetRoot(server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
        Transform::default(),
    ));
    for prop in &plan.props {
        ready.0.push(server.load(prop.model.clone()));
        commands
            .spawn((
                WorldAssetRoot(
                    server.load(GltfAssetLabel::Scene(0).from_asset(prop.model.clone())),
                ),
                Transform::from_translation(Vec3::from_array(prop.position))
                    .with_scale(Vec3::splat(prop.scale)),
            ))
            .observe(
                |event: On<WorldInstanceReady>,
                 children: Query<&Children>,
                 names: Query<&Name>,
                 mut commands: Commands| {
                    for e in children.iter_descendants(event.entity) {
                        if names.get(e).is_ok_and(|n| n.as_str().ends_with("LOD1")) {
                            commands.entity(e).insert(Visibility::Hidden);
                        }
                    }
                },
            );
    }
    commands.spawn(walker_bundle(Vec3::new(-6.8, 0.18, 39.0)));
    commands.spawn((
        DirectionalLight {
            illuminance: 42_000.0,
            color: Color::srgb(1.0, 0.96, 0.88),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(42.0, 70.0, 32.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: if options.mobile { 1 } else { 2 },
            maximum_distance: if options.mobile { 65.0 } else { 120.0 },
            first_cascade_far_bound: 14.0,
            ..default()
        }
        .build(),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.8, 0.87, 1.0),
        brightness: 120.0,
        ..default()
    });
    let sky = images.add(daylight_cubemap());
    let mut camera = commands.spawn((
        ViewCamera,
        Camera3d::default(),
        Hdr,
        Projection::Perspective(PerspectiveProjection {
            fov: 60f32.to_radians(),
            near: 0.04,
            far: 500.0,
            ..default()
        }),
        Transform::from_translation(Vec3::from_array(VIEWS[0].1))
            .looking_at(Vec3::from_array(VIEWS[0].2), Vec3::Y),
        Tonemapping::AgX,
        Exposure {
            ev100: std::env::var("VIVAD_SAMPLE_EV")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(11.2),
        },
        Skybox {
            image: Some(sky.clone()),
            brightness: 4000.0,
            ..default()
        },
        GeneratedEnvironmentMapLight {
            environment_map: sky,
            intensity: 1500.0,
            ..default()
        },
        DistanceFog {
            color: Color::srgb(0.72, 0.79, 0.84),
            falloff: FogFalloff::from_visibility(2200.0),
            ..default()
        },
        Msaa::Off,
    ));
    if options.reflections {
        camera.insert((
            bevy::anti_alias::taa::TemporalAntiAliasing::default(),
            bevy::anti_alias::contrast_adaptive_sharpening::ContrastAdaptiveSharpening {
                sharpening_strength: 0.2,
                ..default()
            },
        ));
    } else {
        camera.insert(bevy::anti_alias::smaa::Smaa::default());
    }
    if !options.mobile {
        camera.insert((bevy::pbr::ScreenSpaceAmbientOcclusion {
            radius: 0.55,
            ..default()
        },));
        if options.reflections {
            camera.insert(ScreenSpaceReflections {
                linear_steps: 10,
                max_perceptual_roughness: 0.35..0.4,
                edge_fadeout: 0.02..0.12,
                ..default()
            });
        }
    }
    if options.capture.is_none() {
        commands.spawn((Text::new("DELHI STREET SAMPLE\nWASD walk · Mouse look · Shift sprint · Space jump\n1 street · 2 arcade · 3 metro · Esc release cursor"),TextFont {font_size:FontSize::Px(15.0),..default()},TextColor(Color::srgb(0.9,0.93,0.93)),Node {position_type:PositionType::Absolute,left:Val::Px(18.0),top:Val::Px(16.0),..default()}));
    }
}

fn input(
    options: Res<Options>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut cursor: Single<&mut CursorOptions>,
    mut walker: Single<(&mut PlayerIntent, &mut Transform, &mut Kinematic), With<SampleWalker>>,
) {
    if options.capture.is_some() {
        return;
    }
    if mouse.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    let (intent, tf, kin) = &mut *walker;
    intent.move_axis = Vec2::new(
        (keys.pressed(KeyCode::KeyD) as u8 as f32) - (keys.pressed(KeyCode::KeyA) as u8 as f32),
        (keys.pressed(KeyCode::KeyW) as u8 as f32) - (keys.pressed(KeyCode::KeyS) as u8 as f32),
    );
    intent.held = if keys.pressed(KeyCode::ShiftLeft) {
        btn::SPRINT
    } else {
        0
    };
    intent.pressed = if keys.just_pressed(KeyCode::Space) {
        btn::JUMP
    } else {
        0
    };
    if cursor.grab_mode == CursorGrabMode::Locked {
        intent.yaw -= motion.delta.x * 0.002;
        intent.pitch = (intent.pitch - motion.delta.y * 0.002).clamp(-1.2, 1.2);
    }
    for (key, pos) in [
        (KeyCode::Digit1, [-6.8, 0.18, 39.0]),
        (KeyCode::Digit2, [-10.0, 0.18, 23.0]),
        (KeyCode::Digit3, [8.2, 0.18, 16.7]),
    ] {
        if keys.just_pressed(key) {
            tf.translation = Vec3::from_array(pos) + Vec3::Y * (shared::sim::HALF_HEIGHT + 0.03);
            intent.yaw = 0.0;
            intent.pitch = 0.0;
            kin.velocity = Vec3::ZERO;
            kin.grounded = false;
        }
    }
}

fn follow(
    options: Res<Options>,
    walker: Single<(&Transform, &PlayerIntent), With<SampleWalker>>,
    mut camera: Single<&mut Transform, (With<ViewCamera>, Without<SampleWalker>)>,
) {
    if options.capture.is_some() {
        return;
    }
    let (tf, intent) = *walker;
    camera.translation = tf.translation + Vec3::Y * EYE_OFFSET;
    camera.rotation = intent.aim_rotation();
}

fn capture(
    mut commands: Commands,
    options: Res<Options>,
    server: Res<AssetServer>,
    assets: Res<AssetsReady>,
    mut state: ResMut<Capture>,
    mut camera: Single<&mut Transform, With<ViewCamera>>,
    mut window: Single<&mut Window>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(dir) = &options.capture else {
        return;
    };
    let expected = if options.mobile {
        [1280, 720]
    } else {
        [1600, 900]
    };
    if window.resolution.physical_width() != expected[0]
        || window.resolution.physical_height() != expected[1]
    {
        window
            .resolution
            .set_physical_resolution(expected[0], expected[1]);
        state.frames = 0;
        state.samples.clear();
        state.last = None;
        return;
    }
    if !state.ready {
        if assets
            .0
            .iter()
            .all(|h| server.is_loaded_with_dependencies(h))
        {
            state.ready = true;
            state.frames = 0;
        } else {
            state.frames += 1;
            if state.frames > 2400 {
                error!("Street sample assets failed to load");
                exit.write(AppExit::error());
            }
            return;
        }
    }
    if state.view >= VIEWS.len() {
        state.frames += 1;
        if state.frames == 30 {
            let report =
                ron::ser::to_string_pretty(&state.results, ron::ser::PrettyConfig::default())
                    .expect("timing report");
            std::fs::write(format!("{dir}/frame-timings.ron"), report).expect("save timing report");
            exit.write(AppExit::Success);
        }
        return;
    }
    if options
        .only
        .as_ref()
        .is_some_and(|only| !only.split(',').any(|key| VIEWS[state.view].0.contains(key)))
    {
        state.view += 1;
        state.frames = 0;
        return;
    }
    let (name, pos, look) = VIEWS[state.view];
    **camera = Transform::from_translation(Vec3::from_array(pos))
        .looking_at(Vec3::from_array(look), Vec3::Y);
    let now = Instant::now();
    if state.frames >= 120 && state.frames < 320 {
        if let Some(last) = state.last {
            state
                .samples
                .push(now.duration_since(last).as_secs_f64() * 1000.0);
        }
    }
    state.last = Some(now);
    state.frames += 1;
    if state.frames == 320 {
        let mut samples = state.samples.clone();
        samples.sort_by(f64::total_cmp);
        let n = samples.len();
        let mean = samples.iter().sum::<f64>() / n as f64;
        let result = ViewTiming {
            name: name.into(),
            profile: if options.mobile {
                "mobile preview on desktop".into()
            } else {
                "Mac desktop".into()
            },
            physical_resolution: expected,
            frames: n,
            mean_fps: 1000.0 / mean,
            median_ms: samples[n / 2],
            p95_ms: samples[(n * 95 / 100).min(n - 1)],
        };
        info!(
            "[sample-frame-cadence] {name}: {:.1} FPS, median {:.2} ms, p95 {:.2} ms, {n} samples",
            result.mean_fps, result.median_ms, result.p95_ms
        );
        state.results.push(result);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{dir}/{name}.png")));
    }
    if state.frames >= 335 {
        state.frames = 0;
        state.view += 1;
        state.samples.clear();
        state.last = None;
    }
}
