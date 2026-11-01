//! Presentation of simulation entities: graybox map, warriors, loot, projectiles, effects, sky.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::light::{Atmosphere, CascadeShadowConfigBuilder, atmosphere::ScatteringMedium};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use shared::items::{ItemDb, ItemKind, Rarity};
use shared::map::{ChestKind, Mat, Shape};
use shared::sim::combat::{Burst, FireZone, Projectile};
use shared::sim::flow::Vimana;
use shared::sim::loot::{Chest, Pickup};
use shared::sim::zone::Zone;
use shared::sim::{
    Eliminated, Inventory, Kinematic, MapBlock, MoveMode,
    PosHistory, Warrior,
};

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_assets, setup_sky).chain())
            .add_systems(
                Update,
                (
                    dress_blocks,
                    dress_terrain,
                    resolve_gltf_materials,
                    dress_warriors,
                    (interpolate_warriors, pose_warriors).chain(),
                    dress_chests,
                    dress_pickups,
                    animate_pickups,
                    dress_projectiles,
                    spent_bullets,
                    spin_chakras,
                    dress_fx,
                    animate_fx,
                    dress_vimana,
                    warm_up_shaders,
                    sway_vimana,
                    update_zone_wall,
                    cleanup_orphans,
                ),
            );
    }
}

/// Baked models in assets/models/weapons, one per item (the arrow-quiver pickup shows the tarkash).
pub const WEAPON_MODELS: [&str; 14] = [
    "bat", "katta", "tamancha", "dunali", "gulel", "chakra", "brahmastra", "arrow", "tarkash", "agneyastra", "vayavyastra", "sanjeevani",
    "soma", "amrita",
];

/// The chakra model's radius (tools/blender/weapons/desi.py CHAKRA_R): its origin is the rim.
const CHAKRA_R: f32 = 0.13;

/// One weapon model's LOD0 mesh and its material (converted from the glTF material once loaded).
pub struct ModelHandles {
    pub mesh: Handle<Mesh>,
    pub mat: Handle<StandardMaterial>,
    gltf_mat: Handle<bevy::gltf::GltfMaterial>,
}

#[derive(Resource)]
pub struct Palette {
    cube: Handle<Mesh>,
    pub(crate) cylinder: Handle<Mesh>,
    pub(crate) sphere: Handle<Mesh>,
    pub(crate) mats: HashMap<(Mat, u8), Handle<StandardMaterial>>,
    rarity_glow: HashMap<Rarity, Handle<StandardMaterial>>,
    /// Under a pickup: a soft pool of its rarity's light on the ground, and a faint beam rising.
    rarity_pool: HashMap<Rarity, Handle<StandardMaterial>>,
    rarity_beam: HashMap<Rarity, Handle<StandardMaterial>>,
    pool_mesh: Handle<Mesh>,
    beam_mesh: Handle<Mesh>,
    wood: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    pub(crate) string: Handle<StandardMaterial>,
    fire: Handle<StandardMaterial>,
    wind: Handle<StandardMaterial>,
    /// The bright point at a flying arrow's nock.
    glint: Handle<StandardMaterial>,
    zone: Handle<StandardMaterial>,
    pub(crate) models: HashMap<&'static str, ModelHandles>,
}

fn mat_color(m: Mat) -> (Color, f32, f32) {
    // (base colour, perceptual roughness, metallic)
    match m {
        Mat::Grass => (Color::srgb(0.42, 0.50, 0.25), 0.95, 0.0),
        Mat::Dirt => (Color::srgb(0.62, 0.48, 0.33), 0.95, 0.0),
        Mat::Sandstone => (Color::srgb(0.84, 0.66, 0.45), 0.85, 0.0),
        Mat::RedSandstone => (Color::srgb(0.66, 0.30, 0.22), 0.85, 0.0),
        Mat::Granite => (Color::srgb(0.58, 0.55, 0.52), 0.9, 0.0),
        Mat::Plaster => (Color::srgb(0.92, 0.90, 0.85), 0.9, 0.0),
        Mat::PlasterOchre => (Color::srgb(0.88, 0.68, 0.36), 0.9, 0.0),
        Mat::PlasterBlue => (Color::srgb(0.45, 0.62, 0.85), 0.9, 0.0),
        Mat::Brick => (Color::srgb(0.70, 0.38, 0.27), 0.9, 0.0),
        Mat::Wood => (Color::srgb(0.42, 0.27, 0.15), 0.8, 0.0),
        Mat::Cloth => (Color::srgb(0.85, 0.20, 0.35), 0.9, 0.0),
        Mat::Foliage => (Color::srgb(0.26, 0.42, 0.18), 0.95, 0.0),
        Mat::Bark => (Color::srgb(0.36, 0.28, 0.22), 0.95, 0.0),
        Mat::Water => (Color::srgba(0.10, 0.26, 0.28, 0.85), 0.06, 0.0),
        Mat::Gold => (Color::srgb(1.0, 0.78, 0.34), 0.3, 1.0),
        Mat::Marble => (Color::srgb(0.95, 0.94, 0.92), 0.3, 0.0),
        Mat::Thatch => (Color::srgb(0.78, 0.66, 0.38), 0.95, 0.0),
    }
}

