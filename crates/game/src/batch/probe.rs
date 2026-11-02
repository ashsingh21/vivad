//! What the GPU is asked to draw, counted, for sizing a map's LODs. Nothing here runs unless asked.
//!
//! VIVAD_DRAWS=1          every 2 s, log the main view's batches (one per mesh and material drawn)
//!                        and triangles, by kind of thing, and the sun's shadow cascades' total.
//! VIVAD_VIEWS=<list>     hold the camera at each of these views in turn, then exit. The list is
//!                        `name:x,y,z,lookx,looky,lookz` joined with `;`. Each view is held for
//!                        VIVAD_VIEW_FRAMES frames (default 200) and reports the last frame, with
//!                        the frame time over its last 100 frames.
//! VIVAD_VIEW_DIR=<dir>   also screenshot each view there.
//! VIVAD_MEM=1            every 5 s, log what is held: meshes and images still in main memory (a
//!                        mesh that is only on the GPU counts nothing there) and the GPU's mesh
//!                        buffers and textures. With VIVAD_DRAWS=1 the meshes stay readable, which
//!                        costs memory but gives each kind of mesh's real size.
//! VIVAD_RELOAD=<map>@<s>,… swap the map at those seconds of play, the way a new match does.
//!
//! The counts are of what is queued: anything inside the camera's frustum and its visibility
//! range. The depth prepass draws the same batches again, so the real draw count is about double
//! the main view's figure, plus the shadows'.

use std::any::TypeId;
use std::collections::{BTreeMap, HashMap, HashSet};

use bevy::camera::visibility::{CascadesVisibleEntities, VisibleEntities};
use bevy::prelude::*;
use bevy::render::mesh::allocator::MeshAllocator;
use bevy::render::render_asset::RenderAssets;
use bevy::render::texture::GpuImage;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::render::{Render, RenderApp, RenderSystems};
use shared::map::MapId;

use crate::camera::{CameraSystems, MainCamera};

/// Whether a report that reads meshes back is on, so meshes must stay readable in the main world.
pub fn reading_meshes() -> bool {
    std::env::var("VIVAD_DRAWS").is_ok() || std::env::var("VIVAD_VIEWS").is_ok() || std::env::var("VIVAD_TRIS").is_ok()
}

