# Porting the NCR map (branch `ncr-port`)

The NCR (crate::map::compose: the real Delhi NCR, 4 km across, its districts from OpenStreetMap and
the Delhi maps merged in) comes from the `game-playable` branch, which split from `main` before
main's restyle (stylised CP, stone base course, start screen, loading screen). Ported in steps,
each checked before the next.

## 1. Baseline

The branch as it is (5 Nov, RTX 3060, `VIVAD_MAP=ncr VIVAD_AUTOPILOT=1`, worktree `build/gp`):
runs, 64 warriors, 3.8-3.9 GB resident and steady over two minutes (its own note: about 3.7 GB on
a Mac; never tried in a browser). It logs no frame rate. Its look is the branch's (realistic
textures), not main's stylised one: step 3 has to restyle what it draws, not just port it.

## 2. The map generated on main (done)

- Three-way merged with main (`git merge-file` against the merge base): map/{mod,kit,cp,delhi}.rs.
  india.rs stays main's (the branch cut it down; Bharat lives on in main). Both sides' materials
  are kept (main's Stucco..Plinth, the branch's Concrete..MetroSign); the branch's richer river is
  `channel` (main's `river` stays the India map's channel).
- New from the branch: compose, ncr, ncr_art, interiors, parking, place, stalls, thickets,
  navigation, and sim/street_floor.rs (its StreetFloor, out of its reworked nav).
- `crate::SmallRng` pinned to Xoshiro256PlusPlus (the same stream on a browser as on a desktop).
- Kept off the small maps for now (they'd change their balance): kiosks and roof tanks (main's
  street kit has no models for them yet: invisible colliders), loot cleared off holy sites.
- Taken onto the small maps (fixes): CP's chests face their own shop front and none stands under
  the stairs; racks and clutter kept off the stair wells (57 pieces floated on CP, 16 on Bharat).
- NCR fixes for main's generators: a district's edge is frayed only where its ground is level
  with the land (main's park hills were cut off, leaving what stood on them in the air).
- Checks: `cargo test -p shared` 57 passed, 2 ignored (below); `map_stats` (new) shows the small
  maps unchanged but for the fixes above; a headless NCR match (`VIVAD_MAP=ncr just sim 1 15`) runs
  to a winner in 48 s, 400 MB; `just net` 0 failed; style check 8 views on the guide.

## 3. Drawn on main (next)

Tried as it is (5 Nov, RTX 3060, `VIVAD_MAP=ncr VIVAD_MENU=0`): out of GPU memory within 30 s
(17 GB resident). Main's renderer builds the NCR as it builds a small map: 13,244 street kit chunk
meshes, 29,640 tree and bush meshes, a million triangles of trim, every one drawn. So the branch's
renderer for it comes too: its long-range terrain (visuals/clipmap, heightmap), building tiers
(buildings/tiers) and their LODs, ported the way step 2 was.

- Render the NCR with main's renderer (and the branch's long-range terrain only if needed);
  measure frame rate and memory on this machine.
