# Vivad: battle arena (ancient India vs ancient China)

Rust + Bevy 0.20 (pinned), assets scripted in Blender 5.2 and exported as GLB.

```
just assets            # rebuild every model from tools/blender (bake, LODs, export)
just weapon khanda     # one weapon
just preview khanda    # review renders -> previews/khanda/latest
just view khanda       # load the GLB in Bevy, screenshot, exit
just run               # game
```

Blender path comes from `.env` (`BLENDER=...`, see `.env.example`).
Weapon axis convention and all reference dimensions: `docs/references.md`.
