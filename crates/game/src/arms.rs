//! Weapons in the hand: where the fist closes on them, the bow string and nocked arrows, and the
//! torso layered over the animation clips (pitch to the aim, flinch when hit).
//!
//! - Grip: one rule for every item. The weapon's +Y (blade, bow's upper limb, vessel's mouth)
//!   runs along the knuckles toward the thumb, its -Z (edge, bow's back) along the hand from wrist
//!   to knuckles, and the grip sits inside the curled fingers. It comes from the hand's own bones,
//!   so it's right for both rigs whatever the clips do with the wrist.
//! - Bows: raised to shoot, a bow turns out of the grip to stand upright (canted a little) with its
//!   back to the aim, whatever the wrist is doing. The model has no string. Each frame the string runs from each nock round the belly of
//!   the ear (while it touches it) to the nock point, which follows the drawing hand by the weight
//!   of the `bow_draw` clip, so it is straight at rest and pulled to the cheek at full draw.
//! - Aim: the spine and neck share the aim pitch (fully for a bow, partly for other items) so the
//!   arms and head point where the shot will go. Hits snap the chest away from the attacker.

use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::prelude::*;
use serde::Deserialize;
use shared::sim::{Action, Eliminated, HitMsg, Inventory, Kinematic, MoveMode, PlayerIntent};

use crate::animation::WarriorAnim;
use crate::visuals::{Palette, WarriorVisual};

/// Arrow model length, nock to point (tools/blender/weapons/arsenal.py ARROW_LEN).
pub const ARROW_LEN: f32 = 0.82;
/// Bow string thickness, a little over life size so it reads at play distance.
const STRING_WIDTH: f32 = 0.006;
/// A raised bow's top leans this far (radians) off vertical, away from the drawing hand.
const CANT: f32 = 0.14;

pub struct ArmsPlugin;

impl Plugin for ArmsPlugin {
    fn build(&self, app: &mut App) {
        let specs: HashMap<String, BowSpec> =
            ron::from_str(include_str!("../../../assets/data/bows.ron")).expect("assets/data/bows.ron");
        app.insert_resource(BowSpecs(specs))
            .init_resource::<GunCheck>()
            .add_systems(Update, (rig_bows, start_flinches))
            .add_systems(
                PostUpdate,
                (
                    pose_torso.after(bevy::app::AnimationSystems).before(TransformSystems::Propagate),
                    (orient_bows, draw_bows).chain().after(TransformSystems::Propagate),
                ),
            );
    }
}

/// String anchors in a bow's glTF frame (limbs along Y, target -Z, string side +Z), upper limb;
/// the lower mirrors y. Written by the asset script.
/// A gulel's two band anchors sit side by side at the fork's tips instead: `mirror_x` mirrors the
/// nock in x, and the pouch rests between them.
#[derive(Deserialize, Clone, Copy)]
struct BowSpec {
    nock: (f32, f32, f32),
    bridge: (f32, f32, f32),
    rest: (f32, f32, f32),
    #[allow(dead_code)]
    half_len: f32,
    #[serde(default)]
    mirror_x: bool,
}

#[derive(Resource)]
struct BowSpecs(HashMap<String, BowSpec>);

/// On a bow model: its string segments (upper nock to bridge, upper bridge to nock point, then the
/// same for the lower limb) and the arrows shown on the string.
#[derive(Component)]
pub struct BowRig {
    model: String,
    pellets: u32,
    string: Vec<Entity>,
    arrows: Vec<Entity>,
}

impl BowRig {
    pub fn new(model: &str, pellets: u32) -> Self {
        Self { model: model.into(), pellets, string: vec![], arrows: vec![] }
    }
}

/// Bones the torso layer and the bow string need, found when the character's rig loads.
#[derive(Component)]
pub struct Body {
    /// The character scene root (its frame is the model space: +Z forward, +Y up).
    pub root: Entity,
    /// spine_01, spine_02, spine_03, neck_01.
    pub spine: [Entity; 4],
    pub hand_l: Entity,
    /// Where the left hand pinches the string, in its bone's frame.
    pub pinch_l: Vec3,
    /// The left hand's middle knuckle and the way out of its palm, in its bone's frame.
    pub knuckle_l: Vec3,
    pub palm_l: Vec3,
    /// The left fingers' bones, each finger knuckle to tip.
    pub fingers_l: Vec<Entity>,
    /// For the gun check: the right shoulder and the head.
    pub shoulder_r: Option<Entity>,
    pub head: Option<Entity>,
}

