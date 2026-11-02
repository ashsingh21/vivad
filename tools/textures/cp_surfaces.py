"""The real surfaces of the city's walls and streets, composited from CC0 Poly Haven scans: those in
assets/models/samples/delhi_street/textures/vendor (painted_plaster_wall, concrete_floor,
asphalt_02), those in build/look_src, out of git (brick_wall_02, dirty_concrete), and procedural
layers. assets/textures/ultra/sources.json has the credits. Writes PNGs to build/ultra_src, which
`just textures-pack` turns into assets/textures/ultra/*.ktx2 (tools/textures/ultra_pack.txt).

python3 tools/textures/cp_surfaces.py [plaster marble asphalt pavers facade brick shop water]

Conventions, each forced by how crate::buildings and crate::surfaces map these:
- World-mapped blocks have v = world height / TILE, so their image rows grow UPWARD on a wall: row
  0 of `plaster` is street level, and grime sits at the top of the image, not the bottom. Blocks
  are tiled every 12 m, taller than the ring, which is what lets one tile carry that height.
- `facade` and `shop` are upright (row 0 is the top of a storey) and keep crate::buildings'
  facade_image and shop_image layouts exactly: the sills and lintels in the mesh land on them.
- Ground maps store the ratio to their own mean, at 0.5 in linear light: the shader multiplies the
  painted ground colour by twice this, so the map's palette and baked contact shadow survive.
- Normals are +Y up (OpenGL), the convention of the Poly Haven scans and of Bevy.
- ORM: red occlusion, green roughness, blue metalness.
- Depth maps: 0 at the wall's face, 1 at DEPTH_RANGE behind it.
"""

import os
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
VENDOR = os.path.join(ROOT, "assets", "models", "samples", "delhi_street", "textures", "vendor")
OUT = os.path.join(ROOT, "build", "ultra_src")

# Metres; crate::buildings keeps the same numbers.
BAY, STOREY = 3.2, 3.4
WALL_TILE = 12.0
# Where the ring's ledges stand (crate::shared::map::delhi::ring: g 5.5, u 4.6, less the slab and
# the cornice), for the rain streaks that start under them.
LEDGES = (5.05, 9.75)
# How deep the darkest value of a depth map is, in metres.
DEPTH_RANGE = 0.16


def to_linear(a):
    return np.where(a <= 0.04045, a / 12.92, ((a + 0.055) / 1.055) ** 2.4)


def to_srgb(a):
    a = np.clip(a, 0.0, 1.0)
    return np.where(a <= 0.0031308, a * 12.92, 1.055 * a ** (1.0 / 2.4) - 0.055)


def read(name, size=None):
    im = Image.open(os.path.join(VENDOR, name)).convert("RGB")
    if size and size != im.size[0]:
        im = im.resize((size, size), Image.LANCZOS)
    return np.asarray(im, dtype=np.float32) / 255.0


def read_tiled(name, repeats, size):
    """A scan from build/look_src (Poly Haven, CC0, kept out of git), `repeats` times each way in
    a square of `size` px, still tiling."""
    im = Image.open(os.path.join(ROOT, "build", "look_src", name)).convert("RGB")
    w, h = im.size
    sheet = Image.new("RGB", (w * repeats, h * repeats))
    for x in range(repeats):
        for y in range(repeats):
            sheet.paste(im, (x * w, y * h))
    return np.asarray(sheet.resize((size, size), Image.LANCZOS), dtype=np.float32) / 255.0


def save(name, rgb):
    os.makedirs(OUT, exist_ok=True)
    a = np.clip(rgb, 0.0, 1.0)
    if a.ndim == 2:
        a = np.stack([a] * 3, -1)
    Image.fromarray((a * 255.0 + 0.5).astype(np.uint8), "RGB").save(os.path.join(OUT, name + ".png"))
    print(f"[surfaces] {name} {a.shape[1]}x{a.shape[0]}")


def periodic_noise(h, w, cells, seed, power=2.0):
    """Tileable noise in 0..1: white noise shaped by FFT, about `cells` blobs across, falling off
    above that by `power`. Evened out so thresholds mean a share of the area."""
    r = np.random.default_rng(seed)
    n = r.normal(size=(h, w))
    fy = np.fft.fftfreq(h)[:, None] * h
    fx = np.fft.rfftfreq(w)[None, :] * w
    f = np.sqrt(fx * fx + fy * fy)
    gain = 1.0 / (1.0 + (f / max(cells, 1e-3)) ** power)
    gain[0, 0] = 0.0
    out = np.fft.irfft2(np.fft.rfft2(n) * gain, s=(h, w))
    order = out.ravel().argsort().argsort().reshape(h, w)
    return (order / (h * w - 1)).astype(np.float32)


