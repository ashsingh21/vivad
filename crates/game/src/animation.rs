//! Warrior animation: authored clips (tools/blender/characters/anims.py, one set per rig) blended
//! from the simulation state each frame.
//!
//! - Locomotion is a blend space: idle, walk and run in four directions and a forward sprint,
//!   weighted by the body-relative velocity. All cycles share one phase, advanced at the
//!   character's real ground speed, so feet stay planted while weights shift.
//! - Weapon actions (sword guard and swing, bow aim, drinking) drive the torso and arms only:
//!   the graph holds every locomotion clip twice, full-body and legs-only (masked), and as an
//!   action fades in the full-body copies hand over to the legs-only ones.
//! - Air states and death take over the whole body. Every weight eases toward its target, so
//!   state changes cross-fade.
//!
//! The character GLBs carry no animations of their own, so the bones aren't animation targets
//! when they spawn; `bind` tags them with the same name-path ids the glTF loader gives the clips.

use std::collections::HashMap;

use bevy::animation::{AnimatedBy, AnimationTargetId};
use bevy::gltf::Gltf;
use bevy::prelude::*;
use shared::items::{ItemDb, ItemKind};
use shared::sim::movement::{AIM_SPEED, SPRINT_SPEED, WALK_SPEED};
use shared::sim::{Action, Eliminated, Inventory, Kinematic, MoveMode, PosHistory, Warrior};

use crate::visuals::WarriorVisual;

pub struct WarriorAnimationPlugin;

impl Plugin for WarriorAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Footstep>().add_systems(Startup, load_sets).add_systems(Update, (build_graphs, drive).chain());
    }
}

/// Clips the torso can't share with the legs: played legs-only under a weapon action.
const LOCOMOTION: [&str; 10] = ["idle", "walk_f", "walk_b", "walk_l", "walk_r", "run_f", "run_b", "run_l", "run_r", "sprint_f"];
const FULL_BODY: [&str; 5] = ["jump", "fall", "freefall", "glide", "death"];
const UPPER_BODY: [&str; 4] = ["melee_guard", "melee_swing", "bow_aim", "consume"];
/// Bones driven by the legs layer; everything else is upper body.
const LOWER_BONES: [&str; 10] = ["Root", "pelvis", "thigh_l", "thigh_r", "calf_l", "calf_r", "foot_l", "foot_r", "ball_l", "ball_r"];
const LOWER: u32 = 0;
const UPPER: u32 = 1;
/// The authored swing lasts this long (melee_swing is 12 frames at 30 fps).
const SWING_TIME: f32 = 0.4;
const FADE_RATE: f32 = 12.0;

struct AnimSet {
    gltf: Handle<Gltf>,
    graph: Option<Handle<AnimationGraph>>,
    /// Clip name -> (full-body node, masked node: legs-only for locomotion, torso-only for actions).
    nodes: HashMap<&'static str, (AnimationNodeIndex, Option<AnimationNodeIndex>)>,
    durations: HashMap<&'static str, f32>,
    /// Bone name -> path from the rig root, to tag targets in spawned characters.
    paths: HashMap<String, Vec<Name>>,
}

#[derive(Resource)]
pub struct AnimSets([AnimSet; 2]);

impl AnimSets {
    pub fn ready(&self, variant: usize) -> bool {
        self.0[variant].graph.is_some()
    }
}

/// A foot coming down (from the locomotion cycle's contact points), for footstep sounds.
#[derive(Message, Clone, Copy)]
pub struct Footstep {
    /// Where the foot lands (ground level under the warrior).
    pub pos: Vec3,
    pub speed: f32,
}

/// Per-warrior blend state, on the visual entity.
#[derive(Component)]
pub struct WarriorAnim {
    variant: usize,
    player: Entity,
    phase: f32,
    clock: f32,
    weights: HashMap<AnimationNodeIndex, f32>,
}

fn load_sets(mut commands: Commands, assets: Res<AssetServer>) {
    let set = |v: &str| AnimSet {
        gltf: assets.load(format!("models/characters/{v}_anims.glb")),
        graph: None,
        nodes: HashMap::new(),
        durations: HashMap::new(),
        paths: HashMap::new(),
    };
    commands.insert_resource(AnimSets([set("male"), set("female")]));
}

