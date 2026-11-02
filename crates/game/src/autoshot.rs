//! Automated capture for play-testing without a human at the keyboard.
//!
//! VIVAD_TOUR=<dir>      fly the camera to each landmark, screenshot, exit.
//!   VIVAD_TOUR_ONLY=a,b only the views whose names contain one of these.
//!   VIVAD_TOUR_FRAMES=n  n consecutive frames per view (`<name>_f<k>.png`), for flicker checks.
//!   VIVAD_TOUR_DRIFT=m   during a burst, slide the camera sideways m metres per frame.
//!   VIVAD_TOUR_SETTLE=n  frames to hold each view before its shot; above 120 it prints each view's
//!                        frame rate (`[tour-fps]`; run with VIVAD_NO_VSYNC=1).
//!   VIVAD_TOUR_VIEW=1    shoot the 3D view's own image, not the window: no HUD, but it works
//!                        with the window hidden.
//! On the NCR map (VIVAD_MAP=ncr) the views come from the map itself: from 500 m up, the bridges,
//! the ridge, and a street in every district, picked by `street_view`.
//!
//! A window that is not on screen (covered by another, on another Space, or on a sleeping or
//! locked display) is not rendered, and its shots come out black. Every shot here is checked: a
//! black one is thrown away and taken again from the 3D view's own image, with a warning, so it
//! loses the HUD but not the picture. Keep the game window in view when the HUD matters. The
//! recipes also run under `caffeinate -d -i -u`.
//!
//! VIVAD_CLIP=<dir>,<from>,<to>,<fps>  with VIVAD_AUTOPILOT=1: small frames (800 px wide) of the
//!                        match between two match times at that rate, then exit; `just clip` joins
//!                        them into an animation. VIVAD_CLIP_VIEW=1 takes the frames from the 3D
//!                        view's own image instead of the window: no HUD, but it works with the
//!                        window hidden.
//! VIVAD_SHOTS=<dir>     with VIVAD_AUTOPILOT=1: screenshot the match at set times, then exit.
//!   VIVAD_SHOTS_AT=a,b   at these match times (seconds) instead.
//!   VIVAD_SHOTS_MATCHES=n then start the next match (on the same map) and shoot it too, n
//!                        matches in all.
//!   VIVAD_SHOTS_KEYS=m:40,n:44 press these keys at these match times, to shoot what they open,
//!                        such as the whole map (M) and its zoom (N), or a quality level (f4).
//!   VIVAD_HUD_IMAGE=1    draw the HUD and the 3D view to an image instead of the window and shoot
//!                        that: the HUD's shots then work with the window hidden, though the window
//!                        itself stays empty.
//! VIVAD_KO=<t>[,aandhi]  the player falls at that match time, to the first bot or to the Aandhi, to
//!                        capture the result screen.
//! VIVAD_SPREE=<t>       from that match time the player fells the others, a second apart and each with
//!                        another weapon, through the real damage path: the kill feed and big calls.
//! VIVAD_ARMOURY=<dir>   stand the player on open ground and screenshot every item in hand, carried
//!   and in use (aiming, drawing, swinging, drinking), from the front and the side, then exit.
//!   VIVAD_ARMOURY_ONLY=a,b only the poses whose names contain one of these. The range poses
//!   freeze the one bot (VIVAD_BOTS=1) in front as a target and shoot or cut at it.
//!   VIVAD_ARMOURY_SPOT=x,z stand there instead (the middle of Connaught Place's park is 30,40).
//!   VIVAD_ARMOURY_OPEN=1 stand out in the open, for long looks through a scope.
//!   VIVAD_ARMOURY_RULES=1 only the scope glint, the Toradar's smoke and the chappal.
//!   VIVAD_ARMOURY_LONGSHOT=<fps>  only one pose, a rifle shot through the scope at a live bot
//!   150 m off, then watch it for a few seconds: frames for tools/clip.py.
//! VIVAD_FOOT=<x>,<z>[,<yaw degrees>]  the player starts the match on foot there, 9 m out in front
//!                        of a gumti's shutter with `gumti`, or down the longest road near a spot
//!                        with `street,<x>,<z>`, and logs what it does as `[foot]` lines:
//!                        a scripted run with VIVAD_KEYS and VIVAD_GIVE (crate::input), filmed by
//!                        VIVAD_CLIP from the game's own camera. VIVAD_TURN=<t>:<degrees>,... turns
//!                        the view at those match times, as a hand on the mouse would, in 0.3 s.

use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use shared::items::{ItemDb, ItemKind, ItemStack, Rarity};
use shared::map::terrain::ground;
use shared::map::{MapLayout, compose};
use shared::sim::flow::Match;
use shared::sim::{Action, DamageCause, DamageMsg, HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, SimSet, Warrior, btn};

use crate::camera::MainCamera;

#[derive(Resource)]
pub struct Tour {
    dir: String,
    /// Name, camera position, the point it looks at.
    shots: Vec<(String, Vec3, Vec3)>,
    index: usize,
    frames: u32,
    /// When this view's timing window opened (frame 30), for VIVAD_TOUR_SETTLE runs.
    timed_from: f64,
    /// Shots come from the 3D view's image rather than the window.
    view: bool,
}

/// Frames of a match between two match times, for a clip of play.
#[derive(Resource)]
pub struct Clip {
    dir: String,
    from: f32,
    to: f32,
    step: f32,
    next: f32,
    frame: u32,
    /// Frames of the 3D view's image rather than the window.
    view: bool,
}

#[derive(Resource)]
pub struct MatchShots {
    dir: String,
    times: Vec<f32>,
    next: usize,
    taken_at: f32,
    matches: u32,
    /// The match being shot (until the restart we asked for has happened).
    generation: u32,
}

/// Keys to press during a match, each with the match time to press it at.
#[derive(Resource)]
struct KeyPresses(Vec<(KeyCode, f32)>);

/// What VIVAD_HUD_IMAGE draws the HUD to.
#[derive(Resource)]
struct HudImage(RenderTarget);

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
    /// Aiming through the scope, then one press of fire.
    Snipe,
    /// Held to the shoulder as just after a shot, not looking through any scope.
    Shoulder,
    /// Carried at a run, on the spot (a treadmill: the run is pinned back each tick).
    Run,
    /// Running back toward the camera.
    RunBack,
    /// Crouch-walking forward.
    Crouch,
    /// Part way through a reload, the shot taken at `after` seconds into it.
    Reload,
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
    /// A close head shot, to judge faces in the game's own light.
    Face,
    /// Close on the left hand, from the left (does it hold the fore-end?).
    Hand,
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
    /// Frames a second of a clip of the pose (VIVAD_ARMOURY_LONGSHOT), the count so far, and when
    /// the next is due.
    frames: Option<f32>,
    frame: u32,
    next: f32,
}

