# Panga: desi battle royale (branch `panga`; `main` is Vivad, the ancient-India version)

Rust + Bevy 0.20 (pinned), assets scripted in Blender 5.2 and exported as GLB.

## Setting up a new machine (to play)

1. **Rust through rustup** (https://rustup.rs), not Homebrew or apt: `rust-toolchain.toml` pins
   the version and rustup fetches it on the first build. A packaged Rust ignores that file and is
   often too old (Bevy 0.20 needs 1.97.1+).
2. **Git LFS and just**: `brew install git-lfs just` (macOS) or `sudo apt install git-lfs just` (Linux).
3. Linux only, the build needs: `sudo apt install build-essential pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`
4. Clone and play (the first build takes several minutes):
   ```
   git lfs install
   git clone git@github.com:ashsingh21/vivad.git && cd vivad
   just play                        # alone, with bots
   just host Arjuna                 # online: prints a code, Enter starts the match
   just join chai-tiger-kulfi-42 Bhima   # online: join with the host's room code (lan=1 on both for a LAN without internet; then the code is long)
   ```

```
just assets            # rebuild every model from tools/blender (bake, LODs, export)
just weapon khanda     # one weapon
just preview khanda    # review renders -> previews/khanda/latest
just view khanda       # load the GLB in Bevy, screenshot, exit
just play              # play the graybox battle royale
just sim 5             # 5 headless bot matches (loop + AI smoke test)
just tour              # screenshot every landmark
just autoplay          # a bot plays as you; screenshots the match
```

## Controls
WASD move, mouse look, Space jump / leap from the Vimana / open Garuda wings,
LMB attack / draw (hold for Pinaka), RMB aim, E open chest / pick up, R reload (R restarts after a match),
1-6 or mouse wheel hotbar, Shift sprint, F3 performance overlay, F4 cycles quality (Low / Medium / High),
Esc releases the mouse. Quality is picked from the GPU at start; `VIVAD_QUALITY=low just play` forces one.
`VIVAD_NO_POST=1` turns off the bloom and vignette, for comparisons.
Maps, all from OpenStreetMap at real scale: Connaught Place (`just play`), Chandni Chowk (`just play map=chandni`),
Hauz Khas (`map=hauzkhas`), India Gate (`map=indiagate`); `map=bharat` is the ancient-India one. Hosting with
`just host Arjuna map=chandni` gives a room code that tells joiners the map. In Chandni Chowk and Hauz Khas up to
50 houses can be entered (break the door): rooms on every floor, stairs to the roof, chests inside.
Each match is on the next Delhi map in turn, unless the players vote: once out of a match (or when it ends), keys
1-4 vote for the next map, and the most votes win (`VIVAD_ROTATE=0` keeps one map).
Gamepad: sticks, A jump, RT fire, LT aim, X interact, Y reload, bumpers cycle.

## In the browser
Chrome or Edge (desktop or Android), or Safari 26+: the game needs WebGPU. One-time setup:
`rustup target add wasm32-unknown-unknown` and `cargo install wasm-bindgen-cli --version 0.2.129`.
Then `just web` builds it and serves it at http://localhost:8080/: a menu to play solo, make a room
(its code shows on screen) or join one by code. Browser and desktop players play together: a room
made in either can be joined from the other (through iroh's relays; a room only just made can take
10-15 s to be found). The menu only sets the address, which can be used directly: `?solo&map=chandni`,
`?host&name=Ashu&map=hauzkhas` (`?host=4` starts at 4 players; otherwise Enter starts),
`?join=chai-tota-ludo-42&name=Ashu`, `&bots=6` for bots online. The browser starts on High quality
and steps down while it measures under 45 fps; `&quality=low|medium|high` fixes it.

Blender path comes from `.env` (`BLENDER=...`, see `.env.example`).
Weapon axis convention and all reference dimensions: `docs/references.md`.
