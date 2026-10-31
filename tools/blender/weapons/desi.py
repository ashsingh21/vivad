"""Panga's street armoury: the cricket bat, the katta (a country-made single-shot pistol), the
tamancha (a crude self-loader with a long magazine), the dunali (a double-barrelled shotgun) and the
gulel (a forked-branch slingshot). Lofted, given procedural materials, baked to one texture set and
exported like the rest of the armoury (arsenal.py).

Axes (docs/references.md): origin where the hand closes. The bat's blade runs Blender +Z (glTF +Y),
its face toward Blender +Y. The guns lie like a bow raised to shoot: the barrel points Blender +Y
(glTF -Z, the target), up is Blender +Z, the grip hangs below the origin. The gulel's fork stands
up (+Z) with its tips side by side (±X); its bands are drawn by the game from the tips, which this
script writes to assets/data/bows.ron (mirror_x).

blender -b --factory-startup -P tools/blender/weapons/desi.py [-- name ...]
"""

import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

from mathutils import Vector  # noqa: E402

import arsenal as A  # noqa: E402
import export  # noqa: E402
import finishes as F  # noqa: E402
import geo  # noqa: E402

X, Y, Z = Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1))

GUNMETAL = (0.20, 0.20, 0.21)
RUST = (0.30, 0.16, 0.08)


def box(name, lo, hi, mat):
    """An axis-aligned block from corner `lo` to `hi`, lofted along Y."""
    lo, hi = Vector(lo), Vector(hi)
    cx, cz = (lo.x + hi.x) / 2, (lo.z + hi.z) / 2
    hx, hz = (hi.x - lo.x) / 2, (hi.z - lo.z) / 2
    ring = [(hx, -hz), (hx, hz), (-hx, hz), (-hx, -hz)]
    frames = [(Vector((cx, y, cz)), X.copy(), Z.copy()) for y in (lo.y, hi.y)]
    return geo.loft(name, frames, [ring, ring], cap_start="fan", cap_end="fan", material=mat, merge=True)


def rod(name, a, b, r, mat, seg=12):
    """A straight round bar from `a` to `b`."""
    a, b = Vector(a), Vector(b)
    pts = [a.lerp(b, t / 3) for t in range(4)]
    return A.tube(name, pts, lambda t: r, mat, seg=seg, caps=("fan", "fan"), up=Z if abs((b - a).normalized().z) < 0.9 else X)


def bent(name, pts, r, mat, seg=8):
    """A round bar bent through `pts` (trigger guards, hammers, wire)."""
    pts = geo.resample_even(geo.catmull_rom([Vector(p) for p in pts], 24), 16)
    return A.tube(name, pts, lambda t: r, mat, seg=seg, caps=("fan", "fan"), up=X)


def grip_slab(name, top, bottom, half_w, depth, mat, rake=0.28):
    """A pistol grip: a rounded slab from `top` down to `bottom` (z), raked back by `rake`."""
    S = 6
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        z = top + (bottom - top) * u
        c = Vector((0, -(top - z) * rake, z))
        frames.append((c, X.copy(), Y.copy()))
        rings.append(geo.ellipse(half_w, depth / 2 * (1 - 0.1 * u), 12))
    return geo.loft(name, frames, rings, cap_start="fan", cap_end="fan", material=mat, merge=True)


# ------------------------------------------------------------------ the bat