const VARIANTS: u8 = 4;

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    let mut mats = HashMap::new();
    let all = [
        Mat::Grass, Mat::Dirt, Mat::Sandstone, Mat::RedSandstone, Mat::Granite, Mat::Plaster, Mat::PlasterOchre,
        Mat::PlasterBlue, Mat::Brick, Mat::Wood, Mat::Cloth, Mat::Foliage, Mat::Bark, Mat::Water, Mat::Gold,
        Mat::Marble, Mat::Thatch,
    ];
    for m in all {
        let (c, rough, metal) = mat_color(m);
        for v in 0..VARIANTS {
            // Small tint variation so repeated blocks don't read as one flat colour.
            let k = 1.0 + (v as f32 - 1.5) * 0.035;
            let lin = c.to_linear();
            let base = Color::linear_rgba(lin.red * k, lin.green * k, lin.blue * k, lin.alpha);
            let mut sm = StandardMaterial { base_color: base, perceptual_roughness: rough, metallic: metal, ..default() };
            if m == Mat::Water {
                sm.alpha_mode = AlphaMode::Blend;
                sm.reflectance = 0.6;
            }
            if m == Mat::Cloth {
                let tints = [Color::srgb(0.85, 0.2, 0.35), Color::srgb(0.95, 0.55, 0.1), Color::srgb(0.2, 0.55, 0.6), Color::srgb(0.9, 0.8, 0.2)];
                sm.base_color = tints[v as usize];
                sm.double_sided = true;
                sm.cull_mode = None;
            }
            mats.insert((m, v), materials.add(sm));
        }
    }
    let mut rarity_glow = HashMap::new();
    for r in Rarity::ALL {
        let l = r.color().to_linear();
        rarity_glow.insert(r, materials.add(StandardMaterial {
            base_color: r.color(),
            emissive: LinearRgba::rgb(l.red * 6.0, l.green * 6.0, l.blue * 6.0),
            unlit: false,
            ..default()
        }));
    }
    // Loot light: a radial pool and a vertical fade, as alpha masks tinted per rarity, added
    // onto what's behind (light, not paint).
    let mask = |images: &mut Assets<Image>, f: &dyn Fn(f32, f32) -> f32| {
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let n = 128usize;
        let mut data = Vec::with_capacity(n * n * 4);
        for y in 0..n {
            for x in 0..n {
                let a = f((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32).clamp(0.0, 1.0);
                data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
            }
        }
        images.add(Image::new(Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data,
                              TextureFormat::Rgba8UnormSrgb, bevy::asset::RenderAssetUsages::RENDER_WORLD))
    };
    let pool_tex = mask(&mut images, &|u, v| {
        let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
        // A soft core fading out, and a faint ring at its rim.
        0.55 * (-(r * r) * 5.0).exp() + 0.28 * (-((r - 0.78) / 0.07).powi(2)).exp()
    });
    let beam_tex = mask(&mut images, &|u, v| {
        // v = 0 at the beam's foot; brighter down the middle of each card.
        (1.0 - v).powf(1.6) * (1.0 - (u - 0.5).abs() * 2.0).powf(1.5) * 0.7
    });
    let mut rarity_pool = HashMap::new();
    let mut rarity_beam = HashMap::new();
    for r in Rarity::ALL {
        let l = r.color().to_linear();
        // Common loot gets a dim pool and barely a beam; rarer loot glows more.
        let k = match r { Rarity::Common => 0.5, Rarity::Rare => 0.8, _ => 1.0 };
        let tint = |a: f32| Color::linear_rgba(l.red * a, l.green * a, l.blue * a, 1.0);
        let light = |tex: &Handle<Image>, a: f32, materials: &mut Assets<StandardMaterial>| materials.add(StandardMaterial {
            base_color: tint(a),
            base_color_texture: Some(tex.clone()),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        rarity_pool.insert(r, light(&pool_tex, 1.6 * k, &mut materials));
        rarity_beam.insert(r, light(&beam_tex, 1.2 * k * k, &mut materials));
    }
    let beam_mesh = {
        use bevy::mesh::{Indices, PrimitiveTopology};
        // Two crossed vertical cards, 0.22 wide and 1.7 tall.
        let (w, h) = (0.11, 1.7);
        let mut pos = vec![];
        let mut uv = vec![];
        for d in [Vec3::X, Vec3::Z] {
            for (s, t) in [(-1.0, 0.0), (1.0, 0.0), (1.0, 1.0), (-1.0, 1.0)] {
                pos.push((d * w * s + Vec3::Y * h * t).to_array());
                uv.push([(s + 1.0) / 2.0, t]);
            }
        }
        Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 8])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]))
    };
    commands.insert_resource(Palette {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(20)),
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(24, 14)),
        mats,
        rarity_glow,
        rarity_pool,
        rarity_beam,
        pool_mesh: meshes.add(Plane3d::default().mesh().size(1.3, 1.3)),
        beam_mesh: meshes.add(beam_mesh),
        wood: materials.add(StandardMaterial { base_color: Color::srgb(0.40, 0.24, 0.12), perceptual_roughness: 0.7, ..default() }),
        gold: materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.78, 0.34), metallic: 1.0, perceptual_roughness: 0.3, ..default() }),
        glint: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.95, 0.8),
            emissive: LinearRgba::rgb(6.0, 5.0, 3.2),
            unlit: true,
            ..default()
        }),
        fire: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.45, 0.1, 0.7),
            emissive: LinearRgba::rgb(12.0, 3.5, 0.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        wind: materials.add(StandardMaterial {
            base_color: Color::srgba(0.75, 0.9, 1.0, 0.25),
            emissive: LinearRgba::rgb(1.0, 1.6, 2.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        zone: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.35, 0.12, 0.22),
            emissive: LinearRgba::rgb(2.5, 0.6, 0.15),
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            unlit: true,
            ..default()
        }),
        models: WEAPON_MODELS
            .iter()
            .map(|&id| {
                let path = format!("models/weapons/{id}.glb");
                (id, ModelHandles {
                    mesh: assets.load(format!("{path}#Mesh0/Primitive0")),
                    // Neutral placeholder until the glTF material loads; then it's replaced in place.
                    mat: materials.add(StandardMaterial { base_color: Color::srgb(0.5, 0.45, 0.4), perceptual_roughness: 0.5, ..default() }),
                    gltf_mat: assets.load(format!("{path}#Material0")),
                })
            })
            .collect(),
        string: materials.add(StandardMaterial { base_color: Color::srgb(0.86, 0.82, 0.70), perceptual_roughness: 0.8, ..default() }),
    });
}

