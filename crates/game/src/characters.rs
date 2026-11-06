//! Character and vegetation art: skinned warriors (animated by `crate::animation`, holding their
//! weapon per `crate::arms`) and map props with distance LODs.
//!
//! Character GLBs face +Z (MakeHuman), origin at the feet, `game_engine` rig.

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
            .add_systems(Startup, spawn_ornaments.after(load_models).run_if(not(resource_exists::<shared::sim::DeferMap>)))
            .add_systems(Update, (load_props, spawn_ornaments).chain().run_if(map_reloaded).before(dress_props))
            .add_systems(Update, (dress_props, dress_warriors, bind_animations, swap_weapons));
    }
}

/// The bodies, in the order warriors take them by `look` (the player's is the first): each a model
/// (models/characters/<model>.glb) and the clips it plays (<clips>_anims.glb). The anime ones are
/// VRMs (tools/blender/characters/vroid.py, `just vroid`): Seed-san, whose skeleton the women share,
/// and VRoid's CC0 samples (docs/references.md) fitted to hers or to the male's (`--fit`), so they
/// play those clips as they are. VIVAD_ANIME=<name> puts another build in the first's place.
pub(crate) fn bodies() -> Vec<(String, &'static str)> {
    let first = std::env::var("VIVAD_ANIME").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "anime".into());
    let rest = [("male", "male"), ("anime_apron", "anime"), ("female", "female"), ("anime_elf", "anime"), ("anime_tall", "male"), ("anime_frills", "anime")];
    std::iter::once((first, "anime")).chain(rest.map(|(m, c)| (m.to_string(), c))).collect()
}

/// A skin's colourway (crate::rim recolours by it in the shader): the clothes' and the hair's hue
/// turned (degrees), saturation and value scaled (per cent). The skin, eyes and face are never
/// touched: a skin is a body in other clothes, not another person.
/// White and pale cloth takes `dye` (sRGB, and how strongly, per cent), by its brightness. The
/// default is a body's own colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Palette {
    pub cloth: (i16, u8, u8),
    pub dye: [u8; 4],
    pub hair: (i16, u8, u8),
}

impl Default for Palette {
    fn default() -> Self {
        PALETTES[0]
    }
}

impl Palette {
    const fn new(cloth: (i16, u8, u8), dye: [u8; 4], hair: (i16, u8, u8)) -> Self {
        Self { cloth, dye, hair }
    }

    /// The shader's uniforms (crate::rim::Rim's recolour and dye) for a part named `part`.
    pub fn uniform(&self, part: &str) -> (Vec4, Vec4) {
        if !recoloured(part) {
            return (Vec4::ZERO, Vec4::ZERO);
        }
        let hair = is_hair(part);
        let (h, sat, val) = if hair { self.hair } else { self.cloth };
        let dye = if hair { [0; 4] } else { self.dye };
        let on = (h, sat, val) != (0, 100, 100) || dye[3] > 0;
        if !on {
            return (Vec4::ZERO, Vec4::ZERO);
        }
        let c = |v: u8| v as f32 / 255.0;
        (Vec4::new((h as f32).to_radians(), sat as f32 / 100.0, val as f32 / 100.0, 1.0), Vec4::new(c(dye[0]), c(dye[1]), c(dye[2]), dye[3] as f32 / 100.0))
    }
}

fn is_hair(part: &str) -> bool {
    part.contains("HAIR") || part.ends_with("_hair")
}

/// Whether a material is clothes or hair, which a colourway recolours (VRoid's part names, and the
/// MakeHuman bodies' hair: their clothes are in their skin's texture).
pub fn recoloured(part: &str) -> bool {
    part.contains("_CLOTH") || part == "toon_huku_bake" || is_hair(part)
}

/// The colourways every body comes in, the first its own. Cloth: hue turned, saturation, value;
/// hair the same. Bold and clean, as a stylised game's skins are; no near-skin tones on clothes.
const PALETTES: [Palette; 4] = [
    Palette::new((0, 100, 100), [0, 0, 0, 0], (0, 100, 100)),
    // Teal: the clothes' hues turned to sea green, white dyed it.
    Palette::new((150, 115, 95), [70, 190, 175, 80], (0, 60, 70)),
    // Saffron and maroon.
    Palette::new((-110, 120, 100), [245, 160, 60, 85], (25, 110, 110)),
    // Charcoal: muted, dark, white gone slate.
    Palette::new((60, 60, 65), [95, 100, 115, 85], (0, 30, 45)),
];

