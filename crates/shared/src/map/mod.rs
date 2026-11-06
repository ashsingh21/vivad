//! Map layout as plain data. The simulation turns blocks into colliders; the client turns them
//! into meshes. Layouts are generated deterministically from a seed so every peer agrees.

pub mod build;
pub mod compose;
pub mod cp;
pub mod delhi;
pub mod footprint;
pub mod india;
pub mod interiors;
pub mod kit;
pub mod navigation;
pub mod ncr;
mod ncr_art;
pub mod place;
pub mod parking;
pub mod stalls;
pub mod terrain;
pub mod thickets;

/// The maps: the NCR, the game's one final map, and the four small Delhi maps merged into it,
/// which stay on their own for VIVAD_MAP and the checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MapId {
    /// Connaught Place and the head of Janpath.
    #[default]
    Cp,
    /// Chandni Chowk's bazaar and the old city's lanes.
    Chandni,
    /// Hauz Khas: the village, the tank and the madrasa's ruins, the deer park.
    HauzKhas,
    /// India Gate, its lawns and canals, Kartavya Path.
    IndiaGate,
    /// The ancient-India map (Vivad's).
    Bharat,
    /// The NCR (crate::map::compose, from the game-playable branch): the real Delhi NCR, 4 km
    /// across, with the Delhi maps merged into it. Desktop only for now: it takes about 3.7 GB on
    /// the Mac and has never been tried in a browser tab.
    Ncr,
}

impl MapId {
    pub const ALL: [MapId; 6] = [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate, MapId::Bharat, MapId::Ncr];
    /// Panga's maps, the ones a room is drawn on.
    pub const DELHI: [MapId; 4] = [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate];

    pub fn name(self) -> &'static str {
        match self {
            MapId::Cp => "cp",
            MapId::Chandni => "chandni",
            MapId::HauzKhas => "hauzkhas",
            MapId::IndiaGate => "indiagate",
            MapId::Bharat => "bharat",
            MapId::Ncr => "ncr",
        }
    }

    /// Its name on screen.
    pub fn title(self) -> &'static str {
        match self {
            MapId::Cp => "Connaught Place",
            MapId::Chandni => "Chandni Chowk",
            MapId::HauzKhas => "Hauz Khas",
            MapId::IndiaGate => "India Gate",
            MapId::Bharat => "Bharat",
            MapId::Ncr => "Dilli NCR",
        }
    }

    pub fn parse(s: &str) -> Option<MapId> {
        let s = s.trim().to_lowercase();
        MapId::ALL.into_iter().find(|m| m.name() == s).or(match s.as_str() {
            "india" | "vivad" => Some(MapId::Bharat),
            "hauz-khas" | "hauz_khas" => Some(MapId::HauzKhas),
            "india-gate" | "india_gate" => Some(MapId::IndiaGate),
            "chandni-chowk" | "chandnichowk" => Some(MapId::Chandni),
            "connaught" | "connaught-place" => Some(MapId::Cp),
            "kali" | "kaliyuga" | "kali-yuga" | "dilli-ncr" => Some(MapId::Ncr),
            _ => None,
        })
    }

    /// VIVAD_MAP=cp|chandni|hauzkhas|indiagate|bharat|ncr, else Connaught Place.
    pub fn from_env() -> MapId {
        std::env::var("VIVAD_MAP").ok().and_then(|s| MapId::parse(&s)).unwrap_or_default()
    }

    /// The bots that fill a match offline, unless VIVAD_BOTS says: 64 warriors on the NCR, so the
    /// first ring is the whole map, and 16 on a small map.
    pub fn default_bots(self) -> u32 {
        match self {
            MapId::Ncr => 63,
            _ => 15,
        }
    }

    pub fn index(self) -> u8 {
        MapId::ALL.iter().position(|m| *m == self).unwrap_or(0) as u8
    }

    pub fn from_index(i: u8) -> MapId {
        MapId::ALL.get(i as usize).copied().unwrap_or_default()
    }
}

/// Build a map. Every peer in a match builds the same one from the same id and seed.
pub fn generate(map: MapId, seed: u64) -> MapLayout {
    match map {
        MapId::Bharat => india::generate(seed),
        MapId::Ncr => compose::generate(seed),
        _ => delhi::generate(map, seed),
    }
}

use bevy::prelude::*;

