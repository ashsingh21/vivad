# Checking changes cheaply

Viewing a full screenshot costs an AI assistant about 2k tokens (a 1600x900 image); a session of
art work can spend most of its budget looking at pictures. Check with numbers first and look at
pixels only when the numbers point somewhere.

1. **Behaviour: text only.** `just walk` (stairs, roofs, doorways, courtyards), `just sim 3`
   (whole matches with bots). Add a `walk_check` case for every new climbable or enterable thing.
   Walks also report fall damage: every walk must end unhurt except those marked `hurts` (the
   jump into the stepwell). Drops of up to 6.5 m are free; `sim` lists "fell to their death"
   in the feed if bots start walking off edges again.
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

Render switches for comparisons: `VIVAD_EV=9.7` (camera exposure, default 10.8), `VIVAD_GRADE=1,1,1` (saturation, contrast, highlight gain; default 1.1,1.06,0.85),
`VIVAD_NO_GRASS=1`, `VIVAD_AA=none|smaa`, `VIVAD_NO_SSAO=1`, `VIVAD_NO_SHADOWS=1`; with
`VIVAD_PERF_LOG=1` the game logs FPS every 10 s; `VIVAD_NO_VSYNC=1` uncaps the frame rate so
the numbers mean something (with vsync they stop at the display's 60).

**Frame rate per view.** `just perf` holds every tour view for 400 frames uncapped and prints
its frame rate (`[tour-fps]`); compare runs before and after a change. `VIVAD_PERF_ALL=1` logs
every diagnostic, including the GPU time of each render pass: the shadow pass is the largest.
Knobs for comparisons: `VIVAD_GRASS=<0..1>` (share of blades; default 0.5),
`VIVAD_CASCADES=n` (default 3), `VIVAD_SHADOW_FAR=m` (default 200).

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
TAA plus sharpening, now the default, 1-4 %. Up close the score also counts edges and carving
shimmering as the camera drifts, so look at where it is before calling it z-fighting.

**Bot skill.** Bots are `chill` by default (loose aim, about a second and a half to react, an
archer picks fights within 35 m, they attack only half the time once engaged, sidestep lazily,
walk instead of sprinting unless the zone presses, and stand about 3-9 s wherever they arrive);
`VIVAD_BOT_SKILL=normal` brings back the sharp ones, `VIVAD_BOTS=n` sets how many.

**Map seeds.** `VIVAD_MAP_SEED` (or `just play <seed>`, `just play random`) picks the map; every
check here takes it too, and seed 7 is the default. The seed changes the hills, groves, villages,
the bazaar and Kashi houses and the loot; the landmarks stay put. `just seeds 40` sweeps the fast
map checks over many seeds; run it after touching the generator. The layout code claims
footprints (`Spots` in villages and Bargad Van, `blocked()` for scattered trees and bushes) so
nothing is placed inside anything else, whatever the seed.

**Z-fighting.** `just zfight` runs three checks: coplanar overlapping faces of different materials
among the map blocks (`zfight_check`); block tops lying in the ground and every placed model's faces
against the blocks and the terrain (`tools/zfight_models.py`); and the models against themselves
and each other (`tools/zfight_meshes.py`, about two minutes; `--model stepwell --where` lists the
spots for one model). Faces that can't be seen are left out: undersides on or in the ground, faces
wholly buried, and faces whose front is pressed into a map block or enclosed by other geometry. The
map generator lowers the ground a few centimetres under any block whose top lies at ground level
(`Terrain::settle_under`). What remains: the stepwell lining's back against its retaining walls and
door frames resting on floor slabs (hidden), and patches under 0.2 m2 on the stupa and
Ajanta.

**Sound** can't be judged by ear here: `just sfx` prints peak / RMS level, length, spectral
centroid and loop-seam size per sound, and `VIVAD_AUDIO_LOG=1` makes the game log how often each
sound fired (run automated sessions with `VIVAD_MUTE=1`). Whether it sounds right needs a listen.
