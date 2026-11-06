"""The start screen's map cards (crate::menu): each map from a tour view, the HUD cropped
off, 512 x 288.

just menu-thumbs   (renders the views into build/menu_thumbs/<map>/, then writes assets/ui/maps/<map>.png)
"""

import os
import sys

from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
# Map -> the tour view it's drawn from (crate::autoshot).
VIEWS = {"cp": "cp_aerial", "chandni": "ch_air", "hauzkhas": "hk_madrasa_air", "indiagate": "ig_gate_side", "ncr": "ncr_karolbagh_air"}


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "build", "menu_thumbs")
    out = os.path.join(ROOT, "assets", "ui", "maps")
    os.makedirs(out, exist_ok=True)
    for m, view in VIEWS.items():
        im = Image.open(os.path.join(src, m, f"{view}.png")).convert("RGB")
        s = im.size[0] / 1600
        # Left of the minimap, above the hint and the bars: a 16:9 piece of the 1600 x 900 view.
        box = [int(x * s) for x in (40, 0, 40 + 1244, 700)]
        im.crop(box).resize((512, 288), Image.LANCZOS).save(os.path.join(out, f"{m}.png"), optimize=True)
        print(f"[thumbs] {m}: {view}")


if __name__ == "__main__":
    main()
