//! Pose fixes: corrections made by eye, kept as data and laid over the animation.
//!
//! - What a fix is: a body part moved by an offset (in the character's model space: +Z forward,
//!   +Y up), for a clip, an arms action, or the pair. `assets/data/pose_fixes.ron` holds them.
//!   Every warrior gets them in every build, eased in and out with what it's playing
//!   (`WarriorAnim::showing`), so a fix made once is there every time that movement plays.
//! - Making one (desktop, `just posefix` or VIVAD_POSEFIX=1): F8 stops time and frees the
//!   pointer. Drag a dot to move that part; the file is written when the drag ends. F8 again
//!   carries on. F7 is slow motion and `.` steps a frame, to land on the moment that looks wrong.
//! - Where it runs: last in the pose chain, after the clips, the torso's aim
//!   (`arms::pose_torso`) and the weapon and left-hand placement (`arms::orient_bows`), on the
//!   propagated global transforms as those do. A fix is the final word on the pose.
//!
//! Limbs follow a moved hand or foot (two-bone reach, the elbow or knee keeping its side); moved
//! hips keep the feet where they stood; `Hands` moves the right hand with whatever it holds and
//! takes the left hand along in the same place on it.

use std::collections::{BTreeMap, HashMap};

use bevy::gizmos::config::{GizmoConfig, GizmoConfigGroup, GizmoLineConfig};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use serde::{Deserialize, Serialize};
use shared::sim::LocalPlayer;

use crate::animation::WarriorAnim;
use crate::arms::{Body, reach, turn_subtree};
use crate::camera::MainCamera;
use crate::visuals::WarriorVisual;

/// How fast a fix comes in and goes out as the clips change (per second), about the clips' own
/// cross-fade.
const BLEND: f32 = 10.0;
/// A dot is picked within this many pixels of the pointer.
const PICK: f32 = 28.0;
/// Slow motion's speed (F7).
const SLOW: f32 = 0.2;

pub struct PoseFixPlugin;

impl Plugin for PoseFixPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PoseFixes::load())
            .insert_resource(FixMode::new())
            // The dots show through the body they sit in.
            .insert_gizmo_config(FixDots, GizmoConfig { depth_bias: -1.0, line: GizmoLineConfig { width: 3.0, ..default() }, ..default() })
            .add_systems(Update, (keys, panel).chain())
            .add_systems(
                PostUpdate,
                (
                    orbit.after(crate::camera::CameraSystems).before(TransformSystems::Propagate),
                    fresh.before(TransformSystems::Propagate),
                    (apply, edit).chain().after(TransformSystems::Propagate).after(crate::arms::HandsPosed),
                ),
            );
    }
}

/// The parts a fix can move.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Part {
    Hips,
    Chest,
    Head,
    /// The right hand with what it holds, the left hand kept in its place on it.
    Hands,
    HandL,
    HandR,
    FootL,
    FootR,
}

impl Part {
    fn label(self) -> &'static str {
        match self {
            Part::Hips => "hips",
            Part::Chest => "chest",
            Part::Head => "head",
            Part::Hands => "both hands",
            Part::HandL => "left hand",
            Part::HandR => "right hand",
            Part::FootL => "left foot",
            Part::FootR => "right foot",
        }
    }
}

/// One fix: where it applies and how far each part moves (metres, model space).
/// `clip` is the whole-body or legs clip showing and `action` the arms' action over it, as
/// `WarriorAnim::showing` names them; "*" is any.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    pub clip: String,
    pub action: String,
    pub parts: BTreeMap<Part, (f32, f32, f32)>,
}

impl Fix {
    fn fits(&self, showing: (&str, &str)) -> bool {
        (self.clip == "*" || self.clip == showing.0) && (self.action == "*" || self.action == showing.1)
    }
}

/// Every fix, as loaded from and saved to `assets/data/pose_fixes.ron`.
#[derive(Resource, Default)]
pub struct PoseFixes {
    fixes: Vec<Fix>,
    /// Counts fixes added and removed, so the eased weights (by index) start over.
    rev: u32,
}

impl PoseFixes {
    /// The file built into the binary, or on the desktop the one on disk now (edited since the
    /// build, perhaps).
    fn load() -> Self {
        #[allow(unused_mut)]
        let mut text = include_str!("../../../assets/data/pose_fixes.ron").to_string();
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(t) = std::fs::read_to_string(path()) {
            text = t;
        }
        match ron::from_str::<Vec<Fix>>(&text) {
            Ok(fixes) => Self { fixes, rev: 0 },
            Err(e) => {
                warn!("[posefix] assets/data/pose_fixes.ron: {e} (no fixes applied)");
                Self::default()
            }
        }
    }

