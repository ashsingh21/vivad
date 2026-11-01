//! Ground detail: the terrain's standard material extended (assets/shaders/ground.wesl) with one
//! small tiling texture of asphalt, paving slabs and trodden earth (tools/textures/city.py), mixed
//! per vertex by what covers the ground there. It darkens joints and cracks and bends the light off
//! them, so a lane reads as worn stone and a road as tarmac instead of flat paint.
//!
//! Mobile cost: one texture sample (512 px, ~1.4 MB with mips) and a few instructions per ground
//! pixel; no extra draws. On at every quality level.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default()).add_systems(Update, mip_detail);
    }
}

/// The detail texture, once loaded, given its mip chain and a repeating sampler.
fn mip_detail(mut events: MessageReader<AssetEvent<Image>>, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    let ids: Vec<AssetId<Image>> = events
        .read()
        .filter_map(|ev| match ev {
            AssetEvent::Added { id } => Some(*id),
            _ => None,
        })
        .filter(|id| assets.get_path(*id).is_some_and(|p| p.path() == std::path::Path::new(DETAIL)))
        .collect();
    for id in ids {
        if let Some(mut img) = images.get_mut(id) {
            crate::buildings::mip_chain(&mut img);
        }
    }
}

pub type GroundMaterial = ExtendedMaterial<StandardMaterial, GroundDetail>;

/// Where the texture is.
pub const DETAIL: &str = "textures/panga/ground_detail.png";

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GroundDetail {
    #[texture(100)]
    #[sampler(101)]
    pub texture: Handle<Image>,
    /// x: repeats per metre, y: how far it shades the colour, z: how far it bends the light.
    #[uniform(102)]
    pub params: Vec4,
}

impl MaterialExtension for GroundDetail {
    fn fragment_shader() -> ShaderRef {
        "shaders/ground.wesl".into()
    }
}

impl GroundDetail {
    pub fn new(texture: Handle<Image>) -> Self {
        Self { texture, params: Vec4::new(0.25, 0.55, 0.06, 0.0) }
    }
}

/// The detail kind's weights a vertex carries in its second UV: (asphalt, slabs); the rest is earth.
pub fn weights(class: Option<u8>) -> [f32; 2] {
    use shared::map::terrain::ground;
    match class {
        Some(ground::ROAD | ground::PARKING) => [1.0, 0.0],
        Some(ground::PARK | ground::WATER | ground::PATH) | None => [0.0, 0.0],
        // Pavement and plazas.
        Some(_) => [0.0, 1.0],
    }
}
