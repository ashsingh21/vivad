"""Screenshot triage in text, so reviewing the game costs a few hundred tokens instead of a few
thousand per image. Look at pixels only when the numbers say something is worth looking at.

  python3 tools/shots.py stats <dir|png...>          per-shot numbers and flags (no images)
  python3 tools/shots.py diff <baseline_dir> <dir>   what changed since the baseline, where
  python3 tools/shots.py sheet <dir|png...> -o out.png [--cols 3 --width 1200]
                                                     one labelled contact sheet to view instead
                                                     of several full-size shots
  python3 tools/shots.py crop <png> <x0> <y0> <x1> <y1> -o out.png [--width 640]
                                                     a region (fractions 0..1) at small size
  python3 tools/shots.py flicker <dir> [--moving]   from bursts (VIVAD_TOUR_FRAMES): pixels that
                                                     change frame to frame (still camera) or that
                                                     don't move smoothly (VIVAD_TOUR_DRIFT)
  python3 tools/shots.py baseline <dir> <baseline_dir>
                                                     keep the current shots as the reference

The HUD (minimap, status text, hotbar, bars) is masked out of every measurement.
"""

import argparse
import glob
import os
import shutil
import sys

import numpy as np
from PIL import Image, ImageDraw

# HUD rectangles as fractions of the frame (x0, y0, x1, y1).
HUD = [(0.85, 0.0, 1.0, 0.34), (0.0, 0.88, 0.25, 1.0), (0.63, 0.86, 1.0, 1.0), (0.33, 0.77, 0.67, 0.83)]
GRID = 4


def files(args):
    out = []
    for a in args:
        out += sorted(glob.glob(os.path.join(a, "*.png"))) if os.path.isdir(a) else [a]
    return out


def load(path):
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.float32)


def hud_mask(h, w):
    m = np.ones((h, w), bool)
    for x0, y0, x1, y1 in HUD:
        m[int(y0 * h):int(y1 * h), int(x0 * w):int(x1 * w)] = False
    return m


def luma(img):
    return img @ np.array([0.2126, 0.7152, 0.0722], np.float32)


def stats(path):
    img = load(path)
    h, w, _ = img.shape
    m = hud_mask(h, w)
    y = luma(img)
    yv = y[m]
    gx = np.abs(np.diff(y, axis=1))[m[:, 1:]]
    gy = np.abs(np.diff(y, axis=0))[m[1:, :]]
    detail = (gx.mean() + gy.mean()) / 2
    r, g, b = (img[..., k][m] for k in range(3))
    sat = (np.max(img, axis=2) - np.min(img, axis=2))[m]
    magenta = ((r > 200) & (g < 60) & (b > 200)).mean() * 100
    s = {
        "mean": yv.mean(), "p5": np.percentile(yv, 5), "p95": np.percentile(yv, 95),
        "dark%": (yv < 25).mean() * 100, "blown%": (yv > 245).mean() * 100,
        "detail": detail, "sat": sat.mean(), "magenta%": magenta,
        "rgb": (r.mean(), g.mean(), b.mean()),
    }
    flags = []
    if s["mean"] < 40 or s["dark%"] > 40:
        flags.append("DARK")
    if s["blown%"] > 12:
        flags.append("BLOWN")
    if s["detail"] < 1.3:
        flags.append("FLAT")  # empty / untextured / camera inside a wall
    if s["magenta%"] > 0.5:
        flags.append("MISSING-TEX")
    # One region taking most of the frame with almost no detail: camera clipped into geometry.
    cells = cell_detail(y, m)
    if (cells < 1.0).sum() >= GRID * GRID // 2:
        flags.append("OCCLUDED?")
    s["flags"] = flags
    s["cells"] = cells
    return s


