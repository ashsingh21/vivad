//! Headless match: runs the authoritative simulation with no window or rendering, as fast as
//! possible, and reports how the match unfolded. A smoke test for the game loop and the bots.
//!
//! cargo run --release -p shared --bin sim_check -- [matches] [bots]
//!
//! SIM_SEED=<n> fixes the match seeds (the k-th match runs on n + k), so two runs see the same
//! vimana paths, Aandhi circles and loot. Bot aim and personalities stay random. SIM_QUIET=1 drops
//! the feed, the 30 s lines and the slow frames. Each match ends with one line per Aandhi death
//! saying why.
//!
//! To look closer: STUCK=1 names each bot going nowhere, INV=<seconds> lists what every bot holds
//! and wants at that time, TRACE=1 and TRACK=<name> follow bots through the early game. Each match
//! also reports two things a warrior's position alone can't show: "dither", bots flipping between
//! two goals (they keep moving, so they never count as stuck), and how much the ring burnt. The
//! `senses:` line says what the bots' senses made them do.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::time::TimeUpdateStrategy;
use shared::items::{ItemDb, ItemKind};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, Pickup, census, census_lines};
use shared::sim::zone::Zone;
use shared::sim::birds::{FLOCK_NOISE, Flock};
use shared::sim::bots::BotBrain;
use shared::sim::senses::{NoiseMsg, Tally};
use shared::sim::talk::FellMsg;
use shared::sim::{DamageCause, DamageMsg, HitMsg, Inventory, Eliminated, FeedMsg, Kinematic, MatchConfig, MoveMode, RestartMatch, SimPlugin, Warrior};

/// Hits landed this match.
#[derive(Resource, Default)]
struct Hits(u32);

fn count_hits(mut hits: MessageReader<HitMsg>, mut n: ResMut<Hits>) {
    n.0 += hits.read().count() as u32;
}

/// Sounds in the world this match, for the bots to hear: shots, and flocks going up.
#[derive(Resource, Default)]
struct Noises {
    shots: u32,
    flocks: u32,
}

fn count_noises(mut noises: MessageReader<NoiseMsg>, mut n: ResMut<Noises>) {
    for noise in noises.read() {
        if noise.radius == FLOCK_NOISE {
            n.flocks += 1;
        } else {
            n.shots += 1;
        }
    }
}

/// Pickups taken this match. Headless, a pickup entity only goes when a warrior takes it (by hand
/// or arrows walked over), or in the restart's sweep: that one lands in the new match's first
/// frame, with `Match::elapsed` just reset, so anything gone before half a second is not counted.
#[derive(Resource, Default)]
struct Taken(u32);

fn count_taken(mut gone: RemovedComponents<Pickup>, m: Res<Match>, mut n: ResMut<Taken>) {
    let gone = gone.read().count() as u32;
    if m.elapsed > 0.5 {
        n.0 += gone;
    }
}

/// What the ring burnt this match, warrior by warrior.
#[derive(Resource, Default)]
struct Burnt(HashMap<Entity, f32>);

fn count_burns(mut hurt: MessageReader<DamageMsg>, mut n: ResMut<Burnt>) {
    for d in hurt.read().filter(|d| d.cause == DamageCause::Pralaya) {
        *n.0.entry(d.target).or_default() += d.amount;
    }
}

/// A bot's goal changing eight times in ten seconds: it is flipping, and getting nowhere.
const FLIPS: usize = 8;
const FLIP_WINDOW: f32 = 10.0;

/// One bot's goal changes: its goal and the last few before it, when they changed, the seconds
/// spent flipping, and whether this spell has been told.
#[derive(Default)]
struct Flips {
    goal: Option<(i32, i32)>,
    last: VecDeque<Option<(i32, i32)>>,
    changes: VecDeque<f32>,
    secs: f32,
    told: bool,
}

/// One warrior's recent history, sampled twice a second, to explain an Aandhi death.
#[derive(Default, Clone)]
struct Track {
    name: String,
    pos: Vec3,
    edge: f32,
    outside_since: Option<f32>,
    outside_from: Vec3,
    walked: f32,
    fighting: u32,
    samples: u32,
    idle: u32,
    goal: Option<Vec3>,
    path: usize,
    urgent: bool,
    high: bool,
}

