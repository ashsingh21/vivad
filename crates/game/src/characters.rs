//! Character and vegetation art: skinned warriors (animated by `crate::animation`, holding their
//! weapon per `crate::arms`) and map props with distance LODs.
//!
//! Character GLBs face +Z (MakeHuman), origin at the feet, `game_engine` rig. `<variant>_parts.glb`
//! has the head, body, outfit, eyes, cornea, mouth and cards as separate meshes over four LODs
//! (tools/blender/characters/finish.py); with VIVAD_CHAR_PARTS=0, or if it isn't there, the old
//! one-mesh `<variant>.glb` is loaded instead. The bones are the same in both, so one set of clips
//! drives either.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::ops::Range;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use shared::map::MapLayout;
use shared::sim::{HALF_HEIGHT, OfMap, PropIndex, Warrior, map_reloaded};

use crate::visuals::{Glider, HeldWeapon, WarriorVisual};

pub struct CharacterArtPlugin;

impl Plugin for CharacterArtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_models)
            .add_systems(Startup, spawn_ornaments.after(load_models))
            .add_systems(Update, (load_props, spawn_ornaments).chain().run_if(map_reloaded).before(dress_props))
            .add_systems(Update, (dress_props, dress_warriors, bind_animations, swap_weapons, retune_part_lods));
    }
}

#[derive(Resource)]
struct Models {
    male: Handle<WorldAsset>,
    female: Handle<WorldAsset>,
    props: HashMap<&'static str, Handle<WorldAsset>>,
}

fn load_models(mut commands: Commands, assets: Res<AssetServer>, layout: Res<MapLayout>) {
    let parts = std::env::var("VIVAD_CHAR_PARTS").as_deref() != Ok("0");
    commands.insert_resource(Models {
        male: assets.load(GltfAssetLabel::Scene(0).from_asset(character_file("male", parts))),
        female: assets.load(GltfAssetLabel::Scene(0).from_asset(character_file("female", parts))),
        props: prop_models(&assets, &layout),
    });
}

/// The model to load for a variant: the split one if it's on disk and not switched off.
fn character_file(variant: &str, parts_on: bool) -> String {
    let parts = format!("models/characters/{variant}_parts.glb");
    // The browser build can't look on disk.
    if parts_on && cfg!(not(target_arch = "wasm32")) && std::path::Path::new(crate::ASSET_DIR).join(&parts).exists() {
        info!("character model: {parts}");
        return parts;
    }
    format!("models/characters/{variant}.glb")
}

fn prop_models(assets: &AssetServer, layout: &MapLayout) -> HashMap<&'static str, Handle<WorldAsset>> {
    let mut props = HashMap::new();
    for model in layout.props.iter().map(|p| p.model).chain(layout.ornaments.iter().map(|o| o.model)) {
        props.entry(model).or_insert_with(|| assets.load(GltfAssetLabel::Scene(0).from_asset(model)));
    }
    props
}

/// Another map: its props' and kit pieces' models.
fn load_props(mut models: ResMut<Models>, assets: Res<AssetServer>, layout: Res<MapLayout>) {
    models.props = prop_models(&assets, &layout);
}

/// A LOD node's visibility range before the quality level scales it (crate::quality::LodScale),
/// and which LOD it is (Low lets only LOD0 cast shadows).
#[derive(Component, Clone)]
pub(crate) struct LodRange(pub VisibilityRange, pub usize);

impl LodRange {
    pub(crate) fn scaled(&self, k: f32) -> VisibilityRange {
        let r = &self.0;
        VisibilityRange { start_margin: r.start_margin.start * k..r.start_margin.end * k, end_margin: r.end_margin.start * k..r.end_margin.end * k, use_aabb: r.use_aabb }
    }
}

