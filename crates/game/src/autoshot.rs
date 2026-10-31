//! Automated capture for play-testing without a human at the keyboard.
//!
//! VIVAD_TOUR=<dir>      fly the camera to each landmark, screenshot, exit.
//!   VIVAD_TOUR_ONLY=a,b only the views whose names contain one of these.
//!   VIVAD_TOUR_FRAMES=n  n consecutive frames per view (`<name>_f<k>.png`), for flicker checks.
//!   VIVAD_TOUR_DRIFT=m   during a burst, slide the camera sideways m metres per frame.
//!   VIVAD_TOUR_SETTLE=n  frames to hold each view before its shot; above 120 it prints each view's
//!                        frame rate (`[tour-fps]`; run with VIVAD_NO_VSYNC=1).
//! VIVAD_SHOTS=<dir>     with VIVAD_AUTOPILOT=1: screenshot the match at set times, then exit.
//!   VIVAD_SHOTS_AT=a,b   at these match times (seconds) instead.
//! VIVAD_ARMOURY=<dir>   stand the player on open ground and screenshot every item in hand, carried
//!   and in use (aiming, drawing, swinging, drinking), from the front and the side, then exit.
//!   VIVAD_ARMOURY_ONLY=a,b only the poses whose names contain one of these. The range poses
//!   freeze the one bot (VIVAD_BOTS=1) in front as a target and shoot or cut at it.

use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use shared::items::{ItemDb, ItemKind, ItemStack, Rarity};
use shared::map::MapLayout;
use shared::sim::flow::Match;
use shared::sim::{Action, HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, Warrior, btn};

use crate::camera::MainCamera;

