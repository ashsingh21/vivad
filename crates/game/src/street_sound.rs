//! The street round the player: how much of each bed to hear where it stands and at what hour, and
//! the city's one-shots (horns, hawkers, bells, birds, dogs) placed round the listener by the
//! hour. Hawkers call from the map's own stalls (`MapLayout::kit`), and parks, open land and roads
//! come from its painted ground, so no map's coordinates are in here. A map without them simply
//! sounds like a street everywhere.
//!
//! The sounds are cut from recordings by tools/audio/street.py; previews/fights-sounds/sound_list.md
//! says which plays where. Client only: none of this touches the simulation.

use std::collections::HashSet;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::RngExt;
use shared::items::{ItemDb, ItemKind};
use shared::map::MapLayout;
use shared::map::terrain::ground;
use shared::sim::combat::KillMsg;
use shared::sim::{HALF_HEIGHT, HitMsg, Inventory, Kinematic, LocalPlayer, MapSwaps, MoveMode, solid_mask};

use crate::audio::Player;
use crate::visuals::TimeOfDay;

/// Every sound played from here, for `audio::load`.
pub const NAMES: [&str; 31] = [
    "horn_0", "horn_1", "horn_2", "horn_3", "horn_4", "horn_5", "horn_6", "horn_bus", "auto_pass", "bike_bell", "whistle", "hawker_0",
    "hawker_1", "hawker_2", "hawker_3", "crowd_argue", "crowd_yell", "crowd_gasp", "shutter", "cooker", "chai_stir", "crow", "crows",
    "koel", "myna", "pigeons_up", "dog", "dhol", "amb_traffic", "amb_market", "amb_far",
];

/// How far from a stall its hawker is heard, metres.
const STALL_REACH: f32 = 45.0;
/// One stall to this square, metres, and no more than this many on a map, about 20 a district on
/// the NCR, which holds 1,363 stalls and carts. Only the nearest ever calls.
const STALL_GAP: f32 = 60.0;
const MOST_STALLS: usize = 380;
/// Above the ground, and past this, the street's small sounds stop.
const TOO_HIGH: f32 = 30.0;
/// How far off water is heard, and what indoors leaves of the street (the rest goes to the city's
/// dull far-off bed, which is what a street sounds like through a wall).
const WATER_REACH: f32 = 80.0;
const THROUGH_WALLS: f32 = 0.25;
/// A roof this close over the head and this many of four sides walled in, this close: indoors.
const ROOM_HIGH: f32 = 7.0;
const ROOM_WIDE: f32 = 9.0;
const ROOM_WALLS: usize = 2;
/// A fight's crowd yells at most this often, seconds.
const YELL_GAP: f32 = 12.0;
/// How near the listener a fight must be for the crowd to matter, metres.
const CROWD_REACH: f32 = 40.0;

/// How much of each bed to hear, 0 to 1, for `audio::beds`.
#[derive(Resource, Default)]
pub struct Ambience {
    pub traffic: f32,
    pub market: f32,
    pub far: f32,
    pub park: f32,
    /// Wind over the open land, the Ridge and the roofs.
    pub wind: f32,
    /// Water close by, on a map whose rivers are in its ground.
    pub river: f32,
    /// Walls and a roof round the listener: the street comes through them, dull.
    pub indoors: bool,
}

/// Where a street sound comes from.
#[derive(Clone, Copy, PartialEq)]
enum From {
    /// Anywhere round the listener, this far away (metres), where there is a street.
    Round(f32, f32),
    /// The same on any ground: birds and dogs.
    Wild(f32, f32),
    /// The nearest of the map's stalls and carts in reach; silent with none.
    Stall,
    /// A chai stall close enough to hear a spoon.
    Chai,
}

/// One kind of street sound.
struct Voice {
    names: &'static [&'static str],
    /// The hours it is heard, wrapping past midnight.
    hours: (f32, f32),
    /// Seconds between one and the next.
    gap: (f32, f32),
    from: From,
    volume: f32,
    /// Comes faster when the traffic is heavy.
    rush: bool,
}

