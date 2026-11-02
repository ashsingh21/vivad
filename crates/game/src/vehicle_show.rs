//! Vehicles driven by a script, for clips and pictures from the real game. It presses the real keys
//! (E, W, A, S, D, H), so everything from `ButtonInput` to the screen is what a player gets.
//!
//! VIVAD_DRIVE=<kind>[,<radius>[,<seconds>]]  stand beside a vehicle on the road round VIVAD_RING
//!                  (x,z of its centre, default Connaught Place's), get in, drive round it, sound
//!                  the horn now and then, turn into the ring's outer wall and scrape along it, brake,
//!                  get out. With VIVAD_CLIP for the frames.
//! VIVAD_ROAD=<kind>[,<seconds>]  on the NCR, get in at the head of the Ring Road and drive it flat
//!                  out for that long by the same keys, braking for the bends, then stop and stand
//!                  for 25 s. Logs the frames of each: mean, 99th percentile, the worst and where, so
//!                  the near tier at road speed is measured against the same run standing still.
//! VIVAD_FLEET=<dir>  on the NCR, a picture of each kind where this match really parked it, then a
//!                  drivable auto beside the street kit's tarped one, then bots at the wheel from the
//!                  roadside as they come. Exits once it has them, or at 170 s of match.
//! VIVAD_SHOW=<dir> one picture of each vehicle parked, driven and from the side with its driver, then
//!                  of health, smoke and a wreck, then exit. VIVAD_SHOW_ONLY=a,b only the pictures whose names
//!                  hold one of these. VIVAD_SHOW_WINDOW=1 takes the window, HUD and all (keep it
//!                  in view); otherwise the 3D view.
//! Kinds: auto, scooter, bike, phatphati, gaadi, khuligaadi, sarkarigaadi, chhotatruck, tractor.

use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use shared::sim::bots::BotBrain;
use shared::map::{MapLayout, compose};
use shared::sim::flow::Match;
use shared::sim::vehicle::{Riding, Vehicle, VehicleKind, spawn_vehicle};
use shared::sim::{HALF_HEIGHT, Health, Kinematic, LocalPlayer, MoveMode};

use crate::camera::{CameraSystems, MainCamera};
use crate::input::LocalInput;

pub struct VehicleShowPlugin;

impl Plugin for VehicleShowPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(spec) = std::env::var("VIVAD_DRIVE") {
            let mut parts = spec.split(',');
            let kind = parts.next().and_then(kind_named).unwrap_or(VehicleKind::Auto);
            let num = |p: Option<&str>, d: f32| p.and_then(|s| s.trim().parse().ok()).unwrap_or(d);
            let (radius, seconds) = (num(parts.next(), 124.0), num(parts.next(), 44.0));
            app.insert_resource(Lap { kind, centre: ring(), radius, seconds, phase: 0, t: 0.0, vehicle: None, horn: 5.0 })
                .add_systems(PreUpdate, lap.after(bevy::input::InputSystems).before(LocalInput));
        }
        if let Ok(spec) = std::env::var("VIVAD_ROAD") {
            let mut parts = spec.split(',');
            let kind = parts.next().and_then(kind_named).unwrap_or(VehicleKind::Bullet);
            let seconds = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(150.0);
            let line = compose::network().swap_remove(0).1;
            app.insert_resource(Road { kind, seconds, line, next: 0, phase: 0, t: 0.0, vehicle: None, frames: vec![], worst: (0.0, Vec2::ZERO, 0.0), top: 0.0 })
                .add_systems(PreUpdate, road.after(bevy::input::InputSystems).before(LocalInput));
        }
        if let Ok(dir) = std::env::var("VIVAD_FLEET") {
            std::fs::create_dir_all(&dir).ok();
            app.insert_resource(Fleet { dir, step: 0, t: 0.0, shot: false, eye: None, watching: None, left: 5 })
                .add_systems(PreUpdate, fleet.after(bevy::input::InputSystems).before(LocalInput))
                .add_systems(PostUpdate, fleet_camera.after(CameraSystems).before(TransformSystems::Propagate));
        }
        if let Ok(dir) = std::env::var("VIVAD_SHOW") {
            std::fs::create_dir_all(&dir).ok();
            let mut list = takes();
            if let Ok(only) = std::env::var("VIVAD_SHOW_ONLY") {
                list.retain(|t| only.split(',').any(|k| t.name.contains(k)));
            }
            app.insert_resource(Show { dir, takes: list, index: 0, t: 0.0, vehicle: None, shot: false, centre: ring(), radius: 124.0 })
                .add_systems(PreUpdate, show.after(bevy::input::InputSystems).before(LocalInput))
                .add_systems(PostUpdate, show_camera.after(CameraSystems).before(TransformSystems::Propagate));
        }
    }
}

