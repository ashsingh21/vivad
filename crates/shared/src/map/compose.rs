//! The one NCR map: real districts from OpenStreetMap, each at true size and in its true compass
//! direction from Connaught Place, on a single 4 km square. The distance between them is
//! compressed, and that ground stays open land with roads, two rivers, bridges and a ridge, so
//! there is room to fight. Axes: x east, z south, metres from the middle of the map.
//! Map data (c) OpenStreetMap contributors, ODbL 1.0.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

use crate::SmallRng;

use super::build::{Builder, CHHATRI, Frame};
use super::terrain::{Terrain, fbm, ground};
use super::{ChestKind, ChestSpawn, Footprint, Mat, MapId, MapLayout, MapStyle, Poi, Prop, Shape, delhi, footprint, india, interiors, kit, parking};

pub const HALF: f32 = 2000.0;
/// The map is always built from this seed, so players can learn it. The seed a match passes in
/// only chooses which of the loot spots are used.
const LAYOUT_SEED: u64 = 7;
/// Metres between terrain samples. A district's own terrain is a metre apart.
const STEP: f32 = 2.0;
/// Boundary walls: the first blocks a district pushes, dropped because it has no edge here.
const TILE_WALLS: usize = 4;
/// The map's own edge, tall enough that nobody glides over it.
const WALL_TOP: f32 = 900.0;
/// Footprints at least this big (square metres) are never thinned out.
const LANDMARK_AREA: f32 = 900.0;
/// Houses built to go inside, at most, in a district.
const HOUSES: usize = 16;
/// Buildings to go inside, at most, in each 128 m square: the one number that turns rooms down.
/// Zero gives the map without them.
pub const ROOMS_PER_CELL: usize = 12;
/// Floor spots on the whole map: the districts' own, each named spot's and each bridge's, and the
/// roadsides and open land make up the rest.
const FLOOR_SPOTS: usize = 1600;
const SPOT_FLOOR: usize = 12;
const BRIDGE_FLOOR: usize = 4;
/// How many sets of chests each district has. A match opens one set in each, drawn from its seed
/// (`sim::loot::chest_sets`), so the chests are not where they were last time.
pub const CHEST_SETS: u8 = 3;

/// Where a district comes from.
#[derive(Clone, Copy)]
pub enum Source {
    Map(MapId),
    Data(&'static str),
}

/// How much of a district's loot and street life it keeps: hotter places hold more and draw more
/// drops.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tier {
    Hot,
    Warm,
    Quiet,
    Small,
}

impl Tier {
    fn chests(self) -> usize {
        match self {
            Tier::Hot => 9,
            Tier::Warm => 6,
            Tier::Quiet => 5,
            Tier::Small => 3,
        }
    }
    /// How many of its chests are golden.
    fn kalash(self) -> usize {
        match self {
            Tier::Hot => 2,
            Tier::Warm | Tier::Quiet => 1,
            Tier::Small => 0,
        }
    }
    fn floor(self) -> usize {
        match self {
            Tier::Hot => 80,
            Tier::Warm => 60,
            Tier::Quiet => 50,
            Tier::Small => 30,
        }
    }
    fn kit(self) -> usize {
        match self {
            Tier::Hot => 750,
            Tier::Warm => 480,
            Tier::Quiet => 330,
            Tier::Small => 150,
        }
    }
}

/// A district and where its middle sits on the map.
pub struct Place {
    pub name: &'static str,
    pub source: Source,
    /// Whole metres, so terrain samples line up.
    pub at: Vec2,
    /// How far out from its middle it is kept.
    pub clip: f32,
    pub tier: Tier,
    /// At most this many ordinary buildings. Big ones always stay, as they are what the place is
    /// known by; the rest are thinned so the streets have open ground between them.
    pub cap: usize,
}

const fn place(name: &'static str, source: Source, x: f32, z: f32, clip: f32, tier: Tier, cap: usize) -> Place {
    Place { name, source, at: Vec2::new(x, z), clip, tier, cap }
}

macro_rules! ncr_data {
    ($f:literal) => {
        Source::Data(include_str!(concat!("../../../../assets/data/maps/ncr_", $f, ".json")))
    };
}

pub const PLACES: [Place; 19] = [
    place("Connaught Place", Source::Map(MapId::Cp), -740.0, -700.0, 250.0, Tier::Hot, 60),
    place("Chandni Chowk", Source::Map(MapId::Chandni), -395.0, -1440.0, 250.0, Tier::Hot, 380),
    place("Kartavya Path", Source::Map(MapId::IndiaGate), -443.0, 0.0, 250.0, Tier::Warm, 6),
    place("Hauz Khas", Source::Map(MapId::HauzKhas), -980.0, 620.0, 250.0, Tier::Warm, 120),
    place("Karol Bagh", ncr_data!("karolbagh"), -1340.0, -1075.0, 250.0, Tier::Warm, 360),
    place("Nehru Place", ncr_data!("nehruplace"), -280.0, 600.0, 250.0, Tier::Warm, 70),
    place("Cyber City", ncr_data!("cybercity"), -1670.0, 960.0, 250.0, Tier::Hot, 49),
    place("MG Road", ncr_data!("mgroad"), -1720.0, 1570.0, 250.0, Tier::Warm, 220),
    place("Golf Course Road", ncr_data!("golfcourse"), -1160.0, 1640.0, 250.0, Tier::Quiet, 120),
    place("Laxmi Nagar", ncr_data!("laxminagar"), 680.0, -600.0, 250.0, Tier::Quiet, 240),
    place("Anand Vihar", ncr_data!("anandvihar"), 920.0, -1220.0, 250.0, Tier::Hot, 80),
    place("Indirapuram", ncr_data!("indirapuram"), 1220.0, -600.0, 250.0, Tier::Quiet, 50),
    place("Ghaziabad", ncr_data!("ghaziabad"), 1670.0, -1340.0, 250.0, Tier::Warm, 160),
    place("Sector 18", ncr_data!("noida18"), 1430.0, 360.0, 250.0, Tier::Hot, 70),
    place("Film City", ncr_data!("filmcity"), 880.0, 400.0, 200.0, Tier::Warm, 27),
    place("Expressway", ncr_data!("expressway"), 1360.0, 1240.0, 200.0, Tier::Small, 14),
    place("Pari Chowk", ncr_data!("greaternoida"), 1800.0, 1480.0, 180.0, Tier::Small, 16),
    place("Saket", ncr_data!("saket"), -520.0, 1130.0, 180.0, Tier::Small, 14),
    place("Aerocity", ncr_data!("aerocity"), -1780.0, 200.0, 200.0, Tier::Small, 12),
];

/// The Yamuna's centreline, with the shape it really has: down from Wazirabad, the loop east at
/// Old Delhi, past ITO, then the long run south-east.
const YAMUNA: [(f32, f32); 19] = [
    (60.0, -2040.0), (50.0, -1880.0), (90.0, -1700.0), (170.0, -1560.0), (230.0, -1420.0), (280.0, -1250.0), (290.0, -1080.0),
    (250.0, -880.0), (190.0, -640.0), (190.0, -330.0), (240.0, -60.0), (330.0, 180.0), (400.0, 400.0), (500.0, 620.0),
    (640.0, 880.0), (790.0, 1140.0), (980.0, 1460.0), (1170.0, 1800.0), (1300.0, 2040.0),
];
const YAMUNA_WATER: f32 = 100.0;
const YAMUNA_BANK: f32 = 225.0;
const HINDON: [(f32, f32); 9] = [
    (1230.0, -2040.0), (1285.0, -1600.0), (1295.0, -1000.0), (1420.0, -960.0), (1570.0, -880.0), (1590.0, -500.0), (1660.0, -200.0),
    (1820.0, 60.0), (2040.0, 240.0),
];
const HINDON_WATER: f32 = 40.0;
const HINDON_BANK: f32 = 70.0;
const WATER_BED: f32 = -1.2;
const WATER_TOP: f32 = -0.2;
/// The Delhi Ridge: green hills screening Delhi from Gurugram.
pub const RIDGE: [(f32, f32); 12] = [
    (-1230.0, -790.0), (-1210.0, -450.0), (-1230.0, -100.0), (-1320.0, 250.0), (-1330.0, 550.0), (-1300.0, 880.0), (-1220.0, 1060.0),
    (-1040.0, 1210.0), (-760.0, 1400.0), (-520.0, 1510.0), (-200.0, 1640.0), (200.0, 1720.0),
];
const RIDGE_HALF: f32 = 125.0;
const RIDGE_CREST: f32 = 24.0;
/// A deck's height over the banks' bunds, flush with them.
const DECK: f32 = 3.0;
/// How far the earth under a bridge slopes down to the water's edge.
const TOE: f32 = 16.0;
/// Where the Okhla wetland lies: reeds, and no buildings, street kit or loot. It is protected.
const MARSH: (f32, f32, f32) = (880.0, 690.0, 110.0);

/// A bridge: its two ends, the deck's width and the height of its top. It stands on earth where it
/// crosses land and on a deck where it crosses water; the road runs over both.
pub struct Span {
    pub a: Vec2,
    pub c: Vec2,
    pub width: f32,
    pub top: f32,
}

impl Span {
    pub fn len(&self) -> f32 {
        self.a.distance(self.c)
    }
    pub fn dir(&self) -> Vec2 {
        (self.c - self.a).normalize()
    }
    /// `along` metres from end a, and `off` metres to one side of the middle of the road.
    pub fn at(&self, along: f32, off: f32) -> Vec2 {
        self.a + self.dir() * along + self.dir().perp() * off
    }
    /// Turns a box's x along the bridge and its z across it.
    fn rot(&self) -> Quat {
        let d = self.dir();
        Quat::from_rotation_y((-d.y).atan2(d.x))
    }
    /// How long the road's slope up to each end is: 1 in 13 at the higher bridges, about what a
    /// real approach has.
    pub fn ramp(&self) -> f32 {
        (self.top * 13.4).max(8.0)
    }
}

const fn span(ax: f32, az: f32, cx: f32, cz: f32, width: f32, top: f32) -> Span {
    Span { a: Vec2::new(ax, az), c: Vec2::new(cx, cz), width, top }
}
/// The bridges' names on the map and in the atlas, in `SPANS` order.
pub const BRIDGES: [&str; 8] = ["Signature Bridge", "Loha Pul", "ITO Bridge", "DND Flyway", "Okhla Barrage", "GT Road Bridge", "Elevated Road Bridge", "NH-9 Bridge"];
pub const SPANS: [Span; 8] = [
    span(-190.0, -1890.0, 290.0, -1890.0, 16.0, DECK), // Signature Bridge
    span(-95.0, -1612.0, 395.0, -1612.0, 12.0, DECK),  // Loha Pul
    span(-50.0, -628.0, 385.0, -628.0, 18.0, DECK),    // ITO
    span(60.0, 130.0, 620.0, 250.0, 26.0, DECK),       // DND Flyway
    span(440.0, 970.0, 840.0, 790.0, 12.0, DECK),      // Okhla Barrage
    span(1225.0, -1250.0, 1375.0, -1250.0, 12.0, 0.4), // The Hindon's three
    span(1220.0, -1040.0, 1450.0, -1088.0, 12.0, 0.4),
    span(1500.0, -440.0, 1680.0, -470.0, 14.0, 0.4),
];