/// What a mesh is, for the report: the tier and family a merged mesh belongs to.
#[derive(Component, Clone, Copy)]
pub struct Tagged(pub &'static str);

/// The views still to hold, and how far into the first.
#[derive(Resource)]
struct Views {
    list: Vec<(String, Vec3, Vec3)>,
    index: usize,
    frames: u32,
    hold: u32,
    dir: Option<String>,
    /// When this view's timing window opened.
    timed_from: f64,
}

pub struct ProbePlugin;

impl Plugin for ProbePlugin {
    fn build(&self, app: &mut App) {
        if let Ok(spec) = std::env::var("VIVAD_VIEWS") {
            let list = spec
                .split(';')
                .filter_map(|v| {
                    let (name, nums) = v.split_once(':')?;
                    let n: Vec<f32> = nums.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    (n.len() == 6).then(|| (name.trim().to_owned(), Vec3::new(n[0], n[1], n[2]), Vec3::new(n[3], n[4], n[5])))
                })
                .collect();
            let hold = std::env::var("VIVAD_VIEW_FRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
            let dir = std::env::var("VIVAD_VIEW_DIR").ok();
            if let Some(d) = &dir {
                std::fs::create_dir_all(d).ok();
            }
            app.insert_resource(Views { list, index: 0, frames: 0, hold, dir, timed_from: 0.0 })
                .add_systems(PostUpdate, hold_view.after(CameraSystems).before(crate::sky::SkyFollow).before(TransformSystems::Propagate));
        }
        if std::env::var("VIVAD_DRAWS").is_ok() {
            app.add_systems(Last, timed_report);
        }
        if let Ok(spec) = std::env::var("VIVAD_RELOAD") {
            let swaps = spec.split(',').filter_map(|s| s.split_once('@')).filter_map(|(map, at)| Some((at.trim().parse().ok()?, MapId::parse(map)?))).collect();
            app.insert_resource(Reloads(swaps)).add_systems(Update, reload);
        }
        if std::env::var("VIVAD_MEM").is_ok() {
            app.add_systems(Last, (main_memory, footprint_steps));
            if let Some(render) = app.get_sub_app_mut(RenderApp) {
                render.add_systems(Render, gpu_memory.in_set(RenderSystems::Cleanup));
            }
        }
    }
}

/// VIVAD_RELOAD: when to swap the map, and for which.
#[derive(Resource)]
struct Reloads(Vec<(f32, MapId)>);

fn reload(world: &mut World) {
    let now = world.resource::<Time<Real>>().elapsed_secs();
    let due = world.resource::<Reloads>().0.iter().position(|(at, _)| now >= *at);
    if let Some(k) = due {
        let (_, map) = world.resource_mut::<Reloads>().0.remove(k);
        info!("swapping the map for {map:?}");
        shared::sim::load_map(world, map);
    }
}

const MB: f64 = 1.0e6;

/// A mesh's bytes still in main memory, and its vertex count.
fn held(m: &Mesh) -> (usize, usize) {
    let Ok(attributes) = m.try_attributes() else { return (0, 0) };
    let vertices: usize = attributes.map(|(_, data)| data.get_bytes().len()).sum();
    let indices = m.try_indices_option().ok().flatten().map_or(0, |i| match i {
        bevy::mesh::Indices::U16(v) => v.len() * 2,
        bevy::mesh::Indices::U32(v) => v.len() * 4,
    });
    (vertices + indices, m.count_vertices())
}

/// VIVAD_MEM: main memory's meshes by kind, and its images, the biggest by name.
fn main_memory(
    time: Res<Time<Real>>,
    mut next: Local<f32>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    assets: Res<AssetServer>,
    q: Query<(&Mesh3d, Option<&Tagged>)>,
) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 5.0;
    let tags: HashMap<AssetId<Mesh>, &'static str> = q.iter().filter_map(|(m, tag)| Some((m.0.id(), tag?.0))).collect();
    let mut by: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for (id, mesh) in meshes.iter() {
        let (bytes, vertices) = held(mesh);
        // Untagged: the file it came from.
        let name = tags.get(&id).map_or_else(|| assets.get_path(id).map_or("from code".to_owned(), |p| p.path().display().to_string()), |t| t.to_string());
        let row = by.entry(name).or_default();
        row.0 += 1;
        row.1 += bytes;
        row.2 += vertices;
    }
    let mut by: Vec<_> = by.into_iter().collect();
    by.sort_by_key(|(_, v)| std::cmp::Reverse(v.1));
    let rows: Vec<String> = by.iter().take(24).map(|(k, v)| format!("{k}: {} meshes, {:.1} MB, {:.2}M vertices", v.0, v.1 as f64 / MB, v.2 as f32 / 1.0e6)).collect();
    println!("[mem] main memory meshes {:.0} MB [{}]", by.iter().map(|(_, v)| v.1).sum::<usize>() as f64 / MB, rows.join("; "));
    let mut held: Vec<(usize, String)> = images
        .iter()
        .filter_map(|(id, img)| {
            let bytes = img.data.as_ref()?.len();
            Some((bytes, assets.get_path(id).map_or(format!("{}x{} from code", img.width(), img.height()), |p| p.to_string())))
        })
        .collect();
    held.sort_by_key(|h| std::cmp::Reverse(h.0));
    let total: usize = held.iter().map(|h| h.0).sum();
    let top: Vec<String> = held.iter().take(12).map(|(b, name)| format!("{name} {:.0}", *b as f64 / MB)).collect();
    println!("[mem] main memory images {:.0} MB in {} [{}]", total as f64 / MB, held.len(), top.join(", "));
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// libproc's: fills `buffer` with a `rusage_info_v4` for the process.
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
}

/// The process's memory footprint now and its highest so far, as the system counts them, in bytes.
#[cfg(target_os = "macos")]
fn footprint() -> (u64, u64) {
    // `rusage_info_v4`, all 64-bit fields: these two are its 10th and 31st.
    const NOW: usize = 9;
    const MOST: usize = 30;
    let mut info = [0u64; 64];
    // SAFETY: `info` is larger than the struct asked for (flavor 4), and is only written to.
    let failed = unsafe { proc_pid_rusage(std::process::id() as i32, 4, info.as_mut_ptr().cast()) } != 0;
    if failed { (0, 0) } else { (info[NOW], info[MOST]) }
}

#[cfg(not(target_os = "macos"))]
fn footprint() -> (u64, u64) {
    (0, 0)
}

/// VIVAD_MEM: a line each time the footprint moves by a hundred megabytes, with the frame.
fn footprint_steps(mut frame: Local<u32>, mut last: Local<u64>, time: Res<Time<Real>>) {
    *frame += 1;
    let (now, most) = footprint();
    if now.abs_diff(*last) as f64 > 100.0 * MB {
        *last = now;
        println!("[mem] frame {} at {:.2}s: footprint {:.0} MB, highest {:.0} MB", *frame, time.elapsed_secs(), now as f64 / MB, most as f64 / MB);
    }
}

/// VIVAD_MEM: the GPU's mesh buffers and textures.
fn gpu_memory(mut last: Local<Option<bevy::platform::time::Instant>>, allocator: Option<Res<MeshAllocator>>, textures: Option<Res<RenderAssets<GpuImage>>>) {
    let (Some(allocator), Some(textures)) = (allocator, textures) else { return };
    if last.is_some_and(|t| t.elapsed().as_secs() < 5) {
        return;
    }
    *last = Some(bevy::platform::time::Instant::now());
    let mut sizes: BTreeMap<(u32, u32, u32, String), (usize, u64)> = BTreeMap::new();
    for (_, image) in textures.iter() {
        let d = &image.texture_descriptor;
        let Some(block) = d.format.block_copy_size(None) else { continue };
        let (w, h) = d.format.block_dimensions();
        let bytes: u64 = (0..d.mip_level_count).map(|m| ((d.size.width >> m).max(1).div_ceil(w) * (d.size.height >> m).max(1).div_ceil(h) * block * d.size.depth_or_array_layers) as u64).sum();
        let row = sizes.entry((d.size.width, d.size.height, d.mip_level_count, format!("{:?}", d.format))).or_default();
        row.0 += 1;
        row.1 += bytes;
    }
    let total: u64 = sizes.values().map(|s| s.1).sum();
    let mut rows: Vec<_> = sizes.iter().collect();
    rows.sort_by_key(|(_, s)| std::cmp::Reverse(s.1));
    let top: Vec<String> = rows.iter().take(12).map(|((w, h, mips, format), (n, bytes))| format!("{n} x {w}x{h} {format} {mips} mips: {:.0} MB", *bytes as f64 / MB)).collect();
    println!("[mem] GPU meshes {:.0} MB in {} slabs, textures {:.0} MB in {} [{}]", allocator.slabs_size() as f64 / MB, allocator.slab_count(), total as f64 / MB, textures.iter().count(), top.join("; "));
}

/// What the reports read of each thing drawn.
type Drawn<'w, 's> = Query<'w, 's, (&'static Mesh3d, Option<&'static MeshMaterial3d<StandardMaterial>>, Option<&'static Tagged>)>;

/// One mesh's triangles, if its data is still in the main world.
fn tris(m: &Mesh) -> usize {
    match m.try_indices_option() {
        Ok(Some(i)) => i.len() / 3,
        Ok(None) => m.count_vertices() / 3,
        Err(_) => 0,
    }
}

/// Batches and triangles among some entities, split by tag.
fn tally<'a>(entities: impl Iterator<Item = &'a Entity>, q: &Drawn, meshes: &Assets<Mesh>) -> BTreeMap<&'static str, (usize, usize)> {
    let mut seen: HashSet<(AssetId<Mesh>, Option<AssetId<StandardMaterial>>)> = HashSet::new();
    let mut by: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    for e in entities {
        let Ok((mesh, material, tag)) = q.get(*e) else { continue };
        let row = by.entry(tag.map_or("other", |t| t.0)).or_default();
        if seen.insert((mesh.0.id(), material.map(|m| m.0.id()))) {
            row.0 += 1;
        }
        row.1 += meshes.get(&mesh.0).map_or(0, tris);
    }
    by
}

