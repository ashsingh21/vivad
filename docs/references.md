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

| MPFB (MakeHuman plugin for Blender) | [extensions.blender.org/add-ons/mpfb](https://extensions.blender.org/add-ons/mpfb/) | GPL (tool only); generated humans are CC0 | Character base bodies, rig (`game_engine`) |
| MakeHuman system assets (`makehuman_system_assets_cc0.zip`, sha256 `b542127a…0107`) | [makehumancommunity.org](https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html) | CC0 | Body proxies, skin detail (re-toned), eyes, brows, lashes, braid hair |

All other textures are generated: weapons and characters are baked from procedural Blender
materials (`tools/blender/lib/`, `tools/blender/characters/finish.py`); leaf and bark textures are
drawn procedurally (`tools/blender/nature/textures.py`).

## Characters

Generic male and female warriors, stylized realistic, warm brown South Asian skin.
- Male: off-white dhoti with zari border and pleated fold, maroon kamarband, saffron angavastram,
  red pagdi (crossed wraps, bandhani dots), gold bajuband, kada and hansuli.
- Female: Maratha nauvari (nine-yard) saree worn for movement, nirya pleats, pallu, maroon choli,
  gold kamarbandh, bangles, bajuband, silver payal, braid.
- Rig: MPFB `game_engine` (53 bones). GLB faces +Z with origin at the feet; the game turns it to -Z.
- Budget: LOD0 30k, LOD1 13.5k, LOD2 5.4k tris; one 2k opaque texture set + alpha hair cards.

## Vegetation

Neem, peepal, banyan (hero, aerial prop roots), coconut palm, lantana and bougainvillea bushes.
Trees have trunk colliders; bushes are soft cover.

## Architecture kit

All procedural (Blender scripts in `tools/blender/architecture/`, numpy textures in `texgen.py`),
no third-party meshes or textures. Each model matches its gameplay colliders in
`crates/shared/src/map/india.rs`; when you change one, change the other.

- **Village huts** (`vernacular.py`): mud-and-dung plaster over a geru (red ochre) dado with a
  white mandana border, or exposed brick; gable thatch on bamboo rafters; plinth (chabutra),
  matka pots, hearth. Scaled per hut in X/Z around a 5.2 x 4.5 m base.
- **Market stalls**: bamboo posts, striped cotton awning, takht counter with spices, brass or cloth.
- **Great stupa** (`buddhist.py`), after Sanchi Stupa 1: medhi with walkway railing, anda,
  harmika, three chhatras, vedika railing, four toranas (three architraves with volutes,
  dharmachakra, triratnas, yakshi brackets).
- **Ajanta cliff**: Deccan basalt cliff with flow ledges and an eroded lip; two chaitya halls
  (horseshoe window, ribbed vault, rock-cut stupa) and two viharas (cells, a shrine Buddha);
  niches with seated Buddhas on the facades; a stair cut up a rock spur at the north end.
- **Rani ki Baoli lining** (`stepwell.py`), after Rani ki Vav: tiers of niches with four-armed
  Vishnu figures, jali panels, cornices; carved pavilion pillars.
- **Hampi** (`hampi.py`): boulders, gopuram, Dravidian temple, stone chariot, and the ruined
  mandapa (pillar states in `RUIN`, mirrored by `MANDAPA_RUIN_PILLARS`). Stucco is weathered with
  black lichen and rain streaks (`texgen.weather_streaks`).

- **Courtyard havelis** (`india.rs::haveli`, built from the kit's walls, arches and jharokhas):
  rooms round an open chowk with a tulsi platform, arcaded below, a bracketed gallery above,
  an arched street gateway, a stair to a barsati on the roof terrace, chhatris.
- **Samai** (`vernacular.py`): the brass standing oil lamp lighting the cave shrines; the game
  adds its flames and light from `MapLayout::lamps`.

**Figures** (`figures.py`, needs MPFB like the characters): the seated Buddha (padmasana,
dhyana mudra, ushnisha) and the four-armed standing Vishnu (abhaya, mace, discus and conch,
dhoti) are posed MakeHuman bodies fused into stone by voxel remeshing, saved as
`figures/*_{hi,lo}.ply` plus attribute anchors in `figures/*.json`. The kit adds pedestals,
halo, crown and attributes. Faces are soft, not carved in detail: they hold up at a few metres.

**Textures**: props export as `.gltf` + `.bin` that point at `assets/models/textures/`, one
file per distinct texture named by content hash, shared by every model that uses it.
`tools/prune_textures.py` (run by `just assets`) dedupes and removes unused ones.

## Warrior animation

Authored procedurally in `tools/blender/characters/anims.py`, no mocap or third-party clips:
idle; walk (3.6 m/s) and run (5.6 m/s) in four directions; forward sprint (7.4 m/s); jump, fall,
skydiving freefall, glide; death; and torso-only sword guard and swing, bow aim, drinking. Legs
use two-bone IK against foot paths that move at the clip's ground speed (the script prints the
remaining foot slide per clip, under 1 cm). Clips are per rig because the male and female rest
bone orientations differ. In game (`crates/game/src/animation.rs`) they are blended from the
simulation state, with weapon actions layered on the torso by bone masks.
