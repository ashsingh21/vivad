"""The game's sky, from a photograph: Poly Haven's kloofendal_38d_partly_cloudy_puresky (CC0), an
8K .hdr kept in build/look_src, out of git. Writes build/sky_src/sky_basecolor.png, which
tools/texpack turns into assets/textures/sky/sky_basecolor.ktx2 (tools/textures/ultra_pack.txt).

python3 tools/textures/sky.py [--look]

What it does to the photograph, each forced by how crate::sky shows it:
- Keeps the upper half, 8192 x 2048: row 0 is the zenith, the last row the horizon. The dome's
  UVs follow from that.
- Turns it so the photograph's sun is at u = 0.5. crate::sky points that column at the game's sun.
- Takes the sun out. The real disc would be a white hole the game's sun doesn't match to the pixel,
  and bloom would make a meal of it. What's left of the glow round it stays.
- Exposes it to the game's camera and rolls off the highlights: the dome is an 8-bit image, drawn
  at crate::sky::PHOTO_GAIN times its value.
- Bakes the haze of a Delhi day up from the horizon, ending in exactly crate::sky::HORIZON, so the
  distance fog sinks into the sky with no line between them.
"""

import os
import sys

import numpy as np
from numba import njit
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SOURCE = os.path.join(ROOT, "build", "look_src", "kloofendal_38d_partly_cloudy_puresky_8k.hdr")
OUT = os.path.join(ROOT, "build", "sky_src")

# crate::sky::HORIZON and PHOTO_GAIN.
HORIZON = (0.90, 0.875, 0.83)
PHOTO_GAIN = 2.3


@njit(cache=False)
def _decode(data, start, w, h):
    """Radiance's new-style run-length scanlines to RGBE bytes, top row first."""
    out = np.zeros((h, w, 4), dtype=np.uint8)
    p = start
    for y in range(h):
        if data[p] != 2 or data[p + 1] != 2:
            raise ValueError("old-style scanline")
        p += 4
        for c in range(4):
            x = 0
            while x < w:
                n = int(data[p])
                p += 1
                if n > 128:
                    n -= 128
                    v = data[p]
                    p += 1
                    for _ in range(n):
                        out[y, x, c] = v
                        x += 1
                else:
                    for _ in range(n):
                        out[y, x, c] = data[p]
                        p += 1
                        x += 1
    return out


def read_hdr(path):
    raw = np.fromfile(path, dtype=np.uint8)
    head = raw[:4096].tobytes()
    end = head.index(b"\n\n") + 2
    line_end = head.index(b"\n", end)
    parts = head[end:line_end].split()
    h, w = int(parts[1]), int(parts[3])
    rgbe = _decode(raw, line_end + 1, w, h)
    scale = np.ldexp(1.0, rgbe[..., 3].astype(np.int32) - (128 + 8)).astype(np.float32)
    rgb = rgbe[..., :3].astype(np.float32) * scale[..., None]
    rgb[rgbe[..., 3] == 0] = 0.0
    return rgb


def to_srgb(a):
    a = np.clip(a, 0.0, 1.0)
    return np.where(a <= 0.0031308, a * 12.92, 1.055 * a ** (1.0 / 2.4) - 0.055)


def to_linear(a):
    a = np.asarray(a, dtype=np.float32)
    return np.where(a <= 0.04045, a / 12.92, ((a + 0.055) / 1.055) ** 2.4)


def elevation(rows):
    """Degrees above the horizon of each row of the upper half."""
    return 90.0 * (1.0 - (np.arange(rows, dtype=np.float32) + 0.5) / rows)


def find_sun(rgb):
    luma = rgb.mean(-1)
    ys, xs = np.nonzero(luma > luma.max() * 0.05)
    return ys.mean(), xs.mean()


def shoulder(x, knee, top):
    """Straight up to `knee`, then rolling off to `top`, never reaching it."""
    over = np.maximum(x - knee, 0.0)
    return np.where(x <= knee, x, knee + (top - knee) * (1.0 - np.exp(-over / (top - knee))))