    /// The file's text: fixes with nothing left in them dropped, numbers to a tenth of a millimetre.
    fn text(&self) -> String {
        let fixes: Vec<Fix> = self
            .fixes
            .iter()
            .filter(|f| !f.parts.is_empty())
            .map(|f| {
                let r = |v: f32| (v * 1e4).round() / 1e4;
                Fix { clip: f.clip.clone(), action: f.action.clone(), parts: f.parts.iter().map(|(p, v)| (*p, (r(v.0), r(v.1), r(v.2)))).collect() }
            })
            .collect();
        let body = ron::ser::to_string_pretty(&fixes, ron::ser::PrettyConfig::default()).unwrap_or_else(|_| "[]".into());
        format!("{HEADER}{body}\n")
    }

    /// Write the file (desktop only). Says where, or why not.
    fn save(&self) -> String {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match std::fs::write(path(), self.text()) {
                Ok(()) => "saved to assets/data/pose_fixes.ron".into(),
                Err(e) => format!("can't write assets/data/pose_fixes.ron: {e}"),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            "not saved: the browser can't write the project's files".into()
        }
    }

    /// The fix for exactly this clip and action, made if there isn't one.
    fn entry(&mut self, clip: &str, action: &str) -> &mut Fix {
        if let Some(i) = self.fixes.iter().position(|f| f.clip == clip && f.action == action) {
            return &mut self.fixes[i];
        }
        self.rev += 1;
        self.fixes.push(Fix { clip: clip.into(), action: action.into(), parts: BTreeMap::new() });
        self.fixes.last_mut().unwrap()
    }
}

const HEADER: &str = "// Pose fixes: body parts moved by eye for a clip, an arms action, or the pair (\"*\" is any), in\n\
// metres in the character's model space (+X its left, +Y up, +Z forward). Written by the game's\n\
// fix mode (crates/game/src/posefix.rs, `just posefix`); edit by hand if you like.\n";

#[cfg(not(target_arch = "wasm32"))]
fn path() -> String {
    format!("{}/data/pose_fixes.ron", crate::ASSET_DIR)
}

/// What a new fix is saved for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    /// This clip under this action.
    Pair,
    /// This clip, whatever the arms do.
    Clip,
    /// This action, whatever the legs do.
    Action,
}

impl Scope {
    fn key(self, showing: (&str, &str)) -> (String, String) {
        match self {
            Scope::Pair => (showing.0.into(), showing.1.into()),
            Scope::Clip => (showing.0.into(), "*".into()),
            Scope::Action => ("*".into(), showing.1.into()),
        }
    }
}

/// The fixing tool's state.
#[derive(Resource)]
pub struct FixMode {
    /// The tool is there at all (VIVAD_POSEFIX, desktop).
    enabled: bool,
    /// Time is stopped and the dots are out.
    pub active: bool,
    scope: Scope,
    /// What the local player is showing (clip, action), for the panel and the keys.
    showing: (String, String),
    hover: Option<Part>,
    held: Option<Part>,
    /// Where the pointer's ray last met the drag plane.
    grab_at: Vec3,
    /// The view round the player while fixing: yaw, pitch, distance.
    view: Option<(f32, f32, f32)>,
    slow: bool,
    /// Frames left of a single step.
    step: u8,
    dirty: bool,
    note: String,
}

impl FixMode {
    fn new() -> Self {
        Self {
            enabled: cfg!(not(target_arch = "wasm32")) && std::env::var("VIVAD_POSEFIX").is_ok(),
            active: false,
            scope: Scope::Pair,
            showing: default(),
            hover: None,
            held: None,
            grab_at: Vec3::ZERO,
            view: None,
            slow: false,
            step: 0,
            dirty: false,
            note: String::new(),
        }
    }
}

/// True while the game is being played, not fixed (the pointer and keys are the player's).
pub fn playing(mode: Option<Res<FixMode>>) -> bool {
    !mode.is_some_and(|m| m.active)
}

/// The bones a fix moves, found when the character's rig loads (crate::characters).
#[derive(Component)]
pub struct FixRig {
    pelvis: Entity,
    /// spine_01 (the waist: the chest turns about it), spine_03, neck_01, head.
    waist: Entity,
    chest: Entity,
    neck: Entity,
    head: Entity,
    /// Upper, lower, end: upperarm, lowerarm, hand; thigh, calf, foot.
    arm_l: [Entity; 3],
    arm_r: [Entity; 3],
    leg_l: [Entity; 3],
    leg_r: [Entity; 3],
}

