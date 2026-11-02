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
   just play                        # the NCR map, you and 63 bots
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
WASD move, mouse look, Space jump, leap from the Udta Auto, open the patang. LMB attack or draw, RMB aim,
E or F open chest, pick up. R reload. 1-6 or wheel hotbar, Shift sprint. M whole map, N zoom.
F3 performance, F4 quality (Low, Medium, High, Ultra). Esc menu. Enter presses the big button.
Quality is picked from the GPU at start; `VIVAD_QUALITY=low just play` forces one.
`VIVAD_NO_POST=1` turns off the bloom and vignette, for comparisons.

`VIVAD_VIMANA=pushpaka` brings back the Pushpaka Vimana as the drop ride instead of the Udta Auto (the default).
The game has one map: the real Delhi NCR from OpenStreetMap, 4 km across, with Connaught Place, Chandni Chowk,
Hauz Khas and India Gate merged into it (`just play`). `just host Arjuna` gives a room code on it. The four small
maps stay on their own for tests and practice, one at a time: `just map chandni` (or `cp`, `hauzkhas`,
`indiagate`), `VIVAD_MAP=<name>` for the other tools, and `just host-map chandni Arjuna` gives a code that tells
joiners that map. A small map stays until the game closes; only the NCR is in the rotation. In Chandni Chowk and
Hauz Khas up to 50 houses can be entered (break the door): rooms on every floor, stairs to the roof, chests inside.
Gamepad: sticks, A jump, RT fire, LT aim, X interact, Y reload, bumpers cycle.

## In the browser
Chrome or Edge (desktop or Android), or Safari 26+: the game needs WebGPU. One-time setup:
`rustup target add wasm32-unknown-unknown` and `cargo install wasm-bindgen-cli --version 0.2.129`.
Then `just web` builds it and serves it at http://localhost:8080/: a menu to play solo, make a room
(its code shows on screen) or join one by code. Browser and desktop players play together: a room
made in either can be joined from the other (through iroh's relays; a room only just made can take
10-15 s to be found). The menu only sets the address, which can be used directly: `?solo&map=chandni`,
`?host&name=Ashu&map=hauzkhas` (`?host=4` starts at 4 players; otherwise Enter starts),
`?join=chai-tota-ludo-42&name=Ashu`, `&bots=6` for bots online. The browser starts on Connaught Place, not the NCR, which takes about 3.7 GB on a Mac and has not been tried in a tab. The browser starts on High quality
and steps down while it measures under 45 fps; `&quality=low|medium|high` fixes it.

Blender path comes from `.env` (`BLENDER=...`, see `.env.example`).
On macOS, use `BLENDER=/Applications/Blender.app/Contents/MacOS/Blender`.
Rebuilding characters and carved figures also requires the
[MPFB extension](https://extensions.blender.org/add-ons/mpfb/) to be enabled and the
[MakeHuman system assets](https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html)
pack to be installed through MPFB. The scripts resolve asset paths from MPFB's settings.
Weapon axis convention and all reference dimensions: `docs/references.md`.

## The NCR map

`just play` launches the one final map: Delhi, Gurugram, Noida and Ghaziabad from real OpenStreetMap data, 4 km across, 19 districts and 25 named spots with open land between them, joined by the Yamuna and Hindon bridges. A match on it is 64 warriors, you and 63 bots (`VIVAD_BOTS=15 just play` for a smaller one), and the first ring covers the whole map. `just ncr-tour` captures it; `just ncr-walk` checks every bridge and pass with the warrior's real capsule; `just ncr-sim` plays a headless 64-player match, `just ncr-sim 1 31` a 32-player one.

See the [plan](docs/PLAN.md) (Section A), the [map atlas](docs/ncr/map-atlas.svg) drawn from the game's own data, and the [list of holy sites](docs/ncr/HOLY-SITES.md). It has no metro, vehicles or seasonal flooding yet.

The old fictional [4 × 4 km masterplan](docs/ncr/masterplan/MASTERPLAN.md) was overridden by the real map and stays for reference: its layout rules and budgets still apply. `just ncr-plan` regenerates its atlas, and `just ncr-plan-check` validates its layout without launching the game.
