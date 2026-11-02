# Panga: the one plan

**This is the only plan.** Every other roadmap, design doc or blueprint in this repo or in
`~/.claude/plans/` is superseded by this file; they stay only as reference and say so at the top.
Progress is tracked in the checklists here. When a step lands, tick it, note the commit, and keep
going down the list. If something here turns out wrong, change this file, not a new one.

Owner decisions recorded 31 October 2026, with later ones added and dated:

1. **Connaught Place first.** Everything below is built and proven on `cp` before any other map
   is touched. After that the work moves to the other maps. The final map is **NCR, built from
   the current code**, with the current Delhi maps as the Delhi part of it. Decision 6 overrides
   the order here for NCR and settles what was open: size, the old blockout, Bharat.
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
6. **NCR is the one final map (1 Nov 2026).** One Delhi NCR map from real OpenStreetMap data, with
   all the other maps merged into it, buildings sparse, and open space to fight. **This overrides
   the Phase 0 rule that NCR waits until a stranger has played the prototype twice**, and the
   fictional-map rule in `docs/ncr/masterplan/MASTERPLAN.md`. The map is 4 km a side. The old 2.2 km
   blockout is gone from play. Bharat is gone. The four small maps stay as they are. Work like a
   lazy expert: copy what exists, merge it, improve only what needs it (`docs/ncr/REUSE.md`).

Machine this is built and measured on: MacBook Pro, Apple M4, 16 GB, Blender 5.2 LTS at
`/Applications/Blender.app`, Python 3.13 (numpy, PIL, scipy, requests), cargo 1.99, git-lfs.
Not installed: ffmpeg, ImageMagick, wasm-bindgen, any KTX2 encoder. The M4 now runs at Ultra
(step 1 of Phase 1).

---

## Where things stand (1 Nov 2026, evening)

- `main` is at `743ae0c`, the Phase 0 prototype plus the NCR, Ultra and character work. That is two
  commits past `origin/main`, and nothing is pushed. The one-map work in Section A is one commit on
  top of it, on the branch `claude/ncr-final-map`, not merged yet. The workspace builds with zero
  warnings and its tests pass.
- Five maps. `ncr`, the one final map (4 km, real OpenStreetMap data), is the default and the only
  map in the rotation. A match on it is 64 warriors by default, you and 63 bots, so the first ring is
  the whole map; a small map fills to 16. Online rooms stay players only unless the host adds bots. The four small ones it is built from, `cp`, `chandni`, `hauzkhas` and
  `indiagate`, stay selectable with `VIVAD_MAP`, and a small map then stays until the game closes.
  The browser still starts on `cp`: the NCR has never been tried in a tab. Bharat is removed
  (`ca78f57`).
- NCR in the real game on the M4, at Medium, which it now picks by itself: 6.6 s from launch to the
  shaders warmed and the match on screen (8.6 s on the first launch after a build, both measured at
  High), and six minutes of a 64-warrior autopilot match at a median of 61 fps with 3.90 GB peak memory
  and 2.47 GB resident. High stays on F4: a whole 64-warrior match ran to its last warrior with no crash,
  3.96 GB peak, but held only about 44 fps for ten minutes in Chandni Chowk. Uncapped, High gives 34 to
  49 fps on the five busiest streets and Medium 44 to 60. These ran with iCloud syncing the project. A rematch stays on the map. The 74 tour views are in
  `previews/ncr_real/`. The corner map, the whole map, their place names and
  the place banner work in a real autopilot match.
- Headless on the final map (`sim_check`, 8, 6 and 4 matches, seeds from 201), every match finishes.
  At 16 warriors: 561 to 663 s, 99 % armed by 90 s, none never armed. At 32: 803 to 884 s, 97 %, none.
  At 64: 1168 to 1254 s, 70 % by 90 s, as the last bots only land after 80 s, and 88 % within 60 s of
  landing, 0 to 6 never armed. 534 of 558 deaths are by weapon, 18 by the ring and 6 by a bot's own
  gun. The ring burnt 3 to 6 warriors a match on average, and bots flipped between goals for 10
  bot-seconds a match at worst. `nav_check`, `peer_check`, `net_check` and `mesh_check` pass on the default map.
