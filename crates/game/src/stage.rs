//! A staged scene from the real game, for the hiding and senses previews, and the senses overlay.
//!
//! VIVAD_SENSES=1 draws what each bot knows over the world: its cone, the lines its eye tries to each
//! warrior's head and chest (green clear, red blocked), where it remembers somebody (an orange ball),
//! where it is going to look (an orange ring, yellow once it is there), and the player, through walls
//! (a cyan pole).
//!
//! VIVAD_STAGE=<dir> with VIVAD_BOTS=1: stands the player and the bot where a street kit piece gives
//! the scene (no coordinates of any map), shoots the crouch pose, the player behind a chai stall from
//! the bot's eye and from the side, and the half shutter, then films a chase: the bot sees the
//! player, the player runs round a corner and crouches, the bot goes to where it last saw him, looks
//! round and gives up. Frames of the 3D view, so the window can be hidden. Prints what the bot knows
//! at each shot (`[stage]`): the pictures show what these lines say. Run by `just stage`.
//! VIVAD_STAGE_ONLY=<text> keeps only the shots whose names contain it, and no chase; `key` presses
//! the real crouch key through the game's input.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use avian3d::prelude::*;
use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigStore};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use shared::items::{ItemStack, Rarity};
use shared::map::MapLayout;
use shared::map::kit::KitPiece;
use shared::sim::bots::BotBrain;
use shared::sim::nav::NavGrid;
use shared::sim::senses::{self, in_sight};
use shared::sim::stance::{CROUCH_DROP, Stance};
use shared::sim::{Eliminated, HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, Warrior, btn, sight_mask};

use crate::camera::MainCamera;

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("VIVAD_SENSES").is_ok() || std::env::var("VIVAD_STAGE").is_ok() {
            app.add_systems(Startup, gizmo_look).add_systems(Update, draw_senses);
        }
        if let Ok(dir) = std::env::var("VIVAD_STAGE") {
            std::fs::create_dir_all(format!("{dir}/search_frames")).ok();
            app.insert_resource(Stage { dir, shots: vec![], chase: None, index: 0, t: 0.0, frame: 0, leg: 0, shot: false, pressed: false })
                .add_systems(PreUpdate, press_keys.after(bevy::input::InputSystems).before(crate::input::LocalInput))
                .add_systems(PreUpdate, (plan, act).chain().after(crate::input::LocalInput))
                .add_systems(PostUpdate, film.after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
        }
    }
}

fn gizmo_look(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.line.width = 5.0;
    config.depth_bias = -1.0;
}

const YELLOW: Color = Color::srgb(1.0, 0.85, 0.1);
const GREEN: Color = Color::srgb(0.1, 1.0, 0.25);
const RED: Color = Color::srgb(1.0, 0.15, 0.1);
const ORANGE: Color = Color::srgb(1.0, 0.55, 0.05);
const CYAN: Color = Color::srgb(0.1, 0.95, 1.0);

