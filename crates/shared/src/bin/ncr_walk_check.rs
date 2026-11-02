//! Real physics walks across the one NCR map: the warrior's own capsule, gravity and
//! collide-and-slide on the map the game plays, with nothing teleported. A walker follows a list
//! of points; the check fails if it stalls, falls off, or does not end where it should.
//!
//!   cargo run --release -p shared --bin ncr_walk_check     (or `just ncr-walk`)
//!   ONLY=<word>   only the walks whose names hold this word.
//!   TRACE=<word>  print those walkers' position every 2 game seconds.
//!
//! The walks: every bridge in `compose::SPANS` both ways (out along the centre lane, back along
//! the side of the deck), the ridge by each road's pass and straight over the crest, a wade across
//! the Yamuna, Connaught Place to Chandni Chowk through the streets, and in through a district's
//! gate. Then a count, without walking: which districts can be reached from Connaught Place.
//!
//! The street walks find their own way from the map's solid geometry (`Streets`), keeping to the
//! roads as a person would, so they prove the map can be walked whatever the bots' router does.
use avian3d::prelude::*;
use bevy::{ecs::message::Messages, prelude::*, time::TimeUpdateStrategy};
use shared::{
    map::{MapId, MapLayout, Mat, Shape, compose, footprint, terrain::ground},
    sim::*,
};
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    time::{Duration, Instant},
};

const TICK: f64 = 1.0 / 60.0;
/// A corner of the way counts as passed this close to it.
const REACH: f32 = 3.0;
/// Seconds without getting closer before the walker sidesteps, as a person would.
const SNAG: f32 = 2.0;
/// Seconds without getting closer before it has failed.
const STUCK: f32 = 12.0;
/// The river's bed under the feet of a warrior wading.
const WATER_BED: f32 = -1.2;
/// Seconds a walk may take to go round things, on top of its length at about a sprint.
const SPARE_SECONDS: f32 = 30.0;

/// What a walk must prove, beyond arriving.
enum Proof {
    /// Over this bridge: on the deck the whole way, without a snag.
    Deck(usize),
    /// Over high ground: the feet get at least this high, without a snag.
    Climbs(f32),
    /// Through a pass: the feet stay under this, without a snag.
    StaysUnder(f32),
    /// Across a river on foot, slowed by the water, without a snag.
    Wades,
    /// Across this district's edge, near this gate, and on inside it.
    Gate { place: usize, gate: Vec2 },
    /// Through the streets from end to end, without a snag.
    Street,
}

struct Walk {
    name: String,
    way: Vec<Vec2>,
    proof: Proof,
}

/// What happened on a walk.
struct Run {
    arrived: bool,
    seconds: f32,
    snags: u32,
    end: Vec3,
    /// Highest and lowest the feet got.
    top: f32,
    low: f32,
    /// Lowest the feet got on the deck, and how much of the deck's length was walked on it.
    deck_low: f32,
    deck_share: f32,
    /// Fall damage taken.
    hurt: f32,
    /// How far inside the district's edge the walk ended, and how far from the gate it crossed.
    inside: f32,
    gate_miss: f32,
}

/// A bridge from end to end: by the roads to the deck, along one lane of it, and on past the far
/// end. The ramps are part of the walk.
fn span_way(streets: &Streets, s: &compose::Span, reverse: bool, lane: f32) -> Vec<Vec2> {
    let dir = (s.c - s.a).normalize();
    let side = dir.perp() * lane;
    let mut way = streets.approach(s.a - dir * 40.0 + side, s.a - dir * 8.0 + side);
    way.extend([s.a + side, s.c + side]);
    way.extend(streets.approach(s.c + dir * 40.0 + side, s.c + dir * 8.0 + side).into_iter().rev());
    if reverse { way.into_iter().rev().collect() } else { way }
}

/// Where a person can walk, on the terrain's samples, and what each sample costs: roads are
/// cheapest, so a way found here keeps to the streets. 0 is blocked.
struct Streets {
    n: usize,
    size: f32,
    step: f32,
    cost: Vec<u8>,
}

impl Streets {
    /// A walker needs this much clear of a wall, measured from the wall's face. Thin walls would
    /// slip between 2 m samples with less.
    const WALL_CLEAR: f32 = 1.3;
    /// And this much from a building's outline.
    const BUILDING_CLEAR: f32 = 0.6;

