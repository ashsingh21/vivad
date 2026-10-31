"""Every sound in the game, synthesized from noise, filters, resonant modes and plucked strings:
no recordings, no third-party samples.

python3 tools/audio/sfx.py            build all into assets/audio/ (ogg via oggenc)
python3 tools/audio/sfx.py --report   also print level / length / brightness / loop-seam numbers

Mono, 44.1 kHz. One-shots peak at -1 dBFS (the game mixes by category); loops are made seamless
by cross-fading their tail into their head.
"""

import os
import subprocess
import sys
import tempfile
import wave

import numpy as np

SR = 44100
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "assets", "audio")
rng = np.random.default_rng(7)


# ------------------------------------------------------------------ DSP kit

def t_axis(dur):
    return np.arange(int(dur * SR)) / SR


def white(dur):
    return rng.standard_normal(int(dur * SR))


def spectral(dur, slope):
    """Noise with a power spectrum ~ 1/f^slope (0 white, 1 pink, 2 brown)."""
    n = int(dur * SR)
    X = np.fft.rfft(rng.standard_normal(n))
    f = np.fft.rfftfreq(n, 1 / SR)
    f[0] = f[1]
    y = np.fft.irfft(X / f ** (slope / 2), n)
    return y / (np.abs(y).max() + 1e-9)


def band(x, lo=None, hi=None, order=2):
    """Zero-phase band-limit by FFT with smooth (Butterworth-shaped) skirts."""
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR) + 1e-6
    g = np.ones_like(f)
    if lo:
        g *= 1 / np.sqrt(1 + (lo / f) ** (2 * order))
    if hi:
        g *= 1 / np.sqrt(1 + (f / hi) ** (2 * order))
    return np.fft.irfft(X * g, len(x))


def svf(x, cutoff, q=1.0, mode="bp"):
    """State-variable filter with a cutoff that can change per sample (array or scalar)."""
    cutoff = np.broadcast_to(np.asarray(cutoff, float), x.shape)
    f = 2 * np.sin(np.pi * np.minimum(cutoff, SR / 6) / SR)
    damp = 1.0 / q
    low = bp = 0.0
    out = np.empty_like(x)
    for i in range(len(x)):
        hp = x[i] - low - damp * bp
        bp += f[i] * hp
        low += f[i] * bp
        out[i] = bp if mode == "bp" else low if mode == "lp" else hp
    return out


def env(dur, attack, decay, curve=4.0):
    """Attack then exponential-ish decay to silence over the rest of `dur`."""
    t = t_axis(dur)
    a = np.clip(t / max(attack, 1e-4), 0, 1)
    d = np.exp(-curve * np.clip(t - attack, 0, None) / max(decay, 1e-4))
    return a * d


def adsr(dur, a, s_level, r):
    t = t_axis(dur)
    e = np.clip(t / a, 0, 1) * (s_level + (1 - s_level) * np.exp(-6 * t / max(dur, 1e-3)))
    rel = np.clip((dur - t) / r, 0, 1)
    return e * rel


def modes(dur, partials, t60s, amps, jitter=0.0):
    """Struck resonator: decaying sines (bells, wood, metal)."""
    t = t_axis(dur)
    y = np.zeros_like(t)
    for f, t60, a in zip(partials, t60s, amps):
        f = f * (1 + jitter * rng.uniform(-1, 1))
        y += a * np.sin(2 * np.pi * f * t + rng.uniform(0, 6.28)) * np.exp(-6.9 * t / t60)
    return y


def pluck(dur, f0, decay=0.996, bright=0.5, buzz=0.0):
    """Karplus-Strong string; `buzz` adds the jawari rattle of a tanpura bridge."""
    n = int(dur * SR)
    p = max(2, int(SR / f0))
    buf = rng.uniform(-1, 1, p)
    buf = band(np.tile(buf, 4), hi=2000 + 8000 * bright)[:p]
    y = np.empty(n)
    for i in range(n):
        s = buf[i % p]
        nxt = buf[(i + 1) % p]
        v = decay * (0.5 * (s + nxt))
        if buzz:
            v = v + buzz * (abs(v) ** 0.5 * np.sign(v) - v) * 0.15
        buf[i % p] = v
        y[i] = s
    return y


