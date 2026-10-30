//! Several peers in one process, joined by an in-memory pipe instead of the network: each tick
//! every peer's `Outbox` is delivered to every other peer's `Inbox`. Checks that they agree on
//! the roster, that each warrior is run by exactly one peer, and that every peer sees every
//! warrior where its owner has it. Local players are on autopilot.
//!
//! cargo run --release -p shared --bin net_check -- [peers] [bots] [seconds]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::sim::bots::BotBrain;
use shared::sim::net::{Inbox, NetId, Outbox, Owner, PeerId, Peers, Remote};
use shared::sim::{Eliminated, Kinematic, MatchConfig, SimPlugin, Warrior};

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let seconds: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    // Unordered ids, as real endpoint keys would be.
    let ids: Vec<PeerId> = (0..n as u64).map(|i| (i * 0x9E37_79B9 + 17) % 1000).collect();

    let mut apps: Vec<App> = ids
        .iter()
        .map(|&id| {
            let mut app = App::new();
            app.insert_resource(MatchConfig { match_seed: Some(99), bots, autopilot: true, ..default() })
                .insert_resource(Peers::new(id, ids.clone()))
                .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
                .add_plugins(PhysicsPlugins::default())
                .add_plugins(SimPlugin { items_ron: items.clone() })
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
            app.finish();
            app.cleanup();
            app
        })
        .collect();

    let started = Instant::now();
    let ticks = (seconds * 60.0) as u32;
    let mut sent = 0usize;
    for _ in 0..ticks {
        for app in &mut apps {
            app.update();
        }
        deliver(&mut apps, &ids, &mut sent);
    }
    println!("{n} peers, {bots} bots, {seconds}s sim in {:.1}s wall, {sent} messages delivered", started.elapsed().as_secs_f32());

    let mut failed = 0;
    let mut check = |ok: bool, what: String| {
        println!("{} {what}", if ok { "PASS" } else { "FAIL" });
        failed += !ok as u32;
    };

    // Roster: the same names and owners everywhere (names differ only for "You").
    let rosters: Vec<BTreeMap<u32, (PeerId, bool)>> = apps
        .iter_mut()
        .map(|app| {
            let w = app.world_mut();
            w.query::<(&NetId, &Owner, &Warrior)>().iter(w).map(|(id, o, war)| (id.0, (o.0, war.is_bot))).collect()
        })
        .collect();
    check(rosters.iter().all(|r| *r == rosters[0]), format!("all peers build the same roster ({} warriors)", rosters[0].len()));

    // Exactly one simulating copy of each warrior: on its owner, without `Remote`.
    let mut runners: BTreeMap<u32, Vec<PeerId>> = BTreeMap::new();
    let mut brains: BTreeMap<u32, usize> = BTreeMap::new();
    for (app, &me) in apps.iter_mut().zip(&ids) {
        let w = app.world_mut();
        for (id, owner, remote, brain) in w.query::<(&NetId, &Owner, Has<Remote>, Has<BotBrain>)>().iter(w) {
            if !remote {
                runners.entry(id.0).or_default().push(me);
                assert_eq!(owner.0, me);
            }
            *brains.entry(id.0).or_default() += brain as usize;
        }
    }
    check(runners.values().all(|r| r.len() == 1), "each warrior is simulated by exactly one peer".into());
    check(brains.values().all(|&b| b == 1), "each warrior has one brain in the whole match".into());
    let per_peer: Vec<usize> = ids.iter().map(|p| runners.values().filter(|r| r[0] == *p).count()).collect();
    check(per_peer.iter().max().unwrap() - per_peer.iter().min().unwrap() <= 1, format!("bots are dealt evenly: {per_peer:?} warriors per peer"));

    // Everyone sees each live warrior where its owner has it (a tick behind at most).
    let truth: BTreeMap<u32, (Vec3, Vec3)> = apps
        .iter_mut()
        .flat_map(|app| {
            let w = app.world_mut();
            w.query_filtered::<(&NetId, &Transform, &Kinematic), (Without<Remote>, Without<Eliminated>)>()
                .iter(w)
                .map(|(id, t, k)| (id.0, (t.translation, k.velocity)))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut worst = (0.0f32, 0u32);
    let mut late = 0;
    for app in &mut apps {
        let w = app.world_mut();
        for (id, t) in w.query_filtered::<(&NetId, &Transform), With<Remote>>().iter(w) {
            let Some((pos, vel)) = truth.get(&id.0) else { continue };
            let err = t.translation.distance(*pos);
            if err > worst.0 {
                worst = (err, id.0);
            }
            late += (err > vel.length() * 2.0 / 60.0 + 0.05) as u32;
        }
    }
    check(late == 0, format!("remote copies follow their owners (worst {:.2} m on warrior {})", worst.0, worst.1));
    let moved = truth.values().filter(|(p, _)| p.y < 100.0).count();
    check(moved == truth.len(), format!("{moved}/{} live warriors left the vimana and landed", truth.len()));
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}

/// The pipe: everything each peer sent this tick reaches every other peer.
fn deliver(apps: &mut [App], ids: &[PeerId], sent: &mut usize) {
    let out: Vec<_> = apps.iter_mut().map(|a| std::mem::take(&mut a.world_mut().resource_mut::<Outbox>().0)).collect();
    for (to, app) in apps.iter_mut().enumerate() {
        let mut inbox = app.world_mut().resource_mut::<Inbox>();
        for (from, msgs) in out.iter().enumerate() {
            if from != to {
                inbox.0.extend(msgs.iter().cloned().map(|m| (ids[from], m)));
                *sent += msgs.len();
            }
        }
    }
}
