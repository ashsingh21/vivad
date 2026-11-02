//! Proves the hollow buildings (crate::map::interiors) work, on the map the game plays: the rooms
//! as a warrior meets them, with the real capsule, gravity and collide-and-slide.
//!
//!   cargo run --release -p shared --bin room_check     (or `just rooms`)
//!   --plan <k>    draw hollow building k's ground floor as the bots' floor sees it, and stop.
//!   --list        list the hollow buildings, and stop.
//!   ONLY=<k>      walk only hollow building k.
//!   EVERY=<n>     walk every nth building (default so that 60 are walked).
//!
//! What it checks: every door has the street's routes at it; every ground-floor room can be reached
//! from a door; everything indoors that loot stands on can be reached; a walker goes door to room to
//! stairs to roof in 60 buildings, and how much of that walk was lost to walls is reported, as
//! sticking is what a player would feel; a bot set on every 10th roof gets down the stairs within
//! 30 s by `way_down` alone, onto a floor the bots' routes lead out of. It exits with a failure if
//! any of that doesn't hold.
use avian3d::prelude::*;
use bevy::{ecs::message::Messages, prelude::*, time::TimeUpdateStrategy};
use shared::{
    map::{
        MapId, MapLayout, compose,
        interiors::{Interior, Plan},
    },
    sim::{nav::StreetFloor, *},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const TICK: f64 = 1.0 / 60.0;
/// A point of the way counts as passed this close to it.
const REACH: f32 = 0.45;

struct Walked {
    arrived: bool,
    seconds: f32,
    /// What a clear run of the way would have taken.
    ideal: f32,
    /// Seconds spent crawling, under a third of the speed asked for, on the flat or the stairs.
    stuck: f32,
    hurt: f32,
    end: Vec3,
    failed_at: usize,
}

fn walker(app: &mut App, at: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            Transform::from_translation(at),
            Kinematic { mode: MoveMode::Ground, ..default() },
            Action::default(),
            PlayerIntent { move_axis: Vec2::Y, ..default() },
            RigidBody::Kinematic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
            CollisionLayers::new(Layer::Character, [Layer::World, Layer::Door]),
        ))
        .id()
}

fn hurt_by(app: &mut App, id: Entity) -> f32 {
    let mut msgs = app.world_mut().resource_mut::<Messages<DamageMsg>>();
    let total = msgs.drain().filter(|m| m.target == id).map(|m| m.amount).sum();
    total
}

/// Walks a list of points, at a sprint on the street and a walk indoors, as a player does.
fn walk(app: &mut App, way: &[(Vec2, f32)], trace: bool) -> Walked {
    let start = Vec3::new(way[0].0.x, way[0].1 + HALF_HEIGHT + 0.05, way[0].0.y);
    let id = walker(app, start);
    let ideal: f32 = way.windows(2).map(|s| s[0].0.distance(s[1].0)).sum::<f32>() / movement::WALK_SPEED;
    let budget = ideal + 25.0;
    let mut out = Walked { arrived: false, seconds: 0.0, ideal, stuck: 0.0, hurt: 0.0, end: start, failed_at: 0 };
    let mut next = 1;
    let mut last = start;
    let mut since = 0.0f32;
    while out.seconds < budget {
        let pos = app.world().get::<Transform>(id).expect("walker").translation;
        let d = way[next].0 - pos.xz();
        if d.length() < REACH {
            next += 1;
            since = 0.0;
            if next == way.len() {
                out.arrived = true;
                break;
            }
            continue;
        }
        {
            let mut intent = app.world_mut().get_mut::<PlayerIntent>(id).expect("walker");
            intent.yaw = (-d.x).atan2(-d.y);
            intent.move_axis = Vec2::Y;
        }
        app.update();
        out.seconds += TICK as f32;
        since += TICK as f32;
        let now = app.world().get::<Transform>(id).expect("walker").translation;
        if (now - last).length() / (TICK as f32) < movement::WALK_SPEED / 3.0 {
            out.stuck += TICK as f32;
        }
        last = now;
        if trace && (out.seconds * 2.0).fract() < 0.02 {
            println!("    t {:.1} at {:.2} next {next} of {}", out.seconds, now, way.len());
        }
        if since > 6.0 {
            break;
        }
    }
    out.failed_at = next;
    out.end = app.world().get::<Transform>(id).expect("walker").translation;
    out.hurt = hurt_by(app, id);
    app.world_mut().despawn(id);
    out
}