/// The local player's long-gun hold by the numbers, this frame (the armoury logs it with each
/// shot): see `gun_check`.
#[derive(Resource, Default)]
pub struct GunCheck(pub Option<String>);

/// A hit's recoil on the torso: lean along `dir` (model space), decaying over a fraction of a second.
#[derive(Component)]
struct Flinch {
    age: f32,
    dir: Vec3,
}

/// The weapon's frame in a hand bone's frame, from the hand's knuckle positions (each child
/// bone's translation): returns (rotation, grip point). `index`, `middle`, `pinky` are the
/// *_01 bones.
pub fn hand_grip(index: Vec3, middle: Vec3, pinky: Vec3) -> (Quat, Vec3) {
    let fwd = middle.normalize();
    let k = (index - pinky).reject_from(fwd).normalize();
    let palm = palm_normal(index, middle, pinky);
    let along = fwd.reject_from(k).normalize();
    // Weapon +Y thumbward, -Z along the hand.
    let (y, z) = (k, -along);
    let rot = Quat::from_mat3(&Mat3::from_cols(y.cross(z), y, z));
    (rot, middle * 0.85 + palm * 0.028)
}

/// Out of the palm: the middle knuckle arches toward the back of the hand.
pub fn palm_normal(index: Vec3, middle: Vec3, pinky: Vec3) -> Vec3 {
    let fwd = middle.normalize();
    let k = (index - pinky).reject_from(fwd).normalize();
    -(middle - index).reject_from(k).reject_from(fwd).normalize()
}

/// A bow gets its string and nocked arrows, a gulel its bands and a pebble in the pouch; a gun
/// (no anchors) neither.
fn rig_bows(mut commands: Commands, mut q: Query<(Entity, &mut BowRig), Added<BowRig>>, palette: Res<Palette>, specs: Res<BowSpecs>) {
    for (e, mut rig) in &mut q {
        let Some(spec) = specs.0.get(&rig.model) else { continue };
        for _ in 0..4 {
            let s = commands
                .spawn((
                    Mesh3d(palette.cylinder.clone()),
                    MeshMaterial3d(palette.string.clone()),
                    Transform::from_scale(Vec3::ZERO),
                    bevy::light::NotShadowCaster,
                    ChildOf(e),
                ))
                .id();
            rig.string.push(s);
        }
        let (mesh, mat) = if spec.mirror_x {
            (palette.sphere.clone(), palette.mats[&(shared::map::Mat::Granite, 1)].clone())
        } else {
            let arrow = &palette.models["arrow"];
            (arrow.mesh.clone(), arrow.mat.clone())
        };
        for _ in 0..rig.pellets.clamp(1, 3) {
            let a = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_scale(Vec3::ZERO), ChildOf(e))).id();
            rig.arrows.push(a);
        }
    }
}

/// A segment of string from `a` to `b` (bow frame): the unit cylinder stretched between them.
fn segment(a: Vec3, b: Vec3) -> Transform {
    let d = b - a;
    let len = d.length();
    if len < 1e-4 {
        return Transform::from_scale(Vec3::ZERO);
    }
    Transform {
        translation: (a + b) * 0.5,
        rotation: Quat::from_rotation_arc(Vec3::Y, d / len),
        scale: Vec3::new(STRING_WIDTH, len, STRING_WIDTH),
    }
}

