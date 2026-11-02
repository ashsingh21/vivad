> **Superseded.** The one plan is `docs/PLAN.md` in the vivad repo. This file is kept for reference only.
>
> **This describes the old blockout.** Everything below is the 2.2 km authored arena in `crates/shared/src/map/ncr.rs`, which the game no longer plays. Since 1 Nov 2026 `MapId::Ncr` builds the real 4 km map from OpenStreetMap in `crates/shared/src/map/compose.rs`: see `docs/PLAN.md`, Section A. The `just` commands below now run that new map, and the numbers, districts, bridges, metro and checks below do not describe it.

# Vivad — NCR / Kali Yuga

A fictional modern NCR arena with divine weapons, rooftop gliding and a closing Pralaya. This document describes the implemented first playable map and the remaining production work. The approved image is in `world-concept.png`; it is an art target, not a screenshot.

## Play and inspect

```sh
just ncr                 # modern arena, seed 7
just ncr 42              # same streets, different seeded tower heights
just ncr-tour            # ten clean in-engine captures in previews/ncr
just ncr-walk            # physics checks of bridges, stairs and interiors
just ncr-sim 1 15        # complete headless match
cargo run --release -p shared --bin ncr_atlas
just play               # the NCR, the default map (just play map=cp for Connaught Place alone)
```

The new version is selected by `VIVAD_MAP=ncr`. Both online peers must select it; join codes reject a mismatched map choice. Regenerate old invitation codes after this protocol change. No original map assets are overwritten.

The current seed-7 build contains 5,662 geometry blocks, 15 named locations and 191 chests. The atlas is exported from that layout.

## Scale and orientation

The playable square is **2,200 × 2,200 metres**, centred on the origin. North is negative Z, east is positive X. The recipe is authored as a 1 km blockout and stretched 2.2× horizontally at the end of generation (`WORLD_SCALE` in `crates/shared/src/map/ncr.rs`); heights, stairs and ramp rises are unchanged, so climbing times stay the same while rotations are longer. Coordinates in this document are world metres, after scaling. It is about twenty times the old map area. A straight kilometre takes about 135 seconds sprinting at 7.4 m/s, before obstacles. Skyline height varies from 32 to 72 m; Vimana altitude remains 120 m, above the skyline.

The terrain has 257 samples per side over 2,288 m (about 8.9 m per cell); it is generated directly in world metres. The western ridge supplies the only substantial natural elevation. Buildings, ramps and platforms use separate colliders.

## District recipes

| District | Primary spaces | Combat role | Access and loot |
|---|---|---|---|
| Delhi / northwest | CP's two ring roads, twelve white arcades, central park; Old Delhi roof lanes; civic market | Short rotating engagements, cover around a curved street, escape through arcades | Open-sided hero interiors, external stairs and roof chests; street-level loot beside homes |
| Gurugram / southwest | Twelve glass towers, Cyber Hub streets, Skycourt Mall, parking pavilion, golf greens | Vertical positioning, exposed glides, street-level counterplay | Towers are solid shells with rooftop loot reached by air; mall and parking roofs have stairs |
| Noida / southeast | Four-by-three sector blocks, broad avenues, green belts, three film studios, wetland boardwalk | Long bow lanes broken by blocks and greenery; close fights inside studios | Selected sector buildings and film studios are enterable; roadside loot supports ground rotations |
| Ghaziabad / northeast | Eight apartment towers, three rail alignments, container stacks, two warehouses | Dense ambush lanes, cross-aisles and warehouse escapes | Ground loot in freight aisles; containers are solid cover; warehouses have open entrances and roof stairs |

There are no combat worship sites, airport areas, military sites or real shop brands. The recipe is authored from scratch and uses no extracted OSM data.

## River and transportation

