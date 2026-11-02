set dotenv-load := true

blender := env_var_or_default("BLENDER", "blender")
weapons := "khanda"

# Rebuild every model from its script (bake + LODs + GLB export).
assets:
    for w in {{weapons}}; do just weapon $w; done
    just arsenal
    just desi
    just nature
    just characters
    just anims
    just architecture
    just sfx
    python3 tools/prune_textures.py

# Buildings and landmarks: temple, arches/jharokha/chhatri, Hampi, huts and stalls, stupa and
# Ajanta caves, stepwell, the Pushpaka Vimana (each script bakes, decimates a LOD1 and exports GLBs to assets/models/props)
architecture:
    for s in temple ornaments hampi vernacular buddhist stepwell fort ghats vimana; do {{blender}} -b --factory-startup -P tools/blender/architecture/$s.py 2>&1 | grep -E '^\[|Error|Traceback' || true; done

# Carved figures for the kit (needs MPFB; writes tools/blender/architecture/figures/, which the
# architecture scripts read, so run this first when a figure changes)
# The patang glider every player opens after the jump (four colourways).
glider:
    {{blender}} -b --factory-startup -P tools/blender/props/glider.py 2>&1 | grep -E '^\[|Error|Traceback' || true

# The Udta Auto, the default drop ride (VIVAD_VIMANA=pushpaka for the vimana).
jugaad:
    {{blender}} -b --factory-startup -P tools/blender/props/jugaad.py 2>&1 | grep -E '^\[|Error|Traceback' || true

# The street kit: parked autos, stalls, poles, rooftop clutter (one GLB, merged per chunk in game).
street-kit:
    {{blender}} -b --factory-startup -P tools/blender/props/street_kit.py 2>&1 | grep -E '^\[|Error|Traceback' || true

figures:
    {{blender}} -b -P tools/blender/architecture/figures.py 2>&1 | grep -E '^\[|Error|Traceback' || true

# Characters (needs the MPFB extension + MakeHuman CC0 assets, see docs/references.md)
characters:
    test -d research/characters/downloads/makehuman || zsh research/characters/scripts/fetch_mh.sh research/characters/downloads/makehuman
    for v in male female; do {{blender}} -b -P tools/blender/characters/human.py -- $v 2>&1 | grep -E '^\[|Error|Traceback' || true; done
    just characters-pack

# The heads' 4K colour and normal from build/textures to KTX2, for the Ultra level.
characters-pack:
    cargo run --release --manifest-path tools/texpack/Cargo.toml -- pack tools/textures/character_pack.txt

# One character, timed (all of `just characters` for one variant): just character male
character variant:
    time {{blender}} -b -P tools/blender/characters/human.py -- {{variant}} 2>&1 | grep -E '^\[|Error|Traceback|real' || true
    just characters-pack

# Checks the split character files from the files alone: rig against the clips, parts and LODs,
# budgets, the neck seam, shape against the old models, texture sizes.
check-characters *variants:
    python3 tools/check_characters.py {{variants}}

# Front, side, back and head of a built character in the game's idle clip: just lookbook male
lookbook variant out="previews/characters/new" samples="64":
    {{blender}} -b --factory-startup -P tools/blender/characters/lookbook.py -- {{out}} {{variant}} assets/models/characters/{{variant}}_parts.glb assets/models/characters/{{variant}}_anims.glb {{samples}} 2>&1 | grep -E '^\[|Error|Traceback' || true

# The split characters beside the old ones, LOD by LOD: just compare-characters male
compare-characters variant="male" samples="64":
    {{blender}} -b --factory-startup -P tools/blender/characters/portrait.py -- previews/characters/old {{variant}} {{samples}} 2>&1 | grep -E '^\[|Error|Traceback' || true
    for k in 0 1 2 3; do {{blender}} -b --factory-startup -P tools/blender/characters/portrait.py -- previews/characters/parts_lod$k {{variant}} {{samples}} parts lod=$k 2>&1 | grep -E '^\[|Error|Traceback' || true; done
    python3 tools/shots.py sheet previews/characters/old previews/characters/parts_lod0 previews/characters/parts_lod1 previews/characters/parts_lod2 previews/characters/parts_lod3 -o previews/characters/compare_{{variant}}.png --cols 3 --width 1500