impl FixRig {
    pub fn find(named: &HashMap<String, Entity>) -> Option<Self> {
        let b = |n: &str| named.get(n).copied();
        let limb = |a: &str, m: &str, e: &str, s: &str| Some([b(&format!("{a}_{s}"))?, b(&format!("{m}_{s}"))?, b(&format!("{e}_{s}"))?]);
        Some(Self {
            pelvis: b("pelvis")?,
            waist: b("spine_01")?,
            chest: b("spine_03")?,
            neck: b("neck_01")?,
            head: b("head")?,
            arm_l: limb("upperarm", "lowerarm", "hand", "l")?,
            arm_r: limb("upperarm", "lowerarm", "hand", "r")?,
            leg_l: limb("thigh", "calf", "foot", "l")?,
            leg_r: limb("thigh", "calf", "foot", "r")?,
        })
    }
}

type Globals<'w, 's, 'a> = Query<'w, 's, (&'a Transform, &'a mut GlobalTransform)>;

fn glob(tf: &Globals, e: Entity) -> Option<GlobalTransform> {
    tf.get(e).ok().map(|(_, g)| *g)
}

fn at(tf: &Globals, e: Entity) -> Option<Vec3> {
    glob(tf, e).map(|g| g.translation())
}

/// Move `e` and everything under it by `d`, in their (already propagated) global transforms.
fn shift_subtree(e: Entity, d: Vec3, kids: &Query<&Children>, tf: &mut Globals) {
    if let Ok((_, mut g)) = tf.get_mut(e) {
        let (scale, rotation, t) = g.to_scale_rotation_translation();
        *g = GlobalTransform::from(Transform { translation: t + d, rotation, scale });
    }
    if let Ok(c) = kids.get(e) {
        for k in c.iter() {
            shift_subtree(k, d, kids, tf);
        }
    }
}

/// Bend a limb so its end lands `d` from where it is (or back where it was: `to`), its middle
/// joint keeping the side it bends to and its end the way it faces.
fn move_end(limb: [Entity; 3], to: Vec3, facing: Quat, kids: &Query<&Children>, tf: &mut Globals) {
    let (Some(root), Some(mid)) = (at(tf, limb[0]), at(tf, limb[1])) else { return };
    reach(limb[0], limb[1], limb[2], to, mid - root, facing, kids, tf);
}

/// Turn everything from `bone` up about `pivot` so the point `top` moves by `d`.
fn swing(bone: Entity, pivot: Vec3, top: Vec3, d: Vec3, kids: &Query<&Children>, tf: &mut Globals) {
    let (Some(a), Some(b)) = ((top - pivot).try_normalize(), (top + d - pivot).try_normalize()) else { return };
    turn_subtree(bone, pivot, Quat::from_rotation_arc(a, b), kids, tf);
}

/// Lay the summed offsets (model space, by `Part`) over one posed character.
fn pose(rig: &FixRig, root: Entity, total: &[Vec3; 8], kids: &Query<&Children>, tf: &mut Globals) {
    let Some(model) = glob(tf, root).map(|g| g.affine()) else { return };
    let world = |p: Part| {
        let v = total[p as usize];
        (v != Vec3::ZERO).then(|| model.transform_vector3(v))
    };
    let up = model.transform_vector3(Vec3::Y).normalize_or(Vec3::Y);
    // The feet as they stand, so lowered hips bend the knees and don't sink the feet.
    let feet = [rig.leg_l, rig.leg_r].map(|l| glob(tf, l[2]).map(|g| (g.translation(), g.rotation())));
    let hips = world(Part::Hips);
    if let Some(d) = hips {
        shift_subtree(rig.pelvis, d, kids, tf);
    }
    if let (Some(d), Some(pivot), Some(top)) = (world(Part::Chest), at(tf, rig.waist), at(tf, rig.neck)) {
        swing(rig.waist, pivot, top, d, kids, tf);
    }
    if let (Some(d), Some(pivot)) = (world(Part::Head), at(tf, rig.neck)) {
        if let Some(top) = at(tf, rig.head).map(|h| h + up * HEAD_TOP) {
            swing(rig.neck, pivot, top, d, kids, tf);
        }
    }
    for (leg, foot, part) in [(rig.leg_l, feet[0], Part::FootL), (rig.leg_r, feet[1], Part::FootR)] {
        let d = world(part);
        if let (true, Some((p, facing))) = (hips.is_some() || d.is_some(), foot) {
            move_end(leg, p + d.unwrap_or(Vec3::ZERO), facing, kids, tf);
        }
    }
    if let (Some(d), Some(right), Some(left)) = (world(Part::Hands), glob(tf, rig.arm_r[2]), glob(tf, rig.arm_l[2])) {
        // The left hand's place on the right (and so on what the right holds), kept through the move.
        let hold = right.affine().inverse() * left.affine();
        move_end(rig.arm_r, right.translation() + d, right.rotation(), kids, tf);
        if let Some(now) = glob(tf, rig.arm_r[2]) {
            let (_, facing, p) = GlobalTransform::from(now.affine() * hold).to_scale_rotation_translation();
            move_end(rig.arm_l, p, facing, kids, tf);
        }
    }
    for (arm, part) in [(rig.arm_r, Part::HandR), (rig.arm_l, Part::HandL)] {
        if let (Some(d), Some(hand)) = (world(part), glob(tf, arm[2])) {
            move_end(arm, hand.translation() + d, hand.rotation(), kids, tf);
        }
    }
}

