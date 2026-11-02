//! The lobby screen for online play: the join code to share, who has joined, and (hosting) Enter
//! to start; then a wait while every player loads the match. Home is always one press away, and
//! after an error there is another try.

use bevy::prelude::*;
use net::Lobby;
use shared::sim::net::{Departures, Heard, Inbox, Outbox, Peers};
use shared::sim::{Hold, MatchConfig};

use crate::front::Press;
use crate::front::theme::{self, Fonts, GHOST, MUSTARD, Shortcut, TEAL, u};

const RETRY: &str = "Phir try karo";
const HOME: &str = "Ghar";

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn.run_if(resource_exists::<Lobby>))
            .add_systems(Update, show.run_if(resource_exists::<Lobby>));
    }
}

#[derive(Component)]
struct Screen;
#[derive(Component)]
struct ScreenText;
#[derive(Component)]
struct RetryButton;

/// This run's online session: how it is gathered, and what a solo match has without it.
#[derive(Resource)]
pub struct Online {
    pub gather: Box<dyn Fn() -> Lobby + Send + Sync>,
    pub solo_bots: u32,
}

/// Gathers the session again, after an error.
pub fn retry(world: &mut World) {
    let lobby = (world.resource::<Online>().gather)();
    world.insert_resource(lobby);
}

/// Out of the online session, if there is one: the game is a solo one again. Dropping the
/// transport closes our links, so the others see us go at once.
pub fn leave_online(world: &mut World) {
    if !world.resource::<MatchConfig>().online {
        return;
    }
    world.remove_resource::<Lobby>();
    world.remove_resource::<net::Transport>();
    world.insert_resource(Peers::default());
    world.resource_mut::<Inbox>().0.clear();
    world.resource_mut::<Outbox>().0.clear();
    world.resource_mut::<Departures>().0.clear();
    world.resource_mut::<Heard>().0.clear();
    let screens: Vec<Entity> = world.query_filtered::<Entity, With<Screen>>().iter(world).collect();
    for e in screens {
        world.despawn(e);
    }
    let bots = world.resource::<Online>().solo_bots;
    let mut config = world.resource_mut::<MatchConfig>();
    config.online = false;
    config.match_seed = None;
    config.bots = bots;
}

fn spawn(mut commands: Commands, fonts: Res<Fonts>) {
    commands
        .spawn((
            Screen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: u(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.03, 0.82)),
            GlobalZIndex(10),
        ))
        .with_children(|p| {
            p.spawn((
                ScreenText,
                theme::line("", &fonts.body, 26.0, MUSTARD),
                TextLayout::justify(Justify::Center),
                Node { max_width: percent(80.0), ..default() },
            ));
            p.spawn((Node { width: u(560.0), column_gap: u(14.0), ..default() }, Pickable::IGNORE)).with_children(|row| {
                let retry = theme::small_button(row, &fonts, RETRY, Press::Retry, TEAL, None);
                row.commands().entity(retry).insert(RetryButton);
                theme::small_button(row, &fonts, HOME, Press::Ghar, GHOST, Some(Shortcut::Esc));
            });
        });
}

fn show(
    lobby: Res<Lobby>,
    hold: Res<Hold>,
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: Query<&mut Visibility, With<Screen>>,
    mut text: Query<&mut Text, With<ScreenText>>,
    mut retry: Query<&mut Node, With<RetryButton>>,
) {
    let display = if lobby.error.is_some() { Display::Flex } else { Display::None };
    for mut n in &mut retry {
        if n.display != display {
            n.display = display;
        }
    }
    if lobby.hosting && !lobby.started && keys.just_pressed(KeyCode::Enter) {
        lobby.start_now();
    }
    let players = if lobby.players.is_empty() { String::new() } else { format!("Players ({}): {}", lobby.players.len(), lobby.players.join(", ")) };
    let msg = if let Some(e) = &lobby.error {
        Some(format!("Could not play online\n\n{e}"))
    } else if lobby.started {
        hold.0.then(|| format!("Waiting for everyone to load the match\n\n{players}"))
    } else if lobby.hosting {
        // Broken into lines: a LAN code (which carries addresses) is too long for one.
        let code = lobby.code.as_ref().map(|c| c.as_bytes().chunks(50).map(|l| String::from_utf8_lossy(l)).collect::<Vec<_>>().join("\n"));
        let code = code.as_deref().unwrap_or("(going online...)");
        Some(format!("Hosting a match\n\nRoom code: share it with your friends\n{code}\n\n{players}\n\nPress Enter to start"))
    } else {
        let waiting = if lobby.players.is_empty() { "Reaching the host..." } else { "Waiting for the host to start" };
        Some(format!("Joining a match\n\n{players}\n\n{waiting}"))
    };
    for mut v in &mut screen {
        *v = if msg.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Some(msg) = msg {
        for mut t in &mut text {
            if t.0 != msg {
                t.0 = msg.clone();
            }
        }
    }
}