impl Armoury {
    fn name(&self, p: &Pose) -> String {
        let state = match p.state {
            Use::Carry => "carry",
            Use::Aim => "aim",
            Use::Hold => "hold",
            Use::Strike => "strike",
            Use::Snipe => "snipe",
            Use::Shoulder => "shoulder",
            Use::Run => "run",
            Use::RunBack => "runback",
            Use::Crouch => "crouch",
            Use::Reload => "reload",
        };
        let view = match p.view {
            View::Front => "front",
            View::Side => "side",
            View::Back => "back",
            View::Range => "range",
            View::Player => "player",
            View::Face => "face",
            View::Hand => "hand",
        };
        let up = if p.pitch > 0.1 { "_up" } else if p.pitch < -0.1 { "_down" } else { "" };
        let at = if p.target.is_some() { format!("_{:.0}ms", p.after * 1000.0) } else { String::new() };
        format!("{:02}_{}_{state}{up}_{view}{at}", self.index, p.item)
    }
}

/// Settle time before each armoury shot; a strike is pressed STRIKE_AT seconds in.
const POSE_TIME: f32 = 1.3;
/// Seconds after the shot that a bullet reaches the long shot's target, which stands still till then.
const LANDS: f32 = 0.5;
/// Where to stand for a long look across the open: the park's south side, off its fountain.
const OPEN_X: f32 = 34.0;
const OPEN_Z: f32 = 72.0;
const STRIKE_AT: f32 = 1.1;

pub struct AutoshotPlugin;

