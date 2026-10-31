//! Vivad client: the shared simulation plus presentation and local input.

use avian3d::prelude::*;
use bevy::prelude::*;
use game::{animation::WarriorAnimationPlugin, arms::ArmsPlugin, fx::FxPlugin, grass::GrassPlugin, audio::SoundPlugin, autoshot::AutoshotPlugin, buildings::BuildingsPlugin, camera::CameraPlugin, characters::CharacterArtPlugin, hud::HudPlugin, input::InputPlugin, lamps::LampsPlugin, visuals::VisualsPlugin, water::WaterPlugin, quality::QualityPlugin, ASSET_DIR};
use shared::sim::{MatchConfig, SimPlugin};

/// A clone without Git LFS has text placeholders where the models and textures should be: the
/// game would start with an empty world (nothing textured ever loads). Say so plainly instead.
#[cfg(not(target_arch = "wasm32"))]
fn check_lfs() {
    let probe = format!("{ASSET_DIR}/models/characters/male.glb");
    if std::fs::read(&probe).is_ok_and(|b| b.starts_with(b"version https://git-lfs")) {
        eprintln!("\nThe game's models and textures haven't been downloaded: they're Git LFS placeholders.");
        eprintln!("In the vivad folder run:\n\n    git lfs install\n    git lfs pull\n\nthen start the game again.\n");
        std::process::exit(1);
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    check_lfs();
    // Built in, so the browser build needn't fetch it before the app starts.
    let items_ron = include_str!("../../../assets/data/items.ron").to_string();
    let bots = std::env::var("VIVAD_BOTS").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    let autopilot = std::env::var("VIVAD_AUTOPILOT").is_ok();
    let mut config = MatchConfig { bots, autopilot, ..default() };
    let mut app = App::new();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(lobby) = online(&mut config) {
        app.insert_resource(lobby);
    }
    app.insert_resource(config)
        .add_plugins(
            {
                let plugins = DefaultPlugins.build();
                // Single-threaded (VIVAD_THREADS=1) also means no render thread, as in the browser.
                #[cfg(not(target_arch = "wasm32"))]
                let plugins = if std::env::var("VIVAD_THREADS").as_deref() == Ok("1") {
                    plugins.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
                } else {
                    plugins
                };
                plugins
            }
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Panga".into(),
                        resolution: (1600u32, 900u32).into(),
                        // VIVAD_NO_VSYNC=1 uncaps the frame rate, for measuring.
                        present_mode: if std::env::var("VIVAD_NO_VSYNC").is_ok() { bevy::window::PresentMode::AutoNoVsync } else { default() },
                        // In the browser: fill the page, and keep the keys and mouse for the game.
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                })
                // No .meta files beside the assets: the browser would fetch (and 404) one for each.
                // VIVAD_THREADS=1 runs everything on one thread, as the browser does, for measuring.
                .set(TaskPoolPlugin {
                    task_pool_options: match std::env::var("VIVAD_THREADS").ok().and_then(|s| s.parse().ok()) {
                        Some(n) => TaskPoolOptions::with_num_threads(n),
                        None => TaskPoolOptions::default(),
                    },
                })
                .set(AssetPlugin { file_path: ASSET_DIR.into(), meta_check: bevy::asset::AssetMetaCheck::Never, ..default() }),
        )
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron })
        .add_plugins((InputPlugin, VisualsPlugin, ArmsPlugin, FxPlugin, GrassPlugin, LampsPlugin, WarriorAnimationPlugin, SoundPlugin, BuildingsPlugin, CharacterArtPlugin, CameraPlugin, HudPlugin, AutoshotPlugin, WaterPlugin))
        .add_plugins((QualityPlugin, game::batch::BatchPlugin))
        .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin {
            wait_duration: std::time::Duration::from_secs(10),
            filter: std::env::var("VIVAD_PERF_ALL").is_ok().then(|| None).unwrap_or_else(|| std::env::var("VIVAD_PERF_LOG").is_ok().then(|| {
                [bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS, bevy::diagnostic::EntityCountDiagnosticsPlugin::ENTITY_COUNT]
                    .into_iter()
                    .collect()
            }).or(Some(Default::default()))),
            ..default()
        });
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins((net::TransportPlugin, game::lobby::LobbyPlugin));
    // VIVAD_PERF_ALL=1: log every diagnostic, GPU time per render pass among them.
    if std::env::var("VIVAD_PERF_ALL").is_ok() {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    }
    app.run();
}

/// Online play: VIVAD_HOST=1 hosts a match (shows a join code; Enter starts it, or VIVAD_HOST=<n>
/// starts at n players counting you), with no bots unless VIVAD_BOTS says; VIVAD_JOIN=<code> joins one. VIVAD_NAME is what others see
/// you as, and VIVAD_LAN=1 skips iroh's relays (enough on one machine or a LAN; the code is then
/// longer, carrying addresses). The window opens on a lobby screen meanwhile.
#[cfg(not(target_arch = "wasm32"))]
fn online(config: &mut MatchConfig) -> Option<net::Lobby> {
    let host = std::env::var("VIVAD_HOST").ok();
    let join = std::env::var("VIVAD_JOIN").ok().filter(|c| !c.is_empty());
    if host.is_none() && join.is_none() {
        return None;
    }
    let name = std::env::var("VIVAD_NAME").ok().filter(|n| !n.is_empty()).or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "Warrior".into());
    let relay = std::env::var("VIVAD_LAN").is_err();
    config.online = true;
    Some(match join {
        Some(code) => {
            // The code names the map, so it is built before the match starts.
            let (_, map_seed) = net::read_code(&code).unwrap_or_else(|e| panic!("VIVAD_JOIN: {e}"));
            config.map_seed = map_seed;
            net::Lobby::join(name, relay, code)
        }
        None => {
            let players = host.and_then(|h| h.parse::<usize>().ok()).filter(|n| *n > 1);
            // Online, players fight each other: no bots unless the host asks (VIVAD_BOTS=<n>).
            let bots = std::env::var("VIVAD_BOTS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            // Across the internet the match gets a room code to read out; its map seed comes with it.
            let room = relay.then(net::new_room);
            if let Some(r) = &room {
                config.map_seed = net::room_map_seed(r);
            }
            net::Lobby::host(name, relay, room, players, bots, config.map_seed)
        }
    })
}
