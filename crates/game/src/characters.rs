//! Character and vegetation art: skinned warriors driven by a procedural animator, and map props
//! with distance LODs.
//!
//! Character GLBs face +Z (MakeHuman), origin at the feet, `game_engine` rig. Until authored
//! animation clips exist, the skeleton is posed procedurally from the simulation state: stride from
//! velocity, idle breathing, freefall / glide poses, melee swings, bow stance.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::ops::Range;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use shared::map::MapLayout;
use shared::sim::{HALF_HEIGHT, PropIndex, Warrior};

use crate::visuals::{Glider, HeldWeapon, WarriorVisual};

pub struct CharacterArtPlugin;

impl Plugin for CharacterArtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_models)
            .add_systems(Startup, spawn_ornaments.after(load_models))
        .add_systems(Update, (dress_props, dress_warriors, bind_animations, swap_weapons));
    }
}

#[derive(Resource)]
struct Models {
    male: Handle<WorldAsset>,
    female: Handle<WorldAsset>,
    props: HashMap<&'static str, Handle<WorldAsset>>,
}

fn load_models(mut commands: Commands, assets: Res<AssetServer>, layout: Res<MapLayout>) {
    let mut props = HashMap::new();
    for p in &layout.props {
        props.entry(p.model).or_insert_with(|| assets.load(GltfAssetLabel::Scene(0).from_asset(p.model)));
    }
    for o in &layout.ornaments {
        props.entry(o.model).or_insert_with(|| assets.load(GltfAssetLabel::Scene(0).from_asset(o.model)));
    }
    commands.insert_resource(Models {
        male: assets.load(GltfAssetLabel::Scene(0).from_asset("models/characters/male.glb")),
        female: assets.load(GltfAssetLabel::Scene(0).from_asset("models/characters/female.glb")),
        props,
    });
}

/// After a glTF scene spawns, give every `..._LOD<n>` node (and its mesh children) a visibility range.
fn apply_lod_ranges(root: Entity, ranges: &[(Range<f32>, Range<f32>)], children: &Query<&Children>, names: &Query<&Name>, commands: &mut Commands) {
    for e in children.iter_descendants(root) {
        let Ok(name) = names.get(e) else { continue };
        let Some(lod) = name.as_str().rsplit("LOD").next().and_then(|s| s.parse::<usize>().ok()) else { continue };
        let Some((s, end)) = ranges.get(lod).cloned() else { continue };
        let vr = VisibilityRange { start_margin: s, end_margin: end, use_aabb: false };
        commands.entity(e).insert(vr.clone());
        for d in children.iter_descendants(e) {
            commands.entity(d).insert(vr.clone());
        }
    }
}

// ------------------------------------------------------------------ props

fn dress_props(mut commands: Commands, q: Query<(Entity, &PropIndex), Added<PropIndex>>, layout: Res<MapLayout>, models: Res<Models>) {
    for (e, PropIndex(i)) in &q {
        let prop = &layout.props[*i];
        let big = ["banyan", "ajanta", "stupa", "gopuram", "temple"].iter().any(|k| prop.model.contains(k));
        let bush = prop.model.contains("bush");
        let (lod0_end, far) = if bush { (35.0, 220.0) } else if big { (90.0, 900.0) } else { (60.0, 700.0) };
        commands.entity(e).insert(Visibility::default());
        commands
            .spawn((WorldAssetRoot(models.props[prop.model].clone()), Transform::default(), ChildOf(e)))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, mut commands: Commands| {
                apply_lod_ranges(
                    ready.entity,
                    &[(0.0..0.0, lod0_end..lod0_end + 8.0), (lod0_end..lod0_end + 8.0, far..far + 30.0)],
                    &children,
                    &names,
                    &mut commands,
                );
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
            ))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, mut commands: Commands| {
                // Pieces with a LOD1 switch to it at 45 m; single-LOD pieces stay on LOD0 until they fade.
                let has_lod1 = children.iter_descendants(ready.entity).any(|e| names.get(e).is_ok_and(|n| n.as_str().ends_with("LOD1")));
                let ranges: &[(Range<f32>, Range<f32>)] = if has_lod1 {
                    &[(0.0..0.0, 45.0..50.0), (45.0..50.0, far..far + 20.0)]
                } else {
                    &[(0.0..0.0, far..far + 20.0)]
                };
                apply_lod_ranges(ready.entity, ranges, &children, &names, &mut commands);
            });
    }
    info!("ornaments: {}", layout.ornaments.len());
}

