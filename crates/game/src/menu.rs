//! The start screen: your name, the map, then play alone, host an online match (a room code to
//! share), or join one by its code. Drawn over the live city, which stands still behind it
//! (`Hold`): a title and the choices down the left, the maps on the right (a big preview of the
//! chosen one, cards to pick from). Skipped when the launch already says what to play
//! (VIVAD_HOST / VIVAD_JOIN, or the browser's ?host / ?join), for the automated runs (tours, the
//! armoury, feel and grip checks, scripted keys), and with VIVAD_MENU=0.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use crate::loading::{self, Loading};
use net::Lobby;
use shared::map::MapId;
use shared::sim::{Hold, MatchConfig};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // With the screen up, the map is built behind the loading screen (not at startup, when
        // nothing could be drawn yet and the window sat black).
        if !skipped() {
            app.insert_resource(shared::sim::DeferMap);
        }
        app.init_resource::<MenuOpen>()
            .add_systems(Startup, open)
            .init_resource::<HiddenHud>()
            .add_systems(Update, hide_hud.run_if(|m: Res<MenuOpen>| m.0))
            .add_systems(Update, (pointer, typing, refresh).chain().run_if(|m: Res<MenuOpen>, l: Option<Res<Loading>>| m.0 && l.is_none()));
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

/// A button's colour, and its colour under the pointer.
#[derive(Component, Clone, Copy)]
struct Look(Color, Color);

/// A map's card in the picker.
#[derive(Component, Clone, Copy)]
struct Card(MapId);

/// The chosen map's big preview, and its name and line under it.
#[derive(Component)]
struct Preview;
#[derive(Component)]
struct PreviewName;
#[derive(Component)]
struct PreviewLine;

#[derive(Resource)]
struct Typing {
    name: String,
    code: String,
    focus: Field,
    error: Option<String>,
    map: MapId,
    /// The map the preview shows (it changes when `map` does).
    shown: Option<MapId>,
}


/// The HUD's panels, hidden while the screen is up (put back when it goes).
#[derive(Resource, Default)]
struct HiddenHud(Vec<Entity>);

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

use crate::ui::{EDGE, FIELD, GLASS, GLASS_HOVER, GOLD, INK, MUTED, SAFFRON, SAFFRON_HOVER};

/// The maps the picker offers: name, a line about it (Hinglish, as the game talks), and its
/// card's picture (`just menu-thumbs`). The NCR (4 km, about 2 GB of GPU memory) on the desktop
/// only, until its memory comes down for phones and browser tabs.
const ALL_MAPS: [(MapId, &str, &str); 5] = [
    (MapId::Cp, "Connaught Place", "Gol chakkar, safed colonnade, har kone pe chai"),
    (MapId::Chandni, "Chandni Chowk", "Tang galiyaan, rangeen makaan, bheed hi bheed"),
    (MapId::HauzKhas, "Hauz Khas", "Jheel, purane khandhar aur rooftop cafe"),
    (MapId::IndiaGate, "India Gate", "Khula maidan, lambi lawns, kahin chhupne ki jagah nahi"),
    (MapId::Ncr, "Dilli NCR", "Poora NCR, 4 km: Karol Bagh se Cyber City tak, Yamuna paar"),
];

#[cfg(not(target_arch = "wasm32"))]
const MAPS: &[(MapId, &str, &str)] = &ALL_MAPS;
#[cfg(target_arch = "wasm32")]
const MAPS: &[(MapId, &str, &str)] = ALL_MAPS.split_at(4).0;

fn thumb(map: MapId) -> String {
    format!("ui/maps/{}.png", map.name())
}

