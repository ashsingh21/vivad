//! The real surfaces: plaster, marble, asphalt, pavers, earth and glass in place of the painted
//! ones, from Medium up (`Quality::surfaces`), with the facades' parallax on Ultra alone. The level
//! can change in play (F4, or the start-up choice a frame in), so each material is changed where it
//! stands: its handle stays on the meshes, and the textures are only loaded the first time a level
//! wants them, and only shown once they are all in.
//!
//! The textures are in assets/textures/ultra, built by tools/textures/cp_surfaces.py and
//! tools/texpack (`just textures-pack`) from the CC0 scans listed in sources.json there.
//!
//! Switches, for comparisons:
//!   VIVAD_NO_SURFACES=1  the painted surfaces on every level.
//!   VIVAD_NO_PARALLAX=1  no depth in the facades' windows, even on Ultra (the costly part).
//!
//! Reflections come from the environment map, not screen-space reflections, and that is a finding
//! rather than a shortcut. In Bevy 0.20's sources SSR runs after the deferred lighting pass and
//! before `main_opaque_pass_3d`, and on a hit it reads the colour image as it stands then. Forward
//! materials are in the depth prepass but not yet in that image, so a deferred puddle or pane in
//! this game's forward world would march into the buildings' depth and read back black. SSR would
//! need every opaque material deferred, the grass and the rimmed characters included, and alpha
//! blended water can't be deferred at all. So there is no SSR camera component here and no
//! VIVAD_NO_SSR to honour; glass and water are forward, reflect the sky the Ultra level generates
//! (crate::ultra), and don't reflect the city.

use bevy::asset::LoadState;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, ParallaxMappingMethod};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use shared::map::MapLayout;
use shared::map::terrain::{Terrain, ground};
use shared::sim::MapTerrain;

use crate::buildings::TILE;
use crate::quality::Quality;

/// Metres a tile covers (tools/textures/cp_surfaces.py): `plaster` holds a wall's whole height,
/// so one repeat runs from the street to above the ring's parapets.
const PLASTER_TILE: f32 = 12.0;
const MARBLE_TILE: f32 = 4.8;
/// What a facade tile's width is, for the parallax depth: `depth = scale * width`. The depth maps
/// run to 16 cm (cp_surfaces.py DEPTH_RANGE).
const FACADE_WIDTH: f32 = 2.0 * crate::buildings::BAY;
const FACADE_DEPTH: f32 = 0.16;
/// Parallax steps at a glancing look; Bevy takes fewer face on. 8 rather than its 16: the wall
/// recesses are shallow, and this is a per-pixel loop run twice (colour and the normal prepass).
const PARALLAX_LAYERS: f32 = 8.0;
/// Metres per repeat of the Ultra ripple map (cp_surfaces.py water).
pub const WATER_TILE: f32 = 16.0;

pub struct SurfacesPlugin;

impl Plugin for SurfacesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default())
            .init_resource::<UltraSurfaces>()
            .add_systems(Update, (sync, sync_ground).chain());
    }
}

/// How a material changes for the real surfaces.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Surface {
    /// White plaster on a world-mapped block, colonnade or roof.
    Plaster,
    /// Polished floors.
    Marble,
    /// The upper storeys' generated facade.
    Facade,
    /// The ground storey's generated shopfronts.
    Shop,
    /// Plate glass: near mirror-smooth, and all its colour from what it reflects.
    Glass,
    /// Open water, its ripples in the water map.
    Water,
}

/// One material with a real surface, and its plain self while that is showing.
struct Entry {
    handle: Handle<StandardMaterial>,
    surface: Surface,
    plain: Option<StandardMaterial>,
}

struct Set {
    base: Handle<Image>,
    normal: Handle<Image>,
    orm: Handle<Image>,
    depth: Option<Handle<Image>>,
}

impl Set {
    fn images(&self) -> impl Iterator<Item = &Handle<Image>> {
        [&self.base, &self.normal, &self.orm].into_iter().chain(self.depth.as_ref())
    }