/// Stand raised bows upright with their backs to the aim, blending from the plain grip by the draw
/// clip's weight; point guns the way they're held whatever the wrist does: raised, along the aim;
/// carried, a long gun from the right hand toward the left (both hands on it), a pistol along
/// the forearm. Runs after transform propagation and rewrites the weapon's and its socket's global
/// transforms for this frame (their local transforms keep the grip).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn orient_bows(
    rigs: Query<(Entity, &BowRig)>,
    parents: Query<&ChildOf>,
    kids: Query<&Children>,
    local: Query<(), With<shared::sim::LocalPlayer>>,
    mut check: ResMut<GunCheck>,
    vis: Query<(&WarriorVisual, &WarriorAnim, &Body)>,
    intents: Query<&PlayerIntent>,
    mut tf: Query<(&Transform, &mut GlobalTransform)>,
) {
    for (e, rig) in &rigs {
        let Some((wv, anim, body)) = parents.iter_ancestors(e).find_map(|a| vis.get(a).ok()) else { continue };
        let w = anim.bow_weight.clamp(0.0, 1.0);
        let gun = crate::visuals::shot_kind(&rig.model) == crate::visuals::Shot::Bullet;
        let long = crate::visuals::long_gun(&rig.model);
        let (Ok(intent), Ok(socket), Ok(hand)) = (intents.get(wv.sim), parents.get(e).map(|p| p.parent()), parents.get(e).and_then(|p| parents.get(p.parent())).map(|p| p.parent())) else {
            continue;
        };
        if w < 1e-3 && !gun {
            continue;
        }
        let (Ok((_, hand_g)), Ok((grip, _)), Ok((bow_local, _))) = (tf.get(hand), tf.get(socket), tf.get(e)) else { continue };
        let (mut hand_g, grip, bow_local) = (*hand_g, *grip, *bow_local);
        let aim = intent.aim_dir();
        let ahead = aim.with_y(0.0).normalize_or(Vec3::NEG_Z);
        // A long gun is placed here rather than by the clips (whose arms ride hips the legs layer
        // tilts its own way, and were authored round other guns): carried, the stock tucked under
        // the right arm and the barrel down and across; aimed, the butt in the shoulder's pocket and
        // the barrel along the aim; the right hand brought to the grip, the left (below) to the
        // fore-end.
        let mut long_f = None;
        if gun && long {
            let chain = parents.get(hand).map(|p| p.parent()).ok().and_then(|l| parents.get(l).ok().map(|u| (u.parent(), l)));
            if let (Some(sh), Some((upper, lower))) = (body.shoulder_r.and_then(|s| tf.get(s).ok()).map(|(_, g)| g.translation()), chain) {
                let right = ahead.cross(Vec3::Y);
                let (dip, across) = CARRY_TILT;
                let low = (ahead * dip.cos() - Vec3::Y * dip.sin() - right * across).normalize();
                let f = low.lerp(aim, w).normalize_or(aim);
                let up = Vec3::Y.reject_from(f).normalize_or(Vec3::Y);
                let rot = Quat::from_mat3(&Mat3::from_cols(up.cross(-f), up, -f));
                let pocket = |(fwd, inward, high): (f32, f32, f32)| ahead * fwd - right * inward + Vec3::Y * high;
                let butt = sh + pocket(CARRY_POCKET).lerp(pocket(SHOULDER_POCKET), w);
                let socket_at = butt + f * BUTT;
                let (hs, _, _) = hand_g.to_scale_rotation_translation();
                let want_rot = rot * grip.rotation.inverse();
                let want_at = socket_at - want_rot * (grip.translation * hs);
                reach(upper, lower, hand, want_at, Vec3::NEG_Y + right * 0.8, want_rot, &kids, &mut tf);
                long_f = Some(f);
                if let Ok((_, g)) = tf.get(hand) {
                    hand_g = *g;
                }
            }
        }
        let held = hand_g.mul_transform(grip);
        // Carried: a long gun along the hands (right toward left), a pistol along the forearm
        // (elbow to wrist, the way a held pistol points), never back past the side.
        let along = |other: Entity, sign: f32| {
            tf.get(other)
                .map(|(_, g)| ((g.translation() - hand_g.translation()) * sign).normalize_or(ahead))
                .ok()
                .filter(|d| d.dot(ahead) > 0.2)
                .unwrap_or(ahead)
        };
        let carry = if long {
            // Never steeper than ~14 degrees down (a run's forward lean tips the hands with it).
            let d = along(body.hand_l, 1.0);
            d.with_y(d.y.max(-0.25)).normalize_or(ahead)
        } else {
            parents.get(hand).map(|p| along(p.parent(), -1.0)).unwrap_or(ahead)
        };
        let f = match long_f {
            Some(f) => f,
            None if gun => carry.lerp(aim, w).normalize_or(aim),
            None => aim,
        };
        let (raise, cant) = if gun { (1.0, if long { 0.0 } else { CANT * w }) } else { (w, CANT) };
        let up = Quat::from_axis_angle(f, cant) * Vec3::Y.reject_from(f).normalize_or(Vec3::Y);
        // Weapon frame: -Z (a bow's back, a barrel's line) along f, +Y up.
        let raised = Quat::from_mat3(&Mat3::from_cols(up.cross(-f), up, -f));
        let (scale, rot, pos) = held.to_scale_rotation_translation();
        let socket_g = GlobalTransform::from(Transform { translation: pos, rotation: rot.slerp(raised, raise), scale });
        if let Ok((_, mut g)) = tf.get_mut(socket) {
            *g = socket_g;
        }
        if let Ok((_, mut g)) = tf.get_mut(e) {
            *g = socket_g.mul_transform(bow_local);
        }
        // A long gun's fore-end in the left hand, wherever the clip left that hand: the palm under
        // the barrel a fore-end's length ahead of the grip, fingers wrapping round its far side.
        if gun && long {
            let (_, rot, _) = socket_g.to_scale_rotation_translation();
            let (along, up, right) = (rot * Vec3::NEG_Z, rot * Vec3::Y, rot * Vec3::X);
            // Under the fore-end, the palm up against the wood, the hand angled across the barrel
            // from the left so the fingers come up round its far side.
            let contact = socket_g.transform_point(Vec3::new(0.0, -0.035, -FOREND));
            let fingers = (along * 0.55 + right * 0.85).normalize();
            let palm = (up - fingers * up.dot(fingers)).normalize();
            // The hand's own frame (knuckle direction, palm) onto those.
            let (k, n) = (body.knuckle_l.normalize(), (body.palm_l - body.knuckle_l.normalize() * body.palm_l.dot(body.knuckle_l.normalize())).normalize());
            let want = Mat3::from_cols(fingers, palm, fingers.cross(palm)) * Mat3::from_cols(k, n, k.cross(n)).transpose();
            let want = Quat::from_mat3(&want);
            let (Ok(lower), hand) = (parents.get(body.hand_l).map(|p| p.parent()), body.hand_l) else { continue };
            let Ok(upper) = parents.get(lower).map(|p| p.parent()) else { continue };
            // Twice: the second pass moves the wrist by however far the palm (in the hand's own
            // scale) landed off the wood.
            let palm_at = |tf: &Query<(&Transform, &mut GlobalTransform)>| {
                tf.get(hand).map(|(_, g)| g.transform_point(body.knuckle_l * 0.55 + body.palm_l * 0.022)).unwrap_or(contact)
            };
            let mut wrist = tf.get(hand).map(|(_, g)| g.translation()).unwrap_or(contact);
            for _ in 0..2 {
                reach(upper, lower, hand, wrist, Vec3::NEG_Y - right * 0.7, want, &kids, &mut tf);
                wrist += contact - palm_at(&tf);
            }
            reach(upper, lower, hand, wrist, Vec3::NEG_Y - right * 0.7, want, &kids, &mut tf);
            if local.contains(wv.sim) {
                check.0 = Some(gun_check(body, &socket_g, ahead, palm_at(&tf), hand_g.translation(), &tf));
            }
            // Close the fingers round it: each joint curled toward the palm.
            for &f in &body.fingers_l {
                let Ok((_, g)) = tf.get(f) else { continue };
                let (at, axis) = (g.translation(), fingers.cross(palm));
                turn_subtree(f, at, Quat::from_axis_angle(axis, GRIP_CURL), &kids, &mut tf);
            }
        }
    }
}