/// Bevy only recomputes the global transforms of what moved. A fix is an offset added to them
/// after propagation, so on a frame the clips didn't rewrite the bones (a far warrior's pose is
/// refreshed every few frames; time stopped) it would be added to its own result. Marking the
/// model's root as moved has the whole skeleton propagated afresh each frame.
fn fresh(fixes: Res<PoseFixes>, vis: Query<&Body, With<FixRig>>, mut tf: Query<&mut Transform>) {
    if fixes.fixes.is_empty() {
        return;
    }
    for body in &vis {
        if let Ok(mut t) = tf.get_mut(body.root) {
            t.set_changed();
        }
    }
}

/// The head's dot (and the point a head fix moves) sits this far over the head bone.
const HEAD_TOP: f32 = 0.1;

/// Each warrior's fixes for what it's showing, eased, laid over its pose.
#[allow(clippy::type_complexity)]
fn apply(
    time: Res<Time>,
    fixes: Res<PoseFixes>,
    mode: Res<FixMode>,
    local: Query<(), With<LocalPlayer>>,
    vis: Query<(Entity, &WarriorVisual, &WarriorAnim, &Body, &FixRig)>,
    kids: Query<&Children>,
    mut tf: Globals,
    mut eased: Local<(u32, HashMap<Entity, Vec<f32>>)>,
) {
    if fixes.fixes.is_empty() {
        return;
    }
    if eased.0 != fixes.rev {
        *eased = (fixes.rev, HashMap::new());
    }
    let k = 1.0 - (-time.delta_secs() * BLEND).exp();
    let mut seen = 0;
    for (e, wv, anim, body, rig) in &vis {
        seen += 1;
        let weights = eased.1.entry(e).or_insert_with(|| vec![0.0; fixes.fixes.len()]);
        weights.resize(fixes.fixes.len(), 0.0);
        // Being fixed, the player's own pose shows its fixes whole: time is stopped, nothing eases.
        let whole = mode.active && local.contains(wv.sim);
        let mut total = [Vec3::ZERO; 8];
        for (f, w) in fixes.fixes.iter().zip(weights.iter_mut()) {
            let want = if f.fits(anim.showing) { 1.0 } else { 0.0 };
            *w = if whole { want } else { *w + (want - *w) * k };
            if *w > 1e-3 {
                for (part, v) in &f.parts {
                    total[*part as usize] += Vec3::new(v.0, v.1, v.2) * *w;
                }
            }
        }
        if total.iter().any(|v| *v != Vec3::ZERO) {
            pose(rig, body.root, &total, &kids, &mut tf);
        }
    }
    if eased.1.len() > seen {
        eased.1.retain(|e, _| vis.contains(*e));
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
struct FixDots;

/// The dots on the local player, picking one with the pointer, and dragging it: the pointer's
/// move across the screen is the part's move in the plane facing the camera.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn edit(
    mut mode: ResMut<FixMode>,
    mut fixes: ResMut<PoseFixes>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    local: Query<Entity, With<LocalPlayer>>,
    vis: Query<(&WarriorVisual, &WarriorAnim, &Body, &FixRig)>,
    tf: Query<&GlobalTransform>,
    mut dots: Gizmos<FixDots>,
) {
    let Ok(me) = local.single() else { return };
    let Some((_, anim, body, rig)) = vis.iter().find(|(wv, ..)| wv.sim == me) else { return };
    if mode.showing.0 != anim.showing.0 || mode.showing.1 != anim.showing.1 {
        mode.showing = (anim.showing.0.into(), anim.showing.1.into());
    }
    if !mode.active {
        return;
    }
    let (Ok((camera, cam_g)), Ok(model)) = (cam.single(), tf.get(body.root)) else { return };
    let p = |e: Entity| tf.get(e).map(|g| g.translation()).ok();
    let up = model.affine().transform_vector3(Vec3::Y).normalize_or(Vec3::Y);
    let mut handles: Vec<(Part, Vec3)> = Vec::new();
    let mut put = |part: Part, at: Option<Vec3>| {
        if let Some(at) = at {
            handles.push((part, at));
        }
    };
    put(Part::Hips, p(rig.pelvis));
    put(Part::Chest, p(rig.chest));
    put(Part::Head, p(rig.head).map(|h| h + up * HEAD_TOP));
    let (l, r) = (p(rig.arm_l[2]), p(rig.arm_r[2]));
    put(Part::HandL, l);
    put(Part::HandR, r);
    // Between the hands and a little over them, clear of both.
    put(Part::Hands, l.zip(r).map(|(l, r)| (l + r) * 0.5 + up * 0.12));
    put(Part::FootL, p(rig.leg_l[2]));
    put(Part::FootR, p(rig.leg_r[2]));

    let cursor = window.single().ok().and_then(|w| w.cursor_position());
    if mode.held.is_none() {
        mode.hover = cursor.and_then(|c| {
            handles
                .iter()
                .filter_map(|(part, at)| camera.world_to_viewport(cam_g, *at).ok().map(|s| (*part, s.distance(c))))
                .filter(|(_, d)| *d < PICK)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(part, _)| part)
        });
        if mouse.just_pressed(MouseButton::Left) {
            if let Some((part, at)) = mode.hover.and_then(|h| handles.iter().find(|(part, _)| *part == h).copied()) {
                mode.held = Some(part);
                mode.grab_at = at;
            }
        }
    } else if !mouse.pressed(MouseButton::Left) {
        mode.held = None;
        if mode.dirty {
            mode.dirty = false;
            mode.note = fixes.save();
        }
    }
    if let (Some(part), Some(c)) = (mode.held, cursor) {
        if let Ok(ray) = camera.viewport_to_world(cam_g, c) {
            let n = *cam_g.forward();
            let along = ray.direction.dot(n);
            if along.abs() > 1e-4 {
                let hit = ray.origin + *ray.direction * ((mode.grab_at - ray.origin).dot(n) / along);
                let d = model.affine().inverse().transform_vector3(hit - mode.grab_at);
                mode.grab_at = hit;
                if d != Vec3::ZERO {
                    let (clip, action) = mode.scope.key((anim.showing.0, anim.showing.1));
                    let v = fixes.entry(&clip, &action).parts.entry(part).or_insert((0.0, 0.0, 0.0));
                    *v = (v.0 + d.x, v.1 + d.y, v.2 + d.z);
                    mode.dirty = true;
                }
            }
        }
    }
    // The same size on screen at any distance.
    let eye = cam_g.translation();
    for (part, at) in handles {
        let (r, colour) = if mode.held == Some(part) {
            (0.022, Color::srgb(1.0, 0.55, 0.15))
        } else if mode.hover == Some(part) {
            (0.022, Color::srgb(1.0, 0.92, 0.3))
        } else {
            (0.016, Color::srgb(0.95, 0.97, 1.0))
        };
        dots.sphere(Isometry3d::from_translation(at), r * eye.distance(at), colour);
    }
}

