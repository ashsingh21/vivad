# Far meshes and HLOD: what we can copy (1 Nov 2026)

**Honest answer: no ready far-city system exists for Bevy.** crates.io has no "hlod" crate, and GitHub repo search returned nothing for "bevy hlod" or "bevy impostor". We join small parts.

**Our code already does the first half.** `footprint::solid` extrudes, `build_footprints` merges per chunk, `batch.rs` does `VisibilityRange` tiers, and Bevy 0.20 has `Mesh::merge` (`bevy_mesh` `src/mesh.rs:2208`). The 17 zone files hold 2,331 buildings and 11,415 corners, about 5 each. That is about 30,000 shell triangles by my sum, so shells may not be the real cost. Run `lod_stats` first for the true count.

**Plan:**
1. Far tier, our own code: one mesh per 128 or 256 m cell from `footprint::solid`, vertex colour only, no windows, trims, signs or clutter. Nothing to copy here.
2. If far lanes shimmer or cells stay heavy, join footprints with `geo` (pick 1).
3. Far trees get `bevy_flora` impostors (pick 2). Buildings get none: each is different, so each needs its own atlas tile, while a box is about 12 triangles.
4. Facade rules come from picks 3 and 4. Use meshopt only if something stays too heavy (pick 6).

Hours are my estimates. Dates are the last commit on the default branch. Nothing here was built or run.

## 1. Join footprints into blocks: `geo`, with DTCC's method
- **Repo:** https://github.com/georust/geo, crate 0.33.1, MIT OR Apache-2.0, 2026-11-01.
- **Files:** `geo/src/algorithm/bool_ops/mod.rs` (`unary_union`, line 259), `geo/src/algorithm/buffer.rs` (`BufferStyle`, `LineJoin::Miter`), `geo/src/algorithm/simplify.rs`.
- **Method from:** https://github.com/dtcc-platform/dtcc-core, MIT, 2026-10-30, `dtcc_core/builder/polygons/polygons.py`, `merge_polygons_buffering` (lines 70-100): grow with square corners, union, shrink back.
- **Change:** that code is Python on shapely for two polygons. Write it in Rust on `geo`, for all footprints of one height band in one cell. `geo` is a new dependency. For the browser build, turn off its default `multithreading` feature and test.
- **Saves:** about 12 hours. Skip DTCC's `dtcc_core/builder/cleaning/construction.py`: far too heavy for this job.

## 2. Far tree impostors: `bevy_flora`
- **Repo:** https://github.com/bevy-coven/bevy_flora, MIT OR Apache-2.0, 2026-10-31, on Bevy 0.20. The only Bevy 0.20 impostor code I found.
- **Files in `src/impostor/`:** `mod.rs`, `bake.rs`, `material.rs` (1,275 lines), `impostor.wesl`, `impostor_bake.wesl`, `impostor_common.wesl`, `impostor_prepass.wesl` (487 lines).
- **Copy:** bake 8 yaws into a colour atlas and a surface atlas, draw one quad, let scene lights shade it. Keep its limit of 4 bakes at once: the file says Metal loses the device otherwise.
- **Change:** remove `TreePool` and `FloraUniform`, feed our tree GLBs, copy what the shaders import from `package::shading::common`, and save atlases to files on desktop instead of baking inside the game.
- **Risk:** one camera height (10° default, 60° max) and the quad stays upright, so trees seen from the plane and parachute may look wrong. Test that first.
- **Saves:** about 14 hours, after about 6 hours of porting.
- **Fallback:** https://github.com/robtfm/boimp, MIT OR Apache-2.0, true octahedral with all view angles: `src/bake.rs`, `src/render.rs`, `src/oct_coords.rs`, `src/shaders/`. `master` is 2025-01-08 on Bevy 0.15, and newer branches (to 2026-10-16) need a private Bevy 0.16 fork. Use only if the test above fails.

