# References, dimensions and licences

All models are built at real-world scale in metres. Sources are listed per weapon. Museum
images were used only as visual reference; no museum imagery is redistributed in this repo.

## Weapon axis convention (all weapons)

| Space | Forward (blade/business end) | Cutting edge | Flats | Up |
|---|---|---|---|---|
| glTF / Bevy (Y-up) | **+Y** | **-Z** | ±X | +Y |
| Blender (Z-up) | +Z | +Y | ±X | +Z |

Origin = centre of the main grip, where the hand closes. Constants live in
`crates/shared/src/weapon.rs` (`WEAPON_FORWARD`, `WEAPON_EDGE`). `just view <name>` loads the GLB in
Bevy and draws these axes at the origin.

---

## Khanda (India)

Broad straight sword with a spatulate tip, typically with riveted reinforcing plates along the
spine and a "Hindu basket hilt": quillon block with langets, broad knuckle guard, disc pommel and
a spike (used as a second-hand grip or forearm guard). Blades were often thin; the plates stiffen
them.

### Measured examples

| Source | Date | Blade | Overall | Weight | Notes |
|---|---|---|---|---|---|
| [Fitzwilliam Museum O.9-1879](https://data.fitzmuseum.cam.ac.uk/id/object/158313) | c. 1600-1700 | 84 cm | 95.5 cm | 1230 g | Ganjam; probably Tanjore armoury |
| [Fitzwilliam Museum O.18-1879](https://data.fitzmuseum.cam.ac.uk/id/object/158322) | 16th c. blade? | 84 cm | 98 cm | 1275 g | Blade possibly earlier than hilt |
| [Mandarin Mansion, "large all steel khanda"](https://mandarinmansion.com/item/large-all-steel-khanda) (dealer) | 17th-18th c. | 83.8 cm | 103 cm | 1337 g | All-steel hilt |
| [Mandarin Mansion, "early Indian khanda"](https://mandarinmansion.com/item/early-indian-khanda) (dealer) | early | — | ~63 cm | — | Widths: base 65 mm, middle 41 mm, tip 54 mm; ~2 mm blade |
| [Worcester Art Museum 48884](https://worcester.emuseum.com/objects/48884/khanda-sword) | — | — | — | — | Two overlaid reinforcing pieces per side, scalloped terminals, riveted; discoid pommel with tall octagonal spike |
| [The Met 36.25.1316](https://www.metmuseum.org/art/collection/search/24322) | 17th-18th c. | — | 106.7 cm | 1304 g | Firangi (related form), for hilt comparison |

Dealer listings are commercial descriptions, not curated museum data, and are used only for
widths and construction details the museum records omit.

### Dimensions used in the model

| Feature | Value | Basis |
|---|---|---|
| Blade, guard to tip | 0.822 m visible (0.830 incl. tang shoulder) | Fitzwilliam 84 cm |
| Overall | 1.008 m (spike tip to blade tip) | Between Fitzwilliam 98 cm and dealer 103 cm |
| Blade width | 46 mm at base, 40 mm at 55 %, 52 mm at 93 % | Narrow-waisted, flaring to a spatulate tip (dealer widths, scaled to an 84 cm blade) |
| Spine thickness | 5.4 mm at the root, tapering to 3.0 mm (distal taper) | Thin blade (~2 mm at the flats) stiffened by plates |
| Edge | 0.36-0.44 mm apex, 5.5-6 mm edge bevel; blunt 1.2 mm ricasso for the first 6 % | Believable fighting edge (not razor) |
| False edge | Spine sharpened over the last ~20 % | Common on spatulate khanda tips |
| Fuller | One 6.8 mm wide x 1.0 mm deep, 13 %-60 % of length, with run-outs | Many khandas carry narrow fullers |
| Reinforcing plates | 12.5 mm wide x 1.3 mm, both flats, half the blade length, cusped terminal, 3 rivets per side | Worcester description |
| Langets | 62 mm long, 31 mm wide, ogee point | Hindu hilt construction |
| Grip | 96 mm long, 24-31 mm diameter, baluster with central ridges | Small Indian grip (hands closed tight between guard and disc) |
| Pommel disc | 80 mm diameter, upturned rim | Hindu hilt |
| Spike | 60 mm, octagonal section, collared, bud finial | Worcester: "tall octagonal section spike" |
| Knuckle guard | 16 → 46 → 16 mm wide band, 3.4 mm thick, central rib | Broad knuckle guard of the Hindu hilt |

**Deliberate deviation from the generic quality bar:** the brief asks for "wrapped grips built as
real spirals". Hindu-hilt khanda grips are solid steel/iron, not wrapped, so the khanda has none.
Spiral wraps will appear where history supports them (e.g. jian, dao).

### Surface finish

- Blade: watered (wootz) crucible steel, polished; edge honed bright; light staining and pitting
  near the hilt.
- Hilt, plates, langets: blackened iron with **koftgari** (gold overlay) borders, a scrolling vine
  with leaves and, on the grip, a field of four-petal rosettes (buti). All of it is procedural and
  baked; no image sources are used.

---

## Third-party assets and licences

| Asset | Source | Licence | Used for |
|---|---|---|---|
| studio_small_08 HDRI (2k) | [Poly Haven](https://polyhaven.com/a/studio_small_08) | CC0 | Preview renders only |
| studio_small_09 HDRI (2k) | [Poly Haven](https://polyhaven.com/a/studio_small_09) | CC0 | Preview renders (alternative) |

All textures shipped in GLBs are baked from procedural Blender materials in `tools/blender/lib/`.
