//! Several peers in one process, each with its own iroh endpoint on this machine: one hosts a
//! lobby, the rest join with its code, and the match is fought out by bots (players on autopilot)
//! over real connections. Checks that the lobby links everyone to everyone, that they build the
//! same roster with the players' names, that remote warriors follow their owners, and that every
//! peer agrees on who fell, the kills, the winner and the loot. In the lobby one player joins and
//! leaves again and another crashes as the match is set; partway through, a peer crashes and the
//! rest fight on without it; then two players ask for a rematch at once.
//!
//! cargo run --release -p net --bin mesh_check -- [peers] [bots] [max seconds] [speed] [leave at seconds, 0 = never] [rematch seconds]
//! (VIVAD_RELAY=1 to go through iroh's relays)

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use net::{Node, Session, Transport, TransportPlugin};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::{Chest, Pickup};
use shared::sim::net::{NetId, Owner, PeerId, Peers, Remote};
use shared::sim::{Hold, Eliminated, Kinematic, MatchConfig, RestartMatch, SimPlugin, Warrior};

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let bots: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(6);
    let seconds: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(420.0);
    // Sim seconds per wall second: the network runs in real time, so this sets how many ticks a
    // message can lag behind.
    let speed: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(4.0);
    let leave_at: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let rematch: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();

    let mut failed = 0;
    let mut check = |ok: bool, what: String| {
        println!("{} {what}", if ok { "PASS" } else { "FAIL" });
        failed += !ok as u32;
    };
    let started = Instant::now();
    // VIVAD_RELAY=1: go through iroh's relays and address lookup, as players across the internet would.
    let relay = std::env::var("VIVAD_RELAY").is_ok();
    let mut nodes: Vec<Node> = (0..n).map(|i| Node::bind(&format!("Player{}", i + 1), relay).expect("bind")).collect();
    let code = nodes[0].code(shared::map::MapId::Cp, 7);
    println!("join code: {code} ({} chars)", code.len());
    let host = nodes.remove(0);
    // The host waits for one more than will play: a player who crashes as the match is set.
    let hosting = std::thread::spawn(move || host.host(n + 1, bots, 7));

    // Someone joins and leaves again before the start: the lobby must forget them.
    let quitter = Node::bind("Quitter", relay).expect("bind");
    let quitter_id = quitter.peer();
    let joined = std::sync::atomic::AtomicBool::new(false);
    let left = quitter.join_reporting(&code, &mut |_| joined.store(true, std::sync::atomic::Ordering::Relaxed), &mut || {
        !joined.load(std::sync::atomic::Ordering::Relaxed)
    });
    check(left.is_err() && joined.into_inner(), "a player can join the lobby and leave again".into());

    let crasher = Node::bind("Crasher", relay).expect("bind");
    let crasher_id = crasher.peer();
    let joining: Vec<_> = nodes
        .into_iter()
        .chain([crasher])
        .map(|node| {
            let code = code.clone();
            std::thread::spawn(move || node.join(&code))
        })
        .collect();
    let mut sessions: Vec<Session> = std::iter::once(hosting).chain(joining).map(|t| t.join().unwrap().expect("lobby")).collect();
    println!("lobby filled and meshed in {:.2}s", started.elapsed().as_secs_f32());
    check(sessions.iter().all(|s| !s.peers.all.contains(&quitter_id)), "the player who left the lobby is not in the match".into());
    // Gone before loading the match: the rest must start without waiting on them.
    sessions.pop().unwrap().transport.crash();

    let seed = sessions[0].seed;
    check(
        sessions.iter().all(|s| s.seed == seed && s.map_seed == 7 && s.bots == bots && s.peers.all == sessions[0].peers.all),
        "every peer got the same seeds, bots and players".into(),
    );
    check(sessions.iter().all(|s| s.transport.linked() == n), format!("every peer is linked to the {} others", n));

    let mut ids: Vec<PeerId> = sessions.iter().map(|s| s.peers.local).collect();
    let mut apps: Vec<App> = sessions.into_iter().map(|s| peer(s, &items)).collect();

    let started = Instant::now();
    let mut tick = 0u32;
    let mut lag = (0.0f32, 0u32, 0usize);
    let mut ended_at = None;
    let mut gone: Option<(PeerId, Instant)> = None;
    let mut noticed = None;
    let mut crash_tick = 0;
    let mut bots_back = None;
    let mut begun: Vec<Option<u32>> = vec![None; apps.len()];
    let mut jerk = Jerk::default();
    // VIVAD_NET_SIM's latency and jitter, in sim seconds, which remote copies may trail by.
    let sim_net: Vec<f32> = std::env::var("VIVAD_NET_SIM").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
    let lag_allowed = (sim_net.first().unwrap_or(&0.0) + sim_net.get(1).unwrap_or(&0.0)) / 1000.0 * speed;
    while tick < (seconds * 60.0) as u32 {
        for app in &mut apps {
            app.update();
        }
        tick += 1;
        for (b, app) in begun.iter_mut().zip(&apps) {
            if b.is_none() && !app.world().resource::<Hold>().0 {
                *b = Some(tick);
            }
        }
        if tick == 1 {
            check(apps.iter().all(|a| a.world().resource::<Hold>().0), "the match waits for everyone to load".into());
        }
        if leave_at > 0.0 && tick == (leave_at * 60.0) as u32 && apps.len() > 2 {
            // The last peer crashes: no goodbye, the rest must notice it has gone silent.
            let mut app = apps.pop().unwrap();
            app.world_mut().remove_resource::<Transport>().unwrap().crash();
            gone = ids.pop().map(|id| (id, Instant::now()));
            crash_tick = tick;
        }
        // Its bots stand still until someone takes them over.
        if let Some((id, _)) = gone
            && bots_back.is_none()
            && apps.iter_mut().all(|a| {
                let w = a.world_mut();
                !w.query_filtered::<(&Owner, &Warrior), Without<Eliminated>>().iter(w).any(|(o, war)| o.0 == id && war.is_bot)
            })
        {
            bots_back = Some((tick - crash_tick) as f32 / 60.0);
        }
        if let Some((id, at)) = gone
            && noticed.is_none()
            && apps.iter().all(|a| !a.world().resource::<Peers>().all.contains(&id))
        {
            noticed = Some(at.elapsed().as_secs_f32());
        }

        pace(started, tick, speed);
        if tick >= 60 * 60 {
            jerk.sample(&mut apps);
        }
        if tick.is_multiple_of(600) && tick >= 60 * 60 {
            let l = follow(&mut apps, speed, lag_allowed);
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
    let begun: Vec<u32> = begun.iter().map(|b| b.unwrap_or(u32::MAX)).collect();
    // The word to begin takes a message's flight (with VIVAD_NET_SIM, its latency and jitter).
    let flight = 2 + (lag_allowed * 60.0).ceil() as u32;
    check(
        begun.iter().max().unwrap() - begun.iter().min().unwrap() <= flight,
        format!("every peer's match began within {flight} ticks of the others (ticks {begun:?})"),
    );
    check(
        begun.iter().all(|b| *b < u32::MAX) && apps.iter().all(|a| !a.world().resource::<Peers>().all.contains(&crasher_id)),
        "the player who crashed before loading was dropped, and the match went on without them".into(),
    );
    let (owned_jerk, remote_jerk, pops) = jerk.report();
    println!("movement per tick, second difference: owners {owned_jerk:.1} mm, remote copies {remote_jerk:.1} mm; {pops} jumps of over a metre");
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

    if gone.is_some() {
        check(
            bots_back.is_some_and(|s| s < 1.5),
            format!("the crashed peer's bots were taken over {:.2} s (match time) after it went silent", bots_back.unwrap_or(f32::NAN)),
        );
        check(
            noticed.is_some_and(|s| s < 4.0),
            format!("every peer noticed the crashed peer had gone ({:.1} s after it went silent)", noticed.unwrap_or(f32::NAN)),
        );
    }
    let outcomes: Vec<_> = apps.iter_mut().map(|app| outcome(app.world_mut())).collect();
    let o = &outcomes[0];
    let total_kills: u32 = o.kills.values().sum();
    check(outcomes.iter().all(|o| o.ended), "the match ended on every peer".into());
    check(outcomes.iter().all(|x| x.fallen == o.fallen), format!("every peer agrees who fell ({} of {})", o.fallen.len(), rosters[0].len()));
    check(outcomes.iter().all(|x| x.kills == o.kills) && total_kills > 0, format!("every peer agrees on the kill counts ({total_kills} weapon kills)"));
    check(outcomes.iter().all(|x| x.winner == o.winner) && o.winner.is_some(), format!("every peer crowns the same winner ({:?})", o.winner));
    check(outcomes.iter().all(|x| x.opened == o.opened) && !o.opened.is_empty(), format!("every peer has the same chests open ({})", o.opened.len()));
    check(outcomes.iter().all(|x| x.pickups == o.pickups), format!("every peer has the same pickups lying about ({})", o.pickups.len()));

    if rematch <= 0.0 {
        println!("\n{failed} failed");
        std::process::exit(failed as i32);
    }
    // Two players ask for the next match on the same tick: everyone must move on to the same one,
    // with nothing from the last match leaking into it.
    // They voted for different maps on that same tick, too late to hear each other: the lower
    // peer's choice must win everywhere.
    let generation = apps[0].world().resource::<Match>().generation;
    for (app, map) in apps.iter_mut().take(2).zip([shared::map::MapId::IndiaGate, shared::map::MapId::HauzKhas]) {
        app.world_mut().write_message(shared::sim::flow::VoteMap(map));
        app.world_mut().write_message(RestartMatch);
    }
    let until = tick + (rematch * 60.0) as u32;
    while tick < until {
        for app in &mut apps {
            app.update();
        }
        tick += 1;
        pace(started, tick, speed);
    }
    let next: Vec<(u32, u64, &str)> = apps
        .iter()
        .map(|a| (a.world().resource::<Match>(), a.world().resource::<MatchConfig>().map))
        .map(|(m, map)| (m.generation, m.seed, map.name()))
        .collect();
    check(next.iter().all(|g| *g == next[0] && g.0 == generation + 1), format!("a double restart starts one next match everywhere, on one map ({next:?})"));
    let layouts: Vec<usize> = apps.iter().map(|a| a.world().resource::<shared::map::MapLayout>().footprints.len()).collect();
    check(layouts.iter().all(|n| *n == layouts[0]), format!("every peer has that map built ({layouts:?} buildings)"));
    let l = follow(&mut apps, speed, lag_allowed);
    check(l.2 > 0 && l.1 == 0, format!("in the rematch, remote copies follow their owners ({} samples, worst {:.2} m)", l.2, l.0));
    let outcomes: Vec<_> = apps.iter_mut().map(|app| outcome(app.world_mut())).collect();
    check(outcomes.iter().all(|x| x.fallen == outcomes[0].fallen), format!("in the rematch, every peer agrees who fell so far ({})", outcomes[0].fallen.len()));
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}

/// Hold the loop to `speed` times real time.
fn pace(started: Instant, tick: u32, speed: f32) {
    let due = Duration::from_secs_f32(tick as f32 / 60.0 / speed);
    if let Some(wait) = due.checked_sub(started.elapsed()) {
        std::thread::sleep(wait);
    }
}

fn peer(session: Session, items: &str) -> App {
    let mut app = App::new();
    app.insert_resource(MatchConfig { map_seed: session.map_seed, autopilot: true, online: true, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items.to_string() })
        .add_plugins(TransportPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    net::begin(app.world_mut(), session);
    app
}

/// How far each remote copy is from where its owner has the warrior: (worst metres, copies
/// further than a few ticks' travel at `speed`, copies sampled).
fn follow(apps: &mut [App], speed: f32, lag_allowed: f32) -> (f32, u32, usize) {
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
        // Over a slow link a warrior that has just turned or stopped is honestly a latency
        // behind at running pace.
        let pace = if lag_allowed > 0.0 { vel.length().max(7.0) } else { vel.length() };
        // A correction (a push out of a wall, a knock) is eased in over a few ticks, not snapped.
        const EASING: f32 = 0.25;
        behind += (err > pace * ((2.0 + speed) / 60.0 + lag_allowed) + EASING) as u32;
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

/// How smoothly warriors move: the second difference of position per tick, for the owners'
/// copies (the real motion) and the remote copies (what other players see), and jumps.
#[derive(Default)]
struct Jerk {
    last: BTreeMap<(usize, u32), (Vec3, Option<Vec3>)>,
    owned: (f64, u32),
    remote: (f64, u32),
    pops: u32,
}

impl Jerk {
    fn sample(&mut self, apps: &mut [App]) {
        for (i, app) in apps.iter_mut().enumerate() {
            let w = app.world_mut();
            for (id, t, remote) in w.query_filtered::<(&NetId, &Transform, Has<Remote>), Without<Eliminated>>().iter(w) {
                let p = t.translation;
                let key = (i, id.0);
                if let Some((p1, Some(p2))) = self.last.get(&key) {
                    let j = (p - 2.0 * *p1 + *p2).length() as f64 * 1000.0;
                    let acc = if remote { &mut self.remote } else { &mut self.owned };
                    acc.0 += j;
                    acc.1 += 1;
                    self.pops += (remote && p.distance(*p1) > 1.0) as u32;
                }
                let prev = self.last.get(&key).map(|l| l.0);
                self.last.insert(key, (p, prev));
            }
        }
    }

    fn report(&self) -> (f64, f64, u32) {
        (self.owned.0 / self.owned.1.max(1) as f64, self.remote.0 / self.remote.1.max(1) as f64, self.pops)
    }
}
