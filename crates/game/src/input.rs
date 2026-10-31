//! Action layer: physical inputs -> named actions -> `PlayerIntent` for the local warrior.
//! Bindings are data, so remapping (and touch controls later) only touch this module.

use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use shared::sim::flow::{Match, MatchState};
use shared::sim::{Eliminated, LocalPlayer, PlayerIntent, RestartMatch, btn};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Act {
    Jump,
    Fire,
    Aim,
    Reload,
    Interact,
    Sprint,
    NextSlot,
    PrevSlot,
    Restart,
    TogglePerf,
    CycleQuality,
}

#[derive(Clone, Copy, Debug)]
pub enum Source {
    Key(KeyCode),
    Mouse(MouseButton),
    Pad(GamepadButton),
}

#[derive(Resource)]
pub struct Bindings {
    pub actions: Vec<(Act, Vec<Source>)>,
    pub slot_keys: [KeyCode; 6],
    pub mouse_sensitivity: f32,
    pub stick_look_speed: f32,
    pub ads_sensitivity_scale: f32,
}

impl Default for Bindings {
    fn default() -> Self {
        use Source::*;
        Self {
            actions: vec![
                (Act::Jump, vec![Key(KeyCode::Space), Pad(GamepadButton::South)]),
                (Act::Fire, vec![Mouse(MouseButton::Left), Pad(GamepadButton::RightTrigger2)]),
                (Act::Aim, vec![Mouse(MouseButton::Right), Pad(GamepadButton::LeftTrigger2)]),
                (Act::Reload, vec![Key(KeyCode::KeyR), Pad(GamepadButton::North)]),
                (Act::Interact, vec![Key(KeyCode::KeyE), Key(KeyCode::KeyF), Pad(GamepadButton::West)]),
                (Act::Sprint, vec![Key(KeyCode::ShiftLeft), Pad(GamepadButton::LeftThumb)]),
                (Act::NextSlot, vec![Pad(GamepadButton::RightTrigger)]),
                (Act::PrevSlot, vec![Pad(GamepadButton::LeftTrigger)]),
                (Act::Restart, vec![Key(KeyCode::KeyR), Key(KeyCode::Enter), Pad(GamepadButton::Start)]),
                (Act::TogglePerf, vec![Key(KeyCode::F3)]),
                (Act::CycleQuality, vec![Key(KeyCode::F4)]),
            ],
            slot_keys: [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6],
            mouse_sensitivity: 0.0022,
            stick_look_speed: 3.2,
            ads_sensitivity_scale: 0.55,
        }
    }
}

/// Per-frame resolved action state (for UI and toggles that live outside the sim).
#[derive(Resource, Default)]
pub struct ActionState {
    held: Vec<Act>,
    just: Vec<Act>,
}

impl ActionState {
    pub fn held(&self, a: Act) -> bool {
        self.held.contains(&a)
    }
    pub fn just_pressed(&self, a: Act) -> bool {
        self.just.contains(&a)
    }
}

/// The perf overlay (F3); VIVAD_PERF_HUD=1 starts with it shown.
#[derive(Resource)]
pub struct ShowPerf(pub bool);

impl Default for ShowPerf {
    fn default() -> Self {
        ShowPerf(std::env::var("VIVAD_PERF_HUD").is_ok())
    }
}

pub struct InputPlugin;

/// The local player's intent is written in here (PreUpdate).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalInput;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bindings>()
            .init_resource::<ActionState>()
            .init_resource::<ShowPerf>()
            .add_systems(PreUpdate, (resolve_actions, write_intent, grab_cursor, meta_actions, vote).chain()
                .in_set(LocalInput).after(bevy::input::InputSystems));
    }
}

