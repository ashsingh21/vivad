//! Presentation of simulation entities: graybox map, warriors, loot, projectiles, effects, sky.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::light::{Atmosphere, CascadeShadowConfigBuilder, atmosphere::ScatteringMedium};
use bevy::prelude::*;
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
                    dress_fx,
                    animate_fx,
                    dress_vimana,
                    update_zone_wall,
                    cleanup_orphans,
                ),
            );
    }
}

/// Baked models in assets/models/weapons, one per item (the arrow-quiver pickup shows the tarkash).
pub const WEAPON_MODELS: [&str; 11] =
    ["khanda", "gandiva", "sharanga", "pinaka", "arrow", "tarkash", "agneyastra", "vayavyastra", "sanjeevani", "soma", "amrita"];

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
    sphere: Handle<Mesh>,
    mats: HashMap<(Mat, u8), Handle<StandardMaterial>>,
    rarity_glow: HashMap<Rarity, Handle<StandardMaterial>>,
    wood: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    pub(crate) string: Handle<StandardMaterial>,
    fire: Handle<StandardMaterial>,
    wind: Handle<StandardMaterial>,
    zone: Handle<StandardMaterial>,
    pub(crate) models: HashMap<&'static str, ModelHandles>,
    outfits: Vec<(Handle<StandardMaterial>, Handle<StandardMaterial>)>,
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
    let outfit_colors = [
        (Color::srgb(0.95, 0.55, 0.10), Color::srgb(0.95, 0.92, 0.85)), // local: saffron + white
        (Color::srgb(0.70, 0.15, 0.20), Color::srgb(0.90, 0.75, 0.30)),
        (Color::srgb(0.15, 0.35, 0.70), Color::srgb(0.95, 0.95, 0.95)),
        (Color::srgb(0.20, 0.50, 0.30), Color::srgb(0.90, 0.60, 0.20)),
        (Color::srgb(0.50, 0.20, 0.55), Color::srgb(0.95, 0.85, 0.40)),
        (Color::srgb(0.85, 0.80, 0.70), Color::srgb(0.70, 0.15, 0.15)),
        (Color::srgb(0.25, 0.25, 0.28), Color::srgb(0.85, 0.55, 0.15)),
        (Color::srgb(0.90, 0.40, 0.55), Color::srgb(0.25, 0.30, 0.60)),
    ];
    let outfits = outfit_colors
        .iter()
        .map(|(a, b)| {
            (
                materials.add(StandardMaterial { base_color: *a, perceptual_roughness: 0.8, ..default() }),
                materials.add(StandardMaterial { base_color: *b, perceptual_roughness: 0.8, ..default() }),
            )
        })
        .collect();
    commands.insert_resource(Palette {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(20)),
        sphere: meshes.add(Sphere::new(1.0).mesh().uv(24, 14)),
        mats,
        rarity_glow,
        wood: materials.add(StandardMaterial { base_color: Color::srgb(0.40, 0.24, 0.12), perceptual_roughness: 0.7, ..default() }),
        gold: materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.78, 0.34), metallic: 1.0, perceptual_roughness: 0.3, ..default() }),
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
        outfits,
    });
}

#[derive(Component)]
pub struct Sun;

