pub mod ambient;
pub mod animation;
pub mod arms;
pub mod audio;
pub mod batch;
pub mod autoshot;
pub mod buildings;
pub mod camera;
pub mod feel;
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
pub mod loading;
pub mod diag;
pub mod menu;
pub mod posefix;
pub mod ui;
pub mod studio_env;
pub mod visuals;

/// Repo-level assets/ regardless of how the binary is launched; in the browser, `assets/` beside
/// the page.
#[cfg(not(target_arch = "wasm32"))]
pub const ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
#[cfg(target_arch = "wasm32")]
pub const ASSET_DIR: &str = "assets";

/// A texture's path: on the desktop its KTX2 where one is packed beside the PNG (GPU-compressed,
/// its mips made: tools/texpack, `just textures-pack`), else the PNG. A fifth to an eighth of the
/// memory. (The browser keeps the PNGs: a phone's GPU wants ASTC, not BC.)
pub fn tex(path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(stem) = path.strip_suffix(".png") {
        let ktx = format!("{stem}.ktx2");
        if std::path::Path::new(ASSET_DIR).join(&ktx).exists() {
            return ktx;
        }
    }
    path.to_string()
}
pub mod water;