/// Ranges for a model's LODs: LOD k gives way to LOD k+1 at `switch[k]` (crossfading over 8 m),
/// and the last fades out at `far`. LODs past the switches given never show.
fn lod_ranges(lods: usize, switch: &[f32], far: Range<f32>) -> Vec<(Range<f32>, Range<f32>)> {
    let mut start = 0.0..0.0;
    (0..lods)
        .map(|k| {
            let end = if k + 1 == lods { far.clone() } else { switch.get(k).map_or(far.clone(), |&d| d..d + 8.0) };
            (std::mem::replace(&mut start, end.clone()), end)
        })
        .collect()
}

/// How many `..._LOD<n>` levels the spawned scene under `root` has.
fn lod_count(root: Entity, children: &Query<&Children>, names: &Query<&Name>) -> usize {
    children.iter_descendants(root).filter_map(|e| names.get(e).ok()?.as_str().rsplit("LOD").next()?.parse::<usize>().ok()).max().map_or(1, |n| n + 1)
}

/// After a glTF scene spawns, give every `..._LOD<n>` node (and its mesh children) a visibility
/// range, scaled by the quality level's `scale`; None keeps it fixed whatever the level (warriors,
/// whom everyone must see as far, and the vimana).
pub(crate) fn apply_lod_ranges(root: Entity, ranges: &[(Range<f32>, Range<f32>)], scale: Option<f32>, children: &Query<&Children>, names: &Query<&Name>, commands: &mut Commands) {
    for e in children.iter_descendants(root) {
        let Ok(name) = names.get(e) else { continue };
        let Some(lod) = name.as_str().rsplit("LOD").next().and_then(|s| s.parse::<usize>().ok()) else { continue };
        let Some((s, end)) = ranges.get(lod).cloned() else { continue };
        let base = LodRange(VisibilityRange { start_margin: s, end_margin: end, use_aabb: false }, lod);
        for d in std::iter::once(e).chain(children.iter_descendants(e)) {
            match scale {
                Some(k) => commands.entity(d).insert((base.scaled(k), base.clone(), crate::quality::LodShadow)),
                None => commands.entity(d).insert(base.0.clone()),
            };
        }
    }
}

// ------------------------------------------------------------------ props

fn dress_props(mut commands: Commands, q: Query<(Entity, &PropIndex), Added<PropIndex>>, layout: Res<MapLayout>, models: Res<Models>) {
    for (e, PropIndex(i)) in &q {
        let prop = &layout.props[*i];
        // Trees and bushes are drawn merged, per chunk (crate::batch).
        if crate::batch::batched(prop.model) {
            continue;
        }
        let big = ["banyan", "ajanta", "stupa", "gopuram", "temple"].iter().any(|k| prop.model.contains(k));
        let bush = prop.model.contains("bush");
        // Where LOD0 gives way to LOD1, and LOD1 to LOD2 (trees, the stepwell, the fort, the caves).
        let (switch, far) = if bush { ([35.0, 220.0], 220.0) } else if big { ([90.0, 260.0], 900.0) } else { ([60.0, 180.0], 700.0) };
        commands.entity(e).insert(Visibility::default());
        commands
            .spawn((WorldAssetRoot(models.props[prop.model].clone()), Transform::default(), ChildOf(e)))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, scale: Res<crate::quality::LodScale>, mut commands: Commands| {
                // A model with no LOD1 (the ghats: long, and light enough) keeps its LOD0 to the end.
                let ranges = lod_ranges(lod_count(ready.entity, &children, &names), &switch, far..far + 30.0);
                apply_lod_ranges(ready.entity, &ranges, Some(scale.0), &children, &names, &mut commands);
            });
    }
}

