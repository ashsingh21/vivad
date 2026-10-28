"""Rani ki Baoli: the stepwell's carved lining. Tiers of niches with Vishnu figures between
pilasters and cornices on the two long walls and the west wall, carved pillars for the two
pavilions, and moulded fascias on the pavilion roofs (the bridges across the well).

Matches `india.rs::stepwell()` (frame at STEPWELL_C, yaw 0): pit 46 x 18, 9 m deep, three
levels 3 m apart, flights descending west from x = +23. Blender X = game X, Blender Y = -game Z.

blender -b --factory-startup -P tools/blender/architecture/stepwell.py
"""

import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import buddhist as B  # noqa: E402
import export  # noqa: E402
import temple as T  # noqa: E402
import texgen  # noqa: E402
from vernacular import cylinder_between  # noqa: E402

OUT = os.path.join(T.ROOT, "assets", "models", "props")
HL, HW, DEPTH, LVL = 23.0, 9.0, 9.0, 3.0
FLIGHT, LANDING = 9.0, 4.0


def vishnu(kit, mat_world, s=1.0):
    """Standing four-armed Vishnu (samabhanga) with a tall crown, conch and discus raised in the
    upper hands. Built at the origin facing -Y, ~1.75*s tall."""
    parts = [T.lathe("vpad", [(0.26 * s, 0.0), (0.3 * s, 0.07 * s), (0.22 * s, 0.14 * s)], 16, (0, 0), uv_strip="padma")]
    for sx in (-1, 1):
        parts.append(cylinder_between("leg", (sx * 0.08 * s, 0, 0.14 * s), (sx * 0.07 * s, 0, 0.82 * s), 0.065 * s, "stone", 8))
    hip = T.lathe("dhoti", [(r * s, z * s) for r, z in ((0.12, 0.55), (0.19, 0.78), (0.17, 0.98))], 12, (0, 0))
    torso = T.lathe("torso", [(r * s, z * s) for r, z in ((0.15, 0.98), (0.14, 1.12), (0.19, 1.3), (0.2, 1.36), (0.08, 1.42))], 12, (0, 0))
    for o in (hip, torso):
        for v in o.data.vertices:
            v.co.y *= 0.7
        parts.append(o)
    for sx in (-1, 1):
        sh = Vector((sx * 0.19, 0, 1.33)) * s
        # Upper arms raised, holding the conch (left) and the discus (right).
        el, hd = Vector((sx * 0.34, 0.02, 1.2)) * s, Vector((sx * 0.37, -0.02, 1.48)) * s
        parts.append(cylinder_between("ua", sh, el, 0.045 * s, "stone", 6))
        parts.append(cylinder_between("fa", el, hd, 0.04 * s, "stone", 6))
        if sx > 0:
            parts.append(cylinder_between("chakra", hd + Vector((0, -0.02, 0.1)) * s, hd + Vector((0, 0.02, 0.1)) * s, 0.09 * s, "trim", 16))
        else:
            parts.append(B.uv_sphere("shankha", hd + Vector((0, 0, 0.08)) * s, (0.05 * s, 0.05 * s, 0.09 * s), "stone", 8, 6))
        # Lower arms: one resting on the mace, one in varada (giving).
        el2, hd2 = Vector((sx * 0.26, -0.02, 1.08)) * s, Vector((sx * 0.3, -0.12, 0.88)) * s
        parts.append(cylinder_between("ua2", sh, el2, 0.05 * s, "stone", 6))
        parts.append(cylinder_between("fa2", el2, hd2, 0.045 * s, "stone", 6))
    parts.append(cylinder_between("gada", (-0.34 * s, -0.14 * s, 0.14 * s), (-0.31 * s, -0.12 * s, 0.86 * s), 0.035 * s, "stone", 6))
    parts.append(B.uv_sphere("gadahead", (-0.31 * s, -0.12 * s, 0.9 * s), (0.07 * s, 0.07 * s, 0.07 * s), "stone", 8, 6))
    parts.append(B.uv_sphere("head", (0, 0.0, 1.5 * s), (0.085 * s, 0.09 * s, 0.1 * s), "stone"))
    parts.append(T.lathe("kirita", [(r * s, z * s) for r, z in ((0.085, 1.56), (0.09, 1.62), (0.07, 1.72), (0.03, 1.78), (0.0, 1.8))],
                         10, (0, 0), uv_strip="bead"))
    parts.append(T.lathe("hara", [(0.16 * s, 1.27 * s), (0.17 * s, 1.3 * s), (0.15 * s, 1.33 * s)], 12, (0, 0), uv_strip="bead"))
    for o in parts:
        kit.add(B.transform(o, mat_world))


