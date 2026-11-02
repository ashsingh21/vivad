//! What sort of place a spot on the map is, and how old. What turns up there goes by this
//! (`sim::loot`, assets/data/drops.ron), so each part of Delhi plays its own way: desi guns in the
//! old city's rooms, spray guns in its lanes, autos on Cyber City's roads, rifles out on the Ridge.
//! Everything here comes from the map, so every peer gets the same answer.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::OnceLock;

use bevy::prelude::*;
use serde::Deserialize;

use super::terrain::ground;
use super::{Block, MapLayout, MapStyle, Mat, Shape, compose, footprint};

/// What a fight looks like here, from close to far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub enum Place {
    /// Inside: a shop, a room, a stair. 1 to 8 m.
    Room,
    /// Lanes, colonnades, markets. 5 to 30 m.
    Lane,
    /// Wide roads and bridges. 30 to 120 m.
    Road,
    /// Roofs and terraces. 50 to 200 m.
    Roof,
    /// Lawns, the Ridge, the land between districts, the river sand. 100 to 270 m.
    Open,
}

impl Place {
    pub const ALL: [Place; 5] = [Place::Room, Place::Lane, Place::Road, Place::Roof, Place::Open];
}

/// How old a part of the city is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub enum Era {
    /// The old city and the Sultanate: desi guns and old blades.
    Old,
    /// Lutyens' Delhi and the railway: the Raj's rifles.
    Raj,
    /// Gurugram, Noida and the malls: modern guns.
    New,
}

/// A spot's place and era. Most of the land between districts has no era.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Spot {
    pub place: Place,
    pub era: Option<Era>,
}

const OLD: [&str; 7] = ["Chandni Chowk", "Hauz Khas", "Karol Bagh", "Tughlaqabad", "Kashmere Gate", "Kamla Nagar", "Shahdara"];
const RAJ: [&str; 5] = ["Connaught Place", "Kartavya Path", "India Gate", "New Delhi Station", "Mandi House"];
const NEW: [&str; 15] = [
    "Cyber City", "MG Road", "Golf Course Road", "Sikanderpur", "Aerocity", "Sector 18", "Sector 62", "Film City", "Expressway", "Expressway Towers", "Pari Chowk",
    "City Centre", "Nehru Place", "Saket", "Jasola",
];

fn era_of(name: &str) -> Option<Era> {
    if OLD.contains(&name) {
        Some(Era::Old)
    } else if RAJ.contains(&name) {
        Some(Era::Raj)
    } else if NEW.contains(&name) {
        Some(Era::New)
    } else {
        None
    }
}

/// Metres a side of a cell of `Overhead`.
const CELL: f32 = 8.0;
/// A slab this far over a warrior's middle roofs him; nearer than the first, he stands on or in it.
const ROOFED: (f32, f32) = (1.2, 9.0);
/// Standing this far over the ground, on no bridge, is standing on a building.
const UP_THERE: f32 = 2.5;
/// A pavement within two of these of a road is the roadside.
const KERB: f32 = 4.0;

/// The map's roof slabs, by cell, built the first time they are asked for: the blocks of the rooms
/// built to go inside. A hollow building has no such slabs: its footprint says where it is.
#[derive(Clone, Debug, Default)]
pub struct Overhead(OnceLock<HashMap<(i32, i32), Vec<u32>>>);

fn is_slab(b: &Block) -> Option<Vec3> {
    let Shape::Box(size) = b.shape else { return None };
    (b.collide && b.visible && size.y <= 1.5 && size.x >= 1.5 && size.z >= 1.5 && size.y < size.x.min(size.z) && (b.rot * Vec3::Y).y > 0.99 && b.mat != Mat::Water).then_some(size)
}

impl Overhead {
    fn cells(&self, blocks: &[Block]) -> &HashMap<(i32, i32), Vec<u32>> {
        self.0.get_or_init(|| {
            let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
            for (k, b) in blocks.iter().enumerate() {
                let Some(size) = is_slab(b) else { continue };
                let (u, v) = ((b.rot * Vec3::X).xz().abs() * size.x * 0.5, (b.rot * Vec3::Z).xz().abs() * size.z * 0.5);
                let (lo, hi) = (b.pos.xz() - u - v, b.pos.xz() + u + v);
                let (from, to) = ((lo / CELL).floor().as_ivec2(), (hi / CELL).floor().as_ivec2());
                for i in from.x..=to.x {
                    for j in from.y..=to.y {
                        cells.entry((i, j)).or_default().push(k as u32);
                    }
                }
            }
            cells
        })
    }

    fn roofed(&self, blocks: &[Block], p: Vec3) -> bool {
        let cell = (p.xz() / CELL).floor().as_ivec2();
        let Some(here) = self.cells(blocks).get(&(cell.x, cell.y)) else { return false };
        here.iter().any(|k| {
            let b = &blocks[*k as usize];
            let Some(size) = is_slab(b) else { return false };
            let q = b.rot.inverse() * (p - b.pos);
            let above = b.pos.y - size.y * 0.5 - p.y;
            q.x.abs() < size.x * 0.5 && q.z.abs() < size.z * 0.5 && above > ROOFED.0 && above < ROOFED.1
        })
    }
}

impl MapLayout {
    /// The place and era of a spot.
    pub fn place_at(&self, p: Vec3) -> Spot {
        Spot { place: self.place_of(p), era: self.era_at(p.xz()) }
    }

