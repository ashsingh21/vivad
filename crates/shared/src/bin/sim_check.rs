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
            if m.elapsed >= next_report {
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