#[derive(Component)]
pub struct Sun;

fn setup_sky(mut commands: Commands, mut media: ResMut<Assets<ScatteringMedium>>, palette: Res<Palette>, mut meshes: ResMut<Assets<Mesh>>) {
    // Low morning sun from the east-south-east: the ghats and the Ajanta cliff both face east.
    commands.spawn((
        Sun,
        // About 34 degrees up and only a little warm: high enough that the walls keep their own
        // colours, low enough for long shadows down the lanes.
        DirectionalLight { illuminance: 27_000.0, shadow_maps_enabled: std::env::var("VIVAD_NO_SHADOWS").is_err(), color: Color::srgb(1.0, 0.91, 0.78), ..default() },
        Transform::from_xyz(1.0, 0.75, 0.45).looking_at(Vec3::ZERO, Vec3::Y),
        // Cascades and reach are the quality level's (crate::quality): the shadow pass is the
        // frame's largest GPU cost.
        CascadeShadowConfigBuilder::default().build(),
    ));
    commands.spawn(Atmosphere::earth(media.add(ScatteringMedium::default())));
    commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.82, 0.87, 1.0), brightness: 1800.0, ..default() });
    // The Pralaya wall.
    commands.spawn((
        ZoneWall,
        Mesh3d(meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(128).segments(1))),
        MeshMaterial3d(palette.zone.clone()),
        Transform::from_scale(Vec3::new(360.0, 400.0, 360.0)),
        bevy::light::NotShadowCaster,
    ));
}

#[derive(Component)]
struct ZoneWall;

fn update_zone_wall(zone: Res<Zone>, mut q: Query<&mut Transform, With<ZoneWall>>) {
    for mut t in &mut q {
        t.translation = Vec3::new(zone.center.x, 100.0, zone.center.y);
        t.scale = Vec3::new(zone.radius.max(0.5), 400.0, zone.radius.max(0.5));
    }
}

/// Bevy 0.20 loads glTF materials as `GltfMaterial`; convert each once and swap it into our handle.
fn resolve_gltf_materials(
    palette: Res<Palette>,
    gltf_mats: Res<Assets<bevy::gltf::GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut done: Local<Vec<&'static str>>,
) {
    if done.len() == palette.models.len() {
        return;
    }
    for (id, m) in &palette.models {
        if done.contains(id) {
            continue;
        }
        if let Some(gm) = gltf_mats.get(&m.gltf_mat) {
            let _ = materials.insert(m.mat.id(), bevy::pbr::gltf::standard_material_from_gltf_material(gm));
            done.push(id);
        }
    }
}

/// Terrain mesh from the shared heightfield, painted by `grass::paint`.
/// Terrain tile side in grid cells, and each LOD's step through the grid with where it gives way
/// to the next (m, from the tile's centre; scaled by quality, crate::quality::LodScale).
const TERRAIN_TILE: usize = 32;
const TERRAIN_LODS: [(usize, f32); 3] = [(1, 90.0), (2, 220.0), (4, 1.0e5)];