The Yamuna meanders through the middle. Its three road bridges sit at Z = −594, 0 and +594 m. Each bridge has two approach ramps, a deck approximately 1.6 m above ground, piers and intermittent protective blocks. The bed is intentionally shallow and traversable in this build: bridge crossings are faster and more legible, but they are not the only legal path. There is no swimming simulation.

The Blue Line is a **walkable underground river underpass**, from X = −374 to +374 around Z = 0. Its floor is at −8 m; stairs and raised entrance landings connect both ends to the street. A roof and terrain excavation form a true underground space, with cool lighting and high-value loot. It is not yet the complete multi-station Blue Line network.

The Yellow Line is an elevated 858 m walkway at X = −176, with three platforms, stairs and loot. Its deck is at about 8.3 m. No moving train, passenger ownership or timetable is implemented. The rail-yard tracks are static scenery.

## Readability and materials

The client has separate concrete, asphalt, glass, steel, transit-blue and transit-yellow materials. Blue lintels mark hero interiors. Vertical mullions and horizontal window ribbons distinguish glass offices from painted housing. Roof tanks and mechanical blocks provide scale and limited cover. Reused neem trees, street-light silhouettes, road dashes, planters and park planting add street context.

Static architectural visuals use the existing 64 m material batching. Small details have no colliders. Major shells and cover use simple boxes; stairs use visual treads plus smooth invisible ramps. Water is non-colliding and uses the existing animated ripple material with a cooler NCR tint.

This remains procedural blockout art. Custom shop signage, cable bundles, road markings beyond dashed centre lines, facade textures, polished train models and modern character outfits are future art work. Existing characters and arsenal remain in use.

## Match and AI integration

Pralaya starts at a radius proportional to map size. Phase radii scale with the map and phase durations scale with the square root of the size ratio. Drop prompts and bot drop limits use `MapLayout.half_extent` instead of the old 230/240 m constants. River ambience and footstep water detection use the NCR river curve.

Bots have a conservative 5 m ground navigation grid and cached A* street paths. Bridge lanes are traversable while the river is excluded from planned routes. Ground bots skip elevated chest goals, and initial drop targets prefer open-sky ground loot. This is **ground-layer navigation**, not a multi-storey navmesh: bots do not deliberately plan routes through the metro, onto the viaduct or up building stairs. Local steering and stuck recovery remain in use. Combat and avoidance still require playtesting across seeds.

## Production gates

1. Validate physical access: all three bridges, both underpass exits, the underground lane, mall, studios, roof access and viaduct stairs.
2. Validate map generation: same seed yields the same blocks; finite geometry, bounded loot and cross-district street routes.
3. Run complete bot matches and inspect weapon eliminations versus zone deaths. A completed match alone does not prove good encounter density.
4. Inspect all ten in-engine camera views; fix intersecting geometry, blocked entrances, unreadable cover and dark interiors.
5. Profile live play on the target machine before claiming a frame-rate target. The map has no measured shipping performance guarantee yet.
6. Human playtest CP and Cyber Hub first: time to loot, time to first encounter, rooftop escape choices and bridge pressure.

Remaining systems: moving trains, seasonal weather/flooding, multi-level AI navigation, bespoke modern outfits, final facade/prop art, audio polish and a full multiplayer playtest. The full four-district layout exists now; these should not be mistaken for completed features.

## Skill references

Applied community guidance: [Game Design](https://github.com/sickn33/antigravity-awesome-skills/blob/main/skills/game-development/game-design/SKILL.md) and [3D Game Development](https://github.com/sickn33/antigravity-awesome-skills/blob/main/skills/game-development/3d-games/SKILL.md). Used for playable-loop validation, simple collision geometry and batched rendering; repository APIs remain the implementation authority.

## Verified build

See [verification results](VERIFICATION.md), [the map atlas](map-atlas.svg) and [actual engine captures](in-engine-preview.png). All ten traversal checks and the final complete bot match pass. The final match produced 10 weapon eliminations and five Pralaya eliminations; this is a smoke-test result, not a balance guarantee.