fn setup_sky(mut commands: Commands, mut media: ResMut<Assets<ScatteringMedium>>, palette: Res<Palette>, mut meshes: ResMut<Assets<Mesh>>) {
    // Late-afternoon sun from the west-south-west: long shadows across the ghats.
    commands.spawn((
        Sun,
        DirectionalLight { illuminance: 32_000.0, shadow_maps_enabled: std::env::var("VIVAD_NO_SHADOWS").is_err(), color: Color::srgb(1.0, 0.94, 0.84), ..default() },
        Transform::from_xyz(-1.0, 0.75, 0.45).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 4, maximum_distance: 260.0, first_cascade_far_bound: 18.0, ..default() }.build(),
    ));
    commands.spawn(Atmosphere::earth(media.add(ScatteringMedium::default())));
    commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.85, 0.88, 1.0), brightness: 450.0, ..default() });
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
fn dress_terrain(
    mut commands: Commands,
    q: Query<Entity, Added<shared::sim::MapTerrain>>,
    layout: Res<shared::map::MapLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    for e in &q {
        let t = &layout.terrain;
        let n = t.n;
        let mut pos = Vec::with_capacity(n * n);
        let mut nrm = Vec::with_capacity(n * n);
        let mut col = Vec::with_capacity(n * n);
        let mut uv = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                let (x, z) = (t.coord(i), t.coord(j));
                let h = t.at(i, j);
                let nn = t.normal(x, z);
                pos.push([x, h, z]);
                nrm.push(nn.to_array());
                uv.push([x / 4.0, z / 4.0]);
                let (c, _) = crate::grass::paint(x, z, h, nn, t.road_d[i * n + j]);
                col.push([c.red, c.green, c.blue, 1.0]);
            }
        }
        let mut idx: Vec<u32> = Vec::with_capacity((n - 1) * (n - 1) * 6);
        for i in 0..n - 1 {
            for j in 0..n - 1 {
                let a = (i * n + j) as u32;
                let b = ((i + 1) * n + j) as u32;
                let c = ((i + 1) * n + j + 1) as u32;
                let d = (i * n + j + 1) as u32;
                idx.extend_from_slice(&[a, d, c, a, c, b]);
            }
        }
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx));
        commands.entity(e).insert((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.95, ..default() })),
            Visibility::default(),
        ));
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
        if !b.visible || (!door && crate::buildings::is_static_visual(b)) {
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

fn dress_warriors(mut commands: Commands, q: Query<(Entity, &Warrior, &Transform), Added<Warrior>>, palette: Res<Palette>) {
    for (e, w, t) in &q {
        let (_, trim) = palette.outfits[(w.look as usize) % palette.outfits.len()].clone();
        let root = commands.spawn((WarriorVisual { sim: e }, Transform::from_translation(t.translation), Visibility::default())).id();
        let held = commands.spawn((HeldWeapon { showing: None }, Transform::from_xyz(0.36, 0.15, -0.25), Visibility::default(), ChildOf(root))).id();
        // Garuda wings: a saffron canopy held overhead while gliding.
        commands.spawn((Glider, Mesh3d(palette.cube.clone()), MeshMaterial3d(trim),
                        Transform::from_xyz(0.0, 1.6, 0.0).with_scale(Vec3::new(3.4, 0.06, 1.3)), Visibility::Hidden, ChildOf(root)));
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
        // Rarity-coloured glow disc under a spinning, bobbing model.
        commands.spawn((Mesh3d(palette.cylinder.clone()), MeshMaterial3d(palette.rarity_glow[&rarity].clone()),
                        Transform::from_xyz(0.0, -0.22, 0.0).with_scale(Vec3::new(0.9, 0.02, 0.9)),
                        bevy::light::NotShadowCaster, ChildOf(e)));
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
        tf.rotation = Quat::from_rotation_y(t * 1.6) * Quat::from_rotation_z(0.5);
        tf.translation.y = 0.3 * tf.scale.y.max(1.0) + (t * 2.0).sin() * 0.08;
    }
}

fn dress_projectiles(mut commands: Commands, q: Query<(Entity, &Projectile), Added<Projectile>>, palette: Res<Palette>) {
    for (e, p) in &q {
        commands.entity(e).insert(Visibility::default()).with_children(|c| {
            if p.astra.is_some() {
                // The astra itself, point first, wrapped in a small glow of its element.
                let glow = if p.item == "agneyastra" { palette.fire.clone() } else { palette.wind.clone() };
                if let Some(m) = palette.models.get(p.item.as_str()) {
                    c.spawn((Mesh3d(m.mesh.clone()), MeshMaterial3d(m.mat.clone()), Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2))));
                }
                c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(glow), Transform::from_xyz(0.0, 0.0, -0.3).with_scale(Vec3::splat(0.1)),
                         bevy::light::NotShadowCaster));
            } else {
                // The arrow model points +Y from its nock; the projectile's point is the arrowhead.
                let m = &palette.models["arrow"];
                c.spawn((Mesh3d(m.mesh.clone()), MeshMaterial3d(m.mat.clone()),
                         Transform::from_xyz(0.0, 0.0, crate::arms::ARROW_LEN).with_rotation(Quat::from_rotation_x(-FRAC_PI_2))));
            }
        });
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
            c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.wind.clone()),
                     Transform::from_scale(Vec3::splat(b.radius * 0.3)), bevy::light::NotShadowCaster, FxSpin(b.radius)));
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

fn dress_vimana(mut commands: Commands, q: Query<Entity, Added<Vimana>>, palette: Res<Palette>) {
    for e in &q {
        commands.entity(e).insert(Visibility::default()).with_children(|c| {
            let white = palette.mats[&(Mat::Marble, 0)].clone();
            // Pushpaka Vimana: a flying temple-chariot. Hull, deck, pillared pavilion, shikhara, wings.
            c.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(palette.gold.clone()),
                     Transform::from_xyz(0.0, -1.0, 0.0).with_scale(Vec3::new(6.0, 2.0, 16.0))));
            c.spawn((Mesh3d(palette.sphere.clone()), MeshMaterial3d(palette.gold.clone()),
                     Transform::from_xyz(0.0, -1.0, -8.0).with_scale(Vec3::new(3.0, 1.0, 3.0))));
            c.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(white.clone()),
                     Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(7.0, 0.4, 15.0))));
            for (x, z) in [(-2.5, -4.0), (2.5, -4.0), (-2.5, 4.0), (2.5, 4.0)] {
                c.spawn((Mesh3d(palette.cylinder.clone()), MeshMaterial3d(white.clone()),
                         Transform::from_xyz(x, 2.2, z).with_scale(Vec3::new(0.5, 4.0, 0.5))));
            }
            c.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(white),
                     Transform::from_xyz(0.0, 4.4, 0.0).with_scale(Vec3::new(7.0, 0.5, 10.0))));
            for (i, s) in [5.0f32, 3.8, 2.6, 1.4].iter().enumerate() {
                c.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(palette.gold.clone()),
                         Transform::from_xyz(0.0, 5.2 + i as f32 * 1.2, 0.0).with_scale(Vec3::new(*s, 1.2, *s))));
            }
            for s in [-1.0, 1.0] {
                c.spawn((Mesh3d(palette.cube.clone()), MeshMaterial3d(palette.rarity_glow[&Rarity::Divya].clone()),
                         Transform::from_xyz(s * 7.5, 0.0, 1.0).with_rotation(Quat::from_rotation_z(s * 0.2))
                             .with_scale(Vec3::new(9.0, 0.2, 4.0))));
            }
        });
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