    fn new(map: &MapLayout) -> Self {
        let t = &map.terrain;
        let (n, step) = (t.n, t.step());
        let cost = t
            .ground
            .iter()
            .map(|g| match *g {
                ground::WATER => 0,
                ground::ROAD => 1,
                ground::PATH | ground::PLAZA | ground::PARKING => 2,
                ground::PARK => 3,
                _ => 4,
            })
            .collect();
        let mut s = Self { n, size: t.size, step, cost };
        for f in map.footprints.iter().filter(|f| f.mat != Mat::Water) {
            let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
            s.fill(lo - Vec2::splat(1.0), hi + Vec2::splat(1.0), |p| {
                footprint::contains(&f.pts, p) || f.pts.iter().any(|q| q.distance(p) < Self::BUILDING_CLEAR)
            });
        }
        // The market's tables stand in a walker's way as any block does.
        let tables = map.stalls.tables().map(|s| s.block());
        for b in map.blocks.iter().filter(|b| b.collide).cloned().chain(tables) {
            let half = match b.shape {
                Shape::Box(v) => v * 0.5,
                Shape::Cylinder { radius, height } => Vec3::new(radius, height / 2.0, radius),
                Shape::Sphere(r) => Vec3::splat(r),
            };
            // The bridges' decks and anything else overhead are walked under or over, not round;
            // the same goes for the ground's own slabs, a step's height at most.
            let floor = map.terrain.height(b.pos.x, b.pos.z);
            if b.pos.y - half.y > floor + 2.0 || b.pos.y + half.y < floor + 0.45 {
                continue;
            }
            let reach = half.xz().length() + Self::WALL_CLEAR;
            let (u, v) = (b.rot * Vec3::X, b.rot * Vec3::Z);
            s.fill(b.pos.xz() - Vec2::splat(reach), b.pos.xz() + Vec2::splat(reach), |p| {
                let d = (p - b.pos.xz()).extend(0.0).xzy();
                d.dot(u).abs() < half.x + Self::WALL_CLEAR && d.dot(v).abs() < half.z + Self::WALL_CLEAR
            });
        }
        for p in map.props.iter().filter(|p| p.trunk.is_some()) {
            let at = p.pos.xz();
            let clear = p.trunk.map_or(0.0, |t| t.0) + Self::BUILDING_CLEAR;
            s.fill(at - Vec2::splat(clear), at + Vec2::splat(clear), |q| q.distance(at) < clear);
        }
        // A deck is walked over the water, between its side walls.
        for d in &compose::SPANS {
            let dir = (d.c - d.a).normalize();
            let wide = d.width * 0.5 - Self::WALL_CLEAR - 0.5;
            let mut along = 0.0;
            while along <= d.a.distance(d.c) {
                let mut across = -wide;
                while across <= wide {
                    let p = d.a + dir * along + dir.perp() * across;
                    let k = s.sample(p.x) * n + s.sample(p.y);
                    s.cost[k] = 1;
                    across += step * 0.5;
                }
                along += step * 0.5;
            }
        }
        s
    }

    fn sample(&self, c: f32) -> usize {
        (((c + self.size * 0.5) / self.step).round().max(0.0) as usize).min(self.n - 1)
    }

    fn point(&self, k: usize) -> Vec2 {
        Vec2::new((k / self.n) as f32 * self.step, (k % self.n) as f32 * self.step) - Vec2::splat(self.size * 0.5)
    }

    fn fill(&mut self, lo: Vec2, hi: Vec2, solid: impl Fn(Vec2) -> bool) {
        for i in self.sample(lo.x)..=self.sample(hi.x) {
            for j in self.sample(lo.y)..=self.sample(hi.y) {
                let k = i * self.n + j;
                if self.cost[k] != 0 && solid(self.point(k)) {
                    self.cost[k] = 0;
                }
            }
        }
    }

