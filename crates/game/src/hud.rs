//! HUD: health/shield, hotbar, reticle + hit markers, prompts, kill feed, minimap,
//! Pralaya timer, match result screens and the performance overlay.

use bevy::diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use shared::items::{ItemDb, ItemKind};
use shared::map::{ChestKind, MapLayout};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, INTERACT_RANGE, Pickup};
use shared::sim::zone::Zone;
use shared::sim::{Action, Eliminated, FeedMsg, Health, HitMsg, INVENTORY_SLOTS, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, Warrior, btn};

use crate::input::{CYCLE, ShowPerf};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FrameTimeDiagnosticsPlugin::default(), EntityCountDiagnosticsPlugin::default()))
            .init_resource::<Feed>()
            .add_systems(Startup, build_hud)
            .add_systems(Update, (vitals, hotbar, prompts, feed, reticle, hit_marker, top_right, minimap, overlay, perf))
            .add_systems(PostUpdate, (damage_numbers, hurt_markers).after(crate::camera::CameraSystems));
    }
}

const MINIMAP: f32 = 190.0;

#[derive(Component)] struct HpBar;
#[derive(Component)] struct ShieldBar;
#[derive(Component)] struct HpText;
#[derive(Component)] struct SlotBox(usize);
#[derive(Component)] struct SlotText(usize);
#[derive(Component)] struct AmmoText;
#[derive(Component)] struct Prompt;
#[derive(Component)] struct FeedText;
/// One arm of the reticle, pointing along `0` (screen space, +y down) from the centre.
#[derive(Component)] struct ReticleBar(Vec2);
/// One diagonal tick of the hit marker.
#[derive(Component)] struct HitTick;
#[derive(Component)] struct TopRight;
#[derive(Component)] struct MapZone;
#[derive(Component)] struct MapSafe;
#[derive(Component)] struct MapMe;
#[derive(Component)] struct Overlay;
#[derive(Component)] struct OverlayText;
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

#[derive(Resource, Default)]
struct Feed {
    lines: Vec<(String, f32)>,
    hit_flash: f32,
    headshot: bool,
}

fn text(s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn build_hud(mut commands: Commands) {
    let panel = Color::srgba(0.05, 0.04, 0.03, 0.55);
    // Vitals, bottom-left.
    commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(24.0), bottom: px(24.0), width: px(320.0),
                      flex_direction: FlexDirection::Column, row_gap: px(6.0), ..default() })
        .with_children(|p| {
            p.spawn((HpText, text("100 | 0", 18.0, Color::WHITE)));
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
            p.spawn((AmmoText, text("", 18.0, Color::WHITE)));
            p.spawn(Node { column_gap: px(6.0), ..default() }).with_children(|row| {
                for i in 0..=INVENTORY_SLOTS {
                    row.spawn((SlotBox(i),
                               Node { width: px(86.0), height: px(64.0), border: UiRect::all(px(2.0)), padding: UiRect::all(px(4.0)),
                                      flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, ..default() },
                               BackgroundColor(panel), BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2))))
                        .with_children(|s| {
                            s.spawn(text(&format!("{}", i + 1), 11.0, Color::srgba(1.0, 1.0, 1.0, 0.5)));
                            s.spawn((SlotText(i), text("", 12.0, Color::WHITE)));
                        });
                }
            });
        });
    // Crosshair.
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0),
                      justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() })
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
            p.spawn((ProgressBar, Node { width: px(220.0), height: px(8.0), ..default() }, BackgroundColor(panel), Visibility::Hidden))
                .with_children(|b| {
                    b.spawn((ProgressFill, Node { width: percent(0.0), height: percent(100.0), ..default() }, BackgroundColor(Color::srgb(1.0, 0.8, 0.3))));
                });
        });
    // Kill feed, top-left.
    commands.spawn((FeedText, text("", 15.0, Color::WHITE),
                    Node { position_type: PositionType::Absolute, left: px(24.0), top: px(20.0), max_width: px(520.0), ..default() }));
    // Minimap + match info, top-right.
    commands
        .spawn(Node { position_type: PositionType::Absolute, right: px(24.0), top: px(20.0),
                      flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: px(8.0), ..default() })
        .with_children(|p| {
            p.spawn((Node { width: px(MINIMAP), height: px(MINIMAP), border: UiRect::all(px(2.0)), overflow: Overflow::clip(), ..default() },
                     BackgroundColor(Color::srgba(0.32, 0.38, 0.22, 0.8)), BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.6))))
                .with_children(|m| {
                    m.spawn((MapZone, Node { position_type: PositionType::Absolute, border: UiRect::all(px(2.0)), border_radius: BorderRadius::MAX, ..default() },
                             BorderColor::all(Color::srgb(1.0, 0.4, 0.15))));
                    m.spawn((MapSafe, Node { position_type: PositionType::Absolute, border: UiRect::all(px(1.5)), border_radius: BorderRadius::MAX, ..default() },
                             BorderColor::all(Color::WHITE)));
                    m.spawn((MapMe, Node { position_type: PositionType::Absolute, width: px(8.0), height: px(8.0), border_radius: BorderRadius::MAX, ..default() },
                             BackgroundColor(Color::srgb(1.0, 0.6, 0.1))));
                });
            p.spawn((TopRight, text("", 16.0, Color::WHITE), TextLayout::justify(Justify::Right)));
        });
    // End-of-match / eliminated overlay.
    commands
        .spawn((Overlay, Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0),
                                justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)), Visibility::Hidden))
        .with_children(|p| {
            p.spawn((OverlayText, text("", 40.0, Color::srgb(1.0, 0.85, 0.5)), TextLayout::justify(Justify::Center)));
        });
    // Performance overlay (F3).
    commands.spawn((PerfText, text("", 14.0, Color::srgb(0.6, 1.0, 0.6)),
                    Node { position_type: PositionType::Absolute, left: px(24.0), bottom: px(110.0), ..default() }, Visibility::Hidden));
}

