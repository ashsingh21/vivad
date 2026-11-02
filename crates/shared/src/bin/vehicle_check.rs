//! Vehicles against the real map, headless: where they park, get in and out, a wall stops one, it
//! slides along a wall it meets at an angle and never goes through, ramming hurts both ways, a wreck
//! throws its driver out, water slows one to a wade, bots hear an engine, a match's end clears them.
//!
//! VIVAD_MAP=cp cargo run --release -p shared --bin vehicle_check (the default map is the NCR)

use std::time::Duration;

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::items::{ItemStack, Rarity};
use shared::map::terrain::ground;
use shared::map::{MapLayout, Mat, footprint};
use shared::sim::bots::BotBrain;
use shared::sim::combat::Projectile;
use shared::sim::flow::{Match, Rules, leave_match};
use shared::sim::senses::ENGINE_NOISE;
use shared::sim::vehicle::{Riding, Vehicle, VehicleKind, spawn_vehicle};
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
        check("parked at match start", parked.len() * 4 >= want * 3, format!("{} of {want} vehicles: {} autos, {} scooters, {} bikes, {} heavy, {} cars", parked.len(), by(VehicleKind::Auto), by(VehicleKind::Scooter), by(VehicleKind::Commuter), by(VehicleKind::Bullet), by(VehicleKind::Car)));
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
        check(&format!("{kind:?} slides along"), slid > 6.0 && inside == 0, format!("{slid:.1} m along the wall, centre {closest:.2} m off it at the nearest, {inside} ticks inside it, still {:.1} m/s, health {:.0}/{:.0}", vehicle.speed, vehicle.hp, spec.hp));
        press(&mut app, me);
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().move_axis = Vec2::ZERO;
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
    check("bots do not drive", app.world().get::<Riding>(bot_brained).is_none() && app.world().get::<Vehicle>(v).unwrap().driver.is_none(), "a bot pressing the key beside an auto stays on foot".into());
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

    // Home: nothing of the match is left on the map.
    leave_match(app.world_mut());
    app.update();
    let world = app.world_mut();
    let left = world.query::<&Vehicle>().iter(world).count();
    check("leaving a match takes the vehicles", left == 0, format!("{left} left"));
    println!("{}", if fails == 0 { "ALL PASS" } else { "FAILED" });
    std::process::exit(fails.min(1) as i32);
}