/// How far ahead of the grip the left hand holds a long gun (m, along the barrel).
const FOREND: f32 = 0.24;
/// A long gun's hold in numbers: the left palm in the gun's frame (want x 0, up -0.035, along
/// -0.32: under the fore-end), the butt from the right shoulder in the body's frame (shouldered:
/// about level with it and a few cm ahead), the barrel's pitch, and how far below the eye it runs.
fn gun_check(body: &Body, socket: &GlobalTransform, ahead: Vec3, palm: Vec3, hand_r: Vec3, tf: &Query<(&Transform, &mut GlobalTransform)>) -> String {
    let inv = socket.affine().inverse();
    let p = inv.transform_point3(palm);
    let butt = socket.transform_point(Vec3::Z * BUTT);
    let (_, rot, at) = socket.to_scale_rotation_translation();
    let along = rot * Vec3::NEG_Z;
    let right = ahead.cross(Vec3::Y);
    let pos = |e: Option<Entity>| e.and_then(|e| tf.get(e).ok()).map(|(_, g)| g.translation());
    let b = pos(body.shoulder_r).map(|s| butt - s).unwrap_or(Vec3::NAN);
    let h = pos(body.shoulder_r).map(|s| hand_r - s).unwrap_or(Vec3::NAN);
    let eye = pos(body.head).map(|h| h.y + 0.08 - at.y).unwrap_or(f32::NAN);
    format!(
        "right hand from shoulder (fwd {:+.2} up {:+.2}) | palm on gun (x {:+.3} up {:+.3} along {:+.3}) | butt from right shoulder (fwd {:+.2} up {:+.2} right {:+.2}) | pitch {:+.0} deg | barrel {:.2} below eye",
        h.dot(ahead), h.y, p.x, p.y, p.z, b.dot(ahead), b.y, b.dot(right), along.y.asin().to_degrees(), eye
    )
}

