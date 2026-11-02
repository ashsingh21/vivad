//! The front door: Home, the pause menu and the result card round a match, so the game opens on
//! something and never ends on a dead end.
//!
//! `Screen` says what is up; the sim knows nothing of it. A first match starts the way a rematch
//! does, with `RestartMatch`, and the screen follows `Match::generation`, so a peer's restart or a
//! test's moves it too. The state layout follows bevy_new_2d's `screens` and `menus` (CC0, MIT or
//! Apache 2.0). Test recipes, online play and the browser (whose page is its menu) skip Home.

mod demo;
mod home;
mod pause;
mod result;
pub mod theme;

use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use bevy::window::{CursorGrabMode, CursorOptions};
use shared::items::ItemDb;
use shared::map::MapLayout;
use shared::sim::flow::Match;
use shared::sim::loot::{Chest, Pickup, census, census_lines};
use shared::sim::{Hold, MatchConfig, RestartMatch};

use crate::input::{Act, ActionState, LocalInput};
use theme::Shortcut;

#[derive(States, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Screen {
    #[default]
    Home,
    /// Online, gathering players (crate::lobby).
    Lobby,
    Match,
    Pause,
    Result,
}

/// What a button does.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Into a match: the first, or the next.
    Play,
    Resume,
    /// Home, leaving the match (and the session, online).
    Ghar,
    Quit,
    /// Try the online session again.
    Retry,
}

/// The test recipes go straight to a match, and so does VIVAD_SKIP_HOME=1.
pub fn skip_home() -> bool {
    const STRAIGHT_IN: [&str; 14] = [
        "VIVAD_AUTOPILOT",
        "VIVAD_TOUR",
        "VIVAD_SHOTS",
        "VIVAD_CLIP",
        "VIVAD_ARMOURY",
        "VIVAD_CLEAN_CAPTURE",
        "VIVAD_SKIP_HOME",
        "VIVAD_STAGE",
        "VIVAD_SHOW",
        "VIVAD_DRIVE",
        "VIVAD_KO",
        "VIVAD_SPREE",
        "VIVAD_WHEEL_AT",
        "VIVAD_BRAWL",
    ];
    cfg!(target_arch = "wasm32") || STRAIGHT_IN.iter().any(|k| std::env::var(k).is_ok())
}

pub struct FrontPlugin;

impl Plugin for FrontPlugin {
    fn build(&self, app: &mut App) {
        let first = if app.world().contains_resource::<net::Lobby>() {
            Screen::Lobby
        } else if app.world().resource::<MatchConfig>().menu {
            Screen::Home
        } else {
            Screen::Match
        };
        app.insert_state(first)
            .init_resource::<theme::Fonts>()
            .init_resource::<theme::ClickSound>()
            .add_plugins((home::plugin, pause::plugin, result::plugin, demo::plugin))
            .add_observer(on_button)
            .add_systems(PreUpdate, free_mouse.after(LocalInput).run_if(not(in_state(Screen::Match))))
            .add_systems(Update, (follow_the_match, shortcuts, show_hud, theme::paint_buttons));
        if std::env::var("VIVAD_LOOT_LOG").is_ok() {
            app.add_systems(Update, loot_log);
        }
        for exited in [Screen::Home, Screen::Lobby, Screen::Pause, Screen::Result] {
            app.add_systems(OnTransition { exited, entered: Screen::Match }, capture_mouse.run_if(not(resource_exists::<demo::Demo>)));
        }
    }
}

fn on_button(on: On<Activate>, presses: Query<&Press>, click: Res<theme::ClickSound>, mut commands: Commands) {
    let Ok(&what) = presses.get(on.entity) else { return };
    commands.spawn((AudioPlayer(click.0.clone()), PlaybackSettings::DESPAWN));
    commands.queue(move |world: &mut World| press(world, what));
}

fn press(world: &mut World, what: Press) {
    match what {
        Press::Play => {
            world.write_message(RestartMatch);
        }
        Press::Resume => world.resource_mut::<NextState<Screen>>().set(Screen::Match),
        Press::Ghar => {
            crate::lobby::leave_online(world);
            shared::sim::flow::leave_match(world);
            world.resource_mut::<Hold>().0 = false;
            world.resource_mut::<NextState<Screen>>().set(Screen::Home);
        }
        Press::Quit => {
            world.write_message(AppExit::Success);
        }
        Press::Retry => crate::lobby::retry(world),
    }
}

/// Enter and Esc press the button that shows them.
fn shortcuts(actions: Res<ActionState>, buttons: Query<(&Press, &Shortcut, &InheritedVisibility), Without<InteractionDisabled>>, mut commands: Commands) {
    for (&what, key, shown) in &buttons {
        let act = if *key == Shortcut::Enter { Act::Restart } else { Act::Menu };
        if shown.get() && actions.just_pressed(act) {
            commands.queue(move |world: &mut World| press(world, what));
        }
    }
}

/// A new match has begun: whatever was up gives way to it.
fn follow_the_match(m: Res<Match>, screen: Res<State<Screen>>, mut next: ResMut<NextState<Screen>>, mut seen: Local<u32>) {
    if m.generation != *seen {
        *seen = m.generation;
        if *screen.get() != Screen::Match {
            next.set(Screen::Match);
        }
    }
}

/// The HUD is up only in a match. Its pieces are the root nodes, all but the 3D view and the
/// full-screen layers (those have a `GlobalZIndex`), so what other parts add to it follows along.
fn show_hud(screen: Res<State<Screen>>, mut roots: Query<&mut Node, (Without<ChildOf>, Without<GlobalZIndex>)>) {
    let display = if *screen.get() == Screen::Match { Display::Flex } else { Display::None };
    for mut n in &mut roots {
        if n.display != display {
            n.display = display;
        }
    }
}

/// VIVAD_LOOT_LOG=1: what lies out at the start of each match, by place.
fn loot_log(world: &mut World, mut seen: Local<u32>) {
    let m = world.resource::<Match>();
    let (generation, seed) = (m.generation, m.seed);
    if generation == *seen {
        return;
    }
    let floor: Vec<(Vec3, String)> = world.query::<(&Transform, &Pickup)>().iter(world).map(|(t, p)| (t.translation, p.stack.id.clone())).collect();
    let chests: Vec<_> = world.query::<&Chest>().iter(world).map(|c| (c.index, c.kind)).collect();
    if floor.is_empty() {
        return;
    }
    *seen = generation;
    let (layout, db) = (world.resource::<MapLayout>(), world.resource::<ItemDb>());
    for line in census_lines(&census(layout, db, seed, floor.into_iter(), chests.into_iter())) {
        println!("loot {line}");
    }
}

/// crate::input captures the mouse on a click, but outside a match a click is for a button.
fn free_mouse(mut cursor: Single<&mut CursorOptions>) {
    if cursor.grab_mode != CursorGrabMode::None {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Into the match from a menu: no click needed to start aiming. Test recipes leave the mouse alone.
fn capture_mouse(mut cursor: Single<&mut CursorOptions>, config: Res<MatchConfig>) {
    if !config.autopilot {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}