fn line(by: &BTreeMap<&'static str, (usize, usize)>) -> String {
    let (b, t) = by.values().fold((0, 0), |(a, c), v| (a + v.0, c + v.1));
    let rows: Vec<String> = by.iter().map(|(k, v)| format!("{k} {}/{}k", v.0, v.1 / 1000)).collect();
    format!("{b} batches, {:.2}M tris [{}]", t as f32 / 1.0e6, rows.join(", "))
}

fn report(
    label: &str,
    cam: &Query<(&GlobalTransform, &VisibleEntities), With<MainCamera>>,
    lights: &Query<&CascadesVisibleEntities>,
    q: &Drawn,
    meshes: &Assets<Mesh>,
) {
    let Ok((tf, seen)) = cam.single() else { return };
    let kind = TypeId::of::<Mesh3d>();
    let main = tally(seen.iter(kind), q, meshes);
    let mut shadow: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    for l in lights {
        for cascades in l.entities.values() {
            for c in cascades {
                for (k, v) in tally(c.entities.iter(), q, meshes) {
                    let row = shadow.entry(k).or_default();
                    row.0 += v.0;
                    row.1 += v.1;
                }
            }
        }
    }
    let p = tf.translation();
    println!("[draws] {label} at ({:.0},{:.0},{:.0}) main: {}", p.x, p.y, p.z, line(&main));
    // What the tags don't name, by mesh size.
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for e in seen.iter(kind) {
        if let Ok((mesh, _, None)) = q.get(*e) {
            *sizes.entry(meshes.get(&mesh.0).map_or(0, tris)).or_default() += 1;
        }
    }
    let mut sizes: Vec<(usize, usize)> = sizes.into_iter().collect();
    sizes.sort_by_key(|(t, n)| std::cmp::Reverse(t * n));
    println!("[draws] {label} untagged: {}", sizes.iter().take(14).map(|(t, n)| format!("{n} x {t}")).collect::<Vec<_>>().join(", "));
    println!("[draws] {label} shadows: {}", line(&shadow));
}

