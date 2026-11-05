"""Clothes textures from the MakeHuman library (CC0, ~/.config/blender/5.2/mpfb/data/clothes),
cleaned for the game: the MakeHuman logo printed on the women's tee is painted out with the plain
fabric beside it.

python3 tools/textures/mh_clothes.py   -> build/clothes/<asset>_diffuse.png (used by
tools/blender/characters/finish.py in place of the library's), and the anime character's
flat-coloured ones, build/clothes/anime_<asset>_diffuse.png
"""

import os

from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
LIB = os.path.expanduser("~/.config/blender/5.2/mpfb/data/clothes")
OUT = os.path.join(ROOT, "build", "clothes")

# Asset -> boxes (x0, y0, x1, y1, dx): each box filled from the same-size patch dx pixels across.
PATCHES = {"female_casualsuit01": [(1440, 300, 1680, 750, -260), (1680, 300, 1902, 750, -260)]}
# Asset -> [(region, sRGB)]: each region recoloured, its own light and shade (weave, folds,
# wear) kept about the colour. A region is a box (x0, y0, x1, y1); None is all the rest.
TINTS = {
    # A charcoal shirt over olive trousers (the jeans: the atlas's middle column).
    "male_casualsuit01": [((740, 0, 1460, 2048), (0.30, 0.31, 0.20)), (None, (0.17, 0.18, 0.19))],
    # An olive tee (the top half and the sleeves, bottom right) over charcoal trousers.
    "female_casualsuit01": [((0, 0, 2048, 880), (0.32, 0.34, 0.22)), ((1460, 1150, 2048, 2048), (0.32, 0.34, 0.22)),
                            (None, (0.17, 0.18, 0.19))],
}


def tint(img, regions):
    """Recolour by region: each pixel the region's colour scaled by its brightness against the
    region's mean (contrast eased so faded denim doesn't go to white)."""
    import numpy as np
    a = np.asarray(img, dtype=np.float32) / 255.0
    lum = a @ np.array([0.299, 0.587, 0.114], dtype=np.float32)
    taken = np.zeros(lum.shape, dtype=bool)
    out = a.copy()
    for box, rgb in regions:
        m = ~taken
        if box is not None:
            x0, y0, x1, y1 = box
            b = np.zeros_like(taken)
            b[y0:y1, x0:x1] = True
            m &= b
        taken |= m
        k = 1.0 + 0.5 * (lum[m] / max(float(lum[m].mean()), 1e-3) - 1.0)
        out[m] = np.clip(np.array(rgb, dtype=np.float32)[None, :] * k[:, None], 0.0, 1.0)
    return Image.fromarray((out * 255).astype(np.uint8))


# The anime character's garments: each pixel recoloured flat by what it was (by hue and
# brightness), the fabric's grain blurred away, for cel shading. (name, [(test, sRGB)]): the
# first test a pixel passes picks its colour; tests see (r, g, b, hue 0..1, sat, value).
ANIME = {
    # A full-length tee in deep teal (well clear of her skin's tone in any light), its dark collar
    # band navy, over navy trousers: the tee is the atlas's top band and the sleeves bottom right
    # (as TINTS' female_casualsuit01).
    "female_casualsuit01": [(lambda r, g, b, h, s, v, x, y: ((y < 880) | ((x >= 1460) & (y >= 1150))) & (v < 0.25), (0.13, 0.17, 0.30)),
                            (lambda r, g, b, h, s, v, x, y: (y < 880) | ((x >= 1460) & (y >= 1150)), (0.08, 0.52, 0.52)),
                            (lambda r, g, b, h, s, v, x, y: v >= 0.0, (0.13, 0.17, 0.30))],
    # Shoes: clean cream, only their darkest parts (the sole) navy.
    "shoes01": [(lambda r, g, b, h, s, v, x, y: v < 0.12, (0.13, 0.17, 0.30)), (lambda r, g, b, h, s, v, x, y: v >= 0.0, (0.99, 0.95, 0.86))],
}


def flat(img, rules):
    import numpy as np
    from PIL import ImageFilter
    a = np.asarray(img.filter(ImageFilter.GaussianBlur(3)), dtype=np.float32) / 255.0
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    mx, mn = a.max(-1), a.min(-1)
    v, d = mx, mx - mn
    s = np.where(mx > 1e-4, d / np.maximum(mx, 1e-4), 0)
    h = np.where(d < 1e-4, 0, np.where(mx == r, ((g - b) / np.maximum(d, 1e-4)) % 6, np.where(mx == g, (b - r) / np.maximum(d, 1e-4) + 2, (r - g) / np.maximum(d, 1e-4) + 4))) / 6
    y, x = np.mgrid[0:v.shape[0], 0:v.shape[1]]
    out = np.zeros_like(a)
    done = np.zeros(v.shape, bool)
    for test, rgb in rules:
        m = test(r, g, b, h, s, v, x, y) & ~done
        out[m] = rgb
        done |= m
    return Image.fromarray((out * 255).astype(np.uint8))


def main():
    os.makedirs(OUT, exist_ok=True)
    for name in sorted(set(PATCHES) | set(TINTS)):
        boxes = PATCHES.get(name, [])
        img = Image.open(os.path.join(LIB, name, f"{name}_diffuse.png")).convert("RGB")
        for x0, y0, x1, y1, dx in boxes:
            # Everything in the box but the tee's dark collar band (kept where the neckline
            # curves into it) takes the fabric beside it.
            src, dst = img.crop((x0 + dx, y0, x1 + dx, y1)), img.crop((x0, y0, x1, y1))
            keep = Image.eval(dst.convert("L"), lambda v: 255 if v < 60 else 0)
            img.paste(Image.composite(dst, src, keep), (x0, y0))
        if name in TINTS:
            img = tint(img, TINTS[name])
        img.save(os.path.join(OUT, f"{name}_diffuse.png"))
        print(f"[mh_clothes] {name}")
    for name, rules in ANIME.items():
        img = Image.open(os.path.join(LIB, name, f"{name}_diffuse.png")).convert("RGB")
        flat(img, rules).save(os.path.join(OUT, f"anime_{name}_diffuse.png"))
        print(f"[mh_clothes] anime {name}")


main()
