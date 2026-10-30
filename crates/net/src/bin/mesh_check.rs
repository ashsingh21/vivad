//! Several peers in one process, each with its own iroh endpoint on this machine: one hosts a
//! lobby, the rest join with its code, and the match is fought out by bots (players on autopilot)
//! over real connections. Checks that the lobby links everyone to everyone, that they build the
//! same roster with the players' names, that remote warriors follow their owners, and that every
//! peer agrees on who fell, the kills, the winner and the loot.
//!
//! cargo run --release -p net --bin mesh_check -- [peers] [bots] [max seconds] [speed]
//! (VIVAD_RELAY=1 to go through iroh's relays)

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use net::{Node, Session, Transport, TransportPlugin};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, Pickup};
use shared::sim::net::{NetId, Owner, PeerId, Remote};
use shared::sim::{Eliminated, Kinematic, MatchConfig, SimPlugin, Warrior};

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let seconds: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(420.0);
    // Sim seconds per wall second: the network runs in real time, so this sets how many ticks a
    // message can lag behind.
    let speed: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(4.0);
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();

    let started = Instant::now();
    // VIVAD_RELAY=1: go through iroh's relays and address lookup, as players across the internet would.
    let relay = std::env::var("VIVAD_RELAY").is_ok();
    let nodes: Vec<Node> = (0..n).map(|i| Node::bind(&format!("Player{}", i + 1), relay).expect("bind")).collect();
    let code = nodes[0].code();
    println!("join code: {} chars", code.len());
    let threads: Vec<_> = nodes
        .into_iter()
        .enumerate()
        .map(|(i, node)| {
            let code = code.clone();
            std::thread::spawn(move || if i == 0 { node.host(n, bots, 7) } else { node.join(&code) })
        })
        .collect();
    let sessions: Vec<Session> = threads.into_iter().map(|t| t.join().unwrap().expect("lobby")).collect();
    println!("lobby filled and meshed in {:.2}s", started.elapsed().as_secs_f32());

    let mut failed = 0;
    let mut check = |ok: bool, what: String| {
        println!("{} {what}", if ok { "PASS" } else { "FAIL" });
        failed += !ok as u32;
    };
    let seed = sessions[0].seed;
    check(
        sessions.iter().all(|s| s.seed == seed && s.map_seed == 7 && s.bots == bots && s.peers.all == sessions[0].peers.all),
        "every peer got the same seeds, bots and players".into(),
    );
    check(sessions.iter().all(|s| s.transport.linked() == n - 1), format!("every peer is linked to the {} others", n - 1));

    let ids: Vec<PeerId> = sessions.iter().map(|s| s.peers.local).collect();
    let mut apps: Vec<App> = sessions.into_iter().map(|s| peer(s, &items)).collect();

    let started = Instant::now();
    let mut tick = 0u32;
    let mut lag = (0.0f32, 0u32, 0usize);
    let mut ended_at = None;
    while tick < (seconds * 60.0) as u32 {
        for app in &mut apps {
            app.update();
        }
        tick += 1;
        // Pace to `speed` times real time.
        let due = Duration::from_secs_f32(tick as f32 / 60.0 / speed);
        if let Some(wait) = due.checked_sub(started.elapsed()) {
            std::thread::sleep(wait);
        }
        if tick.is_multiple_of(600) && tick >= 60 * 60 {
            let l = follow(&mut apps, speed);
            lag = (lag.0.max(l.0), lag.1 + l.1, lag.2 + l.2);
        }
        if apps.iter().all(|a| a.world().resource::<Match>().state == MatchState::Ended) {
            // A second more, for the last messages.
            let end = *ended_at.get_or_insert(tick);
            if tick > end + 60 {
                break;
            }
        }
    }
    let wall = started.elapsed().as_secs_f32();
    let sim = tick as f32 / 60.0;
    println!("{n} peers, {bots} bots, {sim:.0}s sim in {wall:.1}s wall");
    for (app, id) in apps.iter().zip(&ids) {
        let (sent, received) = app.world().resource::<Transport>().traffic();
        println!("  peer {id:016x}: sent {:.1} KB/s, received {:.1} KB/s (per sim second)", sent as f32 / sim / 1000.0, received as f32 / sim / 1000.0);
    }

    // Names agree for the bots; each player sees themself as "You".
    let rosters: Vec<BTreeMap<u32, (PeerId, bool, String)>> = apps
        .iter_mut()
        .map(|app| {
            let w = app.world_mut();
            w.query::<(&NetId, &Owner, &Warrior)>().iter(w).map(|(id, o, war)| (id.0, (o.0, war.is_bot, war.name.clone()))).collect()
        })
        .collect();
    let bots_only = |r: &BTreeMap<u32, (PeerId, bool, String)>| r.iter().map(|(i, (o, bot, name))| (*i, *o, *bot, if *bot { name.clone() } else { String::new() })).collect::<Vec<_>>();
    check(rosters.iter().all(|r| bots_only(r) == bots_only(&rosters[0])), format!("all peers build the same roster ({} warriors)", rosters[0].len()));
    let players: Vec<&str> = rosters[1].values().filter(|r| !r.1).map(|r| r.2.as_str()).collect();
    check(players.iter().filter(|p| p.starts_with("Player")).count() == n - 1, format!("players carry their names ({players:?} on peer 2)"));
    check(lag.2 > 0 && lag.1 == 0, format!("remote copies follow their owners ({} samples, worst {:.2} m, {} too far behind)", lag.2, lag.0, lag.1));

    let outcomes: Vec<_> = apps.iter_mut().map(|app| outcome(app.world_mut())).collect();
    let o = &outcomes[0];
    let total_kills: u32 = o.kills.values().sum();
    check(outcomes.iter().all(|o| o.ended), "the match ended on every peer".into());
    check(outcomes.iter().all(|x| x.fallen == o.fallen), format!("every peer agrees who fell ({} of {})", o.fallen.len(), rosters[0].len()));
    check(outcomes.iter().all(|x| x.kills == o.kills) && total_kills > 0, format!("every peer agrees on the kill counts ({total_kills} weapon kills)"));
    check(outcomes.iter().all(|x| x.winner == o.winner) && o.winner.is_some(), format!("every peer crowns the same winner ({:?})", o.winner));
    check(outcomes.iter().all(|x| x.opened == o.opened) && !o.opened.is_empty(), format!("every peer has the same chests open ({})", o.opened.len()));
    check(outcomes.iter().all(|x| x.pickups == o.pickups), format!("every peer has the same pickups lying about ({})", o.pickups.len()));
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}

