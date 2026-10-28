//! Simulation-side code shared by the game client and a future dedicated server.
//! Must not depend on rendering: no meshes, materials, cameras or UI in here.

pub mod items;
pub mod map;
pub mod sim;
pub mod weapon;