/// Why a warrior died in the Aandhi, from its time outside.
fn verdict(t: &Track, now: f32) -> &'static str {
    let since = t.outside_since.unwrap_or(now);
    let burning = (now - since).max(0.5);
    let share = |n: u32| n as f32 / t.samples.max(1) as f32;
    if t.high {
        "above the street"
    } else if share(t.fighting) > 0.5 {
        "fighting"
    } else if share(t.idle) > 0.5 {
        "stuck"
    } else if !t.urgent {
        "no zone goal"
    } else if t.path == 0 {
        "no path"
    } else if t.walked / burning < 2.0 {
        "slow"
    } else {
        "too far"
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let matches: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(shared::map::MapId::from_env().default_bots());
    let items =std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();

    let started = Instant::now();
    let mut app = App::new();
    let seed: Option<u64> = std::env::var("SIM_SEED").ok().map(|s| s.parse().expect("SIM_SEED: a number"));
    let quiet = std::env::var("SIM_QUIET").is_ok();
    app.insert_resource(MatchConfig { bots, autopilot: true, match_seed: seed, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        // Warnings only, so a double despawn or a dead entity in the sim shows up here.
        .add_plugins(bevy::log::LogPlugin { level: bevy::log::Level::WARN, ..default() })
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.init_resource::<Hits>().init_resource::<Taken>().init_resource::<Burnt>().init_resource::<Noises>().add_systems(FixedUpdate, (count_hits, count_burns, count_noises)).add_systems(Update, count_taken);
    // Driving the app by hand: plugins must be finalised as `App::run` would.
    app.finish();
    app.cleanup();

    // Terrain sanity: the physics heightfield must agree with MapLayout::terrain.height().
    for _ in 0..3 {
        app.update();
    }
    let worst = app
        .world_mut()
        .run_system_once(|spatial: SpatialQuery, layout: Res<shared::map::MapLayout>| {
            let mut worst = 0.0f32;
            for (x, z) in [(-180.0, -170.0), (60.0, 120.0), (130.0, -135.0), (210.0, 200.0), (-90.0, -40.0), (230.0, -230.0)] {
                let filter = SpatialQueryFilter::from_mask(shared::sim::solid_mask());
                if let Some(hit) = spatial.cast_ray(Vec3::new(x, 300.0, z), Dir3::NEG_Y, 600.0, true, &filter) {
                    let phys = 300.0 - hit.distance;
                    let want = layout.terrain.height(x, z);
                    println!("terrain check ({x:>6.0},{z:>6.0}): physics {phys:6.2}  data {want:6.2}");
                    worst = worst.max((phys - want).abs());
                }
            }
            worst
        })
        .unwrap();
    println!("terrain max mismatch {worst:.2} m");
    println!("map up and running after {:.1} s", started.elapsed().as_secs_f32());

    // The ranged weapons: a warrior holding one of these in a slot (arrows or not) is armed.
    let bows: HashSet<String> = app
        .world()
        .resource::<ItemDb>()
        .items
        .iter()
        .filter(|(_, d)| matches!(d.kind, ItemKind::Bow { .. }))
        .map(|(id, _)| id.clone())
        .collect();
    let melee = |id: &str, db: &ItemDb| matches!(db.get(id).kind, ItemKind::Melee { .. });

    let mut results = vec![];
    for n in 0..matches {
        if n > 0 {
            app.world_mut().write_message(RestartMatch);
        }
        let wall = Instant::now();
        let mut feed_seen = 0;
        let mut next_report = 0.0;
        let mut kills_by_weapon = 0;
        let mut kills_by_zone = 0;
        let mut kills_by_self = 0;
        let mut kills_other = 0;
        let mut kills_by_item: HashMap<String, u32> = HashMap::new();
        let mut carried_melee: HashSet<Entity> = HashSet::new();
        let mut tracks: HashMap<Entity, Track> = HashMap::new();
        let mut next_sample = 0.0;
        let mut reasons: Vec<(String, &'static str)> = vec![];
        let mut first_death = None;
        // Each death's match time and the ring's phase then: the shape of the match.
        let mut deaths: Vec<(f32, usize, f32)> = vec![];
        // Every warrior of this match, and the match time it first held a bow (None: never did;
        // one that fell unarmed stays None, its slots being emptied as it falls).
        let mut armed_at: HashMap<Entity, Option<f32>> = HashMap::new();
        let mut landed_at: HashMap<Entity, f32> = HashMap::new();
        let mut flips: HashMap<Entity, Flips> = HashMap::new();
        // Seconds that bots asked to walk and went nowhere, in all and by 40 m patch of the map.
        let mut stuck_secs = 0.0f32;
        let mut stuck_spots: HashMap<(i32, i32), f32> = HashMap::new();
        let mut opening_told = false;
        let mut loot_told = false;
        // Bot-seconds spent on bridge decks, in water, and on high ground, and the bots that
        // ever stood on a deck.
        let (mut on_decks, mut in_water, mut on_hills) = (0.0f32, 0.0f32, 0.0f32);
        let mut crossed: HashSet<Entity> = HashSet::new();
        // Milliseconds each frame took, for what the bots and physics cost.
        let mut frame_ms: Vec<f32> = vec![];
        app.world_mut().resource_mut::<Hits>().0 = 0;
        app.world_mut().resource_mut::<Taken>().0 = 0;
        app.world_mut().resource_mut::<Burnt>().0.clear();
        *app.world_mut().resource_mut::<Noises>() = Noises::default();
        loop {
            let frame = Instant::now();
            app.update();
            frame_ms.push(1000.0 * frame.elapsed().as_secs_f32());
            if !loot_told && std::env::var("SIM_QUIET").is_err() {
                loot_told = true;
                let world = app.world_mut();
                let floor: Vec<(Vec3, String)> = world.query::<(&Transform, &Pickup)>().iter(world).map(|(t, p)| (t.translation, p.stack.id.clone())).collect();
                let chests: Vec<_> = world.query::<&Chest>().iter(world).map(|c| (c.index, c.kind)).collect();
                let seed = world.resource::<Match>().seed;
                let (layout, db) = (world.resource::<shared::map::MapLayout>(), world.resource::<ItemDb>());
                for line in census_lines(&census(layout, db, seed, floor.into_iter(), chests.into_iter())) {
                    println!("loot {line}");
                }
            }
            let world = app.world_mut();
            if !quiet && frame_ms.last().is_some_and(|ms| *ms > 16.0) {
                println!("  slow frame: {:.1} ms at t={:.1}", frame_ms[frame_ms.len() - 1], world.resource::<Match>().elapsed);
            }
            let elapsed = world.resource::<Match>().elapsed;
            let fell: Vec<FellMsg> = world.resource_mut::<Messages<FellMsg>>().drain().collect();
            for f in fell {
                first_death.get_or_insert(elapsed);
                let z = world.resource::<Zone>();
                deaths.push((elapsed, z.phase, z.radius));
                match f.cause {
                    DamageCause::Pralaya => {
                        kills_by_zone += 1;
                        if let Some(t) = tracks.get(&f.victim) {
                            let why = verdict(t, elapsed);
                            println!(
                                "  [aandhi] t={elapsed:.0} {} {why}: at {:.0},{:.1},{:.0} edge {:.0} m, outside {:.0} s, walked {:.0} m, fighting {}/{} idle {}/{}, goal {:?} path {}",
                                t.name, t.pos.x, t.pos.y, t.pos.z, t.edge, elapsed - t.outside_since.unwrap_or(elapsed), t.walked,
                                t.fighting, t.samples, t.idle, t.samples, t.goal.map(|g| g.round()), t.path
                            );
                            reasons.push((t.name.clone(), why));
                        }
                    }
                    // A gun that backfired.
                    DamageCause::Weapon(_) if f.by == Some(f.victim) => kills_by_self += 1,
                    DamageCause::Weapon(item) => {
                        kills_by_weapon += 1;
                        *kills_by_item.entry(item.clone()).or_default() += 1;
                    }
                    DamageCause::Fall => {
                        kills_other += 1;
                        let t = tracks.get(&f.victim);
                        let ground = t.map_or(0.0, |t| world.resource::<shared::map::MapLayout>().terrain.height(t.pos.x, t.pos.z));
                        println!("  [other] {} fell: last seen at {:?}, ground {ground:.1}", t.map_or("?", |t| t.name.as_str()), t.map(|t| t.pos.round()));
                    }
                }
            }
            let feed: Vec<String> = world.resource_mut::<Messages<FeedMsg>>().drain().map(|f| f.0).collect();
            for f in feed {
                feed_seen += 1;
                if !quiet {
                    let spot = tracks.values().find(|t| f.contains(&format!(" {} ", t.name))).map(|t| t.pos.round());
                    println!("  [feed] t={elapsed:.0} {f} {spot:?}");
                }
            }
            let m = world.resource::<Match>().clone();
            if m.state == MatchState::InProgress {
                let zone_now = world.resource::<Zone>().clone();
                for (e, w, tf, b, k) in world.query_filtered::<(Entity, &Warrior, &Transform, &BotBrain, &Kinematic), Without<Eliminated>>().iter(world) {
                    if k.mode != MoveMode::Ground {
                        continue;
                    }
                    let f = flips.entry(e).or_default();
                    let goal = b.goal.map(|g| (g.x.round() as i32, g.z.round() as i32));
                    if goal != f.goal {
                        f.last.push_back(f.goal);
                        if f.last.len() > 3 {
                            f.last.pop_front();
                        }
                        f.goal = goal;
                        f.changes.push_back(m.elapsed);
                    }
                    while f.changes.front().is_some_and(|t| *t < m.elapsed - FLIP_WINDOW) {
                        f.changes.pop_front();
                    }
                    // In a fight a bot's goal means little: it stands and shoots.
                    if f.changes.len() >= FLIPS && b.target.is_none() {
                        f.secs += 1.0 / 60.0;
                        // FLIPS=1 tells what a bot was about when it began to flip.
                        if !f.told && std::env::var("FLIPS").is_ok() {
                            f.told = true;
                            let z = &zone_now;
                            let ratio = z.target_center.distance(tf.translation.xz()) / z.target_radius.max(1.0);
                            println!(
                                "    flips {} t={:.0} at {:.0},{:.0} goal {:?} of {:?} heading_in {} pressed {} urgent {} flee {:.1} target {} unstick {:.1} rest {:.1} centre/r {ratio:.2} closes in {:.0}",
                                w.name, m.elapsed, tf.translation.x, tf.translation.z, goal, f.last, b.heading_in, b.pressed, b.urgent, b.flee, b.target.is_some(), b.unstick, b.rest, z.closes_in()
                            );
                        }
                    } else {
                        f.told = false;
                    }
                }
            }
            if !opening_told && m.vimana_from != Vec3::ZERO {
                opening_told = true;
                let (z, a, b) = (world.resource::<Zone>().clone(), m.vimana_from, m.vimana_to);
                let chests = world.query::<&Chest>().iter(world).count();
                let pickups = world.query::<&Pickup>().iter(world).count();
                println!(
                    "match {}: {} warriors, opens on a disc at {:.0},{:.0} r{:.0}; flight {:.0},{:.0} to {:.0},{:.0} at {:.0} m; {chests} chests, {pickups} pickups",
                    n + 1, m.total, z.center.x, z.center.y, z.radius, a.x, a.z, b.x, b.z, a.y
                );
            }
            if m.elapsed >= next_sample {
                next_sample += 0.5;
                let zone = world.resource::<Zone>().clone();
                for (e, w, tf, b, k) in world
                    .query_filtered::<(Entity, &Warrior, &Transform, &BotBrain, &Kinematic), Without<Eliminated>>()
                    .iter(world)
                {
                    let t = tracks.entry(e).or_default();
                    t.name = w.name.clone();
                    let pos = tf.translation;
                    let step = pos.xz().distance(t.pos.xz());
                    t.edge = pos.xz().distance(zone.center) - zone.radius;
                    t.goal = b.goal;
                    t.path = b.path.len().max(b.route.len());
                    t.urgent = b.urgent;
                    t.high = pos.y > 2.5 && k.mode == MoveMode::Ground;
                    let terrain = &world.resource::<shared::map::MapLayout>().terrain;
                    if k.mode == MoveMode::Ground {
                        if shared::sim::nav::on_bridge(pos) {
                            on_decks += 0.5;
                            crossed.insert(e);
                        } else if terrain.ground_at(pos.x, pos.z) == Some(shared::map::terrain::ground::WATER) {
                            in_water += 0.5;
                        } else if terrain.height(pos.x, pos.z) > 6.0 {
                            on_hills += 0.5;
                        }
                    }
                    if b.walking && step < 0.4 && k.mode == MoveMode::Ground && m.state != MatchState::Ended {
                        stuck_secs += 0.5;
                        *stuck_spots.entry(((pos.x / 40.0).floor() as i32, (pos.z / 40.0).floor() as i32)).or_default() += 0.5;
                        // STUCK=1 names each bot that goes nowhere, and what it wanted.
                        if std::env::var("STUCK").is_ok() && b.stuck_for >= 1 {
                            println!(
                                "    stuck {} t={:.0} at {:.1},{:.1},{:.1} goal {:?} next {:?} leg {:?} route {} for {} urgent {} target {}",
                                w.name, m.elapsed, pos.x, pos.y, pos.z, b.goal.map(|g| g.round()), b.route.last().map(|p| p.round()), b.path.first().map(|p| p.round()),
                                b.route.len(), b.stuck_for, b.urgent, b.target.is_some()
                            );
                        }
                    }
                    if zone.outside(pos) && m.state == MatchState::InProgress {
                        if t.outside_since.is_none() {
                            t.outside_since = Some(m.elapsed);
                            t.outside_from = pos;
                            t.walked = 0.0;
                            t.fighting = 0;
                            t.idle = 0;
                            t.samples = 0;
                        }
                        t.walked += step;
                        t.samples += 1;
                        t.fighting += b.target.is_some() as u32;
                        t.idle += (b.target.is_none() && step < 0.75) as u32;
                    } else {
                        t.outside_since = None;
                    }
                    t.pos = pos;
                    if let Some(at) = b.landed {
                        landed_at.insert(e, at);
                    }
                }
                let db = world.resource::<ItemDb>().clone();
                for (e, inv, fallen) in world.query_filtered::<(Entity, &Inventory, Has<Eliminated>), With<Warrior>>().iter(world) {
                    let armed = armed_at.entry(e).or_insert(None);
                    if armed.is_none() && !fallen && inv.slots.iter().flatten().any(|s| bows.contains(&s.id)) {
                        *armed = Some(m.elapsed);
                    }
                    if inv.slots.iter().flatten().any(|s| melee(&s.id, &db)) {
                        carried_melee.insert(e);
                    }
                }
            }
            if !quiet && m.elapsed >= next_report {
                next_report += 30.0;
                let zone = world.resource::<Zone>().clone();
                let opened = world.query::<&Chest>().iter(world).filter(|c| c.opened).count();
                let total = world.query::<&Chest>().iter(world).count();
                let modes: Vec<MoveMode> = world.query_filtered::<&Kinematic, Without<Eliminated>>().iter(world).map(|k| k.mode).collect();
                let ground = modes.iter().filter(|m| **m == MoveMode::Ground).count();
                println!(
                    "t={:>5.0}s {:?} alive={} on_ground={} chests {}/{} zone r={:.0} phase={}",
                    m.elapsed, m.state, m.alive, ground, opened, total, zone.radius, zone.phase
                );
            }
            if std::env::var("TRACE").is_ok() && (m.elapsed * 60.0).round() as u32 % 180 == 0 && m.elapsed < 150.0 {
                for (i, (tf, b, k, inv)) in world
                    .query_filtered::<(&Transform, &BotBrain, &Kinematic, &Inventory), Without<Eliminated>>()
                    .iter(world)
                    .take(4)
                    .enumerate()
                {
                    let g = b.goal.map(|g| format!("{:.0},{:.0},{:.0} d={:.1}", g.x, g.y, g.z, g.distance(tf.translation))).unwrap_or("-".into());
                    let items: Vec<String> = inv.slots.iter().flatten().map(|s| s.id.clone()).collect();
                    println!("    bot{i} t={:.0} pos={:.0},{:.1},{:.0} {:?} goal={g} target={} gave_up={} items={items:?}",
                        m.elapsed, tf.translation.x, tf.translation.y, tf.translation.z, k.mode, b.target.is_some(), b.give_up.len());
                }
            }
            // TRACK=<name> follows one warrior twice a second: where it is, what it wants, how it moves.
            if let Ok(name) = std::env::var("TRACK") {
                if (m.elapsed * 60.0).round() as u32 % 30 == 0 {
                    let zone = world.resource::<Zone>().clone();
                    for (w, tf, b, k, intent) in world
                        .query_filtered::<(&Warrior, &Transform, &BotBrain, &Kinematic, &shared::sim::PlayerIntent), Without<Eliminated>>()
                        .iter(world)
                        .filter(|(w, tf, ..)| w.name == name || (name == "*" && zone.outside(tf.translation)) || (name == "all" && m.elapsed < 120.0))
                    {
                        let p = tf.translation;
                        let v = k.velocity;
                        let path: Vec<String> = b.path.iter().take(3).map(|q| format!("{:.0},{:.0}", q.x, q.y)).collect();
                        let goal = b.goal.map(|g| format!("{:.0},{:.0},{:.0}", g.x, g.y, g.z)).unwrap_or("-".into());
                        println!(
                            "    track {} t={:.1} pos={:.1},{:.1},{:.1} v={:.1},{:.1},{:.1} {:?} axis={:.1},{:.1} yaw={:.2} goal={goal} path={path:?} len={:.0} walk={} stuck={} unstick={:.1} urgent={} way_down={:?} target={}",
                            w.name, m.elapsed, p.x, p.y, p.z, v.x, v.y, v.z, k.mode, intent.move_axis.x, intent.move_axis.y, intent.yaw,
                            b.path_len, b.walking, b.stuck_for, b.unstick, b.urgent, b.way_down.map(|q| (q.x.round(), q.y.round())), b.target.is_some()
                        );
                    }
                }
            }
            if let Some(at) = std::env::var("INV").ok().and_then(|s| s.parse::<u32>().ok()) {
                if (m.elapsed * 60.0).round() as u32 == at * 60 {
                    let finds: Vec<Vec3> = world.query::<(&Transform, &Pickup)>().iter(world).filter(|(_, p)| bows.contains(&p.stack.id)).map(|(t, _)| t.translation).collect();
                    let closed: Vec<Vec3> = world.query::<(&Transform, &Chest)>().iter(world).filter(|(_, c)| !c.opened).map(|(t, _)| t.translation).collect();
                    for (w, tf, b, inv) in world.query_filtered::<(&Warrior, &Transform, &BotBrain, &Inventory), Without<Eliminated>>().iter(world) {
                        let items: Vec<String> = inv.slots.iter().flatten().map(|s| format!("{}x{}", s.id, s.count)).collect();
                        let near = |v: &[Vec3]| v.iter().map(|p| p.distance(tf.translation)).fold(f32::MAX, f32::min);
                        println!(
                            "    inv {} at {:.0},{:.0} goal {:?} urgent {} landed {:?} gave_up {} nearest bow {:.0} chest {:.0}: arrows {} {items:?}",
                            w.name, tf.translation.x, tf.translation.z, b.goal.map(|g| g.round()), b.urgent, b.landed.map(|t| t.round()), b.give_up.len(), near(&finds), near(&closed), inv.arrows
                        );
                    }
                }
            }
            if m.state == MatchState::Ended || m.elapsed > 1500.0 {
                let winner = m.winner.and_then(|w| world.get::<Warrior>(w)).map(|w| (w.name.clone(), w.kills));
                println!(
                    "match {} on {} ended at {:.0}s sim ({:.1}s wall): winner {:?}, {} weapon kills, {} Aandhi, {} feed lines",
                    n + 1, world.resource::<shared::map::MapLayout>().name, m.elapsed, wall.elapsed().as_secs_f32(), winner, kills_by_weapon, kills_by_zone, feed_seen
                );
                // A map swapped in at a restart leaves nothing of the last one behind.
                let layout = world.resource::<shared::map::MapLayout>();
                let want = layout.footprints.iter().filter(|f| f.mat != shared::map::Mat::Water).count();
                let wanted_props = layout.props.len();
                let terrains = world.query::<&shared::sim::MapTerrain>().iter(world).count();
                let footprints = world.query::<&shared::sim::MapFootprint>().iter(world).count();
                let props = world.query::<&shared::sim::PropIndex>().iter(world).count();
                assert!(terrains == 1 && footprints == want && props == wanted_props,
                    "map entities: {terrains} terrains, {footprints}/{want} footprints, {props}/{wanted_props} props");
                let (paths, mean_ms, worst_ms, expanded) = world.resource::<shared::sim::nav::NavGrid>().stats();
                println!("nav paths {paths} ({:.1}/s): mean {mean_ms:.2} ms, worst {worst_ms:.1} ms, mean {expanded:.0} nodes", paths as f32 / m.elapsed.max(1.0));
                let (paths, mean_ms, worst_ms, expanded) = world.resource::<shared::sim::nav::StreetFloor>().stats();
                println!("street searches {paths} ({:.1}/s): mean {mean_ms:.2} ms, worst {worst_ms:.1} ms, mean {expanded:.0} nodes", paths as f32 / m.elapsed.max(1.0));
                let (routes, mean_ms, worst_ms, expanded) = world.resource::<shared::sim::nav::StreetFloor>().route_stats();
                println!("routes {routes} ({:.1}/s): mean {mean_ms:.2} ms, worst {worst_ms:.1} ms, mean {expanded:.0} nodes", routes as f32 / m.elapsed.max(1.0));
                frame_ms.sort_by(f32::total_cmp);
                let at = |q: f32| frame_ms[((frame_ms.len() - 1) as f32 * q) as usize];
                println!(
                    "frame ms: mean {:.2}, p50 {:.2}, p99 {:.2}, worst {:.1}",
                    frame_ms.iter().sum::<f32>() / frame_ms.len().max(1) as f32, at(0.5), at(0.99), at(1.0)
                );
                let hits = world.resource::<Hits>().0;
                let opened = world.query::<&Chest>().iter(world).filter(|c| c.opened).count();
                println!(
                    "stats {}: seed {} length {:.0} weapon {} aandhi {} own_gun {} other {} hits {} chests {} first_death {:.0}",
                    n + 1, m.seed, m.elapsed, kills_by_weapon, kills_by_zone, kills_by_self, kills_other, hits, opened, first_death.unwrap_or(0.0)
                );
                // How soon the warriors found a bow: counts of those armed by 60 s, by 90 s, and by
                // the time the last one was; the rest never were (fell with none, or outlasted the
                // match without).
                let warriors = armed_at.len();
                let armed: Vec<f32> = armed_at.values().flatten().copied().collect();
                let by = |t: f32| armed.iter().filter(|a| **a <= t).count();
                let last = armed.iter().copied().fold(0.0f32, f32::max).ceil();
                // The fair clock on a big map, where the last bots land after a minute and a half.
                let soon = armed_at.iter().filter(|(e, a)| a.zip(landed_at.get(*e)).is_some_and(|(a, l)| a - l <= 60.0)).count();
                println!(
                    "armed: {}/{warriors} by 60s, {}/{warriors} by 90s, {}/{warriors} by {last:.0}s, {soon}/{warriors} within 60s of landing; never: {}",
                    by(60.0), by(90.0), armed.len(), warriors - armed.len()
                );
                // The stragglers by name, to trace them.
                let mut late: Vec<String> = armed_at
                    .iter()
                    .filter(|(_, a)| a.is_none_or(|t| t > 60.0))
                    .map(|(e, a)| {
                        let name = world.get::<Warrior>(*e).map(|w| w.name.clone()).unwrap_or_default();
                        a.map_or(format!("{name} never"), |t| format!("{name} {t:.0}s"))
                    })
                    .collect();
                late.sort();
                if !late.is_empty() {
                    println!("armed late: {}", late.join(", "));
                }
                let chests = world.query::<&Chest>().iter(world).count();
                println!("chests opened {opened}/{chests} ({:.0}%)", opened as f32 * 100.0 / chests.max(1) as f32);
                println!("pickups taken {}", world.resource::<Taken>().0);
                let mut by_item: Vec<_> = kills_by_item.iter().collect();
                by_item.sort_by_key(|(id, n)| (std::cmp::Reverse(**n), (*id).clone()));
                println!("kills by weapon: {}; {} warriors carried a street weapon", by_item.iter().map(|(id, n)| format!("{id} {n}")).collect::<Vec<_>>().join(", "), carried_melee.len());
                println!(
                    "ground: {on_decks:.0} bot-seconds on bridges ({} bots), {in_water:.0} in water, {on_hills:.0} on the hills",
                    crossed.len()
                );
                let mut spots: Vec<_> = stuck_spots.iter().map(|(k, s)| (*s, k.0 * 40 + 20, k.1 * 40 + 20)).collect();
                spots.sort_by(|a, b| b.0.total_cmp(&a.0));
                let worst: Vec<String> = spots.iter().take(6).map(|s| format!("{:.0}s at {},{}", s.0, s.1, s.2)).collect();
                println!("stuck {stuck_secs:.0} bot-seconds ({:.1} % of {:.0}); worst: {}", 100.0 * stuck_secs / (m.total as f32 * m.elapsed).max(1.0), m.total as f32 * m.elapsed, worst.join(", "));
                let flip_secs: Vec<f32> = flips.values().map(|f| f.secs).filter(|s| *s > 1.0).collect();
                println!(
                    "dither {:.0} bot-seconds ({} bots, worst {:.0} s)",
                    flip_secs.iter().fold(0.0, |a, b| a + b), flip_secs.len(), flip_secs.iter().copied().fold(0.0f32, f32::max)
                );
                // What the bots' senses made them do, summed over every brain, fallen ones too.
                let tally = world.query::<&BotBrain>().iter(world).fold(Tally::default(), |a, b| {
                    let t = b.senses.tally;
                    Tally {
                        noticed: a.noticed + t.noticed,
                        searches: a.searches + t.searches,
                        arrived: a.arrived + t.arrived,
                        found: a.found + t.found,
                        flights: a.flights + t.flights,
                        hid: a.hid + t.hid,
                    }
                });
                let flocks = world.query::<&Flock>().iter(world).count();
                let noises = world.resource::<Noises>();
                println!(
                    "senses: {} sightings; {} searches, {} reached, {} found someone; {} flights, {} to a hiding place; {} shots heard of, {} flocks of {flocks} put up",
                    tally.noticed, tally.searches, tally.arrived, tally.found, tally.flights, tally.hid, noises.shots, noises.flocks
                );
                let burnt = world.resource::<Burnt>();
                println!(
                    "ring burnt {} warriors, {:.0} damage in all, worst {:.0}",
                    burnt.0.len(), burnt.0.values().fold(0.0, |a, b| a + b), burnt.0.values().copied().fold(0.0f32, f32::max)
                );
                for (name, why) in &reasons {
                    println!("reason {why}: {name}");
                }
                let z = world.resource::<Zone>().clone();
                let nav = world.resource::<shared::sim::nav::NavGrid>();
                let timeline: Vec<String> = deaths.iter().map(|(t, p, r)| format!("{t:.0}s/p{p}/r{r:.0}")).collect();
                println!("deaths {}: {}", deaths.len(), timeline.join(" "));
                // The small maps have the street grid to ask; the NCR, its route floor, which knows
                // a spot a warrior can stand on from one a warrior can get to.
                let floor = world.resource::<shared::sim::nav::StreetFloor>();
                let open = |p: Vec2| if floor.ready() { floor.reachable(p) } else { nav.walkable(p) };
                println!(
                    "zone at end: phase {} radius {:.0} centre {:.0},{:.0} ({}), target r{:.0} centre {:.0},{:.0} ({})",
                    z.phase, z.radius, z.center.x, z.center.y, if open(z.center) { "reachable" } else { "BLOCKED" },
                    z.target_radius, z.target_center.x, z.target_center.y, if open(z.target_center) { "reachable" } else { "BLOCKED" }
                );
                results.push((m.elapsed, kills_by_weapon, kills_by_zone, m.state == MatchState::Ended));
                break;
            }
        }
    }
    let ended = results.iter().filter(|r| r.3).count();
    println!("\nSUMMARY: {ended}/{} matches finished; mean length {:.0}s; weapon kills/match {:.1}; Aandhi kills/match {:.1}",
        results.len(),
        results.iter().map(|r| r.0).sum::<f32>() / results.len() as f32,
        results.iter().map(|r| r.1 as f32).sum::<f32>() / results.len() as f32,
        results.iter().map(|r| r.2 as f32).sum::<f32>() / results.len() as f32);
}