/// A connecting road: width and its points. Each end is on another road, on a bridge's approach,
/// at the map's edge, or at a real road's exit from a district.
type Link = (f32, &'static [(f32, f32)]);
const ROADS: [Link; 78] = [
    // Ring Road, a closed loop round Delhi, outside the west bund and past Chandni Chowk's east edge.
    (20.0, &[(0.0, 340.0), (21.0, 122.0), (-35.0, 0.0), (-80.0, -200.0), (-95.0, -400.0), (-95.0, -628.0), (-60.0, -800.0), (-5.0, -1000.0), (-60.0, -1120.0), (-135.0, -1210.0), (-135.0, -1250.0), (-135.0, -1650.0), (-135.0, -1680.0), (-150.0, -1725.0), (-190.0, -1800.0), (-232.0, -1890.0), (-330.0, -1860.0), (-900.0, -1800.0), (-1500.0, -1720.0), (-1760.0, -1400.0), (-1830.0, -760.0), (-1180.0, -100.0), (-1040.0, 240.0), (-600.0, 330.0), (0.0, 340.0)]),
    // NH-48 to Gurugram and off the map.
    (26.0, &[(-1180.0, -100.0), (-1300.0, 230.0), (-1440.0, 560.0), (-1700.0, 630.0), (-2000.0, 640.0)]),
    // Mathura Road, south off the map.
    (18.0, &[(0.0, 340.0), (0.0, 860.0), (-20.0, 1100.0), (-30.0, 1500.0), (-40.0, 1700.0), (-60.0, 2000.0)]),
    // NH-9 to Ghaziabad, over the Hindon and on east off the map.
    (24.0, &[(659.0, 258.0), (700.0, 130.0), (820.0, 0.0), (868.0, -260.0), (900.0, -335.0), (960.0, -335.0), (1300.0, -335.0), (1340.0, -335.0), (1480.0, -340.0), (1500.0, -440.0)]),
    (24.0, &[(1680.0, -470.0), (1760.0, -760.0), (1709.0, -1090.0)]),
    (24.0, &[(1680.0, -470.0), (2000.0, -500.0)]),
    // DND Flyway's road on along Film City to Sector 18.
    (22.0, &[(659.0, 258.0), (668.0, 195.0), (880.0, 195.0), (1100.0, 190.0), (1170.0, 100.0), (1250.0, 98.0), (1250.0, 110.0)]),
    // Okhla Barrage's east end, the bund road to Sector 18.
    (26.0, &[(876.0, 774.0), (1060.0, 760.0), (1250.0, 800.0), (1340.0, 610.0)]),
    // Noida Expressway.
    (16.0, &[(1471.0, 610.0), (1488.0, 1040.0)]),
    // Film City's south gate to the bund road, its east gate to Sector 18.
    (10.0, &[(987.0, 600.0), (999.0, 761.0)]),
    (10.0, &[(1080.0, 346.0), (1180.0, 414.0)]),
    // The Expressway towers' other gates, and Pari Chowk's roads off the map.
    (10.0, &[(1160.0, 1359.0), (1150.0, 1040.0), (1146.0, 785.0)]),
    (10.0, &[(1560.0, 1236.0), (1636.0, 1300.0)]),
    (14.0, &[(1415.0, 1440.0), (1520.0, 1700.0), (1560.0, 2000.0)]),
    (24.0, &[(1657.0, 1660.0), (1660.0, 2000.0)]),
    (24.0, &[(1980.0, 1346.0), (2000.0, 1346.0)]),
    // Noida East, Sector 18 to Pari Chowk.
    (16.0, &[(1680.0, 438.0), (1740.0, 420.0), (1800.0, 760.0), (1760.0, 1300.0)]),
    (16.0, &[(1740.0, 420.0), (2000.0, 420.0)]),
    // Noida North, Sector 18 up to NH-9.
    (14.0, &[(1557.0, 110.0), (1520.0, -100.0), (1482.0, -342.0)]),
    (10.0, &[(1370.0, 110.0), (1340.0, -200.0), (1340.0, -335.0)]),
    // East Spine, between Laxmi Nagar and Indirapuram, and the gates off it and NH-9.
    (14.0, &[(951.0, -336.0), (950.0, -400.0), (950.0, -790.0), (950.0, -860.0), (1090.0, -970.0)]),
    (14.0, &[(930.0, -790.0), (950.0, -790.0)]),
    (10.0, &[(970.0, -707.0), (950.0, -707.0)]),
    (7.0, &[(1062.0, -850.0), (1033.0, -930.0)]),
    (7.0, &[(767.0, -850.0), (777.0, -970.0)]),
    (14.0, &[(753.0, -350.0), (760.0, -322.0), (887.0, -317.0)]),
    (7.0, &[(1253.0, -350.0), (1253.0, -335.0)]),
    // Anand Vihar and Indirapuram to the Hindon's bridges, and on into Ghaziabad.
    (14.0, &[(1170.0, -1223.0), (1225.0, -1250.0)]),
    (14.0, &[(1170.0, -1153.0), (1220.0, -1040.0)]),
    (10.0, &[(1375.0, -1250.0), (1420.0, -1217.0)]),
    (10.0, &[(1450.0, -1088.0), (1460.0, -1090.0)]),
    (10.0, &[(1470.0, -723.0), (1490.0, -600.0), (1500.0, -440.0)]),
    // Wazirabad Road, Signature Bridge round to the Hindon.
    (14.0, &[(332.0, -1890.0), (660.0, -1760.0), (1120.0, -1760.0), (1195.0, -1560.0), (1200.0, -1300.0), (1225.0, -1250.0)]),
    // The GT Road link, Loha Pul to Anand Vihar.
    (14.0, &[(437.0, -1612.0), (520.0, -1560.0), (620.0, -1450.0), (670.0, -1400.0)]),
    // Pushta Road, along the east bund and Laxmi Nagar's west edge.
    (12.0, &[(332.0, -1890.0), (350.0, -1800.0), (400.0, -1720.0), (437.0, -1612.0), (485.0, -1500.0), (525.0, -1400.0), (560.0, -1200.0), (555.0, -1000.0), (500.0, -900.0), (450.0, -868.0), (423.0, -830.0), (423.0, -790.0), (423.0, -420.0), (423.0, -345.0), (445.0, -322.0), (470.0, -200.0), (530.0, 0.0), (600.0, 190.0), (659.0, 258.0)]),
    // Ghaziabad's roads off the map.
    (14.0, &[(1920.0, -1310.0), (2000.0, -1310.0)]),
    (14.0, &[(1711.0, -1590.0), (1711.0, -2000.0)]),
    // Vikas Marg, and the short links between the Delhi districts by their real road exits.
    (14.0, &[(-490.0, -534.0), (-330.0, -620.0), (-95.0, -628.0)]),
    (14.0, &[(-399.0, -250.0), (-395.0, -480.0), (-372.0, -604.0)]),
    (12.0, &[(-575.0, -450.0), (-568.0, -250.0)]),
    (12.0, &[(-896.0, -450.0), (-880.0, -23.0), (-693.0, -23.0)]),
    (12.0, &[(-1180.0, -100.0), (-1000.0, -60.0), (-880.0, -23.0)]),
    (12.0, &[(-636.0, 250.0), (-700.0, 400.0), (-730.0, 550.0)]),
    (10.0, &[(-415.0, 250.0), (-415.0, 340.0)]),
    (12.0, &[(-600.0, -950.0), (-580.0, -1060.0), (-594.0, -1190.0)]),
    (12.0, &[(-501.0, -950.0), (-430.0, -1080.0), (-353.0, -1190.0)]),
    (12.0, &[(-990.0, -920.0), (-1090.0, -947.0)]),
    (12.0, &[(-645.0, -1520.0), (-900.0, -1450.0), (-1090.0, -1258.0)]),
    (10.0, &[(-397.0, -1690.0), (-400.0, -1851.0)]),
    (10.0, &[(-1323.0, -1325.0), (-1333.0, -1768.0)]),
    (10.0, &[(-1464.0, -825.0), (-1516.0, -410.0)]),
    (10.0, &[(-1590.0, -945.0), (-1859.0, -1006.0)]),
    // Nehru Place, Saket and Hauz Khas, whose north and west gates are real footpaths.
    (12.0, &[(-30.0, 799.0), (1.0, 800.0)]),
    (12.0, &[(-146.0, 850.0), (-120.0, 940.0), (-12.0, 1000.0)]),
    (14.0, &[(-530.0, 696.0), (-640.0, 640.0), (-721.0, 499.0)]),
    (14.0, &[(-465.0, 850.0), (-515.0, 950.0)]),
    (16.0, &[(-700.0, 998.0), (-1000.0, 1180.0), (-1290.0, 1240.0), (-1470.0, 1400.0)]),
    (10.0, &[(-366.0, 1310.0), (-25.0, 1310.0)]),
    (8.0, &[(-1038.0, 870.0), (-1039.0, 1193.0)]),
    (8.0, &[(-1156.0, 370.0), (-1041.0, 240.0)]),
    (8.0, &[(-1440.0, 560.0), (-1230.0, 599.0)]),
    // The Gurugram districts and Aerocity.
    (14.0, &[(-1783.0, 710.0), (-1780.0, 637.0)]),
    (14.0, &[(-1420.0, 989.0), (-1370.0, 1100.0), (-1290.0, 1240.0)]),
    (10.0, &[(-1488.0, 1210.0), (-1602.0, 1320.0)]),
    (24.0, &[(-1920.0, 874.0), (-2000.0, 874.0)]),
    (20.0, &[(-1470.0, 1580.0), (-1410.0, 1654.0)]),
    (20.0, &[(-1970.0, 1663.0), (-2000.0, 1663.0)]),
    (10.0, &[(-1607.0, 1820.0), (-1607.0, 2000.0)]),
    (14.0, &[(-1290.0, 1240.0), (-1160.0, 1390.0)]),
    (14.0, &[(-1167.0, 1890.0), (-1170.0, 2000.0)]),
    (10.0, &[(-910.0, 1590.0), (-600.0, 1560.0), (-32.0, 1560.0)]),
    (10.0, &[(-1658.0, 400.0), (-1657.0, 625.0)]),
    (10.0, &[(-1580.0, 125.0), (-1261.0, 126.0)]),
    (14.0, &[(-1905.0, 0.0), (-1960.0, -100.0), (-2000.0, -100.0)]),
    (14.0, &[(-1980.0, 227.0), (-2000.0, 227.0)]),
    // Outer Ring Road to Okhla Barrage, the road past Badarpur, south off the map.
    (14.0, &[(2.0, 700.0), (160.0, 760.0), (300.0, 880.0), (403.0, 986.0)]),
    (12.0, &[(403.0, 986.0), (260.0, 1000.0), (260.0, 1500.0), (340.0, 1700.0), (420.0, 2000.0)]),
    (12.0, &[(-30.0, 1500.0), (260.0, 1500.0)]),
];

/// What stands at a named spot on the open land.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Yard {
    Trucks,
    Buses,
    Market,
    Malls,
    Offices,
    Hotels,
    Ruin,
    Fair,
    Station,
    Thermal,
    /// A junction: little but its name and a few walls.
    Junction,
    Mound,
    Marsh,
}

impl Yard {
    /// How far out from its middle the place spreads.
    fn reach(self) -> f32 {
        match self {
            Yard::Station => 110.0,
            Yard::Thermal => 90.0,
            _ => 46.0,
        }
    }
    /// What it is laid on; None leaves the land's surface as it is.
    fn paving(self) -> Option<u8> {
        match self {
            Yard::Trucks | Yard::Buses | Yard::Thermal => Some(ground::PARKING),
            Yard::Market | Yard::Fair => Some(ground::PLAZA),
            Yard::Malls | Yard::Offices | Yard::Hotels | Yard::Station => Some(ground::PAVEMENT),
            Yard::Ruin => Some(ground::PATH),
            Yard::Junction | Yard::Mound | Yard::Marsh => None,
        }
    }
}

/// Smaller named spots on the open land. Nothing here is a temple, mosque, gurdwara or grave.
const SPOTS: [(&str, f32, f32, Yard); 25] = [
    ("Azadpur Mandi", -1500.0, -1720.0, Yard::Trucks),
    ("Kamla Nagar", -900.0, -1800.0, Yard::Market),
    ("Kashmere Gate", -330.0, -1860.0, Yard::Buses),
    ("Rajouri Garden", -1830.0, -760.0, Yard::Malls),
    ("New Delhi Station", -680.0, -1060.0, Yard::Station),
    ("Mandi House", -330.0, -620.0, Yard::Junction),
    ("Dhaula Kuan", -1180.0, -100.0, Yard::Junction),
    ("Lajpat Nagar", -600.0, 330.0, Yard::Market),
    ("Sarai Kale Khan", 20.0, 60.0, Yard::Buses),
    ("Jasola", 160.0, 760.0, Yard::Offices),
    ("Tughlaqabad", -140.0, 1340.0, Yard::Ruin),
    ("Surajkund Mela Ground", 340.0, 1700.0, Yard::Fair),
    ("Badarpur Thermal", 260.0, 1250.0, Yard::Thermal),
    ("Mahipalpur", -1440.0, 560.0, Yard::Hotels),
    ("Sikanderpur", -1290.0, 1240.0, Yard::Offices),
    ("Shahdara", 660.0, -1760.0, Yard::Market),
    ("Raj Nagar", 1711.0, -1790.0, Yard::Market),
    ("Lal Kuan", 1760.0, -760.0, Yard::Market),
    ("Mayur Vihar", 820.0, 0.0, Yard::Market),
    ("Ghazipur Mound", 1000.0, -210.0, Yard::Mound),
    ("Sector 62", 1340.0, -200.0, Yard::Offices),
    ("City Centre", 1880.0, 420.0, Yard::Malls),
    ("Botanical Garden", 1766.0, 560.0, Yard::Junction),
    ("Kalindi Kunj", 1250.0, 800.0, Yard::Junction),
    ("Okhla Marsh", MARSH.0, MARSH.1, Yard::Marsh),
];