def smooth(lo, hi, x):
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def repeat(tile, h, w):
    ty, tx = tile.shape[:2]
    return np.tile(tile, (h // ty, w // tx, 1))


def turned(tile, is_normal):
    """A square tile a quarter turn anticlockwise, its normals turned with it."""
    t = np.rot90(tile)
    if not is_normal:
        return t
    x, y = t[..., 0] * 2.0 - 1.0, t[..., 1] * 2.0 - 1.0
    return np.stack([-y * 0.5 + 0.5, x * 0.5 + 0.5, t[..., 2]], -1)


def scatter(maps, tile_px, h, w, seed, layers=4, cells=7, turn=False, avoid=None):
    """Several copies of a tile, slid to random places and melted together along soft noise, so a
    surface many times the scan's size doesn't show the scan's blemishes in a grid. Everything
    stays periodic. `maps` is a list of (array, is_normal); returns them composited. `turn` gives
    every other copy a quarter turn (for scans with a direction, like cracks); `avoid` is a tile
    of 0..1, the blemishes to keep mostly out of the result."""
    r = np.random.default_rng(seed)
    masks = np.stack([periodic_noise(h, w, cells, seed + 17 * k) for k in range(layers)])
    shifts = [(int(r.integers(tile_px)), int(r.integers(tile_px))) for _ in range(layers)]
    place = lambda tile, k, normal=False: np.roll(repeat(turned(tile, normal) if turn and k % 2 else tile, h, w), shifts[k], (0, 1))
    # A hard-ish vote, softened: most pixels come from one copy, so detail isn't averaged away.
    votes = masks * 14.0
    if avoid is not None:
        votes = votes - np.stack([place(avoid, k)[..., 0] for k in range(layers)]) * 9.0
    weights = np.exp(votes)
    weights /= weights.sum(0, keepdims=True)
    out = []
    for tile, is_normal in maps:
        acc = np.zeros((h, w, tile.shape[2]), np.float32)
        for k in range(layers):
            acc += place(tile, k, is_normal) * weights[k][..., None]
        if is_normal:
            v = acc * 2.0 - 1.0
            v /= np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-5)
            acc = v * 0.5 + 0.5
        out.append(acc)
    return out


def luminance(a):
    return a[..., 0] * 0.2126 + a[..., 1] * 0.7152 + a[..., 2] * 0.0722


def normal_from_height(hm, metres_per_px, strength=1.0):
    """A height map in metres to a tangent-space normal (0..1), +Y up, wrapping at the edges."""
    dx = (np.roll(hm, -1, 1) - np.roll(hm, 1, 1)) / (2.0 * metres_per_px)
    dy = (np.roll(hm, -1, 0) - np.roll(hm, 1, 0)) / (2.0 * metres_per_px)
    v = np.stack([-dx * strength, dy * strength, np.ones_like(dx)], -1)
    v /= np.linalg.norm(v, axis=-1, keepdims=True)
    return v.astype(np.float32) * 0.5 + 0.5


def merge_normals(a, b):
    """Adds the detail of one normal map to another (slopes add)."""
    na, nb = a * 2.0 - 1.0, b * 2.0 - 1.0
    v = np.stack([na[..., 0] + nb[..., 0], na[..., 1] + nb[..., 1], na[..., 2] * nb[..., 2]], -1)
    v /= np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-5)
    return v * 0.5 + 0.5


def as_ratio(linear, grey=0.0):
    """Linear colour to the ground maps' form: ratio to its mean at 0.5. `grey` drains its hue."""
    mean = linear.reshape(-1, 3).mean(0)
    ratio = linear / mean
    if grey:
        l = luminance(ratio)[..., None]
        ratio = ratio * (1.0 - grey) + l * grey
    return ratio * 0.5


