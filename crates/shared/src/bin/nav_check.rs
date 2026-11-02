//! Headless check of the bots' street grid on a map: how much is walkable, which chests can be
//! reached, whether every Aandhi circle can be walked to, and what a path costs.
//!
//! VIVAD_MAP=cp cargo run --release -p shared --bin nav_check -- [out.pgm]
//!
//! On the NCR the bots go by the street floor's routes instead (`StreetFloor`), and this checks
//! those: every chest reached, every ring of a dozen matches at each size, what a route costs, how
//! the bridges meet the ground, and where it is too steep to climb. Then it tries the NCR's rules
//! for real with one warrior: the dive, wading, the roof rescue, and the map swapped away and back.

use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use rand::{RngExt, SeedableRng};
use shared::SmallRng;
use shared::map::{MapId, MapLayout};
use shared::map::compose::SPANS;
use shared::map::terrain::ground;
use shared::sim::nav::{NavGrid, NoPath, StreetFloor};
use shared::sim::flow::{Match, start_match};
use shared::sim::zone::Zone;
use shared::sim::{HALF_HEIGHT, Health, Kinematic, MapBlock, MapSwaps, MatchConfig, MoveMode, PlayerIntent, SimPlugin, Warrior, btn, load_map, map_reloaded};

#[derive(Default)]
struct Tally {
    ok: u32,
    there: u32,
    here: u32,
    far: f32,
    worst: f32,
    secs: f32,
}

impl Tally {
    fn add(&mut self, nav: &NavGrid, a: Vec3, b: Vec3) {
        let t = Instant::now();
        let r = nav.path(a, b);
        let s = t.elapsed().as_secs_f32();
        self.secs += s;
        self.worst = self.worst.max(s);
        match r {
            Ok(path) => {
                self.ok += 1;
                let mut at = a.xz();
                let mut len = 0.0;
                for p in &path {
                    len += at.distance(*p);
                    at = *p;
                }
                self.far += len / a.xz().distance(b.xz()).max(1.0);
            }
            Err(NoPath::There) => self.there += 1,
            Err(NoPath::Here) => self.here += 1,
        }
    }

    fn print(&self, what: &str) {
        let n = self.ok + self.there + self.here;
        println!(
            "{what}: {}/{n} ok, {} no-way-there, {} lost, detour x{:.2}, mean {:.2} ms, worst {:.1} ms",
            self.ok, self.there, self.here, self.far / self.ok.max(1) as f32,
            1000.0 * self.secs / n.max(1) as f32, 1000.0 * self.worst
        );
    }
}

/// How routes between two points went on the 8 m grid.
#[derive(Default)]
struct Routes {
    ok: u32,
    none: u32,
    wet: u32,
    bridge: u32,
    detour: f32,
    secs: f32,
    worst: f32,
}

impl Routes {
    fn add(&mut self, floor: &StreetFloor, layout: &MapLayout, a: Vec3, b: Vec3) {
        let t = Instant::now();
        let route = floor.route(a, b);
        let s = t.elapsed().as_secs_f32();
        self.secs += s;
        self.worst = self.worst.max(s);
        if route.is_empty() {
            self.none += (a.xz().distance(b.xz()) > 16.0) as u32;
            return;
        }
        self.ok += 1;
        let on_bridge = |p: &Vec2| SPANS.iter().any(|s| p.distance(s.a) + p.distance(s.c) < s.a.distance(s.c) + 1.0);
        self.bridge += route.iter().any(on_bridge) as u32;
        self.wet += route.iter().any(|p| !on_bridge(p) && layout.terrain.ground_at(p.x, p.y) == Some(ground::WATER)) as u32;
        let len: f32 = route.windows(2).map(|w| w[0].distance(w[1])).sum();
        self.detour += len / a.xz().distance(b.xz()).max(1.0);
    }

