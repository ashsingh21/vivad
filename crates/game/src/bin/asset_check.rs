//! Loads a weapon GLB, lays out its LODs side by side, draws the weapon axes at the grip origin,
//! saves a screenshot and exits. Verifies scale, orientation and materials survive the export.
//!
//! cargo run -p game --bin asset_check -- models/weapons/khanda.glb previews/khanda/bevy_khanda.png

use bevy::gltf::Gltf;
use bevy::light::GeneratedEnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::world_serialization::WorldInstanceReady;
use game::studio_env::studio_cubemap;
use shared::weapon::{WEAPON_EDGE, WEAPON_FORWARD};

/// Repo-level assets/ regardless of how the binary is launched.
const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
/// Give up if the model never finishes loading.
const TIMEOUT_FRAMES: u32 = 900;

#[derive(Resource)]
struct Args {
    model: String,
    out: String,
}

#[derive(Resource)]
struct Model(Handle<Gltf>);

#[derive(Resource, Default)]
struct Frames {
    ready_at: Option<u32>,
    count: u32,
}

fn main() {
    let mut argv = std::env::args().skip(1);
    let model = argv.next().unwrap_or_else(|| "models/weapons/khanda.glb".into());
    let out = argv.next().unwrap_or_else(|| "previews/bevy_check.png".into());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "asset check".into(),
                resolution: (1600u32, 900u32).into(),
                ..default()
            }),
            ..default()
        }).set(AssetPlugin { file_path: ASSET_DIR.into(), ..default() }))
        .insert_resource(Args { model, out })
        .init_resource::<Frames>()
        .add_systems(Startup, setup)
        .add_systems(Update, (draw_axes, capture_then_exit))
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let env = images.add(studio_cubemap());
    // Camera looks at the blade flat (+X side); blade runs up the screen (+Y).
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(1.9, 0.38, -0.35).looking_at(Vec3::new(0.0, 0.38, -0.35), Vec3::Y),
        GeneratedEnvironmentMapLight { environment_map: env, intensity: 1800.0, ..default() },
    ));
    commands.spawn((
        DirectionalLight { illuminance: 6000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(2.0, 3.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(ClearColor(Color::srgb(0.18, 0.18, 0.185)));
    commands.insert_resource(Model(assets.load(args.model.clone())));
    let scene = assets.load(GltfAssetLabel::Scene(0).from_asset(args.model.clone()));
    commands.spawn((WorldAssetRoot(scene), Transform::default())).observe(spread_lods);
}

/// LOD0 at the origin, LOD1/LOD2 shifted along -Z so all three are visible.
fn spread_lods(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    mut named: Query<(&Name, &mut Transform)>,
) {
    for e in children.iter_descendants(ready.entity) {
        if let Ok((name, mut tf)) = named.get_mut(e) {
            let n = name.as_str();
            if let Some(i) = n.rsplit("LOD").next().and_then(|s| s.parse::<f32>().ok()) {
                tf.translation.z -= 0.35 * i;
                info!("{n}: placed at {:?}", tf.translation);
            }
        }
    }
}

fn draw_axes(mut gizmos: Gizmos) {
    // Weapon convention: forward (+Y) green, edge (-Z) blue-ish, flats (X) red.
    gizmos.arrow(Vec3::ZERO, WEAPON_FORWARD * 0.25, Color::srgb(0.2, 1.0, 0.2));
    gizmos.arrow(Vec3::ZERO, WEAPON_EDGE * 0.15, Color::srgb(0.2, 0.5, 1.0));
    gizmos.arrow(Vec3::ZERO, Vec3::X * 0.15, Color::srgb(1.0, 0.2, 0.2));
}

fn capture_then_exit(
    mut commands: Commands,
    assets: Res<AssetServer>,
    model: Res<Model>,
    mut frames: ResMut<Frames>,
    args: Res<Args>,
    mut exit: MessageWriter<AppExit>,
) {
    frames.count += 1;
    // Meshes are not drawn until their textures are decoded, so wait for every dependency.
    if frames.ready_at.is_none() && assets.is_loaded_with_dependencies(&model.0) {
        frames.ready_at = Some(frames.count);
    }
    let Some(ready) = frames.ready_at else {
        if frames.count > TIMEOUT_FRAMES {
            error!("model never became ready: {}", args.model);
            exit.write(AppExit::error());
        }
        return;
    };
    // Give the environment map filter and textures a few frames to settle.
    if frames.count == ready + 60 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(args.out.clone()));
    }
    if frames.count == ready + 70 {
        exit.write(AppExit::Success);
    }
}
