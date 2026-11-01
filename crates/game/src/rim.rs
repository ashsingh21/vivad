//! Rim light on the characters: their glTF materials swapped, once each, for the same material
//! extended with a silhouette glow (assets/shaders/rim.wesl), so players read against busy, colourful
//! streets. A few shader instructions on the character pixels only: on at every quality level.
//!
//! Some materials get more, by their glTF name (tools/blender/characters/finish.py): skin a fainter
//! glow, the cornea none, and the heads their 4K colour and normal (assets/textures/characters) on
//! the Ultra level once those are loaded.

use std::collections::HashMap;

use bevy::gltf::GltfMaterialName;
use bevy::image::{ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct RimPlugin;

impl Plugin for RimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<RimMaterial>::default()).init_resource::<Converted>().add_systems(PostUpdate, (convert, sync_heads).chain());
    }
}

pub type RimMaterial = ExtendedMaterial<StandardMaterial, Rim>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct Rim {
    /// rgb: colour (linear), a: strength.
    #[uniform(100)]
    pub rim: Vec4,
}

impl MaterialExtension for Rim {
    fn fragment_shader() -> ShaderRef {
        "shaders/rim.wesl".into()
    }
}

/// Put on an entity (a character's scene root's descendants) to have its material rimmed.
#[derive(Component)]
pub struct RimTarget;

/// The rimmed copy of each material, made once, and the heads among them.
#[derive(Resource, Default)]
struct Converted {
    rimmed: HashMap<AssetId<StandardMaterial>, Handle<RimMaterial>>,
    heads: Vec<Head>,
}

/// A head material and the two sizes of its colour and normal maps.
struct Head {
    material: Handle<RimMaterial>,
    /// "textures/characters/<variant>_head_4k", or None if not on disk.
    large_stem: Option<String>,
    small: [Option<Handle<Image>>; 2],
    large: Option<[Handle<Image>; 2]>,
    showing_large: bool,
}

/// A pale sky-warm white: reads as light, not as a coloured outline.
const RIM: Vec4 = Vec4::new(1.0, 0.95, 0.88, 0.22);
const SKIN_RIM: f32 = 0.08;

fn large_stem(head_material: &str) -> Option<String> {
    let variant = head_material.strip_suffix("_head")?;
    let stem = format!("textures/characters/{variant}_head_4k");
    let on_disk = cfg!(not(target_arch = "wasm32")) && std::path::Path::new(crate::ASSET_DIR).join(format!("{stem}_basecolor.ktx2")).exists();
    on_disk.then_some(stem)
}

/// The material with what its glTF name asks for, and whether it is a head.
fn styled(base: &StandardMaterial, name: &str) -> (RimMaterial, bool) {
    let base = base.clone();
    let mut rim = RIM;
    if name.ends_with("_cornea") {
        // Plain alpha blending keeps the sky's reflection, which the lids don't shade, to a trace;
        // clearcoat or premultiplied alpha grey the whole eye out with it.
        rim = Vec4::ZERO;
    }
    if name.ends_with("_head") || name.ends_with("_body") {
        // Up close the full glow outlines the nose, lips and jaw. Same on both: they meet on the jaw.
        rim.w = SKIN_RIM;
    }
    (ExtendedMaterial { base, extension: Rim { rim } }, name.ends_with("_head"))
}

fn convert(
    mut commands: Commands,
    q: Query<(Entity, &MeshMaterial3d<StandardMaterial>, Option<&GltfMaterialName>), With<RimTarget>>,
    standard: Res<Assets<StandardMaterial>>,
    mut rimmed: ResMut<Assets<RimMaterial>>,
    mut done: ResMut<Converted>,
) {
    let done = &mut *done;
    for (e, m, name) in &q {
        let Some(base) = standard.get(&m.0) else { continue };
        let handle = done
            .rimmed
            .entry(m.0.id())
            .or_insert_with(|| {
                let name = name.map_or("", |n| n.0.as_str());
                let (material, is_head) = styled(base, name);
                let handle = rimmed.add(material);
                if is_head {
                    let small = [base.base_color_texture.clone(), base.normal_map_texture.clone()];
                    done.heads.push(Head { material: handle.clone(), large_stem: large_stem(name), small, large: None, showing_large: false });
                }
                handle
            })
            .clone();
        commands.entity(e).remove::<(MeshMaterial3d<StandardMaterial>, RimTarget)>().insert(MeshMaterial3d(handle));
    }
}

fn head_texture(assets: &AssetServer, path: String, srgb: bool) -> Handle<Image> {
    assets
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 8,
                ..default()
            });
        })
        .load(path)
}

/// Give the heads their large maps on Ultra, once loaded, and the file's own otherwise.
fn sync_heads(quality: Res<crate::quality::Quality>, assets: Res<AssetServer>, mut rimmed: ResMut<Assets<RimMaterial>>, mut done: ResMut<Converted>) {
    for head in &mut done.heads {
        let Some(stem) = &head.large_stem else { continue };
        if quality.is_ultra() && head.large.is_none() {
            head.large = Some([head_texture(&assets, format!("{stem}_basecolor.ktx2"), true), head_texture(&assets, format!("{stem}_normal.ktx2"), false)]);
        }
        let ready = head.large.as_ref().is_some_and(|l| l.iter().all(|h| assets.is_loaded_with_dependencies(h)));
        let large = quality.is_ultra() && ready;
        if large == head.showing_large {
            continue;
        }
        let Some(mut material) = rimmed.get_mut(&head.material) else { continue };
        let [color, normal] = head.large.clone().filter(|_| large).map_or(head.small.clone(), |l| l.map(Some));
        material.base.base_color_texture = color;
        material.base.normal_map_texture = normal;
        head.showing_large = large;
        info!("head textures: {}", if large { "4K" } else { "from the model" });
    }
}