# Everything below is in the game's linear light, as it reaches the tone mapper. The photograph's
# median clear sky comes out at about GAIN x 0.22.
GAIN = 2.2
# Highlights: straight to KNEE, then rolling off to what the 8 bits can hold.
KNEE = 1.0
# Delhi's sky is pale: the share of the photograph's colour that's kept.
BLUE = 0.86
# Share of haze overhead, and how fast it thickens down to all haze at the horizon (degrees).
HAZE_TOP = 0.21
HAZE_FALL = 10.0
# The haze's own colour higher up: dust at the horizon, plain pale air over it (sRGB).
HIGH_HAZE = (0.86, 0.90, 0.95)
# The dusty band in the last few degrees.
LOW_BAND = 3.0
# Round the sun, what the sky can't show: the disc and its coloured fringe go, the glow stays.
SUN_LIMIT = 5.0


def smoothstep(lo, hi, x):
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def bake(rgb):
    h, w = rgb.shape[:2]
    sun_y, sun_x = find_sun(rgb)
    print(f"[sky] sun at u {sun_x / w:.4f}, {90.0 - sun_y / h * 180.0:.1f} degrees up")
    up = rgb[: h // 2].copy()
    up = np.roll(up, w // 2 - int(round(sun_x)), axis=1)
    rows = up.shape[0]
    # The sun: white and no brighter than the cloud round it, however bright it was.
    luma = up.mean(-1, keepdims=True)
    white = smoothstep(1.5, 3.5, luma)
    up = up * (1.0 - white) + luma * white
    up = up * np.minimum(1.0, SUN_LIMIT / np.maximum(luma, 1e-4))
    # Exposed, with the highlights rolled off in brightness, so the clouds keep their colour.
    up = up * GAIN
    luma = up.mean(-1, keepdims=True)
    up = up * shoulder(luma, KNEE, PHOTO_GAIN) / np.maximum(luma, 1e-4)
    # Pale: pulled toward its own grey, a little warm.
    luma = (up * np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)).sum(-1, keepdims=True)
    grey = luma * np.array([1.015, 1.0, 0.975], dtype=np.float32)
    up = grey + (up - grey) * BLUE
    # The haze: all of it at the horizon, so the last row is HORIZON.
    e = elevation(rows)[:, None, None]
    haze = np.maximum(HAZE_TOP + (1.0 - HAZE_TOP) * np.exp(-e / HAZE_FALL), np.exp(-e / LOW_BAND))
    horizon = to_linear(np.array(HORIZON, dtype=np.float32)).astype(np.float32)
    high = to_linear(np.array(HIGH_HAZE, dtype=np.float32)).astype(np.float32)
    air = horizon + (high - horizon) * smoothstep(1.0, 24.0, e)
    return up * (1.0 - haze) + air * haze


def main():
    rgb = read_hdr(SOURCE)
    sky = bake(rgb)
    # A little noise before the 8 bits, or the long soft gradients band.
    noise = np.random.default_rng(7).random(sky.shape, dtype=np.float32) - 0.5
    out = to_srgb(sky / PHOTO_GAIN) * 255.0 + noise * 1.2
    os.makedirs(OUT, exist_ok=True)
    Image.fromarray(np.clip(out + 0.5, 0, 255).astype(np.uint8), "RGB").save(os.path.join(OUT, "sky_basecolor.png"))
    print(f"[sky] {sky.shape[1]}x{sky.shape[0]}, mean {sky.mean():.3f}, last row {sky[-1].mean(0)}")
    if "--look" in sys.argv:
        Image.fromarray((to_srgb(sky[::4, ::4] / PHOTO_GAIN) * 255.0).astype(np.uint8)).save(os.path.join(OUT, "look.jpg"), quality=90)


if __name__ == "__main__":
    main()
