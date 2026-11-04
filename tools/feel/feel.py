"""How movement feels, in numbers and pictures cheap to read, from the game's motion trace
(crates/game/src/feel.rs).

  python3 tools/feel/feel.py run [--only a,b]   play the scripted scenes in the game (about a
                                                minute), then check them (`just feelcheck`)
  python3 tools/feel/feel.py check <trace.csv>  the numbers per scene against the targets, and
                                                against the kept baseline
  python3 tools/feel/feel.py plot <trace.csv> [scene...]
                                                per scene, graphs (speed and input, clip weights,
                                                body) and a ghost trail (stick figures every
                                                0.1 s, side and top) to previews/feel/
  python3 tools/feel/feel.py keep               the last run's numbers become the baseline
  python3 tools/feel/feel.py mark [file.csv]    a mark saved with F9 in the game (default: the
                                                newest in marks/): what happened, its numbers,
                                                and its pictures

What it measures, at each event the trace shows:
  start   key down from still: seconds to 90% of top speed; the body's forward lean as it goes
  stop    keys up: the capsule's stop time and distance; then `settle`, how long the body keeps
          moving after the capsule has stopped (a floaty stop shows here); its overshoot (lean)
  land    off the ground and back: the pelvis dip on landing and the seconds it takes to settle
  flip    the move direction reversed: seconds until the velocity has turned
  turn    a flick of the view: seconds until the body faces the new way; its peak lean
  slide   the drop into it (seconds for the pelvis to go down) and the get-up after
and through each scene: stride (a planted foot's speed back through the body over the ground speed: 1 is no skating),
foot slide (cm/s left over on the ground), pops (a bone jumping off its smooth path in one frame, cm), arm swing (cm, hand front to back
while running), bob (cm, the pelvis up and down while running), the torso's pitch, and its arch (chest bent back from the waist, degrees).
"""

import argparse
import csv
import glob
import json
import math
import os
import subprocess
import sys

import numpy as np

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "previews", "feel")
TRACE = os.path.join(OUT, "trace.csv")
LAST = os.path.join(OUT, "last.json")
BASELINE = os.path.join(ROOT, "tools", "feel", "baseline.json")

# Capsule centre above the ground (shared::sim::HALF_HEIGHT).
HALF_HEIGHT = 0.9

BONES = ["pelvis", "spine_03", "neck_01", "head", "thigh_l", "calf_l", "foot_l", "ball_l", "thigh_r", "calf_r", "foot_r", "ball_r",
         "upperarm_l", "lowerarm_l", "hand_l", "upperarm_r", "lowerarm_r", "hand_r"]
LIMBS = [("pelvis", "spine_03"), ("spine_03", "neck_01"), ("neck_01", "head"),
         ("pelvis", "thigh_l"), ("thigh_l", "calf_l"), ("calf_l", "foot_l"), ("foot_l", "ball_l"),
         ("pelvis", "thigh_r"), ("thigh_r", "calf_r"), ("calf_r", "foot_r"), ("foot_r", "ball_r"),
         ("neck_01", "upperarm_l"), ("upperarm_l", "lowerarm_l"), ("lowerarm_l", "hand_l"),
         ("neck_01", "upperarm_r"), ("upperarm_r", "lowerarm_r"), ("lowerarm_r", "hand_r")]