- `just ncr-walk` passes all 23 walks: every bridge both ways, three ridge passes and the crest, a
  wade across the Yamuna, the streets from Connaught Place to Chandni Chowk and a district gate. All
  19 districts can be walked to from Connaught Place.
- Two characters: MakeHuman bodies split into head, body, outfit and eyes, 4 LODs, new faces.
  Clothes, hair and the eight looks are not done.
- Rendering: Low, Medium, High and Ultra. Ultra has the real sky and day and night, and realistic
  surfaces on Connaught Place (steps 1, 2 and 5 of Phase 1).

---

## Step 0. Repo and baseline (deferred by the owner, do later)

Part done, the rest when the owner says so. Nothing is pushed yet and `origin` is still the only
remote.

- [ ] Add remote `panga` → `https://github.com/Apc0015/Panga.git`; push `main` and its LFS
      objects; set `panga` as the push default (`git push -u panga main`).
- [x] Commit the work as one or more commits on `main`: done up to `743ae0c`. The one-map work is
      on the branch `claude/ncr-final-map`. The stray `* 2.glb` / `* 2.bin` Finder duplicates,
      `.DS_Store` files, `previews/vivad_current_build.mp4` and the old test frames in
      `previews/` (`cp_clip`, `cp_clip_test`, `cp_loot`, `cp_pacing`, `cp_bots`, `phase1`) are left out.

Every later step ends with: build clean with zero warnings, its checks green, captures compared
against the baseline. Once Step 0 is done, each step also gets one commit on `main`, named after
the step and pushed to `panga`.

---

## A. The one NCR map

One 4 × 4 km map of the real Delhi NCR, built from OpenStreetMap, with the other maps merged in as
its Delhi districts, buildings sparse and open land to fight on (owner decision 6, 1 Nov). Code:
`crates/shared/src/map/compose.rs`. It is the default map: `just play`, or `target/release/game`.
The older drafts of this section are dropped: the 2 km `Dilli` map,
`dilli.rs`, `Tile`/`Frame`, `Terrain.gn`, the causeway Yamuna, and the chunked lazy grid.

### Layout (x east, z south, `HALF = 2000`, metres from the middle of the map)

The lists themselves live in `compose.rs`; this table only says what they hold.

| Piece | In `compose.rs` | What it is |
|---|---|---|
| 19 districts | `PLACES` | The four small maps plus 15 new ones in `assets/data/maps/ncr_*.json`, each kept to a square 180 to 250 m out from its middle, at true size, in its true compass direction from Connaught Place (`compass_order_holds`) |
| Open land | `open_land`, `on_open_land` | All the ground between districts, cut down in distance: rough open ground with thin ribbons of sheds along the roads and a scatter of trees, walls and loot, so nobody crosses more than about 80 m of nothing. The river sand gets berms, scrub and the odd hut |
| 25 smaller spots | `SPOTS` | Named places on the open land, each with a yard of its own: truck and bus yards, markets, offices, New Delhi Station, Badarpur's power station, the Ghazipur Mound, the Okhla Marsh. Each keeps its true side of its neighbours (`spots_stand_on_the_right_side_of_their_districts`). Nothing here is a temple, mosque, gurdwara or grave |
| Yamuna | `YAMUNA` | The river's real shape, 200 m of wadeable water inside a 450 m corridor of sand with bunds |
| Hindon | `HINDON` | 80 m of water, leaving by the east edge |
| 8 bridges | `SPANS`, `approaches` | Signature, Loha Pul, ITO, DND, Okhla Barrage, and three short ones on the Hindon, each named on the map and underfoot (`BRIDGES`). A deck over the water, earth over the land, and a ramp of about 1 in 13 at each end. South of ITO the Blue Line's viaduct, train only, ends at a station on each bank |
| Delhi Ridge | `RIDGE` | A green arc screening Delhi from Gurugram, crest up to about 24 m, walkable anywhere, with NH-48, Outer Ring Road and MG Road cut through it. The Ghazipur Mound is the one hill east of the river |
| 78 roads | `ROADS` | Ring Road, NH-48, Mathura Road, NH-9, Noida Expressway, MG Road, Pushta Road and the short links that carry each district's real road exits on to them |

