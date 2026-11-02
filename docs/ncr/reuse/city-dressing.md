# Reuse: street dressing and road shapes (1 Nov 2026)

Read from source. Nothing built or run. Hours are my estimates. GitHub's rate limit stopped 5 of 18 searches, so a small repo may be missing.

## Short answer
- No Rust or Bevy crate dresses real roads. Arnis and Earth2Road do parts of it, so we copy their rules into `kit.rs`.
- Nothing anywhere covers Indian street life (autos, thelas, chai stalls, wires). That stays ours.
- Do picks 1 to 4, about 17 hours saved. Leave picks 5 and 6 until road looks are the bottleneck.

## What our code lacks today
- `kit.rs` `streets()` walks segment by segment and skips any under 8 m. `street_free` never checks other roads, so nothing keeps a stall out of a junction.
- `tools/osm/ncr_extract.py` line 90 drops lone lamp and pole nodes. Signals and crossings on roads are lost later: roads save only `kind`, `w`, `pts`.
- The 17 `ncr_*.json` files hold 17 mapped trees in total, so street trees must be invented.
- How many lamps and signals OSM holds for NCR is not measured yet. Check before relying on them.

## Picks, best first
### 1. Streets GL: junction shapes. Saves about 8 h
https://github.com/StrandedKitty/streets-gl, MIT, last commit 2025-08-21 (branch `dev`), TypeScript.
- Files: `src/lib/road-graph/`: `IntersectionPolygonBuilder.ts`, `RoadGraph.ts`, `Road.ts`, `Intersection.ts`, `LinkedVertex.ts`, `SegmentGroup.ts`. 526 lines in all.
- Copy: from each arm's point and width it builds the junction polygon, and `getTrimmedEnd()` says where each road stops.
- Change: port to Rust with `glam::Vec2`, swap RBush for a HashMap on our rounded points, feed it `pts` and `w`, use the polygons as no-go zones in `street_free`. It needs 3 or more arms, has a fixed 16 m in `Segment`, and does not merge dual carriageways.
### 2. Earth2Road: parked vehicles at the kerb. Saves about 3 h
https://github.com/Alezx311/earth2road, MIT (`LICENSE`; GitHub's search says "other"), last commit 2026-10-11, Python.
- Files: `akadem_maps/core/yards.py` (kerb loop lines 186-209, `blocked()` line 112), `akadem_maps/core/scene.py` (`offset_line`, line 157).
- Copy: walk a road by real distance, seed by way ID, try offsets from 2.6 to 4.6 m until the vehicle's box clears roads, walls and lots. Each item records the rule and OSM feature behind it. `offset_line` is 30 lines for clean parallels on bends.
- Change: write it in Rust in `kit.rs`, swap sedans for autos, scooters and thelas, keep our rule that old lanes may be parked in. Leave the rest. REUSE.md calls it the best road tool, but its lanes and junctions come from SUMO's netconvert (EPL-2.0 or GPL, off our list).
### 3. Arnis: lamp, road paint and roof rules. Saves about 4 h
https://github.com/louis-e/arnis, Apache-2.0 (keep `NOTICE`), last commit 2026-11-01, Rust. It writes Minecraft blocks, so copy rules, not geometry.
- Files in `src/element_processing/`: `highways.rs` lines 1355-1392, `road_markings.rs` (`build`, line 527), `rooftops.rs`. I read the lamp code, but only the headers and function names of the other two.
- Copy: a lamp every 25 cells, switching sides each time and trying the other side if the spot is road or water. Our poles use one side only. `road_markings.rs` decides where lane lines stop, plus stop lines and crossings. `rooftops.rs` lays roof plant in rows, not scattered.
- Change: cells to metres, blocks to our kit names, roof items to Delhi's.
### 4. OSM2World: which tag makes which object. Saves about 2 h
https://github.com/tordanik/OSM2World, MIT (`LICENSE.txt`), last commit 2026-09-09, Java.
- Files in `core/src/main/java/org/osm2world/world/modules/`: `StreetFurnitureModule.java` lines 75-160, `ParkingModule.java`, `TreeModule.java` (`TreeRow`, line 418).
- Copy: the tag table (lamp, bench, bin, billboard, bus stop, cabinet, hydrant) and defaults such as a 5 m lamp and 0.3 parked density seeded by area ID. `TreeRow` plants along a mapped `natural=tree_row` way and skips spots near a mapped tree.
- Change: make it a small Python table in `tools/osm/delhi.py` using our kit names, and drop what Delhi lacks, such as grit bins. `ParkingModule` only fills mapped `parking_space` areas, so kerb parking still needs pick 2.
### 5. osm2streets: lanes, for later. Saves 40+ h then, nothing now
https://github.com/a-b-street/osm2streets, Apache-2.0, last commit 2025-10-02, Rust.
- Files: `osm2streets/src/render/mod.rs` (`to_lane_polygons_geojson`, line 111), `osm2streets/src/render/output.rs`, `osm2streets/src/geometry/mod.rs` (`intersection_polygon`, line 103), `osm2streets-py/src/lib.rs`.
- Use, do not copy: run it offline per district as REUSE.md says. Set the left-hand side with `override_driving_side` (`streets_reader/src/lib.rs` line 51).
- Change: keep it out of the game. It pulls `geom` and `abstutil` from git, with old `geo 0.27` and `pyo3 0.17`. Our roads are painted into terrain, so nothing can show lanes yet. I did not build it.
### 6. osm-world: road ribbons in plain Rust. Saves about 3 h if we move to road meshes
https://github.com/paulrobello/osm-world, MIT, last commit 2026-07-21, Rust with wgpu and glam, 1 star.
- Files: `src/world/road/ribbon.rs` (174 lines), `src/world/road/cap.rs` (88), `src/world/street_sign.rs` (730). Read in part.
- Copy: `generate_road_with_elevations` (ribbon over per-point heights), `append_road_cap` (round ends), and street signs at junctions plus every 260 m.
- Change: turn its `Vertex` lists into a Bevy `Mesh`. It has no junction polygons, so pick 1 still comes first.