/// The ground, in tiles (culled off screen) of three LODs. Each tile hangs a skirt from its edges
/// so a coarser neighbour's edge never shows a crack.
fn dress_terrain(
    mut commands: Commands,
    q: Query<Entity, Added<shared::sim::MapTerrain>>,
    layout: Res<shared::map::MapLayout>,
    scale: Res<crate::quality::LodScale>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::visibility::VisibilityRange;
    use bevy::mesh::{Indices, PrimitiveTopology};
    for e in &q {
        let t = &layout.terrain;
        let n = t.n;
        let material = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() });
        commands.entity(e).insert(Visibility::default());
        // Grid indices a tile samples from `a` to `b` (inclusive) every `step`, always ending on `b`.
        let samples = |a: usize, b: usize, step: usize| -> Vec<usize> {
            let mut v: Vec<usize> = (a..b).step_by(step).collect();
            v.push(b);
            v
        };
        let mut start = 0.0;
        for (lod, &(step, end)) in TERRAIN_LODS.iter().enumerate() {
            for ti in (0..n - 1).step_by(TERRAIN_TILE) {
                for tj in (0..n - 1).step_by(TERRAIN_TILE) {
                    let (is, js) = (samples(ti, (ti + TERRAIN_TILE).min(n - 1), step), samples(tj, (tj + TERRAIN_TILE).min(n - 1), step));
                    let (mut pos, mut nrm, mut col, mut uv, mut idx) = (vec![], vec![], vec![], vec![], vec![]);
                    let mut vert = |i: usize, j: usize, drop: f32| -> u32 {
                        let (x, z) = (t.coord(i), t.coord(j));
                        let h = t.at(i, j);
                        let nn = t.normal(x, z);
                        let (c, _) = match t.ground.get(i * n + j) {
                            Some(&class) => crate::grass::paint_city(x, z, class),
                            None => crate::grass::paint(x, z, h, nn, t.road_d[i * n + j]),
                        };
                        pos.push([x, h - drop, z]);
                        nrm.push(nn.to_array());
                        uv.push([x / 4.0, z / 4.0]);
                        col.push([c.red, c.green, c.blue, 1.0]);
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
                            let (p, q, r, s) = (a * w + b, (a + 1) * w + b, (a + 1) * w + b + 1, a * w + b + 1);
                            idx.extend_from_slice(&[p, s, r, p, r, q]);
                        }
                    }
                    // Skirts: each edge copied a little lower and joined to it, wound both ways.
                    let skirt = 0.6 * step as f32;
                    let edges: [Vec<(usize, usize)>; 4] = [
                        js.iter().map(|&j| (is[0], j)).collect(),
                        js.iter().map(|&j| (*is.last().unwrap(), j)).collect(),
                        is.iter().map(|&i| (i, js[0])).collect(),
                        is.iter().map(|&i| (i, *js.last().unwrap())).collect(),
                    ];
                    for edge in edges {
                        let top: Vec<u32> = edge.iter().map(|&(i, j)| vert(i, j, 0.0)).collect();
                        let low: Vec<u32> = edge.iter().map(|&(i, j)| vert(i, j, skirt)).collect();
                        for k in 0..top.len() - 1 {
                            let (a, b, c, d) = (top[k], top[k + 1], low[k + 1], low[k]);
                            idx.extend_from_slice(&[a, b, c, a, c, d, a, c, b, a, d, c]);
                        }
                    }
                    // The tile's origin at its centre: LOD distances (and the crossfade between
                    // them) are measured from there.
                    let (lo, hi) = pos.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
                    let centre = (lo + hi) / 2.0;
                    let pos: Vec<[f32; 3]> = pos.iter().map(|p| (Vec3::from(*p) - centre).to_array()).collect();
                    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
                        .with_inserted_indices(Indices::U32(idx));
                    let base = crate::characters::LodRange(
                        VisibilityRange { start_margin: start..start + 10.0 * (lod > 0) as u8 as f32, end_margin: end..end + 10.0, use_aabb: false },
                        lod,
                    );
                    commands.spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(centre),
                        base.scaled(scale.0),
                        base,
                        crate::quality::LodShadow,
                        ChildOf(e),
                    ));
                }
            }
            start = end;
        }
    }
}

fn variant(p: Vec3) -> u8 {
    let h = (p.x * 12.9898 + p.z * 78.233 + p.y * 37.719).sin() * 43758.547;
    ((h.fract().abs() * VARIANTS as f32) as u8).min(VARIANTS - 1)
}

/// Map blocks get a mesh child (scaled), so the collider on the parent stays unscaled.
/// Per-entity meshes only for dynamic blocks (breakable doors) and water; static architecture is
/// batched by `buildings`.
fn dress_blocks(mut commands: Commands, q: Query<(Entity, &MapBlock, Has<shared::sim::Door>), Added<MapBlock>>, palette: Res<Palette>) {
    for (e, MapBlock(b), door) in &q {
        commands.entity(e).insert(Visibility::default());
        // Water is drawn by `water`.
        if !b.visible || b.mat == Mat::Water || (!door && crate::buildings::is_static_visual(b)) {
            continue;
        }
        let (mesh, scale) = match b.shape {
            Shape::Box(s) => (palette.cube.clone(), s),
            Shape::Cylinder { radius, height } => (palette.cylinder.clone(), Vec3::new(radius * 2.0, height, radius * 2.0)),
            Shape::Sphere(r) => (palette.sphere.clone(), Vec3::splat(r)),
        };
        let mat = palette.mats[&(b.mat, variant(b.pos))].clone();
        let mut child = commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_scale(scale), ChildOf(e)));
        if matches!(b.mat, Mat::Water | Mat::Foliage) {
            child.insert(bevy::light::NotShadowCaster);
        }
    }
}

// ------------------------------------------------------------------ warriors

