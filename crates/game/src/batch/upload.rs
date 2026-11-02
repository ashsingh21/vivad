//! Getting a map onto the GPU without a spike of memory, and keeping the GPU's queue short.
//!
//! A big map's meshes and textures all arrive in its first frames, more than a gigabyte. Sent in
//! one go, each would stand in main memory, in a staging copy and in its place on the GPU all at
//! once, which is where the memory peak came from. So they are held back and sent a slice a frame
//! (`send`), which takes under a second, and no vertex buffer grows past `SLAB`, because growing one
//! holds its old and new copies at the same time. (Bevy's own `RenderAssetBytesPerFrame` won't do:
//! it writes a mesh to the GPU at once and then holds its main-memory copy until the limit allows,
//! so with textures using up the limit every mesh stands twice.)
//!
//! Presenting to the window is what normally holds the game to the GPU's pace. A window that is
//! covered, on another Space or on a sleeping display presents nothing, so `keep_up` does that job:
//! without it frames pile up behind the GPU, each holding its command buffers, for gigabytes of
//! memory and seconds of lag.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::mesh::allocator::MeshAllocatorSettings;

const MIB: usize = 1024 * 1024;
/// How much goes to the GPU in a frame: a second's worth at 60 fps is more than a whole map.
const PER_FRAME: usize = 48 * MIB;
/// The most a vertex or index buffer holds: past this a new one starts, and nothing is copied.
const SLAB: u64 = 128 * MIB as u64;

pub struct UploadPlugin;

impl Plugin for UploadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Held>().add_systems(Last, send.after(crate::buildings::gpu_only_images).after(crate::buildings::gpu_only_meshes));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            let mut settings = render.world_mut().resource_mut::<MeshAllocatorSettings>();
            settings.max_slab_size = SLAB;
            settings.large_threshold = SLAB;
            // The browser has no way to wait, and always presents.
            #[cfg(not(target_arch = "wasm32"))]
            render.init_resource::<queue::InFlight>().add_systems(bevy::render::Render, queue::keep_up.in_set(bevy::render::RenderSystems::Cleanup));
        }
    }
}

/// What is waiting to go to the GPU, oldest first, with its size.
#[derive(Resource, Default)]
struct Held {
    meshes: VecDeque<(AssetId<Mesh>, usize)>,
    images: VecDeque<(AssetId<Image>, usize)>,
}

/// Lets go of a queue's oldest entries, up to `left` bytes, always moving on by at least one.
fn release<A: Asset>(queue: &mut VecDeque<(AssetId<A>, usize)>, left: &mut usize, mut go: impl FnMut(AssetId<A>)) {
    while let Some(&(id, bytes)) = queue.front() {
        if bytes > *left && *left < PER_FRAME {
            break;
        }
        queue.pop_front();
        *left = left.saturating_sub(bytes);
        go(id);
    }
}

/// Runs after the frame's asset events and before they reach the renderer. Something only the GPU
/// will keep is, for now, kept by main memory alone, which the renderer takes no notice of; when its
/// turn comes it is changed back, and the renderer takes it. Meshes first: a texture is no use
/// without one. What main memory keeps is left alone: it is small, and something reads it.
fn send(
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut held: ResMut<Held>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
) {
    let held = &mut *held;
    let mut left = PER_FRAME;
    release(&mut held.meshes, &mut left, |id| {
        if let Some(mut mesh) = meshes.get_mut(id) {
            mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
    });
    release(&mut held.images, &mut left, |id| {
        if let Some(mut image) = images.get_mut(id) {
            image.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
    });
    for ev in mesh_events.read() {
        let AssetEvent::Added { id } = ev else { continue };
        let Some(mesh) = meshes.get_mut_untracked(*id) else { continue };
        if mesh.asset_usage == RenderAssetUsages::RENDER_WORLD {
            mesh.asset_usage = RenderAssetUsages::MAIN_WORLD;
            held.meshes.push_back((*id, mesh.get_vertex_buffer_size() + mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len)));
        }
    }
    for ev in image_events.read() {
        let AssetEvent::Added { id } = ev else { continue };
        let Some(image) = images.get_mut_untracked(*id) else { continue };
        // A render target has no data to send.
        if image.asset_usage == RenderAssetUsages::RENDER_WORLD
            && let Some(bytes) = image.data.as_ref().map(Vec::len)
        {
            image.asset_usage = RenderAssetUsages::MAIN_WORLD;
            held.images.push_back((*id, bytes));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod queue {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use bevy::prelude::*;
    use bevy::render::render_resource::PollType;
    use bevy::render::renderer::{RenderDevice, RenderQueue};

    /// How many frames the GPU may be behind. Presenting allows three drawables: at or past that,
    /// a window on screen has already stopped the game, and this never waits.
    const MOST_BEHIND: u64 = 3;
    /// The longest one frame waits, in case the GPU has stopped altogether.
    const GIVE_UP: Duration = Duration::from_secs(1);

    /// Frames handed to the GPU, and how many of them it has finished.
    #[derive(Resource, Default)]
    pub struct InFlight {
        sent: u64,
        done: Arc<AtomicU64>,
    }

    /// Runs after the frame is submitted.
    pub fn keep_up(mut frames: ResMut<InFlight>, device: Option<Res<RenderDevice>>, queue: Option<Res<RenderQueue>>) {
        let (Some(device), Some(queue)) = (device, queue) else { return };
        frames.sent += 1;
        let done = frames.done.clone();
        queue.on_submitted_work_done(move || {
            done.fetch_add(1, Ordering::Relaxed);
        });
        let started = Instant::now();
        while frames.sent.saturating_sub(frames.done.load(Ordering::Relaxed)) > MOST_BEHIND && started.elapsed() < GIVE_UP {
            let _ = device.poll(PollType::Poll);
            std::thread::sleep(Duration::from_micros(200));
        }
    }
}
