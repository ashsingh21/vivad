"""The land's two surfaces, from Poly Haven's 2 m scans (CC0): withered_grass for the dry open land
and sparse_grass for the green turf. The JPGs stay in build/look_src, out of git. Writes PNGs to
build/ultra_src, which `just textures-pack` turns into assets/textures/ultra/{dry,turf}_*.ktx2.

python3 tools/textures/ncr_ground.py [dry turf]

The scans are withered_grass and sparse_grass, 2k JPGs of diff, nor_gl and arm, from polyhaven.com.

Conventions, as tools/textures/cp_surfaces.py has them, and what's new:
- Colour is the ratio to its own mean, at 0.5 in linear light: the ground shader multiplies its own
  mean for the surface by twice this. Hue stays in the ratio, so a green blade stays green on any
  mean. Broad light and dark patches are taken out: a 2 m scan's patches, repeated across a park,
  would show as a grid from a rooftop, and the ground's tint carries the broad changes anyway.
- Normals are +Y up (OpenGL), the convention of the Poly Haven scans and of Bevy.
- ORM: red occlusion, green roughness. Blue, which would be metalness, holds a tileable noise of
  blobs about 70 cm across: the ground shader breaks the line between dry and green with it.
"""

import os
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SOURCE = os.path.join(ROOT, "build", "look_src")
OUT = os.path.join(ROOT, "build", "ultra_src")
sys.path.insert(0, os.path.dirname(__file__))
from cp_surfaces import as_ratio, luminance, periodic_noise, save, to_linear, to_srgb  # noqa: E402

N = 2048
# The broad patches go, down to about this many pixels.
FLATTEN = 150.0


def read(name):
    im = Image.open(os.path.join(SOURCE, name)).convert("RGB")
    return np.asarray(im, dtype=np.float32) / 255.0


def flatten(linear, keep=0.25):
    """Takes out the scan's broad patches, down to `keep` of their contrast."""
    broad = ndimage.gaussian_filter(linear, (FLATTEN, FLATTEN, 0), mode="wrap")
    mean = linear.reshape(-1, 3).mean(0)
    return linear / broad * (mean * (1.0 - keep) + broad * keep)


def surface(name, scan, grey, seed, push=None):
    diff = to_linear(read(f"{scan}_diff_2k.jpg"))
    print(f"[ground] {name}: scan's mean colour, sRGB {to_srgb(diff.reshape(-1, 3).mean(0)).round(3)}")
    if push is not None:
        diff = push(diff)
    diff = flatten(diff)
    save(f"{name}_basecolor", to_srgb(as_ratio(diff, grey=grey)))
    save(f"{name}_normal", read(f"{scan}_nor_gl_2k.jpg"))
    arm = read(f"{scan}_arm_2k.jpg")
    noise = periodic_noise(N, N, 6, seed)
    save(f"{name}_orm", np.stack([arm[..., 0], np.clip(arm[..., 1] + 0.05, 0.0, 1.0), noise], -1))


def dry():
    surface("dry", "withered_grass", 0.0, 301)


def turf():
    """The scan's bare red earth would read as rust under a lawn's green: its red is pulled most of
    the way to the green blades' brightness, so what's left is darker green between blades."""

    def less_red(d):
        lum = luminance(d)[..., None]
        redness = np.clip((d[..., 0:1] - d[..., 1:2]) / np.maximum(lum, 1e-3), 0.0, 1.0)
        out = d.copy()
        out[..., 0] = d[..., 0] * (1.0 - 0.55 * redness[..., 0])
        out[..., 2] = d[..., 2] * (1.0 + 0.1 * redness[..., 0])
        return out

    surface("turf", "sparse_grass", 0.0, 302, less_red)


def main():
    names = sys.argv[1:] or ["dry", "turf"]
    for n in names:
        globals()[n]()


if __name__ == "__main__":
    main()