/// The rendered body of a warrior; interpolated between fixed ticks.
#[derive(Component)]
pub struct WarriorVisual {
    pub sim: Entity,
}

#[derive(Component)]
pub struct HeldWeapon {
    showing: Option<String>,
}

#[derive(Component)]
pub struct Glider;

/// The held-weapon socket entity (reparented to the hand bone once the character rig loads).
#[derive(Component)]
pub struct WeaponSocket(pub Entity);

fn dress_warriors(mut commands: Commands, q: Query<(Entity, &Warrior, &Transform), Added<Warrior>>, assets: Res<AssetServer>) {
    for (e, w, t) in &q {
        let root = commands.spawn((WarriorVisual { sim: e }, Transform::from_translation(t.translation), Visibility::default())).id();
        let held = commands.spawn((HeldWeapon { showing: None }, Transform::from_xyz(0.36, 0.15, -0.25), Visibility::default(), ChildOf(root))).id();
        // A patang glider (tools/blender/props/glider.py) held overhead while gliding, in one of
        // four colourways by the warrior's look.
        let colourway = ["a", "b", "c", "d"][w.look as usize % 4];
        commands.spawn((
            Glider,
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/props/patang_glider_{colourway}.glb")))),
            // The model's origin is the feet; the warrior's is the capsule's middle.
            Transform::from_xyz(0.0, -shared::sim::HALF_HEIGHT, 0.0),
            Visibility::Hidden,
            ChildOf(root),
        ));
        commands.entity(root).insert(WeaponSocket(held));
    }
}

fn interpolate_warriors(
    fixed: Res<Time<Fixed>>,
    sims: Query<(&PosHistory, &Transform), Without<WarriorVisual>>,
    mut vis: Query<(&WarriorVisual, &mut Transform)>,
) {
    let a = fixed.overstep_fraction();
    for (v, mut t) in &mut vis {
        if let Ok((h, st)) = sims.get(v.sim) {
            t.translation = h.prev.lerp(h.curr, a);
            t.rotation = st.rotation;
        }
    }
}

#[allow(clippy::type_complexity)]
fn pose_warriors(
    mut commands: Commands,
    sims: Query<(&Kinematic, &Inventory, Has<Eliminated>)>,
    mut vis: Query<(&WarriorVisual, &WeaponSocket, &mut Visibility, &Children, &mut Transform)>,
    mut weapons: Query<&mut HeldWeapon>,
    mut gliders: Query<&mut Visibility, (With<Glider>, Without<WarriorVisual>)>,
    palette: Res<Palette>,
    db: Res<ItemDb>,
) {
    for (v, socket, mut visibility, children, mut vt) in &mut vis {
        let Ok((kin, inv, dead)) = sims.get(v.sim) else { continue };
        *visibility = if kin.mode == MoveMode::Vimana { Visibility::Hidden } else { Visibility::Inherited };
        // Death and the skydiving arch are in the animation clips.
        if !dead && matches!(kin.mode, MoveMode::Gliding) {
            vt.rotation *= Quat::from_rotation_x(-0.25);
        }
        for c in children.iter() {
            if let Ok(mut g) = gliders.get_mut(c) {
                *g = if kin.mode == MoveMode::Gliding && !dead { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
        if let Ok(mut held) = weapons.get_mut(socket.0) {
            let current = inv.current().map(|s| (s.id.clone(), s.rarity));
            let id = current.as_ref().map(|c| c.0.clone());
            if held.showing != id {
                held.showing = id;
                commands.entity(socket.0).despawn_children();
                if let Some((id, rarity)) = current {
                    spawn_weapon_model(&mut commands, socket.0, &id, rarity, &db, &palette);
                }
            }
        }
    }
}

/// The model an item shows (in hand, on the ground): its own, or the quiver for arrows.
pub fn model_for(id: &str) -> &str {
    if id == "arrows" { "tarkash" } else { id }
}

/// Where the hand closes on each model, in its glTF frame (the origin unless noted): vessels are
/// held by the neck, the herb bundle by its binding.
fn grip_point(model: &str) -> Vec3 {
    match model {
        "soma" => Vec3::Y * 0.052,
        "amrita" => Vec3::Y * 0.045,
        "sanjeevani" => Vec3::Y * -0.018,
        _ => Vec3::ZERO,
    }
}

/// Spawn an item's model under `parent` (a hand socket or a pickup), grip at the parent's origin.
/// Bows also get a string, drawn each frame by `crate::arms`.
pub fn spawn_weapon_model(commands: &mut Commands, parent: Entity, id: &str, _rarity: Rarity, db: &ItemDb, p: &Palette) {
    let model = model_for(id);
    let Some(m) = p.models.get(model) else { return };
    let mut e = commands.spawn((
        Mesh3d(m.mesh.clone()),
        MeshMaterial3d(m.mat.clone()),
        Transform::from_translation(-grip_point(model)),
        ChildOf(parent),
    ));
    if let ItemKind::Bow { pellets, .. } = &db.get(id).kind {
        e.insert(crate::arms::BowRig::new(model, *pellets));
    }
}

// ------------------------------------------------------------------ loot

#[derive(Component)]
struct ChestLid;

fn dress_chests(
    mut commands: Commands,
    added: Query<(Entity, &Chest), Added<Chest>>,
    changed: Query<(&Chest, &Children), Changed<Chest>>,
    mut lids: Query<&mut Transform, With<ChestLid>>,
    palette: Res<Palette>,
) {
    for (e, c) in &added {
        commands.entity(e).insert(Visibility::default()).with_children(|p| match c.kind {
            ChestKind::Peti => {
                p.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(palette.wood.clone()),
                         Transform::from_xyz(0.0, 0.3, 0.0).with_scale(Vec3::new(1.1, 0.6, 0.65))));
                for x in [-0.45, 0.45] {
                    p.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(palette.gold.clone()),
                             Transform::from_xyz(x, 0.3, 0.0).with_scale(Vec3::new(0.06, 0.62, 0.67))));
                }
                p.spawn((ChestLid, Mesh3d(palette.cube.clone()), MeshMaterial3d(palette.gold.clone()),
                         Transform::from_xyz(0.0, 0.64, 0.0).with_scale(Vec3::new(1.12, 0.1, 0.68))));
            }
            ChestKind::Kalash => {
                p.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.gold.clone()),
                         Transform::from_xyz(0.0, 0.42, 0.0).with_scale(Vec3::new(0.42, 0.4, 0.42))));
                p.spawn((Mesh3d(palette.cylinder.clone()), MeshMaterial3d(palette.gold.clone()),
                         Transform::from_xyz(0.0, 0.85, 0.0).with_scale(Vec3::new(0.3, 0.2, 0.3))));
                p.spawn((ChestLid, Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.rarity_glow[&Rarity::Divya].clone()),
                         Transform::from_xyz(0.0, 1.05, 0.0).with_scale(Vec3::splat(0.16))));
            }
        });
    }
    for (c, children) in &changed {
        if !c.opened {
            continue;
        }
        for ch in children.iter() {
            if let Ok(mut t) = lids.get_mut(ch) {
                t.rotation = Quat::from_rotation_x(-1.9);
                t.translation += Vec3::new(0.0, 0.3, -0.35);
            }
        }
    }
}

