//! Headless match: runs the authoritative simulation with no window or rendering, as fast as
//! possible, and reports how the match unfolded. A smoke test for the game loop and the bots.
//!
//! cargo run --release -p shared --bin sim_check -- [matches] [bots]

use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::time::TimeUpdateStrategy;
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::Chest;
use shared::sim::zone::Zone;
use shared::sim::bots::BotBrain;
use shared::sim::{Inventory, Eliminated, FeedMsg, Kinematic, MatchConfig, MoveMode, RestartMatch, SimPlugin, Warrior};

fn main() {
    let mut args = std::env::args().skip(1);
    let matches: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(15);
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();

    let mut app = App::new();
    app.insert_resource(MatchConfig { bots, autopilot: true, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
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
        let mut roofed = false;
        let mut last_seen: std::collections::HashMap<Entity, Vec3> = Default::default();
        loop {
            app.update();
            let world = app.world_mut();
            let mut feed: Vec<String> = world.resource_mut::<Messages<FeedMsg>>().drain().map(|f| f.0).collect();
            for f in feed.drain(..) {
                feed_seen += 1;
                if f.contains("Aandhi") { kills_by_zone += 1 } else if f.contains("tapka") { kills_by_weapon += 1 }
                println!("  [feed] {f}");
            }
            let m = world.resource::<Match>().clone();
            // ROOF=1: once nine in ten have landed, every bot on foot is put on a tall building's roof (15-60 m up),
            // to see that each finds its way down (the stranded count below).
            let landed = world.query_filtered::<&Kinematic, Without<Eliminated>>().iter(world).filter(|k| k.mode == MoveMode::Ground).count();
            if std::env::var("ROOF").is_ok() && !roofed && m.state == MatchState::InProgress && landed * 10 >= m.alive as usize * 9 {
                roofed = true;
                let layout = world.resource::<shared::map::MapLayout>().clone();
                let tall: Vec<Vec3> = layout.footprints.iter()
                    .filter(|f| f.mat != shared::map::Mat::Water && (15.0..60.0).contains(&(f.top - f.base)) && f.pts.len() >= 3)
                    .map(|f| { let c = f.pts.iter().copied().sum::<Vec2>() / f.pts.len() as f32; Vec3::new(c.x, f.top, c.y) })
                    .collect();
                let mut bots = world.query_filtered::<(&mut Transform, &mut Kinematic), (With<BotBrain>, Without<Eliminated>)>();
                for (i, (mut tf, mut k)) in bots.iter_mut(world).enumerate() {
                    if k.mode == MoveMode::Ground && !tall.is_empty() {
                        let r = tall[(i * 977) % tall.len()];
                        tf.translation = r + Vec3::Y * (shared::sim::HALF_HEIGHT + 0.2);
                        k.velocity = Vec3::ZERO;
                        println!("    put on a roof at {:.0},{:.1},{:.0}", r.x, r.y, r.z);
                    }
                }
            }
            if m.elapsed >= next_report {
                next_report += 30.0;
                let zone = world.resource::<Zone>().clone();
                let opened = world.query::<&Chest>().iter(world).filter(|c| c.opened).count();
                let total = world.query::<&Chest>().iter(world).count();
                let modes: Vec<MoveMode> = world.query_filtered::<&Kinematic, Without<Eliminated>>().iter(world).map(|k| k.mode).collect();
                let ground = modes.iter().filter(|m| **m == MoveMode::Ground).count();
                // Up off the land and no more than 2 m from where they were 30 s ago: stranded on a roof.
                let layout = world.resource::<shared::map::MapLayout>().clone();
                let now: Vec<(Entity, Vec3, bool)> = world
                    .query_filtered::<(Entity, &Transform, &Kinematic), (With<BotBrain>, Without<Eliminated>)>()
                    .iter(world)
                    .map(|(e, tf, k)| (e, tf.translation, k.mode == MoveMode::Ground))
                    .collect();
                let mut stranded = 0;
                let up_now = now.iter().filter(|(_, p, _)| p.y - layout.terrain.height(p.x, p.z) > 6.0).count();
                for &(e, p, on_foot) in &now {
                    let up = p.y - layout.terrain.height(p.x, p.z) > 6.0;
                    if on_foot && up && last_seen.get(&e).is_some_and(|q| q.distance(p) < 2.0) {
                        stranded += 1;
                        println!("    stranded at {:.0},{:.1},{:.0} ({:.0} m up)", p.x, p.y, p.z, p.y - layout.terrain.height(p.x, p.z));
                    }
                }
                last_seen = now.iter().map(|&(e, p, _)| (e, p)).collect();
                println!(
                    "t={:>5.0}s {:?} alive={} on_ground={} chests {}/{} zone r={:.0} phase={} up={up_now} stranded={stranded}",
                    m.elapsed, m.state, m.alive, ground, opened, total, zone.radius, zone.phase
                );
            }
            // TRACE=<s>: the first four bots every 3 s up to that many seconds in (150 by default).
            let until: f32 = std::env::var("TRACE").ok().and_then(|s| s.parse().ok()).unwrap_or(150.0);
            if std::env::var("TRACE").is_ok() && (m.elapsed * 60.0).round() as u32 % 180 == 0 && m.elapsed < until {
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