    fn print(&self, what: &str) {
        let n = self.ok + self.none;
        println!(
            "{what}: {}/{n} routed, {} over a bridge, {} through water, grid length x{:.2} of the straight line, mean {:.2} ms, worst {:.1} ms",
            self.ok, self.bridge, self.wet, self.detour / self.ok.max(1) as f32, 1000.0 * self.secs / n.max(1) as f32, 1000.0 * self.worst
        );
    }
}

/// Ground too steep to walk, on land: how much, and where the worst of it is.
fn check_slopes(layout: &MapLayout) {
    const CELL: f32 = 2.0;
    let t = &layout.terrain;
    let half = layout.half_extent;
    let steps = (2.0 * half / CELL) as i32;
    // The steepest walkable slope, as rise over run (the cosine of 48 degrees in movement.rs).
    let limit = 1.14;
    let mut steep = 0;
    let mut spots: std::collections::HashMap<(i32, i32), (u32, f32)> = Default::default();
    for i in 0..steps {
        for j in 0..steps {
            let p = Vec2::new(i as f32, j as f32) * CELL - Vec2::splat(half);
            let rise = (t.height(p.x + CELL, p.y) - t.height(p.x, p.y)).abs().max((t.height(p.x, p.y + CELL) - t.height(p.x, p.y)).abs());
            if rise / CELL > limit {
                steep += 1;
                let spot = spots.entry(((p.x / 100.0).floor() as i32, (p.y / 100.0).floor() as i32)).or_default();
                spot.0 += 1;
                spot.1 = spot.1.max(rise);
            }
        }
    }
    let mut worst: Vec<_> = spots.iter().map(|(k, s)| (s.0, k.0 * 100 + 50, k.1 * 100 + 50, s.1)).collect();
    worst.sort_by_key(|s| std::cmp::Reverse(s.0));
    let top: Vec<String> = worst.iter().take(8).map(|s| format!("{} at {},{} (step {:.1} m)", s.0, s.1, s.2, s.3)).collect();
    println!("steep ground: {steep} 2 m cells in {} patches; most: {}", worst.len(), top.join(", "));
}

/// How a warrior gets onto each bridge: the ground under its two ends and the step up to the deck.
fn check_decks(layout: &MapLayout) {
    for (k, s) in SPANS.iter().enumerate() {
        let along = (s.c - s.a).normalize_or_zero();
        let len = s.a.distance(s.c);
        let at = |d: f32| { let p = s.a + along * d; layout.terrain.height(p.x, p.y) };
        let flush: Vec<f32> = (0..=(len as i32)).map(|d| d as f32).filter(|d| (at(*d) - s.top).abs() < 0.3).collect();
        println!(
            "bridge {k}: {:.0} m, top {:.1}, ground at ends {:.1} and {:.1}, beyond the ends {:.1} and {:.1}, flush with the ground from {:?} to {:?} m",
            len, s.top, at(0.0), at(len), at(-6.0), at(len + 6.0), flush.first(), flush.last()
        );
    }
}

/// The NCR's route grid: from one place to every chest, and from anywhere to every ring.
fn check_routes(floor: &StreetFloor, layout: &MapLayout) {
    check_slopes(layout);
    check_decks(layout);
    let (kept, laid) = floor.route_points();
    println!("route points: {kept} of {laid} join up");
    let half = layout.half_extent;
    let mut rng = SmallRng::seed_from_u64(1);
    let mut chests = Routes::default();
    let start = Vec3::new(-400.0, 0.0, -300.0);
    for c in layout.chests.iter().filter(|c| c.pos.y < 2.5) {
        let before = chests.none;
        chests.add(floor, layout, start, c.pos);
        if chests.none > before {
            println!("  no way to the chest at {:.0},{:.0}", c.pos.x, c.pos.z);
        }
    }
    chests.print(&format!("{} ground chests from the middle", layout.chests.iter().filter(|c| c.pos.y < 2.5).count()));
    let on_land = |rng: &mut SmallRng| loop {
        let p = Vec2::new(rng.random_range(-half..half), rng.random_range(-half..half));
        if layout.terrain.ground_at(p.x, p.y) != Some(ground::WATER) {
            return p.extend(0.0).xzy();
        }
    };
    for players in [16, 32, 64] {
        let mut rings = Routes::default();
        let mut wet = 0;
        for seed in 0..12 {
            let mut z = Zone::for_ncr(1000 + seed, players, layout, floor);
            let mut phase = usize::MAX;
            while z.phase < z.phases().len() {
                if z.phase != phase {
                    phase = z.phase;
                    if layout.terrain.ground_at(z.target_center.x, z.target_center.y) == Some(ground::WATER) {
                        wet += 1;
                        println!("  ring on water: seed {seed} phase {phase} at {:.0},{:.0} r{:.0} from {:.0},{:.0} r{:.0}", z.target_center.x, z.target_center.y, z.target_radius, z.from_center.x, z.from_center.y, z.from_radius);
                    }
                    for _ in 0..4 {
                        rings.add(floor, layout, on_land(&mut rng), z.target_center.extend(0.0).xzy());
                    }
                }
                z.advance(0.5);
            }
        }
        println!("{players} players: {wet} rings centred on water");
        rings.print("  anywhere to a ring's centre");
    }
}

