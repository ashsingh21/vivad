//! Vehicles against the real map, headless: where they park, get in and out, a wall stops one, it
//! slides along a wall it meets at an angle and never goes through, ramming hurts both ways, a wreck
//! throws its driver out, water slows one to a wade, bots hear an engine, a match's end clears them.
//! On the NCR also, driven by the same inputs as a player: the ground's prices, the rivers, all eight
//! bridges both ways, a lap of the Ring Road and the passes over the Ridge, by car and by bike.
//!
//! VIVAD_MAP=cp cargo run --release -p shared --bin vehicle_check (the default map is the NCR)

use std::time::Duration;

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::items::{ItemStack, Rarity};
use shared::map::terrain::ground;
use shared::map::{MapLayout, MapStyle, Mat, compose, footprint};
use shared::sim::bots::BotBrain;
use shared::sim::combat::Projectile;
use shared::sim::flow::{Match, Rules, leave_match};
use shared::sim::senses::ENGINE_NOISE;
use shared::sim::vehicle::{Hitches, Riding, Vehicle, VehicleKind, spawn_vehicle};
use shared::sim::movement::SPRINT_SPEED;
use shared::sim::{Action, DamageCause, DamageMsg, Eliminated, HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MatchConfig, MoveMode, PlayerIntent, SimPlugin, Warrior, btn, solid_mask};