# The scripted scenes: (name, item in hand, seconds, [(seconds in, key event)]). Each starts with
# the player standing still on open ground facing down a clear run. Keys as VIVAD_KEYS
# (crates/game/src/input.rs): +Key down, -Key up, TurnN a flick of the view N degrees left.
SCENES = [
    ("run_stop", "bat", 4.0, [(0.5, "+KeyW"), (2.5, "-KeyW")]),
    ("sprint_stop", "bat", 4.5, [(0.5, "+KeyW"), (0.5, "+ShiftLeft"), (3.0, "-KeyW"), (3.0, "-ShiftLeft")]),
    ("sprint_slide", "bat", 5.5, [(0.5, "+KeyW"), (0.5, "+ShiftLeft"), (2.5, "+KeyC"), (2.6, "-KeyC"),
                                  (3.9, "-KeyW"), (3.9, "-ShiftLeft"), (4.3, "+KeyC"), (4.4, "-KeyC")]),
    ("jump_run", "bat", 4.5, [(0.5, "+KeyW"), (1.6, "+Space"), (1.7, "-Space"), (3.4, "-KeyW")]),
    ("jump_still", "bat", 2.8, [(0.5, "+Space"), (0.6, "-Space")]),
    ("jump_sprint", "bat", 4.5, [(0.5, "+KeyW"), (0.5, "+ShiftLeft"), (1.8, "+Space"), (1.9, "-Space"), (3.6, "-KeyW"), (3.6, "-ShiftLeft")]),
    ("rifle_jump_run", "bandook", 4.5, [(0.5, "+KeyW"), (0.5, "+ShiftLeft"), (1.8, "+Space"), (1.9, "-Space"), (3.6, "-KeyW"), (3.6, "-ShiftLeft")]),
    ("strafe_flip", "bat", 4.0, [(0.5, "+KeyA"), (1.7, "-KeyA"), (1.7, "+KeyD"), (2.9, "-KeyD")]),
    ("reverse", "bat", 4.5, [(0.5, "+KeyW"), (2.0, "-KeyW"), (2.0, "+KeyS"), (3.2, "-KeyS")]),
    ("turn_sprint", "bat", 4.0, [(0.5, "+KeyW"), (0.5, "+ShiftLeft"), (1.8, "Turn90"), (3.0, "-KeyW"), (3.0, "-ShiftLeft")]),
    ("rifle_sprint_stop", "bandook", 4.5, [(0.5, "+KeyW"), (0.5, "+ShiftLeft"), (3.0, "-KeyW"), (3.0, "-ShiftLeft")]),
    ("rifle_aim_sprint", "bandook", 4.8, [(0.5, "+MouseRight"), (0.8, "+KeyW"), (2.2, "-MouseRight"), (2.2, "+ShiftLeft"),
                                          (3.8, "-KeyW"), (3.8, "-ShiftLeft")]),
    ("rifle_aim_strafe", "bandook", 4.2, [(0.5, "+MouseRight"), (0.7, "+KeyA"), (1.9, "-KeyA"), (1.9, "+KeyD"), (3.1, "-KeyD"),
                                          (3.5, "-MouseRight")]),
    # Drinking a lassi on the run: the free arm should keep swinging. (The first click takes the
    # cursor, as in the game; the second drinks.)
    ("drink_run", "amrita", 5.0, [(0.5, "+KeyW"), (0.6, "+MouseLeft"), (0.65, "-MouseLeft"), (0.8, "+MouseLeft"), (0.9, "-MouseLeft"), (4.5, "-KeyW")]),
]
FIRST = 9.0

# Targets: (metric, worst allowed, "max" or "min"). First guesses at a snappy, Fortnite-like feel;
# tune them once a run is agreed to feel right.
TARGETS = {
    "start_t90": (0.35, "max"),
    "stop_time": (0.3, "max"),
    "settle": (0.3, "max"),
    "land_settle": (0.35, "max"),
    "flip_time": (0.35, "max"),
    "turn_time": (0.3, "max"),
    "slide_drop": (0.25, "max"),
    "slide_getup": (0.45, "max"),
    "stride": ((0.92, 1.08), "range"),
    "pop": (5.0, "max"),
    "arch": (12.0, "max"),
    # Knees and feet aimed off the body's forward (degrees), and turning fast (degrees in 0.1 s).
    "knee": (35.0, "max"),
    "foot": (35.0, "max"),
    "knee_twist": (25.0, "max"),
    "foot_twist": (25.0, "max"),
}


# ------------------------------------------------------------------ the trace

def load(path):
    with open(path) as f:
        rows = list(csv.DictReader(f))
    if not rows:
        sys.exit(f"{path}: no frames")
    num = lambda k: np.array([float(r[k]) for r in rows])
    # Rates come from the frame clock (rt): match time (t) steps with the sim's ticks.
    d = {k: num(k) for k in ["rt", "dt", "move_x", "move_y", "yaw", "pitch", "held", "grounded", "crouched", "sliding",
                             "vx", "vy", "vz", "px", "py", "pz", "facing", "lean"]}
    d["phase"] = num("phase") if "phase" in rows[0] else np.zeros(len(rows))
    d["mt"], d["t"] = num("t"), d.pop("rt")
    for k in ["scene", "keys", "legs", "action", "clips", "mode"]:
        d[k] = [r[k] for r in rows]
    d["bones"] = np.stack([np.stack([num(f"{b}_x"), num(f"{b}_y"), num(f"{b}_z")], 1) for b in BONES], 1)  # frames x bones x 3
    return d


def take(d, idx):
    return {k: (v[idx] if isinstance(v, np.ndarray) else [v[i] for i in idx]) for k, v in d.items()}


def scenes_of(d):
    names = []
    for s in d["scene"]:
        if s and (not names or names[-1] != s):
            names.append(s)
    return [(n, take(d, np.array([i for i, s in enumerate(d["scene"]) if s == n]))) for n in names if n != "end"]