def bat():
    """A gully cricket bat: a willow blade (flat face, a ridged back, rounded toe), a cane handle
    in a red rubber grip, and a sticker on the face."""
    willow = F.plain("bat_willow", (0.82, 0.68, 0.46), rough=0.55)
    grip = F.wrap("bat_grip", (0.62, 0.06, 0.06), (0.15, 0.02, 0.02), pitch=0.006)
    sticker = F.plain("bat_sticker", (0.95, 0.55, 0.08), rough=0.4)
    parts = [A.lathe("handle", [(0.0, -0.15), (0.017, -0.15), (0.0175, -0.13), (0.0168, 0.0), (0.0172, 0.13), (0.0, 0.14)],
                     grip, seg=16)]
    parts.append(A.lathe("knob", [(0.0, -0.165), (0.016, -0.162), (0.02, -0.152), (0.0175, -0.145), (0.0, -0.145)], grip, seg=16))
    # Blade: cross-sections from the shoulders to the toe. Face flat at +Y; the back swells into a
    # spine, deepest a third of the way down (the sweet spot).
    S = 14
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        z = 0.13 + 0.58 * u
        half = 0.054 * min(1.0, 0.55 + u * 6.0) if u < 0.08 else 0.054
        toe = max(0.0, (u - 0.9) / 0.1)
        half *= math.sqrt(max(0.05, 1 - toe ** 2)) if toe > 0 else 1.0
        spine = 0.018 + 0.026 * math.exp(-((u - 0.6) / 0.35) ** 2)
        edge = 0.016 + 0.006 * u
        n = 9
        ring = [(half * (1 - 2 * k / (n - 1)), 0.012) for k in range(n)]
        for k in range(n):
            x = -half + 2 * half * k / (n - 1)
            f = 1 - (x / half) ** 2
            ring.append((x, 0.012 - edge - (spine - edge) * f ** 0.7))
        frames.append((Vector((0, 0, z)), X.copy(), Y.copy()))
        rings.append(ring)
    parts.append(geo.loft("blade", frames, rings, cap_start="fan", cap_end="fan", material=willow, merge=True))
    parts.append(box("sticker", (-0.04, 0.0119, 0.5), (0.04, 0.0135, 0.62), sticker))
    A.finish("bat", parts, res=1024)


# ------------------------------------------------------------------ guns

def katta():
    """A country-made single-shot pistol: a plain steel tube for a barrel, bound to a welded
    breech with wire, an exposed hammer and a bent-rod trigger guard, and a slab of rough wood for
    a grip held on by two rivets."""
    steel = F.metal("katta_steel", GUNMETAL, rough=0.5, tarnish=RUST)
    wood = F.plain("katta_wood", (0.30, 0.17, 0.08), rough=0.75)
    wire = F.metal("katta_wire", (0.55, 0.42, 0.30), rough=0.4)
    axis = 0.038
    parts = [A.lathe("barrel", [(0.0, -0.02), (0.012, -0.02), (0.012, 0.165), (0.0085, 0.166), (0.0085, 0.15), (0.0, 0.15)],
                     steel, seg=16, axis=Y, origin=Vector((0, 0, axis)))]
    parts.append(box("breech", (-0.014, -0.06, axis - 0.018), (0.014, 0.0, axis + 0.016), steel))
    parts.append(box("frame", (-0.008, -0.05, 0.0), (0.008, 0.035, axis - 0.012), steel))
    for k, y in enumerate((0.03, 0.09)):
        parts.append(A.lathe(f"wire{k}", [(0.0128, y - 0.006), (0.0138, y - 0.005), (0.0138, y + 0.005), (0.0128, y + 0.006)],
                             wire, seg=16, axis=Y, origin=Vector((0, 0, axis))))
    parts.append(bent("hammer", [(0, -0.055, axis + 0.004), (0, -0.07, axis + 0.02), (0, -0.064, axis + 0.034)], 0.004, steel))
    parts.append(bent("guard", [(0, 0.03, 0.002), (0, 0.032, -0.022), (0, 0.0, -0.03), (0, -0.02, -0.02), (0, -0.02, 0.0)], 0.0028, steel))
    parts.append(bent("trigger", [(0, 0.0, 0.004), (0, 0.006, -0.012), (0, 0.0, -0.02)], 0.0035, steel))
    parts.append(grip_slab("grip", 0.004, -0.085, 0.015, 0.032, wood))
    for k, z in enumerate((-0.02, -0.06)):
        y = -(0.004 - z) * 0.28
        parts.append(rod(f"rivet{k}", (-0.016, y, z), (0.016, y, z), 0.003, steel, seg=8))
    A.finish("katta", parts, res=1024, hard=("breech", "frame"))