/// Architectural kit pieces (arches, jharokhas, chhatris): static, visual only, faded out at range.
fn spawn_ornaments(mut commands: Commands, layout: Res<MapLayout>, models: Res<Models>) {
    for o in &layout.ornaments {
        // Pieces that replace a whole building must outlast the distance its silhouette matters.
        let far = if o.model.contains("hut") {
            700.0
        } else if o.model.contains("chhatri") || o.model.contains("gate") || o.model.contains("rock") {
            400.0
        } else if o.model.contains("stall") {
            250.0
        } else {
            160.0
        };
        commands
            .spawn((
                WorldAssetRoot(models.props[o.model].clone()),
                Transform { translation: o.pos, rotation: o.rot, scale: o.scale },
                OfMap,
            ))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, scale: Res<crate::quality::LodScale>, mut commands: Commands| {
                // Pieces with a LOD1 switch to it at 45 m; single-LOD pieces stay on LOD0 until they fade.
                let ranges = lod_ranges(lod_count(ready.entity, &children, &names), &[45.0], far..far + 20.0);
                apply_lod_ranges(ready.entity, &ranges, Some(scale.0), &children, &names, &mut commands);
            });
    }
    info!("ornaments: {}", layout.ornaments.len());
}

// ------------------------------------------------------------------ characters

const BONES: [&str; 17] = [
    "pelvis", "spine_01", "spine_02", "spine_03", "neck_01", "head", "clavicle_l", "clavicle_r", "upperarm_l",
    "upperarm_r", "lowerarm_l", "lowerarm_r", "hand_r", "thigh_l", "thigh_r", "calf_l", "calf_r",
];

/// Bone handles, captured when the character scene is ready.
#[derive(Component)]
struct CharRig {
    bones: HashMap<&'static str, Entity>,
    weapon_attached: bool,
    /// For binding the animation player once the clips are loaded.
    variant: usize,
    rig_root: Option<Entity>,
    named: HashMap<String, Entity>,
}

#[derive(Component)]
struct CharacterModel;

/// The old model's LODs: everything but its cards, which have no LOD names and always show.
fn one_mesh_ranges() -> [(Range<f32>, Range<f32>); 3] {
    [(0.0..0.0, 16.0..20.0), (16.0..20.0, 50.0..56.0), (50.0..56.0, 400.0..420.0)]
}

/// Ranges of the split model's four LODs. LOD0 shows only where the quality level allows it
/// (`Quality::character_lod0`); elsewhere each band moves down a LOD and LOD0 stays hidden, so
/// the near and middle bands cost about what the old model's did.
fn part_ranges(lod0: bool) -> [(Range<f32>, Range<f32>); 4] {
    if lod0 {
        [(0.0..0.0, 16.0..20.0), (16.0..20.0, 50.0..56.0), (50.0..56.0, 150.0..160.0), (150.0..160.0, 400.0..420.0)]
    } else {
        [(0.0..0.0, 16.0..20.0), (0.0..0.0, 16.0..20.0), (16.0..20.0, 50.0..56.0), (50.0..56.0, 400.0..420.0)]
    }
}

fn part_lod_visibility(lod: usize, lod0: bool) -> Visibility {
    if lod == 0 && !lod0 { Visibility::Hidden } else { Visibility::Inherited }
}

/// On every entity of a split character's LOD node: which LOD, to follow the quality level.
#[derive(Component)]
struct PartLod(usize);

/// Set up a spawned split character's LODs. Parts too small to shade the ground cast no shadow,
/// which keeps the shadow passes near the old model's draw count.
fn dress_part_lods(root: Entity, lod0: bool, children: &Query<&Children>, names: &Query<&Name>, commands: &mut Commands) {
    apply_lod_ranges(root, &part_ranges(lod0), None, children, names, commands);
    for e in children.iter_descendants(root) {
        let Ok(name) = names.get(e) else { continue };
        let Some(lod) = name.as_str().rsplit("LOD").next().and_then(|s| s.parse::<usize>().ok()) else { continue };
        let no_shadow = ["_eyes_", "_cornea_", "_mouth_", "_eyebrows_", "_eyelashes_"].iter().any(|p| name.as_str().contains(p));
        for d in std::iter::once(e).chain(children.iter_descendants(e)) {
            commands.entity(d).insert((PartLod(lod), part_lod_visibility(lod, lod0)));
            if no_shadow {
                commands.entity(d).insert(bevy::light::NotShadowCaster);
            }
        }
    }
}