- The street kit's gumti and tanki models (the branch's tools/blender/props/street_kit.py).
- Textures for the NCR's materials (Concrete, Asphalt, Glass, Steel, Pavers: plain for now).
- Retune with the frame rate: bushes (4041 against the branch's 800-2600) and hollow buildings
  (389 against about 500); both tests are ignored until then.
- The safe zone for 4 km (it starts at 360 m, a small map's); bots open few chests.

### Memory (5 Nov, RTX 3060; VIVAD_MEMORY=1 logs it, VIVAD_SKIP=kit,trees,blocks,city leaves art out)

Where the NCR's memory went with main's renderer, CPU-side mesh data: base 0.6 GB, the city and
blocks 1.85 GB (27 M vertices), the street kit 2.7 GB (43 M), trees and bushes 4.6 GB (76 M): merged
copies of each model at every LOD. Done so far:

- Trees and bushes past 2,000, and street kit past 8,000 pieces, are entities sharing their
  model's meshes (drawn instanced) instead of merged copies: the NCR draws, 4.3 GB of GPU memory
  (was out of memory past 11.6 GB). The small maps stay merged (fewer entities: the browser's one
  thread).
- Built meshes (buildings, merged kit and trees) kept on the GPU only (their bounds are cached).
  Resident: NCR 7.4 GB (was 17 GB, then out of memory); Connaught Place 6.3 GB (was 8.8 GB).
- A base of about 3 GB with no map art at all (2.7 GB anonymous): images' CPU copies 0.6 GB,
  decoded audio 0.3 GB, the rest unaccounted (driver, pipelines? needs a heap profiler).

Then (6 Nov):

- Textures: GPU-only once their mips are made, and the characters' embedded ones (16 MB each) at
  once; the city's sets packed GPU-compressed (KTX2 BC7/BC5, tools/texpack from the branch,
  `just textures-pack`; crate::tex loads them on the desktop, the browser keeps PNGs).
- Instanced trees and street kit on every map on the desktop (crate::diag::instanced): on Connaught
  Place it halved GPU memory and was faster (118 to 134 fps on style_block, 102 to 108 on
  style_trees). The browser merges below the thresholds until measured there.
- Ground tiles and grass GPU-only.

| | GPU | resident |
|---|---|---|
| NCR, before | out of memory (>11.6 GB) | 17 GB |
| NCR, now | 4.2 GB | 5.9 GB |
| Connaught Place, before | 4.2-4.8 GB | 8.8 GB |
| Connaught Place, now | 1.9 GB | 3.1 GB |

Then textures capped where models are made (tools/glb_textures.py: characters and props 1024 px,
weapons 512; a texture changed after it loads leaves its first upload alive on the GPU, so it has
to be done to the files). wgpu's own allocator report (VIVAD_MEMORY=1) now names what it holds.

| 6 Nov, later | GPU allocated | resident |
|---|---|---|
| Connaught Place | 1.02 GB (textures 343 MB, mesh 423 MB) | 2.4 GB |
| NCR | 3.3 GB (mesh 2.65 GB, textures 437 MB) | 5.3 GB |

The NCR's mesh memory: the city's buildings about 1.07 GB, the blocks (CP's ring, bridges, halls)
about 0.88 GB, the rest (ground tiles, grass, characters, model templates) about 0.7 GB. All of it
resident however far away: streaming the city and blocks in by distance (a cheap silhouette
beyond) is what's left that's big. For iOS (CPU and GPU share one pool; aim under ~1.5 GB a map)
the small maps are close (1.0 GB GPU on Connaught Place); the NCR needs the streaming.

Resident tracks GPU memory (about 1.5 GB of it per GB of GPU memory: the driver's and wgpu's own
host-side state), so cutting GPU memory cuts both.

Then the blocks' and buildings' near-only meshes streamed (crate::buildings::stream): of the 27 M
vertices, 24 M are drawn near only (wall clutter 12.3 M, full blocks 9.2 M, trim 1.8 M, the windows'
faces 0.9 M, plinths 0.4 M). Only the coarse blocks (2.8 M) and the plain walls, roofs and water
stay loaded; the rest is built per 64 m chunk as the camera comes within reach (TRIM_FAR, scaled by
quality, plus 40 m) and freed 60 m past that. The tour's shots are unchanged on both maps. Built on the
main thread at first, a frame that built chunks spent a median 1.9 ms on it (p90 4.8 ms, 17 ms when
the camera landed somewhere new); now they're made in tasks (AsyncComputeTaskPool, six at a time,
nearest first) and only spawned on the main thread: the stream system never took over 0.3 ms in a
two-minute match. (On the browser the tasks still share its one thread, between frames.)

| 6 Nov, streamed | GPU allocated | resident |
|---|---|---|
| Connaught Place | 1.03 GB (all of it in reach) | 2.4 GB |
| NCR | 1.81 GB (mesh 1.11 GB, textures 437 MB) | 3.6 GB |

Left on the NCR's meshes: the coarse blocks (about 180 MB), ground tiles, grass, the street kit,
trees and characters (about 0.9 GB together).

### At ground level (6 Nov)

Seen from the ground (a match recorded with VIVAD_RECORD_CAST, following each body in turn) the
NCR was a desert of paving: the open land between districts (70% of the map, `ground::OPEN`) and
the river sand were drawn with the pavement's slabs and colour. `map_picture` (new: the map from
above, `cargo run --release -p shared --bin map_picture -- ncr out.ppm`) showed how empty it is.

