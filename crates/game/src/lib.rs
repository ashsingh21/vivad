pub mod ambient;
pub mod animation;
pub mod arms;
pub mod audio;
pub mod batch;
pub mod autoshot;
pub mod brawl;
pub mod buildings;
pub mod camera;
pub mod fonts;
pub mod front;
pub mod fx;
pub mod grass;
pub mod characters;
pub mod hud;
pub mod input;
pub mod lamps;
pub mod life;
pub mod quality;
pub mod rim;
pub mod say;
pub mod scope;
pub mod sky;
pub mod stage;
pub mod street;
pub mod street_sound;
pub mod skyline;
pub mod lobby;
pub mod studio_env;
pub mod ultra;
pub mod street_sample;
pub mod surfaces;
pub mod vehicle_feel;
pub mod vehicle_show;
pub mod vehicles;
pub mod visuals;
pub mod wheel;

/// Repo-level assets/ regardless of how the binary is launched; in the browser, `assets/` beside
/// the page.
#[cfg(not(target_arch = "wasm32"))]
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
#[cfg(target_arch = "wasm32")]
pub const ASSET_DIR: &str = "assets";
pub mod water;
