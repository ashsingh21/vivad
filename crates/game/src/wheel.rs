//! The preset line wheel. Hold T and a page of lines opens round the crosshair; push the mouse
//! towards one and let go to say it; scroll for the next page. Only lines from the file can be
//! said, so there is no typing and no open voice to moderate.
//!
//! VIVAD_WHEEL_AT=<t>,<page>,<slot>  opens the wheel by itself at that match time and says that
//! line two seconds later, for captures.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::text::LineBreak;
use shared::lines::Lines;
use shared::sim::flow::Match;
use shared::sim::net::{NetId, Outbox};
use shared::sim::talk::{SayMsg, say};
use shared::sim::{Eliminated, LocalPlayer};

use crate::fonts::UiFonts;
use crate::front::Screen;

/// The wheel is open: the mouse and the scroll wheel are its, not the camera's or the hotbar's.
#[derive(Resource, Default)]
pub struct WheelOpen(pub bool);

pub struct WheelPlugin;

impl Plugin for WheelPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("VIVAD_CLEAN_CAPTURE").is_ok() {
            return;
        }
        let script = std::env::var("VIVAD_WHEEL_AT").ok().and_then(|s| {
            let n: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            Some((*n.first()?, *n.get(1)? as usize, *n.get(2)? as usize))
        });
        app.insert_resource(Script(script))
            .init_resource::<State>()
            .add_systems(Startup, build)
            .add_systems(Update, (wheel.run_if(in_state(Screen::Match)), draw).chain())
            .add_systems(OnExit(Screen::Match), shut);
    }
}

/// The wheel's pages, by the moments of a fight.
const PAGES: [&str; 5] = ["dekho", "panga", "jeet", "haar", "masti"];
const SLOTS: usize = 8;
/// How far the slots stand from the centre, points; wide, because the lines are.
const REACH: Vec2 = Vec2::new(300.0, 190.0);
/// Mouse travel that picks a slot, and the most it counts for.
const DEAD_ZONE: f32 = 24.0;
const AIM_MAX: f32 = 70.0;
/// The least time between two lines, seconds.
const GAP: f32 = 1.5;
const SCRIPT_HOLD: f32 = 2.0;

#[derive(Resource)]
struct Script(Option<(f32, usize, usize)>);