/// Where a shouldered butt sits from the right shoulder joint (m): ahead, in toward the chest, up.
const SHOULDER_POCKET: (f32, f32, f32) = (0.07, 0.04, -0.02);
/// Carried: the butt tucked in under the front of the right shoulder (from the shoulder joint, as
/// above), the gun across the chest.
const CARRY_POCKET: (f32, f32, f32) = (-0.02, 0.05, -0.07);
/// Carried: the barrel's dip below level (radians) and its lean across the body (toward the left).
const CARRY_TILT: (f32, f32) = (0.14, 0.3);

/// The dunali's butt plate behind the grip (m, its model's +Z extent).
const BUTT: f32 = 0.43;

/// How far each finger joint closes round a fore-end (radians).
const GRIP_CURL: f32 = 0.55;

/// Two-bone IK on already-propagated global transforms: bend the arm `upper` - `lower` - `hand` so
/// the wrist lands on `wrist`, the elbow toward `pole` (a direction), then turn the hand to the
/// global rotation `hand_rot`. Everything under each bone (forearm, hand, fingers) turns with it.
#[allow(clippy::too_many_arguments)]
fn reach(upper: Entity, lower: Entity, hand: Entity, wrist: Vec3, pole: Vec3, hand_rot: Quat, kids: &Query<&Children>,
         tf: &mut Query<(&Transform, &mut GlobalTransform)>) {
    let pos = |tf: &Query<(&Transform, &mut GlobalTransform)>, e: Entity| tf.get(e).map(|(_, g)| g.translation()).ok();
    let (Some(s), Some(e0), Some(w0)) = (pos(tf, upper), pos(tf, lower), pos(tf, hand)) else { return };
    let (l1, l2) = ((e0 - s).length(), (w0 - e0).length());
    let v = wrist - s;
    let Some(u) = v.try_normalize() else { return };
    let d = v.length().clamp((l1 - l2).abs() + 0.01, (l1 + l2) * 0.999);
    let a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0).acos();
    let w = (pole - u * pole.dot(u)).normalize_or(Vec3::NEG_Y);
    let elbow = s + u * (l1 * a.cos()) + w * (l1 * a.sin());
    turn_subtree(upper, s, Quat::from_rotation_arc((e0 - s).normalize(), (elbow - s).normalize()), kids, tf);
    let Some(w1) = pos(tf, hand) else { return };
    turn_subtree(lower, elbow, Quat::from_rotation_arc((w1 - elbow).normalize(), (s + u * d - elbow).normalize()), kids, tf);
    let Ok((_, g)) = tf.get(hand) else { return };
    let (_, cur, at) = g.to_scale_rotation_translation();
    turn_subtree(hand, at, hand_rot * cur.inverse(), kids, tf);
}

