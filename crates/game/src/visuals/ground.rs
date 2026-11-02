//! The ground of a big map. The small maps' way (a vertex per terrain sample, each with its own
//! colour, in three LODs of 32-sample tiles) would cost a 4 km map 12,000 entities and well over a
//! gigabyte, nearly all of it spent on land that is dead flat. So on a big map:
//!
//! - the colour goes into one image, a texel per terrain sample, with the dark band round each
//!   building's foot baked in (`paint_image`);
//! - every cell of the terrain whose samples all stand at one height joins one flat mesh, a quad
//!   per cell on a shared grid of corners (`flat_sheet`): one draw, and no shadow to cast;
//! - every other cell is a relief patch (`Patch`), which is a small map's terrain tile. Its last
//!   LOD, the one seen across the whole map, joins all the others of a `CLUSTER` in one mesh that is
//!   always on, a little under the nearer LODs: the paint is by position, so where two LODs overlap
//!   they show the same pixels. That is a few dozen draws where it was one for each patch in view.
//!
//! All of it shares one material, and on Ultra crate::surfaces moves it to the ground shader with
//! the paint image as its base colour. Meshes go to the GPU and leave main memory.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{ComputeTaskPool, ParallelSliceMut};
use shared::map::terrain::Terrain;
use shared::map::{MapLayout, Mat};

use super::{TERRAIN_LODS, TERRAIN_TILE};

/// Samples a side from which a terrain is drawn here. Every small map has 561.
pub(crate) const BIG_TERRAIN: usize = 1024;
/// How much the samples of a cell may differ and still make it flat, metres: less than the 6 cm
/// `Terrain::settle_under` sinks the ground under a slab, so those cells stay relief.
const FLAT: f32 = 0.01;
/// A relief patch rising less than this has nothing to cast a shadow with.
const SHADOWLESS: f32 = 1.0;
/// Width of the dark band round a building's foot, metres. Wider than the small maps' 2.5 m: at
/// two metres a sample that would hardly reach one sample.
const FOOT_BAND: f32 = 6.0;
/// How dark the ground is under a wall.
const FOOT_SHADE: f32 = 0.62;
/// Rows of texels painted per task.
const PAINT_ROWS: usize = 32;
/// Cells to a side of a far ground mesh: 512 m.
const CLUSTER: usize = 8;
/// How far the far ground lies under the nearer LODs drawn over it, metres.
const FAR_SINK: f32 = 0.1;

