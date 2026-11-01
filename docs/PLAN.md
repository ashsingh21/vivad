# Panga: the one plan

**This is the only plan.** Every other roadmap, design doc or blueprint in this repo or in
`~/.claude/plans/` is superseded by this file; they stay only as reference and say so at the top.
Progress is tracked in the checklists here. When a step lands, tick it, note the commit, and keep
going down the list. If something here turns out wrong, change this file, not a new one.

Owner decisions recorded 31 October 2026:

1. **Connaught Place first.** Everything below is built and proven on `cp` before any other map
   is touched. After that the work moves to the other maps. The final map is **NCR, built from
   the current code**, with the current Delhi maps as the Delhi part of it. Details of that phase
   (size, what happens to the old blockout, Bharat) are still open and not decided.
2. **Desktop first.** Realistic graphics arrive as a new **Ultra** quality tier. Phone realism is
   wanted too, but comes later as its own phase. Low/Medium/High stay as they are for now. This overrides the "polished stylised, mobile first" decision in
   `docs/rendering-guidelines.md`, which is updated to say so.
3. Character **look** first (faces, eyes, textures, garments with footwear, hair, eight looks).
   Character motion is a later pass.
   Engine: stay on Bevy for now. The research on phones is summarised in the Phase 3 notes below.
4. Free CC0 assets (Poly Haven, ambientCG, MakeHuman community) are allowed with credit in
   `docs/references.md`. Each download is confirmed with the owner before fetching.
5. The project lives at `https://github.com/Apc0015/Panga.git` from now on (remote `panga`).
   `https://github.com/ashsingh21/vivad.git` stays as `origin` for pulling upstream changes.

Machine this is built and measured on: MacBook Pro, Apple M4, 16 GB, Blender 5.2 LTS at
`/Applications/Blender.app`, Python 3.13 (numpy, PIL, scipy, requests), cargo 1.99, git-lfs.
Not installed: ffmpeg, ImageMagick, wasm-bindgen, any KTX2 encoder. The game currently classes
the M4 as an integrated GPU and runs at Medium, so step 1 of Phase 1 is the first visible win.

---

## Where things stand (31 Oct 2026, evening)

- `main` is upstream `6c0c608` plus the uncommitted NCR merge (Step 0 deferred): `MapId::Ncr`, its street router,
  urban materials and textures, hit-stop, scaled zone, three weapons, street sample, NCR docs.
  Builds clean, shared tests pass, a headless NCR match finishes. The NCR walk check fails 8 of
  10 cases on geometry from its earlier 2.2× scale pass (pre-existing, not from the merge).
- Six maps: `cp` (default), `chandni`, `hauzkhas`, `indiagate` (OpenStreetMap, 500 m, polished),
  `bharat` (ancient India), `ncr` (2.2 km blockout).
- Two characters: MakeHuman bodies, one 2K atlas each, 30k triangles, painted clothes, barefoot.
- Rendering: AgX, bloom, vignette, SSAO on High, TAA, 3 cascades to 200 m, a painted sky dome,
  baked vertex occlusion, 256² generated facades.

---

## Step 0. Repo and baseline (deferred by the owner, do later)

Skipped for now, to be done when the owner says so. Until then work stays uncommitted in the
local tree, nothing is pushed, and `origin` remains the only remote.

- [ ] Add remote `panga` → `https://github.com/Apc0015/Panga.git`; push `main` and its LFS
      objects; set `panga` as the push default (`git push -u panga main`).
- [ ] Commit the work as one or more commits on `main`. The stray `* 2.glb` / `* 2.bin` Finder
      duplicates, `.DS_Store` files and `previews/vivad_current_build.mp4` are left out.

Every later step ends with: build clean with zero warnings, its checks green, captures compared
against the baseline. Once Step 0 is done, each step also gets one commit on `main`, named after
the step and pushed to `panga`.

---

## A. Map work (A1 and CP navigation in Phase 1; the rest is a Phase 2 draft, to be rewritten)

### Layout (x east, z south, `HALF = 1000`)

| Piece | Source | Placement |
|---|---|---|
| Connaught Place | `delhi_cp.json` + `cp::build` ring | offset (0, 0) |
| Chandni Chowk | `delhi_chandni.json` | offset (−60, −600) |
| India Gate | `delhi_indiagate.json` + `india_gate()` | offset (+430, +600) |
| Hauz Khas | `delhi_hauzkhas.json` | offset (−470, +600) |
| Yamuna | new `River::Sine`, x ≈ 700 + 30·sin(z/350), 70 m wide | three at-grade causeways at z = −600, 0, +600 |
| Noida/Ghaziabad towers | `ncr::tower`/`pavilion` grids from `noida()`, `ghaziabad()`, built 1:1 | x 800..960, z −700..+700 |
| Gurugram towers | from `gurugram()` | x −960..−780, z −150..+350 |
| Infill | Lutyens-style plots / bungalow lots, 40–60 m, ≤ 1500 footprints total | the gaps |
| Connectors | painted ROAD polylines (16 m) along inter-district axes + ring road at ±900 | ground classes, dressed by `kit::streets` |