def local(d):
    """Bones in the body's frame: from the drawn body's place, turned by its facing (not its lean,
    which counts as pose); x right, y up, z forward."""
    rel = d["bones"] - np.stack([d["px"], d["py"] - HALF_HEIGHT, d["pz"]], 1)[:, None, :]
    c, s = np.cos(d["facing"])[:, None], np.sin(d["facing"])[:, None]
    # The model faces -Z at yaw 0; forward = (-sin, 0, -cos), right = (cos, 0, -sin).
    x = rel[..., 0] * c - rel[..., 2] * s
    z = -rel[..., 0] * s - rel[..., 2] * c
    return np.stack([x, rel[..., 1], z], -1)


def B(name):
    return BONES.index(name)


def torso_pitch(loc):
    """The torso's forward lean (degrees, + forward): pelvis to neck."""
    v = loc[:, B("neck_01")] - loc[:, B("pelvis")]
    return np.degrees(np.arctan2(v[:, 2], v[:, 1]))


def arch(loc):
    """The chest bent back from the waist (degrees, + a swayback): pelvis-to-spine_03 against
    spine_03-to-neck, as `just gripcheck` measures it."""
    pitch = lambda v: np.degrees(np.arctan2(v[:, 2], v[:, 1]))
    a, c, n = loc[:, B("pelvis")], loc[:, B("spine_03")], loc[:, B("neck_01")]
    return pitch(c - a) - pitch(n - c)


def speed(d):
    return np.hypot(d["vx"], d["vz"])


def after(t, t0, t1):
    return (t >= t0) & (t < t1)


def first(mask, t, default=None):
    i = np.flatnonzero(mask)
    return t[i[0]] if len(i) else default


def pose_speed(loc, t):
    """Mean speed of the bones in the body's frame (m/s), smoothed over a few frames."""
    v = np.linalg.norm(np.diff(loc, axis=0), axis=-1).mean(1) / np.maximum(np.diff(t), 1e-4)
    v = np.concatenate([v[:1], v])
    k = np.ones(5) / 5
    return np.convolve(v, k, mode="same")


# ------------------------------------------------------------------ events and numbers

def events(d):
    t, mv = d["t"], np.hypot(d["move_x"], d["move_y"])
    out = []
    for i in range(1, len(t)):
        if d["keys"][i] != d["keys"][i - 1]:
            out.append(("keys", t[i]))
    moving = mv > 0.1
    for i in range(1, len(t)):
        if moving[i] and not moving[i - 1]:
            out.append(("start", t[i]))
        if moving[i - 1] and not moving[i]:
            out.append(("stop", t[i]))
        if moving[i] and moving[i - 1]:
            a = np.array([d["move_x"][i - 1], d["move_y"][i - 1]])
            b = np.array([d["move_x"][i], d["move_y"][i]])
            if a @ b < 0:
                out.append(("flip", t[i]))
        if d["grounded"][i] and not d["grounded"][i - 1]:
            # Airborne long enough to count as a jump or fall.
            j = i - 1
            while j > 0 and not d["grounded"][j]:
                j -= 1
            if t[i] - t[j] > 0.15:
                out.append(("land", t[i]))
        if d["sliding"][i] and not d["sliding"][i - 1]:
            out.append(("slide", t[i]))
        dyaw = (d["yaw"][i] - d["yaw"][i - 1] + math.pi) % (2 * math.pi) - math.pi
        if abs(dyaw) > 0.02 and not any(e[0] == "turn" and t[i] - e[1] < 0.5 for e in out):
            out.append(("turn", t[i]))
    return sorted(out, key=lambda e: e[1])


