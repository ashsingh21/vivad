pub mod autoshot;
pub mod buildings;
pub mod camera;
pub mod characters;
pub mod hud;
pub mod input;
pub mod studio_env;
pub mod visuals;

/// Repo-level assets/ regardless of how the binary is launched.
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
