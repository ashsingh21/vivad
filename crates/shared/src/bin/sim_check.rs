//! Headless match: runs the authoritative simulation with no window or rendering, as fast as
//! possible, and reports how the match unfolded. A smoke test for the game loop and the bots.
//!
//! cargo run --release -p shared --bin sim_check -- [matches] [bots]
//!
//! SIM_SEED=<n> fixes the match seeds (the k-th match runs on n + k), so two runs see the same
//! vimana paths, Aandhi circles and loot. Bot aim and personalities stay random. SIM_QUIET=1 drops
//! the feed and the 30 s lines. Each match ends with one line per Aandhi death saying why.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::time::TimeUpdateStrategy;
use shared::items::{ItemDb, ItemKind};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, Pickup};
use shared::sim::zone::Zone;
use shared::sim::bots::BotBrain;
use shared::sim::{HitMsg, Inventory, Eliminated, FeedMsg, Kinematic, MatchConfig, MoveMode, RestartMatch, SimPlugin, Warrior};

/// Hits landed this match.
#[derive(Resource, Default)]
struct Hits(u32);

fn count_hits(mut hits: MessageReader<HitMsg>, mut n: ResMut<Hits>) {
    n.0 += hits.read().count() as u32;
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
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(15);
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();

    let mut app = App::new();
    let seed: Option<u64> = std::env::var("SIM_SEED").ok().map(|s| s.parse().expect("SIM_SEED: a number"));
    let quiet = std::env::var("SIM_QUIET").is_ok();
    app.insert_resource(MatchConfig { bots, autopilot: true, match_seed: seed, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.init_resource::<Hits>().init_resource::<Taken>().add_systems(FixedUpdate, count_hits).add_systems(Update, count_taken);
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

    // The ranged weapons: a warrior holding one of these in a slot (arrows or not) is armed.
    let bows: HashSet<String> = app
        .world()
        .resource::<ItemDb>()
        .items
        .iter()
        .filter(|(_, d)| matches!(d.kind, ItemKind::Bow { .. }))
        .map(|(id, _)| id.clone())
        .collect();

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
        let mut kills_other = 0;
        let mut tracks: HashMap<Entity, Track> = HashMap::new();
        let mut next_sample = 0.0;
        let mut reasons: Vec<(String, &'static str)> = vec![];
        let mut first_death = None;
        // Each death's match time and the ring's phase then: the shape of the match.
        let mut deaths: Vec<(f32, usize, f32)> = vec![];
        // Every warrior of this match, and the match time it first held a bow (None: never did;
        // one that fell unarmed stays None, its slots being emptied as it falls).
        let mut armed_at: HashMap<Entity, Option<f32>> = HashMap::new();
        app.world_mut().resource_mut::<Hits>().0 = 0;
        app.world_mut().resource_mut::<Taken>().0 = 0;
        loop {
            app.update();
            let world = app.world_mut();
            let mut feed: Vec<String> = world.resource_mut::<Messages<FeedMsg>>().drain().map(|f| f.0).collect();
            for f in feed.drain(..) {
                feed_seen += 1;
                let elapsed = world.resource::<Match>().elapsed;
                if !f.contains("jeet") {
                    first_death.get_or_insert(elapsed);
                    let z = world.resource::<Zone>();
                    deaths.push((elapsed, z.phase, z.radius));
                }
                if f.contains("Aandhi") {
                    kills_by_zone += 1;
                    // "Aandhi ne X ko uda diya" / "Aandhi ne tumhe uda diya".
                    let t = tracks.values().find(|t| f.contains(&format!(" {} ", t.name)) || (t.name == "You" && f.contains("tumhe")));
                    if let Some(t) = t {
                        let why = verdict(t, elapsed);
                        println!(
                            "  [aandhi] t={elapsed:.0} {} {why}: at {:.0},{:.1},{:.0} edge {:.0} m, outside {:.0} s, walked {:.0} m, fighting {}/{} idle {}/{}, goal {:?} path {}",
                            t.name, t.pos.x, t.pos.y, t.pos.z, t.edge, elapsed - t.outside_since.unwrap_or(elapsed), t.walked,
                            t.fighting, t.samples, t.idle, t.samples, t.goal.map(|g| g.round()), t.path
                        );
                        reasons.push((t.name.clone(), why));
                    }
                } else if f.contains("tapka") {
                    kills_by_weapon += 1;
                } else if !f.contains("jeet") {
                    kills_other += 1;
                    println!("  [other] {f}");
                }
                if !quiet {
                    println!("  [feed] t={elapsed:.0} {f}");
                }
            }
            let m = world.resource::<Match>().clone();
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
                    t.path = b.path.len();
                    t.urgent = b.urgent;
                    t.high = pos.y > 2.5 && k.mode == MoveMode::Ground;
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
                }
                for (e, inv, fallen) in world.query_filtered::<(Entity, &Inventory, Has<Eliminated>), With<Warrior>>().iter(world) {
                    let armed = armed_at.entry(e).or_insert(None);
                    if armed.is_none() && !fallen && inv.slots.iter().flatten().any(|s| bows.contains(&s.id)) {
                        *armed = Some(m.elapsed);
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
            if std::env::var("INV").is_ok() && (m.elapsed * 60.0).round() as u32 == 100 * 60 {
                for inv in world.query_filtered::<&Inventory, Without<Eliminated>>().iter(world) {
                    let items: Vec<String> = inv.slots.iter().flatten().map(|s| format!("{}x{}", s.id, s.count)).collect();
                    println!("    inv sel={} arrows={} {items:?}", inv.selected, inv.arrows);
                }
            }
            if m.state == MatchState::Ended || m.elapsed > 1200.0 {
                let winner = m.winner.and_then(|w| world.get::<Warrior>(w)).map(|w| (w.name.clone(), w.kills));
                println!(
                    "match {} on {} ended at {:.0}s sim ({:.1}s wall): winner {:?}, {} weapon kills, {} Aandhi, {} feed lines",
                    n + 1, world.resource::<shared::map::MapLayout>().name, m.elapsed, wall.elapsed().as_secs_f32(), winner, kills_by_weapon, kills_by_zone, feed_seen
                );
                // A map swapped in at a restart leaves nothing of the last one behind.
                let layout = world.resource::<shared::map::MapLayout>().clone();
                let terrains = world.query::<&shared::sim::MapTerrain>().iter(world).count();
                let footprints = world.query::<&shared::sim::MapFootprint>().iter(world).count();
                let props = world.query::<&shared::sim::PropIndex>().iter(world).count();
                let want = layout.footprints.iter().filter(|f| f.mat != shared::map::Mat::Water).count();
                assert!(terrains == 1 && footprints == want && props == layout.props.len(),
                    "map entities: {terrains} terrains, {footprints}/{want} footprints, {props}/{} props", layout.props.len());
                let (paths, mean_ms, worst_ms, expanded) = world.resource::<shared::sim::nav::NavGrid>().stats();
                println!("nav paths {paths} ({:.1}/s): mean {mean_ms:.2} ms, worst {worst_ms:.1} ms, mean {expanded:.0} nodes", paths as f32 / m.elapsed.max(1.0));
                let hits = world.resource::<Hits>().0;
                let opened = world.query::<&Chest>().iter(world).filter(|c| c.opened).count();
                println!(
                    "stats {}: seed {} length {:.0} weapon {} aandhi {} other {} hits {} chests {} first_death {:.0}",
                    n + 1, m.seed, m.elapsed, kills_by_weapon, kills_by_zone, kills_other, hits, opened, first_death.unwrap_or(0.0)
                );
                // How soon the warriors found a bow: counts of those armed by 60 s, by 90 s, and by
                // the time the last one was; the rest never were (fell with none, or outlasted the
                // match without).
                let warriors = armed_at.len();
                let armed: Vec<f32> = armed_at.values().flatten().copied().collect();
                let by = |t: f32| armed.iter().filter(|a| **a <= t).count();
                let last = armed.iter().copied().fold(0.0f32, f32::max).ceil();
                println!(
                    "armed: {}/{warriors} by 60s, {}/{warriors} by 90s, {}/{warriors} by {last:.0}s; never: {}",
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
                for (name, why) in &reasons {
                    println!("reason {why}: {name}");
                }
                let z = world.resource::<Zone>().clone();
                let nav = world.resource::<shared::sim::nav::NavGrid>();
                let timeline: Vec<String> = deaths.iter().map(|(t, p, r)| format!("{t:.0}s/p{p}/r{r:.0}")).collect();
                println!("deaths {}: {}", deaths.len(), timeline.join(" "));
                println!(
                    "zone at end: phase {} radius {:.0} centre {:.0},{:.0} ({}), target r{:.0} centre {:.0},{:.0} ({})",
                    z.phase, z.radius, z.center.x, z.center.y, if nav.walkable(z.center) { "walkable" } else { "BLOCKED" },
                    z.target_radius, z.target_center.x, z.target_center.y, if nav.walkable(z.target_center) { "walkable" } else { "BLOCKED" }
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
