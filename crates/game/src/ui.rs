//! The screens' look, shared by the start screen (crate::menu), the loading screen and the lobby:
//! Poppins (assets/fonts, OFL) in three weights, and the colours: ink, the title's gold, saffron
//! for the main button, glass for the rest.

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load);
    }
}

#[derive(Resource, Clone)]
pub struct UiFonts {
    pub bold: Handle<Font>,
    pub semi: Handle<Font>,
    pub body: Handle<Font>,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(UiFonts {
        bold: assets.load("fonts/Poppins-ExtraBold.ttf"),
        semi: assets.load("fonts/Poppins-SemiBold.ttf"),
        body: assets.load("fonts/Poppins-Regular.ttf"),
    });
}

pub const INK: Color = Color::srgba(0.03, 0.035, 0.06, 0.94);
pub const GOLD: Color = Color::srgb(1.0, 0.82, 0.42);
pub const MUTED: Color = Color::srgb(0.74, 0.77, 0.84);
pub const SAFFRON: Color = Color::srgb(0.96, 0.52, 0.12);
pub const SAFFRON_HOVER: Color = Color::srgb(1.0, 0.64, 0.25);
pub const GLASS: Color = Color::srgba(1.0, 1.0, 1.0, 0.08);
pub const GLASS_HOVER: Color = Color::srgba(1.0, 1.0, 1.0, 0.16);
pub const FIELD: Color = Color::srgba(1.0, 1.0, 1.0, 0.07);
pub const EDGE: Color = Color::srgba(1.0, 1.0, 1.0, 0.18);

/// Text in one of the fonts, `size` pixels at a 900 px tall window, scaled by `k`; never under
/// 11 px (a phone's small labels stay readable).
pub fn text(font: &Handle<Font>, s: impl Into<String>, size: f32, k: f32, c: Color) -> (Text, TextFont, TextColor, Pickable) {
    let px = (size * k).max(size.min(11.0));
    (Text::new(s), TextFont { font: bevy::text::FontSource::Handle(font.clone()), font_size: FontSize::Px(px), ..default() }, TextColor(c), Pickable::IGNORE)
}

/// The scale everything is drawn at: the window's height against 900 px (a phone's is smaller).
pub fn scale(window: &Window) -> f32 {
    (window.height() / 900.0).clamp(0.55, 1.6)
}