    /// Roughness and metalness come from the map, with the factors out of its way.
    fn dress(&self, m: &mut StandardMaterial) {
        m.base_color_texture = Some(self.base.clone());
        m.normal_map_texture = Some(self.normal.clone());
        m.metallic_roughness_texture = Some(self.orm.clone());
        m.occlusion_texture = Some(self.orm.clone());
        m.perceptual_roughness = 1.0;
        m.metallic = 1.0;
        m.reflectance = 0.5;
    }
}

struct Textures {
    plaster: Set,
    marble: Set,
    facade: Set,
    shop: Set,
    asphalt: Set,
    pavers: Set,
    soil: Set,
    water: Handle<Image>,
    /// Carries the sampler the ground's detail maps share (`GroundSurfaces`).
    white: Handle<Image>,
}

impl Textures {
    fn images(&self) -> impl Iterator<Item = &Handle<Image>> {
        [&self.plaster, &self.marble, &self.facade, &self.shop, &self.asphalt, &self.pavers, &self.soil].into_iter().flat_map(Set::images).chain([&self.water])
    }

    fn load(assets: &AssetServer, images: &mut Assets<Image>) -> Self {
        let set = |name: &str, depth: bool| Set {
            base: tiled(assets, format!("{name}_basecolor"), true),
            normal: tiled(assets, format!("{name}_normal"), false),
            orm: tiled(assets, format!("{name}_orm"), false),
            depth: depth.then(|| tiled(assets, format!("{name}_depth"), false)),
        };
        let mut white = Image::new_fill(
            Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            TextureDimension::D2,
            &[255; 4],
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        white.sampler = repeating();
        Textures {
            plaster: set("plaster", false),
            marble: set("marble", false),
            facade: set("facade", true),
            shop: set("shop", true),
            asphalt: set("asphalt", false),
            pavers: set("pavers", false),
            soil: set("soil", false),
            water: tiled(assets, "water_normal".into(), false),
            white: images.add(white),
        }
    }
}

fn repeating() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    })
}

/// A tiling KTX2 map, mips included: nothing to do on the CPU once it's here.
fn tiled(assets: &AssetServer, name: String, srgb: bool) -> Handle<Image> {
    assets
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = repeating();
        })
        .load(format!("textures/ultra/{name}.ktx2"))
}

/// What's registered, loaded and showing.
#[derive(Resource)]
pub struct UltraSurfaces {
    entries: Vec<Entry>,
    textures: Option<Textures>,
    failed: bool,
    on: bool,
    /// The ground's two materials, once built for this map (`sync_ground`).
    ground: Option<(Handle<StandardMaterial>, Handle<GroundMaterial>)>,
    ground_on: bool,
    /// Whether the switches allow any of this.
    allowed: bool,
    parallax: bool,
    /// Whether the facades are dressed with their depth (a level with `Quality::is_ultra`).
    relief: bool,
}

impl Default for UltraSurfaces {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            textures: None,
            failed: false,
            on: false,
            ground: None,
            ground_on: false,
            allowed: std::env::var("VIVAD_NO_SURFACES").is_err(),
            parallax: std::env::var("VIVAD_NO_PARALLAX").is_err(),
            relief: false,
        }
    }
}

