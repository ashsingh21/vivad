//! HUD: health/shield, hotbar, reticle + hit markers, prompts, kill feed, the map, place names,
//! Pralaya timer and the performance overlay. The result of a match is crate::front's.
//!
//! The map is a picture of the real ground baked from `MapLayout`: a corner map round the player
//! and the whole map on demand (M), with the place names, the Aandhi's ring, the safe circle and
//! the way to it. N steps either map's zoom; on the whole map the wheel zooms round the cursor and
//! a drag moves it, the cursor being free while it is open. The picture is made from OpenStreetMap
//! data, so the whole-map view carries the credit the licence asks for.

use bevy::asset::RenderAssetUsages;
use bevy::diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;
use bevy::window::{CursorGrabMode, CursorOptions};
use shared::items::{ItemDb, ItemKind};
use shared::lines::{Lines, with};
use shared::map::terrain::ground;
use shared::map::{ChestKind, MapLayout, MapStyle, Mat, Poi, Shape, compose, footprint};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, INTERACT_RANGE, Pickup};
use shared::sim::net::NetId;
use shared::sim::zone::Zone;
use shared::sim::{Action, Eliminated, FeedMsg, Health, HitMsg, INVENTORY_SLOTS, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, Warrior, btn};

use crate::fonts::UiFonts;
use crate::front::Screen;
use crate::input::{CYCLE, ShowPerf};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("VIVAD_CLEAN_CAPTURE").is_ok() {
            app.add_plugins((FrameTimeDiagnosticsPlugin::default(), EntityCountDiagnosticsPlugin::default()));
            return;
        }
        app.add_plugins((FrameTimeDiagnosticsPlugin::default(), EntityCountDiagnosticsPlugin::default()))
            .init_resource::<Feed>()
            .init_resource::<MapView>()
            .add_systems(Startup, build_hud)
            // Before the local input, so the wheel is used up before it changes weapons. The keys
            // are for a match: on a menu they do nothing, and leaving a match shuts the whole map.
            .add_systems(PreUpdate, map_keys.in_set(MapKeys).after(bevy::input::InputSystems).before(crate::input::LocalInput).run_if(in_state(Screen::Match)))
            .add_systems(PreUpdate, map_cursor.after(crate::input::LocalInput).run_if(in_state(Screen::Match)))
            .add_systems(OnExit(Screen::Match), shut_big_map)
            .add_systems(Update, (vitals, hotbar, prompts, feed, reticle, crosshair_at_scope, hit_marker, top_right, minimap, map_names, place_banner, perf))
            // The first frame and each map swap; not each match, which only borrows the layout.
            .add_systems(Update, refresh_map.run_if(resource_changed::<shared::sim::MapSwaps>))
            .add_systems(PostUpdate, (damage_numbers, hurt_markers).after(crate::camera::CameraSystems));
    }
}

/// Reads the map's keys and mouse; a tool that presses keys for a shot runs before it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapKeys;

/// The corner map's side, in pixels on screen, its border included.
const MINIMAP: f32 = 200.0;
/// The whole-map view's side, as a share of the window's shorter side.
const BIG_MAP: f32 = 0.84;
/// The border round both maps. The picture fills the frame inside it.
const MAP_BORDER: f32 = 2.0;
/// The whole-map view's zoom steps; each is twice the one before, from the whole map.
const BIG_ZOOMS: usize = 3;
/// Pixels of a trackpad's scroll that make one wheel step.
const SCROLL_STEP: f32 = 40.0;
/// The maps are made from OpenStreetMap, whose licence (ODbL) asks for this wherever one is shown.
const MAP_CREDIT: &str = "Map data © OpenStreetMap contributors";

#[derive(Component)] struct HpBar;
#[derive(Component)] struct ShieldBar;
#[derive(Component)] struct HpText;
#[derive(Component)] struct SlotBox(usize);
#[derive(Component)] struct SlotText(usize);
#[derive(Component)] struct AmmoText;
#[derive(Component)] struct Prompt;
#[derive(Component)] struct PromptLore;
#[derive(Component)] struct FeedText;
/// One arm of the reticle, pointing along `0` (screen space, +y down) from the centre.
#[derive(Component)] struct ReticleBar(Vec2);
/// One diagonal tick of the hit marker.
#[derive(Component)] struct HitTick;
#[derive(Component)] struct Crosshair;
#[derive(Component)] struct TopRight;
/// The sheet the map is drawn on: the picture, which zooms by its size and scrolls by a transform
/// (no layout, no pixel snapping). The names and rings sit in percent of it, and the moving marks
/// by transforms in its pixels. There are two, the corner map's and the whole-map view's.
#[derive(Component)] struct MapSheet { big: bool }
/// The whole-map view, shown and hidden by M.
#[derive(Component)] struct BigMap;
/// The whole-map view's frame, sized to the window.
#[derive(Component)] struct BigMapFrame;
/// Holds a sheet's names, drawn under the rings and the player.
#[derive(Component)] struct MapNames { big: bool }
/// A place's name on a sheet. `major`: a district, not a smaller spot.
#[derive(Component)] struct MapName { big: bool, major: bool }
#[derive(Component)] struct MapZone;
#[derive(Component)] struct MapSafe;
#[derive(Component)] struct MapMe { big: bool }
/// The tick off the player's dot, showing where they face.
#[derive(Component)] struct MapFacing { big: bool }
/// The line from the player to the safe circle, while outside it.
#[derive(Component)] struct MapRun { big: bool }
/// The drop ride's line across the map, during the flight.
#[derive(Component)] struct MapFlight { big: bool }
/// The place's name, big, across the top of the screen as you come into it.
#[derive(Component)] struct PlaceName;
#[derive(Component)] struct PlaceRule;
#[derive(Component)] struct PerfText;
#[derive(Component)] struct ProgressBar;
#[derive(Component)] struct ProgressFill;

/// Damage dealt by the local player, floating up from where it landed.
#[derive(Component)]
struct DamageNumber {
    target: Entity,
    at: Vec3,
    amount: f32,
    size: f32,
    age: f32,
}

/// A red bar round the crosshair pointing at whoever just hit the local player.
#[derive(Component)]
struct HurtMarker {
    from: Entity,
    last: Vec3,
    age: f32,
}

/// How the map is being looked at.
#[derive(Resource, Default)]
struct MapView {
    /// Which step of `zooms` the corner map is on.
    zoom: usize,
    /// How far in the whole-map view is: 1, 2 or 4 times.
    big_zoom: usize,
    /// The whole-map view is open.
    big: bool,
    /// Where a drag or the wheel left the middle of the whole-map view; else it follows the player.
    big_focus: Option<Vec2>,
    /// A scroll not yet worth a whole wheel step.
    scrolled: f32,
    /// The picture, kept alive for as long as this map lasts.
    picture: Option<Handle<Image>>,
}

impl MapView {
    /// The whole-map view as it stands.
    fn big_view(&self, half: f32, panel: f32, me: Vec2) -> Scrolled {
        Scrolled::new(panel, 2.0 * half / (1 << self.big_zoom) as f32, half, self.big_focus.unwrap_or(me))
    }
}

/// The whole-map frame's side in pixels, its border included.
fn big_side(window: Option<&Window>) -> f32 {
    window.map_or(800.0, |w| (w.width().min(w.height()) * BIG_MAP).round())
}

/// Metres the corner map shows across at each zoom step (N steps through them), first the one a
/// match starts on. On the 4 km map a whole-map corner would be 0.05 pixels a metre, so it starts
/// close in, round the player.
fn zooms(half: f32) -> Vec<f32> {
    if half > 1000.0 { vec![800.0, 400.0, 2.0 * half, 1600.0] } else { vec![2.0 * half, half] }
}

#[derive(Resource, Default)]
struct Feed {
    lines: Vec<(String, f32)>,
    hit_flash: f32,
    headshot: bool,
    /// The last hit was the one that dropped them: red ticks, held longer.
    kill: bool,
}