fn kind_named(s: &str) -> Option<VehicleKind> {
    VehicleKind::ALL.into_iter().find(|k| k.spec().name.replace(' ', "").eq_ignore_ascii_case(s.trim()))
}

/// VIVAD_RING=x,z: the middle of the road's circle.
fn ring() -> Vec2 {
    let v: Vec<f32> = std::env::var("VIVAD_RING").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if v.len() == 2 { Vec2::new(v[0], v[1]) } else { Vec2::new(0.0, -40.0) }
}

fn on_ring(centre: Vec2, radius: f32, a: f32) -> Vec2 {
    centre + Vec2::new(a.cos(), a.sin()) * radius
}

/// The yaw facing along (x, z).
fn facing(d: Vec2) -> f32 {
    (-d.x).atan2(-d.y)
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn hold(keys: &mut ButtonInput<KeyCode>, key: KeyCode, down: bool) {
    if down {
        keys.press(key);
    } else {
        keys.release(key);
    }
}

fn tap(keys: &mut ButtonInput<KeyCode>, key: KeyCode, on: bool) {
    hold(keys, key, on && !keys.pressed(key));
}

/// A and D held to chase a point ahead on the circle.
fn steer_round(keys: &mut ButtonInput<KeyCode>, centre: Vec2, radius: f32, t: &Transform, v: &Vehicle) {
    let p = t.translation.xz() - centre;
    let target = on_ring(centre, radius, p.y.atan2(p.x) + 18.0 / radius);
    let off = wrap(facing(target - t.translation.xz()) - v.heading);
    hold(keys, KeyCode::KeyA, off > 0.03);
    hold(keys, KeyCode::KeyD, off < -0.03);
}

/// Stand the player on the ground, facing `yaw`.
fn stand(tf: &mut Transform, kin: &mut Kinematic, at: Vec3, yaw: f32) {
    tf.translation = at + Vec3::Y * (HALF_HEIGHT + 0.02);
    tf.rotation = Quat::from_rotation_y(yaw);
    kin.mode = MoveMode::Ground;
    kin.velocity = Vec3::ZERO;
    kin.grounded = true;
    kin.ground_normal = Vec3::Y;
}

#[derive(Resource)]
struct Lap {
    kind: VehicleKind,
    centre: Vec2,
    radius: f32,
    seconds: f32,
    phase: u8,
    t: f32,
    vehicle: Option<Entity>,
    horn: f32,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn lap(
    mut commands: Commands,
    time: Res<Time>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut lap: ResMut<Lap>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    mut me: Query<(&mut Transform, &mut Kinematic, &mut Health, Has<Riding>), With<LocalPlayer>>,
    vehicles: Query<(&Transform, &Vehicle), Without<LocalPlayer>>,
) {
    let Ok((mut tf, mut kin, mut hp, riding)) = me.single_mut() else { return };
    hp.hp = hp.max_hp;
    lap.t += time.delta_secs();
    match lap.phase {
        // After the first seconds' shader warm-up.
        0 if m.elapsed > 13.0 => {
            let a = 2.9f32;
            let at = on_ring(lap.centre, lap.radius, a);
            let dir = Vec2::new(-a.sin(), a.cos());
            let foot = Vec3::new(at.x, layout.terrain.height(at.x, at.y), at.y);
            let side = Vec3::new(-dir.y, 0.0, dir.x) * (lap.kind.spec().size.x / 2.0 + 0.9);
            lap.vehicle = Some(spawn_vehicle(&mut commands, lap.kind, foot, facing(dir)));
            stand(&mut tf, &mut kin, foot + side, facing(dir));
            info!("drive: {:?} at {at:?}", lap.kind);
            lap.phase = 1;
            lap.t = 0.0;
        }
        1 if lap.t > 0.6 => {
            kin.grounded = true;
            tap(&mut keys, KeyCode::KeyE, true);
            if riding {
                info!("drive: got in by the E key");
                hold(&mut keys, KeyCode::KeyE, false);
                lap.phase = 2;
                lap.t = 0.0;
            }
        }
        2 => {
            let Some(Ok((t, v))) = lap.vehicle.map(|e| vehicles.get(e)) else { return };
            hold(&mut keys, KeyCode::KeyW, true);
            // The last seconds: out to the wall behind the columns.
            let radius = if lap.t > lap.seconds - 7.0 { lap.radius + 30.0 } else { lap.radius };
            let centre = lap.centre;
            steer_round(&mut keys, centre, radius, t, v);
            let sound = lap.t > lap.horn;
            tap(&mut keys, KeyCode::KeyH, sound);
            if sound {
                lap.horn += 9.0;
            }
            if lap.t > lap.seconds {
                info!("drive: braking at {:.1} m/s, health {:.0}, {:.0} m from the middle", v.speed, v.hp, (t.translation.xz() - centre).length());
                for k in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyD] {
                    keys.release(k);
                }
                lap.phase = 3;
            }
        }
        3 => {
            let Some(Ok((_, v))) = lap.vehicle.map(|e| vehicles.get(e)) else { return };
            hold(&mut keys, KeyCode::KeyS, v.speed > 0.3);
            if v.speed <= 0.3 {
                tap(&mut keys, KeyCode::KeyE, true);
                lap.phase = 4;
                lap.t = 0.0;
            }
        }
        4 => {
            hold(&mut keys, KeyCode::KeyE, false);
            if !riding && lap.t < 3.0 {
                keys.press(KeyCode::KeyW);
            } else {
                keys.release(KeyCode::KeyW);
            }
            if !riding && lap.t > 0.2 && lap.t < 0.3 {
                info!("drive: got out by the E key");
            }
        }
        _ => {}
    }
}

#[derive(Resource)]
struct Road {
    kind: VehicleKind,
    seconds: f32,
    line: Vec<Vec2>,
    next: usize,
    phase: u8,
    t: f32,
    vehicle: Option<Entity>,
    frames: Vec<f32>,
    /// The worst frame, milliseconds, where it was and at what speed.
    worst: (f32, Vec2, f32),
    top: f32,
}

/// The point `metres` on along `line` from its point `from`.
fn road_ahead(line: &[Vec2], from: usize, metres: f32) -> Vec2 {
    let mut left = metres;
    for s in line[from.min(line.len() - 1)..].windows(2) {
        let len = s[0].distance(s[1]);
        if len >= left {
            return s[0].lerp(s[1], left / len.max(1e-3));
        }
        left -= len;
    }
    *line.last().unwrap_or(&Vec2::ZERO)
}

/// Down the Ring Road by the real keys, timing every frame.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn road(
    mut commands: Commands,
    time: Res<Time>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut road: ResMut<Road>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    mut me: Query<(&mut Transform, &mut Kinematic, &mut Health, Has<Riding>), With<LocalPlayer>>,
    vehicles: Query<(&Transform, &Vehicle), Without<LocalPlayer>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((mut tf, mut kin, mut hp, riding)) = me.single_mut() else { return };
    hp.hp = hp.max_hp;
    let dt = time.delta_secs();
    road.t += dt;
    match road.phase {
        // After the first seconds' shader warm-up.
        0 if m.elapsed > 13.0 => {
            let at = road.line[0];
            let dir = (road.line[1] - at).normalize();
            let foot = Vec3::new(at.x, layout.terrain.height(at.x, at.y), at.y);
            let side = Vec3::new(-dir.y, 0.0, dir.x) * (road.kind.spec().size.x / 2.0 + 0.9);
            road.vehicle = Some(spawn_vehicle(&mut commands, road.kind, foot, facing(dir)));
            stand(&mut tf, &mut kin, foot + side, facing(dir));
            road.phase = 1;
            road.t = 0.0;
        }
        1 if road.t > 0.6 => {
            kin.grounded = true;
            tap(&mut keys, KeyCode::KeyE, true);
            if riding {
                hold(&mut keys, KeyCode::KeyE, false);
                road.phase = 2;
                road.t = 0.0;
            }
        }
        2 => {
            let Some(Ok((t, v))) = road.vehicle.map(|e| vehicles.get(e)) else { return };
            let p = t.translation.xz();
            let next = road.next;
            let n = road.line.len();
            road.next = (next..(next + 6).min(n)).min_by(|a, b| road.line[*a].distance_squared(p).total_cmp(&road.line[*b].distance_squared(p))).unwrap_or(next);
            let aim = road_ahead(&road.line, road.next + 1, (v.speed * 0.5).max(6.0));
            let off = wrap(facing(aim - p) - v.heading);
            let far = road_ahead(&road.line, road.next + 1, v.speed * 2.5 + 24.0);
            let bend = wrap(facing(far - aim) - facing(aim - p)).abs();
            let slow = (bend > 0.45 || off.abs() > 0.35) && v.speed > 10.0;
            hold(&mut keys, KeyCode::KeyW, !slow);
            hold(&mut keys, KeyCode::KeyS, slow);
            hold(&mut keys, KeyCode::KeyA, off > 0.02);
            hold(&mut keys, KeyCode::KeyD, off < -0.02);
            let ms = dt * 1000.0;
            road.top = road.top.max(v.speed);
            road.frames.push(ms);
            // The first second is the vehicle's own shaders settling.
            if road.t > 1.0 && ms > road.worst.0 {
                road.worst = (ms, p, v.speed);
            }
            if road.t > road.seconds || road.next + 2 >= n {
                let (mean, p99) = frames(&road.frames);
                let (w, at, speed) = road.worst;
                info!("[road] {:?}: {:.0} s, {} frames, top {:.1} m/s, ended at {:.0},{:.0}, health {:.0}: mean {mean:.1} ms, 1 in 100 over {p99:.1} ms, worst {w:.1} ms at {:.0},{:.0} doing {speed:.1} m/s", road.kind, road.t, road.frames.len(), road.top, p.x, p.y, v.hp, at.x, at.y);
                for k in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyD] {
                    keys.release(k);
                }
                road.frames.clear();
                road.phase = 3;
            }
        }
        3 => {
            let Some(Ok((_, v))) = road.vehicle.map(|e| vehicles.get(e)) else { return };
            hold(&mut keys, KeyCode::KeyS, v.speed > 0.3);
            if v.speed <= 0.3 {
                keys.release(KeyCode::KeyS);
                road.frames.clear();
                road.t = 0.0;
                road.phase = 4;
            }
        }
        4 => {
            road.frames.push(dt * 1000.0);
            if road.t > 25.0 {
                let (mean, p99) = frames(&road.frames);
                let worst = road.frames.iter().copied().fold(0.0, f32::max);
                info!("[road] standing still after it: {} frames, mean {mean:.1} ms, 1 in 100 over {p99:.1} ms, worst {worst:.1} ms", road.frames.len());
                exit.write(AppExit::Success);
            }
        }
        _ => {}
    }
}