At the last count: 1,815 buildings, 146 chests a match out of 373 spots, 32 of them Kalash, 1,600
floor spots, 5,164 trees and props, and buildings on 3.6 % of the map (`just ncr-map-check`).

### How it is built

- **Real data.** `tools/osm/ncr_extract.py` reads Geofabrik's Northern and Central Zone files with
  pyosmium, offline, and `tools/osm/ncr.py` converts each district to JSON. Credit: OpenStreetMap,
  ODbL 1.0, on the map screen and in each JSON. `ncr_janakpuri.json` and `ncr_vaishali.json` are
  converted but not placed.
- **One layout.** The map is always built from seed 7, so players can learn it. Each district
  holds three sets of chests, and every match opens one set in each, drawn from the match's seed
  (`chest_sets`), so a player who knows the map still has to look. The map seed only changes which
  spots make up the sets.
- **Districts are copied, not rewritten.** `delhi.rs` builds each one alone, then `add` copies what
  lies inside its square into the big map. Big footprints always stay; the rest are thinned to the
  place's `cap`, so streets get open ground between buildings. Loot and street life are cut to the
  place's `Tier`: hot, warm, quiet or small.
- **The four small maps stay exactly as they are.** `maps_are_unchanged` must keep passing. Never
  re-run the converter on `delhi_*.json`.
- **Open means open.** `map_stays_open` fails if roofs pass 6 % of the map.
- **Care.** No invented temple, mosque, gurdwara or grave, the skyline's roofs included. No loot at
  holy sites, at the India Gate arch, by the Red Fort's sealed wall, which has no name, or in the
  Okhla wetland, and nothing a chest or a bow drops lands in a holy circle either. Each district's
  JSON brings its `holy` list, and rings keep 120 m clear of those sites. No real brand on screen.
- **Nothing stands where it can't.** A lake or canal cut off at a district's edge keeps its surface,
  and its shore frays into the land. A roof mapped with nothing under it stands on columns. No chest
  or find lies inside a building, no open-land building stands on a road or another building, and
  no tree stands on a road or in a building (`no_water_is_dry`, `no_roof_hangs_in_the_air`,
  `no_loot_behind_walls`, `nothing_stands_in_the_way`).

### The match on it

- Rules per map style are in `Rules` (`crates/shared/src/sim/flow.rs`): the drop flies at 500 m at
  60 m/s, wading is 2.4 m/s, and a warrior falling off a tall roof opens the glider (roof rescue).
- Rings are `NCR_16`, `NCR_32` and `NCR_64` in `zone.rs`: 10:50, 14:30 and 19:50 from the first
  wait to the last shrink, opening on a 2 km disc, a 3 km disc or the whole map. Rings stay off
  water, bridges and holy sites, and every ring's centre is ground a warrior can walk to.
- A bot stays out looting until the last moment it can sprint in ahead of the ring, and a bot with
  no bow goes for the nearest one in sight. Each decision has two limits, so walking in doesn't undo
  it: bots no longer flip between two goals, which `sim_check` now counts as "dither". Some bots cut
  it fine (`BotBrain::nerve`), so the ring catches a few.
- Whole matches finish at 16, 32 and 64 warriors (`sim_check`). The NCR is the only map in the turn
  (`flow::ROTATION`), so a rematch stays on it and there is nothing to vote on.

### Navigation

A route graph over the whole map (`StreetFloor` in `crates/shared/src/sim/nav.rs`) says which way to
go: a point every 8 m, each bridge's deck a level of its own joined to the ground at its ends, and
no way up ground too steep to climb. A half-metre street grid drawn from the map's own shapes says
how to get along the next stretch of it, and the small maps' full half-metre grid stays as it was.
Roofs and upstairs are not routed. The old 8 m `StreetNav` grid is no longer built in play; only its
test `districts_connected_without_wading` still uses it.

### Rendering

