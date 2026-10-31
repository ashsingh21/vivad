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
Maps, all from OpenStreetMap at real scale: Connaught Place (`just play`), Chandni Chowk (`just play map=chandni`),
Hauz Khas (`map=hauzkhas`), India Gate (`map=indiagate`); `map=bharat` is the ancient-India one. Hosting with
`just host Arjuna map=chandni` gives a room code that tells joiners the map. In Chandni Chowk and Hauz Khas up to
50 houses can be entered (break the door): rooms on every floor, stairs to the roof, chests inside.
Gamepad: sticks, A jump, RT fire, LT aim, X interact, Y reload, bumpers cycle.

Blender path comes from `.env` (`BLENDER=...`, see `.env.example`).
Weapon axis convention and all reference dimensions: `docs/references.md`.
