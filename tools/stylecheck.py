"""The style board by the numbers (docs/style-guide.md sections 2, 5 and 8).

  python3 tools/stylecheck.py            play the style_* tour views (Connaught Place, seed 7) and
                                         measure them (`just stylecheck`)
  python3 tools/stylecheck.py <dir>      measure shots already taken
  python3 tools/stylecheck.py --keep     the last run's numbers become the baseline

Per view, over the frame with the HUD left out:
  bright   median and 90th-percentile brightness (HSV value): the guide's high key wants sunlit
           white near 0.85-0.92, so a street view's p90 sits there
  clip     the share of pixels blown out (value over 0.98)
  dark     the share darker than 0.15 (the guide: nothing large that dark but shade on tarmac)
  sat      mean saturation, and the share of pixels strongly coloured (over 0.45): calm large
           surfaces, accents no more than about a fifth
  blue     how blue the shade is: mean (b - r) over the darker half of the near-grey pixels (the
           guide's shade is light blue, not grey)
Writes previews/style/sheet.png, a contact sheet of the views.
"""

import json
import os
import subprocess
import sys

import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT = os.path.join(ROOT, "previews", "style")
LAST = os.path.join(OUT, "last.json")
BASELINE = os.path.join(OUT, "baseline.json")
VIEWS = ["style_block", "style_facade", "style_trees", "style_kerb", "style_verandah", "style_shop", "style_roof", "style_room"]

# (metric, low, high): None for no bound. The views facing the sun (the roof) are held to the high
# key; the block faces away from it (Connaught Place is a ring: half of it always does), so its
# views are held to light shade instead; rooms are lit, not sunlit. Mean saturation and the shade's
# blue are reported, not judged: the guide's white walls dominate a frame on purpose.
SUNLIT = {"p90": (0.82, 0.95), "clip": (None, 0.02), "dark": (None, 0.10), "loud": (None, 0.22)}
SHADE = {"p50": (0.45, None), "clip": (None, 0.02), "dark": (None, 0.10), "loud": (0.02, 0.22)}
ROOM = {"p50": (0.35, None), "dark": (None, 0.10)}
# The park: open sky and lawn fill most of the frame, both strongly coloured by design, so its
# loudness isn't judged; only that it's bright and nothing blows out or goes black.
PARK = {"p50": (0.45, None), "clip": (None, 0.02), "dark": (None, 0.10)}
KIND = {"style_roof": SUNLIT, "style_room": ROOM, "style_trees": PARK}


def measure(path):
    a = np.asarray(Image.open(path).convert("RGB"), dtype=np.float32) / 255.0
    h, w, _ = a.shape
    # The HUD: the minimap and match lines top right, the health and hotbar along the bottom.
    a = a[int(h * 0.06):int(h * 0.86), int(w * 0.02):int(w * 0.84)]
    mx, mn = a.max(-1), a.min(-1)
    sat = np.where(mx > 1e-4, (mx - mn) / np.maximum(mx, 1e-4), 0)
    grey = (sat < 0.25) & (mx < np.median(mx))
    blue = float(np.mean(a[..., 2][grey] - a[..., 0][grey])) if grey.any() else 0.0
    return {
        "p50": float(np.median(mx)),
        "p90": float(np.percentile(mx, 90)),
        "clip": float(np.mean(mx > 0.98)),
        "dark": float(np.mean(mx < 0.15)),
        "sat": float(np.mean(sat)),
        "loud": float(np.mean(sat > 0.45)),
        "blue": blue,
    }


def verdict(view, k, v):
    lo, hi = KIND.get(view, SHADE).get(k, (None, None))
    if lo is not None and v < lo:
        return f"! under {lo:g}"
    if hi is not None and v > hi:
        return f"! over {hi:g}"
    return "ok" if (lo is not None or hi is not None) else ""


def take(dirpath):
    os.makedirs(dirpath, exist_ok=True)
    env = dict(os.environ, VIVAD_MUTE="1", VIVAD_TOUR=dirpath, VIVAD_TOUR_ONLY=",".join(VIEWS), VIVAD_MAP="cp", VIVAD_MAP_SEED="7")
    subprocess.run([os.path.join(ROOT, "target", "release", "game")], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    args = sys.argv[1:]
    if args == ["--keep"]:
        os.replace(LAST, BASELINE) if os.path.exists(LAST) else sys.exit("no last run")
        print(f"[style] baseline kept: {BASELINE}")
        return
    shots = args[0] if args else os.path.join(OUT, "shots")
    if not args:
        take(shots)
    base = json.load(open(BASELINE)) if os.path.exists(BASELINE) else {}
    res, bad = {}, 0
    for view in VIEWS:
        p = os.path.join(shots, f"{view}.png")
        if not os.path.exists(p):
            print(f"[style] {view}: no shot")
            continue
        m = res[view] = measure(p)
        print(f"[style] {view}")
        for k, v in m.items():
            vd = verdict(view, k, v)
            bad += vd.startswith("!")
            was = base.get(view, {}).get(k)
            print(f"[style]   {k:5s} {v:6.3f}  {vd:12s}" + (f"  (was {was:.3f})" if was is not None and abs(was - v) > 0.005 else ""))
    os.makedirs(OUT, exist_ok=True)
    json.dump(res, open(LAST, "w"), indent=1)
    have = [os.path.join(shots, f"{v}.png") for v in VIEWS if os.path.exists(os.path.join(shots, f"{v}.png"))]
    subprocess.run([sys.executable, os.path.join(ROOT, "tools", "shots.py"), "sheet", *have, "-o", os.path.join(OUT, "sheet.png"), "--width", "1500"],
                   stdout=subprocess.DEVNULL)
    print(f"[style] {len(res)} views, {bad} off the guide; sheet: previews/style/sheet.png")


if __name__ == "__main__":
    main()