# Head shots of the exported characters (front and three-quarter): just faces, or just faces after
faces tag="latest":
    {{blender}} -b --factory-startup -P tools/blender/characters/portrait.py -- previews/faces/{{tag}} 64 2>&1 | grep -E '^\[|Error|Traceback' || true
    python3 tools/shots.py sheet previews/faces/{{tag}} -o previews/faces_{{tag}}.png --cols 4 --width 1600

# Quick face iteration without garments or bake (~1 min): just face-lab female, just face-lab male "x1.5 hair=short04 age=0.8 --sheet"
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
    VIVAD_MAP=cp VIVAD_MUTE=1 VIVAD_BOTS=1 VIVAD_ARMOURY=previews/armoury VIVAD_ARMOURY_ONLY="{{only}}" cargo run --release -p game --bin game
    python3 tools/shots.py sheet previews/armoury -o previews/armoury_sheet.png --cols 4 --width 1600

# Panga's street armoury (bat, katta, tamancha, dunali, gulel): just desi, or just desi katta
desi *names:
    {{blender}} -b --factory-startup -P tools/blender/weapons/desi.py -- {{names}} 2>&1 | grep -E '^\[|Error|Traceback|  File' || true

# Render review images from the exported GLB: just preview khanda pass2
preview name tag="latest" samples="64":
    {{blender}} -b --factory-startup -P tools/blender/render_preview.py -- assets/models/weapons/{{name}}.glb previews/{{name}}/{{tag}} {{samples}} 2>&1 | grep -E '^\[|Error|Traceback' || true

# Load the GLB in Bevy, screenshot it, exit.
view name:
    cargo run -p game --bin asset_check -- models/weapons/{{name}}.glb previews/{{name}}/bevy_{{name}}.png


# Play the NCR, the game's one map, you and 63 bots (release build; first build takes a few minutes).
# `just play random` for a fresh seed (printed), `just play 1234` to replay one.
# VIVAD_BOTS=15 just play for a smaller match; a small map fills to 16.
play seed="7" map="ncr":
    VIVAD_MAP={{map}} VIVAD_MAP_SEED={{seed}} cargo run --release -p game --bin game

# One of the four small maps on its own: just map chandni (or cp, hauzkhas, indiagate).
map name seed="7":
    just play {{seed}} {{name}}

# Two minutes of a match from the player's seat, as an animation the owner can open in a browser:
# `just clip` records 95-215 s of an autopilot match on Connaught Place at 8 frames a second
# (previews/cp_clip/clip.webp); `just clip 60 120 10` picks the times and rate. Keep the game
# window in view while it records (a hidden window renders nothing), or set VIVAD_CLIP_VIEW=1
# for frames of the 3D view without the HUD, which work with the window hidden.
clip from="95" to="215" fps="8" map="cp":
    rm -rf previews/{{map}}_clip && mkdir -p previews/{{map}}_clip
    VIVAD_MAP={{map}} VIVAD_ROTATE=0 VIVAD_AUTOPILOT=1 VIVAD_CLIP=previews/{{map}}_clip,{{from}},{{to}},{{fps}} caffeinate -d -i -u cargo run --release -q -p game --bin game
    python3 tools/clip.py previews/{{map}}_clip {{fps}}

# Headless matches to sanity-check the loop and bots: just sim 5, or just sim 3 15 for 16 warriors
sim matches="3" bots="63":
    cargo run --release -p shared --bin sim_check -- {{matches}} {{bots}}

# Peers agree without a host: the same match seed builds the same match in separate processes.
peers seed="1234":
    cargo run --release -p shared --bin peer_check -- {{seed}}

# Peers in one process over an in-memory pipe, fighting a whole match: roster, ownership, positions, who fell, kills,
# winner, chests and pickups, a contested pickup, and a peer leaving at `leave` seconds (0: never).
net peers="3" bots="6" seconds="1000" leave="90":
    cargo run --release -p shared --bin net_check -- {{peers}} {{bots}} {{seconds}} {{leave}}

