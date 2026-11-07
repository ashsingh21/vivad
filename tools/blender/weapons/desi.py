"""Panga's street armoury: the cricket bat, the katta (a country-made single-shot pistol), the
dunali (a double-barrelled shotgun), the bandook (a workshop-built rifle) and the
gulel (a forked-branch slingshot); and two divine weapons, the Sudarshan Chakra and the Brahmastra. Lofted, given procedural materials, baked to one texture set and
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


def grip_slab(name, top, bottom, half_w, depth, mat, rake=0.28, cx=0.0, cy=0.0):
    """A pistol grip: a rounded slab from `top` down to `bottom` (z), raked back by `rake`, its top's
    centre (`cx`, `cy`) off the origin."""
    S = 6
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        z = top + (bottom - top) * u
        c = Vector((cx, cy - (top - z) * rake, z))
        frames.append((c, X.copy(), Y.copy()))
        rings.append(geo.ellipse(half_w, depth / 2 * (1 - 0.1 * u), 12))
    return geo.loft(name, frames, rings, cap_start="fan", cap_end="fan", material=mat, merge=True)


# The long guns' pistol grips, where the right hand closes (the rifle pack's hold on its weapon
# bone, the origin): top, bottom, rake, half width, half depth, the top's centre across and along.
# Fitted to the male's hand skin over the held clips (characters/grip_fit.py --fit).
GRIPS = {name: (0.004, -0.07, 0.2, 0.016, 0.018, -0.004, 0.008) for name in ("dunali", "bandook", "patakha")}


def pistol_grip(model, mat):
    top, bottom, rake, a, b, cx, cy = GRIPS[model]
    return grip_slab("grip", top, bottom, a, 2 * b, mat, rake=rake, cx=cx, cy=cy)


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
    parts.append(pistol_grip("dunali", wood))
    for k, z in enumerate((-0.02, -0.06)):
        y = -(0.004 - z) * 0.28
        parts.append(rod(f"rivet{k}", (-0.016, y, z), (0.016, y, z), 0.003, steel, seg=8))
    A.finish("katta", parts, res=1024, hard=("breech", "frame"))


# The dunali's measures (m, from the grip along the barrel): the muzzle, the fore-end's middle, the
# butt plate. FOREND and BUTT must match crates/game/src/arms.rs.
MUZZLE, FOREND, BUTT = 0.64, 0.26, 0.21


def dunali():
    """A double-barrelled shotgun: two barrels side by side with a rib between, a box-lock
    receiver, two triggers in a guard, a wooden fore-end under the barrels and a short stock back
    past the hand, with a rubber butt pad. Sized to the rifle the character animations were
    captured with (MoCap Online's Rifle Basic, measured from its weapon bone): the fore-end where
    the left palm closes, 26 cm ahead of the grip; the butt plate 21 cm behind it, in the shoulder's
    pocket when aimed; about as long overall (84 cm)."""
    steel = F.metal("dunali_steel", (0.17, 0.17, 0.18), rough=0.35, tarnish=RUST)
    wood = F.plain("dunali_wood", (0.36, 0.19, 0.08), rough=0.55)
    pad = F.plain("dunali_pad", (0.05, 0.05, 0.05), rough=0.85)
    axis = 0.045
    parts = []
    for k, x in enumerate((-0.0125, 0.0125)):
        parts.append(A.lathe(f"barrel{k}", [(0.0, 0.07), (0.0125, 0.07), (0.0118, 0.52), (0.0112, MUZZLE), (0.0085, MUZZLE + 0.001),
                                             (0.0085, MUZZLE - 0.01), (0.0, MUZZLE - 0.01)], steel, seg=16, axis=Y, origin=Vector((x, 0, axis))))
    parts.append(box("rib", (-0.004, 0.08, axis + 0.006), (0.004, MUZZLE - 0.01, axis + 0.013), steel))
    parts.append(A.lathe("bead", [(0.0, 0.0), (0.0025, 0.0005), (0.0, 0.004)], steel, seg=8, origin=Vector((0, MUZZLE - 0.015, axis + 0.013))))
    parts.append(box("receiver", (-0.021, -0.02, 0.012), (0.021, 0.085, axis + 0.012), steel))
    parts.append(box("forend", (-0.019, FOREND - 0.1, axis - 0.032), (0.019, FOREND + 0.1, axis - 0.008), wood))
    parts.append(bent("guard", [(0, 0.05, 0.012), (0, 0.054, -0.014), (0, 0.02, -0.022), (0, -0.01, -0.01), (0, -0.012, 0.012)], 0.003, steel))
    for k, y in enumerate((0.03, 0.012)):
        parts.append(bent(f"trigger{k}", [(0, y, 0.012), (0, y + 0.006, 0.0), (0, y, -0.008)], 0.0032, steel))
    # The stock: from the receiver back past the hand (the wrist, at the origin), dropping a little
    # to the butt (near the barrels' line, so it beds in the shoulder as the captured rifle's did).
    S = 8
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        y = -0.02 - (BUTT - 0.034) * u
        top = axis + 0.006 - 0.02 * u
        bottom = 0.0 - 0.006 - 0.05 * u ** 1.3
        hw = 0.016 + 0.006 * u
        frames.append((Vector((0, y, (top + bottom) / 2)), X.copy(), Z.copy()))
        rings.append(geo.ellipse(hw, (top - bottom) / 2, 14))
    parts.append(geo.loft("stock", frames, rings, cap_start="fan", cap_end=None, material=wood, merge=True))
    # Rubber pad on the butt (its top and bottom where the stock ends: u = 1 above).
    end_y = frames[-1][0].y
    parts.append(box("pad", (-0.0235, end_y - 0.014, -0.058), (0.0235, end_y + 0.002, axis - 0.012), pad))
    A.finish("dunali", parts, res=1024, hard=("receiver", "rib", "forend", "pad"))



# The bandook's measures (m, from the grip along the barrel, as for the dunali), and its bore's
# height over the grip. Its handguard's underside, where the left palm closes, is AXIS - GUARD_R
# above the grip: crates/game/src/arms.rs forend() must match.
B_MUZZLE, B_AXIS, GUARD_R = 0.63, 0.09, 0.026


def bandook():
    """Panga's rifle, a stylised workshop build rather than any real gun: a chunky receiver in
    truck-paint teal with brass rivets, a round wooden handguard bound with blue tape, a straight
    box magazine with an orange band, a stubby brass muzzle brake, a small red-dot sight with an
    orange lens, and a wooden stock in line with the bore, a rubber butt pad. Sized to the rifle the
    animations were captured with (like the dunali): the handguard where the left palm closes, 26 cm
    ahead of the grip, the butt 21 cm behind it, 84 cm overall."""
    paint = F.plain("bandook_paint", (0.05, 0.30, 0.31), rough=0.42, metal_v=0.25)
    steel = F.metal("bandook_steel", (0.16, 0.16, 0.17), rough=0.38, tarnish=RUST)
    brass = F.metal("bandook_brass", (0.78, 0.56, 0.24), rough=0.3)
    wood = F.plain("bandook_wood", (0.44, 0.23, 0.09), rough=0.5)
    rubber = F.plain("bandook_rubber", (0.05, 0.05, 0.05), rough=0.85)
    tape = F.wrap("bandook_tape", (0.08, 0.22, 0.62), pitch=0.004)
    orange = F.plain("bandook_orange", (0.95, 0.45, 0.06), rough=0.45)
    lens = F.plain("bandook_lens", (1.0, 0.42, 0.08), rough=0.15)
    ax = B_AXIS
    o = Vector((0, 0, ax))
    # The receiver: a rounded block, deep over the grip and the magazine well, sloping up at the
    # front to meet the handguard; an orange coachline down each side.
    def rrect(hw, lo, hi, r, n=3):
        c = (lo + hi) / 2
        hh = (hi - lo) / 2
        pts = []
        for cx, cz, a0 in ((hw - r, hh - r, 0.0), (-hw + r, hh - r, 90.0), (-hw + r, -hh + r, 180.0), (hw - r, -hh + r, 270.0)):
            for k in range(n + 1):
                a = math.radians(a0 + 90.0 * k / n)
                pts.append((cx + r * math.cos(a), c + cz + r * math.sin(a)))
        return pts
    frames, rings = [], []
    for y in (-0.035, -0.03, 0.12, 0.135, 0.15, 0.165):
        lo = 0.004 if y <= 0.12 else 0.004 + (ax - 0.03 - 0.004) * ((y - 0.12) / 0.045) ** 0.8
        hw = 0.021 if y in (-0.035, 0.165) else 0.023
        frames.append((Vector((0, y, 0)), X.copy(), Z.copy()))
        rings.append(rrect(hw, lo, ax + 0.024, 0.007))
    parts = [geo.loft("receiver", frames, rings, cap_start="fan", cap_end="fan", material=paint, merge=True)]
    for sx in (-1, 1):
        parts.append(box(f"line{sx}", (sx * 0.0232 - 0.0006, -0.02, ax - 0.004), (sx * 0.0232 + 0.0006, 0.14, ax + 0.002), orange))
    # A raised top cover, a shade narrower, and the sight's mount on it.
    parts.append(box("cover", (-0.019, -0.03, ax + 0.024), (0.019, 0.15, ax + 0.032), steel))
    parts.append(box("mount", (-0.011, 0.035, ax + 0.032), (0.011, 0.1, ax + 0.042), steel))
    sight_o = Vector((0, 0, ax + 0.062))
    parts.append(A.lathe("sight", [(0.0, 0.03), (0.017, 0.03), (0.019, 0.04), (0.019, 0.098), (0.0165, 0.106), (0.0, 0.106)],
                         steel, seg=20, axis=Y, origin=sight_o))
    parts.append(A.lathe("lens", [(0.0, 0.1065), (0.0145, 0.1065), (0.0145, 0.104), (0.0, 0.104)], lens, seg=20, axis=Y, origin=sight_o))
    for k, (y, z) in enumerate(((0.0, 0.022), (0.105, 0.022), (-0.012, ax + 0.012), (0.125, ax + 0.012))):
        parts.append(rod(f"rivet{k}", (-0.0243, y, z), (0.0243, y, z), 0.0038, brass, seg=10))
    # Handguard: round wood round the barrel, eased at both ends, two bands of tape.
    parts.append(A.lathe("guard", [(0.0, 0.15), (GUARD_R - 0.004, 0.15), (GUARD_R, 0.162), (GUARD_R, 0.39), (GUARD_R - 0.005, 0.405), (0.0, 0.405)],
                         wood, seg=20, axis=Y, origin=o))
    for k, y in enumerate((0.185, 0.355)):
        parts.append(A.lathe(f"tape{k}", [(GUARD_R + 0.0003, y - 0.013), (GUARD_R + 0.0016, y - 0.012), (GUARD_R + 0.0016, y + 0.012),
                                          (GUARD_R + 0.0003, y + 0.013)], tape, seg=20, axis=Y, origin=o))
    parts.append(A.lathe("barrel", [(0.0, 0.4), (0.0105, 0.4), (0.0105, 0.565), (0.0, 0.565)], steel, seg=16, axis=Y, origin=o))
    # A stubby brass brake with three cuts round it.
    brake = [(0.0, 0.562), (0.0155, 0.562)]
    for k in range(3):
        y = 0.572 + k * 0.018
        brake += [(0.0155, y), (0.0125, y + 0.002), (0.0125, y + 0.006), (0.0155, y + 0.008)]
    brake += [(0.0155, B_MUZZLE - 0.004), (0.0135, B_MUZZLE), (0.006, B_MUZZLE), (0.006, B_MUZZLE - 0.012), (0.0, B_MUZZLE - 0.012)]
    parts.append(A.lathe("brake", brake, brass, seg=16, axis=Y, origin=o))
    parts.append(A.lathe("post", [(0.0, 0.0), (0.0035, 0.0), (0.0025, 0.024), (0.0, 0.026)], steel, seg=8,
                         origin=Vector((0, 0.535, ax + 0.008))))
    # A straight box magazine, raked forward a little as it drops, an orange band round its middle.
    frames, rings = [], []
    for s in range(4):
        u = s / 3
        z = 0.006 - 0.118 * u
        frames.append((Vector((0, 0.092 + 0.022 * u, z)), X.copy(), Y.copy()))
        rings.append([(0.0125, -0.024), (0.0125, 0.024), (-0.0125, 0.024), (-0.0125, -0.024)])
    parts.append(geo.loft("mag", frames, rings, cap_start="fan", cap_end="fan", material=steel, merge=True))
    parts.append(box("band", (-0.0138, 0.077, -0.06), (0.0138, 0.127, -0.042), orange))
    parts.append(box("magbase", (-0.0145, 0.084, -0.122), (0.0145, 0.14, -0.111), steel))
    parts.append(bent("tguard", [(0, 0.06, 0.004), (0, 0.062, -0.02), (0, 0.03, -0.03), (0, 0.0, -0.022), (0, -0.004, 0.004)], 0.0032, steel))
    parts.append(bent("trigger", [(0, 0.03, 0.004), (0, 0.036, -0.01), (0, 0.03, -0.019)], 0.0035, steel))
    parts.append(pistol_grip("bandook", rubber))
    # The stock: from the receiver back in line with the bore, deepening to the butt.
    S = 8
    frames, rings = [], []
    for s in range(S):
        u = s / (S - 1)
        y = -0.03 - (BUTT - 0.044) * u
        top = ax + 0.02 - 0.008 * u
        bottom = 0.03 - 0.075 * u ** 1.2
        hw = 0.018 + 0.004 * u
        frames.append((Vector((0, y, (top + bottom) / 2)), X.copy(), Z.copy()))
        rings.append(geo.ellipse(hw, (top - bottom) / 2, 14))
    parts.append(geo.loft("stock", frames, rings, cap_start="fan", cap_end=None, material=wood, merge=True))
    end_y = frames[-1][0].y
    parts.append(box("pad", (-0.0235, end_y - 0.014, -0.047), (0.0235, end_y + 0.002, ax + 0.014), rubber))
    A.finish("bandook", parts, res=1024, hard=("cover", "mount", "mag", "band", "magbase", "pad"))


# ------------------------------------------------------------------ jugaad gadgets

def chappal():
    """Mummy's rubber chappal, thrown spinning (like the chakra): a blue-and-white flip-flop sole,
    flat in the XY plane (its toe +Y), the hand at its middle; a blue V strap from the toe post."""
    sole_top = F.plain("chappal_top", (0.86, 0.86, 0.82), rough=0.7)
    sole = F.plain("chappal_sole", (0.08, 0.28, 0.62), rough=0.8)
    strap = F.plain("chappal_strap", (0.06, 0.25, 0.70), rough=0.55)
    # The sole's outline: a footprint, wider at the ball than the heel, from heel (-y) to toe.
    def half(u):
        return 0.036 + 0.014 * math.sin(math.pi * min(1.0, u * 1.25)) + 0.008 * u
    S = 14
    out = []
    for s in range(S):
        u = s / (S - 1)
        out.append((half(u) * math.sqrt(max(0.0, 1 - ((u - 0.5) * 2) ** 8)) + 0.002, -0.125 + 0.25 * u))
    ring = [(x, y) for x, y in out] + [(-x, y) for x, y in reversed(out)]
    for name, z0, z1, m in (("sole", -0.009, 0.0, sole), ("top", 0.0, 0.006, sole_top)):
        frames = [(Vector((0, 0, z)), X.copy(), Y.copy()) for z in (z0, z1)]
        parts_ring = [ring, ring]
        o = geo.loft(name, frames, parts_ring, cap_start="fan", cap_end="fan", material=m, merge=True)
        if name == "sole":
            parts = [o]
        else:
            parts.append(o)
    post = Vector((0, 0.07, 0.006))
    for sx in (-1, 1):
        parts.append(bent(f"strap{sx}", [post + Vector((0, 0, 0.012)), Vector((sx * 0.022, 0.02, 0.03)), Vector((sx * 0.045, -0.035, 0.006))],
                          0.0055, strap, seg=8))
    parts.append(A.lathe("post", [(0.0, 0.0), (0.006, 0.0), (0.006, 0.014), (0.0, 0.016)], strap, seg=10, origin=post))
    A.finish("chappal", parts, res=512)


def cooker():
    """A pressure cooker: an aluminium pot with a black bakelite handle (the hand's), the lid's
    long handle laid over it, and the whistle weight on its steam vent; it whistles before it
    blows. The handle runs +X from the pot; the origin is on the handle where the hand closes."""
    alu = F.metal("cooker_alu", (0.72, 0.72, 0.70), rough=0.32, tarnish=(0.35, 0.33, 0.30))
    bake = F.plain("cooker_bakelite", (0.05, 0.04, 0.035), rough=0.45)
    red = F.plain("cooker_gasket", (0.6, 0.08, 0.05), rough=0.6)
    r, h = 0.085, 0.13
    c = Vector((-0.17, 0, -0.06))
    parts = [A.lathe("pot", [(0.0, 0.0), (r - 0.012, 0.0), (r, 0.012), (r, h - 0.01), (r + 0.004, h), (r - 0.004, h + 0.003), (0.0, h + 0.003)],
                     alu, seg=28, origin=c)]
    parts.append(A.lathe("lid", [(0.0, h + 0.003), (r - 0.002, h + 0.003), (r - 0.012, h + 0.02), (0.02, h + 0.03), (0.0, h + 0.031)],
                         alu, seg=28, origin=c))
    parts.append(A.lathe("gasket", [(r - 0.001, h - 0.006), (r + 0.0025, h - 0.005), (r + 0.0025, h + 0.0), (r - 0.001, h + 0.001)], red,
                         seg=28, origin=c))
    # The whistle: a stubby weight on its vent.
    vent = c + Vector((0, 0, h + 0.03))
    parts.append(A.lathe("whistle", [(0.0, 0.0), (0.006, 0.0), (0.006, 0.012), (0.016, 0.016), (0.017, 0.03), (0.012, 0.04), (0.004, 0.046),
                                      (0.0, 0.047)], bake, seg=16, origin=vent))
    # The pot's handle (held) and the lid's (laid over it), black bakelite.
    for name, z, ln in (("handle", 0.0, 0.17), ("lid_handle", 0.022, 0.15)):
        frames, rings = [], []
        for k in range(5):
            u = k / 4
            x = c.x + r - 0.005 + (ln + 0.01) * u
            frames.append((Vector((x, 0, z + c.z + h - 0.03 + 0.012 * u)), Y.copy(), Z.copy()))
            rings.append(geo.ellipse(0.016 - 0.003 * u, 0.012, 12))
        parts.append(geo.loft(name, frames, rings, cap_start="fan", cap_end="fan", material=bake, merge=True))
    A.finish("cooker", parts, res=512)


def mirchi():
    """The mirchi bomb: five fat red chillies bound into a bundle with green thread, their stalks
    up, a twist of fuse from the middle. Stands along +Z, the hand round the binding."""
    chilli = F.plain("mirchi_red", (0.75, 0.05, 0.03), rough=0.3)
    stalk = F.plain("mirchi_stalk", (0.20, 0.42, 0.10), rough=0.6)
    thread = F.wrap("mirchi_thread", (0.10, 0.55, 0.15), pitch=0.003)
    fuse = F.wrap("mirchi_fuse", (0.85, 0.75, 0.55), (0.55, 0.15, 0.08), pitch=0.004)
    parts = []
    for k in range(5):
        a = 2 * math.pi * k / 5 + 0.3
        d = Vector((math.cos(a), math.sin(a), 0))
        # A chilli: thick at the shoulder, curving out and down to a point.
        pts = [Vector((0, 0, 0.07)) + d * 0.016, Vector((0, 0, 0.0)) + d * 0.022, Vector((0, 0, -0.07)) + d * 0.03,
               Vector((0, 0, -0.11)) + d * 0.045]
        pts = geo.resample_even(geo.catmull_rom(pts, 20), 12)
        parts.append(A.tube(f"chilli{k}", pts, lambda t: 0.016 * (1 - t) ** 0.6 * min(1.0, 0.6 + t * 4) + 0.001, chilli, seg=12,
                            caps=("fan", "fan"), up=X))
        top = Vector((0, 0, 0.07)) + d * 0.016
        parts.append(bent(f"stalk{k}", [top, top + Vector((0, 0, 0.015)) + d * 0.004, top + Vector((0, 0, 0.025)) + d * 0.012], 0.003,
                          stalk, seg=6))
    parts.append(A.lathe("binding", [(0.033, -0.012), (0.036, -0.01), (0.036, 0.012), (0.033, 0.014)], thread, seg=20))
    parts.append(bent("fuse", [(0, 0, 0.06), (0.004, 0.0, 0.1), (-0.006, 0.004, 0.125), (0.004, -0.002, 0.14)], 0.0025, fuse, seg=6))
    A.finish("mirchi", parts, res=512)


# The launcher's tube: its bore's height over the grip and radius (its underside, where the left
# palm closes, is P_AXIS - P_R above the grip: crates/game/src/arms.rs forend() must match).
P_AXIS, P_R = 0.1, 0.048


def patakha():
    """The patakha launcher: a length of grey PVC pipe, bound with silver tape, on a wooden pistol
    grip, with a fore grip under the tube where the left hand holds it, a foam pad at the back
    and a bent-wire sight; a Diwali rocket's red nose peeks out of the front. Held like the
    rifle (barrel +Y, up +Z, the grip at the origin)."""
    pvc = F.plain("patakha_pvc", (0.55, 0.57, 0.58), rough=0.5)
    tape = F.wrap("patakha_tape", (0.62, 0.62, 0.64), pitch=0.005)
    wood = F.plain("patakha_wood", (0.46, 0.26, 0.10), rough=0.55)
    foam = F.plain("patakha_foam", (0.10, 0.10, 0.11), rough=0.9)
    red = F.plain("patakha_red", (0.85, 0.08, 0.05), rough=0.45)
    yellow = F.plain("patakha_yellow", (0.98, 0.78, 0.10), rough=0.5)
    wire = F.metal("patakha_wire", (0.4, 0.4, 0.42), rough=0.4)
    o = Vector((0, 0, P_AXIS))
    y0, y1 = -0.34, 0.5
    parts = [A.lathe("tube", [(P_R - 0.006, y1 - 0.01), (P_R - 0.006, y0), (P_R, y0), (P_R, y1), (P_R - 0.006, y1)], pvc, seg=24, axis=Y, origin=o)]
    for k, y in enumerate((-0.18, 0.05, 0.3, 0.46)):
        parts.append(A.lathe(f"tape{k}", [(P_R + 0.0003, y - 0.02), (P_R + 0.002, y - 0.018), (P_R + 0.002, y + 0.018), (P_R + 0.0003, y + 0.02)],
                             tape, seg=24, axis=Y, origin=o))
    # Yellow and red paint bands at the mouth, like the rocket's own wrapper.
    for k, (y, m) in enumerate(((0.38, yellow), (0.405, red))):
        parts.append(A.lathe(f"stripe{k}", [(P_R + 0.0002, y - 0.011), (P_R + 0.0012, y - 0.01), (P_R + 0.0012, y + 0.01), (P_R + 0.0002, y + 0.011)],
                             m, seg=24, axis=Y, origin=o))
    parts.append(A.lathe("nose", [(0.0, y1 + 0.075), (0.012, y1 + 0.06), (0.03, y1 + 0.02), (0.034, y1 - 0.03), (0.0, y1 - 0.03)], red, seg=20,
                         axis=Y, origin=o))
    parts.append(A.lathe("pad", [(0.0, y0 - 0.03), (P_R + 0.004, y0 - 0.03), (P_R + 0.006, y0 - 0.015), (P_R + 0.006, y0 + 0.06),
                                 (P_R + 0.002, y0 + 0.065)], foam, seg=24, axis=Y, origin=o))
    parts.append(box("mount", (-0.012, -0.05, 0.004), (0.012, 0.08, P_AXIS - P_R + 0.006), wood))
    parts.append(pistol_grip("patakha", wood))
    parts.append(grip_slab("foregrip", P_AXIS - P_R + 0.002, P_AXIS - P_R - 0.09, 0.015, 0.032, wood, rake=-0.05))
    parts.append(bent("tguard", [(0, 0.05, 0.004), (0, 0.052, -0.02), (0, 0.025, -0.03), (0, 0.0, -0.022), (0, -0.004, 0.004)], 0.003, wire))
    parts.append(bent("trigger", [(0, 0.028, 0.004), (0, 0.034, -0.01), (0, 0.028, -0.019)], 0.0035, wire))
    sight = Vector((0, 0.32, P_AXIS + P_R))
    parts.append(rod("sight_post", sight, sight + Vector((0, 0, 0.03)), 0.0025, wire, seg=6))
    ring = [sight + Vector((0.016 * math.cos(a), 0, 0.046 + 0.016 * math.sin(a))) for a in [2 * math.pi * k / 16 for k in range(17)]]
    parts.append(A.tube("sight_ring", ring, lambda t: 0.002, wire, seg=6, up=Y))
    A.finish("patakha", parts, res=1024, hard=("mount",))


def rocket():
    """The patakha launcher's rocket in flight: a Diwali sky rocket, a red paper tube with a
    yellow spiral, a red cone nose (+Z, first) and a bamboo stick trailing behind."""
    red = F.plain("rocket_red", (0.85, 0.08, 0.05), rough=0.45)
    yellow = F.plain("rocket_yellow", (0.98, 0.78, 0.10), rough=0.5)
    bamboo = F.plain("rocket_bamboo", (0.72, 0.58, 0.32), rough=0.7)
    parts = [A.lathe("body", [(0.0, -0.1), (0.03, -0.1), (0.03, 0.1), (0.0, 0.1)], red, seg=16)]
    for k in range(4):
        z = -0.08 + 0.05 * k
        parts.append(A.lathe(f"band{k}", [(0.0302, z), (0.031, z + 0.002), (0.031, z + 0.014), (0.0302, z + 0.016)], yellow, seg=16))
    parts.append(A.lathe("nose", [(0.0, 0.17), (0.008, 0.16), (0.026, 0.125), (0.031, 0.1), (0.0, 0.1)], red, seg=16))
    parts.append(rod("stick", (0.03, 0.0, -0.06), (0.03, 0.0, -0.5), 0.005, bamboo, seg=6))
    A.finish("rocket", parts, res=512)

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


# ------------------------------------------------------------------ the two divine weapons

CHAKRA_R = 0.13


def chakra():
    """The Sudarshan Chakra: a gold disc with a ring of saw teeth round its rim, a blue enamelled
    face between raised rings, and a hub. Held by its rim: the origin is the bottom of the disc,
    which stands up (+Z) with its faces to the sides (±X)."""
    gold = F.metal("chakra_gold", A.GOLD, rough=0.2, chase=True)
    blue = F.lacquer("chakra_enamel", base=(0.05, 0.14, 0.42))
    R, teeth, th = CHAKRA_R, 24, 0.012
    c = Vector((0, 0, R))
    # The toothed rim: a saw-tooth outline (each tooth leaning the way it spins), extruded across X.
    outline = []
    for k in range(teeth):
        a0 = 2 * math.pi * k / teeth
        for frac, r in ((0.0, R - 0.012), (0.55, R - 0.008), (0.98, R + 0.024)):
            a = a0 + frac * 2 * math.pi / teeth
            outline.append((math.cos(a) * r, math.sin(a) * r))
    frames = [(c + X * x, Y.copy(), Z.copy()) for x in (-th / 2, th / 2)]
    parts = [geo.loft("rim", frames, [outline, outline], cap_start="fan", cap_end="fan", material=gold, merge=True)]
    # Faces: a raised ring at the rim, the enamel field, an inner ring and the hub, both sides.
    prof = [(0.0, th * 0.9), (0.03, th * 0.9), (0.032, th * 0.6), (R - 0.03, th * 0.6), (R - 0.028, th * 0.8),
            (R - 0.016, th * 0.8), (R - 0.014, 0.0)]
    for k, sgn in enumerate((-1, 1)):
        axis = X * sgn
        parts.append(A.lathe(f"field{k}", [(r, h) for r, h in prof], blue, seg=48, axis=axis, origin=c))
        parts.append(A.lathe(f"hub{k}", [(0.0, th * 1.6), (0.018, th * 1.5), (0.024, th * 1.0), (0.034, th * 0.9), (0.034, 0.0)],
                             gold, seg=32, axis=axis, origin=c))
        parts.append(A.lathe(f"ring{k}", [(R - 0.032, th * 0.6), (R - 0.03, th * 1.0), (R - 0.018, th * 1.0), (R - 0.016, th * 0.8)],
                             gold, seg=48, axis=axis, origin=c))
    A.finish("chakra", parts, res=1024, hard=("rim",))


def brahmastra():
    """The Brahmastra: a white-and-gold javelin with a long four-faced head of pale crystal held
    in a gold crown, rings of gold down the shaft."""
    lac = F.lacquer("brahmastra_lac", base=(0.85, 0.82, 0.74), paint=(0.86, 0.62, 0.26))
    gold = F.metal("brahmastra_gold", A.GOLD, rough=0.22, chase=True)
    crystal = F.plain("brahmastra_crystal", (0.75, 0.88, 1.0), rough=0.08)
    parts = [A.lathe("shaft", [(0.0, -0.24), (0.009, -0.235), (0.0095, 0.0), (0.0085, 0.25), (0.0, 0.255)], lac, seg=12,
                     extra={"kv": lambda *a: 1.0})]
    for z0 in (-0.24, -0.12, -0.02, 0.08, 0.2):
        parts.append(A.lathe(f"ring{z0}", [(0.0098, z0), (0.0122, z0 + 0.004), (0.0122, z0 + 0.02), (0.0098, z0 + 0.024)], gold, seg=12))
    parts.append(A.lathe("crown", [(0.009, 0.24), (0.02, 0.255), (0.024, 0.27), (0.016, 0.29), (0.012, 0.3)], gold, seg=16))
    parts.append(A.lathe("head", [(0.0, 0.28), (0.022, 0.31), (0.018, 0.4), (0.0, 0.5)], crystal, seg=4))
    parts.append(A.lathe("pommel", [(0.0, -0.28), (0.012, -0.275), (0.016, -0.26), (0.0095, -0.242)], gold, seg=12))
    A.finish("brahmastra", parts, res=1024, hard=("pommel",))


# (The tamancha is the pistol from MoCap Online's rifle pack: pack_guns.py.)
BUILDERS = {"bat": bat, "katta": katta, "dunali": dunali, "bandook": bandook, "gulel": gulel, "chakra": chakra,
            "brahmastra": brahmastra, "chappal": chappal, "cooker": cooker, "mirchi": mirchi, "patakha": patakha, "rocket": rocket}


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
