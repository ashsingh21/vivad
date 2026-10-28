set dotenv-load := true

blender := env_var_or_default("BLENDER", "blender")
weapons := "khanda"

# Rebuild every model from its script (bake + LODs + GLB export).
assets:
    for w in {{weapons}}; do just weapon $w; done
    just nature
    just characters
    just anims
    just architecture
    just sfx
    python3 tools/prune_textures.py

# Buildings and landmarks: temple, arches/jharokha/chhatri, Hampi, huts and stalls, stupa and
# Ajanta caves, stepwell (each script bakes, decimates a LOD1 and exports GLBs to assets/models/props)
architecture:
    for s in temple ornaments hampi vernacular buddhist stepwell; do {{blender}} -b --factory-startup -P tools/blender/architecture/$s.py 2>&1 | grep -E '^\[|Error|Traceback' || true; done

# Carved figures for the kit (needs MPFB; writes tools/blender/architecture/figures/, which the
# architecture scripts read, so run this first when a figure changes)
figures:
    {{blender}} -b -P tools/blender/architecture/figures.py 2>&1 | grep -E '^\[|Error|Traceback' || true

# Characters (needs the MPFB extension + MakeHuman CC0 assets, see docs/references.md)
characters:
    for v in male female; do {{blender}} -b -P tools/blender/characters/human.py -- $v 2>&1 | grep -E '^\[|Error|Traceback' || true; done

# Warrior animation clips, per rig (needs build/char_<variant>_final.blend from `just characters`).
# Prints foot slide per clip; add `-- male --preview DIR --clips run_f,idle` for pose renders.
anims:
    for v in male female; do {{blender}} -b -P tools/blender/characters/anims.py -- $v 2>&1 | grep -E '^\[|Error|Traceback' || true; done

# Every sound effect, synthesized (prints level, length, brightness and loop-seam numbers).
sfx:
    python3 tools/audio/sfx.py --report

# Trees and bushes
nature:
    {{blender}} -b --factory-startup -P tools/blender/nature/trees.py 2>&1 | grep -E '^\[|Error|Traceback' || true

# Build one weapon: just weapon khanda
weapon name:
    {{blender}} -b --factory-startup -P tools/blender/weapons/{{name}}.py 2>&1 | grep -E '^\[|Error|Traceback|  File' || true
    test -f assets/models/weapons/{{name}}.glb

# Render review images from the exported GLB: just preview khanda pass2
preview name tag="latest" samples="64":
    {{blender}} -b --factory-startup -P tools/blender/render_preview.py -- assets/models/weapons/{{name}}.glb previews/{{name}}/{{tag}} {{samples}} 2>&1 | grep -E '^\[|Error|Traceback' || true

# Load the GLB in Bevy, screenshot it, exit.
view name:
    cargo run -p game --bin asset_check -- models/weapons/{{name}}.glb previews/{{name}}/bevy_{{name}}.png


# Play the graybox (release build; first build takes a few minutes).
play:
    cargo run --release -p game --bin game

# Headless matches to sanity-check the loop and bots: just sim 5
sim matches="3" bots="15":
    cargo run --release -p shared --bin sim_check -- {{matches}} {{bots}}

# Walk a test character up the stairs, into the caves and over roofs; fails on a stuck walker.
walk:
    cargo run --release -p shared --bin walk_check

# Fly the camera over every landmark and screenshot it.
tour:
    mkdir -p previews/graybox && VIVAD_TOUR=previews/graybox cargo run --release -p game --bin game

# Screenshot triage in text (see docs/testing.md): numbers and flags per tour shot.
shots dir="previews/graybox":
    python3 tools/shots.py stats {{dir}}

# What changed in the tour since `just shots-keep`: changed %, brightness shift, 4x4 change map.
shots-diff dir="previews/graybox":
    python3 tools/shots.py diff previews/baseline {{dir}}

# Keep the current tour shots as the reference for shots-diff.
shots-keep dir="previews/graybox":
    python3 tools/shots.py baseline {{dir}} previews/baseline

# One small labelled contact sheet of some shots: just sheet "previews/graybox/2*.png"
sheet glob out="previews/sheet.png":
    python3 tools/shots.py sheet {{glob}} -o {{out}} --width 1200

# Flicker while moving: bursts with a drifting camera, scored per view (no AA ~5-16%, TAA ~1-4%).
# Compare render settings with VIVAD_AA=none|smaa, VIVAD_NO_SSAO=1, VIVAD_NO_SHADOWS=1.
flicker views="haveli_front,bazaar_street,hampi_temple,village_close,stupa":
    rm -rf previews/flicker && mkdir -p previews/flicker
    VIVAD_TOUR=previews/flicker VIVAD_TOUR_ONLY={{views}} VIVAD_TOUR_FRAMES=8 VIVAD_TOUR_DRIFT=0.004 cargo run --release -p game --bin game
    python3 tools/shots.py flicker --moving previews/flicker

# Z-fighting: coplanar overlapping faces between map blocks, and between the art models and the
# blocks / terrain.
zfight:
    cargo run --release -p shared --bin zfight_check -- --dump build/zfight.json
    python3 tools/zfight_models.py build/zfight.json

# Geometry report for exported models (tris per LOD, size, open edges, pieces, materials).
mesh +models:
    {{blender}} -b --factory-startup -P tools/blender/mesh_report.py -- {{models}} 2>&1 | grep -E '^==|^  ' || true

# Let a bot play as you and screenshot the match.
autoplay:
    mkdir -p previews/graybox/match && VIVAD_AUTOPILOT=1 VIVAD_SHOTS=previews/graybox/match cargo run --release -p game --bin game