/// Turn `e` and everything under it by `q` about `pivot`, in their (already propagated) global
/// transforms.
fn turn_subtree(e: Entity, pivot: Vec3, q: Quat, kids: &Query<&Children>, tf: &mut Query<(&Transform, &mut GlobalTransform)>) {
    if let Ok((_, mut g)) = tf.get_mut(e) {
        let (scale, rot, t) = g.to_scale_rotation_translation();
        *g = GlobalTransform::from(Transform { translation: pivot + q * (t - pivot), rotation: q * rot, scale });
    }
    if let Ok(c) = kids.get(e) {
        for k in c.iter() {
            turn_subtree(k, pivot, q, kids, tf);
        }
    }
}

#[allow(clippy::type_complexity)]
fn draw_bows(
    rigs: Query<(Entity, &BowRig)>,
    specs: Res<BowSpecs>,
    parents: Query<&ChildOf>,
    vis: Query<(&WarriorVisual, &Body, Option<&WarriorAnim>)>,
    sims: Query<(&Inventory, &Action, Has<Eliminated>)>,
    mut tf: Query<(&mut Transform, &mut GlobalTransform)>,
) {
    for (e, rig) in &rigs {
        let Some(spec) = specs.0.get(&rig.model) else { continue };
        let Ok((_, bow_g)) = tf.get(e) else { continue };
        let bow_g = *bow_g;
        // The warrior holding it, if any (pickups have none).
        let owner = parents.iter_ancestors(e).find_map(|a| vis.get(a).ok());
        let v3 = |t: (f32, f32, f32)| Vec3::new(t.0, t.1, t.2);
        let (nock, bridge, rest) = (v3(spec.nock), v3(spec.bridge), v3(spec.rest));
        let at_rest = if spec.mirror_x { Vec3::new(0.0, nock.y, bridge.z) } else { Vec3::new(0.0, 0.0, bridge.z) };
        let mut point = at_rest;
        let mut show_arrows = false;
        if let Some((wv, body, Some(anim))) = owner {
            let w = anim.bow_weight;
            if let Ok((_, hand_g)) = tf.get(body.hand_l) {
                let pinch = bow_g.affine().inverse().transform_point3(hand_g.transform_point(body.pinch_l));
                point = at_rest.lerp(pinch, w.clamp(0.0, 1.0));
                point.z = point.z.max(at_rest.z);
            }
            if let Ok((inv, act, dead)) = sims.get(wv.sim) {
                let loaded = inv.current().is_some_and(|s| s.count > 0);
                show_arrows = !dead && w > 0.25 && loaded && act.reload_left <= 0.0;
            }
        }
        let mut locals = [Transform::from_scale(Vec3::ZERO); 4];
        for (k, s) in [1.0f32, -1.0].into_iter().enumerate() {
            let (n, b) = if spec.mirror_x {
                (Vec3::new(nock.x * s, nock.y, nock.z), Vec3::new(bridge.x * s, bridge.y, bridge.z))
            } else {
                (Vec3::new(nock.x, nock.y * s, nock.z), Vec3::new(bridge.x, bridge.y * s, bridge.z))
            };
            // Does the straight nock-to-point line pass through the ear? Then the string wraps
            // its belly at the bridge first.
            let t = (b.y - n.y) / (point.y - n.y);
            let line_z = n.z + t * (point.z - n.z);
            if b.z > line_z + 1e-4 {
                locals[k * 2] = segment(n, b);
                locals[k * 2 + 1] = segment(b, point);
            } else {
                locals[k * 2] = segment(n, point);
            }
        }
        for (s, local) in rig.string.iter().zip(locals) {
            if let Ok((mut t, mut g)) = tf.get_mut(*s) {
                *t = local;
                *g = bow_g.mul_transform(local);
            }
        }
        // Nocked arrows: from the nock point through the arrow rest beside the grip (on the side the
        // string is pulled to), fanned for the Sharanga.
        let side = if point.x < 0.0 { -1.0 } else { 1.0 };
        let rest = Vec3::new(rest.x.abs() * side, rest.y, rest.z);
        let dir = (rest - point).normalize_or(Vec3::NEG_Z);
        let n = rig.arrows.len();
        for (i, a) in rig.arrows.iter().enumerate() {
            let fan = (i as f32 - (n as f32 - 1.0) * 0.5) * 0.06;
            let local = if show_arrows && spec.mirror_x {
                Transform::from_translation(point).with_scale(Vec3::splat(0.022))
            } else if show_arrows {
                Transform::from_translation(point).with_rotation(Quat::from_axis_angle(Vec3::Y, fan) * Quat::from_rotation_arc(Vec3::Y, dir))
            } else {
                Transform::from_scale(Vec3::ZERO)
            };
            if let Ok((mut t, mut g)) = tf.get_mut(*a) {
                *t = local;
                *g = bow_g.mul_transform(local);
            }
        }
    }
}