def sweep_sine(dur, f0, f1, curve=1.0):
    t = t_axis(dur)
    k = (t / dur) ** curve
    f = f0 + (f1 - f0) * k
    return np.sin(2 * np.pi * np.cumsum(f) / SR)


def at(total, *parts):
    """Mix (start_seconds, signal) parts into one buffer `total` seconds long."""
    y = np.zeros(int(total * SR))
    for start, s in parts:
        i = int(start * SR)
        n = min(len(s), len(y) - i)
        if n > 0:
            y[i:i + n] += s[:n]
    return y


def norm(x, peak_db=-1.0):
    return x / (np.abs(x).max() + 1e-9) * 10 ** (peak_db / 20)


def fade(x, a=0.0003, r=0.01):
    n = len(x)
    e = np.ones(n)
    na, nr = int(a * SR), int(r * SR)
    if na:
        e[:na] = np.linspace(0, 1, na)
    if nr:
        e[-nr:] = np.minimum(e[-nr:], np.linspace(1, 0, nr))
    return x * e


def loopable(x, xfade=1.0):
    """Cross-fade the last `xfade` seconds into the first so the loop point is seamless."""
    n = int(xfade * SR)
    head, body, tail = x[:n], x[n:-n] if n else x, x[-n:]
    w = np.linspace(0, 1, n)
    mixed = tail * np.cos(w * np.pi / 2) + head * np.sin(w * np.pi / 2)
    return np.concatenate([mixed, x[n:-n]]) if n else x


def pitch(x, ratio):
    """Resample (changes length) for variation."""
    idx = np.arange(0, len(x) - 1, ratio)
    return np.interp(idx, np.arange(len(x)), x)


# ------------------------------------------------------------------ sounds

def step(surface, k):
    r = 1 + rng.uniform(-0.08, 0.08)
    if surface == "dirt":
        body = band(spectral(0.12, 1.4), 120, 1400) * env(0.12, 0.004, 0.06)
        grit = band(white(0.12) * (rng.random(int(0.12 * SR)) < 0.01), 1500, 5000) * env(0.12, 0.002, 0.08) * 0.8
        thump = np.sin(2 * np.pi * 75 * t_axis(0.12)) * env(0.12, 0.002, 0.03) * 0.6
        y = body + grit + thump
    elif surface == "stone":
        click = band(white(0.1), 1800, 6000) * env(0.1, 0.0005, 0.008) * 0.8
        knock = band(white(0.1), 300, 1200) * env(0.1, 0.001, 0.035)
        scuff = band(white(0.1), 2500, 8000) * env(0.1, 0.02, 0.05) * 0.15
        y = click + knock + scuff
    elif surface == "wood":
        y = modes(0.18, [190, 430, 780, 1250], [0.12, 0.08, 0.05, 0.03], [1, 0.6, 0.35, 0.2], 0.08) * env(0.18, 0.001, 0.15)
        y += band(white(0.18), 1500, 5000) * env(0.18, 0.0005, 0.01) * 0.5
    elif surface == "grass":
        y = band(white(0.16), 1800, 7000) * env(0.16, 0.025, 0.09) * 0.7
        y += band(spectral(0.16, 1.5), 100, 600) * env(0.16, 0.003, 0.04) * 0.6
    else:  # water
        y = band(white(0.3), 700, 4000) * env(0.3, 0.005, 0.18)
        for _ in range(4):
            s = rng.uniform(0.02, 0.18)
            y += at(0.3, (s, sweep_sine(0.04, rng.uniform(350, 600), rng.uniform(800, 1400)) * env(0.04, 0.002, 0.03) * 0.4))
    return fade(norm(pitch(y, r)))


