# Panga style guide

The look every map, model, texture and light is built to. Decided 5 Nov 2026: **Fortnite-like
stylised**, our own Delhi in it (not a copy of anyone's buildings or props). Proven first on one
Connaught Place block (the style board, below), then rolled out over the map, then to the others.

Where this and `docs/rendering-guidelines.md` meet, the budgets there still hold: everything here
is plain `StandardMaterial`, baked textures and vertex colours, nothing per frame.

The colours are tokens in `assets/style/palette.json`, the one source: the game reads them
(`shared::style`), and so do the Blender and texture scripts (`tools/style.py`). A colour used
anywhere that isn't a token is a bug.

## 1. The look in one line

Bright afternoon, chunky soft shapes, big clean colour, little fine detail: readable at a sprint,
cheerful, and unmistakably Delhi (white colonnades, painted signboards, yellow-black kerbs, green
autos, neem shade).

What it is not: grey, hazy, photographic, noisy, thin, or sharp-cornered.

## 2. Colour

- **High key.** Sunlit white stucco shows at 85-92% brightness on screen, never clipped; its shade
  side light and a little cool (55-75%), never grey or lavender: the sky fill is a soft blue and
  the stucco token warm, so the two meet as a clean light shade. Nothing large on screen is darker than 15% but
  tarmac in shade, interiors' far corners and the characters' hair.
- **Saturated, but few colours per view.** Large surfaces (walls, ground, road) are calm: white,
  cream, grass green, blue-grey tarmac. The strong colours are the accents: signboards, shutters,
  autos, cars, awnings, cloth, flowers. An accent covers no more than about a fifth of a view.
- **Warm light, cool shade.** Sun cream-yellow, sky fill blue; shadows take the sky's colour.
- **No haze tint.** The distance fades to the sky's pale blue horizon, late (past 600 m), never
  pink or orange.
- **The palette** (`palette.json`) is grouped: `sky`, `light`, `ground`, `architecture`, `wood`,
  `metal`, `accent` (signage and paint), `vehicle`, `foliage`, `cloth`. New colours are added
  there, named for what they are (`stucco`, `kerb_yellow`, `auto_green`), with a line on where
  they're used.

## 3. Shape

- **Chunky.** Trim (cornices, sills, door and window frames, pilasters, kerbs) at 1.3-1.5x its
  real thickness; frames and posts never under 8 cm across; tabletops 5 cm and more; cushions and
  mattresses puffed (rounded tops, a visible bulge).
- **Soft edges.** Every edge a player can get within 5 m of is bevelled: 1.5-3 cm on props
  and furniture, 4-8 cm on architecture (columns, cornices, kerbs, steps). The bevel is what
  catches the light: it's the edge highlight for free.
- **Smallest modelled feature 4 cm.** Anything smaller (screws, grain, cracks, text) is painted.
- **Silhouette first.** A prop should be recognisable as a black shape at 20 m. Exaggerate the
  part that says what it is (a charpai's legs and rope, a fridge's handle and rounded top, an
  almirah's cornice and mirror).
- **Straight architecture, lively clutter.** Buildings stay true; small clutter (pots, crates,
  stacked goods, chairs) may lean or turn up to 3 degrees and vary a little in size.

## 4. Surfaces (textures)

The painted recipe, the same for every material:

1. A **flat base** from the palette, no noise.
2. A **soft vertical gradient**: 6-10% lighter at the top of a wall or object, darker at the
   foot (and under overhangs).
3. **Edge light and corner dark**: convex edges a strip lighter (on models: baked from the bevels'
   curvature), concave corners and contact with the ground darker (baked AO, 15-25%).
4. **Big, slow variation**: patches 1-4 m across. Since 2026-11-05 (the owner's call, against
   Fortnite's current look) the ground and the masonry carry **mid-scale surface detail from
   stylised CC0 scans** (tools/textures/scans.py): each brick, sett and tile its own tone, cracks
   in the asphalt, pebbles in the earth, worn tile faces, plaster in soft patches. The scan's fine
   grain is smoothed away and its crevices darkened; walls a game tints stay near-white so their
   palette colour still reads. Still no speckle: up close, plaster patches broad, not grimy.
5. **Painted detail** where it tells the story: tile joints, planks, a few drawn cracks, stains
   under a sill, wear on a step. Drawn as shapes with soft edges, not noise.

Numbers: tiling architecture 256 px a metre; props 400-600 px a metre in a 2048 atlas. Normal maps
only for forms you'd see at 10 m (rustication bands, tile joints, planks, bricks), soft. Roughness
0.75-0.9 for nearly everything; glossy only for glass, car paint, polished steel and the fridge
(0.25-0.4). Metal is painted metal unless it's steel utensils.

## 5. Light and grade

- Sun 32-40 degrees up, cream (`light.sun`), shadows on.
- Sky fill blue (`light.sky_fill`); a warm bounce from the ground is faked in the ambient's colour.
- Exposure and grade set so the numbers in section 2 hold in the style board views (measured by
  `just stylecheck`, below). Saturation lift about 1.15, a gentle S-curve, no crushed shadows
  (the characters' brown skin must keep its detail).
- Bloom barely there; vignette light.
- Interiors are lit warm and bright enough to read (rooms average at least 35% on screen): a warm
  ceiling light and its pool on the floor in every room.

## 6. Density

- **Street:** along a pavement, something every 3-5 m (bike, planter, stall, sign stand, crate,
  bench, parked car).
- **Shop:** every shop has a big board (0.8-1.2 m tall, the bay's width), a shutter or glass front,
  stock you can read as its trade.
- **Room:** a piece of furniture on each wall, and 4-8 small things a room (a pot, utensils, a
  calendar, a clock, a god picture, slippers, a bucket, clothes on a line, a gas cylinder).
- Cover still matters: clutter never blocks a door or a stair, and stays off the play lanes.

## 7. The style board

One Connaught Place block built fully to this guide before anything else changes: a colonnade bay
with its verandah, two shopfronts and their boards, the road and kerb in front, a parked car and
auto, the roof above, and a furnished room upstairs. Its views are the autoshot views
`style_*` (crates/game/src/autoshot.rs). The owner signs it off; then the rest of the map follows.

## 8. Checks

- `just stylecheck`: the style board views, screenshotted, with each view's brightness,
  saturation and clipping, and the sunlit and shaded stucco's and the rooms' brightness against
  section 2.
- Before calling any art done: a close-up next to a Fortnite reference frame, from two sides
  (see the memory notes on verifying close up).
- fps on Low and Medium in the style board views, against the last commit's.