fn open(
    mut commands: Commands,
    mut menu: ResMut<MenuOpen>,
    mut hold: ResMut<Hold>,
    lobby: Option<Res<Lobby>>,
    config: Res<MatchConfig>,
    assets: Res<AssetServer>,
    font: Res<crate::ui::UiFonts>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    if lobby.is_some() || skipped() {
        return;
    }
    menu.0 = true;
    hold.0 = true;
    commands.queue(|w: &mut World| {
        let map = w.resource::<MatchConfig>().map;
        loading::load(w, map, shared::sim::load_deferred);
    });
    #[cfg(not(target_arch = "wasm32"))]
    let name = std::env::var("VIVAD_NAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Warrior".into());
    #[cfg(target_arch = "wasm32")]
    let name = String::from("Warrior");
    let map = if MAPS.iter().any(|m| m.0 == config.map) { config.map } else { MapId::Cp };
    commands.insert_resource(Typing { name, code: String::new(), focus: Field::Name, error: None, map, shown: None });
    // Everything in pixels at a 900 px tall window, scaled to this one (a phone's is smaller).
    let k = crate::ui::scale(&window);
    let z = |v: f32| px(v * k);
    let text = move |f: &Handle<Font>, s: &str, size: f32, c: Color| crate::ui::text(f, s, size, k, c);
    let field = |p: &mut ChildSpawnerCommands, f: Field| {
        p.spawn((
            f,
            Node { width: percent(100.0), padding: UiRect::axes(z(16.0), z(11.0)), border: UiRect::all(z(2.0)), border_radius: BorderRadius::all(z(10.0)), ..default() },
            BackgroundColor(FIELD),
            BorderColor::all(EDGE),
        ))
        .with_children(|b| {
            b.spawn((FieldText(f), text(&font.semi, "", 22.0, Color::WHITE)));
        });
    };
    let button = |p: &mut ChildSpawnerCommands, label: &str, sub: &str, c: Choice, look: Look| {
        p.spawn((
            c,
            look,
            Node {
                width: percent(100.0),
                padding: UiRect::axes(z(20.0), z(12.0)),
                margin: UiRect::top(z(12.0)),
                flex_direction: FlexDirection::Column,
                border_radius: BorderRadius::all(z(12.0)),
                border: UiRect::all(z(1.0)),
                ..default()
            },
            BackgroundColor(look.0),
            BorderColor::all(EDGE),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.35), z(0.0), z(4.0), z(0.0), z(12.0)),
        ))
        .with_children(|b| {
            b.spawn(text(&font.bold, label, 26.0, Color::WHITE));
            if !sub.is_empty() {
                b.spawn(text(&font.body, sub, 15.0, Color::srgba(1.0, 1.0, 1.0, 0.8)));
            }
        });
    };
    let label = |p: &mut ChildSpawnerCommands, s: &str, top: f32| {
        p.spawn((text(&font.semi, s, 14.0, MUTED), Node { margin: UiRect::new(z(2.0), z(0.0), z(top), z(6.0)), ..default() }));
    };
    commands
        .spawn((
            Menu,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                padding: UiRect::axes(percent(5.0), percent(4.0)),
                column_gap: percent(4.0),
                align_items: AlignItems::Center,
                ..default()
            },
            // The city shows through on the right; the choices sit on the dark left.
            BackgroundGradient::from(LinearGradient::to_right(vec![
                ColorStop::percent(INK, 0.0),
                ColorStop::percent(Color::srgba(0.03, 0.035, 0.06, 0.82), 38.0),
                ColorStop::percent(Color::srgba(0.03, 0.035, 0.06, 0.35), 100.0),
            ])),
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            // The left: the title, your name, the three ways in.
            root.spawn(Node { width: percent(34.0), flex_direction: FlexDirection::Column, ..default() }).with_children(|p| {
                p.spawn(text(&font.bold, "PANGA", 112.0, GOLD));
                p.spawn((text(&font.semi, "Dilli ka battle royale. Utro, loot karo, aakhri tak tiko.", 18.0, MUTED), Node { margin: UiRect::bottom(z(26.0)), ..default() }));
                label(p, "TUMHARA NAAM", 0.0);
                field(p, Field::Name);
                button(p, "Akele khelo", "Play solo, against bots", Choice::Solo, Look(SAFFRON, SAFFRON_HOVER));
                button(p, "Dost bulao", "Host online: get a room code to share", Choice::Host, Look(GLASS, GLASS_HOVER));
                label(p, "CODE MILA? JOIN KARO", 22.0);
                p.spawn(Node { width: percent(100.0), column_gap: z(10.0), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                    r.spawn(Node { flex_grow: 1.0, ..default() }).with_children(|f| field(f, Field::Code));
                    r.spawn((
                        Choice::Join,
                        Look(GLASS, GLASS_HOVER),
                        Node { padding: UiRect::axes(z(22.0), z(13.0)), border_radius: BorderRadius::all(z(10.0)), border: UiRect::all(z(1.0)), ..default() },
                        BackgroundColor(GLASS),
                        BorderColor::all(EDGE),
                    ))
                    .with_children(|b| {
                        b.spawn(text(&font.bold, "Join", 20.0, Color::WHITE));
                    });
                });
                p.spawn((ErrorText, text(&font.semi, "", 16.0, Color::srgb(1.0, 0.5, 0.45)), Node { margin: UiRect::top(z(10.0)), ..default() }));
            });
            // The right: the chosen map, big, and the cards.
            root.spawn(Node { width: percent(58.0), flex_direction: FlexDirection::Column, ..default() }).with_children(|p| {
                label(p, "MAP CHUNO", 0.0);
                // A 16:9 frame the picture fills (as the frame's child: an image node takes its
                // picture's own size, whatever its aspect is told).
                p.spawn((
                    Node { width: percent(100.0), aspect_ratio: Some(16.0 / 9.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::FlexEnd, border_radius: BorderRadius::all(z(16.0)), overflow: Overflow::clip(), ..default() },
                    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), z(0.0), z(10.0), z(0.0), z(28.0)),
                ))
                .with_children(|v| {
                    v.spawn((
                        Preview,
                        ImageNode::new(assets.load(thumb(map))),
                        Node { position_type: PositionType::Absolute, left: px(0.0), top: px(0.0), width: percent(100.0), height: percent(100.0), ..default() },
                        Pickable::IGNORE,
                    ));
                    // The name and line over a shade along the picture's foot.
                    v.spawn((
                        Node { width: percent(100.0), padding: UiRect::new(z(26.0), z(26.0), z(40.0), z(20.0)), flex_direction: FlexDirection::Column, ..default() },
                        BackgroundGradient::from(LinearGradient::to_top(vec![ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.75), 0.0), ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 100.0)])),
                        Pickable::IGNORE,
                    ))
                    .with_children(|t| {
                        t.spawn((PreviewName, text(&font.bold, "", 40.0, Color::WHITE)));
                        t.spawn((PreviewLine, text(&font.body, "", 17.0, Color::srgba(1.0, 1.0, 1.0, 0.85))));
                    });
                });
                p.spawn(Node { width: percent(100.0), column_gap: percent(2.0), margin: UiRect::top(z(16.0)), ..default() }).with_children(|row| {
                    for &(id, title, _) in MAPS {
                        row.spawn((
                            Card(id),
                            Node { width: percent((100.0 - 2.0 * (MAPS.len() - 1) as f32) / MAPS.len() as f32), flex_direction: FlexDirection::Column, padding: UiRect::all(z(5.0)), border: UiRect::all(z(2.0)), border_radius: BorderRadius::all(z(12.0)), ..default() },
                            BackgroundColor(GLASS),
                            BorderColor::all(EDGE),
                        ))
                        .with_children(|c| {
                            c.spawn((Node { width: percent(100.0), aspect_ratio: Some(16.0 / 9.0), border_radius: BorderRadius::all(z(8.0)), overflow: Overflow::clip(), ..default() }, Pickable::IGNORE)).with_children(|f| {
                                f.spawn((
                                    ImageNode::new(assets.load(thumb(id))),
                                    Node { position_type: PositionType::Absolute, left: px(0.0), top: px(0.0), width: percent(100.0), height: percent(100.0), ..default() },
                                    Pickable::IGNORE,
                                ));
                            });
                            c.spawn((text(&font.semi, title, 15.0, Color::WHITE), Node { margin: UiRect::new(z(4.0), z(0.0), z(6.0), z(2.0)), ..default() }));
                        });
                    }
                });
            });
        });
}

