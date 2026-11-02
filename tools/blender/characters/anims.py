"""Animation clips for the warriors, authored procedurally on each variant's own rig.

blender -b -P tools/blender/characters/anims.py -- male|female

Opens build/char_<variant>_final.blend (written by human.py, so the rig is exactly the one in
<variant>.glb) and keys these actions, exported to assets/models/characters/<variant>_anims.glb:

  locomotion  (loops, one cycle = two steps, all start at the left foot's contact)
    idle, walk_{f,b,l,r} (STROLL_SPEED 1.8 m/s), run_{f,b,l,r} (WALK_SPEED 5.6), sprint_f (SPRINT_SPEED 8.6),
    crouch_idle, crouch_f (CROUCH_SPEED 1.4), slide
  air         jump, fall, freefall, glide (loops / holds)
  upper body  melee_guard, melee_swing(_b), bow_draw, rifle_aim, consume (the game masks these to the torso)
  death       one-shot, holds its last frame

Legs are posed by analytic two-bone IK against foot paths in which the planted foot travels
backward at exactly the clip's ground speed, so with the game advancing the cycle at the
character's real speed the feet don't skate. Everything else is posed by aiming bones in world
space, which reads the same on both rigs although their bone rolls differ.
Speeds must match crates/shared/src/sim/movement.rs.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
FPS = 30

WALK, RUN, SPRINT, CROUCH = 1.8, 5.6, 8.6, 1.4

# Character faces -Y in Blender; its left is +X.
FWD = Vector((0, -1, 0))
LEFT = Vector((1, 0, 0))
UP = Vector((0, 0, 1))


class Rig:
    def __init__(self, obj):
        self.obj = obj
        self.pb = obj.pose.bones
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.mode_set(mode="POSE")
        for pb in self.pb:
            pb.rotation_mode = "QUATERNION"
        self.rest()
        d = lambda n: (self.head(n) - self.tail(n)).length
        self.thigh = {s: (self.head(f"calf_{s}") - self.head(f"thigh_{s}")).length for s in "lr"}
        self.shin = {s: (self.head(f"foot_{s}") - self.head(f"calf_{s}")).length for s in "lr"}
        self.ankle_z = {s: self.head(f"foot_{s}").z for s in "lr"}
        self.hip_x = {s: self.head(f"thigh_{s}").x for s in "lr"}
        self.foot_dir = {s: (self.tail(f"foot_{s}") - self.head(f"foot_{s}")).normalized() for s in "lr"}
        self.pelvis_rest = self.head("pelvis").copy()
        self.leg = min(self.thigh["l"] + self.shin["l"], self.thigh["r"] + self.shin["r"])
        self.clamped = 0
        self.clip = ""
        self.worst = {}
        self.duty = {}
        del d

    def rest(self):
        for pb in self.pb:
            pb.matrix_basis = Matrix.Identity(4)
        bpy.context.view_layer.update()

    def head(self, n):
        return self.obj.matrix_world @ self.pb[n].head

    def tail(self, n):
        return self.obj.matrix_world @ self.pb[n].tail

    def _apply(self, n, R, pivot):
        pb = self.pb[n]
        inv = self.obj.matrix_world.inverted()
        p = inv @ pivot
        Rl = inv.to_3x3().to_4x4() @ R @ self.obj.matrix_world.to_3x3().to_4x4()
        pb.matrix = Matrix.Translation(p) @ Rl @ Matrix.Translation(-p) @ pb.matrix
        bpy.context.view_layer.update()

    def rotate(self, n, axis, angle):
        """Rotate bone `n` (and so its children) about its head by `angle` round world `axis`."""
        self._apply(n, Matrix.Rotation(angle, 4, Vector(axis)), self.head(n))

    def world_rot(self, n):
        return (self.obj.matrix_world @ self.pb[n].matrix).to_quaternion()

    def set_world_rot(self, n, q):
        """Turn bone `n` (and its children) about its head until its world rotation is `q`."""
        d = q @ self.world_rot(n).inverted()
        self._apply(n, d.to_matrix().to_4x4(), self.head(n))

    def aim(self, n, direction):
        cur = (self.tail(n) - self.head(n)).normalized()
        q = cur.rotation_difference(Vector(direction).normalized())
        self._apply(n, q.to_matrix().to_4x4(), self.head(n))

    def move(self, n, offset):
        pb = self.pb[n]
        pb.matrix = Matrix.Translation(self.obj.matrix_world.inverted().to_3x3() @ Vector(offset)) @ pb.matrix
        bpy.context.view_layer.update()

    def leg_ik(self, s, ankle, pole=FWD):
        """Two-bone IK: put side `s`'s ankle at world `ankle`, knee bending toward `pole`."""
        h = self.head(f"thigh_{s}")
        l1, l2 = self.thigh[s], self.shin[s]
        v = Vector(ankle) - h
        d = v.length
        if d > (l1 + l2) * 0.999:
            self.clamped += 1
            # How far short of its target the ankle stays (foot slide, in metres).
            self.worst[self.clip] = max(self.worst.get(self.clip, 0.0), d - (l1 + l2) * 0.999)
        d = min(max(d, abs(l1 - l2) + 0.02), (l1 + l2) * 0.999)
        u = v.normalized()
        a = math.acos(max(-1.0, min(1.0, (l1 * l1 + d * d - l2 * l2) / (2 * l1 * d))))
        w = Vector(pole) - u * Vector(pole).dot(u)
        w.normalize()
        knee = h + u * (l1 * math.cos(a)) + w * (l1 * math.sin(a))
        self.aim(f"thigh_{s}", knee - h)
        self.aim(f"calf_{s}", h + u * d - knee)

    def foot_pitch(self, s, pitch, yaw=0.0):
        """Foot at its rest angle to the ground, toes raised by `pitch` (negative: pointed)."""
        d = Matrix.Rotation(yaw, 3, UP) @ Matrix.Rotation(pitch, 3, LEFT) @ self.foot_dir[s]
        self.aim(f"foot_{s}", d)

    def key(self, frame):
        for pb in self.pb:
            pb.keyframe_insert("rotation_quaternion", frame=frame)
        self.pb["pelvis"].keyframe_insert("location", frame=frame)


def new_action(rig, name):
    rig.clip = name
    # A clip made again (by motion capture over the procedural one) replaces it.
    if name in bpy.data.actions:
        bpy.data.actions.remove(bpy.data.actions[name])
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    rig.obj.animation_data_create()
    rig.obj.animation_data.action = act
    return act


def lerp(a, b, t):
    return a + (b - a) * t


def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def arm_swing(rig, s, swing, elbow, out=0.12):
    """Arms from the shoulders: `swing` radians forward (+) / back, forearm bent `elbow` radians."""
    sx = 1 if s == "l" else -1
    down = Vector((sx * out, 0, -1)).normalized()
    # A positive angle about +X tips the down vector toward +Y, which is backward (FWD is -Y):
    # turn the other way, so `swing` and the elbow's bend both carry the hand forward.
    ua = Matrix.Rotation(-swing, 3, LEFT) @ down
    rig.aim(f"upperarm_{s}", ua)
    fa = Matrix.Rotation(-(swing + elbow), 3, LEFT) @ down
    fa = (fa + Vector((-sx * 0.12 * math.sin(elbow), 0, 0))).normalized()
    rig.aim(f"lowerarm_{s}", fa)
    rig.aim(f"hand_{s}", fa)


def palm_frame(rig, s):
    """(forward, thumbward, palm normal) of hand `s` from its knuckles: forward runs wrist to
    knuckles, thumbward from the little finger's knuckle to the index's, and the palm normal points
    away from the back of the hand (the middle knuckle arches toward the back)."""
    h = rig.head(f"hand_{s}")
    i, p, m = rig.head(f"index_01_{s}"), rig.head(f"pinky_01_{s}"), rig.head(f"middle_01_{s}")
    fwd = (m - h).normalized()
    k = i - p
    k = (k - fwd * k.dot(fwd)).normalized()
    arch = m - i
    arch = arch - k * arch.dot(k) - fwd * arch.dot(fwd)
    return fwd, k, -arch.normalized()


def relax_fingers(rig, s, curl=0.35):
    """Curl the fingers toward the palm (about the knuckle hinge, whatever the arm's pose)."""
    fwd, _, palm = palm_frame(rig, s)
    hinge = fwd.cross(palm)
    for f in ("index", "middle", "ring", "pinky"):
        for k in (1, 2, 3):
            rig.rotate(f"{f}_0{k}_{s}", hinge, curl * (0.6 if k == 1 else 1.0))


def arm_ik(rig, s, wrist, pole):
    """Two-bone IK: put side `s`'s wrist at world `wrist`, elbow bending toward `pole`."""
    h = rig.head(f"upperarm_{s}")
    l1 = (rig.head(f"lowerarm_{s}") - h).length
    l2 = (rig.head(f"hand_{s}") - rig.head(f"lowerarm_{s}")).length
    v = Vector(wrist) - h
    d = min(max(v.length, abs(l1 - l2) + 0.02), (l1 + l2) * 0.999)
    u = v.normalized()
    a = math.acos(max(-1.0, min(1.0, (l1 * l1 + d * d - l2 * l2) / (2 * l1 * d))))
    w = Vector(pole) - u * Vector(pole).dot(u)
    w.normalize()
    elbow = h + u * (l1 * math.cos(a)) + w * (l1 * math.sin(a))
    rig.aim(f"upperarm_{s}", elbow - h)
    rig.aim(f"lowerarm_{s}", h + u * d - elbow)


