//! The lobby screen for online play, in the start screen's look (crate::ui): the room code to share
//! (with a button to copy it), who has joined, and (hosting) a button, or Enter, to start; then a
//! wait while every player loads the match.

use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use crate::ui::{EDGE, GLASS, GLASS_HOVER, GOLD, MUTED, SAFFRON, SAFFRON_HOVER};
use net::Lobby;
use shared::sim::Hold;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        // From launch, or once the start screen (crate::menu) opens a lobby.
        app.init_resource::<Copied>()
            .add_systems(Update, spawn.run_if(resource_added::<Lobby>))
            .add_systems(Update, (press, show).chain().run_if(resource_exists::<Lobby>));
    }
}

#[derive(Component)]
struct Screen;

/// The screen's lines, each set from the lobby's state.
#[derive(Component, Clone, Copy, PartialEq)]
enum Line {
    Title,
    About,
    Code,
    Players,
    Hint,
}

/// The two buttons, with their colour and their colour under the pointer.
#[derive(Component, Clone, Copy, PartialEq)]
enum Button {
    Copy,
    Start,
}

#[derive(Component)]
struct Look(Color, Color);

/// The code's tile (hidden when there's no code to show).
#[derive(Component)]
struct CodeTile;

/// When the code was last copied (for "Copied!").
#[derive(Resource, Default)]
struct Copied(Option<f32>);

fn spawn(mut commands: Commands, font: Res<crate::ui::UiFonts>, window: Single<&Window, With<PrimaryWindow>>) {
    let k = crate::ui::scale(&window);
    let z = |v: f32| px(v * k);
    let text = |f: &Handle<Font>, size: f32, c: Color| crate::ui::text(f, "", size, k, c);
    let button = |p: &mut ChildSpawnerCommands, b: Button, label: &str, look: Look| {
        p.spawn((
            b,
            Node { padding: UiRect::axes(z(26.0), z(12.0)), margin: UiRect::top(z(14.0)), border_radius: BorderRadius::all(z(12.0)), border: UiRect::all(z(1.0)), justify_content: JustifyContent::Center, ..default() },
            BackgroundColor(look.0),
            BorderColor::all(EDGE),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.35), z(0.0), z(4.0), z(0.0), z(12.0)),
            look,
        ))
        .with_children(|c| {
            c.spawn((b, crate::ui::text(&font.bold, label, 22.0, k, Color::WHITE)));
        });
    };
    commands
        .spawn((
            Screen,
            Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.045, 0.78)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: z(620.0),
                    max_width: percent(92.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(z(34.0)),
                    border_radius: BorderRadius::all(z(20.0)),
                    border: UiRect::all(z(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.07, 0.11, 0.92)),
                BorderColor::all(EDGE),
                BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), z(0.0), z(12.0), z(0.0), z(36.0)),
            ))
            .with_children(|p| {
                p.spawn((Line::Title, text(&font.bold, 40.0, Color::WHITE)));
                p.spawn((Line::About, text(&font.semi, 17.0, MUTED), Node { margin: UiRect::top(z(4.0)), ..default() }));
                p.spawn((
                    CodeTile,
                    Node { margin: UiRect::top(z(22.0)), padding: UiRect::axes(z(26.0), z(14.0)), border: UiRect::all(z(2.0)), border_radius: BorderRadius::all(z(14.0)), ..default() },
                    BackgroundColor(Color::srgba(1.0, 0.82, 0.42, 0.10)),
                    BorderColor::all(GOLD),
                ))
                .with_children(|c| {
                    c.spawn((Line::Code, text(&font.bold, 40.0, GOLD), TextLayout::justify(Justify::Center)));
                });
                button(p, Button::Copy, "Copy code", Look(GLASS, GLASS_HOVER));
                p.spawn((Line::Players, text(&font.semi, 20.0, Color::WHITE), Node { margin: UiRect::top(z(22.0)), ..default() }, TextLayout::justify(Justify::Center)));
                button(p, Button::Start, "Shuru karo", Look(SAFFRON, SAFFRON_HOVER));
                p.spawn((Line::Hint, text(&font.body, 15.0, MUTED), Node { margin: UiRect::top(z(14.0)), ..default() }, TextLayout::justify(Justify::Center)));
            });
        });
}