/// Graybox surface categories. The client maps these to colours now and to art kits later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum Mat {
    Grass,
    Dirt,
    Sandstone,
    RedSandstone,
    Granite,
    Plaster,
    PlasterOchre,
    PlasterBlue,
    Brick,
    Wood,
    Cloth,
    Foliage,
    Bark,
    Water,
    Gold,
    Marble,
    Thatch,
    /// Clean lime-washed stucco (Connaught Place's white).
    Stucco,
    /// Floors of square stone tiles (verandahs).
    Stone,
    /// Stucco painted one of the PAINT colours (shutters).
    Paint(u8),
    /// Rusticated stucco: white, cut in horizontal bands (Connaught Place's ground storey).
    Rustic,
    /// Pale buff sandstone (Lutyens' Delhi: India Gate, Kartavya Path's buildings).
    Buff,
    /// Dressed grey-buff stone blocks: the base course along Connaught Place's walls and under its
    /// columns.
    Plinth,
    Concrete,
    Asphalt,
    Glass,
    Steel,
    TransitBlue,
    TransitYellow,
    ContainerRed,
    Pavers,
    Rubber,
    PaintWhite,
    ShopSign,
    MetroSign,
}

/// Paint colours (sRGB): Delhi's limewash, the style guide's `paint` tokens
/// (assets/style/palette.json) in order: rose, ochre, mint, powder blue, cream, terracotta, lilac,
/// and the old green of shutters; then the kerbs' yellow and black (KERB_YELLOW, KERB_BLACK).
/// `k` wraps.
pub fn paint(k: u8) -> [f32; 3] {
    const NAMES: [&str; 10] = [
        "paint.rose", "paint.ochre", "paint.mint", "paint.powder_blue", "paint.cream", "paint.terracotta", "paint.lilac", "paint.old_green",
        "ground.kerb_yellow", "ground.kerb_black",
    ];
    crate::style::srgb(NAMES[k as usize % NAMES.len()])
}

/// Delhi's kerbs, painted in yellow and black stripes: `Mat::Paint` of these.
pub const KERB_YELLOW: u8 = 8;
pub const KERB_BLACK: u8 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum Shape {
    /// Full extents.
    Box(Vec3),
    Cylinder { radius: f32, height: f32 },
    Sphere(f32),
}

#[derive(Clone, Debug, Reflect)]
pub struct Block {
    pub pos: Vec3,
    pub rot: Quat,
    pub shape: Shape,
    pub mat: Mat,
    pub collide: bool,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum ChestKind {
    /// The painted steel trunk.
    Peti,
    /// An old iron safe, gold inside: better odds of high rarity.
    Tijori,
}

#[derive(Clone, Debug, Reflect)]
pub struct ChestSpawn {
    pub pos: Vec3,
    pub yaw: f32,
    pub kind: ChestKind,
    /// The district it belongs to and which of that district's sets it is in: out only in a match
    /// that draws that set (`sim::loot::chest_sets`). None: out in every match.
    pub set: Option<(u8, u8)>,
}

#[derive(Clone, Debug, Reflect)]
pub struct Poi {
    pub name: String,
    pub center: Vec3,
    pub radius: f32,
}

/// A placed art asset (tree, bush). `trunk` = (radius, height) of its solid collider, if any.
#[derive(Clone, Debug, Reflect)]
pub struct Prop {
    pub model: &'static str,
    pub pos: Vec3,
    pub yaw: f32,
    pub scale: f32,
    pub trunk: Option<(f32, f32)>,
}

/// A visual-only kit piece (arch surround, jharokha, chhatri) with non-uniform scale.
#[derive(Clone, Debug, Reflect)]
pub struct Ornament {
    pub model: &'static str,
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

/// A light for the client to place: an oil lamp (flames drawn and flickering) or an unseen fill
/// standing in for daylight bounced into a dark interior.
#[derive(Clone, Debug, Reflect)]
pub struct Lamp {
    pub pos: Vec3,
    /// Linear RGB.
    pub color: Vec3,
    pub lumens: f32,
    pub range: f32,
    /// Flames on a ring round `pos`: (radius, count). None for a fill light.
    pub flames: Option<(f32, u32)>,
}

/// A building from a map's footprint data: its outline (counter-clockwise seen from above)
/// extruded from `base` to `top`. Solid; the simulation gives it a mesh collider.
/// What a ShopSign is.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum SignKind {
    /// A lettered signboard.
    Board,
    /// A glowing tube light under a ceiling, facing down; `facing` is the way it runs.
    Light,
    /// A shop window's glass.
    Glass,
    /// A tube light's glow on the floor under it, facing up; `facing` is the way its length runs.
    Pool,
    /// A small lettered board hung between columns, always red.
    Hanging,
}

/// A shop's signboard, its shelves or its lights, for the client to draw (crate::buildings in the
/// game): the centre of its face, the way it faces (x, z), its size.
#[derive(Clone, Debug, Reflect)]
pub struct ShopSign {
    pub pos: Vec3,
    pub facing: Vec2,
    pub width: f32,
    pub height: f32,
    pub kind: SignKind,
}

#[derive(Clone, Debug, Reflect)]
pub struct Footprint {
    pub pts: Vec<Vec2>,
    pub base: f32,
    pub top: f32,
    pub mat: Mat,
}

impl MapLayout {
    /// Whether a point lies in a holy site's circle.
    pub fn is_holy(&self, p: Vec2) -> bool {
        self.holy.iter().any(|(c, r)| p.distance(*c) < *r)
    }