#[derive(Resource)]
pub struct Tour {
    dir: String,
    shots: Vec<(&'static str, Vec3, Vec3)>,
    index: usize,
    frames: u32,
    /// When this view's timing window opened (frame 30), for VIVAD_TOUR_SETTLE runs.
    timed_from: f64,
}

#[derive(Resource)]
pub struct MatchShots {
    dir: String,
    times: Vec<f32>,
    next: usize,
    taken_at: f32,
}

/// One armoury shot: item in hand, what the warrior is doing, and where the camera stands.
#[derive(Clone, Copy)]
struct Pose {
    item: &'static str,
    state: Use,
    view: View,
    pitch: f32,
    /// A target this far ahead (the frozen bot), aimed at; the shot is taken `after` seconds
    /// after the strike.
    target: Option<f32>,
    after: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Use {
    Carry,
    Aim,
    /// Hold fire: draw a charged bow, or drink.
    Hold,
    /// One press of fire, shot mid-swing.
    Strike,
}

#[derive(Clone, Copy)]
enum View {
    Front,
    Side,
    Back,
    /// Wide, from the side, taking in the player and the target.
    Range,
    /// The game's own camera (HUD numbers, kick and shake show as a player sees them).
    Player,
}

#[derive(Resource)]
pub struct Armoury {
    dir: String,
    poses: Vec<Pose>,
    index: usize,
    /// Seconds into the current pose.
    t: f32,
    shot: bool,
    /// Fire has been pressed for this pose (drinking and swinging start on a press).
    pressed: bool,
    spot: Option<Vec3>,
}

impl Armoury {
    fn name(&self, p: &Pose) -> String {
        let state = match p.state {
            Use::Carry => "carry",
            Use::Aim => "aim",
            Use::Hold => "hold",
            Use::Strike => "strike",
        };
        let view = match p.view {
            View::Front => "front",
            View::Side => "side",
            View::Back => "back",
            View::Range => "range",
            View::Player => "player",
        };
        let up = if p.pitch > 0.1 { "_up" } else if p.pitch < -0.1 { "_down" } else { "" };
        let at = if p.target.is_some() { format!("_{:.0}ms", p.after * 1000.0) } else { String::new() };
        format!("{:02}_{}_{state}{up}_{view}{at}", self.index, p.item)
    }
}

/// Settle time before each armoury shot; a strike is pressed STRIKE_AT seconds in.
const POSE_TIME: f32 = 1.3;
const STRIKE_AT: f32 = 1.1;

pub struct AutoshotPlugin;

impl Plugin for AutoshotPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(dir) = std::env::var("VIVAD_TOUR") {
            let v = |x, y, z| Vec3::new(x, y, z);
            app.insert_resource(Tour {
                timed_from: 0.0,
                dir,
                shots: vec![
                    ("01_overview", v(-60.0, 260.0, 300.0), v(0.0, 0.0, 0.0)),
                    ("02_bazaar_street", v(-45.0, 4.5, 2.0), v(20.0, 3.0, 0.0)),
                    ("cp_aerial", v(40.0, 20.0, 175.0), v(0.0, 3.0, 118.0)),
                    ("cp_park", v(8.0, 1.7, 103.0), v(-14.0, 5.0, 138.0)),
                    ("cp_street", v(30.0, 2.2, 164.0), v(-2.0, 4.0, 150.0)),
                    ("cp_verandah", v(39.5, 1.7, 113.0), v(36.5, 1.5, 135.0)),
                    ("cp_upper", v(-32.0, 6.7, 118.0), v(-26.0, 6.0, 100.0)),
                    ("03_bazaar_roofs", v(-30.0, 26.0, 40.0), v(5.0, 2.0, 0.0)),
                    ("04_stepwell", v(-66.0, 22.0, 112.0), v(-112.0, -12.0, 95.0)),
                    ("04b_stepwell_above", v(-96.0, 45.0, 138.0), v(-108.0, -6.0, 95.0)),
                    ("04c_stepwell_bridge", v(-85.1, 1.7, 80.0), v(-85.1, 1.2, 110.0)),
                    ("05_stepwell_inside", v(-83.0, 1.2, 95.0), v(-112.0, -12.0, 95.0)),
                    ("05b_stepwell_bottom", v(-131.5, -19.3, 99.0), v(-95.0, -4.0, 94.0)),
                    ("05c_stepwell_mid", v(-104.0, -10.3, 98.5), v(-125.0, -16.0, 92.0)),
                    ("06_hampi", v(-95.0, 22.0, -95.0), v(-140.0, 2.0, -130.0)),
                    ("07_fort", v(70.0, 40.0, -60.0), v(125.0, 15.0, -135.0)),
                    ("07b_fort_gate", v(108.0, 21.0, -96.0), v(118.0, 22.0, -112.0)),
                    ("07c_fort_court", v(125.0, 20.2, -117.5), v(125.0, 21.0, -150.0)),
                    ("07d_fort_rampart", v(98.5, 27.7, -128.0), v(125.0, 22.0, -150.0)),
                    ("07e_fort_diwan", v(119.0, 20.6, -142.0), v(127.0, 21.5, -151.0)),
                    ("07f_fort_mahal", v(117.0, 20.2, -127.0), v(139.0, 22.0, -139.0)),
                    ("07g_fort_terrace", v(110.5, 24.7, -135.0), v(140.0, 25.0, -139.0)),
                    ("08_ghats", v(200.0, 10.0, 60.0), v(160.0, 0.0, 30.0)),
                    ("08b_ghats_river", v(193.0, 1.6, 12.0), v(160.0, 4.0, 45.0)),
                    ("08c_ghats_landing", v(161.0, 4.5, 9.5), v(166.0, 2.6, 30.0)),
                    ("08d_ghats_deck", v(153.0, 6.9, 22.0), v(176.0, 0.0, 50.0)),
                    ("09_caves_stupa", v(-120.0, 14.0, 40.0), v(-185.0, 6.0, 0.0)),
                    ("10_banyan", v(60.0, 9.0, 140.0), v(95.0, 6.0, 170.0)),
                    ("11_village", v(-10.0, 12.0, 160.0), v(-40.0, 1.0, 195.0)),
                    ("12_house_interior", v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
                    ("13_kashi_temple", v(150.0, 9.0, 40.0), v(125.0, 8.0, 15.0)),
                    ("14_chowk_temple", v(-9.0, 4.0, 22.0), v(0.0, 7.0, -2.0)),
                    ("15_hampi_gopuram", v(-120.0, 6.0, -120.0), v(-131.0, 10.0, -161.0)),
                    ("16_hampi_temple", v(-153.7, 2.0, -151.3), v(-138.8, 4.0, -125.2)),
                    ("16b_hampi_bazaar", v(-154.8, 1.7, -188.1), v(-147.4, 6.0, -159.1)),
                    ("16c_hampi_tank", v(-165.0, 3.0, -110.0), v(-179.0, -3.0, -122.0)),
                    ("16d_hampi_cloister", v(-165.8, 7.0, -150.2), v(-127.8, 1.0, -122.8)),
                    ("17_stupa", v(-128.0, 7.0, 32.0), v(-158.0, 6.0, 8.0)),
                    ("18_cave_facade", v(-168.0, 7.0, 10.0), v(-192.0, 8.0, 16.0)),
                    ("19_vihara_inside", v(-193.0, 2.0, -16.0), v(-212.0, 1.8, -16.0)),
                    ("20_chaitya_inside", v(-193.0, 2.0, 16.0), v(-210.0, 2.6, 16.0)),
                    ("21_ajanta_wide", v(-120.0, 30.0, 90.0), v(-200.0, 10.0, 20.0)),
                    ("22_hampi_mandapa", v(-118.3, 4.0, -150.0), v(-103.7, 2.0, -141.3)),
                    ("23_village_close", v(-28.0, 3.5, 172.0), v(-40.0, 1.5, 195.0)),
                    ("24_stalls", v(-30.0, 2.2, -1.0), v(-48.0, 1.5, -4.0)),
                    ("25_haveli_front", v(-41.0, 3.0, 0.5), v(-50.0, 4.5, 8.0)),
                    ("26_haveli_court", v(-47.6, 5.3, 12.6), v(-52.5, 1.2, 17.5)),
                    ("31_pickup_sword", v(0.0, 2.0, 0.0), v(1.0, 0.0, 0.0)),
                    ("31_pickup_bow", v(0.0, 2.0, 0.0), v(1.0, 0.0, 0.0)),
                    ("27_haveli_air", v(-22.0, 24.0, 34.0), v(-42.0, 2.0, 14.0)),
                    ("28_stepwell_niches", v(-100.0, -9.0, 92.0), v(-112.0, -11.0, 85.6)),
                    ("28b_stepwell_figures", v(-112.0, -9.5, 91.0), v(-118.0, -10.5, 85.6)),
                    ("28c_stepwell_wall", v(-97.0, -7.6, 89.5), v(-109.0, -9.2, 85.4)),
                    ("29_shrine_buddha", v(-205.0, 2.0, -16.0), v(-212.0, 2.2, -16.0)),
                ],
                index: 0,
                frames: 0,
            });
            if let Ok(only) = std::env::var("VIVAD_TOUR_ONLY") {
                let keys: Vec<String> = only.split(',').map(str::to_owned).collect();
                app.world_mut().resource_mut::<Tour>().shots.retain(|s| keys.iter().any(|k| s.0.contains(k.as_str())));
            }
            app.add_systems(PostUpdate, run_tour.after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
        }
        if let Ok(dir) = std::env::var("VIVAD_ARMOURY") {
            let p = |item, state, view, pitch| Pose { item, state, view, pitch, target: None, after: 0.17 };
            let range = |item, view, dist, after| Pose { item, state: Use::Strike, view, pitch: 0.0, target: Some(dist), after };
            let mut poses = vec![
                p("bat", Use::Carry, View::Front, 0.0),
                p("bat", Use::Strike, View::Front, 0.0),
                p("bat", Use::Strike, View::Side, 0.0),
                p("tamancha", Use::Carry, View::Front, 0.0),
                p("tamancha", Use::Aim, View::Front, 0.0),
                p("tamancha", Use::Aim, View::Side, 0.0),
                p("tamancha", Use::Aim, View::Back, 0.0),
                p("tamancha", Use::Aim, View::Side, 0.5),
                p("katta", Use::Carry, View::Front, 0.0),
                p("katta", Use::Aim, View::Side, 0.0),
                p("katta", Use::Aim, View::Back, 0.0),
                p("dunali", Use::Carry, View::Side, 0.0),
                p("dunali", Use::Aim, View::Side, 0.0),
                p("gulel", Use::Hold, View::Side, 0.0),
                p("gulel", Use::Hold, View::Front, -0.4),
                p("agneyastra", Use::Carry, View::Front, 0.0),
                p("vayavyastra", Use::Carry, View::Side, 0.0),
                p("sanjeevani", Use::Carry, View::Front, 0.0),
                p("soma", Use::Hold, View::Front, 0.0),
                p("amrita", Use::Carry, View::Front, 0.0),
                range("bat", View::Front, 1.7, 0.21),
                range("bat", View::Player, 1.7, 0.24),
                range("tamancha", View::Range, 12.0, 0.05),
                range("tamancha", View::Player, 12.0, 0.25),
                // The arrow in flight, seen down the shot.
                range("tamancha", View::Player, 30.0, 0.05),
                range("tamancha", View::Player, 30.0, 0.12),
                // The muzzle flash, a moment after the shot.
                range("tamancha", View::Range, 12.0, 0.02),
                range("katta", View::Range, 12.0, 0.02),
                range("dunali", View::Range, 9.0, 0.02),
                range("dunali", View::Range, 9.0, 0.08),
                range("dunali", View::Player, 9.0, 0.3),
                range("agneyastra", View::Range, 9.0, 0.25),
            ];
            if let Ok(only) = std::env::var("VIVAD_ARMOURY_ONLY").map(|s| s.trim().to_owned()).and_then(|s| if s.is_empty() { Err(std::env::VarError::NotPresent) } else { Ok(s) }) {
                let keys: Vec<String> = only.split(',').map(str::to_owned).collect();
                poses.retain(|q| keys.iter().any(|k| q.item.contains(k.as_str())));
            }
            app.insert_resource(Armoury { dir, poses, index: 0, t: -3.0, shot: false, pressed: false, spot: None })
                .add_systems(PreUpdate, armoury_actor.after(crate::input::LocalInput))
                .add_systems(PostUpdate, armoury_camera.after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
        }
        if let Ok(dir) = std::env::var("VIVAD_SHOTS") {
            let times = std::env::var("VIVAD_SHOTS_AT")
                .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
                .unwrap_or_else(|_| vec![6.0, 20.0, 34.0, 50.0, 80.0, 120.0, 170.0, 230.0]);
            app.insert_resource(MatchShots { dir, times, next: 0, taken_at: -1.0 })
                .add_systems(Update, match_shots);
        }
    }
}

fn run_tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
    houses: Query<(&shared::sim::MapBlock, &Transform), Without<MainCamera>>,
    chests: Query<(&shared::sim::loot::Chest, &Transform), Without<MainCamera>>,
    pickups: Query<(&shared::sim::loot::Pickup, &Transform), Without<MainCamera>>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(mut ct) = cam.single_mut() else { return };
    if tour.index >= tour.shots.len() {
        tour.frames += 1;
        if tour.frames > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (name, mut pos, mut look) = tour.shots[tour.index];
    if name == "12_house_interior" {
        // Stand inside the first wooden door's house, looking in.
        if let Some((_, t)) = houses.iter().find(|(b, _)| b.0.mat == shared::map::Mat::Wood && b.0.pos.x.abs() < 60.0 && b.0.pos.z.abs() < 25.0) {
            let inward = t.rotation * Vec3::NEG_Z;
            pos = t.translation + inward * 1.5 + Vec3::Y * 0.6;
            look = pos + inward * 5.0 + Vec3::Y * 0.5;
        }
    }
    if name.starts_with("2") && name.contains("haveli") {
        // The bazaar's layout follows the map seed: find a courtyard haveli by its court's
        // kalash chest (off the chowk), and frame it from the street, the gallery and the air.
        if let Some((_, t)) = chests.iter().find(|(c, t)| c.kind == shared::map::ChestKind::Kalash && t.translation.x.abs() > 16.0 && t.translation.x.abs() < 60.0 && t.translation.z.abs() < 30.0) {
            let chest = t.translation;
            let side = chest.z.signum();
            let court = chest + t.rotation * Vec3::Z * 2.5;
            (pos, look) = match name {
                "25_haveli_front" => (Vec3::new(court.x + 9.0, 3.0, side * 0.5), Vec3::new(court.x, 4.5, side * 8.0)),
                "26_haveli_court" => (court + Vec3::new(2.0, 5.3, -side * 1.5), court + Vec3::new(-2.0, 1.2, side * 2.5)),
                _ => (Vec3::new(court.x + 20.0, 24.0, side * 34.0), Vec3::new(court.x, 2.0, court.z)),
            };
        }
    }
    if name.starts_with("31_pickup") {
        // Frame a weapon lying in the open (a bow or a sword), from standing height and 3 m off.
        let want = if name.ends_with("bow") { "tamancha" } else { "bat" };
        let found = pickups.iter().find(|(p, t)| p.stack.id == want && t.translation.y < 3.0).or_else(|| pickups.iter().next());
        if let Some((_, t)) = found {
            let at = t.translation;
            (pos, look) = (at + Vec3::new(2.4, 1.3, 1.6), at + Vec3::Y * 0.3);
        }
    }
    let settle = std::env::var("VIVAD_TOUR_SETTLE").ok().and_then(|s| s.parse().ok()).unwrap_or(if tour.index == 0 { 150 } else { 45 });
    let drift: f32 = std::env::var("VIVAD_TOUR_DRIFT").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let side = (look - pos).cross(Vec3::Y).normalize_or_zero() * drift * tour.frames.saturating_sub(settle - 1) as f32;
    *ct = Transform::from_translation(pos + side).looking_at(look + side, Vec3::Y);
    tour.frames += 1;
    // With a long settle, time each view (after 30 frames for loading and LOD to settle).
    if settle > 120 && tour.frames == 30 {
        tour.timed_from = time.elapsed_secs_f64();
    }
    if settle > 120 && tour.frames == settle {
        let fps = (settle - 30) as f64 / (time.elapsed_secs_f64() - tour.timed_from);
        println!("[tour-fps] {name:<24} {fps:6.1}");
    }
    // Let shadows, the atmosphere and LOD settle for a few frames per view (the first view also
    // waits for the scene to finish its first renders); `settle` is set above.
    let burst: u32 = std::env::var("VIVAD_TOUR_FRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    if tour.frames >= settle && tour.frames < settle + burst {
        let path = if burst > 1 {
            format!("{}/{}_f{}.png", tour.dir, name, tour.frames - settle)
        } else {
            format!("{}/{}.png", tour.dir, name)
        };
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if tour.frames >= settle + burst + 10 {
        tour.frames = 0;
        tour.index += 1;
    }
}

fn match_shots(
    mut commands: Commands,
    mut shots: ResMut<MatchShots>,
    m: Res<Match>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    if shots.next >= shots.times.len() {
        if now - shots.taken_at > 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    }
    if m.elapsed >= shots.times[shots.next] {
        let path = format!("{}/match_{:02}_{:03.0}s.png", shots.dir, shots.next, m.elapsed);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        shots.next += 1;
        shots.taken_at = now;
    }
}

/// Holds the local player still on open ground with the pose's item, pitch and buttons. Runs in
/// place of the keyboard and mouse (just after them) so the sim and the animation both see it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn armoury_actor(
    mut commands: Commands,
    mut arm: ResMut<Armoury>,
    db: Res<ItemDb>,
    layout: Res<MapLayout>,
    mut q: Query<(&mut Transform, &mut Kinematic, &mut Inventory, &mut PlayerIntent, &mut Health, &mut Action), With<LocalPlayer>>,
    mut bots: Query<(Entity, &mut Transform, &mut Kinematic, &mut Health, &mut PlayerIntent), (With<Warrior>, Without<LocalPlayer>)>,
) {
    let Ok((mut tf, mut kin, mut inv, mut intent, mut health, mut act)) = q.single_mut() else { return };
    let spot = *arm.spot.get_or_insert_with(|| {
        let (x, z) = (38.0, 104.0);
        Vec3::new(x, layout.terrain.height(x, z) + HALF_HEIGHT + 0.05, z)
    });
    tf.translation = spot;
    kin.mode = MoveMode::Ground;
    kin.velocity = Vec3::ZERO;
    health.hp = 40.0;
    let Some(pose) = arm.poses.get(arm.index).copied() else { return };
    let t = arm.t;
    let def = db.get(pose.item);
    let stack = |count| Some(ItemStack { id: pose.item.into(), rarity: Rarity::Epic, count });
    match &def.kind {
        ItemKind::Melee { .. } => inv.selected = 0,
        ItemKind::Bow { magazine, .. } => {
            inv.slots[0] = stack(*magazine);
            inv.arrows = 60;
            inv.selected = 1;
        }
        _ => {
            inv.slots[0] = stack(3);
            inv.selected = 1;
        }
    }
    if pose.state == Use::Carry {
        act.since_attack = act.since_attack.max(5.0);
    }
    intent.yaw = 0.0;
    intent.pitch = pose.pitch;
    // The range target: the bot, frozen ahead, healed every tick; aim the crosshair ray at its chest.
    if let Some((bot, mut btf, mut bkin, mut bhp, mut bint)) = bots.iter_mut().next() {
        commands.entity(bot).remove::<shared::sim::bots::BotBrain>();
        let ahead = pose.target.unwrap_or(40.0);
        btf.translation = Vec3::new(spot.x, layout.terrain.height(spot.x, spot.z - ahead) + HALF_HEIGHT + 0.05, spot.z - ahead);
        btf.rotation = Quat::from_rotation_y(std::f32::consts::PI);
        bkin.mode = MoveMode::Ground;
        bkin.velocity = Vec3::ZERO;
        bhp.hp = 100.0;
        bhp.shield = if pose.item == "dunali" { 50.0 } else { 0.0 };
        *bint = PlayerIntent::default();
        if pose.target.is_some() {
            let aim_at = btf.translation + Vec3::Y * 0.3;
            for _ in 0..3 {
                let (origin, _) = shared::sim::aim_ray(spot, &intent);
                let d = aim_at - origin;
                intent.yaw = (-d.x).atan2(-d.z);
                intent.pitch = d.y.atan2(d.xz().length());
            }
        }
    }
    intent.move_axis = Vec2::ZERO;
    intent.select = None;
    intent.held = match pose.state {
        Use::Carry => 0,
        Use::Aim => btn::AIM,
        Use::Hold => btn::FIRE | btn::AIM,
        Use::Strike => if t >= STRIKE_AT { btn::FIRE } else { 0 },
    };
    let press_at = if pose.state == Use::Strike { STRIKE_AT } else { 0.2 };
    if intent.held(btn::FIRE) && t >= press_at && !arm.pressed {
        intent.pressed |= btn::FIRE;
        arm.pressed = true;
    }
}

/// Frames the player for each pose, screenshots it once settled, then moves to the next.
fn armoury_camera(
    mut commands: Commands,
    mut arm: ResMut<Armoury>,
    time: Res<Time>,
    player: Query<Entity, With<LocalPlayer>>,
    visuals: Query<(&crate::visuals::WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
    mut exit: MessageWriter<AppExit>,
) {
    arm.t += time.delta_secs();
    let (Ok(me), Ok(mut ct)) = (player.single(), cam.single_mut()) else { return };
    let Some(body) = visuals.iter().find(|(v, _)| v.sim == me).map(|(_, t)| t.translation) else { return };
    let Some(pose) = arm.poses.get(arm.index).copied() else {
        if arm.t > 0.5 {
            exit.write(AppExit::Success);
        }
        return;
    };
    // The player faces -Z; its right is +X.
    let chest = body + Vec3::Y * 0.45;
    let far = pose.target.unwrap_or(0.0);
    let view = match pose.view {
        View::Front => Some((chest + Vec3::new(1.5, 0.35, -2.1), chest + Vec3::new(0.15, -0.05, 0.0))),
        View::Side => Some((chest + Vec3::new(2.6, 0.2, -0.5), chest + Vec3::new(0.0, 0.0, -0.45))),
        View::Back => Some((chest + Vec3::new(0.9, 0.5, 2.3), chest + Vec3::new(0.0, 0.1, -0.6))),
        View::Range => Some((chest + Vec3::new(far * 0.55 + 2.0, 0.8, -far * 0.5), chest + Vec3::new(0.0, -0.2, -far * 0.5))),
        View::Player => None,
    };
    if let Some((eye, look)) = view {
        *ct = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    }
    let at = if pose.state == Use::Strike { STRIKE_AT + pose.after } else { POSE_TIME };
    if arm.t >= at && !arm.shot {
        let path = format!("{}/{}.png", arm.dir, arm.name(&pose));
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        arm.shot = true;
    }
    if arm.t >= at + 0.25 {
        arm.index += 1;
        arm.t = 0.0;
        arm.shot = false;
        arm.pressed = false;
    }
}