/// The buttons under the pointer or a finger (hit-tested as the start screen's are), and Enter.
#[allow(clippy::type_complexity)]
fn press(
    lobby: Res<Lobby>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut buttons: Query<(&Button, &Look, &ComputedNode, &UiGlobalTransform, &InheritedVisibility, &mut BackgroundColor)>,
    mut copied: ResMut<Copied>,
) {
    if lobby.hosting && !lobby.started && keys.just_pressed(KeyCode::Enter) {
        lobby.start_now();
    }
    let hover = window.physical_cursor_position();
    let tap = touches.iter_just_released().map(|f| f.position() * window.scale_factor()).next().or(hover.filter(|_| mouse.just_released(MouseButton::Left)));
    for (b, look, node, at, seen, mut bg) in &mut buttons {
        let hit = |p: Option<Vec2>| seen.get() && p.is_some_and(|p| node.contains_point(*at, p));
        let colour = if hit(hover) { look.1 } else { look.0 };
        if bg.0 != colour {
            bg.0 = colour;
        }
        if !hit(tap) {
            continue;
        }
        match b {
            Button::Start if lobby.hosting && !lobby.started => lobby.start_now(),
            Button::Copy => {
                if let Some(code) = &lobby.code {
                    copy(code);
                    copied.0 = Some(time.elapsed_secs());
                }
            }
            _ => {}
        }
    }
}

/// The code onto the clipboard (in the browser, navigator.clipboard).
fn copy(code: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(code));
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(code);
    }
}

#[allow(clippy::type_complexity)]
fn show(
    lobby: Res<Lobby>,
    hold: Res<Hold>,
    copied: Res<Copied>,
    time: Res<Time<Real>>,
    mut screen: Query<&mut Visibility, (With<Screen>, Without<CodeTile>, Without<Button>)>,
    mut tile: Query<&mut Node, With<CodeTile>>,
    mut buttons: Query<(&Button, &mut Node), (Without<CodeTile>, Without<Text>)>,
    mut labels: Query<(&Button, &mut Text), Without<Line>>,
    mut lines: Query<(&Line, &mut Text, &mut TextFont), Without<Button>>,
) {
    let players = if lobby.players.is_empty() { String::new() } else { format!("Players ({}): {}", lobby.players.len(), lobby.players.join(", ")) };
    // (title, about, code, players, hint), or nothing once the match is under way.
    let state: Option<(String, String, Option<String>, String, String)> = if let Some(e) = &lobby.error {
        Some(("Online nahi ho paaya".into(), "Could not play online".into(), None, String::new(), e.clone()))
    } else if lobby.started {
        hold.0.then(|| ("Match load ho raha hai".into(), "Waiting for everyone to load the match".into(), None, players.clone(), String::new()))
    } else if lobby.hosting {
        // Broken into lines: a LAN code (which carries addresses) is too long for one.
        let code = lobby.code.as_ref().map(|c| c.as_bytes().chunks(32).map(|l| String::from_utf8_lossy(l)).collect::<Vec<_>>().join("\n"));
        let about = if code.is_some() { "Yeh code doston ko bhejo (send this code to your friends)" } else { "Online ho rahe hain... (going online)" };
        Some(("Tumhara room".into(), about.into(), code, players.clone(), "Sab aa gaye? Shuru karo, ya Enter dabao".into()))
    } else {
        let waiting = if lobby.players.is_empty() { "Host tak pahunch rahe hain... (reaching the host)" } else { "Host shuru karega (waiting for the host to start)" };
        Some(("Room mein ho".into(), waiting.into(), None, players.clone(), String::new()))
    };
    for mut v in &mut screen {
        *v = if state.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    let Some((title, about, code, players, hint)) = state else { return };
    let host = lobby.hosting && !lobby.started && lobby.error.is_none();
    let display = |on: bool| if on { Display::Flex } else { Display::None };
    for mut n in &mut tile {
        n.display = display(code.is_some());
    }
    for (b, mut n) in &mut buttons {
        n.display = display(match b {
            Button::Copy => code.is_some(),
            Button::Start => host,
        });
    }
    let just_copied = copied.0.is_some_and(|t| time.elapsed_secs() - t < 2.0);
    for (b, mut t) in &mut labels {
        if *b == Button::Copy {
            let s = if just_copied { "Copied!" } else { "Copy code" };
            if t.0 != s {
                t.0 = s.into();
            }
        }
    }
    for (line, mut t, mut f) in &mut lines {
        let s = match line {
            Line::Title => title.clone(),
            Line::About => about.clone(),
            Line::Code => code.clone().unwrap_or_default(),
            Line::Players => players.clone(),
            Line::Hint => hint.clone(),
        };
        // A long (LAN) code smaller.
        if *line == Line::Code
            && code.as_ref().is_some_and(|c| c.len() > 40)
            && let FontSize::Px(p) = f.font_size
            && p > 26.0
        {
            f.font_size = FontSize::Px(p * 0.55);
        }
        if t.0 != s {
            t.0 = s;
        }
    }
}
