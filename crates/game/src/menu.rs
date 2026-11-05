//! The start screen: your name, then play alone, host an online match (a room code to share), or
//! join one by its code. Shown at launch over the solo match, which stands still behind it
//! (`Hold`). Skipped when the launch already says what to play (VIVAD_HOST / VIVAD_JOIN, or the
//! browser's ?host / ?join), for the automated runs (tours, the armoury, feel and grip checks,
//! scripted keys), and with VIVAD_MENU=0.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use net::Lobby;
use shared::sim::{Hold, MatchConfig};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuOpen>()
            .add_systems(Startup, open)
            .add_systems(Update, (pointer, typing, refresh).chain().run_if(|m: Res<MenuOpen>| m.0));
    }
}

/// Whether the start screen is up (input leaves the cursor free while it is).
#[derive(Resource, Default)]
pub struct MenuOpen(pub bool);

#[derive(Component)]
struct Menu;

/// The two text fields; `Field` marks the one being typed into.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Code,
}

#[derive(Component, Clone, Copy)]
enum Choice {
    Solo,
    Host,
    Join,
}

#[derive(Resource)]
struct Typing {
    name: String,
    code: String,
    focus: Field,
    error: Option<String>,
}

#[derive(Component)]
struct FieldText(Field);

#[derive(Component)]
struct ErrorText;

/// Launches that skip the screen: told what to play already, or run by a tool.
fn skipped() -> bool {
    const TOOLS: [&str; 11] = ["VIVAD_TOUR", "VIVAD_ARMOURY", "VIVAD_SHOTS", "VIVAD_SHOWCASE", "VIVAD_FEEL", "VIVAD_GRIPCHECK", "VIVAD_KEYS", "VIVAD_AUTOPILOT", "VIVAD_HOST", "VIVAD_JOIN", "VIVAD_PERF_ALL"];
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("VIVAD_MENU").as_deref() == Ok("0") || TOOLS.iter().any(|k| std::env::var_os(k).is_some())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = TOOLS;
        let search = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
        search.contains("host") || search.contains("join") || search.contains("menu=0")
    }
}

const PANEL: Color = Color::srgba(0.05, 0.06, 0.09, 0.92);
const GOLD: Color = Color::srgb(1.0, 0.82, 0.42);
const FIELD: Color = Color::srgb(0.13, 0.15, 0.2);
const FIELD_ON: Color = Color::srgb(0.2, 0.24, 0.32);
const BUTTON: Color = Color::srgb(0.88, 0.45, 0.12);
const BUTTON_HOVER: Color = Color::srgb(0.98, 0.58, 0.2);

fn open(mut commands: Commands, mut menu: ResMut<MenuOpen>, mut hold: ResMut<Hold>, lobby: Option<Res<Lobby>>) {
    if lobby.is_some() || skipped() {
        return;
    }
    menu.0 = true;
    hold.0 = true;
    #[cfg(not(target_arch = "wasm32"))]
    let name = std::env::var("VIVAD_NAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Warrior".into());
    #[cfg(target_arch = "wasm32")]
    let name = String::from("Warrior");
    commands.insert_resource(Typing { name, code: String::new(), focus: Field::Name, error: None });
    let text = |s: &str, px: f32, c: Color| (Text::new(s), TextFont { font_size: FontSize::Px(px), ..default() }, TextColor(c));
    let field = |p: &mut ChildSpawnerCommands, label: &str, f: Field| {
        p.spawn(text(label, 18.0, Color::srgb(0.75, 0.78, 0.85)));
        p.spawn((
            f,
            Node { width: px(420.0), padding: UiRect::all(px(10.0)), margin: UiRect::bottom(px(14.0)), border_radius: BorderRadius::all(px(6.0)), ..default() },
            BackgroundColor(FIELD),
        ))
        .with_children(|b| {
            b.spawn((FieldText(f), text("", 22.0, Color::WHITE), Pickable::IGNORE));
        });
    };
    let button = |p: &mut ChildSpawnerCommands, label: &str, c: Choice| {
        p.spawn((
            c,
            Node {
                width: px(420.0),
                padding: UiRect::all(px(12.0)),
                margin: UiRect::bottom(px(10.0)),
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(px(8.0)),
                ..default()
            },
            BackgroundColor(BUTTON),
        ))
        .with_children(|b| {
            b.spawn((text(label, 24.0, Color::WHITE), Pickable::IGNORE));
        });
    };
    commands
        .spawn((
            Menu,
            Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::all(px(32.0)), border_radius: BorderRadius::all(px(14.0)), ..default() },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((text("PANGA", 64.0, GOLD), Node { margin: UiRect::bottom(px(20.0)), ..default() }));
                field(p, "Your name", Field::Name);
                button(p, "Play solo", Choice::Solo);
                button(p, "Host online match", Choice::Host);
                p.spawn((text("or join a friend's match: type their room code", 18.0, Color::srgb(0.75, 0.78, 0.85)), Node { margin: UiRect::top(px(14.0)), ..default() }));
                field(p, "", Field::Code);
                button(p, "Join", Choice::Join);
                p.spawn((ErrorText, text("", 18.0, Color::srgb(1.0, 0.45, 0.4))));
            });
        });
}