#[derive(Component)]
struct PickupSpin;

fn dress_pickups(mut commands: Commands, q: Query<(Entity, &Pickup), Added<Pickup>>, palette: Res<Palette>, db: Res<ItemDb>) {
    for (e, p) in &q {
        let rarity = p.stack.rarity;
        commands.entity(e).insert(Visibility::default());
        // A pool of the rarity's light on the ground and a faint beam rising through the model,
        // which hovers and turns slowly above.
        commands.spawn((Mesh3d(palette.pool_mesh.clone()), MeshMaterial3d(palette.rarity_pool[&rarity].clone()),
                        Transform::from_xyz(0.0, -0.27, 0.0), bevy::light::NotShadowCaster, ChildOf(e)));
        commands.spawn((Mesh3d(palette.beam_mesh.clone()), MeshMaterial3d(palette.rarity_beam[&rarity].clone()),
                        Transform::from_xyz(0.0, -0.27, 0.0), bevy::light::NotShadowCaster, ChildOf(e)));
        // Small things (vessels, herbs, astras) are shown larger so they read from a distance.
        let scale = match db.get(&p.stack.id).kind {
            ItemKind::Consumable { .. } => 2.2,
            ItemKind::Astra { .. } => 1.6,
            _ => 1.0,
        };
        let spin = commands.spawn((PickupSpin, Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::splat(scale)), Visibility::default(), ChildOf(e))).id();
        spawn_weapon_model(&mut commands, spin, &p.stack.id, rarity, &db, &palette);
    }
}

fn animate_pickups(time: Res<Time>, mut q: Query<&mut Transform, With<PickupSpin>>) {
    let t = time.elapsed_secs();
    for mut tf in &mut q {
        // Slow turn, a slight lean, a gentle breathing bob at waist height.
        tf.rotation = Quat::from_rotation_y(t * 0.9) * Quat::from_rotation_z(0.3);
        tf.translation.y = 0.42 * tf.scale.y.max(1.0) + (t * 1.5).sin() * 0.045;
    }
}

/// Arrow model scale in flight: (fletching and shaft width, length, width).
const FLIGHT_SCALE: Vec3 = Vec3::new(2.0, 1.2, 2.0);

/// What a weapon fires: the guns bullets, the gulel a pebble (bows, if any, arrows).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shot {
    Bullet,
    Pebble,
    Arrow,
}

/// Guns held to the shoulder (the `rifle_aim` clip) rather than out in the hand.
pub fn long_gun(item: &str) -> bool {
    item == "dunali"
}

pub fn shot_kind(item: &str) -> Shot {
    match item {
        "tamancha" | "dunali" | "katta" => Shot::Bullet,
        "gulel" => Shot::Pebble,
        _ => Shot::Arrow,
    }
}

