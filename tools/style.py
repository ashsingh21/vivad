"""The style guide's colour tokens (docs/style-guide.md) for the Blender and texture scripts:
assets/style/palette.json, the one source the game reads too (shared::style).

    from style import srgb, linear
    srgb("architecture.stucco")    # (r, g, b) 0..1, sRGB: for PIL painting
    linear("architecture.stucco")  # linear: for Blender materials and vertex colours
"""

import json
import os

_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "style", "palette.json")
_T = {}
for _g, _cs in json.load(open(_PATH)).items():
    if isinstance(_cs, dict):
        for _n, _h in _cs.items():
            if isinstance(_h, str) and _h.startswith("#"):
                _T[f"{_g}.{_n}"] = tuple(int(_h[i:i + 2], 16) / 255 for i in (1, 3, 5))


def srgb(name):
    if name not in _T:
        raise KeyError(f"no colour `{name}` in assets/style/palette.json")
    return _T[name]


def linear(name):
    return tuple(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in srgb(name))


def srgb8(name):
    return tuple(round(c * 255) for c in srgb(name))
