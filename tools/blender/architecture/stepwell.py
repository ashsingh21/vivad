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
    """Standing four-armed Vishnu (samabhanga) on a lotus pedestal: the sculpted figure
    (figures.py) with a tall crown, the discus and conch in the upper hands and the mace under
    the lower left. ~1.75*s tall, at the origin facing -Y, moved by `mat_world`."""
    base, h = 0.14 * s, 1.5 * s
    parts = [T.lathe("vpad", [(0.26 * s, 0.0), (0.3 * s, 0.07 * s), (0.22 * s, base)], 16, (0, 0), uv_strip="padma")]
    meta = B.figure_meta("vishnu")
    at = {k: Vector((v.x * h, v.y * h, base + v.z * h)) for k, v in meta.items()}
    lod = "hi" if s > 1.2 else "lo"
    kit.add(B.figure("vishnu", lod, mat_world @ Matrix.Translation((0, 0, base)) @ Matrix.Scale(h, 4)))
    # Kirita mukuta: the tall cylindrical crown, set down over the crown of the head.
    top = at["head_top"]
    parts.append(T.lathe("kirita", [(r * s, top.z + z * s) for r, z in ((0.07, -0.05), (0.075, 0.0), (0.068, 0.08), (0.05, 0.16), (0.02, 0.21), (0.0, 0.23))],
                         12, (top.x, top.y), uv_strip="bead"))
    d, c = at["discus"], at["conch"]
    parts.append(cylinder_between("chakra", d + Vector((0, -0.02, 0.07)) * s, d + Vector((0, 0.02, 0.07)) * s, 0.09 * s, "trim", 16))
    parts.append(B.uv_sphere("shankha", c + Vector((0, 0, 0.06)) * s, (0.045 * s, 0.045 * s, 0.08 * s), "stone", 8, 6))
    m = at["mace"]
    parts.append(cylinder_between("gada", (m.x, m.y, base), (m.x, m.y, m.z - 0.04 * s), 0.03 * s, "stone", 6))
    parts.append(B.uv_sphere("gadahead", (m.x, m.y, m.z - 0.02 * s), (0.065 * s, 0.065 * s, 0.07 * s), "stone", 8, 6))
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
    # Coping at ground level, tall enough to cap the top tier's niche heads (to z = +0.30) and
    # set 2 cm behind the wall plane so their backs, flush with that plane, sit inside it.
    xm, half = 0.0, -x0 + 0.3
    kit.add(B.transform(T.molded("coping", [(-0.05, T.rect(half, 0.31, xm, -0.29), "padma"),
                                            (0.39, T.rect(half, 0.31, xm, -0.29), None)]), M))


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
    print(f"[stepwell] {export.export_glb(lods, os.path.join(OUT, 'stepwell.gltf'))}")


if __name__ == "__main__":
    main()
