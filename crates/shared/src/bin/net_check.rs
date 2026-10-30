//! Several peers in one process, joined by an in-memory pipe instead of the network: each tick
//! every peer's `Outbox` is delivered to every other peer's `Inbox`. Checks that they agree on
//! the roster, that each warrior is run by exactly one peer, that every peer sees every warrior
//! where its owner has it, and, once the match is fought out (local players on autopilot), on who
//! fell, the kill counts and the winner. Partway through, the last peer leaves: the others must
//! take its bots over and fight on to the same end.
//!
//! cargo run --release -p shared --bin net_check -- [peers] [bots] [max seconds] [leave at seconds, 0 = never]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::sim::bots::BotBrain;
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, Pickup};
use shared::sim::net::{Departures, Inbox, NetId, Outbox, Owner, PeerId, Peers, Remote};
use shared::sim::{Eliminated, Inventory, Kinematic, MatchConfig, MoveMode, PlayerIntent, SimPlugin, Warrior, btn};

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let seconds: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(420.0);
    let leave_at: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    // Unordered ids, as real endpoint keys would be.
    let ids: Vec<PeerId> = (0..n as u64).map(|i| (i * 0x9E37_79B9 + 17) % 1000).collect();

    let mut apps: Vec<App> = ids.iter().map(|&id| peer(id, &ids, bots, true, &items)).collect();
    let mut live = ids.clone();
    let mut leaver: Option<(PeerId, Vec<u32>)> = None;
    let mut after_leave = None;

    let started = Instant::now();
    let mut sent = 0usize;
    let mut tick = 0;
    let mut mid_match = None;
    // Peer 1's game stalls for two seconds (its connection stays up): its bots must pass to their
    // heirs, and it must take that when it comes back.
    let stall = (45 * 60, 47 * 60);
    let owned_by = |app: &mut App, p: PeerId| {
        let w = app.world_mut();
        w.query_filtered::<(&Owner, &Warrior), Without<Eliminated>>().iter(w).filter(|(o, war)| o.0 == p && war.is_bot).count()
    };
    let mut stalled_bots = (0, 0);
    for app in &mut apps {
        app.update();
    }
    let dealt: Vec<usize> = {
        let w = apps[0].world_mut();
        let owners: Vec<PeerId> = w.query::<&Owner>().iter(w).map(|o| o.0).collect();
        ids.iter().map(|p| owners.iter().filter(|o| *o == p).count()).collect()
    };
    while tick < (seconds * 60.0) as u32 {
        if tick == stall.0 {
            stalled_bots.0 = owned_by(&mut apps[0], ids[1]);
        }
        for (i, app) in apps.iter_mut().enumerate() {
            if !(i == 1 && (stall.0..stall.1).contains(&tick)) {
                app.update();
            }
        }
        deliver(&mut apps, &live, &mut sent);
        tick += 1;
        if tick == stall.1 + 60 {
            stalled_bots.1 = owned_by(&mut apps[0], ids[1]);
        }
        // Where everyone is, sampled once the warriors have landed.
        if tick == 60 * 60 {
            mid_match = Some(positions(&mut apps));
        }
        if leave_at > 0.0 && tick == (leave_at * 60.0) as u32 && apps.len() > 1 {
            let mut gone = apps.pop().unwrap();
            let id = live.pop().unwrap();
            let w = gone.world_mut();
            let owned = w.query::<(&NetId, &Owner)>().iter(w).filter(|(_, o)| o.0 == id).map(|(i, _)| i.0).collect();
            for app in &mut apps {
                app.world_mut().resource_mut::<Departures>().0.push(id);
            }
            leaver = Some((id, owned));
        }
        if leave_at > 0.0 && tick == ((leave_at + 30.0) * 60.0) as u32 {
            after_leave = Some(positions(&mut apps));
        }
        if apps.iter().all(|a| a.world().resource::<Match>().state == MatchState::Ended) {
            break;
        }
    }
    // Let the last messages land.
    for _ in 0..2 {
        for app in &mut apps {
            app.update();
        }
        deliver(&mut apps, &live, &mut sent);
    }
    println!(
        "{n} peers, {bots} bots, {:.0}s sim in {:.1}s wall, {sent} messages delivered",
        tick as f32 / 60.0,
        started.elapsed().as_secs_f32()
    );

    let mut failed = 0;
    let mut check = |ok: bool, what: String| {
        println!("{} {what}", if ok { "PASS" } else { "FAIL" });
        failed += !ok as u32;
    };
    check(
        stalled_bots.0 > 0 && stalled_bots.1 == 0,
        format!("a stalled peer's bots pass to their heirs ({} live bots before the stall, {} after)", stalled_bots.0, stalled_bots.1),
    );

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
    for (app, &me) in apps.iter_mut().zip(&live) {
        let w = app.world_mut();
        for (id, owner, remote, brain) in w.query::<(&NetId, &Owner, Has<Remote>, Has<BotBrain>)>().iter(w) {
            if !remote {
                runners.entry(id.0).or_default().push(me);
                assert_eq!(owner.0, me);
            }
            *brains.entry(id.0).or_default() += brain as usize;
        }
    }
    // The leaver's own warrior is left to no one.
    let gone_player = |id: &u32| leaver.as_ref().is_some_and(|(_, owned)| owned.contains(id) && !rosters[0][id].1);
    check(
        rosters[0].keys().all(|id| runners.get(id).map_or(0, Vec::len) == if gone_player(id) { 0 } else { 1 }),
        "each warrior is simulated by exactly one peer".into(),
    );
    check(
        rosters[0].keys().all(|id| brains.get(id).copied().unwrap_or(0) == if gone_player(id) { 0 } else { 1 }),
        "each warrior still playing has one brain (players on autopilot)".into(),
    );
    // At the start (leaving and stalling move bots on later).
    check(dealt.iter().max().unwrap() - dealt.iter().min().unwrap() <= 1, format!("bots are dealt evenly: {dealt:?} warriors per peer"));

    // A minute in (and after the leave): everyone sees each live warrior where its owner has it.
    let (truth, _) = mid_match.clone().expect("the match lasted a minute");
    for (when, sample) in [("a minute in", mid_match), ("after the leave", after_leave)] {
        let Some((truth, seen)) = sample else { continue };
        let mut worst = (0.0f32, 0u32);
        let mut late = 0;
        for (id, pos) in &seen {
            let Some((owned, vel)) = truth.get(id) else { continue };
            let err = pos.distance(*owned);
            if err > worst.0 {
                worst = (err, *id);
            }
            // Corrections (a push out of a wall, a knock) are eased in over a few ticks.
            late += (err > vel.length() * 2.0 / 60.0 + 0.1) as u32;
        }
        check(late == 0, format!("{when}, remote copies follow their owners (worst {:.2} m on warrior {}, {} copies)", worst.0, worst.1, seen.len()));
    }
    let landed = truth.values().filter(|(p, _)| p.y < 100.0).count();
    check(landed == truth.len(), format!("{landed}/{} live warriors left the vimana and landed", truth.len()));

    // The fight, as each peer saw it.
    let outcomes: Vec<(Vec<u32>, BTreeMap<u32, u32>, Option<u32>, MatchState)> = apps
        .iter_mut()
        .map(|app| {
            let w = app.world_mut();
            let mut fallen: Vec<u32> = w.query_filtered::<&NetId, With<Eliminated>>().iter(w).map(|i| i.0).collect();
            fallen.sort();
            let kills = w.query::<(&NetId, &Warrior)>().iter(w).map(|(i, war)| (i.0, war.kills)).collect();
            let m = w.resource::<Match>().clone();
            let winner = m.winner.and_then(|e| w.get::<NetId>(e)).map(|i| i.0);
            (fallen, kills, winner, m.state)
        })
        .collect();
    let (fallen, kills, winner, _) = &outcomes[0];
    let total_kills: u32 = kills.values().sum();
    check(outcomes.iter().all(|o| o.3 == MatchState::Ended), "the match ended on every peer".into());
    check(outcomes.iter().all(|o| o.0 == *fallen), format!("every peer agrees who fell ({} of {})", fallen.len(), rosters[0].len()));
    check(outcomes.iter().all(|o| o.1 == *kills), format!("every peer agrees on the kill counts ({total_kills} weapon kills)"));
    check(total_kills > 0, "warriors killed each other across peers".into());
    check(outcomes.iter().all(|o| o.2 == *winner) && winner.is_some(), format!("every peer crowns the same winner ({winner:?})"));

    // The loot, as each peer has it.
    let loot: Vec<(Vec<u64>, Vec<usize>)> = apps
        .iter_mut()
        .map(|app| {
            let w = app.world_mut();
            let mut pickups: Vec<u64> = w.query::<&Pickup>().iter(w).map(|p| p.id.0).collect();
            pickups.sort();
            let mut opened: Vec<usize> = w.query::<&Chest>().iter(w).filter(|c| c.opened).map(|c| c.index).collect();
            opened.sort();
            (pickups, opened)
        })
        .collect();
    check(loot.iter().all(|l| l.1 == loot[0].1), format!("every peer has the same chests open ({})", loot[0].1.len()));
    check(loot.iter().all(|l| l.0 == loot[0].0), format!("every peer has the same pickups lying about ({})", loot[0].0.len()));
    check(!loot[0].1.is_empty(), "chests were opened".into());

    // Two players grab the same item on the same tick: exactly one keeps it.
    let pair = [ids[0], ids[1]];
    let mut duel: Vec<App> = pair.iter().map(|&id| peer(id, &pair, 0, false, &items)).collect();
    for app in &mut duel {
        app.update();
    }
    let (item, at) = {
        let w = duel[0].world_mut();
        let p = w.query::<(&Pickup, &Transform)>().iter(w).find(|(p, _)| p.stack.id != "arrows").map(|(p, t)| (p.clone(), t.translation)).unwrap();
        p
    };
    for app in &mut duel {
        let w = app.world_mut();
        let mut q = w.query_filtered::<(&mut Transform, &mut Kinematic, &mut PlayerIntent), Without<Remote>>();
        let (mut t, mut kin, mut intent) = q.single_mut(w).unwrap();
        // The same spot on both: each peer only collides its own warrior with the world.
        t.translation = at + Vec3::Y * 1.0;
        kin.mode = MoveMode::Ground;
        intent.pressed = btn::INTERACT;
    }
    for _ in 0..3 {
        for app in &mut duel {
            app.update();
        }
        deliver(&mut duel, &pair, &mut sent);
    }
    let holders: Vec<bool> = duel
        .iter_mut()
        .map(|app| {
            let w = app.world_mut();
            let mut q = w.query_filtered::<&Inventory, Without<Remote>>();
            q.single(w).unwrap().slots.iter().flatten().any(|s| s.id == item.stack.id)
        })
        .collect();
    let left = duel
        .iter_mut()
        .filter_map(|app| {
            let w = app.world_mut();
            w.query::<&Pickup>().iter(w).any(|p| p.id == item.id).then_some(())
        })
        .count();
    check(left == 0 && holders.iter().filter(|h| **h).count() == 1, format!("a contested {} ends up with one player ({holders:?})", item.stack.id));
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}

fn peer(id: PeerId, ids: &[PeerId], bots: u32, autopilot: bool, items: &str) -> App {
    let mut app = App::new();
    app.insert_resource(MatchConfig { match_seed: Some(99), bots, autopilot, ..default() })
        .insert_resource(Peers::new(id, ids.to_vec()))
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items.to_string() })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    app
}

/// Each live warrior's position and velocity on its owner, and every remote copy's position.
#[allow(clippy::type_complexity)]
fn positions(apps: &mut [App]) -> (BTreeMap<u32, (Vec3, Vec3)>, Vec<(u32, Vec3)>) {
    let mut truth = BTreeMap::new();
    let mut seen = vec![];
    for app in apps {
        let w = app.world_mut();
        for (id, t, k, remote) in w.query_filtered::<(&NetId, &Transform, &Kinematic, Has<Remote>), Without<Eliminated>>().iter(w) {
            if remote {
                seen.push((id.0, t.translation));
            } else {
                truth.insert(id.0, (t.translation, k.velocity));
            }
        }
    }
    (truth, seen)
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