fn text(s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

/// Text in a face of its own: Khand for numbers, Gajraj One for titles.
fn text_in(font: &Handle<Font>, s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font: font.into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn build_hud(mut commands: Commands, fonts: Res<UiFonts>) {
    let panel = Color::srgba(0.05, 0.04, 0.03, 0.55);
    // Vitals, bottom-left.
    commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(24.0), bottom: px(24.0), width: px(320.0),
                      flex_direction: FlexDirection::Column, row_gap: px(6.0), ..default() })
        .with_children(|p| {
            p.spawn((HpText, text_in(&fonts.number, "100 | 0", 22.0, Color::WHITE)));
            for (marker_shield, color) in [(true, Color::srgb(0.30, 0.65, 1.0)), (false, Color::srgb(0.35, 0.85, 0.35))] {
                p.spawn((Node { width: percent(100.0), height: px(14.0), ..default() }, BackgroundColor(panel)))
                    .with_children(|b| {
                        let mut e = b.spawn((Node { width: percent(0.0), height: percent(100.0), ..default() }, BackgroundColor(color)));
                        if marker_shield { e.insert(ShieldBar); } else { e.insert(HpBar); }
                    });
            }
        });
    // Hotbar, bottom-right.
    commands
        .spawn(Node { position_type: PositionType::Absolute, right: px(24.0), bottom: px(24.0),
                      flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: px(6.0), ..default() })
        .with_children(|p| {
            p.spawn((AmmoText, text_in(&fonts.number, "", 22.0, Color::WHITE)));
            p.spawn(Node { column_gap: px(6.0), ..default() }).with_children(|row| {
                for i in 0..=INVENTORY_SLOTS {
                    row.spawn((SlotBox(i),
                               Node { width: px(86.0), height: px(64.0), border: UiRect::all(px(2.0)), padding: UiRect::all(px(4.0)),
                                      flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, ..default() },
                               BackgroundColor(panel), BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2))))
                        .with_children(|s| {
                            s.spawn(text_in(&fonts.number, &format!("{}", i + 1), 13.0, Color::srgba(1.0, 1.0, 1.0, 0.5)));
                            s.spawn((SlotText(i), text("", 12.0, Color::WHITE)));
                        });
                }
            });
        });
    // Crosshair.
    commands
        .spawn((Crosshair, Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0),
                                  justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
        .with_children(|p| {
            // A zero-size anchor at the screen centre; the reticle's pieces hang off it.
            p.spawn(Node { width: px(0.0), height: px(0.0), ..default() }).with_children(|c| {
                let outline = Outline { width: px(1.0), offset: px(0.0), color: Color::srgba(0.0, 0.0, 0.0, 0.55) };
                c.spawn((Node { position_type: PositionType::Absolute, left: px(-1.5), top: px(-1.5), width: px(3.0), height: px(3.0), ..default() },
                         BackgroundColor(RETICLE), outline));
                for d in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                    c.spawn((ReticleBar(d), Node { position_type: PositionType::Absolute, ..default() }, BackgroundColor(RETICLE), outline));
                }
                for d in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0)] {
                    let at = d.normalize() * (HIT_TICK_AT + HIT_TICK_LEN / 2.0);
                    c.spawn((
                        HitTick,
                        Node { position_type: PositionType::Absolute, left: px(at.x - HIT_TICK_LEN / 2.0), top: px(at.y - 1.0),
                               width: px(HIT_TICK_LEN), height: px(2.0), ..default() },
                        UiTransform { rotation: Rot2::radians((d.y * d.x).signum() * std::f32::consts::FRAC_PI_4), ..default() },
                        BackgroundColor(Color::WHITE),
                        Outline { width: px(1.0), offset: px(0.0), color: Color::srgba(0.0, 0.0, 0.0, 0.5) },
                        Visibility::Hidden,
                    ));
                }
            });
        });
    // Prompt + use/reload progress, lower centre.
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100.0), bottom: px(150.0),
                      flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(6.0), ..default() })
        .with_children(|p| {
            p.spawn((Prompt, text("", 20.0, Color::srgb(1.0, 0.92, 0.7))));
            p.spawn((PromptLore, text("", 15.0, Color::srgba(1.0, 0.95, 0.85, 0.8))));
            p.spawn((ProgressBar, Node { width: px(220.0), height: px(8.0), ..default() }, BackgroundColor(panel), Visibility::Hidden))
                .with_children(|b| {
                    b.spawn((ProgressFill, Node { width: percent(0.0), height: percent(100.0), ..default() }, BackgroundColor(Color::srgb(1.0, 0.8, 0.3))));
                });
        });
    // Kill feed, top-left.
    commands.spawn((FeedText, text("", 15.0, Color::WHITE),
                    Node { position_type: PositionType::Absolute, left: px(24.0), top: px(20.0), max_width: px(520.0), ..default() }));
    // Corner map + match info, top-right.
    commands
        .spawn(Node { position_type: PositionType::Absolute, right: px(24.0), top: px(20.0),
                      flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: px(8.0), ..default() })
        .with_children(|p| {
            p.spawn((Node { width: px(MINIMAP), height: px(MINIMAP), border: UiRect::all(px(MAP_BORDER)), overflow: Overflow::clip(), ..default() },
                     BackgroundColor(Color::srgba(0.32, 0.38, 0.22, 0.8)), BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6))))
                .with_children(|m| map_sheet(m, false));
            p.spawn(text(MAP_CREDIT, 11.0, Color::srgba(1.0, 1.0, 1.0, 0.75)));
            p.spawn((TopRight, text("", 16.0, Color::WHITE), TextLayout::justify(Justify::Right)));
        });
    // The whole map, in the middle of the screen, over the rest of the HUD.
    commands
        .spawn((BigMap, Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0),
                               flex_direction: FlexDirection::Column, row_gap: px(6.0),
                               justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), GlobalZIndex(5), Visibility::Hidden))
        .with_children(|p| {
            p.spawn((BigMapFrame, RelativeCursorPosition::default(), Node { border: UiRect::all(px(MAP_BORDER)), overflow: Overflow::clip(), ..default() },
                     BackgroundColor(Color::srgb(0.32, 0.38, 0.22)), BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.5))))
                .with_children(|m| map_sheet(m, true));
            // Two short lines, which clear the hotbar at the foot of the screen.
            p.spawn((text("M closes   Wheel zooms   Drag moves   N zooms round you\nMap data (c) OpenStreetMap contributors, ODbL 1.0", 13.0, Color::srgba(1.0, 1.0, 1.0, 0.75)), TextLayout::justify(Justify::Center)));
        });
    // The place you have come into, across the top.
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100.0), top: percent(13.0),
                      flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(6.0), ..default() })
        .with_children(|p| {
            p.spawn((PlaceName, text("", 34.0, Color::NONE), TextShadow { offset: Vec2::splat(2.0), color: Color::NONE }));
            p.spawn((PlaceRule, Node { width: px(220.0), height: px(2.0), ..default() }, BackgroundColor(Color::NONE)));
        });
    // Performance overlay (F3).
    commands.spawn((PerfText, text("", 14.0, Color::srgb(0.6, 1.0, 0.6)),
                    Node { position_type: PositionType::Absolute, left: px(24.0), bottom: px(110.0), ..default() }, Visibility::Hidden));
}

/// A sheet of the map; `minimap` sizes it and moves what is on it.
fn map_sheet(frame: &mut ChildSpawnerCommands, big: bool) {
    let dot = if big { 11.0 } else { 8.0 };
    let abs = Node { position_type: PositionType::Absolute, ..default() };
    frame.spawn((MapSheet { big }, ImageNode::default(), UiTransform::default(), abs.clone())).with_children(|m| {
        m.spawn((MapNames { big }, Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() }));
        m.spawn((MapZone, Node { position_type: PositionType::Absolute, border: UiRect::all(px(2.0)), border_radius: BorderRadius::MAX, ..default() },
                 BorderColor::all(Color::srgb(1.0, 0.4, 0.15))));
        m.spawn((MapSafe, Node { position_type: PositionType::Absolute, border: UiRect::all(px(1.5)), border_radius: BorderRadius::MAX, ..default() },
                 BorderColor::all(Color::WHITE)));
        m.spawn((MapFlight { big }, Node { height: px(2.0), ..abs.clone() }, UiTransform::default(), BackgroundColor(Color::srgba(1.0, 0.85, 0.3, 0.9)), Visibility::Hidden));
        m.spawn((MapRun { big }, Node { height: px(1.5), ..abs.clone() }, UiTransform::default(), BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)), Visibility::Hidden));
        m.spawn((MapFacing { big }, Node { width: px(2.0), height: px(dot + 2.0), ..abs.clone() }, UiTransform::default(), BackgroundColor(Color::WHITE)));
        m.spawn((MapMe { big }, Node { width: px(dot), height: px(dot), border_radius: BorderRadius::MAX, ..abs.clone() }, UiTransform::default(),
                 BackgroundColor(Color::srgb(1.0, 0.6, 0.1)), Outline { width: px(1.0), offset: px(0.0), color: Color::srgba(0.0, 0.0, 0.0, 0.7) }));
    });
}

fn vitals(
    q: Query<&Health, With<LocalPlayer>>,
    mut hp: Query<&mut Node, (With<HpBar>, Without<ShieldBar>)>,
    mut sh: Query<&mut Node, (With<ShieldBar>, Without<HpBar>)>,
    mut t: Query<&mut Text, With<HpText>>,
    lines: Res<Lines>,
) {
    let Ok(h) = q.single() else { return };
    for mut n in &mut hp { n.width = percent(100.0 * h.hp / h.max_hp); }
    for mut n in &mut sh { n.width = percent(100.0 * h.shield / h.max_shield); }
    let (hp, sh) = (format!("{:.0}", h.hp.ceil()), format!("{:.0}", h.shield.ceil()));
    for mut s in &mut t { set_text(&mut s, with(lines.text("hud.vitals"), &[("hp", &hp), ("sh", &sh)])); }
}