Tiles stay at real scale, yaw 0, each clipped to |local| ≤ 250 so skyline-only buildings and
over-run infill lots never overlap a neighbour or a connector.

### Mechanism

- New `crates/shared/src/map/dilli.rs`, `pub fn generate(seed) -> MapLayout`. `delhi.rs` is
  refactored so its stages take a tile and a frame instead of the `HALF` constant:
  `Tile { data, offset: Vec2, clip: 250, id }`, `Frame { half }`. `delhi::load(id) -> Data`
  (the `include_str` match) + `Data::shift(offset)` translates every pts/x/z once, drops
  buildings/trees/pois whose centroid is outside the clip square, clips road polylines at the
  boundary. `paint_ground`, `water`, `footprints`, `trees`, `street_loot`, `boundary`, `open`,
  `enterable` then work in world coordinates. `delhi::generate` = `Tile{offset 0}` +
  `Frame{half 250}` so the four small maps stay identical (a test on block/footprint/chest counts
  recorded before the refactor proves it).
- `paint_ground` accumulates: the caller owns the `vec![PAVEMENT; n*n]`; dilli paints areas for
  every tile, then connectors, then roads for every tile.
- `cp::build` runs with `ring(cp_offset)`; `india_gate()` with its tile offset. `MAX_HOUSES`
  becomes an argument: Chandni 60, Hauz Khas 40.
- Loot: per-tile chest odds stay (~110/tile → ~440); `street_loot` targets ×4 (280 chests, 360
  floor loot) sampled only on PAVEMENT/PARK/PLAZA cells.
- Towers: `ncr::tower`, `ncr::pavilion` become `pub(super)` and are called with world frames. No
  `scale_horizontal`, no NCR block roads, no metro trench, no deck bridges.
- Terrain (`terrain.rs`): add `gn` (ground samples per side) so `ground` is 1 m (`gn = 2061`,
  4.2 MB u8) while `h`/`road_d` stay `n = 513` (4 m). `ground_at`, `paint_ground`,
  `settle_under` use `gn`; existing maps set `gn = n`.
- River / style fields replace the string check. In `mod.rs`:
  `enum MapStyle { Bharat, City, Ncr }` and
  `enum River { None, Straight{x0,x1}, Sine{x0,amp,wavelength,half_width} }` with
  `River::distance(p)`. `river: Option<(f32,f32)>` → `river: River`; `is_ncr()` deleted.
  Call sites: `audio::river_distance`, `skyline.rs`, `ambient.rs`, `grass.rs`, `visuals.rs`,
  `camera.rs`, `sim/nav.rs`. Dilli is `MapStyle::City`, so `paint_city`, kit, skyline ring and
  the kites apply.
- Yamuna in City style: channel cells painted WATER and sunk to −1.4 except where a ROAD class
  was painted (the causeways stay at 0 with 1 m parapet blocks); the surface is
  `Footprint { mat: Water }` strips of 50 m split at the causeways.

### Navigation (`crates/shared/src/sim/nav.rs`)