def face_anchor(rig, body):
    """The bow's draw anchor, in the head bone's frame: the corner of the mouth on the string-hand
    (left) side, a finger's width off the skin, found from the nose tip of the rest-pose face."""
    hz = rig.head("head").z
    mw = body.matrix_world
    face = [mw @ v.co for v in body.data.vertices]
    # The nose tip: the most forward point on the midline between the lips and the eyes (below a
    # pagri's brim, which juts further out above the brow).
    nose = min((p for p in face if abs(p.x) < 0.012 and hz - 0.04 < p.z < hz + 0.05), key=lambda p: p.y)
    anchor = nose + Vector((0.045, 0.03, -0.045))
    head = rig.obj.matrix_world @ rig.pb["head"].matrix
    print(f"[anims] nose tip {nose.z - hz:+.3f} m above the head bone, {nose.y - rig.head('head').y:+.3f} forward")
    return head.inverted() @ anchor


def roll_hand(rig, s, angle):
    """Turn the hand about its own length (pronation / supination)."""
    rig.rotate(f"hand_{s}", rig.tail(f"hand_{s}") - rig.head(f"hand_{s}"), angle)


# ------------------------------------------------------------------ locomotion

def catmull(points, k):
    """A point on the Catmull-Rom curve through `points` (evenly spaced in k, 0..1)."""
    n = len(points) - 1
    x = min(max(k, 0.0), 1.0) * n
    i = min(int(x), n - 1)
    u = x - i
    p0, p1, p2, p3 = (points[max(0, min(n, j))] for j in (i - 1, i, i + 1, i + 2))
    return tuple(0.5 * (2 * b + (-a + c) * u + (2 * a - 5 * b + 4 * c - d) * u * u + (-a + 3 * b - 3 * c + d) * u ** 3)
                 for a, b, c, d in zip(p0, p1, p2, p3))


# A running stride's swing, as (along the travel in half-stances, height in `kick`s), evenly in
# time from toe-off to touchdown: the heel flicks up behind, the knee drives through high with the
# foot tucked under the seat, the shin swings out ahead, and the foot paws down and back to land.
# The male rig's leg (hip to ankle, m), on which the running numbers were tuned.
RUN_LEG = 0.92
RUN_SWING = [(-1.0, 0.0), (-0.92, 0.78), (-0.35, 1.0), (0.4, 0.66), (1.0, 0.28), (1.0, 0.0)]


def locomotion(rig, name, speed, direction, cadence, duty, lift, crouch, bob, lean, arm, elbow, kick=0.0, arm_back=0.0, hips_back=0.0):
    """One cycle (two steps). `direction` in body space (FWD, -FWD, LEFT, -LEFT); cadence in
    steps per second. The planted foot slides back at `speed`, so stride = speed * cycle time.
    `kick` (m): a running stride's swing (RUN_SWING) instead of a walk's low arc; `arm_back`
    (rad): the arms' swing centred that far behind hanging, as a runner's drive from the shoulder.
    `hips_back` (m): the pelvis sat back behind the feet, as in a crouch."""
    if kick:
        # Running strides scaled to the legs (RUN_LEG's are what the numbers were tuned on): shorter
        # legs take quicker, shorter steps and kick lower, rather than lunging to keep up.
        cadence *= (RUN_LEG / rig.leg) ** 0.5
        kick *= rig.leg / RUN_LEG
    period = 2.0 / cadence
    frames = max(8, round(period * FPS))
    period = frames / FPS
    stride = speed * period
    # Ground covered while planted: as asked, but no more than the bent leg can reach either side
    # of the hip at the stance's lowest pelvis (plus the toe roll at push-off).
    rig.rest()
    hip_z = min(rig.head("thigh_l").z, rig.head("thigh_r").z) - crouch - bob
    dx = rig.hip_x["l"] * 0.25          # feet track a little inside the hips
    reach = 0.92 * math.sqrt(max(0.0, (rig.leg * 0.99) ** 2 - (hip_z - min(rig.ankle_z.values())) ** 2 - dx * dx))
    stance = min(stride * duty, 2 * reach + 0.04)
    duty = stance / stride
    d = Vector(direction).normalized()
    new_action(rig, name)
    sideways = abs(d.x) > 0.5
    for f in range(frames + 1):
        t = f / frames
        rig.rest()
        # Pelvis: lower for running, bob twice a cycle (lowest mid-stance), sway and twist.
        drop = crouch + bob * (0.5 + 0.5 * math.cos(4 * math.pi * (t - duty / 2)))
        sway = 0.025 * math.sin(2 * math.pi * t) if not sideways else 0.0
        rig.move("pelvis", (sway, hips_back, -drop))
        twist = 0.12 * math.sin(2 * math.pi * t) * (speed / RUN) if not sideways else 0.0
        rig.rotate("pelvis", UP, twist)
        # The hip over the planted foot rides up, the swinging leg's side drops a little.
        if not sideways:
            rig.rotate("pelvis", FWD, 0.045 * math.cos(2 * math.pi * (t - duty / 2)))
            rig.rotate("spine_01", FWD, -0.03 * math.cos(2 * math.pi * (t - duty / 2)))
        rig.rotate("spine_01", LEFT, lean * 0.4)
        rig.rotate("spine_02", LEFT, lean * 0.6)
        rig.rotate("spine_02", UP, -twist * 1.8)
        for s, offset in (("l", 0.0), ("r", 0.5)):
            ph = (t - offset) % 1.0
            hx = rig.hip_x[s] * (0.75 if not sideways else 1.0)
            base = Vector((hx, 0, 0))
            if ph < duty:
                # Planted: from in front to behind, along the direction of travel.
                k = ph / duty
                g = d * (stance / 2 - stance * k)
                z = rig.ankle_z[s]
                pitch = lerp(0.12, -0.25, smooth((k - 0.6) / 0.4)) if k > 0.6 else lerp(0.12, 0.0, smooth(k / 0.3))
                z += max(0.0, -pitch) * 0.12          # heel rises as the foot rolls onto the toes
            else:
                # Swing: back to front on an arc, lifted most just after passing under the hip.
                k = (ph - duty) / (1 - duty)
                if kick:
                    u, h = catmull(RUN_SWING, k)
                    g = d * (u * stance / 2)
                    z = rig.ankle_z[s] + h * kick
                    # Toes pointed as the heel flicks up, drawn up as the foot reaches to land.
                    pitch = lerp(-0.75, 0.12, smooth((k - 0.25) / 0.6))
                else:
                    e = smooth(k)
                    g = d * lerp(-stance / 2, stance / 2, e)
                    z = rig.ankle_z[s] + lift * math.sin(math.pi * min(1.0, k * 1.15))
                    pitch = lerp(-0.5, 0.15, smooth((k - 0.1) / 0.8))
            rig.leg_ik(s, base + g + Vector((0, 0, z)), FWD + UP * 0.1)
            rig.foot_pitch(s, pitch)
        # Arms swing against the legs: the left arm furthest back as the left foot lands in front
        # (t = 0), furthest forward as the right one does (t = 0.5).
        for s, sign in (("l", 1), ("r", -1)):
            c = -math.cos(2 * math.pi * t) * sign * (-1 if d.y > 0.5 else 1)
            a = arm * c
            if sideways:
                arm_swing(rig, s, 0.25 * a, elbow)
            elif arm_back:
                # A runner's arms: driven from the shoulder, the hand back past the hip and forward
                # to the chest, the elbow closing as it comes forward and the hand crossing in a little.
                arm_swing(rig, s, a - arm_back, elbow + 0.3 * c, out=0.12 - 0.08 * max(0.0, c))
            else:
                arm_swing(rig, s, a, elbow)
            relax_fingers(rig, s, 0.55 if arm_back else 0.35)
        # Keep the head level and looking ahead.
        rig.rotate("neck_01", LEFT, -lean * 0.5)
        rig.rotate("head", UP, twist * 0.8)
        rig.key(f + 1)
    rig.duty[name] = round(duty, 2)
    return frames


# ------------------------------------------------------------------ motion capture

# Quaternius' Universal Animation Libraries 1 and 2 (CC0; tools/blender/characters/mocap): an
# Unreal-style mannequin with our rig's bone names, its clips in place, facing -Y like ours.
MOCAP = {"ual": os.path.join(HERE, "mocap", "UAL_Standard.fbx"), "ual2": os.path.join(HERE, "mocap", "UAL2_Standard.glb")}
TORSO = ["pelvis", "spine_01", "spine_02", "spine_03", "neck_01", "head", "clavicle_l", "clavicle_r"]
# How much of each torso bone's turn (in world space, from standing) we keep.
TORSO_KEEP = {"spine_01": 0.8, "spine_02": 0.75, "spine_03": 0.75, "neck_01": 0.6, "head": 0.5}


def load_mocap(pack):
    """Import a pack: its armature, the objects and the actions it brought."""
    objs, acts = set(bpy.data.objects), set(bpy.data.actions)
    bpy.ops.object.mode_set(mode="OBJECT")
    path = MOCAP[pack]
    if path.endswith(".fbx"):
        bpy.ops.import_scene.fbx(filepath=path)
    else:
        bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in objs]
    return next(o for o in new if o.type == "ARMATURE"), new, [a for a in bpy.data.actions if a not in acts]