fn hotbar(
    q: Query<(&Inventory, &Action), With<LocalPlayer>>,
    db: Res<ItemDb>,
    mut boxes: Query<(&SlotBox, &mut BorderColor, &mut BackgroundColor)>,
    mut texts: Query<(&SlotText, &mut Text), Without<AmmoText>>,
    mut ammo: Query<&mut Text, With<AmmoText>>,
    lines: Res<Lines>,
) {
    let Ok((inv, act)) = q.single() else { return };
    CYCLE.with(|c| c.set(inv.selected as u8));
    let stack_of = |i: usize| if i == 0 { Some(&inv.melee) } else { inv.slots[i - 1].as_ref() };
    for (SlotBox(i), mut border, mut bg) in &mut boxes {
        let s = stack_of(*i);
        let c = s.map(|s| s.rarity.color()).unwrap_or(Color::srgba(1.0, 1.0, 1.0, 0.15));
        *border = BorderColor::all(if *i == inv.selected { Color::WHITE } else { c });
        bg.0 = s.map(|s| s.rarity.color().with_alpha(if *i == inv.selected { 0.45 } else { 0.22 }))
            .unwrap_or(Color::srgba(0.05, 0.04, 0.03, 0.55));
    }
    for (SlotText(i), mut t) in &mut texts {
        let new = match stack_of(*i) {
            Some(s) => {
                let d = db.get(&s.id);
                match d.kind {
                    ItemKind::Melee { durability: 0, .. } => d.name.clone(),
                    _ => format!("{}\n{}", d.name, s.count),
                }
            }
            None => String::new(),
        };
        set_text(&mut t, new);
    }
    for mut t in &mut ammo {
        let cur = inv.current();
        let goli = with(lines.text("hud.ammo"), &[("n", &inv.arrows.to_string())]);
        let new = match cur.map(|s| (s, &db.get(&s.id).kind)) {
            Some((s, ItemKind::Bow { magazine, .. })) => {
                let state = if act.reload_left > 0.0 { lines.text("hud.reload") } else if act.charge > 0.0 { lines.text("hud.draw") } else { "" };
                format!("{} / {}   {goli}  {state}", s.count, magazine)
            }
            _ => goli,
        };
        set_text(&mut t, new);
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn prompts(
    layout: Res<shared::map::MapLayout>,
    q: Query<(&Transform, &Kinematic, &Inventory, &Action, Has<Eliminated>, &NetId), With<LocalPlayer>>,
    chests: Query<(&Transform, &Chest)>,
    pickups: Query<(&Transform, &Pickup)>,
    db: Res<ItemDb>,
    lines: Res<Lines>,
    m: Res<Match>,
    mut prompt: Query<&mut Text, (With<Prompt>, Without<PromptLore>)>,
    mut prompt_lore: Query<&mut Text, (With<PromptLore>, Without<Prompt>)>,
    mut bar: Query<&mut Visibility, With<ProgressBar>>,
    mut fill: Query<&mut Node, With<ProgressFill>>,
    mut misfires: MessageReader<shared::sim::combat::MisfireMsg>,
    local: Query<Entity, With<LocalPlayer>>,
    time: Res<Time>,
    mut toast: Local<(String, f32)>,
) {
    // The katta letting the player down, shown over the usual prompt for a moment.
    let now = time.elapsed_secs();
    for m in misfires.read() {
        if local.single().is_ok_and(|e| e == m.who) {
            *toast = (lines.text(if m.backfire { "prompt.backfire" } else { "prompt.misfire" }).to_string(), now + 1.4);
        }
    }
    let Ok((tf, kin, inv, act, dead, id)) = q.single() else { return };
    let me = tf.translation;
    let mut s = String::new();
    let mut lore = String::new();
    let mut progress: Option<f32> = None;
    let key = id.0 as u64;
    if dead {
    } else if kin.mode == MoveMode::Vimana {
        let over = me.x.abs() < layout.half_extent - 10.0 && me.z.abs() < layout.half_extent - 10.0;
        s = if m.vimana_t < 0.05 {
            lines.text("drop.ride").into()
        } else if over && m.state == MatchState::Flight {
            lines.pick(if m.vimana_t > 0.8 { "drop.end" } else { "drop.jump" }, m.seed, key).into()
        } else {
            with(lines.pick("drop.far", m.seed, key), &[("map", &layout.name)])
        };
    } else if matches!(kin.mode, MoveMode::Freefall | MoveMode::Gliding) {
        let low = me.y - layout.terrain.height(me.x, me.z) < 40.0;
        s = if low { lines.text("drop.ground") } else if kin.mode == MoveMode::Freefall { lines.text("drop.open") } else { "" }.into();
    } else if let Some((_, c)) = chests.iter().filter(|(t, c)| !c.opened && t.translation.distance(me) < INTERACT_RANGE).next() {
        s = lines.text(match c.kind {
            ChestKind::Peti => "prompt.peti",
            ChestKind::Kalash => "prompt.ghada",
        }).into();
    } else if let Some((_, p)) = pickups
        .iter()
        .filter(|(t, _)| t.translation.distance(me) < INTERACT_RANGE)
        .min_by(|a, b| a.0.translation.distance(me).total_cmp(&b.0.translation.distance(me)))
    {
        let item = db.get(&p.stack.id);
        s = with(lines.text("prompt.pickup"), &[("item", &item.name), ("rarity", lines.rarity(p.stack.rarity))]);
        lore = item.lore.clone();
    } else if let Some((st, ItemKind::Bow { magazine, .. })) = inv.current().map(|s| (s, &db.get(&s.id).kind)) {
        if act.reload_left <= 0.0 && st.count == 0 && inv.arrows == 0 {
            s = lines.text("prompt.empty").into();
        } else if act.reload_left <= 0.0 && st.count * 4 <= *magazine && inv.arrows > 0 {
            s = lines.text("prompt.reload").into();
        }
    }
    if let Some((slot, left)) = act.using {
        if let Some(st) = inv.slots[slot - 1].as_ref() {
            if let ItemKind::Consumable { use_time, .. } = db.get(&st.id).kind {
                s = with(lines.text("prompt.drink"), &[("item", &db.get(&st.id).name)]);
                lore.clear();
                progress = Some(1.0 - left / use_time);
            }
        }
    }
    if act.reload_left > 0.0 {
        if let Some(ItemKind::Bow { reload, .. }) = inv.current().map(|s| &db.get(&s.id).kind) {
            progress = Some(1.0 - act.reload_left / reload);
        }
    }
    if act.charge > 0.0 {
        if let Some(ItemKind::Bow { charge, .. }) = inv.current().map(|s| &db.get(&s.id).kind) {
            progress = Some(act.charge / charge);
        }
    }
    if now < toast.1 {
        s = toast.0.clone();
    }
    for mut t in &mut prompt { set_text(&mut t, s.clone()); }
    for mut t in &mut prompt_lore { set_text(&mut t, lore.clone()); }
    for mut v in &mut bar { *v = if progress.is_some() { Visibility::Inherited } else { Visibility::Hidden }; }
    for mut n in &mut fill { n.width = percent(100.0 * progress.unwrap_or(0.0)); }
}

fn feed(
    mut msgs: MessageReader<FeedMsg>,
    mut f: ResMut<Feed>,
    time: Res<Time>,
    m: Res<Match>,
    mut generation: Local<u32>,
    mut q: Query<&mut Text, With<FeedText>>,
) {
    let now = time.elapsed_secs();
    // A new match starts with a clean feed.
    if *generation != m.generation {
        *generation = m.generation;
        f.lines.clear();
    }
    for m in msgs.read() {
        f.lines.push((m.0.clone(), now));
    }
    f.lines.retain(|(_, t)| now - t < 8.0);
    let n = f.lines.len();
    if n > 6 {
        f.lines.drain(0..n - 6);
    }
    for mut t in &mut q {
        set_text(&mut t, f.lines.iter().map(|l| l.0.as_str()).collect::<Vec<_>>().join("\n"));
    }
}

const RETICLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const BAR_LEN: f32 = 7.0;
const BAR_W: f32 = 2.0;
/// Gap from the centre to each arm with no spread.
const BASE_GAP: f32 = 5.0;
const HIT_TICK_AT: f32 = 9.0;
const HIT_TICK_LEN: f32 = 8.0;

/// The reticle's arms sit as far out as the held bow's spread reaches on screen (wider for the
/// seven-arrow bow, tighter when aiming down the sights), and kick out briefly after each shot.
fn reticle(
    q: Query<(&Inventory, &Action, &PlayerIntent), With<LocalPlayer>>,
    db: Res<ItemDb>,
    windows: Query<&Window>,
    mut bars: Query<(&ReticleBar, &mut Node)>,
) {
    let Ok((inv, act, intent)) = q.single() else { return };
    let h = windows.iter().next().map(|w| w.height()).unwrap_or(900.0);
    let mut gap = BASE_GAP;
    if let Some(ItemKind::Bow { spread_deg, hip_spread, .. }) = inv.current().map(|s| &db.get(&s.id).kind) {
        let spread = (spread_deg + hip_spread + act.bloom).to_radians() * if intent.held(btn::AIM) { 0.6 } else { 1.0 };
        // Camera vertical field of view 70 degrees (camera.rs).
        gap += spread.tan() / 35f32.to_radians().tan() * h / 2.0;
    }
    gap += 6.0 * (1.0 - act.since_attack / 0.15).clamp(0.0, 1.0);
    for (bar, mut n) in &mut bars {
        let d = bar.0;
        let (w, l) = if d.x != 0.0 { (BAR_LEN, BAR_W) } else { (BAR_W, BAR_LEN) };
        let c = d * (gap + BAR_LEN / 2.0);
        n.left = px(c.x - w / 2.0);
        n.top = px(c.y - l / 2.0);
        n.width = px(w);
        n.height = px(l);
    }
}

/// A scope has its own reticle (crate::scope).
fn crosshair_at_scope(scope: Res<crate::scope::Scope>, mut q: Query<&mut Visibility, With<Crosshair>>) {
    if scope.is_changed() {
        for mut v in &mut q {
            *v = if scope.active() { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
}

/// Four diagonal ticks round the reticle when the local player's hit lands (gold for a headshot),
/// and larger red ones, held longer, for the hit that drops someone.
fn hit_marker(
    mut hits: MessageReader<HitMsg>,
    mut kills: MessageReader<shared::sim::combat::KillMsg>,
    local: Query<Entity, With<LocalPlayer>>,
    mut f: ResMut<Feed>,
    time: Res<Time>,
    mut q: Query<(&mut Visibility, &mut BackgroundColor, &mut UiTransform), With<HitTick>>,
) {
    let Ok(me) = local.single() else { return };
    for h in hits.read() {
        if h.attacker == me && h.target != me {
            f.hit_flash = 0.18;
            f.headshot = h.headshot;
            f.kill = false;
        }
    }
    for k in kills.read() {
        if k.killer == me {
            f.hit_flash = 0.5;
            f.kill = true;
        }
    }
    f.hit_flash = (f.hit_flash - time.delta_secs()).max(0.0);
    for (mut v, mut c, mut t) in &mut q {
        *v = if f.hit_flash > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        t.scale = Vec2::splat(if f.kill { 1.7 } else { 1.0 });
        c.0 = if f.kill { Color::srgb(1.0, 0.3, 0.2) } else if f.headshot { Color::srgb(1.0, 0.85, 0.2) } else { Color::WHITE };
    }
}

fn top_right(
    m: Res<Match>,
    zone: Res<Zone>,
    local: Query<(&Transform, &Warrior), With<LocalPlayer>>,
    layout: Res<MapLayout>,
    lines: Res<Lines>,
    mut q: Query<&mut Text, With<TopRight>>,
) {
    let Ok((tf, w)) = local.single() else { return };
    let (t, shrinking) = zone.countdown();
    let zone_line = if m.state != MatchState::InProgress {
        lines.text("hud.idle").to_string()
    } else {
        let clock = format!("{}:{:02}", t as u32 / 60, t as u32 % 60);
        with(lines.text(if shrinking { "hud.move" } else { "hud.wait" }), &[("t", &clock)])
    };
    let me = tf.translation.xz();
    let to_safe = (me.distance(zone.target_center) - zone.target_radius).max(0.0);
    let here = place_at(&layout, me).map(|p| p.name.as_str()).unwrap_or(layout.name.as_str());
    let safe = if to_safe > 1.0 { with(lines.text("hud.safe.far"), &[("d", &format!("{to_safe:.0}"))]) } else { lines.text("hud.safe.in").to_string() };
    let counts = with(lines.text("hud.counts"), &[("alive", &m.alive.to_string()), ("kills", &w.kills.to_string())]);
    for mut s in &mut q {
        set_text(&mut s, format!("{here}\n{counts}\n{zone_line}\n{safe}"));
    }
}

/// The named place a point stands in. NCR's districts are squares, so a corner counts as inside. A
/// bridge's name is only for those on it, not for the river banks round its middle.
fn place_at(layout: &MapLayout, p: Vec2) -> Option<&Poi> {
    let square = layout.style == MapStyle::Ncr;
    if square && let Some(bridge) = compose::bridge_at(p) {
        return layout.pois.iter().find(|q| q.name == bridge);
    }
    let bridge = |q: &&Poi| square && compose::BRIDGES.contains(&q.name.as_str());
    layout.pois.iter().filter(|q| !bridge(q)).find(|q| if square { (q.center.xz() - p).abs().max_element() < q.radius } else { q.center.xz().distance(p) < q.radius })
}

/// M opens and closes the whole map, and Esc closes it. N zooms whichever map is up, the whole map
/// round the player. On the whole map the wheel zooms round the cursor and a drag moves the map.
#[allow(clippy::too_many_arguments)]
fn map_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: ResMut<AccumulatedMouseScroll>,
    frame: Query<&RelativeCursorPosition, With<BigMapFrame>>,
    windows: Query<&Window>,
    layout: Res<MapLayout>,
    local: Query<&Transform, With<LocalPlayer>>,
    mut view: ResMut<MapView>,
    mut grabbed_at: Local<Option<Vec2>>,
) {
    if keys.just_pressed(KeyCode::KeyN) {
        if view.big {
            view.big_zoom = (view.big_zoom + 1) % BIG_ZOOMS;
            view.big_focus = None;
        } else {
            view.zoom += 1;
        }
    }
    // With the map open, Esc is used up here: it shuts the map, not opens the menu as well.
    if keys.just_pressed(KeyCode::KeyM) || (view.big && keys.clear_just_pressed(KeyCode::Escape)) {
        view.big = !view.big;
        view.big_focus = None;
    }
    if !view.big {
        *grabbed_at = None;
        return;
    }
    let side = big_side(windows.iter().next());
    let panel = side - 2.0 * MAP_BORDER;
    let half = layout.half_extent;
    let me = local.single().map_or(Vec2::ZERO, |t| t.translation.xz());
    // The cursor in the panel's pixels, while it is over the panel.
    let spot = frame.single().ok().and_then(|f| f.normalized).filter(|n| n.abs().max_element() <= 0.5).map(|n| (n + 0.5) * side - MAP_BORDER);
    let shown = view.big_view(half, panel, me);

    // The wheel: whole steps, so a trackpad's many small scrolls do not jump to the end. It is
    // used up here, or it would change weapons behind the map.
    view.scrolled += match wheel.unit {
        MouseScrollUnit::Line => wheel.delta.y,
        MouseScrollUnit::Pixel => wheel.delta.y / SCROLL_STEP,
    };
    wheel.delta = Vec2::ZERO;
    let steps = view.scrolled.trunc();
    view.scrolled -= steps;
    let zoom = (view.big_zoom as i32 + steps as i32).clamp(0, BIG_ZOOMS as i32 - 1) as usize;
    if zoom != view.big_zoom {
        let at = spot.unwrap_or(Vec2::splat(panel / 2.0));
        view.big_zoom = zoom;
        let next = view.big_view(half, panel, me);
        view.big_focus = (zoom > 0).then(|| shown.keeping(at, next.scale));
    }

    if mouse.just_pressed(MouseButton::Left) {
        *grabbed_at = spot;
    }
    if !mouse.pressed(MouseButton::Left) {
        *grabbed_at = None;
    }
    if let (Some(from), Some(to)) = (*grabbed_at, spot)
        && from != to
    {
        let dragged = view.big_view(half, panel, me);
        view.big_focus = Some(dragged.middle() - (to - from) / dragged.scale);
        *grabbed_at = spot;
    }
    // Past the map's edge there is nothing to drag to.
    if let Some(focus) = view.big_focus {
        view.big_focus = Some(Scrolled::new(panel, 2.0 * half / (1 << view.big_zoom) as f32, half, focus).middle());
    }
}

/// Whatever comes after a match, the whole map is not left open behind it.
fn shut_big_map(mut view: ResMut<MapView>) {
    view.big = false;
    view.big_focus = None;
}

/// While the whole map is open the cursor is free to drag it, so the view stops following the
/// mouse and the gun stops firing, and it goes back as it was when the map shuts. Any click takes
/// the cursor (`grab_cursor`), so this runs after that and gives it back the same frame.
fn map_cursor(view: Res<MapView>, mut cursor: Single<&mut CursorOptions>, mut before: Local<Option<CursorGrabMode>>) {
    if view.big {
        if cursor.grab_mode != CursorGrabMode::None {
            before.get_or_insert(cursor.grab_mode);
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    } else if let Some(mode) = before.take() {
        cursor.grab_mode = mode;
        cursor.visible = false;
    }
}

/// Bakes the map's picture and lays out its names, whenever the map changes.
fn refresh_map(
    mut commands: Commands,
    layout: Res<MapLayout>,
    mut view: ResMut<MapView>,
    mut images: ResMut<Assets<Image>>,
    mut sheets: Query<&mut ImageNode, With<MapSheet>>,
    holders: Query<(Entity, &MapNames)>,
    old: Query<Entity, With<MapName>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    view.zoom = 0;
    view.big_zoom = 0;
    view.big_focus = None;
    if layout.terrain.ground.is_empty() {
        return;
    }
    let clock = bevy::platform::time::Instant::now();
    let mut picture = map_picture(&layout);
    crate::buildings::mip_chain(&mut picture);
    info!("map picture baked in {:.0} ms", clock.elapsed().as_secs_f32() * 1000.0);
    let picture = images.add(picture);
    for mut n in &mut sheets {
        n.image = picture.clone();
    }
    view.picture = Some(picture);
    let half = layout.half_extent;
    let at = |c: f32| (c + half) / (2.0 * half) * 100.0;
    for (holder, names) in &holders {
        let name_size = |p: &Poi| {
            let major = layout.style != MapStyle::Ncr || compose::PLACES.iter().any(|d| d.name == p.name);
            (major, if names.big { if major { 15.0 } else { 12.0 } } else if major { 11.0 } else { 10.0 })
        };
        // The big names first, so that the small ones are the ones to move.
        let mut order: Vec<&Poi> = layout.pois.iter().collect();
        order.sort_by_key(|p| !name_size(p).0);
        let sizes: Vec<(Vec2, f32, f32)> = order
            .iter()
            .map(|p| {
                let size = name_size(p).1;
                (Vec2::new(at(p.center.x), at(p.center.z)), p.name.chars().count() as f32 * size * NAME_WIDE, size * 1.3)
            })
            .collect();
        for (p, (back, drop)) in order.iter().zip(name_spots(&sizes)) {
            let (major, size) = name_size(p);
            let shade = if major { Color::WHITE } else { Color::srgb(0.9, 0.92, 0.84) };
            commands.spawn((
                MapName { big: names.big, major },
                text(&p.name, size, shade),
                TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.95) },
                TextLayout::no_wrap(),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(at(p.center.x)),
                    top: percent(at(p.center.z)),
                    // On the corner map names are few at a time, and move with the zoom.
                    margin: UiRect::top(px(if names.big { drop } else { 0.0 })),
                    ..default()
                },
                UiTransform::from_translation(Val2::percent(-back, -50.0)),
                ChildOf(holder),
            ));
        }
    }
}

/// A character's width as a share of the type size, near enough for the HUD's font.
const NAME_WIDE: f32 = 0.62;
/// The whole map's usual size on screen, pixels, at which names are kept apart.
const NAME_PANEL: f32 = 740.0;
/// How near the map's side, in percent, a name reads inward instead of round its place.
const NAME_EDGE: f32 = 12.0;

/// Where to set each name, given its place in percent of the map, its width and its height in
/// pixels: how much of the name, in percent, lies to the left of its place, and how many pixels to
/// move it down. One by the map's side reads inward, so it isn't cut off, and one that would print
/// over a name before it moves off a line or more.
fn name_spots(names: &[(Vec2, f32, f32)]) -> Vec<(f32, f32)> {
    let mut taken: Vec<Rect> = vec![];
    names
        .iter()
        .map(|&(at, wide, high)| {
            let back = if at.x < NAME_EDGE {
                0.0
            } else if at.x > 100.0 - NAME_EDGE {
                100.0
            } else {
                50.0
            };
            let left = at.x * 0.01 * NAME_PANEL - wide * back * 0.01;
            let box_at = |drop: f32| {
                let top = at.y * 0.01 * NAME_PANEL + drop - high * 0.5;
                Rect::new(left, top, left + wide, top + high)
            };
            let drop = [0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0].iter().map(|k| k * high).find(|d| taken.iter().all(|r| r.intersect(box_at(*d)).is_empty())).unwrap_or(0.0);
            taken.push(box_at(drop));
            (back, drop)
        })
        .collect()
}

/// The map as a picture, north up: the ground's classes, hills, trees, buildings and bridges. Big
/// enough that the closest zoom on the 4 km map is sharp; mipmapped for the whole-map views.
fn map_picture(layout: &MapLayout) -> Image {
    let (t, half) = (&layout.terrain, layout.half_extent);
    let side: usize = match (half > 1000.0, cfg!(target_arch = "wasm32")) {
        (true, false) => 2048,
        (true, true) => 1024,
        _ => 512,
    };
    let world = |k: usize| (k as f32 + 0.5) / side as f32 * 2.0 * half - half;
    let cell = |c: f32| (((c + half) / (2.0 * half) * side as f32).max(0.0) as usize).min(side - 1);
    let mut px = vec![255u8; side * side * 4];
    let light = Vec3::new(-0.55, 0.62, -0.55).normalize();
    for j in 0..side {
        for i in 0..side {
            let (x, z) = (world(i), world(j));
            let c: [f32; 3] = match t.ground_at(x, z) {
                Some(ground::ROAD) => [70.0, 74.0, 80.0],
                Some(ground::PARK) => [172.0, 186.0, 136.0],
                Some(ground::WATER) => [76.0, 146.0, 168.0],
                Some(ground::PATH) => [194.0, 174.0, 134.0],
                Some(ground::PARKING) => [136.0, 136.0, 136.0],
                Some(ground::PLAZA) => [205.0, 200.0, 190.0],
                Some(ground::OPEN) => [186.0, 184.0, 138.0],
                Some(ground::SAND) => [212.0, 194.0, 148.0],
                _ => [216.0, 210.0, 196.0],
            };
            // Hills: lit from the north-west and darker with height, so the ridge reads.
            let h = t.height(x, z);
            let (shade, deep) = if h.abs() > 0.4 { ((t.normal(x, z).dot(light) / light.y).clamp(0.72, 1.22), 1.0 - h.clamp(0.0, 40.0) / 90.0) } else { (1.0, 1.0) };
            let k = (j * side + i) * 4;
            for n in 0..3 {
                px[k + n] = (c[n] * shade * deep).clamp(0.0, 255.0) as u8;
            }
        }
    }
    for p in &layout.props {
        let k = (cell(p.pos.z) * side + cell(p.pos.x)) * 4;
        px[k..k + 3].copy_from_slice(&[104, 128, 84]);
    }
    // Roofs and thick walls made of blocks rather than outlines, such as Connaught Place's ring.
    for b in layout.blocks.iter().filter(|b| b.collide && b.visible) {
        let Shape::Box(v) = b.shape else { continue };
        let floor = t.height(b.pos.x, b.pos.z);
        if v.x.min(v.z) < 1.5 || b.pos.y + v.y / 2.0 < floor + 2.0 || b.pos.y - v.y / 2.0 > floor + 14.0 {
            continue;
        }
        let (u, w) = (b.rot * Vec3::X, b.rot * Vec3::Z);
        let reach = Vec2::new(v.x, v.z).length() / 2.0;
        for j in cell(b.pos.z - reach)..=cell(b.pos.z + reach) {
            for i in cell(b.pos.x - reach)..=cell(b.pos.x + reach) {
                let d = Vec3::new(world(i) - b.pos.x, 0.0, world(j) - b.pos.z);
                if d.dot(u).abs() <= v.x / 2.0 && d.dot(w).abs() <= v.z / 2.0 {
                    let k = (j * side + i) * 4;
                    px[k..k + 3].copy_from_slice(&[96, 102, 112]);
                }
            }
        }
    }
    for f in layout.footprints.iter().filter(|f| f.mat != Mat::Water) {
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let d = 1.0 - (f.top / 160.0).clamp(0.0, 0.5);
        let c = [(66.0 * d) as u8, (72.0 * d) as u8, (84.0 * d) as u8];
        for j in cell(lo.y)..=cell(hi.y) {
            for i in cell(lo.x)..=cell(hi.x) {
                if footprint::contains(&f.pts, Vec2::new(world(i), world(j))) {
                    let k = (j * side + i) * 4;
                    px[k..k + 3].copy_from_slice(&c);
                }
            }
        }
    }
    if layout.style == MapStyle::Ncr {
        // The bridges' decks, pale over the water.
        for s in &compose::SPANS {
            let (dir, len) = ((s.c - s.a).normalize(), s.a.distance(s.c));
            let metre = (2.0 * half / side as f32).min(1.0);
            let (mut a, half_w) = (0.0, s.width / 2.0 + metre);
            while a <= len {
                let mut b = -half_w;
                while b <= half_w {
                    let q = s.a + dir * a + dir.perp() * b;
                    let k = (cell(q.y) * side + cell(q.x)) * 4;
                    px[k..k + 3].copy_from_slice(&[243, 239, 226]);
                    b += metre;
                }
                a += metre;
            }
        }
    }
    let size = Extent3d { width: side as u32, height: side as u32, depth_or_array_layers: 1 };
    Image::new(size, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// A sheet at its current zoom: its side in pixels, and how far it is scrolled.
#[derive(Clone, Copy)]
struct Scrolled {
    /// The panel's side in pixels.
    panel: f32,
    /// Pixels a metre.
    scale: f32,
    /// The panel's top-left corner on the sheet, in pixels.
    corner: Vec2,
    /// The map's half width.
    half: f32,
}

impl Scrolled {
    fn new(panel: f32, shows: f32, half: f32, focus: Vec2) -> Self {
        let scale = panel / shows;
        let room = Vec2::splat((2.0 * half * scale - panel).max(0.0));
        // Follow the focus, stopping at the map's edge so the panel never shows past it.
        Self { panel, scale, corner: ((focus + half) * scale - panel / 2.0).clamp(Vec2::ZERO, room), half }
    }

    /// A world point, in pixels on the sheet.
    fn on_sheet(&self, p: Vec2) -> Vec2 {
        (p + self.half) * self.scale
    }

    /// The world point at this spot in the panel.
    fn under(&self, spot: Vec2) -> Vec2 {
        (self.corner + spot) / self.scale - self.half
    }

    /// The world point in the middle of the panel.
    fn middle(&self) -> Vec2 {
        self.under(Vec2::splat(self.panel / 2.0))
    }

    /// Where the middle must be, at another scale, for what is at `spot` now to stay at `spot`.
    fn keeping(&self, spot: Vec2, scale: f32) -> Vec2 {
        self.under(spot) + (Vec2::splat(self.panel / 2.0) - spot) / scale
    }

    /// What of the line between two world points lies in the panel, as sheet pixels. Bevy's UI
    /// clip breaks on a rotated node, so the line is cut to the panel here instead.
    fn cut(&self, a: Vec2, b: Vec2) -> Option<(Vec2, Vec2)> {
        let (a, b) = (self.on_sheet(a), self.on_sheet(b));
        let d = b - a;
        let (mut from, mut to) = (0.0f32, 1.0f32);
        for axis in 0..2 {
            for (edge, sign) in [(self.corner[axis], 1.0), (self.corner[axis] + self.panel, -1.0)] {
                let inside = sign * (a[axis] - edge);
                let closing = sign * d[axis];
                if closing == 0.0 {
                    if inside < 0.0 {
                        return None;
                    }
                } else if closing > 0.0 {
                    from = from.max(-inside / closing);
                } else {
                    to = to.min(-inside / closing);
                }
            }
        }
        (from < to).then(|| (a + d * from, a + d * to))
    }
}

/// Places a thin node along a line on a sheet, or hides it.
fn place_line(n: &mut Node, ui: &mut UiTransform, v: &mut Visibility, thick: f32, line: Option<(Vec2, Vec2)>) {
    let want = if line.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if *v != want {
        *v = want;
    }
    if let Some((a, b)) = line {
        let (mid, d) = ((a + b) / 2.0, b - a);
        let length = d.length().round();
        if n.width != px(length) {
            n.width = px(length);
        }
        ui.translation = Val2::px(mid.x - length / 2.0, mid.y - thick / 2.0);
        ui.rotation = Rot2::radians(d.y.atan2(d.x));
    }
}

/// Moves and zooms the sheets, and places the rings, the player and the lines on them.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn minimap(
    zone: Res<Zone>,
    layout: Res<MapLayout>,
    m: Res<Match>,
    view: Res<MapView>,
    local: Query<(&Transform, &PlayerIntent, &Kinematic), With<LocalPlayer>>,
    windows: Query<&Window>,
    mut parts: ParamSet<(
        Query<(&MapSheet, &mut Node, &mut UiTransform)>,
        Query<&mut Node, With<BigMapFrame>>,
        Query<&mut Node, With<MapZone>>,
        Query<&mut Node, With<MapSafe>>,
        Query<(&MapMe, &mut UiTransform)>,
        Query<(&MapFacing, &mut UiTransform)>,
        Query<(&MapRun, &mut Node, &mut UiTransform, &mut Visibility)>,
        Query<(&MapFlight, &mut Node, &mut UiTransform, &mut Visibility)>,
    )>,
) {
    let half = layout.half_extent;
    let whole = 2.0 * half;
    let player = local.single().ok();
    let me = player.map_or(Vec2::ZERO, |(t, ..)| t.translation.xz());
    let across = corner_map_shows(half, &view, player.map(|p| p.2.mode));
    let side = big_side(windows.iter().next());
    for mut n in &mut parts.p1() {
        if n.width != px(side) {
            n.width = px(side);
            n.height = px(side);
        }
    }
    let corner_map = Scrolled::new(MINIMAP - 2.0 * MAP_BORDER, across, half, me);
    let big_map_view = view.big_view(half, side - 2.0 * MAP_BORDER, me);
    let scrolled = |big: bool| if big { big_map_view } else { corner_map };
    for (sheet, mut n, mut ui) in &mut parts.p0() {
        let s = scrolled(sheet.big);
        let size = px(whole * s.scale);
        if n.width != size {
            n.width = size;
            n.height = size;
        }
        ui.translation = Val2::px(-s.corner.x, -s.corner.y);
    }
    // The rings sit in percent of the sheet, so both sheets take the same numbers.
    let at = |c: f32| (c + half) / whole * 100.0;
    let circle = |n: &mut Node, c: Vec2, r: f32| {
        let (left, top, size) = (percent(at(c.x - r)), percent(at(c.y - r)), percent(2.0 * r / whole * 100.0));
        if n.left != left || n.top != top || n.width != size {
            n.left = left;
            n.top = top;
            n.width = size;
            n.height = size;
        }
    };
    for mut n in &mut parts.p2() { circle(&mut n, zone.center, zone.radius); }
    for mut n in &mut parts.p3() { circle(&mut n, zone.target_center, zone.target_radius); }
    if let Some((_, intent, _)) = player {
        for (mark, mut ui) in &mut parts.p4() {
            let (p, dot) = (scrolled(mark.big).on_sheet(me), if mark.big { 11.0 } else { 8.0 });
            ui.translation = Val2::px(p.x - dot / 2.0, p.y - dot / 2.0);
        }
        // Facing: the sim's yaw turns from north towards the west; screen rotation is clockwise.
        let ahead = Vec2::new(-intent.yaw.sin(), -intent.yaw.cos());
        for (mark, mut ui) in &mut parts.p5() {
            let (dot, long) = if mark.big { (11.0, 13.0) } else { (8.0, 10.0) };
            let p = scrolled(mark.big).on_sheet(me) + ahead * (dot / 2.0 + long / 2.0);
            ui.translation = Val2::px(p.x - 1.0, p.y - long / 2.0);
            ui.rotation = Rot2::radians(-intent.yaw);
        }
    }
    let to_safe = zone.target_center - me;
    let outside = player.is_some() && to_safe.length() > zone.target_radius;
    let edge = zone.target_center - to_safe.normalize_or_zero() * zone.target_radius;
    for (line, mut n, mut ui, mut v) in &mut parts.p6() {
        place_line(&mut n, &mut ui, &mut v, 1.5, outside.then(|| scrolled(line.big).cut(me, edge)).flatten());
    }
    let flying = m.state == MatchState::Flight;
    for (line, mut n, mut ui, mut v) in &mut parts.p7() {
        place_line(&mut n, &mut ui, &mut v, 2.0, flying.then(|| scrolled(line.big).cut(m.vimana_from.xz(), m.vimana_to.xz())).flatten());
    }
}

/// How many metres the corner map shows across just now: the chosen step on foot; in the air the
/// whole map matters, and under a glider the next kilometre.
fn corner_map_shows(half: f32, view: &MapView, mode: Option<MoveMode>) -> f32 {
    let steps = zooms(half);
    let chosen = steps[view.zoom % steps.len()];
    match mode {
        Some(MoveMode::Vimana | MoveMode::Freefall) | None => 2.0 * half,
        Some(MoveMode::Gliding) => chosen.max(1600.0).min(2.0 * half),
        _ => chosen,
    }
}

/// Shows and hides the whole-map view, and the names: all of them there; on the corner map the
/// districts once it is close enough to read them, the smaller spots closer still.
fn map_names(
    layout: Res<MapLayout>,
    view: Res<MapView>,
    local: Query<&Kinematic, With<LocalPlayer>>,
    mut big_map: Query<&mut Visibility, (With<BigMap>, Without<MapName>)>,
    mut names: Query<(&MapName, &mut Visibility)>,
) {
    let half = layout.half_extent;
    let across = corner_map_shows(half, &view, local.single().ok().map(|k| k.mode));
    for mut v in &mut big_map {
        let want = if view.big { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    for (name, mut v) in &mut names {
        let show = name.big || across <= 800.0 || (across <= 1600.0 && name.major) || half < 1000.0;
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// What the place banner is showing.
#[derive(Default)]
struct Banner {
    /// The last place named, kept while walking out of it so stepping back in stays quiet.
    name: String,
    /// Seconds since it was named.
    age: f32,
    /// Seconds spent outside it.
    away: f32,
}

/// Names a place across the top of the screen as you come into it, then fades.
fn place_banner(
    layout: Res<MapLayout>,
    time: Res<Time>,
    local: Query<(&Transform, &Kinematic, Has<Eliminated>), With<LocalPlayer>>,
    mut banner: Local<Banner>,
    mut name: Query<(&mut Text, &mut TextColor, &mut TextShadow), With<PlaceName>>,
    mut rule: Query<&mut BackgroundColor, With<PlaceRule>>,
) {
    const IN: f32 = 0.3;
    const HOLD: f32 = 2.6;
    const OUT: f32 = 0.8;
    /// Coming back within this many seconds does not name it again.
    const QUIET: f32 = 25.0;
    let dt = time.delta_secs();
    banner.age += dt;
    let Ok((tf, kin, dead)) = local.single() else { return };
    let here = place_at(&layout, tf.translation.xz()).map(|p| p.name.as_str());
    // On the way down it is named under the glider, not from the sky.
    let low = matches!(kin.mode, MoveMode::Ground | MoveMode::Gliding);
    match here {
        Some(place) if low && !dead && (place != banner.name || banner.away > QUIET) => {
            info!("coming into {place}");
            banner.name = place.to_owned();
            banner.age = 0.0;
            banner.away = 0.0;
            for (mut t, ..) in &mut name {
                set_text(&mut t, place.to_uppercase());
            }
        }
        Some(place) if place == banner.name => banner.away = 0.0,
        _ => banner.away += dt,
    }
    let a = (banner.age / IN).min(1.0) * (1.0 - ((banner.age - IN - HOLD) / OUT).clamp(0.0, 1.0));
    let a = if banner.name.is_empty() { 0.0 } else { a };
    for (_, mut c, mut shadow) in &mut name {
        let want = Color::srgba(1.0, 0.95, 0.82, a);
        if c.0 != want {
            c.0 = want;
            shadow.color = Color::srgba(0.0, 0.0, 0.0, 0.8 * a);
        }
    }
    for mut c in &mut rule {
        let want = Color::srgba(1.0, 0.75, 0.3, 0.8 * a);
        if c.0 != want {
            c.0 = want;
        }
    }
}

fn perf(
    show: Res<ShowPerf>,
    diag: Res<DiagnosticsStore>,
    quality: Res<crate::quality::Quality>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
    mut q: Query<(&mut Text, &mut Visibility), With<PerfText>>,
) {
    for (mut t, mut v) in &mut q {
        *v = if show.0 { Visibility::Inherited } else { Visibility::Hidden };
        if !show.0 {
            continue;
        }
        let fps = diag.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
        let ms = diag.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed()).unwrap_or(0.0);
        let ents = diag.get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT).and_then(|d| d.value()).unwrap_or(0.0);
        let mut line = format!("{fps:.0} fps   {ms:.2} ms   {ents:.0} entities   {quality:?} quality (F4)", quality = *quality);
        // The GPU, and the render passes that cost it most (natively; the browser has no timers).
        if let Some(a) = &adapter {
            line += &format!("\n{} ({:?})", a.name, a.backend);
        }
        let mut passes: Vec<(String, f64)> = diag
            .iter()
            .filter(|d| d.path().as_str().ends_with("/elapsed_gpu"))
            .filter_map(|d| Some((d.path().as_str().trim_start_matches("render/").trim_end_matches("/elapsed_gpu").to_string(), d.smoothed()?)))
            .collect();
        if !passes.is_empty() {
            let total: f64 = passes.iter().map(|p| p.1).sum();
            passes.sort_by(|a, b| b.1.total_cmp(&a.1));
            line += &format!("\nGPU {total:.1} ms:");
            for (name, t) in passes.iter().take(5) {
                line += &format!("  {name} {t:.1}");
            }
        }
        set_text(&mut t, line);
    }
}

/// Numbers for the local player's hits: white on flesh, blue on a ward (shield), gold for a
/// headshot. Hits on one target within a moment add up (the Sharanga's fan reads as one number).
#[allow(clippy::type_complexity)]
fn damage_numbers(
    mut commands: Commands,
    mut hits: MessageReader<HitMsg>,
    local: Query<Entity, With<LocalPlayer>>,
    cam: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    mut nums: Query<(Entity, &mut DamageNumber, &mut Node, &mut Text, &mut TextColor, &mut TextFont)>,
    time: Res<Time>,
    view_scale: Res<crate::quality::ViewScale>,
    fonts: Res<UiFonts>,
) {
    let Ok(me) = local.single() else { return };
    // This frame's hits per target (a volley lands in one tick), added to a recent number or new.
    let mut fresh: Vec<(Entity, Vec3, f32, bool, bool)> = vec![];
    for h in hits.read() {
        if h.attacker != me || h.target == me {
            continue;
        }
        match fresh.iter_mut().find(|f| f.0 == h.target) {
            Some(f) => {
                f.2 += h.amount;
                f.3 |= h.headshot;
                f.4 &= h.shield;
            }
            None => fresh.push((h.target, h.at, h.amount, h.headshot, h.shield)),
        }
    }
    for (target, at, amount, headshot, shield) in fresh {
        if let Some((_, mut n, ..)) = nums.iter_mut().find(|(_, n, ..)| n.target == target && n.age < 0.15) {
            n.amount += amount;
            continue;
        }
        let color = if headshot {
            Color::srgb(1.0, 0.82, 0.25)
        } else if shield {
            Color::srgb(0.55, 0.8, 1.0)
        } else {
            Color::WHITE
        };
        let size = if headshot { 30.0 } else { 24.0 };
        commands.spawn((
            DamageNumber { target, at: at + Vec3::Y * 0.5, amount, size, age: 0.0 },
            text_in(&fonts.number, "", size, color),
            TextShadow::default(),
            Node { position_type: PositionType::Absolute, ..default() },
            Visibility::Hidden,
        ));
    }
    let Ok((camera, cam_g)) = cam.single() else { return };
    let dt = time.delta_secs();
    for (e, mut n, mut node, mut t, mut c, mut font) in &mut nums {
        n.age += dt;
        if n.age > 0.9 {
            commands.entity(e).despawn();
            continue;
        }
        set_text(&mut t, format!("{}", n.amount.round() as i32));
        let rise = n.at + Vec3::Y * (n.age * 0.9);
        if let Ok(p) = camera.world_to_viewport(cam_g, rise).map(|p| p * view_scale.0) {
            node.left = px(p.x - 14.0);
            node.top = px(p.y - 14.0);
            commands.entity(e).insert(Visibility::Inherited);
        }
        // Pop in a touch larger, then settle and fade.
        font.font_size = FontSize::Px(n.size * (1.0 + 0.35 * (1.0 - n.age / 0.12).max(0.0)));
        c.0.set_alpha(1.0 - ((n.age - 0.55) / 0.35).clamp(0.0, 1.0));
    }
}

/// Markers round the crosshair for the local player's attackers; each follows its attacker as
/// the view turns and fades over a second.
#[allow(clippy::type_complexity)]
fn hurt_markers(
    mut commands: Commands,
    mut hits: MessageReader<HitMsg>,
    local: Query<(Entity, &Transform), With<LocalPlayer>>,
    sims: Query<&Transform, (With<Warrior>, Without<LocalPlayer>)>,
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    window: Query<&Window>,
    mut markers: Query<(Entity, &mut HurtMarker, &mut Node, &mut UiTransform, &mut BackgroundColor)>,
    time: Res<Time>,
) {
    const RADIUS: f32 = 95.0;
    let Ok((me, my_tf)) = local.single() else { return };
    for h in hits.read() {
        if h.target != me || h.attacker == me {
            continue;
        }
        let from = sims.get(h.attacker).map(|t| t.translation).unwrap_or(h.at);
        if let Some((_, mut m, ..)) = markers.iter_mut().find(|(_, m, ..)| m.from == h.attacker) {
            m.age = 0.0;
            m.last = from;
            continue;
        }
        commands.spawn((
            HurtMarker { from: h.attacker, last: from, age: 0.0 },
            Node { position_type: PositionType::Absolute, width: px(64.0), height: px(7.0), border_radius: BorderRadius::MAX, ..default() },
            UiTransform::default(),
            BackgroundColor(Color::srgba(0.95, 0.12, 0.08, 0.9)),
        ));
    }
    let (Ok(cam_g), Ok(win)) = (cam.single(), window.single()) else { return };
    let (fwd, right) = (cam_g.forward().as_vec3().with_y(0.0).normalize_or(Vec3::NEG_Z), cam_g.right().as_vec3().with_y(0.0).normalize_or(Vec3::X));
    let centre = Vec2::new(win.width(), win.height()) * 0.5;
    let dt = time.delta_secs();
    for (e, mut m, mut node, mut ui, mut bg) in &mut markers {
        m.age += dt;
        if m.age > 1.1 {
            commands.entity(e).despawn();
            continue;
        }
        if let Ok(t) = sims.get(m.from) {
            m.last = t.translation;
        }
        let d = m.last - my_tf.translation;
        let a = d.dot(right).atan2(d.dot(fwd));
        node.left = px(centre.x + RADIUS * a.sin() - 32.0);
        node.top = px(centre.y - RADIUS * a.cos() - 3.5);
        ui.rotation = Rot2::radians(a);
        bg.0.set_alpha(0.9 * (1.0 - ((m.age - 0.5) / 0.6).clamp(0.0, 1.0)));
    }
}


/// Set a text only if it changed: writing the same string still marks it changed, and a changed
/// text re-measures and re-lays out the UI (on the browser's one thread, every frame).
fn set_text(t: &mut Mut<Text>, s: String) {
    if t.as_str() != s {
        ***t = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF: f32 = 2000.0;

    #[test]
    fn a_zoom_keeps_the_point_under_the_cursor() {
        let shown = Scrolled::new(800.0, 2000.0, HALF, Vec2::new(-300.0, 500.0));
        let spot = Vec2::new(620.0, 90.0);
        let scale = 2.0 * shown.scale;
        let closer = Scrolled::new(800.0, 800.0 / scale, HALF, shown.keeping(spot, scale));
        assert!(closer.under(spot).distance(shown.under(spot)) < 0.01);
    }

    /// Every name on the NCR's whole map stays on the map, and none prints over another, and a
    /// bridge has its name underfoot.
    #[test]
    fn names_stay_on_the_map_and_apart() {
        let m = compose::generate(7);
        for (s, name) in compose::SPANS.iter().zip(compose::BRIDGES) {
            let middle = s.a.lerp(s.c, 0.5);
            assert_eq!(place_at(&m, middle).map(|p| p.name.as_str()), Some(name));
        }
        assert!(place_at(&m, Vec2::new(167.0, -528.0)).is_none_or(|p| !compose::BRIDGES.contains(&p.name.as_str())));
        let at = |c: f32| (c + m.half_extent) / (2.0 * m.half_extent) * 100.0;
        let mut order: Vec<&Poi> = m.pois.iter().collect();
        let major = |p: &Poi| compose::PLACES.iter().any(|d| d.name == p.name);
        order.sort_by_key(|p| !major(p));
        let names: Vec<(Vec2, f32, f32)> = order
            .iter()
            .map(|p| {
                let size = if major(p) { 15.0 } else { 12.0 };
                (Vec2::new(at(p.center.x), at(p.center.z)), p.name.chars().count() as f32 * size * NAME_WIDE, size * 1.3)
            })
            .collect();
        let boxes: Vec<Rect> = names
            .iter()
            .zip(name_spots(&names))
            .map(|(&(c, wide, high), (back, drop))| {
                let left = c.x * 0.01 * NAME_PANEL - wide * back * 0.01;
                let top = c.y * 0.01 * NAME_PANEL + drop - high * 0.5;
                Rect::new(left, top, left + wide, top + high)
            })
            .collect();
        for (k, b) in boxes.iter().enumerate() {
            assert!(b.min.x >= 0.0 && b.max.x <= NAME_PANEL, "{} leaves the map: {b:?}", order[k].name);
            for (j, other) in boxes[k + 1..].iter().enumerate() {
                assert!(b.intersect(*other).is_empty(), "{} prints over {}", order[k].name, order[k + 1 + j].name);
            }
        }
    }

    #[test]
    fn the_panel_stops_at_the_map_edge() {
        // Dragged far past the south-east corner: the panel's far corner meets the map's.
        let s = Scrolled::new(800.0, 1000.0, HALF, Vec2::splat(9000.0));
        assert!(s.under(Vec2::splat(800.0)).distance(Vec2::splat(HALF)) < 0.01);
        assert!(s.middle().abs().max_element() <= HALF);
        // The whole map cannot move at all.
        let whole = Scrolled::new(800.0, 2.0 * HALF, HALF, Vec2::new(500.0, -700.0));
        assert_eq!(whole.corner, Vec2::ZERO);
    }

    /// The whole map open, with a cursor at this spot on it, as `map_keys` and `map_cursor` see it.
    fn open_map(zoom: usize, cursor: Vec2) -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseScroll>()
            .insert_resource(MapLayout { half_extent: HALF, ..default() })
            .insert_resource(MapView { big: true, big_zoom: zoom, ..default() })
            .add_systems(Update, (map_keys, map_cursor).chain());
        app.world_mut().spawn((Window::default(), CursorOptions { grab_mode: CursorGrabMode::Locked, visible: false, ..default() }));
        app.world_mut().spawn((BigMapFrame, RelativeCursorPosition { cursor_over: true, normalized: Some(cursor) }));
        app
    }

    fn panel(app: &mut App) -> f32 {
        big_side(app.world_mut().query::<&Window>().iter(app.world()).next()) - 2.0 * MAP_BORDER
    }

    fn put_cursor(app: &mut App, cursor: Vec2) {
        for mut f in app.world_mut().query::<&mut RelativeCursorPosition>().iter_mut(app.world_mut()).collect::<Vec<_>>() {
            f.normalized = Some(cursor);
        }
    }

    #[test]
    fn the_wheel_zooms_round_the_cursor_and_is_used_up() {
        let cursor = Vec2::new(0.25, -0.1);
        let mut app = open_map(0, cursor);
        let side = panel(&mut app) + 2.0 * MAP_BORDER;
        let spot = (cursor + 0.5) * side - MAP_BORDER;
        let before = app.world().resource::<MapView>().big_view(HALF, side - 2.0 * MAP_BORDER, Vec2::ZERO).under(spot);
        app.world_mut().resource_mut::<AccumulatedMouseScroll>().delta.y = 1.0;
        app.update();
        let view = app.world().resource::<MapView>();
        assert_eq!(view.big_zoom, 1);
        assert!(view.big_view(HALF, side - 2.0 * MAP_BORDER, Vec2::ZERO).under(spot).distance(before) < 0.1);
        assert_eq!(app.world().resource::<AccumulatedMouseScroll>().delta, Vec2::ZERO, "the wheel must not reach the hotbar");
    }

    #[test]
    fn a_trackpad_takes_many_small_scrolls_to_make_a_step() {
        let mut app = open_map(0, Vec2::ZERO);
        let mut scroll = app.world_mut().resource_mut::<AccumulatedMouseScroll>();
        scroll.unit = MouseScrollUnit::Pixel;
        scroll.delta.y = SCROLL_STEP / 2.0 - 1.0;
        app.update();
        assert_eq!(app.world().resource::<MapView>().big_zoom, 0);
        app.world_mut().resource_mut::<AccumulatedMouseScroll>().delta.y = SCROLL_STEP / 2.0 + 2.0;
        app.update();
        assert_eq!(app.world().resource::<MapView>().big_zoom, 1);
    }

    #[test]
    fn a_drag_moves_the_map_with_the_cursor() {
        let mut app = open_map(2, Vec2::ZERO);
        let side = panel(&mut app) + 2.0 * MAP_BORDER;
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        // The press is a frame old now, as Bevy's own input system would leave it.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        put_cursor(&mut app, Vec2::new(0.1, 0.0));
        app.update();
        let view = app.world().resource::<MapView>();
        let scale = view.big_view(HALF, side - 2.0 * MAP_BORDER, Vec2::ZERO).scale;
        let moved = view.big_focus.expect("a drag leaves the view where it was put");
        assert!((moved.x + 0.1 * side / scale).abs() < 0.1 && moved.y.abs() < 0.1, "{moved}");
        // Let go, and a move of the cursor does nothing.
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        put_cursor(&mut app, Vec2::new(-0.2, 0.3));
        app.update();
        assert_eq!(app.world().resource::<MapView>().big_focus, Some(moved));
    }

    #[test]
    fn the_cursor_is_free_while_the_map_is_open_and_goes_back_after() {
        let mut app = open_map(0, Vec2::ZERO);
        app.update();
        let grab = |app: &mut App| app.world_mut().query::<&CursorOptions>().single(app.world()).expect("the window").grab_mode;
        assert_eq!(grab(&mut app), CursorGrabMode::None);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyM);
        app.update();
        assert!(!app.world().resource::<MapView>().big);
        assert_eq!(grab(&mut app), CursorGrabMode::Locked);
    }

    #[test]
    fn esc_shuts_the_map_and_goes_no_further() {
        let mut app = open_map(0, Vec2::ZERO);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<MapView>().big);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Escape), "the menu must not open as well");
    }

    #[test]
    fn a_line_is_cut_to_the_panel() {
        let s = Scrolled::new(200.0, 400.0, HALF, Vec2::ZERO);
        let (a, b) = s.cut(Vec2::new(-1000.0, 0.0), Vec2::new(1000.0, 0.0)).expect("it crosses the panel");
        assert!((a.x - s.corner.x).abs() < 0.01 && (b.x - s.corner.x - 200.0).abs() < 0.01);
        assert!(s.cut(Vec2::new(-1000.0, 900.0), Vec2::new(1000.0, 900.0)).is_none());
    }
}