def sword_whoosh(k):
    d = 0.32
    t = t_axis(d)
    c = 350 + 1500 * np.sin(np.pi * np.clip(t / d * 1.15, 0, 1)) ** 2 * (1 + 0.15 * k)
    y = svf(white(d), c, q=2.2) * np.sin(np.pi * np.clip(t / d, 0, 1)) ** 1.5
    return fade(norm(y))


def sword_hit_flesh():
    y = band(spectral(0.25, 1.5), 60, 700) * env(0.25, 0.002, 0.09)
    y += np.sin(2 * np.pi * 95 * t_axis(0.25)) * env(0.25, 0.001, 0.07) * 0.8
    y += band(white(0.25), 2500, 7000) * env(0.25, 0.001, 0.04) * 0.25
    return fade(norm(y))


def sword_hit_stone():
    y = modes(0.7, [1180, 2630, 3910, 5420, 7100], [0.5, 0.35, 0.25, 0.18, 0.1], [1, 0.7, 0.5, 0.35, 0.2], 0.03)
    y += band(white(0.7), 2000, 9000) * env(0.7, 0.0005, 0.02)
    return fade(norm(y * env(0.7, 0.001, 0.6, 3)))


def bow_release(heavy=False):
    f0 = 82 if heavy else 120
    d = 0.7 if heavy else 0.45
    string = pluck(d, f0, decay=0.994, bright=0.8) * env(d, 0.001, d * 0.7)
    thump = np.sin(2 * np.pi * (55 if heavy else 80) * t_axis(d)) * env(d, 0.001, 0.05) * 0.7
    whoosh = svf(white(0.2), np.linspace(2500, 900, int(0.2 * SR)), q=1.5) * env(0.2, 0.01, 0.15) * 0.5
    return fade(norm(at(d, (0, string + thump), (0.01, whoosh))))


def bow_draw():
    d = 0.7
    grains = (rng.random(int(d * SR)) < 0.004).astype(float) * rng.uniform(0.3, 1, int(d * SR))
    y = svf(band(grains, 100, 3000) + 0.05 * white(d), np.linspace(280, 520, int(d * SR)), q=8)
    return fade(norm(y * adsr(d, 0.08, 0.8, 0.1)))


def arrow_impact(surface):
    if surface == "wood":
        y = modes(0.5, [210, 480, 900], [0.2, 0.12, 0.06], [1, 0.5, 0.3], 0.05) * env(0.5, 0.001, 0.2)
        # The shaft quivering.
        t = t_axis(0.5)
        y += np.sin(2 * np.pi * 140 * t + 3 * np.sin(2 * np.pi * 22 * t)) * env(0.5, 0.01, 0.35) * 0.35
    else:
        y = band(white(0.3), 1500, 7000) * env(0.3, 0.0005, 0.015)
        y += modes(0.3, [1900, 3300, 4700], [0.08, 0.06, 0.04], [0.6, 0.4, 0.3], 0.1)
        y += band(white(0.3), 200, 900) * env(0.3, 0.001, 0.04) * 0.6
    return fade(norm(y))


