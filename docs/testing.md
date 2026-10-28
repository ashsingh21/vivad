# Checking changes cheaply

Viewing a full screenshot costs an AI assistant about 2k tokens (a 1600x900 image); a session of
art work can spend most of its budget looking at pictures. Check with numbers first and look at
pixels only when the numbers point somewhere.

1. **Behaviour: text only.** `just walk` (stairs, roofs, doorways, courtyards), `just sim 3`
   (whole matches with bots). Add a `walk_check` case for every new climbable or enterable thing.
2. **Models: text only.** `just mesh assets/models/props/x.gltf` prints triangles per LOD, size
   and height range, open edges, loose pieces and materials: enough to catch a broken export,
   a missing LOD, a model floating or sunk, or a mesh that lost its closure.
3. **The game's look: numbers, then a diff.**
   - `VIVAD_TOUR_ONLY=cave,stupa just tour` captures only the views you touched.
   - `just shots` prints brightness, contrast, detail and saturation per view and flags DARK,
     BLOWN, FLAT (camera in a wall, untextured), MISSING-TEX.
   - `just shots-keep` once on a good state; after a change `just shots-diff` says per view how
     much changed, the brightness shift, and a 4x4 map of where. Unchanged views need no look.
4. **Only then look**, small: `just sheet "previews/graybox/2*.png"` tiles the views that need
   eyes into one labelled ~1200 px sheet (~1k tokens for six views), and
   `python3 tools/shots.py crop <png> x0 y0 x1 y1 -o out.png` cuts out the region the 4x4 map
   pointed at. Full-size shots only for a final check of something new.

The HUD (minimap, status, hotbar) is masked out of every measurement.

Render switches for comparisons: `VIVAD_EV=9.7` (camera exposure, default 10.35),
`VIVAD_NO_GRASS=1`, `VIVAD_AA=none|smaa`, `VIVAD_NO_SSAO=1`, `VIVAD_NO_SHADOWS=1`; with
`VIVAD_PERF_LOG=1` the game logs FPS every 10 s.

**Weapons in the hand.** `just armoury` stands the player on open ground and captures every item
carried and in use (aiming, drawing, swinging, drinking) from the front, side and back, then
tiles them into `previews/armoury_sheet.png`; `just armoury pinaka,soma` for a few. The last
poses use the bot as a frozen target (healed every tick): a sword cut and bow volleys seen from
the side and through the game camera, shot a set number of milliseconds after the strike, so the
blade ribbon, arrow streaks, hit puffs and damage numbers can be checked. Crop the interesting
ones before looking.

**Flicker and shimmer.** `just flicker` captures 8-frame bursts while the camera drifts a few
millimetres per frame and scores each view by how many pixels fail to move smoothly. Without
anti-aliasing the carved, normal-mapped surfaces scored 5-16 % (they crawl and sparkle); with
TAA plus sharpening, now the default, 1-4 %. `just zfight` lists coplanar overlapping faces of
different materials in the map blocks, then checks every placed model's faces against the blocks
and the terrain (`tools/zfight_models.py`). What remains is faces pressed together inside walls.

**Sound** can't be judged by ear here: `just sfx` prints peak / RMS level, length, spectral
centroid and loop-seam size per sound, and `VIVAD_AUDIO_LOG=1` makes the game log how often each
sound fired (run automated sessions with `VIVAD_MUTE=1`). Whether it sounds right needs a listen.