/// The buttons, fields and cards hit-tested here, not by picking: picking finds what a press lands
/// on from the previous frame's hover, so while the map is still loading (frames a second long)
/// the move, press and release all fall in one frame and the first click was lost.
#[allow(clippy::type_complexity)]
fn pointer(
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    fields: Query<(&Field, &ComputedNode, &UiGlobalTransform)>,
    mut buttons: Query<(&Choice, &Look, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor), (Without<Field>, Without<Card>)>,
    cards: Query<(&Card, &ComputedNode, &UiGlobalTransform)>,
    mut t: ResMut<Typing>,
    mut commands: Commands,
) {
    let scale = window.scale_factor();
    let hover = window.physical_cursor_position();
    let touched = touches.iter_just_released().map(|f| f.position() * scale).next();
    let tap = touched.or(hover.filter(|_| mouse.just_released(MouseButton::Left)));
    let hit = |node: &ComputedNode, at: &UiGlobalTransform, p: Option<Vec2>| p.is_some_and(|p| node.contains_point(*at, p));
    for (c, look, node, at, mut bg) in &mut buttons {
        let colour = if hit(node, at, hover) { look.1 } else { look.0 };
        if bg.0 != colour {
            bg.0 = colour;
        }
        if hit(node, at, tap) {
            commands.run_system_cached_with(choose, *c);
        }
    }
    for (f, node, at) in &fields {
        if hit(node, at, tap) {
            t.focus = *f;
            #[cfg(target_arch = "wasm32")]
            if hit(node, at, touched) {
                let now = if *f == Field::Name { t.name.clone() } else { t.code.clone() };
                if let Some(text) = ask(*f, &now) {
                    let s = if *f == Field::Name { &mut t.name } else { &mut t.code };
                    s.clear();
                    put(s, *f, text.trim());
                }
            }
        }
    }
    for (card, node, at) in &cards {
        if hit(node, at, tap) && t.map != card.0 {
            t.map = card.0;
            t.error = None;
        }
    }
}