def gunshot(kind):
    """A country-made gun going off: a crack (the muzzle blast's broadband edge), a body (the
    low boom, longer for a bigger charge), a thump under it, and a tail of the shot rolling off
    the surroundings. Tamancha: sharp and short. Dunali: two barrels' worth of boom. Katta: one
    heavy, rough shot."""
    crack_hi, body_lp, body_t, thump_f, tail_t, d = {
        "tamancha": (9000, 1800, 0.07, 95, 0.45, 0.7),
        "dunali": (6000, 900, 0.22, 55, 1.1, 1.4),
        "katta": (7000, 1200, 0.14, 70, 0.8, 1.1),
    }[kind]
    crack = band(white(d), 1200, crack_hi) * env(d, 0.0002, 0.006)
    body = band(spectral(d, 1.2), 60, body_lp) * env(d, 0.001, body_t, 3.5)
    thump = np.sin(2 * np.pi * thump_f * t_axis(d) * (1 - 0.3 * t_axis(d))) * env(d, 0.0005, body_t * 0.6) * 0.8
    tail = band(spectral(d, 1.6), 80, 1400) * env(d, 0.02, tail_t, 2.5) * 0.35
    return fade(norm(crack * 0.45 + body * 1.3 + thump * 1.4 + at(d, (0.03, tail))))


def katta_click():
    """A katta's hammer falling on a dud: a dry metal click and the spring's ring."""
    d = 0.3
    click = band(white(d), 2000, 9000) * env(d, 0.0002, 0.004)
    ring = modes(d, [2900, 4300, 6100], [0.06, 0.04, 0.03], [1, 0.6, 0.4], 0.05) * 0.4
    return fade(norm(click + ring))


def katta_backfire():
    """A katta blowing back: a choked, ragged bang, then crackle and fizz."""
    d = 1.0
    bang = band(spectral(d, 1.4), 50, 1500) * env(d, 0.001, 0.09, 3.0)
    thump = np.sin(2 * np.pi * 60 * t_axis(d)) * env(d, 0.0005, 0.05)
    grains = (rng.random(int(d * SR)) < 0.003).astype(float) * rng.uniform(0.2, 1, int(d * SR))
    crackle = band(grains, 1500, 7000) * env(d, 0.05, 0.6, 2.0) * 0.5
    fizz = band(white(d), 3000, 9000) * env(d, 0.05, 0.5, 2.5) * 0.12
    return fade(norm(bang + thump + at(d, (0.04, crackle + fizz))))


def gulel_release():
    """The rubber of a slingshot snapping back, and the pebble's flick off the pouch."""
    d = 0.35
    snap = band(white(d), 1500, 6000) * env(d, 0.0003, 0.012)
    thwap = sweep_sine(d, 320, 110, 0.6) * env(d, 0.001, 0.05) * 0.8
    flick = svf(white(0.12), np.linspace(3000, 1200, int(0.12 * SR)), q=2.0) * env(0.12, 0.005, 0.08) * 0.3
    return fade(norm(at(d, (0, snap + thwap), (0.01, flick))))


def bullet_impact(surface):
    """A bullet striking: off stone a click and a whining ricochet; into wood or earth a thud."""
    d = 0.5
    if surface == "stone":
        click = band(white(d), 2500, 9000) * env(d, 0.0003, 0.008)
        t = t_axis(0.3)
        whine = sweep_sine(0.3, 3600, 1700, 0.8) * (1 + 0.4 * np.sin(2 * np.pi * 38 * t)) * env(0.3, 0.01, 0.25) * 0.25
        chips = band(white(d), 800, 4000) * env(d, 0.001, 0.05) * 0.4
        return fade(norm(at(d, (0, click + chips), (0.02, whine))))
    thud = band(spectral(d, 1.5), 60, 900) * env(d, 0.0005, 0.06)
    knock = modes(d, [230, 520], [0.12, 0.07], [1, 0.5], 0.05) * 0.5
    return fade(norm(thud + knock))


def hit_flesh():
    y = band(spectral(0.2, 1.8), 40, 400) * env(0.2, 0.001, 0.07)
    y += np.sin(2 * np.pi * 70 * t_axis(0.2)) * env(0.2, 0.001, 0.06)
    return fade(norm(y))


def hit_shield():
    d = 0.8
    y = modes(d, [820, 1330, 2210, 3170, 4380], [0.6, 0.5, 0.4, 0.3, 0.2], [1, 0.8, 0.6, 0.4, 0.3], 0.02)
    y += modes(d, [826, 1338], [0.6, 0.5], [0.6, 0.5])  # a beating shimmer
    y += band(white(d), 3000, 10000) * env(d, 0.001, 0.03) * 0.3
    return fade(norm(y * env(d, 0.001, 0.7, 3)))