// ------------------------------------------------------------------ characters

const BONES: [&str; 17] = [
    "pelvis", "spine_01", "spine_02", "spine_03", "neck_01", "head", "clavicle_l", "clavicle_r", "upperarm_l",
    "upperarm_r", "lowerarm_l", "lowerarm_r", "hand_r", "thigh_l", "thigh_r", "calf_l", "calf_r",
];

/// Bone handles plus their rest pose, captured when the character scene is ready.
#[derive(Component)]
struct CharRig {
    bones: HashMap<&'static str, Entity>,
    /// (rest local rotation, rest model-space rotation)
    rest: HashMap<Entity, (Quat, Quat)>,
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
                           parents: Query<&ChildOf>,
                           mut commands: Commands| {
                apply_lod_ranges(
                    ready.entity,
                    &[(0.0..0.0, 16.0..20.0), (16.0..20.0, 50.0..56.0), (50.0..56.0, 400.0..420.0)],
                    &children,
                    &names,
                    &mut commands,
                );
                let mut bones = HashMap::new();
                let mut named = HashMap::new();
                for e in children.iter_descendants(ready.entity) {
                    if let Ok(n) = names.get(e) {
                        named.entry(n.as_str().to_owned()).or_insert(e);
                        if let Some(b) = BONES.iter().find(|b| **b == n.as_str()) {
                            bones.entry(*b).or_insert(e);
                        }
                    }
                }
                let rig_root = named.get("Human.rig").copied();
                // Rest model-space rotation = product of rest local rotations up to the scene root.
                let mut rest = HashMap::new();
                for &e in bones.values() {
                    let local = transforms.get(e).map(|t| t.rotation).unwrap_or(Quat::IDENTITY);
                    let mut model = local;
                    let mut cur = e;
                    while let Ok(p) = parents.get(cur) {
                        cur = p.parent();
                        if cur == ready.entity {
                            break;
                        }
                        model = transforms.get(cur).map(|t| t.rotation).unwrap_or(Quat::IDENTITY) * model;
                    }
                    rest.insert(e, (local, model));
                }
                commands.entity(vis).insert(CharRig { bones, rest, weapon_attached: false, variant, rig_root, named });
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

/// Move the held-weapon socket onto the right hand bone once the rig exists.
fn swap_weapons(
    mut commands: Commands,
    mut vis: Query<(&mut CharRig, &Children), With<WarriorVisual>>,
    mut held: Query<(Entity, &mut Transform), (With<HeldWeapon>, Without<WarriorVisual>)>,
    gliders: Query<(), With<Glider>>,
) {
    for (mut rig, children) in &mut vis {
        if rig.weapon_attached {
            continue;
        }
        let Some(&hand) = rig.bones.get("hand_r") else { continue };
        let Some(&(_, hand_model)) = rig.rest.get(&hand) else { continue };
        for c in children.iter() {
            if gliders.contains(c) {
                continue;
            }
            if let Ok((e, mut t)) = held.get_mut(c) {
                // Blade (+Y) forward and up in model space (+Z forward), expressed in the hand's frame.
                let want = Quat::from_rotation_arc(Vec3::Y, Vec3::new(0.0, 0.55, 1.0).normalize());
                t.rotation = hand_model.inverse() * want;
                t.translation = Vec3::new(0.0, 0.085, 0.0);
                commands.entity(e).insert(ChildOf(hand));
                rig.weapon_attached = true;
            }
        }
    }
}
