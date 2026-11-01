> **Superseded.** The one plan is `docs/PLAN.md` in the vivad repo. This file is kept for reference only.

# Vivad asset reuse guide for coding agents

Realistic environments with existing assets

Prepared for the Vivad project on 31 October 2026

Use this guide to ask a coding agent to improve the game's visual quality with suitable existing models, materials, and animations. Reuse ordinary world assets and create custom details where they give Vivad its identity. The first deliverable is a small playable scene with verified asset permissions, consistent art, and measured performance.

## 1 Give this assignment to your agent

### Copy this instruction with the guide

Improve Vivad's visual quality using existing free, already-owned, or explicitly approved market assets. Read this guide and inspect the repository before changing anything. Keep the current Rust, Bevy, Avian3D, and Blender workflow. Preserve existing gameplay and unrelated local changes.

Start with the existing Delhi street sample. Establish an in-engine baseline, find a coherent set of reusable assets, record their sources and licenses, prepare them in Blender, and integrate them into Bevy. Use custom work for fictional local signage, distinctive architecture, outfits, and mythic weapons where generic assets do not fit. Judge quality from the playable game and actual engine captures.

Deliver the improved sample, editable source assets, an asset register, reproducible build instructions, before-and-after captures, and measured performance. Report what passed and what remains incomplete. Complete useful local work without repeatedly asking about routine choices. If a paid asset is worthwhile, present its exact listing, current price, license, compatible files, and a free alternative before requesting purchase approval. No paid asset budget has been supplied.

### How to use this guide

1. Give the agent this file and access to the Vivad repository. The Markdown version is convenient for an agent; the Word version is easier to review and share.
2. Add any reference images, owned asset packs, or approved budget you already have. If none are supplied, the agent should use the existing project art direction and suitable free assets.
3. Review the first playable sample before expanding final art across the world. Ask for screenshots and measurements from the game, alongside any Blender previews.

### First result to request

Request one coherent Delhi street with believable materials, consistent object sizes, clear movement routes, and a small set of reused props. This establishes a repeatable asset workflow before larger map production.

## 2 Read the project context

### Current setup

This is a setup snapshot from this session. Verify the environment again when starting work on another machine.

| Component | Confirmed setup |
| --- | --- |
| Game code | Rust workspace with Bevy 0.20.0 and Avian3D 0.8.0 pinned in Cargo.toml |
| Asset authoring | Blender 5.2.3 LTS on Apple Silicon macOS |
| Character tools | MPFB 2.0.18 enabled and MakeHuman system assets installed |
| Asset transfer | Existing GLB and glTF assets with texture files |
| Installation check | Male and female bodies, 53-bone rigs, skin, hair, and eyes loaded successfully; this does not certify final character art |

The local Blender executable is `/Applications/Blender.app/Contents/MacOS/Blender`. The project reads `BLENDER` from `.env`. Character and figure scripts now resolve asset locations through MPFB's settings. Preserve that portable behavior.

### Read before implementing

Paths below are relative to the repository root.

- `README.md`, `Cargo.toml`, and `justfile` describe the stack and current commands.
- `docs/references.md` records asset conventions and existing sources.
- `docs/ncr/DESIGN.md` describes the NCR arena and its remaining work.
- `docs/ncr/masterplan/MASTERPLAN.md` describes the proposed 4 by 4 km map and future production targets.
- `crates/game/src/street_sample.rs`, `crates/game/src/bin/street_sample.rs`, and `tools/street_sample/build_scene.py` define the separate street sample.
- `crates/shared/src/map/ncr.rs` and `docs/testing.md` provide implementation and verification context.

### Keep the scope clear

The 4 km map is a design blueprint, not evidence of a completed runtime. Older map documents and current dimensions can differ; inspect the implementation before assuming scale. Improve the existing sample first. Treat its footprint as the starting scope, and keep original and NCR game modes working.

The longer-term design targets Mac and high-end Android. This session verified the Mac tools; Android packaging and device performance still need separate validation.

## 3 Find and select reusable assets

### Start with these sources