fn dress_projectiles(mut commands: Commands, q: Query<(Entity, &Projectile), Added<Projectile>>, palette: Res<Palette>) {
    for (e, p) in &q {
        commands.entity(e).insert(Visibility::default()).with_children(|c| {
            if p.item == "chakra" {
                // Spinning flat, a little larger than in the hand so it reads in flight, in a halo.
                if let Some(m) = palette.models.get("chakra") {
                    c.spawn((ChakraSpin, Transform::from_scale(Vec3::splat(1.6)), Visibility::default())).with_children(|s| {
                        s.spawn((Mesh3d(m.mesh.clone()), MeshMaterial3d(m.mat.clone()),
                                 Transform::from_translation(Vec3::X * CHAKRA_R).with_rotation(Quat::from_rotation_z(FRAC_PI_2))));
                    });
                }
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.rarity_glow[&Rarity::Divya].clone()),
                         Transform::from_scale(Vec3::new(0.12, 0.03, 0.12)), bevy::light::NotShadowCaster));
                c.spawn((PointLight { color: Color::srgb(1.0, 0.8, 0.4), intensity: 80_000.0, range: 5.0, ..default() }, Transform::default()));
            } else if p.astra.is_some() {
                // The astra itself, point first, wrapped in a small glow of its element.
                let glow = match p.item.as_str() {
                    "agneyastra" => palette.fire.clone(),
                    "brahmastra" => palette.rarity_glow[&Rarity::Divya].clone(),
                    _ => palette.wind.clone(),
                };
                if let Some(m) = palette.models.get(p.item.as_str()) {
                    c.spawn((Mesh3d(m.mesh.clone()), MeshMaterial3d(m.mat.clone()), Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2))));
                }
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(glow), Transform::from_xyz(0.0, 0.0, -0.3).with_scale(Vec3::splat(0.1)),
                         bevy::light::NotShadowCaster));
            } else if shot_kind(&p.item) == Shot::Bullet {
                // A bullet is too small and fast to see: its tracer is the streak (crate::fx),
                // with a hot point at its head.
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.glint.clone()),
                         Transform::from_scale(Vec3::splat(0.03)), bevy::light::NotShadowCaster));
            } else if shot_kind(&p.item) == Shot::Pebble {
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.mats[&(Mat::Granite, 1)].clone()),
                         Transform::from_scale(Vec3::new(0.05, 0.045, 0.055))));
            } else {
                // The arrow model points +Y from its nock; the projectile's point is the arrowhead.
                // In flight it's drawn thicker and a little longer than the nocked arrow (seen from
                // behind, a true-size shaft and fletching vanish within a few metres), with a
                // bright glint at the nock, the end the shooter sees.
                let m = &palette.models["arrow"];
                let len = crate::arms::ARROW_LEN * FLIGHT_SCALE.y;
                c.spawn((Mesh3d(m.mesh.clone()), MeshMaterial3d(m.mat.clone()),
                         Transform::from_xyz(0.0, 0.0, len).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)).with_scale(FLIGHT_SCALE)));
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.glint.clone()),
                         Transform::from_xyz(0.0, 0.0, len).with_scale(Vec3::splat(0.035)), bevy::light::NotShadowCaster));
            }
        });
    }
}

/// A flying chakra's spin.
#[derive(Component)]
struct ChakraSpin;

fn spin_chakras(time: Res<Time>, mut q: Query<&mut Transform, With<ChakraSpin>>) {
    for mut t in &mut q {
        t.rotate_y(-28.0 * time.delta_secs());
    }
}

/// A bullet that has hit the world is gone (its puff and spark are crate::fx's).
fn spent_bullets(mut q: Query<(&Projectile, &mut Visibility), Changed<Projectile>>) {
    for (p, mut v) in &mut q {
        if p.stuck && p.astra.is_none() && shot_kind(&p.item) == Shot::Bullet {
            *v = Visibility::Hidden;
        }
    }
}

#[derive(Component)]
struct FxSpin(f32);

fn dress_fx(
    mut commands: Commands,
    fires: Query<(Entity, &FireZone), Added<FireZone>>,
    bursts: Query<(Entity, &Burst), Added<Burst>>,
    palette: Res<Palette>,
) {
    for (e, f) in &fires {
        commands.entity(e).insert(Visibility::default()).with_children(|c| {
            c.spawn((Mesh3d(palette.cylinder.clone()), MeshMaterial3d(palette.fire.clone()),
                     Transform::from_xyz(0.0, 0.4, 0.0).with_scale(Vec3::new(f.radius * 2.0, 0.8, f.radius * 2.0)),
                     bevy::light::NotShadowCaster, FxSpin(0.0)));
            c.spawn((PointLight { color: Color::srgb(1.0, 0.5, 0.15), intensity: 600_000.0, range: f.radius * 4.0, ..default() },
                     Transform::from_xyz(0.0, 1.5, 0.0)));
        });
    }
    for (e, b) in &bursts {
        commands.entity(e).insert(Visibility::default()).with_children(|c| {
            if b.blast {
                // The Brahmastra: a core of golden light inside a shockwave swelling to the blast's
                // edge, lighting everything round it.
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.rarity_glow[&Rarity::Divya].clone()),
                         Transform::from_scale(Vec3::splat(b.radius * 0.05)), bevy::light::NotShadowCaster, FxSpin(b.radius * 0.2)));
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.wind.clone()),
                         Transform::from_scale(Vec3::splat(b.radius * 0.3)), bevy::light::NotShadowCaster, FxSpin(b.radius)));
                c.spawn((PointLight { color: Color::srgb(1.0, 0.85, 0.55), intensity: 8.0e6, range: b.radius * 4.0, ..default() },
                         Transform::from_xyz(0.0, 2.0, 0.0)));
            } else {
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.wind.clone()),
                         Transform::from_scale(Vec3::splat(b.radius * 0.3)), bevy::light::NotShadowCaster, FxSpin(b.radius)));
            }
        });
    }
}

