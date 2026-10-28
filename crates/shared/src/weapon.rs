//! Weapon asset conventions shared by every exported .glb.
//!
//! Convention (glTF / Bevy space, metres, right-handed, Y up):
//! - Origin is the centre of the main grip, where the hand closes.
//! - The blade (or business end) points along local **+Y**.
//! - The cutting edge faces local **-Z**, the spine faces **+Z**.
//! - Flat faces of the blade point along ±X.
//!
//! In Blender the same axes are: blade +Z, edge +Y, spine -Y
//! (the glTF exporter converts Z-up to Y-up: Blender (x, y, z) -> glTF (x, z, -y)).

use bevy::math::Vec3;

pub const WEAPON_FORWARD: Vec3 = Vec3::Y;
pub const WEAPON_EDGE: Vec3 = Vec3::NEG_Z;