| Source | Useful content | Usage check |
| --- | --- | --- |
| [Poly Haven](https://polyhaven.com/license) | Realistic models, textures, and HDR lighting environments | Assets use CC0 and may be used commercially |
| [ambientCG](https://docs.ambientcg.com/license/) | Surface materials such as paving, concrete, metal, and wood | Assets use CC0 and may be used commercially |
| [Fab](https://www.fab.com/o/about) | Environment kits, buildings, props, characters, and animation packs | Check each listing's license, price, and downloadable formats |
| [Mixamo](https://helpx.adobe.com/creative-cloud/faq/mixamo-faq.html) | Humanoid rigging and character animations | Adobe ID required; Adobe permits characters and animations in commercial games |

Fab's Standard License permits compatible tools beyond Unreal Engine, but source-file access depends on the offering. A downloadable model does not imply that an engine-specific scene, shader, or script works in Bevy. Check the listing and [Fab license terms](https://www.fab.com/eula) before choosing a pack.

### Choose for the scene and the game

Prefer a small, consistent family of assets. Match architectural style, wear, colour, scale, and level of detail. Reuse road surfaces, trees, furniture, AC units, bins, railings, and ordinary buildings. Adapt signage, facades, and distinctive props to the fictional NCR setting.

Shortlist assets with useful source files, complete textures, reasonable geometry, and a clear license. Prefer GLB, glTF, or Blender sources; inspect other formats before committing to conversion. For an animation, check the skeleton, pose, clip names, and root motion. For an environment kit, inspect one representative asset before importing the entire pack.

Use reference images to guide composition and proportion. Reference images are not automatically licensed game assets. Preserve the project's fictional brands and authored locations.

### Keep an asset register

Create `docs/assets/ASSET_REGISTER.md` or an equivalent structured register. For every imported asset, record its name, creator, exact source URL, acquisition date, license and saved evidence, attribution requirement, cost or ownership status, original filename and hash, local source location, exported game paths, and modifications.

Keep marketplace archives and editable sources separate from runtime assets. Respect restrictions on redistributing source files, including in public repositories. Do not treat free download access as permission for every use.

## 4 Prepare assets in Blender

### Use a repeatable conversion process

1. Preserve the original download and work on a copy. Record the archive hash and import settings so the asset can be rebuilt later.
2. Check real dimensions, origin, pivot, and orientation. Use one metre per engine unit. Match the project's existing export conventions and verify them with a simple reference object. Avoid stretching objects to fill a larger map.
3. Inspect geometry, normals, UVs, material slots, and missing images. Remove hidden or duplicate geometry where appropriate. Preserve silhouettes and deformation quality. Apply transforms only when compatible with the rig and animation workflow.
4. Prepare physically based materials. Use base colour for surface colour, roughness for surface response, metallic for metal behaviour, and normal maps for fine detail. Keep lighting out of base-colour textures. Use sRGB for base colour and emission, and non-colour data for roughness, metallic, and normal maps.
5. Check channel packing. glTF reads roughness from green and metallic from blue. If using an ORM image, red supplies occlusion through the material's occlusion input. Verify normal-map orientation and the actual exported material. See the [glTF material specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).
6. Bake procedural details that cannot be exported into textures. Blender's [baking workflow](https://docs.blender.org/manual/en/latest/render/cycles/baking.html) supports transferring base colour and normal detail to game assets. A Blender render alone does not prove an exported shader will match.
7. Prepare simpler collision shapes and lower-detail mesh variants where needed. Keep collision separate from decorative geometry. Reduce material slots and reuse textures. Choose texture sizes from visible screen detail and measured memory cost.
8. For animated characters, check skin weights, clips, foot contact, root motion, and weapon attachment points. Retarget external animation to the game's rig deliberately; do not assume a Mixamo clip matches the existing MPFB skeleton.
9. Export GLB or the existing glTF plus external-texture layout, whichever fits the asset. Reopen the exact export and verify geometry, textures, materials, and animations before importing it into the game.

### Preserve editable sources

Keep the `.blend` file, texture inputs, conversion script, and export settings together. Document any manual adjustments. Put game exports under the existing `assets` structure. If a generator owns an output, update the generator or its inputs so the next rebuild preserves the improvement.

### Make failures visible

Run Blender automation with `--python-exit-code 1` and inspect its log and outputs. Some existing recipes filter logs and suppress exit failures; a recipe completing is not sufficient proof that an asset was produced correctly.

## 5 Integrate and improve the playable sample

### Establish a baseline first

Capture the existing sample at fixed camera locations. Record the machine, operating system, build mode, seed if applicable, physical render resolution, and graphics settings. Measure a repeatable route before and after the change. A Retina window's logical size may differ from its actual render resolution.

### Build the first scene

Use one coherent facade kit, road and pavement materials, a small set of street props, restrained vegetation, and fictional local signage. Keep doors, stairs, and exits readable. Place wear where use and weather explain it, such as dampness beneath pipes or dirt near ground contact.

Use consistent daylight for the first pass. Establish material response, shadows, and contact with the ground before adding weather or heavy effects. Compare the scene with its references from player-eye height. Preserve readable opponents and gameplay cues against the environment.

### Connect assets to the existing systems

- Load assets through the existing Bevy systems and reuse mesh and material handles. Verify the pinned APIs before introducing new rendering code.
- Keep gameplay colliders and camera collision aligned with the visuals. Decorative replacement must preserve cover, traversable openings, and access to loot.
- Preserve batching and visibility behaviour. Lower-detail files require explicit selection or switching in the runtime; naming a mesh LOD1 does not prove that the game uses it.
- Verify every required glTF extension and shader feature in the project. Rebuild unsupported effects with the available Bevy renderer or choose simpler materials.
- Check character animation transitions and held items if characters change. Maintain the gameplay skeleton and attachment conventions unless the task specifically changes them.
- Preserve source-versus-export ownership. In particular, understand `tools/street_sample/build_scene.py` before regenerating the street asset.

### Measure before expanding

Use the masterplan's performance values as provisional targets, not achieved results. Record CPU and GPU frame time where available, texture memory, visible geometry, loading pauses, and the test conditions. State unavailable measurements explicitly. A Mac result does not establish Android performance.

When the sample meets visual, gameplay, and performance checks, expand the same reusable asset family into the intended district. Extend the world by adding space while preserving the dimensions of doors, cars, people, and street furniture.

## 6 Verify and hand off the result

### Completion checks

| Area | Evidence required |
| --- | --- |
| Asset permissions | Every external asset has a register entry and retained license evidence; required credits are included |
| Visual quality | In-engine captures show consistent scale, complete textures, plausible materials, and no visible clipping or floating props |
| Gameplay | The sample route is traversable; stairs, exits, camera collision, and any changed cover or character interactions work |
| Performance | Repeatable before-and-after measurements include hardware, resolution, graphics settings, and build mode |
| Reproducibility | Editable sources and conversion instructions rebuild the exports; a fresh game launch loads them without missing-file errors |

### Use the relevant project commands

| Command | Purpose |
| --- | --- |
| `just street-sample` | Launch the separate Delhi environment sample |
| `just street-capture` | Capture the sample in the actual engine |
| `just street-walk` | Check sample traversal |
| `cargo check -p game` | Check game code after Rust changes |
| `just ncr-walk` and `just ncr-sim 1 15` | Check traversal and a bot match when changes reach the main NCR arena |

Verify the current `justfile` before running commands. Run `just street-assets` only when its generator includes the intended asset changes. After full-map integration, use the relevant tour and manual playtests as well. Passing a walk test or bot match alone does not establish visual quality or balanced gameplay.

### Deliver these files and results

- The working scene and game exports, plus editable source files or permitted source locations.
- The asset register and any required credits or redistribution notes.
- Exact rebuild and launch commands, with meaningful failures preserved in logs.
- Matching before-and-after engine captures and a short repeatable walkthrough route.
- A concise report of measured results, files changed, known issues, and the next useful improvement.

### Skills that support the work

Use existing image-generation capabilities for concept references and applicable QA guidance for tests. Blender modeling, materials, lighting, animation, export, and optimization skills can support asset reuse. The community [Blender Agent Skills pack](https://github.com/EL4CTEO/blender-skills) is an optional candidate for Blender 5.2; it has not been installed or tested in this project. Verify relevant instructions and scripts before adopting them. The repository and actual engine output remain the authority for compatibility and completion.