fn animate_fx(time: Res<Time>, mut q: Query<(&mut Transform, &FxSpin)>) {
    let t = time.elapsed_secs();
    for (mut tf, s) in &mut q {
        if s.0 > 0.0 {
            tf.scale = (tf.scale + Vec3::splat(s.0 * 3.0 * time.delta_secs())).min(Vec3::splat(s.0));
        } else {
            tf.scale.y = 0.8 + (t * 9.0).sin() * 0.25;
        }
    }
}

/// Shader warm-up (as Fortnite's "optimising shaders"): a render pipeline is compiled the first
/// time something is drawn with it, and in the browser that's a hitch mid-fight (the first shot,
/// the first chest, the first Brahmastra). A couple of seconds in (on the vimana, where a hitch
/// costs nothing), every material is drawn once for a few seconds, a speck in front of the camera:
/// the weapons and items on their own meshes, every other untextured material on a sphere. The
/// browser keeps compiled shaders between visits.
#[derive(Component)]
struct WarmUp;

fn warm_up_shaders(
    mut commands: Commands,
    mut frame: Local<u32>,
    palette: Res<Palette>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cam: Query<Entity, With<crate::camera::MainCamera>>,
    specks: Query<Entity, With<WarmUp>>,
) {
    const START: u32 = 120;
    const STOP: u32 = START + 200;
    *frame += 1;
    if *frame == START {
        let Ok(cam) = cam.single() else { return };
        let mut k = 0.0;
        let mut speck = |commands: &mut Commands, mesh: Handle<Mesh>, mat: Handle<StandardMaterial>| {
            commands.spawn((WarmUp, Mesh3d(mesh), MeshMaterial3d(mat), bevy::light::NotShadowCaster,
                            Transform::from_xyz(-0.4 + (k % 40.0) * 0.02, -0.2 + (k / 40.0).floor() * 0.02, -2.0).with_scale(Vec3::splat(0.0004)),
                            ChildOf(cam)));
            k += 1.0;
        };
        for m in palette.models.values() {
            speck(&mut commands, m.mesh.clone(), m.mat.clone());
        }
        let plain: Vec<_> = materials.iter().filter(|(_, m)| m.normal_map_texture.is_none()).map(|(id, _)| id).collect();
        for id in plain {
            if let Some(h) = materials.get_strong_handle(id) {
                speck(&mut commands, palette.sphere.clone(), h);
            }
        }
        // Shadow-pass pipelines too: one caster per model.
        for m in palette.models.values() {
            commands.spawn((WarmUp, Mesh3d(m.mesh.clone()), MeshMaterial3d(m.mat.clone()),
                            Transform::from_xyz(0.0, -0.3, -2.0).with_scale(Vec3::splat(0.0004)), ChildOf(cam)));
        }
        info!("shader warm-up: {} specks", specks.iter().count() + k as usize);
    }
    if *frame == STOP {
        for e in &specks {
            commands.entity(e).despawn();
        }
    }
}

/// The Pushpaka Vimana model (tools/blender/architecture/vimana.py), riding a little on the air.
#[derive(Component)]
struct VimanaHull;

fn dress_vimana(mut commands: Commands, q: Query<Entity, Added<Vimana>>, assets: Res<AssetServer>) {
    for e in &q {
        commands.entity(e).insert(Visibility::default());
        commands
            .spawn((
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("models/props/vimana.gltf"))),
                Transform::default(),
                VimanaHull,
                ChildOf(e),
            ))
            .observe(|ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, mut commands: Commands| {
                // Riders see it close up; from the ground it is 120 m away.
                crate::characters::apply_lod_ranges(ready.entity, &[(0.0..0.0, 70.0..80.0), (70.0..80.0, 2000.0..2100.0)], None,
                                                    &children, &names, &mut commands);
            });
    }
}

/// A slow heave and roll, so it floats rather than slides.
fn sway_vimana(time: Res<Time>, mut q: Query<&mut Transform, With<VimanaHull>>) {
    let t = time.elapsed_secs();
    for mut tf in &mut q {
        tf.translation.y = (t * 0.9).sin() * 0.35;
        tf.rotation = Quat::from_rotation_z((t * 0.6).sin() * 0.025) * Quat::from_rotation_x((t * 0.45).cos() * 0.015);
    }
}

/// Visual entities whose simulation entity is gone (restart) are removed.
fn cleanup_orphans(mut commands: Commands, vis: Query<(Entity, &WarriorVisual)>, sims: Query<(), With<Warrior>>) {
    for (e, v) in &vis {
        if !sims.contains(v.sim) {
            commands.entity(e).despawn();
        }
    }
}
