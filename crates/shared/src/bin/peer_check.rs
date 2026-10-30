//! Peers must agree without a host: given the same match seed, two separate processes have to
//! build the same match (the vimana's path, bot names, floor loot, every chest's contents, every
//! zone circle). This runs a match setup, then itself as a second process, and compares digests;
//! a different seed must give a different match.
//!
//! cargo run --release -p shared --bin peer_check -- [seed]

use std::process::Command;
use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::items::ItemDb;
use shared::sim::flow::Match;
use shared::sim::loot::{Chest, Pickup, chest_contents};
use shared::sim::zone::Zone;
use shared::sim::{MatchConfig, SimPlugin, Warrior};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--child") {
        let seed = args[1].parse().unwrap();
        println!("{}", describe(seed).0);
        return;
    }
    let seed: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(1234);
    let (mine, summary) = describe(seed);
    let mine = mine.trim().to_string();
    println!("{summary}");
    let child = |seed: u64| {
        let out = Command::new(std::env::current_exe().unwrap()).args(["--child", &seed.to_string()]).output().unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let mut failed = 0;
    let mut check = |ok: bool, what: &str| {
        println!("{} {what}", if ok { "PASS" } else { "FAIL" });
        failed += !ok as u32;
    };
    check(child(seed) == mine, "another process with the same seed builds the same match");
    check(child(seed + 1) != mine, "a different seed builds a different match");
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}

/// The match as text (exact float bits), and a one-line summary.
fn describe(seed: u64) -> (String, String) {
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    let mut app = App::new();
    app.insert_resource(MatchConfig { match_seed: Some(seed), bots: 15, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    app.update();
    let world = app.world_mut();
    let mut out = String::new();

    let m = world.resource::<Match>().clone();
    out += &format!("vimana {:?} -> {:?}\n", m.vimana_from, m.vimana_to);
    let mut names: Vec<String> = world.query::<&Warrior>().iter(world).map(|w| w.name.clone()).collect();
    names.sort();
    out += &format!("warriors {names:?}\n");

    let mut floor: Vec<String> = world
        .query::<(&Transform, &Pickup)>()
        .iter(world)
        .map(|(t, p)| format!("{:?} {} {:?} {}", t.translation, p.stack.id, p.stack.rarity, p.stack.count))
        .collect();
    floor.sort();
    out += &format!("floor loot ({}):\n  {}\n", floor.len(), floor.join("\n  "));

    let db = world.resource::<ItemDb>().clone();
    let mut chests: Vec<(usize, String)> = world
        .query::<&Chest>()
        .iter(world)
        .map(|c| {
            let items: Vec<String> = chest_contents(&db, m.seed, c.index, c.kind).iter().map(|s| format!("{} {:?} {}", s.id, s.rarity, s.count)).collect();
            (c.index, items.join(", "))
        })
        .collect();
    chests.sort();
    out += &format!("chests ({}):\n", chests.len());
    for (i, items) in &chests {
        out += &format!("  {i}: {items}\n");
    }

    // The whole battle's zone, a tick at a time as the sim runs it.
    let mut zone = world.resource::<Zone>().clone();
    let mut circles = vec![(zone.target_center, zone.target_radius)];
    for _ in 0..60 * 600 {
        let phase = zone.phase;
        zone.advance(1.0 / 60.0);
        if zone.phase != phase {
            circles.push((zone.target_center, zone.target_radius));
        }
    }
    out += &format!("zone circles {circles:?} final {:?} {}\n", zone.center, zone.radius);

    let summary = format!(
        "seed {seed}: {} warriors, {} floor pickups, {} chests, {} zone circles, first circle at ({:.1}, {:.1}) r {}",
        names.len(),
        floor.len(),
        chests.len(),
        circles.len(),
        circles[0].0.x,
        circles[0].0.y,
        circles[0].1
    );
    (out, summary)
}
