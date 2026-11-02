//! VIVAD_LOOP=<dir>[,<fps>]: the game plays its own front door in the real window, for pictures of
//! every screen and a clip of the whole loop. It presses keys and clicks through the same input
//! messages a keyboard and mouse send, so the real buttons, menus and match run; only the player's
//! walk is steered, straight at the nearest warrior, until something kills it. Keep the window in
//! view. The pictures go to <dir>, the frames for tools/clip.py to <dir>/frames, and how long each
//! step took to the log as `[loop]` lines. With VIVAD_BOTS=0 the player wins instead of falling.
//! Online (VIVAD_HOST=1 VIVAD_LAN=1) a host starts alone with Enter and plays the same loop; with
//! VIVAD_JOIN=<a bad code> it tries again after the error, then goes Home. If the match goes on
//! without the player (VIVAD_HOST=2, mesh_check's MESH_JOIN as the other), it leaves with Ghar.
//! A hidden window or a locked screen draws nothing: VIVAD_LOOP_OFFSCREEN=1 has the UI camera draw
//! to an image instead, for the pictures. A pointer only reaches a window, so clicks then press the
//! button directly. On the NCR nothing may reach the player for minutes: VIVAD_LOOP_FALL=<seconds>
//! ends the walk that long after landing, unless the player is alone in the match.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::ui::{InteractionDisabled, UiGlobalTransform};
use bevy::ui_widgets::Activate;
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};
use shared::map::MapLayout;
use shared::sim::flow::Match;
use shared::sim::{DamageCause, DamageMsg, Eliminated, Kinematic, LocalPlayer, MoveMode, PlayerIntent, Warrior};

use super::{Press, Screen};

/// Seconds: a frame this long means something is still loading.
const SLOW_FRAME: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Lobby,
    Home,
    Ride,
    Fall,
    Walk,
    Card,
    NextRide,
    Leaving,
    Done,
}

#[derive(Resource)]
pub(super) struct Demo {
    dir: String,
    stage: Stage,
    /// The step within the stage, and when the stage began.
    step: u32,
    from: f32,
    frame_every: f32,
    next_frame: f32,
    frame: u32,
    /// A picture to take this frame (one screenshot of the window a frame, so the clip skips it).
    shot: Option<&'static str>,
    /// Keys held down, to let go of at these times.
    held: Vec<(KeyCode, f32)>,
    fell_at: Option<f32>,
    pressed_at: f32,
    /// The longest frame since the press.
    longest: f32,
    /// When a frame last took long: at the start, shaders are still compiling.
    busy_at: f32,
    /// VIVAD_LOOP_FALL: seconds of walking before the player falls.
    fall_after: Option<f32>,
    generation: u32,
    /// The button the pointer is on.
    hovered: Option<Entity>,
}

/// The image the UI camera draws to, with VIVAD_LOOP_OFFSCREEN=1.
#[derive(Resource)]
struct Offscreen(Handle<Image>);