fn app() -> App {
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    let mut app = App::new();
    app.insert_resource(MatchConfig { bots: 3, match_seed: Some(7), ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    for _ in 0..40 {
        app.update();
    }
    app
}

/// The player, marked as the local one: no bot's brain, on foot.
fn player(app: &mut App) -> Entity {
    let world = app.world_mut();
    let e = world.query_filtered::<Entity, With<Warrior>>().iter(world).next().unwrap();
    world.entity_mut(e).remove::<shared::sim::bots::BotBrain>().insert(LocalPlayer);
    e
}

fn put(app: &mut App, e: Entity, at: Vec3) {
    let mut w = app.world_mut().entity_mut(e);
    w.get_mut::<Transform>().unwrap().translation = at + Vec3::Y * (HALF_HEIGHT + 0.02);
    let mut k = w.get_mut::<Kinematic>().unwrap();
    k.mode = MoveMode::Ground;
    k.velocity = Vec3::ZERO;
    k.grounded = true;
    k.ground_normal = Vec3::Y;
}

fn press(app: &mut App, e: Entity) {
    app.world_mut().get_mut::<PlayerIntent>(e).unwrap().pressed |= btn::INTERACT;
    app.update();
    app.update();
}

fn spawn(app: &mut App, kind: VehicleKind, at: Vec3, yaw: f32) -> Entity {
    let e = app.world_mut().run_system_once(move |mut c: Commands| spawn_vehicle(&mut c, kind, at, yaw)).unwrap();
    app.update();
    app.update();
    e
}

/// A long straight wall of a real building with open road in front of it: (a point on the wall, its
/// outward normal, the wall's direction).
fn wall(app: &mut App) -> (Vec3, Vec2, Vec2) {
    app.world_mut()
        .run_system_once(|spatial: SpatialQuery, layout: Res<MapLayout>| {
            let filter = SpatialQueryFilter::from_mask(solid_mask());
            let mut best = None;
            let mut best_len = 0.0;
            for f in layout.footprints.iter().filter(|f| f.mat != Mat::Water) {
                for i in 0..f.pts.len() {
                    let (a, b) = (f.pts[i], f.pts[(i + 1) % f.pts.len()]);
                    let len = a.distance(b);
                    if len < 18.0 || len < best_len {
                        continue;
                    }
                    let along = (b - a) / len;
                    // Counter-clockwise outline: outward is to the right of each edge.
                    let out = Vec2::new(along.y, -along.x);
                    let m = (a + b) / 2.0;
                    let ground = layout.terrain.height(m.x, m.y);
                    let clear = |p: Vec2, d: Dir3, reach: f32| spatial.cast_ray(Vec3::new(p.x, ground + 0.7, p.y), d, reach, true, &filter).is_none();
                    let from = m + out * 26.0;
                    let toward = Dir3::new(Vec3::new(-out.x, 0.0, -out.y)).unwrap();
                    let side = Dir3::new(Vec3::new(along.x, 0.0, along.y)).unwrap();
                    let reversed = Dir3::new(Vec3::new(-along.x, 0.0, -along.y)).unwrap();
                    let hit = spatial.cast_ray(Vec3::new(from.x, ground + 0.7, from.y), toward, 30.0, true, &filter);
                    if hit.is_some_and(|h| (h.distance - 26.0).abs() < 0.6) && clear(m + out * 3.0, side, len / 2.0 + 8.0) && clear(m + out * 3.0, reversed, len / 2.0 + 8.0) {
                        best = Some((Vec3::new(m.x, ground, m.y), out, along));
                        best_len = len;
                    }
                }
            }
            println!("wall: {best_len:.1} m long");
            best.expect("a clear wall on this map")
        })
        .unwrap()
}

fn yaw_along(d: Vec2) -> f32 {
    (-d.x).atan2(-d.y)
}

/// Land a short run from open water, and the way in: the banks nearest the map's middle first.
fn water_edge(layout: &MapLayout) -> Option<(Vec3, Vec2)> {
    const RUN: f32 = 30.0;
    let (t, reach) = (&layout.terrain, layout.half_extent * 0.9);
    let water = |p: Vec2| t.ground_at(p.x, p.y) == Some(ground::WATER);
    let mut found: Vec<(Vec2, Vec2)> = vec![];
    let cells = (2.0 * reach / 16.0) as i32;
    for (i, j) in (0..cells).flat_map(|i| (0..cells).map(move |j| (i, j))) {
        let wet = Vec2::new(i as f32, j as f32) * 16.0 - Vec2::splat(reach);
        // Wide water, as a stream a cell across is stepped over.
        if !water(wet) || ![Vec2::X, Vec2::Y].iter().all(|d| water(wet + *d * 24.0) && water(wet - *d * 24.0)) {
            continue;
        }
        for k in 0..16 {
            let d = Vec2::from_angle(k as f32 / 16.0 * std::f32::consts::TAU);
            let dry = wet + d * RUN;
            if !water(dry) && (1..6).all(|n| !water(dry + d * n as f32 * 3.0)) {
                found.push((dry, -d));
            }
        }
    }
    found.sort_by(|a, b| a.0.length().total_cmp(&b.0.length()));
    found.first().map(|(p, d)| (Vec3::new(p.x, t.height(p.x, p.y), p.y), *d))
}

/// How a drive went.
struct Run {
    done: bool,
    seconds: f32,
    top: f32,
    /// The slowest it went once under way, and its mean.
    low: f32,
    mean: f32,
    /// Health lost, and where it stopped.
    lost: f32,
    end: Vec2,
    /// The highest and lowest it rode, and the steepest it was, in degrees.
    high: f32,
    deep: f32,
    steep: f32,
}

impl Run {
    fn line(&self) -> String {
        format!("{:.0} s, top {:.1} m/s, mean {:.1}, slowest {:.1}, health lost {:.0}, rode {:.1} to {:.1} m, steepest {:.1} degrees, ended at {:.0},{:.0}", self.seconds, self.top, self.mean, self.low, self.lost, self.deep, self.high, self.steep, self.end.x, self.end.y)
    }
}

/// The point `metres` on along `line` from its point `from`.
fn along_line(line: &[Vec2], from: usize, metres: f32) -> Vec2 {
    let mut left = metres;
    for s in line[from.min(line.len() - 1)..].windows(2) {
        let len = s[0].distance(s[1]);
        if len >= left {
            return s[0].lerp(s[1], left / len.max(1e-3));
        }
        left -= len;
    }
    *line.last().unwrap()
}

/// `kind` driven the length of `line` by the player's own controls: it steers at a point ahead and
/// brakes for what bends are coming, as a player would, and gives up if it stops for 5 s.
fn pilot(app: &mut App, me: Entity, kind: VehicleKind, line: &[Vec2], seconds: f32) -> Run {
    let spec = kind.spec();
    // A point every few metres, so that the nearest of them is where it has got to.
    let total: f32 = line.windows(2).map(|s| s[0].distance(s[1])).sum();
    let dense: Vec<Vec2> = (0..=(total / 4.0) as usize).map(|k| along_line(line, 0, k as f32 * 4.0)).chain([*line.last().unwrap()]).collect();
    let line = &dense[..];
    let height = |app: &App, p: Vec2| app.world().resource::<MapLayout>().terrain.height(p.x, p.y);
    let foot = Vec3::new(line[0].x, height(app, line[0]), line[0].y);
    let d = (line[1] - line[0]).normalize();
    let v = spawn(app, kind, foot, yaw_along(d));
    put(app, me, foot + Vec3::new(d.y, 0.0, -d.x) * (spec.size.x / 2.0 + 0.9));
    app.update();
    app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
    press(app, me);
    assert!(app.world().get::<Riding>(me).is_some(), "{kind:?} at {:?}: not in", line[0]);
    let mut run = Run { done: false, seconds: 0.0, top: 0.0, low: f32::MAX, mean: 0.0, lost: 0.0, end: line[0], high: f32::MIN, deep: f32::MAX, steep: 0.0 };
    let (mut next, mut moved, mut still, mut ticks) = (0usize, foot, 0.0f32, 0u32);
    let last = *line.last().unwrap();
    for _ in 0..(seconds * 60.0) as u32 {
        let t = app.world().get::<Transform>(v).unwrap().translation;
        let vehicle = app.world().get::<Vehicle>(v).unwrap();
        let (speed, heading, pitch) = (vehicle.speed, vehicle.heading, vehicle.pitch);
        run.lost = spec.hp - vehicle.hp;
        let p = t.xz();
        run.end = p;
        run.high = run.high.max(t.y - spec.centre());
        run.deep = run.deep.min(t.y - spec.centre());
        run.steep = run.steep.max(pitch.abs().to_degrees());
        run.top = run.top.max(speed);
        ticks += 1;
        run.mean += speed;
        if ticks > 300 {
            run.low = run.low.min(speed);
        }
        // A closed road ends where it began.
        if p.distance(last) < 9.0 && next + 8 > line.len() {
            run.done = true;
            break;
        }
        // Where it has got to along the line, then a point ahead of that.
        let look = (next..(next + 24).min(line.len())).min_by(|a, b| line[*a].distance_squared(p).total_cmp(&line[*b].distance_squared(p)));
        next = look.unwrap_or(next);
        let aim = along_line(line, next, (speed * 0.9).max(10.0));
        let off = wrap(yaw_along(aim - p) - heading);
        let far = along_line(line, next, speed * 2.5 + 24.0);
        let bend = wrap(yaw_along(far - aim) - yaw_along(aim - p)).abs();
        let slow = (bend > 0.45 || off.abs() > 0.35) && speed > 10.0;
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::new((-off * 3.0).clamp(-1.0, 1.0), if slow { -1.0 } else { 1.0 });
        app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
        app.update();
        run.seconds += 1.0 / 60.0;
        if t.distance(moved) > 1.0 {
            moved = t;
            still = 0.0;
        } else {
            still += 1.0 / 60.0;
            if still > 5.0 {
                break;
            }
        }
    }
    run.mean /= ticks.max(1) as f32;
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
    press(app, me);
    app.world_mut().entity_mut(v).despawn();
    app.update();
    run
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// A straight stretch of one ground class, long enough to reach what it allows.
fn stretch(layout: &MapLayout, class: u8, len: f32) -> Option<Vec<Vec2>> {
    let t = &layout.terrain;
    let reach = layout.half_extent - 60.0 - len;
    let mut x = -reach;
    while x < reach {
        let mut z = -reach;
        while z < reach {
            let a = Vec2::new(x, z);
            for d in [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y] {
                let clear = (0..=(len / 4.0) as i32).all(|k| {
                    let p = a + d * k as f32 * 4.0;
                    (-3..=3).all(|side| {
                        let q = p + d.perp() * side as f32 * 2.0;
                        t.ground_at(q.x, q.y) == Some(class) && !compose::built(layout, q) && !layout.props.iter().any(|o| o.pos.xz().distance(q) < 3.0)
                    })
                });
                if clear && (t.height(a.x, a.y) - t.height(a.x + d.x * len, a.y + d.y * len)).abs() < 2.0 {
                    return Some(vec![a, a + d * len]);
                }
            }
            z += 37.0;
        }
        x += 37.0;
    }
    None
}

/// Where each road crosses the Ridge: the stretch of road either side, the climbiest first.
fn ridge_passes(layout: &MapLayout) -> Vec<Vec<Vec2>> {
    let ridge: Vec<Vec2> = compose::RIDGE.iter().map(|p| Vec2::new(p.0, p.1)).collect();
    let cross = |a: Vec2, b: Vec2, c: Vec2, d: Vec2| {
        let (u, v) = (b - a, d - c);
        let den = u.perp_dot(v);
        let (t, s) = ((c - a).perp_dot(v) / den, (c - a).perp_dot(u) / den);
        den.abs() > 1e-6 && (0.0..1.0).contains(&t) && (0.0..1.0).contains(&s)
    };
    let mut found: Vec<(f32, Vec<Vec2>)> = vec![];
    for (_, line) in compose::network() {
        for k in 1..line.len() {
            if !ridge.windows(2).any(|r| cross(line[k - 1], line[k], r[0], r[1])) {
                continue;
            }
            let part: Vec<Vec2> = line[k.saturating_sub(16)..(k + 16).min(line.len())].to_vec();
            let (lo, hi) = part.iter().map(|p| layout.terrain.height(p.x, p.y)).fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(h), b.max(h)));
            if part.len() > 8 && found.iter().all(|f| f.1[f.1.len() / 2].distance(line[k]) > 250.0) {
                found.push((hi - lo, part));
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("  the Ridge: {} road crossings, rising {:?} m", found.len(), found.iter().map(|f| f.0.round()).collect::<Vec<_>>());
    found.into_iter().map(|f| f.1).collect()
}

fn main() {
    let mut app = app();
    let mut fails = 0;
    let mut check = |name: &str, ok: bool, detail: String| {
        println!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" });
        fails += !ok as u32;
    };
    {
        let world = app.world_mut();
        let parked: Vec<(VehicleKind, Vec3)> = world.query::<(&Vehicle, &Transform)>().iter(world).map(|(v, t)| (v.kind, t.translation)).collect();
        let by = |k| parked.iter().filter(|p| p.0 == k).count();
        let layout = world.resource::<MapLayout>();
        let want = Rules::of(layout.style).parked;
        let fleet = VehicleKind::ALL.map(|k| format!("{} {}", by(k), k.spec().name)).join(", ");
        check("parked at match start", parked.len() * 4 >= want * 3, format!("{} of {want} vehicles: {fleet}", parked.len()));
        if layout.style == MapStyle::Ncr {
            check("the whole fleet is out", parked.len() == want, format!("{} of {want}, {} bays on the map", parked.len(), layout.parking.len()));
            // Jeeps and tractors in yards only.
            let at_yard = |p: &Vec3| layout.parking.iter().any(|b| b.yard && b.pos.xz().distance(p.xz()) < 0.5);
            let strays = parked.iter().filter(|p| p.0.rough() && !at_yard(&p.1)).count();
            check("Khuli Gaadi and tractors at yards only", strays == 0 && by(VehicleKind::Khuli) == 6 && by(VehicleKind::Tractor) == 2, format!("{strays} out of a yard"));
            // From anywhere dry: how far to the nearest one, at a sprint.
            let mut far: Vec<f32> = vec![];
            for i in -47..=47 {
                for j in -47..=47 {
                    let p = Vec2::new(i as f32, j as f32) * 40.0;
                    if layout.terrain.ground_at(p.x, p.y) != Some(ground::WATER) {
                        far.push(parked.iter().map(|v| v.1.xz().distance(p)).fold(f32::MAX, f32::min));
                    }
                }
            }
            far.sort_by(f32::total_cmp);
            let at = |q: f32| far[((far.len() - 1) as f32 * q) as usize];
            check("a vehicle within about a 30 s run", at(0.5) / SPRINT_SPEED < 30.0 && at(0.9) / SPRINT_SPEED < 50.0, format!("half of the dry map within {:.0} m ({:.0} s at a sprint), nine in ten within {:.0} m ({:.0} s), the farthest spot {:.0} m", at(0.5), at(0.5) / SPRINT_SPEED, at(0.9), at(0.9) / SPRINT_SPEED, at(1.0)));
        }
        // Roads and hard ground, none in the water or on a lawn.
        let class = |p: &Vec3| layout.terrain.ground_at(p.x, p.z);
        let on = |c| parked.iter().filter(|p| class(&p.1) == Some(c)).count();
        let soft = on(ground::WATER) + on(ground::PARK);
        let walled = parked.iter().filter(|p| layout.footprints.iter().any(|f| f.mat != Mat::Water && footprint::contains(&f.pts, p.1.xz()))).count();
        let nearest: Vec<f32> = parked.iter().map(|a| parked.iter().filter(|b| b.1 != a.1).map(|b| a.1.xz().distance(b.1.xz())).fold(f32::MAX, f32::min)).collect();
        check("parked in sensible places", soft == 0 && walled == 0, format!("road {}, pavement {}, parking {}, plaza {}, path {}, open land {}, sand {}, park or water {soft}, in a building {walled}; nearest neighbour {:.0} m on average", on(ground::ROAD), on(ground::PAVEMENT), on(ground::PARKING), on(ground::PLAZA), on(ground::PATH), on(ground::OPEN), on(ground::SAND), nearest.iter().sum::<f32>() / nearest.len().max(1) as f32));
    }
    let me = player(&mut app);
    let (point, out, along) = wall(&mut app);

    for kind in VehicleKind::ALL {
        let spec = kind.spec();
        let half = spec.size.z / 2.0;
        // Each crash costs the driver something: every kind starts whole.
        app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
        // Head on, flat out.
        let start = point + Vec3::new(out.x, 0.0, out.y) * 24.0;
        let v = spawn(&mut app, kind, start, yaw_along(-out));
        put(&mut app, me, start + Vec3::new(out.y, 0.0, -out.x) * (spec.size.x / 2.0 + 0.9));
        app.update();
        press(&mut app, me);
        let riding = app.world().get::<Riding>(me).is_some();
        let k = app.world().get::<Kinematic>(me).unwrap();
        check(
            &format!("{kind:?} get in"),
            riding,
            format!("mode {:?}, grounded {}, health {:.0}, down {}, at match time {:.0} s", k.mode, k.grounded, app.world().get::<Health>(me).unwrap().hp, app.world().get::<Eliminated>(me).is_some(), app.world().resource::<Match>().elapsed),
        );
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
        let (mut peak, mut closest) = (0.0f32, f32::MAX);
        for _ in 0..60 * 7 {
            app.update();
            let t = app.world().get::<Transform>(v).unwrap().translation;
            peak = peak.max(app.world().get::<Vehicle>(v).unwrap().speed);
            closest = closest.min((t - point).xz().dot(out));
        }
        let vehicle = app.world().get::<Vehicle>(v).unwrap();
        let (speed, hp) = (vehicle.speed, vehicle.hp);
        check(&format!("{kind:?} head on"), closest > half - 0.05 && closest < half + 0.8 && speed.abs() < 2.0, format!("peak {peak:.1} m/s, nose {:.2} m off the wall, then {speed:.1} m/s, health {hp:.0}/{:.0}", closest - half, spec.hp));
        check(&format!("{kind:?} crash hurts"), hp < spec.hp, format!("lost {:.0}", spec.hp - hp));
        // Back off, and turn away: at 25 degrees to the wall, flat out.
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
        press(&mut app, me);
        check(&format!("{kind:?} get out"), app.world().get::<Riding>(me).is_none() && app.world().get::<Kinematic>(me).unwrap().mode == MoveMode::Ground, format!("{:.1} m from it", app.world().get::<Transform>(me).unwrap().translation.distance(app.world().get::<Transform>(v).unwrap().translation)));
        app.world_mut().entity_mut(v).despawn();
        app.update();

        let slant = (along * 25f32.to_radians().cos() - out * 25f32.to_radians().sin()).normalize();
        let start = point + Vec3::new(out.x, 0.0, out.y) * 7.0 - Vec3::new(along.x, 0.0, along.y) * 8.0;
        let v = spawn(&mut app, kind, start, yaw_along(slant));
        put(&mut app, me, start + Vec3::new(out.y, 0.0, -out.x) * (spec.size.x / 2.0 + 0.9));
        app.update();
        press(&mut app, me);
        app.world_mut().get_mut::<Vehicle>(v).unwrap().speed = spec.law.top * 0.8;
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
        let (mut touch, mut slid, mut closest, mut inside) = (None, 0.0f32, f32::MAX, 0);
        for _ in 0..150 {
            app.update();
            let tf = *app.world().get::<Transform>(v).unwrap();
            let t = tf.translation;
            let off = (t - point).xz().dot(out);
            closest = closest.min(off);
            let run = (t - point).xz().dot(along);
            if off < spec.size.x / 2.0 + 0.3 && touch.is_none() {
                touch = Some(run);
            }
            if let Some(from) = touch {
                slid = slid.max(run - from);
            }
            // The box shrunk by a hair: touching is fine, being inside is not.
            let body = VehicleKind::spec(kind);
            let shape = Collider::round_cuboid(body.size.x - 0.5, body.size.y - 0.5, body.size.z - 0.5, 0.05);
            inside += app.world_mut().run_system_once(move |spatial: SpatialQuery| spatial.shape_intersections(&shape, t, tf.rotation, &SpatialQueryFilter::from_mask(solid_mask())).len()).unwrap();
        }
        let vehicle = app.world().get::<Vehicle>(v).unwrap();
        let slide = if kind == VehicleKind::Tractor { 2.0 } else { 6.0 };
        check(&format!("{kind:?} slides along"), slid > slide && inside == 0, format!("{slid:.1} m along the wall, centre {closest:.2} m off it at the nearest, {inside} ticks inside it, still {:.1} m/s, health {:.0}/{:.0}", vehicle.speed, vehicle.hp, spec.hp));
        press(&mut app, me);
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
        app.world_mut().entity_mut(v).despawn();
        app.update();
    }

    // Parked flush along a wall, with the wall on either side: a solid building's collider is only
    // its surface, so a spot past the wall looks free to a capsule.
    app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
    let street = if compose::built(app.world().resource::<MapLayout>(), point.xz() + out * 3.0) { -out } else { out };
    let street = Vec3::new(street.x, 0.0, street.y);
    for (side, way) in [("one", along), ("other", -along)] {
        let spec = VehicleKind::Car.spec();
        let at = point + street * (spec.size.x / 2.0 + 0.12);
        let v = spawn(&mut app, VehicleKind::Car, at, yaw_along(way));
        put(&mut app, me, at + street * (spec.size.x / 2.0 + 0.9));
        app.update();
        press(&mut app, me);
        let rode = app.world().get::<Riding>(me).is_some();
        press(&mut app, me);
        let feet = app.world().get::<Transform>(me).unwrap().translation - Vec3::Y * HALF_HEIGHT;
        let walled = compose::in_building(app.world().resource::<MapLayout>(), feet);
        check(&format!("out of a car with a wall on its {side} side"), rode && !walled, format!("{:.2} m out from the wall, inside the building {walled}", (feet - point).dot(street)));
        app.world_mut().entity_mut(v).despawn();
        app.update();
    }

    // Rules: no shooting at the wheel, and bots stay out of the driving seat.
    let start = point + Vec3::new(out.x, 0.0, out.y) * 24.0;
    let v = spawn(&mut app, VehicleKind::Auto, start, yaw_along(along));
    put(&mut app, me, start + Vec3::new(out.y, 0.0, -out.x) * 1.5);
    {
        let world = app.world_mut();
        let mut inv = world.get_mut::<Inventory>(me).unwrap();
        inv.slots[0] = Some(ItemStack { id: "tamancha".into(), rarity: Rarity::Common, count: 6 });
        inv.selected = 1;
        world.get_mut::<Action>(me).unwrap().cooldown = 0.0;
        world.get_mut::<PlayerIntent>(me).unwrap().held = btn::FIRE;
    }
    for _ in 0..30 {
        app.update();
    }
    let shots_on_foot = app.world_mut().query::<&Projectile>().iter(app.world()).count();
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().held = 0;
    for _ in 0..30 {
        app.update();
    }
    put(&mut app, me, start + Vec3::new(out.y, 0.0, -out.x) * 1.5);
    app.update();
    press(&mut app, me);
    let before = app.world().get::<Inventory>(me).unwrap().slots[0].as_ref().map_or(0, |s| s.count);
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().held = btn::FIRE;
    for _ in 0..60 {
        app.update();
    }
    let after = app.world().get::<Inventory>(me).unwrap().slots[0].as_ref().map_or(0, |s| s.count);
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().held = 0;
    check("no shooting at the wheel", shots_on_foot > 0 && app.world().get::<Riding>(me).is_some() && before == after, format!("{shots_on_foot} shots on foot, none of {before} rounds at the wheel"));
    press(&mut app, me);
    app.world_mut().entity_mut(v).despawn();
    app.update();
    let v = spawn(&mut app, VehicleKind::Auto, start, yaw_along(along));
    let bot_brained = {
        let world = app.world_mut();
        world.query_filtered::<Entity, (With<shared::sim::bots::BotBrain>, Without<Eliminated>)>().iter(world).next().unwrap()
    };
    put(&mut app, bot_brained, start + Vec3::new(out.y, 0.0, -out.x) * 1.5);
    app.update();
    press(&mut app, bot_brained);
    check("bots use no key to drive", app.world().get::<Riding>(bot_brained).is_none() && app.world().get::<Vehicle>(v).unwrap().driver.is_none(), "a bot pressing the key beside an auto stays on foot".into());
    app.world_mut().entity_mut(v).despawn();
    app.update();

    // Ramming a warrior.
    let bot = {
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, (With<Warrior>, Without<LocalPlayer>)>().iter(world).next().unwrap();
        world.entity_mut(e).remove::<shared::sim::bots::BotBrain>();
        e
    };
    let v = spawn(&mut app, VehicleKind::Commuter, start, yaw_along(along));
    put(&mut app, me, start + Vec3::new(out.y, 0.0, -out.x) * 1.2);
    app.update();
    press(&mut app, me);
    app.world_mut().get_mut::<Vehicle>(v).unwrap().speed = 14.0;
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
    let mut ahead = start + Vec3::new(along.x, 0.0, along.y) * 12.0;
    ahead.y = app.world().resource::<MapLayout>().terrain.height(ahead.x, ahead.z);
    println!("  ground under the warrior {:.2}, at the wall {:.2}", ahead.y, point.y);
    put(&mut app, bot, ahead);
    println!("  rammed warrior: {:?}, shield {:.0}, down {}", app.world().get::<Kinematic>(bot).unwrap().mode, app.world().get::<Health>(bot).unwrap().shield, app.world().get::<Eliminated>(bot).is_some());
    app.world_mut().get_mut::<Health>(bot).unwrap().shield = 0.0;
    *app.world_mut().get_mut::<PlayerIntent>(bot).unwrap() = PlayerIntent::default();
    let hp0 = app.world().get::<Health>(bot).unwrap().hp;
    let mut flung = 0.0f32;
    for i in 0..90 {
        app.update();
        let speed = app.world().get::<Kinematic>(bot).unwrap().velocity.length();
        if speed > flung + 2.0 {
            println!("  tick {i}: thrown at {speed:.1} m/s, health {:.0}, vehicle at {:.1} m/s, {:.1} m from it", app.world().get::<Health>(bot).unwrap().hp, app.world().get::<Vehicle>(v).unwrap().speed, app.world().get::<Transform>(bot).unwrap().translation.distance(app.world().get::<Transform>(v).unwrap().translation));
        }
        flung = flung.max(speed);
    }
    let hp1 = app.world().get::<Health>(bot).unwrap().hp;
    check("ram hurts and throws", hp1 < hp0 && flung > 5.0, format!("health {hp0:.0} to {hp1:.0}, thrown at {flung:.1} m/s, {}", if app.world().get::<Eliminated>(bot).is_some() { "dead" } else { "alive" }));

    // Weapon damage, down to a wreck, the driver thrown clear.
    for amount in [40.0, 500.0] {
        app.world_mut().write_message(DamageMsg { target: v, amount, source: Some(bot), headshot: false, cause: DamageCause::Weapon("tamancha".into()) });
        app.update();
        app.update();
        let vehicle = app.world().get::<Vehicle>(v).unwrap();
        println!("  after {amount} damage: health {:.0}, wrecked {}", vehicle.hp, vehicle.wrecked);
    }
    app.update();
    let wrecked = app.world().get::<Vehicle>(v).unwrap().wrecked;
    let (me_mode, out_of_it) = (app.world().get::<Kinematic>(me).unwrap().mode, app.world().get::<Riding>(me).is_none());
    check("a wreck throws its driver out", wrecked && out_of_it && me_mode == MoveMode::Ground, format!("wrecked {wrecked}, player {me_mode:?}"));
    // No getting back in.
    let at = app.world().get::<Transform>(v).unwrap().translation;
    let foot = at - Vec3::Y * VehicleKind::Commuter.spec().centre();
    put(&mut app, me, foot + Vec3::X * 1.0);
    app.update();
    press(&mut app, me);
    check("a wreck can't be driven", app.world().get::<Riding>(me).is_none(), format!("{:?}", app.world().get::<Kinematic>(me).unwrap().mode));

    // A new match brings fresh vehicles.
    app.world_mut().write_message(shared::sim::RestartMatch);
    for _ in 0..60 {
        app.update();
    }
    let world = app.world_mut();
    let (count, wrecks) = (world.query::<&Vehicle>().iter(world).count(), world.query::<&Vehicle>().iter(world).filter(|v| v.wrecked).count());
    let generation = world.resource::<Match>().generation;
    check("a new match, new vehicles", count >= 16 && wrecks == 0 && generation == 2, format!("match {generation}: {count} vehicles, {wrecks} wrecks"));

    // An engine is heard past any step, by a bot who can see no further than its nose.
    let me = player(&mut app);
    let listener = {
        let world = app.world_mut();
        world.query_filtered::<Entity, (With<BotBrain>, Without<Eliminated>)>().iter(world).next().unwrap()
    };
    app.world_mut().get_mut::<BotBrain>(listener).unwrap().sight = (1.0, 1.0);
    let v = spawn(&mut app, VehicleKind::Car, start, yaw_along(along));
    let knows = |app: &App| app.world().get::<BotBrain>(listener).unwrap().senses.known.iter().any(|k| k.who == me);
    let off = Vec3::new(along.x, 0.0, along.y) * (ENGINE_NOISE - 15.0);
    for _ in 0..24 {
        put(&mut app, listener, start + off);
        put(&mut app, me, start + Vec3::new(out.y, 0.0, -out.x) * 1.5);
        app.update();
    }
    let on_foot = knows(&app);
    press(&mut app, me);
    for _ in 0..24 {
        put(&mut app, listener, start + off);
        app.update();
    }
    let riding = app.world().get::<Riding>(me).is_some();
    check("bots hear an engine", riding && !on_foot && knows(&app), format!("at {:.0} m: standing by it unheard {}, at the wheel heard {}", off.length(), !on_foot, knows(&app)));
    press(&mut app, me);

    // A bot with a long way to go takes a vehicle beside it, drives the street route and gets out at
    // the end of it; a driver who is stopped gets out in seconds.
    if app.world().resource::<MapLayout>().style == MapStyle::Ncr {
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
        let driver = listener;
        let ring = compose::network().swap_remove(0).1;
        let at = |k: usize| ring[k];
        let (from, to) = (at(6), at(40));
        let height = |app: &App, p: Vec2| app.world().resource::<MapLayout>().terrain.height(p.x, p.y);
        let foot = Vec3::new(from.x, height(&app, from), from.y);
        let beside = foot + Vec3::new(1.0, 0.0, 0.0) * 9.0;
        let goal = Vec3::new(to.x, height(&app, to), to.y);
        // Nobody else matters here, and the match must go on.
        let tick = |app: &mut App| {
            {
                let mut b = app.world_mut().get_mut::<BotBrain>(driver).unwrap();
                b.goal = Some(goal);
                b.target = None;
                b.provoked = None;
                b.think = 5.0;
            }
            for who in [driver, me] {
                app.world_mut().get_mut::<Health>(who).unwrap().hp = 100.0;
            }
        };
        // A bot stood on the road gets off the line of what is coming at it, at the player's top speed.
        let trials = 6;
        let mut run_down = 0;
        for k in 0..trials {
            let (start, stood) = (at(60 + k * 12), at(62 + k * 12));
            let ground = |app: &App, p: Vec2| Vec3::new(p.x, height(app, p), p.y);
            let d = (stood - start).normalize();
            let from_here = ground(&app, start);
            let bike = spawn(&mut app, VehicleKind::Bullet, from_here, yaw_along(d));
            put(&mut app, me, from_here + Vec3::new(d.y, 0.0, -d.x) * 1.2);
            app.update();
            press(&mut app, me);
            app.world_mut().get_mut::<Vehicle>(bike).unwrap().speed = 19.0;
            app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
            let spot = ground(&app, stood);
            put(&mut app, driver, spot);
            let before = app.world().get::<Health>(driver).unwrap().shield;
            let mut hit = false;
            for _ in 0..110 {
                {
                    let mut b = app.world_mut().get_mut::<BotBrain>(driver).unwrap();
                    b.goal = Some(spot);
                    b.target = None;
                    b.provoked = None;
                    b.calm = false;
                    b.think = 5.0;
                }
                app.update();
                let h = app.world().get::<Health>(driver).unwrap();
                hit |= h.hp < 100.0 || h.shield < before;
                let h = app.world().get::<Kinematic>(driver).unwrap().velocity.length();
                hit |= h > 11.0;
                app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
            }
            run_down += hit as usize;
            app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
            press(&mut app, me);
            app.world_mut().entity_mut(bike).despawn();
            app.world_mut().get_mut::<Health>(driver).unwrap().hp = 100.0;
            app.update();
        }
        check("bots sidestep a closing vehicle", run_down * 3 <= trials as usize, format!("{run_down} of {trials} run down, at 19 m/s on the Ring Road"));
        // Blocked: its wheels turn and it goes nowhere.
        let car = spawn(&mut app, VehicleKind::Car, foot, yaw_along(at(7) - from));
        put(&mut app, driver, beside);
        let (mut in_at, mut out_at) = (None, None);
        for n in 0..60 * 20 {
            tick(&mut app);
            if in_at.is_some() {
                app.world_mut().get_mut::<Vehicle>(car).unwrap().speed = 0.0;
            }
            app.update();
            let riding = app.world().get::<Riding>(driver).is_some();
            if riding && in_at.is_none() {
                in_at = Some(n);
            }
            if !riding && in_at.is_some() {
                out_at = Some(n);
                break;
            }
        }
        let stuck = app.world().resource::<Hitches>().stuck;
        check("a stuck driver gets out", out_at.is_some() && stuck == 1, format!("in at tick {in_at:?}, out at tick {out_at:?}: {stuck} ride ended stuck"));
        app.world_mut().entity_mut(car).despawn();
        app.update();
        // A long way to go. It won't take another for a while after that one.
        let car = spawn(&mut app, VehicleKind::Car, foot, yaw_along(at(7) - from));
        put(&mut app, driver, beside);
        let (mut rode, mut far, mut ticks, mut top) = (false, 0.0f32, 0, 0.0f32);
        for _ in 0..60 * 140 {
            tick(&mut app);
            app.update();
            ticks += 1;
            if app.world().get::<Riding>(driver).is_some() {
                rode = true;
                let t = app.world().get::<Transform>(car).unwrap().translation;
                far = far.max(t.xz().distance(from));
                top = top.max(app.world().get::<Vehicle>(car).unwrap().speed);
            } else if rode {
                break;
            } else if ticks % 120 == 0 {
                put(&mut app, driver, beside);
            }
        }
        let t = app.world().get::<Transform>(driver).unwrap().translation;
        let short = t.xz().distance(to);
        let rides = app.world().resource::<Hitches>().rides;
        let on_foot = app.world().get::<Riding>(driver).is_none();
        check("a bot drives a long way and gets out", rode && far > 300.0 && short < 60.0 && on_foot, format!("{rides} rides, {far:.0} m from where it set out, top {top:.1} m/s, on foot {short:.0} m from its goal after {:.0} s", ticks as f32 / 60.0));
        app.world_mut().entity_mut(car).despawn();
        app.update();
    }

    // Water takes a vehicle down to a wade.
    let wade = Rules::of(app.world().resource::<MapLayout>().style).wade_speed;
    let edge = water_edge(app.world().resource::<MapLayout>());
    match (wade, edge) {
        (Some(wade), Some((bank, into))) => {
            app.world_mut().entity_mut(v).despawn();
            let v = spawn(&mut app, VehicleKind::Commuter, bank, yaw_along(into));
            put(&mut app, me, bank + Vec3::new(into.y, 0.0, -into.x) * 1.2);
            app.update();
            press(&mut app, me);
            app.world_mut().get_mut::<Vehicle>(v).unwrap().speed = VehicleKind::Commuter.spec().law.top;
            app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
            let (mut dry, mut wet, mut ticks) = (0.0f32, 0.0f32, 0);
            for _ in 0..60 * 6 {
                app.update();
                let t = app.world().get::<Transform>(v).unwrap().translation;
                let speed = app.world().get::<Vehicle>(v).unwrap().speed.abs();
                if app.world().resource::<MapLayout>().terrain.ground_at(t.x, t.z) == Some(ground::WATER) {
                    // The tick it crosses over it still has its speed.
                    ticks += 1;
                    wet = if ticks > 1 { wet.max(speed) } else { 0.0 };
                } else {
                    dry = dry.max(speed);
                }
            }
            app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
            check("water slows a vehicle to a wade", ticks > 30 && dry > wade * 2.0 && wet <= wade + 0.01, format!("from {:.0},{:.0}: {dry:.1} m/s on the bank, {wet:.1} m/s over {ticks} ticks in the water, wade {wade}", bank.x, bank.z));
        }
        (Some(_), None) => check("water slows a vehicle to a wade", false, "no bank found".into()),
        _ => println!("  no wading on this map"),
    }

    if app.world().resource::<MapLayout>().style == MapStyle::Ncr {
        if app.world().get::<Riding>(me).is_some() {
            press(&mut app, me);
        }
        // A tractor wades, unhurt; anything else loses its engine trying.
        let (bank, into) = water_edge(app.world().resource::<MapLayout>()).expect("a bank");
        for kind in [VehicleKind::Tractor, VehicleKind::Car] {
            let spec = kind.spec();
            let foot = bank - Vec3::new(into.x, 0.0, into.y) * 12.0;
            let foot = foot.with_y(app.world().resource::<MapLayout>().terrain.height(foot.x, foot.z));
            let v = spawn(&mut app, kind, foot, yaw_along(into));
            put(&mut app, me, foot + Vec3::new(into.y, 0.0, -into.x) * (spec.size.x / 2.0 + 0.9));
            app.update();
            press(&mut app, me);
            app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
            let (mut wet, mut ticks) = (0.0f32, 0);
            for _ in 0..60 * 18 {
                app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
                app.update();
                let t = app.world().get::<Transform>(v).unwrap().translation;
                let speed = app.world().get::<Vehicle>(v).unwrap().speed.abs();
                if app.world().resource::<MapLayout>().terrain.ground_at(t.x, t.z) == Some(ground::WATER) {
                    ticks += 1;
                    if ticks > 1 {
                        wet = wet.max(speed);
                    }
                }
            }
            let vehicle = app.world().get::<Vehicle>(v).unwrap();
            let (hp, wrecked) = (vehicle.hp, vehicle.wrecked);
            let seconds = ticks as f32 / 60.0;
            if kind == VehicleKind::Tractor {
                check("a tractor wades unhurt", ticks > 60 && (3.0..=3.51).contains(&wet) && hp == spec.hp, format!("{wet:.1} m/s over {seconds:.0} s of water, health {hp:.0}/{:.0}", spec.hp));
            } else {
                check("a river drowns any other engine", ticks > 60 && wet <= 2.01 && hp < spec.hp - 10.0 * seconds.min(10.0), format!("{wet:.1} m/s over {seconds:.0} s of water, health {hp:.0}/{:.0}, wrecked {wrecked}", spec.hp));
            }
            app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
            if app.world().get::<Riding>(me).is_some() {
                press(&mut app, me);
            }
            app.world_mut().entity_mut(v).despawn();
            app.update();
        }

        // Each ground's price, reached flat out on a straight of it.
        for (name, class, car, jeep) in [("open land", ground::OPEN, 0.7, 1.0), ("sand", ground::SAND, 0.45, 0.8)] {
            let Some(line) = stretch(app.world().resource::<MapLayout>(), class, 260.0) else {
                check(&format!("speed on {name}"), false, "no straight of it".into());
                continue;
            };
            for (kind, share) in [(VehicleKind::Car, car), (VehicleKind::Khuli, jeep)] {
                let run = pilot(&mut app, me, kind, &line, 30.0);
                let limit = kind.spec().law.top * share;
                check(&format!("{kind:?} on {name}"), run.top < limit + 3.0 && run.top > limit * 0.75, format!("holds {:.1} m/s of a {limit:.1} limit: {}", run.top, run.line()));
            }
        }

        // The market's tables stop what is driven at them, and leave a bay's vehicle its way out.
        let table = {
            let layout = app.world().resource::<MapLayout>();
            let open = |p: Vec2| layout.terrain.ground_at(p.x, p.y) == Some(ground::ROAD);
            layout.stalls.tables().filter(|s| s.first).map(|s| (s.pos, s.facing())).find(|(p, f)| (2..=22).all(|k| open(p.xz() + *f * k as f32) && layout.stalls.tables().all(|o| !o.covers(p.xz() + *f * k as f32))))
        };
        if let Some((at, f)) = table {
            for kind in [VehicleKind::Commuter, VehicleKind::Car] {
                let spec = kind.spec();
                let from = at.xz() + f * 22.0;
                let foot = Vec3::new(from.x, app.world().resource::<MapLayout>().terrain.height(from.x, from.y), from.y);
                let v = spawn(&mut app, kind, foot, yaw_along(-f));
                put(&mut app, me, foot + Vec3::new(-f.y, 0.0, f.x) * (spec.size.x / 2.0 + 0.9));
                app.update();
                press(&mut app, me);
                app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::Y;
                let (mut peak, mut nearest) = (0.0f32, f32::MAX);
                for _ in 0..60 * 5 {
                    app.world_mut().get_mut::<Health>(me).unwrap().hp = 100.0;
                    app.update();
                    let t = app.world().get::<Transform>(v).unwrap().translation;
                    peak = peak.max(app.world().get::<Vehicle>(v).unwrap().speed);
                    nearest = nearest.min((t.xz() - at.xz()).dot(f));
                }
                let vehicle = app.world().get::<Vehicle>(v).unwrap();
                let (speed, hp) = (vehicle.speed, vehicle.hp);
                check(&format!("a market table stops a {kind:?}"), nearest > 0.55 + spec.size.z / 2.0 - 0.1 && speed.abs() < 3.0, format!("peak {peak:.1} m/s, its nose {:.2} m off the table, then {speed:.1} m/s, health {hp:.0}/{:.0}", nearest - 0.55 - spec.size.z / 2.0, spec.hp));
                app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
                press(&mut app, me);
                app.world_mut().entity_mut(v).despawn();
                app.update();
            }
        } else {
            check("a market table stops a vehicle", false, "no table with a run-up".into());
        }
        // Every bay with a row near it where a car fits: a clear way out, and driven.
        let boxes = VehicleKind::Car.spec();
        let (ways, shut, boxed): (Vec<Vec<Vec2>>, Vec<Vec2>, usize) = app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery, layout: Res<MapLayout>, parked: Query<&Transform, With<Vehicle>>| {
                let filter = SpatialQueryFilter::from_mask(solid_mask());
                let shape = boxes.collider();
                let tables: Vec<_> = layout.stalls.tables().collect();
                let (mut ways, mut shut, mut boxed) = (vec![], vec![], 0);
                // This match's own vehicles have some of the bays: a car set down there is in one.
                let free = |b: &&shared::map::parking::Bay| parked.iter().all(|t| t.translation.xz().distance_squared(b.pos.xz()) > 64.0);
                for b in layout.parking.iter().filter(free).filter(|b| tables.iter().any(|s| s.pos.xz().distance_squared(b.pos.xz()) < 400.0)) {
                    let at = b.pos + Vec3::Y * boxes.centre();
                    let yaw = Quat::from_rotation_y(b.yaw);
                    if !spatial.shape_intersections(&shape, at, yaw, &filter).is_empty() {
                        continue;
                    }
                    // A car and a half clear of the bay: from there a driver can turn.
                    let config = ShapeCastConfig { max_distance: 6.0, ..default() };
                    let (mut clear, mut by_table) = (None, false);
                    for d in (0..16).map(|k| Vec2::from_angle(k as f32 / 16.0 * std::f32::consts::TAU)) {
                        let turn = Quat::from_rotation_y(yaw_along(d));
                        if !spatial.shape_intersections(&shape, at, turn, &filter).is_empty() {
                            continue;
                        }
                        let dir = Dir3::new(Vec3::new(d.x, 0.0, d.y)).unwrap();
                        match spatial.cast_shape(&shape, at, turn, dir, &config, &filter) {
                            None => clear = clear.or(Some(d)),
                            // A table's fault only if nothing else stands in that way.
                            Some(hit) => {
                                let table = tables.iter().any(|s| s.pos.xz().distance_squared(hit.point1.xz()) < 9.0 && s.covers(hit.point1.xz() + d * 0.2));
                                let past = filter.clone().with_excluded_entities([hit.entity]);
                                by_table |= table && spatial.cast_shape(&shape, at, turn, dir, &config, &past).is_none();
                            }
                        }
                    }
                    match clear {
                        Some(d) => ways.push(vec![b.pos.xz(), b.pos.xz() + d * 14.0]),
                        None if by_table => shut.push(b.pos.xz()),
                        // Hemmed in by the street kit, tables or no tables.
                        None => boxed += 1,
                    }
                }
                (ways, shut, boxed)
            })
            .unwrap();
        let mut drove = 0;
        for way in ways.iter().step_by(4) {
            let run = pilot(&mut app, me, VehicleKind::Car, way, 10.0);
            drove += (run.done || run.end.distance(way[0]) > 6.0) as usize;
            if !run.done && run.end.distance(way[0]) <= 6.0 {
                println!("  not out of the bay at {:.0}, heading for {:.0}: {}", way[0], way[1], run.line());
            }
        }
        check("a car has a clear way out of every bay by a row", shut.is_empty() && !ways.is_empty() && drove == ways.len().div_ceil(4), format!("{} bays with a table within 20 m where a car fits, {} shut in by tables {shut:.0?}, {boxed} hemmed in by the street kit, {drove} of {} driven out", ways.len() + shut.len() + boxed, shut.len(), ways.len().div_ceil(4)));
        if std::env::var("BAYS_ONLY").is_ok() {
            std::process::exit(0);
        }

        // All eight bridges, both ways, by car and by bike, down the middle of the road.
        for (s, name) in compose::SPANS.iter().zip(compose::BRIDGES) {
            // From the foot of one ramp to the foot of the other: past them the road may turn away.
            let out = [s.at(-s.ramp() - 6.0, 0.0), s.at(s.len() * 0.25, 0.0), s.at(s.len() * 0.75, 0.0), s.at(s.len() + s.ramp() + 6.0, 0.0)];
            let back = [out[3], out[2], out[1], out[0]];
            for kind in [VehicleKind::Car, VehicleKind::Commuter] {
                for (way, line) in [("east", out), ("west", back)] {
                    let run = pilot(&mut app, me, kind, &line, 70.0);
                    check(&format!("{name} {way}, {kind:?}"), run.done && run.lost == 0.0 && run.mean > 8.0, run.line());
                }
            }
        }

        // The Ring Road's whole loop.
        let ring = compose::network().swap_remove(0).1;
        let length: f32 = ring.windows(2).map(|s| s[0].distance(s[1])).sum();
        for kind in [VehicleKind::Car, VehicleKind::Commuter] {
            let run = pilot(&mut app, me, kind, &ring, 900.0);
            check(&format!("Ring Road lap, {kind:?}"), run.done && run.mean > 12.0, format!("{length:.0} m: {}", run.line()));
        }

        // The Ridge, by road and straight over its crest.
        let passes = ridge_passes(app.world().resource::<MapLayout>());
        check("three passes over the Ridge", passes.len() >= 3, format!("{} found", passes.len()));
        for (k, line) in passes.iter().take(3).enumerate() {
            for kind in [VehicleKind::Car, VehicleKind::Commuter] {
                let run = pilot(&mut app, me, kind, line, 90.0);
                check(&format!("Ridge pass {}, {kind:?}", k + 1), run.done, run.line());
            }
        }
        let over = [Vec2::new(-1390.0, -450.0), Vec2::new(-1030.0, -450.0)];
        for kind in [VehicleKind::Car, VehicleKind::Khuli] {
            let run = pilot(&mut app, me, kind, &over, 60.0);
            println!("  straight over the crest, {kind:?}: {}", run.line());
        }
    }

    // Home: nothing of the match is left on the map.
    leave_match(app.world_mut());
    app.update();
    let world = app.world_mut();
    let left = world.query::<&Vehicle>().iter(world).count();
    check("leaving a match takes the vehicles", left == 0, format!("{left} left"));
    println!("{}", if fails == 0 { "ALL PASS" } else { "FAILED" });
    std::process::exit(fails.min(1) as i32);
}