/// Draws the ground under `terrain` (the map's terrain entity).
pub(super) fn spawn(
    commands: &mut Commands,
    terrain: Entity,
    layout: &MapLayout,
    lod_scale: f32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let t = &layout.terrain;
    let started = bevy::platform::time::Instant::now();
    let paint = images.add(paint_image(layout));
    let painted = started.elapsed();
    let material = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, base_color_texture: Some(paint), ..default() });
    commands.entity(terrain).insert(Visibility::default());

    let tile = TERRAIN_TILE;
    let cells = (t.n - 1).div_ceil(tile);
    // The lowest and highest sample of each cell.
    let mut span = vec![(f32::MAX, f32::MIN); cells * cells];
    for ci in 0..cells {
        for cj in 0..cells {
            let (lo, hi) = &mut span[ci * cells + cj];
            for i in ci * tile..=((ci + 1) * tile).min(t.n - 1) {
                for j in cj * tile..=((cj + 1) * tile).min(t.n - 1) {
                    let h = t.at(i, j);
                    *lo = lo.min(h);
                    *hi = hi.max(h);
                }
            }
        }
    }
    let flat = |c: (f32, f32)| c.1 - c.0 <= FLAT;

    commands.spawn((Mesh3d(meshes.add(flat_sheet(t, cells, &span, flat))), MeshMaterial3d(material.clone()), Transform::default(), bevy::light::NotShadowCaster, crate::batch::probe::Tagged("ground"), ChildOf(terrain)));

    // The nearer LODs of each relief cell have their own meshes; the last, a mesh per cluster.
    let (near_lods, last) = TERRAIN_LODS.split_at(TERRAIN_LODS.len() - 1);
    let mut far: HashMap<(usize, usize), Patch> = HashMap::new();
    let mut patches = 0;
    let mut vertices = 0;
    for ci in 0..cells {
        for cj in 0..cells {
            let c = span[ci * cells + cj];
            if flat(c) {
                continue;
            }
            patches += 1;
            far.entry((ci / CLUSTER, cj / CLUSTER)).or_default().add_cell(t, ci * tile, cj * tile, last[0].0, FAR_SINK);
            let mut start = 0.0;
            for (lod, &(step, end)) in near_lods.iter().enumerate() {
                let mut patch = Patch::default();
                patch.add_cell(t, ci * tile, cj * tile, step, 0.0);
                let (mesh, centre) = patch.into_mesh();
                vertices += mesh.count_vertices();
                let base = crate::characters::LodRange(
                    VisibilityRange { start_margin: start..start + 10.0 * (lod > 0) as u8 as f32, end_margin: end..end + 10.0, use_aabb: false },
                    lod,
                );
                let mut e = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::from_translation(centre), base.scaled(lod_scale), base, crate::batch::probe::Tagged("ground"), ChildOf(terrain)));
                if c.1 - c.0 < SHADOWLESS {
                    e.insert(bevy::light::NotShadowCaster);
                } else {
                    e.insert(crate::quality::LodShadow);
                }
                start = end;
            }
        }
    }
    // Past any shadow's reach by the time it is the only ground there.
    let clusters = far.len();
    for patch in far.into_values() {
        let (mesh, centre) = patch.into_mesh();
        vertices += mesh.count_vertices();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), Transform::from_translation(centre), bevy::light::NotShadowCaster, crate::batch::probe::Tagged("far ground"), ChildOf(terrain)));
    }
    info!("ground: {} of {} cells flat, {patches} relief patches in {clusters} far meshes ({vertices} vertices), paint {}x{} in {:.2}s, {:.2}s in all", span.iter().filter(|c| flat(**c)).count(), span.len(), t.n, t.n, painted.as_secs_f32(), started.elapsed().as_secs_f32());
}

/// World XZ to the paint image's UV: texel i's centre on terrain sample i, as the Ultra level's
/// ground mask has it (crate::surfaces).
fn uv(t: &Terrain, x: f32, z: f32) -> [f32; 2] {
    let (scale, half) = (1.0 / (t.step() * t.n as f32), t.size * 0.5);
    let texel = 0.5 / t.n as f32;
    [(x + half) * scale + texel, (z + half) * scale + texel]
}

/// The flat cells, one quad each on a grid of corners shared between them, so two neighbours
/// meet at the same vertices and leave no crack. A quad's edge against a relief patch is hidden by
/// that patch's skirt.
fn flat_sheet(t: &Terrain, cells: usize, span: &[(f32, f32)], flat: impl Fn((f32, f32)) -> bool) -> Mesh {
    let (mut pos, mut uvs, mut idx) = (vec![], vec![], vec![]);
    // A corner is shared only at one height (to the millimetre): plateaus keep their own.
    let mut corners: HashMap<(usize, usize, i32), u32> = HashMap::new();
    let tile = TERRAIN_TILE;
    let mut corner = |ci: usize, cj: usize, h: f32| -> u32 {
        *corners.entry((ci, cj, (h * 1000.0).round() as i32)).or_insert_with(|| {
            let (x, z) = (t.coord((ci * tile).min(t.n - 1)), t.coord((cj * tile).min(t.n - 1)));
            pos.push([x, h, z]);
            uvs.push(uv(t, x, z));
            pos.len() as u32 - 1
        })
    };
    for ci in 0..cells {
        for cj in 0..cells {
            let c = span[ci * cells + cj];
            if !flat(c) {
                continue;
            }
            let h = c.0;
            let (p, q, r, s) = (corner(ci, cj, h), corner(ci + 1, cj, h), corner(ci + 1, cj + 1, h), corner(ci, cj + 1, h));
            idx.extend_from_slice(&[p, s, r, p, r, q]);
        }
    }
    let n = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, crate::buildings::tiers::usage())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(idx))
}