/// The quality level changed: move the split characters' LODs to its bands.
fn retune_part_lods(quality: Res<crate::quality::Quality>, mut q: Query<(&PartLod, &mut VisibilityRange, &mut Visibility)>) {
    if !quality.is_changed() {
        return;
    }
    let lod0 = quality.character_lod0();
    let ranges = part_ranges(lod0);
    for (lod, mut range, mut visibility) in &mut q {
        let Some((start, end)) = ranges.get(lod.0).cloned() else { continue };
        range.start_margin = start;
        range.end_margin = end;
        visibility.set_if_neq(part_lod_visibility(lod.0, lod0));
    }
}

fn dress_warriors(mut commands: Commands, q: Query<(Entity, &WarriorVisual), Added<WarriorVisual>>, warriors: Query<&Warrior>, models: Res<Models>) {
    for (vis, wv) in &q {
        let look = warriors.get(wv.sim).map(|w| w.look).unwrap_or(0);
        let variant = (look % 2) as usize;
        let scene = if variant == 0 { models.male.clone() } else { models.female.clone() };
        commands
            .spawn((
                CharacterModel,
                WorldAssetRoot(scene),
                // Feet on the capsule bottom; turn the +Z-facing model to face the sim's -Z forward.
                Transform::from_xyz(0.0, -HALF_HEIGHT, 0.0).with_rotation(Quat::from_rotation_y(PI)),
                ChildOf(vis),
            ))
            .observe(move |ready: On<WorldInstanceReady>,
                           children: Query<&Children>,
                           names: Query<&Name>,
                           transforms: Query<&Transform>,
                           quality: Res<crate::quality::Quality>,
                           mut commands: Commands| {
                if lod_count(ready.entity, &children, &names) >= 4 {
                    dress_part_lods(ready.entity, quality.character_lod0(), &children, &names, &mut commands);
                } else {
                    apply_lod_ranges(ready.entity, &one_mesh_ranges(), None, &children, &names, &mut commands);
                }
                let mut bones = HashMap::new();
                let mut named = HashMap::new();
                for e in children.iter_descendants(ready.entity) {
                    commands.entity(e).insert(crate::rim::RimTarget);
                    if let Ok(n) = names.get(e) {
                        named.entry(n.as_str().to_owned()).or_insert(e);
                        if let Some(b) = BONES.iter().find(|b| **b == n.as_str()) {
                            bones.entry(*b).or_insert(e);
                        }
                    }
                }
                let rig_root = named.get("Human.rig").copied();
                // Torso and drawing-hand bones for the aim layer and the bow string.
                let at = |n: &str| named.get(n).and_then(|&e| transforms.get(e).ok()).map(|t| t.translation);
                let spine = ["spine_01", "spine_02", "spine_03", "neck_01"].map(|n| named.get(n).copied());
                if let (Some(hand_l), [Some(a), Some(b), Some(c), Some(d)], Some(i), Some(m), Some(p)) =
                    (named.get("hand_l").copied(), spine, at("index_01_l"), at("middle_01_l"), at("pinky_01_l"))
                {
                    // The string sits in the crook of the curled index and middle fingers.
                    let pinch_l = m * 1.3 + crate::arms::palm_normal(i, m, p) * 0.025;
                    commands.entity(vis).insert(crate::arms::Body { root: ready.entity, spine: [a, b, c, d], hand_l, pinch_l });
                }
                commands.entity(vis).insert(CharRig { bones, weapon_attached: false, variant, rig_root, named });
            });
    }
}

/// Hand each character to the animation system once its rig and the clips are both loaded.
fn bind_animations(
    mut commands: Commands,
    q: Query<(Entity, &CharRig), Without<crate::animation::WarriorAnim>>,
    sets: Option<Res<crate::animation::AnimSets>>,
) {
    let Some(sets) = sets else { return };
    for (e, rig) in &q {
        let Some(root) = rig.rig_root else { continue };
        if !sets.ready(rig.variant) {
            continue;
        }
        if let Some(anim) = crate::animation::bind(&mut commands, &sets, rig.variant, root, &rig.named) {
            commands.entity(e).insert(anim);
        }
    }
}