    fn place_of(&self, p: Vec3) -> Place {
        // A point inside an outline and under its roof is in a room, there being nothing else in
        // there to stand on: no rooms on the map, no such point. On the roof, a roof.
        let xz = p.xz();
        let building = self.footprints.iter().find(|f| f.mat != Mat::Water && f.pts[0].distance(xz) < 400.0 && footprint::contains(&f.pts, xz));
        if let Some(f) = building {
            return if p.y < f.top - 1.0 { Place::Room } else { Place::Roof };
        }
        if self.overhead.roofed(&self.blocks, p) {
            return Place::Room;
        }
        let bridge = self.style == MapStyle::Ncr && compose::bridge_at(xz).is_some();
        if bridge {
            return Place::Road;
        }
        if p.y - self.terrain.height(p.x, p.z) > UP_THERE {
            return Place::Roof;
        }
        match self.terrain.ground_at(p.x, p.z) {
            Some(ground::ROAD | ground::PARKING) => Place::Road,
            Some(ground::PARK | ground::OPEN | ground::SAND | ground::WATER) | None => Place::Open,
            Some(ground::PAVEMENT) if self.beside_road(xz) => Place::Road,
            Some(_) => Place::Lane,
        }
    }

    /// Whether a pavement runs beside a road. Finds aren't laid on the road itself, so this is what
    /// a road's loot stands on, and what tells Cyber City's kerbs from a back lane.
    fn beside_road(&self, p: Vec2) -> bool {
        (1..=2).any(|r| {
            (0..8).any(|k| {
                let q = p + Vec2::from_angle(k as f32 / 8.0 * TAU) * r as f32 * KERB;
                self.terrain.ground_at(q.x, q.y) == Some(ground::ROAD)
            })
        })
    }

    /// The era of the named place a point is in. On one of the small maps the whole map has one.
    pub fn era_at(&self, p: Vec2) -> Option<Era> {
        let here = |c: Vec3, r: f32| (p - c.xz()).abs().max_element() <= r;
        self.pois.iter().filter(|poi| here(poi.center, poi.radius)).find_map(|poi| era_of(&poi.name)).or_else(|| era_of(&self.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Footprint, MapId, Poi, generate};

    fn at(map: &MapLayout, name: &str) -> Vec3 {
        map.pois.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("{name}")).center
    }

    #[test]
    fn eras_go_by_the_district() {
        let map = generate(MapId::Ncr, 7);
        for (name, era) in [("Chandni Chowk", Era::Old), ("Hauz Khas", Era::Old), ("Karol Bagh", Era::Old), ("Tughlaqabad", Era::Old), ("Connaught Place", Era::Raj),
            ("Kartavya Path", Era::Raj), ("New Delhi Station", Era::Raj), ("Cyber City", Era::New), ("MG Road", Era::New), ("Sector 18", Era::New)] {
            assert_eq!(map.era_at(at(&map, name).xz()), Some(era), "{name}");
        }
        assert_eq!(map.era_at(Vec2::new(-1320.0, 250.0)), None, "the Ridge has none");
    }

    #[test]
    fn a_small_map_is_all_one_era() {
        assert_eq!(generate(MapId::Cp, 7).era_at(Vec2::ZERO), Some(Era::Raj));
        assert_eq!(generate(MapId::Chandni, 7).era_at(Vec2::new(40.0, 40.0)), Some(Era::Old));
    }

    /// Every class of ground answers, with no rooms on the map at all.
    #[test]
    fn ground_gives_the_place() {
        let t = |class: u8| {
            let mut map = MapLayout { style: MapStyle::Ncr, ..default() };
            map.terrain = crate::map::terrain::Terrain { n: 3, size: 8.0, h: vec![0.0; 9], road_d: vec![0.0; 9], ground: vec![class; 9] };
            map.place_of(Vec3::new(5000.0, 1.0, 5000.0))
        };
        let (road, lane, open) = (t(ground::ROAD), t(ground::PATH), t(ground::OPEN));
        assert_eq!((road, lane, open), (Place::Road, Place::Lane, Place::Open));
        assert_eq!((t(ground::PARKING), t(ground::PLAZA), t(ground::PARK), t(ground::SAND)), (Place::Road, Place::Lane, Place::Open, Place::Open));
    }

    #[test]
    fn a_pavement_by_a_road_is_the_roadside() {
        let n = 21;
        let mut ground_at = vec![ground::PAVEMENT; n * n];
        ground_at[..n * 3].fill(ground::ROAD);
        let mut map = MapLayout { style: MapStyle::Ncr, ..default() };
        map.terrain = crate::map::terrain::Terrain { n, size: 40.0, h: vec![0.0; n * n], road_d: vec![0.0; n * n], ground: ground_at };
        assert_eq!(map.place_of(Vec3::new(-10.0, 0.1, 0.0)), Place::Road, "6 m from the road");
        assert_eq!(map.place_of(Vec3::new(10.0, 0.1, 0.0)), Place::Lane, "26 m from it");
    }

    /// A hollow building is an outline with a floor in it: no special case, so rooms need nothing
    /// from here when they land.
    #[test]
    fn inside_an_outline_is_a_room_and_on_it_a_roof() {
        let pts = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let mut map = MapLayout { ..default() };
        map.footprints.push(Footprint { pts, base: 0.0, top: 9.0, mat: Mat::Plaster });
        assert_eq!(map.place_of(Vec3::new(5.0, 1.0, 5.0)), Place::Room);
        assert_eq!(map.place_of(Vec3::new(5.0, 7.0, 5.0)), Place::Room);
        assert_eq!(map.place_of(Vec3::new(5.0, 9.9, 5.0)), Place::Roof);
    }

    #[test]
    fn a_named_spot_gives_its_era_only_inside_it() {
        let map = MapLayout { pois: vec![Poi { name: "Saket".into(), center: Vec3::ZERO, radius: 10.0 }], ..default() };
        assert_eq!(map.era_at(Vec2::new(9.0, 9.0)), Some(Era::New));
        assert_eq!(map.era_at(Vec2::new(11.0, 0.0)), None);
    }
}
