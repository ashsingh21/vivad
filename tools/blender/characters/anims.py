"""Animation clips for the warriors, authored procedurally on each variant's own rig.

blender -b -P tools/blender/characters/anims.py -- male|female

Opens build/char_<variant>_final.blend (written by human.py, so the rig is exactly the one in
<variant>.glb) and keys these actions, exported to assets/models/characters/<variant>_anims.glb:

  locomotion  (loops, one cycle = two steps, all start at the left foot's contact)
    idle, walk_{f,b,l,r} (AIM_SPEED 3.6 m/s), run_{f,b,l,r} (WALK_SPEED 5.6), sprint_f (SPRINT_SPEED 8.6)
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

WALK, RUN, SPRINT = 3.6, 5.6, 8.6

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

def locomotion(rig, name, speed, direction, cadence, duty, lift, crouch, bob, lean, arm, elbow):
    """One cycle (two steps). `direction` in body space (FWD, -FWD, LEFT, -LEFT); cadence in
    steps per second. The planted foot slides back at `speed`, so stride = speed * cycle time."""
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
        rig.move("pelvis", (sway, 0, -drop))
        twist = 0.12 * math.sin(2 * math.pi * t) * (speed / RUN) if not sideways else 0.0
        rig.rotate("pelvis", UP, twist)
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
                e = smooth(k)
                g = d * lerp(-stance / 2, stance / 2, e)
                z = rig.ankle_z[s] + lift * math.sin(math.pi * min(1.0, k * 1.15))
                pitch = lerp(-0.5, 0.15, smooth((k - 0.1) / 0.8))
            rig.leg_ik(s, base + g + Vector((0, 0, z)), FWD + UP * 0.1)
            rig.foot_pitch(s, pitch)
        # Arms swing against the legs: the left arm furthest back as the left foot lands in front
        # (t = 0), furthest forward as the right one does (t = 0.5).
        for s, sign in (("l", 1), ("r", -1)):
            a = -arm * math.cos(2 * math.pi * t) * sign * (-1 if d.y > 0.5 else 1)
            arm_swing(rig, s, a if not sideways else 0.25 * a, elbow)
            relax_fingers(rig, s)
        # Keep the head level and looking ahead.
        rig.rotate("neck_01", LEFT, -lean * 0.5)
        rig.rotate("head", UP, twist * 0.8)
        rig.key(f + 1)
    rig.duty[name] = round(duty, 2)
    return frames


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
RIFLE_GRIP = 0.3
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
    """Small workbench renders of each clip at four points in its cycle, side view and front."""
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
    for name in clips:
        act = bpy.data.actions[name]
        arm.animation_data.action = act
        f0, f1 = act.frame_range
        for view, loc, rot in (("side", (3.5, 0, 0.95), (math.pi / 2, 0, math.pi / 2)), ("front", (0, -3.5, 0.95), (math.pi / 2, 0, 0))):
            cam.location, cam.rotation_euler = loc, rot
            for k in range(4):
                sc.frame_set(int(f0 + (f1 - f0) * k / 4))
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
        cad = 2.8 if speed == WALK else 3.0
        p = dict(cadence=cad, duty=0.42 if speed == WALK else 0.33, lift=0.16 if speed == WALK else 0.24,
                 crouch=0.07 if speed == WALK else 0.1, bob=0.025 if speed == WALK else 0.035,
                 arm=0.45 if speed == WALK else 0.6, elbow=1.2 if speed == WALK else 1.5)
        for suf, dvec in (("f", FWD), ("b", -FWD), ("l", LEFT), ("r", -LEFT)):
            lean = (0.1 if speed == WALK else 0.18) * (1 if suf == "f" else -0.4 if suf == "b" else 0.3)
            cycles[f"{name}_{suf}"] = locomotion(rig, f"{name}_{suf}", speed, dvec, lean=lean, **p)
    cycles["sprint_f"] = locomotion(rig, "sprint_f", SPRINT, FWD, cadence=3.45, duty=0.29, lift=0.32, crouch=0.11,
                                    bob=0.04, lean=0.3, arm=0.85, elbow=1.6)
    idle(rig)
    hold_pose(rig, "jump", 12, jump_pose)
    hold_pose(rig, "fall", 30, fall_pose)
    hold_pose(rig, "freefall", 40, freefall_pose)
    hold_pose(rig, "glide", 40, glide_pose)
    hold_pose(rig, "death", 30, death_pose)
    hold_pose(rig, "melee_guard", 60, melee_guard_pose)
    hold_pose(rig, "melee_swing", 12, melee_swing_pose)
    hold_pose(rig, "melee_swing_b", 12, lambda r, t: melee_swing_pose(r, t, SWING_BACK))
    hold_pose(rig, "bow_draw", 20, bow_draw_pose)
    hold_pose(rig, "rifle_aim", 10, rifle_aim_pose)
    hold_pose(rig, "consume", 30, consume_pose)
    print(f"[anims] {variant}: cycles {cycles}, IK reach clamped on {rig.clamped} leg poses")
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
