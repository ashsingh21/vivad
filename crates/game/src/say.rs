//! The lines that come and go on screen: the big call of the player's own kill, the Aandhi alerts
//! in the voice of a Metro announcement, and what warriors shout, over their heads.

use bevy::prelude::*;
use shared::lines::{Lines, with};
use shared::map::MapLayout;
use shared::sim::flow::{Match, MatchState};
use shared::sim::talk::{CalloutMsg, FellMsg, SayMsg};
use shared::sim::zone::Zone;
use shared::sim::{DamageCause, Eliminated, LocalPlayer, Warrior};

pub struct SayPlugin;

impl Plugin for SayPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("VIVAD_CLEAN_CAPTURE").is_ok() {
            return;
        }
        app.add_systems(Startup, build)
            .add_systems(Update, (callouts, alerts))
            .add_systems(PostUpdate, bubbles.after(crate::camera::CameraSystems));
    }
}

/// How long a big call and an alert stay up, seconds.
const CALLOUT_TIME: f32 = 1.6;
const ALERT_TIME: f32 = 5.0;
/// How often the player outside the ring is told, seconds.
const OUTSIDE_EVERY: f32 = 12.0;
/// A kill this near drops loot worth hearing about, metres, and how often that is said, seconds.
const LOOT_NEAR: f32 = 30.0;
const LOOT_EVERY: f32 = 40.0;
/// How far a shout carries, metres, and how long it hangs there, seconds.
const SHOUT_REACH: f32 = 55.0;
const SHOUT_TIME: f32 = 2.4;

#[derive(Component)]
struct Callout;
#[derive(Component)]
struct AlertBar;
#[derive(Component)]
struct AlertText;

/// What a warrior just shouted, kept over its head.
#[derive(Component)]
struct Bubble {
    who: Entity,
    age: f32,
}

fn build(mut commands: Commands, fonts: Res<crate::fonts::UiFonts>) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100.0), top: percent(24.0), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((Callout, Text::new(""), TextFont { font: (&fonts.title).into(), font_size: FontSize::Px(40.0), ..default() }, TextColor(Color::srgb(1.0, 0.8, 0.25)), TextShadow::default()));
        });
    // An amber display board, like a Metro coach's.
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100.0), top: px(20.0), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((
                AlertBar,
                Node { padding: UiRect::axes(px(16.0), px(6.0)), border_radius: BorderRadius::all(px(6.0)), max_width: px(620.0), ..default() },
                BackgroundColor(Color::srgba(0.03, 0.03, 0.04, 0.8)),
                Visibility::Hidden,
            ))
            .with_children(|b| {
                b.spawn((AlertText, Text::new(""), TextFont { font_size: FontSize::Px(19.0), ..default() }, TextColor(Color::srgb(1.0, 0.72, 0.2)), TextLayout::justify(Justify::Center)));
            });
        });
}

fn callouts(
    mut calls: MessageReader<CalloutMsg>,
    mut q: Query<(&mut Text, &mut TextColor), With<Callout>>,
    time: Res<Time>,
    mut until: Local<f32>,
) {
    let now = time.elapsed_secs();
    for (mut t, mut c) in &mut q {
        for call in calls.read() {
            t.0 = call.0.clone();
            *until = now + CALLOUT_TIME;
        }
        if now > *until && !t.0.is_empty() {
            t.0.clear();
        }
        c.0.set_alpha(((*until - now) / 0.3).clamp(0.0, 1.0));
    }
}

/// The name of the place nearest a point on the map.
fn place_near(layout: &MapLayout, at: Vec2) -> &str {
    layout.pois.iter().min_by(|a, b| a.center.xz().distance(at).total_cmp(&b.center.xz().distance(at))).map_or(layout.name.as_str(), |p| p.name.as_str())
}

#[derive(Default)]
struct Watch {
    generation: u32,
    /// The zone state last announced, and how much of this wait's warning has been given.
    state: Option<(usize, bool)>,
    warned: u8,
    was_outside: bool,
    outside_at: f32,
    loot_at: f32,
    until: f32,
}