def wall_plaster(tile_px, h, w, seed):
    """Scanned painted plaster, whitened: the scan's mottling and trowel marks at a lime wash's
    brightness. Returns linear albedo (mean of the old plaster's), normal, roughness, occlusion."""
    diff = to_linear(read("painted_plaster_wall_diff_1k.png", tile_px))
    nor = read("painted_plaster_wall_nor_gl_1k.png", tile_px)
    arm = read("painted_plaster_wall_arm_1k.png", tile_px)
    diff, nor, arm = scatter([(diff, False), (nor, True), (arm, False)], tile_px, h, w, seed)
    grain = luminance(diff)
    ratio = (grain / grain.mean()) ** 0.55
    blotch = periodic_noise(h, w, 5, seed + 3)
    tone = 0.78 * ratio * (0.95 + 0.10 * blotch)
    # A slightly warm white, cooler where the wash is thin.
    albedo = np.stack([tone * 1.02, tone * 1.0, tone * 0.965], -1)
    return albedo, nor, arm[..., 1], arm[..., 0]


def plaster():
    """The ring's white walls and columns, one 12 m tile: row 0 at street level."""
    n = 2048
    albedo, nor, rough, occ = wall_plaster(512, n, n, 41)
    y = (np.arange(n, dtype=np.float32) / n * WALL_TILE)[:, None]

    # Rising damp and splashed street dirt: a broken tide line up to a metre or so.
    tide = 0.30 + 0.85 * periodic_noise(1, n, 14, 42)[0][None, :] + 0.25 * periodic_noise(n, n, 40, 43)
    damp = np.clip(1.0 - y / tide, 0.0, 1.0) ** 1.4
    # The tile wraps at 12 m: lead back into the damp over the last stretch, above any wall.
    damp = np.maximum(damp, smooth(11.5, 12.0, y) * damp[0])
    speckle = 0.75 + 0.5 * periodic_noise(n, n, 120, 44)
    k = np.clip(damp * speckle, 0.0, 1.0)[..., None]
    albedo = albedo * (1.0 - k * 0.42 * np.array([0.8, 0.9, 1.05], np.float32))

    # Rain marks: streaks running down from under each ledge (down the wall is down the rows).
    fringe = periodic_noise(1, n, 90, 45)[0]
    length = 0.5 + 2.6 * periodic_noise(1, n, 30, 46)[0] ** 2
    wash = np.zeros((n, n), np.float32)
    for ledge in LEDGES:
        below = np.maximum(ledge - y, 0.0)
        wash = np.maximum(wash, np.exp(-below / length[None, :]) * smooth(0.0, 0.25, below) * (y < ledge) * smooth(0.5, 0.85, fringe)[None, :])
    # Water doesn't run plumb: each streak wanders, and blurs as it thins.
    wander = (periodic_noise(n, n, 9, 48) - 0.5) * 34.0 + (periodic_noise(n, n, 40, 49) - 0.5) * 7.0
    cols = (np.arange(n)[None, :] + wander.astype(np.int32)) % n
    wash = np.take_along_axis(wash, cols, 1)
    wash = ndimage.gaussian_filter(wash, (3.0, 2.0), mode="wrap") * (0.65 + 0.35 * periodic_noise(n, n, 60, 47))
    albedo = albedo * (1.0 - wash[..., None] * 0.20 * np.array([0.85, 0.92, 1.05], np.float32))
    rough = np.clip(rough + 0.04 * k[..., 0] - 0.06 * wash, 0.0, 1.0)

    save("plaster_basecolor", to_srgb(albedo))
    save("plaster_normal", nor)
    save("plaster_orm", np.stack([occ, rough, np.zeros_like(rough)], -1))