    /// The nearest place a person can stand to a point, which may be inside a building.
    fn free_near(&self, p: Vec2) -> Vec2 {
        const SEARCH: i32 = 40;
        let (i, j) = (self.sample(p.x) as i32, self.sample(p.y) as i32);
        let offsets = (-SEARCH..=SEARCH).flat_map(|di| (-SEARCH..=SEARCH).map(move |dj| (di, dj)));
        offsets
            .map(|(di, dj)| (i + di, j + dj))
            .filter(|&(x, z)| x >= 0 && z >= 0 && x < self.n as i32 && z < self.n as i32 && self.cost[x as usize * self.n + z as usize] != 0)
            .map(|(x, z)| self.point(x as usize * self.n + z as usize))
            .min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)))
            .unwrap_or(p)
    }

    /// The way in to a bridge from a point near it: by the roads, or straight if there is none.
    fn approach(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let way = self.way(self.free_near(from), to);
        if way.is_empty() { vec![from, to] } else { way }
    }

    /// The cost of the cheapest way from one point to every sample, and each sample's step back.
    fn search(&self, start: usize, end: Option<usize>) -> (Vec<u32>, Vec<usize>) {
        let n = self.n;
        let mut best = vec![u32::MAX; n * n];
        let mut came = vec![usize::MAX; n * n];
        let mut heap = BinaryHeap::new();
        best[start] = 0;
        heap.push(Reverse((0u32, start)));
        while let Some(Reverse((c, k))) = heap.pop() {
            if Some(k) == end {
                break;
            }
            if c > best[k] {
                continue;
            }
            let (i, j) = (k / n, k % n);
            for (di, dj, far) in [(-1i32, 0i32, 10u32), (1, 0, 10), (0, -1, 10), (0, 1, 10), (-1, -1, 14), (-1, 1, 14), (1, -1, 14), (1, 1, 14)] {
                let (x, z) = (i as i32 + di, j as i32 + dj);
                if x < 0 || z < 0 || x >= n as i32 || z >= n as i32 {
                    continue;
                }
                let v = x as usize * n + z as usize;
                // A diagonal step may not cut a blocked corner.
                let squeeze = di != 0 && dj != 0 && (self.cost[x as usize * n + j] == 0 || self.cost[i * n + z as usize] == 0);
                if self.cost[v] == 0 || squeeze {
                    continue;
                }
                let next = c + far * self.cost[v] as u32;
                if next < best[v] {
                    best[v] = next;
                    came[v] = k;
                    heap.push(Reverse((next, v)));
                }
            }
        }
        (best, came)
    }

    /// The cheapest way between two points, first step first, ending on `to`. Empty if none.
    fn way(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let n = self.n;
        let (start, end) = (self.sample(from.x) * n + self.sample(from.y), self.sample(to.x) * n + self.sample(to.y));
        let (best, came) = self.search(start, Some(end));
        if best[end] == u32::MAX {
            return vec![];
        }
        let mut way = vec![to];
        let mut k = end;
        while k != start {
            way.push(self.point(k));
            k = came[k];
        }
        way.push(from);
        way.reverse();
        way
    }

    /// The districts nobody can walk to from `from` without wading.
    fn cut_off(&self, from: Vec2) -> Vec<&'static str> {
        let n = self.n;
        let (best, _) = self.search(self.sample(from.x) * n + self.sample(from.y), None);
        let reached = |p: Vec2| best[self.sample(p.x) * n + self.sample(p.y)] != u32::MAX;
        compose::PLACES.iter().filter(|d| !reached(self.free_near(d.at))).map(|d| d.name).collect()
    }
}

