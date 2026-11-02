//! The game's three faces, all with Devanagari: Mukta for on-screen text, Gajraj One for titles,
//! Khand for numbers (SIL Open Font License, assets/fonts/). Built in, so the browser needn't
//! fetch them and no text ever shows in a font that lacks its letters.

use bevy::prelude::*;

#[derive(Resource)]
pub struct UiFonts {
    pub title: Handle<Font>,
    pub number: Handle<Font>,
}

pub struct FontsPlugin;

impl Plugin for FontsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load);
    }
}

fn load(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    // Text that names no font takes the default one: putting Mukta in its place covers it all.
    fonts.insert(AssetId::default(), Font::from_bytes(include_bytes!("../../../assets/fonts/Mukta-Bold.ttf").to_vec())).expect("the default font");
    commands.insert_resource(UiFonts {
        title: fonts.add(Font::from_bytes(include_bytes!("../../../assets/fonts/GajrajOne-Regular.ttf").to_vec())),
        number: fonts.add(Font::from_bytes(include_bytes!("../../../assets/fonts/Khand-SemiBold.ttf").to_vec())),
    });
}