def marble():
    """The verandah floors: pale marble slabs, polished by feet, 4.8 m a tile, 1.2 by 0.6 m slabs
    in running bond. Procedural veining over the scanned floor's gentle waviness."""
    n = 1024
    px_m = n / 4.8
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    row = (yy / (0.6 * px_m)).astype(np.int32)
    xs = xx + (row % 2) * 0.6 * px_m
    col = (xs / (1.2 * px_m)).astype(np.int32)
    col %= 4
    seam = np.minimum.reduce([(xs % (1.2 * px_m)), (1.2 * px_m) - (xs % (1.2 * px_m)), (yy % (0.6 * px_m)), (0.6 * px_m) - (yy % (0.6 * px_m))])
    joint = 1.0 - smooth(0.6, 1.6, seam)
    slab = (row * 8 + col).astype(np.int32)

    # Veins: ridges of warped noise, each slab cut from a different part of the block.
    warp_a, warp_b = periodic_noise(n, n, 6, 51), periodic_noise(n, n, 6, 52)
    field = periodic_noise(n, n, 4, 53)
    r = np.random.default_rng(54)
    offsets = r.integers(0, n, size=(64, 2))
    py = (yy + offsets[slab % 64, 0] + (warp_a - 0.5) * 90).astype(np.int32) % n
    px = (xx + offsets[slab % 64, 1] + (warp_b - 0.5) * 90).astype(np.int32) % n
    ridge = 1.0 - np.abs(field[py, px] - 0.5) * 2.0
    vein = smooth(0.93, 0.995, ridge)
    cloud = periodic_noise(n, n, 10, 55)[py, px]
    tone = 0.74 + 0.08 * cloud + (r.random(64)[slab % 64] - 0.5) * 0.05
    base = tone - vein * 0.22
    albedo = np.stack([base * 1.0, base * 0.985, base * 0.955], -1)
    albedo = albedo * (1.0 - joint[..., None] * 0.28)

    height = -joint * 0.002
    nor = normal_from_height(height, 1.0 / px_m)
    wavy = read("concrete_floor_nor_gl_1k.png", n)
    wavy = (wavy - 0.5) * 0.12 + 0.5
    wavy[..., 2] = 1.0
    nor = merge_normals(nor, wavy)

    wear = periodic_noise(n, n, 5, 56)
    rough = 0.24 + 0.16 * wear + joint * 0.5
    occ = 1.0 - joint * 0.5
    save("marble_basecolor", to_srgb(albedo))
    save("marble_normal", nor)
    save("marble_orm", np.stack([occ, rough, np.zeros_like(rough)], -1))


def asphalt():
    """Roads: the asphalt scan, 12 m a tile, its cracks healed away."""
    n = 2048
    diff = to_linear(read("asphalt_02_diff_1k.png", 512))
    nor = read("asphalt_02_nor_gl_1k.png", 512)
    arm = read("asphalt_02_arm_1k.png", 512)
    lum = luminance(diff)
    cracks = ndimage.gaussian_filter(smooth(0.12, 0.4, 1.0 - lum / np.median(lum)), 1.0, mode="wrap")
    cracks = ndimage.grey_dilation(cracks, size=9, mode="wrap")
    # Their pattern is regular enough to read as laid slabs, so heal them all with clean scan from
    # elsewhere in the tile.
    r = np.random.default_rng(62)
    left = cracks
    maps = [diff, nor, arm]
    healed = [m.copy() for m in maps]
    for _ in range(24):
        shift = (int(r.integers(512)), int(r.integers(512)))
        take = smooth(0.15, 0.6, left) * (1.0 - smooth(0.05, 0.3, np.roll(cracks, shift, (0, 1))))
        for h, m in zip(healed, maps):
            h += (np.roll(m, shift, (0, 1)) - h) * take[..., None]
        left = left * (1.0 - take)
    diff, nor, arm = healed
    diff, nor, arm = scatter([(diff, False), (nor, True), (arm, False)], 512, n, n, 61, turn=True)
    save("asphalt_basecolor", to_srgb(as_ratio(diff, grey=0.6)))
    save("asphalt_normal", nor)
    save("asphalt_orm", np.stack([arm[..., 0], arm[..., 1], np.zeros((n, n), np.float32)], -1))