fn walks(streets: &Streets) -> Vec<Walk> {
    let mut all = vec![];
    for (k, s) in compose::SPANS.iter().enumerate() {
        let lane = s.width * 0.28;
        all.push(Walk { name: format!("bridge {} there", k + 1), way: span_way(streets, s, false, 0.0), proof: Proof::Deck(k) });
        all.push(Walk { name: format!("bridge {} back", k + 1), way: span_way(streets, s, true, lane), proof: Proof::Deck(k) });
    }
    let line = |pts: &[(f32, f32)]| pts.iter().map(|p| Vec2::new(p.0, p.1)).collect::<Vec<_>>();
    // The ridge: three roads are cut through it, and the crest between them can be climbed anywhere.
    all.push(Walk { name: "ridge pass NH-48".into(), way: line(&[(-1180.0, -100.0), (-1300.0, 230.0), (-1440.0, 560.0)]), proof: Proof::StaysUnder(4.0) });
    all.push(Walk { name: "ridge pass Outer Ring Road".into(), way: line(&[(-1440.0, 560.0), (-1230.0, 599.0)]), proof: Proof::StaysUnder(4.0) });
    all.push(Walk { name: "ridge pass MG Road".into(), way: line(&[(-900.0, 1163.0), (-1000.0, 1180.0), (-1290.0, 1240.0)]), proof: Proof::StaysUnder(4.0) });
    all.push(Walk { name: "ridge crest".into(), way: line(&[(-1040.0, -420.0), (-1380.0, -420.0)]), proof: Proof::Climbs(8.0) });
    // The river is a barrier nobody is trapped by: in at one bank and out at the other.
    all.push(Walk { name: "wade the Yamuna".into(), way: line(&[(-90.0, -330.0), (470.0, -330.0)]), proof: Proof::Wades });
    // Connaught Place's park to the middle of Chandni Chowk, by the streets.
    let place = |name: &str| compose::PLACES.iter().position(|p| p.name == name).expect("district");
    let (cp, chandni) = (place("Connaught Place"), place("Chandni Chowk"));
    all.push(Walk {
        name: "Connaught Place to Chandni Chowk".into(),
        way: streets.way(compose::PLACES[cp].at, compose::PLACES[chandni].at),
        proof: Proof::Street,
    });
    // In from open land by the link road off Kasturba Gandhi Marg, to Connaught Place's inner road.
    let gate = Vec2::new(-575.0, -450.0);
    let mut way = vec![Vec2::new(-568.0, -300.0), gate];
    way.extend(streets.way(gate, Vec2::new(-650.0, -590.0)));
    all.push(Walk { name: "gate into Connaught Place".into(), way, proof: Proof::Gate { place: cp, gate } });
    all
}

fn walk(app: &mut App, w: &Walk, trace: bool) -> Run {
    let map = app.world().resource::<MapLayout>();
    let start = w.way[0];
    let y = map.terrain.height(start.x, start.y) + HALF_HEIGHT + 0.05;
    let spawn = Vec3::new(start.x, y, start.y);
    let id = app
        .world_mut()
        .spawn((
            Transform::from_translation(spawn),
            Kinematic { mode: MoveMode::Ground, ..default() },
            Action::default(),
            PlayerIntent { move_axis: Vec2::Y, held: btn::SPRINT, ..default() },
            RigidBody::Kinematic,
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
            CollisionLayers::new(Layer::Character, [Layer::World, Layer::Door]),
        ))
        .id();
    let total: f32 = w.way.windows(2).map(|s| s[0].distance(s[1])).sum();
    let wade = if matches!(w.proof, Proof::Wades) { total / 2.0 } else { total / 5.0 };
    let budget = wade + SPARE_SECONDS;
    let span = match w.proof {
        Proof::Deck(k) => Some(&compose::SPANS[k]),
        _ => None,
    };
    let mut run = Run { arrived: false, seconds: 0.0, snags: 0, end: spawn, top: f32::MIN, low: f32::MAX, deck_low: f32::MAX, deck_share: 0.0, hurt: 0.0, inside: 0.0, gate_miss: f32::MAX };
    let mut next = 1;
    let (mut best, mut last_gain) = (f32::MAX, 0.0f32);
    // The side of a sidestep flips each snag; while `dodge` runs the walker is sidestepping.
    let (mut side, mut dodge) = (1.0f32, 0.0f32);
    let mut deck_seen: Vec<bool> = vec![false; 40];
    let mut was_inside = None;
    let ticks = (budget * 60.0) as usize;
    for tick in 0..ticks {
        let t = tick as f32 / 60.0;
        let pos = app.world().get::<Transform>(id).expect("walker").translation;
        let p = pos.xz();
        while next + 1 < w.way.len() && p.distance(w.way[next]) < REACH {
            next += 1;
        }
        let left = p.distance(w.way[next]) + w.way[next..].windows(2).map(|s| s[0].distance(s[1])).sum::<f32>();
        if left < REACH {
            run.arrived = true;
            break;
        }
        if left < best - 0.5 {
            best = left;
            last_gain = t;
        }
        let waited = t - last_gain;
        if waited > STUCK {
            break;
        }
        if waited > SNAG && dodge <= 0.0 {
            run.snags += 1;
            side = -side;
            dodge = 1.2;
            println!("  snag at {:.0}, {:.1}, {:.0}", pos.x, pos.y - HALF_HEIGHT, pos.z);
            if run.snags == 1 {
                nearby(app.world().resource::<MapLayout>(), pos);
            }
        }
        let d = (w.way[next] - p).normalize_or_zero();
        {
            let mut intent = app.world_mut().get_mut::<PlayerIntent>(id).expect("walker");
            intent.yaw = (-d.x).atan2(-d.y);
            if dodge > 0.0 {
                dodge -= TICK as f32;
                intent.move_axis = Vec2::new(side, 0.3);
                if tick % 40 == 0 {
                    intent.pressed |= btn::JUMP;
                }
            } else {
                intent.move_axis = Vec2::Y;
            }
        }
        app.update();
        let pos = app.world().get::<Transform>(id).expect("walker").translation;
        let feet = pos.y - HALF_HEIGHT;
        run.top = run.top.max(feet);
        run.low = run.low.min(feet);
        run.end = pos;
        run.seconds = t;
        run.hurt += app.world_mut().resource_mut::<Messages<DamageMsg>>().drain().filter(|m| m.target == id).map(|m| m.amount).sum::<f32>();
        if let Some(s) = span {
            // Its height, while over the deck: not the first or last metres, where the approach
            // meets it, and not when past its side.
            let dir = (s.c - s.a).normalize();
            let along = (pos.xz() - s.a).dot(dir) / s.a.distance(s.c);
            let off = (pos.xz() - s.a).dot(dir.perp()).abs();
            if (0.01..0.99).contains(&along) && off < s.width * 0.5 {
                run.deck_low = run.deck_low.min(feet);
                if feet > s.top - 0.5 {
                    deck_seen[(along * 40.0) as usize] = true;
                }
            }
        }
        if let Proof::Gate { place, gate } = w.proof {
            let d = &compose::PLACES[place];
            let inside = d.clip - (pos.xz() - d.at).abs().max_element();
            if was_inside.is_some_and(|w: f32| w < 0.0) && inside >= 0.0 {
                run.gate_miss = pos.xz().distance(gate);
            }
            was_inside = Some(inside);
            run.inside = inside;
        }
        if trace && tick % 120 == 0 {
            let kin = app.world().get::<Kinematic>(id).expect("walker");
            println!("  t {:>4.0} at {:.1} vel {:.1} grounded {} left {:.0}", t, pos, kin.velocity, kin.grounded, left);
        }
    }
    run.deck_share = deck_seen.iter().filter(|s| **s).count() as f32 / deck_seen.len() as f32;
    app.world_mut().despawn(id);
    run
}