const HORNS: &[&str] = &["horn_0", "horn_1", "horn_2", "horn_3", "horn_4", "horn_5", "horn_6"];
const HAWKERS: &[&str] = &["hawker_0", "hawker_1", "hawker_2", "hawker_3"];

const VOICES: [Voice; 16] = [
    Voice { names: HORNS, hours: (5.5, 24.0), gap: (2.5, 8.0), from: From::Round(20.0, 60.0), volume: 0.7, rush: true },
    Voice { names: &["horn_bus"], hours: (7.0, 23.0), gap: (50.0, 110.0), from: From::Round(30.0, 70.0), volume: 0.8, rush: false },
    Voice { names: &["auto_pass"], hours: (6.0, 23.5), gap: (12.0, 26.0), from: From::Round(15.0, 40.0), volume: 0.8, rush: true },
    Voice { names: &["bike_bell"], hours: (6.0, 21.0), gap: (15.0, 32.0), from: From::Round(10.0, 30.0), volume: 0.5, rush: false },
    Voice { names: &["whistle"], hours: (17.0, 20.5), gap: (20.0, 45.0), from: From::Round(30.0, 70.0), volume: 0.6, rush: false },
    // The night chowkidar.
    Voice { names: &["whistle"], hours: (22.0, 5.0), gap: (40.0, 80.0), from: From::Round(40.0, 80.0), volume: 0.5, rush: false },
    Voice { names: HAWKERS, hours: (8.0, 21.5), gap: (6.0, 14.0), from: From::Stall, volume: 0.8, rush: false },
    Voice { names: &["chai_stir"], hours: (6.0, 23.0), gap: (8.0, 16.0), from: From::Chai, volume: 0.5, rush: false },
    Voice { names: &["crowd_argue"], hours: (10.0, 22.0), gap: (40.0, 90.0), from: From::Round(25.0, 50.0), volume: 0.6, rush: false },
    Voice { names: &["shutter"], hours: (7.0, 10.0), gap: (25.0, 60.0), from: From::Round(20.0, 40.0), volume: 0.6, rush: false },
    Voice { names: &["shutter"], hours: (20.5, 23.0), gap: (25.0, 60.0), from: From::Round(20.0, 40.0), volume: 0.6, rush: false },
    Voice { names: &["cooker"], hours: (7.0, 13.5), gap: (40.0, 85.0), from: From::Round(20.0, 40.0), volume: 0.5, rush: false },
    Voice { names: &["crow", "crows", "pigeons_up"], hours: (6.0, 19.0), gap: (9.0, 22.0), from: From::Wild(10.0, 40.0), volume: 0.6, rush: false },
    Voice { names: &["koel", "myna"], hours: (5.0, 9.5), gap: (10.0, 24.0), from: From::Wild(15.0, 40.0), volume: 0.6, rush: false },
    Voice { names: &["dog"], hours: (21.0, 5.5), gap: (12.0, 32.0), from: From::Wild(30.0, 60.0), volume: 0.7, rush: false },
    Voice { names: &["dhol"], hours: (19.0, 23.0), gap: (90.0, 170.0), from: From::Round(50.0, 80.0), volume: 0.8, rush: false },
];

/// How heavy the traffic and how thick the crowd are through the day, 0 to 1: (hour, level).
const TRAFFIC: [(f32, f32); 10] =
    [(0.0, 0.15), (5.0, 0.12), (7.0, 0.6), (10.0, 1.0), (13.0, 0.85), (15.0, 0.7), (18.0, 1.0), (21.0, 0.7), (23.0, 0.3), (24.0, 0.15)];
const CROWD: [(f32, f32); 8] = [(0.0, 0.02), (7.0, 0.2), (10.0, 1.0), (14.0, 0.7), (18.0, 1.0), (21.0, 0.6), (23.0, 0.1), (24.0, 0.02)];