/// Sets a walker on the roof, and lets `way_down` take it to the street.
fn come_down(app: &mut App, r: &Interior, floor: &StreetFloor) -> (bool, f32, Vec3) {
    // Mid-roof, away from the stair room.
    let z = r.stair.map_or(r.d / 2.0, |s| (s.z.0 - 1.0).max(1.0) / 2.0 + 0.5);
    let start = r.at(r.w / 2.0, r.roof + HALF_HEIGHT + 0.1, z);
    let id = walker(app, start);
    let mut seconds = 0.0;
    let mut end = start;
    let mut down = false;
    while seconds < 30.0 {
        let pos = app.world().get::<Transform>(id).expect("walker").translation;
        end = pos;
        let layout = app.world().resource::<MapLayout>();
        if std::env::var("ONLY").is_ok() && (seconds * 2.0f32).fract() < 0.02 {
            println!("    t {seconds:.1} local {:.2} y {:.2} heading {:.2?}", r.local(pos.xz()), pos.y, nav::way_down(layout, pos).map(|p| r.local(p)));
        }
        let Some(p) = nav::way_down(layout, pos) else {
            // Down: from where it stands, the floor must know the way on, as it does for a bot.
            down = pos.y - HALF_HEIGHT - layout.terrain.height(pos.x, pos.z) < 1.5 && floor.snap(pos.xz(), 2.0).is_some_and(|q| floor.reachable(q));
            break;
        };
        let d = p - pos.xz();
        {
            let mut intent = app.world_mut().get_mut::<PlayerIntent>(id).expect("walker");
            intent.yaw = (-d.x).atan2(-d.y);
            intent.move_axis = Vec2::Y;
        }
        app.update();
        seconds += TICK as f32;
    }
    app.world_mut().despawn(id);
    let _ = hurt_by(app, id);
    (down, seconds, end)
}

/// One storey as the bots' floor sees it, front at the top, as text.
fn sketch(r: &Interior, floor: &StreetFloor) {
    println!("{:?} {:.1} x {:.1} at {:.1}, {} storeys, {} rooms, {} pieces, {} boxes, {} doors", r.plan, r.w, r.d, r.corner, r.storeys, r.rooms.len(), r.pieces.len(), r.solids().len(), r.doors.len());
    println!("stair {:?}", r.stair);
    for w in r.walls.iter().filter(|w| w.y.0 < r.floor + 3.0) {
        println!("  wall {} at {:.2} {:?} s {:.2?} y {:.2?} cuts {:.2?}", if w.along_x { "x" } else { "z" }, w.at, w.kind, w.s, w.y, w.cuts.iter().map(|c| (c.s0, c.s1, c.y1)).collect::<Vec<_>>());
    }
    for m in r.rooms.iter().filter(|m| m.storey == 0) {
        println!("  room x {:.2?} z {:.2?} shaft {} exit {:.2?}", m.x, m.z, m.shaft, m.exit.map(|e| (e.before, e.beyond)));
    }
    for p in &r.pieces {
        let l = r.local(p.pos.xz());
        if p.pos.y < r.floor + 1.0 {
            println!("  {} at {:.2}", p.name, l);
        }
    }
    if let Ok(probe) = std::env::var("PROBE") {
        let v: Vec<f32> = probe.split(',').map(|x| x.parse().unwrap()).collect();
        for k in 0..20 {
            let z = v[1] + k as f32 * 0.25;
            let p = r.at(v[0], 0.0, z).xz();
            let hit: Vec<String> = r.solids().iter().filter(|b| {
                let q = b.rot.inverse() * (Vec3::new(p.x, 1.0, p.y) - b.pos);
                match b.shape {
                    shared::map::Shape::Box(s) => q.x.abs() < s.x / 2.0 + 0.4 && q.z.abs() < s.z / 2.0 + 0.4 && q.y.abs() < s.y / 2.0,
                    _ => false,
                }
            }).map(|b| format!("{:.2?} {:?}", r.local(b.pos.xz()), b.shape)).collect();
            println!("  z {z:.2} walkable {} {hit:?}", floor.walkable(p));
        }
    }
    let cells = |m: f32| (m / 0.25) as i32;
    for j in -cells(3.0)..cells(r.d + 1.0) {
        let mut line = String::new();
        for i in -cells(1.0)..cells(r.w + 1.0) {
            let p = r.at(i as f32 * 0.25, 0.0, j as f32 * 0.25).xz();
            line.push(if floor.is_indoors(p) {
                '.'
            } else if floor.walkable(p) {
                if r.contains(p) { '?' } else { ' ' }
            } else {
                '#'
            });
        }
        println!("{line}");
    }
}

