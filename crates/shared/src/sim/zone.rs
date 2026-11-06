//! The Pralaya: a closing ring of cosmic fire. Outside it, warriors burn.

use bevy::prelude::*;
use rand::RngExt;

use super::{DamageCause, DamageMsg, Eliminated, SimSet, Warrior, match_rng, stream};
use super::flow::{Match, MatchState};
use crate::SmallRng;
use crate::map::MapLayout;
use crate::map::compose::{self, SPANS};
use crate::map::terrain::ground;

/// One phase of the ring: wait seconds, shrink seconds, end radius, damage per second outside.
pub type Phase = (f32, f32, f32, f32);

/// The small maps' ring.
pub const PHASES: [Phase; 6] = [
    (50.0, 35.0, 190.0, 1.0),
    (40.0, 30.0, 115.0, 2.0),
    (35.0, 25.0, 60.0, 4.0),
    (25.0, 20.0, 28.0, 7.0),
    (20.0, 20.0, 10.0, 10.0),
    (15.0, 25.0, 0.0, 14.0),
];
pub const START_RADIUS: f32 = 360.0;

/// The NCR's rings (from the game-playable branch), in real metres and seconds, by the size of the
/// match: from the first wait to the last shrink they run 10:50, 14:30 and 19:50.
pub const NCR_16: [Phase; 6] = [
    (110.0, 90.0, 550.0, 1.0),
    (70.0, 70.0, 300.0, 2.0),
    (60.0, 50.0, 150.0, 4.0),
    (45.0, 40.0, 70.0, 7.0),
    (30.0, 35.0, 25.0, 10.0),
    (20.0, 30.0, 0.0, 14.0),
];
pub const NCR_32: [Phase; 6] = [
    (140.0, 130.0, 800.0, 1.0),
    (90.0, 90.0, 450.0, 2.0),
    (75.0, 70.0, 230.0, 4.0),
    (55.0, 55.0, 110.0, 7.0),
    (40.0, 45.0, 45.0, 10.0),
    (30.0, 50.0, 0.0, 14.0),
];
pub const NCR_64: [Phase; 7] = [
    (180.0, 180.0, 1400.0, 1.0),
    (100.0, 120.0, 800.0, 2.0),
    (90.0, 90.0, 450.0, 4.0),
    (70.0, 70.0, 230.0, 7.0),
    (50.0, 60.0, 110.0, 10.0),
    (40.0, 50.0, 45.0, 14.0),
    (30.0, 60.0, 0.0, 14.0),
];

/// Where a match of 16 opens on the NCR: a 2 km disc, one of these by the seed, so a small match
/// is a fight and not a walk. 32 opens on a 3 km disc, 64 on the whole map. (The branch had one at
/// the middle too, (100, -100): that is the Yamuna, and its flight ran down the river.)
const DISCS_16: [(f32, f32); 5] = [(-700.0, -800.0), (-450.0, 750.0), (-1100.0, 1150.0), (900.0, -950.0), (1050.0, 700.0)];
const DISCS_32: [(f32, f32); 5] = [(-500.0, -500.0), (500.0, -500.0), (-500.0, 500.0), (500.0, 500.0), (0.0, 0.0)];
const WHOLE_MAP: [(f32, f32); 1] = [(0.0, 0.0)];
/// The furthest a ring's centre moves in one phase: the first ring of a big match must stay walkable.
const MAX_SHIFT: f32 = 600.0;
/// No ring is centred closer than this to a holy site.
const HOLY_CLEAR: f32 = 120.0;
/// Candidates tried for the final circle, the one with the most cover winning, and for each ring.
const FINAL_TRIES: usize = 6;
const RING_TRIES: usize = 24;
/// How much of the distance a last circle could be from the disc's middle it may use.
const FINAL_REACH: f32 = 0.75;
/// Times a whole match's rings are planned again when one of them won't sit on land.
const PLAN_TRIES: usize = 8;
/// How far out in a ring the last circle's centre may lie, as a share of its radius.
const KEEP_INSIDE: f32 = 0.7;