fn tick(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as u32 {
        app.update();
    }
}

/// Puts a warrior at a spot as if it had just stepped off something, unhurt and doing nothing.
fn place(app: &mut App, me: Entity, at: Vec3) {
    let world = app.world_mut();
    world.get_mut::<Transform>(me).unwrap().translation = at;
    *world.get_mut::<Kinematic>(me).unwrap() = Kinematic { mode: MoveMode::Ground, fall_from: Some(at.y), ..default() };
    *world.get_mut::<Health>(me).unwrap() = Health::default();
    *world.get_mut::<PlayerIntent>(me).unwrap() = PlayerIntent::default();
}

/// Walks forward (sprinting) from a spot and says how fast a warrior goes there.
fn pace(app: &mut App, me: Entity, at: Vec3) -> f32 {
    place(app, me, at);
    let mut intent = app.world_mut().get_mut::<PlayerIntent>(me).unwrap();
    intent.move_axis = Vec2::Y;
    intent.held = btn::SPRINT;
    tick(app, 1.0);
    let from = app.world().get::<Transform>(me).unwrap().translation;
    tick(app, 3.0);
    app.world().get::<Transform>(me).unwrap().translation.distance(from) / 3.0
}

/// Whether, and how badly, a warrior comes down from `height` over open ground; `press` asks for
/// the glider part of the way down. Returns the mode a second in, and the health it lands with.
fn fall(app: &mut App, me: Entity, spot: Vec2, height: f32, press: bool) -> (MoveMode, f32) {
    let ground = app.world().resource::<MapLayout>().terrain.height(spot.x, spot.y);
    place(app, me, Vec3::new(spot.x, ground + height, spot.y));
    tick(app, 0.4);
    if press {
        app.world_mut().get_mut::<PlayerIntent>(me).unwrap().pressed |= btn::JUMP;
    }
    tick(app, 0.6);
    let mode = app.world().get::<Kinematic>(me).unwrap().mode;
    for _ in 0..20 {
        tick(app, 1.0);
        if app.world().get::<Kinematic>(me).unwrap().grounded {
            break;
        }
    }
    (mode, app.world().get::<Health>(me).unwrap().hp)
}