/// The mean of some frame times and the one that 1 in 100 are over.
fn frames(times: &[f32]) -> (f32, f32) {
    let mut f = times.to_vec();
    f.sort_by(f32::total_cmp);
    (f.iter().sum::<f32>() / f.len().max(1) as f32, f[(f.len().max(1) - 1) * 99 / 100])
}

#[derive(Resource)]
struct Fleet {
    dir: String,
    step: usize,
    t: f32,
    shot: bool,
    /// The camera's place and what it looks at; a driver it is watching instead.
    eye: Option<(Vec3, Vec3)>,
    watching: Option<Entity>,
    /// Roadside pictures still to take.
    left: u32,
}

/// Seconds at a new place before the picture, for the near tier to arrive; less for one coming down
/// the road, which won't hold its line for long.
const SETTLE: f32 = 3.0;
const ROADSIDE: f32 = 1.5;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn fleet(
    mut commands: Commands,
    time: Res<Time>,
    mut fleet: ResMut<Fleet>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    view: Res<crate::quality::View3d>,
    mut me: Query<(&mut Transform, &mut Kinematic, &mut Health), With<LocalPlayer>>,
    vehicles: Query<(Entity, &Transform, &Vehicle), Without<LocalPlayer>>,
    bots: Query<(), With<BotBrain>>,
    mut exit: MessageWriter<AppExit>,
) {
    // Once the drop's own camera has let go.
    if m.elapsed < 16.0 {
        return;
    }
    let Ok((mut tf, mut kin, mut hp)) = me.single_mut() else { return };
    hp.hp = hp.max_hp;
    fleet.t += time.delta_secs();
    if m.elapsed > 170.0 {
        info!("[fleet] out of time, {} roadside pictures short", fleet.left);
        exit.write(AppExit::Success);
        return;
    }
    let (kinds, kerb) = (VehicleKind::ALL.len(), VehicleKind::ALL.len());
    if fleet.eye.is_none() && fleet.watching.is_none() {
        fleet.t = 0.0;
        fleet.shot = false;
        if fleet.step < kinds {
            let kind = VehicleKind::ALL[fleet.step];
            let Some((_, vt, v)) = vehicles.iter().find(|(_, _, v)| v.kind == kind && v.driver.is_none()) else {
                fleet.step += 1;
                return;
            };
            let spec = v.kind.spec();
            let foot = vt.translation - Vec3::Y * spec.centre();
            let dist = (spec.size.z * 1.5).max(4.0);
            let eye = vt.translation + vt.rotation * (Quat::from_rotation_y(0.8) * Vec3::NEG_Z * dist) + Vec3::Y * 0.7;
            stand(&mut tf, &mut kin, foot + vt.rotation * Vec3::Z * (spec.size.z / 2.0 + 14.0), 0.0);
            fleet.eye = Some((eye, vt.translation));
            info!("[fleet] {} at {:.0},{:.0}", spec.name, foot.x, foot.z);
            return;
        }
        if fleet.step == kerb {
            let pair = vehicles.iter().filter(|(_, _, v)| v.kind == VehicleKind::Auto).find_map(|(_, vt, _)| layout.kit.iter().find(|k| k.name == "auto" && k.pos.xz().distance(vt.translation.xz()) < 9.0).map(|k| (vt.translation, k.pos)));
            let Some((live, kit)) = pair else {
                info!("[fleet] no drivable auto beside a tarped one in this match");
                fleet.step += 1;
                return;
            };
            let along = (live - kit).normalize_or_zero();
            let mid = (live + kit) / 2.0;
            let eye = mid + Vec3::new(-along.z, 0.0, along.x) * 7.5 + along * 2.5 + Vec3::Y * 1.2;
            stand(&mut tf, &mut kin, mid + Vec3::new(along.z, 0.0, -along.x) * 16.0 - Vec3::Y * 0.9, 0.0);
            fleet.eye = Some((eye, mid));
            return;
        }
        // Bots at the wheel: from the road ahead of one, as it comes.
        let driven = vehicles.iter().find(|(_, _, v)| v.speed > 7.0 && v.driver.is_some_and(|d| bots.contains(d)));
        let Some((e, vt, v)) = driven else { return };
        let dir = vt.rotation * Vec3::NEG_Z;
        let eye = vt.translation + dir * v.speed * ROADSIDE * 1.5 + Vec3::new(-dir.z, 0.0, dir.x) * 4.5 + Vec3::Y * 0.6;
        stand(&mut tf, &mut kin, eye.with_y(vt.translation.y - v.kind.spec().centre()) + dir * 6.0, 0.0);
        fleet.eye = Some((eye, vt.translation));
        fleet.watching = Some(e);
        info!("[fleet] a bot driving a {} at {:.1} m/s, at {:.0},{:.0}", v.kind.spec().name, v.speed, vt.translation.x, vt.translation.z);
        return;
    }
    if let Some(Ok((_, vt, _))) = fleet.watching.map(|e| vehicles.get(e)) {
        let look = vt.translation;
        if let Some(e) = fleet.eye.as_mut() {
            e.1 = look;
        }
    }
    let wait = if fleet.watching.is_some() { ROADSIDE } else { SETTLE };
    if fleet.t >= wait && !fleet.shot {
        fleet.shot = true;
        let name = if fleet.step < kinds {
            format!("{:02}_{}", fleet.step + 1, VehicleKind::ALL[fleet.step].spec().name.to_lowercase().replace(' ', "_"))
        } else if fleet.step == kerb {
            "10_kit_auto_under_tarp_and_a_drivable_one".to_string()
        } else {
            format!("2{}_bot_at_the_wheel", fleet.left)
        };
        let path = format!("{}/{name}.png", fleet.dir);
        commands.spawn(Screenshot::image(view.image.clone())).observe(save_to_disk(path));
    }
    if fleet.t >= wait + 0.4 {
        fleet.eye = None;
        fleet.watching = None;
        if fleet.step <= kerb {
            fleet.step += 1;
        } else {
            fleet.left -= 1;
            if fleet.left == 0 {
                exit.write(AppExit::Success);
            }
        }
    }
}