## Also checked
- Crates to add, not copy (crates.io data only): `cavalier_contours` 0.9.0 for parallels on tight bends, `i_overlay` 9.1.0 to join road polygons, `lyon_tessellation` 1.0.23 to stroke lines into meshes. All MIT OR Apache-2.0.
- Never copy: 3DStreet and Streetmix are AGPL-3.0 (Streetmix's older code was BSD-3). ProbableTrain/MapGenerator is LGPL-3.0. blosm shows no licence. 3DStreet's `src/tested/clone-slots.js` looks close by name; I did not open it.
- Bevy `examples/large_scenes/bevy_city` (MIT OR Apache-2.0, main at 0.20.0-dev): a fixed grid with hand-set numbers and no road following, so nothing to copy.
- `TheDuckCow/godot-road-generator` (MIT, last commit 2026-10-12): `addons/road-generator/procgen/intersection_ngon.gd` makes curved shoulders at junctions but is tied to Godot. Header read only.
- `mrclputra/bevy_citygen_v3` (MIT, Bevy 0.16.1, 2025-12-15) is a Voronoi slum generator, maybe for colony infill later; README only. `robick/osm-road-mesh` is for 3D printing. `CottageHop/PolyMap` is a web map renderer. `YusufEminoglu/osm2threejs` and `josauder/procedural_city_generation` (MPL-2.0) were not opened.

## Order of work (prove each step in the game before the next)
1. Keep lamp, signal, crossing and pole nodes through the extractor and into `ncr_*.json`.
2. Port pick 1 and make `street_free` stay out of junctions.
3. Rewrite the `streets()` walk by real distance with rules from picks 2, 3 and 4.
4. Then roofs from pick 3. Keep the licence notice in every ported file.
