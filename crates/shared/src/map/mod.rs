//! Map layout as plain data. The simulation turns blocks into colliders; the client turns them
//! into meshes. Layouts are generated deterministically from a seed so every peer agrees.

pub mod build;
pub mod india;
pub mod terrain;

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
    pub terrain: terrain::Terrain,
}