- Open land painted as Delhi scrub (greener low, sun-dried higher, bare earth in patches) with
  grass on it; the river banks as sand.
- 61 villages along the roads (compose::villages): two to four storeys of plaster and brick in rows
  along 4 m lanes, a chowk with a peepal and a trunk (out one match in three). 2,600 houses: GPU
  memory on the tour within its noise (1.9-2.2 GB with and without), frame rate the same.
- Chests drawn by set again (sim::loot::chest_sets, missed in step 2): a match had all three sets
  out, about 370 chests; now about 170.

Then the districts and the match:

- OpenStreetMap has few of an Indian city's buildings (Sector 18 has 77, Cyber City 32), so the
  districts from it were bare paving between their streets: their empty plots are filled
  (compose::infill: three to seven storey blocks squared to the nearest street, big ones first,
  nearest the street first; up to 320 a district) and the four with more in the data keep more of
  them (caps raised). 6,800 buildings in all (2,191 before); GPU memory on the tour 2.0-2.2 GB.
- The ring for 4 km (sim::zone::for_ncr, from the branch): a match of 16 opens on a 2 km disc
  of the map and every ring is planned on land at the start; 32 on 3 km, 64 the whole map. The
  branch's disc at the middle (the Yamuna) is left out. The flight is a chord of that disc
  passing two big districts (flow::ncr_flight).
- Bots only leapt within 230 m of the middle (a small map's rule): on the NCR every bot landed in
  the river. Now each picks a chest a dive from the flight's line, inside the ring, and leaps as
  the flight passes it.

Then the match itself (7 Nov), from the branch: the glider comes back off a tall roof and bots
step off one (`ROOF=1 just sim`: 8 of 16 stranded before, none after); the Yamuna waded at 2.4 m/s;
the flight at 500 m and 60 m/s, leaping anywhere over the map (it was only within 240 m of the
middle, a small map's rule: nobody could leap until the flight ended), the leap a dive; bots look
250 m for a chest.

Left: the camera close behind a warrior by walls; bots take mostly floor loot (1,600 finds),
opening few of the 170 chests.

### Rooms and memory (7 Nov)

- No loot sealed in buildings (221 chests were); the gumti and tanki modelled (159 and 268 were
  colliders with nothing drawn).
- The 573 hollow buildings enterable, on main's renderer: Interior::drawn makes their rooms the
  map's blocks (drawn near only, no sun shadow), the sim collides with each as one compound of
  Interior::solids, the facade is cut round the holes, furniture is the street kit's streamed
  within 50 m. walk_check: 60 of 60 doors in, 43 of 43 first flights up.
- The finer ground streamed near the camera (the NCR's whole 2,031 grid at every LOD was 430 MB).

| NCR, 7 Nov | GPU allocated | resident |
|---|---|---|
| High, before | 2067 MB | 4095 MB |
| High | 1578 MB | 3494 MB |
| Medium | 1328 MB | 3348 MB |
| Low | 1220 MB | 3330 MB |

What's left of it (High): textures 454 MB (model textures uncompressed: characters 98, props 72,
weapons 59, models/textures 115), render targets about 200 MB, blocks about 430 MB (the Delhi maps'
blocks in their far LOD over the whole 4 km, and the near ones streamed), the city 85 MB.

## 4. On the start screen (done, merged to main 7 Nov)

A fifth card, Dilli NCR, desktop only until its memory comes down (about 2 GB of GPU memory; the
iOS target is 1.5 GB a map). Solo only: a room's code names one of the Delhi maps.

## 5. Later, if wanted

Interiors furnished, the market's stalls and parking used in play, vehicles, crowds.
