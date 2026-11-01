pub mod ambient;
pub mod animation;
pub mod arms;
pub mod audio;
pub mod batch;
pub mod autoshot;
pub mod buildings;
pub mod camera;
pub mod fx;
pub mod grass;
pub mod ground;
pub mod characters;
pub mod hud;
pub mod input;
pub mod lamps;
pub mod quality;
pub mod rim;
pub mod sky;
pub mod street;
pub mod skyline;
pub mod lobby;
pub mod studio_env;
pub mod visuals;

/// Repo-level assets/ regardless of how the binary is launched; in the browser, `assets/` beside
/// the page.
#[cfg(not(target_arch = "wasm32"))]
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
#[cfg(target_arch = "wasm32")]
pub const ASSET_DIR: &str = "assets";
pub mod water;