Past 192 m the map is drawn as two cheaper shells, one mesh per cell and material
(`crates/game/src/buildings/tiers.rs`). The ground is one flat sheet with relief patches where the
land rises (`crates/game/src/visuals/ground.rs`): about 33 draws and 62 MB. `clipmap.rs` and
`heightmap.rs`, copied from `jackdaw_terrain`, MIT, notice kept, stand unwired, as a clipmap turned
out not to be needed. Near kit is streamed round the camera (`crates/game/src/batch/near.rs`).
Meshes and textures live on the GPU only, sent 48 MB a frame (`crates/game/src/batch/upload.rs`).

### Map on screen

`crates/game/src/hud.rs` bakes one picture from `MapLayout` when the map loads (2048 px, about
75 ms on NCR), so it needs no file and no second camera. The corner map follows the player, and N
steps its zoom. M opens the whole map with every name, the rings, the drop's line and the way to the
safe zone: the cursor is free while it is open, the wheel zooms round the cursor, a drag moves the
map, and N zooms round the player. It carries the OpenStreetMap credit. Names come from `pois` only:
one by the map's side reads inward, and one that would print over another moves off a line.
Coming into a place puts its name across the screen. This was written here, not copied: rustcraft's
map screen (`docs/ncr/reuse/minimap.md`) gave the idea only.

### Checklist

Ticked means built and proven by the check named. See "Where things stand" for the latest results.

- [x] **A1 One map from the districts** — `compose.rs`, 19 districts, rivers, ridge, roads,
      `MapId::Ncr` goes to it, the old blockout's generator is no longer used by the game.
- [x] **A2 Walkable from end to end** — every bridge both ways, the ridge, a wade, a district gate,
      and every district reached on foot from Connaught Place (`just ncr-walk`, 23 of 23, seed 7).
- [x] **A3 Bots at scale** — bots cross bridges and reach chests and rings across the 4 km map
      (`nav_check`, `sim_check`), 16, 32 and 64. No bot went unarmed at 16 or 32 in the last 14
      matches, and 0 to 6 did at 64, where some fall before they reach a gun. Still open: bots skip
      loot upstairs and indoors.
- [x] **A4 Match pacing** — ring and drop timed on the real map; the final circle lands on open
      ground a warrior can walk to, in cover; about 96 % of deaths come from fights, and the ring
      burns a few warriors a match and kills about one.
- [ ] **A5 Frame rate and memory** — `just perf` on the M4 at or under the budgets in
      `docs/ncr/masterplan/MASTERPLAN.md` section 6: 800 batches, 2.5M triangles. Peak memory is
      under 4 GB and Medium, the NCR's level on the M4, holds about 60 fps with 64 warriors. High does
      not: 34 to 49 fps on the busiest streets, and the busiest is 770 batches and 2.77M triangles.
      Open until High holds 60, or the budget changes. Karol Bagh is the slowest street at both levels.
- [x] **A6 Map on screen** — corner map at each zoom, whole map at each zoom, names and the place
      banner seen in a real autopilot match (`just ncr-hud`). The wheel, the drag and the free
      cursor pass `cargo test -p game --lib hud`, but nobody has tried them by hand yet.
- [x] **A7 Default map** — NCR is `MapId`'s default and the only map in `flow::ROTATION`. Rooms take
      the host's map, so they default to it too. The small maps stay for `VIVAD_MAP`. The browser
      still starts on `cp` (`the_ncr_is_the_one_map_in_the_turn`, a whole real match on the default).
- [ ] **A8 Real heights and ground** — Open Buildings 2.5D for heights, WorldCover for open land,
      as `docs/ncr/REUSE.md` lists them.
- [ ] **A9 Later** — metro, then vehicles; no ziplines. Roads and rail dressing, landmark art.

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
has (High/Ultra as built). Hair (C3), clothes and looks (C4, C5) and final polish (B5–B7) wait
until a stranger has played the prototype twice. **NCR no longer waits (decision 6): it is being
built now, on top of this prototype's bots, guns and loot.**

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
- Step 7 is about behaviour on this map. It started from a headless match on `cp` (seed 7, 15
  bots) that ended with 8 of the 15 deaths from the Aandhi and 7 from weapons. Phase 0's P1 and P2
  have since fixed that: 15 weapon kills and 0 Aandhi kills on five seeds.