/// Says each turn of the Aandhi as a Metro train would. Everything here is the player's own
/// screen: nothing goes to the other peers.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn alerts(
    zone: Res<Zone>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    lines: Res<Lines>,
    local: Query<(Entity, &Transform, Has<Eliminated>), With<LocalPlayer>>,
    mut fell: MessageReader<FellMsg>,
    time: Res<Time>,
    mut watch: Local<Watch>,
    mut bar: Query<&mut Visibility, With<AlertBar>>,
    mut text: Query<&mut Text, With<AlertText>>,
) {
    let now = time.elapsed_secs();
    let Ok((me, tf, dead)) = local.single() else { return };
    if watch.generation != m.generation {
        *watch = Watch { generation: m.generation, loot_at: -LOOT_EVERY, ..default() };
    }
    let seed = m.seed;
    let step = zone.phase as u64 * 2 + zone.shrinking as u64;
    let mut say: Option<String> = None;
    if m.state == MatchState::InProgress && !dead {
        let (left, shrinking) = zone.countdown();
        let place = place_near(&layout, zone.target_center);
        if watch.state != Some((zone.phase, shrinking)) {
            watch.state = Some((zone.phase, shrinking));
            watch.warned = 0;
            let rings = zone.phases().len();
            let last = zone.phase + 1 >= rings;
            say = Some(if shrinking && last {
                lines.text("alert.tiny").to_string()
            } else if shrinking && zone.phase + 2 >= rings {
                lines.pick("alert.last", seed, step).to_string()
            } else {
                with(lines.pick(if shrinking { "alert.moving" } else { "alert.next" }, seed, step), &[("place", place)])
            });
        } else if !shrinking && left <= 10.0 && watch.warned < 2 {
            watch.warned = 2;
            say = Some(lines.text("alert.doors").to_string());
        } else if !shrinking && left <= 30.0 && watch.warned < 1 {
            watch.warned = 1;
            say = Some(with(lines.text("alert.soon"), &[("t", "30")]));
        }
        let outside = zone.outside(tf.translation);
        if outside && !watch.was_outside {
            watch.was_outside = true;
            watch.outside_at = now;
            say = Some(lines.text("alert.first").to_string());
        } else if outside && now - watch.outside_at > OUTSIDE_EVERY {
            watch.outside_at = now;
            say = Some(lines.pick("alert.outside", seed, (now / OUTSIDE_EVERY) as u64).to_string());
        }
    }
    for f in fell.read() {
        if f.victim == me && f.cause == DamageCause::Pralaya {
            say = Some(lines.text("alert.thanks").to_string());
        } else if !dead && f.victim != me && f.at.distance(tf.translation) < LOOT_NEAR && now - watch.loot_at > LOOT_EVERY {
            watch.loot_at = now;
            say = Some(lines.pick("alert.loot", seed, m.alive as u64).to_string());
        }
    }
    if let Some(s) = say {
        watch.until = now + ALERT_TIME;
        for mut t in &mut text {
            t.0 = s.clone();
        }
    }
    for mut v in &mut bar {
        *v = if now < watch.until { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// Shouts over the heads of those who shouted them, for anyone near enough to hear.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn bubbles(
    mut commands: Commands,
    mut said: MessageReader<SayMsg>,
    lines: Res<Lines>,
    people: Query<&Transform, (With<Warrior>, Without<Eliminated>)>,
    local: Query<&Transform, With<LocalPlayer>>,
    cam: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    mut q: Query<(Entity, &mut Bubble, &mut Node, &mut Text, &mut Visibility)>,
    time: Res<Time>,
    view_scale: Res<crate::quality::ViewScale>,
) {
    for s in said.read() {
        let Some(line) = lines.get(&s.line) else { continue };
        if let Some((_, mut b, _, mut t, _)) = q.iter_mut().find(|(_, b, ..)| b.who == s.who) {
            b.age = 0.0;
            t.0 = line.text.clone();
            continue;
        }
        commands.spawn((
            Bubble { who: s.who, age: 0.0 },
            Text::new(line.text.clone()),
            TextFont { font_size: FontSize::Px(17.0), ..default() },
            TextColor(Color::srgb(0.1, 0.08, 0.06)),
            Node { position_type: PositionType::Absolute, padding: UiRect::axes(px(9.0), px(3.0)), border_radius: BorderRadius::all(px(8.0)), ..default() },
            BackgroundColor(Color::srgba(1.0, 0.96, 0.86, 0.92)),
            UiTransform { translation: Val2::percent(-50.0, -100.0), ..default() },
            Visibility::Hidden,
        ));
    }
    let (Ok((camera, cam_g)), Ok(me)) = (cam.single(), local.single()) else { return };
    let dt = time.delta_secs();
    for (e, mut b, mut node, _, mut vis) in &mut q {
        b.age += dt;
        let Some(tf) = people.get(b.who).ok().filter(|_| b.age < SHOUT_TIME) else {
            commands.entity(e).despawn();
            continue;
        };
        let head = tf.translation + Vec3::Y * 1.25;
        let near = head.distance(me.translation) < SHOUT_REACH;
        match camera.world_to_viewport(cam_g, head).map(|p| p * view_scale.0) {
            Ok(p) if near => {
                node.left = px(p.x);
                node.top = px(p.y);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}