def measure(d):
    """Numbers for one scene (or a mark): {metric: value}, and notes per event."""
    t = d["t"]
    loc = local(d)
    sp = speed(d)
    ps = pose_speed(loc, t)
    pel = loc[:, B("pelvis"), 1]
    pitch = torso_pitch(loc)
    m, notes = {}, []
    put = lambda k, v: m.__setitem__(k, max(m.get(k, v), v)) if v is not None else None
    evs = events(d)
    for k, (kind, at) in enumerate(evs):
        # Up to the next key change (a sprint pressed later isn't this start's top speed).
        nxt = min([e for _, e in evs[k + 1:] if e > at + 0.05] + [at + 1.6])
        if kind == "start":
            w = after(t, at, nxt)
            top = sp[w].max() if w.any() else 0
            t90 = first(w & (sp >= 0.9 * top), t)
            if top > 0.5 and t90 is not None:
                put("start_t90", t90 - at)
                lw = after(t, at, at + 0.5)
                notes.append(f"start {at:.2f}: 90% of {top:.1f} m/s in {t90 - at:.2f}s; lean peak {pitch[lw].max():+.0f} deg")
        elif kind == "stop":
            w = after(t, at, at + 1.5)
            v0 = sp[np.searchsorted(t, at) - 1] if np.searchsorted(t, at) > 0 else 0
            still = first(w & (sp < 0.2), t)
            if v0 > 0.5 and still is not None:
                put("stop_time", still - at)
                i0, i1 = np.searchsorted(t, at), np.searchsorted(t, still)
                dist = np.sum(sp[i0:i1] * d["dt"][i0:i1])
                # Settle: the body still moving (in its own frame) after the capsule stopped, until
                # it's held still for 0.1 s.
                ws = after(t, still, still + 1.5)
                calm = ws & (ps < 0.25)
                settle = None
                for i in np.flatnonzero(calm):
                    j = np.searchsorted(t, t[i] + 0.1)
                    if j <= len(t) and np.all(ps[i:j] < 0.25):
                        settle = t[i] - still
                        break
                settle = settle if settle is not None else 1.5
                put("settle", settle)
                lw = after(t, at, still + 0.6)
                notes.append(f"stop {at:.2f}: from {v0:.1f} m/s, still in {still - at:.2f}s over {dist:.2f} m; body settles {settle:.2f}s later; "
                             f"lean {pitch[lw].min():+.0f}..{pitch[lw].max():+.0f} deg")
        elif kind == "land":
            # Settled: the airborne clips (jump, fall) faded out, the ground's clips playing. The
            # dip: the pelvis's lowest just after against its usual low (a run's own bob is no dip).
            air = np.array([sum(float(p.rsplit("=", 1)[1]) for p in c.split("|") if p and p.split("=")[0].removeprefix("rifle_").rstrip("~") in ("jump", "fall"))
                            for c in d["clips"]])
            w = after(t, at, at + 0.8)
            later = after(t, at + 0.8, at + 1.6)
            if w.any():
                usual = np.percentile(pel[later], 5) if later.sum() > 10 else np.median(pel[w][-5:])
                dip = usual - pel[w].min()
                done = first(w & (air < 0.1), t)
                settle = (done - at) if done is not None else 0.8
                put("land_dip", dip * 100)
                put("land_settle", settle)
                notes.append(f"land {at:.2f}: pelvis dips {dip * 100:.1f} cm below its usual low, back on the ground's clips in {settle:.2f}s")
        elif kind == "flip":
            i = np.searchsorted(t, at)
            v_before = np.array([d["vx"][i - 1], d["vz"][i - 1]])
            w = after(t, at, at + 1.2)
            turned = first(w & ((d["vx"] * v_before[0] + d["vz"] * v_before[1]) < -0.8 * (v_before @ v_before)), t)
            if turned is not None:
                put("flip_time", turned - at)
                notes.append(f"flip {at:.2f}: velocity reversed in {turned - at:.2f}s")
        elif kind == "turn":
            i = np.searchsorted(t, at)
            w = after(t, at, at + 1.0)
            # Where the view ends up, then when the body faces within 10 degrees of it (the drawn
            # body faces the motion when just running: compare with the yaw of the velocity).
            end = np.flatnonzero(w)[-1]
            goal = d["yaw"][end]
            off = np.abs((d["facing"] - goal + math.pi) % (2 * math.pi) - math.pi)
            faced = first(w & (off < math.radians(10)), t)
            lean = np.degrees(np.abs(d["lean"][w])).max()
            if faced is not None:
                put("turn_time", faced - at)
            put("turn_lean", lean)
            notes.append(f"turn {at:.2f}: faces the new way in {(faced - at) if faced else float('nan'):.2f}s; leans {lean:.1f} deg")
        elif kind == "slide":
            w = after(t, at - 0.2, at + 1.5)
            stand = np.median(pel[after(t, at - 0.4, at - 0.05)]) if after(t, at - 0.4, at - 0.05).any() else pel[w][0]
            low = pel[w].min()
            down = first(after(t, at, at + 1.0) & (pel <= stand - 0.9 * (stand - low)), t)
            end = first(after(t, at, at + 4.0) & (d["sliding"] < 0.5), t)
            if down is not None:
                put("slide_drop", down - at)
            note = f"slide {at:.2f}: pelvis down {(stand - low) * 100:.0f} cm in {(down - at) if down else float('nan'):.2f}s"
            if end is not None:
                up_at = first(after(t, end, end + 2.0) & (pel >= stand - 0.1 * (stand - low)), t)
                crouched = d["crouched"][np.searchsorted(t, end)] > 0.5
                if up_at is not None and not crouched:
                    put("slide_getup", up_at - end)
                note += f"; ends {end:.2f}, " + (f"up {up_at - end:.2f}s later" if up_at else "stays low") + (" (crouched)" if crouched else "")
            notes.append(note)

    # Planted feet. A foot on the ground moves back through the body's frame at the ground speed:
    # `stride` is how fast it does (1 = no skating; 1.16 = the feet run 16% fast and slide
    # forward); `foot_slide` its leftover speed over the ground (cm/s).
    ground = (d["grounded"] > 0.5) & (d["sliding"] < 0.5)
    world = d["bones"]
    dt = np.maximum(np.diff(t), 1e-4)
    c, sn = np.cos(d["facing"]), np.sin(d["facing"])
    gv = np.stack([d["vx"] * c - d["vz"] * sn, -d["vx"] * sn - d["vz"] * c], 1)  # body frame (x right, z forward)
    ratios, slides = [], []
    for foot in ["ball_l", "ball_r"]:
        h = loc[:, B(foot), 1]
        # The foot's lowest while running, whatever else the window holds.
        run = ground & (sp > 2.5)
        floor = np.percentile(h[run], 3) if run.sum() > 10 else 0
        lv = np.diff(loc[:, B(foot)][:, [0, 2]], axis=0) / dt[:, None]
        p = world[:, B(foot)]
        vh = np.hypot(np.diff(p[:, 0]), np.diff(p[:, 2])) / dt
        planted = (h[1:] < floor + 0.03) & ground[1:] & (sp[1:] > 2.5)
        g = gv[1:][planted]
        ratios.append(-(lv[planted] * g).sum(1) / np.maximum((g * g).sum(1), 1e-4))
        slides.append(vh[planted])
    r, sl = np.concatenate(ratios), np.concatenate(slides)
    if len(r) > 10:
        m["stride"] = float(np.median(r))
        m["foot_slide"] = float(np.median(sl) * 100)
    # Pops: a bone off the path its last two frames predict (body frame, scaled for uneven frame
    # times), by far more than it strays nearby (a foot striking the ground or an arm swinging
    # back stray a little every stride; a blend snapping or a clip's seam jumps out of that).
    # Reported as the jump in cm; the scene's first frames (its teleport) left out.
    if len(t) > 40:
        h = np.diff(t)
        r = (h[1:] / np.maximum(h[:-1], 1e-4))[:, None, None]
        off = np.linalg.norm(loc[2:] - loc[1:-1] - (loc[1:-1] - loc[:-2]) * r, axis=-1) * 100  # frames x bones, cm
        from numpy.lib.stride_tricks import sliding_window_view
        pad = np.pad(off, ((15, 15), (0, 0)), mode="edge")
        usual = np.median(sliding_window_view(pad, 31, axis=0), axis=-1)
        spike = np.where(off > 4 * usual + 1.5, off, 0)
        spike[:10] = 0
        i, b = np.unravel_index(int(np.argmax(spike)), spike.shape)
        m["pop"] = float(spike[i, b])
        if spike[i, b] > TARGETS["pop"][0]:
            notes.append(f"pop {t[i + 1]:.2f}: {BONES[b]} jumps {spike[i, b]:.1f} cm (usual {usual[i, b]:.1f}; {d['legs'][i + 1]} {d['action'][i + 1]})")
    # Through the run: arm swing and torso pitch at speed, and the worst arch.
    fast = ground & (sp > 4.0)
    if fast.sum() > 20:
        hz = loc[fast][:, B("hand_l"), 2]
        m["arm_swing"] = float((np.percentile(hz, 95) - np.percentile(hz, 5)) * 100)
        m["run_pitch"] = float(np.median(pitch[fast]))
        m["bob"] = float((np.percentile(pel[fast], 95) - np.percentile(pel[fast], 5)) * 100)
    # Legs: which way each knee and foot points from the body's forward (degrees, + outward), on
    # the ground and in the air. A knee is aimed where the leg bends (only read while it bends 4 cm
    # or more off the hip-to-ankle line); a twist is the knee or foot turning fast (degrees in 0.1 s).
    for where, w in (("ground", ground), ("air", (d["grounded"] < 0.5) & (np.array(d["mode"]) == "ground"))):
        worst = {}
        for side, sx in (("l", -1), ("r", 1)):
            hip, knee, ankle, ball = (loc[:, B(f"{b}_{side}")] for b in ("thigh", "calf", "foot", "ball"))
            line = ankle - hip
            u = line / np.maximum(np.linalg.norm(line, axis=1, keepdims=True), 1e-4)
            off = (knee - hip) - ((knee - hip) * u).sum(1, keepdims=True) * u
            bent = np.linalg.norm(off[:, [0, 2]], axis=1) > 0.04
            kyaw = np.degrees(np.arctan2(off[:, 0] * sx, off[:, 2]))
            fv = ball - ankle
            fyaw = np.degrees(np.arctan2(fv[:, 0] * sx, fv[:, 2]))
            # (Only while the foot lies near level: one kicked up behind points anywhere.)
            flat = np.linalg.norm(fv[:, [0, 2]], axis=1) > 0.7 * np.linalg.norm(fv, axis=1)
            for k, y, ok in (("knee", kyaw, w & bent), ("foot", fyaw, w & flat)):
                if ok.sum() < 5:
                    continue
                i = np.flatnonzero(ok)[np.argmax(np.abs(y[ok]))]
                if abs(y[i]) > abs(worst.get(k, (0, 0, ""))[0]):
                    worst[k] = (float(y[i]), t[i], f"{side} {d['legs'][i]} {d['action'][i]}")
                # Twist: the change over 0.1 s, within the window.
                j = np.searchsorted(t, t + 0.1).clip(0, len(t) - 1)
                both = ok & ok[j]
                if both.any():
                    dy = np.abs((y[j] - y + 180) % 360 - 180)
                    i = np.flatnonzero(both)[np.argmax(dy[both])]
                    if dy[i] > worst.get(k + "_twist", (0, 0, ""))[0]:
                        worst[k + "_twist"] = (float(dy[i]), t[i], f"{side} {d['legs'][i]} {d['action'][i]} {y[i]:+.0f}->{y[j[i]]:+.0f}")
        for k, (v, at, what) in worst.items():
            m[f"{where}_{k}"] = abs(v)
            lim = TARGETS.get(f"{k}"[:4] + ("_twist" if "twist" in k else ""), (None,))[0]
            if lim is not None and abs(v) > lim:
                notes.append(f"{where} {k} {at:.2f}: {v:+.0f} deg ({what})")
    a = arch(loc)
    upright = ground & (d["crouched"] < 0.5)
    if upright.any():
        i = np.flatnonzero(upright)[np.argmax(a[upright])]
        m["arch"] = float(a[i])
        if a[i] > TARGETS["arch"][0]:
            notes.append(f"arch {t[i]:.2f}: back bent {a[i]:.0f} deg ({d['legs'][i]} {d['action'][i]})")
    return m, notes