def lined_wall(kit, M, length, x_skip=()):
    """Tiers on one wall in piece space (X along the wall centred on 0, front -Y, plane Y=0)."""
    bay = 2.3
    n = int(length / bay)
    x0 = -n * bay / 2
    for t in range(3):
        z_hi = -t * LVL
        z_lo = z_hi - LVL
        # Cornice ledge at the top of the tier and a carved band under it.
        B.frieze(kit, M, x0, -x0, z_hi - 0.35, z_hi - 0.05, 0.42, "dentil")
        B.frieze(kit, M, x0, -x0, z_hi - 0.75, z_hi - 0.35, 0.18, "lata" if t % 2 == 0 else "gavaksha")
        for k in range(n + 1):
            x = x0 + k * bay
            kit.add(B.transform(T.box("pil", (x - 0.16, -0.22, z_lo), (x + 0.16, 0.0, z_hi - 0.75), "trim"), M))
        for k in range(n):
            xc = x0 + (k + 0.5) * bay
            if any(a < xc < b for a, b in x_skip):
                continue
            if (k + t) % 3 == 0:
                B.niche(kit, M, xc, z_lo + 0.35, 0.95, 1.75, 0.92, figure=vishnu)
            else:
                # Carved jali panel: a lattice of small horseshoe arches.
                B.frieze(kit, M, xc - 0.85, xc + 0.85, z_lo + 0.6, z_lo + 1.9, 0.08, "gavaksha")
                B.frieze(kit, M, xc - 0.85, xc + 0.85, z_lo + 0.3, z_lo + 0.6, 0.12, "padma")
    # Coping at ground level.
    B.frieze(kit, M, x0 - 0.3, -x0 + 0.3, -0.05, 0.25, 0.6, "padma")


def stepwell():
    kit = T.Kit()
    # Long walls: the north one (game z = -9, Blender Y = +9) faces -Y already.
    lined_wall(kit, Matrix.Translation((0.0, HW, 0.0)), 2 * HL)
    lined_wall(kit, Matrix.Translation((0.0, -HW, 0.0)) @ Matrix.Rotation(math.pi, 4, "Z"), 2 * HL)
    lined_wall(kit, Matrix.Translation((-HL, 0.0, 0.0)) @ Matrix.Rotation(math.pi / 2, 4, "Z"), 2 * HW, x_skip=((-1.5, 1.5),))
    # Pavilions over the first two landings: carved pillars and a moulded roof-bridge fascia.
    x = HL
    for i in range(2):
        bottom = -(i + 1) * LVL
        x -= FLIGHT
        for px in (x - LANDING + 0.6, x - 0.6):
            for pz in (-HW + 0.8, -3.0, 3.0, HW - 0.8):
                T.pillar(kit, px, -pz, bottom, -bottom - 0.5)
        cx = x - LANDING / 2
        kit.add(T.molded("bridge", [(-0.55, T.rect(LANDING / 2 + 0.08, HW, cx, 0.0), "dentil"),
                                    (-0.3, T.rect(LANDING / 2 + 0.08, HW, cx, 0.0), "lata"),
                                    (0.0, T.rect(LANDING / 2 + 0.08, HW, cx, 0.0), None)]))
        # Kirtimukha brackets under the bridge edges.
        for px in (x - LANDING + 0.6, x - 0.6):
            for pz in (-6.0, 0.0, 6.0):
                kit.add(T.box("brk", (px - 0.15, -pz - 0.5, -0.9), (px + 0.15, -pz + 0.5, -0.55), "trim"))
        x -= LANDING
    return B.finish("stepwell", kit, lods=(0.3,))


def main():
    export.reset_scene()
    texgen.sandstone(T.TEX)
    texgen.trim_sheet(T.TEX)
    T.MATS.update({
        "stone": T.tex_material("sandstone", "sandstone"),
        "trim": T.tex_material("trim", "trim"),
        "shadow": T.flat_material("shadow", (0.05, 0.035, 0.025), 0.0, 0.95),
        "gold": T.flat_material("gold", (1.0, 0.72, 0.30), 1.0, 0.3),
    })
    for o in list(bpy.data.objects):
        bpy.data.objects.remove(o)
    lods = stepwell()
    print(f"[stepwell] {export.export_glb(lods, os.path.join(OUT, 'stepwell.glb'))}")


if __name__ == "__main__":
    main()
