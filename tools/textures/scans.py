"""Surface textures from photo scans (Poly Haven, CC0), stylised for the game: the shapes a scan
gives (each brick and tile its own tone, chipped arrises, dark mortar and joints, worn patches)
kept, its fine grain smoothed away, its crevices darkened from the scan's own occlusion so they
read at a distance, and its colour either graded (a material drawn in its own colours) or turned
to near-white greys (one the game tints with a palette token, as Mat::Stone and Mat::Stucco are).

Each scan is repeated to the game's 2 m tile (crate::buildings::TILE) by its real-world size.
Raw downloads land in build/scans/<asset>/ (not committed); the outputs replace the generated
sets in assets/textures/.

Also the ground's detail (crate::ground, 4 m a repeat): asphalt, paving slabs and earth as the
three channels of one texture, each a scan's shading about mid-grey.

python3 tools/textures/scans.py [name ...]   -> assets/textures/<set>_{basecolor,normal}.png,
                                                assets/textures/panga/ground_detail.png ("ground")
"""

import io
import json
import os
import subprocess
import sys
import urllib.request

import numpy as np
from PIL import Image, ImageFilter

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
RAW = os.path.join(ROOT, "build", "scans")
TILE_MM = 2000.0
N = 1024

# Output set -> the scan and how it's styled. `tint`: the game tints it (greys, its lightest at
# near-white, darker by `contrast` of the scan's own shading; `chequer`: (tiles a side, the
# darker tiles' step), `grout`: how dark the chequer's joints are drawn); else graded (`sat` scales saturation, `gain` brightness). `cavity`: how hard the occlusion
# darkens crevices. `smooth`: the grain removed (a median's size, pixels at 1k). `normal`: its
# strength.
SETS = {
    # 50 cm tiles on the old procedural floor's grid, laid as its two-tone chequer again.
    "panga/stone": dict(asset="worn_tile_floor", tint=True, contrast=0.15, cavity=1.0, smooth=5, normal=1.0, chequer=(4, 0.15), grout=0.4),
    "panga/stucco": dict(asset="grey_plaster_02", tint=True, contrast=0.2, cavity=0.6, smooth=7, normal=0.9),
    "india/brick": dict(asset="red_brick_03", tint=False, sat=1.15, gain=1.12, cavity=1.4, smooth=3, normal=1.2),
    # Painted by tools/textures/city.py (its bands, bays and windows), the scan's patchy plaster
    # laid over them (`over`): its shading by `contrast`, its relief by `normal`.
    # (Patches chipped to brick were tried: on a 2 m repeat they read as rows of black blots.)
    "panga/rustic": dict(asset="grey_plaster_02", over="rustic", contrast=0.35, smooth=5, normal=0.45),
    "panga/facade": dict(asset="grey_plaster_02", over="facade", contrast=0.3, smooth=5, normal=0.8),
}


def get(url):
    """A URL's bytes (Poly Haven turns away Python's default user agent)."""
    req = urllib.request.Request(url, headers={"User-Agent": "vivad-textures/1.0"})
    with urllib.request.urlopen(req) as r:
        return r.read()


def fetch(asset):
    """The scan's 1k diffuse, OpenGL normal and occlusion, downloaded once; its size in mm."""
    folder = os.path.join(RAW, asset)
    os.makedirs(folder, exist_ok=True)
    info = os.path.join(folder, "info.json")
    if not os.path.exists(info):
        open(info, "wb").write(get(f"https://api.polyhaven.com/info/{asset}"))
    for key in ["Diffuse", "nor_gl", "AO"]:
        path = os.path.join(folder, f"{key}.png")
        if os.path.exists(path):
            continue
        m = json.loads(get(f"https://api.polyhaven.com/files/{asset}"))[key]["1k"]
        Image.open(io.BytesIO(get((m.get("png") or m["jpg"])["url"]))).save(path)
    dims = json.load(open(info)).get("dimensions") or [TILE_MM, TILE_MM]
    return folder, dims


def tiled(img, repeat):
    """`img` repeated `repeat` times each way (a seamless scan: whole repeats only), at N."""
    k = max(1, round(repeat))
    w, h = img.size
    out = Image.new(img.mode, (w * k, h * k))
    for i in range(k):
        for j in range(k):
            out.paste(img, (i * w, j * h))
    return out.resize((N, N), Image.LANCZOS)


def arr(img):
    return np.asarray(img, dtype=np.float32) / 255.0


