# Vivad: battle arena (ancient India vs ancient China)

Rust + Bevy 0.20 (pinned), assets scripted in Blender 5.2 and exported as GLB.

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
1-6 or mouse wheel hotbar, Shift sprint, F3 performance overlay, Esc releases the mouse.
Gamepad: sticks, A jump, RT fire, LT aim, X interact, Y reload, bumpers cycle.

Blender path comes from `.env` (`BLENDER=...`, see `.env.example`).
Weapon axis convention and all reference dimensions: `docs/references.md`.