fn by_hour(keys: &[(f32, f32)], h: f32) -> f32 {
    let i = keys.partition_point(|k| k.0 <= h).clamp(1, keys.len() - 1);
    let (a, b) = (keys[i - 1], keys[i]);
    a.1 + (b.1 - a.1) * ((h - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
}

fn in_hours(h: f32, (from, to): (f32, f32)) -> bool {
    if from <= to { h >= from && h < to } else { h >= from || h < to }
}

/// The map's stalls and carts, and its chai stalls.
#[derive(Resource, Default)]
struct Stalls {
    any: Vec<Vec3>,
    chai: Vec<Vec3>,
}

pub struct StreetSoundPlugin;

impl Plugin for StreetSoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ambience>().init_resource::<Stalls>().add_systems(Update, (find_stalls.run_if(resource_changed::<MapSwaps>), listen, street, crowd));
    }
}

fn find_stalls(layout: Res<MapLayout>, mut stalls: ResMut<Stalls>) {
    let of = |names: &[&str]| spaced(layout.kit.iter().filter(|k| names.contains(&k.name)).map(|k| k.pos));
    *stalls = Stalls { any: of(&["thela", "chai"]), chai: of(&["chai"]) };
    info!("street sound: {} stalls and carts, {} chai stalls", stalls.any.len(), stalls.chai.len());
}

/// Spots kept apart by `STALL_GAP`, thinned evenly down to `MOST_STALLS`, so no district goes quiet.
fn spaced(spots: impl Iterator<Item = Vec3>) -> Vec<Vec3> {
    let mut taken = HashSet::new();
    let apart: Vec<Vec3> = spots.filter(|p| taken.insert((p.xz() / STALL_GAP).floor().as_ivec2())).collect();
    apart.iter().copied().step_by(apart.len().div_ceil(MOST_STALLS).max(1)).collect()
}

/// How much of a street the ground under `p` is: some of the traffic carries into a park, none out
/// over open land, river sand or water.
fn street_here(layout: &MapLayout, p: Vec3) -> f32 {
    match layout.terrain.ground_at(p.x, p.z) {
        Some(ground::PARK) => 0.3,
        Some(ground::OPEN | ground::SAND | ground::WATER) => 0.0,
        _ => 1.0,
    }
}

/// How much of a river's sound carries to `p`, 0 to 1. The NCR's rivers are in its ground.
fn water_near(layout: &MapLayout, p: Vec2) -> f32 {
    let wet = |r: f32| (0..8).any(|k| {
        let q = p + Vec2::from_angle(k as f32 / 8.0 * std::f32::consts::TAU) * r;
        layout.terrain.ground_at(q.x, q.y) == Some(ground::WATER)
    });
    [0.0, 8.0, 20.0, 40.0, 60.0].into_iter().find(|r| wet(*r)).map_or(0.0, |r| 1.0 - r / WATER_REACH)
}

/// How much wind there is at `p`: out on the open, and more the higher up.
fn wind_at(here: f32, up: f32, airborne: bool) -> f32 {
    if airborne {
        return 0.0;
    }
    let open = 1.0 - here;
    (open * 0.7).max(((up - 3.0) / 12.0).clamp(0.0, 1.0)).max(0.15 * (1.0 - here))
}

fn nearest(spots: &[Vec3], p: Vec3) -> Option<Vec3> {
    spots.iter().copied().min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)))
}