fn area(pts: &[Vec2]) -> f32 {
    let n = pts.len();
    (0..n).map(|k| pts[k].perp_dot(pts[(k + 1) % n])).sum::<f32>() / 2.0
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn v2s(pts: &[(f32, f32)]) -> Vec<Vec2> {
    pts.iter().map(|p| Vec2::new(p.0, p.1)).collect()
}

/// A polyline bent into a smooth curve, a point about every 20 m.
fn curve(pts: &[Vec2]) -> Vec<Vec2> {
    let at = |k: isize| pts[k.clamp(0, pts.len() as isize - 1) as usize];
    let mut out = vec![];
    for k in 0..pts.len() as isize - 1 {
        let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
        let n = (p1.distance(p2) / 20.0).ceil().max(1.0) as usize;
        for s in 0..n {
            let t = s as f32 / n as f32;
            out.push(0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t));
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// A road through its points, a point about every 20 m. Unlike `curve`, each bend is scaled by the
/// stretches either side of it, so a short stretch next to a long one cannot overshoot.
fn road_line(pts: &[Vec2]) -> Vec<Vec2> {
    let at = |k: isize| pts[k.clamp(0, pts.len() as isize - 1) as usize];
    let mut out = vec![];
    for k in 0..pts.len() as isize - 1 {
        let (p0, p1, p2, p3) = (at(k - 1), at(k), at(k + 1), at(k + 2));
        let (d0, d1, d2) = (p0.distance(p1), p1.distance(p2), p2.distance(p3));
        let (m1, m2) = ((p2 - p0) * d1 / (d0 + d1), (p3 - p1) * d1 / (d1 + d2));
        let n = (d1 / 20.0).ceil().max(1.0) as usize;
        for s in 0..n {
            let t = s as f32 / n as f32;
            let (t2, t3) = (t * t, t * t * t);
            out.push(p1 * (2.0 * t3 - 3.0 * t2 + 1.0) + m1 * (t3 - 2.0 * t2 + t) + p2 * (3.0 * t2 - 2.0 * t3) + m2 * (t3 - t2));
        }
    }
    out.push(*pts.last().unwrap());
    out
}

/// Every connecting road, as a width and a line: `ROADS`, the Ring Road first, and each bridge's
/// approach at each end.
pub fn network() -> Vec<(f32, Vec<Vec2>)> {
    let mut all: Vec<(f32, Vec<Vec2>)> = ROADS.iter().map(|(width, pts)| (*width, road_line(&v2s(pts)))).collect();
    for s in &SPANS {
        all.push((s.width, vec![s.a, s.at(-s.ramp(), 0.0)]));
        all.push((s.width, vec![s.c, s.at(s.len() + s.ramp(), 0.0)]));
    }
    all
}

/// The sample row at a coordinate, kept on the map.
fn row(t: &Terrain, c: f32) -> usize {
    ((c + t.size * 0.5) / t.step()).clamp(0.0, (t.n - 1) as f32) as usize
}

/// Writes into `field` the distance from each sample to a line, where it is under `reach` and less
/// than what is already there.
fn fill_distance(t: &Terrain, field: &mut [f32], line: &[Vec2], reach: f32) {
    let n = t.n;
    for s in line.windows(2) {
        let (a, c) = (s[0], s[1]);
        let (lo, hi) = (a.min(c) - Vec2::splat(reach), a.max(c) + Vec2::splat(reach));
        for i in row(t, lo.x)..=row(t, hi.x) {
            for j in row(t, lo.y)..=row(t, hi.y) {
                let p = Vec2::new(t.coord(i), t.coord(j));
                let u = c - a;
                let f = ((p - a).dot(u) / u.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let d = p.distance(a + u * f);
                let k = i * n + j;
                if d < field[k] {
                    field[k] = d;
                }
            }
        }
    }
}

/// Distance from every sample to a line, out to `reach`; far away beyond it.
fn distance_field(t: &Terrain, line: &[Vec2], reach: f32) -> Vec<f32> {
    let mut field = vec![1e4f32; t.n * t.n];
    fill_distance(t, &mut field, line, reach);
    field
}

/// A river cut into the land, its sand and bunds, and the water's surface.
fn river(map: &mut MapLayout, line: &[Vec2], water: f32, bank: f32, bund: f32) {
    let field = distance_field(&map.terrain, line, bank + 30.0);
    let t = &mut map.terrain;
    for (k, d) in field.iter().enumerate() {
        let d = *d;
        if d < water {
            t.ground[k] = ground::WATER;
            t.h[k] = WATER_BED + (WATER_TOP - 0.3 - WATER_BED) * smooth(water - 14.0, water, d);
        } else if d < bank {
            t.ground[k] = ground::SAND;
            t.h[k] = -0.5 + 0.5 * (d - water) / (bank - water) + bund * smooth(bank - 16.0, bank, d);
        } else if d < bank + 24.0 {
            t.h[k] = bund * (1.0 - smooth(bank + 4.0, bank + 24.0, d));
        }
    }
    // The surface, a strip of quads that share their edges, mitred at each point so that it stays
    // as wide round a bend as the channel cut for it, and a little longer at each end, as the
    // channel is.
    let reach = |end: Vec2, next: Vec2| end + (end - next).normalize_or_zero() * 8.0;
    let (first, last) = (reach(line[0], line[1]), reach(line[line.len() - 1], line[line.len() - 2]));
    let line: Vec<Vec2> = std::iter::once(first).chain(line.iter().copied()).chain(std::iter::once(last)).collect();
    for k in 0..line.len() - 1 {
        let (a, c) = (line[k], line[k + 1]);
        let side = |i: usize| {
            let (p, q) = (line[i.saturating_sub(1)], line[(i + 1).min(line.len() - 1)]);
            let (ahead, behind) = (q - line[i], line[i] - p);
            let (ahead, behind) = (ahead.try_normalize().or(behind.try_normalize()).unwrap_or(Vec2::X), behind.try_normalize().or(ahead.try_normalize()).unwrap_or(Vec2::X));
            let across = (ahead + behind).normalize_or_zero().perp();
            across * (water + 4.0) / across.dot(ahead.perp()).max(0.5)
        };
        let (sa, sc) = (side(k), side(k + 1));
        let mut pts = vec![a - sa, a + sa, c + sc, c - sc];
        if area(&pts) > 0.0 {
            pts.reverse();
        }
        map.footprints.push(Footprint { pts, base: WATER_BED, top: WATER_TOP, mat: Mat::Water });
    }
}

fn hills(map: &mut MapLayout) {
    let line = curve(&v2s(&RIDGE));
    let field = distance_field(&map.terrain, &line, RIDGE_HALF + 10.0);
    let t = &mut map.terrain;
    let n = t.n;
    let mound = Vec2::new(1000.0, -210.0);
    for i in 0..n {
        for j in 0..n {
            let k = i * n + j;
            let p = Vec2::new(t.coord(i), t.coord(j));
            if t.ground[k] == ground::WATER {
                continue;
            }
            let wobble = fbm(p.x / 90.0, p.y / 90.0, 3, 11) - 0.5;
            let d = field[k];
            let ridge = RIDGE_CREST * (0.7 + wobble) * (1.0 - smooth(0.0, RIDGE_HALF, d));
            // The Ghazipur mound, the only high ground east of the river.
            let heap = 38.0 * (1.0 - smooth(20.0, 110.0, p.distance(mound)));
            t.h[k] += ridge.max(heap);
        }
    }
}

/// Paint the connecting roads, flatten the ground under them and thin any hill they cross.
fn roads(map: &mut MapLayout, network: &[(f32, Vec<Vec2>)]) {
    let t = &mut map.terrain;
    let n = t.n;
    let mut field = vec![1e4f32; n * n];
    for (width, line) in network {
        let hw = *width * 0.5;
        // Wide enough that a 24 m ridge comes down to the road at a walkable slope.
        let reach = hw + 60.0;
        let (lo, hi) = line.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let (i0, i1) = (row(t, lo.x - reach), row(t, hi.x + reach));
        let (j0, j1) = (row(t, lo.y - reach), row(t, hi.y + reach));
        for i in i0..=i1 {
            field[i * n + j0..=i * n + j1].fill(1e4);
        }
        fill_distance(t, &mut field, line, reach);
        for i in i0..=i1 {
            for j in j0..=j1 {
                let k = i * n + j;
                let d = field[k];
                if d >= reach || t.ground[k] == ground::WATER {
                    continue;
                }
                let rise = smooth(hw + 2.0, reach, d);
                t.h[k] = t.h[k] * rise + t.h[k].clamp(0.0, 3.0) * (1.0 - rise);
                if d < hw {
                    t.ground[k] = ground::ROAD;
                    t.road_d[k] = 0.0;
                }
            }
        }
    }
}

/// The ground each named spot stands on: level, and paved or bare by what it is. A station is a
/// long thin place; the rest are round. The mound and the marsh are left as they lie.
fn pads(t: &mut Terrain) {
    const BLEND: f32 = 26.0;
    let n = t.n;
    for (_, x, z, yard) in SPOTS {
        if matches!(yard, Yard::Mound | Yard::Marsh) {
            continue;
        }
        let (c, r) = (Vec2::new(x, z), yard.reach());
        let squeeze = if yard == Yard::Station { Vec2::new(1.0, 1.8) } else { Vec2::ONE };
        let (i0, i1) = (row(t, x - r - BLEND), row(t, x + r + BLEND));
        let (j0, j1) = (row(t, z - r - BLEND), row(t, z + r + BLEND));
        for i in i0..=i1 {
            for j in j0..=j1 {
                let k = i * n + j;
                let p = Vec2::new(t.coord(i), t.coord(j));
                let d = ((p - c) * squeeze).length();
                if d >= r + BLEND || t.ground[k] == ground::WATER {
                    continue;
                }
                t.h[k] *= smooth(r, r + BLEND, d);
                if let Some(class) = yard.paving()
                    && d < r * (0.72 + 0.56 * fbm(p.x / 14.0, p.y / 14.0, 2, 31))
                    && t.ground[k] != ground::ROAD
                {
                    t.ground[k] = class;
                }
            }
        }
    }
}

/// How far along a span it first and last crosses water.
fn wet(t: &Terrain, s: &Span) -> (f32, f32) {
    let len = s.len();
    let mut over = (0..len as usize).map(|d| d as f32).filter(|d| {
        let p = s.at(*d, 0.0);
        t.ground_at(p.x, p.y) == Some(ground::WATER)
    });
    let first = over.next().unwrap_or(len * 0.5);
    (first, over.last().unwrap_or(first))
}

/// The stretch of a span that needs a deck: over the water and the foot of the earth either side.
fn decked(t: &Terrain, s: &Span) -> (f32, f32) {
    let (w0, w1) = wet(t, s);
    ((w0 - TOE - 2.0).max(0.0), (w1 + TOE + 2.0).min(s.len()))
}

/// Bridges on the ground: earth under the road where a span crosses land, down to the water's edge
/// under the deck, and a slope up to it past each end. The road is painted all the way over.
fn approaches(t: &mut Terrain) {
    const SKIRT: f32 = 12.0;
    let n = t.n;
    for s in &SPANS {
        let (len, hw, ramp) = (s.len(), s.width * 0.5, s.ramp());
        let (dir, side) = (s.dir(), s.dir().perp());
        let (w0, w1) = wet(t, s);
        let reach = Vec2::splat(ramp + hw + SKIRT);
        let (lo, hi) = (s.a.min(s.c) - reach, s.a.max(s.c) + reach);
        let (i0, i1) = (row(t, lo.x), row(t, hi.x));
        let (j0, j1) = (row(t, lo.y), row(t, hi.y));
        for i in i0..=i1 {
            for j in j0..=j1 {
                let k = i * n + j;
                let p = Vec2::new(t.coord(i), t.coord(j));
                let (along, off) = ((p - s.a).dot(dir), (p - s.a).dot(side).abs());
                let past = (-along).max(along - len).max(0.0);
                if past >= ramp || off >= hw + SKIRT || t.ground[k] == ground::WATER {
                    continue;
                }
                let earth = s.top * (1.0 - smooth(0.0, ramp, past)) * smooth(0.0, TOE, (w0 - along).max(along - w1));
                let flank = smooth(hw, hw + SKIRT, off);
                t.h[k] = t.h[k].max(earth * (1.0 - flank) + t.h[k] * flank);
                if off < hw {
                    t.ground[k] = ground::ROAD;
                    t.road_d[k] = 0.0;
                }
            }
        }
    }
}

/// How far a point is inside a district's kept square (negative outside).
fn inside_by(place: &Place, p: Vec2) -> f32 {
    place.clip - (p - place.at).abs().max_element()
}

/// Copy one district into the big map. It is cut down before it is built (`delhi::Cut`), so all
/// that is left to do here is to draw its share of the loot and move it into place.
fn add(map: &mut MapLayout, place: &Place, index: usize, seed: u64) {
    let cut = delhi::Cut { clip: place.clip, cap: place.cap, big: LANDMARK_AREA, houses: HOUSES, kit: place.tier.kit(), floor: place.tier.floor() * 2 };
    let mut t = match place.source {
        Source::Map(id) => delhi::generate_cut(id, LAYOUT_SEED, &cut),
        Source::Data(json) => delhi::district(json, LAYOUT_SEED, &cut),
    };
    let at = Vec3::new(place.at.x, 0.0, place.at.y);
    let inside = |p: Vec2| p.abs().max_element() <= place.clip;
    let mut rng = SmallRng::seed_from_u64(seed ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    t.blocks.drain(..TILE_WALLS);
    for mut b in t.blocks.drain(..).filter(|b| inside(b.pos.xz())) {
        b.pos += at;
        map.blocks.push(b);
    }
    // With rooms in the map nothing has a door to break: leaves swing open by themselves, and bots
    // go in and out of the old houses too.
    for mut b in t.doors.drain(..).filter(|b| inside(b.pos.xz()) && ROOMS_PER_CELL == 0) {
        b.pos += at;
        map.doors.push(b);
    }
    map.closed.extend(t.closed.iter().map(|c| *c + place.at));
    map.ring_shops.extend(t.ring_shops.iter().map(|s| interiors::RingShop { back: s.back + at, ..*s }));
    // Nothing behind a wall: a chest set down at the foot of one building can be in the next.
    let mut chests: Vec<_> = t.chests.iter().filter(|c| inside(c.pos.xz()) && !in_building(&t, c.pos)).cloned().collect();
    let mut floor: Vec<_> = t.floor_loot.iter().copied().filter(|p| inside(p.xz()) && !in_building(&t, *p)).collect();
    for set in 0..CHEST_SETS {
        for mut c in choose_chests(&mut chests, place.tier, &mut rng) {
            c.pos += at;
            c.set = Some((index as u8, set));
            map.chests.push(c);
        }
    }
    shuffle(&mut floor, &mut rng);
    map.floor_loot.extend(floor.into_iter().take(place.tier.floor()).map(|p| p + at));
    for mut p in t.props.drain(..).filter(|p| inside(p.pos.xz())) {
        p.pos += at;
        map.props.push(p);
    }
    for mut o in t.ornaments.drain(..).filter(|o| inside(o.pos.xz())) {
        o.pos += at;
        map.ornaments.push(o);
    }
    for mut l in t.lamps.drain(..).filter(|l| inside(l.pos.xz())) {
        l.pos += at;
        map.lamps.push(l);
    }
    for mut k in t.kit.drain(..).filter(|k| inside(k.pos.xz())) {
        k.pos += at;
        map.kit.push(k);
    }
    for mut f in t.footprints.drain(..) {
        // A lake or a canal goes on past the edge: what is left of it here still has its surface.
        // A building goes whole or not at all.
        if f.mat == Mat::Water {
            f.pts = clip_outline(&f.pts, place.clip);
        }
        if f.pts.len() < 3 || !f.pts.iter().all(|p| inside(*p)) {
            continue;
        }
        f.pts.iter_mut().for_each(|p| *p += place.at);
        map.footprints.push(f);
    }
    stamp(&mut map.terrain, &t.terrain, place);
    map.pois.push(Poi { name: place.name.into(), center: at, radius: place.clip });
}

/// How many buildings a district's empty plots are filled with, at most: OpenStreetMap has few of
/// an Indian city's buildings (Noida's Sector 18 has 77, Cyber City 32), so most of a district was
/// bare paving between its streets.
fn infill_count(tier: Tier) -> usize {
    match tier {
        Tier::Hot => 320,
        Tier::Warm => 260,
        Tier::Quiet => 200,
        Tier::Small => 80,
    }
}

/// The empty plots of a district from OpenStreetMap built up: blocks of three to seven storeys set
/// back from the street and squared to it (along the distance-to-road field's contours), a lane
/// between neighbours, off the parks, plazas and car parks and clear of everything already there
/// (buildings, street kit, trees, holy sites). Not of the loot, which the match's seed moves (the
/// layout doesn't): a find that ends up inside one is moved out past its nearest wall.
fn infill(map: &mut MapLayout, place: &Place) {
    /// Metres a cell of the plot grid; setback from a road's middle line field; lane between.
    const CELL: f32 = 2.0;
    const SETBACK: f32 = 5.0;
    const LANE: f32 = 2.0;
    let mut rng = SmallRng::seed_from_u64(LAYOUT_SEED ^ 0x1F11 ^ (place.at.x as i64 as u64).wrapping_mul(31) ^ (place.at.y as i64 as u64));
    let half = place.clip;
    let n = (2.0 * half / CELL) as i32;
    let cell = |p: Vec2| {
        let q = ((p - place.at + Vec2::splat(half)) / CELL).floor();
        (q.x as i32, q.y as i32)
    };
    let centre = |i: i32, j: i32| place.at - Vec2::splat(half) + (Vec2::new(i as f32, j as f32) + 0.5) * CELL;
    let mut blocked = vec![false; (n * n) as usize];
    let mark = |blocked: &mut Vec<bool>, p: Vec2, r: f32| {
        let (ci, cj) = cell(p);
        let k = (r / CELL).ceil() as i32;
        for i in (ci - k).max(0)..=(ci + k).min(n - 1) {
            for j in (cj - k).max(0)..=(cj + k).min(n - 1) {
                if centre(i, j).distance(p) <= r + CELL * 0.71 {
                    blocked[(i * n + j) as usize] = true;
                }
            }
        }
    };
    let sites = holy();
    for i in 0..n {
        for j in 0..n {
            let p = centre(i, j);
            let t = &map.terrain;
            let k = (row(t, p.x) * t.n + row(t, p.y)) as usize;
            let free = matches!(t.ground[k], ground::PAVEMENT | ground::OPEN) && t.road_d[k] > SETBACK && inside_by(place, p) > 14.0 && sites.iter().all(|(c, r)| p.distance(*c) > r + 6.0);
            if !free {
                blocked[(i * n + j) as usize] = true;
            }
        }
    }
    let near = |p: Vec2| (p - place.at).abs().max_element() < half + 10.0;
    for f in map.footprints.iter().filter(|f| f.mat != Mat::Water && near(f.pts[0])) {
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let mut y = lo.y - 4.0;
        while y <= hi.y + 4.0 {
            let mut x = lo.x - 4.0;
            while x <= hi.x + 4.0 {
                mark(&mut blocked, Vec2::new(x, y), 0.0);
                x += CELL;
            }
            y += CELL;
        }
    }
    let things: Vec<(Vec2, f32)> = map.kit.iter().map(|k| (k.pos.xz(), 2.0))
        .chain(map.props.iter().map(|p| (p.pos.xz(), if p.trunk.is_some() { 4.0 } else { 1.5 })))
        .chain(map.lamps.iter().map(|l| (l.pos.xz(), 1.5)))
        .chain(map.blocks.iter().filter(|b| b.collide).map(|b| (b.pos.xz(), 2.0)))
        .filter(|(p, _)| near(*p))
        .collect();
    for (p, r) in things {
        mark(&mut blocked, p, r);
    }
    // Nearest the street first, so they line it; then further in.
    let mut order: Vec<(f32, (i32, i32))> = (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .filter(|(i, j)| !blocked[(i * n + j) as usize])
        .map(|(i, j)| {
            let p = centre(i, j);
            (map.terrain.road_d[row(&map.terrain, p.x) * map.terrain.n + row(&map.terrain, p.y)] + rng.random_range(0.0..8.0), (i, j))
        })
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let order: Vec<(i32, i32)> = order.into_iter().map(|o| o.1).collect();
    let wash = [Mat::Concrete, Mat::Plaster, Mat::PlasterOchre, Mat::PlasterBlue, Mat::Brick, Mat::Paint(0), Mat::Paint(1), Mat::Paint(3), Mat::Paint(4), Mat::Paint(5)];
    let first = map.footprints.len();
    let mut built = 0;
    // Big blocks first everywhere, then middling ones, then small ones in what is left.
    let sizes: [(std::ops::Range<f32>, std::ops::Range<f32>); 3] = [(22.0..32.0, 14.0..20.0), (14.0..22.0, 11.0..15.0), (9.0..13.0, 8.0..11.0)];
    for (i, j) in sizes.iter().flat_map(|s| order.iter().map(move |o| (s, *o))).map(|(s, (i, j))| ((s, i), j)) {
        let ((size, i), j) = (i, j);
        if built >= infill_count(place.tier) {
            break;
        }
        if blocked[(i * n + j) as usize] {
            continue;
        }
        let c = centre(i, j);
        // Squared to the nearest road: along the contours of the distance to it.
        let t = &map.terrain;
        let rd = |q: Vec2| t.road_d[row(t, q.x) * t.n + row(t, q.y)];
        let grad = Vec2::new(rd(c + Vec2::X * 3.0) - rd(c - Vec2::X * 3.0), rd(c + Vec2::Y * 3.0) - rd(c - Vec2::Y * 3.0));
        let u = grad.perp().try_normalize().unwrap_or(Vec2::X);
        let v = u.perp();
        let fits = |w: f32, d: f32, blocked: &Vec<bool>| {
            let (nu, nv) = ((w / CELL).ceil() as i32, (d / CELL).ceil() as i32);
            (0..=nu).all(|a| (0..=nv).all(|b| {
                let q = c + u * (a as f32 / nu as f32 - 0.5) * w + v * (b as f32 / nv as f32 - 0.5) * d;
                let (qi, qj) = cell(q);
                (0..n).contains(&qi) && (0..n).contains(&qj) && !blocked[(qi * n + qj) as usize]
            }))
        };
        let (w, d) = (rng.random_range(size.0.clone()), rng.random_range(size.1.clone()));
        if !fits(w, d, &blocked) {
            continue;
        }
        let (hu, hv) = (u * w * 0.5, v * d * 0.5);
        let mut pts = vec![c - hu - hv, c + hu - hv, c + hu + hv, c - hu + hv];
        if area(&pts) > 0.0 {
            pts.reverse();
        }
        let low = pts.iter().map(|q| map.terrain.height(q.x, q.y)).fold(f32::MAX, f32::min) - 0.3;
        let floors = [3, 3, 4, 4, 4, 5, 5, 6, 7][rng.random_range(0..9)] as f32;
        map.footprints.push(Footprint { pts, base: low, top: low + 0.3 + floors * 3.2 + 0.9, mat: wash[rng.random_range(0..wash.len())] });
        built += 1;
        // Its plot and a lane round it are taken.
        let r = (w.max(d) * 0.5 + LANE) * 1.0;
        let (ci, cj) = cell(c);
        let k = (r / CELL).ceil() as i32 + 1;
        for a in (ci - k).max(0)..=(ci + k).min(n - 1) {
            for b in (cj - k).max(0)..=(cj + k).min(n - 1) {
                let q = centre(a, b) - c;
                if q.dot(u).abs() <= w * 0.5 + LANE + CELL * 0.5 && q.dot(v).abs() <= d * 0.5 + LANE + CELL * 0.5 {
                    blocked[(a * n + b) as usize] = true;
                }
            }
        }
    }
    // Loot that a new block stands on steps out past its nearest wall.
    let new = &map.footprints[first..];
    let step_out = |p: Vec3| -> Vec3 {
        let Some(f) = new.iter().find(|f| footprint::contains(&f.pts, p.xz())) else { return p };
        let n = f.pts.len();
        let (q, out) = (0..n)
            .map(|k| {
                let (a, b) = (f.pts[k], f.pts[(k + 1) % n]);
                let t = ((p.xz() - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                let q = a + (b - a) * t;
                let mid = f.pts.iter().sum::<Vec2>() / n as f32;
                (q, (q - mid).normalize_or_zero())
            })
            .min_by(|x, y| x.0.distance(p.xz()).total_cmp(&y.0.distance(p.xz())))
            .unwrap();
        let q = q + out * 1.2;
        Vec3::new(q.x, p.y, q.y)
    };
    let moved: Vec<Vec3> = map.chests.iter().map(|c| step_out(c.pos)).collect();
    for (c, p) in map.chests.iter_mut().zip(moved) {
        c.pos = p;
    }
    let moved: Vec<Vec3> = map.floor_loot.iter().map(|p| step_out(*p)).collect();
    map.floor_loot = moved;
}

/// What is left of an outline inside the square of half-size `half` round the origin.
fn clip_outline(pts: &[Vec2], half: f32) -> Vec<Vec2> {
    let mut out = pts.to_vec();
    for (axis, side) in [(0, 1.0), (0, -1.0), (1, 1.0), (1, -1.0)] {
        let keeps = |p: Vec2| p[axis] * side <= half;
        let was = std::mem::take(&mut out);
        for (k, &p) in was.iter().enumerate() {
            let q = was[(k + 1) % was.len()];
            if keeps(p) {
                out.push(p);
            }
            if keeps(p) != keeps(q) {
                let mut cut = p.lerp(q, (half * side - p[axis]) / (q[axis] - p[axis]));
                cut[axis] = half * side;
                out.push(cut);
            }
        }
    }
    out
}

/// A seeded draw of one set from a district's chests, spread out and with its golden ones among
/// them. The set is taken out of `chests`, so the next set stands somewhere else.
fn choose_chests(chests: &mut Vec<ChestSpawn>, tier: Tier, rng: &mut SmallRng) -> Vec<ChestSpawn> {
    const GAP: f32 = 40.0;
    shuffle(chests, rng);
    chests.sort_by_key(|c| c.kind != ChestKind::Tijori);
    let (mut picked, mut rest): (Vec<ChestSpawn>, Vec<ChestSpawn>) = (vec![], vec![]);
    for c in chests.drain(..) {
        if picked.len() < tier.chests() && picked.iter().all(|p| p.pos.distance(c.pos) >= GAP) {
            picked.push(c);
        } else {
            rest.push(c);
        }
    }
    let short = tier.chests().saturating_sub(picked.len());
    picked.extend(rest.drain(..short.min(rest.len())));
    *chests = rest;
    let gold = picked.iter().filter(|c| c.kind == ChestKind::Tijori).count();
    picked.iter_mut().rev().take(tier.kalash().saturating_sub(gold)).for_each(|c| c.kind = ChestKind::Tijori);
    picked
}

pub(super) fn shuffle<T>(items: &mut [T], rng: &mut SmallRng) {
    for k in (1..items.len()).rev() {
        items.swap(k, rng.random_range(0..=k));
    }
}

/// Whether a district's road near its edge leads out onto a connecting road.
fn leads_out(world: &Terrain, place: &Place, p: Vec2) -> bool {
    let local = p - place.at;
    let out = place.clip + 4.0;
    let edge = place.at + if local.x.abs() > local.y.abs() { Vec2::new(out.copysign(local.x), local.y) } else { Vec2::new(local.x, out.copysign(local.y)) };
    (-3..=3).any(|i| (-3..=3).any(|j| world.ground_at(edge.x + i as f32 * 4.0, edge.y + j as f32 * 4.0) == Some(ground::ROAD)))
}

/// Copy a district's ground into the big map's. Its plain pavement, its lawns and any lake cut off
/// at the edge fray out into the land, so no square outline shows, and so does any road that leads
/// nowhere, which ends in the district's own scruffy edge and not at a straight cut.
fn stamp(world: &mut Terrain, tile: &Terrain, place: &Place) {
    const FRAY: f32 = 36.0;
    let n = world.n;
    let at = place.at;
    let idx = |c: f32| ((c + world.size * 0.5) / STEP).clamp(0.0, (n - 1) as f32) as usize;
    let (i0, i1) = (idx(at.x - place.clip), idx(at.x + place.clip));
    let (j0, j1) = (idx(at.y - place.clip), idx(at.y + place.clip));
    let local = |c: f32| (((c + tile.size * 0.5) / tile.step()).round() as usize).min(tile.n - 1);
    for i in i0..=i1 {
        let x = world.coord(i);
        let li = local(x - at.x);
        for j in j0..=j1 {
            let z = world.coord(j);
            let lj = local(z - at.y);
            let (w, l) = (i * n + j, li * tile.n + lj);
            let p = Vec2::new(x, z);
            let edge = inside_by(place, p);
            // Frayed only where the district's ground is level with the land round it: a hill of its
            // own (the Delhi maps' parks have them) is kept whole, or what stands on it floats.
            if edge < FRAY && fbm(x / 40.0, z / 40.0, 3, 5) * FRAY > edge && (tile.h[l] - world.h[w]).abs() < 0.05 {
                let class = tile.ground[l];
                if matches!(class, ground::PAVEMENT | ground::PARK | ground::WATER) || class == ground::ROAD && !leads_out(world, place, p) {
                    continue;
                }
            }
            world.h[w] = tile.h[l];
            world.road_d[w] = tile.road_d[l];
            world.ground[w] = tile.ground[l];
        }
    }
}

/// Hills and bunds settle down to the street at a district's edge.
fn level_districts(t: &mut Terrain) {
    const SKIRT: f32 = 60.0;
    let n = t.n;
    for i in 0..n {
        for j in 0..n {
            let p = Vec2::new(t.coord(i), t.coord(j));
            let near = PLACES.iter().map(|d| inside_by(d, p)).fold(f32::MIN, f32::max);
            if near > -SKIRT {
                t.h[i * n + j] *= smooth(0.0, SKIRT, -near);
            }
        }
    }
}

/// A straight member from one point to another, of any tilt.
fn strut(b: &mut Builder, from: Vec3, to: Vec3, thick: f32, mat: Mat, collide: bool) {
    let d = to - from;
    b.push((from + to) * 0.5, Quat::from_rotation_arc(Vec3::Y, d.normalize()), Shape::Box(Vec3::new(thick, d.length(), thick)), mat, collide, true);
}

/// A box in a bridge's own frame: `along` it, `off` to one side, centred at height `y`, and sized
/// along, up and across.
fn part(b: &mut Builder, s: &Span, along: f32, off: f32, y: f32, size: Vec3, mat: Mat, collide: bool) {
    let q = s.at(along, off);
    b.push(Vec3::new(q.x, y, q.y), s.rot(), Shape::Box(size), mat, collide, true);
}

/// The deck over the water, in short panels as one long collider snags a sweeping capsule, with
/// its parapets and, if it wants them, a pier under every other panel.
fn deck(b: &mut Builder, s: &Span, from: f32, to: f32, rail: Mat, piers: bool) {
    let f = Frame::new(Vec3::ZERO, 0.0);
    let panels = ((to - from) / 20.0).ceil() as usize;
    let step = (to - from) / panels as f32;
    for k in 0..panels {
        let along = from + (k as f32 + 0.5) * step;
        part(b, s, along, 0.0, s.top - 0.25, Vec3::new(step + 0.02, 0.5, s.width), Mat::Concrete, true);
        for e in [-1.0, 1.0] {
            part(b, s, along, e * (s.width * 0.5 - 0.15), s.top + 0.5, Vec3::new(step, 1.0, 0.3), rail, true);
        }
        if piers && k % 2 == 0 && s.top > 1.0 {
            let c = s.at(along, 0.0);
            b.cyl(f, Vec3::new(c.x, WATER_BED, c.y), 1.6, s.top - 0.5 - WATER_BED, Mat::Concrete);
        }
    }
}

/// Something to get behind every 40 to 60 m, against each parapet in turn, which keeps the middle
/// of the road clear. Returns how far along each piece stands.
fn cover(b: &mut Builder, s: &Span, rng: &mut SmallRng) -> Vec<f32> {
    const KINDS: [&str; 4] = ["auto", "thela", "auto", "crates"];
    let (off, yaw) = (s.width * 0.5 - 1.4, kit::facing(s.dir()));
    let mut along = rng.random_range(20.0..40.0);
    let mut placed = vec![];
    let mut side = 1.0;
    while along < s.len() - 5.0 {
        let q = s.at(along, side * off);
        kit::place(b, KINDS[placed.len() % KINDS.len()], Vec3::new(q.x, s.top, q.y), yaw);
        placed.push(along);
        side = -side;
        along += rng.random_range(40.0..60.0);
    }
    placed
}

/// A chest halfway along, between two pieces of cover, and a few finds down its length.
fn finds(b: &mut Builder, s: &Span, kind: ChestKind, placed: &[f32]) {
    let f = Frame::new(Vec3::ZERO, 0.0);
    let middle = placed.len() / 2;
    let along = (placed[middle.saturating_sub(1)] + placed[middle]) * 0.5;
    let q = s.at(along, s.width * 0.5 - 1.2);
    b.chest(f, Vec3::new(q.x, s.top, q.y), 0.0, kind);
    for k in 0..BRIDGE_FLOOR {
        let q = s.at(s.len() * (k as f32 + 1.0) / (BRIDGE_FLOOR as f32 + 1.0), 3.2);
        b.loot(f, Vec3::new(q.x, s.top + 0.1, q.y));
    }
}

/// Signature Bridge: two legs lean together over the deck to a point 154 m up, and cables fan down
/// to both edges of the deck, both ways along it. It has no piers in the river.
fn signature(b: &mut Builder, s: &Span) {
    let (mid, hw) = (s.len() * 0.5, s.width * 0.5);
    let top = s.at(mid, 0.0);
    let apex = Vec3::new(top.x, 154.0, top.y);
    for e in [-1.0, 1.0] {
        let foot = s.at(mid, e * (hw + 3.0));
        let foot = Vec3::new(foot.x, WATER_BED, foot.y);
        strut(b, foot, apex, 3.2, Mat::Steel, true);
        for n in 1..=8 {
            let high = foot.lerp(apex, 0.3 + 0.075 * n as f32);
            for way in [-1.0, 1.0] {
                let low = s.at(mid + way * 14.0 * n as f32, e * (hw - 0.1));
                strut(b, high, Vec3::new(low.x, s.top + 0.9, low.y), 0.22, Mat::Steel, false);
            }
        }
    }
    b.push(apex + Vec3::Y * 6.0, s.rot(), Shape::Box(Vec3::new(9.0, 12.0, 9.0)), Mat::Glass, true, true);
}

/// Loha Pul: the road runs inside a riveted iron truss, panel after panel. Each side has posts, a
/// top chord and diagonals, and beams tie the sides together overhead.
fn loha_pul(b: &mut Builder, s: &Span, from: f32, to: f32) {
    const HIGH: f32 = 7.5;
    let off = s.width * 0.5 + 0.25;
    let panels = ((to - from) / 20.0).ceil() as usize;
    let step = (to - from) / panels as f32;
    for k in 0..=panels {
        let along = from + k as f32 * step;
        for e in [-1.0, 1.0] {
            part(b, s, along, e * off, s.top + HIGH * 0.5, Vec3::new(0.5, HIGH, 0.5), Mat::Steel, true);
            if k < panels {
                part(b, s, along + step * 0.5, e * off, s.top + HIGH, Vec3::new(step, 0.5, 0.5), Mat::Steel, false);
                let (low, high) = if k % 2 == 0 { (along, along + step) } else { (along + step, along) };
                let (p, q) = (s.at(low, e * off), s.at(high, e * off));
                strut(b, Vec3::new(p.x, s.top, p.y), Vec3::new(q.x, s.top + HIGH, q.y), 0.4, Mat::Steel, false);
            }
        }
        if k % 2 == 0 {
            part(b, s, along, 0.0, s.top + HIGH, Vec3::new(0.5, 0.5, 2.0 * off), Mat::Steel, false);
        }
    }
}

/// South of ITO the Blue Line crosses on tall piers, train only, breaking the long view down the
/// river. It ends at a station on each bank, Indraprastha and Yamuna Bank, as it really does.
fn blue_viaduct(b: &mut Builder) {
    const Z: f32 = -470.0;
    const TOP: f32 = 11.0;
    let f = Frame::new(Vec3::ZERO, 0.0);
    let (west, east) = (-50.0, 398.0);
    for x in [west + 14.0, east - 14.0] {
        b.bx(f, Vec3::new(x, TOP * 0.5, Z), Vec3::new(52.0, TOP, 15.0), Mat::Concrete);
        b.bx(f, Vec3::new(x, TOP + 3.1, Z), Vec3::new(52.0, 0.5, 13.0), Mat::Steel);
        for along in [-24.0, -8.0, 8.0, 24.0] {
            for side in [-5.8, 5.8] {
                b.cyl(f, Vec3::new(x + along, TOP, Z + side), 0.25, 3.0, Mat::Steel);
            }
        }
        for side in [-1.0, 1.0] {
            b.deco(f, Vec3::new(x, TOP - 2.2, Z + side * 7.56), Vec3::new(52.0, 1.1, 0.12), Mat::TransitBlue);
        }
    }
    let bays = ((east - west) / 40.0f32).round() as usize;
    let step = (east - west) / bays as f32;
    for k in 0..=bays {
        let x = west + k as f32 * step;
        b.cyl(f, Vec3::new(x, WATER_BED, Z), 1.5, TOP - 1.8 - WATER_BED, Mat::Concrete);
        b.bx(f, Vec3::new(x, TOP - 1.4, Z), Vec3::new(2.4, 0.8, 7.0), Mat::Concrete);
        if k < bays {
            b.bx(f, Vec3::new(x + step * 0.5, TOP - 0.5, Z), Vec3::new(step + 0.02, 1.0, 6.0), Mat::Concrete);
            b.deco(f, Vec3::new(x + step * 0.5, TOP + 0.1, Z), Vec3::new(step, 0.2, 6.2), Mat::TransitBlue);
        }
    }
}

/// DND Flyway: two low medians with the centre lane open between them, and at its Noida end a toll
/// plaza under one wide canopy.
fn dnd(b: &mut Builder, s: &Span) {
    const HIGH: f32 = 6.4;
    let len = s.len();
    let pieces = (len / 20.0).ceil() as usize;
    let step = len / pieces as f32;
    for k in 0..pieces {
        for e in [-1.0, 1.0] {
            part(b, s, (k as f32 + 0.5) * step, e * 2.5, s.top + 0.45, Vec3::new(step, 0.9, 0.4), Mat::Concrete, true);
        }
    }
    let plaza = len - 28.0;
    part(b, s, plaza, 0.0, s.top + HIGH + 0.3, Vec3::new(16.0, 0.6, s.width + 2.0), Mat::Steel, true);
    part(b, s, plaza, 0.0, s.top + HIGH + 0.8, Vec3::new(16.6, 0.5, s.width + 2.6), Mat::TransitYellow, false);
    for off in [-10.0, -5.5, 5.5, 10.0] {
        part(b, s, plaza, off, s.top + 1.4, Vec3::new(3.4, 2.8, 1.8), Mat::Plaster, true);
        for along in [-6.5, 6.5] {
            part(b, s, plaza + along, off, s.top + HIGH * 0.5, Vec3::new(0.5, HIGH, 0.5), Mat::Concrete, true);
        }
    }
}

/// Okhla Barrage: a low road over a row of gates. A pier every 18 m runs through the water, posts
/// rise past the deck to a gantry over it, and a steel gate hangs between each pair on the lake side.
fn okhla(b: &mut Builder, s: &Span, from: f32, to: f32) {
    const HIGH: f32 = 5.6;
    let hw = s.width * 0.5;
    let gates = ((to - from) / 18.0).round() as usize;
    let step = (to - from) / gates as f32;
    for k in 0..=gates {
        let along = from + k as f32 * step;
        part(b, s, along, 0.0, (WATER_BED + s.top - 0.5) * 0.5, Vec3::new(2.4, s.top - 0.5 - WATER_BED, s.width + 9.0), Mat::Concrete, true);
        for e in [-1.0, 1.0] {
            part(b, s, along, e * (hw + 1.0), s.top + HIGH * 0.5 - 0.5, Vec3::new(1.4, HIGH + 1.0, 1.6), Mat::Concrete, true);
        }
        part(b, s, along, 0.0, s.top + HIGH, Vec3::new(1.0, 0.8, s.width + 3.6), Mat::Steel, true);
        if k < gates {
            part(b, s, along + step * 0.5, -(hw + 3.6), (WATER_BED + s.top) * 0.5, Vec3::new(step - 2.4, s.top - WATER_BED, 0.5), Mat::Steel, true);
        }
    }
}

/// Every bridge in the same order as `SPANS`, each of the Yamuna's five its own. The Hindon's three
/// are plain low decks.
fn bridges(b: &mut Builder, rng: &mut SmallRng) {
    for (k, s) in SPANS.iter().enumerate() {
        let (from, to) = decked(&b.map.terrain, s);
        deck(b, s, from, to, if k == 1 { Mat::Steel } else { Mat::Concrete }, !matches!(k, 0 | 4));
        match k {
            0 => signature(b, s),
            1 => loha_pul(b, s, from, to),
            2 => blue_viaduct(b),
            3 => dnd(b, s),
            4 => okhla(b, s, from, to),
            _ => {}
        }
        let placed = cover(b, s, rng);
        finds(b, s, if k < 5 { ChestKind::Tijori } else { ChestKind::Peti }, &placed);
    }
    red_fort(b);
}

/// The Red Fort's wall facing the old city, sealed, with no name and no loot: red sandstone with
/// battlements along its top, a round bastion and a chhatri every 68 m, and the Lahori Gate's two
/// towers in the middle.
fn red_fort(b: &mut Builder) {
    const X: f32 = -85.0;
    const MIDDLE: f32 = -1410.0;
    const LONG: f32 = 340.0;
    const HIGH: f32 = 18.0;
    const THICK: f32 = 30.0;
    let f = Frame::new(Vec3::ZERO, 0.0);
    let face = X - THICK * 0.5;
    b.bx(f, Vec3::new(X, HIGH * 0.5, MIDDLE), Vec3::new(THICK, HIGH, LONG), Mat::RedSandstone);
    // A lighter band under the battlements, and a merlon every 5 m along both faces.
    for side in [-1.0, 1.0] {
        let x = X + side * (THICK * 0.5 + 0.06);
        b.deco(f, Vec3::new(x, HIGH - 1.6, MIDDLE), Vec3::new(0.14, 0.7, LONG), Mat::Sandstone);
        let merlons = (LONG / 5.0) as usize;
        for k in 0..merlons {
            let z = MIDDLE - LONG * 0.5 + (k as f32 + 0.5) * 5.0;
            b.deco(f, Vec3::new(X + side * (THICK * 0.5 - 0.5), HIGH + 0.8, z), Vec3::new(1.0, 1.6, 2.8), Mat::RedSandstone);
        }
    }
    for k in 0..=5 {
        let z = MIDDLE - LONG * 0.5 + 68.0 * k as f32;
        // The two in the middle stand either side of the gate instead.
        if k == 2 || k == 3 {
            continue;
        }
        b.cyl(f, Vec3::new(face, 0.0, z), 7.0, HIGH + 1.0, Mat::RedSandstone);
        b.deco_cyl(f, Vec3::new(face, HIGH - 2.0, z), 7.15, 0.7, Mat::Sandstone);
        b.ornament(f, Vec3::new(face, HIGH + 1.0, z), 0.0, CHHATRI, Vec3::splat(1.6));
    }
    // The gate: a tall dark arch in a block that stands proud of the wall, a tower either side.
    b.bx(f, Vec3::new(face - 3.0, HIGH * 0.6, MIDDLE), Vec3::new(6.0, HIGH * 1.2, 26.0), Mat::RedSandstone);
    b.deco(f, Vec3::new(face - 6.06, 5.0, MIDDLE), Vec3::new(0.14, 10.0, 7.0), Mat::Granite);
    b.deco(f, Vec3::new(face - 6.1, 11.4, MIDDLE), Vec3::new(0.14, 0.8, 9.0), Mat::Sandstone);
    for side in [-1.0, 1.0] {
        let z = MIDDLE + side * 15.0;
        b.cyl(f, Vec3::new(face - 3.0, 0.0, z), 5.0, HIGH * 1.4, Mat::RedSandstone);
        b.ornament(f, Vec3::new(face - 3.0, HIGH * 1.4, z), 0.0, CHHATRI, Vec3::splat(1.4));
    }
}

fn on_open_land(map: &MapLayout, p: Vec2) -> bool {
    p.abs().max_element() < HALF - 10.0
        && map.terrain.ground_at(p.x, p.y) == Some(ground::OPEN)
        && PLACES.iter().all(|d| inside_by(d, p) < -8.0)
        && !in_marsh(p)
}

fn in_marsh(p: Vec2) -> bool {
    p.distance(Vec2::new(MARSH.0, MARSH.1)) < MARSH.2
}

/// Clear ground for something `r` across: no water, no road, no district.
fn clear(map: &MapLayout, p: Vec2, r: f32) -> bool {
    [p, p + Vec2::X * r, p - Vec2::X * r, p + Vec2::Y * r, p - Vec2::Y * r]
        .iter()
        .all(|q| !matches!(map.terrain.ground_at(q.x, q.y), Some(ground::WATER | ground::ROAD)) && PLACES.iter().all(|d| inside_by(d, *q) < 0.0))
}

/// Room for a building `w` by `d` at `p`, turned by `yaw`, with a walk round it: the whole plot off
/// the roads and the water and out of the districts, and clear of the land's other buildings,
/// which start at `from` in `map.footprints`.
fn site_free(map: &MapLayout, from: usize, p: Vec2, yaw: f32, w: f32, d: f32) -> bool {
    const ROOM: f32 = 3.0;
    let (u, v) = (Vec2::from_angle(yaw) * (w * 0.5 + ROOM), Vec2::from_angle(yaw).perp() * (d * 0.5 + ROOM));
    let (nu, nv) = (((w * 0.5 + ROOM) / 3.0).ceil() as i32, ((d * 0.5 + ROOM) / 3.0).ceil() as i32);
    let on_plot = (-nu..=nu).flat_map(|i| (-nv..=nv).map(move |j| (i, j))).map(|(i, j)| p + u * i as f32 / nu as f32 + v * j as f32 / nv as f32);
    let reach = (u + v).length();
    on_plot.into_iter().all(|q| !matches!(map.terrain.ground_at(q.x, q.y), Some(ground::WATER | ground::ROAD)) && PLACES.iter().all(|d| inside_by(d, q) < 0.0))
        && map.footprints[from..].iter().filter(|f| f.mat != Mat::Water).all(|f| {
            let c = f.pts.iter().sum::<Vec2>() / f.pts.len() as f32;
            c.distance(p) > c.distance(f.pts[0]) + reach
        })
}

fn shed(map: &mut MapLayout, rng: &mut SmallRng, p: Vec2, yaw: f32, w: f32, d: f32, h: f32) {
    let (u, v) = (Vec2::from_angle(yaw) * w * 0.5, Vec2::from_angle(yaw).perp() * d * 0.5);
    let mut pts = vec![p - u - v, p + u - v, p + u + v, p - u + v];
    if area(&pts) > 0.0 {
        pts.reverse();
    }
    let mat = [Mat::Plaster, Mat::PlasterOchre, Mat::Brick, Mat::Concrete, Mat::PlasterBlue][rng.random_range(0..5)];
    let base = map.terrain.height(p.x, p.y).min(0.0) - 0.2;
    map.footprints.push(Footprint { pts, base, top: base + h, mat });
}

/// Boxes scattered over a yard, each set down clear of roads and water.
fn scatter(b: &mut Builder, rng: &mut SmallRng, c: Vec2, r: f32, count: usize, size: Vec3, mat: Mat) {
    for _ in 0..count {
        let q = c + Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(r * 0.15..r);
        if clear(b.map, q, size.x * 0.6) {
            b.push(Vec3::new(q.x, size.y * 0.5, q.y), Quat::from_rotation_y(rng.random_range(0.0..TAU)), Shape::Box(size), mat, true, true);
        }
    }
}

/// New Delhi's platforms, long slabs under canopies, with goods wagons stopped between them.
fn station(b: &mut Builder, c: Vec2) {
    let f = Frame::new(Vec3::ZERO, 0.0);
    for k in [-1.0f32, 0.0, 1.0] {
        let z = c.y + k * 30.0;
        b.bx(f, Vec3::new(c.x, 0.45, z), Vec3::new(150.0, 0.9, 7.0), Mat::Concrete);
        b.bx(f, Vec3::new(c.x, 4.8, z), Vec3::new(132.0, 0.3, 5.6), Mat::Steel);
        for n in -3..=3 {
            b.cyl(f, Vec3::new(c.x + n as f32 * 20.0, 0.9, z), 0.22, 3.8, Mat::Concrete);
        }
    }
    for (k, x) in [-52.0f32, -34.0, 26.0, 44.0].into_iter().enumerate() {
        let z = c.y + if k < 2 { -15.0 } else { 15.0 };
        b.bx(f, Vec3::new(c.x + x, 1.7, z), Vec3::new(16.0, 3.4, 3.0), Mat::ContainerRed);
    }
}

/// Badarpur's power station: two bare boiler houses with a long low turbine hall beside each,
/// either side of the road, and two chimneys with their tops banded red and white.
fn power_station(b: &mut Builder, c: Vec2) {
    let f = Frame::new(Vec3::ZERO, 0.0);
    for side in [-1.0f32, 1.0] {
        let (x, z) = (c.x + side * 34.0, c.y + side * 8.0);
        b.bx(f, Vec3::new(x, 15.0, z), Vec3::new(26.0, 30.0, 42.0), Mat::Concrete);
        b.bx(f, Vec3::new(x + side * 18.0, 7.0, z), Vec3::new(10.0, 14.0, 42.0), Mat::Steel);
        b.deco(f, Vec3::new(x, 30.3, z), Vec3::new(27.0, 0.6, 43.0), Mat::Steel);
    }
    for x in [-12.0, 14.0] {
        let c = Vec3::new(c.x + x, 0.0, c.y);
        b.cyl(f, c, 5.5, 110.0, Mat::Concrete);
        for (y, mat) in [(92.0, Mat::ContainerRed), (98.0, Mat::PaintWhite), (104.0, Mat::ContainerRed)] {
            b.deco_cyl(f, c + Vec3::Y * y, 5.65, 6.0, mat);
        }
    }
}

/// A place on the open land: a few sheds round a yard, walls and whatever belongs there. Never
/// more than six buildings.
fn yard(map: &mut MapLayout, rng: &mut SmallRng, from: usize, c: Vec2, kind: Yard) {
    let r = kind.reach();
    let (sheds, w, d, h): (usize, f32, f32, f32) = match kind {
        Yard::Trucks => (2, 30.0, 12.0, 7.0),
        Yard::Buses => (1, 36.0, 12.0, 6.0),
        Yard::Market => (5, 9.0, 7.0, 3.6),
        Yard::Malls => (2, 40.0, 24.0, 14.0),
        Yard::Offices => (3, 18.0, 18.0, 26.0),
        Yard::Hotels => (3, 16.0, 24.0, 20.0),
        Yard::Fair => (6, 6.0, 5.0, 3.0),
        _ => (0, 0.0, 0.0, 0.0),
    };
    let turn = rng.random_range(0.0..TAU);
    for k in 0..sheds {
        let a = turn + k as f32 / sheds as f32 * TAU;
        let p = c + Vec2::from_angle(a) * r * 0.62;
        if site_free(map, from, p, a + FRAC_PI_2, w, d) {
            shed(map, rng, p, a + FRAC_PI_2, w, d, h);
        }
    }
    let mut b = Builder::new(map);
    match kind {
        Yard::Trucks => scatter(&mut b, rng, c, r * 0.45, 6, Vec3::new(6.1, 2.6, 2.4), Mat::ContainerRed),
        Yard::Buses => scatter(&mut b, rng, c, r * 0.5, 4, Vec3::new(10.0, 3.0, 2.6), Mat::TransitBlue),
        Yard::Ruin => scatter(&mut b, rng, c, r * 0.9, 9, Vec3::new(16.0, 3.4, 1.6), Mat::Granite),
        Yard::Thermal => {
            power_station(&mut b, c);
            scatter(&mut b, rng, c, r * 0.8, 6, Vec3::new(9.0, 2.2, 6.0), Mat::Dirt);
        }
        Yard::Station => station(&mut b, c),
        Yard::Marsh => {
            for _ in 0..36 {
                let q = c + Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(10.0..MARSH.2);
                if clear(b.map, q, 1.0) {
                    let y = b.map.terrain.height(q.x, q.y);
                    b.map.props.push(Prop { model: india::BUSH_LANTANA, pos: Vec3::new(q.x, y, q.y), yaw: rng.random_range(0.0..TAU), scale: rng.random_range(1.0..1.6), trunk: None });
                }
            }
            return;
        }
        _ => {}
    }
    if kind != Yard::Thermal && kind.paving().is_some() {
        scatter(&mut b, rng, c, r * 0.6, 2, Vec3::new(10.0, 1.6, 0.4), Mat::Brick);
    }
    const PIECES: [&str; 6] = ["auto", "thela", "crates", "drum", "scooter", "chai"];
    for _ in 0..8 {
        let q = c + Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(6.0..r * 0.8);
        if clear(b.map, q, 2.0) {
            let y = b.map.terrain.height(q.x, q.y);
            kit::place(&mut b, PIECES[rng.random_range(0..PIECES.len())], Vec3::new(q.x, y, q.y), rng.random_range(0.0..TAU));
        }
    }
}

/// What Anand Vihar is known for: its bus terminal. Tarmac spreads out round the platform roofs,
/// and buses stand in rows beside them, which gives a hot drop something to fight round. Returns
/// the first new block.
fn bus_terminal(map: &mut MapLayout, place: &Place) -> usize {
    // Where the terminal is in the district, and how far its yard spreads.
    const TERMINAL: Vec2 = Vec2::new(-50.0, 190.0);
    const YARD: f32 = 190.0;
    // The way the platforms run, radians from east towards south; metres between rows of buses,
    // and between the buses in a row.
    const ALONG: f32 = 0.86;
    const ROW: f32 = 14.0;
    const BAY: f32 = 34.0;
    let c = place.at + TERMINAL;
    let t = &mut map.terrain;
    let n = t.n;
    for i in row(t, c.x - YARD)..=row(t, c.x + YARD) {
        for j in row(t, c.y - YARD)..=row(t, c.y + YARD) {
            let p = Vec2::new(t.coord(i), t.coord(j));
            if inside_by(place, p) > 0.0 && p.distance(c) < YARD * (0.7 + 0.6 * fbm(p.x / 30.0, p.y / 30.0, 2, 37)) && t.ground[i * n + j] == ground::PAVEMENT {
                t.ground[i * n + j] = ground::PARKING;
            }
        }
    }
    let first = map.blocks.len();
    let dir = Vec2::from_angle(ALONG);
    let mut b = Builder::new(map);
    for lane in -5..=5 {
        for k in -3..=3 {
            let p = c + dir * k as f32 * BAY + dir.perp() * lane as f32 * ROW;
            let room = [p, p + dir * 5.0, p - dir * 5.0];
            let free = |m: &MapLayout, q: &Vec2| m.terrain.ground_at(q.x, q.y) == Some(ground::PARKING) && !built(m, *q);
            let parked = b.map.kit.iter().any(|k| k.pos.xz().distance(p) < 7.0) || b.map.props.iter().any(|t| t.pos.xz().distance(p) < 7.0);
            if !parked && room.iter().all(|q| free(b.map, q)) {
                let bus = Frame::new(Vec3::new(p.x, 0.0, p.y), -ALONG);
                b.bx(bus, Vec3::Y * 1.5, Vec3::new(10.0, 3.0, 2.6), Mat::TransitBlue);
                b.deco(bus, Vec3::Y * 2.1, Vec3::new(10.06, 0.9, 2.66), Mat::Glass);
            }
        }
    }
    first
}

/// Whether a point stands inside a box, or within `margin` of it.
fn in_block(b: &super::Block, p: Vec3, margin: f32) -> bool {
    let Shape::Box(size) = b.shape else { return b.pos.xz().distance(p.xz()) < margin + 6.0 };
    let q = b.rot.inverse() * (p - b.pos);
    q.x.abs() < size.x * 0.5 + margin && q.z.abs() < size.z * 0.5 + margin
}

/// The named spots: what stands at each, and a chest and a few finds, drawn with the loot's seed.
fn spots(map: &mut MapLayout, rng: &mut SmallRng, loot: &mut SmallRng, from: usize) {
    for (name, x, z, kind) in SPOTS {
        let c = Vec2::new(x, z);
        map.pois.push(Poi { name: name.into(), center: Vec3::new(x, 0.0, z), radius: 90.0 });
        let (blocks, kit_pieces) = (map.blocks.len(), map.kit.len());
        yard(map, rng, from, c, kind);
        if kind == Yard::Marsh {
            continue;
        }
        let mut places = vec![];
        for _ in 0..300 {
            let q = c + Vec2::from_angle(rng.random_range(0.0..TAU)) * rng.random_range(4.0..kind.reach() * 0.85);
            let p = Vec3::new(q.x, map.terrain.height(q.x, q.y), q.y);
            let walled = map.blocks[blocks..].iter().any(|b| b.collide && in_block(b, p, 1.5)) || map.kit[kit_pieces..].iter().any(|k| k.pos.xz().distance(q) < 2.0);
            if clear(map, q, 1.5) && !walled && !built(map, q) {
                places.push(p);
            }
            if places.len() > SPOT_FLOOR * 2 {
                break;
            }
        }
        shuffle(&mut places, loot);
        let Some(spot) = places.pop() else { continue };
        map.chests.push(ChestSpawn { pos: spot, yaw: rng.random_range(0.0..TAU), kind: ChestKind::Peti, set: None });
        map.floor_loot.extend(places.into_iter().take(SPOT_FLOOR).map(|p| p + Vec3::Y * 0.1));
    }
}

/// The land between districts: buildings in thin ribbons along the roads, and everywhere else a
/// scatter of cover, so nobody crosses more than about 80 m of nothing. The river sand gets low
/// earth berms, scrub and the odd nursery hut. Nothing here is a temple, mosque, gurdwara or grave.
/// Returns spots for loose finds.
fn open_land(map: &mut MapLayout, rng: &mut SmallRng, network: &[(f32, Vec<Vec2>)], from: usize) -> Vec<Vec3> {
    let mut spare = vec![];
    for (width, line) in network {
        let mut d = 0.0;
        for s in line.windows(2) {
            let dir = (s[1] - s[0]).normalize_or_zero();
            d += s[0].distance(s[1]);
            if d < 60.0 {
                continue;
            }
            d = 0.0;
            for e in [-1.0, 1.0] {
                let p = s[0] + dir.perp() * e * (*width * 0.5 + rng.random_range(12.0..20.0));
                if rng.random_bool(0.45) && on_open_land(map, p) && map.terrain.height(p.x, p.y).abs() < 0.8 {
                    let (w, depth, h) = (rng.random_range(14.0..24.0), rng.random_range(10.0..16.0), rng.random_range(6.0..16.0));
                    if site_free(map, from, p, dir.y.atan2(dir.x), w, depth) {
                        shed(map, rng, p, dir.y.atan2(dir.x), w, depth, h);
                        let q = p - dir.perp() * e * 10.0;
                        spare.push(Vec3::new(q.x, map.terrain.height(q.x, q.y) + 0.1, q.y));
                    }
                }
            }
        }
    }
    let cell = 80.0;
    let n = (2.0 * HALF / cell) as i32;
    for i in 0..n {
        for j in 0..n {
            let p = Vec2::new((i as f32 + 0.5) * cell - HALF, (j as f32 + 0.5) * cell - HALF) + Vec2::new(rng.random_range(-30.0..30.0), rng.random_range(-30.0..30.0));
            if map.terrain.ground_at(p.x, p.y) == Some(ground::SAND) && PLACES.iter().all(|d| inside_by(d, p) < -8.0) {
                sand_cover(map, rng, p);
                continue;
            }
            if !on_open_land(map, p) {
                continue;
            }
            let y = map.terrain.height(p.x, p.y);
            match rng.random_range(0..10) {
                0..=5 => {
                    for _ in 0..rng.random_range(2..5) {
                        let q = p + Vec2::new(rng.random_range(-9.0..9.0), rng.random_range(-9.0..9.0));
                        let model = [india::TREE_NEEM, india::TREE_PEEPAL][rng.random_range(0..2)];
                        map.props.push(Prop { model, pos: Vec3::new(q.x, map.terrain.height(q.x, q.y), q.y), yaw: rng.random_range(0.0..TAU), scale: rng.random_range(0.85..1.25), trunk: Some((0.25, 4.0)) });
                    }
                }
                6 | 7 if y.abs() < 1.0 => {
                    let yaw = rng.random_range(0.0..TAU);
                    let mut b = Builder::new(map);
                    b.push(Vec3::new(p.x, y + 0.9, p.y), Quat::from_rotation_y(yaw), Shape::Box(Vec3::new(rng.random_range(8.0..16.0), 1.8, 0.4)), Mat::Brick, true, true);
                }
                8 if y.abs() < 1.0 => {
                    let yaw = rng.random_range(0.0..TAU);
                    if site_free(map, from, p, yaw, 8.0, 6.0) {
                        shed(map, rng, p, yaw, 8.0, 6.0, 3.4);
                    }
                }
                _ => spare.push(Vec3::new(p.x, y + 0.1, p.y)),
            }
        }
    }
    spare
}

/// Villages on the open land, a road's length apart along the roads: Delhi's old gaons that the
/// city grew round, two to four storeys of plastered brick packed along lanes a few metres wide,
/// a chowk with a peepal in the middle. Their ground is beaten earth (`ground::PATH`), so the
/// open land's groves and sheds keep off it. Returns spots in the lanes for floor loot; a trunk
/// under the peepal is out one match in three.
fn villages(map: &mut MapLayout, network: &[(f32, Vec<Vec2>)], from: usize) -> Vec<Vec3> {
    /// Metres along a road between tries, and how far apart two villages stand at least.
    const EVERY: f32 = 230.0;
    const APART: f32 = 160.0;
    let mut rng = SmallRng::seed_from_u64(LAYOUT_SEED ^ 0x6A0);
    let sites = holy();
    let mut placed: Vec<Vec2> = vec![];
    let mut spare = vec![];
    for (width, line) in network {
        let mut d = EVERY * 0.5;
        for s in line.windows(2) {
            let (len, dir) = (s[0].distance(s[1]), (s[1] - s[0]).normalize_or_zero());
            while d < len {
                let at = s[0] + dir * d;
                d += EVERY;
                let side = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
                let (w, depth) = (rng.random_range(60.0..110.0), rng.random_range(40.0..70.0));
                // u along the road, v away from it.
                let (u, v) = (dir, dir.perp() * side);
                let c = at + v * (*width * 0.5 + 8.0 + depth * 0.5);
                let h0 = map.terrain.height(c.x, c.y);
                let ok = |q: Vec2| {
                    map.terrain.ground_at(q.x, q.y) == Some(ground::OPEN)
                        && q.abs().max_element() < HALF - 30.0
                        && PLACES.iter().all(|p| inside_by(p, q) < -20.0)
                        && !in_marsh(q)
                        && !no_loot(q)
                        && sites.iter().all(|(h, r)| q.distance(*h) > r + 10.0)
                        && (map.terrain.height(q.x, q.y) - h0).abs() < 1.2
                };
                let grid = |step: f32| {
                    let (nu, nv) = ((w / step) as i32, (depth / step) as i32);
                    (0..=nu).flat_map(move |i| (0..=nv).map(move |j| c + u * (i as f32 / nu as f32 - 0.5) * w + v * (j as f32 / nv as f32 - 0.5) * depth))
                };
                if placed.iter().any(|p| p.distance(c) < APART) || !grid(4.0).all(ok) {
                    continue;
                }
                let near: Vec<usize> = (from..map.footprints.len()).filter(|k| map.footprints[*k].pts[0].distance(c) < w + depth + 60.0).collect();
                if grid(6.0).any(|q| near.iter().any(|k| map.footprints[*k].mat != Mat::Water && footprint::contains(&map.footprints[*k].pts, q))) {
                    continue;
                }
                placed.push(c);
                spare.extend(village(map, &mut rng, c, u, v, w, depth));
            }
            d -= len;
        }
    }
    spare
}

/// One village: rows of houses back to back between lanes running along the road, cut across
/// every few plots by a gali, its middle left open for the chowk.
fn village(map: &mut MapLayout, rng: &mut SmallRng, c: Vec2, u: Vec2, v: Vec2, w: f32, depth: f32) -> Vec<Vec3> {
    const LANE: f32 = 4.0;
    const GALI: f32 = 2.6;
    const GAP: f32 = 0.5;
    let origin = c - u * w * 0.5 - v * depth * 0.5;
    let at = |a: f32, b: f32| origin + u * a + v * b;
    // Beaten earth over the whole village, lanes and all.
    {
        let t = &mut map.terrain;
        let (lo, hi) = [at(0.0, 0.0), at(w, 0.0), at(0.0, depth), at(w, depth)].iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        for i in row(t, lo.x)..=row(t, hi.x) {
            for j in row(t, lo.y)..=row(t, hi.y) {
                let q = Vec2::new(t.coord(i), t.coord(j)) - origin;
                if (-2.0..w + 2.0).contains(&q.dot(u)) && (-2.0..depth + 2.0).contains(&q.dot(v)) {
                    t.ground[i * t.n + j] = ground::PATH;
                }
            }
        }
    }
    let chowk = (w * 0.5, depth * 0.5);
    let wash = [Mat::Plaster, Mat::PlasterOchre, Mat::PlasterBlue, Mat::Brick, Mat::Paint(0), Mat::Paint(2), Mat::Paint(3), Mat::Paint(4), Mat::Paint(5), Mat::Paint(6)];
    let mut spare = vec![];
    // Rows away from the road: a lane, then two rows of houses back to back.
    let mut b = LANE;
    while b < depth - 8.0 {
        let rows = if depth - b > 22.0 { 2 } else { 1 };
        for _ in 0..rows {
            let deep = rng.random_range(8.0f32..11.0).min(depth - b - 0.5);
            let mut a = 0.0;
            while a < w - 4.5 {
                // A gali across every few houses.
                if rng.random_bool(0.16) && a > 0.0 {
                    a += GALI;
                    continue;
                }
                let wide = rng.random_range(4.8f32..8.0).min(w - a);
                let mid = (a + wide * 0.5, b + deep * 0.5);
                let in_chowk = (mid.0 - chowk.0).abs() < 11.0 && (mid.1 - chowk.1).abs() < 9.0;
                if !in_chowk && !rng.random_bool(0.07) {
                    let pts0 = [at(a, b), at(a + wide - GAP, b), at(a + wide - GAP, b + deep - GAP), at(a, b + deep - GAP)];
                    let mut pts = pts0.to_vec();
                    if area(&pts) > 0.0 {
                        pts.reverse();
                    }
                    let low = pts.iter().map(|q| map.terrain.height(q.x, q.y)).fold(f32::MAX, f32::min) - 0.3;
                    let floors = [2, 2, 3, 3, 3, 4][rng.random_range(0..6)] as f32;
                    map.footprints.push(Footprint { pts, base: low, top: low + 0.3 + floors * 3.1 + 0.9, mat: wash[rng.random_range(0..wash.len())] });
                }
                a += wide;
            }
            b += deep;
        }
        // The lane behind, with a spot for loot along it now and then.
        let lane = b + LANE * 0.5;
        if lane < depth {
            for _ in 0..2 {
                let q = at(rng.random_range(4.0..w - 4.0), lane);
                spare.push(Vec3::new(q.x, map.terrain.height(q.x, q.y) + 0.1, q.y));
            }
        }
        b += LANE;
    }
    // The chowk: a peepal, and a trunk under it; another down a lane.
    let mid = at(chowk.0, chowk.1);
    let y = map.terrain.height(mid.x, mid.y);
    map.props.push(Prop { model: india::TREE_PEEPAL, pos: Vec3::new(mid.x, y, mid.y), yaw: rng.random_range(0.0..TAU), scale: rng.random_range(1.1..1.4), trunk: Some((0.4, 4.0)) });
    // Out one match in three, with the nearest district's draw.
    let chest = mid + u * 5.0;
    let district = (0..PLACES.len()).max_by(|a, b| inside_by(&PLACES[*a], mid).total_cmp(&inside_by(&PLACES[*b], mid))).unwrap_or(0);
    let set = Some((district as u8, rng.random_range(0..CHEST_SETS)));
    map.chests.push(ChestSpawn { pos: Vec3::new(chest.x, map.terrain.height(chest.x, chest.y), chest.y), yaw: u.y.atan2(u.x), kind: ChestKind::Peti, set });
    spare
}

/// Cover on the river sand. No loot here: a bank is for crossing.
fn sand_cover(map: &mut MapLayout, rng: &mut SmallRng, p: Vec2) {
    let y = map.terrain.height(p.x, p.y);
    let yaw = rng.random_range(0.0..TAU);
    match rng.random_range(0..10) {
        0..=5 => {
            let size = Vec3::new(rng.random_range(6.0..12.0), 1.5, 2.4);
            Builder::new(map).push(Vec3::new(p.x, y + 0.5, p.y), Quat::from_rotation_y(yaw), Shape::Box(size), Mat::Dirt, true, true);
        }
        6..=8 => {
            map.props.push(Prop { model: india::TREE_NEEM, pos: Vec3::new(p.x, y, p.y), yaw, scale: rng.random_range(0.7..1.0), trunk: Some((0.2, 3.0)) });
        }
        _ => shed(map, rng, p, yaw, 6.0, 5.0, 2.8),
    }
}

fn edge_walls(b: &mut Builder) {
    let (y, tall, long) = (WALL_TOP * 0.5 - 20.0, WALL_TOP + 40.0, 2.0 * HALF + 8.0);
    for (c, s) in [
        (Vec3::new(HALF + 2.0, y, 0.0), Vec3::new(4.0, tall, long)),
        (Vec3::new(-HALF - 2.0, y, 0.0), Vec3::new(4.0, tall, long)),
        (Vec3::new(0.0, y, HALF + 2.0), Vec3::new(long, tall, 4.0)),
        (Vec3::new(0.0, y, -HALF - 2.0), Vec3::new(long, tall, 4.0)),
    ] {
        b.push(c, Quat::IDENTITY, Shape::Box(s), Mat::Granite, true, false);
    }
}

/// Every holy site on the map, as a world position and a radius.
pub fn holy() -> Vec<(Vec2, f32)> {
    let mut all = vec![];
    for place in &PLACES {
        let sites = match place.source {
            Source::Map(id) => delhi::holy_sites(id).to_vec(),
            Source::Data(json) => delhi::data_holy(json),
        };
        all.extend(sites.into_iter().map(|(x, z, r)| (place.at + Vec2::new(x, z), r)));
    }
    all
}

/// Where no loot may lie: the India Gate arch, the Red Fort's wall, the Okhla wetland.
pub(super) fn no_loot(p: Vec2) -> bool {
    p.distance(Vec2::new(-327.0, -5.0)) < 60.0 || (p.x - -85.0).abs() < 55.0 && (p.y - -1410.0).abs() < 210.0 || in_marsh(p)
}

pub fn generate(seed: u64) -> MapLayout {
    let size = 2.0 * HALF + 60.0;
    let n = (size / STEP) as usize + 1;
    let terrain = Terrain { n, size, h: vec![0.0; n * n], road_d: vec![1e3; n * n], ground: vec![ground::OPEN; n * n] };
    let mut map = MapLayout { name: "Dilli NCR".into(), half_extent: HALF, style: MapStyle::Ncr, terrain, ..default() };
    let mut rng = SmallRng::seed_from_u64(LAYOUT_SEED ^ 0xD31);
    let mut loot = SmallRng::seed_from_u64(seed ^ 0x100D);
    let network = network();
    edge_walls(&mut Builder::new(&mut map));
    river(&mut map, &curve(&v2s(&YAMUNA)), YAMUNA_WATER, YAMUNA_BANK, DECK);
    river(&mut map, &curve(&v2s(&HINDON)), HINDON_WATER, HINDON_BANK, 0.0);
    hills(&mut map);
    roads(&mut map, &network);
    pads(&mut map.terrain);
    level_districts(&mut map.terrain);
    approaches(&mut map.terrain);
    for (index, place) in PLACES.iter().enumerate() {
        add(&mut map, place, index, seed);
    }
    let buses = PLACES.iter().find(|d| d.name == "Anand Vihar").map_or(map.blocks.len(), |d| bus_terminal(&mut map, d));
    let bus_blocks: Vec<super::Block> = map.blocks[buses..].to_vec();
    for place in &PLACES {
        if matches!(place.source, Source::Data(_)) {
            infill(&mut map, place);
        }
    }
    bridges(&mut Builder::new(&mut map), &mut rng);
    let open_buildings = map.footprints.len();
    spots(&mut map, &mut rng, &mut loot, open_buildings);
    for (name, s) in BRIDGES.iter().zip(&SPANS) {
        let c = s.at(s.len() * 0.5, 0.0);
        map.pois.push(Poi { name: (*name).into(), center: Vec3::new(c.x, 0.0, c.y), radius: s.len() * 0.5 });
    }
    let in_villages = villages(&mut map, &network, open_buildings);
    let mut spare = open_land(&mut map, &mut rng, &network, open_buildings);
    spare.extend(in_villages);
    let sites = holy();
    let allowed = |p: Vec3| !no_loot(p.xz()) && sites.iter().all(|(c, r)| p.xz().distance(*c) >= *r) && !bus_blocks.iter().any(|b| in_block(b, p, 1.5));
    let sealed = |map: &MapLayout, p: Vec3| in_building(map, p);
    let mut chests = std::mem::take(&mut map.chests);
    chests.retain(|c| allowed(c.pos) && !sealed(&map, c.pos));
    map.chests = chests;
    let mut floor = std::mem::take(&mut map.floor_loot);
    floor.retain(|p| allowed(*p) && !sealed(&map, *p));
    map.floor_loot = floor;
    spare.retain(|p| allowed(*p) && !sealed(&map, *p));
    shuffle(&mut spare, &mut loot);
    let short = FLOOR_SPOTS.saturating_sub(map.floor_loot.len());
    map.floor_loot.extend(spare.into_iter().take(short));
    map.holy = sites;
    super::thickets::plant(&mut map);
    clear_the_ground(&mut map);
    let MapLayout { terrain, blocks, .. } = &mut map;
    terrain.settle_under(blocks);
    // Last, so that the rest of the map is laid out exactly as it would be without rooms.
    let spots = interiors::hollow(&mut map, ROOMS_PER_CELL);
    interiors::furnish_ring(&mut map);
    interiors::move_loot(&mut map, &spots, &mut loot);
    parking::lay(&mut map, &network, &SPOTS.map(|s| (Vec2::new(s.1, s.2), s.3.reach())));
    super::stalls::lay(&mut map);
    map
}

/// The last word on what was set down where it can't stand: a tree mid-road or in the water, and
/// anything inside a building, go. A scattered grove doesn't know the road it spreads over, and a
/// shed put up later doesn't know the tree already standing on its plot.
fn clear_the_ground(map: &mut MapLayout) {
    let mut props = std::mem::take(&mut map.props);
    props.retain(|p| {
        let in_the_way = p.trunk.is_some() && matches!(map.terrain.ground_at(p.pos.x, p.pos.z), Some(ground::ROAD | ground::WATER));
        !in_the_way && !in_building(map, p.pos)
    });
    map.props = props;
    let stays: Vec<bool> = map.kit.iter().map(|k| !in_building(map, k.pos)).collect();
    kit::keep(map, &stays);
}

/// The name of the bridge a point stands on, if any.
pub fn bridge_at(p: Vec2) -> Option<&'static str> {
    let on = |s: &Span| p.distance(s.at((p - s.a).dot(s.dir()).clamp(0.0, s.len()), 0.0)) < s.width * 0.5 + 2.0;
    SPANS.iter().zip(BRIDGES).find(|(s, _)| on(s)).map(|(_, name)| name)
}

/// Whether something standing at `p` is inside a solid building: on a roof it isn't.
pub fn in_building(map: &MapLayout, p: Vec3) -> bool {
    let y = p.y + 0.5;
    map.footprints.iter().enumerate().any(|(k, f)| f.mat != Mat::Water && y > f.base && y < f.top - 0.05 && f.pts[0].distance(p.xz()) < 400.0 && footprint::contains(&f.pts, p.xz()) && map.rooms_of(k).is_none())
}

/// Whether a point is inside any solid building, for tools that draw or check the map.
pub fn built(map: &MapLayout, p: Vec2) -> bool {
    map.footprints.iter().any(|f| f.mat != Mat::Water && f.pts[0].distance(p) < 400.0 && footprint::contains(&f.pts, p))
}

/// Kit pieces that stand on nothing: above 1.5 m, with no roof, block or ground under them. What
/// stood on a building thinned out of the map must have gone with it.
pub fn floating(map: &MapLayout) -> Vec<&kit::KitPiece> {
    let on_block = |b: &super::Block, p: Vec3| {
        let Shape::Box(size) = b.shape else { return false };
        let q = b.rot.inverse() * (p - b.pos);
        b.collide && q.x.abs() <= size.x * 0.5 && q.z.abs() <= size.z * 0.5 && (q.y - size.y * 0.5).abs() < 0.6
    };
    map.kit
        .iter()
        .filter(|k| k.pos.y - map.terrain.height(k.pos.x, k.pos.z) > 1.5)
        // Hung on a wall or from a ceiling (Connaught Place's upstairs rooms), not stood on one.
        .filter(|k| !matches!(k.name, "calendar" | "clock" | "god_photo" | "fan"))
        .filter(|k| {
            let roof = map.footprints.iter().any(|f| f.mat != Mat::Water && (f.top - k.pos.y).abs() < 0.6 && footprint::contains(&f.pts, k.pos.xz()));
            !roof && !map.blocks.iter().any(|b| on_block(b, k.pos))
        })
        .collect()
}

/// How many separate stretches of a district's edge have a connecting road meeting a road or path
/// inside.
pub fn exits(map: &MapLayout, place: &Place) -> usize {
    let t = &map.terrain;
    let road = |p: Vec2| t.ground_at(p.x, p.y) == Some(ground::ROAD);
    let street = |p: Vec2| matches!(t.ground_at(p.x, p.y), Some(ground::ROAD | ground::PATH | ground::PLAZA));
    let mut found = 0;
    let long = (place.clip * 2.0) as i32;
    for (corner, along, out) in [
        (Vec2::new(-1.0, -1.0), Vec2::X, Vec2::NEG_Y),
        (Vec2::new(-1.0, 1.0), Vec2::X, Vec2::Y),
        (Vec2::new(-1.0, -1.0), Vec2::Y, Vec2::NEG_X),
        (Vec2::new(1.0, -1.0), Vec2::Y, Vec2::X),
    ] {
        let start = place.at + corner * place.clip;
        let mut gap = 99;
        for k in (0..long).step_by(2) {
            let p = start + along * k as f32;
            let meets = (2..=12).step_by(2).any(|d| road(p + out * d as f32)) && (0..=20).step_by(2).any(|d| street(p - out * d as f32));
            if meets {
                if gap >= 12 {
                    found += 1;
                }
                gap = 0;
            } else {
                gap += 2;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::OnceLock;

    /// The map, built once: it takes a while in a test build.
    fn map() -> &'static MapLayout {
        static MAP: OnceLock<MapLayout> = OnceLock::new();
        MAP.get_or_init(|| generate(7))
    }

    #[test]
    fn places_do_not_overlap() {
        for (k, a) in PLACES.iter().enumerate() {
            assert!(a.at.abs().max_element() + a.clip <= HALF, "{} leaves the map", a.name);
            assert_eq!(a.at, a.at.round(), "{} must sit on whole metres", a.name);
            for b in &PLACES[k + 1..] {
                assert!((a.at - b.at).abs().max_element() >= a.clip + b.clip, "{} overlaps {}", a.name, b.name);
            }
        }
    }

    /// Degrees clockwise from north, on the map, and how far that is from the real bearing.
    fn bearing_off(from: Vec2, to: Vec2, real: f32) -> f32 {
        let d = to - from;
        let got = d.x.atan2(-d.y).to_degrees().rem_euclid(360.0);
        (got - real + 540.0f32).rem_euclid(360.0) - 180.0
    }

    fn where_is(name: &str) -> Vec2 {
        let spot = SPOTS.iter().find(|s| s.0 == name).map(|s| Vec2::new(s.1, s.2));
        spot.or(PLACES.iter().find(|d| d.name == name).map(|d| d.at)).unwrap_or_else(|| panic!("{name}"))
    }

    /// True bearings from Connaught Place, degrees clockwise from north, from OpenStreetMap: a
    /// place may stray from its real direction only as far as the map's corners force it.
    #[test]
    fn compass_order_holds() {
        let real = [
            ("Chandni Chowk", 25.0), ("Kartavya Path", 159.0), ("Hauz Khas", 196.0), ("Karol Bagh", 298.0), ("Nehru Place", 161.0), ("Cyber City", 220.0),
            ("MG Road", 219.0), ("Golf Course Road", 206.0), ("Laxmi Nagar", 92.0), ("Anand Vihar", 80.0), ("Indirapuram", 88.0), ("Ghaziabad", 80.0),
            ("Sector 18", 125.0), ("Film City", 127.0), ("Expressway", 132.0), ("Pari Chowk", 124.0), ("Saket", 180.0), ("Aerocity", 226.0),
            ("Azadpur Mandi", 332.0), ("Kamla Nagar", 344.0), ("Kashmere Gate", 14.0), ("Rajouri Garden", 282.0), ("New Delhi Station", 5.0), ("Mandi House", 119.0),
            ("Dhaula Kuan", 231.0), ("Lajpat Nagar", 159.0), ("Sarai Kale Khan", 142.0), ("Jasola", 147.0), ("Tughlaqabad", 163.0), ("Surajkund Mela Ground", 160.0),
            ("Badarpur Thermal", 149.0), ("Mahipalpur", 224.0), ("Sikanderpur", 216.0), ("Shahdara", 57.0), ("Raj Nagar", 76.0), ("Lal Kuan", 90.0),
            ("Mayur Vihar", 110.0), ("Ghazipur Mound", 96.0), ("Sector 62", 95.0), ("City Centre", 116.0), ("Botanical Garden", 124.0), ("Kalindi Kunj", 138.0),
        ];
        for (name, want) in real {
            let off = bearing_off(PLACES[0].at, where_is(name), want);
            assert!(off.abs() <= 22.0, "{name}: {off} degrees off");
        }
    }

    /// Near its own district a spot keeps its side too, which the long view from Connaught Place
    /// can't show: true bearings from the district's middle, from OpenStreetMap.
    #[test]
    fn spots_stand_on_the_right_side_of_their_districts() {
        for (from, to, want) in [
            ("Ghaziabad", "Raj Nagar", 21.0),
            ("Ghaziabad", "Lal Kuan", 139.0),
            ("Sector 18", "Botanical Garden", 124.0),
            ("Sector 18", "City Centre", 79.0),
            ("Sector 18", "Kalindi Kunj", 204.0),
        ] {
            let off = bearing_off(where_is(from), where_is(to), want);
            assert!(off.abs() <= 35.0, "{to} from {from}: {off} degrees off");
        }
    }

    /// The layout is the same whatever the match's seed, and only the loot moves.
    #[test]
    fn layout_is_fixed_and_loot_varies() {
        let (a, b) = (map(), generate(8));
        assert_eq!(a.blocks.len(), b.blocks.len());
        assert_eq!(a.footprints.len(), b.footprints.len());
        assert_eq!(a.kit.len(), b.kit.len());
        assert!(a.blocks.iter().zip(&b.blocks).all(|(p, q)| p.pos == q.pos));
        assert!(a.footprints.iter().zip(&b.footprints).all(|(p, q)| p.pts == q.pts));
        let same = a.chests.iter().filter(|c| b.chests.iter().any(|d| d.pos == c.pos)).count();
        assert!(same < a.chests.len() * 3 / 4, "{same} of {} chests stayed put", a.chests.len());
        let c = generate(7);
        assert!(a.chests.iter().zip(&c.chests).all(|(p, q)| p.pos == q.pos));
        assert!(a.floor_loot.iter().zip(&c.floor_loot).all(|(p, q)| p == q));
    }

    /// The open land stays open: buildings cover under 8 % of the map (the districts' infill and
    /// the villages are most of it).
    #[test]
    fn map_stays_open() {
        let m = map();
        let roof: f32 = m.footprints.iter().filter(|f| f.mat != Mat::Water).map(|f| area(&f.pts).abs()).sum();
        assert!(roof / (4.0 * HALF * HALF) < 0.08, "{roof}");
    }

    /// The land between the districts is rough land and river sand, not parkland.
    #[test]
    fn open_land_has_its_own_ground() {
        let t = &map().terrain;
        let share = |class: u8| t.ground.iter().filter(|g| **g == class).count() as f32 / t.ground.len() as f32;
        assert!(share(ground::OPEN) > 0.4, "{}", share(ground::OPEN));
        assert!(share(ground::SAND) > 0.05, "{}", share(ground::SAND));
    }

    #[test]
    fn nothing_floats() {
        let m = map();
        let raised = m.kit.iter().filter(|k| k.pos.y - m.terrain.height(k.pos.x, k.pos.z) > 1.5).count();
        assert!(raised > 200, "the test needs roofs to look at: {raised}");
        let lost = floating(m);
        assert!(lost.is_empty(), "{} float, as {:?}", lost.len(), lost.first());
    }

    #[test]
    fn every_district_has_three_exits() {
        let m = map();
        for place in &PLACES {
            let n = exits(m, place);
            assert!(n >= 3, "{} has {n}", place.name);
        }
    }

    /// The roads are one network: every road sample on the open land, every district and both
    /// ends of every bridge can be reached from the Ring Road. A district counts as one place, as
    /// its streets are all walkable.
    #[test]
    fn roads_join_up() {
        let t = &map().terrain;
        let n = t.n;
        let place_at = |p: Vec2| PLACES.iter().position(|d| inside_by(d, p) >= -6.0);
        let road = |k: usize| t.ground[k] == ground::ROAD;
        let start = t.ground.iter().enumerate().position(|(k, _)| road(k) && (Vec2::new(t.coord(k / n), t.coord(k % n)) - Vec2::new(-95.0, -400.0)).length() < 6.0).expect("Ring Road");
        let sample = |p: Vec2| {
            let i = |c: f32| ((c + t.size * 0.5) / t.step()).round() as usize;
            i(p.x) * n + i(p.y)
        };
        let mut seen = vec![false; n * n];
        let mut districts = [false; PLACES.len()];
        let mut bridges = [false; SPANS.len()];
        let mut todo = vec![start];
        seen[start] = true;
        while let Some(k) = todo.pop() {
            let p = Vec2::new(t.coord(k / n), t.coord(k % n));
            // Reaching a district reaches everything that leads out of it.
            if let Some(d) = place_at(p)
                && !districts[d]
            {
                districts[d] = true;
                let c = &PLACES[d];
                let r = (c.clip + 6.0) as i32;
                for x in (-r..=r).step_by(2) {
                    for z in (-r..=r).step_by(2) {
                        let q = c.at + Vec2::new(x as f32, z as f32);
                        let w = sample(q);
                        if inside_by(c, q) < 0.0 && !seen[w] && road(w) {
                            seen[w] = true;
                            todo.push(w);
                        }
                    }
                }
            }
            // Crossing a bridge reaches its far bank.
            for (b, s) in SPANS.iter().enumerate() {
                if !bridges[b] && (p.distance(s.at(0.0, 0.0)) < 3.0 || p.distance(s.at(s.len(), 0.0)) < 3.0) {
                    bridges[b] = true;
                    for q in [s.a, s.c] {
                        let w = sample(q);
                        if !seen[w] {
                            seen[w] = true;
                            todo.push(w);
                        }
                    }
                }
            }
            let (i, j) = (k / n, k % n);
            for x in i.saturating_sub(1)..=(i + 1).min(n - 1) {
                for z in j.saturating_sub(1)..=(j + 1).min(n - 1) {
                    let w = x * n + z;
                    if !seen[w] && road(w) && PLACES.iter().all(|d| inside_by(d, Vec2::new(t.coord(x), t.coord(z))) < 0.0) {
                        seen[w] = true;
                        todo.push(w);
                    }
                }
            }
        }
        for (d, place) in districts.iter().zip(&PLACES) {
            assert!(d, "no road reaches {}", place.name);
        }
        assert!(bridges.iter().all(|b| *b), "{bridges:?}");
        let orphan = (0..n * n).find(|k| road(*k) && !seen[*k] && place_at(Vec2::new(t.coord(k / n), t.coord(k % n))).is_none());
        assert!(orphan.is_none(), "a road nothing reaches, at {:?}", orphan.map(|k| (t.coord(k / n), t.coord(k % n))));
    }

    /// No road stops in the middle of nowhere: each end is at the map's edge, on another road, on
    /// a bridge or where a road or path carries on into a district. No road crosses water.
    #[test]
    fn no_road_ends_in_nowhere() {
        let (m, all) = (map(), network());
        let t = &m.terrain;
        for (k, (width, line)) in all.iter().enumerate() {
            assert!(line.iter().all(|p| t.ground_at(p.x, p.y) != Some(ground::WATER)), "road {k} crosses water");
            for end in [line[0], *line.last().unwrap()] {
                let edge = end.abs().max_element() >= HALF - 1.0;
                let bridge = SPANS.iter().any(|s| end.distance(s.a) < 2.0 || end.distance(s.c) < 2.0);
                let junction = all.iter().enumerate().any(|(o, (w, other))| o != k && other.windows(2).any(|s| {
                    let u = s[1] - s[0];
                    let f = ((end - s[0]).dot(u) / u.length_squared().max(1e-6)).clamp(0.0, 1.0);
                    end.distance(s[0] + u * f) <= (w + width) * 0.5 + 2.0
                }));
                let district = PLACES.iter().any(|d| {
                    inside_by(d, end).abs() < 2.0 && (-6..=6).any(|i| (-6..=6).any(|j| {
                        let q = end + Vec2::new(i as f32, j as f32) * 2.0;
                        inside_by(d, q) > 0.0 && matches!(t.ground_at(q.x, q.y), Some(ground::ROAD | ground::PATH | ground::PLAZA))
                    }))
                });
                assert!(edge || bridge || junction || district, "road {k} stops at {end}");
            }
        }
    }

    /// A bridge shows on the ground: road the whole way from bank to bank over everything but the
    /// water, a walkable slope up to each end, and ground flush with the deck where they meet.
    #[test]
    fn bridges_show_on_the_ground() {
        let t = &map().terrain;
        for (k, s) in SPANS.iter().enumerate() {
            let len = s.len();
            let mut along = -s.ramp();
            while along <= len + s.ramp() {
                let p = s.at(along, 0.0);
                let class = t.ground_at(p.x, p.y);
                assert!(matches!(class, Some(ground::ROAD | ground::WATER)), "span {k} at {along}: {class:?}");
                along += 2.0;
            }
            let (from, to) = decked(t, s);
            for d in [from, to] {
                let p = s.at(d, 0.0);
                if d > 0.0 && d < len {
                    assert!((t.height(p.x, p.y) - s.top).abs() < 0.3, "span {k} earth at the deck, {}", t.height(p.x, p.y));
                }
            }
            for way in [-1.0, 1.0] {
                let start = if way < 0.0 { 0.0 } else { len };
                let (mut steepest, mut d) = (0.0f32, 0.0);
                while d < s.ramp() {
                    let (p, q) = (s.at(start + way * d, 0.0), s.at(start + way * (d + 2.0), 0.0));
                    steepest = steepest.max((t.height(p.x, p.y) - t.height(q.x, q.y)).abs() / 2.0);
                    d += 2.0;
                }
                assert!(steepest < 0.3, "span {k} approach {steepest}");
            }
        }
    }

    /// Every piece of cover on a bridge: how far along each stands, in order.
    fn cover_on(m: &MapLayout, s: &Span) -> Vec<f32> {
        let mut on: Vec<f32> = m
            .kit
            .iter()
            .filter(|k| (k.pos.y - s.top).abs() < 0.3)
            .map(|k| ((k.pos.xz() - s.a).dot(s.dir()), (k.pos.xz() - s.a).dot(s.dir().perp())))
            .filter(|(along, off)| *along >= 0.0 && *along <= s.len() && off.abs() < s.width * 0.5)
            .map(|(along, _)| along)
            .collect();
        on.sort_by(f32::total_cmp);
        on
    }

    #[test]
    fn bridges_have_cover_and_a_clear_lane() {
        let m = map();
        for (k, s) in SPANS.iter().enumerate() {
            let on = cover_on(m, s);
            assert!(!on.is_empty(), "span {k} has no cover");
            assert!(on[0] <= 60.0 && s.len() - on[on.len() - 1] <= 70.0, "span {k}: {on:?}");
            assert!(on.windows(2).all(|g| (40.0..=60.0).contains(&(g[1] - g[0]))), "span {k}: {on:?}");
            let centre = s.at(s.len() * 0.5, 0.0);
            let reach = s.len() * 0.5 + 1.0;
            let blocked = m.blocks.iter().filter(|b| b.collide && b.pos.xz().distance(centre) < reach + 30.0).find(|b| {
                let half = match b.shape {
                    Shape::Box(size) => size * 0.5,
                    Shape::Cylinder { radius, height } => Vec3::new(radius, height * 0.5, radius),
                    Shape::Sphere(r) => Vec3::splat(r),
                };
                let flat = half.x.min(half.z);
                let (along, off) = ((b.pos.xz() - s.a).dot(s.dir()), (b.pos.xz() - s.a).dot(s.dir().perp()));
                let wall = (b.rot * Vec3::Y).y > 0.99;
                wall && along > 0.0 && along < s.len() && off.abs() < 2.0 + flat && b.pos.y - half.y < s.top + 2.2 && b.pos.y + half.y > s.top + 0.1
            });
            assert!(blocked.is_none(), "span {k} centre lane: {blocked:?}");
        }
    }

    /// The Yamuna's five bridges each have their own look: what stands round each is different.
    #[test]
    fn bridges_differ() {
        let m = map();
        let mut seen = HashSet::new();
        for (k, s) in SPANS[..5].iter().enumerate() {
            let near: Vec<_> = m.blocks.iter().filter(|b| {
                let (along, off) = ((b.pos.xz() - s.a).dot(s.dir()), (b.pos.xz() - s.a).dot(s.dir().perp()));
                along > -5.0 && along < s.len() + 5.0 && off.abs() < 170.0 && b.mat != Mat::Wood
            }).collect();
            let count = |mat: Mat| near.iter().filter(|b| b.mat == mat).count();
            let tall = near.iter().map(|b| b.pos.y as i32).max().unwrap_or(0);
            let tilted = near.iter().filter(|b| (b.rot * Vec3::Y).y < 0.99).count();
            assert!(seen.insert((count(Mat::Steel), count(Mat::TransitBlue), count(Mat::TransitYellow), tilted, tall)), "span {k} looks like another");
            match k {
                0 => assert!(tall > 100 && tilted >= 30, "{tall} {tilted}"),
                1 => assert!(count(Mat::Steel) > 100, "{}", count(Mat::Steel)),
                2 => assert!(count(Mat::TransitBlue) > 8, "{}", count(Mat::TransitBlue)),
                3 => assert!(count(Mat::TransitYellow) > 0, "toll canopy"),
                _ => assert!(count(Mat::Steel) > 16, "{}", count(Mat::Steel)),
            }
        }
    }

    /// About 9,000 pieces, spread evenly: every district has its share and no 100 m square of the
    /// map is crowded.
    #[test]
    fn kit_is_thin_and_even() {
        let m = map();
        assert!((8000..=10000).contains(&m.kit.len()), "{}", m.kit.len());
        for place in &PLACES {
            let here = m.kit.iter().filter(|k| inside_by(place, k.pos.xz()) >= 0.0).count();
            assert!(here >= place.tier.kit() / 3, "{}: {here}", place.name);
        }
        let mut cells = std::collections::HashMap::new();
        for k in &m.kit {
            *cells.entry(((k.pos.x / 100.0).floor() as i32, (k.pos.z / 100.0).floor() as i32)).or_insert(0) += 1;
        }
        let most = cells.values().max().unwrap();
        assert!(*most < 90, "{most} in one 100 m square");
    }

    /// Going with a kit piece, its collider has gone too: none is left with nothing to draw.
    /// A house's stairs are hidden wooden ramps as well, but those are tilted.
    #[test]
    fn no_collider_is_left_behind() {
        let m = map();
        let drawn: HashSet<[u32; 2]> = m.kit.iter().map(|k| [k.pos.x.to_bits(), k.pos.z.to_bits()]).collect();
        let hidden = |b: &&crate::map::Block| b.mat == Mat::Wood && !b.visible && b.collide && (b.rot * Vec3::Y).y > 0.99;
        let orphans: Vec<_> = m.blocks.iter().filter(hidden).filter(|b| !drawn.contains(&[b.pos.x.to_bits(), b.pos.z.to_bits()])).collect();
        assert!(orphans.is_empty(), "{} {:?}", orphans.len(), orphans.first());
        // A gumti's walls stand off to the side of it, and are steel: the whole kit, each way round.
        assert_eq!(kit::out_of_step(m), (0, 0));
    }

    #[test]
    fn loot_adds_up() {
        let m = map();
        let one_match = m.chests.iter().filter(|c| c.set.is_none_or(|s| s.1 == 0)).count();
        // The districts' 136-154, and a village's trunk one match in three (`villages`).
        assert!((150..=180).contains(&one_match), "{one_match} chests a match");
        assert!((1500..=1700).contains(&m.floor_loot.len()), "{} floor spots", m.floor_loot.len());
        assert!(m.chests.iter().all(|c| c.pos.xz().abs().max_element() <= HALF));
        assert!(m.floor_loot.iter().all(|p| p.xz().abs().max_element() <= HALF));
        let kalash = m.chests.iter().filter(|c| c.kind == ChestKind::Tijori).count();
        assert!(kalash >= 20, "{kalash} kalash");
    }

    /// A bridge's deck is over water but dry.
    #[test]
    fn no_loot_on_water() {
        let m = map();
        let wet = |p: Vec3| m.terrain.ground_at(p.x, p.z) == Some(ground::WATER) && p.y < 0.2;
        let bad: Vec<_> = m.chests.iter().map(|c| c.pos).chain(m.floor_loot.iter().copied()).filter(|p| wet(*p)).collect();
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn no_loot_at_holy_sites() {
        let (m, sites) = (map(), holy());
        assert!(sites.len() > 30);
        for p in m.chests.iter().map(|c| c.pos).chain(m.floor_loot.iter().copied()) {
            assert!(sites.iter().all(|(c, r)| p.xz().distance(*c) >= *r), "loot at {p}");
            assert!(!no_loot(p.xz()), "loot at {p}");
        }
    }

    /// A warrior can walk up all the open land: nothing between neighbouring samples is steeper
    /// than the 48 degrees `movement` allows, unless a building's plot or a wall made it so.
    #[test]
    fn open_land_has_no_cliffs() {
        let (m, t) = (map(), &map().terrain);
        let limit = 1.05 * t.step();
        let (mut steep, mut worst) = (0, 0.0f32);
        for i in 0..t.n - 1 {
            for j in 0..t.n - 1 {
                let k = i * t.n + j;
                let drop = (t.h[k] - t.h[k + t.n]).abs().max((t.h[k] - t.h[k + 1]).abs());
                if drop > limit && t.ground[k] != ground::WATER && on_open_land(m, Vec2::new(t.coord(i), t.coord(j))) {
                    steep += 1;
                    worst = worst.max(drop);
                }
            }
        }
        assert_eq!(steep, 0, "worst {worst:.1} m");
    }

    /// Every named spot stands on land, clear of the districts, every bridge has its name, and no
    /// name is used twice.
    #[test]
    fn named_spots_stand_clear() {
        let m = map();
        assert_eq!(m.pois.len(), PLACES.len() + SPOTS.len() + BRIDGES.len());
        let names: HashSet<_> = m.pois.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.len(), m.pois.len(), "a name is used twice");
        for (name, x, z, kind) in SPOTS {
            let p = Vec2::new(x, z);
            assert_ne!(m.terrain.ground_at(x, z), Some(ground::WATER), "{name}");
            assert!(PLACES.iter().all(|d| inside_by(d, p) < 0.0), "{name} is inside a district");
            let sheds = m.footprints.iter().filter(|f| f.mat != Mat::Water && f.pts[0].distance(p) < kind.reach()).count();
            assert!(sheds <= 12, "{name} has {sheds}");
        }
    }

    /// Water on the ground has a surface over it, lakes cut off at a district's edge included, out
    /// to the map's walls: past them a river's end is never seen from close.
    #[test]
    fn no_water_is_dry() {
        let m = map();
        let t = &m.terrain;
        let water: Vec<(&Footprint, Vec2, Vec2)> = m.footprints.iter().filter(|f| f.mat == Mat::Water).map(|f| {
            let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
            (f, lo, hi)
        }).collect();
        let (mut samples, mut dry) = (0, vec![]);
        for i in (0..t.n).step_by(3) {
            for j in (0..t.n).step_by(3) {
                let p = Vec2::new(t.coord(i), t.coord(j));
                if t.ground[i * t.n + j] == ground::WATER && p.abs().max_element() < HALF {
                    samples += 1;
                    if !water.iter().any(|(f, lo, hi)| p.cmpge(*lo).all() && p.cmple(*hi).all() && footprint::contains(&f.pts, p)) {
                        dry.push(p);
                    }
                }
            }
        }
        assert!(samples > 10000, "{samples}");
        assert!(dry.is_empty(), "{} of {samples} water samples are dry, as {:?}", dry.len(), dry.first());
    }

    /// Every roof held up in the air has columns under it.
    #[test]
    fn no_roof_hangs_in_the_air() {
        let m = map();
        let posts: Vec<Vec2> = m.blocks.iter().filter(|b| b.collide && b.mat == Mat::Concrete && matches!(b.shape, Shape::Cylinder { .. })).map(|b| b.pos.xz()).collect();
        for f in m.footprints.iter().filter(|f| f.mat != Mat::Water && f.base > 1.0) {
            let under = posts.iter().filter(|p| footprint::contains(&f.pts, **p)).count();
            assert!(under >= 4, "a roof at {} has {under} columns", f.pts[0]);
        }
    }

    /// No chest or find stands inside a building, on any seed.
    #[test]
    fn no_loot_behind_walls() {
        for seed in [7, 3, 77, 4242, 90001] {
            let m = generate(seed);
            let sealed: Vec<Vec3> = m.chests.iter().map(|c| c.pos).chain(m.floor_loot.iter().copied()).filter(|p| in_building(&m, *p)).collect();
            assert!(sealed.is_empty(), "seed {seed}: {} inside, as {:?}", sealed.len(), sealed.first());
        }
    }

    /// Each district has its sets of chests, all of a size, and they stand in different places.
    #[test]
    fn every_district_has_its_sets_of_chests() {
        let m = map();
        for (index, place) in PLACES.iter().enumerate() {
            let in_set = |set: u8| m.chests.iter().filter(|c| c.set == Some((index as u8, set))).map(|c| c.pos).collect::<Vec<_>>();
            let first = in_set(0);
            for set in 0..CHEST_SETS {
                let chests = in_set(set);
                assert!(chests.len() * 3 >= place.tier.chests() * 2, "{} set {set}: {} chests", place.name, chests.len());
                assert!(set == 0 || chests.iter().all(|c| !first.contains(c)), "{} set {set} shares a chest with set 0", place.name);
            }
        }
    }

    /// The open land's buildings keep off the roads and each other, its trees stand off the roads,
    /// and neither a tree nor a piece of the kit stands in a building.
    #[test]
    fn nothing_stands_in_the_way() {
        let m = map();
        let t = &m.terrain;
        let in_a_district = |p: Vec2| PLACES.iter().any(|d| inside_by(d, p) >= 0.0);
        let open: Vec<&Footprint> = m.footprints.iter().filter(|f| f.mat != Mat::Water && !in_a_district(f.pts[0])).collect();
        assert!(open.len() > 100, "{}", open.len());
        for (k, f) in open.iter().enumerate() {
            let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
            let (mut x, mut road) = (lo.x, 0);
            while x <= hi.x {
                let mut z = lo.y;
                while z <= hi.y {
                    road += (footprint::contains(&f.pts, Vec2::new(x, z)) && t.ground_at(x, z) == Some(ground::ROAD)) as u32;
                    z += 2.0;
                }
                x += 2.0;
            }
            assert_eq!(road, 0, "a building on a road at {}", f.pts[0]);
            for other in &open[k + 1..] {
                assert!(!other.pts.iter().any(|p| footprint::contains(&f.pts, *p)) && !f.pts.iter().any(|p| footprint::contains(&other.pts, *p)), "two buildings at {}", f.pts[0]);
            }
        }
        for p in m.props.iter().filter(|p| p.trunk.is_some()) {
            assert!(!matches!(t.ground_at(p.pos.x, p.pos.z), Some(ground::ROAD | ground::WATER)), "a tree in the way at {}", p.pos);
            assert!(!in_building(m, p.pos), "a tree in a building at {}", p.pos);
        }
        assert!(m.kit.iter().all(|k| !in_building(m, k.pos)));
    }

    /// The fort, the viaduct's stations and the power station are built of more than one box, and
    /// Anand Vihar has buses to fight round.
    #[test]
    fn landmarks_have_their_shapes() {
        let m = map();
        let near = |c: Vec2, r: f32, mat: Mat| m.blocks.iter().filter(|b| b.mat == mat && b.pos.xz().distance(c) < r).count();
        assert!(near(Vec2::new(-85.0, -1410.0), 200.0, Mat::RedSandstone) > 100, "fort");
        for x in [-36.0, 384.0] {
            assert!(near(Vec2::new(x, -470.0), 40.0, Mat::Concrete) >= 2, "no station at {x}");
        }
        assert!(near(Vec2::new(260.0, 1250.0), 90.0, Mat::Steel) >= 2, "power station");
        assert!(near(where_is("Anand Vihar"), 300.0, Mat::TransitBlue) >= 12, "buses");
    }

    #[test]
    fn a_cut_outline_stays_inside() {
        let lake = [Vec2::new(-300.0, -10.0), Vec2::new(-200.0, -10.0), Vec2::new(-200.0, 10.0), Vec2::new(-300.0, 10.0)];
        let cut = clip_outline(&lake, 250.0);
        assert_eq!(cut.len(), 4);
        assert!(cut.iter().all(|p| p.x >= -250.0 && p.x <= -200.0));
        assert!((area(&cut).abs() - 1000.0).abs() < 0.01);
        assert!(clip_outline(&[Vec2::new(300.0, 0.0), Vec2::new(310.0, 0.0), Vec2::new(310.0, 9.0)], 250.0).is_empty());
    }
}
