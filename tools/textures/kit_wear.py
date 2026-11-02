"""The street kit's wear: scuffs, grime and chips, multiplied over its flat vertex colours by
crate::street so a drum or a scooter stops reading as a toy.

Derived from two CC0 Poly Haven scans already in the repo (assets/models/samples/delhi_street/textures/
vendor): painted_plaster_wall for flaking paint, concrete_floor for grime. Only their fine detail is
kept, so no big blotch repeats across a piece, and they stay tiling. Greyscale, a touch warm in the
darks, as dirt is. Needs numpy, scipy and Pillow:

    python3 tools/textures/kit_wear.py
"""

import os

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
VENDOR = os.path.join(ROOT, "assets", "models", "samples", "delhi_street", "textures", "vendor")
OUT = os.path.join(ROOT, "assets", "models", "textures", "kit_wear.png")
SIZE = 512
# What the map averages in linear light: crate::street's WEAR_MEAN.
MEAN = 0.80


def detail(name):
    """A scan's luminance with everything larger than a tenth of the tile taken out, unit deviation."""
    img = Image.open(os.path.join(VENDOR, name)).convert("L").resize((SIZE, SIZE), Image.LANCZOS)
    y = np.asarray(img, np.float32) / 255.0
    fine = y - ndimage.gaussian_filter(y, SIZE * 0.1, mode="wrap")
    return fine / fine.std()


def main():
    wear = 0.6 * detail("painted_plaster_wall_diff_1k.png") + 0.8 * detail("concrete_floor_diff_1k.png")
    wear /= wear.std()
    # Mostly a soft mottle, and dark marks in its low tail: those are the chips and stains.
    value = MEAN + 0.09 * np.clip(wear, -3.0, 1.5) - 0.10 * np.clip(-wear - 1.0, 0.0, 2.0) ** 2
    value = np.clip(value, 0.25, 1.0)
    value *= MEAN / value.mean()
    warm = np.clip((MEAN - value) * 2.0, 0.0, 1.0)
    rgb = np.dstack([value, value * (1.0 - 0.05 * warm), value * (1.0 - 0.14 * warm)])
    srgb = np.where(rgb <= 0.0031308, rgb * 12.92, 1.055 * np.clip(rgb, 0.0, 1.0) ** (1.0 / 2.4) - 0.055)
    rgba = np.dstack([srgb, np.ones((SIZE, SIZE), np.float32)])
    Image.fromarray((rgba * 255.0 + 0.5).astype(np.uint8), "RGBA").save(OUT, optimize=True)
    print(f"wrote {OUT}: mean {value.mean():.3f}, {value.min():.2f} to {value.max():.2f}")


if __name__ == "__main__":
    main()
