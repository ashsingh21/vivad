# Vivaad — Rendering & Mobile Performance Guidelines

Standing rules for any rendering, asset, world or building-system work. **Mobile is the constraint
we design for; PC gets scaled up from there.** (Given by the project owner, 2026-11-01. The status
appendix at the end records where the codebase stands against it and what conflicts are open.)

> Bevy APIs change between versions. Before using any API named here, check the Bevy version in
> `Cargo.toml` and confirm the API exists / how it's spelled in that version. If something here
> doesn't exist in our version, say so and propose the closest equivalent — don't silently skip it.

## 1. Core principles

1. **Triangle meshes, not voxels.** Building pieces (walls, floors, ramps, roofs) are prefab meshes
   snapped to a grid. Destruction = health → swap to broken mesh / particles → despawn. No voxels in v1.
2. **The phone should never draw most of the world.** Cull what isn't visible, reduce detail with
   distance, and make each pixel cheap.
3. **On mobile, pixels and bandwidth usually matter more than triangle count.** Draw calls, overdraw,
   expensive shaders, full-screen post-processing and memory bandwidth are the usual bottlenecks;
   they also cause heat and throttling.
4. **Measure, don't guess.** Every perf change needs a before/after number from the profiling setup (§6).

## 2. Culling (draw less)

**Frustum culling.** Bevy does it from each mesh's `Aabb`; don't disable it. Runtime-generated or
edited meshes must have correct bounding boxes (stale AABBs pop or never cull).

**Distance culling + LOD.** Use `VisibilityRange` to switch LOD meshes and hide small props past a
distance. Every non-trivial asset has LODs (§5). Grass, debris and small props get a hard cull distance.

**Occlusion culling.** Render a cheap depth of the big occluders, test bounding boxes against it,
skip what's fully hidden.
- Check our Bevy version's built-in GPU occlusion culling (experimental) and whether it runs on the
  mobile backends before relying on it.
- Either way, design for coarse, game-specific occlusion: split maps into zones/cells with a simple
  potentially-visible-set and hide whole zones that can't be seen; make interiors (temples, palaces,
  courtyards) occlusion-friendly — solid walls and doorways, not wide-open sightlines everywhere.
- Player-built walls change constantly: never rebuild expensive occlusion data per frame. Use
  runtime GPU depth tests if available, otherwise ignore player structures as occluders.

## 3. Draw calls (batching & instancing)

- On phones the CPU cost of issuing draw calls is often the bottleneck.
- **Share meshes and materials.** Every piece of the same type and material uses the *same*
  `Handle<Mesh>` and `Handle<StandardMaterial>` so Bevy batches/instances them. Never clone a
  material per entity to tweak one value.
- Prefer texture atlases / array textures so different props share one material.
- Damage states: a few shared material variants or a per-instance value, not unique materials per piece.
- Static scenery: merge into larger meshes per zone (offline or at load) to cut entities and draws.

## 4. Cheap pixels (tile-based GPU rules)

Phone GPUs (Apple, Mali, Adreno, PowerVR) are tile-based: on-chip tiles are cheap, trips to main
memory are expensive. So:
- **Minimise full-screen passes.** Bloom, SSAO, SSR, DoF, TAA, motion blur each usually force a
  round-trip to memory. Mobile profiles default to none or one (tonemapping, maybe cheap bloom).
- **Avoid overdraw.** Opaque front-to-back (Bevy sorts). Keep alpha-blended surfaces (foliage, smoke,
  cloth, flags) small and few; prefer alpha-to-coverage / alpha-mask over blending.
- **No framebuffer reads mid-frame** unless explicitly mobile-safe.
- **MSAA is cheap on tilers** (resolved on-chip), cheaper than TAA: prefer 2x/4x MSAA on mobile.
- **Lighting:** baked (lightmaps / light probes / environment maps) for static geometry. At most one
  real-time shadowed directional light (the sun) on mobile, short distance, low-res cascades; no
  shadows on Low.
- **Shaders:** simple `StandardMaterial`; no parallax, many texture layers or per-pixel loops.
- **Dynamic resolution:** 3D at 70–85% and upscale, UI native; scale configurable per tier.

## 5. Assets (Blender pipeline)

Detail comes from **textures and normal maps, not raw triangle count**.
- Bake high-poly detail to normal maps on a reasonable low-poly mesh.
- Each asset exports LOD0 / LOD1 / LOD2 (~100% / 40% / 10%), plus an impostor or very simple mesh
  for distance where relevant.
- glTF/GLB, 1 unit = 1 m, transforms applied.
- Textures: power-of-two, **compressed** (KTX2: ASTC for mobile, BC for PC), mipmapped. No raw
  PNG/JPG in game on mobile.