def srgb_to_lin(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def lin_to_srgb(c):
    c = np.clip(c, 0, 1)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def overlay(name, s):
    """City.py's set `s["over"]`, repainted, with the scan's plaster over it."""
    subprocess.run([sys.executable, os.path.join(ROOT, "tools", "textures", "city.py"), s["over"]], check=True)
    out = os.path.join(ROOT, "assets", "textures", name)
    base = Image.open(f"{out}_basecolor.png").convert("RGB")
    size = base.size
    folder, dims = fetch(s["asset"])
    repeat = TILE_MM / max(dims[0], 1.0)
    load = lambda k, mode: tiled(Image.open(os.path.join(folder, f"{k}.png")).convert(mode), repeat).resize(size, Image.LANCZOS)
    # Broad patches only (blurred): up close a plaster's speckle reads as grime.
    lum = srgb_to_lin(arr(load("Diffuse", "L").filter(ImageFilter.MedianFilter(s["smooth"])).filter(ImageFilter.GaussianBlur(3))))
    v = 1 + (lum / lum.mean() - 1) * s["contrast"]
    col = srgb_to_lin(arr(base)) * v[..., None]
    # Relief added: the scan's tilt on top of the painted one.
    n = arr(Image.open(f"{out}_normal.png").convert("RGB")) * 2 - 1
    sn = arr(load("nor_gl", "RGB").filter(ImageFilter.GaussianBlur(0.8))) * 2 - 1
    n[..., :2] += sn[..., :2] * s["normal"]
    Image.fromarray((lin_to_srgb(col) * 255 + 0.5).astype(np.uint8)).save(f"{out}_basecolor.png")
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    Image.fromarray(((n * 0.5 + 0.5) * 255 + 0.5).astype(np.uint8)).save(f"{out}_normal.png")
    print(f"{name}: city.py {s['over']} + {s['asset']}")


def build(name, s):
    if "over" in s:
        return overlay(name, s)
    folder, dims = fetch(s["asset"])
    repeat = TILE_MM / max(dims[0], 1.0)
    load = lambda k, mode: tiled(Image.open(os.path.join(folder, f"{k}.png")).convert(mode), repeat)
    diffuse, normal, ao = load("Diffuse", "RGB"), load("nor_gl", "RGB"), load("AO", "L")
    # Grain away, shapes kept: a median keeps edges (joints, arrises) and flattens speckle.
    diffuse = diffuse.filter(ImageFilter.MedianFilter(s["smooth"]))
    normal = normal.filter(ImageFilter.MedianFilter(s["smooth"])).filter(ImageFilter.GaussianBlur(0.8))
    col = srgb_to_lin(arr(diffuse))
    occ = arr(ao.filter(ImageFilter.GaussianBlur(1.0)))[..., None]
    col *= occ ** s["cavity"]
    if s["tint"]:
        lum = (col @ np.array([0.2126, 0.7152, 0.0722], np.float32))
        dark = np.clip(1 - lum / np.percentile(lum, 97), 0, 1)
        g = 0.97 * (1 - dark * s["contrast"])
        if "chequer" in s:
            k, step = s["chequer"]
            y, x = np.mgrid[0:N, 0:N]
            g *= 1 - step * (((x * k // N) + (y * k // N)) % 2)
            # Crisp joints on the grid (the scan's own blur into its wear): 6 mm a side.
            t = N / k
            edge = np.minimum(np.minimum(x % t, t - x % t), np.minimum(y % t, t - y % t))
            g *= 1 - s.get("grout", 0) * np.clip(1.5 - edge / 2.0, 0, 1)
        col = np.repeat(np.clip(g, 0.03, 1.0)[..., None], 3, axis=2)
    else:
        lum = (col @ np.array([0.2126, 0.7152, 0.0722], np.float32))[..., None]
        col = (lum + (col - lum) * s["sat"]) * s["gain"]
    base = Image.fromarray((lin_to_srgb(col) * 255 + 0.5).astype(np.uint8))
    # The normal's tilt scaled (z rebuilt to unit length).
    n = arr(normal) * 2 - 1
    n[..., :2] *= s["normal"]
    n[..., 2] = np.sqrt(np.clip(1 - n[..., 0] ** 2 - n[..., 1] ** 2, 0, 1))
    nrm = Image.fromarray(((n * 0.5 + 0.5) * 255 + 0.5).astype(np.uint8))
    out = os.path.join(ROOT, "assets", "textures", name)
    base.save(f"{out}_basecolor.png")
    nrm.save(f"{out}_normal.png")
    print(f"{name}: {s['asset']} x{max(1, round(repeat))} -> {N}px")


# The ground detail's channels: (scan, spread about mid-grey, cavity, repeats in the 4 m or None
# for the scan's own size). The pavers at one repeat: 12 cm setts, not 6.
GROUND = [("asphalt_02", 0.12, 1.0, None), ("concrete_pavers_02", 0.14, 1.6, 1), ("dry_ground_rocks", 0.13, 1.2, None)]
GROUND_MM = 4000.0


def ground():
    chans = []
    for asset, spread, cavity, fixed in GROUND:
        folder, dims = fetch(asset)
        repeat = fixed or GROUND_MM / max(dims[0], 1.0)
        load = lambda k, mode: tiled(Image.open(os.path.join(folder, f"{k}.png")).convert(mode), repeat)
        lum = srgb_to_lin(arr(load("Diffuse", "L").filter(ImageFilter.MedianFilter(3))))
        lum = lum * arr(load("AO", "L").filter(ImageFilter.GaussianBlur(1.0))) ** cavity
        z = (lum - lum.mean()) / max(lum.std(), 1e-6)
        chans.append(np.clip(0.5 + spread * z, 0, 1))
        print(f"ground: {asset} x{max(1, round(repeat))}")
    rgba = np.dstack(chans + [np.ones_like(chans[0])])
    Image.fromarray((rgba * 255 + 0.5).astype(np.uint8)).save(os.path.join(ROOT, "assets", "textures", "panga", "ground_detail.png"))


if __name__ == "__main__":
    for name in sys.argv[1:] or [*SETS, "ground"]:
        ground() if name == "ground" else build(name, SETS[name])