# The same over real iroh connections on this machine: a lobby hosts, the rest join by code, and the
# match runs at `speed` times real time (VIVAD_RELAY=1 to go through iroh's relays;
# VIVAD_NET_SIM=80,30,5 imitates 80 ms latency, 30 ms jitter, 5% loss: use speed=1 with it).
p2p peers="3" bots="6" seconds="1000" speed="4" leave="90" rematch="90":
    cargo run --release -p net --bin mesh_check -- {{peers}} {{bots}} {{seconds}} {{speed}} {{leave}} {{rematch}}

# Host an online match: shows the code the others pass to `just join`; Enter starts it (or it starts
# by itself at `players`, you included). Players only, unless `bots` adds some. lan=1 skips iroh's
# relays (one machine or a LAN).
host name="" players="0" bots="0" lan="" map="ncr":
    VIVAD_MAP={{map}} VIVAD_HOST={{players}} VIVAD_NAME="{{name}}" VIVAD_BOTS={{bots}} {{ if lan != "" { "VIVAD_LAN=1" } else { "" } }} cargo run --release -p game --bin game

# The same on a small map: just host-map chandni Arjuna
host-map map name="" players="0" bots="0" lan="":
    just host "{{name}}" {{players}} {{bots}} "{{lan}}" {{map}}

# Join an online match by its code (lan=1 if the host used it).
join code name="" lan="":
    VIVAD_JOIN={{code}} VIVAD_NAME="{{name}}" {{ if lan != "" { "VIVAD_LAN=1" } else { "" } }} cargo run --release -p game --bin game

# Fly the camera over a small map's landmarks and screenshot them (VIVAD_MAP picks it, Connaught
# Place if unset). `just ncr-tour` is the NCR's.
tour:
    mkdir -p previews/graybox && VIVAD_MAP=${VIVAD_MAP:-cp} VIVAD_TOUR=previews/graybox cargo run --release -p game --bin game

# Frame rate per tour view, uncapped (shots go to previews/perf). VIVAD_MAP=ncr for the NCR's.
perf:
    mkdir -p previews/perf && VIVAD_MAP=${VIVAD_MAP:-cp} VIVAD_TOUR=previews/perf VIVAD_TOUR_SETTLE=400 VIVAD_NO_VSYNC=1 cargo run --release -q -p game --bin game 2>/dev/null | grep tour-fps

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
    VIVAD_MAP=${VIVAD_MAP:-cp} VIVAD_TOUR=previews/flicker VIVAD_TOUR_ONLY={{views}} VIVAD_TOUR_FRAMES=8 VIVAD_TOUR_DRIFT=0.004 cargo run --release -p game --bin game
    python3 tools/shots.py flicker --moving previews/flicker

# Z-fighting: coplanar overlapping faces between map blocks, and between the art models and the
# blocks / terrain.
# The map checks over many seeds: block z-fighting and block tops in the ground, per seed.
seeds n="40":
    cargo build --release -q -p shared --bin zfight_check
    for s in $(seq 1 {{n}}); do VIVAD_MAP=${VIVAD_MAP:-cp} VIVAD_MAP_SEED=$s ./target/release/zfight_check --dump build/zfight_seed.json | sed -n 2p | sed "s/^/seed $s: /"; python3 tools/zfight_models.py build/zfight_seed.json | awk '/block material/{f=1;next} /^$/{f=0} f{a+=$2} END{printf "        block tops in the ground: %.1f m2\n", a}'; done

zfight:
    VIVAD_MAP=${VIVAD_MAP:-cp} cargo run --release -p shared --bin zfight_check -- --dump build/zfight.json
    python3 tools/zfight_models.py build/zfight.json
    python3 tools/zfight_meshes.py build/zfight.json

# Geometry report for exported models (tris per LOD, size, open edges, pieces, materials).
mesh +models:
    {{blender}} -b --factory-startup -P tools/blender/mesh_report.py -- {{models}} 2>&1 | grep -E '^==|^  ' || true

# Let a bot play as you and screenshot the match.
autoplay:
    mkdir -p previews/graybox/match && VIVAD_AUTOPILOT=1 VIVAD_SHOTS=previews/graybox/match cargo run --release -p game --bin game