def hitmarker(head=False):
    if head:
        y = modes(0.45, [1560, 3120, 4210], [0.4, 0.25, 0.15], [1, 0.4, 0.25])
        return fade(norm(y * env(0.45, 0.001, 0.4, 3)))
    y = np.sin(2 * np.pi * 1900 * t_axis(0.05)) * env(0.05, 0.0005, 0.02)
    return fade(norm(y))


def hurt():
    y = band(spectral(0.3, 2), 50, 500) * env(0.3, 0.003, 0.12)
    y += np.sin(2 * np.pi * np.linspace(110, 60, int(0.3 * SR)).cumsum() / SR) * env(0.3, 0.002, 0.1) * 0.8
    return fade(norm(y))


def jump():
    d = 0.18
    return fade(norm(band(white(d), 600, 2500) * np.sin(np.pi * t_axis(d) / d) ** 2))


def land():
    y = np.sin(2 * np.pi * 55 * t_axis(0.25)) * env(0.25, 0.002, 0.08)
    y += band(spectral(0.25, 1.5), 60, 500) * env(0.25, 0.002, 0.1)
    y += band(white(0.25), 1500, 5000) * env(0.25, 0.005, 0.05) * 0.2
    return fade(norm(y))


def glider_open():
    d = 0.45
    t = t_axis(d)
    flap = (0.5 + 0.5 * np.sin(2 * np.pi * 24 * t)) ** 2
    y = band(white(d), 300, 3000) * flap * env(d, 0.01, 0.3)
    y += svf(white(d), np.linspace(400, 1800, len(t)), q=1.2) * env(d, 0.05, 0.3) * 0.6
    return fade(norm(y))


def wind_loop(dur, cutoff, gust, whistle):
    t = t_axis(dur + 2)
    lfo = 0.6 + 0.4 * np.sin(2 * np.pi * 0.13 * t + 1) * np.sin(2 * np.pi * 0.071 * t)
    y = band(spectral(dur + 2, 1.2), 80, cutoff) * (1 - gust + gust * lfo)
    if whistle:
        y += svf(white(dur + 2), 900 + 400 * np.sin(2 * np.pi * 0.09 * t), q=25) * whistle * lfo
    return norm(loopable(y, 2.0), -3)


def tanpura_loop():
    """Four plucks, Pa-Sa-Sa-Sa (low), on a tanpura with a buzzing jawari bridge."""
    sa = 130.8
    beat = 1.2
    total = 4 * beat
    parts = []
    for i, f in enumerate((sa * 1.5 / 2, sa, sa, sa / 2)):
        s = pluck(total, f, decay=0.9993, bright=0.6, buzz=0.9)
        parts.append((i * beat, s * env(total, 0.004, total, 1.5)))
    # Pre-roll a cycle so the decays from the previous loop are already ringing at the start.
    y = at(total * 2, *parts, *[(p + total, s) for p, s in parts])[int(total * SR):]
    y = band(y, 50, 6000)
    return norm(y, -3)


def chest_open():
    d = 1.0
    creak = svf(band((rng.random(int(0.5 * SR)) < 0.006) * 1.0, 100, 2500) + 0.03 * white(0.5),
                np.linspace(350, 600, int(0.5 * SR)), q=10) * adsr(0.5, 0.05, 0.8, 0.1)
    latch = band(white(0.05), 1500, 6000) * env(0.05, 0.0005, 0.01)
    coins = np.zeros(int(d * SR))
    for _ in range(9):
        c = modes(0.25, [rng.uniform(3000, 6500), rng.uniform(7000, 9000)], [0.15, 0.1], [1, 0.5])
        coins = coins + at(d, (rng.uniform(0.35, 0.75), c * rng.uniform(0.2, 0.5)))
    return fade(norm(at(d, (0, latch), (0.05, creak)) + coins))


