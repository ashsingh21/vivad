//! Map layout as plain data. The simulation turns blocks into colliders; the client turns them
//! into meshes. Layouts are generated deterministically from a seed so every peer agrees.

pub mod build;
pub mod compose;
pub mod cp;
pub mod delhi;
pub mod footprint;
pub mod india;
pub mod kit;
pub mod navigation;
pub mod ncr;
mod ncr_art;
pub mod terrain;

/// The maps: the NCR, the game's one final map, and the four small Delhi maps merged into it,
/// which stay on their own for VIVAD_MAP and the checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MapId {
    /// Connaught Place and the head of Janpath. The browser's default: the NCR takes about 3.7 GB
    /// on the Mac and has never been tried in a tab.
    #[cfg_attr(target_arch = "wasm32", default)]
    Cp,
    /// Chandni Chowk's bazaar and the old city's lanes.
    Chandni,
    /// Hauz Khas: the village, the tank and the madrasa's ruins, the deer park.
    HauzKhas,
    /// India Gate, its lawns and canals, Kartavya Path.
    IndiaGate,
    /// The one final map (crate::map::compose): the real Delhi NCR, 4 km across, with the other
    /// maps merged into it. The default everywhere but the browser.
    #[cfg_attr(not(target_arch = "wasm32"), default)]
    Ncr,
}

impl MapId {
    pub const ALL: [MapId; 5] = [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate, MapId::Ncr];

    pub fn name(self) -> &'static str {
        match self {
            MapId::Cp => "cp",
            MapId::Chandni => "chandni",
            MapId::HauzKhas => "hauzkhas",
            MapId::IndiaGate => "indiagate",
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
            MapId::Ncr => "Dilli NCR",
        }
    }

    pub fn parse(s: &str) -> Option<MapId> {
        let s = s.trim().to_lowercase();
        MapId::ALL.into_iter().find(|m| m.name() == s).or(match s.as_str() {
            "hauz-khas" | "hauz_khas" => Some(MapId::HauzKhas),
            "india-gate" | "india_gate" => Some(MapId::IndiaGate),
            "chandni-chowk" | "chandnichowk" => Some(MapId::Chandni),
            "connaught" | "connaught-place" => Some(MapId::Cp),
            "kali" | "kaliyuga" | "kali-yuga" => Some(MapId::Ncr),
            _ => None,
        })
    }

    /// VIVAD_MAP=cp|chandni|hauzkhas|indiagate|ncr, else the default map.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum ChestKind {
    /// Carved wooden temple chest.
    Peti,
    /// Golden urn: better odds of high rarity.
    Kalash,
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
    /// Holy sites, as a point and a radius: nothing that draws a fight lies inside one.
    pub holy: Vec<(Vec2, f32)>,
    /// The street kit's pieces on a city map (crate::map::kit); drawn by the client, merged.
    pub kit: Vec<kit::KitPiece>,
    /// The river across the map, if it has one.
    pub river: River,
    /// Which family of map this is: what the client paints it with and how bots find their way.
    pub style: MapStyle,
    pub terrain: terrain::Terrain,
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

    /// The owner's default match: 64 warriors on the NCR, and a small map keeps its own size.
    #[test]
    fn the_default_match_is_64_on_the_ncr() {
        assert_eq!(MapId::Ncr.default_bots() + 1, 64);
        for map in [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate] {
            assert_eq!(map.default_bots() + 1, 16, "{map:?}");
        }
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(MapId::default(), MapId::Ncr);
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

    /// Every map's seed-7 contents, recorded before the style and river refactor. A change here
    /// means a map changed: update the numbers only if that was the intent.
    #[test]
    fn maps_are_unchanged() {
        assert_eq!(fingerprint(MapId::Cp), (6811, 60, 147, 102, 379, 728, 11, 10784036215856373148));
        assert_eq!(fingerprint(MapId::Chandni), (14376, 879, 98, 124, 69, 2371, 11, 10426603982683760837));
        assert_eq!(fingerprint(MapId::HauzKhas), (6431, 134, 81, 81, 350, 479, 2, 3684859097812863807));
        assert_eq!(fingerprint(MapId::IndiaGate), (630, 6, 70, 60, 585, 774, 5, 1864056156504546495));
    }

    #[test]
    fn holy_sites_have_no_loot_and_no_names() {
        for map in [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate] {
            for seed in [1, 7, 23, 99] {
                let m = generate(map, seed);
                let spots = m.chests.iter().map(|c| c.pos).chain(m.floor_loot.iter().copied());
                for p in spots {
                    let near = delhi::holy_sites(map).iter().any(|&(x, z, r)| p.xz().distance(Vec2::new(x, z)) < r);
                    assert!(!near, "loot at {p} on {map:?}, seed {seed}");
                }
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
        let river = ncr::generate(7).river;
        for p in [Vec2::ZERO, Vec2::new(300.0, 120.0), Vec2::new(-40.0, 900.0), Vec2::new(30.0, -500.0)] {
            assert!((river.distance(p) - ncr::river_distance(p)).abs() < 1e-3);
        }
        assert_eq!(River::None.distance(Vec2::ZERO), 1.0e4);
    }
}
