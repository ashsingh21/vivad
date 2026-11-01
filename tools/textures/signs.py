"""Shop signboards for the city maps, one atlas: 32 made-up Delhi shops in truck-art style, each a
4:1 board in one of the festival colours (crate::buildings ACCENTS, same order: board k takes
ACCENTS[k % 8]), a contrasting border, a little motif each end, the name in bold Latin letters and,
on most, a line in Devanagari. No real brands or businesses.

Fonts: Noto Sans Bold and Noto Sans Devanagari Bold (SIL Open Font License; docs/references.md).

python3 tools/textures/signs.py   -> assets/textures/panga/signs.png (2048 x 1024, 4 x 8 boards)
"""

import os

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "assets", "textures", "panga", "signs.png")
LATIN = "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf"
DEVA = "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Bold.ttf"
COLS, ROWS, W, H = 4, 8, 512, 128

# crate::buildings::ACCENTS: marigold, rani pink, peacock blue, parrot green, sindoor red, haldi
# yellow, jamun purple, white.
ACCENTS = [(0.98, 0.58, 0.06), (0.90, 0.12, 0.46), (0.04, 0.46, 0.62), (0.18, 0.66, 0.24),
           (0.86, 0.12, 0.10), (0.99, 0.82, 0.10), (0.46, 0.16, 0.58), (0.97, 0.95, 0.90)]

SHOPS = [
    ("SHARMA SWEETS", "शर्मा स्वीट्स"), ("BUNTY MOBILE REPAIR", "बंटी मोबाइल"), ("PAPPU CHAAT BHANDAR", "पप्पू चाट भंडार"),
    ("GUPTA GENERAL STORE", "गुप्ता जनरल स्टोर"), ("RAJU TAILORS", "राजू टेलर्स"), ("DILLI DARBAR DHABA", "दिल्ली दरबार ढाबा"),
    ("LUCKY PHOTOSTAT", None), ("CHHABRA ELECTRICALS", "छाबड़ा इलेक्ट्रिकल्स"), ("JAI MATA DI TENT HOUSE", "जय माता दी टेंट हाउस"),
    ("MONU KIRANA STORE", "मोनू किराना स्टोर"), ("SHIV SHAKTI HARDWARE", "शिव शक्ति हार्डवेयर"), ("RAHUL CYCLE WORKS", "राहुल साइकिल वर्क्स"),
    ("BABLOO PAAN BHANDAR", "बबलू पान भंडार"), ("GARG MEDICOS", "गर्ग मेडिकोज़"), ("OM SAI JEWELLERS", "ओम साईं ज्वैलर्स"),
    ("PANGA CHAI POINT", "पंगा चाय पॉइंट"), ("KHANNA OPTICALS", None), ("SETHI SAREE CENTRE", "सेठी साड़ी सेंटर"),
    ("PINKY BEAUTY PARLOUR", "पिंकी ब्यूटी पार्लर"), ("GOLU MOMOS", "गोलू मोमोज़"), ("TIWARI DAIRY", "तिवारी डेयरी"),
    ("NEW DILLI SOUND SERVICE", None), ("ARORA BOOK DEPOT", "अरोड़ा बुक डिपो"), ("MEHTA MITHAI WALA", "मेहता मिठाई वाला"),
    ("SINGH AUTO PARTS", "सिंह ऑटो पार्ट्स"), ("LAXMI BARTAN BHANDAR", "लक्ष्मी बर्तन भंडार"), ("CHOTU KI LASSI", "छोटू की लस्सी"),
    ("KUMAR PHOTO STUDIO", "कुमार फोटो स्टूडियो"), ("BITTU JUICE CORNER", "बिट्टू जूस कॉर्नर"), ("VERMA KIRYANA", "वर्मा किरयाना"),
    ("JUGAAD REPAIR CENTRE", "जुगाड़ रिपेयर सेंटर"), ("HORN OK PLEASE MOTORS", None),
]


def rgb(c):
    return tuple(int(round(x * 255)) for x in c)


def fit(draw, text, font_path, max_w, max_h):
    """The largest font size at which `text` fits the box."""
    size = max_h
    while size > 8:
        f = ImageFont.truetype(font_path, size)
        l, t, r, b = draw.textbbox((0, 0), text, font=f)
        if r - l <= max_w and b - t <= max_h:
            return f
        size -= 2
    return ImageFont.truetype(font_path, 8)


def motif(draw, cx, cy, r, colour):
    """A little truck-art flower: petals round a dot."""
    for k in range(8):
        import math
        a = k * math.pi / 4
        x, y = cx + r * 0.6 * math.cos(a), cy + r * 0.6 * math.sin(a)
        draw.ellipse((x - r * 0.32, y - r * 0.32, x + r * 0.32, y + r * 0.32), fill=colour)
    draw.ellipse((cx - r * 0.3, cy - r * 0.3, cx + r * 0.3, cy + r * 0.3), fill=(255, 255, 255))


def board(k, latin, deva):
    bg = rgb(ACCENTS[k % len(ACCENTS)])
    light = sum(bg) > 450
    ink = (25, 20, 20) if light else (255, 250, 236)
    trim = (200, 20, 40) if light else (255, 214, 40)
    img = Image.new("RGB", (W, H), bg)
    d = ImageDraw.Draw(img)
    d.rectangle((3, 3, W - 4, H - 4), outline=trim, width=6)
    d.rectangle((13, 13, W - 14, H - 14), outline=ink, width=2)
    motif(d, 42, H / 2, 22, trim)
    motif(d, W - 42, H / 2, 22, trim)
    left, right = 74, W - 74
    if deva:
        f1 = fit(d, latin, LATIN, right - left, 50)
        f2 = fit(d, deva, DEVA, right - left, 38)
        d.text(((left + right) / 2, 50), latin, font=f1, fill=ink, anchor="mm")
        d.text(((left + right) / 2, 94), deva, font=f2, fill=ink, anchor="mm", language="hi")
    else:
        f1 = fit(d, latin, LATIN, right - left, 66)
        d.text(((left + right) / 2, H / 2), latin, font=f1, fill=ink, anchor="mm")
    return img


def main():
    atlas = Image.new("RGB", (COLS * W, ROWS * H))
    for k, (latin, deva) in enumerate(SHOPS):
        atlas.paste(board(k, latin, deva), ((k % COLS) * W, (k // COLS) * H))
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    atlas.save(OUT)
    print(f"[signs] {OUT}: {len(SHOPS)} boards")


main()
