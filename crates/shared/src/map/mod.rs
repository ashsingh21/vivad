//! Map layout as plain data. The simulation turns blocks into colliders; the client turns them
//! into meshes. Layouts are generated deterministically from a seed so every peer agrees.

pub mod build;
pub mod cp;
pub mod delhi;
pub mod footprint;
pub mod india;
pub mod kit;
pub mod terrain;

/// The maps: Panga's four from Delhi, and Vivad's ancient India.
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
}

impl MapId {
    pub const ALL: [MapId; 5] = [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate, MapId::Bharat];
    /// Panga's maps, the ones a room is drawn on.
    pub const DELHI: [MapId; 4] = [MapId::Cp, MapId::Chandni, MapId::HauzKhas, MapId::IndiaGate];

    pub fn name(self) -> &'static str {
        match self {
            MapId::Cp => "cp",
            MapId::Chandni => "chandni",
            MapId::HauzKhas => "hauzkhas",
            MapId::IndiaGate => "indiagate",
            MapId::Bharat => "bharat",
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
            _ => None,
        })
    }

    /// VIVAD_MAP=cp|chandni|hauzkhas|indiagate|bharat, else Connaught Place.
    pub fn from_env() -> MapId {
        std::env::var("VIVAD_MAP").ok().and_then(|s| MapId::parse(&s)).unwrap_or_default()
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
/// A shop's signboard, or the shelves of goods at the back of a shop, for the client to draw
/// (crate::buildings in the game): the centre of its face, the way it faces (x, z), its size.
#[derive(Clone, Debug, Reflect)]
pub struct ShopSign {
    pub pos: Vec3,
    pub facing: Vec2,
    pub width: f32,
    pub height: f32,
    pub shelves: bool,
}

#[derive(Clone, Debug, Reflect)]
pub struct Footprint {
    pub pts: Vec<Vec2>,
    pub base: f32,
    pub top: f32,
    pub mat: Mat,
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
    /// The street kit's pieces on a city map (crate::map::kit); drawn by the client, merged.
    pub kit: Vec<kit::KitPiece>,
    /// Signboards and shop shelves set into hand-built blocks (Connaught Place's ring).
    pub signs: Vec<ShopSign>,
    /// A river across the map, as its channel's x range (the India map's).
    pub river: Option<(f32, f32)>,
    pub terrain: terrain::Terrain,
}
