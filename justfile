set dotenv-load := true

blender := env_var_or_default("BLENDER", "blender")
weapons := "khanda"

# Rebuild every model from its script (bake + LODs + GLB export).
assets:
    for w in {{weapons}}; do just weapon $w; done

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

run:
    cargo run -p game