def tamancha():
    """A crude self-loading pistol: a blocky slide with filed grip serrations, a barrel stub at the
    muzzle, a long twenty-round magazine out of the grip and a taped handle."""
    steel = F.metal("tamancha_steel", (0.16, 0.16, 0.17), rough=0.42, tarnish=RUST)
    tape = F.wrap("tamancha_tape", (0.06, 0.06, 0.06), (0.10, 0.10, 0.10), pitch=0.012)
    axis = 0.035
    parts = [box("slide", (-0.013, -0.065, axis - 0.014), (0.013, 0.12, axis + 0.015), steel)]
    for k in range(5):
        y = -0.058 + k * 0.007
        parts.append(box(f"serr{k}", (-0.0134, y, axis - 0.006), (0.0134, y + 0.0028, axis + 0.012), steel))
    parts.append(A.lathe("muzzle", [(0.0, 0.118), (0.0075, 0.118), (0.0075, 0.132), (0.005, 0.133), (0.005, 0.125), (0.0, 0.125)],
                         steel, seg=12, axis=Y, origin=Vector((0, 0, axis - 0.002))))
    parts.append(box("frame", (-0.011, -0.055, 0.0), (0.011, 0.1, axis - 0.014), steel))
    parts.append(bent("guard", [(0, 0.045, 0.0), (0, 0.047, -0.024), (0, 0.012, -0.03), (0, -0.01, -0.016), (0, -0.01, 0.0)], 0.003, steel))
    parts.append(bent("trigger", [(0, 0.012, 0.0), (0, 0.018, -0.012), (0, 0.012, -0.02)], 0.0035, steel))
    parts.append(grip_slab("grip", 0.002, -0.088, 0.0135, 0.034, tape, rake=0.22))
    # The long magazine, raked with the grip.
    mz0, mz1 = -0.08, -0.17
    parts.append(geo.loft("mag", [(Vector((0, -(0.002 - z) * 0.22, z)), X.copy(), Y.copy()) for z in (mz0, mz1)],
                          [[(0.009, 0.015), (-0.009, 0.015), (-0.009, -0.013), (0.009, -0.013)]] * 2,
                          cap_start="fan", cap_end="fan", material=steel, merge=True))
    A.finish("tamancha", parts, res=1024, hard=("slide", "frame", "mag") + tuple(f"serr{k}" for k in range(5)))


def dunali():
    """A double-barrelled shotgun: two barrels side by side with a rib between, a box-lock
    receiver, two triggers in a guard, a wooden fore-end under the barrels and a stock dropping
    back past the hand, with a rubber butt pad."""
    steel = F.metal("dunali_steel", (0.17, 0.17, 0.18), rough=0.35, tarnish=RUST)
    wood = F.plain("dunali_wood", (0.36, 0.19, 0.08), rough=0.55)
    pad = F.plain("dunali_pad", (0.05, 0.05, 0.05), rough=0.85)
    axis = 0.045
    parts = []
    for k, x in enumerate((-0.0125, 0.0125)):
        parts.append(A.lathe(f"barrel{k}", [(0.0, 0.07), (0.0125, 0.07), (0.0118, 0.6), (0.0112, 0.78), (0.0085, 0.781), (0.0085, 0.77),
                                             (0.0, 0.77)], steel, seg=16, axis=Y, origin=Vector((x, 0, axis))))
    parts.append(box("rib", (-0.004, 0.08, axis + 0.006), (0.004, 0.77, axis + 0.013), steel))
    parts.append(A.lathe("bead", [(0.0, 0.0), (0.0025, 0.0005), (0.0, 0.004)], steel, seg=8, origin=Vector((0, 0.765, axis + 0.013))))
    parts.append(box("receiver", (-0.021, -0.02, 0.012), (0.021, 0.085, axis + 0.012), steel))
    parts.append(box("forend", (-0.019, 0.17, axis - 0.032), (0.019, 0.4, axis - 0.008), wood))
    parts.append(bent("guard", [(0, 0.05, 0.012), (0, 0.054, -0.014), (0, 0.02, -0.022), (0, -0.01, -0.01), (0, -0.012, 0.012)], 0.003, steel))
    for k, y in enumerate((0.03, 0.012)):
        parts.append(bent(f"trigger{k}", [(0, y, 0.012), (0, y + 0.006, 0.0), (0, y, -0.008)], 0.0032, steel))
    # The stock: from the receiver back past the hand (the wrist, at the origin), dropping to the butt.
    S = 8
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        y = -0.02 - 0.40 * u
        top = axis + 0.006 - 0.035 * u
        bottom = 0.0 - 0.006 - 0.095 * u ** 1.3
        hw = 0.016 + 0.006 * u
        frames.append((Vector((0, y, (top + bottom) / 2)), X.copy(), Z.copy()))
        rings.append(geo.ellipse(hw, (top - bottom) / 2, 14))
    parts.append(geo.loft("stock", frames, rings, cap_start="fan", cap_end=None, material=wood, merge=True))
    # Rubber pad on the butt (its top and bottom where the stock ends: u = 1 above).
    end_y = frames[-1][0].y
    parts.append(box("pad", (-0.0235, end_y - 0.014, -0.103), (0.0235, end_y + 0.002, axis - 0.027), pad))
    A.finish("dunali", parts, res=1024, hard=("receiver", "rib", "forend", "pad"))