def verdict(k, v):
    k = k.removeprefix("ground_").removeprefix("air_")
    if k not in TARGETS:
        return ""
    lim, how = TARGETS[k]
    if how == "range":
        return f"! outside {lim[0]:g}-{lim[1]:g}" if not lim[0] <= v <= lim[1] else "ok"
    bad = v > lim if how == "max" else v < lim
    return f"! over {lim:g}" if bad and how == "max" else (f"! under {lim:g}" if bad else "ok")


UNITS = {"start_t90": "s", "stop_time": "s", "settle": "s", "land_settle": "s", "flip_time": "s", "turn_time": "s",
         "slide_drop": "s", "slide_getup": "s", "land_dip": "cm", "foot_slide": "cm/s", "stride": "x",
         "pop": "cm", "arch": "deg", "arm_swing": "cm", "bob": "cm", "run_pitch": "deg", "turn_lean": "deg",
         **{f"{w}_{k}": "deg" for w in ("ground", "air") for k in ("knee", "foot", "knee_twist", "foot_twist")}}


def check(path, quiet_notes=False):
    d = load(path)
    base = json.load(open(BASELINE)) if os.path.exists(BASELINE) else {}
    result, bad = {}, 0
    for name, s in scenes_of(d):
        # The first frames are the scene's start (the teleport); leave them out.
        s = take(s, np.flatnonzero(s["t"] >= s["t"][0] + 0.15))
        m, notes = measure(s)
        result[name] = m
        print(f"[feel] {name}")
        for k, v in m.items():
            was = base.get(name, {}).get(k)
            # Runs differ a little (frame timing): stride by about 0.05, foot slide by 20%.
            delta = f"  (was {was:.2f})" if was is not None and abs(was - v) > 0.05 * max(abs(was), 1) else ""
            mark = verdict(k, v)
            bad += mark.startswith("!")
            print(f"[feel]   {k:15} {v:7.2f} {UNITS.get(k, ''):5} {mark:12}{delta}")
        if not quiet_notes:
            for n in notes:
                print(f"[feel]     - {n}")
    os.makedirs(OUT, exist_ok=True)
    json.dump(result, open(LAST, "w"), indent=1)
    print(f"[feel] {len(result)} scenes, {bad} over target")
    return result