/// Why a walk failed, if it did.
fn verdict(w: &Walk, r: &Run) -> Option<String> {
    if !r.arrived {
        return Some(format!("did not arrive, {} snags", r.snags));
    }
    if r.hurt > 0.0 {
        return Some(format!("took {:.0} fall damage", r.hurt));
    }
    match w.proof {
        Proof::Deck(k) => {
            let s = &compose::SPANS[k];
            if r.snags > 0 {
                return Some(format!("{} snags", r.snags));
            }
            if r.deck_low < s.top - 0.5 || r.deck_share < 0.95 {
                return Some(format!("left the deck: lowest {:.2} on a {:.1} deck, {:.0} % of it walked", r.deck_low, s.top, r.deck_share * 100.0));
            }
        }
        Proof::Climbs(h) => {
            if r.top < h {
                return Some(format!("only reached {:.1} m", r.top));
            }
            if r.snags > 0 {
                return Some(format!("{} snags", r.snags));
            }
        }
        Proof::StaysUnder(h) => {
            if r.top > h {
                return Some(format!("climbed to {:.1} m", r.top));
            }
            if r.snags > 0 {
                return Some(format!("{} snags", r.snags));
            }
        }
        Proof::Wades => {
            if r.low > WATER_BED + 0.2 {
                return Some(format!("never in the river: lowest {:.1} m", r.low));
            }
            if r.snags > 0 {
                return Some(format!("{} snags", r.snags));
            }
        }
        Proof::Gate { .. } => {
            if r.gate_miss > 20.0 {
                return Some(format!("crossed the edge {:.0} m from the gate", r.gate_miss));
            }
            if r.inside < 40.0 {
                return Some(format!("ended only {:.0} m inside", r.inside));
            }
            if r.snags > 0 {
                return Some(format!("{} snags", r.snags));
            }
        }
        Proof::Street => {
            if r.snags > 0 {
                return Some(format!("{} snags", r.snags));
            }
        }
    }
    None
}