def bell(f0=620, dur=3.0, gain=1.0):
    """Temple ghanta: the inharmonic partials of a cast bell with a slow beat."""
    ratios = [0.56, 0.92, 1.19, 1.71, 2.0, 2.74, 3.0, 3.76, 4.07]
    t60 = [dur, dur * 0.8, dur * 0.7, dur * 0.5, dur * 0.45, dur * 0.35, dur * 0.3, dur * 0.25, dur * 0.2]
    amps = [0.6, 0.9, 1.0, 0.7, 0.6, 0.4, 0.35, 0.25, 0.2]
    y = modes(dur, [f0 * r for r in ratios], t60, amps) + modes(dur, [f0 * r * 1.003 for r in ratios[:3]], t60[:3], [0.3] * 3)
    y += band(white(dur), 2000, 8000) * env(dur, 0.0005, 0.01) * 0.4
    return y * gain


def pickup():
    grab = band(white(0.12), 800, 4000) * env(0.12, 0.01, 0.06) * 0.6
    y = at(0.3, (0, grab), (0.05, modes(0.25, [2600, 4100], [0.15, 0.1], [1, 0.6]) * 0.5))
    return fade(norm(y))


def drink():
    d = 2.0
    parts = []
    for k in range(3):
        g = sweep_sine(0.18, 260, 120, 0.6) * env(0.18, 0.01, 0.12)
        g += band(spectral(0.18, 1.5), 100, 700) * env(0.18, 0.01, 0.1) * 0.5
        parts.append((0.2 + k * 0.55 + rng.uniform(-0.05, 0.05), g))
    return fade(norm(at(d, *parts)))


def reload():
    d = 0.9
    parts = [(0.0, band(white(0.25), 1000, 5000) * env(0.25, 0.02, 0.15) * 0.5)]
    for _ in range(6):
        parts.append((rng.uniform(0.1, 0.7), modes(0.06, [rng.uniform(900, 1600), rng.uniform(2200, 3000)], [0.04, 0.03], [1, 0.5])))
    parts.append((0.75, band(white(0.08), 300, 1500) * env(0.08, 0.001, 0.03)))
    return fade(norm(at(d, *parts)))


def fire_ignite():
    d = 1.0
    y = band(spectral(d, 1.6), 40, 1200) * adsr(d, 0.12, 0.5, 0.5)
    y += np.sin(2 * np.pi * 50 * t_axis(d)) * env(d, 0.05, 0.4) * 0.6
    return fade(norm(y))


def fire_loop():
    dur = 6.0
    y = band(spectral(dur + 1, 1.8), 40, 900) * 0.6
    crackle = (rng.random(int((dur + 1) * SR)) < 0.0012) * rng.uniform(0.2, 1, int((dur + 1) * SR))
    y += band(crackle, 1500, 6000) * 2.5
    return norm(loopable(y, 1.0), -3)


def wind_gust():
    d = 1.4
    t = t_axis(d)
    c = 250 + 1800 * np.sin(np.pi * t / d) ** 2
    y = svf(white(d), c, q=1.4) * np.sin(np.pi * t / d) ** 1.2
    y += band(spectral(d, 2), 30, 300) * np.sin(np.pi * t / d) * 0.8
    return fade(norm(y))


def throw():
    d = 0.3
    return fade(norm(svf(white(d), np.linspace(600, 2200, int(d * SR)), q=1.5) * np.sin(np.pi * t_axis(d) / d)))