/// Keys into the focused field (letters, digits, dashes; Backspace; Tab switches; Enter acts;
/// Ctrl/Cmd+V pastes).
fn typing(mut keys: MessageReader<KeyboardInput>, held: Res<ButtonInput<KeyCode>>, mut t: ResMut<Typing>, mut commands: Commands) {
    let mut act = None;
    let command = held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
    #[cfg(target_arch = "wasm32")]
    if let Some(text) = PASTED.lock().unwrap().take() {
        let focus = t.focus;
        let s = if focus == Field::Name { &mut t.name } else { &mut t.code };
        put(s, focus, text.trim());
    }
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
            Key::Character(c) if command => {
                if c.eq_ignore_ascii_case("v") {
                    let pasted = paste();
                    put(s, focus, pasted.trim());
                }
            }
            Key::Character(c) => put(s, focus, c),
            _ => {}
        }
    }
    if let Some(c) = act {
        commands.run_system_cached_with(choose, c);
    }
}

/// The clipboard's text. In the browser it comes later: the read is asked for here (a key press
/// lets the page read the clipboard, perhaps after the browser asks), and its answer lands in
/// `PASTED` for `typing` to take.
fn paste() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(w) = web_sys::window() {
            let read = wasm_bindgen_futures::JsFuture::from(w.navigator().clipboard().read_text());
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(text) = read.await
                    && let Some(text) = text.as_string()
                {
                    *PASTED.lock().unwrap() = Some(text);
                }
            });
        }
        String::new()
    }
}

/// The browser's clipboard text, once it has answered `paste`'s read.
#[cfg(target_arch = "wasm32")]
static PASTED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// On a phone in the browser the page can't raise the keyboard over the game: a tapped field asks
/// with the browser's own prompt instead (its keyboard, and paste), the field's text to start.
#[cfg(target_arch = "wasm32")]
fn ask(field: Field, now: &str) -> Option<String> {
    let q = if field == Field::Name { "Tumhara naam (your name)" } else { "Room code (paste or type it)" };
    web_sys::window()?.prompt_with_message_and_default(q, now).ok().flatten()
}

/// Typed or pasted text into a field, keeping only what a name or a room code can hold.
fn put(s: &mut String, focus: Field, text: &str) {
    for ch in text.chars() {
        let ok = ch.is_alphanumeric() || ch == '-' || (focus == Field::Name && (ch == '_' || ch == '.'));
        if ok && s.len() < if focus == Field::Name { 20 } else { 200 } {
            s.push(if focus == Field::Code { ch.to_ascii_lowercase() } else { ch });
        }
    }
}