impl Plugin for AutoshotPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(dir) = std::env::var("VIVAD_TOUR") {
            let v = |x, y, z| Vec3::new(x, y, z);
            let shots: Vec<(&str, Vec3, Vec3)> = vec![
                    ("01_overview", v(-60.0, 260.0, 300.0), v(0.0, 0.0, 0.0)),
                    ("02_bazaar_street", v(-45.0, 4.5, 2.0), v(20.0, 3.0, 0.0)),
                    // India Gate (VIVAD_MAP=indiagate): from down Kartavya Path, and from the side.
                    ("ig_gate", v(30.0, 3.0, -4.6), v(115.8, 20.0, -4.6)),
                    ("ig_gate_side", v(115.8, 8.0, 70.0), v(115.8, 18.0, -4.6)),
                    // Chandni Chowk's houses (VIVAD_MAP=chandni, seed 7): one from its lane,
                    // inside, and from above.
                    ("ch_house_n", v(-2.0, 2.0, -40.0), v(-2.0, 3.5, -50.0)),
                    ("ch_house_s", v(-2.0, 2.0, -55.0), v(-2.0, 3.5, -45.0)),
                    ("ch_inside_n", v(-2.0, 1.6, -44.6), v(-2.0, 1.5, -40.0)),
                    ("ch_inside_s", v(-2.0, 1.6, -50.2), v(-2.0, 1.5, -55.0)),
                    ("ch_air", v(-25.0, 30.0, -25.0), v(-2.0, 0.0, -47.0)),
                    // Down Chandni's lanes and a road, at eye height.
                    ("ch_lane", v(52.1, 1.8, 4.5), v(49.7, 4.0, 34.5)),
                    ("ch_lane2", v(48.1, 1.8, 57.0), v(50.5, 4.0, 27.0)),
                    ("ch_road", v(-58.6, 1.8, 60.2), v(-55.0, 4.0, 30.5)),
                    ("cp_aerial", v(40.0, 20.0, 175.0), v(0.0, 3.0, 118.0)),
                    ("cp_park", v(8.0, 1.7, 103.0), v(-14.0, 5.0, 138.0)),
                    ("cp_street", v(30.0, 2.2, 164.0), v(-2.0, 4.0, 150.0)),
                    ("cp_lane", v(163.1, 1.8, 135.8), v(185.2, 4.0, 115.6)),
                    ("cp_verandah", v(39.5, 1.7, 113.0), v(36.5, 1.5, 135.0)),
                    ("cp_upper", v(-32.0, 6.7, 118.0), v(-26.0, 6.0, 100.0)),
                    ("cp_ring_face", v(54.9, 1.7, 63.3), v(56.9, 4.0, 87.9)),
                    ("cp_ring_arcade", v(77.2, 1.7, 74.4), v(47.2, 2.2, 89.7)),
                    ("cp_ring_park", v(64.3, 2.5, 36.6), v(38.8, 4.0, 104.9)),
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
            ];
            let view = std::env::var("VIVAD_TOUR_VIEW").is_ok();
            app.insert_resource(Tour { timed_from: 0.0, dir, shots: shots.into_iter().map(|(n, p, l)| (n.to_owned(), p, l)).collect(), index: 0, frames: 0, view });
            if shared::map::ncr::enabled() {
                // Built from the map once it exists.
                app.add_systems(Startup, ncr_views);
            } else {
                keep_chosen_views(&mut app.world_mut().resource_mut::<Tour>());
            }
            app.add_systems(PostUpdate, run_tour.after(crate::camera::CameraSystems).before(crate::sky::SkyFollow).before(TransformSystems::Propagate));
        }
        if let Ok(dir) = std::env::var("VIVAD_ARMOURY") {
            let p = |item, state, view, pitch| Pose { item, state, view, pitch, target: None, after: 0.17 };
            let range = |item, view, dist, after| Pose { item, state: Use::Strike, view, pitch: 0.0, target: Some(dist), after };
            let snipe = |item, dist, after| Pose { item, state: if after > 0.0 { Use::Snipe } else { Use::Aim }, view: View::Player, pitch: 0.0, target: Some(dist), after };
            let mut poses = vec![
                p("bat", Use::Carry, View::Face, 0.0),
                p("bat", Use::Carry, View::Front, 0.0),
                p("bat", Use::Strike, View::Front, 0.0),
                p("bat", Use::Strike, View::Side, 0.0),
                p("lathi", Use::Carry, View::Front, 0.0),
                p("lathi", Use::Aim, View::Front, 0.0),
                p("lathi", Use::Strike, View::Side, 0.0),
                p("tawa", Use::Carry, View::Front, 0.0),
                p("tawa", Use::Aim, View::Front, 0.0),
                p("tawa", Use::Strike, View::Side, 0.0),
                p("dhal", Use::Carry, View::Front, 0.0),
                p("dhal", Use::Aim, View::Front, 0.0),
                p("dhal", Use::Strike, View::Side, 0.0),
                p("hockey_stick", Use::Carry, View::Front, 0.0),
                p("hockey_stick", Use::Aim, View::Front, 0.0),
                p("hockey_stick", Use::Strike, View::Side, 0.0),
                p("talwar", Use::Carry, View::Front, 0.0),
                p("talwar", Use::Aim, View::Front, 0.0),
                p("talwar", Use::Strike, View::Side, 0.0),
                p("gada", Use::Carry, View::Front, 0.0),
                p("gada", Use::Aim, View::Front, 0.0),
                p("gada", Use::Strike, View::Side, 0.0),
                p("bat", Use::Aim, View::Front, 0.0),
                p("tamancha", Use::Carry, View::Front, 0.0),
                p("tamancha", Use::Aim, View::Front, 0.0),
                p("tamancha", Use::Aim, View::Side, 0.0),
                p("tamancha", Use::Aim, View::Back, 0.0),
                p("tamancha", Use::Aim, View::Side, 0.5),
                p("katta", Use::Carry, View::Front, 0.0),
                p("katta", Use::Aim, View::Side, 0.0),
                p("katta", Use::Aim, View::Back, 0.0),
                p("dunali", Use::Carry, View::Side, 0.0),
                p("dunali", Use::Run, View::Side, 0.0),
                p("dunali", Use::RunBack, View::Back, 0.0),
                p("dunali", Use::RunBack, View::Side, 0.0),
                p("dunali", Use::Crouch, View::Side, 0.0),
                p("dunali", Use::Aim, View::Side, 0.0),
                p("gulel", Use::Hold, View::Side, 0.0),
                p("gulel", Use::Hold, View::Front, -0.4),
                p("vajra9", Use::Carry, View::Side, 0.0),
                p("vajra9", Use::Aim, View::Side, 0.0),
                p("trishul_carbine", Use::Aim, View::Side, 0.0),
                p("saintalis", Use::Carry, View::Side, 0.0),
                p("saintalis", Use::Aim, View::Side, 0.0),
                p("thaana_slr", Use::Shoulder, View::Side, 0.0),
                p("thri_not_thri", Use::Carry, View::Side, 0.0),
                p("thri_not_thri", Use::Shoulder, View::Side, 0.0),
                p("toradar", Use::Shoulder, View::Side, 0.0),
                p("thri_not_thri", Use::Shoulder, View::Back, 0.0),
                p("toradar", Use::Shoulder, View::Back, 0.0),
                // Through each scope, at a warrior 80 m off.
                snipe("thaana_slr", 80.0, 0.0),
                snipe("thri_not_thri", 80.0, 0.0),
                snipe("toradar", 80.0, 0.0),
                // The long shot: a hit at 150 m through the 4x, as the bullet lands.
                snipe("thri_not_thri", 150.0, 0.45),
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
                range("chakra", View::Range, 12.0, 0.3),
                range("chakra", View::Range, 12.0, 1.6),
                range("brahmastra", View::Range, 25.0, 1.4),
                range("brahmastra", View::Range, 25.0, 1.8),
            ];
            // Every long gun's hold: the left hand on the fore-end, and from behind, no stock
            // through the shoulder.
            for gun in ["dunali", "bandook", "vajra9", "trishul_carbine", "saintalis", "thaana_slr", "thri_not_thri", "toradar"] {
                poses.extend([
                    p(gun, Use::Carry, View::Hand, 0.0),
                    p(gun, Use::Run, View::Hand, 0.0),
                    p(gun, Use::Aim, View::Hand, 0.0),
                    p(gun, Use::Carry, View::Back, 0.0),
                    p(gun, Use::Run, View::Back, 0.0),
                    p(gun, Use::Aim, View::Back, 0.0),
                ]);
            }
            for after in [0.3, 1.0, 1.7] {
                for view in [View::Side, View::Front] {
                    poses.push(Pose { item: "saintalis", state: Use::Reload, view, pitch: 0.0, target: None, after });
                }
            }
            if let Ok(only) = std::env::var("VIVAD_ARMOURY_ONLY").map(|s| s.trim().to_owned()).and_then(|s| if s.is_empty() { Err(std::env::VarError::NotPresent) } else { Ok(s) }) {
                let keys: Vec<String> = only.split(',').map(str::to_owned).collect();
                poses.retain(|q| keys.iter().any(|k| q.item.contains(k.as_str())));
            }
            let frames = std::env::var("VIVAD_ARMOURY_LONGSHOT").ok().and_then(|s| s.parse().ok());
            if frames.is_some() {
                poses = vec![snipe("thri_not_thri", 150.0, 4.5)];
            }
            // VIVAD_ARMOURY_RULES=1: a far rifleman's scope catching the light, the Toradar's smoke
            // as it hangs, and the chappal in the hand and in the air.
            if std::env::var("VIVAD_ARMOURY_RULES").is_ok() {
                poses = vec![
                    snipe("thri_not_thri", 150.0, 0.0),
                    range("thaana_slr", View::Player, 80.0, 0.8),
                    p("chappal", Use::Carry, View::Front, 0.0),
                    range("chappal", View::Side, 12.0, 0.12),
                    range("chappal", View::Range, 12.0, 0.2),
                    range("chappal", View::Range, 12.0, 0.35),
                    range("toradar", View::Back, 40.0, 0.5),
                    range("toradar", View::Back, 40.0, 3.0),
                    range("toradar", View::Back, 40.0, 6.0),
                ];
            }
            app.insert_resource(Armoury { dir, poses, index: 0, t: -3.0, shot: false, pressed: false, spot: None, frames, frame: 0, next: 0.0 })
                .add_systems(PreUpdate, armoury_actor.after(crate::input::LocalInput))
                .add_systems(PostUpdate, armoury_camera.after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
        }
        if let Ok(spec) = std::env::var("VIVAD_CLIP") {
            let parts: Vec<&str> = spec.split(',').collect();
            let num = |k: usize, d: f32| parts.get(k).and_then(|s| s.trim().parse().ok()).unwrap_or(d);
            let (from, to, fps) = (num(1, 90.0), num(2, 210.0), num(3, 8.0));
            std::fs::create_dir_all(parts[0]).ok();
            let view = std::env::var("VIVAD_CLIP_VIEW").is_ok();
            app.insert_resource(Clip { dir: parts[0].to_owned(), from, to, step: 1.0 / fps.max(1.0), next: from, frame: 0, view })
                .add_systems(Update, clip_frames);
        }
        if std::env::var("VIVAD_FOOT").is_ok() {
            app.add_systems(PreUpdate, foot_run.after(crate::input::LocalInput));
        }
        if let Ok(spec) = std::env::var("VIVAD_KO") {
            let at = spec.split(',').next().and_then(|t| t.trim().parse().ok()).unwrap_or(60.0);
            app.insert_resource(Ko { at, aandhi: spec.contains("aandhi"), done: false }).add_systems(FixedUpdate, knock_out.before(SimSet::Damage));
        }
        if let Some(next) = std::env::var("VIVAD_SPREE").ok().and_then(|t| t.trim().parse().ok()) {
            app.insert_resource(Spree { next, done: 0 }).add_systems(FixedUpdate, spree.before(SimSet::Damage));
        }
        if let Ok(dir) = std::env::var("VIVAD_SHOTS") {
            let times = std::env::var("VIVAD_SHOTS_AT")
                .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
                .unwrap_or_else(|_| vec![6.0, 20.0, 34.0, 50.0, 80.0, 120.0, 170.0, 230.0]);
            let matches = std::env::var("VIVAD_SHOTS_MATCHES").ok().and_then(|n| n.parse().ok()).unwrap_or(1);
            app.insert_resource(MatchShots { dir, times, next: 0, taken_at: -1.0, matches, generation: 1 })
                .add_systems(Update, match_shots);
            if let Ok(spec) = std::env::var("VIVAD_SHOTS_KEYS") {
                let presses = spec.split(',').filter_map(|p| {
                    let (key, at) = p.split_once(':')?;
                    Some((letter_key(key.trim())?, at.trim().parse().ok()?))
                });
                app.insert_resource(KeyPresses(presses.collect()))
                    .add_systems(PreUpdate, press_keys.after(bevy::input::InputSystems).before(crate::hud::MapKeys));
            }
            if std::env::var("VIVAD_HUD_IMAGE").is_ok() {
                app.add_systems(Update, hud_to_image.run_if(not(resource_exists::<HudImage>)));
            }
        }
    }
}