- Step 3 is limited to `MapStyle` and `River`, replacing the string check. The audit (1 Nov) found
  `Terrain.gn` never reaches the screen and a 250 m tile clip cannot leave the small maps identical,
  so `gn` and `Tile`/`Frame` were dropped. `compose.rs` copies each district whole instead, and
  `dilli.rs` was never started. Audit findings for later steps: SSR cannot reflect the
  forward-rendered city (step 5 uses the environment map), and the head subdivide and cloth drape
  designs need rework (steps 6 and 8).

### Phase 2: NCR, the one final map (brought forward, 1 Nov 2026)

Decision 6 moved this ahead of the rest of Phase 1. Section A is the plan and its checklist tracks
the work. Connaught Place's standard still has to reach the other districts over time, but the map
comes first and art follows where players go.

Decided: 4 km a side; all 19 districts and the rivers fit. The old NCR blockout is out of play, and
`ncr.rs` only lends small helpers (`enabled`, `river_distance`) until someone removes it. Bharat is
removed. Gurugram, Noida and Ghaziabad are real districts from OpenStreetMap.

### Phase 3: phones (after Phase 2)

Research summary: realistic phone shooters are built as a separate phone version with baked
lighting, phone-specific characters and foliage, heavy texture compression and heat-based quality
scaling. No realistic 3D shooter shipped on Bevy for phones was found, so start with a 2 to 4 week
test scene on Mali, Adreno and iPhone hardware before committing. If it fails, reconsider the engine.

External downloads (CC0 facade/fabric textures) are each confirmed with the owner before
fetching, stored under `tools/blender/textures/cc0/` or `assets/textures/facades/`, and credited
in `docs/references.md`. Overpass is not needed: every district's JSON is already in the repo, and
`tools/osm/ncr_extract.py` reads Geofabrik's files offline.

## How each step is run

Each step runs as a workflow: parallel implementers per independent file group, then an
adversarial verify stage (build, `cargo test -p shared`, `sim_check`, walk check, tour captures
compared against the baseline) before the step is accepted. The owner reviews the captures
between steps. Commits and pushes start once Step 0 is done.

## Verification (end to end)

- `cargo check --workspace --all-targets` clean, zero warnings, after every step.
- `cargo test -p shared`: `maps_are_unchanged`, the `compose` tests, the zone tests. `cargo test -p
  game --lib hud`: the map's zoom, drag and cursor.
- `just ncr-sim 1 15`: matches finish, no fall deaths, most deaths from fights.
- `just ncr-walk`: all walks pass and every district can be walked to.
- `VIVAD_QUALITY=ultra just perf`: ≥ 60 fps median on every tour view at 1512×982, vsync off;
  `just flicker` no worse than High.
- Character files pass `tools/check_characters.py`; `just looks` shows eight distinct
  warriors; in-game captures at `CAMERA_DISTANCE` on High and Low confirm the 16 m crossfade.
- Browser build (`just web serve=no`) still compiles with Ultra excluded.
- Sheets in `previews/ultra_m<N>.png`, `previews/ncr/`, `previews/ncr_hud/`,
  `previews/characters/looks/`.

## Known trade-offs

- Until Phase 3, Ultra is desktop only. Low/Medium/High keep the stylised dome, baked AO and 256² facades.
- NCR's buildings have guessed heights until real height data lands (A8).
- Eight looks share one rig per sex, so faces vary by targets and skin, not by body shape.
- Volumetric haze and PCSS are the first things cut if the M4 misses 60 fps.
- Character motion (foot IK, turn-in-place, new clips) is the next plan, after this one.

## Superseded by this file

- `~/.claude/plans/pure-twirling-blum.md` "Gaming Quality & Fidelity Roadmap" (31 Oct, morning):
  its phases 1–2 landed in the NCR merge; phase 3 is folded into B4.
- `~/.claude/plans/hashed-gathering-quill.md`: the draft of this file.
- `docs/ncr/DESIGN.md`: describes the old 2.2 km blockout, which is out of play; reference only.
- `docs/ncr/masterplan/MASTERPLAN.md` + `ncr-4km.plan.json`: the fictional 4 km blueprint,
  overridden by decision 6. Its layout rules and budgets still serve as reference for Section A.
- `docs/agent-briefs/VIVAD_ASSET_REUSE_GUIDE.md`: reference for B4's asset sourcing.