def shankha():
    """Conch: a brassy, slowly swelling tone that bends up into pitch, with breath noise."""
    d = 2.8
    t = t_axis(d)
    f = 290 * (1 + 0.04 * np.exp(-t / 0.25) * -1) * (1 + 0.006 * np.sin(2 * np.pi * 5.2 * t))
    ph = 2 * np.pi * np.cumsum(f) / SR
    y = sum(np.sin(k * ph) / k ** 0.9 for k in range(1, 14))
    y = band(y, 150, 4000)
    y = y + svf(y, 900, q=3) * 1.5 + svf(y, 1700, q=4) * 0.8
    y += band(white(d), 1000, 5000) * 0.03
    return fade(norm(y * adsr(d, 0.35, 0.85, 0.6)), 0.01, 0.3)


def rumble_loop():
    dur = 6.0
    t = t_axis(dur + 1)
    y = band(spectral(dur + 1, 2), 25, 160) * (0.75 + 0.25 * np.sin(2 * np.pi * 0.31 * t))
    y += band(white(dur + 1), 2000, 6000) * 0.008 * (0.5 + 0.5 * np.sin(2 * np.pi * 7 * t))
    return norm(loopable(y, 1.0), -3)


def koel(start, n):
    """The koel's rising 'ku-oo', repeated a little higher each time."""
    parts = []
    for i in range(n):
        f = 760 * (1.06 ** i)
        note = sweep_sine(0.32, f * 0.8, f * 1.12, 0.6) * adsr(0.32, 0.04, 0.8, 0.12)
        parts.append((start + i * 0.55, note))
    return parts


def sparrow(start):
    parts = []
    for i in range(rng.integers(3, 7)):
        f = rng.uniform(3500, 5200)
        parts.append((start + i * rng.uniform(0.07, 0.12), sweep_sine(0.05, f, f * rng.uniform(0.7, 1.3)) * env(0.05, 0.003, 0.04)))
    return parts


def crow(start):
    """A crow's harsh 'kaa': a buzzy tone through a nasal formant."""
    d = 0.35
    t = t_axis(d)
    f = 520 * (1 - 0.15 * t / d)
    ph = 2 * np.pi * np.cumsum(f) / SR
    y = sum(np.sin(k * ph) / k for k in range(1, 18))
    y = svf(y, 1400, q=4) + svf(y, 2400, q=5) * 0.6
    return [(start, y * adsr(d, 0.03, 0.8, 0.1) * 0.5)]


def ambience_day():
    dur = 40.0
    y = band(spectral(dur + 3, 1.3), 60, 1400) * 0.35
    t = t_axis(dur + 3)
    y *= 0.7 + 0.3 * np.sin(2 * np.pi * 0.05 * t) * np.sin(2 * np.pi * 0.023 * t + 1)
    birds = []
    birds += koel(4.0, 5) + koel(27.0, 4)
    for s in (1.5, 9.0, 13.5, 19.0, 23.0, 31.0, 35.5):
        birds += sparrow(s)
    birds += crow(16.0) + crow(16.5) + crow(37.2)
    b = at(dur + 3, *birds)
    # Birds sit back in the mix and a little dulled by distance.
    y += band(b, 300, 7000) * 0.18
    return norm(loopable(y, 3.0), -6)


def river_loop():
    dur = 20.0
    y = band(spectral(dur + 2, 1.0), 200, 3500) * 0.5
    parts = []
    for _ in range(260):
        f = rng.uniform(300, 1500)
        parts.append((rng.uniform(0, dur + 1.9), sweep_sine(0.03, f, f * 1.6) * env(0.03, 0.002, 0.02) * rng.uniform(0.05, 0.2)))
    y += at(dur + 2, *parts)
    return norm(loopable(y, 2.0), -6)


def vimana_loop():
    return tanpura_loop()