#[allow(clippy::type_complexity)]
fn refresh(
    mut t: ResMut<Typing>,
    mut texts: Query<(&FieldText, &mut Text, &mut TextColor)>,
    mut fields: Query<(&Field, &mut BorderColor), Without<Card>>,
    mut cards: Query<(&Card, &mut BorderColor, &mut BackgroundColor), Without<Field>>,
    mut err: Query<&mut Text, (With<ErrorText>, Without<FieldText>, Without<PreviewName>, Without<PreviewLine>)>,
    mut preview: Query<&mut ImageNode, With<Preview>>,
    mut name: Query<&mut Text, (With<PreviewName>, Without<FieldText>, Without<PreviewLine>)>,
    mut line: Query<&mut Text, (With<PreviewLine>, Without<FieldText>, Without<PreviewName>)>,
    assets: Res<AssetServer>,
) {
    for (f, mut text, mut colour) in &mut texts {
        let s = if f.0 == Field::Name { &t.name } else { &t.code };
        // An empty code field shows what a code looks like, dimmed so it doesn't read as one.
        let hint = s.is_empty() && f.0 == Field::Code;
        let shown = if hint { format!("{}e.g. chai-tiger-kulfi-42", if t.focus == f.0 { "|" } else { "" }) } else if t.focus == f.0 { format!("{s}|") } else { s.clone() };
        if text.0 != shown {
            text.0 = shown;
        }
        let c = if hint { Color::srgba(1.0, 1.0, 1.0, 0.35) } else { Color::WHITE };
        if colour.0 != c {
            colour.0 = c;
        }
    }
    for (f, mut edge) in &mut fields {
        let c = if *f == t.focus { GOLD } else { EDGE };
        if edge.top != c {
            *edge = BorderColor::all(c);
        }
    }
    for (card, mut edge, mut bg) in &mut cards {
        let on = card.0 == t.map;
        let (c, b) = if on { (GOLD, Color::srgba(1.0, 0.82, 0.42, 0.18)) } else { (EDGE, GLASS) };
        if edge.top != c {
            *edge = BorderColor::all(c);
            bg.0 = b;
        }
    }
    for mut e in &mut err {
        let s = t.error.clone().unwrap_or_default();
        if e.0 != s {
            e.0 = s;
        }
    }
    if t.shown != Some(t.map) {
        t.shown = Some(t.map);
        let (_, title, about) = MAPS.iter().find(|m| m.0 == t.map).copied().unwrap_or(MAPS[0]);
        for mut img in &mut preview {
            img.image = assets.load(thumb(t.map));
        }
        for mut n in &mut name {
            n.0 = title.into();
        }
        for mut l in &mut line {
            l.0 = about.into();
        }
    }
}

/// The HUD (every other panel at the top of the UI) out of the way under the screen; the loading
/// screen and the 3D view (drawn as a UI image, crate::quality) left alone.
#[allow(clippy::type_complexity)]
fn hide_hud(
    mut panels: Query<(Entity, &mut Visibility), (With<Node>, Without<ChildOf>, Without<Menu>, Without<loading::Screen>, Without<crate::quality::ViewImage>)>,
    mut hidden: ResMut<HiddenHud>,
) {
    for (e, mut v) in &mut panels {
        if *v != Visibility::Hidden {
            *v = Visibility::Hidden;
            hidden.0.push(e);
        }
    }
}

/// Do what was chosen: play on alone (on the chosen map: a fresh match there, built behind the
/// loading screen, if it isn't the one behind the menu), or go online (the solo match cleared, the
/// map the room's: the chosen one when hosting, the code's when joining).
fn choose(In(choice): In<Choice>, world: &mut World) {
    let (name, code, chosen) = {
        let t = world.resource::<Typing>();
        let name = if t.name.trim().is_empty() { "Warrior".to_string() } else { t.name.trim().to_string() };
        (name, t.code.trim().to_string(), t.map)
    };
    let online = match choice {
        Choice::Solo => None,
        Choice::Join => {
            let Ok((_, map, map_seed)) = net::read_code(&code) else {
                world.resource_mut::<Typing>().error = Some(if code.is_empty() { "Pehle room code daalo (type the room code first)".into() } else { "Yeh room code nahi lagta (that isn't a room code)".into() });
                return;
            };
            Some((map, map_seed, Lobby::join(name, true, code)))
        }
        Choice::Host => {
            // A room's code names one of the Delhi maps, and a browser tab can't hold the NCR yet.
            if !MapId::DELHI.contains(&chosen) {
                world.resource_mut::<Typing>().error = Some("NCR abhi sirf akele (the NCR is solo only for now)".into());
                return;
            }
            let room = net::new_room_for(chosen);
            let (map, seed) = (net::room_map(&room), net::room_map_seed(&room));
            Some((map, seed, Lobby::host(name, true, Some(room), None, 0, map, seed)))
        }
    };
    for e in world.query_filtered::<Entity, With<Menu>>().iter(world).collect::<Vec<_>>() {
        world.despawn(e);
    }
    world.resource_mut::<MenuOpen>().0 = false;
    for e in std::mem::take(&mut world.resource_mut::<HiddenHud>().0) {
        if let Ok(mut v) = world.get_entity_mut(e) {
            v.insert(Visibility::Inherited);
        }
    }
    match online {
        // The lobby holds the match until everyone has loaded it.
        Some((map, seed, lobby)) => loading::load(world, map, move |w| {
            go_online(w, map, seed, 0);
            w.insert_resource(lobby);
        }),
        None if world.resource::<MatchConfig>().map != chosen => loading::load(world, chosen, move |w| {
            shared::sim::flow::clear_match(w);
            shared::sim::load_map(w, chosen);
            shared::sim::flow::start_match(w);
            w.resource_mut::<Hold>().0 = false;
        }),
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
