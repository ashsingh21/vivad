//! Simulation-side code shared by the game client and a future dedicated server.
//! Must not depend on rendering: no meshes, materials, cameras or UI in here.

pub mod items;
pub mod map;
pub mod sim;
pub mod weapon;

/// The generator behind everything peers must agree on. rand's own `SmallRng` is this one on a
/// 64-bit machine and another on a 32-bit one, and a browser is 32-bit: it would have built a
/// different map from the same seed as the desktop player beside it.
pub use rand::rngs::Xoshiro256PlusPlus as SmallRng;
