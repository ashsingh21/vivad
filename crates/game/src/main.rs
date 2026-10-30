//! Vivad client: the shared simulation plus presentation and local input.

use avian3d::prelude::*;
use bevy::prelude::*;
use game::{animation::WarriorAnimationPlugin, arms::ArmsPlugin, fx::FxPlugin, grass::GrassPlugin, audio::SoundPlugin, autoshot::AutoshotPlugin, buildings::BuildingsPlugin, camera::CameraPlugin, characters::CharacterArtPlugin, hud::HudPlugin, input::InputPlugin, lamps::LampsPlugin, visuals::VisualsPlugin, water::WaterPlugin, ASSET_DIR};
use shared::sim::{MatchConfig, SimPlugin};

fn main() {
    let items_ron = std::fs::read_to_string(format!("{ASSET_DIR}/data/items.ron")).expect("read items.ron");
    let bots = std::env::var("VIVAD_BOTS").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    let autopilot = std::env::var("VIVAD_AUTOPILOT").is_ok();
    let mut config = MatchConfig { bots, autopilot, ..default() };
    let mut app = App::new();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(session) = online(&config) {
        config.map_seed = session.map_seed;
        config.match_seed = Some(session.seed);
        config.bots = session.bots;
        app.insert_resource(session.peers).insert_resource(session.transport).add_plugins(net::TransportPlugin);
    }
    app.insert_resource(config)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Vivad".into(),
                        resolution: (1600u32, 900u32).into(),
                        // VIVAD_NO_VSYNC=1 uncaps the frame rate, for measuring.
                        present_mode: if std::env::var("VIVAD_NO_VSYNC").is_ok() { bevy::window::PresentMode::AutoNoVsync } else { default() },
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin { file_path: ASSET_DIR.into(), ..default() }),
        )
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron })
        .add_plugins((InputPlugin, VisualsPlugin, ArmsPlugin, FxPlugin, GrassPlugin, LampsPlugin, WarriorAnimationPlugin, SoundPlugin, BuildingsPlugin, CharacterArtPlugin, CameraPlugin, HudPlugin, AutoshotPlugin, WaterPlugin))
        .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin {
            wait_duration: std::time::Duration::from_secs(10),
            filter: std::env::var("VIVAD_PERF_ALL").is_ok().then(|| None).unwrap_or_else(|| std::env::var("VIVAD_PERF_LOG").is_ok().then(|| {
                [bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS, bevy::diagnostic::EntityCountDiagnosticsPlugin::ENTITY_COUNT]
                    .into_iter()
                    .collect()
            }).or(Some(Default::default()))),
            ..default()
        });
    // VIVAD_PERF_ALL=1: log every diagnostic, GPU time per render pass among them.
    if std::env::var("VIVAD_PERF_ALL").is_ok() {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
    app.run();
}

/// Online play: VIVAD_HOST=<players> prints a join code and waits for that many players (counting
/// you); VIVAD_JOIN=<code> joins that match. VIVAD_NAME is what others see you as, and VIVAD_LAN=1
/// skips iroh's relays (enough on one machine or a LAN). Blocks until the match starts.
#[cfg(not(target_arch = "wasm32"))]
fn online(config: &MatchConfig) -> Option<net::Session> {
    let host = std::env::var("VIVAD_HOST").ok().and_then(|s| s.parse::<usize>().ok());
    let join = std::env::var("VIVAD_JOIN").ok();
    if host.is_none() && join.is_none() {
        return None;
    }
    let name = std::env::var("VIVAD_NAME").ok().filter(|n| !n.is_empty()).or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "Warrior".into());
    let node = net::Node::bind(&name, std::env::var("VIVAD_LAN").is_err()).expect("start networking");
    let session = match (host, join) {
        (Some(players), _) => {
            println!("join code (VIVAD_JOIN=...):\n{}", node.code());
            println!("waiting for {} more player(s)...", players.saturating_sub(1));
            node.host(players, config.bots, config.map_seed)
        }
        (None, Some(code)) => {
            println!("joining...");
            node.join(&code)
        }
        _ => unreachable!(),
    };
    let session = session.unwrap_or_else(|e| panic!("could not join the match: {e}"));
    println!("match starting: {} players, seed {}", session.peers.all.len(), session.seed);
    Some(session)
}
