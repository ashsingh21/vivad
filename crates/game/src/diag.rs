//! Memory diagnostics for big maps (docs/ncr-port.md, step 3).
//!
//! VIVAD_SKIP=kit,trees,blocks,city leaves those out of the map's art (the street kit, the batched
//! trees and bushes, the hand-built blocks, the city's buildings), to see what each costs.
//! VIVAD_MEMORY=1 logs every 10 s the meshes and images held on the CPU side and the resident set.

use bevy::prelude::*;
use std::sync::OnceLock;

pub struct DiagPlugin;

impl Plugin for DiagPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("VIVAD_MEMORY").is_some() {
            app.add_systems(Update, report);
        }
    }
}

/// Whether VIVAD_SKIP names `part`.
pub fn skip(part: &str) -> bool {
    static SKIP: OnceLock<Vec<String>> = OnceLock::new();
    SKIP.get_or_init(|| std::env::var("VIVAD_SKIP").unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .iter()
        .any(|s| s == part)
}

fn report(time: Res<Time<Real>>, mut last: Local<f32>, meshes: Res<Assets<Mesh>>, images: Res<Assets<Image>>) {
    if time.elapsed_secs() - *last < 10.0 {
        return;
    }
    *last = time.elapsed_secs();
    let (mut n, mut verts, mut bytes, mut gpu_only) = (0usize, 0usize, 0u64, 0usize);
    for (_, m) in meshes.iter() {
        // Already handed to the GPU and its own copy dropped (RenderAssetUsages::RENDER_WORLD).
        if !m.asset_usage.contains(bevy::asset::RenderAssetUsages::MAIN_WORLD) {
            gpu_only += 1;
            continue;
        }
        n += 1;
        let v = m.count_vertices();
        verts += v;
        bytes += v as u64 * m.get_vertex_size();
        bytes += m.indices().map_or(0, |i| i.len() as u64 * if matches!(i, bevy::mesh::Indices::U16(_)) { 2 } else { 4 });
    }
    let (imgs, img_bytes) = images.iter().fold((0usize, 0u64), |(c, b), (_, i)| (c + 1, b + i.data.as_ref().map_or(0, |d| d.len() as u64)));
    #[cfg(target_os = "linux")]
    let rss = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmRSS")).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())).unwrap_or(0) / 1024;
    #[cfg(not(target_os = "linux"))]
    let rss = 0;
    info!("[memory] {n} meshes, {:.1} M vertices, {} MB (and {gpu_only} on the GPU only); {imgs} images, {} MB; resident {rss} MB", verts as f32 / 1e6, bytes >> 20, img_bytes >> 20);
}