fn fleet_camera(fleet: Res<Fleet>, mut cam: Query<&mut Transform, With<MainCamera>>) {
    if let (Some((eye, look)), Ok(mut ct)) = (fleet.eye, cam.single_mut()) {
        *ct = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    }
}

#[derive(Clone)]
struct Take {
    name: String,
    kind: VehicleKind,
    /// The player at the wheel, and driving; `corner`: hard right for the last moments.
    aboard: bool,
    driving: bool,
    corner: bool,
    /// Seen from this angle, radians round from the nose; none: the game's own camera.
    view: Option<f32>,
    /// Its health, as a share of full, from a moment in.
    hp: f32,
    /// Seconds in that it is taken.
    at: f32,
}

fn takes() -> Vec<Take> {
    let mut v = vec![];
    let word = |k: VehicleKind| k.spec().name.to_lowercase().replace(' ', "_");
    let parked = |name: String, kind, view, hp, at| Take { name, kind, aboard: false, driving: false, corner: false, view: Some(view), hp, at };
    let driven = |name: String, kind, hp, at| Take { name, kind, aboard: true, driving: true, corner: false, view: None, hp, at };
    for (i, k) in VehicleKind::ALL.into_iter().enumerate() {
        v.push(parked(format!("{:02}_{}", i + 1, word(k)), k, 0.75, 1.0, 2.5));
    }
    for (i, k) in VehicleKind::ALL.into_iter().enumerate() {
        v.push(driven(format!("{:02}_{}_driven", i + 11, word(k)), k, 1.0, 5.0));
    }
    for (i, k) in VehicleKind::ALL.into_iter().enumerate() {
        v.push(Take { aboard: true, ..parked(format!("{:02}_{}_driver", i + 21, word(k)), k, 1.75, 1.0, 2.5) });
    }
    for (i, k) in [VehicleKind::Scooter, VehicleKind::Commuter, VehicleKind::Bullet].into_iter().enumerate() {
        v.push(Take { corner: true, ..driven(format!("{:02}_{}_lean", i + 41, word(k)), k, 1.0, 5.2) });
    }
    let auto = VehicleKind::Auto;
    v.push(Take { aboard: false, driving: false, ..driven("30_get_in_prompt".into(), auto, 1.0, 2.0) });
    v.push(driven("31_health_full".into(), auto, 1.0, 5.0));
    v.push(driven("32_health_half".into(), auto, 0.5, 6.0));
    v.push(driven("33_health_low".into(), auto, 0.2, 6.0));
    v.push(parked("34_smoke_grey".into(), auto, 2.4, 0.5, 5.0));
    v.push(parked("35_smoke_black".into(), VehicleKind::Car, 0.6, 0.2, 5.0));
    v.push(parked("36_wreck_blast".into(), auto, 0.75, 0.0, 1.75));
    v.push(parked("37_wreck_burning".into(), auto, 0.75, 0.0, 6.0));
    v.push(parked("38_wreck_shell".into(), auto, 0.75, 0.0, 19.0));
    v
}