/// The NCR's ring table for a match of this many warriors, the start disc's radius, and the discs
/// to choose from.
pub fn ncr_table(players: u32) -> (&'static [Phase], f32, &'static [(f32, f32)]) {
    match players {
        0..=16 => (&NCR_16, 1000.0, &DISCS_16),
        17..=32 => (&NCR_32, 1500.0, &DISCS_32),
        _ => (&NCR_64, 2830.0, &WHOLE_MAP),
    }
}

/// Every holy site on the map, worked out once.
fn holy_sites() -> &'static [(Vec2, f32)] {
    static SITES: std::sync::OnceLock<Vec<(Vec2, f32)>> = std::sync::OnceLock::new();
    SITES.get_or_init(compose::holy)
}

/// Whether a ring may be centred here: on dry land inside the map, off the river sand, the
/// bridges and the buildings, and well away from any holy site. (The source branch also asked
/// its street floor whether the spot can be walked to; this one has none at the match's start.)
fn ring_spot(layout: &MapLayout, p: Vec2) -> bool {
    p.abs().max_element() < layout.half_extent - 80.0
        && !matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::WATER | ground::SAND))
        && !SPANS.iter().any(|s| {
            let along = (s.c - s.a).normalize_or_zero();
            let t = (p - s.a).dot(along).clamp(0.0, s.a.distance(s.c));
            p.distance(s.a + along * t) < s.width
        })
        && holy_sites().iter().all(|(c, r)| p.distance(*c) >= r + HOLY_CLEAR)
        && !compose::built(layout, p)
}

/// How much cover stands round a spot: what a last stand there has to fight from.
fn cover_at(layout: &MapLayout, p: Vec2) -> usize {
    let near = |q: Vec2| q.distance(p) < 60.0;
    layout.props.iter().filter(|t| near(t.pos.xz())).count() + 3 * layout.footprints.iter().filter(|f| f.pts.first().is_some_and(|q| near(*q))).count()
}

/// A point of the disc, evenly over its area.
fn in_disc(rng: &mut SmallRng, centre: Vec2, radius: f32) -> Vec2 {
    centre + Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * radius * rng.random_range(0.0f32..1.0).sqrt()
}

/// The centre of every ring of a match, worked out before it starts: first the last circle, on
/// land with cover round it, then each ring in turn, so that each sits inside the one before and
/// still holds the last circle. A ring therefore always closes on ground a fight can finish on.
fn plan_rings(rng: &mut SmallRng, layout: &MapLayout, disc: Vec2, radius: f32, table: &[Phase]) -> Vec<Vec2> {
    // Now and then a river fills all the room a ring has: start again from another last circle.
    let mut plan = plan_once(rng, layout, disc, radius, table);
    for _ in 0..PLAN_TRIES {
        if plan.iter().all(|c| ring_spot(layout, *c)) {
            break;
        }
        plan = plan_once(rng, layout, disc, radius, table);
    }
    plan
}