/// The skins: every body (`bodies`) in every colourway, body first, so a match's sixteen warriors
/// are seven bodies before any repeats in another colour. The MakeHuman bodies (their clothes in
/// their skin's texture) come in their own colours only. A warrior's `look` picks one.
pub fn skins() -> Vec<(usize, Palette)> {
    let bodies = bodies();
    let mut out = vec![];
    for (k, palette) in PALETTES.iter().enumerate() {
        for (b, (model, _)) in bodies.iter().enumerate() {
            if k == 0 || model.starts_with("anime") {
                out.push((b, *palette));
            }
        }
    }
    out
}

pub fn skin(look: u32) -> (usize, Palette) {
    let all = skins();
    all[look as usize % all.len()]
}

/// Each clip set once, in the order bodies first name them (crate::animation's AnimSets).
pub(crate) fn clip_sets() -> Vec<&'static str> {
    let mut sets = vec![];
    for (_, c) in bodies() {
        if !sets.contains(&c) {
            sets.push(c);
        }
    }
    sets
}

#[derive(Resource)]
struct Models {
    /// One per body (`bodies`), and the clip set it plays (`clip_sets`); the anime ones cel shaded
    /// (crate::rim).
    bodies: Vec<(Handle<WorldAsset>, usize)>,
    props: HashMap<&'static str, Handle<WorldAsset>>,
}

fn load_models(mut commands: Commands, assets: Res<AssetServer>, layout: Res<MapLayout>) {
    commands.insert_resource(Models {
        bodies: bodies()
            .iter()
            .map(|(m, c)| (assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/characters/{m}.glb"))), clip_sets().iter().position(|s| s == c).unwrap_or(0)))
            .collect(),
        props: prop_models(&assets, &layout),
    });
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

fn dress_warriors(mut commands: Commands, q: Query<(Entity, &WarriorVisual), Added<WarriorVisual>>, warriors: Query<&Warrior>, models: Res<Models>) {
    for (vis, wv) in &q {
        let look = warriors.get(wv.sim).map(|w| w.look).unwrap_or(0);
        // The bodies in turn, each on its skeleton's clips (`variant`: its set in crate::animation's
        // AnimSets), in its skin's colourway.
        let (body, palette) = skin(look);
        let (scene, variant) = models.bodies[body % models.bodies.len()].clone();
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
                           mut commands: Commands| {
                apply_lod_ranges(
                    ready.entity,
                    &[(0.0..0.0, 16.0..20.0), (16.0..20.0, 50.0..56.0), (50.0..56.0, 400.0..420.0)],
                    None,
                    &children,
                    &names,
                    &mut commands,
                );
                let mut bones = HashMap::new();
                let mut named = HashMap::new();
                for e in children.iter_descendants(ready.entity) {
                    commands.entity(e).insert(crate::rim::RimTarget(vis, palette));
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
                let pelvis = named.get("pelvis").copied();
                let pelvis_rest = pelvis.and_then(|e| transforms.get(e).ok()).map(|t| t.rotation).unwrap_or_default();
                if let (Some(hand_l), [Some(a), Some(b), Some(c), Some(d)], Some(i), Some(m), Some(p), Some(pelvis)) =
                    (named.get("hand_l").copied(), spine, at("index_01_l"), at("middle_01_l"), at("pinky_01_l"), pelvis)
                {
                    // The string sits in the crook of the curled index and middle fingers.
                    let pinch_l = m * 1.3 + crate::arms::palm_normal(i, m, p) * 0.025;
                    // The left fingers, knuckle to tip, to close round a long gun's fore-end.
                    let fingers_l = ["index", "middle", "ring", "pinky"]
                        .iter()
                        .flat_map(|f| (1..=3).filter_map(|k| named.get(&format!("{f}_0{k}_l")).copied()).collect::<Vec<_>>())
                        .collect();
                    commands.entity(vis).insert(crate::arms::Body {
                        root: ready.entity,
                        spine: [a, b, c, d],
                        pelvis,
                        pelvis_rest,
                        hand_l,
                        pinch_l,
                        knuckle_l: m,
                        palm_l: crate::arms::palm_normal(i, m, p),
                        fingers_l,
                        shoulder_r: named.get("upperarm_r").copied(),
                        head: named.get("head").copied(),
                        weapon_r: named.get("weapon_r").copied(),
                    });
                }
                if let Some(b) = crate::feel::TRACE_BONES.iter().map(|n| named.get(*n).copied()).collect::<Option<Vec<_>>>() {
                    commands.entity(vis).insert(crate::feel::TraceBones(b));
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