/// Cells of the heightfield, gathered into one mesh.
#[derive(Default)]
struct Patch {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl Patch {
    /// The cell from sample (`i0`, `j0`), every `step` samples, `sink` metres down, with a skirt hung
    /// from its edges.
    fn add_cell(&mut self, t: &Terrain, i0: usize, j0: usize, step: usize, sink: f32) {
        let samples = |a: usize, b: usize| -> Vec<usize> {
            let mut v: Vec<usize> = (a..b).step_by(step).collect();
            v.push(b);
            v
        };
        let (is, js) = (samples(i0, (i0 + TERRAIN_TILE).min(t.n - 1)), samples(j0, (j0 + TERRAIN_TILE).min(t.n - 1)));
        let Patch { pos, nrm, uvs, idx } = self;
        let base = pos.len() as u32;
        let mut vert = |i: usize, j: usize, drop: f32| -> u32 {
            let (x, z) = (t.coord(i), t.coord(j));
            pos.push([x, t.at(i, j) - sink - drop, z]);
            nrm.push(t.normal(x, z).to_array());
            uvs.push(uv(t, x, z));
            pos.len() as u32 - 1
        };
        let w = js.len() as u32;
        for &i in &is {
            for &j in &js {
                vert(i, j, 0.0);
            }
        }
        for a in 0..is.len() as u32 - 1 {
            for b in 0..w - 1 {
                let (p, q, r, s) = (base + a * w + b, base + (a + 1) * w + b, base + (a + 1) * w + b + 1, base + a * w + b + 1);
                idx.extend_from_slice(&[p, s, r, p, r, q]);
            }
        }
        // Each edge copied a little lower and joined to it, wound both ways.
        let skirt = 0.6 * step as f32 * t.step();
        let (first_i, last_i, first_j, last_j) = (is[0], *is.last().unwrap(), js[0], *js.last().unwrap());
        let edges: [Vec<(usize, usize)>; 4] = [js.iter().map(|&j| (first_i, j)).collect(), js.iter().map(|&j| (last_i, j)).collect(), is.iter().map(|&i| (i, first_j)).collect(), is.iter().map(|&i| (i, last_j)).collect()];
        for edge in edges {
            let top: Vec<u32> = edge.iter().map(|&(i, j)| vert(i, j, 0.0)).collect();
            let low: Vec<u32> = edge.iter().map(|&(i, j)| vert(i, j, skirt)).collect();
            for k in 0..top.len() - 1 {
                let (a, b, c, d) = (top[k], top[k + 1], low[k + 1], low[k]);
                idx.extend_from_slice(&[a, b, c, a, c, d, a, c, b, a, d, c]);
            }
        }
    }

    /// The mesh, its vertices measured from the middle of its bounds (LOD distances start there),
    /// and that middle.
    fn into_mesh(self) -> (Mesh, Vec3) {
        let (lo, hi) = self.pos.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
        let centre = (lo + hi) / 2.0;
        let pos: Vec<[f32; 3]> = self.pos.iter().map(|p| (Vec3::from(*p) - centre).to_array()).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, crate::buildings::tiers::usage())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_indices(Indices::U32(self.idx));
        crate::buildings::tiers::pack(&mut mesh);
        (mesh, centre)
    }
}

/// How much light reaches each terrain sample past the buildings: under a wall `FOOT_SHADE`,
/// rising to full over `FOOT_BAND`. Only samples next to a wall spread the band outward, so the
/// open land costs nothing.
fn foot_shade(layout: &MapLayout) -> Vec<f32> {
    let t = &layout.terrain;
    let n = t.n;
    let half = t.size * 0.5;
    let step = t.step();
    let mut inside = vec![false; n * n];
    for fp in layout.footprints.iter().filter(|f| f.mat != Mat::Water) {
        let (lo, hi) = fp.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let idx = |c: f32| (((c + half) / step).round().max(0.0) as usize).min(n - 1);
        for i in idx(lo.x)..=idx(hi.x) {
            for j in idx(lo.y)..=idx(hi.y) {
                if shared::map::footprint::contains(&fp.pts, Vec2::new(t.coord(i), t.coord(j))) {
                    inside[i * n + j] = true;
                }
            }
        }
    }
    let reach = (FOOT_BAND / step).ceil() as i32;
    let mut shade = vec![1.0f32; n * n];
    let at = |i: i32, j: i32| (i >= 0 && j >= 0 && (i as usize) < n && (j as usize) < n).then(|| i as usize * n + j as usize);
    for i in 0..n as i32 {
        for j in 0..n as i32 {
            if !inside[i as usize * n + j as usize] {
                continue;
            }
            shade[i as usize * n + j as usize] = FOOT_SHADE;
            let walled = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(a, b)| at(i + a, j + b).is_some_and(|k| !inside[k]));
            if !walled {
                continue;
            }
            for di in -reach..=reach {
                for dj in -reach..=reach {
                    let Some(k) = at(i + di, j + dj) else { continue };
                    if inside[k] {
                        continue;
                    }
                    let d = ((di * di + dj * dj) as f32).sqrt() * step;
                    let s = (1.0 - d / FOOT_BAND).max(0.0);
                    shade[k] = shade[k].min(1.0 - (1.0 - FOOT_SHADE) * s * s);
                }
            }
        }
    }
    shade
}