fn plan_once(rng: &mut SmallRng, layout: &MapLayout, disc: Vec2, radius: f32, table: &[Phase]) -> Vec<Vec2> {
    // The first ring's centre moves at most MAX_SHIFT, so the last circle must be that near. A
    // last circle at the very limit would leave each ring only one place to go, river or not.
    let first = table.first().map_or(0.0, |p| p.2);
    let reach = FINAL_REACH * radius.min(first + (radius - first).min(MAX_SHIFT));
    let last = (0..FINAL_TRIES * RING_TRIES)
        .map(|_| in_disc(rng, disc, reach))
        .filter(|p| ring_spot(layout, *p))
        .take(FINAL_TRIES)
        .max_by_key(|p| cover_at(layout, *p))
        .unwrap_or(disc);
    let (mut at, mut from_r) = (disc, radius);
    let mut out = vec![];
    for (k, &(_, _, r, _)) in table.iter().enumerate() {
        let shift = (from_r - r).min(MAX_SHIFT);
        let c = if k + 1 == table.len() {
            last
        } else {
            // Every spot both near enough to the last ring and to the last circle lies in the lens
            // where two discs cross; drawing from the biggest disc inside it never misses.
            let to_last = last - at;
            let d = to_last.length();
            // With the last circle well inside this ring, the next rings have room to choose.
            let held = if d - KEEP_INSIDE * r <= shift { KEEP_INSIDE * r } else { r };
            let (lo, hi) = ((d - held).max(-shift), shift.min(d + held));
            let mid = at + to_last.normalize_or(Vec2::X) * (lo + hi) / 2.0;
            let spots = (0..RING_TRIES).map(|_| in_disc(rng, mid, (hi - lo) / 2.0)).collect::<Vec<_>>();
            // The lens can be a sliver over the river: then the last circle's centre, if near enough.
            let fallback = if d <= shift { last } else { mid };
            spots.into_iter().find(|p| ring_spot(layout, *p)).unwrap_or(fallback)
        };
        out.push(c);
        (at, from_r) = (c, r);
    }
    out
}

#[derive(Resource, Clone, Debug, Default)]
pub struct Zone {
    pub phase: usize,
    pub shrinking: bool,
    pub timer: f32,
    pub center: Vec2,
    pub radius: f32,
    pub from_center: Vec2,
    pub from_radius: f32,
    pub target_center: Vec2,
    pub target_radius: f32,
    pub dps: f32,
    /// The match seed: each phase's circle comes from it, so every peer closes the same ring.
    seed: u64,
    tick: f32,
    /// The ring table this match runs.
    phases: &'static [Phase],
    /// Every ring's centre, planned from the map at the start (the NCR); empty: each is drawn as
    /// its phase begins.
    centres: Vec<Vec2>,
}

impl Zone {
    pub fn new(seed: u64) -> Self {
        let mut z = Zone { radius: START_RADIUS, from_radius: START_RADIUS, dps: 1.0, seed, phases: &PHASES, ..default() };
        z.plan_next();
        z
    }

    /// The NCR's ring: it opens on a disc of the map picked by the seed, and every circle is
    /// planned now, from the map, so all peers close the same ring on land.
    pub fn for_ncr(seed: u64, players: u32, layout: &MapLayout) -> Self {
        let (phases, radius, discs) = ncr_table(players);
        let mut rng = match_rng(seed, stream::DISC);
        let (x, z) = discs[rng.random_range(0..discs.len())];
        let center = Vec2::new(x, z);
        let centres = plan_rings(&mut rng, layout, center, radius, phases);
        let mut zone = Zone { center, radius, from_center: center, from_radius: radius, dps: 1.0, seed, phases, centres, ..default() };
        zone.plan_next();
        zone
    }

