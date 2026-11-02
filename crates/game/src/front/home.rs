//! Home: the live map turning behind the name of the game, and one big button.

use bevy::prelude::*;
use shared::map::{MapLayout, MapStyle, Poi};

use super::theme::{self, CREAM, Fonts, GHOST, MUSTARD, Shortcut, u};
use super::{Press, Screen};
use crate::camera::{CameraSystems, MainCamera};

const TAGLINE: &str = "Dilli ki galiyon mein last one standing.";
const PLAY: &str = "PANGA LO";
const QUIT: &str = "Bahar niklo";
const CREDITS: &str = "Credits";

/// Radians a second the camera turns round the map's main place.
const TURN: f32 = 0.05;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Home), spawn)
        .add_systems(PostUpdate, orbit.run_if(|screen: Res<State<Screen>>| matches!(screen.get(), Screen::Home | Screen::Credits)).after(CameraSystems).before(TransformSystems::Propagate));
}

fn spawn(mut commands: Commands, fonts: Res<Fonts>, layout: Res<MapLayout>) {
    commands.spawn((theme::layer(5), theme::scrim_left(), DespawnOnExit(Screen::Home))).with_children(|p| {
        p.spawn((Node { position_type: PositionType::Absolute, left: u(72.0), top: u(64.0), flex_direction: FlexDirection::Column, ..default() }, Pickable::IGNORE))
            .with_children(|c| {
                c.spawn(theme::painted("PANGA", &fonts.title, 124.0, MUSTARD));
                c.spawn(theme::painted("पंगा", &fonts.title, 58.0, CREAM));
                c.spawn(theme::line(TAGLINE, &fonts.body, 25.0, CREAM));
            });
        #[cfg(not(target_arch = "wasm32"))]
        p.spawn((Node { position_type: PositionType::Absolute, left: u(72.0), bottom: u(72.0), width: u(360.0), flex_direction: FlexDirection::Column, row_gap: u(12.0), ..default() }, Pickable::IGNORE)).with_children(|c| {
            theme::small_button(c, &fonts, CREDITS, Press::Credits, GHOST, None);
            theme::small_button(c, &fonts, QUIT, Press::Quit, GHOST, None);
        });
        p.spawn(theme::corner()).with_children(|c| {
            // The map, on a plate of its own: the map pick goes here when there are two.
            c.spawn(theme::sign(theme::BLUE, MUSTARD, UiRect::axes(u(26.0), u(12.0))))
                .with_child(theme::line(layout.name.clone(), &fonts.title, 27.0, CREAM));
            theme::big_button(c, &fonts, PLAY, Press::Play, Shortcut::Enter);
        });
    });
}

/// The widest place, on a small map. The NCR lists its districts first, in order of fame, and the
/// widest there would be a bridge: half a bridge's length is what counts as its radius.
fn main_place(layout: &MapLayout) -> Option<&Poi> {
    match layout.style {
        MapStyle::Ncr => layout.pois.first(),
        MapStyle::City => layout.pois.iter().max_by(|a, b| a.radius.total_cmp(&b.radius)),
    }
}

/// With no match there is no one to follow: the camera circles the map's main place from above.
fn orbit(time: Res<Time>, layout: Res<MapLayout>, mut cam: Query<&mut Transform, With<MainCamera>>) {
    let Ok(mut t) = cam.single_mut() else { return };
    let (focus, reach) = main_place(&layout).map_or((Vec3::ZERO, layout.half_extent), |p| (p.center, p.radius));
    let reach = reach.clamp(60.0, 240.0);
    let a = time.elapsed_secs() * TURN;
    let at = focus + Vec3::new(a.cos(), 0.0, a.sin()) * reach * 1.15;
    let ground = layout.terrain.height(at.x, at.z);
    *t = Transform::from_xyz(at.x, ground + reach * 0.34, at.z).looking_at(focus.with_y(ground + reach * 0.08), Vec3::Y);
}

#[cfg(test)]
mod tests {
    use shared::map::{MapId, compose, generate};

    use super::main_place;

    #[test]
    fn home_looks_at_a_district_of_the_ncr_not_a_bridge() {
        let layout = generate(MapId::Ncr, 7);
        assert_eq!(main_place(&layout).map(|p| p.name.as_str()), Some(compose::PLACES[0].name));
    }
}