/// The map's colour, mips included: what a small map keeps per vertex.
fn paint_image(layout: &MapLayout) -> Image {
    let t = &layout.terrain;
    let n = t.n;
    let shade = foot_shade(layout);
    let mut data = vec![255u8; n * n * 4];
    data.par_chunk_map_mut(ComputeTaskPool::get(), n * 4 * PAINT_ROWS, |chunk, rows| {
        for (r, row) in rows.chunks_exact_mut(n * 4).enumerate() {
            let j = chunk * PAINT_ROWS + r;
            let z = t.coord(j);
            for (i, texel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x = t.coord(i);
                let k = i * n + j;
                let (c, _) = crate::grass::paint_big(x, z, t.ground[k], t.h[k], t.size * 0.5);
                let c = c * shade[k];
                let s = Srgba::from(LinearRgba::new(c.red, c.green, c.blue, 1.0)).to_u8_array();
                texel.copy_from_slice(&s);
            }
        }
    });
    let mut image = Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    crate::buildings::mip_chain(&mut image);
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slope(n: usize) -> Terrain {
        let size = (n - 1) as f32 * 2.0;
        let h = (0..n * n).map(|k| (k / n) as f32 * 0.25).collect();
        Terrain { n, size, h, road_d: vec![], ground: vec![] }
    }

    #[test]
    fn a_patch_keeps_its_cells_apart_and_small_meshes_small() {
        let t = slope(129);
        let mut one = Patch::default();
        one.add_cell(&t, 0, 0, 4, 0.0);
        let (verts, indices) = (one.pos.len() as u32, one.idx.len());
        let mut two = Patch::default();
        two.add_cell(&t, 0, 0, 4, 0.0);
        two.add_cell(&t, TERRAIN_TILE, 0, 4, 0.0);
        assert_eq!(two.pos.len() as u32, verts * 2);
        assert_eq!(two.idx.len(), indices * 2);
        // The second cell's triangles use the second cell's vertices only.
        assert!(two.idx[indices..].iter().all(|&i| (verts..2 * verts).contains(&i)));
        let (mesh, _) = two.into_mesh();
        assert!(matches!(mesh.indices(), Some(Indices::U16(_))));
        assert_eq!(mesh.get_vertex_size(), 20, "normals and UVs should be packed");
    }

    /// The far ground has to stay under the LODs drawn over it, wherever they share a sample.
    #[test]
    fn the_far_ground_lies_under_the_near_ground() {
        let t = slope(129);
        let (mut near, mut far) = (Patch::default(), Patch::default());
        near.add_cell(&t, 0, 0, 4, 0.0);
        far.add_cell(&t, 0, 0, 4, FAR_SINK);
        for (a, b) in near.pos.iter().zip(&far.pos) {
            assert!((a[1] - b[1] - FAR_SINK).abs() < 1e-5);
        }
    }

    #[test]
    fn a_cluster_stays_within_short_indices_or_goes_long() {
        // Every cell of a cluster at the last LOD, the most a far mesh can hold.
        let t = slope((CLUSTER * TERRAIN_TILE) + 1);
        let mut far = Patch::default();
        for ci in 0..CLUSTER {
            for cj in 0..CLUSTER {
                far.add_cell(&t, ci * TERRAIN_TILE, cj * TERRAIN_TILE, TERRAIN_LODS[2].0, FAR_SINK);
            }
        }
        let vertices = far.pos.len();
        let (mesh, _) = far.into_mesh();
        assert_eq!(mesh.count_vertices(), vertices);
        let short = matches!(mesh.indices(), Some(Indices::U16(_)));
        assert_eq!(short, vertices <= u16::MAX as usize);
    }
}