def cell_detail(y, m):
    h, w = y.shape
    g = np.zeros_like(y)
    g[:, 1:] += np.abs(np.diff(y, axis=1))
    g[1:, :] += np.abs(np.diff(y, axis=0))
    out = np.zeros((GRID, GRID))
    for i in range(GRID):
        for j in range(GRID):
            sl = (slice(i * h // GRID, (i + 1) * h // GRID), slice(j * w // GRID, (j + 1) * w // GRID))
            mm = m[sl]
            out[i, j] = g[sl][mm].mean() if mm.any() else 99
    return out


def cmd_stats(a):
    print(f"{'shot':<26} {'mean':>5} {'p5':>4} {'p95':>4} {'dark%':>6} {'blown%':>6} {'detail':>6} {'sat':>5}  flags")
    for p in files(a.paths):
        s = stats(p)
        print(f"{os.path.basename(p)[:26]:<26} {s['mean']:5.0f} {s['p5']:4.0f} {s['p95']:4.0f} {s['dark%']:6.1f} "
              f"{s['blown%']:6.1f} {s['detail']:6.1f} {s['sat']:5.0f}  {' '.join(s['flags']) or '-'}")


def cmd_diff(a):
    base = {os.path.basename(p): p for p in files([a.baseline])}
    print(f"{'shot':<26} {'changed%':>8} {'meanΔ':>6} {'lumaΔ':>6}  where (4x4, row-major; # >20% . >3%)")
    for p in files([a.dir]):
        n = os.path.basename(p)
        if n not in base:
            print(f"{n[:26]:<26}  (new, no baseline)")
            continue
        A, B = load(base[n]), load(p)
        if A.shape != B.shape:
            print(f"{n[:26]:<26}  (size changed)")
            continue
        h, w, _ = A.shape
        m = hud_mask(h, w)
        d = np.abs(A - B).max(axis=2)
        ch = (d > 24) & m
        grid = []
        for i in range(GRID):
            row = ""
            for j in range(GRID):
                c = ch[i * h // GRID:(i + 1) * h // GRID, j * w // GRID:(j + 1) * w // GRID].mean()
                row += "#" if c > 0.2 else "." if c > 0.03 else " "
            grid.append(row)
        dl = luma(B)[m].mean() - luma(A)[m].mean()
        print(f"{n[:26]:<26} {ch[m].mean() * 100:8.1f} {d[m].mean():6.1f} {dl:+6.1f}  [{'|'.join(grid)}]")


def cmd_sheet(a):
    paths = files(a.paths)
    cols = min(a.cols, len(paths))
    rows = (len(paths) + cols - 1) // cols
    tw = a.width // cols
    th = None
    tiles = []
    for p in paths:
        im = Image.open(p).convert("RGB")
        th = th or int(tw * im.height / im.width)
        tiles.append(im.resize((tw, th), Image.LANCZOS))
    sheet = Image.new("RGB", (tw * cols, (th + 16) * rows), (20, 20, 20))
    d = ImageDraw.Draw(sheet)
    for k, (p, t) in enumerate(zip(paths, tiles)):
        x, y = (k % cols) * tw, (k // cols) * (th + 16)
        sheet.paste(t, (x, y + 16))
        d.text((x + 4, y + 2), os.path.splitext(os.path.basename(p))[0][:40], fill=(255, 230, 150))
    sheet.save(a.out)
    tokens = sheet.width * sheet.height / 750
    print(f"{a.out}: {len(paths)} shots, {sheet.width}x{sheet.height} (~{tokens:.0f} tokens to view)")


def cmd_crop(a):
    im = Image.open(a.png).convert("RGB")
    W, H = im.size
    c = im.crop((int(a.x0 * W), int(a.y0 * H), int(a.x1 * W), int(a.y1 * H)))
    if c.width > a.width:
        c = c.resize((a.width, int(c.height * a.width / c.width)), Image.LANCZOS)
    c.save(a.out)
    print(f"{a.out}: {c.width}x{c.height} (~{c.width * c.height / 750:.0f} tokens to view)")


def cmd_flicker(a):
    """Shots named <view>_f<k>.png: with the camera still, any change between frames is the
    renderer's own noise (unresolved SSAO, shadow or LOD dithering)."""
    views = {}
    for p in files([a.dir]):
        n = os.path.splitext(os.path.basename(p))[0]
        if "_f" in n:
            views.setdefault(n.rsplit("_f", 1)[0], []).append(p)
    print(f"{'view':<24} {'frames':>6} {'flicker%':>8} {'meanΔ':>6} {'maxΔ':>5}  where (4x4; # >5% . >1%)")
    for v, ps in sorted(views.items()):
        imgs = [luma(load(p)) for p in sorted(ps)]
        h, w = imgs[0].shape
        m = hud_mask(h, w)
        if a.moving:
            # Camera drifting steadily: a smoothly moving image has each frame close to the mean
            # of its neighbours; shimmer, z-fighting and popping break that.
            d = np.max([np.abs(m1 - (m0 + m2) / 2) for m0, m1, m2 in zip(imgs, imgs[1:], imgs[2:])], axis=0)
        else:
            d = np.max([np.abs(b - a) for a, b in zip(imgs, imgs[1:])], axis=0)
        fl = (d > 6) & m
        grid = []
        for i in range(GRID):
            row = ""
            for j in range(GRID):
                c = fl[i * h // GRID:(i + 1) * h // GRID, j * w // GRID:(j + 1) * w // GRID].mean()
                row += "#" if c > 0.05 else "." if c > 0.01 else " "
            grid.append(row)
        print(f"{v[:24]:<24} {len(ps):6d} {fl[m].mean() * 100:8.2f} {d[m].mean():6.2f} {d[m].max():5.0f}  [{'|'.join(grid)}]")


def cmd_baseline(a):
    os.makedirs(a.baseline, exist_ok=True)
    ps = files([a.dir])
    for p in ps:
        shutil.copy(p, os.path.join(a.baseline, os.path.basename(p)))
    print(f"{len(ps)} shots -> {a.baseline}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("stats"); p.add_argument("paths", nargs="+"); p.set_defaults(f=cmd_stats)
    p = sub.add_parser("diff"); p.add_argument("baseline"); p.add_argument("dir"); p.set_defaults(f=cmd_diff)
    p = sub.add_parser("sheet"); p.add_argument("paths", nargs="+"); p.add_argument("-o", "--out", required=True)
    p.add_argument("--cols", type=int, default=3); p.add_argument("--width", type=int, default=1200); p.set_defaults(f=cmd_sheet)
    p = sub.add_parser("crop"); p.add_argument("png")
    for k in ("x0", "y0", "x1", "y1"):
        p.add_argument(k, type=float)
    p.add_argument("-o", "--out", required=True); p.add_argument("--width", type=int, default=640); p.set_defaults(f=cmd_crop)
    p = sub.add_parser("flicker"); p.add_argument("dir"); p.add_argument("--moving", action="store_true")
    p.set_defaults(f=cmd_flicker)
    p = sub.add_parser("baseline"); p.add_argument("dir"); p.add_argument("baseline"); p.set_defaults(f=cmd_baseline)
    a = ap.parse_args()
    a.f(a)


if __name__ == "__main__":
    sys.exit(main())
