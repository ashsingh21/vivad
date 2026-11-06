//! Each map's size and a fingerprint of where everything in it stands, to see that a change to
//! the map code leaves a map as it was (or how much it moved).
//!
//! cargo run --release -p shared --bin map_stats [-- map ...]   (VIVAD_MAP_SEED picks the seed; all maps by default)

use std::hash::{Hash, Hasher};

use shared::map::{MapId, generate};

fn main() {
    let seed = shared::sim::map_seed();
    let maps: Vec<MapId> = {
        let named: Vec<MapId> = std::env::args().skip(1).filter_map(|s| MapId::parse(&s)).collect();
        if named.is_empty() { MapId::ALL.to_vec() } else { named }
    };
    for id in maps {
        let t = std::time::Instant::now();
        let m = generate(id, seed);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut put = |v: bevy::math::Vec3| [v.x, v.y, v.z].iter().for_each(|f| f.to_bits().hash(&mut h));
        m.blocks.iter().for_each(|b| put(b.pos));
        m.props.iter().for_each(|p| put(p.pos));
        m.kit.iter().for_each(|k| put(k.pos));
        m.chests.iter().for_each(|c| put(c.pos));
        m.floor_loot.iter().for_each(|p| put(*p));
        // Kit standing on nothing (crate::map::compose::floating): a roof, a slab or a step under it.
        let floats = shared::map::compose::floating(&m);
        println!(
            "{:<10} {:>7} blocks {:>6} props {:>6} kit {:>5} footprints {:>4} chests {:>5} loot {:>4} floating  hash {:016x}  ({:.1} s)",
            id.name(), m.blocks.len(), m.props.len(), m.kit.len(), m.footprints.len(), m.chests.len(), m.floor_loot.len(), floats.len(), h.finish(), t.elapsed().as_secs_f32()
        );
        for k in floats.iter().take(3) {
            println!("    floating: {} at {:.1?}", k.name, k.pos);
        }
    }
}
