"""Clothes textures from the MakeHuman library (CC0, ~/.config/blender/5.2/mpfb/data/clothes),
cleaned for the game: the MakeHuman logo printed on the women's tee is painted out with the plain
fabric beside it.

python3 tools/textures/mh_clothes.py   -> build/clothes/<asset>_diffuse.png (used by
tools/blender/characters/finish.py in place of the library's)
"""

import os

from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
LIB = os.path.expanduser("~/.config/blender/5.2/mpfb/data/clothes")
OUT = os.path.join(ROOT, "build", "clothes")

# Asset -> boxes (x0, y0, x1, y1, dx): each box filled from the same-size patch dx pixels across.
PATCHES = {"female_casualsuit01": [(1440, 300, 1680, 750, -260), (1680, 300, 1902, 750, -260)]}


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, boxes in PATCHES.items():
        img = Image.open(os.path.join(LIB, name, f"{name}_diffuse.png")).convert("RGB")
        for x0, y0, x1, y1, dx in boxes:
            # Everything in the box but the tee's dark collar band (kept where the neckline
            # curves into it) takes the fabric beside it.
            src, dst = img.crop((x0 + dx, y0, x1 + dx, y1)), img.crop((x0, y0, x1, y1))
            keep = Image.eval(dst.convert("L"), lambda v: 255 if v < 60 else 0)
            img.paste(Image.composite(dst, src, keep), (x0, y0))
        img.save(os.path.join(OUT, f"{name}_diffuse.png"))
        print(f"[mh_clothes] {name}")


main()