#[derive(Resource, Default)]
struct State {
    page: usize,
    aim: Vec2,
    pick: Option<usize>,
    /// The time the last line was said.
    said: f32,
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct Title;
#[derive(Component)]
struct Dot;
#[derive(Component)]
struct Slot(usize);

fn slot_dir(i: usize) -> Vec2 {
    let a = i as f32 * std::f32::consts::TAU / SLOTS as f32;
    Vec2::new(a.sin(), -a.cos())
}

/// The slot the mouse has been pushed towards, if it has gone far enough and the page has one.
fn slot_at(aim: Vec2, count: usize) -> Option<usize> {
    let a = aim.x.atan2(-aim.y).rem_euclid(std::f32::consts::TAU);
    let near = (a / (std::f32::consts::TAU / SLOTS as f32)).round() as usize % SLOTS;
    (aim.length() > DEAD_ZONE && near < count).then_some(near)
}

fn build(mut commands: Commands, fonts: Res<UiFonts>, lines: Res<Lines>) {
    let centred = UiTransform { translation: Val2::percent(-50.0, -50.0), ..default() };
    commands
        .spawn((Root, Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, Visibility::Hidden))
        .with_children(|p| {
            p.spawn(Node { width: px(0.0), height: px(0.0), ..default() }).with_children(|c| {
                c.spawn((
                    Node { position_type: PositionType::Absolute, width: px(170.0), height: px(170.0), border_radius: BorderRadius::MAX, ..default() },
                    centred,
                    BackgroundColor(Color::srgba(0.03, 0.03, 0.04, 0.72)),
                ));
                c.spawn((Dot, Node { position_type: PositionType::Absolute, width: px(12.0), height: px(12.0), border_radius: BorderRadius::MAX, ..default() }, centred, BackgroundColor(Color::srgb(1.0, 0.72, 0.2))));
                c.spawn((Title, Text::new(""), TextFont { font: (&fonts.title).into(), font_size: FontSize::Px(30.0), ..default() }, TextColor(Color::srgb(1.0, 0.85, 0.5)), Node { position_type: PositionType::Absolute, top: px(-42.0), ..default() }, centred));
                for i in 0..SLOTS {
                    let at = slot_dir(i) * REACH;
                    c.spawn((
                        Slot(i),
                        Text::new(""),
                        TextFont { font_size: FontSize::Px(19.0), ..default() },
                        TextColor(Color::WHITE),
                        TextLayout::new(Justify::Center, LineBreak::NoWrap),
                        Node { position_type: PositionType::Absolute, left: px(at.x), top: px(at.y), padding: UiRect::axes(px(12.0), px(5.0)), border_radius: BorderRadius::all(px(8.0)), ..default() },
                        centred,
                        BackgroundColor(Color::srgba(0.03, 0.03, 0.04, 0.72)),
                    ));
                }
                c.spawn((
                    Text::new(lines.text("ui.wheel.hint")),
                    TextFont { font_size: FontSize::Px(15.0), ..default() },
                    TextColor(Color::srgba(1.0, 0.95, 0.85, 0.85)),
                    Node { position_type: PositionType::Absolute, top: px(-REACH.y - 62.0), width: px(520.0), ..default() },
                    TextLayout::justify(Justify::Center),
                    centred,
                ));
            });
        });
}

/// The page's lines in play, one per slot.
fn page(lines: &Lines, n: usize) -> Vec<&shared::lines::Line> {
    lines.pool(&format!("wheel.{}", PAGES[n % PAGES.len()])).take(SLOTS).collect()
}

#[allow(clippy::too_many_arguments)]
fn wheel(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    script: Res<Script>,
    local: Query<(Entity, &NetId, Has<Eliminated>), With<LocalPlayer>>,
    lines: Res<Lines>,
    m: Res<Match>,
    time: Res<Time>,
    mut open: ResMut<WheelOpen>,
    mut state: ResMut<State>,
    mut said: MessageWriter<SayMsg>,
    mut outbox: ResMut<Outbox>,
    mut ui: Query<&mut Visibility, With<Root>>,
) {
    let Ok((me, id, dead)) = local.single() else { return };
    let scripted = script.0.filter(|(t, ..)| (*t..*t + SCRIPT_HOLD).contains(&m.elapsed));
    let held = !dead && (keys.pressed(KeyCode::KeyT) || scripted.is_some());
    if held {
        if !open.0 {
            state.aim = Vec2::ZERO;
            state.pick = None;
        }
        state.aim = (state.aim + motion.delta).clamp_length_max(AIM_MAX);
        if scroll.delta.y != 0.0 {
            let step = if scroll.delta.y < 0.0 { 1 } else { PAGES.len() - 1 };
            state.page = (state.page + step) % PAGES.len();
            state.pick = None;
        }
        state.pick = slot_at(state.aim, page(&lines, state.page).len());
        if let Some((_, p, s)) = scripted {
            state.page = p;
            state.pick = Some(s);
            state.aim = slot_dir(s) * AIM_MAX;
        }
    } else if open.0 {
        // Let go: say the line picked, if the last one has had its moment.
        let now = time.elapsed_secs();
        if let Some(line) = state.pick.and_then(|i| page(&lines, state.page).get(i).copied()) {
            if !dead && now - state.said >= GAP {
                state.said = now;
                say(me, *id, line.id.clone(), true, &mut said, &mut outbox);
            }
        }
    }
    if open.0 != held {
        open.0 = held;
        for mut v in &mut ui {
            *v = if held { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

/// Whatever comes after a match, the wheel is not left open behind it, and nothing is said.
fn shut(mut open: ResMut<WheelOpen>, mut ui: Query<&mut Visibility, With<Root>>) {
    open.0 = false;
    for mut v in &mut ui {
        *v = Visibility::Hidden;
    }
}

#[allow(clippy::type_complexity)]
fn draw(
    open: Res<WheelOpen>,
    state: Res<State>,
    lines: Res<Lines>,
    mut slots: Query<(&Slot, &mut Text, &mut TextColor, &mut BackgroundColor, &mut Visibility), Without<Title>>,
    mut title: Query<&mut Text, (With<Title>, Without<Slot>)>,
    mut dot: Query<&mut Node, With<Dot>>,
) {
    if !open.0 {
        return;
    }
    let on_page = page(&lines, state.page);
    for (Slot(i), mut t, mut ink, mut bg, mut v) in &mut slots {
        let Some(line) = on_page.get(*i) else {
            *v = Visibility::Hidden;
            continue;
        };
        *v = Visibility::Inherited;
        if t.0 != line.text {
            t.0 = line.text.clone();
        }
        let picked = state.pick == Some(*i);
        ink.0 = if picked { Color::srgb(0.1, 0.08, 0.06) } else { Color::WHITE };
        bg.0 = if picked { Color::srgb(1.0, 0.72, 0.2) } else { Color::srgba(0.03, 0.03, 0.04, 0.72) };
    }
    let name = lines.text(&format!("ui.wheel.{}", PAGES[state.page]));
    for mut t in &mut title {
        if t.0 != name {
            t.0 = name.to_string();
        }
    }
    for mut n in &mut dot {
        n.left = px(state.aim.x);
        n.top = px(state.aim.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushing_the_mouse_towards_a_slot_picks_it() {
        for i in 0..SLOTS {
            assert_eq!(slot_at(slot_dir(i) * AIM_MAX, SLOTS), Some(i));
            // Anywhere within its slice of the circle.
            assert_eq!(slot_at(Vec2::from_angle(0.3).rotate(slot_dir(i)) * AIM_MAX, SLOTS), Some(i));
        }
        assert_eq!(slot_at(Vec2::new(0.0, -DEAD_ZONE / 2.0), SLOTS), None);
        // A short page has nothing in its last slots.
        assert_eq!(slot_at(slot_dir(7) * AIM_MAX, 5), None);
    }
}