fn resolve_actions(
    bindings: Res<Bindings>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<&Gamepad>,
    mut state: ResMut<ActionState>,
) {
    state.held.clear();
    state.just.clear();
    for (act, sources) in &bindings.actions {
        let mut held = false;
        let mut just = false;
        for s in sources {
            match *s {
                Source::Key(k) => {
                    held |= keys.pressed(k);
                    just |= keys.just_pressed(k);
                }
                Source::Mouse(b) => {
                    held |= mouse.pressed(b);
                    just |= mouse.just_pressed(b);
                }
                Source::Pad(b) => {
                    for p in &pads {
                        held |= p.pressed(b);
                        just |= p.just_pressed(b);
                    }
                }
            }
        }
        if held {
            state.held.push(*act);
        }
        if just {
            state.just.push(*act);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_intent(
    bindings: Res<Bindings>,
    state: Res<ActionState>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Query<&Gamepad>,
    cursor: Single<&CursorOptions>,
    time: Res<Time>,
    mut q: Query<&mut PlayerIntent, With<LocalPlayer>>,
) {
    let Ok(mut intent) = q.single_mut() else { return };
    let grabbed = cursor.grab_mode != CursorGrabMode::None;
    let dt = time.delta_secs();

    // Movement.
    let mut mv = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) { mv.y += 1.0; }
    if keys.pressed(KeyCode::KeyS) { mv.y -= 1.0; }
    if keys.pressed(KeyCode::KeyD) { mv.x += 1.0; }
    if keys.pressed(KeyCode::KeyA) { mv.x -= 1.0; }
    let mut look = Vec2::ZERO;
    for p in &pads {
        let l = Vec2::new(p.get(GamepadAxis::LeftStickX).unwrap_or(0.0), p.get(GamepadAxis::LeftStickY).unwrap_or(0.0));
        if l.length() > 0.15 { mv += l; }
        let r = Vec2::new(p.get(GamepadAxis::RightStickX).unwrap_or(0.0), p.get(GamepadAxis::RightStickY).unwrap_or(0.0));
        if r.length() > 0.12 { look += r * bindings.stick_look_speed * dt; }
    }
    intent.move_axis = mv.clamp_length_max(1.0);

    // Look.
    let ads = state.held(Act::Aim);
    let sens = if ads { bindings.ads_sensitivity_scale } else { 1.0 };
    if grabbed {
        intent.yaw -= motion.delta.x * bindings.mouse_sensitivity * sens;
        intent.pitch -= motion.delta.y * bindings.mouse_sensitivity * sens;
    }
    intent.yaw -= look.x * sens;
    intent.pitch += look.y * sens;
    intent.pitch = intent.pitch.clamp(-1.45, 1.35);

    // Buttons: held is the live state; pressed/released are latched until the next sim tick.
    let map = [
        (Act::Jump, btn::JUMP),
        (Act::Fire, btn::FIRE),
        (Act::Aim, btn::AIM),
        (Act::Reload, btn::RELOAD),
        (Act::Interact, btn::INTERACT),
        (Act::Sprint, btn::SPRINT),
    ];
    let before = intent.held;
    let mut held = 0;
    for (act, bit) in map {
        // Firing only counts while the cursor is captured (first click just grabs it).
        if act == Act::Fire && !grabbed {
            continue;
        }
        if state.held(act) {
            held |= bit;
        }
        if state.just_pressed(act) && (act != Act::Fire || grabbed) {
            intent.pressed |= bit;
        }
    }
    intent.released |= before & !held;
    intent.held = held;

    // Hotbar.
    for (i, k) in bindings.slot_keys.iter().enumerate() {
        if keys.just_pressed(*k) {
            intent.select = Some(i as u8);
        }
    }
    let wheel = scroll.delta.y;
    let step = if wheel < 0.0 || state.just_pressed(Act::NextSlot) {
        1
    } else if wheel > 0.0 || state.just_pressed(Act::PrevSlot) {
        -1
    } else {
        0
    };
    if step != 0 {
        intent.select = Some(CYCLE.with(|c| {
            let v = (c.get() as i32 + step).rem_euclid(6) as u8;
            c.set(v);
            v
        }));
    }
}

thread_local! {
    // Hotbar cursor for wheel cycling; resynced from the inventory by the HUD each frame.
    pub static CYCLE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

fn grab_cursor(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if mouse.just_pressed(MouseButton::Left) && cursor.grab_mode == CursorGrabMode::None {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Between matches (or once out of this one), the number keys vote for the next map: the n-th
/// in the rotation (shared::sim::flow::rotation).
fn vote(
    keys: Res<ButtonInput<KeyCode>>,
    m: Res<Match>,
    local: Query<Has<Eliminated>, With<LocalPlayer>>,
    config: Res<shared::sim::MatchConfig>,
    mut votes: MessageWriter<shared::sim::flow::VoteMap>,
) {
    let dead = local.single().unwrap_or(true);
    if m.state != MatchState::Ended && !dead {
        return;
    }
    let keys_for = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5];
    for (map, key) in shared::sim::flow::rotation(config.map).iter().zip(keys_for) {
        if keys.just_pressed(key) {
            votes.write(shared::sim::flow::VoteMap(*map));
        }
    }
}

fn meta_actions(
    state: Res<ActionState>,
    m: Res<Match>,
    local: Query<Has<Eliminated>, With<LocalPlayer>>,
    mut perf: ResMut<ShowPerf>,
    mut restart: MessageWriter<RestartMatch>,
) {
    if state.just_pressed(Act::TogglePerf) {
        perf.0 = !perf.0;
    }
    let dead = local.single().unwrap_or(true);
    if state.just_pressed(Act::Restart) && (m.state == MatchState::Ended || dead) {
        restart.write(RestartMatch);
    }
}
