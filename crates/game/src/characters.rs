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
use shared::items::{ItemDb, ItemKind};
use shared::map::MapLayout;
use shared::sim::{Action, Eliminated, HALF_HEIGHT, Inventory, Kinematic, MoveMode, PosHistory, PropIndex, Warrior};

use crate::visuals::{Glider, HeldWeapon, WarriorVisual};

pub struct CharacterArtPlugin;

impl Plugin for CharacterArtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_models)
            .add_systems(Startup, spawn_ornaments.after(load_models))
        .add_systems(Update, (dress_props, dress_warriors, animate_warriors, swap_weapons));
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
        let big = prop.model.contains("banyan");
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
        let far = if o.model.contains("chhatri") || o.model.contains("gate") { 400.0 } else { 160.0 };
        commands
            .spawn((
                WorldAssetRoot(models.props[o.model].clone()),
                Transform { translation: o.pos, rotation: o.rot, scale: o.scale },
            ))
            .observe(move |ready: On<WorldInstanceReady>, children: Query<&Children>, names: Query<&Name>, mut commands: Commands| {
                apply_lod_ranges(ready.entity, &[(0.0..0.0, far..far + 20.0)], &children, &names, &mut commands);
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
    phase: f32,
    weapon_attached: bool,
}

#[derive(Component)]
struct CharacterModel;

fn dress_warriors(mut commands: Commands, q: Query<(Entity, &WarriorVisual), Added<WarriorVisual>>, warriors: Query<&Warrior>, models: Res<Models>) {
    for (vis, wv) in &q {
        let look = warriors.get(wv.sim).map(|w| w.look).unwrap_or(0);
        let scene = if look % 2 == 0 { models.male.clone() } else { models.female.clone() };
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
                for e in children.iter_descendants(ready.entity) {
                    if let Ok(n) = names.get(e) {
                        if let Some(b) = BONES.iter().find(|b| **b == n.as_str()) {
                            bones.entry(*b).or_insert(e);
                        }
                    }
                }
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
                commands.entity(vis).insert(CharRig { bones, rest, phase: 0.0, weapon_attached: false });
            });
    }
}

/// Rotate a bone by `delta`, expressed in the character's model space (+Z forward, +Y up, +X left).
fn pose(rig: &CharRig, tfs: &mut Query<&mut Transform, Without<WarriorVisual>>, bone: &str, delta: Quat) {
    let Some(&e) = rig.bones.get(bone) else { return };
    let Some(&(local, model)) = rig.rest.get(&e) else { return };
    if let Ok(mut t) = tfs.get_mut(e) {
        t.rotation = local * (model.inverse() * delta * model);
    }
}