/// Where the local warrior stands, as the beds should hear it (every quarter second).
#[allow(clippy::too_many_arguments)]
fn listen(
    time: Res<Time>,
    me: Query<(Entity, &Transform, &Kinematic), With<LocalPlayer>>,
    layout: Res<MapLayout>,
    tod: Res<TimeOfDay>,
    stalls: Res<Stalls>,
    spatial: SpatialQuery,
    mut amb: ResMut<Ambience>,
    mut next: Local<f32>,
) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 0.25;
    let hour = tod.hour.rem_euclid(24.0);
    let Ok((e, tf, kin)) = me.single() else {
        // At the menu, before a match: the city far off.
        *amb = Ambience { far: 0.7, ..default() };
        return;
    };
    let p = tf.translation;
    let up = p.y - HALF_HEIGHT - layout.terrain.height(p.x, p.z);
    let high = if kin.mode == MoveMode::Ground { ((up - 3.0) / 9.0).clamp(0.0, 1.0) } else { 1.0 };
    let here = street_here(&layout, p);
    let park = if here < 1.0 { 1.0 } else { 0.0 };
    let filter = SpatialQueryFilter::from_mask(solid_mask()).with_excluded_entities([e]);
    let ground_level = kin.mode == MoveMode::Ground;
    let covered = ground_level && spatial.cast_ray(p, Dir3::Y, 12.0, true, &filter).is_some();
    let roofed = covered && spatial.cast_ray(p, Dir3::Y, ROOM_HIGH, true, &filter).is_some();
    let indoors = roofed && [Dir3::X, Dir3::NEG_X, Dir3::Z, Dir3::NEG_Z].into_iter().filter(|d| spatial.cast_ray(p, *d, ROOM_WIDE, true, &filter).is_some()).count() >= ROOM_WALLS;
    let dull = if indoors { THROUGH_WALLS } else { 1.0 };
    let stall = nearest(&stalls.any, p).map_or(0.0, |s| (1.0 - s.distance(p) / STALL_REACH).clamp(0.0, 1.0));
    let (traffic, crowd) = (by_hour(&TRAFFIC, hour), by_hour(&CROWD, hour));
    *amb = Ambience {
        traffic: traffic * (1.0 - high) * here * if covered { 0.7 } else { 1.0 } * dull,
        market: crowd * (1.0 - high) * (1.0 - park) * (0.3 + 0.7 * stall.max(if covered { 0.5 } else { 0.0 })) * dull,
        far: (0.35f32).max(high).max(park).max(1.0 - traffic).max(if indoors { 0.8 } else { 0.0 }) * 0.8,
        park: park * (1.0 - high) * dull,
        wind: wind_at(here, up, !ground_level) * if roofed { 0.0 } else { 1.0 },
        river: water_near(&layout, p.xz()) * dull,
        indoors,
    };
}

/// The city's one-shots.
fn street(
    mut play: Player,
    time: Res<Time>,
    me: Query<(&Transform, &Kinematic), With<LocalPlayer>>,
    layout: Res<MapLayout>,
    tod: Res<TimeOfDay>,
    stalls: Res<Stalls>,
    amb: Res<Ambience>,
    mut next: Local<Vec<f32>>,
) {
    let now = time.elapsed_secs();
    if next.is_empty() {
        let mut rng = rand::rng();
        // Staggered, so a match doesn't open on every sound at once.
        *next = VOICES.iter().map(|v| now + rng.random_range(0.0..v.gap.1)).collect();
    }
    let Ok((tf, kin)) = me.single() else { return };
    let p = tf.translation;
    if kin.mode == MoveMode::Vimana || p.y - layout.terrain.height(p.x, p.z) > TOO_HIGH {
        return;
    }
    let hour = tod.hour.rem_euclid(24.0);
    let traffic = by_hour(&TRAFFIC, hour);
    let open = street_here(&layout, p) < 1.0;
    let mut rng = rand::rng();
    for (v, when) in VOICES.iter().zip(next.iter_mut()) {
        if now < *when {
            continue;
        }
        let wait = rng.random_range(v.gap.0..v.gap.1);
        *when = now + if v.rush { wait / traffic.max(0.2) } else { wait };
        if !in_hours(hour, v.hours) {
            continue;
        }
        let at = match v.from {
            From::Round(..) if open => None,
            From::Round(near, far) | From::Wild(near, far) => Some(p + (Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU)) * rng.random_range(near..far)).extend(0.0).xzy()),
            From::Stall => nearest(&stalls.any, p).filter(|s| s.distance(p) < STALL_REACH),
            From::Chai => nearest(&stalls.chai, p).filter(|s| s.distance(p) < 12.0),
        };
        if let Some(at) = at {
            let name = v.names[rng.random_range(0..v.names.len())];
            // Through a wall a street sound is quieter and a little lower.
            let (volume, pitch) = if amb.indoors { (v.volume * 0.45, 0.92) } else { (v.volume, 1.0) };
            play.street(name, at + Vec3::Y, volume, pitch * rng.random_range(0.95..1.05));
        }
    }
}