/// The key for a letter, of the few a shot ever needs.
fn letter_key(letter: &str) -> Option<KeyCode> {
    match letter {
        "m" => Some(KeyCode::KeyM),
        "n" => Some(KeyCode::KeyN),
        "f4" => Some(KeyCode::F4),
        _ => None,
    }
}

/// Presses each key once its time comes, and lets go of it the frame after.
fn press_keys(mut keys: ResMut<ButtonInput<KeyCode>>, m: Res<Match>, mut presses: ResMut<KeyPresses>, mut down: Local<Vec<KeyCode>>) {
    for key in down.drain(..) {
        keys.release(key);
    }
    let elapsed = m.elapsed;
    presses.0.retain(|(key, at)| {
        let due = elapsed >= *at;
        if due {
            keys.press(*key);
            down.push(*key);
        }
        !due
    });
}

/// Sends the camera that draws the HUD to an image of the window's size: a hidden window is not
/// drawn, but an image is.
fn hud_to_image(mut commands: Commands, mut images: ResMut<Assets<Image>>, window: Query<&Window>, hud: Query<Entity, With<IsDefaultUiCamera>>) {
    let (Ok(w), Ok(camera)) = (window.single(), hud.single()) else { return };
    let size = w.physical_size().max(UVec2::ONE);
    let image = images.add(Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    let target = RenderTarget::Image(ImageRenderTarget { handle: image, scale_factor: w.scale_factor() });
    commands.entity(camera).insert(target.clone());
    commands.insert_resource(HudImage(target));
}

/// VIVAD_TOUR_ONLY: drop the views it doesn't name.
fn keep_chosen_views(tour: &mut Tour) {
    if let Ok(only) = std::env::var("VIVAD_TOUR_ONLY") {
        let keys: Vec<String> = only.split(',').map(str::to_owned).collect();
        tour.shots.retain(|s| keys.iter().any(|k| s.0.contains(k.as_str())));
    }
}

/// A street view near `at`: eye height, looking down the longest clear run of road near the
/// middle of the district. Read from the ground classes, so it follows the map as it changes.
fn street_view(layout: &MapLayout, at: Vec2) -> Option<(Vec3, Vec3)> {
    const LATTICE: f32 = 12.0;
    const TURNS: usize = 16;
    let t = &layout.terrain;
    // Some real buildings stand over a road's line; a view from inside one sees only its back faces.
    let near: Vec<_> = layout.footprints.iter().filter(|f| f.mat != shared::map::Mat::Water && f.pts[0].distance(at) < 400.0).collect();
    let road = |p: Vec2| t.ground_at(p.x, p.y) == Some(ground::ROAD) && !near.iter().any(|f| shared::map::footprint::contains(&f.pts, p));
    let mut best: Option<(f32, Vec2, Vec2)> = None;
    for i in -15..=15 {
        for j in -15..=15 {
            let p = at + Vec2::new(i as f32, j as f32) * LATTICE;
            if !road(p) {
                continue;
            }
            for k in 0..TURNS {
                let dir = Vec2::from_angle(k as f32 / TURNS as f32 * std::f32::consts::TAU);
                let mut run = 0.0;
                while run < 140.0 && road(p + dir * (run + 4.0)) {
                    run += 4.0;
                }
                let score = run - p.distance(at) * 0.2;
                if best.is_none_or(|b| score > b.0) {
                    best = Some((score, p, dir));
                }
            }
        }
    }
    let (_, p, dir) = best?;
    let look = p + dir * 60.0;
    Some((Vec3::new(p.x, t.height(p.x, p.y) + 1.8, p.y), Vec3::new(look.x, t.height(look.x, look.y) + 3.5, look.y)))
}

/// The tour of the one NCR map, taken from the map: high views, the bridges, the ridge and the
/// land between districts, then a street in every district.
fn ncr_views(layout: Res<MapLayout>, mut tour: ResMut<Tour>) {
    let v = Vec3::new;
    let ground_at = |p: Vec2, up: f32| v(p.x, layout.terrain.height(p.x, p.y) + up, p.y);
    let mut shots: Vec<(String, Vec3, Vec3)> = vec![
        // From 500 m, the height of the drop: over the river, and over each side of it.
        ("01_500m_river".into(), v(120.0, 500.0, 1000.0), v(120.0, 0.0, -300.0)),
        ("02_500m_delhi".into(), v(-740.0, 500.0, 250.0), v(-740.0, 0.0, -1000.0)),
        ("03_500m_gurugram".into(), v(-1100.0, 500.0, 300.0), v(-1500.0, 0.0, 1300.0)),
        ("04_500m_noida".into(), v(900.0, 500.0, -300.0), v(1300.0, 0.0, 700.0)),
    ];
    // The bridges, at eye height down the deck and from the bank; `compose::SPANS` order.
    for (k, name) in [(0, "signature"), (1, "loha_pul"), (2, "ito"), (3, "dnd"), (4, "okhla"), (6, "hindon")] {
        let s = &compose::SPANS[k];
        let dir = (s.c - s.a).normalize();
        let eye = s.a + dir * 40.0;
        shots.push((format!("05_bridge_{name}_deck"), v(eye.x, s.top + 1.8, eye.y), v(s.c.x, s.top + 2.5, s.c.y)));
        let bank = s.a - dir * 60.0 + dir.perp() * 90.0;
        let mid = s.a.lerp(s.c, 0.4);
        shots.push((format!("06_bridge_{name}_bank"), ground_at(bank, 6.0), v(mid.x, s.top, mid.y)));
    }
    // The DND's toll plaza, walking up to its canopy: it stands 28 m short of the far end.
    let s = &compose::SPANS[3];
    let dir = (s.c - s.a).normalize();
    let (eye, plaza) = (s.c - dir * 75.0, s.c - dir * 28.0);
    shots.push(("05_bridge_dnd_toll_plaza".to_string(), v(eye.x, s.top + 1.8, eye.y), v(plaza.x, s.top + 3.5, plaza.y)));
    // The land between the districts.
    shots.extend([
        ("07_ridge_pass".to_string(), ground_at(Vec2::new(-1240.0, 60.0), 1.8), ground_at(Vec2::new(-1340.0, 330.0), 3.0)),
        ("07_ridge_crest".to_string(), ground_at(Vec2::new(-1215.0, -420.0), 1.8), ground_at(Vec2::new(-1000.0, -560.0), 2.0)),
        ("07_yamuna_bank".to_string(), ground_at(Vec2::new(30.0, -300.0), 1.8), ground_at(Vec2::new(330.0, -350.0), 1.0)),
        ("07_ring_road".to_string(), ground_at(Vec2::new(-1830.0, -700.0), 1.8), ground_at(Vec2::new(-1600.0, -470.0), 3.0)),
    ]);
    // Leaves: a thicket on the Ridge from its edge, a clump on the open land and a park's hedge.
    let soft = |low: f32, high: f32, turn: f32, back: f32, up: f32| {
        let s = layout.soft.iter().filter(|s| (low..high).contains(&s.size.y)).nth(9)?;
        let dir = Vec2::from_angle(turn);
        Some((ground_at(s.pos.xz() - dir * back, up), ground_at(s.pos.xz(), 0.8)))
    };
    for (name, view) in [("07_leaves_thicket", soft(2.0, 3.0, 0.7, 34.0, 1.8)), ("07_leaves_clump", soft(1.45, 1.55, 2.2, 11.0, 1.8)), ("07_leaves_ridge_above", soft(2.0, 3.0, 2.4, 70.0, 16.0))] {
        if let Some((eye, look)) = view {
            shots.push((name.to_string(), eye, look));
        }
    }
    if let Some(s) = layout.soft.iter().find(|s| s.size.x == 12.0) {
        let across = Quat::from_rotation_y(s.yaw) * Vec3::Z;
        shots.push(("07_leaves_hedge".to_string(), ground_at(s.pos.xz() + across.xz() * 9.0 + Vec2::splat(4.0), 1.8), ground_at(s.pos.xz(), 0.8)));
    }
    // A street in every district.
    for d in &compose::PLACES {
        if let Some((eye, look)) = street_view(&layout, d.at) {
            shots.push((format!("08_street_{}", d.name.to_lowercase().replace(' ', "_")), eye, look));
        } else {
            warn!("no street to shoot in {}", d.name);
        }
    }
    // The named spots on the open land, each seen across its yard from a rooftop's height.
    for spot in layout.pois.iter().filter(|p| !compose::PLACES.iter().any(|d| d.name == p.name) && !compose::BRIDGES.contains(&p.name.as_str())) {
        let at = spot.center.xz();
        // From the side the map's middle is on, or the next side round with no building in the way.
        let eyes = (0..8).map(|k| at + Vec2::from_angle((-at).to_angle() + k as f32 * std::f32::consts::FRAC_PI_4) * 70.0);
        let blocked = |e: &Vec2| (0..=6).filter(|s| compose::built(&layout, e.lerp(at, *s as f32 / 7.0))).count();
        let eye = eyes.min_by_key(blocked).unwrap_or(at);
        shots.push((format!("09_spot_{}", spot.name.to_lowercase().replace(' ', "_")), ground_at(eye, 12.0), ground_at(at, 1.0)));
    }
    // The places built by hand, and water that was cut at a district's edge.
    shots.extend([
        ("10_red_fort_gate".to_string(), v(-132.0, 3.0, -1350.0), v(-100.0, 11.0, -1410.0)),
        ("10_red_fort_wall".to_string(), v(-140.0, 2.0, -1610.0), v(-90.0, 8.0, -1420.0)),
        ("10_blue_line_west".to_string(), v(-125.0, 2.0, -420.0), v(-36.0, 9.0, -470.0)),
        ("10_blue_line_east".to_string(), v(330.0, 6.0, -420.0), v(384.0, 9.0, -470.0)),
        ("10_hauz_khas_lake".to_string(), v(-960.0, 40.0, 640.0), v(-1120.0, -1.0, 480.0)),
        ("10_kartavya_canal".to_string(), v(-690.0, 1.8, -109.0), v(-520.0, -0.5, -109.0)),
        ("10_anand_vihar_terminal".to_string(), v(800.0, 3.0, -1130.0), v(886.0, 4.0, -1045.0)),
        ("10_anand_vihar_air".to_string(), v(760.0, 90.0, -1190.0), v(880.0, 0.0, -1040.0)),
        ("10_badarpur_plant".to_string(), v(120.0, 8.0, 1180.0), v(260.0, 20.0, 1250.0)),
    ]);
    tour.shots = shots;
    keep_chosen_views(&mut tour);
    info!("tour: {} views of the NCR map", tour.shots.len());
}

/// Is this shot black? A window that is not on screen gives one of nothing but zeros.
fn is_black(shot: &Image) -> bool {
    shot.data.as_ref().is_none_or(|d| d.chunks_exact(4).step_by(7).all(|p| p[0] < 6 && p[1] < 6 && p[2] < 6))
}

/// Screenshots the window; if that comes back black, takes the 3D view's own image instead.
fn shoot(commands: &mut Commands, path: String) {
    commands.spawn(Screenshot::primary_window()).observe(move |shot: On<ScreenshotCaptured>, mut commands: Commands, view: Res<crate::quality::View3d>| {
        if is_black(&shot.image) {
            warn!("{path}: the window's shot is black (is the window on screen?); taking the 3D view's own image, without the HUD");
            commands.spawn(Screenshot::image(view.image.clone())).observe(save_to_disk(path.clone()));
        } else {
            save_to_disk(path.clone())(shot);
        }
    });
}

/// When the player falls for a capture of the result screen (VIVAD_KO).
#[derive(Resource)]
struct Ko {
    at: f32,
    aandhi: bool,
    done: bool,
}

fn knock_out(
    mut ko: ResMut<Ko>,
    m: Res<Match>,
    me: Query<Entity, With<LocalPlayer>>,
    others: Query<Entity, (With<Warrior>, Without<LocalPlayer>)>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let Ok(me) = me.single() else { return };
    if ko.done || m.elapsed < ko.at {
        return;
    }
    ko.done = true;
    let (source, cause) = if ko.aandhi { (None, DamageCause::Pralaya) } else { (others.iter().next(), DamageCause::Weapon("katta".into())) };
    damage.write(DamageMsg { target: me, amount: 1000.0, source, headshot: false, cause });
}

/// The next kill of a capture's spree (VIVAD_SPREE).
#[derive(Resource)]
struct Spree {
    next: f32,
    done: usize,
}

/// Each kill's weapon, and whether it is a headshot.
const SPREE: [(&str, bool); 6] = [("bat", false), ("tamancha", true), ("dunali", false), ("katta", false), ("gulel", true), ("agneyastra", false)];

fn spree(
    mut s: ResMut<Spree>,
    m: Res<Match>,
    me: Query<Entity, With<LocalPlayer>>,
    others: Query<Entity, (With<Warrior>, Without<LocalPlayer>, Without<shared::sim::Eliminated>)>,
    mut damage: MessageWriter<DamageMsg>,
) {
    let (Ok(me), Some(target)) = (me.single(), others.iter().next()) else { return };
    let Some(&(item, headshot)) = SPREE.get(s.done).filter(|_| m.elapsed >= s.next) else { return };
    s.done += 1;
    s.next += 1.0;
    damage.write(DamageMsg { target, amount: 1000.0, source: Some(me), headshot, cause: DamageCause::Weapon(item.into()) });
}

fn run_tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
    houses: Query<(&shared::sim::MapBlock, &Transform), Without<MainCamera>>,
    chests: Query<(&shared::sim::loot::Chest, &Transform), Without<MainCamera>>,
    pickups: Query<(&shared::sim::loot::Pickup, &Transform), Without<MainCamera>>,
    time: Res<Time<Real>>,
    view: Res<crate::quality::View3d>,
    kit: Res<crate::street::Kit>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(mut ct) = cam.single_mut() else { return };
    // The street kit's file comes a while after the first frames, later on a busy machine, and a
    // first view shot before it has no street life. The limit is for a file that never comes.
    if tour.index == 0 && !kit.built && time.elapsed_secs() < 60.0 {
        return;
    }
    if tour.index >= tour.shots.len() {
        tour.frames += 1;
        if tour.frames > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (name, mut pos, mut look) = tour.shots[tour.index].clone();
    let name = name.as_str();
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
        if tour.view {
            commands.spawn(Screenshot::image(view.image.clone())).observe(save_to_disk(path));
        } else {
            shoot(&mut commands, path);
        }
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
    mut restart: MessageWriter<shared::sim::RestartMatch>,
    hud: Option<Res<HudImage>>,
) {
    let now = time.elapsed_secs();
    if m.generation < shots.generation {
        return;
    }
    if shots.next >= shots.times.len() {
        if now - shots.taken_at > 1.0 {
            if m.generation < shots.matches {
                restart.write(shared::sim::RestartMatch);
                shots.next = 0;
                shots.generation = m.generation + 1;
            } else {
                exit.write(AppExit::Success);
            }
        }
        return;
    }
    if m.elapsed >= shots.times[shots.next] {
        let path = format!("{}/match{}_{:02}_{:03.0}s.png", shots.dir, m.generation, shots.next, m.elapsed);
        match hud {
            Some(hud) => {
                // The whole target, as the camera has it: a shot of the image alone would be a
                // target with another scale, which Bevy would leave black.
                commands.spawn(Screenshot(hud.0.clone())).observe(save_to_disk(path));
            }
            None => shoot(&mut commands, path),
        }
        shots.next += 1;
        shots.taken_at = now;
    }
}

/// A frame of the clip, shrunk to 800 px wide so two minutes of them fit on a disk.
pub fn save_small(path: String) -> impl FnMut(On<ScreenshotCaptured>) {
    move |captured| {
        let img = captured.image.clone();
        let path = path.clone();
        // Shrinking and encoding take longer than a frame: off the main thread with them.
        std::thread::spawn(move || {
            if let Ok(full) = img.try_into_dynamic() {
                let (w, h) = (full.width().max(1), full.height());
                if let Err(e) = full.thumbnail(800, 800 * h / w).save(&path) {
                    warn!("clip frame {path}: {e}");
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn clip_frames(
    mut commands: Commands,
    mut clip: ResMut<Clip>,
    m: Res<Match>,
    time: Res<Time<Real>>,
    view: Res<crate::quality::View3d>,
    mut exit: MessageWriter<AppExit>,
) {
    if m.elapsed > clip.to + 1.0 {
        info!("clip done: {} frames", clip.frame);
        exit.write(AppExit::Success);
        return;
    }
    if m.elapsed < clip.from || m.elapsed < clip.next || m.elapsed > clip.to {
        return;
    }
    if clip.frame == 0 && !clip.view {
        info!("clip: keep the game window in view; a window off screen is not rendered and its frames are black");
    }
    let path = format!("{}/frame_{:05}.png", clip.dir, clip.frame);
    let shot = if clip.view { Screenshot::image(view.image.clone()) } else { Screenshot::primary_window() };
    commands.spawn(shot).observe(save_small(path));
    if clip.frame % 8 == 0 {
        info!("clip frame {} at match {:.1}s ({:?}, {} alive), wall {:.1}s", clip.frame, m.elapsed, m.state, m.alive, time.elapsed_secs());
    }
    clip.frame += 1;
    clip.next += clip.step;
}

/// Puts the player on foot once, then logs the run in numbers (VIVAD_FOOT).
#[allow(clippy::type_complexity)]
fn foot_run(
    layout: Res<MapLayout>,
    m: Res<Match>,
    mut q: Query<(&mut Transform, &mut Kinematic, &mut PlayerIntent, &Inventory), With<LocalPlayer>>,
    mut placed: Local<bool>,
    mut next: Local<f32>,
    mut turned: Local<Vec<f32>>,
) {
    let Ok((mut tf, mut kin, mut intent, inv)) = q.single_mut() else { return };
    // By how far through each turn the match is, so a slow frame can't skip one.
    let turns: Vec<(f32, f32)> = std::env::var("VIVAD_TURN")
        .unwrap_or_default()
        .split(',')
        .filter_map(|turn| {
            let (at, degrees) = turn.split_once(':')?;
            Some((at.trim().parse().ok()?, degrees.trim().parse().ok()?))
        })
        .collect();
    turned.resize(turns.len(), 0.0);
    for ((at, degrees), done) in turns.into_iter().zip(turned.iter_mut()) {
        let share = ((m.elapsed - at) / 0.3).clamp(0.0, 1.0);
        intent.yaw += degrees.to_radians() * (share - *done);
        *done = share;
    }
    if !*placed {
        *placed = true;
        let spec = std::env::var("VIVAD_FOOT").unwrap_or_default();
        let num: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let (at, yaw) = match (spec.as_str(), num.as_slice()) {
            ("gumti", _) => {
                let g = layout.kit.iter().find(|k| k.name == "gumti").expect("a map with gumtis");
                let out = Quat::from_rotation_y(g.yaw) * Vec3::NEG_Z;
                (g.pos.xz() + out.xz() * 9.0, (out.x).atan2(out.z))
            }
            (s, [x, z, ..]) if s.starts_with("street") => {
                // At the near end of the longest straight road by there.
                let (eye, look) = street_view(&layout, Vec2::new(*x, *z)).expect("a road near there");
                let d = (look - eye).xz();
                (eye.xz(), (-d.x).atan2(-d.y))
            }
            (_, [x, z, yaw, ..]) => (Vec2::new(*x, *z), yaw.to_radians()),
            (_, [x, z, ..]) => (Vec2::new(*x, *z), 0.0),
            _ => (Vec2::ZERO, 0.0),
        };
        tf.translation = Vec3::new(at.x, layout.terrain.height(at.x, at.y) + HALF_HEIGHT + 0.05, at.y);
        kin.mode = MoveMode::Ground;
        kin.velocity = Vec3::ZERO;
        kin.fall_from = Some(tf.translation.y);
        intent.yaw = yaw;
        println!("[foot] on foot at {at} facing {:.0} degrees", yaw.to_degrees());
    }
    if m.elapsed >= *next {
        *next = m.elapsed + 0.25;
        let low = if kin.crouched { " low" } else { "" };
        let slide = if kin.sliding { " sliding" } else { "" };
        let air = if kin.grounded { "" } else { " air" };
        let item = inv.current().map_or("nothing", |s| s.id.as_str());
        println!(
            "[foot] t {:.2} speed {:.2} vy {:.2} at ({:.1}, {:.2}, {:.1}) held {:#x} {item}{low}{slide}{air}",
            m.elapsed, kin.velocity.with_y(0.0).length(), kin.velocity.y, tf.translation.x, tf.translation.y, tf.translation.z, intent.held
        );
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
    mut bots: Query<(Entity, &mut Transform, &mut Kinematic, &mut Health, &mut PlayerIntent, &mut Inventory), (With<Warrior>, Without<LocalPlayer>)>,
    mut told: Local<bool>,
    brains: Query<&shared::sim::bots::BotBrain>,
) {
    let Ok((mut tf, mut kin, mut inv, mut intent, mut health, mut act)) = q.single_mut() else { return };
    let open = std::env::var("VIVAD_ARMOURY_OPEN").is_ok() || arm.frames.is_some();
    let spot = *arm.spot.get_or_insert_with(|| {
        let parse = |s: &str| s.trim().parse::<f32>().ok();
        let spot = std::env::var("VIVAD_ARMOURY_SPOT").ok().and_then(|s| {
            let (x, z) = s.split_once(',')?;
            Some((parse(x)?, parse(z)?))
        });
        let (x, z) = spot.unwrap_or(if open { (OPEN_X, OPEN_Z) } else { (38.0, 104.0) });
        Vec3::new(x, layout.terrain.height(x, z) + HALF_HEIGHT + 0.05, z)
    });
    tf.translation = spot;
    kin.mode = MoveMode::Ground;
    health.hp = 40.0;
    let Some(pose) = arm.poses.get(arm.index).copied() else { return };
    if !matches!(pose.state, Use::Run | Use::RunBack | Use::Crouch) {
        kin.velocity = Vec3::ZERO;
    }
    let t = arm.t;
    let def = db.get(pose.item);
    let stack = |count| Some(ItemStack { id: pose.item.into(), rarity: Rarity::Epic, count });
    match &def.kind {
        ItemKind::Melee { .. } if pose.item == db.default_melee => inv.selected = 0,
        ItemKind::Bow { magazine, .. } => {
            inv.slots[0] = stack(*magazine - (pose.state == Use::Reload) as u32);
            inv.arrows = 60;
            inv.selected = 1;
        }
        _ => {
            inv.slots[0] = stack(3);
            inv.selected = 1;
        }
    }
    if matches!(pose.state, Use::Carry | Use::Run | Use::RunBack | Use::Crouch | Use::Reload) {
        act.since_attack = act.since_attack.max(5.0);
    }
    intent.yaw = 0.0;
    intent.pitch = pose.pitch;
    // The range target: the bot, frozen ahead, healed every tick; aim the crosshair ray at its chest.
    if let Some((bot, mut btf, mut bkin, mut bhp, mut bint, mut binv)) = bots.iter_mut().next() {
        // A long-shot clip leaves the bot its wits, and it is on its own once hit.
        let live = arm.frames.is_some();
        if !live {
            commands.entity(bot).remove::<shared::sim::bots::BotBrain>();
        }
        let ahead = pose.target.unwrap_or(40.0);
        if live && t >= STRIKE_AT + LANDS {
            if t >= STRIKE_AT + pose.after - 0.3 && !*told {
                *told = true;
                println!("[longshot] {:.0} m after the hit, {:.0} m from the shooter, {:.0} health left", btf.translation.distance(Vec3::new(spot.x, btf.translation.y, spot.z - ahead)), btf.translation.distance(spot), bhp.hp);
                if let Ok(b) = brains.get(bot) {
                    println!("[longshot] still running from the shot for {:.1} s", b.flee.max(0.0));
                }
            }
            return_aim(&mut intent, pose, t, &mut arm);
            return;
        }
        btf.translation = Vec3::new(spot.x, layout.terrain.height(spot.x, spot.z - ahead) + HALF_HEIGHT + 0.05, spot.z - ahead);
        btf.rotation = Quat::from_rotation_y(std::f32::consts::PI);
        bkin.mode = MoveMode::Ground;
        bkin.velocity = Vec3::ZERO;
        bhp.hp = 100.0;
        bhp.shield = if pose.item == "dunali" { 50.0 } else { 0.0 };
        *bint = PlayerIntent::default();
        // A rifleman looking back down his scope.
        if std::env::var("VIVAD_ARMOURY_RULES").is_ok() && pose.target.is_some_and(|d| d >= 80.0) {
            binv.slots[0] = Some(ItemStack { id: "thri_not_thri".into(), rarity: Rarity::Rare, count: 5 });
            binv.selected = 1;
            let d = spot - btf.translation;
            bint.yaw = (-d.x).atan2(-d.z);
            bint.held = btn::AIM;
        }
        // The player's far shot is answered at once, for the sound of a rifle heard from far off.
        if pose.state == Use::Snipe && (STRIKE_AT..STRIKE_AT + 0.25).contains(&t) {
            binv.slots[0] = Some(ItemStack { id: "thri_not_thri".into(), rarity: Rarity::Rare, count: 5 });
            binv.selected = 1;
            let d = spot - btf.translation;
            bint.yaw = (-d.x).atan2(-d.z);
            bint.held = btn::FIRE;
        }
        if pose.target.is_some() {
            let aim_at = btf.translation + Vec3::Y * 0.3;
            for _ in 0..3 {
                let (origin, _) = shared::sim::aim_ray(spot, &intent);
                let d = aim_at - origin;
                intent.yaw = (-d.x).atan2(-d.z);
                intent.pitch = d.y.atan2(d.xz().length());
            }
            // Through a scope the line is from the eye, held over for the bullet's drop.
            if let (Some(_), ItemKind::Bow { speed, gravity, .. }) = (def.kind.scope(), &def.kind) {
                let eye = spot + Vec3::Y * shared::sim::EYE_OFFSET;
                let flight = ahead / speed;
                let d = aim_at + Vec3::Y * (0.5 * gravity * flight * flight) - eye;
                intent.yaw = (-d.x).atan2(-d.z);
                intent.pitch = d.y.atan2(d.xz().length());
            }
        }
    }
    return_aim(&mut intent, pose, t, &mut arm);
    if pose.state == Use::Shoulder {
        act.since_attack = 0.5;
    }
}

/// The pose's buttons, the last thing the actor does each frame.
fn return_aim(intent: &mut PlayerIntent, pose: Pose, t: f32, arm: &mut Armoury) {
    intent.move_axis = match pose.state {
        Use::Run | Use::Crouch => Vec2::Y,
        Use::RunBack => Vec2::NEG_Y,
        _ => Vec2::ZERO,
    };
    intent.select = None;
    intent.held = match pose.state {
        Use::Carry | Use::Shoulder | Use::Run | Use::RunBack | Use::Reload => 0,
        Use::Crouch => btn::CROUCH,
        Use::Aim => btn::AIM,
        Use::Hold => btn::FIRE | btn::AIM,
        Use::Strike => if t >= STRIKE_AT { btn::FIRE } else { 0 },
        // One shot, and no second when the bolt is back: the clip watches what it did.
        Use::Snipe => btn::AIM | if (STRIKE_AT..STRIKE_AT + 0.4).contains(&t) { btn::FIRE } else { 0 },
    };
    if pose.state == Use::Reload && t >= STRIKE_AT && !arm.pressed {
        intent.pressed |= btn::RELOAD;
        arm.pressed = true;
    }
    let press_at = if matches!(pose.state, Use::Strike | Use::Snipe) { STRIKE_AT } else { 0.2 };
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
    view3d: Res<crate::quality::View3d>,
    mut exit: MessageWriter<AppExit>,
    mut check: ResMut<crate::arms::GunCheck>,
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
        View::Face => Some((chest + Vec3::new(0.22, 0.34, -0.62), chest + Vec3::new(0.0, 0.3, -0.05))),
        // On the fore-end where the left hand should hold it, from the left and a little below.
        View::Hand => check.1.map(|at| (at + Vec3::new(if std::env::var("VIVAD_HAND_RIGHT").is_ok() { 0.42 } else { -0.42 }, -0.08, 0.05), at)),
    };
    if let Some((eye, look)) = view {
        *ct = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    }
    let at = if matches!(pose.state, Use::Strike | Use::Snipe | Use::Reload) { STRIKE_AT + pose.after } else { POSE_TIME };
    if let Some(fps) = arm.frames.filter(|_| arm.t >= STRIKE_AT - 0.5 && arm.t >= arm.next) {
        let path = format!("{}/frame_{:05}.png", arm.dir, arm.frame);
        commands.spawn(Screenshot::primary_window()).observe(save_small(path));
        arm.frame += 1;
        arm.next = arm.t + 1.0 / fps;
    }
    if arm.t >= at && !arm.shot {
        let path = format!("{}/{}.png", arm.dir, arm.name(&pose));
        if let Some(c) = check.0.take().filter(|_| crate::visuals::long_gun(pose.item)) {
            info!("[guncheck] {}: {c}", arm.name(&pose));
        }
        // As for a clip: VIVAD_CLIP_VIEW=1 works with the window hidden, without the HUD.
        if std::env::var("VIVAD_CLIP_VIEW").is_ok() {
            commands.spawn(Screenshot::image(view3d.image.clone())).observe(save_to_disk(path));
        } else {
            shoot(&mut commands, path);
        }
        arm.shot = true;
    }
    if arm.t >= at + 0.25 {
        arm.index += 1;
        arm.t = 0.0;
        arm.shot = false;
        arm.pressed = false;
    }
}