    /// The ring table this match runs.
    pub fn phases(&self) -> &'static [Phase] {
        self.phases
    }

    fn plan_next(&mut self) {
        let Some(&(wait, _, end_r, dps)) = self.phases.get(self.phase) else { return };
        let c = if let Some(&planned) = self.centres.get(self.phase) {
            planned
        } else {
            let mut rng = match_rng(self.seed, stream::ZONE + self.phase as u64);
            // Next circle sits fully inside the current one.
            let slack = (self.radius - end_r).max(0.0) * 0.8;
            let a = rng.random_range(0.0..std::f32::consts::TAU);
            let d = rng.random_range(0.0..slack.min(self.radius));
            let c = self.center + Vec2::new(a.cos(), a.sin()) * d;
            c.clamp(Vec2::splat(-200.0), Vec2::splat(200.0))
        };
        self.from_center = self.center;
        self.from_radius = self.radius;
        self.target_center = c;
        self.target_radius = end_r;
        self.timer = wait;
        self.shrinking = false;
        self.dps = dps;
    }

    pub fn outside(&self, p: Vec3) -> bool {
        p.xz().distance(self.center) > self.radius
    }

    /// One tick of the battle: wait, then shrink towards the planned circle, then plan the next.
    pub fn advance(&mut self, dt: f32) {
        let Some(&(_, shrink, _, _)) = self.phases.get(self.phase) else { return };
        self.timer -= dt;
        if !self.shrinking {
            if self.timer <= 0.0 {
                self.shrinking = true;
                self.timer = shrink;
            }
        } else {
            let t = 1.0 - (self.timer / shrink).clamp(0.0, 1.0);
            self.center = self.from_center.lerp(self.target_center, t);
            self.radius = self.from_radius + (self.target_radius - self.from_radius) * t;
            if self.timer <= 0.0 {
                self.center = self.target_center;
                self.radius = self.target_radius;
                self.phase += 1;
                self.plan_next();
            }
        }
    }

    /// Seconds until the next change, and whether it is a shrink.
    pub fn countdown(&self) -> (f32, bool) {
        (self.timer, self.shrinking)
    }
}

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Zone::new(0)).add_systems(FixedUpdate, (advance_zone, burn_outside).chain().in_set(SimSet::Zone));
    }
}

fn advance_zone(mut zone: ResMut<Zone>, m: Res<Match>, time: Res<Time>) {
    if m.state == MatchState::InProgress {
        zone.advance(time.delta_secs());
    }
}

fn burn_outside(
    mut zone: ResMut<Zone>,
    m: Res<Match>,
    q: Query<(Entity, &Transform), (With<Warrior>, Without<Eliminated>, Without<super::net::Remote>)>,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    if m.state != MatchState::InProgress {
        return;
    }
    zone.tick += time.delta_secs();
    if zone.tick < 1.0 {
        return;
    }
    zone.tick -= 1.0;
    for (e, tf) in &q {
        if zone.outside(tf.translation) {
            damage.write(DamageMsg { target: e, amount: zone.dps, source: None, headshot: false, cause: DamageCause::Pralaya });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The match lengths the NCR's tables promise, wait and shrink added up.
    #[test]
    fn ncr_tables_run_their_lengths() {
        let length = |t: &[Phase]| t.iter().map(|p| p.0 + p.1).sum::<f32>();
        assert_eq!(length(&NCR_16), 650.0);
        assert_eq!(length(&NCR_32), 870.0);
        assert_eq!(length(&NCR_64), 1190.0);
        for t in [&NCR_16[..], &NCR_32[..], &NCR_64[..]] {
            assert_eq!(t.last().unwrap().2, 0.0);
            assert!(t.windows(2).all(|w| w[1].2 < w[0].2));
        }
    }

    /// On the NCR every ring sits inside the one before, on any seed and size of match, and the
    /// last circle is on dry land, on no bridge, in no building and away from every holy site.
    #[test]
    fn ncr_rings_nest_and_end_on_land() {
        let map = compose::generate(7);
        for players in [16, 32, 64] {
            for seed in 0..12 {
                let mut z = Zone::for_ncr(seed, players, &map);
                let (table, radius, _) = ncr_table(players);
                assert_eq!(z.radius, radius);
                for k in 0..table.len() {
                    assert_eq!(z.phase, k);
                    assert!(z.target_center.distance(z.from_center) + z.target_radius <= z.from_radius + 0.01, "seed {seed}, phase {k}");
                    assert!(z.target_center.abs().max_element() < 2000.0, "seed {seed}, phase {k}: {}", z.target_center);
                    assert!(map.terrain.ground_at(z.target_center.x, z.target_center.y) != Some(ground::WATER), "seed {seed}, phase {k}: {} is on water", z.target_center);
                    if k + 1 == table.len() {
                        assert!(ring_spot(&map, z.target_center), "seed {seed}: last circle at {}", z.target_center);
                    }
                    while z.phase == k {
                        z.advance(0.5);
                    }
                }
                assert_eq!(z.radius, 0.0);
            }
        }
    }
}