fn peer(session: Session, items: &str) -> App {
    let mut app = App::new();
    app.insert_resource(MatchConfig { map_seed: session.map_seed, match_seed: Some(session.seed), bots: session.bots, autopilot: true, ..default() })
        .insert_resource(session.peers)
        .insert_resource(session.transport)
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items.to_string() })
        .add_plugins(TransportPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    app
}

/// How far each remote copy is from where its owner has the warrior: (worst metres, copies
/// further than a few ticks' travel at `speed`, copies sampled).
fn follow(apps: &mut [App], speed: f32) -> (f32, u32, usize) {
    let mut truth = BTreeMap::new();
    let mut seen = vec![];
    for app in apps.iter_mut() {
        let w = app.world_mut();
        for (id, t, k, remote) in w.query_filtered::<(&NetId, &Transform, &Kinematic, Has<Remote>), Without<Eliminated>>().iter(w) {
            if remote {
                seen.push((id.0, t.translation));
            } else {
                truth.insert(id.0, (t.translation, k.velocity));
            }
        }
    }
    let mut worst = 0.0f32;
    let mut behind = 0;
    let mut count = 0;
    for (id, pos) in seen {
        let Some((owned, vel)) = truth.get(&id) else { continue };
        let err = pos.distance(*owned);
        worst = worst.max(err);
        count += 1;
        // A localhost round trip is well under a millisecond: allow two ticks plus scheduling.
        behind += (err > vel.length() * (2.0 + speed) / 60.0 + 0.05) as u32;
    }
    (worst, behind, count)
}

struct Outcome {
    ended: bool,
    fallen: Vec<u32>,
    kills: BTreeMap<u32, u32>,
    winner: Option<u32>,
    opened: Vec<usize>,
    pickups: Vec<u64>,
}

fn outcome(w: &mut World) -> Outcome {
    let mut fallen: Vec<u32> = w.query_filtered::<&NetId, With<Eliminated>>().iter(w).map(|i| i.0).collect();
    fallen.sort();
    let kills = w.query::<(&NetId, &Warrior)>().iter(w).map(|(i, war)| (i.0, war.kills)).collect();
    let m = w.resource::<Match>().clone();
    let winner = m.winner.and_then(|e| w.get::<NetId>(e)).map(|i| i.0);
    let mut opened: Vec<usize> = w.query::<&Chest>().iter(w).filter(|c| c.opened).map(|c| c.index).collect();
    opened.sort();
    let mut pickups: Vec<u64> = w.query::<&Pickup>().iter(w).map(|p| p.id.0).collect();
    pickups.sort();
    Outcome { ended: m.state == MatchState::Ended, fallen, kills, winner, opened, pickups }
}