fn start_flinches(
    mut commands: Commands,
    mut hits: MessageReader<HitMsg>,
    vis: Query<(Entity, &WarriorVisual, &Transform)>,
    attackers: Query<&Transform, Without<WarriorVisual>>,
) {
    for h in hits.read() {
        let Some((e, _, t)) = vis.iter().find(|(_, v, _)| v.sim == h.target) else { continue };
        let from = attackers.get(h.attacker).map(|a| a.translation).unwrap_or(t.translation + t.rotation * Vec3::NEG_Z);
        let world = (t.translation - from).with_y(0.0).normalize_or(t.rotation * Vec3::Z);
        // Into model space (the character scene is turned half round to face the sim's -Z).
        let dir = (t.rotation * Quat::from_rotation_y(PI)).inverse() * world;
        commands.entity(e).insert(Flinch { age: 0.0, dir });
    }
}

/// Bend the spine and neck to the aim pitch and add any hit flinch, after the clips have posed
/// the skeleton and before transforms propagate.
#[allow(clippy::type_complexity)]
fn pose_torso(
    mut commands: Commands,
    time: Res<Time>,
    mut vis: Query<(Entity, &WarriorVisual, &Body, &WarriorAnim, Option<&mut Flinch>)>,
    sims: Query<(&PlayerIntent, &Kinematic, Has<Eliminated>)>,
    parents: Query<&ChildOf>,
    mut tf: Query<&mut Transform>,
) {
    const SHARE: [f32; 4] = [0.2, 0.3, 0.3, 0.2];
    let dt = time.delta_secs();
    for (e, wv, body, anim, flinch) in &mut vis {
        let Ok((intent, kin, dead)) = sims.get(wv.sim) else { continue };
        let mut lean = Quat::IDENTITY;
        if let Some(mut f) = flinch {
            f.age += dt;
            let amp = 0.32 * (f.age / 0.04).min(1.0) * (-f.age * 9.0).exp();
            let axis = Vec3::Y.cross(f.dir).normalize_or_zero();
            if f.age > 0.6 || axis == Vec3::ZERO {
                commands.entity(e).remove::<Flinch>();
            } else {
                lean = Quat::from_axis_angle(axis, amp);
            }
        }
        let pitch = if dead || kin.mode != MoveMode::Ground { 0.0 } else { intent.pitch.clamp(-1.1, 1.1) * anim.aim_share };
        if pitch.abs() < 1e-3 && lean == Quat::IDENTITY {
            continue;
        }
        // Model-space rotation of the first spine bone's parent.
        let rot = |e: Entity| tf.get(e).map(|t| t.rotation).unwrap_or(Quat::IDENTITY);
        let mut parent = Quat::IDENTITY;
        if let Ok(p) = parents.get(body.spine[0]) {
            let mut cur = p.parent();
            while cur != body.root {
                parent = rot(cur) * parent;
                let Ok(p) = parents.get(cur) else { break };
                cur = p.parent();
            }
        }
        for (i, &bone) in body.spine.iter().enumerate() {
            let Ok(mut t) = tf.get_mut(bone) else { break };
            // Model +X is the character's left: pitching up turns forward (+Z) toward +Y.
            let mut r = Quat::from_rotation_x(-pitch * SHARE[i]);
            if i == 1 {
                r = lean * r;
            }
            t.rotation = parent.inverse() * r * parent * t.rotation;
            parent *= t.rotation;
        }
    }
}

