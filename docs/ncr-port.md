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

Next for memory: images' CPU copies dropped after their mips are made, and compressed textures
(KTX2); the city drawn near only (its 2 GB of GPU memory); the base.

## 4. On the start screen

A fifth card, desktop only until its memory comes down.

## 5. Later, if wanted

Interiors furnished, the market's stalls and parking used in play, vehicles, crowds.
