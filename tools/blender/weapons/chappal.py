"""The hawai chappal: a blue rubber sole, a white footbed and a blue strap between the toes.

Axes as the bat: origin where the hand closes (the heel), the long axis Blender +Z (glTF +Y), the
sole's top toward Blender +Y.

blender -b --factory-startup -P tools/blender/weapons/chappal.py
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "lib"))
sys.path.insert(0, HERE)

import bpy  # noqa: E402

import arsenal as A  # noqa: E402
import desi as D  # noqa: E402
import export  # noqa: E402
import finishes as F  # noqa: E402

BLUE = (0.035, 0.12, 0.42)
WHITE = (0.72, 0.72, 0.68)
LONG = 0.27
# A foot's outline, heel to toe: (across, along). The left edge is the right one, a little fuller at
# the big toe.
RIGHT = [(0.0, 0.0), (0.024, 0.004), (0.037, 0.03), (0.039, 0.075), (0.037, 0.115), (0.043, 0.165), (0.05, 0.205), (0.047, 0.24), (0.03, 0.262), (0.0, LONG)]


def outline():
    left = [(-a * (1.0 + 0.06 * (b / LONG) ** 2), b) for a, b in reversed(RIGHT[1:-1])]
    return RIGHT + left


def slab(name, low, high, mat, shrink=1.0):
    """The sole's outline as a slab from height `low` to `high`."""
    pts = [(a * shrink, b) for a, b in outline()]
    n = len(pts)
    verts = [(a, low, b) for a, b in pts] + [(a, high, b) for a, b in pts]
    faces = [list(range(n)), list(range(2 * n - 1, n - 1, -1))] + [[k, n + k, n + (k + 1) % n, (k + 1) % n] for k in range(n)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    mesh.materials.append(mat)
    return obj


def main():
    export.reset_scene()
    blue = F.plain("chappal_blue", BLUE, rough=0.75)
    white = F.plain("chappal_white", WHITE, rough=0.85)
    parts = [slab("sole", 0.0, 0.009, blue), slab("bed", 0.009, 0.015, white, shrink=0.97)]
    # The strap: from the toe post out and back to the sole's two sides.
    post = (0.006, 0.015, 0.205)
    for side in (-1.0, 1.0):
        parts.append(D.bent(f"strap_{side}", [post, (side * 0.024, 0.034, 0.165), (side * 0.034, 0.04, 0.135), (side * 0.04, 0.03, 0.11), (side * 0.04, 0.012, 0.09)], 0.0055, blue))
    A.finish("chappal", parts, res=256)


if __name__ == "__main__":
    main()