/// The buttons and fields hit-tested here, not by picking: picking finds what a press lands on
/// from the previous frame's hover, so while the map is still loading (frames a second long) the
/// move, press and release all fall in one frame and the first click was lost.
fn pointer(
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    fields: Query<(&Field, &ComputedNode, &UiGlobalTransform)>,
    mut buttons: Query<(&Choice, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor), Without<Field>>,
    mut t: ResMut<Typing>,
    mut commands: Commands,
) {
    let scale = window.scale_factor();
    let hover = window.physical_cursor_position();
    let tap = touches.iter_just_released().map(|f| f.position() * scale).next().or(hover.filter(|_| mouse.just_released(MouseButton::Left)));
    for (c, node, at, mut bg) in &mut buttons {
        let over = hover.is_some_and(|p| node.contains_point(*at, p));
        let colour = if over { BUTTON_HOVER } else { BUTTON };
        if bg.0 != colour {
            bg.0 = colour;
        }
        if tap.is_some_and(|p| node.contains_point(*at, p)) {
            commands.run_system_cached_with(choose, *c);
        }
    }
    for (f, node, at) in &fields {
        if tap.is_some_and(|p| node.contains_point(*at, p)) {
            t.focus = *f;
        }
    }
}

/// Keys into the focused field (letters, digits, dashes; Backspace; Tab switches; Enter acts).
fn typing(mut keys: MessageReader<KeyboardInput>, mut t: ResMut<Typing>, mut commands: Commands) {
    let mut act = None;
    for k in keys.read() {
        if !k.state.is_pressed() {
            continue;
        }
        let focus = t.focus;
        let s = if focus == Field::Name { &mut t.name } else { &mut t.code };
        match &k.logical_key {
            Key::Backspace => {
                s.pop();
            }
            Key::Tab => t.focus = if focus == Field::Name { Field::Code } else { Field::Name },
            Key::Enter => act = Some(if focus == Field::Code { Choice::Join } else { Choice::Solo }),
            Key::Space if focus == Field::Name && s.len() < 20 => s.push(' '),
            Key::Character(c) => {
                for ch in c.chars() {
                    let ok = ch.is_alphanumeric() || ch == '-' || (focus == Field::Name && (ch == '_' || ch == '.'));
                    if ok && s.len() < if focus == Field::Name { 20 } else { 200 } {
                        s.push(if focus == Field::Code { ch.to_ascii_lowercase() } else { ch });
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(c) = act {
        commands.run_system_cached_with(choose, c);
    }
}

fn refresh(t: Res<Typing>, mut texts: Query<(&FieldText, &mut Text)>, mut fields: Query<(&Field, &mut BackgroundColor)>, mut err: Query<&mut Text, (With<ErrorText>, Without<FieldText>)>) {
    for (f, mut text) in &mut texts {
        let s = if f.0 == Field::Name { &t.name } else { &t.code };
        let shown = if t.focus == f.0 { format!("{s}|") } else if s.is_empty() && f.0 == Field::Code { "chai-tiger-kulfi-42".into() } else { s.clone() };
        if text.0 != shown {
            text.0 = shown;
        }
    }
    for (f, mut bg) in &mut fields {
        bg.0 = if *f == t.focus { FIELD_ON } else { FIELD };
    }
    for mut e in &mut err {
        let s = t.error.clone().unwrap_or_default();
        if e.0 != s {
            e.0 = s;
        }
    }
}

/// Do what was chosen: play on alone, or go online (the solo match cleared, the map the room's).
fn choose(In(choice): In<Choice>, world: &mut World) {
    let (name, code) = {
        let t = world.resource::<Typing>();
        let name = if t.name.trim().is_empty() { "Warrior".to_string() } else { t.name.trim().to_string() };
        (name, t.code.trim().to_string())
    };
    let lobby = match choice {
        Choice::Solo => None,
        Choice::Join => {
            let Ok((_, map, map_seed)) = net::read_code(&code) else {
                world.resource_mut::<Typing>().error = Some(if code.is_empty() { "Type the room code first".into() } else { "That isn't a room code".into() });
                return;
            };
            go_online(world, map, map_seed, 0);
            Some(Lobby::join(name, true, code))
        }
        Choice::Host => {
            let map = world.resource::<MatchConfig>().map;
            let room = net::new_room_for(map);
            let (map, seed) = (net::room_map(&room), net::room_map_seed(&room));
            go_online(world, map, seed, 0);
            Some(Lobby::host(name, true, Some(room), None, 0, map, seed))
        }
    };
    for e in world.query_filtered::<Entity, With<Menu>>().iter(world).collect::<Vec<_>>() {
        world.despawn(e);
    }
    world.resource_mut::<MenuOpen>().0 = false;
    match lobby {
        // The lobby holds the match until everyone has loaded it.
        Some(l) => world.insert_resource(l),
        None => world.resource_mut::<Hold>().0 = false,
    }
}

/// The solo match standing behind the menu cleared, and the online match's map built (the room
/// names it and its seed): the lobby starts the match once everyone's in (net::begin).
fn go_online(world: &mut World, map: shared::map::MapId, map_seed: u64, bots: u32) {
    shared::sim::flow::clear_match(world);
    let mut config = *world.resource::<MatchConfig>();
    let rebuild = config.map != map || config.map_seed != map_seed;
    config.online = true;
    config.map_seed = map_seed;
    config.bots = bots;
    world.insert_resource(config);
    if rebuild {
        shared::sim::load_map(world, map);
    }
}