def pavers():
    """Pavements and plazas: 60 by 30 cm slabs in running bond, 6 m a tile, every slab its own
    shade and set a little off level, with chipped corners and sand in the joints. The grain
    comes from the concrete floor scan."""
    n = 2048
    px_m = n / 6.0
    w, h = 0.6 * px_m, 0.3 * px_m
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    # Edges wobble a few millimetres, as hand-laid slabs do.
    wob_a, wob_b = periodic_noise(n, n, 24, 71) - 0.5, periodic_noise(n, n, 24, 72) - 0.5
    ys, xs0 = yy + wob_a * 6.0, xx + wob_b * 6.0
    row = np.floor(ys / h).astype(np.int32)
    xs = xs0 + (row % 2) * w * 0.5
    col = np.floor(xs / w).astype(np.int32)
    rows, cols = int(round(n / h)), int(round(n / w))
    slab = (row % rows) * cols + (col % cols)
    seam = np.minimum.reduce([xs % w, w - (xs % w), ys % h, h - (ys % h)])
    chips = periodic_noise(n, n, 60, 73)
    gap = 2.4 + 5.0 * smooth(0.75, 1.0, chips)
    joint = 1.0 - smooth(gap - 1.2, gap + 0.8, seam)

    r = np.random.default_rng(74)
    shade = r.random(rows * cols + cols)
    tilt = r.random(rows * cols + cols)
    grain = luminance(to_linear(read("concrete_floor_diff_1k.png", 1024)))
    grain = repeat(grain[..., None], n, n)[..., 0]
    grain = (grain / grain.mean()) ** 0.7
    tone = (0.9 + 0.2 * shade[slab]) * grain * (0.92 + 0.16 * periodic_noise(n, n, 6, 75))
    sand = 0.55 + 0.1 * chips
    tone = tone * (1.0 - joint) + sand * joint
    linear = np.stack([tone * 1.01, tone, tone * 0.97], -1)
    height = (tilt[slab] - 0.5) * 0.004 * (1.0 - joint) - joint * 0.012
    nor = normal_from_height(ndimage.gaussian_filter(height, 0.7, mode="wrap"), 1.0 / px_m)
    fine = repeat(read("concrete_floor_nor_gl_1k.png", 1024), n, n)
    fine = (fine - 0.5) * 0.5 + 0.5
    fine[..., 2] = 1.0
    nor = merge_normals(nor, fine)
    occ = 1.0 - joint * 0.6
    rough = np.clip(0.78 + 0.14 * periodic_noise(n, n, 18, 76) + joint * 0.12, 0.0, 1.0)
    save("pavers_basecolor", to_srgb(as_ratio(linear)))
    save("pavers_normal", nor)
    save("pavers_orm", np.stack([occ, rough, np.zeros_like(rough)], -1))


class Painter:
    """A facade being laid up in layers: where a mask is set, a layer replaces what's under it.
    Heights are metres from the wall's face; depth is what parallax sees (inward only), so things
    that stand proud, like an AC unit, are shaped by the normal map alone."""

    def __init__(self, albedo, rough, occ, nor):
        self.albedo, self.rough, self.occ, self.nor = albedo, rough, occ, nor
        h, w = rough.shape
        self.height = np.zeros((h, w), np.float32)
        self.depth = np.zeros((h, w), np.float32)
        self.flat = np.zeros((h, w), np.float32)  # 1 where the plaster's own bumps don't show

    def put(self, mask, colour=None, rough=None, occ=None, height=None, depth=None):
        m = mask.astype(bool)
        if colour is not None:
            c = np.asarray(colour, np.float32)
            self.albedo[m] = c[m] if c.ndim == 3 else c
        if rough is not None:
            self.rough[m] = rough[m] if isinstance(rough, np.ndarray) else rough
        if occ is not None:
            self.occ[m] = occ[m] if isinstance(occ, np.ndarray) else occ
        if height is not None:
            self.height[m] = height[m] if isinstance(height, np.ndarray) else height
        if depth is not None:
            self.depth[m] = depth[m] if isinstance(depth, np.ndarray) else depth
        self.flat[m] = 1.0

    def finish(self, name, metres_per_px):
        hm = ndimage.gaussian_filter(self.height, 0.8, mode="wrap")
        nor = normal_from_height(hm, metres_per_px)
        plaster_part = self.nor * (1.0 - self.flat[..., None]) + np.array([0.5, 0.5, 1.0], np.float32) * self.flat[..., None]
        nor = merge_normals(nor, plaster_part)
        save(f"{name}_basecolor", to_srgb(self.albedo))
        save(f"{name}_normal", nor)
        save(f"{name}_orm", np.stack([self.occ, self.rough, np.zeros_like(self.rough)], -1))
        depth = ndimage.gaussian_filter(self.depth, 0.6, mode="wrap")
        save(f"{name}_depth", np.clip(depth / DEPTH_RANGE, 0.0, 1.0))


def linear_rgb(*c):
    return to_linear(np.array(c, np.float32))


def inner_edge(mask, reach):
    """How close to the rim of a mask each of its pixels is: 1 at the rim, 0 `reach` px inside."""
    d = ndimage.distance_transform_edt(mask)
    return np.clip(1.0 - d / reach, 0.0, 1.0) * mask


def grime(size):
    """Soft dark smudges from the dirty_concrete scan, 0 clean to 1 at the worst, tiling: its grit
    and straw blurred out of it, and only what is darker than the concrete kept."""
    lum = luminance(to_linear(read_tiled("dirty_concrete_diff_2k.jpg", 1, size)))
    soft = ndimage.gaussian_filter(lum, 14.0, mode="wrap")
    return smooth(0.0, 0.34, 1.0 - soft / np.percentile(soft, 80))