GULEL_TIPS = (0.034, 0.118)


def gulel():
    """A slingshot cut from a forked branch: a bark-on handle, two arms rising to grooved tips. Its
    rubber bands and leather pouch are drawn by the game, from the tips."""
    wood = F.plain("gulel_wood", (0.42, 0.28, 0.15), rough=0.7)
    bark = F.wrap("gulel_bark", (0.28, 0.20, 0.12), (0.22, 0.15, 0.09), pitch=0.009)
    tx, tz = GULEL_TIPS
    parts = [A.tube("handle", [Vector((0, 0, z)) for z in (-0.07, -0.03, 0.01, 0.04)],
                    lambda t: 0.0135 - 0.002 * t, bark, seg=12, caps=("fan", None), up=X)]
    for k, s in enumerate((-1, 1)):
        pts = geo.resample_even(geo.catmull_rom([Vector(p) for p in ((0, 0, 0.03), (s * 0.012, 0, 0.06), (s * tx, 0, 0.09), (s * tx, 0, tz))], 24), 10)
        parts.append(A.tube(f"arm{k}", pts, lambda t: 0.011 - 0.003 * t, wood, seg=10, caps=(None, "fan"), up=Y))
    A.finish("gulel", parts, res=512)
    # Band anchors in the glTF frame (x, y up, z toward the shooter): the tips, mirrored in x.
    tip = (tx, tz, 0.0)
    return {"nock": tip, "bridge": tip, "rest": (0.0, tz, -0.03), "half_len": tz}


BUILDERS = {"bat": bat, "katta": katta, "tamancha": tamancha, "dunali": dunali, "gulel": gulel}


def write_gulel(spec):
    path = os.path.join(A.ROOT, "assets", "data", "bows.ron")
    text = open(path).read()
    r = lambda v: tuple(round(c, 4) for c in v)
    line = (f'    "gulel": (nock: {r(spec["nock"])}, bridge: {r(spec["bridge"])}, rest: {r(spec["rest"])}, '
            f'half_len: {round(spec["half_len"], 4)}, mirror_x: true),\n')
    text = re.sub(r'    "gulel": \(.*?\),\n', "", text)
    text = text.replace("}\n", line + "}\n") if text.rstrip().endswith("}") else text
    open(path, "w").write(text)


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else list(BUILDERS)
    for name in names:
        export.reset_scene()
        spec = BUILDERS[name]()
        if name == "gulel":
            write_gulel(spec)


if __name__ == "__main__":
    main()
