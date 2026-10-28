//! Vivad client: the shared simulation plus presentation and local input.

use avian3d::prelude::*;
use bevy::prelude::*;
use game::{animation::WarriorAnimationPlugin, arms::ArmsPlugin, fx::FxPlugin, grass::GrassPlugin, audio::SoundPlugin, autoshot::AutoshotPlugin, buildings::BuildingsPlugin, camera::CameraPlugin, characters::CharacterArtPlugin, hud::HudPlugin, input::InputPlugin, lamps::LampsPlugin, visuals::VisualsPlugin, ASSET_DIR};
use shared::sim::{MatchConfig, SimPlugin};

fn main() {
    let items_ron = std::fs::read_to_string(format!("{ASSET_DIR}/data/items.ron")).expect("read items.ron");
    let bots = std::env::var("VIVAD_BOTS").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    let autopilot = std::env::var("VIVAD_AUTOPILOT").is_ok();
    App::new()
        .insert_resource(MatchConfig { bots, autopilot, ..default() })
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window { title: "Vivad".into(), resolution: (1600u32, 900u32).into(), ..default() }),
                    ..default()
                })
                .set(AssetPlugin { file_path: ASSET_DIR.into(), ..default() }),
        )
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron })
        .add_plugins((InputPlugin, VisualsPlugin, ArmsPlugin, FxPlugin, GrassPlugin, LampsPlugin, WarriorAnimationPlugin, SoundPlugin, BuildingsPlugin, CharacterArtPlugin, CameraPlugin, HudPlugin, AutoshotPlugin))
        .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin {
            wait_duration: std::time::Duration::from_secs(10),
            filter: std::env::var("VIVAD_PERF_LOG").is_ok().then(|| {
                [bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS, bevy::diagnostic::EntityCountDiagnosticsPlugin::ENTITY_COUNT]
                    .into_iter()
                    .collect()
            }).or(Some(Default::default())),
            ..default()
        })
        .run();
}
