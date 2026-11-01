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
use shared::sim::movement::{CROUCH_SPEED, SPRINT_SPEED, STROLL_SPEED, WALK_SPEED};
use shared::sim::{Action, Eliminated, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, PosHistory, Warrior, btn};

use crate::visuals::WarriorVisual;

pub struct WarriorAnimationPlugin;

impl Plugin for WarriorAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Footstep>().add_systems(Startup, load_sets).add_systems(Update, (build_graphs, drive).chain());
    }
}

/// Clips the torso can't share with the legs: played legs-only under a weapon action.
const LOCOMOTION: [&str; 13] =
    ["idle", "walk_f", "walk_b", "walk_l", "walk_r", "run_f", "run_b", "run_l", "run_r", "sprint_f", "crouch_idle", "crouch_f", "slide"];
const FULL_BODY: [&str; 5] = ["jump", "fall", "freefall", "glide", "death"];
/// Locomotion the file may or may not have (Mixamo's, tools/blender/characters/anims.py, built where
/// its downloads are): the crouch's other directions, and whole-body sets for holding a long gun
/// or a pistol. Picked where present (`AnimSet::has`), else the plain ones.
const OPTIONAL_LOCOMOTION: [&str; 31] = [
    "crouch_b", "crouch_l", "crouch_r",
    "rifle_idle", "rifle_crouch_idle", "rifle_sprint_f",
    "rifle_walk_f", "rifle_walk_b", "rifle_walk_l", "rifle_walk_r",
    "rifle_run_f", "rifle_run_b", "rifle_run_l", "rifle_run_r",
    "rifle_crouch_f", "rifle_crouch_b", "rifle_crouch_l", "rifle_crouch_r",
    "pistol_idle", "pistol_crouch_idle",
    "pistol_walk_f", "pistol_walk_b", "pistol_walk_l", "pistol_walk_r",
    "pistol_run_f", "pistol_run_b", "pistol_run_l", "pistol_run_r",
    "pistol_sprint_f", "pistol_crouch_f", "pistol_crouch_b",
];
const UPPER_BODY: [&str; 6] = ["melee_guard", "melee_swing", "melee_swing_b", "bow_draw", "rifle_aim", "consume"];
/// Upper-body actions the file may have (Mixamo's): a pistol's two-handed aim.
const OPTIONAL_UPPER: [&str; 1] = ["pistol_aim"];
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

impl AnimSet {
    fn has(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }

    /// `name` in the held weapon's set (`prefix`: "rifle_", "pistol_" or none) where the file has
    /// it, a pistol's sprint and crouch falling back to its run and the plain crouch.
    fn pick(&self, prefix: &str, name: &'static str) -> &'static str {
        // A pistol is carried low on the run, the arms swinging (the pack's runs hold it up at the
        // chest, a tactical jog that reads stiff): it comes up to aim or fire (the pistol_aim layer).
        if prefix.is_empty() || (prefix == "pistol_" && (name.starts_with("run_") || name.starts_with("sprint_"))) {
            return name;
        }
        let want = format!("{prefix}{name}");
        let found = self.nodes.keys().find(|k| **k == want).copied();
        match (found, name) {
            (Some(n), _) => n,
            (None, "sprint_f") => self.nodes.keys().find(|k| **k == format!("{prefix}run_f")).copied().unwrap_or(name),
            _ => name,
        }
    }
}

impl AnimSets {
    pub fn ready(&self, variant: usize) -> bool {
        self.0[variant].graph.is_some()
    }
}

/// A foot coming down (from the locomotion cycle's contact points), for footstep sounds.
#[derive(Message, Clone, Copy)]
pub struct Footstep {
    /// Whose foot (the warrior's sim entity).
    pub who: Entity,
    /// Where the foot lands (ground level under the warrior).
    pub pos: Vec3,
    pub speed: f32,
    /// Crouched (quiet: no footstep marker on others' screens).
    pub crouched: bool,
}

/// Per-warrior blend state, on the visual entity.
#[derive(Component)]
pub struct WarriorAnim {
    variant: usize,
    player: Entity,
    phase: f32,
    clock: f32,
    weights: HashMap<AnimationNodeIndex, f32>,
    /// Alternate sword cuts: forehand, then the backhand return.
    backhand: bool,
    last_swing: f32,
    /// Seconds left of a hit-stop (the swing holds), and how far the swing has fallen behind.
    pub hit_stop: f32,
    swing_lag: f32,
    /// Weight of the bow draw clip: how far the string follows the drawing hand.
    pub bow_weight: f32,
    /// Seconds since this warrior's pose was last updated (see `due`).
    since_pose: f32,
    /// How much of the aim pitch the torso takes (eased with the held item).
    pub aim_share: f32,
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
        // Wait for every clip the file has to load; one it lacks (an animations file older than
        // the code: the clip came later) stands in with a near one, rather than leaving every
        // warrior unanimated.
        let clip = |n: &str| gltf.named_animations.get(n).filter(|h| clips.get(*h).is_some()).cloned();
        if gltf.named_animations.values().any(|h| clips.get(h).is_none()) {
            continue;
        }
        let stand_in = |n: &str| match n {
            "crouch_idle" => "idle",
            "crouch_f" => "walk_f",
            "slide" | "sprint_f" => "run_f",
            _ => "idle",
        };
        let missing: Vec<&str> = all.iter().copied().filter(|n| clip(n).is_none()).collect();
        if !missing.is_empty() {
            warn!("animations file lacks {missing:?}: standing in with near clips (rebuild with `just anims`, or `git lfs pull`)");
        }
        if missing.iter().any(|n| clip(stand_in(n)).is_none()) {
            error!("animations file lacks {missing:?} and their stand-ins: warriors won't animate");
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
        let present: Vec<&'static str> = OPTIONAL_LOCOMOTION.iter().chain(&OPTIONAL_UPPER).copied().filter(|n| clip(n).is_some()).collect();
        for name in all.into_iter().chain(present) {
            let h = clip(name).or_else(|| clip(stand_in(name))).expect("checked above");
            set.durations.insert(name, clips.get(&h).map(|c| c.duration()).unwrap_or(1.0).max(1e-3));
            let full = graph.add_clip(h.clone(), 1.0, root);
            let masked = if LOCOMOTION.contains(&name) || OPTIONAL_LOCOMOTION.contains(&name) {
                Some(graph.add_clip_with_mask(h, 1 << UPPER, 1.0, root))
            } else if UPPER_BODY.contains(&name) || OPTIONAL_UPPER.contains(&name) {
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
    Some(WarriorAnim {
        variant,
        player: rig_root,
        phase: 0.0,
        clock: 0.0,
        weights: HashMap::new(),
        backhand: false,
        last_swing: f32::MAX,
        hit_stop: 0.0,
        swing_lag: 0.0,
        bow_weight: 0.0,
        since_pose: 0.0,
        aim_share: 0.0,
    })
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
    if speed <= STROLL_SPEED {
        let a = speed / STROLL_SPEED;
        out.push(("idle", 1.0 - a));
        tier(&mut out, "walk", a);
    } else if speed <= WALK_SPEED {
        let b = (speed - STROLL_SPEED) / (WALK_SPEED - STROLL_SPEED);
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

/// How far a held bow is drawn (0 = string at rest, 1 = full draw), or None when it's carried
/// rather than raised. Raised while aiming, charging, or for a moment after a shot; a fast bow
/// redraws over its shot interval, a charged one draws as the charge builds.
pub fn bow_draw(kind: &ItemKind, act: &Action, loaded: bool, aiming: bool) -> Option<f32> {
    let ItemKind::Bow { interval, charge, .. } = kind else { return None };
    if !(aiming || act.since_attack < 1.2 || act.charge > 0.0) || act.reload_left > 0.0 {
        return aiming.then_some(0.0);
    }
    if !loaded {
        return Some(0.0);
    }
    Some(if *charge > 0.0 {
        0.1 + 0.9 * (act.charge / charge).clamp(0.0, 1.0)
    } else {
        ((act.since_attack - 0.04) / (interval * 0.85)).clamp(0.0, 1.0)
    })
}

fn clip_speed(name: &str) -> f32 {
    let name = name.trim_start_matches("rifle_").trim_start_matches("pistol_");
    match name {
        n if n.starts_with("walk") => STROLL_SPEED,
        n if n.starts_with("run") => WALK_SPEED,
        "sprint_f" => SPRINT_SPEED,
        n if n.starts_with("crouch_") && n != "crouch_idle" => CROUCH_SPEED,
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
    sims: Query<(&Kinematic, &PosHistory, &Inventory, &Action, &PlayerIntent, Option<&Eliminated>), With<Warrior>>,
    mut players: Query<&mut AnimationPlayer>,
    mut died: Local<HashMap<Entity, f32>>,
    mut steps: MessageWriter<Footstep>,
    mine: Query<(), With<LocalPlayer>>,
    cam: Query<&GlobalTransform, With<crate::camera::MainCamera>>,
    mut frame: Local<u32>,
) {
    let dt = time.delta_secs();
    *frame = frame.wrapping_add(1);
    let eye = cam.single().ok().map(|g| (g.translation(), g.forward()));
    for (wv, tf, mut anim) in &mut vis {
        let Ok((kin, hist, inv, act, intent, elim)) = sims.get(wv.sim) else { continue };
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
                    // Low (the crouch button held, which every peer sees): sliding while still fast,
                    // else the crouch walk over the crouch idle.
                    let low = intent.held(btn::CROUCH) || kin.sliding;
                    let speed = forward.hypot(right);
                    let loco = if low && speed > CROUCH_SPEED + 1.2 {
                        vec![("slide", 1.0)]
                    } else if low {
                        // The crouch walk in the direction of travel (two nearest by angle), over
                        // the crouch idle; forward only where the file has no others.
                        let a = (speed / CROUCH_SPEED).clamp(0.0, 1.0);
                        let mut out = vec![("crouch_idle", 1.0 - a)];
                        if set.has("crouch_b") && speed > 0.05 {
                            let (f, r) = (forward / speed, right / speed);
                            for (n, w) in [("crouch_f", f.max(0.0)), ("crouch_b", (-f).max(0.0)), ("crouch_l", (-r).max(0.0)), ("crouch_r", r.max(0.0))] {
                                if w * w * a > 1e-3 {
                                    out.push((n, w * w * a));
                                }
                            }
                        } else {
                            out.push(("crouch_f", a));
                        }
                        out
                    } else {
                        locomotion_weights(forward, right)
                    };
                    // Holding a gun: its own whole-body set (the gun held as you move), where the
                    // file has it; a bow, the bat and empty hands move with the plain ones.
                    let prefix = match inv.current() {
                        Some(st) if matches!(db.get(&st.id).kind, ItemKind::Bow { .. })
                            && crate::visuals::shot_kind(&st.id) == crate::visuals::Shot::Bullet =>
                        {
                            if crate::visuals::long_gun(&st.id) { "rifle_" } else { "pistol_" }
                        }
                        _ => "",
                    };
                    let loco: Vec<(&'static str, f32)> = loco.into_iter().map(|(n, w)| (set.pick(prefix, n), w)).collect();
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
                            steps.write(Footstep { who: wv.sim, pos: tf.translation - Vec3::Y * shared::sim::HALF_HEIGHT, speed, crouched: low });
                        }
                    }
                    // Upper-body action from what's in hand. A new swing starts when the time
                    // since the last attack drops; each one alternates forehand and backhand.
                    let held = inv.current().map(|s| &db.get(&s.id).kind);
                    if matches!(held, Some(ItemKind::Melee { .. })) && act.since_attack < anim.last_swing {
                        anim.backhand = !anim.backhand;
                        anim.swing_lag = 0.0;
                    }
                    anim.last_swing = act.since_attack;
                    if anim.hit_stop > 0.0 {
                        anim.hit_stop -= dt;
                        anim.swing_lag += dt;
                    }
                    let swing = if anim.backhand { "melee_swing_b" } else { "melee_swing" };
                    let swung = (act.since_attack - anim.swing_lag).max(0.0);
                    let loaded = inv.current().is_some_and(|s| s.count > 0);
                    let action = match held {
                        Some(ItemKind::Melee { .. }) if swung < SWING_TIME => Some((swing, swung / SWING_TIME * dur(swing))),
                        // On the move or crouched and not fighting, the body's own pose (the arms
                        // swinging with the run, the bat in hand; the crouch's lean, which the guard's
                        // upright back would undo); standing, aiming or just after a swing, the guard.
                        Some(ItemKind::Melee { .. }) if (speed > STROLL_SPEED * 0.6 || low) && act.since_attack > 1.0 && !intent.held(btn::AIM) => None,
                        Some(ItemKind::Melee { .. }) => Some(("melee_guard", clock % dur("melee_guard"))),
                        // A long gun is shouldered; pistols are held up at the start of the draw, both
                        // hands on the gun.
                        Some(k @ ItemKind::Bow { .. }) => {
                            let id = inv.current().map(|s| s.id.as_str()).unwrap_or_default();
                            let gun = crate::visuals::shot_kind(id) == crate::visuals::Shot::Bullet;
                            // Sliding, the gun comes up ready to shoot, as when aiming.
                            let sliding = loco.iter().any(|(n, _)| *n == "slide");
                            bow_draw(k, act, loaded, intent.held(btn::AIM) || (sliding && gun)).map(|d| {
                                if crate::visuals::long_gun(id) {
                                    ("rifle_aim", 0.0)
                                } else if gun && set.has("pistol_aim") {
                                    ("pistol_aim", clock % dur("pistol_aim"))
                                } else {
                                    ("bow_draw", if gun { 0.0 } else { d * (dur("bow_draw") - 1e-3) })
                                }
                            })
                        }
                        Some(ItemKind::Consumable { .. }) if act.using.is_some() => Some(("consume", clock % dur("consume"))),
                        _ => None,
                    };
                    let phase = anim.phase;
                    // Clips that would barely show are left out (each costs a pass over the bones).
                    for (n, w) in loco.into_iter().filter(|(_, w)| *w >= 0.05) {
                        let t = if clip_speed(n) == 0.0 { clock % dur(n) } else { phase * dur(n) };
                        want.push((n, action.is_some(), w, t));
                    }
                    if let Some((n, t)) = action {
                        want.push((n, true, 1.0, t));
                    }
                }
            }
        }

        // The torso follows the aim fully with a bow up, partly otherwise.
        let raised = want.iter().any(|w| w.0 == "bow_draw" || w.0 == "rifle_aim" || w.0 == "pistol_aim");
        let share = if elim.is_some() { 0.0 } else if raised { 1.0 } else { 0.45 };
        anim.aim_share += (share - anim.aim_share) * (dt * 8.0).min(1.0);

        // An animation budget: far warriors (and those behind the camera) are posed every few
        // frames, staggered; between times their clips are stopped, so the bones keep their last
        // pose and cost nothing to evaluate.
        let Ok(mut player) = players.get_mut(anim.player) else { continue };
        anim.since_pose += dt;
        let every = match eye {
            _ if mine.contains(wv.sim) => 1,
            Some((at, fwd)) => {
                let to = tf.translation - at;
                let d = to.length();
                if d > 6.0 && to.dot(*fwd) < -0.2 * d {
                    8
                } else if d < 20.0 {
                    1
                } else if d < 45.0 {
                    2
                } else if d < 90.0 {
                    3
                } else {
                    6
                }
            }
            None => 1,
        };
        if (*frame).wrapping_add(wv.sim.index_u32()) % every != 0 {
            if !player.all_finished() {
                player.stop_all();
            }
            continue;
        }
        let pose_dt = std::mem::take(&mut anim.since_pose);
        let k = (pose_dt * FADE_RATE).min(1.0);
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
        anim.bow_weight = 0.0;
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
            if ["bow_draw", "rifle_aim", "pistol_aim"].iter().any(|c| set.nodes.get(c).is_some_and(|n| n.1 == Some(node))) {
                anim.bow_weight = anim.bow_weight.max(w);
            }
            let a = if player.is_playing_animation(node) { player.animation_mut(node).unwrap() } else { player.play(node) };
            a.set_speed(0.0).set_weight(w);
            // Fading-out nodes keep their last time.
            if t >= 0.0 {
                a.seek_to(t);
            }
        }
    }
}