/// F8 in and out of fixing, F7 slow motion, and while fixing: Tab what a fix is saved for, `.` a
/// frame on, Backspace the fix for this scope gone.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<FixMode>,
    mut fixes: ResMut<PoseFixes>,
    mut virt: ResMut<Time<Virtual>>,
    mut cursor: Query<&mut CursorOptions>,
) {
    if !mode.enabled {
        return;
    }
    if mode.step > 0 {
        mode.step -= 1;
        if mode.step == 0 && mode.active {
            virt.pause();
        }
    }
    if keys.just_pressed(KeyCode::F7) {
        mode.slow = !mode.slow;
        virt.set_relative_speed(if mode.slow { SLOW } else { 1.0 });
    }
    if keys.just_pressed(KeyCode::F8) {
        mode.active = !mode.active;
        mode.held = None;
        mode.hover = None;
        mode.step = 0;
        if mode.active {
            virt.pause();
            mode.view = None;
            mode.note.clear();
            if let Ok(mut c) = cursor.single_mut() {
                c.grab_mode = CursorGrabMode::None;
                c.visible = true;
            }
        } else {
            virt.unpause();
            if mode.dirty {
                mode.dirty = false;
                mode.note = fixes.save();
            }
        }
    }
    if !mode.active {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        mode.scope = match mode.scope {
            Scope::Pair => Scope::Clip,
            Scope::Clip => Scope::Action,
            Scope::Action => Scope::Pair,
        };
    }
    if keys.just_pressed(KeyCode::Period) {
        // One rendered frame of time, then stopped again.
        virt.unpause();
        mode.step = 1;
    }
    if keys.just_pressed(KeyCode::Backspace) {
        let (clip, action) = mode.scope.key((&mode.showing.0, &mode.showing.1));
        let before = fixes.fixes.len();
        fixes.fixes.retain(|f| !(f.clip == clip && f.action == action));
        if fixes.fixes.len() != before {
            fixes.rev += 1;
            mode.note = fixes.save();
        }
    }
}