#[derive(Resource)]
struct Show {
    dir: String,
    takes: Vec<Take>,
    index: usize,
    t: f32,
    vehicle: Option<Entity>,
    shot: bool,
    centre: Vec2,
    radius: f32,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    time: Res<Time>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut show: ResMut<Show>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    view: Res<crate::quality::View3d>,
    mut me: Query<(&mut Transform, &mut Kinematic, &mut Health, Has<Riding>), With<LocalPlayer>>,
    mut vehicles: Query<(&Transform, &mut Vehicle), Without<LocalPlayer>>,
    mut exit: MessageWriter<AppExit>,
) {
    if m.elapsed < 3.0 {
        return;
    }
    let Ok((mut tf, mut kin, mut hp, riding)) = me.single_mut() else { return };
    hp.hp = hp.max_hp;
    let Some(take) = show.takes.get(show.index).cloned() else {
        exit.write(AppExit::Success);
        return;
    };
    // In the sun.
    let a = 2.9f32;
    let at = on_ring(show.centre, show.radius, a);
    let dir = Vec2::new(-a.sin(), a.cos());
    let foot = Vec3::new(at.x, layout.terrain.height(at.x, at.y), at.y);
    if show.t == 0.0 {
        if let Some(e) = show.vehicle.take() {
            commands.entity(e).despawn();
        }
        for k in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyD, KeyCode::KeyE] {
            keys.release(k);
        }
        show.vehicle = Some(spawn_vehicle(&mut commands, take.kind, foot, facing(dir)));
        let side = Vec3::new(-dir.y, 0.0, dir.x) * (take.kind.spec().size.x / 2.0 + 0.9);
        // Out of the picture for a parked one.
        stand(&mut tf, &mut kin, foot + side * if take.aboard || take.view.is_none() { 1.0 } else { 12.0 }, facing(dir));
    }
    show.t += time.delta_secs().max(1e-4);
    let t = show.t;
    if let Some(Ok((vt, mut v))) = show.vehicle.map(|e| vehicles.get_mut(e)) {
        // A moment first, for leaving the last take's vehicle to settle.
        if take.aboard && t > 0.6 && !riding {
            kin.grounded = true;
            tap(&mut keys, KeyCode::KeyE, true);
        }
        if riding && t > 0.6 {
            keys.release(KeyCode::KeyE);
            if take.driving {
                hold(&mut keys, KeyCode::KeyW, true);
                if take.corner && t > take.at - 0.7 {
                    keys.release(KeyCode::KeyA);
                    keys.press(KeyCode::KeyD);
                } else {
                    let (centre, radius) = (show.centre, show.radius);
                    steer_round(&mut keys, centre, radius, vt, &v);
                }
            }
        }
        if t > 1.5 && take.hp < 1.0 && !v.wrecked {
            v.hp = v.kind.spec().hp * take.hp;
        }
    }
    if t >= take.at && !show.shot {
        show.shot = true;
        let path = format!("{}/{}.png", show.dir, take.name);
        let shot = if std::env::var("VIVAD_SHOW_WINDOW").is_ok() { Screenshot::primary_window() } else { Screenshot::image(view.image.clone()) };
        commands.spawn(shot).observe(save_to_disk(path));
    }
    if t >= take.at + 0.4 {
        show.index += 1;
        show.t = 0.0;
        show.shot = false;
    }
}

/// A parked take's view: three quarters on, at eye height.
fn show_camera(show: Res<Show>, mut cam: Query<&mut Transform, With<MainCamera>>, vehicles: Query<(&Transform, &Vehicle), Without<MainCamera>>) {
    let Some(take) = show.takes.get(show.index) else { return };
    let (Some(az), Some(Ok((vt, v))), Ok(mut ct)) = (take.view, show.vehicle.map(|e| vehicles.get(e)), cam.single_mut()) else { return };
    let spec = v.kind.spec();
    let dist = (spec.size.z * 1.15).max(2.7);
    let eye = vt.translation + vt.rotation * (Quat::from_rotation_y(az) * Vec3::NEG_Z * dist) + Vec3::Y * 0.5;
    *ct = Transform::from_translation(eye).looking_at(vt.translation + Vec3::Y * 0.2, Vec3::Y);
}
