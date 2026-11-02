# Minimap and map screen: what we reuse (1 Nov 2026)

No plugin was found. crates.io has only `bevy_twitch_minimap` (Bevy 0.14, a Twitch extension, not a game map). But there is good code to copy, so "nothing ready-made fits the minimap" in `docs/ncr/REUSE.md` is too strong. Nothing below has been compiled or run. Hours are estimates.

**Decision: a baked picture, not render-to-texture.** The map is 4 km a side and does not change, so a live top-down camera would redraw the 3D city every frame to show the same picture. People and the ring move, so they stay as UI nodes on top of it.

## 1. The picture: our own code
- File: `crates/shared/src/bin/ncr_atlas.rs`, function `picture()`. Licence: ours, with OSM data under ODbL 1.0.
- What it does now: paints roads, parks, water, hills and buildings at 2 m a pixel (2000 px). No new crate.
- Change: `png()` deletes this picture once it is read for the SVG. Keep a copy at `assets/textures/ncr/map_base.png`.
- Watch out: the picture fits one seed. A game run with another `VIVAD_MAP_SEED` has a different layout.
- Hours saved: 4.

## 2. Full map screen: rustcraft, a Minecraft clone
- Repo: https://github.com/c2i-junia/rustcraft
- Files: `client/src/ui/hud/minimap/world_map.rs` (605 lines), `client/src/ui/hud/minimap/markers.rs` (`heading`, `arrow_image`), `client/src/ui/widgets/scroll.rs` (`scroll_steps`).
- Licence: MIT for code. Its textures are CC BY-SA 4.0, so take no textures. Last commit: 2026-10-23. Bevy 0.19.
- Copy: M opens and closes. Drag moves the map. The wheel zooms around the cursor (`zoom_focus`). Space goes back to the player. Also `picture_corner`, `keep_on_the_world`, `place_world_map` and the two tests.
- Why it fits: one image is scaled and never redrawn. Markers are children of that image, placed in percent, so they move with it and keep their size. North is -Z as in our game, so nothing flips.
- Change: delete `draw_world_map`, `compose_world`, `MinimapSurface`, the theme, the hint bar and the accessibility parts. Load our picture. Set `SCALES` to screen px a metre, `[0.25, 0.5, 1.0]`: the first shows the whole map 1000 px tall. Swap mobs for squad mates. Add the zone and safe rings as percent children with `BorderRadius::MAX`, the way `hud.rs` draws them now.
- Bevy 0.20 port: `On<Pointer<Drag>>` becomes `On<PointerDrag>`, `On<Pointer<Scroll>>` becomes `On<PointerScroll>`, and `scroll.pointer_location.position` becomes `scroll.pointer.position` (Bevy guide `flat_pointer_events.md`). `delta`, `button`, `unit` and `y` keep their names.
- Our input: opening the map must unlock the cursor. Mouse look and fire already stop then, but `grab_cursor` in `crates/game/src/input.rs` locks it again on any left click, so it must skip while the map is open. KeyM is free.
- Hours saved: 5. The port takes about 2.

## 3. Corner minimap: keep `hud.rs`, add a crop
- Repo: Bevy itself, MIT or Apache-2.0, our pinned 0.20.0. File: `bevy_ui/src/widget/image.rs`, field `ImageNode.rect`, which shows one part of an image.
- Copy: nothing to paste. Put the picture on the minimap node and move `rect` with the player, about 400 m across at 190 px.
- Change: in `minimap()` in `crates/game/src/hud.rs`, make `to_map` take off the crop corner and use the new scale. The ring and dot nodes stay, and `Overflow::clip()` already cuts them at the edge.
- Idea seen in `crop_window` in `crates/pla/src/game/minimap.rs`, https://github.com/NicoRuedaA/project-arceus, MIT, last commit 2026-10-30, Bevy 0.19.1. It is a few lines of clamping. Write it ourselves: that repo is a Pokemon Legends: Arceus port, which is a needless risk for a paid game.
- Hours saved: 1.