/// The NCR's rules, each tried for real by one warrior under the game's own movement, then the
/// map's entities, and the map swapped away and back.
fn check_rules(app: &mut App) {
    tick(app, 0.2);
    let mut warriors = app.world_mut().query_filtered::<Entity, With<Warrior>>();
    let me = warriors.iter(app.world()).next().expect("a warrior");
    let half = app.world().resource::<MapLayout>().half_extent;
    // The drop: the press that leaves the ride is a dive, and a second press is the glider.
    while app.world().resource::<Match>().vimana_pos().xz().abs().max_element() > half - 30.0 {
        tick(app, 0.25);
    }
    let height = app.world().resource::<Match>().vimana_from.y;
    let mode = |app: &App| app.world().get::<Kinematic>(me).unwrap().mode;
    let y = |app: &App| app.world().get::<Transform>(me).unwrap().translation.y;
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().pressed |= btn::JUMP;
    tick(app, 0.2);
    let (left, top) = (mode(app), y(app));
    tick(app, 3.0);
    let (later, low) = (mode(app), y(app));
    app.world_mut().get_mut::<PlayerIntent>(me).unwrap().pressed |= btn::JUMP;
    tick(app, 0.2);
    println!("drop from {height:.0} m: first press {left:?}, 3 s on {later:?} and {:.0} m lower, second press {:?}", top - low, mode(app));
    // Wading against walking, on the Yamuna's bed and the sand beside it.
    let (bed, sand) = (Vec2::new(190.0, -330.0), Vec2::new(340.0, -330.0));
    let terrain = |app: &App, p: Vec2| app.world().resource::<MapLayout>().terrain.height(p.x, p.y) + HALF_HEIGHT + 0.05;
    let wade = pace(app, me, bed.extend(terrain(app, bed)).xzy());
    let run = pace(app, me, sand.extend(terrain(app, sand)).xzy());
    println!("sprint on sand {run:.1} m/s, through the river {wade:.1} m/s");
    // The roof rescue.
    for (from, press) in [(60.0, false), (20.0, false), (20.0, true)] {
        let (mode, hp) = fall(app, me, sand, from, press);
        println!("off a {from:.0} m roof{}: {mode:?} a second in, lands on {hp:.0} hp", if press { ", glider asked for" } else { "" });
    }
    let count = |app: &mut App| {
        let mut blocks = app.world_mut().query_filtered::<(), With<MapBlock>>();
        blocks.iter(app.world()).count()
    };
    let layout = app.world().resource::<MapLayout>();
    let (all, solid, water) = (layout.blocks.len(), layout.blocks.iter().filter(|b| b.collide).count(), layout.blocks.iter().filter(|b| !b.collide && b.mat == shared::map::Mat::Water && b.visible).count());
    let doors = layout.doors.len();
    println!("blocks: {all} in the map, {solid} solid, {water} water, {doors} doors; {} entities", count(app));
    // What the client's rebuilds run on, asked as they ask it, one frame's question at a time.
    let swapped = app.world_mut().register_system(|swaps: Res<MapSwaps>| map_reloaded(swaps));
    let ask = |app: &mut App| app.world_mut().run_system(swapped).unwrap();
    let first = ask(app);
    start_match(app.world_mut());
    let rematch = ask(app);
    let t = Instant::now();
    load_map(app.world_mut(), MapId::Cp);
    let small = app.world().resource::<StreetFloor>().ready();
    let on_cp = (t.elapsed().as_secs_f32(), count(app));
    start_match(app.world_mut());
    let swap = ask(app);
    let after = ask(app);
    let t = Instant::now();
    load_map(app.world_mut(), MapId::Ncr);
    let big = app.world().resource::<StreetFloor>().ready();
    println!(
        "map swap: to CP in {:.2} s, route grid there {small:?}, {} entities; back to the NCR in {:.2} s, route grid {big:?}, {} entities",
        on_cp.0, on_cp.1, t.elapsed().as_secs_f32(), count(app)
    );
    println!("map_reloaded: first look {first}, a plain rematch {rematch}, a swap then a match start {swap}, the frame after {after}");
    assert_eq!((first, rematch, swap, after), (false, false, true, false), "the client's rebuild signal");
}