impl UltraSurfaces {
    /// A new map is being built: its materials replace the last one's.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.ground = None;
    }

    /// Gives a material its real surface whenever a level shows those.
    pub fn add(&mut self, handle: Handle<StandardMaterial>, surface: Surface) {
        if !self.entries.iter().any(|e| e.handle == handle) {
            self.entries.push(Entry { handle, surface, plain: None });
        }
    }

    /// Whether the real surfaces are what's on screen.
    pub fn showing(&self) -> bool {
        self.on
    }

    fn dress(&self, surface: Surface, m: &mut StandardMaterial) {
        let Some(t) = &self.textures else { return };
        match surface {
            Surface::Plaster => {
                t.plaster.dress(m);
                m.uv_transform = Affine2::from_scale(Vec2::splat(TILE / PLASTER_TILE));
            }
            Surface::Marble => {
                t.marble.dress(m);
                m.uv_transform = Affine2::from_scale(Vec2::splat(TILE / MARBLE_TILE));
            }
            Surface::Facade | Surface::Shop => {
                let set = if surface == Surface::Facade { &t.facade } else { &t.shop };
                set.dress(m);
                m.depth_map = None;
                if self.relief {
                    m.depth_map = set.depth.clone();
                    m.parallax_depth_scale = FACADE_DEPTH / FACADE_WIDTH;
                    m.max_parallax_layer_count = PARALLAX_LAYERS;
                    m.parallax_mapping_method = ParallaxMappingMethod::Occlusion;
                }
            }
            Surface::Glass => {
                m.metallic_roughness_texture = None;
                m.metallic = 0.0;
                m.perceptual_roughness = 0.08;
                m.reflectance = 0.9;
            }
            Surface::Water => {
                m.normal_map_texture = Some(t.water.clone());
                m.perceptual_roughness = 0.08;
                m.reflectance = 0.6;
            }
        }
    }
}

/// Starts the textures loading the first time a level wants them, and flips the materials once
/// they have all arrived; a change of level dresses them again, for the parallax. If one can't be
/// read (a GPU without BC formats, say), the level keeps the plain surfaces and says so once.
fn sync(quality: Res<Quality>, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>, mut state: ResMut<UltraSurfaces>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let want = quality.surfaces() && state.allowed;
    if want && state.textures.is_none() {
        state.textures = Some(Textures::load(&assets, &mut images));
    }
    let ready = want
        && state.textures.as_ref().is_some_and(|t| {
            t.images().all(|h| matches!(assets.get_load_state(h), Some(LoadState::Loaded)) && images.contains(h))
        });
    if want && !ready {
        if !state.failed && state.textures.as_ref().is_some_and(|t| t.images().any(|h| matches!(assets.get_load_state(h), Some(LoadState::Failed(_))))) {
            warn!("surfaces: a texture in assets/textures/ultra would not load; the plain surfaces stay");
            state.failed = true;
        }
        return;
    }
    let state = &mut *state;
    state.on = want;
    let relief = quality.is_ultra() && state.parallax;
    let redress = relief != state.relief;
    state.relief = relief;
    let mut entries = std::mem::take(&mut state.entries);
    for e in &mut entries {
        let shown = e.plain.is_some();
        let walls = matches!(e.surface, Surface::Facade | Surface::Shop);
        if shown == want && !(want && redress && walls) {
            continue;
        }
        let Some(mut m) = materials.get_mut(&e.handle) else { continue };
        if want {
            if !shown {
                e.plain = Some(m.clone());
            }
            state.dress(e.surface, &mut m);
        } else if let Some(plain) = e.plain.take() {
            *m = plain;
        }
    }
    state.entries = entries;
}

/// The ground on a city map is one mesh under several surfaces, so its detail is chosen per pixel
/// by a mask of the map's ground classes: a custom fragment shader, and the one place here that
/// swaps a component rather than editing a material. The terrain's tiles are made by
/// `visuals::dress_terrain`.
pub type GroundMaterial = ExtendedMaterial<StandardMaterial, GroundSurfaces>;

/// All sampled with the base material's sampler, not samplers of their own: Metal allows 16 in a
/// shader stage, and Bevy's forward pipeline on Ultra is close to that.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct GroundSurfaces {
    /// World XZ to mask UV: uv = (xz + x) * y + z.
    #[uniform(100)]
    pub place: Vec4,
    /// Share of each surface, one texel per terrain sample. r: asphalt, g: pavers, b: earth.
    #[texture(101)]
    pub mask: Handle<Image>,
    #[texture(102)]
    pub asphalt_color: Handle<Image>,
    #[texture(103)]
    pub asphalt_normal: Handle<Image>,
    #[texture(104)]
    pub asphalt_orm: Handle<Image>,
    #[texture(105)]
    pub pavers_color: Handle<Image>,
    #[texture(106)]
    pub pavers_normal: Handle<Image>,
    #[texture(107)]
    pub pavers_orm: Handle<Image>,
    #[texture(108)]
    pub soil_color: Handle<Image>,
    #[texture(109)]
    pub soil_normal: Handle<Image>,
    #[texture(110)]
    pub soil_orm: Handle<Image>,
}