Keep 1 m cells (Chandni's lanes are 2.5 m) but make the grid chunked, lazy and class-seeded:
`free: Vec<u8>` Unknown/Free/Blocked seeded from `terrain.ground` + footprints; sample up to 2
64 m chunks per tick, prioritised by distance to bots and pending goals; hierarchical A*: a
coarse 16 m grid (125²) built once from classes and footprint coverage, then fine A* between
coarse waypoints within the existing 40k budget, Unknown treated as free. Delete
`map/navigation.rs` (`StreetNav`), its `insert_resource` in `sim/mod.rs`, the `street` param
and its filters in `bots.rs`. NCR then also runs on NavGrid.

### Pacing, identity, rooms

- `Zone::for_map(map, seed)` with a per-map phase table: at half 1000 (scale 4) start radius
  1440 m, `PHASES[0].0` 40 s, `[1].0` 30 s, total ≈ 680 s; zone test extended with half 1000.
- `vimana_speed(half) = 22·sqrt(half/250).clamp(1,2)` → 44 m/s, chord ≈ 47 s.
- `MatchConfig::default_bots(map)` → 36 on Dilli, 15 elsewhere.
- `MapId::Dilli` appended last in `ALL` (ticket-stable index); `#[default]` moves to Dilli;
  `name "dilli"`, `title "Dilli"`, `parse` adds "delhi". `MapId::DELHI` stays the four tiles;
  new `MapId::ROOMS = [Dilli]` for `room_map`/`new_room_for`; `rotation(Dilli) = &[Dilli]`.

### Performance estimate (seed 7)

~2.4k footprints and trimesh colliders, ~40k blocks → 3–5k chunk meshes, ~6k kit pieces,
≤ 1500 trees. No streaming: `end_margin` on far facade LOD and kit last LOD at 900 m, trees
600 m. A 32 m spatial bucket for footprints/kit in `open()`/`street_free()` if generation
exceeds ~2 s. Connector segments ≤ 50 m.

### Checklist

- [ ] **A1 Style/river refactor** — `MapStyle`, `River`, `Terrain.gn`, delhi.rs `Tile`/`Frame`,
      count test. No visible change.
- [ ] **A2 Dilli skeleton** — four tiles, boundary, scaled loot, Yamuna + causeways, behind
      `VIVAD_MAP=dilli`; bots on the existing NavGrid.
- [ ] **A3 Navigation** — chunked lazy NavGrid + coarse A*, StreetNav deleted, `sim_check` green.
- [ ] **A4 Frame content** — connectors, ring road, infill, tower districts, trees, POIs,
      `crates/shared/src/bin/dilli_walk_check.rs` (causeways, a Chandni house door → roof, CP
      ring stairs, Hauz Khas tank wade, a Noida tower stair, India Gate plaza).
- [ ] **A5 Default + pacing** — Dilli default, rooms, zone/vimana/bot tuning.
- [ ] **A6 Perf pass** — culling ranges, caps, `VIVAD_TRIS`/`just perf` numbers, `docs/dilli.md`.

---

## B. Desktop Ultra render tier (Bevy 0.20.0)

### Verified in Bevy's source

- `Bloom` requires `Hdr` and `camera.rs::post()` always inserts Bloom, so the main pass is
  already Rgba16Float; the Rgba8 view image in `quality.rs` only receives the tonemapped result.
  Keep that image path (HUD, `ViewScale`, tour captures depend on it).
- Atmosphere: `bevy::light::Atmosphere::earth(Handle<ScatteringMedium>)`,
  `ScatteringMedium::earth(..).with_density_multiplier(..)`, camera `bevy::pbr::AtmosphereSettings`,
  `AtmosphereEnvironmentMapLight` on the camera auto-generates an environment map. Commit
  `3491c64` shows the exact lines it replaced.
- `VolumetricFog`/`FogVolume`/`VolumetricLight` are in `PbrPlugin`. `ScreenSpaceReflections`
  only shades pixels drawn by deferred opaque materials. `ScreenSpaceAmbientOcclusionQualityLevel::Ultra`
  and `ShadowFilteringMethod::Temporal` exist; PCSS is behind cargo feature `experimental_pbr_pcss`.
- `AutoExposurePlugin` is not in `PostProcessPlugin`; add it in `main.rs` natively.
- `StandardMaterial` has `diffuse_transmission`, `thickness`, `clearcoat*`, `anisotropy_*`,
  `specular_tint`, `depth_map` + `parallax_*`. KTX2 with zstd loads by default (raw BC blocks).
- Meshlets and `IrradianceVolume` are skipped (no fit, no baker).

### Tier model (`crates/game/src/quality.rs`)

`Quality::Ultra`: native only (`next()` skips it on wasm; browser settle caps at High).
`choose()`: discrete GPU → Ultra; integrated GPU on the Metal backend (Apple Silicon) → Ultra;
other integrated → Medium. Values: `max_rows None`, SSAO `Ultra` radius 0.9, shadows
(4 cascades, 420 m, 4096) with first cascade to 14 m, `ShadowFilteringMethod::Temporal`,
TAA + CAS 0.3, `lod_scale 1.5`, grass 1.0, far-LOD shadows on, far plane 3000. `apply()`
calls a new `crate::ultra::apply(..)` in `crates/game/src/ultra.rs`; `pub fn is_ultra()`.

### Lighting (`ultra.rs`, `visuals.rs`, `sky.rs`, `camera.rs`, `lamps.rs`)

- Atmosphere on Ultra only: `Atmosphere::earth(media.add(ScatteringMedium::earth(..)
  .with_density_multiplier(1.6–2.2)))` tagged `UltraOnly`; camera gets `AtmosphereSettings
  { aerial_view_lut_max_distance: 3000.0, .. }` + `AtmosphereEnvironmentMapLight { size 256 }`.
  Removed when the level drops.
- `sky.rs`: hide the `SkyRig` dome while Ultra (sun disc stays). Drop NCR's hemispherical
  `EnvironmentMapLight` on Ultra; `GlobalAmbientLight` 1800 → ~300.
- Day/night: `visuals::daynight()` true on Ultra; `update_sun` keeps ~100k lux × elevation with
  a white colour; `camera::update_fog` and `DistanceFog` skipped on Ultra. `VIVAD_HOUR=` pins a
  time for captures.
- Lamps at night: shadow maps on the ≤ 6 nearest lamps within 35 m, `PointLightShadowMap 1024`.
- Volumetric haze: `VolumetricFog { step_count 48, jitter 0.3 }`, `VolumetricLight` on the sun,
  one `FogVolume` 600×60×600 m, density tied to `TimeOfDay`. Perf-gated, last.
- PCSS: `experimental_pbr_pcss` forwarded through a `pcss` feature; `soft_shadow_size: Some(1.5)`.

### Materials and textures (`buildings.rs`, `visuals.rs`, `water.rs`, `tools/texpack/`, `tools/textures/`)

- Facade PBR sets `assets/textures/facades/<kind>_{basecolor,normal,orm}` at 2048² for
  `plaster_old, plaster_painted, brick_exposed, concrete_rcc, shopfront` from CC0 sources.
  `tools/textures/facades.py` (numpy/PIL) composites window/AC/balcony decals, dirt streaks and
  roughness variation. Ultra branch in `build_footprints`' material assignment uses them with
  normal/ORM/occlusion + `depth_map`, `parallax_depth_scale 0.03`, keeping the 2 m window tile.
- World-UV block sets: `_2k` KTX2 variants chosen by path suffix in `load_set`/`load_urban` on
  Ultra; `mip_chain` bypassed when the image already has mips.
- Compression: `tools/texpack/` (standalone Rust bin, `intel_tex_2` BC7/BC5 + hand-written KTX2
  container with zstd), recipe `just textures-pack`. ASTC for mobile later via the same tool.
- Glass on Ultra: `metallic 0`, `reflectance 0.9`, `roughness 0.08`, deferred. Water on Ultra:
  opaque, deferred, roughness 0.08, reflectance 0.6; camera `ScreenSpaceReflections
  { max_perceptual_roughness 0.3–0.5 }`. The blend path stays for other tiers.

### Vegetation, props, characters (shader side)

- Tree leaves in `resolve_gltf_materials`: `Mask(0.5)` + TAA, `diffuse_transmission 0.35`,
  `thickness 0.05`, double-sided.
- Street kit re-exported from `tools/blender/props/street_kit.py` with 3 materials (painted
  metal, tarp, rubber); `street.rs::build_kit` merges per material on Ultra.
- `rim.rs::convert` builds Ultra variants by glTF material name: skin `diffuse_transmission
  0.15`, `thickness 0.02`, rim 0.22 → 0.10; hair `anisotropy_strength 0.6`, double-sided; eye
  cornea (once C2 splits it) `clearcoat 1.0`, `clearcoat_perceptual_roughness 0.05`; cloth
  `specular_tint`, roughness 0.85.

### Post (`camera.rs`, `main.rs`)

Ultra: `Tonemapping::TonyMcMapface` (AgX stays on lower tiers), grading saturation 1.05 /
contrast 1.0 / highlight gain 1.0, `Bloom 0.05`, `Vignette 0.12`, `AutoExposure { range
9.0..=12.5, speed_brighten 2.0, speed_darken 1.0 }` with `AutoExposurePlugin` added natively
(`VIVAD_EV` still overrides). Optional behind env: `VIVAD_DOF=1` (bokeh while aiming),
`VIVAD_CA=1`. No motion blur.

### Performance plan (M4, 1512×982 window, 60 fps)

Baseline with `VIVAD_QUALITY=high just perf` + `VIVAD_PERF_ALL=1`. Add one feature at a time,
each with a kill switch (`VIVAD_NO_ATMO`, `VIVAD_NO_VOLUMETRIC`, `VIVAD_NO_SSR`,
`VIVAD_NO_PCSS`, existing `VIVAD_NO_SSAO/SHADOWS`). Expected: atmosphere ~1 ms, 4×4096
cascades +2–4 ms (biggest risk; fallback 4×2048 or 300 m), SSAO Ultra +0.8 ms, SSR 1–2 ms,
volumetric 1.5–3 ms, lamp shadows ≤ 1.5 ms. Drop order if over 15 ms on `ch_lane`,
`cp_street`, `cp_aerial`: volumetric → PCSS → 4096 maps → lamp shadows. Acceptance: `just perf`
≥ 60 fps median on every view with vsync off; `just flicker` not worse than High.

### Checklist (each: `VIVAD_QUALITY=ultra VIVAD_CLEAN_CAPTURE=1 just tour`, `just shots-diff`, a sheet against `previews/baseline_high`)

- [ ] **B0 Baseline first** — before any B1 change: `VIVAD_QUALITY=high VIVAD_CLEAN_CAPTURE=1
      VIVAD_TOUR=previews/baseline_high cargo run --release -p game --bin game`, plus `just perf`
      numbers, kept for every later comparison.
- [ ] **B1 Tier plumbing** — `Quality::Ultra`, selection, shadows/SSAO/LOD/far plane/grass,
      `ultra.rs` skeleton, docs. Parity with High plus crisper shadows.
- [ ] **B2 Atmosphere** — env map, dome hidden, day/night on Ultra, `VIVAD_HOUR`; captures at
      8:00, 13:00, 18:30, 22:00.
- [ ] **B3 Post** — tonemapper, auto exposure, bloom/vignette retune (immediately after B2).
- [ ] **B4 Materials** — texpack, 2k KTX2 sets, facade sets, glass, water + SSR.
- [ ] **B5 Vegetation, kit, character materials, lamp shadows, PCSS.**
- [ ] **B6 Volumetric haze** (perf-gated).
- [ ] **B7 Perf pass + docs** — `docs/rendering-guidelines.md` owner decision (dated), Ultra row
      in the tier table, `Quality::Ultra` in §7, appendix status; `docs/testing.md` env switches.

---

## C. Character look

### Verified state

Two characters (`tools/blender/characters/human.py` `VARIANTS`), MPFB bodies with
`eyes/low-poly`, no teeth, one joined mesh + one 2048² atlas baked by `finish.py`, LOD0 capped
at 30k, hair cards without `_LOD` names. `crates/game/src/characters.rs::dress_warriors` picks
`look % 2`, three hard-coded ranges, `RimTarget` on every part; `rim.rs::convert` caches by
material id. The MPFB data already installed under
`~/Library/Application Support/Blender/5.2/extensions/.user/blender_org/mpfb/data` holds
high-poly eyes, `teeth_base`, `tongue01`, 10 hair styles, 12 brows, young/middle-age/old skins,
`clothes/shoes01–06`, the full target library.

### Budgets

| Tier | LOD at 0 m | Tris (body+head+outfit) | Hair |
|---|---|---|---|
| Ultra | LOD0 | 45–60k (head 15–20k) | all cards |
| High | LOD1 | ~25k | all cards |
| Medium | LOD2 | ~12k | half cards |
| Low | LOD3 | ~5k | scalp cap only |

Switch distances stay 16/50/400 m; LOD0 shows only on Ultra inside 16 m. Textures: head 4096
(Ultra) + 2048 default, body 2048, outfit 2048 + 1024 tint mask, hair 1024 per style, eyes/mouth
512. Characters move from `.glb` to `.gltf` + shared `../textures/` (existing
`export._export_gltf_shared()`): ≈160 MB in LFS, ≈45 MB loaded per match worst case.

### Face (`human.py`, `finish.py`, new `tools/blender/characters/head.py`)

- `build_body()` uses `eyes/high-poly`; teeth and tongue as before.
- `head.py::subdivide()` (as built, replacing the shrinkwrap design, which pulled lips and lids onto
  the wrong sheet of a surface that runs into the mouth and sockets, and left 31k triangles): one
  Catmull-Clark level on the head with the neck ring pinned and the cage pre-corrected so the surface
  still passes through the proxy's vertices; the body's side of the ring is split to match, so the
  seam still welds. `finish.skin_lod` then cuts LOD0's head to 18k, holding lids, lips and nose
  (`detail`, `face.regions`). LOD1 down are cut from the subdivided head.
- `head.py::eye_parts()` → `eyes` (ball and iris, front only, 512 bake, lid shadow painted in) +
  `cornea` (front cap of MPFB's transparent shell, roughness 0.02, alpha 0.08, Blend, unbaked, LOD0-1).
  Mouth = teeth + tongue, 512 bake, dropped from LOD2+.
- `finish.head_material()` + `body_material()`: pores, creases, lips (the basemesh's own `lips`
  group), blush, sockets, T-zone shine and age-scaled creases, from vertex-attribute masks painted by
  `face.regions()` from `face.Face` landmarks.
- 4K head: baked at 4096; the GLB carries 2048; colour and normal go through
  `tools/textures/character_pack.txt` (`just characters-pack`) to `assets/textures/characters/*.ktx2`.
  `rim.rs` finds heads by glTF material name (not extras) and swaps once loaded, on Ultra only.
- `face_lab.py` has `look=`, `age=`, `eyes=`, `skin=`, `subdiv=`, `--sheet`; `just face-lab`.
- Not built yet: `skin_mask` (fourth bake; the engine has no use for it before the B5 skin variant).
  Full beard, earrings, nose pin and `FACES` per look wait for the eight looks (step 8).
- Cornea clearcoat (section B) is out: the lids don't shade the sky reflection, it greys the eye.

### Hair

MPFB card hair, upgraded: per-style 1024 basecolor tinted per look + baked strand normal with
AO roots; a scalp cap shell baked into the head set (the whole hair at LOD2+);
`{v}_hair_LOD0/1`, brows/lashes `_LOD0` only; `face.trim_hair()` generalised under any
`hair_replace` item. Styles: male `short02`, `short01`+beard, `short04`; female `long01`,
`braid01`, `ponytail01`, `bob02`.

### Garments and footwear (`garments.py`, `outfits.py`)

- `sneakers()` from `clothes/shoes01|02`; `chappal()` and `jutti()` from foot shells + straps;
  origin moves to the sole (`SOLE` constant shared with `anims.py` foot IK: the one motion
  change this pass forces).
- `garments.drape()`: Blender cloth sim (40 frames, pinned at waist/shoulders) applied and
  re-weighted for hems, dupatta, kurta skirt. Fabric weave normals from CC0 textures under
  `tools/blender/textures/cc0/`, procedural fallback. Wear via curvature.
- Outfits: `male_street`+sneakers+watch, new `male_kurta` (kurta, pyjama, chappals, kada),
  `female_street`+juttis+earrings+bangles, new `female_saree` (from `female_warrior` nauvari
  pieces, chappals, bindi).
- Tint mask: bake `tint_mask` (R primary, G secondary, B accent), desaturate masked base
  colour; `COLOURWAYS` per outfit (3 × 3 colours).

### Looks

`sex = look % 2; face = (look/2) % 2; outfit = (look/4) % 2; colourway = (look/8) % 3`, in
both Python and `characters.rs` (the sim knows nothing). Files
`assets/models/characters/{sex}_{face}_{outfit}.gltf`; `just characters` builds all eight,
`just character look=5` one. One rig and one `{sex}_anims.glb` per sex serve all looks.

### Engine (`characters.rs`, `rim.rs`, `assets/shaders/rim.wesl`, `quality.rs`)

- `look_parts(look) -> Look { file, colourway }`; models loaded lazily per file.
- Four LOD ranges plus `Quality::character_lod0()` (Ultra only); cards get ranges from `_LOD` names.
- `Rim` grows `tint: [Vec4; 3]` + optional `tint_mask`; the shader multiplies base colour by
  `mix(1, tint_i, mask_i)`; `Converted` keyed by (material, colourway); `RimTarget(u8)` carries
  the colourway. Mask/skin-mask/`head_4k` handles come from glTF material extras resolved by a
  `characters::material_extras` system; Ultra swaps in the 4K head textures.
- Previews: `just looks` (portraits + full body → `previews/characters/looks/`),
  `just look-shots look=k` (in-game at `CAMERA_DISTANCE`, High and Low for the LOD check).

### Checklist

- [ ] **C1 Split pipeline + engine consumption** — separate head/body/outfit/eyes/mouth meshes
      and materials, `.gltf` shared textures, four LODs, card LODs, lazy models, quality-gated
      LOD0, extras plumbing. Same two looks; no regression in game.
- [x] **C2 Face** — high-poly eyes/cornea, teeth, subdivided head, skin materials, 4K head, face lab.
- [ ] **C3 Hair** — card bakes, scalp cap, LODs, trimming.
- [ ] **C4 Garments** — footwear, drape, fabric normals, new outfits, accessories, tint mask + engine tint.
- [ ] **C5 Looks** — matrix, eight files, `just looks`, `VIVAD_LOOK` sweep.
- [ ] **C6 Docs/budgets** — `docs/references.md` §Characters, guideline budget row, CC0
      credits, LFS audit, `tools/check_characters.py`.

---

## Delivery order

### Phase 0: a playable prototype (owner decision, 1 Nov 2026 — current focus)

**Decision.** Phase 1 pauses after step 5 (C2 of step 6 is landed and stays). The next work is a
small match that is fun to play against bots on Connaught Place, at the graphics the game already
has (High/Ultra as built). Hair (C3), clothes and looks (C4, C5), final polish (B5–B7) and
NCR wait until a stranger has played the prototype twice.

**Done means.** One match on `cp`, 15 bots, start to result screen and back to the lobby, about
8 minutes long, where most deaths come from fights, not from the Aandhi, and the player wants to
go again. Measured by `just sim` (`SIM_SEED=7`, 5 matches) and by playing it by hand.

| # | Step | What it is | Done when | Done |
|---|---|---|---|---|
| P1 | Bots that walk the city | Done 1 Nov. The grid's probe was narrower than the warrior (0.6 m against 0.76 m), so "free" cells beside walls and in doorways trapped bots, and a path could start in the room beyond a ledge's wall. Now half-metre cells, a 0.8 m probe, A* with a closed set, a 1.4× heuristic and reused scratch memory, and paths start in the bot's own cell. `VIVAD_MAP=cp VIVAD_ROTATE=0 SIM_SEED=7 just sim 5`: 5 of 5 finish, 15 weapon kills and 0 Aandhi kills every seed (was 13.6 and 1.4); 2–3 searches a second in a match, mean 0.36 ms, worst 10.5 ms; `nav_check` 128/128 ground and 26/26 upstairs chests (was 20/26). Seen in the real game: bots fight under the colonnade and on the plaza; `net_check` 3 peers 0 failed | [x] |
| P2 | Match pacing | Done 1 Nov. Half the lobby used to die in cricket-bat brawls in the first 80 s: bots glided onto the same street cells, looted their way to the same chests and duelled whoever passed in a colonnade. Now a bot leaps for a landing no one else is making for, loots for 75 s (30 s sharp) fighting only whoever hits it, leaves chests others stand by, and below 40 health runs for the far side of a block unless the ring is at its back or under 60 m; the ring's waits and shrinks total 520 s (was 340). Five seeds: mean 460 s (445–478; was 293), 15 weapon kills and 0 Aandhi kills each, every match ending in phase 3 or 4 with the last two inside a 27–30 m ring on walkable ground; first death at 105–125 s; 22–42 chests opened (was 3–17). Real game: 16 alive at 100 s, 15 at 160 s; `net_check` and `mesh_check` 0 failed | mean length 420–540 s over 5 seeds; final circle lands on walkable ground every seed | [x] |
| P3 | Weapon feel | Built 1 Nov, awaiting the owner's hands. The six guns already had flash, tracers, sparks, a hit marker and sounds; what they lacked: the Vajra-9 and Trishul Carbine played a bow twang (now the tamancha's shot pitched up and down), no gun had spread bloom so the Vajra-9 was a 150-damage-a-second laser (now each shot opens the cone: Vajra-9 0.45° up to +3°, tamancha 0.3° to +1.8°, carbine 0.7° to +2.4°, closing at 2.5°/s; the reticle shows it; bots suffer it too), the camera kick was one formula (now per gun: the katta and dunali wrench the view, the Vajra-9 chatters, the tamancha taps), and a kill had only a sound (now red ticks, larger, held half a second). Vajra-9 damage 18 → 16. Time to kill on 100 health: bat 2.0 s, katta 2 shots or 1 to the head, tamancha 1.2 s, dunali 2 shots point-blank, gulel 2 charged shots or 1 to the head, Vajra-9 0.7 s before bloom, carbine 2 bursts; Kavach up to doubles it. Bot aim untouched: `VIVAD_BOT_SKILL=chill` (default) and `normal`. Five seeds still 434 s mean, 0 Aandhi deaths; checks green. `just clip` + `tools/clip.py` make the two-minute animation (`previews/cp_clip/clip.webp`, HUD-less from the 3D view since the game window was hidden). Found on the way: the last two could burn in one tick and nobody was crowned; the last to fall is now the winner | played by hand on `cp`; a 2-minute clip for the owner | [~] |
| P4 | Loot that moves people | Done 1 Nov. Bots opened 15-28 % of chests because floor finds within 45 m outranked every chest, a calm bot's 3-9 s rest was re-applied every tick it stood at a goal (a quarter of the loot window standing), the ring's "must move" test flipped on and off as a bot walked so it dithered instead of looting, a bot's own goal counted as claimed, and some landed on the ring's roof and then on its 0.65 m ledge, where the footing test saw a free cell through the 0.3 m wall. Now: a find at the feet (14 m) first, then the nearest chest, then finds; goli only under 120; one short rest per chest and none at finds; a jog between chests; no upstairs urns during the grace; ring decision kept once made (30 s slack); landings aimed a few metres off the chest; footing must be reachable without crossing a blocked cell. Map: the 5 chests inside staircases removed, every ring chest turned to face its shop front (spill lands toward the looter; unit test), floor finds 154 → 102 spots (verandah 30 → 20 %, open 90 → 60, park 4 → 8), 147 chests at seed 7. Five seeds: chests 44-63 % (was 15-28), all 16 armed by 90 s and 14-15 by 60 s, never 0, mean length 436 s | ≥ 40 % of chests opened per headless match; no bot without a weapon after 90 s | [x] |
| P5 | Front door | Start, pick `cp`, drop, play, die or win, result screen, play again, with no crash or dead end. (The black `VIVAD_SHOTS` frames seen on 1 Nov were shots of a sleeping display, not the game: capture recipes now run under `caffeinate`) | one full loop recorded; `cargo check` clean | [ ] |
| P6 | Playtest | Three people who have not seen the game play two matches each; their notes go under `docs/playtests/` | the notes exist and the next phase is chosen from them | [ ] |

Rules for Phase 0 are the Phase 1 working rule: one step at a time, build with zero warnings,
checks green, run in the real game, images or clips to the owner, then on to the next step. Step 0
(commit and push) stays deferred to the owner but is recommended before P1.

**Holy sites (owner, 1 Nov 2026).** Real places of worship and graves stand on the Delhi maps. Their on-screen names are gone and no loot lands near them (`holy_sites` in `crates/shared/src/map/delhi.rs`, pinned by a test). Still open before P6: stopping players and bots landing on them, a review by local Sikh, Muslim, Jain and Hindu players, and whether Chandni Chowk stays in the rotation. Old Delhi has more shrines than any source lists, so that site list will never be complete without a local check.

### Phase 1: Connaught Place (paused after step 5; resumes after Phase 0)

Every step is built and judged on `VIVAD_MAP=cp`. The game stays playable after each step.

**Working rule (owner, 1 Nov 2026): one step at a time, no mistakes.** A step is done only when it
builds with zero warnings, its checks pass and it has run in the real game. Before/after images
then go to the owner and the next step starts without waiting for a reply, continuing down this
plan until Phase 1 is finished. Stop and ask only for a texture download, a feature cut, or a
destructive action. Steps are not run side by side. Speed and
screenshot measurements are taken with nothing else running on the Mac.

| # | Step | Plan items | What you see on Connaught Place | Done |
|---|---|---|---|---|
| 1 | Ultra tier | B0, B1 | Done 1 Nov. Crash fixed; Ultra measured at 1080 rows, 3 cascades to 250 m at 2048, SSAO Medium, grass 0.5, LOD 1.0 | [x] |
| 2 | Real sky and day/night | B2, B3 | Done 1 Nov. 50 to 62 fps on `cp` at 09:00. Night sky still needs tuning (red horizon at 22:00), carried to step 9. Auto exposure is opt-in (`VIVAD_AUTOEXP=1`) | [x] |
| 3 | Map code groundwork | A1 | Done 1 Nov. `MapStyle` and `River` replace the name check; a fingerprint test pins all five maps | [x] |
| 4 | Character pipeline rebuild | C1 | Same two people, now separate head, body, outfit and eyes, 4 LODs | [x] |
| 5 | Realistic materials | B4 | The white colonnades, roads and glass stop looking painted | [x] |
| 6 | Faces and hair | C2, C3 | Stopped part way through C2 on 1 Nov (owner). Faces, eyes and teeth are in the character files and work at Ultra and Low, but the three planned reviews did not run. Hair (C3) not started | [ ] |
| 7 | Bot navigation | CP part of A3 | Bots use the ring, stairs and radial roads and reach the safe zone | [ ] |
| 8 | Clothes and eight looks | C4, C5 | Shoes, draped cloth, 8 distinct warriors | [ ] |
| 9 | Final polish | B5, B6, B7, C6 | Trees, street props, lamps, haze; 60 fps on the M4; docs updated | [ ] |

Measure frame rates on mains power with Low Power Mode off: on battery in Low Power Mode, Ultra falls from about 50 to 70 fps to about 30 to 38 on every view.

Carried into step 9 from step 5: the ring's shop arches and upper windows are empty openings (no glass, frames or shop interiors); building colours are still flat pastels; room shade is too blue; the cartoon clouds now stand out.

Notes for Phase 1:

- Step 5 starts with the surfaces Connaught Place actually uses: white plaster and colonnades,
  asphalt, pavers, shop glass, park ground. Other facade kinds wait for Phase 2.
- Step 7 is about behaviour on this map, not 2 km scale. A headless match on `cp` (seed 7, 15
  bots) ended with 8 of the 15 deaths from the Aandhi and 7 from weapons, so bots are losing to
  the zone more than to each other. The chunked, hierarchical navigation waits for Phase 2.
- Step 3 is limited to `MapStyle` and `River`, replacing the string check. The audit (1 Nov) found
  `Terrain.gn` never reaches the screen and a 250 m tile clip cannot leave the small maps identical,
  so `gn` and `Tile`/`Frame` are out of Phase 1 and must be redesigned for Phase 2. `dilli.rs` is not
  started. Audit findings for later steps: SSR cannot reflect the forward-rendered city (step 5 uses
  the environment map), and the head subdivide and cloth drape designs need rework (steps 6 and 8).

### Phase 2: the other maps, building NCR (after Phase 1)

Bring the Connaught Place standard to the other Delhi districts, then build NCR from the current
code with them as its Delhi part: map composition, navigation at scale, frame content, default
map and pacing (Section A: A2, the rest of A3, A4, A5, A6). Section A was written for a
stand-alone `Dilli` map beside the old NCR map, so it must be rewritten before Phase 2 starts.

Open decisions for Phase 2: the final map size, how much of Gurugram, Noida and Ghaziabad are
full districts, what happens to the old NCR blockout and to Bharat.

### Phase 3: phones (after Phase 2)

Research summary: realistic phone shooters are built as a separate phone version with baked
lighting, phone-specific characters and foliage, heavy texture compression and heat-based quality
scaling. No realistic 3D shooter shipped on Bevy for phones was found, so start with a 2 to 4 week
test scene on Mali, Adreno and iPhone hardware before committing. If it fails, reconsider the engine.

External downloads (CC0 facade/fabric textures) are each confirmed with the owner before
fetching, stored under `tools/blender/textures/cc0/` or `assets/textures/facades/`, and credited
in `docs/references.md`. Overpass is not needed: the four district JSONs are already in the repo.

## How each step is run

Each step runs as a workflow: parallel implementers per independent file group, then an
adversarial verify stage (build, `cargo test -p shared`, `sim_check`, walk check, tour captures
compared against the baseline) before the step is accepted. The owner reviews the captures
between steps. Commits and pushes start once Step 0 is done.

## Verification (end to end)

- `cargo check --workspace --all-targets` clean, zero warnings, after every step.
- `cargo test -p shared`: delhi refactor count test, `dilli::tests`, zone test at half 1000,
  NCR tests still green.
- `VIVAD_MAP=dilli cargo run --release -p shared --bin sim_check -- 3 36`: matches finish, nav
  grid ≥ 35 % walkable and ready within 20 s, no fall deaths.
- `crates/shared/src/bin/dilli_walk_check.rs`: all cases pass.
- `VIVAD_QUALITY=ultra just perf`: ≥ 60 fps median on every tour view at 1512×982, vsync off;
  `just flicker` no worse than High.
- Character files pass `tools/check_characters.py`; `just looks` shows eight distinct
  warriors; in-game captures at `CAMERA_DISTANCE` on High and Low confirm the 16 m crossfade.
- Browser build (`just web serve=no`) still compiles with Ultra excluded.
- Sheets in `previews/ultra_m<N>.png`, `previews/dilli/`, `previews/characters/looks/`.

## Known trade-offs

- Until Phase 3, Ultra is desktop only. Low/Medium/High keep the stylised dome, baked AO and 256² facades.
- Section A's causeway Yamuna and its layout are proposals for Phase 2, not decisions.
- Eight looks share one rig per sex, so faces vary by targets and skin, not by body shape.
- Volumetric haze and PCSS are the first things cut if the M4 misses 60 fps.
- Character motion (foot IK, turn-in-place, new clips) is the next plan, after this one.

## Superseded by this file

- `~/.claude/plans/pure-twirling-blum.md` "Gaming Quality & Fidelity Roadmap" (31 Oct, morning):
  its phases 1–2 landed in the NCR merge; phase 3 is folded into B4.
- `~/.claude/plans/hashed-gathering-quill.md`: the draft of this file.
- `docs/ncr/DESIGN.md`: describes the NCR map as built; reference only.
- `docs/ncr/masterplan/MASTERPLAN.md` + `ncr-4km.plan.json`: the 4 km blueprint. Its ideas
  (named destinations, contrasting districts, deliberate rotations) live on in Dilli at 2 km.
- `docs/agent-briefs/VIVAD_ASSET_REUSE_GUIDE.md`: reference for B4's asset sourcing.
