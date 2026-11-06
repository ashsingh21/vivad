//! Memory diagnostics for big maps (docs/ncr-port.md, step 3).
//!
//! VIVAD_SKIP=kit,trees,blocks,city leaves those out of the map's art (the street kit, the batched
//! trees and bushes, the hand-built blocks, the city's buildings), to see what each costs.
//! VIVAD_INSTANCE=0|1 merges or instances the trees and street kit (`instanced`), to compare.
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

/// Whether a map's trees or street kit (`count` of them) are drawn instanced, each copy sharing its
/// model's meshes, rather than merged into a mesh per chunk. On the desktop always: on Connaught
/// Place it halved the GPU memory (4.7 GB to 2.4) and was faster too (118 to 134 fps over the
/// block). In the browser (one thread, where entities cost) only past `above`, until it's measured
/// there. VIVAD_INSTANCE=0 or 1 forces it, to compare.
pub fn instanced(count: usize, above: usize) -> bool {
    static FORCE: OnceLock<Option<bool>> = OnceLock::new();
    let force = *FORCE.get_or_init(|| match std::env::var("VIVAD_INSTANCE").as_deref() {
        Ok("0") => Some(false),
        Ok("1") => Some(true),
        _ => None,
    });
    force.unwrap_or(!cfg!(target_arch = "wasm32") || count > above)
}

/// Whether VIVAD_SKIP names `part`.
pub fn skip(part: &str) -> bool {
    static SKIP: OnceLock<Vec<String>> = OnceLock::new();
    SKIP.get_or_init(|| std::env::var("VIVAD_SKIP").unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .iter()
        .any(|s| s == part)
}

fn report(
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    assets: Res<AssetServer>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
) {
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
    // By their size on the GPU (from the descriptor: the data itself may be gone, GPU only).
    let gpu = |i: &Image| {
        let d = &i.texture_descriptor;
        let px = (d.size.width as u64 * d.size.height as u64 * d.size.depth_or_array_layers as u64) as f64;
        let (bw, bh) = d.format.block_dimensions();
        let bytes = d.format.block_copy_size(None).unwrap_or(4) as f64 / (bw * bh) as f64;
        (px * bytes * if d.mip_level_count > 1 { 4.0 / 3.0 } else { 1.0 }) as u64
    };
    let mut big: Vec<(u64, String)> = images.iter().map(|(id, i)| (gpu(i), assets.get_path(id).map_or_else(|| format!("{:?} {}x{}", i.texture_descriptor.format, i.width(), i.height()), |p| p.to_string()))).collect();
    big.sort_by(|a, b| b.0.cmp(&a.0));
    let total: u64 = big.iter().map(|b| b.0).sum();
    // Grouped by folder, then the biggest.
    let mut folders: std::collections::HashMap<String, u64> = default();
    for (b, n) in &big {
        let f = n.rsplit_once('/').map_or("(made here)".to_string(), |(d, _)| d.split('#').next().unwrap_or(d).to_string());
        *folders.entry(f).or_default() += b;
    }
    let mut folders: Vec<_> = folders.into_iter().collect();
    folders.sort_by(|a, b| b.1.cmp(&a.1));
    info!("[memory] textures on the GPU: {} MB; by folder: {}", total >> 20, folders.iter().take(8).map(|(f, b)| format!("{} MB {f}", b >> 20)).collect::<Vec<_>>().join("; "));
    let top: Vec<String> = big.iter().take(8).map(|(b, n)| format!("{} MB {n}", b >> 20)).collect();
    // The GPU's own account (wgpu's allocator): what each labelled buffer and texture holds.
    if let Some(report) = device.and_then(|d| d.wgpu_device().generate_allocator_report()) {
        let mut by: std::collections::HashMap<String, (u64, usize)> = default();
        for a in &report.allocations {
            // Numbered labels grouped ("slab 17" with "slab 3").
            let name: String = a.name.chars().filter(|c| !c.is_ascii_digit()).collect();
            let e = by.entry(if name.is_empty() { "(unlabelled)".into() } else { name }).or_default();
            e.0 += a.size;
            e.1 += 1;
        }
        let mut by: Vec<_> = by.into_iter().collect();
        by.sort_by(|a, b| b.1.0.cmp(&a.1.0));
        let top: Vec<String> = by.iter().take(12).map(|(n, (b, c))| format!("{} MB {n} (x{c})", b >> 20)).collect();
        info!("[memory] GPU: {} MB allocated, {} MB reserved; {}", report.total_allocated_bytes >> 20, report.total_reserved_bytes >> 20, top.join("; "));
    }
    info!("[memory] largest images: {}", top.join("; "));
    info!("[memory] {n} meshes, {:.1} M vertices, {} MB (and {gpu_only} on the GPU only); {imgs} images, {} MB; resident {rss} MB", verts as f32 / 1e6, bytes >> 20, img_bytes >> 20);
}