#[derive(Component)]
struct Panel;

/// The tool's words on screen: one line while playing, the full panel while fixing.
fn panel(mut commands: Commands, mode: Res<FixMode>, fixes: Res<PoseFixes>, mut text: Query<&mut Text, With<Panel>>) {
    if !mode.enabled {
        return;
    }
    let Ok(mut text) = text.single_mut() else {
        commands.spawn((
            Panel,
            Text::new(""),
            TextFont { font_size: bevy::text::FontSize::Px(18.0), ..default() },
            TextColor(Color::srgb(1.0, 0.95, 0.75)),
            bevy::ui::widget::TextShadow::default(),
            Node { position_type: PositionType::Absolute, top: px(150.0), left: px(24.0), ..default() },
        ));
        return;
    };
    let name = |s: &str| if s.is_empty() { "none".to_string() } else { s.to_string() };
    let showing = (mode.showing.0.as_str(), mode.showing.1.as_str());
    let words = if !mode.active {
        format!("F8 fix pose   F7 slow motion{}", if mode.slow { " (on)" } else { "" })
    } else {
        let (clip, action) = mode.scope.key(showing);
        let saved = fixes
            .fixes
            .iter()
            .find(|f| f.clip == clip && f.action == action)
            .map(|f| f.parts.iter().map(|(p, v)| format!("{} ({:+.2}, {:+.2}, {:+.2})", p.label(), v.0, v.1, v.2)).collect::<Vec<_>>().join(", "))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "nothing yet".into());
        let others = fixes.fixes.iter().filter(|f| f.fits(showing) && !(f.clip == clip && f.action == action) && !f.parts.is_empty()).count();
        format!(
            "FIXING POSE (time stopped)\n\
             showing: clip {}, arms {}\n\
             saving for: clip {}, arms {}   [Tab changes]\n\
             moved here: {}{}\n\
             {}\n\
             drag a dot to move that part   right-drag or arrows: look round   wheel: closer\n\
             . one frame on   Backspace: clear this   F7 slow motion   F8 done\n\
             {}",
            name(showing.0),
            name(showing.1),
            name(&clip),
            name(&action),
            saved,
            if others > 0 { format!("   (+{others} other fix{} also applying)", if others == 1 { "" } else { "es" }) } else { String::new() },
            mode.held.or(mode.hover).map(|p| format!("> {}", p.label())).unwrap_or_default(),
            mode.note,
        )
    };
    if text.0 != words {
        text.0 = words;
    }
}