/// Once a set's glTF is in, build its graph: every clip once full-body, locomotion again
/// legs-only and the weapon actions torso-only.
fn build_graphs(
    mut sets: ResMut<AnimSets>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<bevy::gltf::GltfNode>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    for set in sets.0.iter_mut() {
        if set.graph.is_some() {
            continue;
        }
        let Some(gltf) = gltfs.get(&set.gltf) else { continue };
        let all: Vec<&'static str> = LOCOMOTION.iter().chain(&FULL_BODY).chain(&UPPER_BODY).copied().collect();
        if all.iter().any(|n| gltf.named_animations.get(*n).and_then(|h| clips.get(h)).is_none()) {
            continue;
        }
        // Name paths from the rig root, as the loader computes AnimationTargetIds.
        let mut paths = HashMap::new();
        let mut stack: Vec<(Handle<bevy::gltf::GltfNode>, Vec<Name>)> =
            gltf.named_nodes.get("Human.rig").map(|h| vec![(h.clone(), vec![])]).unwrap_or_default();
        while let Some((h, mut path)) = stack.pop() {
            let Some(n) = nodes.get(&h) else { continue };
            path.push(Name::new(n.name.clone()));
            paths.insert(n.name.clone(), path.clone());
            for c in &n.children {
                stack.push((c.clone(), path.clone()));
            }
        }
        let mut graph = AnimationGraph::new();
        for (name, path) in &paths {
            let group = if LOWER_BONES.contains(&name.as_str()) { LOWER } else { UPPER };
            graph.add_target_to_mask_group(AnimationTargetId::from_names(path.iter()), group);
        }
        let root = graph.root;
        for name in all {
            let h = gltf.named_animations[name].clone();
            set.durations.insert(name, clips.get(&h).map(|c| c.duration()).unwrap_or(1.0).max(1e-3));
            let full = graph.add_clip(h.clone(), 1.0, root);
            let masked = if LOCOMOTION.contains(&name) {
                Some(graph.add_clip_with_mask(h, 1 << UPPER, 1.0, root))
            } else if UPPER_BODY.contains(&name) {
                Some(graph.add_clip_with_mask(h, 1 << LOWER, 1.0, root))
            } else {
                None
            };
            set.nodes.insert(name, (full, masked));
        }
        set.paths = paths;
        set.graph = Some(graphs.add(graph));
        info!("warrior animations ready: {} bones", set.paths.len());
    }
}

/// Make a freshly spawned character animatable: tag its bones as targets of a player on the rig.
pub fn bind(commands: &mut Commands, sets: &AnimSets, variant: usize, rig_root: Entity, bones: &HashMap<String, Entity>) -> Option<WarriorAnim> {
    let set = &sets.0[variant];
    let graph = set.graph.clone()?;
    for (name, &e) in bones {
        if let Some(path) = set.paths.get(name) {
            commands.entity(e).insert((AnimationTargetId::from_names(path.iter()), AnimatedBy(rig_root)));
        }
    }
    commands.entity(rig_root).insert((AnimationPlayer::default(), AnimationGraphHandle(graph)));
    Some(WarriorAnim { variant, player: rig_root, phase: 0.0, clock: 0.0, weights: HashMap::new() })
}

/// Body-relative locomotion weights from velocity: two nearest directions by angle, then the
/// two nearest speed tiers.
fn locomotion_weights(forward: f32, right: f32) -> Vec<(&'static str, f32)> {
    let speed = forward.hypot(right);
    if speed < 0.15 {
        return vec![("idle", 1.0)];
    }
    let (f, r) = (forward / speed, right / speed);
    let dirs = [("f", f.max(0.0).powi(2)), ("b", (-f).max(0.0).powi(2)), ("l", (-r).max(0.0).powi(2)), ("r", r.max(0.0).powi(2))];
    let mut out = vec![];
    let tier = |out: &mut Vec<(&'static str, f32)>, prefix: &str, w: f32| {
        for (d, dw) in dirs {
            if dw * w > 1e-3 {
                let name: &'static str = match (prefix, d) {
                    ("walk", "f") => "walk_f", ("walk", "b") => "walk_b", ("walk", "l") => "walk_l", ("walk", "r") => "walk_r",
                    ("run", "f") => "run_f", ("run", "b") => "run_b", ("run", "l") => "run_l", _ => "run_r",
                };
                out.push((name, dw * w));
            }
        }
    };
    if speed <= AIM_SPEED {
        let a = speed / AIM_SPEED;
        out.push(("idle", 1.0 - a));
        tier(&mut out, "walk", a);
    } else if speed <= WALK_SPEED {
        let b = (speed - AIM_SPEED) / (WALK_SPEED - AIM_SPEED);
        tier(&mut out, "walk", 1.0 - b);
        tier(&mut out, "run", b);
    } else {
        // Sprint exists forward only; the forward share of the run hands over to it.
        let c = ((speed - WALK_SPEED) / (SPRINT_SPEED - WALK_SPEED)).clamp(0.0, 1.0);
        tier(&mut out, "run", 1.0);
        for (n, w) in out.iter_mut() {
            if *n == "run_f" {
                *w *= 1.0 - c;
            }
        }
        out.push(("sprint_f", dirs[0].1 * c));
    }
    out
}

fn clip_speed(name: &str) -> f32 {
    match name {
        n if n.starts_with("walk") => AIM_SPEED,
        n if n.starts_with("run") => WALK_SPEED,
        "sprint_f" => SPRINT_SPEED,
        _ => 0.0,
    }
}