SOUNDS = {
    **{f"step_{s}_{k}": (lambda s=s, k=k: step(s, k)) for s in ("dirt", "stone", "wood", "grass", "water") for k in range(4)},
    **{f"sword_whoosh_{k}": (lambda k=k: sword_whoosh(k)) for k in range(3)},
    "sword_hit_flesh": sword_hit_flesh,
    "sword_hit_stone": sword_hit_stone,
    "bow_release": bow_release,
    "bow_release_heavy": lambda: bow_release(True),
    "bow_draw": bow_draw,
    "arrow_impact_wood": lambda: arrow_impact("wood"),
    "arrow_impact_stone": lambda: arrow_impact("stone"),
    "gun_tamancha": lambda: gunshot("tamancha"),
    "gun_dunali": lambda: gunshot("dunali"),
    "gun_katta": lambda: gunshot("katta"),
    "gulel_release": gulel_release,
    "katta_click": katta_click,
    "katta_backfire": katta_backfire,
    "bullet_impact_stone": lambda: bullet_impact("stone"),
    "bullet_impact_wood": lambda: bullet_impact("wood"),
    "hit_flesh": hit_flesh,
    "hit_shield": hit_shield,
    "hitmarker": hitmarker,
    "hitmarker_head": lambda: hitmarker(True),
    "hurt": hurt,
    "jump": jump,
    "land": land,
    "glider_open": glider_open,
    "glide_loop": lambda: wind_loop(8.0, 1100, 0.5, 0.15),
    "freefall_loop": lambda: wind_loop(8.0, 3000, 0.3, 0.05),
    "vimana_loop": vimana_loop,
    "chest_open": chest_open,
    "kalash_open": lambda: fade(norm(bell(620, 3.0))),
    "pickup": pickup,
    "drink": drink,
    "reload": reload,
    "fire_ignite": fire_ignite,
    "fire_loop": fire_loop,
    "wind_gust": wind_gust,
    "throw": throw,
    "zone_warn": shankha,
    "zone_loop": rumble_loop,
    "kill": lambda: fade(norm(bell(1240, 1.2))),
    "eliminated": lambda: fade(norm(bell(150, 5.0))),
    "victory": lambda: fade(norm(at(5.0, (0, bell(620, 3.0)), (0.6, bell(780, 3.0)), (1.2, bell(930, 3.5))))),
    "amb_day": ambience_day,
    "amb_river": river_loop,
}


def write_ogg(name, x):
    os.makedirs(OUT, exist_ok=True)
    pcm = (np.clip(x, -1, 1) * 32767).astype(np.int16)
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        tmp = f.name
    with wave.open(tmp, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    subprocess.run(["oggenc", "-Q", "-q", "5", "-o", os.path.join(OUT, f"{name}.ogg"), tmp], check=True)
    os.remove(tmp)


def report(name, x):
    db = lambda v: 20 * np.log10(max(v, 1e-9))
    X = np.abs(np.fft.rfft(x))
    f = np.fft.rfftfreq(len(x), 1 / SR)
    centroid = (X * f).sum() / (X.sum() + 1e-9)
    # Loop seam: the jump from last sample to first against the sound's typical sample step
    # (about 1 = inaudible, much more = a click).
    loop = name.endswith("loop") or name.startswith("amb")
    seam = abs(x[0] - x[-1]) / (np.median(np.abs(np.diff(x))) + 1e-9) if loop else float("nan")
    print(f"{name:<22} {len(x) / SR:6.2f}s  peak {db(np.abs(x).max()):6.1f}  rms {db(np.sqrt((x ** 2).mean())):6.1f} dBFS"
          f"  centroid {centroid:6.0f} Hz  clip {(np.abs(x) >= 0.999).sum():3d}  seam {seam:4.1f}x")


def main():
    only = [a for a in sys.argv[1:] if not a.startswith("--")]
    for name, make in SOUNDS.items():
        if only and not any(o in name for o in only):
            continue
        x = make()
        if "--report" in sys.argv:
            report(name, x)
        write_ogg(name, x)
    print(f"[sfx] {len(SOUNDS)} sounds -> {OUT}")


if __name__ == "__main__":
    main()
