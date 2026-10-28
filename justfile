set dotenv-load := true

blender := env_var_or_default("BLENDER", "blender")
weapons := "khanda"

# Rebuild every model from its script (bake + LODs + GLB export).
assets:
    for w in {{weapons}}; do just weapon $w; done
    just nature
    just characters
    just architecture
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
    cargo run --release -p game

# Headless matches to sanity-check the loop and bots: just sim 5
sim matches="3" bots="15":
    cargo run --release -p shared --bin sim_check -- {{matches}} {{bots}}

# Walk a test character up the stairs, into the caves and over roofs; fails on a stuck walker.
walk:
    cargo run --release -p shared --bin walk_check

# Fly the camera over every landmark and screenshot it.
tour:
    mkdir -p previews/graybox && VIVAD_TOUR=previews/graybox cargo run --release -p game

# Let a bot play as you and screenshot the match.
autoplay:
    mkdir -p previews/graybox/match && VIVAD_AUTOPILOT=1 VIVAD_SHOTS=previews/graybox/match cargo run --release -p game
