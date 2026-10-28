//! Automated capture for play-testing without a human at the keyboard.
//!
//! VIVAD_TOUR=<dir>      fly the camera to each landmark, screenshot, exit.
//! VIVAD_SHOTS=<dir>     with VIVAD_AUTOPILOT=1: screenshot the match at set times, then exit.

use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use shared::sim::flow::Match;

use crate::camera::MainCamera;

#[derive(Resource)]
pub struct Tour {
    dir: String,
    shots: Vec<(&'static str, Vec3, Vec3)>,
    index: usize,
    frames: u32,
}

#[derive(Resource)]
pub struct MatchShots {
    dir: String,
    times: Vec<f32>,
    next: usize,
    taken_at: f32,
}

pub struct AutoshotPlugin;

impl Plugin for AutoshotPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(dir) = std::env::var("VIVAD_TOUR") {
            let v = |x, y, z| Vec3::new(x, y, z);
            app.insert_resource(Tour {
                dir,
                shots: vec![
                    ("01_overview", v(-60.0, 260.0, 300.0), v(0.0, 0.0, 0.0)),
                    ("02_bazaar_street", v(-45.0, 4.5, 2.0), v(20.0, 3.0, 0.0)),
                    ("03_bazaar_roofs", v(-30.0, 26.0, 40.0), v(5.0, 2.0, 0.0)),
                    ("04_stepwell", v(-80.0, 16.0, 108.0), v(-112.0, -6.0, 95.0)),
                    ("05_stepwell_inside", v(-92.0, -1.5, 95.0), v(-130.0, -9.0, 95.0)),
                    ("06_hampi", v(-95.0, 22.0, -95.0), v(-140.0, 2.0, -130.0)),
                    ("07_fort", v(70.0, 40.0, -60.0), v(125.0, 15.0, -135.0)),
                    ("08_ghats", v(200.0, 10.0, 60.0), v(160.0, 0.0, 30.0)),
                    ("09_caves_stupa", v(-120.0, 14.0, 40.0), v(-185.0, 6.0, 0.0)),
                    ("10_banyan", v(60.0, 9.0, 140.0), v(95.0, 6.0, 170.0)),
                    ("11_village", v(-10.0, 12.0, 160.0), v(-40.0, 1.0, 195.0)),
                    ("12_house_interior", v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
                ],
                index: 0,
                frames: 0,
            })
            .add_systems(PostUpdate, run_tour.after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
        }
        if let Ok(dir) = std::env::var("VIVAD_SHOTS") {
            app.insert_resource(MatchShots { dir, times: vec![6.0, 20.0, 34.0, 50.0, 80.0, 120.0, 170.0, 230.0], next: 0, taken_at: -1.0 })
                .add_systems(Update, match_shots);
        }
    }
}

fn run_tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
    houses: Query<(&shared::sim::MapBlock, &Transform), Without<MainCamera>>,
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
    *ct = Transform::from_translation(pos).looking_at(look, Vec3::Y);
    tour.frames += 1;
    // Let shadows, the atmosphere and LOD settle for a few frames per view.
    // The first view also waits for the scene to finish its first renders.
    let settle = if tour.index == 0 { 150 } else { 45 };
    if tour.frames == settle {
        let path = format!("{}/{}.png", tour.dir, name);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if tour.frames >= settle + 10 {
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