- Reuse materials and atlases across a culture's set (Indian, Chinese) to keep batching effective.

| Asset (LOD0, starting budget) | Triangles |
|---|---|
| Player character | 15k–25k |
| Weapon | 3k–8k |
| Building piece (wall/floor/ramp) | 200–1,000 |
| Large prop (chariot, statue, gate) | 5k–15k |
| Small prop / debris | < 500 |

## 6. Performance budget & profiling

| Tier | Example devices | FPS | Frame time | Render scale | Shadows | Post-FX |
|---|---|---|---|---|---|---|
| Low | ~4 GB RAM Android, older iPhones | 30 | 33 ms | 70% | Off | None |
| Mid | Recent mid-range | 30–60 | 16–33 ms | 85% | Sun, short range | Tonemap |
| High | Flagship phones | 60 | 16.6 ms | 100% | Sun | Tonemap + light bloom |
| PC | — | 60–144+ | ≤ 16.6 ms | 100%+ | Full | Configurable |

Per-frame visible budgets, Low/Mid mobile (initial guesses): draw calls ≤ 300–500; visible
triangles ≤ 500k–1M; app memory ≤ ~1.5 GB on Low; a full ~20 min match without thermal throttling
(leave headroom, don't run the GPU at 100%).

Profiling: a toggleable overlay (FPS, frame time, entities, draw calls / visible meshes); `tracing`
/ Tracy for CPU systems; Xcode GPU capture, Android GPU Inspector, Mali / Snapdragon profilers on
devices; a **benchmark scene** — a fixed camera path through a dense part of a map with ~100
player-built structures — recorded before/after perf changes.

## 7. Quality tiers

One `GraphicsSettings` resource (Low / Mid / High / PC / Custom) controls render scale, MSAA,
shadows on/off + distance + resolution, LOD distance multiplier, prop/foliage cull distances,
post-processing toggles, and the FPS cap (30/60/uncapped). Auto-pick a tier from platform/device on
first launch, let players override. All rendering features read from it; nothing hard-coded.

## 8. When making changes

- Before adding a rendering feature, state its expected mobile cost and which tiers enable it.
- PC-only features stay behind tiers, never default-on for mobile.
- If a change costs performance, report the measured impact and propose a cheaper Low-tier fallback.
- If this doc conflicts with the codebase or the Bevy version, flag it rather than guess.

---

## Appendix: status in this codebase (Bevy 0.20.0, checked 2026-11-01)

**APIs, as spelled in 0.20**
- `VisibilityRange` (bevy_camera): exists, already used for building chunks, clutter, characters'
  LODs (`crate::characters::LodRange`, scaled by `crate::quality::LodScale`).
- GPU occlusion culling: exists as the `OcclusionCulling` camera component (bevy_render
  `occlusion_culling`), two-phase, no baking; needs a depth prepass and GPU preprocessing in
  `GpuPreprocessingMode::Culling`. Bevy turns GPU preprocessing **off** on the GL backend and on a
  list of Android devices, and limits it (no culling) where compute features are missing — so it
  can't be relied on for every phone. Not yet enabled or measured here.
- Texture compression: `ktx2` and `basis-universal` features exist (Basis transcodes one file to ASTC
  or BC at load, depending on the GPU).
- Draw-call counts: no built-in diagnostic found in 0.20; `RenderDiagnosticsPlugin` gives per-pass
  GPU/CPU time (already in the F3 overlay). A visible-mesh count can be had from `ViewVisibility`.

**Where the code stands**
- Tiers: `crate::quality::Quality` (High / Medium / Low) already sets render scale (the 3D view renders
  to an image, HUD native), shadows/cascades, SSAO, AA, LOD scale; F4 cycles, `VIVAD_QUALITY` forces,
  the browser steps down by measured fps. Missing vs §7: PC / Custom tiers, MSAA level, FPS cap, a
  single settings resource every feature reads.
- Post: bloom + vignette are on at **every** tier (§4/§6 say none on Low); atmosphere is compute.
  AA is TAA/FXAA/CAS, MSAA off (§4 prefers MSAA on mobile).
- Assets: ~93 PNG textures, no KTX2; LOD0/LOD1 on many models, not LOD2; the city is merged per
  64 m chunk (good for §3).
- Lighting: fully real-time (sun + ambient + atmosphere), nothing baked.

**Decisions on the conflicts (owner, 2026-11-01)**
- Platforms: **native first** (iOS, Android, PC); the browser build is no longer the phone path.
- Art: **polished stylised** stays; "not toy-like" means polished, not cheap.
- Setting: the `panga` branch (modern Delhi) stays the focus; ancient India / China are for Vivad later.
- Player building: **not yet**; keep the rules building-ready, benchmark on dense city streets meanwhile.