    /// The rooms of `footprints[index]`, if it is one of the hollow ones.
    pub fn rooms_of(&self, index: usize) -> Option<&interiors::Interior> {
        self.room_of.get(index).and_then(|k| self.rooms.get(*k as usize))
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MapLayout {
    pub name: String,
    /// Half size of the playable square.
    pub half_extent: f32,
    pub blocks: Vec<Block>,
    /// Breakable doors (also blocks, kept separate so they get health).
    pub doors: Vec<Block>,
    pub chests: Vec<ChestSpawn>,
    pub floor_loot: Vec<Vec3>,
    pub pois: Vec<Poi>,
    pub props: Vec<Prop>,
    pub ornaments: Vec<Ornament>,
    pub lamps: Vec<Lamp>,
    pub footprints: Vec<Footprint>,
    /// The hollow buildings (crate::map::interiors), and by footprint which of them it is. They
    /// are kept here and not on `Footprint` so that a building is the same data whether it is
    /// hollow or not.
    pub rooms: Vec<interiors::Interior>,
    pub room_of: Vec<u32>,
    /// Where in `blocks` the rooms are drawn (Interior::drawn): the game draws them near only,
    /// casting no shadow.
    pub rooms_drawn: std::ops::Range<usize>,
    /// Where a place of worship or a ruin stands: buildings never opened up.
    pub closed: Vec<Vec2>,
    /// Connaught Place's ring of shops, which the NCR furnishes (crate::map::interiors).
    pub ring_shops: Vec<interiors::RingShop>,
    pub ring_pieces: Vec<interiors::Piece>,
    /// Holy sites, as a point and a radius: nothing that draws a fight lies inside one.
    pub holy: Vec<(Vec2, f32)>,
    /// The street kit's pieces on a city map (crate::map::kit); drawn by the client, merged.
    pub kit: Vec<kit::KitPiece>,
    /// Signboards and shop shelves set into hand-built blocks (Connaught Place's ring).
    pub signs: Vec<ShopSign>,
    /// The halls' street doorways (crate::map::delhi): where, on the wall, and outward (x, z).
    /// Kept clear of street furniture and trees.
    pub entrances: Vec<(Vec2, Vec2)>,
    /// A river across the map, as its channel's x range (the India map's).
    pub river: Option<(f32, f32)>,
    /// Where vehicles may stand on the NCR (crate::map::parking); none on a small map.
    pub parking: Vec<parking::Bay>,
    /// The market's rows on the NCR (crate::map::stalls); none on a small map.
    pub stalls: stalls::Rows,
    /// The river across the map, if it has one (the NCR's: `river` is the India map's channel).
    pub channel: River,
    /// Which family of map this is: what the client paints it with and how bots find their way.
    pub style: MapStyle,
    pub terrain: terrain::Terrain,
    /// Where the roof slabs are, for `place_at`.
    pub overhead: place::Overhead,
    /// Leaves: they hide a body and stop nothing.
    pub soft: Vec<thickets::SoftCover>,
}

/// How a map is dressed and walked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapStyle {
    /// Painted ground classes, footprints, street kit, skyline, kites: the Delhi maps.
    #[default]
    City,
    /// The NCR arena (crate::map::ncr): its own ground paint and street router.
    Ncr,
}

/// A river running north to south across a map.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum River {
    #[default]
    None,
    /// A channel winding round x = `x0`: its centre is at `x0 + amp * sin(z / wavelength)`.
    Sine { x0: f32, amp: f32, wavelength: f32, half_width: f32 },
}

impl River {
    /// Metres from the channel's bank, 0 in the channel. Far away on a map with no river.
    pub fn distance(&self, p: Vec2) -> f32 {
        match *self {
            River::None => 1.0e4,
            River::Sine { x0, amp, wavelength, half_width } => ((p.x - x0 - amp * (p.y / wavelength).sin()).abs() - half_width).max(0.0),
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    /// 64 warriors on the NCR, and a small map keeps its own size; Connaught Place is the default
    /// map (the NCR is desktop only for now, and picked on the start screen).
    #[test]
    fn the_ncr_match_is_64() {
        assert_eq!(MapId::Ncr.default_bots() + 1, 64);
        for map in [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate] {
            assert_eq!(map.default_bots() + 1, 16, "{map:?}");
        }
        assert_eq!(MapId::default(), MapId::Cp);
    }

    /// Blocks, footprints, chests, floor loot, props, kit pieces, POIs and a checksum of their
    /// positions, so a refactor that should change nothing can prove it.
    fn fingerprint(map: MapId) -> (usize, usize, usize, usize, usize, usize, usize, u64) {
        let m = generate(map, 7);
        let mut sum = 0u64;
        let mut add = |p: Vec3| {
            for c in p.to_array() {
                sum = sum.wrapping_mul(31).wrapping_add(c.to_bits() as u64);
            }
        };
        m.blocks.iter().for_each(|b| add(b.pos));
        m.chests.iter().for_each(|c| add(c.pos));
        m.floor_loot.iter().for_each(|p| add(*p));
        m.props.iter().for_each(|p| add(p.pos));
        m.kit.iter().for_each(|k| add(k.pos));
        (m.blocks.len(), m.footprints.len(), m.chests.len(), m.floor_loot.len(), m.props.len(), m.kit.len(), m.pois.len(), sum)
    }

    /// Every map's seed-7 contents, recorded when the NCR was ported (with the game-playable
    /// branch's fixes to Connaught Place's chests, and its racks kept off the stair wells; then
    /// Connaught Place's wider stairs and railings, and windows framed inside). A
    /// change here means a map changed: update the numbers only if that was the intent.
    #[test]
    fn maps_are_unchanged() {
        assert_eq!(fingerprint(MapId::Cp), (42085, 60, 173, 161, 485, 6647, 11, 10353479721749200354));
        assert_eq!(fingerprint(MapId::Chandni), (22527, 879, 171, 194, 98, 3623, 11, 14345054941457027638));
        assert_eq!(fingerprint(MapId::HauzKhas), (11673, 137, 95, 144, 432, 1529, 2, 11088843970420611116));
        assert_eq!(fingerprint(MapId::IndiaGate), (1882, 6, 70, 92, 747, 1644, 5, 18077901147479690835));
    }

    /// No holy site's name on screen. (Their loot is kept off them on the NCR only so far: on the
    /// small maps that would change their balance, which is still to be decided.)
    #[test]
    fn holy_sites_have_no_names() {
        for map in [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate] {
            for seed in [1, 7, 23, 99] {
                let m = generate(map, seed);
                assert!(m.pois.iter().all(|p| !delhi::HOLY_NAMES.contains(&p.name.as_str())));
            }
        }
    }

    /// The same two rules across the NCR, whose holy sites come from all its merged districts.
    #[test]
    fn ncr_holy_sites_have_no_loot_and_no_names() {
        let sites = compose::holy();
        for seed in [1, 7, 23, 99] {
            let m = generate(MapId::Ncr, seed);
            for p in m.chests.iter().map(|c| c.pos).chain(m.floor_loot.iter().copied()) {
                assert!(sites.iter().all(|(c, r)| p.xz().distance(*c) >= *r), "loot at {p}, seed {seed}");
            }
            assert!(m.pois.iter().all(|p| !delhi::HOLY_NAMES.contains(&p.name.as_str())));
        }
    }

    #[test]
    fn river_distance_matches_the_ncr_arena() {
        let river = ncr::generate(7).channel;
        for p in [Vec2::ZERO, Vec2::new(300.0, 120.0), Vec2::new(-40.0, 900.0), Vec2::new(30.0, -500.0)] {
            assert!((river.distance(p) - ncr::river_distance(p)).abs() < 1e-3);
        }
        assert_eq!(River::None.distance(Vec2::ZERO), 1.0e4);
    }
}
