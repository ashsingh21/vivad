set dotenv-load := true

blender := env_var_or_default("BLENDER", "blender")
weapons := "khanda"

# Rebuild every model from its script (bake + LODs + GLB export).
assets:
    for w in {{weapons}}; do just weapon $w; done
    just arsenal
    just nature
    just characters
    just anims
    just architecture
    just sfx
    python3 tools/prune_textures.py

# Buildings and landmarks: temple, arches/jharokha/chhatri, Hampi, huts and stalls, stupa and
# Ajanta caves, stepwell (each script bakes, decimates a LOD1 and exports GLBs to assets/models/props)
architecture:
    for s in temple ornaments hampi vernacular buddhist stepwell fort ghats; do {{blender}} -b --factory-startup -P tools/blender/architecture/$s.py 2>&1 | grep -E '^\[|Error|Traceback' || true; done

# Carved figures for the kit (needs MPFB; writes tools/blender/architecture/figures/, which the
# architecture scripts read, so run this first when a figure changes)
figures:
    {{blender}} -b -P tools/blender/architecture/figures.py 2>&1 | grep -E '^\[|Error|Traceback' || true

# Characters (needs the MPFB extension + MakeHuman CC0 assets, see docs/references.md)
characters:
    for v in male female; do {{blender}} -b -P tools/blender/characters/human.py -- $v 2>&1 | grep -E '^\[|Error|Traceback' || true; done

# Head shots of the exported characters (front and three-quarter): just faces, or just faces after
faces tag="latest":
    {{blender}} -b --factory-startup -P tools/blender/characters/portrait.py -- previews/faces/{{tag}} 64 2>&1 | grep -E '^\[|Error|Traceback' || true
    python3 tools/shots.py sheet previews/faces/{{tag}} -o previews/faces_{{tag}}.png --cols 4 --width 1600

# Quick face iteration without garments or bake (~1 min): just face-lab female, just face-lab male "x1.5 hair=short04"
face-lab variant opts="":
    {{blender}} -b -P tools/blender/characters/face_lab.py -- previews/faces/lab {{variant}} {{opts}} 2>&1 | grep -E '^\[|Error|Traceback' || true

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

# The bows, arrow and quiver, astras and healing vessels (all by default): just arsenal pinaka soma
# Writes assets/models/weapons/<name>.glb and the bow string anchors in assets/data/bows.ron.
arsenal *names:
    {{blender}} -b --factory-startup -P tools/blender/weapons/arsenal.py -- {{names}} 2>&1 | grep -E '^\[|Error|Traceback|  File' || true

# Every item in the player's hand (carried, aimed, drawn, swung, drunk) from set angles, then a
# contact sheet: just armoury, or just armoury pinaka,soma
armoury only="":
    rm -rf previews/armoury && mkdir -p previews/armoury
    VIVAD_MUTE=1 VIVAD_BOTS=1 VIVAD_ARMOURY=previews/armoury VIVAD_ARMOURY_ONLY="{{only}}" cargo run --release -p game --bin game
    python3 tools/shots.py sheet previews/armoury -o previews/armoury_sheet.png --cols 4 --width 1600

# Render review images from the exported GLB: just preview khanda pass2
preview name tag="latest" samples="64":
    {{blender}} -b --factory-startup -P tools/blender/render_preview.py -- assets/models/weapons/{{name}}.glb previews/{{name}}/{{tag}} {{samples}} 2>&1 | grep -E '^\[|Error|Traceback' || true

# Load the GLB in Bevy, screenshot it, exit.
view name:
    cargo run -p game --bin asset_check -- models/weapons/{{name}}.glb previews/{{name}}/bevy_{{name}}.png


# Play (release build; first build takes a few minutes). `just play random` for a new map (its
# seed is printed), `just play 1234` to replay one.
play seed="7":
    VIVAD_MAP_SEED={{seed}} cargo run --release -p game --bin game

# Headless matches to sanity-check the loop and bots: just sim 5
sim matches="3" bots="15":
    cargo run --release -p shared --bin sim_check -- {{matches}} {{bots}}

# Walk a test character up the stairs, into the caves and over roofs; fails on a stuck walker.
# Peers agree without a host: the same match seed builds the same match in separate processes.
peers seed="1234":
    cargo run --release -p shared --bin peer_check -- {{seed}}

# Peers in one process over an in-memory pipe, fighting a whole match: roster, ownership, positions, who fell, kills,
# winner, chests and pickups, a contested pickup, and a peer leaving at `leave` seconds (0: never).
net peers="3" bots="6" seconds="420" leave="90":
    cargo run --release -p shared --bin net_check -- {{peers}} {{bots}} {{seconds}} {{leave}}

# The same over real iroh connections on this machine: a lobby hosts, the rest join by code, and the
# match runs at `speed` times real time (VIVAD_RELAY=1 to go through iroh's relays;
# VIVAD_NET_SIM=80,30,5 imitates 80 ms latency, 30 ms jitter, 5% loss: use speed=1 with it).
p2p peers="3" bots="6" seconds="420" speed="4" leave="90" rematch="90":
    cargo run --release -p net --bin mesh_check -- {{peers}} {{bots}} {{seconds}} {{speed}} {{leave}} {{rematch}}

# Host an online match: shows the code the others pass to `just join`; Enter starts it (or it starts
# by itself at `players`, you included). lan=1 skips iroh's relays (one machine or a LAN).
host name="" players="0" lan="":
    VIVAD_HOST={{players}} VIVAD_NAME="{{name}}" {{ if lan != "" { "VIVAD_LAN=1" } else { "" } }} cargo run --release -p game --bin game

# Join an online match by its code (lan=1 if the host used it).
join code name="" lan="":
    VIVAD_JOIN={{code}} VIVAD_NAME="{{name}}" {{ if lan != "" { "VIVAD_LAN=1" } else { "" } }} cargo run --release -p game --bin game

walk:
    cargo run --release -p shared --bin walk_check

# Fly the camera over every landmark and screenshot it.
tour:
    mkdir -p previews/graybox && VIVAD_TOUR=previews/graybox cargo run --release -p game --bin game

# Frame rate per tour view, uncapped (shots go to previews/perf).
perf:
    mkdir -p previews/perf && VIVAD_TOUR=previews/perf VIVAD_TOUR_SETTLE=400 VIVAD_NO_VSYNC=1 cargo run --release -q -p game --bin game 2>/dev/null | grep tour-fps

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
# The map checks over many seeds: block z-fighting and block tops in the ground, per seed.
seeds n="40":
    cargo build --release -q -p shared --bin zfight_check
    for s in $(seq 1 {{n}}); do VIVAD_MAP_SEED=$s ./target/release/zfight_check --dump build/zfight_seed.json | sed -n 2p | sed "s/^/seed $s: /"; python3 tools/zfight_models.py build/zfight_seed.json | awk '/block material/{f=1;next} /^$/{f=0} f{a+=$2} END{printf "        block tops in the ground: %.1f m2\n", a}'; done

zfight:
    cargo run --release -p shared --bin zfight_check -- --dump build/zfight.json
    python3 tools/zfight_models.py build/zfight.json
    python3 tools/zfight_meshes.py build/zfight.json

# Geometry report for exported models (tris per LOD, size, open edges, pieces, materials).
mesh +models:
    {{blender}} -b --factory-startup -P tools/blender/mesh_report.py -- {{models}} 2>&1 | grep -E '^==|^  ' || true

# Let a bot play as you and screenshot the match.
autoplay:
    mkdir -p previews/graybox/match && VIVAD_AUTOPILOT=1 VIVAD_SHOTS=previews/graybox/match cargo run --release -p game --bin game