# ------------------------------------------------------------------ pictures

def plot(path, only=None, out=OUT, title=None):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    os.makedirs(out, exist_ok=True)
    d = load(path)
    # A mark is one window, whatever scenes it crosses.
    scenes = [(title, d)] if title else (scenes_of(d) or [("trace", d)])
    made = []
    for name, s in scenes:
        if only and name not in only:
            continue
        s = s if title else take(s, np.flatnonzero(s["t"] >= s["t"][0] + 0.15))
        t = s["t"] - s["t"][0]
        loc = local(s)
        fig, ax = plt.subplots(4, 1, figsize=(9, 9), sharex=True)
        fig.suptitle(name)
        # Input and speed.
        ax[0].plot(t, speed(s), "k", lw=1.5, label="speed m/s")
        ax[0].plot(t, s["vy"], color="0.6", lw=1, label="vertical")
        ax[0].plot(t, pose_speed(loc, s["t"]), color="tab:purple", lw=1, label="pose speed")
        prev = ""
        for i, k in enumerate(s["keys"]):
            if k != prev:
                ax[0].axvline(t[i], color="tab:orange", lw=0.8)
                ax[0].text(t[i], ax[0].get_ylim()[1] if i else 9, k or "-", fontsize=7, rotation=90, va="top", color="tab:orange")
                prev = k
        for kind, at in [e for e in events(s) if e[0] != "keys"]:
            for a in ax:
                a.axvline(at - s["t"][0], color="tab:red", lw=0.6, ls=":")
            ax[3].text(at - s["t"][0], 0, kind, fontsize=7, color="tab:red")
        ax[0].legend(fontsize=7, loc="upper right")
        # Clip weights: the four heaviest seen.
        weights = {}
        for i, c in enumerate(s["clips"]):
            for part in filter(None, c.split("|")):
                n, w = part.rsplit("=", 1)
                weights.setdefault(n, np.zeros(len(t)))[i] = float(w)
        for n in sorted(weights, key=lambda n: -weights[n].sum())[:6]:
            ax[1].plot(t, weights[n], lw=1.2, label=n)
        ax[1].set_ylabel("clip weight")
        ax[1].legend(fontsize=7, loc="upper right", ncol=2)
        # The body.
        ax[2].plot(t, loc[:, B("pelvis"), 1] * 100, label="pelvis height cm")
        ax[2].legend(fontsize=7, loc="upper right")
        ax[3].plot(t, torso_pitch(loc), label="torso pitch deg")
        ax[3].plot(t, arch(loc), label="arch deg")
        ax[3].plot(t, np.degrees(s["lean"]), label="turn lean deg")
        ax[3].axhline(0, color="0.8", lw=0.6)
        ax[3].legend(fontsize=7, loc="upper right")
        ax[3].set_xlabel("seconds")
        fig.tight_layout()
        p = os.path.join(out, f"{name}_graph.png")
        fig.savefig(p, dpi=80)
        plt.close(fig)
        made.append(p)
        made.append(ghosts(s, name, out))
    for p in made:
        print(f"[feel] {os.path.relpath(p, ROOT)}")
    return made