# Mixamo (tools/blender/characters/mixamo, not in the repository: Adobe's terms): our bones' names
# in its rig.
MIXAMO_BONES = {
    "pelvis": "mixamorig:Hips", "spine_01": "mixamorig:Spine", "spine_02": "mixamorig:Spine1", "spine_03": "mixamorig:Spine2",
    "neck_01": "mixamorig:Neck", "head": "mixamorig:Head",
    **{f"{a}_{s}": f"mixamorig:{S}{b}" for s, S in (("l", "Left"), ("r", "Right"))
       for a, b in (("clavicle", "Shoulder"), ("upperarm", "Arm"), ("lowerarm", "ForeArm"), ("hand", "Hand"),
                    ("thigh", "UpLeg"), ("calf", "Leg"), ("foot", "Foot"), ("ball", "ToeBase"))},
}
MIXAMO_BONES.update({f"{f}_0{k}_{s}": f"mixamorig:{S}Hand{F}{k}" for s, S in (("l", "Left"), ("r", "Right"))
                     for f, F in (("index", "Index"), ("middle", "Middle"), ("ring", "Ring"), ("pinky", "Pinky"), ("thumb", "Thumb"))
                     for k in (1, 2, 3)})
MIXAMO = os.path.join(HERE, "mixamo")
# (our clip, the file in mixamo/, the speed it's played at, mocap options). Missing files are skipped.
_R, _P = "rifle/", "pistol/"
_DIRS = (("f", "forward"), ("b", "backward"), ("l", "left"), ("r", "right"))
MIXAMO_CLIPS = [
    # Empty-handed (and melee): the walk, the run, the crouch idle; the crouch walk in four
    # directions on the rifle pack's crouched legs, the arms hanging relaxed.
    ("walk_f", "Walking", 1.8, {}),
    ("crouch_idle", "Crouch Idle", 0.0, {}),
    ("crouch_f", f"{_R}walk crouching forward", 1.4, {"relax": True}),
    ("crouch_b", "Walk Crouching Backward", 1.4, {}),
    ("crouch_l", "Walk Crouching Left", 1.4, {}),
    ("crouch_r", "Walk Crouching Right", 1.4, {}),
    # Holding a long gun: the rifle pack, whole-body.
    ("rifle_idle", f"{_R}idle", 0.0, {}),
    ("rifle_aim", f"{_R}idle aiming", 0.0, {}),
    ("rifle_crouch_idle", f"{_R}idle crouching", 0.0, {}),
    *[(f"rifle_walk_{d}", f"{_R}walk {w}", 1.8, {}) for d, w in _DIRS],
    *[(f"rifle_run_{d}", f"{_R}run {w}", 5.6, {}) for d, w in _DIRS],
    *[(f"rifle_crouch_{d}", f"{_R}walk crouching {w}", 1.4, {}) for d, w in _DIRS],
    ("rifle_sprint_f", f"{_R}sprint forward", 8.6, {}),
    # Holding a pistol: the pistol pack (its strafes for both walking and running sideways).
    ("pistol_idle", f"{_P}pistol idle", 0.0, {}),
    # Aiming or firing a pistol: its idle's two-handed hold, laid over the legs.
    ("pistol_aim", f"{_P}pistol idle", 0.0, {}),
    ("pistol_crouch_idle", f"{_P}pistol kneeling idle", 0.0, {}),
    ("pistol_walk_f", f"{_P}pistol walk", 1.8, {}),
    ("pistol_walk_b", f"{_P}pistol walk backward", 1.8, {}),
    ("pistol_walk_l", f"{_P}pistol strafe", 1.8, {}),
    ("pistol_walk_r", f"{_P}pistol strafe (2)", 1.8, {}),
    ("pistol_run_f", f"{_P}pistol run", 5.6, {}),
    ("pistol_run_b", f"{_P}pistol run backward", 5.6, {}),
    ("pistol_run_l", f"{_P}pistol strafe", 5.6, {}),
    ("pistol_run_r", f"{_P}pistol strafe (2)", 5.6, {}),
    # In the air: the jump's rise off the ground, knees tucking (the game holds its middle), and a
    # drop off a ledge for the fall, both hips-at-rest so the legs move under the body.
    ("jump", "Jumping Up", 0.0, {"window": (33, 51), "air": True}),
    ("fall", "Jumping", 0.0, {"window": (61, 79), "pingpong": True, "air": True}),
    # Sliding: the held stretch of the slide, sat up, head level, as you can shoot from it.
    ("slide", "Running Slide", 0.0, {"window": (24, 44), "pingpong": True,
                                     "keep": {"spine_01": 0.45, "spine_02": 0.4, "spine_03": 0.4, "neck_01": 0.3, "head": 0.1}}),
]


def load_mixamo(name):
    """Import one Mixamo download (mixamo/<name>.fbx): its armature, objects, actions and frame rate,
    or None if it isn't there."""
    path = os.path.join(MIXAMO, f"{name}.fbx")
    if not os.path.exists(path):
        return None
    objs, acts = set(bpy.data.objects), set(bpy.data.actions)
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.import_scene.fbx(filepath=path)
    fps = bpy.context.scene.render.fps / bpy.context.scene.render.fps_base
    bpy.context.scene.render.fps, bpy.context.scene.render.fps_base = FPS, 1.0
    new = [o for o in bpy.data.objects if o not in objs]
    return next(o for o in new if o.type == "ARMATURE"), new, [a for a in bpy.data.actions if a not in acts], fps


def drop_mocap(objs, acts, arm):
    bpy.ops.object.mode_set(mode="OBJECT")
    for o in objs:
        bpy.data.objects.remove(o)
    for a in acts:
        bpy.data.actions.remove(a)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")


