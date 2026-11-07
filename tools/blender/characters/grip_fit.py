"""How a long gun's pistol grip sits in the right hand, by the skin: the male (on the rifle pack's
skeleton, its clips exact) plays each held clip and his right hand's skin is measured against the
grip, an elliptic column raked back (desi.py's grip_slab, in the gun's frame: barrel +Y, up +Z,
the origin the weapon bone). Per clip: how much of the fingers' and palm's skin lies on it (within
4 mm), how deep any goes into it, and the widest gap between the closed fingers and it (`!`
past 4 mm in, or 6 mm off).
`--fit` searches the column's size and place for the most skin on it with none through it.

  blender -b -t 1 build/anims_male.blend -P tools/blender/characters/grip_fit.py [-- --fit]
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "weapons"))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))

import bpy  # noqa: E402

import skeleton  # noqa: E402

CLIPS = ["rifle_idle", "rifleaim_idle", "rifle_run_f", "rifle_walk_f", "rifle_crouch_f", "rifleaim_run_f", "rifle_crouch_idle"]
HAND = ("hand_r", "thumb_", "index_", "middle_", "ring_", "pinky_")
# The fingers that close round the grip (the index lies along the receiver, off the trigger).
CLOSE = ("middle_", "ring_", "pinky_")


def grip_of(model="bandook"):
    """The model's grip column (desi.GRIPS): top and bottom heights, rake, half width, half depth,
    centre across and along at the top."""
    import desi
    return desi.GRIPS[model]


def hand_points(frames_per_clip=4):
    """The right hand's skin in the gun's frame, each point with whether a closing finger carries it,
    per clip and frame."""
    arm = bpy.data.objects["Human.rig"]
    body = bpy.data.objects["male_LOD0"]
    sc = bpy.context.scene
    groups = {g.index: g.name for g in body.vertex_groups}
    hand, closing = [], {}
    for v in body.data.vertices:
        w = sum(e.weight for e in v.groups if groups[e.group].startswith(HAND) and groups[e.group].endswith("_r"))
        if w > 0.5:
            hand.append(v.index)
            # A closing finger's last two joints (its first carries the back of the knuckle too).
            for k, f in enumerate(CLOSE):
                if sum(e.weight for e in v.groups if groups[e.group] in (f"{f}02_r", f"{f}03_r")) > 0.5:
                    closing[v.index] = k + 1
    out = {}
    for name in CLIPS:
        act = bpy.data.actions.get(name)
        if act is None:
            continue
        arm.animation_data.action = act
        f0, f1 = map(int, act.frame_range)
        out[name] = []
        for k in range(frames_per_clip):
            sc.frame_set(f0 + (f1 - f0) * k // frames_per_clip)
            G = (arm.matrix_world @ arm.pose.bones[skeleton.WEAPON].matrix @ skeleton.TO_GLTF).inverted()
            dg = bpy.context.evaluated_depsgraph_get()
            e = body.evaluated_get(dg)
            m = e.to_mesh()
            W = G @ body.matrix_world
            import numpy as np
            P = np.array([tuple(W @ m.vertices[i].co) for i in hand])
            which = np.array([closing.get(i, 0) for i in hand])
            out[name].append((P, which > 0, which))
            e.to_mesh_clear()
    return out


def gaps(P, g):
    """Signed distance (m, about) from each point of `P` (N x 3) to the grip's surface: < 0 inside,
    NaN above or below it."""
    import numpy as np
    top, bottom, rake, a, b, cx, cy = g
    dx, dy = P[:, 0] - cx, P[:, 1] - (cy - (top - P[:, 2]) * rake)
    r = np.hypot(dx, dy)
    c, s = dx / np.maximum(r, 1e-9), dy / np.maximum(r, 1e-9)
    d = r - a * b / np.hypot(b * c, a * s)
    d[(P[:, 2] > top) | (P[:, 2] < bottom)] = np.nan
    return d


def score(frames, g):
    """Skin on the grip (within 4 mm, per frame), the deepest through it, and the widest gap
    between a closed finger's end and it (each finger's nearest skin), over `frames` ((points,
    closing mask, which finger) each)."""
    import numpy as np
    on, deep, wide = 0, 0.0, 0.0
    for P, closing, band in frames:
        d = gaps(P, g)
        ok = ~np.isnan(d)
        on += int(np.sum(ok & (d > -0.002) & (d < 0.004)))
        if ok.any():
            deep = min(deep, float(np.nanmin(d)))
        m = ok & closing
        if m.any():
            nearest = {}
            for k, v in zip(band[m], d[m]):
                nearest[k] = min(nearest.get(k, 1.0), v)
            wide = max(wide, max(nearest.values()))
    return on / max(len(frames), 1), deep, wide


def main():
    pts = hand_points()
    fit = "--fit" in sys.argv
    g = grip_of()
    for name, frames in pts.items():
        on, deep, wide = score(frames, g)
        # A knuckle's inner crease bunches a few millimetres into what the finger wraps.
        flag = "" if deep > -0.004 and wide < 0.006 else "  !"
        print(f"[grip_fit] {name:18} skin on the grip {on:5.0f} pts/frame, deepest in {deep * 1000:5.1f} mm, closed fingers off by {wide * 1000:4.1f} mm at most{flag}")
    if not fit:
        return
    allf = [f for frames in pts.values() for f in frames]
    top = g[0]
    best = None
    for bottom in (-0.07, -0.08, -0.09):
        for rake in (0.2, 0.28, 0.36):
            for a in [0.014 + 0.002 * i for i in range(9)]:
                for b in [0.016 + 0.002 * i for i in range(12)]:
                    for cx in (-0.004, 0.0, 0.004, 0.008):
                        for cy in [-0.012 + 0.004 * i for i in range(8)]:
                            c = (top, bottom, rake, a, b, cx, cy)
                            on, deep, wide = score(allf, c)
                            if deep < -0.003:
                                continue
                            key = (on - 400 * max(0.0, wide - 0.004), on)
                            if best is None or key > best[0]:
                                best = (key, c, on, deep, wide)
    key, c, on, deep, wide = best
    print(f"[grip_fit] best: top {c[0]} bottom {c[1]} rake {c[2]} half width {c[3]:.3f} half depth {c[4]:.3f} centre {c[5]:.3f} {c[6]:.3f}: "
          f"{on:.0f} pts/frame on, deepest {deep * 1000:.1f} mm, fingers off {wide * 1000:.1f} mm")


if __name__ == "__main__":
    main()
