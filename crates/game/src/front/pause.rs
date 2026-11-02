//! Esc in a match. Alone, the match really stops; online it runs on behind the menu.
//! Follows bevy_new_2d's `menus/pause.rs` (CC0, MIT or Apache 2.0), with the sim's `Hold` for its pause.

use bevy::prelude::*;
use shared::sim::{Hold, LocalPlayer, MatchConfig, PlayerIntent};

use super::theme::{self, Fonts, GHOST, MUSTARD, Shortcut, u};
use super::{Press, Screen};
use crate::input::{Act, ActionState};

const TITLE: &str = "RUKO!";
const RESUME: &str = "WAPAS CHALO";
const LEAVE: &str = "Match chhodo";
const QUIT: &str = "Bahar niklo";

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Pause), (spawn, stop))
        .add_systems(OnExit(Screen::Pause), go_on)
        .add_systems(Update, open.run_if(in_state(Screen::Match)));
}

/// While everyone loads the match, Esc is the lobby's way home.
fn open(actions: Res<ActionState>, hold: Res<Hold>, mut next: ResMut<NextState<Screen>>) {
    if actions.just_pressed(Act::Menu) && !hold.0 {
        next.set(Screen::Pause);
    }
}

fn spawn(mut commands: Commands, fonts: Res<Fonts>) {
    commands.spawn((theme::layer(5), BackgroundColor(Color::srgba(0.078, 0.047, 0.031, 0.55)), DespawnOnExit(Screen::Pause))).with_children(|p| {
        p.spawn((
            Node { position_type: PositionType::Absolute, left: u(72.0), top: u(96.0), width: u(560.0), flex_direction: FlexDirection::Column, row_gap: u(15.0), ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|c| {
            c.spawn(theme::painted(TITLE, &fonts.title, 74.0, MUSTARD));
            theme::big_button(c, &fonts, RESUME, Press::Resume, Shortcut::Esc);
            theme::small_button(c, &fonts, LEAVE, Press::Ghar, GHOST, None);
            #[cfg(not(target_arch = "wasm32"))]
            theme::small_button(c, &fonts, QUIT, Press::Quit, GHOST, None);
        });
    });
}

fn stop(config: Res<MatchConfig>, mut hold: ResMut<Hold>) {
    if !config.online {
        hold.0 = true;
    }
}

/// Keys pressed in the menu were not meant for the warrior.
fn go_on(config: Res<MatchConfig>, mut hold: ResMut<Hold>, mut intent: Query<&mut PlayerIntent, With<LocalPlayer>>) {
    if !config.online {
        hold.0 = false;
    }
    for mut i in &mut intent {
        i.pressed = 0;
        i.released = 0;
        i.select = None;
    }
}