fn vitals(
    q: Query<&Health, With<LocalPlayer>>,
    mut hp: Query<&mut Node, (With<HpBar>, Without<ShieldBar>)>,
    mut sh: Query<&mut Node, (With<ShieldBar>, Without<HpBar>)>,
    mut t: Query<&mut Text, With<HpText>>,
) {
    let Ok(h) = q.single() else { return };
    for mut n in &mut hp { n.width = percent(100.0 * h.hp / h.max_hp); }
    for mut n in &mut sh { n.width = percent(100.0 * h.shield / h.max_shield); }
    for mut s in &mut t { **s = format!("Jaan {:.0}   Kavach {:.0}", h.hp.ceil(), h.shield.ceil()); }
}

fn hotbar(
    q: Query<(&Inventory, &Action), With<LocalPlayer>>,
    db: Res<ItemDb>,
    mut boxes: Query<(&SlotBox, &mut BorderColor, &mut BackgroundColor)>,
    mut texts: Query<(&SlotText, &mut Text), Without<AmmoText>>,
    mut ammo: Query<&mut Text, With<AmmoText>>,
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
        **t = match stack_of(*i) {
            Some(s) => {
                let d = db.get(&s.id);
                match d.kind {
                    ItemKind::Melee { .. } => d.name.clone(),
                    _ => format!("{}\n{}", d.name, s.count),
                }
            }
            None => String::new(),
        };
    }
    for mut t in &mut ammo {
        let cur = inv.current();
        **t = match cur.map(|s| (s, &db.get(&s.id).kind)) {
            Some((s, ItemKind::Bow { magazine, .. })) => {
                let state = if act.reload_left > 0.0 { "  reloading" } else if act.charge > 0.0 { "  drawing" } else { "" };
                format!("{} / {}   Goli {}{}", s.count, magazine, inv.arrows, state)
            }
            _ => format!("Goli {}", inv.arrows),
        };
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn prompts(
    q: Query<(&Transform, &Kinematic, &Inventory, &Action, Has<Eliminated>), With<LocalPlayer>>,
    chests: Query<(&Transform, &Chest)>,
    pickups: Query<(&Transform, &Pickup)>,
    db: Res<ItemDb>,
    m: Res<Match>,
    mut prompt: Query<&mut Text, With<Prompt>>,
    mut bar: Query<&mut Visibility, With<ProgressBar>>,
    mut fill: Query<&mut Node, With<ProgressFill>>,
) {
    let Ok((tf, kin, inv, act, dead)) = q.single() else { return };
    let me = tf.translation;
    let mut s = String::new();
    let mut progress: Option<f32> = None;
    if dead {
    } else if kin.mode == MoveMode::Vimana {
        let over = me.x.abs() < 240.0 && me.z.abs() < 240.0;
        s = if over && m.state == MatchState::Flight {
            "[Space] Vimana se chhalang lagao".into()
        } else {
            "Pushpaka Vimana Bharat ki taraf...".into()
        };
    } else if kin.mode == MoveMode::Freefall {
        s = "[Space] Garuda pankh kholo".into();
    } else if let Some((_, c)) = chests.iter().filter(|(t, c)| !c.opened && t.translation.distance(me) < INTERACT_RANGE).next() {
        s = match c.kind {
            ChestKind::Peti => "[E] Peti kholo".into(),
            ChestKind::Kalash => "[E] Sone ka Kalash kholo".into(),
        };
    } else if let Some((_, p)) = pickups
        .iter()
        .filter(|(t, _)| t.translation.distance(me) < INTERACT_RANGE)
        .min_by(|a, b| a.0.translation.distance(me).total_cmp(&b.0.translation.distance(me)))
    {
        s = format!("[E] {} uthao ({:?})", db.get(&p.stack.id).name, p.stack.rarity);
    }
    if let Some((slot, left)) = act.using {
        if let Some(st) = inv.slots[slot - 1].as_ref() {
            if let ItemKind::Consumable { use_time, .. } = db.get(&st.id).kind {
                s = format!("{} pee rahe ho...", db.get(&st.id).name);
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
    for mut t in &mut prompt { **t = s.clone(); }
    for mut v in &mut bar { *v = if progress.is_some() { Visibility::Inherited } else { Visibility::Hidden }; }
    for mut n in &mut fill { n.width = percent(100.0 * progress.unwrap_or(0.0)); }
}

fn feed(
    mut msgs: MessageReader<FeedMsg>,
    mut f: ResMut<Feed>,
    time: Res<Time>,
    mut q: Query<&mut Text, With<FeedText>>,
) {
    let now = time.elapsed_secs();
    for m in msgs.read() {
        f.lines.push((m.0.clone(), now));
    }
    f.lines.retain(|(_, t)| now - t < 8.0);
    let n = f.lines.len();
    if n > 6 {
        f.lines.drain(0..n - 6);
    }
    for mut t in &mut q {
        **t = f.lines.iter().map(|l| l.0.as_str()).collect::<Vec<_>>().join("\n");
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
    if let Some(ItemKind::Bow { spread_deg, .. }) = inv.current().map(|s| &db.get(&s.id).kind) {
        let spread = spread_deg.to_radians() * if intent.held(btn::AIM) { 0.6 } else { 1.0 };
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

/// Four diagonal ticks round the reticle when the local player's hit lands (gold for a headshot).
fn hit_marker(
    mut hits: MessageReader<HitMsg>,
    local: Query<Entity, With<LocalPlayer>>,
    mut f: ResMut<Feed>,
    time: Res<Time>,
    mut q: Query<(&mut Visibility, &mut BackgroundColor), With<HitTick>>,
) {
    let Ok(me) = local.single() else { return };
    for h in hits.read() {
        if h.attacker == me && h.target != me {
            f.hit_flash = 0.18;
            f.headshot = h.headshot;
        }
    }
    f.hit_flash = (f.hit_flash - time.delta_secs()).max(0.0);
    for (mut v, mut c) in &mut q {
        *v = if f.hit_flash > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        c.0 = if f.headshot { Color::srgb(1.0, 0.85, 0.2) } else { Color::WHITE };
    }
}

fn top_right(
    m: Res<Match>,
    zone: Res<Zone>,
    local: Query<(&Transform, &Warrior), With<LocalPlayer>>,
    layout: Res<MapLayout>,
    mut q: Query<&mut Text, With<TopRight>>,
) {
    let Ok((tf, w)) = local.single() else { return };
    let (t, shrinking) = zone.countdown();
    let zone_line = if m.state != MatchState::InProgress {
        "Aandhi aa rahi hai...".to_string()
    } else if shrinking {
        format!("Aandhi chal rahi  {:>2}:{:02}", t as u32 / 60, t as u32 % 60)
    } else {
        format!("Aandhi in  {:>2}:{:02}", t as u32 / 60, t as u32 % 60)
    };
    let me = tf.translation.xz();
    let to_safe = (me.distance(zone.target_center) - zone.target_radius).max(0.0);
    let here = layout.pois.iter().find(|p| p.center.xz().distance(me) < p.radius).map(|p| p.name.as_str()).unwrap_or("Bharat");
    let safe = if to_safe > 1.0 { format!("Safe zone {to_safe:.0} m door") } else { "Safe zone mein ho".into() };
    for mut s in &mut q {
        **s = format!("{here}\nAlive {}   Kills {}\n{zone_line}\n{safe}", m.alive, w.kills);
    }
}

#[allow(clippy::type_complexity)]
fn minimap(
    zone: Res<Zone>,
    layout: Res<MapLayout>,
    local: Query<&Transform, With<LocalPlayer>>,
    mut parts: ParamSet<(
        Query<&mut Node, With<MapZone>>,
        Query<&mut Node, With<MapSafe>>,
        Query<&mut Node, With<MapMe>>,
    )>,
) {
    let scale = MINIMAP / (layout.half_extent * 2.0);
    let to_map = |p: Vec2| (p + Vec2::splat(layout.half_extent)) * scale;
    let circle = |n: &mut Node, c: Vec2, r: f32| {
        let c = to_map(c);
        let r = r * scale;
        n.left = px(c.x - r);
        n.top = px(c.y - r);
        n.width = px(r * 2.0);
        n.height = px(r * 2.0);
    };
    for mut n in &mut parts.p0() { circle(&mut n, zone.center, zone.radius); }
    for mut n in &mut parts.p1() { circle(&mut n, zone.target_center, zone.target_radius); }
    if let Ok(t) = local.single() {
        let p = to_map(t.translation.xz());
        for mut n in &mut parts.p2() {
            n.left = px(p.x - 4.0);
            n.top = px(p.y - 4.0);
        }
    }
}

fn overlay(
    m: Res<Match>,
    peers: Res<shared::sim::net::Peers>,
    local: Query<(Entity, Option<&Eliminated>, &Warrior), With<LocalPlayer>>,
    names: Query<&Warrior>,
    mut o: Query<&mut Visibility, With<Overlay>>,
    mut t: Query<&mut Text, With<OverlayText>>,
) {
    let Ok((me, elim, w)) = local.single() else { return };
    let msg = if m.state == MatchState::Ended && m.winner == Some(me) {
        Some(format!("PANGA KING!\n#1 of {}   Kills {}\n\nR dabao, phir se panga lo", m.total, w.kills))
    } else if let Some(e) = elim {
        let by = e.by.and_then(|b| names.get(b).ok()).map(|k| format!("{} ne tapka diya", k.name)).unwrap_or_else(|| "Tapak gaye".into());
        // Online, the next match starts for everyone at once, when this one is over.
        let next = if peers.all.len() > 1 && m.state != MatchState::Ended { "Panga tumhare bina chalu hai" } else { "R dabao, phir se panga lo" };
        Some(format!("{by}\n#{} of {}   Kills {}\n\n{next}", e.placement, m.total, w.kills))
    } else {
        None
    };
    for mut v in &mut o { *v = if msg.is_some() { Visibility::Inherited } else { Visibility::Hidden }; }
    if let Some(s) = msg {
        for mut tt in &mut t { **tt = s.clone(); }
    }
}

fn perf(show: Res<ShowPerf>, diag: Res<DiagnosticsStore>, quality: Res<crate::quality::Quality>, mut q: Query<(&mut Text, &mut Visibility), With<PerfText>>) {
    for (mut t, mut v) in &mut q {
        *v = if show.0 { Visibility::Inherited } else { Visibility::Hidden };
        if !show.0 {
            continue;
        }
        let fps = diag.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
        let ms = diag.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed()).unwrap_or(0.0);
        let ents = diag.get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT).and_then(|d| d.value()).unwrap_or(0.0);
        **t = format!("{fps:.0} fps   {ms:.2} ms   {ents:.0} entities   {quality:?} quality (F4)", quality = *quality);
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
            text("", size, color),
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
        **t = format!("{}", n.amount.round() as i32);
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