fn offscreen(mut commands: Commands, mut images: ResMut<Assets<Image>>, cam: Query<Entity, With<IsDefaultUiCamera>>) {
    let image = images.add(Image::new_target_texture(1600, 900, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    for e in &cam {
        commands.entity(e).insert(RenderTarget::Image(ImageRenderTarget::from(image.clone())));
    }
    commands.insert_resource(Offscreen(image));
}

pub(super) fn plugin(app: &mut App) {
    let Ok(spec) = std::env::var("VIVAD_LOOP") else { return };
    let mut parts = spec.split(',');
    let dir = parts.next().unwrap_or("previews/front-door").to_owned();
    let fps: f32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(6.0);
    std::fs::create_dir_all(format!("{dir}/frames")).ok();
    let fall_after = std::env::var("VIVAD_LOOP_FALL").ok().and_then(|s| s.parse().ok());
    app.insert_resource(Demo { dir, stage: Stage::Home, step: 0, from: 0.0, frame_every: 1.0 / fps.max(1.0), next_frame: 2.0, frame: 0, shot: None, held: vec![], fell_at: None, pressed_at: 0.0, longest: 0.0, busy_at: 0.0, fall_after, generation: 0, hovered: None })
        .add_systems(Update, drive);
    if std::env::var("VIVAD_LOOP_OFFSCREEN").is_ok() {
        app.add_systems(PostStartup, offscreen);
    }
}

impl Demo {
    fn go(&mut self, stage: Stage, now: f32) {
        info!("[loop] {:?} at {now:.2}s", stage);
        self.stage = stage;
        self.step = 0;
        self.from = now;
    }

}

fn key_event(key_code: KeyCode, state: ButtonState, window: Entity) -> KeyboardInput {
    KeyboardInput { key_code, logical_key: Key::Unidentified(NativeKey::Unidentified), state, text: None, repeat: false, window }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive(
    mut commands: Commands,
    mut demo: ResMut<Demo>,
    image: Option<Res<Offscreen>>,
    time: Res<Time<Real>>,
    screen: Res<State<Screen>>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    window: Single<Entity, With<PrimaryWindow>>,
    mut me: Query<(Entity, &Transform, &Kinematic, &mut PlayerIntent, Has<Eliminated>), With<LocalPlayer>>,
    others: Query<&Transform, (With<Warrior>, Without<LocalPlayer>, Without<Eliminated>)>,
    on_screen: Query<(Entity, &Press, &ComputedNode, &UiGlobalTransform, Has<InteractionDisabled>, &InheritedVisibility)>,
    lobby: Option<Res<net::Lobby>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut events: MessageWriter<WindowEvent>,
    mut buttons: MessageWriter<MouseButtonInput>,
    (mut hurt, mut exit): (MessageWriter<DamageMsg>, MessageWriter<AppExit>),
) {
    let now = time.elapsed_secs();
    demo.longest = demo.longest.max(time.delta_secs());
    if time.delta_secs() > SLOW_FRAME {
        demo.busy_at = now;
    }
    let window = *window;
    let t = now - demo.from;
    let screen = *screen.get();

    // Frames for the clip; the long walk at a quarter of the rate, so it plays fast.
    let every = demo.frame_every * if demo.stage == Stage::Walk { 4.0 } else { 1.0 };
    let picture = || image.as_ref().map_or_else(Screenshot::primary_window, |i| Screenshot::image(i.0.clone()));
    if let Some(name) = demo.shot.take() {
        commands.spawn(picture()).observe(save_to_disk(format!("{}/{name}.png", demo.dir)));
    } else if now >= demo.next_frame && demo.stage != Stage::Done {
        let path = format!("{}/frames/frame_{:05}.png", demo.dir, demo.frame);
        commands.spawn(picture()).observe(crate::autoshot::save_small(path));
        demo.frame += 1;
        demo.next_frame = now + every;
    }

    let mut held = std::mem::take(&mut demo.held);
    held.retain(|(k, until)| {
        let down = now < *until;
        if !down {
            keys.write(key_event(*k, ButtonState::Released, window));
        }
        down
    });
    demo.held = held;
    let hover = |events: &mut MessageWriter<WindowEvent>, press: Press| {
        let (e, _, n, g, ..) = on_screen.iter().find(|(_, p, .., shown)| **p == press && shown.get())?;
        let position = g.translation * n.inverse_scale_factor;
        events.write(WindowEvent::CursorMoved(CursorMoved { window, position, delta: None }));
        Some(e)
    };
    // Up from a click on what the pointer is on.
    let click_done = |commands: &mut Commands, demo: &mut Demo| {
        if let (true, Some(entity)) = (image.is_some(), demo.hovered.take()) {
            commands.trigger(Activate { entity });
        }
    };
    let won = me.single().is_ok_and(|(e, ..)| m.winner == Some(e));
    let play_locked = on_screen.iter().any(|(_, p, _, _, off, _)| *p == Press::Play && off);
    let hosting = lobby.as_ref().is_some_and(|l| l.hosting);
    // VIVAD_HOST=<n> starts by itself once n players are in.
    let host_waits = std::env::var("VIVAD_HOST").is_ok_and(|n| n != "1");

    match (demo.stage, demo.step) {
        (Stage::Home, 0) if screen == Screen::Lobby => demo.go(Stage::Lobby, now),
        (Stage::Home, 0) if screen == Screen::Match => demo.go(Stage::Ride, now),
        // Once the frames have settled, so the picture is what a player gets to look at.
        (Stage::Home, 0) if (t > 5.0 && now - demo.busy_at > 1.0) || t > 30.0 => {
            info!("[loop] Home settled {t:.2}s in");
            demo.shot = Some("1_home");
            demo.from = now - 5.0;
            demo.step += 1;
        }
        (Stage::Home, 1) if t > 5.4 => {
            demo.hovered = hover(&mut events, Press::Play);
            demo.step += 1;
        }
        (Stage::Home, 2) if t > 5.7 => {
            mouse(&mut events, &mut buttons, ButtonState::Pressed, window);
            demo.step += 1;
        }
        (Stage::Home, 3) if t > 5.8 => {
            mouse(&mut events, &mut buttons, ButtonState::Released, window);
            click_done(&mut commands, &mut demo);
            demo.pressed_at = now;
            demo.longest = 0.0;
            demo.step += 1;
        }
        (Stage::Home, 4) if screen == Screen::Match => {
            info!("[loop] match {} on {} with {} warriors, {:.2}s after the press, longest frame {:.0} ms", m.generation, layout.name, m.total, now - demo.pressed_at, demo.longest * 1000.0);
            demo.go(Stage::Ride, now);
        }

        // Online: the lobby's picture. A host starts with Enter; a joiner tries again, then goes Home.
        (Stage::Lobby, _) if screen == Screen::Match => {
            info!("[loop] match {} began with {} local player(s)", m.generation, me.iter().count());
            demo.go(Stage::Ride, now);
        }
        (Stage::Lobby, 0) if t > 5.0 => {
            let error = lobby.as_ref().is_some_and(|l| l.error.is_some());
            demo.shot = Some(if error { "lobby_error" } else if hosting { "lobby_host" } else { "lobby_join" });
            demo.step += 1;
        }
        (Stage::Lobby, 1) if t > 5.5 && hosting && !host_waits => {
            tap(&mut keys, &mut demo, KeyCode::Enter, window, now);
            demo.step = 10;
        }
        (Stage::Lobby, 1) if t > 5.5 && !hosting => {
            demo.hovered = hover(&mut events, Press::Retry);
            demo.step += 1;
        }
        (Stage::Lobby, 2) if t > 5.8 => {
            mouse(&mut events, &mut buttons, ButtonState::Pressed, window);
            demo.step += 1;
        }
        (Stage::Lobby, 3) if t > 5.9 => {
            mouse(&mut events, &mut buttons, ButtonState::Released, window);
            click_done(&mut commands, &mut demo);
            demo.step += 1;
        }
        (Stage::Lobby, 4) if t > 7.0 => {
            demo.hovered = hover(&mut events, Press::Ghar);
            demo.step += 1;
        }
        (Stage::Lobby, 5) if t > 7.3 => {
            mouse(&mut events, &mut buttons, ButtonState::Pressed, window);
            demo.step += 1;
        }
        (Stage::Lobby, 6) if t > 7.4 => {
            mouse(&mut events, &mut buttons, ButtonState::Released, window);
            click_done(&mut commands, &mut demo);
            demo.step += 1;
        }
        (Stage::Lobby, 7) if screen == Screen::Home => demo.go(Stage::Leaving, now),

        (Stage::Ride, 0) if t > 3.0 => {
            demo.shot = Some("2_drop");
            demo.step += 1;
        }
        (Stage::Ride, 1) => {
            let over = over_land(&m, &layout);
            if over && t > 4.0 {
                tap(&mut keys, &mut demo, KeyCode::Space, window, now);
                demo.step += 1;
            }
        }
        (Stage::Ride, 2) if me.single().is_ok_and(|(_, _, k, ..)| k.mode != MoveMode::Vimana) => demo.go(Stage::Fall, now),

        (Stage::Fall, 0) if t > 1.5 => {
            demo.shot = Some("3_fall");
            demo.step += 1;
        }
        (Stage::Fall, 1) if me.single().is_ok_and(|(_, _, k, ..)| k.mode == MoveMode::Ground) => {
            keys.write(key_event(KeyCode::KeyW, ButtonState::Pressed, window));
            demo.go(Stage::Walk, now);
        }
        // The whole map, which Esc only shuts; a look at the pause menu, then on.
        (Stage::Walk, 0) if t > 1.0 => {
            tap(&mut keys, &mut demo, KeyCode::KeyM, window, now);
            demo.step += 1;
        }
        (Stage::Walk, 1) if t > 2.0 => {
            demo.shot = Some("4_big_map");
            demo.step += 1;
        }
        (Stage::Walk, 2) if t > 2.5 => {
            tap(&mut keys, &mut demo, KeyCode::Escape, window, now);
            demo.step += 1;
        }
        (Stage::Walk, 3) if t > 3.2 => {
            info!("[loop] Esc on the whole map left {screen:?} up");
            demo.shot = Some("4_map_shut");
            demo.step += 1;
        }
        (Stage::Walk, 4) if t > 3.7 => {
            tap(&mut keys, &mut demo, KeyCode::Escape, window, now);
            demo.step += 1;
        }
        (Stage::Walk, 5) if t > 5.2 => {
            info!("[loop] the next Esc left {screen:?} up");
            demo.shot = Some("4_pause");
            demo.step += 1;
        }
        (Stage::Walk, 6) if t > 5.7 => {
            tap(&mut keys, &mut demo, KeyCode::Escape, window, now);
            demo.step += 1;
        }
        (Stage::Walk, 7) if t > 6.2 => {
            demo.shot = Some("4_match");
            demo.step += 1;
        }
        // Falling with the whole map open: the card must come up alone.
        (Stage::Walk, 8) if m.total > 1 && demo.fall_after.is_some_and(|after| t > after - 1.0) => {
            tap(&mut keys, &mut demo, KeyCode::KeyM, window, now);
            demo.step += 1;
        }
        (Stage::Walk, _) => {
            let Ok((e, tf, _, mut intent, dead)) = me.single_mut() else { return };
            if !dead && m.total > 1 && demo.fall_after.is_some_and(|after| t > after) {
                hurt.write(DamageMsg { target: e, amount: 100_000.0, source: None, headshot: false, cause: DamageCause::Pralaya });
            }
            if dead || m.winner.is_some() {
                keys.write(key_event(KeyCode::KeyW, ButtonState::Released, window));
                demo.fell_at = Some(now);
                info!("[loop] out of the match at match time {:.1}s", m.elapsed);
                demo.go(Stage::Card, now);
                return;
            }
            let at = tf.translation;
            if let Some(near) = others.iter().map(|o| o.translation).min_by(|a, b| a.distance(at).total_cmp(&b.distance(at))) {
                let d = near - at;
                intent.yaw = (-d.x).atan2(-d.z);
            }
        }

        (Stage::Card, 0) if screen == Screen::Result => {
            let lag = demo.fell_at.map_or(0.0, |f| now - f);
            info!("[loop] the card came up {lag:.2}s after that");
            demo.go(Stage::Card, now);
            demo.step = 1;
        }
        (Stage::Card, 1) if t > 1.4 => {
            demo.shot = Some(if won { "6_win" } else { "5_died" });
            demo.step += 1;
        }
        // Online, the match goes on without us: no next match yet, but the way out is open.
        (Stage::Card, 2) if t > 2.2 && play_locked => {
            info!("[loop] the next match is locked while others play; leaving with Ghar");
            demo.shot = Some("5_died_online");
            demo.step = 20;
        }
        (Stage::Card, 20) if t > 2.6 => {
            demo.hovered = hover(&mut events, Press::Ghar);
            demo.step += 1;
        }
        (Stage::Card, 21) if t > 2.9 => {
            mouse(&mut events, &mut buttons, ButtonState::Pressed, window);
            demo.step += 1;
        }
        (Stage::Card, 22) if t > 3.0 => {
            mouse(&mut events, &mut buttons, ButtonState::Released, window);
            click_done(&mut commands, &mut demo);
            demo.step += 1;
        }
        (Stage::Card, 23) if screen == Screen::Home => {
            let since = demo.fell_at.map_or(0.0, |f| now - f);
            info!("[loop] home {since:.2}s after falling");
            demo.go(Stage::Leaving, now);
        }
        (Stage::Card, 2) if t > 2.1 && !play_locked => {
            demo.generation = m.generation;
            demo.pressed_at = now;
            demo.longest = 0.0;
            tap(&mut keys, &mut demo, KeyCode::Enter, window, now);
            demo.step += 1;
        }
        (Stage::Card, 3) if m.generation != demo.generation => demo.go(Stage::NextRide, now),

        // The frame that built the match is over by now: this is the next drop on screen.
        (Stage::NextRide, 0) => {
            let since = demo.fell_at.map_or(0.0, |f| now - f);
            info!("[loop] next drop on screen {:.2}s after Enter, {since:.2}s after falling: match {} on {} with {} warriors", now - demo.pressed_at, m.generation, layout.name, m.total);
            demo.step += 1;
        }
        (Stage::NextRide, 1) if t > 0.5 => {
            demo.shot = Some("7_next_drop");
            demo.step += 1;
        }
        (Stage::NextRide, 2) if over_land(&m, &layout) => {
            tap(&mut keys, &mut demo, KeyCode::Space, window, now);
            let since = demo.fell_at.map_or(0.0, |f| now - f);
            info!("[loop] jumped out of the next drop {:.2}s after Enter, {since:.2}s after falling", now - demo.pressed_at);
            demo.step += 1;
        }
        (Stage::NextRide, 3) if t > 4.0 => {
            info!("[loop] longest frame from Enter to here: {:.0} ms", demo.longest * 1000.0);
            tap(&mut keys, &mut demo, KeyCode::Escape, window, now);
            demo.step += 1;
        }
        (Stage::NextRide, 4) if t > 5.0 => {
            demo.hovered = hover(&mut events, Press::Ghar);
            demo.step += 1;
        }
        (Stage::NextRide, 5) if t > 5.3 => {
            mouse(&mut events, &mut buttons, ButtonState::Pressed, window);
            demo.step += 1;
        }
        (Stage::NextRide, 6) if t > 5.4 => {
            mouse(&mut events, &mut buttons, ButtonState::Released, window);
            click_done(&mut commands, &mut demo);
            demo.step += 1;
        }
        (Stage::NextRide, 7) if screen == Screen::Home => {
            demo.go(Stage::Leaving, now);
        }
        (Stage::Leaving, 0) if t > 2.0 => {
            demo.shot = Some("8_back_home");
            demo.step += 1;
        }
        // Home again, then Enter: a new match, solo even after leaving a session.
        (Stage::Leaving, 1) if t > 2.5 => {
            demo.generation = m.generation;
            tap(&mut keys, &mut demo, KeyCode::Enter, window, now);
            demo.step += 1;
        }
        (Stage::Leaving, 2) if m.generation != demo.generation && t > 4.0 => {
            info!("[loop] from Home again: match {}, {} warriors, {} frames", m.generation, others.iter().count() + 1, demo.frame);
            demo.go(Stage::Done, now);
        }
        (Stage::Done, _) if t > 1.0 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn tap(keys: &mut MessageWriter<KeyboardInput>, demo: &mut Demo, key: KeyCode, window: Entity, now: f32) {
    keys.write(key_event(key, ButtonState::Pressed, window));
    demo.held.push((key, now + 0.08));
}

fn mouse(events: &mut MessageWriter<WindowEvent>, buttons: &mut MessageWriter<MouseButtonInput>, state: ButtonState, window: Entity) {
    let event = MouseButtonInput { button: MouseButton::Left, state, window };
    events.write(WindowEvent::MouseButtonInput(event.clone()));
    buttons.write(event);
}

/// The ride is over the map, so a jump lands on it.
fn over_land(m: &Match, layout: &MapLayout) -> bool {
    let p = m.vimana_pos();
    p.x.abs() < layout.half_extent - 10.0 && p.z.abs() < layout.half_extent - 10.0
}