fn timed_report(
    time: Res<Time<Real>>,
    mut next: Local<f32>,
    cam: Query<(&GlobalTransform, &VisibleEntities), With<MainCamera>>,
    lights: Query<&CascadesVisibleEntities>,
    q: Drawn,
    meshes: Res<Assets<Mesh>>,
) {
    if time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 2.0;
    report("now", &cam, &lights, &q, &meshes);
}

#[allow(clippy::too_many_arguments)]
fn hold_view(
    mut commands: Commands,
    mut views: ResMut<Views>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
    seen: Query<(&GlobalTransform, &VisibleEntities), With<MainCamera>>,
    lights: Query<&CascadesVisibleEntities>,
    q: Drawn,
    meshes: Res<Assets<Mesh>>,
    time: Res<Time<Real>>,
    image: Res<crate::quality::View3d>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(mut ct) = camera.single_mut() else { return };
    let Some((name, pos, look)) = views.list.get(views.index).cloned() else {
        views.frames += 1;
        if views.frames > 10 {
            exit.write(AppExit::Success);
        }
        return;
    };
    *ct = Transform::from_translation(pos).looking_at(look, Vec3::Y);
    views.frames += 1;
    let hold = views.hold;
    if views.frames + 100 == hold {
        views.timed_from = time.elapsed_secs_f64();
    }
    if views.frames == hold {
        let ms = (time.elapsed_secs_f64() - views.timed_from) * 1000.0 / 100.0;
        report(&name, &seen, &lights, &q, &meshes);
        println!("[draws] {name} frame {ms:.2} ms");
        if let Some(d) = &views.dir {
            // The 3D view's own image: it needs no window on screen (and has no HUD).
            commands.spawn(Screenshot::image(image.image.clone())).observe(save_to_disk(format!("{d}/{name}.png")));
        }
    }
    if views.frames >= hold + 5 {
        views.frames = 0;
        views.index += 1;
    }
}