/// People react: yelling round a melee fight near the listener, a gasp at a kill. Only while there
/// are people about: by the hour, and on a street.
#[allow(clippy::too_many_arguments)]
fn crowd(
    mut play: Player,
    time: Res<Time>,
    mut hits: MessageReader<HitMsg>,
    mut kills: MessageReader<KillMsg>,
    who: Query<&Transform>,
    inv: Query<&Inventory>,
    db: Res<ItemDb>,
    me: Query<&Transform, With<LocalPlayer>>,
    layout: Res<MapLayout>,
    tod: Res<TimeOfDay>,
    mut last: Local<f32>,
) {
    let Ok(ear) = me.single().map(|t| t.translation) else { return };
    let people = by_hour(&CROWD, tod.hour.rem_euclid(24.0)) > 0.15 && street_here(&layout, ear) == 1.0;
    let now = time.elapsed_secs();
    for h in hits.read() {
        let melee = inv.get(h.attacker).ok().and_then(|i| i.current()).is_some_and(|s| matches!(db.get(&s.id).kind, ItemKind::Melee { .. }));
        if melee && people && now - *last > YELL_GAP && h.at.distance(ear) < CROWD_REACH {
            *last = now;
            let round = (Vec2::from_angle(now * 7.0) * 9.0).extend(0.0).xzy();
            play.street("crowd_yell", h.at + round, 0.7, 1.0);
        }
    }
    for k in kills.read() {
        if let (true, Ok(t)) = (people, who.get(k.killer)) {
            if t.translation.distance(ear) < CROWD_REACH {
                play.street("crowd_gasp", t.translation + Vec3::X * 7.0, 0.7, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalls_stay_apart_and_few() {
        let row = (0..4000).map(|k| Vec3::new(k as f32 * 3.0, 0.0, 0.0));
        let kept = spaced(row);
        assert_eq!(kept.len(), (3999.0 * 3.0 / STALL_GAP) as usize + 1);
        assert!(kept.windows(2).all(|w| w[0].distance(w[1]) >= STALL_GAP - 3.0));
        let many: Vec<Vec3> = spaced((0..MOST_STALLS * 3).map(|k| Vec3::new(k as f32 * STALL_GAP, 0.0, 0.0)));
        assert!(many.len() <= MOST_STALLS);
        assert!(many.last().is_some_and(|p| p.x > STALL_GAP * MOST_STALLS as f32 * 2.0), "the far end of the map keeps its stalls");
    }

    #[test]
    fn wind_blows_out_in_the_open_and_up_high() {
        assert_eq!(wind_at(1.0, 0.0, false), 0.0, "none on a street");
        assert!(wind_at(0.0, 0.0, false) > 0.5, "the open land");
        assert!(wind_at(1.0, 12.0, false) > 0.5, "a roof");
        assert_eq!(wind_at(0.0, 0.0, true), 0.0, "the drop has its own");
    }

    /// The NCR's rivers: heard from the bank and not from Connaught Place.
    #[test]
    fn water_is_heard_near_the_river() {
        let map = shared::map::generate(shared::map::MapId::Ncr, 7);
        assert!(water_near(&map, Vec2::new(190.0, -330.0)) > 0.9, "in the Yamuna");
        assert_eq!(water_near(&map, Vec2::new(-740.0, -700.0)), 0.0);
    }

    #[test]
    fn hours_wrap_past_midnight() {
        assert!(in_hours(23.0, (22.0, 5.0)));
        assert!(in_hours(1.0, (22.0, 5.0)));
        assert!(!in_hours(12.0, (22.0, 5.0)));
    }

    #[test]
    fn the_day_rises_and_falls() {
        assert!((by_hour(&TRAFFIC, 10.0) - 1.0).abs() < 1e-5);
        assert!(by_hour(&TRAFFIC, 3.0) < 0.2);
        assert!(by_hour(&TRAFFIC, 23.9) < 0.3);
    }

    #[test]
    fn every_street_sound_is_on_disk() {
        let voices = VOICES.iter().flat_map(|v| v.names.iter());
        for name in NAMES.iter().chain(voices).chain(crate::audio::MELEE_NAMES.iter()) {
            let path = format!("{}/audio/{name}.ogg", crate::ASSET_DIR);
            assert!(std::path::Path::new(&path).exists(), "{path}");
        }
    }
}