# The one final map: the real Delhi NCR from OpenStreetMap, 4 km across (docs/PLAN.md, Section A).
# The default map: this is `just play` with the seed first. The layout never changes; `just ncr 42`
# only picks other loot spots.
ncr seed="7":
    VIVAD_MAP=ncr VIVAD_MAP_SEED={{seed}} cargo run --release -p game --bin game

# Screenshot the NCR map's views into previews/ncr (then `just shots previews/ncr`).
# VIVAD_TOUR_ONLY=bridge,ridge takes only the views with those words in their names.
ncr-tour:
    mkdir -p previews/ncr && VIVAD_MAP=ncr VIVAD_TOUR=previews/ncr cargo run --release -p game --bin game

# Real physics walks on NCR: every bridge both ways, the ridge, the streets from Connaught Place to
# Chandni Chowk and a district gate; fails on a stuck walker. ONLY=<word> runs the walks with that
# word in their names, TRACE=<word> prints where those walkers are every 2 game seconds.
ncr-walk:
    VIVAD_MAP=ncr cargo run --release -p shared --bin ncr_walk_check

# What the NCR map holds, and a picture of it from above: just ncr-map-check build/ncr.ppm
ncr-map-check out="build/ncr.ppm":
    cargo run --release -p shared --bin ncr_map_check -- {{out}}

# The map on screen in a real match, into previews/ncr_hud: the corner map at each zoom, the whole
# map at each zoom. It draws to an image, so the game window may be hidden, and stays empty.
ncr-hud:
    rm -rf previews/ncr_hud && mkdir -p previews/ncr_hud
    VIVAD_MAP=ncr VIVAD_MAP_SEED=7 VIVAD_ROTATE=0 VIVAD_MUTE=1 VIVAD_AUTOPILOT=1 VIVAD_HUD_IMAGE=1 VIVAD_SHOTS=previews/ncr_hud VIVAD_SHOTS_AT=10,100,104,108,112,118,122,126,130 VIVAD_SHOTS_KEYS=n:102,n:106,n:110,n:114,m:116,n:120,n:124,m:128 caffeinate -d -i -u cargo run --release -p game --bin game

# Headless NCR matches, the full 64 warriors unless told: just ncr-sim 1 15 for 16
ncr-sim matches="1" bots="63":
    VIVAD_MAP=ncr cargo run --release -p shared --bin sim_check -- {{matches}} {{bots}}

# Draw the NCR map's atlas from the game's own data: districts, bridges, loot, names.
ncr-atlas out="docs/ncr/map-atlas.svg":
    VIVAD_MAP=ncr cargo run --release -p shared --bin ncr_atlas -- {{out}}

# The old fictional 4 km blueprint (docs/ncr/masterplan, overridden by the real map, kept for
# reference): regenerate its atlas and validation data, or only check its spatial constraints.
ncr-plan:
    python3 tools/maps/ncr_masterplan.py
ncr-plan-check:
    python3 tools/maps/ncr_masterplan.py --check

# The Delhi street sample (assets/models/samples/delhi_street): walk it in a window, or check
# its four routes headlessly.
street-sample:
    cargo run --release -p game --bin street_sample
street-sample-walk:
    cargo run --release -p game --bin street_sample_walk

# The browser build (WebGPU): writes build/web (page, wasm, a link to assets/), serves it on
# http://localhost:8080. `just web serve=no` only builds.
web serve="yes":
    cargo build --release --target wasm32-unknown-unknown -p game --bin game
    mkdir -p build/web
    wasm-bindgen --target web --no-typescript --out-dir build/web --out-name game target/wasm32-unknown-unknown/release/game.wasm
    cp web/index.html build/web/
    ln -sfn ../../assets build/web/assets
    @ls -la build/web/game_bg.wasm
    {{ if serve == "yes" { "cd build/web && python3 -m http.server 8080" } else { "true" } }}

# Encode the Ultra surfaces (tools/textures/ultra_pack.txt) to KTX2.
textures-pack:
    cargo run --release --manifest-path tools/texpack/Cargo.toml -- pack tools/textures/ultra_pack.txt