#[allow(clippy::type_complexity, unused_assignments)]
fn animate_warriors(
    time: Res<Time>,
    mut vis: Query<(&WarriorVisual, &mut CharRig)>,
    sims: Query<(&Kinematic, &PosHistory, &Inventory, &Action, Has<Eliminated>)>,
    mut tfs: Query<&mut Transform, Without<WarriorVisual>>,
    db: Res<ItemDb>,
    fixed: Res<Time<Fixed>>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    for (wv, mut rig) in &mut vis {
        let Ok((kin, hist, inv, act, dead)) = sims.get(wv.sim) else { continue };
        let vel = (hist.curr - hist.prev) / fixed.timestep().as_secs_f32().max(1e-4);
        let speed = vel.xz().length();
        let held = inv.current().map(|s| db.get(&s.id).kind.clone());
        let qx = |a: f32| Quat::from_rotation_x(a);
        let qy = |a: f32| Quat::from_rotation_y(a);
        let qz = |a: f32| Quat::from_rotation_z(a);

        // Defaults: arms relaxed down from the A-pose.
        let mut arm_l = qz(-0.75);
        let mut arm_r = qz(0.75);
        let mut fore_l = Quat::IDENTITY;
        let mut fore_r = Quat::IDENTITY;
        let mut thigh_l = Quat::IDENTITY;
        let mut thigh_r = Quat::IDENTITY;
        let mut calf_l = Quat::IDENTITY;
        let mut calf_r = Quat::IDENTITY;
        let mut spine = qx(0.03 * (t * 1.7).sin()); // breathing
        let mut head = Quat::IDENTITY;

        match kin.mode {
            MoveMode::Freefall => {
                arm_l = qz(0.5);
                arm_r = qz(-0.5);
                thigh_l = qx(0.25) * qz(0.15);
                thigh_r = qx(0.25) * qz(-0.15);
                calf_l = qx(0.5);
                calf_r = qx(0.5);
            }
            MoveMode::Gliding => {
                // Hands up on the glider's crossbar.
                arm_l = qz(1.9);
                arm_r = qz(-1.9);
                fore_l = qz(0.3);
                fore_r = qz(-0.3);
                thigh_l = qx(-0.15);
                thigh_r = qx(-0.15);
                calf_l = qx(0.3);
                calf_r = qx(0.3);
            }
            _ if !kin.grounded && !dead => {
                // Jumping: knees tucked, arms up a little.
                arm_l = qz(-0.2);
                arm_r = qz(0.2);
                thigh_l = qx(-0.5);
                thigh_r = qx(-0.2);
                calf_l = qx(0.9);
                calf_r = qx(0.5);
            }
            _ => {
                // Stride: cadence grows with speed, amplitude saturates.
                rig.phase += dt * (1.2 + speed * 1.15);
                let amp = (speed / 6.0).clamp(0.0, 1.0);
                let s = rig.phase.sin();
                thigh_l = qx(-0.55 * amp * s);
                thigh_r = qx(0.55 * amp * s);
                calf_l = qx(0.9 * amp * (-(rig.phase).cos()).max(0.0) + 0.05 * amp);
                calf_r = qx(0.9 * amp * (rig.phase.cos()).max(0.0) + 0.05 * amp);
                arm_l = qz(-0.75) * qx(0.4 * amp * s);
                arm_r = qz(0.75) * qx(-0.4 * amp * s);
                fore_l = qx(-0.25 * amp);
                fore_r = qx(-0.25 * amp);
                spine = qx(0.12 * amp) * spine;
            }
        }

        // Upper body actions override the arms.
        match held {
            Some(ItemKind::Melee { .. }) if kin.mode == MoveMode::Ground => {
                // Guard: sword arm forward; swing overhead -> down on attack.
                let swing = (act.since_attack / 0.35).clamp(0.0, 1.0);
                let raise = if swing < 1.0 { (1.0 - swing) * 2.2 - 0.6 } else { 0.0 };
                arm_r = qz(0.55) * qx(-0.9 - raise.max(-0.6));
                fore_r = qx(-0.7 + 0.5 * (1.0 - swing));
                spine = qy(-0.25 * (1.0 - swing)) * spine;
            }
            Some(ItemKind::Bow { .. }) if kin.mode == MoveMode::Ground => {
                // Archer's stance: bow arm forward, draw hand at the chest.
                arm_r = qx(-1.45) * qz(0.15);
                fore_r = Quat::IDENTITY;
                arm_l = qx(-1.3) * qz(-0.5);
                fore_l = qz(-1.4);
                spine = qy(0.35) * spine;
                head = qy(-0.3);
            }
            Some(ItemKind::Consumable { .. }) if act.using.is_some() => {
                arm_r = qx(-1.2);
                fore_r = qx(-1.6);
                head = qx(0.25);
            }
            _ => {}
        }

        pose(&rig, &mut tfs, "upperarm_l", arm_l);
        pose(&rig, &mut tfs, "upperarm_r", arm_r);
        pose(&rig, &mut tfs, "lowerarm_l", fore_l);
        pose(&rig, &mut tfs, "lowerarm_r", fore_r);
        pose(&rig, &mut tfs, "thigh_l", thigh_l);
        pose(&rig, &mut tfs, "thigh_r", thigh_r);
        pose(&rig, &mut tfs, "calf_l", calf_l);
        pose(&rig, &mut tfs, "calf_r", calf_r);
        pose(&rig, &mut tfs, "spine_02", spine);
        pose(&rig, &mut tfs, "head", head);
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