def mocap(rig, src, acts, name, clip, speed, stride=1.0, keep=None, lift=0.0, bend=0.0, swing=1.0, spread=0.0, bones=None, src_fps=FPS,
          relax=None):
    """Retarget the mocap `clip` onto our rig as `name`, a locomotion cycle at `speed` (m/s; 0 for
    one standing still, or in place). The torso's and head's rotations are copied; the arms aim along
    the source's bones (whatever the two rigs' rest poses and rolls); the legs are put by IK where the
    source's feet are, scaled by leg length, the knees bending the same way. A moving clip is
    retimed so its stride, scaled by leg length and by `stride` (under 1: shorter, quicker steps),
    covers the ground at `speed`; a planted foot is then locked sliding back at exactly `speed`, so
    with the game advancing the cycle at the character's real speed it doesn't skate, and after
    lift-off what's left of the difference from the source eases out over the swing. `keep`
    overrides TORSO_KEEP (how much of each torso bone's turn is kept) for this clip; `lift` (m) raises
    the pelvis over the clip's (a shallower crouch; negative, lower: a walk made a crouch walk), the
    feet staying on their paths; `bend` (rad) leans the back that much further forward, the head
    kept up; `swing` scales the arms' swing fore and aft; `spread` (m) sets each foot that much
    further out to its side (a wider stance), the knee following it out. `bones` maps our bone names
    to the source's (Mixamo's mixamorig:*), whose reference is then its rest pose (a T-pose) rather
    than an A_TPose clip; `src_fps` is the source's frame rate. A clip that travels (not "in place")
    has its travel taken out. `relax` (default: when `bend` is set) poses the arms hanging relaxed,
    swinging gently against the legs, instead of the source's (a rifle clip's legs, empty-handed).
    `stride="auto"` meets the game's speed half by longer steps and half by quicker ones."""
    keep = {**TORSO_KEEP, **(keep or {})}
    if bones:
        bone = lambda n: src.pose.bones.get(bones.get(n, n))
        find = lambda c: acts[0] if c is None else next(a for a in acts if a.name.split("|")[-1] == c)
    else:
        bone = lambda n: next((b for b in src.pose.bones if b.name.lower() == n), None)
        find = lambda c: next(a for a in acts if a.name.split("|")[-1] == c)
    W = src.matrix_world
    sc = bpy.context.scene
    rig.obj.animation_data.action = None
    if bones:
        # Mixamo's reference: its rest pose, standing straight in a T.
        src.animation_data.action = None
        for pb in src.pose.bones:
            pb.matrix_basis = Matrix.Identity(4)
        bpy.context.view_layer.update()
    else:
        # The source's reference: its A-pose clip, standing straight as our rig's rest does (its bind
        # pose isn't a standing pose).
        src.animation_data.action = find("A_TPose")
        sc.frame_set(int(src.animation_data.action.frame_range[0]))
    rest = {n: (W @ bone(n).matrix).copy() for n in TORSO + ["thigh_l", "foot_l", "calf_l"] if bone(n)}
    act = find(clip)
    src.animation_data.action = act
    f0, f1 = map(int, act.frame_range)
    frames = f1 - f0          # the loop's last frame repeats its first
    src_leg = (rest["calf_l"].translation - rest["thigh_l"].translation).length + (rest["foot_l"].translation - rest["calf_l"].translation).length
    r = rig.leg / src_leg
    src_ankle_z = rest["foot_l"].translation.z
    src_pelvis = rest["pelvis"].translation

    # The clip's travel over the loop (its hips' from first frame to last, along the ground), taken
    # out evenly: in place.
    sc.frame_set(f0)
    p0 = W @ bone("pelvis").head
    sc.frame_set(f1)
    p1 = W @ bone("pelvis").head
    travel = Vector((p1.x - p0.x, p1.y - p0.y, 0.0))
    if travel.length < 0.2:
        travel = Vector()

    def finger(s, f):
        if bones:
            return bone_of(f"mixamorig:{'Left' if s == 'l' else 'Right'}Hand{f.capitalize()}1")
        return bone(f"{f}_01_{s}")

    bone_of = lambda n: src.pose.bones.get(n)

    def src_palm(s, hd):
        """The source hand's palm normal (away from the back of the hand), from its knuckles, as
        palm_frame finds ours; None if the rig has no fingers."""
        i, p_, m = finger(s, "index"), finger(s, "pinky"), finger(s, "middle")
        if not (i and p_ and m):
            return None
        h = hd(f"hand_{s}")
        at = lambda b: W @ b.head
        fwd = (at(m) - h).normalized()
        k = at(i) - at(p_)
        k = (k - fwd * k.dot(fwd)).normalized()
        arch = at(m) - at(i)
        arch = arch - k * arch.dot(k) - fwd * arch.dot(fwd)
        return -arch.normalized()

    def sample(t):
        sc.frame_set(f0 + int(t), subframe=t - int(t))
        drift = travel * (t / frames)
        hd = lambda n: W @ bone(n).head - drift
        tl = lambda n: W @ bone(n).tail - drift
        rec = {"rot": {n: (W @ bone(n).matrix).to_quaternion() for n in TORSO if bone(n)}, "pelvis": hd("pelvis")}
        for s in "lr":
            rec[s] = dict(hip=hd(f"thigh_{s}"), knee=hd(f"calf_{s}"), ankle=hd(f"foot_{s}"), ball=hd(f"ball_{s}"),
                          foot=tl(f"foot_{s}") - hd(f"foot_{s}"), toe=tl(f"ball_{s}") - hd(f"ball_{s}"),
                          ua=tl(f"upperarm_{s}") - hd(f"upperarm_{s}"), fa=tl(f"lowerarm_{s}") - hd(f"lowerarm_{s}"),
                          hand=tl(f"hand_{s}") - hd(f"hand_{s}"), palm=src_palm(s, hd))
        return rec

    def planted(S):
        out = {}
        for s in "lr":
            zmin = min(rec[s]["ball"].z for rec in S)
            out[s] = [rec[s]["ball"].z < zmin + 0.025 for rec in S]
        return out

    def runs(down):
        """Each stretch of planted frames, as (first, length), wrapping round the loop."""
        n = len(down)
        out = []
        for a in (i for i in range(n) if down[i] and not down[i - 1]):
            k = 0
            while down[(a + k) % n] and k < n:
                k += 1
            out.append((a, k))
        return out

    # Along the ground, the planted feet slide back against the direction of travel: forward for
    # an in-place clip, else the way its hips went. `u` is a point's distance along that backward axis.
    back = -travel.normalized() if travel.length else -FWD
    u = lambda p: p.x * back.x + p.y * back.y
    # Retime: the source's own ground speed (its planted feet's), scaled, against `speed`.
    out = max(8, round(frames * FPS / src_fps))
    if speed:
        S = [sample(i) for i in range(frames)]
        down = planted(S)
        v = sorted((u(S[(a + k - 1) % frames][s]["ankle"]) - u(S[a][s]["ankle"])) / (k - 1) * src_fps
                   for s in "lr" for a, k in runs(down[s]) if k > 1)
        if stride == "auto":
            stride = (speed / (v[len(v) // 2] * r)) ** 0.5
        natural = v[len(v) // 2] * r * stride
        out = max(8, round(frames * natural / speed * FPS / src_fps))
        print(f"[anims] {name} from {clip}: natural {natural:.2f} m/s at stride {stride:.2f}, {frames} -> {out} frames")
    S = [sample(i * frames / out) for i in range(out)]
    src.animation_data.action = None
    down = planted(S)
    # Start the cycle as the left foot lands, as every locomotion clip does (the game blends them on
    # one shared phase, so their steps must line up).
    if speed:
        lands = [a for a, _ in runs(down["l"])]
        if lands:
            k = lands[0]
            S = S[k:] + S[:k]
            down = planted(S)
    # Each foot's place relative to the hips (scaled): across the travel as the source has it, along
    # it stretched by `stride` and, planted, locked to slide back at exactly `speed`.
    path = {}
    for s in "lr":
        rel = [(rec[s]["ankle"] - src_pelvis) * r for rec in S]
        us = [u(p) * (stride if speed else 1.0) for p in rel]
        if speed:
            locked = list(us)
            for a, n in runs(down[s]):
                for k in range(n):
                    locked[(a + k) % out] = us[a] + speed * k / FPS
                # After lift-off, ease the gap to the source's path out over the swing.
                gap = us[a] + speed * (n - 1) / FPS - us[(a + n - 1) % out]
                air = out - n
                for k in range(air):
                    j = (a + n + k) % out
                    if not down[s][j]:
                        locked[j] = us[j] + gap * (1.0 - smooth(k / max(1.0, air * 0.6)))
            us = locked
        path[s] = [Vector((p.x, p.y, 0.0)) - back * u(p) + back * us[j] for j, p in enumerate(rel)]
    new_action(rig, name)
    for i in range(out + 1):
        rec = S[i % out]
        rig.rest()
        rig.move("pelvis", rig.pelvis_rest + (rec["pelvis"] - src_pelvis) * r + UP * lift - rig.head("pelvis"))
        for n in TORSO:
            if n in rec["rot"] and n in rig.pb:
                # The source's turn from its rest, on our bone's rest.
                d = rec["rot"][n] @ rest[n].to_quaternion().inverted()
                # Softened up the back (the clips' stylised hunch reads heavier on our builds), and
                # the head kept nearer level, looking where it's going.
                d = Quaternion().slerp(d, keep.get(n, 1.0))
                q = d @ (rig.obj.matrix_world @ rig.obj.data.bones[n].matrix_local).to_quaternion()
                rig.set_world_rot(n, q)
        if bend:
            rig.rotate("spine_01", LEFT, bend * 0.45)
            rig.rotate("spine_02", LEFT, bend * 0.55)
            rig.rotate("neck_01", LEFT, -bend * 0.7)
        for s in "lr":
            m = rec[s]
            # The mannequin's arms clear its bulky chest: on our slimmer builds the elbows come in.
            if bend if relax is None else relax:
                # Leaning in, the captured arms (an upright walk's) read wrong: relaxed arms as the
                # crouch idle holds them, swinging gently against the legs (each arm furthest
                # forward as the other side's foot lands; the left foot lands as the walk begins).
                c = math.cos(2 * math.pi * i / out) * (1 if s == "r" else -1)
                arm_swing(rig, s, 0.12 + 0.2 * swing * c, 0.9 + 0.15 * c, out=0.16)
            else:
                rig.aim(f"upperarm_{s}", Vector((m["ua"].x * 0.55, m["ua"].y * swing, m["ua"].z)))
                rig.aim(f"lowerarm_{s}", Vector((m["fa"].x * 0.8, m["fa"].y * swing, m["fa"].z)))
                rig.aim(f"hand_{s}", m["hand"])
                if m.get("palm") is not None and name.startswith(("rifle_", "pistol_")):
                    # The hand's twist too (what it holds turns with it): its palm faced as the source's.
                    _, _, palm = palm_frame(rig, s)
                    axis = (rig.tail(f"hand_{s}") - rig.head(f"hand_{s}")).normalized()
                    a, b = palm - axis * palm.dot(axis), m["palm"] - axis * m["palm"].dot(axis)
                    if a.length > 1e-4 and b.length > 1e-4:
                        a.normalize(); b.normalize()
                        ang = math.atan2(a.cross(b).dot(axis), a.dot(b))
                        rig.rotate(f"hand_{s}", axis, ang)
            relax_fingers(rig, s, 0.85 if name.startswith(("rifle_", "pistol_")) else 0.5)
            side = 1 if s == "l" else -1
            g = path[s][i % out]
            ankle = Vector((g.x + rig.pelvis_rest.x + side * spread, g.y + rig.pelvis_rest.y,
                            rig.ankle_z[s] + (m["ankle"].z - src_ankle_z) * r))
            pole = m["knee"] - (m["hip"] + m["ankle"]) * 0.5 + Vector((side * spread * 0.4, 0, 0))
            rig.leg_ik(s, ankle, pole.normalized() if pole.length > 1e-4 else FWD)
            rig.aim(f"foot_{s}", m["foot"])
            rig.aim(f"ball_{s}", m["toe"])
        rig.key(i + 1)
    return out


def crouch_idle(rig):
    """Crouched still, matching the crouch walk: hips sat back, knees bent, the chest leaning in
    over them and the head up, feet a little apart and one ahead, breathing."""
    frames = 60
    new_action(rig, "crouch_idle")
    for f in range(frames + 1):
        t = f / frames
        b = math.sin(2 * math.pi * t)
        rig.rest()
        # The hips dropped low between the heels, the knees pushed out over the toes; the back
        # only a little forward.
        rig.move("pelvis", (0.0, 0.05, -0.44 - 0.008 * b))
        rig.rotate("spine_01", LEFT, 0.14)
        rig.rotate("spine_02", LEFT, 0.22 + 0.01 * b)
        rig.rotate("neck_01", LEFT, -0.2)
        for s, ahead in (("l", -0.05), ("r", 0.08)):
            sx = 1 if s == "l" else -1
            rig.leg_ik(s, (rig.hip_x[s] * 1.05, ahead, rig.ankle_z[s]), FWD + UP * 0.1)
            rig.foot_pitch(s, 0.0, yaw=sx * 0.2)
            arm_swing(rig, s, 0.35 + 0.02 * b, 1.2, out=0.16)
            relax_fingers(rig, s, 0.45)
        rig.key(f + 1)


def transfer(rig, src, acts, name, clip, speed, bones=None, src_fps=FPS, ref="A_TPose", relax=False, keep=None, window=None,
             pingpong=False, air=False):
    """Retarget a clip bone by bone: every joint's rotation copied from the source, as a turn from
    its rest in world space, onto ours (each source bone first aligned to our rest's direction, so
    the rigs' different rest poses don't matter: a T-pose's arms onto our A). Twists, shoulders,
    hips and the timing between them come through as captured. The pelvis moves as the source's,
    scaled by leg length, its travel taken out (in place). A moving clip is retimed (steps quicker
    or slower, not longer) so its feet cover the ground at `speed`, and starts as the left foot
    lands. `bones` maps our names to the source's; `ref` names the source's rest clip (None: its
    rest pose, as Mixamo's T). `relax`: the arms hang relaxed, swinging gently against the legs,
    instead of the source's (a rifle clip's legs, empty-handed). `keep` softens the torso's turns
    (how much of each bone's world turn from rest is kept: TORSO_KEEP for the library's stylised
    hunch). `window`: only these source frames (first, last), `pingpong` played there and back to
    loop seamlessly. `air`: airborne, the hips held at rest height (the body is where the game puts
    it; the legs tuck up under the hips rather than the whole body rising)."""
    bmap = bones or {}
    sb = lambda n: src.pose.bones.get(bmap.get(n, n)) or next((b for b in src.pose.bones if b.name.lower() == n), None)
    ours = [b.name for b in rig.obj.data.bones if b.name in rig.pb and b.name != "Root" and sb(b.name)]
    find = lambda c: acts[0] if c is None else next(a for a in acts if a.name.split("|")[-1] == c)
    W = src.matrix_world
    M = rig.obj.matrix_world
    sc = bpy.context.scene
    rig.obj.animation_data.action = None
    # The source's rest.
    if ref:
        src.animation_data.action = find(ref)
        sc.frame_set(int(src.animation_data.action.frame_range[0]))
    else:
        src.animation_data.action = None
        for pb in src.pose.bones:
            pb.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    s_rot = {n: (W @ sb(n).matrix).to_quaternion() for n in ours}
    s_dir = {n: ((W @ sb(n).tail) - (W @ sb(n).head)).normalized() for n in ours}
    s_pelvis = W @ sb("pelvis").head
    s_leg = ((W @ sb("calf_l").head) - (W @ sb("thigh_l").head)).length + ((W @ sb("foot_l").head) - (W @ sb("calf_l").head)).length
    rig.rest()
    t_rot = {n: (M @ rig.obj.data.bones[n].matrix_local).to_quaternion() for n in ours}
    t_dir = {n: (M.to_3x3() @ (rig.obj.data.bones[n].tail_local - rig.obj.data.bones[n].head_local)).normalized() for n in ours}
    # Per bone: rest turn out of the source frame, aligned onto ours.
    corr = {n: s_rot[n].inverted() @ s_dir[n].rotation_difference(t_dir[n]).inverted() @ t_rot[n] for n in ours}
    r = rig.leg / s_leg
    act = find(clip)
    src.animation_data.action = act
    f0, f1 = window or map(int, act.frame_range)
    frames = f1 - f0
    sc.frame_set(f0)
    p0 = W @ sb("pelvis").head
    sc.frame_set(f1)
    p1 = W @ sb("pelvis").head
    travel = Vector((p1.x - p0.x, p1.y - p0.y, 0.0))
    if travel.length < 0.2:
        travel = Vector()

    # Where the window starts (a clip's first frame stands over the source's origin; a window's
    # need not): taken out with the travel, so it plays on the spot.
    start = Vector((p0.x - s_pelvis.x, p0.y - s_pelvis.y, 0.0)) if window else Vector()

    def sample(t):
        sc.frame_set(f0 + int(t), subframe=t - int(t))
        drift = start + travel * (t / frames)
        return ({n: (W @ sb(n).matrix).to_quaternion() for n in ours}, W @ sb("pelvis").head - drift,
                {s: W @ sb(f"ball_{s}").head - drift for s in "lr"}, {s: W @ sb(f"foot_{s}").head - drift for s in "lr"})

    out = max(8, round(frames * FPS / src_fps))
    if speed:
        # The source's own ground speed: its planted feet's, sliding back against the travel.
        back = -travel.normalized() if travel.length else -FWD
        S = [sample(i) for i in range(frames)]
        v = []
        for s in "lr":
            zmin = min(x[2][s].z for x in S)
            down = [x[2][s].z < zmin + 0.025 for x in S]
            for i in range(frames):
                j = (i + 1) % frames
                if down[i] and down[j] and j:
                    v.append((S[j][3][s] - S[i][3][s]).dot(back) * src_fps)
        v.sort()
        natural = v[len(v) // 2] * r if v else speed
        out = max(8, round(frames * natural / speed * FPS / src_fps))
        print(f"[anims] {name} from {clip}: natural {natural:.2f} m/s, {frames} -> {out} frames")
    S = [sample(i * frames / out) for i in range(out)]
    if pingpong:
        S = S + [sample(frames)] + S[:0:-1]
        out = len(S)
    elif window:
        # A one-way stretch ends on its own last frame, not wrapping back to its first.
        S.append(sample(frames))
    src.animation_data.action = None
    if speed:
        zmin = min(x[2]["l"].z for x in S)
        down = [x[2]["l"].z < zmin + 0.025 for x in S]
        lands = [i for i in range(out) if down[i] and not down[i - 1]]
        if lands:
            S = S[lands[0]:] + S[:lands[0]]
    new_action(rig, name)
    depth = lambda n: len(rig.obj.data.bones[n].parent_recursive)
    order = sorted(ours, key=depth)
    for i in range(out + 1):
        rots, pelvis, _, _ = S[i] if i < len(S) else S[i % out]
        rig.rest()
        off = (pelvis - s_pelvis) * r
        if air:
            off.z = 0.0
        rig.move("pelvis", rig.pelvis_rest + off - rig.head("pelvis"))
        for n in order:
            if relax and n.split("_")[0] in ("upperarm", "lowerarm", "hand", "index", "middle", "ring", "pinky", "thumb"):
                continue
            q = rots[n] @ corr[n]
            if keep and n in keep:
                # Part of the turn from our rest, in world space.
                q = Quaternion().slerp(q @ t_rot[n].inverted(), keep[n]) @ t_rot[n]
            rig.set_world_rot(n, q)
        if relax:
            for s in "lr":
                c = math.cos(2 * math.pi * i / out) * (1 if s == "r" else -1)
                arm_swing(rig, s, 0.12 + 0.2 * c, 0.9 + 0.15 * c, out=0.16)
                relax_fingers(rig, s, 0.5)
        rig.key(i + 1)
    return out


PISTOL_DROP = 0.6
# A long gun's clips: the rifle pack's own, whole body (its stance puts the left hand on the gun; the
# game places the gun and brings both hands to it); False: the plain moves under our own carry.
RIFLE_PACK = False
# A long gun carried: the shoulders turned this far (radians), the left one forward.
RIFLE_TURN = 0.7
# ...and the torso straightened to this forward lean (radians; the jog leans well past it).
RIFLE_LEAN = math.radians(6)
# ...and the spine's swing on the move eased this far (the jog's shoulder twist would swing the
# fore-end out of the left hand's reach).
RIFLE_CALM = 0.7
ARM_BONES = ("clavicle", "upperarm", "lowerarm", "hand", "thumb", "index", "middle", "ring", "pinky")


def carry(rig, name, base, hold, drop=0.0, turn=0.0, upright=None, calm=0.0):
    """A gun carried on the move: `base`'s legs, hips and torso (the plain walk or run, its natural
    bounce and twist) with `hold`'s arms kept as they sit on the chest (each arm bone's turn from
    its parent copied, so the gun rides the torso as it swings). `drop` (radians) lowers both arms
    from the shoulders about the chest's side-to-side line, hands together: a pistol's aim grip
    carried at low ready. `turn` (radians) brings the left shoulder forward, the head kept to the
    front: a long gun's fore-end within the left hand's reach. `upright` (radians): the torso's
    forward lean (hips to neck) eased to this, frame by frame, the run's bounce and twist kept.
    `calm` (0-1): the spine's swing eased this far toward its average over the cycle (the chest
    steadier under a gun held in both hands)."""
    held = [b.name for b in rig.pb if b.name.split("_")[0] in ARM_BONES]
    sc = bpy.context.scene
    rig.obj.animation_data.action = bpy.data.actions[hold]
    sc.frame_set(1)
    grip = {n: rig.pb[n].matrix_basis.copy() for n in held}
    act = bpy.data.actions[base]
    rig.obj.animation_data.action = act
    f0, f1 = map(int, act.frame_range)
    poses = []
    for f in range(f0, f1 + 1):
        sc.frame_set(f)
        poses.append({pb.name: pb.matrix_basis.copy() for pb in rig.pb})
    if calm:
        for n in ("spine_01", "spine_02", "spine_03"):
            qs = [p[n].to_quaternion() for p in poses]
            avg = Quaternion((0, 0, 0, 0))
            for q in qs:
                avg = avg + (q if q.dot(qs[0]) >= 0 else -q)
            avg.normalize()
            for p, q in zip(poses, qs):
                p[n] = q.slerp(avg, calm).to_matrix().to_4x4()
    new_action(rig, name)
    leans, leans_after = [], []
    for i, pose in enumerate(poses):
        for pb in rig.pb:
            pb.matrix_basis = grip.get(pb.name, pose[pb.name])
        bpy.context.view_layer.update()
        if upright is not None:
            v = rig.head("neck_01") - rig.head("spine_01")
            lean = math.atan2(-v.y, v.z)
            leans.append(lean)
            # The head keeps looking where it did (the clip's head is bent up to see out of its
            # lean; straightened, it would tip back).
            look = rig.world_rot("head")
            rig.rotate("spine_01", LEFT, -(lean - upright) * 0.5)
            rig.rotate("spine_02", LEFT, -(lean - upright) * 0.5)
            rig.set_world_rot("head", look)
            v = rig.head("neck_01") - rig.head("spine_01")
            leans_after.append(math.atan2(-v.y, v.z))
        if turn:
            rig.rotate("spine_02", UP, -turn * 0.5)
            rig.rotate("spine_03", UP, -turn * 0.5)
            rig.rotate("neck_01", UP, turn * 0.6)
            rig.rotate("head", UP, turn * 0.4)
        if drop:
            side = (rig.head("upperarm_l") - rig.head("upperarm_r")).normalized()
            for s in "lr":
                rig.rotate(f"upperarm_{s}", side, drop)
        rig.key(i + 1)
    if leans:
        print(f"[anims] {name}: torso lean {math.degrees(sum(leans) / len(leans)):.0f} -> {math.degrees(sum(leans_after) / len(leans_after)):.0f} deg")
    return f1 - f0


def idle(rig):
    frames = 75
    new_action(rig, "idle")
    for f in range(frames + 1):
        t = f / frames
        b = math.sin(2 * math.pi * t)
        rig.rest()
        rig.move("pelvis", (0.012 * b, 0, -0.025))
        rig.rotate("spine_02", LEFT, 0.03 + 0.015 * b)
        for s in "lr":
            sx = 1 if s == "l" else -1
            rig.leg_ik(s, (rig.hip_x[s] * 1.15, 0.02 * sx, rig.ankle_z[s]), FWD + UP * 0.1)
            rig.foot_pitch(s, 0.0, yaw=sx * 0.15)
            arm_swing(rig, s, 0.05 + 0.02 * b, 0.25, out=0.16)
            relax_fingers(rig, s, 0.45)
        rig.rotate("head", UP, 0.08 * math.sin(2 * math.pi * t * 0.5))
        rig.key(f + 1)


# ------------------------------------------------------------------ air

def blade_clearance(rig):
    """How far the khanda's blade (as the game holds it: grip in the right fist, blade out of the
    thumb side, 0.06 to 0.88 m) passes from the head and pagri (a capsule up the head bone)."""
    h = rig.head("hand_r")
    fwd, k, palm = palm_frame(rig, "r")
    grip = h + (rig.head("middle_01_r") - h) * 0.85 + palm * 0.028
    head = rig.obj.matrix_world @ rig.pb["head"].matrix
    up = (head.to_3x3() @ Vector((0, 1, 0))).normalized()
    a, b = head.translation + up * 0.04, head.translation + up * 0.2
    best = 9.0
    for i in range(41):
        q = grip + k * (0.06 + 0.82 * i / 40)
        t = max(0.0, min(1.0, (q - a).dot(b - a) / (b - a).length_squared))
        best = min(best, (q - a.lerp(b, t)).length - HEAD_R)
    return best


HEAD_R = 0.13   # the pagri's radius round the head bone, roughly


def hold_pose(rig, name, frames, pose):
    """A pose (with optional motion over t in [0, 1]) keyed over `frames`."""
    new_action(rig, name)
    gap = []
    for f in range(frames + 1):
        rig.rest()
        pose(rig, f / frames)
        if name.startswith("melee"):
            gap.append(blade_clearance(rig))
        rig.key(f + 1)
    if gap:
        print(f"[anims] {name}: blade clears the head by {min(gap):+.3f} m at closest")


def jump_pose(rig, t):
    rig.move("pelvis", (0, 0, -0.04))
    rig.rotate("spine_02", LEFT, 0.12)
    rig.leg_ik("l", (rig.hip_x["l"], -0.18, rig.ankle_z["l"] + 0.38), FWD)
    rig.leg_ik("r", (rig.hip_x["r"], 0.12, rig.ankle_z["r"] + 0.2), FWD)
    rig.foot_pitch("l", -0.3)
    rig.foot_pitch("r", -0.5)
    arm_swing(rig, "l", 0.9, 0.5, out=0.35)
    arm_swing(rig, "r", -0.3, 0.6, out=0.35)


def fall_pose(rig, t):
    w = math.sin(2 * math.pi * t)
    rig.rotate("spine_02", LEFT, -0.08)
    rig.leg_ik("l", (rig.hip_x["l"] * 1.3, -0.1, rig.ankle_z["l"] + 0.22), FWD)
    rig.leg_ik("r", (rig.hip_x["r"] * 1.3, 0.05, rig.ankle_z["r"] + 0.12), FWD)
    rig.foot_pitch("l", -0.3)
    rig.foot_pitch("r", -0.4)
    for s, sx in (("l", 1), ("r", -1)):
        rig.aim(f"upperarm_{s}", (sx * 0.9, -0.1, 0.35 + 0.1 * w * sx))
        rig.aim(f"lowerarm_{s}", (sx * 0.6, -0.2, 0.7))


def freefall_pose(rig, t):
    """Skydiver's arch: body tipped face-down, arms and legs spread."""
    w = math.sin(2 * math.pi * t)
    rig.rotate("pelvis", LEFT, 1.25)
    rig.rotate("spine_02", LEFT, -0.25)
    rig.rotate("neck_01", LEFT, -0.5)
    for s, sx in (("l", 1), ("r", -1)):
        h = rig.head(f"thigh_{s}")
        body = (rig.head("neck_01") - rig.head("pelvis")).normalized()
        leg = (-body + Vector((sx * 0.45, 0, 0)) + UP * 0.35).normalized()
        knee = h + leg * rig.thigh[s]
        rig.leg_ik(s, knee + (leg + UP * 0.9).normalized() * rig.shin[s] * 0.95, UP)
        rig.aim(f"upperarm_{s}", (body + Vector((sx * 1.3, 0, 0)) + UP * 0.25).normalized())
        rig.aim(f"lowerarm_{s}", (body * 0.8 + Vector((sx * 0.6, 0, 0)) + UP * (0.5 + 0.05 * w)).normalized())


def glide_pose(rig, t):
    """Hanging from the glider's crossbar: arms up, legs together trailing a little."""
    w = math.sin(2 * math.pi * t)
    rig.rotate("pelvis", LEFT, 0.15)
    rig.rotate("spine_02", LEFT, -0.1)
    for s, sx in (("l", 1), ("r", -1)):
        rig.leg_ik(s, (rig.hip_x[s] * 0.7, 0.12 + 0.03 * w * sx, rig.ankle_z[s] + 0.1), FWD)
        rig.foot_pitch(s, -0.6)
        rig.aim(f"upperarm_{s}", (sx * 0.38, 0.05, 1.0))
        rig.aim(f"lowerarm_{s}", (sx * 0.1, 0.0, 1.0))
        rig.aim(f"hand_{s}", (sx * 0.0, -0.3, 1.0))


def death_pose(rig, t):
    """Knees give, then the body falls back to lie on the ground."""
    k = smooth(t / 0.45)
    f = smooth((t - 0.3) / 0.55)
    rig.move("pelvis", (0, 0.25 * f, -lerp(0.0, 0.32, k) - (rig.pelvis_rest.z - 0.4) * f))
    rig.rotate("pelvis", LEFT, -1.45 * f)
    rig.rotate("spine_02", LEFT, 0.35 * k * (1 - f))
    for s, sx in (("l", 1), ("r", -1)):
        h = rig.head(f"thigh_{s}")
        rig.leg_ik(s, h + Vector((sx * 0.15, -0.55 * (1 - f) - 0.1, -0.6 * (1 - f) - 0.05)) + FWD * 0.5 * f, FWD + UP)
        rig.aim(f"upperarm_{s}", (sx * (0.3 + 0.9 * f), 0.2 * f, -1 + 1.2 * f))
        rig.aim(f"lowerarm_{s}", (sx * (0.2 + 0.9 * f), 0.4 * f, -1 + 1.3 * f))
    rig.rotate("head", UP, 0.5 * f)


# ------------------------------------------------------------------ upper body (masked in game)

def legs_neutral(rig):
    for s in "lr":
        rig.leg_ik(s, (rig.hip_x[s] * 1.1, 0.0, rig.ankle_z[s]), FWD + UP * 0.1)
        rig.foot_pitch(s, 0.0)


def grip_blade(rig, blade):
    """Turn the sword hand so the blade (out of the thumb side of the fist) points along `blade`,
    bending the wrist as little as that allows."""
    fwd, k, _ = palm_frame(rig, "r")
    b = Vector(blade).normalized()
    f = fwd - b * fwd.dot(b)
    f.normalize()
    cur = Matrix((fwd, k, fwd.cross(k))).transposed()
    goal = Matrix((f, b, f.cross(b))).transposed()
    rig._apply("hand_r", (goal @ cur.transposed()).to_4x4(), rig.head("hand_r"))


def melee_guard_pose(rig, t):
    """Khanda held ready: sword fist forward of the right chest, blade up and leaning forward
    (clear of the face); off hand guarding."""
    b = math.sin(2 * math.pi * t)
    legs_neutral(rig)
    rig.rotate("spine_02", UP, -0.2)
    rig.rotate("spine_02", LEFT, 0.08 + 0.01 * b)
    rig.aim("upperarm_r", GUARD[2])
    rig.aim("lowerarm_r", GUARD[3])
    grip_blade(rig, GUARD[4])
    rig.aim("upperarm_l", (0.4, -0.35, -0.85))
    rig.aim("lowerarm_l", (-0.35, -0.85, 0.3))
    relax_fingers(rig, "r", 0.9)
    relax_fingers(rig, "l", 0.5)


# (t, spine twist, upper arm dir, forearm dir, blade dir)
GUARD = (0.0, -0.2, (-0.35, -0.55, -0.75), (0.1, -0.9, 0.4), (-0.15, -0.55, 0.82))
SWING_FORE = [  # windup over the right shoulder, a diagonal cut down to the left, follow-through
    GUARD,
    (0.3, -0.75, (-0.6, 0.2, 0.75), (-0.2, 0.6, 0.75), (-0.35, 0.55, 0.75)),
    (0.42, -0.15, (-0.35, -0.45, 0.75), (0.1, -0.75, 0.6), (0.3, -0.85, 0.45)),
    (0.55, 0.45, (0.2, -0.95, 0.05), (0.6, -0.75, -0.25), (0.6, -0.75, -0.2)),
    (1.0, 0.35, (0.15, -0.6, -0.75), (0.55, -0.7, -0.4), (0.55, -0.45, -0.7)),
]
SWING_BACK = [  # the return cut: from over the left shoulder, back across and down to the right
    GUARD,
    (0.3, 0.55, (0.45, -0.45, 0.35), (0.55, 0.25, 0.7), (0.55, 0.35, 0.75)),
    (0.42, 0.0, (0.0, -0.85, 0.3), (0.05, -0.85, 0.5), (0.1, -0.9, 0.4)),
    (0.55, -0.5, (-0.6, -0.75, 0.05), (-0.8, -0.5, -0.2), (-0.7, -0.65, -0.2)),
    (1.0, -0.4, (-0.5, -0.4, -0.7), (-0.6, -0.6, -0.45), (-0.55, -0.4, -0.7)),
]


def melee_swing_pose(rig, t, keys=SWING_FORE):
    """Windup, cut and follow-through between the keys above, the blade direction turning with
    the arm."""
    legs_neutral(rig)
    for (t0, s0, u0, f0, b0), (t1, s1, u1, f1, b1) in zip(keys, keys[1:]):
        if t0 <= t <= t1:
            k = smooth((t - t0) / (t1 - t0))
            rig.rotate("spine_02", UP, lerp(s0, s1, k))
            rig.rotate("spine_02", LEFT, 0.1 + 0.15 * math.sin(math.pi * t))
            rig.aim("upperarm_r", Vector(u0).lerp(Vector(u1), k))
            rig.aim("lowerarm_r", Vector(f0).lerp(Vector(f1), k))
            grip_blade(rig, Vector(b0).normalized().slerp(Vector(b1).normalized(), k))
            break
    rig.aim("upperarm_l", (0.45, -0.2, -0.85))
    rig.aim("lowerarm_l", (0.1, -0.8, -0.3))
    relax_fingers(rig, "r", 0.9)


BOW_BRACE = 0.2   # grip to string at rest, roughly, across the three bows


def bow_draw_pose(rig, t):
    """Side-on archer drawing: the bow arm (right, holding the bow) straight at the target; the left
    hand travels from the string at rest (t = 0) back to the cheek at full draw (t = 1). The game
    seeks this clip by draw fraction and pulls the string to wherever the left hand is."""
    legs_neutral(rig)
    rig.rotate("spine_02", UP, 0.55)
    rig.rotate("spine_02", LEFT, 0.04)
    rig.rotate("head", UP, -0.5)
    rig.aim("upperarm_r", (-0.08, -1.0, 0.04))
    rig.aim("lowerarm_r", (-0.04, -1.0, 0.04))
    rig.aim("hand_r", (-0.04, -1.0, 0.25))
    relax_fingers(rig, "r", 1.0)
    # Where the string sits: back from the bow hand along the shot, at the bow's grip height.
    grip = rig.head("hand_r").lerp(rig.head("middle_01_r"), 0.6)
    shot = (rig.head("hand_r") - rig.head("upperarm_r")).normalized()
    rest = grip - shot * BOW_BRACE
    head = rig.obj.matrix_world @ rig.pb["head"].matrix
    cheek = head @ rig.anchor
    k = smooth(t)
    goal = rest.lerp(cheek, k)
    # Place the wrist, then nudge it until the pinch (index and middle fingers on the string, where
    # the game hangs the string: 1.3 x hand-to-middle-knuckle) lands on the goal.
    wrist = goal + shot * 0.07 * (1 - 0.3 * k) + Vector((0, 0, -0.02))
    for _ in range(6):
        arm_ik(rig, "l", wrist, Vector((0.3, 0.8, 0.25 + 0.4 * k)))
        rig.aim("hand_l", goal - rig.head("hand_l") + shot * 0.05)
        h = rig.head("hand_l")
        err = goal - (h + (rig.head("middle_01_l") - h) * 1.3)
        if err.length < 0.004:
            break
        wrist += err
    relax_fingers(rig, "l", 0.55)


# Where the long gun's hands go (crates/game/src/arms.rs holds the gun at the right hand, barrel
# along the aim): the right hand on the grip this far in front of the shoulder, so the stock's
# butt (0.4 m behind the grip) sits in the shoulder pocket; the left hand under the fore-end.
RIFLE_GRIP = 0.45
RIFLE_FOREND = 0.3


def rifle_aim_pose(rig, t):
    """A long gun shouldered: left shoulder toward the target, the right hand on the grip in front
    of the right shoulder, the left hand cupping the fore-end, the head down along the barrel."""
    legs_neutral(rig)
    rig.rotate("spine_02", UP, -0.35)
    rig.rotate("spine_02", LEFT, 0.05)
    rig.rotate("head", UP, 0.3)
    rig.rotate("head", LEFT, 0.12)
    shoulder = rig.head("upperarm_r")
    grip = shoulder + FWD * RIFLE_GRIP + LEFT * 0.1 + UP * 0.02
    for _ in range(4):
        arm_ik(rig, "r", grip, Vector((-1.0, 0.2, -0.7)))
        rig.aim("hand_r", FWD + UP * 0.15 - LEFT * 0.1)
        grip += (shoulder + FWD * RIFLE_GRIP + LEFT * 0.1 + UP * 0.02) - rig.head("hand_r").lerp(rig.head("middle_01_r"), 0.6)
    shot = FWD
    forend = rig.head("hand_r") + shot * RIFLE_FOREND + UP * 0.0
    arm_ik(rig, "l", forend + UP * 0.035, Vector((0.6, 0.1, -1.0)))
    rig.aim("hand_l", FWD + UP * 0.1 - LEFT * 0.3)
    relax_fingers(rig, "r", 0.95)
    relax_fingers(rig, "l", 0.85)


def rifle_carry_pose(rig, t):
    """A long gun carried at low ready, both hands on it: the right hand on the grip in front of the
    right hip, the stock back along the forearm, the left hand under the fore-end, the barrel
    forward, angled down and in across the body."""
    legs_neutral(rig)
    down, across = 0.25, 0.45
    shot = (FWD * math.cos(down) - UP * math.sin(down)).normalized()
    shot = (shot * math.cos(across) + LEFT * math.sin(across) * 0.55).normalized()
    shoulder = rig.head("upperarm_r")
    want = shoulder + FWD * 0.38 + LEFT * 0.05 - UP * 0.2
    grip = want.copy()
    for _ in range(4):
        arm_ik(rig, "r", grip, Vector((-1.0, 0.4, -0.4)))
        rig.aim("hand_r", shot + UP * 0.1)
        grip += want - rig.head("hand_r").lerp(rig.head("middle_01_r"), 0.6)
    # The left wrist on the line from the right one along the barrel (the game lays a carried long
    # gun from the right wrist toward the left), a fore-end's length on.
    arm_ik(rig, "l", rig.head("hand_r") + shot * RIFLE_FOREND, Vector((0.5, 0.2, -1.0)))
    rig.aim("hand_l", shot - LEFT * 0.4 + UP * 0.1)
    relax_fingers(rig, "r", 0.95)
    relax_fingers(rig, "l", 0.85)


def consume_pose(rig, t):
    """Raise a flask or herb to the mouth and back."""
    legs_neutral(rig)
    k = math.sin(math.pi * t)
    rig.rotate("head", LEFT, -0.25 * k)
    rig.aim("upperarm_r", Vector((-0.2, -0.3, -1.0)).lerp(Vector((-0.35, -0.75, -0.2)), k))
    mouth = rig.head("head") + Vector((0, -0.12, -0.02))
    rig.aim("lowerarm_r", Vector((0.1, -0.4, -0.6)).lerp(mouth - rig.head("lowerarm_r"), k))
    relax_fingers(rig, "r", 0.9)
    arm_swing(rig, "l", 0.05, 0.3)


# ------------------------------------------------------------------ export

def render_previews(arm, out_dir):
    """Small workbench renders of each clip at four (--frames n: n) points in its cycle, side view
    and front."""
    os.makedirs(out_dir, exist_ok=True)
    bpy.ops.object.mode_set(mode="OBJECT")
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.render.resolution_x, sc.render.resolution_y = 240, 300
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "SINGLE"
    sc.display.shading.single_color = (0.75, 0.6, 0.5)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    sc.collection.objects.link(cam)
    sc.camera = cam
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = 2.3
    clips = sys.argv[sys.argv.index("--clips") + 1].split(",") if "--clips" in sys.argv else [a.name for a in bpy.data.actions]
    n = int(sys.argv[sys.argv.index("--frames") + 1]) if "--frames" in sys.argv else 4
    for name in clips:
        act = bpy.data.actions[name]
        arm.animation_data.action = act
        f0, f1 = act.frame_range
        for view, loc, rot in (("side", (3.5, 0, 0.95), (math.pi / 2, 0, math.pi / 2)), ("front", (0, -3.5, 0.95), (math.pi / 2, 0, 0))):
            cam.location, cam.rotation_euler = loc, rot
            for k in range(n):
                sc.frame_set(int(f0 + (f1 - f0) * k / n))
                sc.render.filepath = os.path.join(out_dir, f"{name}_{view}_{k}.png")
                bpy.ops.render.render(write_still=True)

def main():
    variant = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "male"
    bpy.ops.wm.open_mainfile(filepath=os.path.join(ROOT, "build", f"char_{variant}_final.blend"))
    preview = sys.argv[sys.argv.index("--preview") + 1] if "--preview" in sys.argv else None
    arm = bpy.data.objects["Human.rig"]
    rig = Rig(arm)
    rig.anchor = face_anchor(rig, bpy.data.objects[f"{variant}_LOD0"])
    for o in list(bpy.data.objects):
        # Keep the LOD0 body for previews; everything else goes.
        if o is not arm and not (preview and o.type == "MESH" and o.name.endswith("LOD0")):
            bpy.data.objects.remove(o)
    cycles = {}
    for name, speed, d in (("walk", WALK, None), ("run", RUN, None)):
        cad = 2.7 if speed == WALK else 3.0
        p = dict(cadence=cad, duty=0.42 if speed == WALK else 0.33, lift=0.16 if speed == WALK else 0.24,
                 crouch=0.07 if speed == WALK else 0.1, bob=0.025 if speed == WALK else 0.035,
                 arm=0.45 if speed == WALK else 0.6, elbow=1.2 if speed == WALK else 1.5)
        for suf, dvec in (("f", FWD), ("b", -FWD), ("l", LEFT), ("r", -LEFT)):
            lean = (0.1 if speed == WALK else 0.18) * (1 if suf == "f" else -0.4 if suf == "b" else 0.3)
            q = dict(p)
            if speed == RUN and suf == "f":
                # Running forward: a real running stride (shorter contact, a flight, the heel kick).
                q.update(cadence=3.1, duty=0.28, crouch=0.045, bob=0.035, arm=0.62, elbow=1.45, kick=0.4, arm_back=0.18)
            cycles[f"{name}_{suf}"] = locomotion(rig, f"{name}_{suf}", speed, dvec, lean=lean, **q)
    cycles["sprint_f"] = locomotion(rig, "sprint_f", SPRINT, FWD, cadence=3.45, duty=0.25, lift=0.32, crouch=0.05,
                                    bob=0.04, lean=0.28, arm=0.85, elbow=1.5, kick=0.52, arm_back=0.2)
    # The forward run and the sprint (shorter, quicker steps than the clips'), standing, crouching,
    # sliding and jumping, from motion capture.
    src, objs, acts = load_mocap("ual")
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="POSE")
    cycles["run_f"] = transfer(rig, src, acts, "run_f", "Jog_Fwd_Loop", RUN, keep=TORSO_KEEP)
    cycles["sprint_f"] = transfer(rig, src, acts, "sprint_f", "Sprint_Loop", SPRINT, keep=TORSO_KEEP)
    transfer(rig, src, acts, "idle", "Idle_Loop", 0.0, keep=TORSO_KEEP)
    # A stroll, its steps a little longer than the clip's (it walks slowly).
    cycles["walk_f"] = mocap(rig, src, acts, "walk_f", "Walk_Loop", WALK, stride=1.4)
    # Crouching: the clips' own motion, their deep hunch eased to a fairly upright back with the
    # head up, the walk in shorter, quicker steps (at its own stride the trailing leg stretched
    # straight out behind the low hips); both higher than the clips' creep, whose back knee
    # dropped nearly to the ground every step.
    # The lift scaled to the legs (17 cm on the male's 0.92 m): the female's shorter legs crouch as deep.
    crouch_lift = 0.17 * (rig.leg / 0.923) ** 1.5
    crouch_keep = {"spine_01": 0.6, "spine_02": 0.55, "spine_03": 0.55, "neck_01": 0.4, "head": 0.2}
    # The crouch walk: the walk itself, the hips lowered to the crouch idle's height and the back
    # leaning in: its heel-to-toe steps and swinging arms, knees bent (the library's crouch walk
    # is a slow creep, its hands hanging, that reads oddly at any speed).
    cycles["crouch_f"] = mocap(rig, src, acts, "crouch_f", "Walk_Loop", CROUCH, stride=0.75, lift=-0.23 * (rig.leg / 0.923), bend=0.6,
                               spread=0.035 * rig.leg / 0.923)
    mocap(rig, src, acts, "crouch_idle", "Crouch_Idle_Loop", 0.0, keep=crouch_keep, lift=crouch_lift)
    # The game holds the jump at its middle (tucked, rising) and loops the fall.
    mocap(rig, src, acts, "jump", "Jump_Start", 0.0)
    mocap(rig, src, acts, "fall", "Jump_Loop", 0.0)
    drop_mocap(objs, acts, arm)
    # Mixamo's clips, where downloaded (tools/blender/characters/mixamo/<file>.fbx), over the
    # library's: (our clip, the file, the speed it's played at).
    for name, file, speed, opts in MIXAMO_CLIPS:
        got = load_mixamo(file)
        if not got:
            continue
        msrc, mobjs, macts, mfps = got
        bpy.context.view_layer.objects.active = arm
        bpy.ops.object.mode_set(mode="POSE")
        n = transfer(rig, msrc, macts, name, None, speed, bones=MIXAMO_BONES, src_fps=mfps, ref=None, relax=opts.get("relax", False),
                     keep=opts.get("keep"), window=opts.get("window"), pingpong=opts.get("pingpong", False), air=opts.get("air", False))
        if speed:
            cycles[name] = n
        print(f"[anims] {name} from Mixamo's {file} ({mfps:.0f} fps)")
        drop_mocap(mobjs, macts, arm)
    # Moving with a gun: the plain walk and run under the gun's hold (the packs' own runs are a
    # tactical jog, hunched, arms locked out), and still: a long gun at low ready (our own hold,
    # fitted to the gun: the packs' left hand isn't where our fore-end is), a pistol in both hands
    # at low ready.
    for d in "fblr":
        for m in ("walk", "run"):
            if "pistol_idle" in bpy.data.actions:
                cycles[f"pistol_{m}_{d}"] = carry(rig, f"pistol_{m}_{d}", f"{m}_{d}", "pistol_idle", drop=PISTOL_DROP)
    if not RIFLE_PACK:
        hold_pose(rig, "rifle_carry", 1, rifle_carry_pose)
        carry(rig, "rifle_idle", "idle", "rifle_carry", turn=RIFLE_TURN, upright=RIFLE_LEAN, calm=RIFLE_CALM)
        carry(rig, "rifle_crouch_idle", "crouch_idle", "rifle_carry", turn=RIFLE_TURN, upright=RIFLE_LEAN, calm=RIFLE_CALM)
        for d in "fblr":
            cycles[f"rifle_crouch_{d}"] = carry(rig, f"rifle_crouch_{d}", f"crouch_{d}", "rifle_carry", turn=RIFLE_TURN, upright=RIFLE_LEAN, calm=RIFLE_CALM)
            for m in ("walk", "run"):
                cycles[f"rifle_{m}_{d}"] = carry(rig, f"rifle_{m}_{d}", f"{m}_{d}", "rifle_carry", turn=RIFLE_TURN, upright=RIFLE_LEAN, calm=RIFLE_CALM)
        cycles["rifle_sprint_f"] = carry(rig, "rifle_sprint_f", "sprint_f", "rifle_carry", turn=RIFLE_TURN, upright=RIFLE_LEAN, calm=RIFLE_CALM)
        bpy.data.actions.remove(bpy.data.actions["rifle_carry"])
    if not os.path.exists(os.path.join(MIXAMO, "Running Slide.fbx")):
        src, objs, acts = load_mocap("ual2")
        bpy.context.view_layer.objects.active = arm
        bpy.ops.object.mode_set(mode="POSE")
        # Sitting up in the slide, head level, as you can shoot from it (the clip lies far back).
        mocap(rig, src, acts, "slide", "Slide_Loop", 0.0, keep={"pelvis": 0.45, "spine_01": 0.3, "spine_02": 0.25, "spine_03": 0.25, "neck_01": 0.15, "head": 0.0})
        drop_mocap(objs, acts, arm)
    hold_pose(rig, "freefall", 40, freefall_pose)
    hold_pose(rig, "glide", 40, glide_pose)
    hold_pose(rig, "death", 30, death_pose)
    hold_pose(rig, "melee_guard", 60, melee_guard_pose)
    hold_pose(rig, "melee_swing", 12, melee_swing_pose)
    hold_pose(rig, "melee_swing_b", 12, lambda r, t: melee_swing_pose(r, t, SWING_BACK))
    hold_pose(rig, "bow_draw", 20, bow_draw_pose)
    hold_pose(rig, "rifle_aim", 10, rifle_aim_pose)
    hold_pose(rig, "consume", 30, consume_pose)
    print(f"[anims] {variant}: leg {rig.leg:.3f} m, cycles {cycles}, IK reach clamped on {rig.clamped} leg poses")
    print(f"[anims] stance duty: {rig.duty}")
    print("[anims] worst ankle shortfall (m): " + ", ".join(f"{k} {v:.3f}" for k, v in sorted(rig.worst.items())))

    if preview:
        render_previews(arm, preview)
        return
    # One NLA track per action so the exporter writes each as its own glTF animation.
    arm.animation_data.action = None
    for act in bpy.data.actions:
        tr = arm.animation_data.nla_tracks.new()
        tr.name = act.name
        tr.strips.new(act.name, int(act.frame_range[0]), act)
        tr.mute = True
    bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.select_all(action="DESELECT")
    arm.select_set(True)
    out = os.path.join(ROOT, "assets", "models", "characters", f"{variant}_anims.glb")
    bpy.context.scene.render.fps = FPS
    bpy.ops.export_scene.gltf(
        filepath=out, export_format="GLB", use_selection=True, export_yup=True, export_skins=True,
        export_def_bones=True, export_animations=True, export_animation_mode="NLA_TRACKS",
        export_force_sampling=True, export_frame_step=1, export_optimize_animation_size=False,
        export_anim_single_armature=True, export_reset_pose_bones=True,
    )
    print(f"[anims] exported {out}: {[a.name for a in bpy.data.actions]}")


if __name__ == "__main__":
    main()
