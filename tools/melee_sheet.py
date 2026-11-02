"""One sheet of every melee weapon in the game, from armoury shots (VIVAD_ARMOURY, see
crates/game/src/autoshot.rs): each weapon carried, on guard and mid-swing, with its numbers from
assets/data/items.ron.

python3 tools/melee_sheet.py <armoury_dir> <out.png>
"""

import glob
import os
import re
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ROWS = [
    ("bat", "New: hits hard"),
    ("gada", "Old twin of the bat: slowest, biggest throw"),
    ("hockey_stick", "New: fast, hooks and pulls in"),
    ("talwar", "Old twin of the stick: fast, cuts hardest"),
    ("tawa", "New: blocks blows, stops most of a bullet"),
    ("dhal", "Old twin of the tawa: blocks nearly all"),
    ("lathi", "Both ages: longest reach, narrow swing"),
]
TILE = (300, 300)
TEXT_W = 400


def row_of(items, item):
    m = re.search(r'"' + item + r'": \(\s*name: "([^"]+)".*?kind: Melee\(([^)]*)\),\s*drops: \[([^\]]*)\]', items, re.S)
    name, kind, drops = m.groups()
    n = dict(p.split(": ") for p in kind.split(", "))
    return name, n, re.findall(r"\((\w+),", drops)


def font(size):
    for path in (os.path.join(ROOT, "assets", "fonts", "Mukta-SemiBold.ttf"), "/System/Library/Fonts/Supplemental/Arial.ttf"):
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def tile(path):
    """The warrior: the middle of the shot."""
    im = Image.open(path).convert("RGB")
    w, h = im.size
    side = int(h * 0.82)
    x, y = (w - side) // 2, int(h * 0.02)
    return im.crop((x, y, x + side, y + side)).resize(TILE, Image.LANCZOS)


def main():
    src, out = sys.argv[1:3]
    items = open(os.path.join(ROOT, "assets", "data", "items.ron")).read()
    big, small = font(30), font(19)
    sheet = Image.new("RGB", (TEXT_W + TILE[0] * 3, 44 + TILE[1] * len(ROWS)), (20, 20, 22))
    d = ImageDraw.Draw(sheet)
    for k, head in enumerate(("Carried", "Guard (aim held)", "Mid-swing")):
        d.text((TEXT_W + k * TILE[0] + 10, 10), head, font=small, fill=(240, 225, 180))
    for r, (item, job) in enumerate(ROWS):
        name, n, drops = row_of(items, item)
        y = 44 + r * TILE[1]
        for c, state in enumerate(("carry_front", "aim_front", "strike_side")):
            hits = sorted(glob.glob(os.path.join(src, f"*_{item}_{state}.png")))
            if hits:
                sheet.paste(tile(hits[-1]), (TEXT_W + c * TILE[0], y))
        old = int(n.get("durability", 0)) > 0
        d.text((16, y + 14), name, font=big, fill=(255, 205, 120) if old else (235, 235, 235))
        lines = [job, "Drops: " + (" ".join(drops) if drops else "everyone starts with it"),
                 f"Damage {float(n['damage']):.0f}, every {n['interval']} s", f"Wind-up {n['windup']} s, arc {float(n['arc_deg']):.0f} deg",
                 f"Reach {n['reach']} m, strikes up to {n['max_hits']}", f"Guard stops {float(n['guard']) * 100:.0f}% of a blow"]
        if "shot_guard" in n:
            lines.append(f"and {float(n['shot_guard']) * 100:.0f}% of a bullet")
        if "knock" in n:
            lines.append(("Pulls in at " if float(n["knock"]) < 0 else "Knocks back at ") + f"{abs(float(n['knock'])):g} m/s")
        if "stun" in n:
            lines.append(f"Dazes for {n['stun']} s")
        lines.append(f"Breaks after {n['durability']} blows" if old else "Never breaks")
        for i, line in enumerate(lines):
            d.text((16, y + 56 + i * 22), line, font=small, fill=(200, 200, 200))
    sheet.save(out)
    print(out, sheet.size)


if __name__ == "__main__":
    main()