fn main() {
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    let out = std::env::args().nth(1);
    let mut app = App::new();
    app.insert_resource(MatchConfig { bots: 0, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    if app.world().resource::<StreetFloor>().ready() {
        app.world_mut().run_system_once(|floor: Res<StreetFloor>, layout: Res<MapLayout>| check_routes(&floor, &layout)).unwrap();
        check_rules(&mut app);
        return;
    }
    let t0 = Instant::now();
    let mut ticks = 0;
    while !app.world().resource::<NavGrid>().ready() {
        app.update();
        ticks += 1;
        assert!(ticks < 5000, "nav grid never finished");
    }
    println!("grid ready after {ticks} ticks, {:.1} s wall", t0.elapsed().as_secs_f32());
    app.world_mut()
        .run_system_once(move |nav: Res<NavGrid>, layout: Res<MapLayout>| {
            let half = layout.half_extent;
            let n = (2.0 * half) as usize;
            let mut rng = SmallRng::seed_from_u64(1);
            let mut free = vec![];
            let mut pgm = format!("P5\n{n} {n}\n255\n").into_bytes();
            for j in 0..n {
                for i in 0..n {
                    let p = Vec2::new(i as f32 + 0.5 - half, j as f32 + 0.5 - half);
                    let w = nav.walkable(p);
                    if w {
                        free.push(p);
                    }
                    pgm.push(if w { 255 } else { 0 });
                }
            }
            println!("walkable {:.1}%", 100.0 * free.len() as f32 / (n * n) as f32);
            for c in &layout.chests {
                let p = (c.pos.xz() + Vec2::splat(half)).as_uvec2();
                let k = (p.y as usize * n + p.x as usize).min(n * n - 1) + pgm.len() - n * n;
                pgm[k] = 128;
            }
            if let Some(out) = &out {
                std::fs::write(out, pgm).unwrap();
            }
            let (joined, stairs) = nav.stair_count();
            println!("stairs joined {joined} of {stairs}");
            for (foot, head) in nav.stair_ends() {
                println!("  stair {:.1},{:.1},{:.1} -> {:.1},{:.1},{:.1}", foot.x, foot.y, foot.z, head.x, head.y, head.z);
            }
            // SKETCH=level,x0,z0,x1,z1 draws that part of the grid.
            if let Ok(s) = std::env::var("SKETCH") {
                let v: Vec<f32> = s.split(',').map(|x| x.parse().unwrap()).collect();
                println!("{}", nav.sketch(v[0] as usize, Vec2::new(v[1], v[2]), Vec2::new(v[3], v[4])));
            }
            let pick = |rng: &mut SmallRng| free[rng.random_range(0..free.len())].extend(0.9).xzy();
            // From the park's middle to every chest.
            let start = layout.chests.iter().map(|c| c.pos).find(|p| p.length() < 100.0).unwrap_or(Vec3::ZERO);
            let (mut ground, mut upper) = (Tally::default(), Tally::default());
            let mut low = 0;
            for c in &layout.chests {
                if c.pos.y > 2.5 {
                    upper.add(&nav, start, c.pos);
                } else {
                    low += 1;
                    let before = ground.there + ground.here;
                    ground.add(&nav, start, c.pos);
                    if ground.there + ground.here > before {
                        println!("  no way to the chest at {:.0},{:.0}", c.pos.x, c.pos.z);
                    }
                }
            }
            ground.print(&format!("{low} ground chests from the park"));
            upper.print("upstairs chests, by the stairs");
            let mut anywhere = Tally::default();
            for _ in 0..300 {
                let (a, b) = (pick(&mut rng), pick(&mut rng));
                anywhere.add(&nav, a, b);
            }
            anywhere.print("anywhere to anywhere");
            // Every Aandhi circle of a dozen matches, from a warrior outside it.
            let mut zones = Tally::default();
            let mut buried = 0;
            let mut circles = 0;
            for seed in 0..12 {
                let mut z = Zone::for_extent(1000 + seed, half);
                let mut phase = 99;
                for _ in 0..4000 {
                    z.advance(0.5);
                    if z.phase != phase {
                        phase = z.phase;
                        circles += 1;
                        buried += (!nav.walkable(z.target_center)) as u32;
                        for _ in 0..8 {
                            zones.add(&nav, pick(&mut rng), z.target_center.extend(0.0).xzy());
                        }
                    }
                }
            }
            println!("{circles} circles, {buried} centred inside something");
            zones.print("anywhere to a circle's centre");
        })
        .unwrap();
}