fn main() {
    let mut app = App::new();
    app.insert_resource(MatchConfig { map: MapId::Ncr, bots: 0, online: true, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: include_str!("../../../../assets/data/items.ron").into() })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(TICK)));
    app.finish();
    app.cleanup();
    let clock = Instant::now();
    for _ in 0..3 {
        app.update();
    }
    let args: Vec<String> = std::env::args().collect();
    let map = app.world().resource::<MapLayout>().clone();
    let t = Instant::now();
    let _ = shared::map::generate(MapId::Ncr, 7);
    let built = t.elapsed().as_secs_f32();
    let t = Instant::now();
    let floor = StreetFloor::new(&map);
    println!("the map builds in {built:.2} s, its street floor in {:.2} s", t.elapsed().as_secs_f32());
    if let Some(k) = args.iter().position(|a| a == "--plan").and_then(|i| args.get(i + 1)).and_then(|s| s.parse::<usize>().ok()) {
        sketch(&map.rooms[k], &floor);
        return;
    }
    if args.iter().any(|a| a == "--list") {
        for (k, r) in map.rooms.iter().enumerate() {
            let c = r.at(r.w / 2.0, 0.0, r.d / 2.0).xz();
            let place = compose::PLACES.iter().find(|d| (c - d.at).abs().max_element() <= d.clip).map_or("open land", |d| d.name);
            println!("{k} {:?} {} storeys, {:.1} x {:.1}, {} rooms, {place} at {c:.0}", r.plan, r.storeys, r.w, r.d, r.rooms.len());
        }
        return;
    }
    let mut failures = 0;
    // What there is.
    let mut plans: BTreeMap<String, usize> = BTreeMap::new();
    let mut places: BTreeMap<String, usize> = BTreeMap::new();
    let (mut boxes, mut most, mut rooms, mut pieces) = (0, 0, 0, 0);
    for r in &map.rooms {
        *plans.entry(format!("{:?}", r.plan)).or_default() += 1;
        let c = r.at(r.w / 2.0, 0.0, r.d / 2.0).xz();
        let place = compose::PLACES.iter().find(|d| (c - d.at).abs().max_element() <= d.clip).map_or("open land", |d| d.name);
        *places.entry(place.into()).or_default() += 1;
        let n = r.solids().len();
        boxes += n;
        most = most.max(n);
        rooms += r.rooms.iter().filter(|m| !m.shaft).count();
        pieces += r.pieces.len();
    }
    let count = map.rooms.len().max(1);
    println!("{} hollow buildings: {plans:?}", map.rooms.len());
    println!("{places:?}");
    println!("{rooms} rooms, {pieces} pieces; {:.0} boxes a building, {most} at most; {} doors in `doors`", boxes as f32 / count as f32, map.doors.len());
    // Doors.
    let mut shut = 0;
    let mut mains = 0;
    for r in &map.rooms {
        let reached = r.doors.iter().filter(|d| floor.reachable(d.pos.xz() + d.out * 1.5)).count();
        mains += 1;
        if reached == 0 {
            shut += 1;
            println!("  no door of {:?} at {:.1} is reachable", r.plan, r.corner);
        }
    }
    println!("doors: {} of {mains} buildings can be walked into from the routes", mains - shut);
    failures += shut;
    // Rooms.
    let mut sealed_rooms = 0;
    for (k, r) in map.rooms.iter().enumerate() {
        for m in r.rooms.iter().filter(|m| m.storey == 0 && !m.shaft) {
            let mut any = false;
            let mut x = m.x.0;
            while x < m.x.1 {
                let mut z = m.z.0;
                while z < m.z.1 {
                    any |= floor.is_indoors(r.at(x, 0.0, z).xz());
                    z += 0.25;
                }
                x += 0.25;
            }
            if !any {
                sealed_rooms += 1;
                if sealed_rooms < 6 {
                    println!("  building {k}: a {:?} room at x {:.1?} z {:.1?} can't be reached", m.plan, m.x, m.z);
                }
            }
        }
    }
    println!("ground-floor rooms no bot can reach: {sealed_rooms}");
    failures += sealed_rooms;
    // Loot.
    let hollow_at = |p: Vec2| map.rooms.iter().find(|r| r.corner.distance_squared(p) < 3600.0 && r.contains(p));
    let (mut indoors, mut unreached, mut upstairs) = (0, 0, 0);
    for p in map.chests.iter().map(|c| c.pos).chain(map.floor_loot.iter().copied()) {
        let Some(r) = hollow_at(p.xz()) else { continue };
        indoors += 1;
        if p.y > r.floor + 2.0 {
            upstairs += 1;
            let l = r.local(p.xz());
            let storey = (((p.y - r.base + 0.2) / shared::map::interiors::STOREY) as u32).min(r.storeys);
            let up = storey == r.storeys || r.rooms.iter().any(|m| m.storey == storey && l.x > m.x.0 - 0.1 && l.x < m.x.1 + 0.1 && l.y > m.z.0 - 0.1 && l.y < m.z.1 + 0.1 && (m.exit.is_some() || m.shaft));
            if !up {
                unreached += 1;
                println!("  upstairs loot at {p:.1} in no room that leads to the stairs");
            }
        } else if !floor.is_indoors(p.xz()) {
            unreached += 1;
            let room = r.rooms.iter().find(|m| m.storey == 0 && r.local(p.xz()).x > m.x.0 - 0.1 && r.local(p.xz()).x < m.x.1 + 0.1 && r.local(p.xz()).y > m.z.0 - 0.1 && r.local(p.xz()).y < m.z.1 + 0.1);
            println!("  loot at {p:.1} is off the floor the bots can reach: building {}, local {:.1}, room {:.1?} {:.1?} {:?}", map.rooms.iter().position(|q| std::ptr::eq(q, r)).unwrap_or(0), r.local(p.xz()), room.map(|m| m.x), room.map(|m| m.z), room.map(|m| m.plan));
        }
    }
    println!("loot indoors: {indoors}, {upstairs} of it upstairs, {unreached} out of reach");
    failures += unreached;
    let gold = map.chests.iter().filter(|c| c.kind == shared::map::ChestKind::Kalash && hollow_at(c.pos.xz()).is_some()).count();
    let in_sets: Vec<_> = map.chests.iter().filter(|c| c.set.is_some()).collect();
    let inside = in_sets.iter().filter(|c| hollow_at(c.pos.xz()).is_some()).count();
    let finds = map.floor_loot.iter().filter(|p| hollow_at(p.xz()).is_some()).count();
    println!("chests indoors {inside} of the {} in sets, kalash {gold}; floor finds indoors {finds} of {}; {} ring shops furnished with {} pieces", in_sets.len(), map.floor_loot.len(), map.ring_shops.len().div_ceil(2), map.ring_pieces.len());
    // Walks.
    let tall: Vec<usize> = (0..map.rooms.len()).filter(|k| map.rooms[*k].plan != Plan::Lobby).collect();
    let every = std::env::var("EVERY").ok().and_then(|s| s.parse().ok()).unwrap_or((tall.len() / 60).max(1));
    let only: Option<usize> = std::env::var("ONLY").ok().and_then(|s| s.parse().ok());
    let list: Vec<usize> = match only {
        Some(k) => vec![k],
        None => tall.iter().copied().step_by(every).collect(),
    };
    let (mut ok, mut seconds, mut ideal, mut stuck, mut worst) = (0, 0.0, 0.0, 0.0, (0.0f32, 0usize));
    for &k in &list {
        let Some(way) = map.rooms[k].way_up(&floor) else {
            println!("  building {k}: no way up");
            failures += 1;
            continue;
        };
        let w = walk(&mut app, &way, only.is_some());
        if w.arrived && w.hurt == 0.0 {
            ok += 1;
            seconds += w.seconds;
            ideal += w.ideal;
            stuck += w.stuck;
            if w.stuck > worst.0 {
                worst = (w.stuck, k);
            }
        } else {
            failures += 1;
            println!("  building {k} ({:?}, {} storeys): stopped at point {} of {}, at {:.2}, hurt {:.0}, heading for {:.2}", map.rooms[k].plan, map.rooms[k].storeys, w.failed_at, way.len(), w.end, w.hurt, way[w.failed_at.min(way.len() - 1)].0);
        }
    }
    println!("walks up: {ok} of {} from the street to the roof; {seconds:.0} s against {ideal:.0} s clear, {stuck:.1} s crawling, worst {:.1} s in building {}", list.len(), worst.0, worst.1);
    // Down again.
    let downs: Vec<usize> = match only {
        Some(k) => vec![k],
        None => tall.iter().copied().step_by(10).collect(),
    };
    let (mut down, mut longest) = (0, 0.0f32);
    for &k in &downs {
        let (done, s, end) = come_down(&mut app, &map.rooms[k], &floor);
        if done {
            down += 1;
            longest = longest.max(s);
        } else {
            failures += 1;
            println!("  building {k} ({:?}, {} storeys): stuck on the way down at {end:.2} after {s:.1} s", map.rooms[k].plan, map.rooms[k].storeys);
        }
    }
    println!("down from the roof: {down} of {}, the longest {longest:.1} s", downs.len());
    println!("{failures} failures in {:.0} s", clock.elapsed().as_secs_f32());
    if failures > 0 {
        std::process::exit(1);
    }
}
