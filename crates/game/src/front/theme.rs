//! The front door's look: truck-art colours, three fonts and a painted-board button.
//!
//! The button (a `Button` with a `Hovered`, and one colour per state) follows bevy_new_2d's
//! `theme` (CC0, MIT or Apache 2.0) and bevy_game_template's `menu.rs` (CC0).
//! Sizes are given in pixels of a 1600x900 window and scale with the window's height (`u`, `size`).

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::Button;

use super::Press;

pub const RED: Color = Color::srgb(0.784, 0.157, 0.118);
pub const MARIGOLD: Color = Color::srgb(0.965, 0.659, 0.110);
pub const MUSTARD: Color = Color::srgb(0.957, 0.776, 0.310);
pub const TEAL: Color = Color::srgb(0.059, 0.431, 0.431);
pub const BLUE: Color = Color::srgb(0.090, 0.259, 0.478);
pub const CREAM: Color = Color::srgb(0.984, 0.945, 0.863);
pub const INK: Color = Color::srgb(0.106, 0.078, 0.063);
const SCRIM: Color = Color::srgba(0.078, 0.047, 0.031, 0.88);

pub const GHOST: Color = Color::srgba(0.106, 0.078, 0.063, 0.86);

/// `x` pixels of a 900-high window.
pub fn u(x: f32) -> Val {
    Val::Vh(x / 9.0)
}

fn size(x: f32) -> FontSize {
    FontSize::Vh(x / 9.0)
}

/// Mukta for what is read, Gajraj One for titles, Khand for numbers (assets/fonts, all OFL).
#[derive(Resource)]
pub struct Fonts {
    pub body: Handle<Font>,
    pub title: Handle<Font>,
    pub number: Handle<Font>,
}

impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self { body: assets.load("fonts/Mukta-Bold.ttf"), title: assets.load("fonts/GajrajOne-Regular.ttf"), number: assets.load("fonts/Khand-Bold.ttf") }
    }
}

/// A line of text. It takes no clicks, so what it sits on gets them.
pub fn line(s: impl Into<String>, font: &Handle<Font>, px: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font: font.clone().into(), font_size: size(px), ..default() }, TextColor(color), Pickable::IGNORE)
}

/// A title with the sign painter's offset shadow.
pub fn painted(s: impl Into<String>, font: &Handle<Font>, px: f32, color: Color) -> impl Bundle {
    (line(s, font, px, color), TextShadow { offset: Vec2::splat(px / 24.0), color: RED })
}

/// A whole-window layer. Clicks pass through it to what is under it.
pub fn layer(z: i32) -> impl Bundle {
    (Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(z), Pickable::IGNORE)
}

/// Darkens the left of the screen so text reads on any map.
pub fn scrim_left() -> impl Bundle {
    BackgroundGradient::from(LinearGradient::to_right(vec![
        ColorStop::new(SCRIM, percent(0)),
        ColorStop::new(SCRIM.with_alpha(0.74), percent(30)),
        ColorStop::new(SCRIM.with_alpha(0.12), percent(56)),
        ColorStop::new(SCRIM.with_alpha(0.0), percent(70)),
    ]))
}

/// The bottom-right corner, for Home and the result card alike: the big button never moves.
pub fn corner() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            right: u(64.0),
            bottom: u(72.0),
            width: u(560.0),
            flex_direction: FlexDirection::Column,
            row_gap: u(14.0),
            ..default()
        },
        Pickable::IGNORE,
    )
}

/// A painted tin sign: `paper` with a black edge, a thin inner line and a hard shadow.
pub fn sign(paper: Color, trim: Color, padding: UiRect) -> impl Bundle {
    (
        Node { border: UiRect::all(u(5.0)), padding, flex_direction: FlexDirection::Column, ..default() },
        BackgroundColor(paper),
        BorderColor::all(INK),
        Outline { width: u(5.0), offset: u(-14.0), color: trim },
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.65), u(9.0), u(9.0), Val::ZERO, Val::ZERO),
        Pickable::IGNORE,
    )
}

/// The key that also presses a button.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shortcut {
    Enter,
    Esc,
}

impl Shortcut {
    fn label(self) -> &'static str {
        match self {
            Shortcut::Enter => "Enter",
            Shortcut::Esc => "Esc",
        }
    }
}

/// A button's colours when idle and with the pointer on it.
#[derive(Component)]
pub struct Paint {
    idle: Color,
    hovered: Color,
}

/// The one big red button.
pub fn big_button(p: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, press: Press, key: Shortcut) -> Entity {
    let node = Node {
        width: percent(100),
        padding: UiRect::axes(u(26.0), u(18.0)),
        border: UiRect::all(u(5.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        ..default()
    };
    let tilt = UiTransform { rotation: Rot2::degrees(-1.2), ..default() };
    let trim = Outline { width: u(4.0), offset: u(3.0), color: MUSTARD };
    let mut b = p.spawn((board(node, RED, Color::srgb(0.88, 0.22, 0.16), press), tilt, trim));
    b.with_children(|b| {
        b.spawn(line(label, &fonts.title, 50.0, MUSTARD));
        chip(b, fonts, key);
    })
    .insert(key);
    b.id()
}

/// A smaller board. `key` adds the key that also presses it.
pub fn small_button(p: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, press: Press, paper: Color, key: Option<Shortcut>) -> Entity {
    let node = Node {
        flex_grow: 1.0,
        padding: UiRect::axes(u(20.0), u(11.0)),
        border: UiRect::all(u(4.0)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        ..default()
    };
    let hovered = paper.with_alpha(1.0).lighter(0.07);
    let mut b = p.spawn(board(node, paper, hovered, press));
    b.with_children(|b| {
        b.spawn(line(label, &fonts.title, 27.0, CREAM));
        if let Some(key) = key {
            chip(b, fonts, key);
        }
    });
    if let Some(key) = key {
        b.insert(key);
    }
    b.id()
}

fn board(node: Node, paper: Color, hovered: Color, press: Press) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        press,
        Paint { idle: paper, hovered },
        node,
        BackgroundColor(paper),
        BorderColor::all(INK),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.7), u(6.0), u(6.0), Val::ZERO, Val::ZERO),
    )
}

/// The little cream key cap on a button.
fn chip(b: &mut ChildSpawnerCommands, fonts: &Fonts, key: Shortcut) {
    b.spawn((
        Node { padding: UiRect::axes(u(9.0), u(1.0)), border: UiRect::all(u(2.0)), border_radius: BorderRadius::all(u(5.0)), ..default() },
        BackgroundColor(CREAM),
        BorderColor::all(INK),
        Pickable::IGNORE,
    ))
    .with_child(line(key.label(), &fonts.body, 18.0, INK));
}

/// Keeps each button's colour with its state: hovered, pressed, or not yet ready to press.
#[allow(clippy::type_complexity)]
pub(super) fn paint_buttons(mut buttons: Query<(&Hovered, Has<Pressed>, Has<InteractionDisabled>, &Paint, &mut BackgroundColor), With<Button>>) {
    for (hovered, pressed, off, paint, mut bg) in &mut buttons {
        let c = if off {
            paint.idle.with_alpha(0.5)
        } else if pressed {
            paint.idle.darker(0.06)
        } else if hovered.get() {
            paint.hovered
        } else {
            paint.idle
        };
        bg.set_if_neq(BackgroundColor(c));
    }
}

/// The note a button's press makes (bevy_new_2d's button_click.ogg, CC0).
#[derive(Resource)]
pub(super) struct ClickSound(pub Handle<AudioSource>);

impl FromWorld for ClickSound {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource::<AssetServer>().load("audio/ui_click.ogg"))
    }
}