## 3. Which windows at which distance: OSM2World
- **Repo:** https://github.com/tordanik/OSM2World, MIT (`LICENSE.txt`), 2026-09-09, Java. The fullest OSM building code I found, with five LOD levels.
- **Files under `core/src/main/java/org/osm2world/`:** `world/modules/building/ExteriorBuildingWall.java` (`chooseWindowImplementations` line 464, `placeDefaultWindows`), `world/modules/building/BuildingDefaults.java`, `scene/mesh/MeshStore.java` (`MergeMeshes` line 146, `MoveColorsToVertices` line 321, `ReplaceTexturesWithAtlas` line 379).
- **Copy:** the rule for windows not mapped in OSM: none at LOD0-1, flat texture at LOD2-3, real geometry at LOD4. Also its mesh steps: merge by material, move colours to vertices, use an atlas.
- **Change:** ideas only, Java to Rust. Do not port whole classes: they lean on its own world model.
- **Saves:** about 2 hours.

## 4. Whole windows on every wall: Streets GL
- **Repo:** https://github.com/StrandedKitty/streets-gl, MIT, 2025-08-21, TypeScript. No building LOD or impostor files by name.
- **Files under `src/lib/tile-processing/tile3d/builders/`:** `WallsBuilder.ts` (`getWalls`, `getEdgeSmoothness`, `getWallUVsAndTextureIds` line 324), `roofs/FlatRoofBuilder.ts` (38 lines).
- **Copy:** `windowCount = round(wallLength / windowWidth)`, then stretch, so a wall ends on a whole window and near-straight corners count as one wall. Flat roof UVs follow the oriented bounding box.
- **Change:** port about 60 lines into `crates/game/src/buildings.rs`. Our walls use a fixed 3.2 m bay with no fit, so change the UVs (line 706) and the sills that follow them (lines 741-745) together.
- **Saves:** about 2 hours.

## 5. Near-tier facade generator (not for far meshes): ProceduralToolkit
- https://github.com/Syomus/ProceduralToolkit, MIT, 2024-04-05, Unity C#: `Runtime/Buildings/BuildingGenerator.cs`, `FacadePlanner.cs`, `FacadeConstructor.cs`, `Layout.cs`, `Samples/Buildings/Runtime/ProceduralFacadePlanner.cs`. Takes a polygon and a floor count. I read only `BuildingGenerator.cs` and the file list. Saves about 4 hours if we redo near facades.

## 6. Mesh shrinking, only if needed: meshopt
- https://github.com/gwihlidal/meshopt-rs, crate `meshopt` 0.7.0, MIT OR Apache-2.0, 2026-10-17: `src/simplify.rs` (`simplify` line 77, `simplify_sloppy` line 410), `src/optimize.rs`. It wraps C++, so run it on desktop and ship the result.
- Bevy glue: https://github.com/Aceeri/bevy_meshopt, MIT OR Apache-2.0, 2025-11-25, `src/lib.rs` (`MeshExt`). It is on Bevy 0.17 and meshopt 0.6.2, so copy the file and bump both.
- A box with 5 corners has nothing to collapse, so this is for props, not shells (my judgement, not measured). Saves about 3 hours if needed at all.
- Pure Rust ports `meshopt-pure` and `meshoptimizer-rs` (MIT, latest 2026-10-30) are unread and very new. `awsm-renderer-lod-bake` says MIT OR Apache-2.0 on crates.io, but GitHub shows no repo licence.

## Skip
- **Licence blocks copying (GitHub's label, code not read):** `prochitecture/blosm` GPL-3.0, `tudelft3d/City4CFD` AGPL-3.0, `Oslandia/py3dtilers` LGPL-2.1. `hdrsniped/octahedral-impostor-lod-godot` shows no licence, and a search result says custom terms.
- **Not useful:** `wojtekpil/Godot-Octahedral-Impostors` (MIT, last pushed 2021). `pysal/momepy` `momepy/elements.py` `generate_blocks` (BSD-3, body not read): cells are simpler than street blocks. `tudelft3d/Random3Dcity` (MIT, archived, per search result makes synthetic cities).
- **Gaps:** GitHub rate limits cut some searches, and repo search misses small repos, so one may exist that I did not see. `Geodan/pg2b3dm` (MIT, C#) was fetched but not read.