/// Move the held-weapon socket into the right fist once the rig exists (see `crate::arms`).
fn swap_weapons(
    mut commands: Commands,
    mut vis: Query<(&mut CharRig, &Children), With<WarriorVisual>>,
    mut held: Query<(Entity, &mut Transform), (With<HeldWeapon>, Without<WarriorVisual>)>,
    bones: Query<&Transform, Without<HeldWeapon>>,
    gliders: Query<(), With<Glider>>,
) {
    for (mut rig, children) in &mut vis {
        if rig.weapon_attached {
            continue;
        }
        let Some(&hand) = rig.bones.get("hand_r") else { continue };
        let knuckle = |n: &str| rig.named.get(n).and_then(|&e| bones.get(e).ok()).map(|t| t.translation);
        let (Some(i), Some(m), Some(p)) = (knuckle("index_01_r"), knuckle("middle_01_r"), knuckle("pinky_01_r")) else { continue };
        let (rotation, grip) = crate::arms::hand_grip(i, m, p);
        for c in children.iter() {
            if gliders.contains(c) {
                continue;
            }
            if let Ok((e, mut t)) = held.get_mut(c) {
                t.rotation = rotation;
                t.translation = grip;
                commands.entity(e).insert(ChildOf(hand));
                rig.weapon_attached = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    use bevy::animation::AnimationTargetId;
    use bevy::asset::AssetMetaCheck;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::gltf::Gltf;
    use bevy::log::LogPlugin;
    use bevy::mesh::skinning::SkinnedMesh;
    use bevy::render::RenderPlugin;
    use bevy::render::settings::WgpuSettings;
    use bevy::winit::WinitPlugin;

    use super::*;
    use crate::quality::Quality;

    #[test]
    fn part_lods_cover_every_distance_without_gaps() {
        for lod0 in [false, true] {
            let ranges = part_ranges(lod0);
            let shown: Vec<_> = ranges.iter().enumerate().filter(|(k, _)| *k > 0 || lod0).map(|(_, r)| r).collect();
            assert_eq!(shown[0].0, 0.0..0.0);
            assert_eq!(shown.last().unwrap().1, 400.0..420.0);
            for pair in shown.windows(2) {
                assert_eq!(pair[0].1, pair[1].0, "a band must start as the one before it fades");
            }
        }
    }

    #[test]
    fn the_kill_switch_loads_the_old_model() {
        assert_eq!(character_file("male", false), "models/characters/male.glb");
        assert_eq!(character_file("nobody", true), "models/characters/nobody.glb");
    }

    #[derive(Resource, Default)]
    struct Spawned(Option<Entity>);

    fn headless() -> App {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<WinitPlugin>()
                .disable::<LogPlugin>()
                .set(AssetPlugin { file_path: crate::ASSET_DIR.into(), meta_check: AssetMetaCheck::Never, ..default() })
                .set(RenderPlugin { render_creation: WgpuSettings { backends: None, ..default() }.into(), ..default() }),
        );
        app.finish();
        app.cleanup();
        app.init_resource::<Spawned>();
        app
    }

    /// Bevy's real glTF loader, no GPU: every split model spawns its parts skinned to the bones
    /// the clips animate, by the name paths the clips were loaded with, and its LODs follow the
    /// quality level.
    #[test]
    fn split_models_spawn_and_take_the_clips() {
        for variant in ["male", "female"] {
            let file = format!("models/characters/{variant}_parts.glb");
            if !std::path::Path::new(crate::ASSET_DIR).join(&file).exists() {
                eprintln!("{file} not built: skipped");
                continue;
            }
            let mut app = headless();
            let assets = app.world().resource::<AssetServer>().clone();
            let scene = assets.load(GltfAssetLabel::Scene(0).from_asset(file));
            let anims: Handle<Gltf> = assets.load(format!("models/characters/{variant}_anims.glb"));
            app.world_mut().spawn(WorldAssetRoot(scene)).observe(|ready: On<WorldInstanceReady>, mut spawned: ResMut<Spawned>| spawned.0 = Some(ready.entity));
            let deadline = Instant::now() + Duration::from_secs(120);
            let root = loop {
                app.update();
                let world = app.world();
                let clips_in = world.resource::<Assets<Gltf>>().get(&anims).is_some_and(|g| g.named_animations.values().all(|h| world.resource::<Assets<AnimationClip>>().get(h).is_some()));
                if let (Some(root), true) = (world.resource::<Spawned>().0, clips_in) {
                    break root;
                }
                assert!(Instant::now() < deadline, "{variant}: never finished loading");
                std::thread::sleep(Duration::from_millis(5));
            };

            let lod0 = false;
            app.world_mut().run_system_once(move |children: Query<&Children>, names: Query<&Name>, mut commands: Commands| {
                assert_eq!(lod_count(root, &children, &names), 4);
                dress_part_lods(root, lod0, &children, &names, &mut commands);
            }).unwrap();

            // The name paths from the rig root, as `animation::bind` tags the bones.
            let world = app.world_mut();
            let mut q = world.query::<(Entity, &Name)>();
            let rig = q.iter(world).find(|(_, n)| n.as_str() == "Human.rig").expect("no rig").0;
            let mut ids = HashSet::new();
            let mut stack = vec![(rig, vec![Name::new("Human.rig")])];
            while let Some((e, path)) = stack.pop() {
                ids.insert(AnimationTargetId::from_names(path.iter()));
                for c in world.get::<Children>(e).into_iter().flat_map(|c| c.iter()) {
                    if let Some(n) = world.get::<Name>(c) {
                        let mut path = path.clone();
                        path.push(n.clone());
                        stack.push((c, path));
                    }
                }
            }
            let gltf = world.resource::<Assets<Gltf>>().get(&anims).unwrap();
            let clips = world.resource::<Assets<AnimationClip>>();
            assert_eq!(gltf.named_animations.len(), 21);
            for (name, h) in &gltf.named_animations {
                let missing = clips.get(h).unwrap().curves().keys().filter(|id| !ids.contains(id)).count();
                assert_eq!(missing, 0, "{variant} {name}: {missing} animated bones not found");
            }

            let mut parts = world.query::<(&Mesh3d, &SkinnedMesh, &PartLod, &Visibility, &VisibilityRange, Has<bevy::light::NotShadowCaster>, &Name)>();
            let mut count = 0;
            for (_, skin, lod, visibility, range, no_shadow, name) in parts.iter(world) {
                count += 1;
                assert_eq!(skin.joints.len(), 53, "{name}");
                assert_eq!(*visibility == Visibility::Hidden, lod.0 == 0, "{name}");
                assert_eq!(range.end_margin, part_ranges(lod0)[lod.0].1, "{name}");
                assert_eq!(no_shadow, ["eyes", "cornea", "mouth", "eyebrows", "eyelashes"].iter().any(|p| name.as_str().contains(&format!("{variant}_{p}"))), "{name}");
            }
            assert_eq!(count, 29, "{variant}: parts over the LODs");
            eprintln!("{variant}: {count} parts skinned to 53 bones, {} bone paths match the clips", ids.len());

            // Up to Ultra: LOD0 comes back, the bands shift.
            world.insert_resource(Quality::Ultra);
            world.run_system_once(retune_part_lods).unwrap();
            for (_, _, lod, visibility, range, ..) in parts.iter(world) {
                assert_eq!(*visibility, Visibility::Inherited);
                assert_eq!(range.end_margin, part_ranges(true)[lod.0].1);
            }
        }
    }
}