#[allow(clippy::type_complexity)]
fn drive(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    sets: Res<AnimSets>,
    db: Res<ItemDb>,
    mut vis: Query<(&WarriorVisual, &Transform, &mut WarriorAnim)>,
    sims: Query<(&Kinematic, &PosHistory, &Inventory, &Action, Option<&Eliminated>), With<Warrior>>,
    mut players: Query<&mut AnimationPlayer>,
    mut died: Local<HashMap<Entity, f32>>,
    mut steps: MessageWriter<Footstep>,
) {
    let dt = time.delta_secs();
    for (wv, tf, mut anim) in &mut vis {
        let Ok((kin, hist, inv, act, elim)) = sims.get(wv.sim) else { continue };
        let set = &sets.0[anim.variant];
        anim.clock += dt;
        let clock = anim.clock;
        let vel = (hist.curr - hist.prev) / fixed.timestep().as_secs_f32().max(1e-4);
        let local = tf.rotation.inverse() * vel;
        let (forward, right) = (-local.z, local.x);

        // Targets: (clip, use the masked node?, weight, time in the clip).
        let mut want: Vec<(&'static str, bool, f32, f32)> = vec![];
        let dur = |n: &str| set.durations.get(n).copied().unwrap_or(1.0);
        if elim.is_some() {
            let since = *died.entry(wv.sim).or_insert(clock);
            want.push(("death", false, 1.0, (clock - since).min(dur("death") - 1e-3)));
        } else {
            died.remove(&wv.sim);
            match kin.mode {
                // Riding the vimana (hidden): stand still, no steps.
                MoveMode::Vimana => want.push(("idle", false, 1.0, clock % dur("idle"))),
                MoveMode::Freefall => want.push(("freefall", false, 1.0, clock % dur("freefall"))),
                MoveMode::Gliding => want.push(("glide", false, 1.0, clock % dur("glide"))),
                MoveMode::Ground if !kin.grounded => {
                    let n = if vel.y > 1.0 { "jump" } else { "fall" };
                    want.push((n, false, 1.0, if n == "jump" { dur(n) * 0.5 } else { clock % dur(n) }));
                }
                _ => {
                    let loco = locomotion_weights(forward, right);
                    // Advance the shared cycle at the real ground speed.
                    let speed = forward.hypot(right);
                    let (mut rate, mut wsum) = (0.0, 0.0);
                    for (n, w) in &loco {
                        let v = clip_speed(n);
                        if v > 0.0 {
                            rate += w * (speed / v) / dur(n);
                            wsum += w;
                        }
                    }
                    if wsum > 0.0 {
                        // Contacts are at the cycle's start (left foot) and middle (right).
                        let before = anim.phase;
                        anim.phase = (anim.phase + dt * rate / wsum).fract();
                        let crossed = anim.phase < before || (before < 0.5 && anim.phase >= 0.5);
                        if crossed && speed > 0.8 {
                            steps.write(Footstep { pos: tf.translation - Vec3::Y * shared::sim::HALF_HEIGHT, speed });
                        }
                    }
                    // Upper-body action from what's in hand.
                    let held = inv.current().map(|s| &db.get(&s.id).kind);
                    let action = match held {
                        Some(ItemKind::Melee { .. }) if act.since_attack < SWING_TIME => Some(("melee_swing", act.since_attack / SWING_TIME * dur("melee_swing"))),
                        Some(ItemKind::Melee { .. }) => Some(("melee_guard", clock % dur("melee_guard"))),
                        Some(ItemKind::Bow { .. }) => Some(("bow_aim", clock % dur("bow_aim"))),
                        Some(ItemKind::Consumable { .. }) if act.using.is_some() => Some(("consume", clock % dur("consume"))),
                        _ => None,
                    };
                    let phase = anim.phase;
                    for (n, w) in loco {
                        let t = if n == "idle" { clock % dur(n) } else { phase * dur(n) };
                        want.push((n, action.is_some(), w, t));
                    }
                    if let Some((n, t)) = action {
                        want.push((n, true, 1.0, t));
                    }
                }
            }
        }

        // Ease every node toward its target weight; nodes not wanted fade to zero.
        let Ok(mut player) = players.get_mut(anim.player) else { continue };
        let k = (dt * FADE_RATE).min(1.0);
        let mut targets: HashMap<AnimationNodeIndex, (f32, f32)> = HashMap::new();
        for (n, masked, w, t) in want {
            let Some(&(full, mask)) = set.nodes.get(n) else { continue };
            let node = if masked { mask.unwrap_or(full) } else { full };
            targets.insert(node, (w, t));
        }
        let mut all: Vec<AnimationNodeIndex> = anim.weights.keys().copied().collect();
        all.extend(targets.keys().copied());
        all.sort();
        all.dedup();
        for node in all {
            let (target, t) = targets.get(&node).copied().unwrap_or((0.0, -1.0));
            let w = anim.weights.entry(node).or_insert(0.0);
            *w += (target - *w) * k;
            if *w < 1e-3 && target == 0.0 {
                anim.weights.remove(&node);
                player.stop(node);
                continue;
            }
            let w = *w;
            let a = if player.is_playing_animation(node) { player.animation_mut(node).unwrap() } else { player.play(node) };
            a.set_speed(0.0).set_weight(w);
            // Fading-out nodes keep their last time.
            if t >= 0.0 {
                a.seek_to(t);
            }
        }
    }
}