def ghosts(s, name, out):
    """Stick figures every 0.1 s, coloured by time, from the side (along the run's start facing)
    and from above: their spacing shows the timing (ease in and out, holds), their shapes the
    poses and arcs."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    t = s["t"]
    yaw0 = s["facing"][0]
    fwd, right = np.array([-math.sin(yaw0), -math.cos(yaw0)]), np.array([math.cos(yaw0), -math.sin(yaw0)])
    o = np.array([s["px"][0], s["pz"][0]])
    w = s["bones"]
    along = (w[..., 0] - o[0]) * fwd[0] + (w[..., 2] - o[1]) * fwd[1]
    across = (w[..., 0] - o[0]) * right[0] + (w[..., 2] - o[1]) * right[1]
    up = w[..., 1] - (s["py"][0] - HALF_HEIGHT)
    picks, last = [], -1.0
    for i, ti in enumerate(t):
        if ti - last >= 0.1:
            picks.append(i)
            last = ti
    cmap = plt.get_cmap("viridis")
    span = max(along.max() - along.min(), 2.0)
    fig, ax = plt.subplots(2, 1, figsize=(min(4 + span * 0.55, 16), 6.5), gridspec_kw={"height_ratios": [1, 1]})
    fig.suptitle(f"{name}: every 0.1 s, dark to light")
    for k, i in enumerate(picks):
        c = cmap(k / max(len(picks) - 1, 1))
        for a, b in LIMBS:
            ia, ib = B(a), B(b)
            ax[0].plot([along[i, ia], along[i, ib]], [up[i, ia], up[i, ib]], color=c, lw=1.1, alpha=0.85)
            ax[1].plot([along[i, ia], along[i, ib]], [across[i, ia], across[i, ib]], color=c, lw=1.0, alpha=0.75)
        ax[0].plot(along[i, B("head")], up[i, B("head")], "o", color=c, ms=3)
    # Key changes along the path.
    prev = ""
    for i, k in enumerate(s["keys"]):
        if k != prev:
            x = along[i, B("pelvis")]
            ax[0].axvline(x, color="tab:orange", lw=0.6, ls=":")
            ax[0].text(x, 2.05, k or "-", fontsize=7, color="tab:orange", rotation=90, va="top")
            prev = k
    ax[0].set_aspect("equal")
    ax[0].set_ylim(-0.05, 2.1)
    ax[0].set_ylabel("side: up (m)")
    ax[1].set_aspect("equal")
    ax[1].set_ylabel("top: across (m)")
    ax[1].set_xlabel("along the run (m)")
    fig.tight_layout()
    p = os.path.join(out, f"{name}_ghosts.png")
    fig.savefig(p, dpi=80)
    plt.close(fig)
    return p


# ------------------------------------------------------------------ running the game

def script(only=None):
    keys, scenes, at = [], [], FIRST
    for name, item, dur, evs in SCENES:
        if only and name not in only:
            continue
        scenes.append(f"{name}:{at:.2f}:{item}")
        keys += [(at + dt, e) for dt, e in evs]
        at += dur
    scenes.append(f"end:{at:.2f}:")
    keys.sort(key=lambda k: k[0])
    return ",".join(scenes), ",".join(f"{t:.2f}:{e}" for t, e in keys)


def run(only=None):
    os.makedirs(OUT, exist_ok=True)
    scenes, keys = script(only)
    env = dict(os.environ, VIVAD_MUTE="1", VIVAD_BOTS="1", VIVAD_MAP="cp", VIVAD_MAP_SEED="7",
               VIVAD_FEEL=scenes, VIVAD_KEYS=keys, VIVAD_TRACE=TRACE)
    # Never measure an old run's trace by mistake.
    if os.path.exists(TRACE):
        os.remove(TRACE)
    p = subprocess.run(["cargo", "run", "--release", "-q", "-p", "game", "--bin", "game"], cwd=ROOT, env=env,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for line in p.stdout.splitlines():
        if line.startswith("[feel]") or "panicked" in line or line.startswith("error"):
            print(line)
    if not os.path.exists(TRACE) or "[feel] scene end" not in p.stdout:
        print("\n".join(p.stdout.splitlines()[-20:]))
        sys.exit(f"[feel] the game stopped before the scenes ended (exit {p.returncode}): no numbers")
    check(TRACE)


def newest_mark():
    marks = sorted(glob.glob(os.path.join(ROOT, "marks", "mark_*.csv")))
    if not marks:
        sys.exit("no marks yet: press F9 in the game when something feels off")
    return marks[-1]


def mark(path):
    path = path or newest_mark()
    d = load(path)
    t = d["t"]
    print(f"[feel] {os.path.relpath(path, ROOT)}: {t[-1] - t[0]:.1f}s ending at match time {d["mt"][-1]:.1f}")
    # What happened, as a timeline: keys, the clip on show, the state.
    prev = None
    for i in range(len(t)):
        state = (d["keys"][i], d["legs"][i], d["action"][i], int(d["grounded"][i]), int(d["sliding"][i]), int(d["crouched"][i]))
        if state != prev:
            print(f"[feel]   {t[i] - t[-1]:+6.2f}s keys [{state[0]}] {state[1]} {state[2]} speed {speed(d)[i]:.1f}"
                  + ("" if state[3] else " air") + (" slide" if state[4] else "") + (" low" if state[5] else ""))
            prev = state
    m, notes = measure(d)
    for k, v in m.items():
        print(f"[feel]   {k:15} {v:7.2f} {UNITS.get(k, ''):5} {verdict(k, v)}")
    for n in notes:
        print(f"[feel]     - {n}")
    name = os.path.splitext(os.path.basename(path))[0]
    plot(path, out=os.path.join(OUT, "marks", name), title=name)
    png = os.path.splitext(path)[0] + ".png"
    if os.path.exists(png):
        print(f"[feel] {os.path.relpath(png, ROOT)} (screenshot)")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--only", default="")
    c = sub.add_parser("check")
    c.add_argument("trace", nargs="?", default=TRACE)
    p = sub.add_parser("plot")
    p.add_argument("trace", nargs="?", default=TRACE)
    p.add_argument("scenes", nargs="*")
    sub.add_parser("keep")
    k = sub.add_parser("mark")
    k.add_argument("file", nargs="?")
    a = ap.parse_args()
    if a.cmd == "run":
        run([s for s in a.only.split(",") if s] or None)
    elif a.cmd == "check":
        check(a.trace)
    elif a.cmd == "plot":
        plot(a.trace, a.scenes or None)
    elif a.cmd == "keep":
        if not os.path.exists(LAST):
            sys.exit("no run yet")
        with open(BASELINE, "w") as f:
            f.write(open(LAST).read())
        print(f"[feel] baseline kept: {os.path.relpath(BASELINE, ROOT)}")
    elif a.cmd == "mark":
        mark(a.file)


if __name__ == "__main__":
    main()
