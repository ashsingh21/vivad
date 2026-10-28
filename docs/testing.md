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