def brick_wall(size):
    """Bare brick, the brick_wall_02 scan three times a tile: about 2.2 m each, as it stands."""
    diff = to_linear(read_tiled("brick_wall_02_diff_2k.jpg", 3, size))
    nor = read_tiled("brick_wall_02_nor_gl_2k.jpg", 3, size)
    arm = read_tiled("brick_wall_02_arm_2k.jpg", 3, size)
    return diff, nor, arm[..., 1], arm[..., 0]


def facade(name="facade", walls=None):
    """The upper storeys: crate::buildings::facade_image's two bays by two storeys, each window
    different, in real materials. Near-white plaster, stained: the mesh's vertex colour tints it
    per building. `walls` is another wall to build the same bays in."""
    n = 1024
    N = 2 * n
    metres = (2 * BAY) / N
    albedo, nor, rough, occ = walls(N) if walls else wall_plaster(n, N, N, 91)
    if not walls:
        # Weather over the whole wall, mostly hanging down from the storey above.
        stain = grime(N)
        hang = ndimage.gaussian_filter(stain * (0.6 + 0.8 * periodic_noise(N, N, 34, 99)), (46.0, 2.5), mode="wrap")
        albedo = albedo * (1.0 - np.maximum(stain * 0.09, hang * 0.26)[..., None] * np.array([0.92, 1.0, 1.14], np.float32))
    p = Painter(albedo, rough, occ, nor)
    yy, xx = np.mgrid[0:N, 0:N].astype(np.float32)
    cx, cy = (xx // n).astype(np.int32), (yy // n).astype(np.int32)
    u, v = (xx % n) / n, (yy % n) / n
    kind = np.array([[0, 2], [3, 1]])[cy, cx]

    def box(u0, u1, v0, v1):
        return (u >= u0) & (u < u1) & (v >= v0) & (v < v1)

    # Dirt on the plaster first: under each sill, and a gentle gradient under the string course.
    drips = smooth(0.45, 0.9, periodic_noise(1, N, 60, 92))
    reach = 0.08 + 0.17 * periodic_noise(1, N, 18, 96)
    sideways = smooth(0.20, 0.27, u) * (1.0 - smooth(0.73, 0.80, u))
    under = np.exp(-np.maximum(v - 0.78, 0.0) / reach) * (v > 0.78) * sideways * drips
    under = ndimage.gaussian_filter(under, (6.0, 2.5), mode="wrap")
    p.albedo *= 1.0 - (under * 0.26)[..., None]

    deep = kind == 3
    frame = box(0.25, 0.75, 0.13, 0.73) | (deep & box(0.25, 0.75, 0.13, 0.82))
    sill = box(0.22, 0.78, 0.73, 0.78) & ~deep
    band = v > 0.95
    opening = box(0.28, 0.72, 0.16, 0.70) | (deep & box(0.28, 0.72, 0.16, 0.80))

    off_white = linear_rgb(0.82, 0.80, 0.77)
    p.put(frame, off_white, rough=0.45, occ=0.9, height=0.02)
    p.put(sill, linear_rgb(0.80, 0.78, 0.75), rough=0.6, height=0.03)
    p.put(band, linear_rgb(0.77, 0.75, 0.71) * (0.92 + 0.16 * periodic_noise(N, N, 30, 93))[..., None], rough=0.75, height=0.012)

    # The reveal: the wall's thickness round the glass, a dark band that catches no sky.
    reveal = inner_edge(opening, 18)
    depth_px = reveal * 0.04 + 0.12 * (1.0 - reveal)
    pane = np.stack([0.030 + 0.03 * (1.0 - v), 0.036 + 0.035 * (1.0 - v), 0.046 + 0.05 * (1.0 - v)], -1).astype(np.float32)
    # A curtain drawn across half of the plain window, seen through the glass.
    folds = 0.82 + 0.18 * np.sin(u * 150.0 + 3.0 * periodic_noise(N, N, 4, 94)) ** 2
    curtain = (kind == 0) & (u > 0.5)
    curtain_col = linear_rgb(0.55, 0.32, 0.26) * folds[..., None] * 0.7
    pane = np.where(curtain[..., None], curtain_col, pane)
    p.put(opening, pane, rough=0.06, occ=1.0 - reveal * 0.55, height=-depth_px, depth=depth_px)
    p.albedo = np.where((reveal > 0.35)[..., None], p.albedo * 0.35 + off_white * 0.25, p.albedo)
    p.rough = np.where(reveal > 0.35, 0.6, p.rough)
    # Glazing bars: one upright, one across, in the frame's paint.
    bars = opening & (reveal < 0.3) & ((np.abs(u - 0.5) < 0.011) | (np.abs(v - 0.34) < 0.011))
    p.put(bars, off_white * 0.9, rough=0.45, occ=0.85, height=-0.07, depth=0.07)

    # Shutters either side of one window, painted green, louvred, the paint worn at the slats.
    shutter = (kind == 1) & (box(0.10, 0.25, 0.13, 0.73) | box(0.75, 0.90, 0.13, 0.73))
    slat = (yy / 13.0) % 1.0
    wear = periodic_noise(N, N, 50, 95)
    green = linear_rgb(0.30, 0.50, 0.38) * (0.72 + 0.28 * slat)[..., None]
    chips = periodic_noise(N, N, 160, 97) * (0.82 + 0.18 * wear)
    green = np.where((chips > 0.93)[..., None], linear_rgb(0.42, 0.36, 0.28), green)
    p.put(shutter, green, rough=0.5, occ=0.72 + 0.28 * slat, height=0.015 * slat + 0.01)

    # An AC unit under one window: a pale case, a grille, and its drip down the wall.
    ac = (kind == 2) & box(0.34, 0.66, 0.79, 0.94)
    grille = ac & box(0.38, 0.62, 0.81, 0.92) & ((xx % 7.0) < 3.0)
    p.put(ac, linear_rgb(0.76, 0.76, 0.74) * (0.92 + 0.08 * wear)[..., None], rough=0.5, height=0.03)
    p.put(grille, linear_rgb(0.40, 0.41, 0.42), occ=0.6, height=0.02)
    drip = (kind == 2) & box(0.48, 0.53, 0.94, 1.0) & ~band
    p.albedo = np.where(drip[..., None], p.albedo * 0.72, p.albedo)

    # The balcony door's rail: wrought iron across its lower half.
    rail = (kind == 3) & box(0.20, 0.80, 0.52, 0.80) & (((xx % 24.0) < 7.0) | box(0.20, 0.80, 0.52, 0.545) | box(0.20, 0.80, 0.755, 0.78))
    p.put(rail, linear_rgb(0.18, 0.17, 0.17), rough=0.55, occ=0.85, height=0.02, depth=0.0)

    p.finish(name, metres)


def shop():
    """The ground storey: crate::buildings::shop_image's two bays, a shutter and a glazed shop,
    under the beam the signboards hang on. 2:1, near-white plaster like the facade."""
    n = 1024
    N, H = 2 * n, n
    metres = (2 * BAY) / N
    albedo, nor, rough, occ = wall_plaster(n, N, N, 101)
    albedo, nor, rough, occ = albedo[:H], nor[:H], rough[:H], occ[:H]
    p = Painter(albedo, rough, occ, nor)
    yy, xx = np.mgrid[0:H, 0:N].astype(np.float32)
    bay = (xx // n).astype(np.int32)
    u, v = (xx % n) / n, yy / H

    def box(u0, u1, v0, v1):
        return (u >= u0) & (u < u1) & (v >= v0) & (v < v1)

    # Street grime rising up the piers.
    dirt = smooth(0.8, 1.0, v) * (0.7 + 0.3 * periodic_noise(H, N, 40, 102))
    p.albedo *= 1.0 - dirt[..., None] * 0.34
    p.albedo *= 0.92

    pier = ~box(0.06, 0.94, 0.20, 2.0)

    # The shutter: ribbed steel, painted, rustier toward its foot, a lock bar along the bottom.
    bay0 = (bay == 0) & ~pier
    rib = (yy % 24.0) / 24.0
    rib_h = 0.012 * np.minimum(rib, 1.0 - rib) * 2.0
    rust_n = periodic_noise(H, N, 28, 103)
    grey = 0.58 * (0.94 + 0.06 * rust_n)
    steel = np.stack([grey, grey, grey * 1.01], -1)
    steel = to_linear(steel)
    t = np.clip((v - 0.2) / 0.8, 0.0, 1.0)
    runs = periodic_noise(1, N, 110, 105) * (0.7 + 0.3 * rust_n)
    rust = smooth(0.55, 1.0, runs + t * t * 0.55) * smooth(0.25, 1.0, t) * 0.5
    rust = ndimage.gaussian_filter(rust, (5.0, 1.5), mode="nearest")
    steel = steel * (1.0 - rust[..., None]) + linear_rgb(0.42, 0.25, 0.15) * rust[..., None]
    p.put(bay0, steel, rough=0.45 + 0.35 * rust, occ=0.75 + 0.25 * np.abs(rib - 0.5) * 2.0, height=rib_h - 0.03, depth=0.03)
    bar = bay0 & box(0.06, 0.94, 0.93, 0.965)
    p.put(bar, linear_rgb(0.30, 0.30, 0.31), rough=0.45, height=-0.01, depth=0.01)

    # The shop: a pane on a low riser, goods on shelves behind it, dark at the back.
    bay1 = (bay == 1) & ~pier
    riser = bay1 & box(0.06, 0.94, 0.88, 2.0)
    p.put(bay1, linear_rgb(0.20, 0.19, 0.18), rough=0.45, occ=0.8, height=-0.01, depth=0.01)
    p.put(riser, linear_rgb(0.62, 0.60, 0.56) * (0.9 + 0.1 * rust_n)[..., None], rough=0.7, height=-0.005, depth=0.005)
    pane = bay1 & box(0.09, 0.91, 0.23, 0.86)
    pv = (v - 0.23) / 0.63
    shelves = pv * 4.0
    row = np.floor(shelves)
    frac = shelves - row
    interior = np.broadcast_to(linear_rgb(0.05, 0.045, 0.04) * 1.0, (H, N, 3)).copy()
    r = np.random.default_rng(104)
    palette = to_linear(np.array([[0.75, 0.20, 0.18], [0.90, 0.70, 0.20], [0.20, 0.45, 0.70], [0.85, 0.85, 0.80], [0.25, 0.55, 0.30], [0.60, 0.35, 0.60], [0.80, 0.50, 0.30]], np.float32))
    # Stock: boxes of random widths and heights along each shelf.
    stock = np.zeros((H, N), np.int32) - 1
    top = np.ones((H, N), np.float32)
    for s in range(4):
        x = 0
        while x < N:
            w = int(r.integers(26, 90))
            stock[(row == s) & (xx >= x) & (xx < x + w - 4)] = int(r.integers(len(palette)))
            top[(row == s) & (xx >= x) & (xx < x + w - 4)] = float(r.uniform(0.25, 0.7))
            x += w
    goods = (stock >= 0) & (frac > top) & (frac < 0.92)
    interior = np.where(goods[..., None], palette[np.maximum(stock, 0)] * (0.35 + 0.3 * (1.0 - pv))[..., None], interior)
    board = frac >= 0.92
    interior = np.where(board[..., None], linear_rgb(0.35, 0.28, 0.22) * 0.6, interior)
    p.put(pane, interior, rough=0.06, occ=1.0, height=-0.03, depth=0.12)
    bars = pane & (np.abs(u - 0.5) < 0.012)
    p.put(bars, linear_rgb(0.20, 0.19, 0.18), rough=0.4, height=-0.01, depth=0.01)
    p.finish("shop", metres)


def water():
    """Ripples for the Ultra level's water, a mix of sizes down to a few centimetres. Tileable."""
    n = 1024
    r = np.random.default_rng(111)
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32) / n
    h = np.zeros((n, n), np.float32)
    for _ in range(90):
        f = r.integers(1, 42, size=2) * r.choice([-1, 1], size=2)
        mag = np.hypot(*f)
        h += np.sin(2.0 * np.pi * (f[0] * xx + f[1] * yy) + r.uniform(0, 6.283)) / mag ** 1.35
    h /= h.std()
    # About 16 m a tile; the slope sets how lively the surface looks.
    save("water_normal", normal_from_height(h * 0.012, 16.0 / n))


def brick():
    """The same bays in bare brick, for brick buildings."""
    facade("brick", brick_wall)


def main():
    jobs = {"plaster": plaster, "marble": marble, "asphalt": asphalt, "pavers": pavers, "facade": facade, "brick": brick, "shop": shop, "water": water}
    for name in sys.argv[1:] or jobs:
        jobs[name]()


if __name__ == "__main__":
    main()