/// The solid things touching a point, to say what a walker ran into.
fn nearby(map: &MapLayout, pos: Vec3) {
    for b in map.blocks.iter().filter(|b| b.collide) {
        if let Shape::Box(size) = b.shape {
            let q = b.rot.inverse() * (pos - b.pos);
            if q.x.abs() < size.x * 0.5 + 1.0 && q.z.abs() < size.z * 0.5 + 1.0 && q.y.abs() < size.y * 0.5 + 1.0 {
                println!("  nearby solid {:?} at {:.1} size {:.1}", b.mat, b.pos, size);
            }
        }
    }
    for p in map.props.iter().filter(|p| p.trunk.is_some() && p.pos.xz().distance(pos.xz()) < 2.0) {
        println!("  nearby tree at {:.1}", p.pos);
    }
    if compose::built(map, pos.xz()) {
        println!("  inside a building's outline");
    }
    println!("  ground class {:?}, terrain {:.2}", map.terrain.ground_at(pos.x, pos.z), map.terrain.height(pos.x, pos.z));
    if map.terrain.ground_at(pos.x, pos.z) == Some(ground::WATER) {
        println!("  in the water");
    }
}

fn main() {
    let mut app = App::new();
    app.insert_resource(MatchConfig { map: MapId::Ncr, bots: 0, online: true, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: include_str!("../../../../assets/data/items.ron").into() })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(TICK)));
    app.finish();
    app.cleanup();
    let clock = Instant::now();
    for _ in 0..3 {
        app.update();
    }
    println!("map ready in {:.1} s", clock.elapsed().as_secs_f32());
    let only = std::env::var("ONLY").unwrap_or_default();
    let trace = std::env::var("TRACE").unwrap_or_default();
    let map = app.world().resource::<MapLayout>();
    // How far each bridge's deck stands over the ground at its two ends: a warrior jumps 1.3 m.
    for (k, s) in compose::SPANS.iter().enumerate() {
        let dir = (s.c - s.a).normalize();
        let step = |p: Vec2| s.top - map.terrain.height(p.x, p.y);
        println!("bridge {}: deck {:.1} m, step up at its ends {:.2} and {:.2} m", k + 1, s.top, step(s.a - dir), step(s.c + dir));
    }
    let streets = Streets::new(map);
    let all = walks(&streets);
    let (mut failed, mut ran, mut game_seconds) = (0, 0, 0.0);
    for w in all.iter().filter(|w| w.name.contains(&only)) {
        ran += 1;
        if w.way.len() < 2 {
            println!("FAIL {}: no way through the map's solid geometry", w.name);
            failed += 1;
            continue;
        }
        let length: f32 = w.way.windows(2).map(|s| s[0].distance(s[1])).sum();
        let r = walk(&mut app, w, !trace.is_empty() && w.name.contains(&trace));
        game_seconds += r.seconds;
        let why = verdict(w, &r);
        println!(
            "{} {}: {:.0} m in {:.0} s, feet {:.1} to {:.1}, {} snags, ended at {:.0}, {:.0}{}",
            if why.is_none() { "PASS" } else { "FAIL" },
            w.name,
            length,
            r.seconds,
            r.low,
            r.top,
            r.snags,
            r.end.x,
            r.end.z,
            why.as_ref().map(|s| format!(" -- {s}")).unwrap_or_default(),
        );
        if why.is_some() {
            nearby(app.world().resource::<MapLayout>(), r.end);
            failed += 1;
        }
    }
    // Who can be reached at all, from the middle of the map's busiest place.
    if only.is_empty() {
        let lost = streets.cut_off(compose::PLACES[0].at);
        println!("{} walked to from Connaught Place, cut off: {lost:?}", compose::PLACES.len() - lost.len());
        failed += lost.len();
    }
    let m = app.world().resource::<MapLayout>();
    println!(
        "{ran} walks, {failed} failed; {game_seconds:.0} game seconds in {:.0} s; {} blocks, {} footprints, {} places",
        clock.elapsed().as_secs_f32(),
        m.blocks.len(),
        m.footprints.len(),
        m.pois.len()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}
