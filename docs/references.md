# References, dimensions and licences

Paths under `research/` below are on the `claude/research-*` branches, not on `main`.

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

## Bows, astras and vessels

Built by `tools/blender/weapons/arsenal.py` (`just arsenal`), lofted and baked like the khanda, but
stylised further toward their myths: they are divine weapons, not museum pieces.

| Item | Built as | Notes |
|---|---|---|
| Gandiva (Arjuna) | Indo-Persian composite bow, 1.4 m strung | Strung profile: flat grip, limbs bending back in a widening arc to the knee, stiff ears (siyah) angled forward. Crimson lacquer with gold-painted vines, as on lacquered Persian and Mughal bows; thread-bound grip, bone nocks. |
| Sharanga (Vishnu) | Short, deeply recurved horn bow, 1.15 m | *Sharanga* means "of horn". Indigo lacquer, polished horn ears, a gold Sudarshana chakra boss each side of the grip. |
| Pinaka (Shiva) | Great bow, 1.8 m | Charcoal limbs striped with ash (the tripundra's three lines), silver fittings, Vasuki coiled three and a half times round the grip with his hood raised, a trishula at each tip. |
| Arrow | Cane shaft, 0.82 m | Leaf-shaped steel head, three barred feathers, thread bindings, bone nock. |
| Tarkash | Leather quiver, gold-embroidered bands, brass mouth | Seven arrows; the arrow pickup. |
| Agneyastra / Vayavyastra | Short ornamented javelins | Flame-tongued copper head on red lacquer; twisted silver wind-vane head and white fletching on blue. |
| Sanjeevani | Five glowing herb sprigs bound with red and gold thread | |
| Soma | Small chased-silver lota with a gold stopper | |
| Amrita Kalash | Purna kalash: gold pot, five mango leaves, a coconut, mauli thread | |

Strings are not modelled: the game draws them between the anchors the script writes to
`assets/data/bows.ron` (each nock, and the bridge where the string leaves the ear's belly at rest),
so they bend with the draw. Every item is held by one rule from the hand's own bones (blade or
upper limb toward the thumb, edge or bow's back along the hand); see `crates/game/src/arms.rs`.

---

## Street guns

Built by `tools/blender/weapons/street_guns.py` (`just street-guns`) from the free models in the
licence table, read from `research/sniper-guns/models`; the guns are baked to one 1K texture set with
two LODs like the rest of the armoury. The game's names are made up and the models carry no maker's
marks. No free matchlock was on disk, so the Toradar is lofted in that script from general references
to Mughal matchlocks: a long eight-sided barrel in brass bands, a slim lacquered stock. The scopes are
lofted too, and taped on as a joke. Their rings are `tools/textures/scope.py`.

## Third-party assets and licences

| Asset | Source | Licence | Used for |
|---|---|---|---|
| Flat Guns East and West, by Pichuliru | [East](https://opengameart.org/content/cc0-flat-guns-east), [West](https://opengameart.org/content/cc0-flat-guns-west) | CC0 | Ladi-9, Teen Tigada and Thaana SLR shapes (`tools/blender/weapons/street_guns.py`) |
| Bolt action Rifle: Lee Enfield, by Lucian Pavel | [OpenGameArt](https://opengameart.org/content/bolt-action-rifle-lee-enfield) | CC0 | Thri-Not-Thri shape and textures |
| AK, by loafbrr_1 | [OpenGameArt](https://opengameart.org/content/ak) | CC0 | Saintalis shape and textures |
| The Free Firearm Sound Library (prepared set), Still North Media | [OpenGameArt](https://opengameart.org/content/the-free-firearm-sound-library) | CC0 | `gun_rifle_far`, `gun_auto_far`, `gun_thaana_slr`, `gun_saintalis` (`tools/audio/rifles.py`) |
| EnfieldRifle.wav, by SuperPhat | [Freesound](https://freesound.org/people/SuperPhat/sounds/410442/) | CC0 | The shot in `gun_thri_not_thri` |
| Mosin Nagant Bolt (fast), by rammbostein | [Freesound](https://freesound.org/people/rammbostein/sounds/267895/) | CC0 | The bolt in `gun_thri_not_thri` |
| Bullet Crack.wav, by morganpurkis | [Freesound](https://freesound.org/people/morganpurkis/sounds/386854/) | CC0 | `bullet_crack` |
| studio_small_08 HDRI (2k) | [Poly Haven](https://polyhaven.com/a/studio_small_08) | CC0 | Preview renders only |
| studio_small_09 HDRI (2k) | [Poly Haven](https://polyhaven.com/a/studio_small_09) | CC0 | Preview renders (alternative) |

| Surface scans (plaster, asphalt, concrete, withered_grass, sparse_grass, brick_wall_02, dirty_concrete, listed in `assets/textures/ultra/sources.json`) | [Poly Haven](https://polyhaven.com) | CC0 | Real surfaces on the city maps, Medium and up (`assets/textures/ultra/`) |
| kloofendal_38d_partly_cloudy_puresky HDRI (8k) | [Poly Haven](https://polyhaven.com/a/kloofendal_38d_partly_cloudy_puresky) | CC0 | The sky on every level, with Delhi's haze baked in (`assets/textures/sky/`, `tools/textures/sky.py`) |
| Room surfaces: PaintedPlaster017, Terrazzo013 | [ambientCG](https://ambientcg.com/view?id=PaintedPlaster017) | CC0 | Walls and mosaic floors of the hollow buildings, baked into `assets/textures/rooms/room_atlas.png` by `tools/textures/room_atlas.py` |
| Room surfaces: grey_plaster, damaged_plaster, interior_tiles, floor_tiles_06, concrete_floor_worn_001 | [Poly Haven](https://polyhaven.com/textures) | CC0 | The same atlas: stains, old plaster and brick, shop tiles, marble squares, godown floors |
| Ultimate Animated Animals: ShibaInu, Cow (rigged, 12 to 13 clips each) | [Quaternius](https://quaternius.com/packs/ultimateanimatedanimals.html), files taken from the mirror in [danwahl/animasim](https://github.com/danwahl/animasim) because the pack's Drive folder was over quota | CC0 | The street dog and the zebu (`assets/models/life/dog.glb`, `cow.glb`): reshaped and repainted by `tools/blender/life/animals.py` |
| Fabric scans: hessian_230, gingham_check, floral_jacquard, rough_linen, quatrefoil_jacquard_fabric, stretch_poplin, denim_fabric, terlenka (1K diffuse) | [Poly Haven](https://polyhaven.com/textures/fabric) | CC0 | Sacks and cloth in the market goods' atlas (`assets/models/life/goods_atlas.png`, `tools/blender/life/goods_atlas.py`) |
| MPFB (MakeHuman plugin for Blender) | [extensions.blender.org/add-ons/mpfb](https://extensions.blender.org/add-ons/mpfb/) | GPL (tool only); generated humans are CC0 | Character base bodies, rig (`game_engine`) |
| LGDuel's bot aim (`crates/bot/src/lib.rs`), adapted in `crates/shared/src/sim/aim.rs` | [github.com/s0pk/lgduel](https://github.com/s0pk/lgduel) | MIT, notice kept in the file | How bots turn and see: a limited turn speed and a late sight of targets |
| Leaf scans LeafSet013, 022, 023 and 024 (2K) | [ambientCG](https://ambientcg.com/view?id=LeafSet023) | CC0 | Trees' and bushes' leaf atlas (`tools/blender/nature/leaf_atlas.py`, `assets/models/textures/leaf_atlas_*.png`) |
| MakeHuman system assets (`makehuman_system_assets_cc0.zip`, sha256 `b542127a…0107`) | [makehumancommunity.org](https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html) | CC0 | Body proxies, skin detail (re-toned), eyes, brows, lashes, hair (short04, braid01) |
| "Male classic jeans", punkduck, MakeHuman community pack pants02 | [asset packs](https://static.makehumancommunity.org/assets/assetpacks.html) | CC-BY 4.0: credit needed | Man's jeans, recoloured, top cut away under the polo |
| "Running shoes 01", punkduck, MakeHuman community pack shoes02 | [asset packs](https://static.makehumancommunity.org/assets/assetpacks.html) | CC-BY 3.0: credit needed | Man's shoes, recoloured |
| Polo shirt, Namuhekam, pack shirts01; flapper dress, AEthelraed_Unraed, pack dress01; harem pants and cloth shoes, MRT (toigo), packs pants01 and shoes01; brows 02 and 05, Mindfront, pack eyebrows01 | [asset packs](https://static.makehumancommunity.org/assets/assetpacks.html) | CC0 | Polo; the dress cut into the woman's kurta; salwar; flat shoes; brow strands baked into the head |
| Mukta Bold, 2014 Girish Dalvi, Ek Type | [Ek Type](https://github.com/EkType/Mukta), [Google Fonts](https://fonts.google.com/specimen/Mukta) | SIL OFL 1.1 (`assets/fonts/OFL-Mukta.txt`) | All on-screen text, and five shop boards |
| Gajraj One, 2022 The Gajrajone Project Authors | [Google Fonts](https://fonts.google.com/specimen/Gajraj+One) | SIL OFL 1.1 (`assets/fonts/OFL-GajrajOne.txt`) | Titles and buttons |
| Khand Bold and SemiBold, 2014 Indian Type Foundry | [Google Fonts](https://fonts.google.com/specimen/Khand) | SIL OFL 1.1 (`assets/fonts/OFL-Khand.txt`) | Numbers |
| Button click (`assets/audio/ui_click.ogg`) | [bevy_new_2d](https://github.com/TheBevyFlock/bevy_new_2d), by Jaszunio15 | CC0 | Front door buttons |
| Screen, menu and button patterns in `crates/game/src/front/` | [bevy_new_2d](https://github.com/TheBevyFlock/bevy_new_2d) (CC0, MIT or Apache 2.0), [bevy_game_template](https://github.com/NiklasEi/bevy_game_template) (CC0) | CC0 | Followed by hand, adapted to Bevy 0.20 |
| Furniture Pack 1.0 by Kenney: `table`, `stoolBarSquare`, `kitchenBar`, `desk`, `cabinetTelevisionDoors`, `televisionModern` | [OpenGameArt](https://opengameart.org/content/furniture-kit), from [kenney.nl](https://kenney.nl) | CC0 | Room kit: base meshes of the table, stool, counter, desk and TV unit, stretched to real sizes (`tools/blender/props/room_kit.py`) |
| Ultimate House Interior Pack, June 2020, by Quaternius: `Couch_Medium2` | [OpenGameArt](https://opengameart.org/node/126986), from [quaternius.com](https://quaternius.com) | CC0 | Room kit: the sofa, thinned to under 300 triangles and baked from the full mesh |
| Scans: rosewood_veneer1, stained_pine, teak_veneer, plywood, green_metal_rust, hessian_380, velour_velvet | [Poly Haven](https://polyhaven.com) | CC0 | Room kit: wood, painted steel, jute and velvet in the atlas |
| Scans: Metal009, PaintedMetal004, Plastic006, Plastic008, Plastic010 | [ambientCG](https://ambientcg.com) | CC0 | Room kit: bare steel, the cylinder's red paint and plastics in the atlas |

All other textures are generated: weapons and characters are baked from procedural Blender
materials (`tools/blender/lib/`, `tools/blender/characters/finish.py`); leaf and bark textures are
drawn procedurally (`tools/blender/nature/textures.py`).

## Characters

Two everyday Delhi people, chosen from street photos and video (`research/characters/README.md`):
no real person's face or body, warm brown South Asian skin, softer bodies than the rig's build.
- Man: plain maroon polo, indigo jeans, running shoes, a black school bag (`outfits.backpack`,
  our own, rigid on `spine_03`), short side-parted hair, stubble.
- Woman: plain magenta kurta (MakeHuman's flapper dress cut at the knee, slit at the sides),
  cream salwar (harem pants tucked in under the kurta), flat shoes, a long plait, kajal.
- Clothes: MakeHuman community garments fitted by MPFB (`outfits.worn`); they need the packs from
  `tools/blender/characters/fetch_mh.sh <dest>`, with `research/characters/downloads/makehuman`
  as `<dest>`. `finish.fitted_material` recolours them.
- Faces: MPFB detail targets (narrower nose and nostrils, a slight bridge on the male, finer
  lips, eyes a little larger and more open) over the race macros, in `human.py` VARIANTS.
  `face.py` measures eyes and nose tip on the mesh, adds the male's stubble (a `beard` attribute
  the skin shader darkens), paints the scalp under the hair, and bakes strand brows into the skin.
  Check with `just faces` (exported GLB), `just face-lab` or `just lookbook`.
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

## Street fights

| Item | Built as | Source and licence |
|---|---|---|
| Lathi, gada, tawa, hockey stick | Lathed and lofted by `tools/blender/weapons/melee.py` | Ours. The gada is the plain wrestler's stone on bamboo, no gold, no god. The stick has no maker's name |
| Talwar | Our old straight sword (`old_sword.glb`, `old_sword.py`) with the blade bent | Ours |
| Dhal | ["Indian dhal (shield), 19th century"](https://sketchfab.com/3d-models/indian-dhal-shield-19th-century-fca1088c468c480a845977189d66bf59) by Pedram Ashoori, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Changed: painted lions and sun face painted out, red lacquer and plain brass in their place, resized, glow removed | Credit required |

| Swing rules | `crates/shared/src/sim/melee.rs`: swing phases, the `struck` list, `in_front` and the order of the guard's tests, adapted by hand | [wow-souls](https://github.com/jackke88/wow-souls), MIT (notice kept at the top of that file). Its GPL `dsr-*` crates are not used. The swing as data (direction and time, no bones) is an idea from Left 4 Dead 2's `cricket_bat.txt`, no code. All numbers are ours |

The credit above must appear on the game's credits screen. The dhal came through the public
Objaverse mirror; fetch the original with a free Sketchfab login before shipping.

## Sound

The rifles are free recordings, cut by `tools/audio/rifles.py` (see the licence table). The street
and the fights are cut from recordings too (below). Most of the rest are synthesized in
`tools/audio/sfx.py` (noise, filters, modal resonators, Karplus-Strong
strings), no recordings: footsteps on dirt, stone, wood, grass and water; sword whooshes and
hits; bow twang and draw, arrow impacts; flesh and shield hits; hit markers; jump, land, glider;
chest creak with coins, and a temple bell (ghanta) for the golden kalash; drinking, reloading;
Agni fire and Vayu gusts; a conch (shankha) when the Pralaya starts to close, a bell for kills,
a low gong on elimination; loops for wind, freefall, the zone wall, the river, a tanpura drone
aboard the vimana and the day's ambience (wind, koel, sparrows, crows).

### Street and fight recordings

Cut by `tools/audio/street.py` from CC0 recordings, so no credit is owed, but the sources are
named here: Freesound files by SoundEnsemble, Alcappuccino, florianreichelt, kyles, Kriaa,
AntumDeluge, erosax, Ambientsoundapp, IENBA, HowardV, Jibanshil, cupido-1, Jofae, giddster,
ramblinglibrarian, edranil, csaszi, FreeToUseSounds, makbulchowdhury, Dalesome, zapsplat.com,
newlocknew, worthahep88, Whiprealgood, ethanchase7744, spycrah and tbone1999, and Kenney's Impact
Sounds (https://kenney.nl/assets/impact-sounds). `previews/fights-sounds/sound_list.md` gives each
sound's source, the part cut and where it plays. Each licence was read on the sound's own page
(`research/fights-sounds/data/freesound_licences.csv`). They came as Freesound's public previews.
CC0 is the uploader's claim, and nobody has listened to them all yet: music or clear voices must be
checked by ear before release. Left out: temple crowds, real Metro announcements, anything CC-BY.

## Hiding and bot senses

Read by hand and rewritten for this game; none of their code is a dependency.

- [shards-engine](https://github.com/larelgit/shards-engine), Copyright (c) 2026 larelgit, MIT OR
  Apache-2.0. The crouch rules (`shards_motion/src/controller/stance.rs`: feet stay, standing up
  waits for room) in `crates/shared/src/sim/stance.rs`; sight, hearing, memory
  (`shards_npc/src/perception.rs`) and the hiding search (`shards_npc/src/hide.rs`) in
  `crates/shared/src/sim/senses.rs`.
- [saddle-ai-fov](https://github.com/julien-blanchon/saddle-ai-fov), Copyright (c) 2025 Julien
  Blanchon, MIT-0. The detection meter (`update_stimulus_entry`) in `senses.rs`.
- [bevy_ahoy](https://github.com/janhohenheim/bevy_ahoy), MIT or Apache-2.0. The stand-up check
  (`handle_crouching`), as a sweep up from the crouched head in `stance.rs`.
- [Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html) by
  Quaternius, CC0. The crouch's proportions, measured from its `Crouch_Idle_Loop`, in
  `tools/blender/characters/anims.py`. Our clips are authored on our own rigs; none of its data ships.

## Delhi map (Panga)

Connaught Place and the head of Janpath, New Delhi, from OpenStreetMap: building outlines and
heights, roads, parks, car parks, trees and place names, converted by `tools/osm/delhi.py` into
`assets/data/maps/delhi_cp.json`. Map data © OpenStreetMap contributors, available under the Open
Database License (ODbL 1.0): https://www.openstreetmap.org/copyright. The game must credit this
wherever the map is shown: it stands under the minimap and on the web menu. The inner circle's blocks are generated at the radii
and radial-road bearings measured from that data (`crates/shared/src/map/delhi.rs`).

## NCR map data

The NCR districts come from OpenStreetMap, through Geofabrik's Northern Zone and Central Zone extracts
of India (https://download.geofabrik.de/asia/india.html, read on 31 Oct 2026). `tools/osm/ncr_extract.py`
cuts them and `tools/osm/ncr.py` converts them to `assets/data/maps/ncr_*.json`.

- Map data © OpenStreetMap contributors, Open Database License 1.0:
  https://www.openstreetmap.org/copyright
- Geofabrik's own wording: "Data processed by Geofabrik GmbH and created by OpenStreetMap
  Contributors". The extracts carry the same ODbL 1.0 licence.
- Some holy-site positions come from Overture Places, release `2026-10-21.0`, in rows supplied by
  Meta under CDLA Permissive 2.0 (https://docs.overturemaps.org/attribution/). See
  `docs/ncr/HOLY-SITES.md`, which also lists the sources used only to find sites.

### What the game must show

Credits screen, or a splash screen, or a menu: OpenStreetMap's guideline accepts any of these for
games (https://osmfoundation.org/wiki/Licence/Attribution_Guidelines). This wording covers all three
sources:

    Map data © OpenStreetMap contributors, available under the Open Database License
    (openstreetmap.org/copyright). OpenStreetMap extracts processed by Geofabrik GmbH.
    Some place data from Overture Maps Foundation and Meta, CDLA Permissive 2.0.

The full map screen already shows "Map data (c) OpenStreetMap contributors, ODbL 1.0"
(`crates/game/src/hud.rs`). That covers OpenStreetMap while the map is open. The full wording above
still needs a home on a credits or splash screen before anyone outside the team plays the game.

### What share-alike means for `ncr_*.json`

This is a plain reading of the ODbL, not legal advice.

`ncr_*.json` and `delhi_*.json` are adapted OpenStreetMap data: a "Derivative Database". Our heights,
holy lists and other changes in them are part of that database.

- **While the game stays inside the team, nothing has to be shared.** Internal use is not public
  (ODbL 4.5c).
- **Once anyone outside the team gets it, these files must stay under ODbL** (4.4), and we must offer
  a machine-readable copy of them, or of the changes and how to make them, free of charge if it is
  online (4.6). A link in the credits to the files and `tools/osm/` is the usual way. Anyone may then
  reuse them, commercial rivals included.
- **The offer applies even if the files stay hidden inside the game.** A public picture or video made
  from the data counts too, as ODbL 4.4 and 4.6 include Produced Works.
- **The game's code, art, models, sound and design are not covered.** Share-alike reaches the
  database, not what is built with it (4.5b). Keep our own creative data out of these files.
- **Anything merged into these files joins the database.** Only data we may share this way can go in.
  Overture's permissive rows can. Google Maps data cannot, and nor can Wikimapia's CC BY-SA 3.0 data.
  This is why `docs/ncr/HOLY-SITES.md` has its rule for positions.
- **No locks on the data alone.** Encrypting it is allowed only if an unlocked copy is offered too, at
  no extra charge and just as easy to get (4.7).

One thing is not settled. OpenStreetMap's guideline says a result meant for extracting the data is a
database and anything else is a Produced Work, and it says nothing about games
(https://osmfoundation.org/wiki/Licence/Community_Guidelines/Produced_Work_-_Guideline). The game
embeds the JSON as plain text in its binary, so the safe reading is that we convey the database
itself: keep the ODbL notice with it, and publish the files. Ask a lawyer before a commercial release.

## Vehicles

The drivable vehicles (`crates/shared/src/sim/vehicle.rs`). Models come from `research/vehicles/models` through
`tools/blender/props/vehicles.py`; every one is CC-BY, so its line below must also go on the game's credits screen.
The line on the auto's hood (`tools/blender/props/hood_line.png`) is our own text set in Mukta Bold, OFL, from `assets/fonts`.

| Item | Source | Licence | Use |
|---|---|---|---|
| Drive law | [TukTuk.io](https://github.com/anjulalk/tuktukio) by Anjula Karunarathne, `client/src/drive/tuk.ts` | MIT | Copied into `crates/shared/src/sim/drive.rs`, notice kept |
| Body lean, wheel turn, engine pitch | [Starter Kit Racing](https://github.com/KenneyNL/Starter-Kit-Racing) by Kenney | MIT | Adapted in `crates/game/src/vehicle_feel.rs`, notice kept |
| Auto: "Tuk Tuk Rikshaw" | [alnmathew](https://sketchfab.com/3d-models/tuk-tuk-rikshaw-0bdfdf97a0f248008806840f7188367c) | CC-BY-4.0 | Scaled, wheels reduced, repainted in Delhi's green and yellow |
| Khuli Gaadi: "Jeep Willys" | [skudgee (Willy Decarpentrie)](https://sketchfab.com/3d-models/jeep-willys-a28600c16b9e44bcb160ede0d199180d) | CC-BY-4.0 | `gaadi_khuli.glb`. Toon shell and three mounted guns removed, turned into right-hand drive, wheels rounded, a star, serial numbers and a maker's name painted out, panes added, repainted one olive |
| Scooter: "honda-smart-dio-z4-2006-1" | [AlexanderChen394](https://sketchfab.com/3d-models/none-ef6060608fe04cacbf6d4c817f494d6c) | CC-BY | Scaled, reduced, painted (the model had no colour) |
| Chhota Truck: "Mini-Truck" | [ROY (roy.3dartist)](https://sketchfab.com/3d-models/mini-truck-71e499219de548a7a1be74c237b6a2fb) | CC-BY-4.0 | `gaadi_minitruck.glb`. Cut from 153,449 triangles, hidden engine parts dropped. The model's four textures are not used: they look like stock photographs, and the tyre's still has a picture library's watermark. Our own colours, rust and tread are baked into one atlas. New windscreen and steering wheel |
| Bike: "Pulsar 135 Ls 2" | [EmanuelRestrepoVelez](https://sketchfab.com/3d-models/none-accbf2048c9344718791588b70d2ebf8) | CC-BY | Scaled, reduced, levelled, number plate removed |
| Tractor: "Tractor" | [le0tard](https://sketchfab.com/3d-models/tractor-278fc1e0516c401eba358d3aa2e55635) | CC-BY-4.0 | `gaadi_tractor.glb`. Cut from 24,759 triangles, narrowed to real width, nine materials baked to one atlas, tyres blackened |
| Phatphati: "Classic Motorcycle" | [DeclanLea14](https://sketchfab.com/3d-models/none-f246ff1a68d3459892f9420b744a58b6) | CC-BY | Scaled, reduced, levelled; two makers' names and a number plate painted out |
| Gaadi: "Stylized Suzuki Alto Works" | [EddyNL](https://sketchfab.com/3d-models/none-467f2e28ff334919879841216766903b) | CC-BY | Scaled; a maker's badge and a brand's labels painted out |
| Sarkari Gaadi: "Hindustan Ambassador" | [rSquare](https://sketchfab.com/3d-models/hindustan-ambassador-a84ef7126b2b43adb7b4b28a4d42e5fb) | CC-BY-4.0 | `gaadi_ambassador.glb`. Narrowed to real width, repainted white with dust and chrome, a hub cap mark and the number plate removed, window glass redone, panel maps softened |
| Engine loops (`engine_auto`, `engine_bike`, `engine_thump`) | Freesound: [DavidBlouin 832083](https://freesound.org/people/DavidBlouin/sounds/832083/), [rsn267 274962](https://freesound.org/people/rsn267/sounds/274962/), [ashimda 159326](https://freesound.org/people/ashimda/sounds/159326/) | CC0 | Cut and looped by `tools/audio/vehicles.py` from the public previews |
| Horns | `tools/audio/vehicles.py` | Ours | Synthesized |

The research behind these choices is in `research/vehicles/README.md`, on the branch
`claude/research-vehicles-1a3b7e`.

## Shop sign lettering

`assets/textures/panga/signs.png` (made by `tools/textures/signs.py`) is lettered in Noto Sans
Bold and Noto Sans Devanagari Bold, by Google and the Noto project, under the SIL Open Font
License 1.1 (https://openfontlicense.org). The OFL allows text rendered in the fonts to be used in
any work; the font files themselves aren't shipped. The shop names are made up: no real brands or
businesses. Five boards (the tent house, hardware, jewellers, chai stall and bartan boards) were
redrawn in Mukta Bold, with the Hindi set by macOS CoreText, when their names changed.

## On-screen fonts

The three on-screen fonts have their rows in the table above. All are under the SIL Open Font
License 1.1; each licence sits beside its font in `assets/fonts/`, and the fonts are built into
the game.

## Delhi words

`assets/data/lines.ron` holds every on-screen line. The words were drafted from public clips and
photos of daily Delhi speech (`research/delhi-words/`); no text was copied from any game. Every line
is a draft until a Delhi native has read it (`ok: false`).