#[allow(clippy::type_complexity)]
fn draw_senses(
    mut gizmos: Gizmos,
    bots: Query<(Entity, &Transform, &PlayerIntent, &BotBrain), Without<Eliminated>>,
    warriors: Query<(Entity, &Transform, &Stance), (With<Warrior>, Without<Eliminated>)>,
    player: Query<&Transform, (With<LocalPlayer>, Without<Eliminated>)>,
    spatial: SpatialQuery,
) {
    let filter = SpatialQueryFilter::from_mask(sight_mask());
    for tf in &player {
        let foot = tf.translation - Vec3::Y * HALF_HEIGHT;
        gizmos.line(foot, foot + Vec3::Y * 3.0, CYAN);
        gizmos.circle(Isometry3d::new(foot + Vec3::Y * 0.05, Quat::from_rotation_x(FRAC_PI_2)), 0.7, CYAN);
    }
    for (me, tf, intent, brain) in &bots {
        let eye = senses::eye(tf.translation, intent.drop());
        let reach = brain.sight.0.min(24.0);
        let half = (senses::FOV_DEG * 0.5).to_radians();
        let ray = |a: f32| eye + Quat::from_rotation_y(intent.yaw + a) * Vec3::NEG_Z * reach;
        gizmos.line(eye, ray(-half), YELLOW);
        gizmos.line(eye, ray(half), YELLOW);
        gizmos.linestrip((0..=24).map(|k| ray(-half + 2.0 * half * k as f32 / 24.0)), YELLOW);
        // The lines the bot's eye tries: to each warrior's head and chest, clear or blocked.
        for (who, wt, stance) in &warriors {
            if who == me || wt.translation.distance(eye) > brain.sight.0 {
                continue;
            }
            for p in senses::body_points(wt.translation, stance.drop()) {
                gizmos.line(eye, p, if senses::clear_line(&spatial, &filter, eye, p) { GREEN } else { RED });
            }
        }
        // Where it remembers somebody.
        for k in brain.senses.known.iter().filter(|k| !k.seen) {
            gizmos.sphere(Isometry3d::from_translation(k.pos), 0.35, ORANGE);
        }
        if let Some(s) = brain.senses.search {
            let colour = if s.looking.is_some() { YELLOW } else { ORANGE };
            gizmos.circle(Isometry3d::new(s.spot + Vec3::Y * 0.1, Quat::from_rotation_x(FRAC_PI_2)), 1.2, colour);
            gizmos.line(eye, s.spot + Vec3::Y * 0.1, colour);
        }
    }
}

/// One still: where everyone is and where the camera stands, held `secs` before the shot.
#[derive(Clone)]
struct Shot {
    name: &'static str,
    player: Vec3,
    yaw: f32,
    crouch: bool,
    /// Walking forward through the shot.
    walk: bool,
    bot: Vec3,
    bot_yaw: f32,
    view: (Vec3, Vec3),
    fov: f32,
    secs: f32,
    /// The player's buttons are left to the keyboard, and the crouch key is pressed once.
    key: bool,
}

/// The chase: the bot's first place, the player's, and the way he runs.
#[derive(Clone)]
struct Chase {
    bot: Vec3,
    start: Vec3,
    route: Vec<Vec3>,
}

#[derive(Resource)]
struct Stage {
    dir: String,
    shots: Vec<Shot>,
    chase: Option<Chase>,
    index: usize,
    /// Seconds into the current shot; below zero, waiting for the world.
    t: f32,
    frame: u32,
    leg: usize,
    shot: bool,
    pressed: bool,
}

const CLIP_FPS: f32 = 8.0;
const CLIP_SECS: f32 = 22.0;
/// When in the clip the player breaks for it.
const BREAK_AT: f32 = 2.5;

fn feet(layout: &MapLayout, p: Vec2) -> Vec3 {
    Vec3::new(p.x, layout.terrain.height(p.x, p.y), p.y)
}

fn body(feet: Vec3) -> Vec3 {
    feet + Vec3::Y * HALF_HEIGHT
}