## Later, only if needed
- Round map, or dark outside the ring: Bevy UI clips square only (no corner handling found in `bevy_ui_render` `clipping.rs`), so this needs a `UiMaterial`. Start from Bevy's `examples/ui/ui_material.rs`. Read `crates/f13-game/src/shaders/minimap.wgsl` (23 lines) and the `Minimap` struct in `crates/f13-game/src/ui.rs`, https://github.com/JLAgamer/f13-engine-poc, MIT or Apache-2.0, last commit 2026-10-21, Bevy 0.19. Rewrite, do not paste: its `ui/map.rs` comments quote addresses from the commercial game it rebuilds. Hours saved: 1.
- Tip from that same file: move fast markers with `UiTransform`, not `left` and `top`. Its comment says layout snaps to whole pixels, which shook its icons.
- Live camera view: `examples/demo_platformer/minimap.rs`, https://github.com/adrien-bon/bevy_ecs_tiled, MIT, last commit 2026-10-24, Bevy 0.20, 83 lines. A camera shown through `ViewportNode`. Bevy's `examples/ui/widgets/viewport_node.rs` shows the same.
- Click the map to set a marker: `src/ui/minimap/input.rs`, https://github.com/nanaisisi/nanai-alien-com-game, MIT and Apache-2.0, last commit 2026-11-02, Bevy 0.20.0. `RelativeCursorPosition` gives 0 to 1 across the node.

## Looked at, not picked
- `DianranQian/game_demo` `src/ui/minimap.rs` (MIT, Bevy 0.19): a live camera for both maps, keyboard pan, no wheel zoom.
- `johanhelsing/bevy_pancam` (MIT or Apache-2.0, Bevy 0.20, 2026-10-30): drives a camera's projection, and our map is UI.
- `o-moba/omoba-bevy` `client/src/minimap.rs` (MPL-2.0, 2,173 lines): tied to its network types.
- `samwhosung/benilla` `crates/benilla-app/src/minimap` (MIT or Apache-2.0, Bevy 0.18, 4,266 lines): built on another game's tile files. We have one picture.
- Permissive, but no full map in the file (keyword check only): `EndurnyrProject/lifthrasir`, `william-franco/racing-car`, `budgiedownunder/maze`, `lambdaclass/argentum`, `OpenRsG/GunZRs`, `snoworwind/starforge`, `Kalcode/spaceprojectsim`, `exjam/rose-offline-client`, `variouscafe/terrafolk-rust` (CC0).
- Read only, never copy. GPL: `jondwillis/kuluu-ffxi` (`kuluu-render/src/minimap/topdown.rs` is a top-down bake), `ferdoran/openroad`, `Matute289/xindeler`, `Sm1jjj/fh1_rewrite`, `mohsensamiei/redshift`, `Ladvien/foundation_vs_slop`. AGPL: `Ry-ot/Ryot`, `DigitalExtinction/Game`. Noncommercial: `miskibin/warbell`.
- No root licence file, so no copying: `Sigma-dev/bevy_kart`, `XertroV/ssol-simulator`, `buxx/oc`, `georgik/iotcraft`, `danielsobrado/drusniel-voxels-bevy`, `AnonJon/RustTS`, `Ganngann/siege-factory`.
- I read only the top of the GPL files, so I cannot rank them. The plan above does not need them.

## Still ours to write
- The vimana route line on the map screen. No pick has it. A thin rotated UI node will do.

## Duties
- Keep `Copyright (c) 2024 C2I (Club Informatique ISEN)` and the MIT text with the ported code.
- The picture is made from OSM data, so show "(c) OpenStreetMap contributors" on the map screen.

## Limits of this search
- GitHub's repo search hit its rate limit. Candidates came from code search, then 36 repos were cloned to read licences and dates. A repo that never uses the word "minimap" could be missed.
