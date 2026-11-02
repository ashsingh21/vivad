# Checking changes cheaply

Viewing a full screenshot costs an AI assistant about 2k tokens (a 1600x900 image); a session of
art work can spend most of its budget looking at pictures. Check with numbers first and look at
pixels only when the numbers point somewhere.

1. **Behaviour: text only.** `just walk` (stairs, roofs, doorways, courtyards), `just sim 3`
   (whole matches with bots). Add a `walk_check` case for every new climbable or enterable thing.
   Walks also report fall damage: every walk must end unhurt except those marked `hurts` (the
   jump into the stepwell). Drops of up to 6.5 m are free; `sim` lists "fell to their death"
   in the feed if bots start walking off edges again. `sim` ends each match with a verdict per
   Aandhi death (stuck, no path, above the street, too far...) and a `nav paths` line (searches a
   second, their mean and worst milliseconds); `VIVAD_MAP=cp VIVAD_ROTATE=0 SIM_SEED=7 just sim 5`
   plays the same five Connaught Place matches every time, and `TRACK=<name>` (or `TRACK='*'` for
   everyone outside the ring) prints a warrior's position, goal, path and intent twice a second.
   `VIVAD_MAP=cp cargo run --release -p shared --bin nav_check` checks the grid itself: walkable
   share, stairs joined, chests and Aandhi circles reachable, path cost; `SKETCH=level,x0,z0,x1,z1`
   draws a patch of it.
   A game window that is not on screen (covered by another window, on another Space, or on a
   sleeping or locked display) is not rendered, and every shot of it is black (the "black frames
   after death" of 1 Nov were this, not the game): keep the window in view while a capture runs.
   The recipes also run under `caffeinate -d -i -u`. `VIVAD_CLIP_VIEW=1` takes clip frames from
   the 3D view's own image instead, which works with the window hidden but shows no HUD.
   `just clip` is the one moving picture: two minutes of an autopilot match from the player's
   seat as `previews/cp_clip/clip.webp` (`VIVAD_CLIP=<dir>,<from>,<to>,<fps>` writes the frames,
   `tools/clip.py` joins them), for judging how the guns feel: the reticle opens as a Vajra-9 or
   tamancha sprays (spread bloom, closing again at 2.5 degrees a second once the trigger is let
   go), each gun has its own kick, and the hit ticks go red and large for the hit that drops
   someone.
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
`VIVAD_BOT_SKILL=normal` brings back the sharp ones, `VIVAD_BOTS=n` sets how many (63 on the
NCR, so a match is 64 warriors, and 15 on a small map). Every bot
loots first: for 75 s after landing (30 s for the sharp ones) it fights only whoever hits it,
leaps for a landing no one else is making for, and leaves the chests others stand by; and below
40 health it breaks off and runs for the far side of a block, unless the ring is at its back or
too small to run in. That is what makes a sixteen-warrior match on Connaught Place last seven
to eight minutes instead of four. Looting itself: a find at a bot's feet (14 m) is taken on the
way, otherwise the nearest chest comes before any find across the street; goli only while under
120; no upstairs urns until the grace is over; a jog between chests; one short rest per chest
(1.5-3.5 s), none at a find, and a stand-about only at a wander spot. On the NCR a bot stays out
until the last it can sprint in ahead of the ring, and every decision about the ring has two
limits, so walking in doesn't undo the decision to walk in. `sim` reports it: an "armed" line
(warriors holding a gun by 60 s and 90 s and within 60 s of landing, the stragglers by name),
"chests opened N/T", "pickups taken", "dither" (bots out of a fight whose goal changed 8 times in
10 s: they keep moving, so "stuck" can't see them; `FLIPS=1` prints what each was about), and "ring
burnt" (how many warriors the ring touched, and for how much). Deaths are counted as weapon,
Aandhi or own gun. Connaught Place at map seed 7 has 147 chests (103 ring Peti facing their
shop fronts, 26 upstairs Kalash, 18 at building feet) and 102 floor-find spots.

**The NCR map.** Text first, as with the rest. `just ncr-walk` walks the warrior's real capsule
over every bridge both ways, through the ridge's three passes and over its crest, across the
Yamuna on foot, through the streets from Connaught Place to Chandni Chowk and in by a district's
gate, in about two minutes, then counts which districts can be walked to from Connaught Place. At
the top it prints how far each bridge's deck stands over the ground at its ends: a warrior jumps
1.3 m. `ONLY=bridge just ncr-walk` runs a few walks, `TRACE=<word>` prints those walkers' positions,
and a failed walk says what stands by its first snag. `just ncr-map-check` prints what the map holds,
and `cargo run --release -p shared --bin ncr_map_check -- --at x z radius` lists what stands there.
`just ncr-tour` shoots every view, `VIVAD_TOUR_ONLY=bridge,10_` a few of them, and `just
ncr-atlas` draws the whole map as an SVG with loot and bridges on it; give it a scratch path while
checking. `just ncr-sim 1 15` plays a headless match.

The NCR is the default map, so `just sim`, `just peers`, `just net` and `just p2p` run on it; a whole
NCR match takes up to about 900 game seconds, which is why `net` and `p2p` now allow 1000.
`just tour`, `perf`, `flicker`, `zfight`, `seeds` and `armoury` stay on Connaught Place unless
`VIVAD_MAP` says otherwise, because their views and seeds are the small maps'.

**The map on screen.** `cargo test -p game --lib hud` covers the zoom, the drag and the free cursor
without a game. `just ncr-hud` plays a real autopilot match on NCR and shoots the corner map at each
zoom and the whole map at each zoom, with the keys pressed for it by `VIVAD_SHOTS_KEYS=m:116,n:120`.
`VIVAD_HUD_IMAGE=1` draws the HUD to an image instead of the window, so these shots keep the HUD
even with the window hidden, while the window itself stays empty. The log says how long the picture took to bake (`map picture baked in`)
and names each place as the player comes into it (`coming into`). Dragging and the wheel by hand are
for a person to try.

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