impl MaterialExtension for GroundSurfaces {
    fn fragment_shader() -> ShaderRef {
        "shaders/ground.wesl".into()
    }
}

/// The mask of which surface covers each terrain sample.
fn ground_mask(t: &Terrain) -> Image {
    let n = t.n;
    let mut data = vec![255u8; n * n * 4];
    for i in 0..n {
        for j in 0..n {
            let share = match t.ground.get(i * n + j).copied().unwrap_or(ground::PAVEMENT) {
                ground::ROAD | ground::PARKING => [255, 0, 0],
                ground::PARK | ground::PATH | ground::WATER | ground::OPEN | ground::SAND => [0, 0, 255],
                _ => [0, 255, 0],
            };
            data[(j * n + i) * 4..(j * n + i) * 4 + 3].copy_from_slice(&share);
        }
    }
    Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8Unorm, bevy::asset::RenderAssetUsages::RENDER_WORLD)
}

/// World XZ to the mask's UV: texel i's centre on terrain sample i.
fn mask_place(t: &Terrain) -> Vec4 {
    let n = t.n as f32;
    Vec4::new(t.size * 0.5, 1.0 / (t.step() * n), 0.5 / n, 0.0)
}

/// Moves the terrain's tiles to and from the ground material. It follows `sync`, so it changes
/// over in the same frame the buildings do, and catches a new map's tiles as they appear.
#[allow(clippy::too_many_arguments)]
fn sync_ground(
    mut commands: Commands,
    layout: Res<MapLayout>,
    mut state: ResMut<UltraSurfaces>,
    terrain: Query<Ref<Children>, With<MapTerrain>>,
    tiles: Query<&MeshMaterial3d<StandardMaterial>>,
    standard: Res<Assets<StandardMaterial>>,
    mut grounds: ResMut<Assets<GroundMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let want = state.on && !layout.terrain.ground.is_empty();
    let flipped = want != state.ground_on;
    state.ground_on = want;
    for children in &terrain {
        if !flipped && !children.is_changed() {
            continue;
        }
        for tile in children.iter() {
            if want {
                let Ok(plain) = tiles.get(tile) else { continue };
                if state.ground.is_none() {
                    let (Some(t), Some(base)) = (state.textures.as_ref(), standard.get(&plain.0)) else { continue };
                    let mut base = base.clone();
                    // A big map's paint stays (crate::visuals::ground: it has this sampler's
                    // settings). Otherwise the image's only use is the sampler: see `GroundSurfaces`.
                    if base.base_color_texture.is_none() {
                        base.base_color_texture = Some(t.white.clone());
                    }
                    let extension = GroundSurfaces {
                        place: mask_place(&layout.terrain),
                        mask: images.add(ground_mask(&layout.terrain)),
                        asphalt_color: t.asphalt.base.clone(),
                        asphalt_normal: t.asphalt.normal.clone(),
                        asphalt_orm: t.asphalt.orm.clone(),
                        pavers_color: t.pavers.base.clone(),
                        pavers_normal: t.pavers.normal.clone(),
                        pavers_orm: t.pavers.orm.clone(),
                        soil_color: t.soil.base.clone(),
                        soil_normal: t.soil.normal.clone(),
                        soil_orm: t.soil.orm.clone(),
                    };
                    let ground = grounds.add(ExtendedMaterial { base, extension });
                    state.ground = Some((plain.0.clone(), ground));
                }
                if let Some((_, ground)) = &state.ground {
                    commands.entity(tile).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(ground.clone()));
                }
            } else if let Some((plain, _)) = &state.ground {
                commands.entity(tile).remove::<MeshMaterial3d<GroundMaterial>>().insert(MeshMaterial3d(plain.clone()));
            }
        }
    }
}