fn eye_of(feet: Vec3) -> Vec3 {
    senses::eye(body(feet), 0.0)
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

/// A chai stall with a clear view of it from 16 m on the side its counter faces.
fn stall_scene(layout: &MapLayout, nav: &NavGrid, spatial: &SpatialQuery) -> Option<(KitPiece, Vec3, Vec3, Vec3)> {
    layout.kit.iter().filter(|k| k.name == "chai" && k.pos.y < 1.0).find_map(|k| {
        let along = (Quat::from_rotation_y(k.yaw) * Vec3::Z).xz();
        [along, -along].into_iter().find_map(|d| {
            let c = k.pos.xz();
            let (bot, hide, open) = (feet(layout, c + d * 16.0), feet(layout, c - d * 1.0), feet(layout, c - d * 0.3 + d.perp() * 4.5));
            let good = nav.walkable(bot.xz())
                && nav.walkable(hide.xz())
                && nav.walkable(open.xz())
                && in_sight(spatial, eye_of(bot), body(hide), 0.0)
                && in_sight(spatial, eye_of(bot), body(open), 0.0)
                && !in_sight(spatial, eye_of(bot), body(hide), CROUCH_DROP);
            good.then(|| (k.clone(), bot, hide, open))
        })
    })
}

/// A place the bot can see, and a way round a corner from it to a place it couldn't see from
/// where it lost the runner either.
fn chase_scene(layout: &MapLayout, nav: &NavGrid, spatial: &SpatialQuery) -> Option<Chase> {
    for k in layout.kit.iter().filter(|k| k.pos.y < 1.0).step_by(5) {
        for b in 0..4 {
            let start = feet(layout, k.pos.xz() + Vec2::from_angle(b as f32 * TAU / 4.0) * 4.0);
            let at = |d: Vec2| feet(layout, start.xz() + d * 15.0);
            for bearing in 0..8 {
                let bot = at(Vec2::from_angle(bearing as f32 * TAU / 8.0));
                if !nav.walkable(start.xz()) || !nav.walkable(bot.xz()) || !in_sight(spatial, eye_of(bot), body(start), 0.0) {
                    continue;
                }
                for r in [12.0, 16.0, 20.0, 26.0] {
                    for h in 0..12 {
                        let hide = feet(layout, start.xz() + Vec2::from_angle(h as f32 * TAU / 12.0) * r);
                        if !nav.walkable(hide.xz()) || in_sight(spatial, eye_of(bot), body(hide), CROUCH_DROP) {
                            continue;
                        }
                        let Ok(path) = nav.path(body(start), body(hide)) else { continue };
                        let route: Vec<Vec3> = path.iter().map(|p| feet(layout, *p)).collect();
                        let len: f32 = route.windows(2).map(|w| w[0].distance(w[1])).sum();
                        if route.len() < 3 || len > 34.0 || route.last().is_none_or(|e| e.xz().distance(hide.xz()) > 1.0) {
                            continue;
                        }
                        // Where he drops out of the bot's sight along the way.
                        let mut lost = None;
                        for w in route.windows(2) {
                            for s in 0..8 {
                                let p = w[0].lerp(w[1], s as f32 / 8.0);
                                if lost.is_none() && !in_sight(spatial, eye_of(bot), body(p), 0.0) {
                                    lost = Some(p);
                                }
                            }
                        }
                        let Some(lost) = lost else { continue };
                        // Nowhere a tree would hide the action from above.
                        let bare = |p: Vec3| layout.props.iter().all(|q| q.pos.xz().distance(p.xz()) > 11.0);
                        if !in_sight(spatial, eye_of(lost), body(hide), CROUCH_DROP) && lost.distance(hide) > 6.0 && bare(lost) && bare(hide) {
                            return Some(Chase { bot, start, route });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Once the map and the grid are up: find the places and write the shot list.
#[allow(clippy::type_complexity)]
fn plan(
    mut stage: ResMut<Stage>,
    layout: Res<MapLayout>,
    nav: Res<NavGrid>,
    spatial: SpatialQuery,
    time: Res<Time<Real>>,
    mut bots: Query<&mut BotBrain, Without<LocalPlayer>>,
) {
    if !stage.shots.is_empty() || !nav.ready() || time.elapsed_secs() < 10.0 {
        return;
    }
    let Some((stall, bot, hide, open)) = stall_scene(&layout, &nav, &spatial) else {
        error!("stage: no chai stall to use");
        std::process::exit(1);
    };
    let c = stall.pos;
    let (to_bot, side) = ((bot - c).normalize(), (bot - c).normalize().cross(Vec3::Y));
    let chest = Vec3::Y * 0.7;
    let bot_eye = eye_of(bot);
    let yaw = yaw_to(open, open + side);
    let round = |at: Vec3, a: f32, r: f32, h: f32| (at + Quat::from_rotation_y(a) * Vec3::new(r, h, 0.0), at + chest);
    let still = |name, player: Vec3, yaw, crouch, view| Shot { name, player, yaw, crouch, walk: false, bot, bot_yaw: yaw_to(bot, hide), view, fov: 70.0, secs: 1.6, key: false };
    // The bot's own view, from just ahead of its head.
    let sight = bot_eye + (hide + chest - bot_eye).normalize() * 0.7;
    let mut shots = vec![
        // The real key, through the game's own input: once down, once up.
        Shot { key: true, ..still("key_1_c_goes_down", open, yaw, false, round(open, -yaw, 3.0, 1.0)) },
        Shot { key: true, ..still("key_2_c_comes_up", open, yaw, false, round(open, -yaw, 3.0, 1.0)) },
        // The pose, on the open street.
        still("pose_1_stand_side", open, yaw, false, round(open, -yaw, 3.0, 1.0)),
        still("pose_2_crouch_side", open, yaw, true, round(open, -yaw, 3.0, 1.0)),
        still("pose_3_crouch_front", open, yaw, true, round(open, -yaw - FRAC_PI_2, 2.6, 1.0)),
        still("pose_4_crouch_back", open, yaw, true, round(open, -yaw + FRAC_PI_2, 2.6, 1.3)),
        Shot { walk: true, ..still("pose_5_crouch_walk", open - side * 2.0, yaw, true, round(open, -yaw, 3.6, 1.0)) },
        // Behind the stall, from the bot's eye, then from the side.
        Shot { fov: 26.0, ..still("stall_1_standing_bot_eye", hide, yaw_to(hide, bot), false, (sight, hide + chest)) },
        Shot { fov: 26.0, ..still("stall_2_crouched_bot_eye", hide, yaw_to(hide, bot), true, (sight, hide + chest)) },
        still("stall_3_crouched_side", hide, yaw_to(hide, bot), true, (c + side * 5.0 + to_bot * 0.6 + Vec3::Y * 1.9, c - to_bot * 0.6 + Vec3::Y * 0.6)),
        still("stall_4_crouched_above", hide, yaw_to(hide, bot), true, (c - to_bot * 7.0 + side * 3.0 + Vec3::Y * 5.0, c + to_bot * 2.0)),
    ];
    // The half shutter: stopped upright, under it on his knees, standing inside.
    if let Some(g) = layout.kit.iter().find(|k| k.name == "gumti" && k.pos.y < 1.0) {
        let out = Quat::from_rotation_y(g.yaw) * Vec3::NEG_Z;
        let flat = out.cross(Vec3::Y);
        let at = g.pos;
        let view = (at + out * 5.2 + flat * 2.4 + Vec3::Y * 1.5, at + Vec3::Y * 1.0);
        let inward = yaw_to(at + out, at);
        shots.push(still("shutter_1_upright_stopped", at + out * 1.85, inward, false, view));
        shots.push(still("shutter_2_crouched_under", at + out * 1.2, inward, true, view));
        shots.push(still("shutter_3_inside_standing", at, inward + PI, false, (at + out * 3.6 + Vec3::Y * 0.62, at + Vec3::Y * 0.9)));
    }
    // Birds on a pole: a crouch passes under them, a walk puts them up.
    // The pole with flocks farthest from any tree.
    let clearance = |p: &KitPiece| layout.props.iter().map(|q| q.pos.xz().distance(p.pos.xz())).fold(f32::MAX, f32::min);
    let mut poles: Vec<&KitPiece> = layout.kit.iter().filter(|k| k.name == "pole").step_by(3).collect();
    poles.sort_by(|a, b| clearance(b).total_cmp(&clearance(a)));
    let open_pole = poles.into_iter().find_map(|p| {
        let top = p.pos + Vec3::Y * 9.0;
        (0..8).find_map(|b| {
            let d = Vec2::from_angle(b as f32 * TAU / 8.0);
            let eye = feet(&layout, p.pos.xz() + d * 13.0) + Vec3::Y * 1.6;
            let walk = feet(&layout, p.pos.xz() + d * 3.0 + d.perp() * 3.0);
            // Stopping short of the pole, or it would be in its own way.
            let clear = spatial.cast_ray(eye, Dir3::new(top - eye).ok()?, eye.distance(top) - 1.0, true, &SpatialQueryFilter::from_mask(sight_mask())).is_none();
            (clear && nav.walkable(walk.xz())).then_some((walk, top, eye, d))
        })
    });
    if let Some((walk, top, eye, d)) = open_pole {
        let view = (eye, top - Vec3::Y * 1.2);
        let along = yaw_to(walk, walk - d.perp().extend(0.0).xzy());
        shots.push(Shot { walk: true, secs: 2.0, fov: 42.0, ..still("birds_1_crouch_passes", walk, along, true, view) });
        shots.push(Shot { walk: true, secs: 1.1, fov: 42.0, ..still("birds_2_walk_flushes", walk, along, false, view) });
    }
    let only = std::env::var("VIVAD_STAGE_ONLY").ok();
    if let Some(only) = &only {
        shots.retain(|s| s.name.contains(only.as_str()));
    } else {
        let Some(chase) = chase_scene(&layout, &nav, &spatial) else {
            error!("stage: no corner to run round");
            std::process::exit(1);
        };
        println!("[stage] stall at {c}, bot at {bot}, chase {:?} -> {:?}", chase.start, chase.route.last());
        stage.chase = Some(chase);
    }
    stage.shots = shots;
    stage.t = 0.0;
    // Past looting, and quick: this bot acts on what it senses.
    for mut brain in &mut bots {
        brain.landed = Some(-1000.0);
        brain.calm = false;
    }
}

/// Taps the crouch key, as the keyboard would, in a shot that asks for it.
fn press_keys(mut stage: ResMut<Stage>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    if keys.pressed(KeyCode::KeyC) {
        keys.release(KeyCode::KeyC);
    } else if !stage.pressed && stage.t > 0.4 && stage.shots.get(stage.index).is_some_and(|s| s.key) {
        keys.press(KeyCode::KeyC);
        stage.pressed = true;
    }
}

/// Puts everyone where the shot wants them. Runs in place of the keyboard, just after it.
#[allow(clippy::type_complexity)]
fn act(
    mut stage: ResMut<Stage>,
    time: Res<Time<Real>>,
    mut player: Query<(&mut Transform, &mut Kinematic, &mut PlayerIntent, &mut Health), (With<LocalPlayer>, Without<BotBrain>)>,
    mut bots: Query<(&mut Transform, &mut Kinematic, &mut PlayerIntent, &mut Health, &mut BotBrain, &mut Inventory), (With<Warrior>, Without<LocalPlayer>)>,
    mut exit: MessageWriter<AppExit>,
) {
    if stage.shots.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let (Ok((mut ptf, mut pkin, mut pint, mut php)), Ok((mut btf, mut bkin, mut bint, mut bhp, mut brain, mut inv))) = (player.single_mut(), bots.single_mut()) else { return };
    php.hp = 100.0;
    bhp.hp = 100.0;
    stage.t += dt;
    pint.move_axis = Vec2::ZERO;
    pkin.mode = MoveMode::Ground;
    bkin.mode = MoveMode::Ground;
    // No fall from the flight's height for being put on the street.
    pkin.fall_from = None;
    bkin.fall_from = None;
    if let Some(shot) = stage.shots.get(stage.index).cloned() {
        pint.yaw = shot.yaw;
        if !shot.key {
            pint.held = if shot.crouch { btn::CROUCH } else { 0 };
        }
        if shot.walk {
            pint.move_axis = Vec2::Y;
            if stage.t < 0.3 {
                ptf.translation = body(shot.player) + Vec3::Y * 0.03;
            }
        } else {
            ptf.translation = body(shot.player) + Vec3::Y * 0.03;
            pkin.velocity = Vec3::ZERO;
        }
        btf.translation = body(shot.bot) + Vec3::Y * 0.03;
        bkin.velocity = Vec3::ZERO;
        bint.yaw = shot.bot_yaw;
        // Resting, so its brain leaves its head where the shot put it.
        brain.calm = true;
        brain.rest = 5.0;
        return;
    }
    let Some(chase) = stage.chase.clone() else {
        exit.write(AppExit::Success);
        return;
    };
    let t = stage.t;
    if inv.slots[0].is_none() {
        // A clean slate, and a gun.
        brain.senses = default();
        brain.target = None;
        brain.goal = None;
        brain.calm = false;
        brain.rest = 0.0;
        inv.slots[0] = Some(ItemStack { id: "tamancha".into(), rarity: Rarity::Common, count: 6 });
        inv.arrows = 200;
    }
    if t < BREAK_AT {
        ptf.translation = body(chase.start) + Vec3::Y * 0.03;
        btf.translation = body(chase.bot) + Vec3::Y * 0.03;
        pint.yaw = yaw_to(chase.start, chase.bot);
        bint.yaw = yaw_to(chase.bot, chase.start);
        pint.held = 0;
    } else if let Some(next) = chase.route.get(stage.leg) {
        pint.yaw = yaw_to(ptf.translation, *next);
        pint.move_axis = Vec2::Y;
        pint.held = btn::SPRINT;
        if ptf.translation.xz().distance(next.xz()) < 0.7 {
            stage.leg += 1;
        }
    } else {
        pint.held = btn::CROUCH;
    }
    if t > CLIP_SECS {
        exit.write(AppExit::Success);
    }
}

#[allow(clippy::type_complexity)]
fn film(
    mut commands: Commands,
    mut stage: ResMut<Stage>,
    view: Res<crate::quality::View3d>,
    mut cam: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
    player: Query<(Entity, &Transform, &Stance), (With<LocalPlayer>, Without<MainCamera>)>,
    bots: Query<(&Transform, &BotBrain), (Without<LocalPlayer>, Without<MainCamera>)>,
) {
    if stage.shots.is_empty() {
        return;
    }
    let (Ok((mut ct, mut proj)), Ok((me, ptf, stance)), Ok((btf, brain))) = (cam.single_mut(), player.single(), bots.single()) else { return };
    let knows = |at: f32| {
        let k = brain.senses.known.iter().find(|k| k.who == me);
        format!(
            "t={at:.1} crouched={} sees={} knows={} target={} search={:?} looking={} dist={:.1}",
            stance.crouched,
            brain.senses.sees(me),
            k.is_some(),
            brain.target == Some(me),
            brain.senses.search.map(|s| s.spot.round()),
            brain.senses.search.is_some_and(|s| s.looking.is_some()),
            btf.translation.distance(ptf.translation)
        )
    };
    if let Some(shot) = stage.shots.get(stage.index).cloned() {
        *ct = Transform::from_translation(shot.view.0).looking_at(shot.view.1, Vec3::Y);
        if let Projection::Perspective(p) = proj.as_mut() {
            p.fov = shot.fov.to_radians();
        }
        if stage.t >= shot.secs && !stage.shot {
            stage.shot = true;
            println!("[stage] {}: {}", shot.name, knows(stage.t));
            let path = format!("{}/{}.png", stage.dir, shot.name);
            commands.spawn(Screenshot::image(view.image.clone())).observe(save_to_disk(path));
        }
        if stage.t >= shot.secs + 0.2 {
            stage.index += 1;
            stage.t = 0.0;
            stage.shot = false;
            stage.pressed = false;
        }
        return;
    }
    // High above them both, on a long lens: over the roofs and the trees, down into the street.
    let (a, b) = (btf.translation, ptf.translation);
    let mid = (a + b) * 0.5;
    let spread = a.distance(b);
    let eye = mid + Vec3::new(0.0, 26.0 + spread * 0.9, 6.0 + spread * 0.25);
    *ct = Transform::from_translation(eye).looking_at(mid, Vec3::Y);
    if let Projection::Perspective(p) = proj.as_mut() {
        p.fov = 30f32.to_radians();
    }
    if stage.t >= stage.frame as f32 / CLIP_FPS {
        if stage.frame % 4 == 0 {
            println!("[stage] chase {}", knows(stage.t));
        }
        let path = format!("{}/search_frames/frame_{:05}.png", stage.dir, stage.frame);
        commands.spawn(Screenshot::image(view.image.clone())).observe(crate::autoshot::save_small(path));
        stage.frame += 1;
    }
}