/// While fixing, the view circles the player (the game's camera follows the aim, which is the
/// player's to keep): right-drag or the arrow keys turn it, the wheel moves it in and out.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn orbit(
    mut mode: ResMut<FixMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    real: Res<Time<Real>>,
    local: Query<Entity, With<LocalPlayer>>,
    visuals: Query<(&WarriorVisual, &Transform), Without<MainCamera>>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    if !mode.active {
        return;
    }
    let (Ok(me), Ok(mut ct)) = (local.single(), cam.single_mut()) else { return };
    let Some(pivot) = visuals.iter().find(|(v, _)| v.sim == me).map(|(_, t)| t.translation) else { return };
    let (mut yaw, mut pitch, mut dist) = mode.view.unwrap_or_else(|| {
        // Start from where the game's camera stands.
        let off = ct.translation - pivot;
        let d = off.length().max(0.5);
        (off.x.atan2(off.z), (off.y / d).clamp(-1.0, 1.0).asin(), d.clamp(1.5, 6.0))
    });
    if mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) {
        yaw -= motion.delta.x * 0.006;
        pitch += motion.delta.y * 0.006;
    }
    let dt = real.delta_secs();
    let turn = |a: KeyCode, b: KeyCode| (keys.pressed(a) as i32 - keys.pressed(b) as i32) as f32 * 1.6 * dt;
    yaw += turn(KeyCode::ArrowRight, KeyCode::ArrowLeft);
    pitch += turn(KeyCode::ArrowUp, KeyCode::ArrowDown);
    pitch = pitch.clamp(-1.2, 1.45);
    dist = (dist * (1.0 - scroll.delta.y * 0.08)).clamp(0.8, 12.0);
    mode.view = Some((yaw, pitch, dist));
    let off = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()) * dist;
    *ct = Transform::from_translation(pivot + off).looking_at(pivot, Vec3::Y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn fix(clip: &str, action: &str) -> Fix {
        Fix { clip: clip.into(), action: action.into(), parts: BTreeMap::from([(Part::Hands, (0.0, 0.1, 0.0))]) }
    }

    #[test]
    fn a_fix_fits_its_clip_its_action_or_any() {
        assert!(fix("rifle_slide", "rifle_aim").fits(("rifle_slide", "rifle_aim")));
        assert!(!fix("rifle_slide", "rifle_aim").fits(("rifle_slide", "")));
        assert!(fix("rifle_slide", "*").fits(("rifle_slide", "")));
        assert!(fix("*", "rifle_aim").fits(("run_f", "rifle_aim")));
        assert!(!fix("*", "rifle_aim").fits(("run_f", "bow_draw")));
        assert_eq!(Scope::Clip.key(("slide", "consume")), ("slide".to_string(), "*".to_string()));
        assert_eq!(Scope::Action.key(("slide", "consume")), ("*".to_string(), "consume".to_string()));
    }

    #[test]
    fn the_file_reads_back_as_written() {
        let mut all = PoseFixes::default();
        all.entry("rifle_slide", "*").parts.insert(Part::Hands, (0.012345, 0.1, -0.02));
        all.entry("idle", "pistol_aim").parts.insert(Part::Head, (0.0, 0.0, 0.03));
        all.entry("run_f", "").parts.clear();
        let text = all.text();
        let back: Vec<Fix> = ron::from_str(&text).expect(&text);
        assert_eq!(back.len(), 2, "an empty fix is not written");
        assert_eq!(back[0].parts[&Part::Hands], (0.0123, 0.1, -0.02));
        assert_eq!(back[1], Fix { clip: "idle".into(), action: "pistol_aim".into(), parts: BTreeMap::from([(Part::Head, (0.0, 0.0, 0.03))]) });
        // And the file in the repo parses.
        ron::from_str::<Vec<Fix>>(include_str!("../../../assets/data/pose_fixes.ron")).expect("assets/data/pose_fixes.ron");
    }

    /// A stick figure with propagated globals: pelvis, spine to head, two arms, two legs, the
    /// arms bent forward as if on a gun, the knees bent a little.
    fn figure(world: &mut World) -> (FixRig, Entity) {
        let mut bone = |world: &mut World, parent: Option<Entity>, at: Vec3| {
            let t = Transform::from_translation(at);
            let e = world.spawn((t, GlobalTransform::from(t))).id();
            if let Some(p) = parent {
                world.entity_mut(e).insert(ChildOf(p));
            }
            e
        };
        let root = bone(world, None, Vec3::ZERO);
        let pelvis = bone(world, Some(root), Vec3::new(0.0, 0.95, 0.0));
        let waist = bone(world, Some(pelvis), Vec3::new(0.0, 1.05, 0.0));
        let chest = bone(world, Some(waist), Vec3::new(0.0, 1.3, 0.0));
        let neck = bone(world, Some(chest), Vec3::new(0.0, 1.5, 0.0));
        let head = bone(world, Some(neck), Vec3::new(0.0, 1.6, 0.0));
        let mut limb = |world: &mut World, parent: Entity, a: Vec3, b: Vec3, c: Vec3| {
            let a = bone(world, Some(parent), a);
            let b = bone(world, Some(a), b);
            [a, b, bone(world, Some(b), c)]
        };
        // +X is the character's left.
        let arm_l = limb(world, chest, Vec3::new(0.2, 1.45, 0.0), Vec3::new(0.25, 1.25, 0.2), Vec3::new(0.05, 1.3, 0.45));
        let arm_r = limb(world, chest, Vec3::new(-0.2, 1.45, 0.0), Vec3::new(-0.25, 1.2, 0.1), Vec3::new(-0.1, 1.25, 0.3));
        let leg_l = limb(world, pelvis, Vec3::new(0.1, 0.9, 0.0), Vec3::new(0.1, 0.5, 0.08), Vec3::new(0.1, 0.08, 0.0));
        let leg_r = limb(world, pelvis, Vec3::new(-0.1, 0.9, 0.0), Vec3::new(-0.1, 0.5, 0.08), Vec3::new(-0.1, 0.08, 0.0));
        (FixRig { pelvis, waist, chest, neck, head, arm_l, arm_r, leg_l, leg_r }, root)
    }

    /// Pose the figure with `total` and hand back where the named bones ended up.
    fn posed(part: Part, by: Vec3) -> (HashMap<&'static str, GlobalTransform>, HashMap<&'static str, GlobalTransform>) {
        let mut world = World::new();
        let (rig, root) = figure(&mut world);
        let named = [("pelvis", rig.pelvis), ("head", rig.head), ("hand_l", rig.arm_l[2]), ("hand_r", rig.arm_r[2]), ("elbow_r", rig.arm_r[1]), ("foot_l", rig.leg_l[2]),
            ("foot_r", rig.leg_r[2]), ("knee_l", rig.leg_l[1]), ("neck", rig.neck)];
        let read = |world: &World| named.iter().map(|(n, e)| (*n, *world.get::<GlobalTransform>(*e).unwrap())).collect::<HashMap<_, _>>();
        let before = read(&world);
        let mut total = [Vec3::ZERO; 8];
        total[part as usize] = by;
        world.run_system_once(move |kids: Query<&Children>, mut tf: Globals| pose(&rig, root, &total, &kids, &mut tf)).unwrap();
        (before, read(&world))
    }

    fn near(a: Vec3, b: Vec3, what: &str) {
        assert!(a.distance(b) < 1e-3, "{what}: {a} vs {b}");
    }

    #[test]
    fn a_hand_goes_where_it_is_dragged_and_keeps_its_facing() {
        let by = Vec3::new(0.03, 0.08, 0.05);
        let (was, now) = posed(Part::HandR, by);
        near(now["hand_r"].translation(), was["hand_r"].translation() + by, "right hand");
        assert!(now["hand_r"].rotation().angle_between(was["hand_r"].rotation()) < 1e-3);
        near(now["hand_l"].translation(), was["hand_l"].translation(), "left hand untouched");
        // The elbow still bends to the same side of the shoulder-to-wrist line (outward: -X).
        assert!(now["elbow_r"].translation().x < now["hand_r"].translation().x);
    }

    #[test]
    fn both_hands_move_as_one() {
        let by = Vec3::new(0.0, 0.1, 0.04);
        let (was, now) = posed(Part::Hands, by);
        near(now["hand_r"].translation(), was["hand_r"].translation() + by, "right hand");
        let hold = |m: &HashMap<&str, GlobalTransform>| m["hand_r"].affine().inverse() * m["hand_l"].affine();
        near(hold(&now).translation.into(), hold(&was).translation.into(), "the left hand's place on the right");
    }

    #[test]
    fn lowered_hips_bend_the_knees_and_leave_the_feet() {
        let by = Vec3::new(0.0, -0.12, 0.0);
        let (was, now) = posed(Part::Hips, by);
        near(now["pelvis"].translation(), was["pelvis"].translation() + by, "pelvis");
        near(now["head"].translation(), was["head"].translation() + by, "the body comes down with it");
        near(now["foot_l"].translation(), was["foot_l"].translation(), "left foot");
        near(now["foot_r"].translation(), was["foot_r"].translation(), "right foot");
        assert!(now["knee_l"].translation().z > was["knee_l"].translation().z, "the knee goes forward as it bends");
    }

    #[test]
    fn a_foot_moves_alone_and_the_head_nods_about_the_neck() {
        let (was, now) = posed(Part::FootL, Vec3::new(0.0, 0.0, 0.15));
        near(now["foot_l"].translation(), was["foot_l"].translation() + Vec3::new(0.0, 0.0, 0.15), "left foot");
        near(now["foot_r"].translation(), was["foot_r"].translation(), "right foot");
        let (was, now) = posed(Part::Head, Vec3::new(0.0, 0.0, 0.05));
        near(now["neck"].translation(), was["neck"].translation(), "the neck is the pivot");
        assert!(now["head"].translation().z > was["head"].translation().z + 0.01, "the head tips forward");
        near(now["hand_r"].translation(), was["hand_r"].translation(), "hands untouched");
    }
}
