//! The lobby screen for online play: the join code to share, who has joined, and (hosting) Enter
//! to start; then a wait while every player loads the match.

use bevy::prelude::*;
use net::Lobby;
use shared::sim::Hold;

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

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Screen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
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
                Text::new(""),
                TextFont { font_size: FontSize::Px(26.0), ..default() },
                TextColor(Color::srgb(1.0, 0.85, 0.5)),
                TextLayout::justify(Justify::Center),
                Node { max_width: percent(80.0), ..default() },
            ));
        });
}

fn show(
    lobby: Res<Lobby>,
    hold: Res<Hold>,
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: Query<&mut Visibility, With<Screen>>,
    mut text: Query<&mut Text, With<ScreenText>>,
) {
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
