//! Where vehicles stand on the NCR: about 300 fixed bays, the same in every match, on the kerbs of
//! the roads, at the named yards, at both ends of each bridge and in each district. A match draws
//! its vehicles from them by the match seed (`crate::sim::vehicle`), so a player learns where to
//! look and still doesn't know what will be there.

use bevy::prelude::*;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};

use crate::SmallRng;

use super::compose::{self, PLACES, SPANS};
use super::terrain::ground;
use super::{MapLayout, kit};

/// A place for one vehicle, on the ground, nose along `yaw`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bay {
    pub pos: Vec3,
    pub yaw: f32,
    /// In a named yard: the only place a Khuli Gaadi or a tractor is parked.
    pub yard: bool,
}

/// The side of the squares a match spreads its vehicles over, one to a square, metres.
pub const CELL: f32 = 420.0;
/// Bays are their own draw, so that the rest of the map comes out the same with or without them.
const SEED: u64 = 7 ^ 0xBA5;
/// Metres of road between one verge bay and the next.
const ROAD_EVERY: f32 = 260.0;
/// How far in from a road's edge a vehicle's middle stands.
const KERB: f32 = 1.3;
const AT_YARD: usize = 3;
/// How far out from a yard's middle, as a share of its reach.
const YARD_OUT: f32 = 0.55;
/// Bays at each end of a bridge, and how far past the top of its ramp.
const AT_BRIDGE: usize = 2;
const BRIDGE_OFF: f32 = 14.0;
const IN_DISTRICT: usize = 6;
/// How far apart two of a district's bays are at the least.
const DISTRICT_APART: f32 = 45.0;
/// How far along the kerb from a street kit's parked auto or scooter.
const KIT_ROOM: f32 = 4.6;
/// No two bays closer than this.
const APART: f32 = 12.0;
/// How far in from the map's edge walls.
const EDGE: f32 = 30.0;

/// Which of a match's squares a point lies in.
pub fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// Dry, hard enough, clear of a holy circle, a building and the map's edge.
fn fit(map: &MapLayout, p: Vec2, bays: &[Bay]) -> bool {
    let reach = map.half_extent - EDGE;
    p.x.abs() < reach
        && p.y.abs() < reach
        && !matches!(map.terrain.ground_at(p.x, p.y), Some(ground::WATER | ground::PARK))
        && map.holy.iter().all(|(c, r)| p.distance(*c) >= *r + 4.0)
        && !compose::no_loot(p)
        && bays.iter().all(|b| b.pos.xz().distance(p) >= APART)
        && !compose::in_building(map, Vec3::new(p.x, map.terrain.height(p.x, p.y), p.y))
}

/// Each road's verge, every `ROAD_EVERY` metres, turn about on either kerb, nosing the way the
/// traffic on that side goes.
fn verges(map: &MapLayout, network: &[(f32, Vec<Vec2>)], bays: &mut Vec<Bay>) {
    for (width, line) in network {
        let (mut run, mut next, mut left) = (0.0, ROAD_EVERY / 2.0, true);
        for s in line.windows(2) {
            let (a, c) = (s[0], s[1]);
            let len = a.distance(c);
            if len < 0.01 {
                continue;
            }
            let dir = (c - a) / len;
            while next < run + len {
                let side = if left { -dir.perp() } else { dir.perp() };
                let p = a + dir * (next - run) + side * (*width / 2.0 - KERB);
                if fit(map, p, bays) {
                    bays.push(Bay { pos: Vec3::new(p.x, map.terrain.height(p.x, p.y), p.y), yaw: kit::facing(if left { dir } else { -dir }), yard: false });
                }
                next += ROAD_EVERY;
                left = !left;
            }
            run += len;
        }
    }
}

/// Yards, bridge ends and districts.
fn places(map: &MapLayout, yards: &[(Vec2, f32)], rng: &mut SmallRng, bays: &mut Vec<Bay>) {
    let stand = |p: Vec2, yaw: f32, yard: bool| Bay { pos: Vec3::new(p.x, map.terrain.height(p.x, p.y), p.y), yaw, yard };
    for (centre, reach) in yards {
        let turn = rng.random_range(0.0..std::f32::consts::TAU);
        for k in 0..AT_YARD {
            let a = turn + k as f32 / AT_YARD as f32 * std::f32::consts::TAU;
            let p = *centre + Vec2::from_angle(a) * *reach * YARD_OUT;
            if fit(map, p, bays) {
                bays.push(stand(p, a + rng.random_range(-0.6..0.6), true));
            }
        }
    }
    for s in &SPANS {
        let ends = [(-s.ramp() - BRIDGE_OFF, s.dir()), (s.len() + s.ramp() + BRIDGE_OFF, -s.dir())];
        for (along, nose) in ends {
            for k in 0..AT_BRIDGE {
                let off = if k == 0 { 1.0 } else { -1.0 } * (s.width / 2.0 - KERB);
                let p = s.at(along, off);
                if fit(map, p, bays) {
                    bays.push(stand(p, kit::facing(nose), false));
                }
            }
        }
    }
    for place in &PLACES {
        let mut pieces: Vec<&kit::KitPiece> = map.kit.iter().filter(|k| matches!(k.name, "auto" | "scooter") && k.pos.xz().distance(place.at) < place.clip).collect();
        pieces.shuffle(rng);
        let mut here: Vec<Vec2> = vec![];
        for k in pieces {
            if here.len() == IN_DISTRICT {
                break;
            }
            let along = (Quat::from_rotation_y(k.yaw) * Vec3::NEG_Z).xz();
            for p in [k.pos.xz() + along * KIT_ROOM, k.pos.xz() - along * KIT_ROOM] {
                if here.iter().all(|h| h.distance(p) >= DISTRICT_APART) && fit(map, p, bays) {
                    here.push(p);
                    bays.push(stand(p, k.yaw, false));
                    break;
                }
            }
        }
    }
}

/// Lays out the map's bays, given its roads and its yards (the middle and the reach of each).
pub(super) fn lay(map: &mut MapLayout, network: &[(f32, Vec<Vec2>)], yards: &[(Vec2, f32)]) {
    let mut rng = SmallRng::seed_from_u64(SEED);
    let mut bays = vec![];
    places(map, yards, &mut rng, &mut bays);
    verges(map, network, &mut bays);
    map.parking = bays;
}

#[cfg(test)]
mod tests {
    use super::super::{MapId, generate};
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn about_300_bays_spread_over_the_map_and_none_in_water_or_a_holy_circle() {
        let map = generate(MapId::Ncr, 1);
        let again = generate(MapId::Ncr, 2);
        assert_eq!(map.parking, again.parking, "the bays don't change with the match");
        let (n, yards) = (map.parking.len(), map.parking.iter().filter(|b| b.yard).count());
        let cells: HashSet<_> = map.parking.iter().map(|b| cell(b.pos.xz())).collect();
        println!("{n} bays, {yards} at yards, in {} squares", cells.len());
        assert!((250..=450).contains(&n), "{n} bays");
        assert!(yards >= 40, "{yards} at yards");
        assert!(cells.len() >= 72, "{} squares", cells.len());
        for b in &map.parking {
            let p = b.pos.xz();
            assert!(!matches!(map.terrain.ground_at(p.x, p.y), Some(ground::WATER | ground::PARK)), "{p}");
            assert!(map.holy.iter().all(|(c, r)| p.distance(*c) >= *r), "{p}");
        }
    }

    #[test]
    fn a_small_map_has_none() {
        assert!(generate(MapId::Cp, 1).parking.is_empty());
    }
}
