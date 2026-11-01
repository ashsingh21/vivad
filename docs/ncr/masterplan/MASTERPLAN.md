> **Superseded.** The one plan is `docs/PLAN.md` in the vivad repo. This file is kept for reference only.

# NCR / Kali Yuga — 4 km production map design

Version 1 · 31 October 2026 · **Design blueprint; not integrated into the playable map.**

The confirmed brief is a **4,000 × 4,000 metre arena (16 km²)** for Apple Silicon Mac and high-end Android gaming phones. The visual target is grounded contemporary NCR with mythic weapons and readable third-person combat. The map borrows the useful principles of named destinations, contrasting terrain and deliberate rotations from battle royale games; its architecture, routes and artwork are original.

The canonical spatial data is [ncr-4km.plan.json](ncr-4km.plan.json). The [atlas](ncr-4km-atlas.svg) is generated from that data by `tools/maps/ncr_masterplan.py`. It depicts an authored, compressed fictional region, not surveyed city boundaries or the real metro network. All coordinates use metres: east is +X, north is −Z, and the playable bounds are −2,000…+2,000 on each axis.

## 1. Scale and match structure

| Item | Design decision |
| --- | --- |
| Full playable area | 4 × 4 km; 3.31 times the current 2.2 × 2.2 km area |
| Main destinations | 16; four per district |
| Small compounds | 24 planned between destinations; provide supplies and short cover routes |
| Full-map population target | 64, admitted only after 16 → 32 → 64 simulation, network and rendering gates |
| Initial 16-player play space | Approximately 1.5 × 1.5 km around a rotating district cluster |
| Match duration target | 18–22 minutes for the full map; tune through future playtests |
| Detailed production slice | 600 × 600 m around Delhi's Ring Bazaar |
| River crossings | Four above-ground crossings plus one underground pedestrian crossing |
| Buildings | A small set of playable hero interiors; background buildings use convincing sealed facades |

One engine unit represents one metre. Enlarge the world by adding neighbourhoods, green belts and transition space. Roads, cars, doors and stairs keep their physical dimensions. The existing global horizontal scale transform must not become the generator for this map.

## 2. Geographic composition and combat

Delhi sits northwest, Gurugram southwest, Ghaziabad northeast and Noida southeast. A gently meandering Yamuna corridor separates the banks. The ridge provides the western boundary; its 25–75 m hills form broken sightlines and a distant silhouette. Neighbourhood boundaries blend through workshops, service roads and gardens rather than straight walls.

| District | Destinations | Combat and visual character |
| --- | --- | --- |
| Delhi | D1 Ring Bazaar; D2 Metro Interchange; D3 Old Lanes; D4 Civic Gardens | Arcades, courtyard loops, shaded lanes and a legible ring road. Roofs are 3–22 m high; stairs and descents are visible. |
| Gurugram | G1 Glass Quarter; G2 Meridian Mall; G3 Fairway Estates; G4 Ridge Workshops | Tower podiums, service alleys, parking decks and selected 85 m rooftops. Height offers exits and observation, with exposed access routes and counter positions. |
| Noida | N1 Sector 18 Grid; N2 Film Backlots; N3 Okhla Margins; N4 Campus Depot | Planned blocks, green belts and long streets broken by parked vehicles, walls and planted berms. Wetlands provide raised dry paths and low cover. |
| Ghaziabad | H1 Rail Freight; H2 Society Courts; H3 Foundry Blocks; H4 Vaishali Steps | Parallel rail-yard lanes, loading courts, apartment podiums and stepped station plazas. Cross aisles prevent single-file container corridors. |

Every destination receives a recognisable silhouette, a specific ground material palette, one audible environmental cue, two or more approaches and an exit that does not depend on a moving train. Major destinations are roughly 500–1,000 m apart; the small compounds and local cover chains make those transitions useful space.

### Local layout rules

- A ground route should reach meaningful cover every 20–35 m through open transition space; validate this at player-eye height. Do not place a continuous shield across a high-value sightline.
- Normal fighting lanes aim for 25–80 m views. Intended 120–250 m firing lanes need lateral exits, readable cover and a way to flank the firing position.
- Each hero building has at least two independent ground exits. Each playable roof has an accessible route up and an alternate route down.
- Open courtyards need a covered edge route. Tall positions must expose their user to more than one counter angle.
- Roads carry traffic logic: loading access reaches loading courts, service alleys reach bins and plant rooms, parking ramps connect to parking floors.
- Place clutter according to use and wear. AC units, drains, cables, damp marks, dust and shop stock establish scale; random props across doorways do not.

## 3. Crossings and movement

| Crossing | Position Z | Form | Gameplay requirement |
| --- | ---: | --- | --- |
| C1 North Rail Walk | −1,450 m | 8 m wide, deck +8 m | Separate screened foot lane, independent stairs at both banks; stationary rail scenery initially. |
| C2 North Boulevard | −600 m | 18 m wide, deck +5 m | Staggered hard cover and a lower maintenance route; both approaches can be flanked. |
| C3 Central Interchange | +250 m | 22 m wide, deck +5 m | Split approaches and two usable ramps rather than one exposed funnel. |
| C4 Southern Causeway | +1,330 m | 10 m wide, deck +2.5 m | Two dry raised paths; protected wetland areas remain scenery. |
| C5 Blue Underpass | −120 m | 9 m wide pedestrian corridor, floor −14 m | Two independent exits per bank; separated from any future moving train. |

The river is approximately 140 m wide, with a −1.2 m water surface and −4.5 m bed. The underpass floor at −14 m and roof below the riverbed leave space for an excavated underground structure. Heightfields and collision openings must be authored from these same coordinates; a visible station staircase cannot terminate inside solid terrain.

The Blue corridor connects Old Lanes, Ring Bazaar, Metro Interchange, Vaishali Steps and Sector 18. The Yellow corridor connects Civic Gardens, Ring Bazaar, Glass Quarter and Meridian Mall. These are fictional compressed game routes. Stations have clear pedestrian connections before train movement is introduced.

### Required traversal before full-map release

The current sprint is 7.4 m/s. A straight 4 km sprint would take about **9 minutes**, before combat or terrain. Existing gliding travels at 14 m/s while losing 6.5 m of height each second: a clean 85 m descent provides only about 183 m of theoretical glide travel. A tower does not enable unlimited cross-map movement.

Build short roof ascenders, 90–220 m ziplines at a proposed 12 m/s, and clearly signalled mythic updraft launch points. A proposed 120 m launch at the existing glide parameters offers roughly 258 m of theoretical horizontal range; actual routes need landing margin and must be shorter. Chain these deliberately with covered ground routes. None of these new systems currently exists in the runtime.

The full-map Vimana design target is a 220 m flight height and 45 m/s travel. A central 4 km pass then takes about 89 seconds, versus about 182 seconds at the current 22 m/s. Validate drop reach, streaming preparation and readability before adopting those values.

Vehicles remain parked cover during this phase. Moving trains and passenger vehicles introduce ownership, attachment, collision and replication work and must pass separate gameplay gates.

## 4. Ring Bazaar: the first finished district slice

Build the 600 × 600 m area centred on D1 before producing all 16 destinations to final quality. The atlas highlights its boundary.

1. Establish a 55 m inner courtyard, a 110 m arcade loop and a 190 m outer service loop. Radial alleys connect all three; shop rows interrupt long tangential views.
2. Finish four hero buildings: a two-floor market, a corner office with a roof exit, a service courtyard and a shallow metro entrance. Add a 120 m street connector inside the slice boundary.
3. Author the same stairs, cover, railings and collision for Mac and Android. Cosmetic detail and reflection quality may differ.
4. Dress a complete ground → stairs → roof → alternate descent → arcade loop. Check capsule clearance, camera collision, aim alignment and jump recovery on each connection.
5. Produce final facade, ground, signage, prop and foliage kits in this slice. These become the reusable asset library for the rest of the map.
6. Measure a repeatable 16-player worst-view route on both target device classes before expanding final art production.

## 5. Realism and asset direction

Realism comes from consistent dimensions, material response, believable infrastructure and purposeful wear. Maintain readable human silhouettes against the environment; use muted environmental colour with restrained supernatural effects.

| Element | Authoring target |
| --- | --- |
| Floor-to-floor height | Approximately 3.4 m; commercial ground floors can be taller |
| Main passage clearance | 2.4 m preferred; test the narrowest door and camera collision |
| Doors | Approximately 1.2–1.4 m clear width and 2.2–2.4 m height |
| Stairs | Approximately 0.17–0.18 m risers, 0.28–0.30 m treads; 2–3 m wide where fighting occurs |
| Local two-lane roads | Approximately 7 m carriageway, plus separate pavement and furnishing space |
| Parked passenger cars | Approximately 4.5 × 1.8 m; never enlarged with district coordinates |
| Texture detail | Tiling surfaces around 256 texels/metre near the player; uniquely bake only assets that need it |
| Facades | Shared trim sheets and material atlases; geometry for silhouettes, normals for small surface detail |
| Interiors | Selected navigable rooms, lit exits, believable circulation; sealed background shells have no loot |
| Vegetation | Sparse readable planting near fights; distant clustered trees use lower-detail proxies |
| Signage | Fictional brands, legible local wayfinding, modest material ageing |

Use four core environment kits rather than a unique asset per building. Reuse mesh and material identities for batching. Keep glass opaque or use a cheap surface where no interior exists; avoid overlapping translucent tower shells. Full reflections of neighbouring buildings are an optional Mac enhancement after the base render budget is met.

Keep per-match lighting consistent across platforms. The first mobile-ready art target uses authored daylight presets, baked surface shading/AO and a shared environment light; continuous day/night cycling needs its own budget and visibility validation. Cosmetic seasonal variants must preserve the same collision and viable exits. Combat does not occupy active worship sites, restricted airport areas or military facilities.

## 6. Performance design for both platforms

**These are engineering targets, not measured FPS or compatibility promises.** No game was launched for this design task. Exact Android phone models and sustained thermal measurements are still required.

| Setting | Apple Silicon Mac: balanced | High-end Android: sustained |
| --- | --- | --- |
| Base target | 60 FPS / 16.67 ms per frame | 60 FPS / 16.67 ms per frame |
| Initial internal render size | 1,600 × 900; then test 1,920 × 1,080 | 1,600 × 900; adapt toward 1,280 × 720 if needed |
| Further target | Higher refresh after 60 FPS passes | Optional 90 FPS / 11.11 ms after thermal validation |
| Full-detail radius | 192 m starting point | 128 m starting point |
| Mid-detail radius | 640 m | 384 m |
| Simplified district proxies | To 1,600 m | To 1,100 m |
| Initial visible batch budget | ≤800 | ≤350 |
| Initial visible triangle budget | ≤2.5 million | ≤1.2 million |
| Initial resident texture budget | ≤1,200 MB | ≤600 MB |
| Dynamic shadow starting point | Two cascades / 120 m | One cascade / 70 m |
| Effects | Moderate SSAO and subtle bloom after measurement | Baked AO; optional low bloom; measured effect caps |

These batch, geometry, memory and distance budgets are initial constraints to tune against hardware. They do not predict FPS by themselves. CPU and GPU overlap; measure their timing independently rather than adding them as if they execute sequentially.

### How a large map stays affordable

- Divide the 4 km world into 128 m streaming cells. A 32 × 32 grid spans 4,096 m; the playable square clips 48 m from each outside edge. Load nearby detail and prefetch along fast movement paths.
- Keep a cheap silhouette representation of distant districts. Build lower-detail merged proxies from already simplified assets. The playable world can be large while the fully detailed working set stays small.
- Share meshes and material atlases for windows, AC units, street furniture, trees and parked cars. Prefer one or two material slots per environment asset.
- Stream hero interiors as separate rooms or groups with visible doorway/roof boundaries. Fully hidden interiors do not need full rendering.
- Preserve coarse cover at every distance where shots are possible. A low graphics setting cannot remove a wall, opponent or important cover prop.
- Separate visual residency from collision and authoritative actor simulation. Keep support under the player and query coarse cover along a projectile's complete path. Visible actors and networking cannot be suspended merely because a decorative cell unloads.
- Replace one whole-world ground A* search with a district/crossing graph plus local ground, roof and metro navigation tiles. Reuse navigation storage and stagger bot planning.
- Generate detail in background jobs and prebuild assets offline. Avoid generating mip chains and thousands of new meshes during a combat frame.
- Set an explicit **physical internal render resolution**. A 1,600 × 900 logical Retina window can render at 3,200 × 1,800, which is four times as many pixels. UI size and 3D render size need separate controls.

Epic documents cell streaming, distant grouped representations and draw-call reduction for Fortnite. Those are references for the architecture of this Bevy game; Unreal's World Partition, Nanite and Lumen are not available in this repository and are not assumed in these budgets. See [Streaming and HLODs](https://dev.epicgames.com/documentation/fortnite/streaming-and-hlods-in-unreal-editor-for-fortnite), [Reducing Draw Calls](https://dev.epicgames.com/documentation/fortnite/reducing-draw-calls-in-fortnite) and [Spatial Profiler](https://dev.epicgames.com/documentation/en-us/fortnite/spatial-profiler-in-unreal-editor-for-fortnite).

PUBG's official pages document [Sanhok at 4 × 4 km](https://www.pubg.com/en/game-info/maps/sanhok) and [Erangel at 8 × 8 km](https://pubg.com/en/game-info/maps/erangel). Those dimensions provide scale context; their terrain, layouts, assets, names and device performance are not copied or used as benchmarks for Vivad.

## 7. Existing implementation constraints

The repo uses Rust, Bevy 0.20 and Avian. The current map creates all static map entities/colliders and all architecture chunk meshes at startup. It has material batching and a grass streaming system, but it does not yet have the environment streaming, district proxies or layered navigation described above. The mobile package, Android entry point, asset packaging, touch controls and lifecycle integration also need implementation; a desktop build does not validate Android compatibility.

Current scale issues must be resolved when the generator is replaced:

- `BRIDGES` already multiplies its Z positions by `WORLD_SCALE`, then `scale_horizontal()` multiplies the generated blocks and POIs again. The outer bridges therefore land at approximately ±1,307 m, beyond the current ±1,100 m playable half-extent.
- Terrain excavation still uses an approximately ±170 m metro trench while the station structure is stretched to approximately ±374 m. Roads, water strips, ridge masks and plant/park masks also mix local and scaled world coordinates.
- The navigation grid stretches its 5 m cells with the same world multiplier. Increasing the map multiplier further would reduce local navigation precision instead of adding navigation detail.

The 4 km plan fixes these decisions at the design level by using one canonical metre-based coordinate system. Runtime corrections remain an implementation milestone; these design files do not silently alter the working game.

## 8. Production sequence and acceptance

| Gate | Deliverable | Acceptance |
| --- | --- | --- |
| A — map design | This blueprint, canonical coordinates and atlas | Dimensions, POI roles, crossing connectivity, unique IDs and slice bounds checked offline |
| B — spatial foundation | New 4 km terrain, roads, crossings and district graph | No double scaling; terrain/tunnel/bridge agreement; endpoints and pickups remain in bounds |
| C — performance foundation | Explicit render size, streaming cells, material/mesh reuse, collision proxies | No combat-frame blocking loads; cover and remote actor parity across profiles |
| D — finished Delhi slice | 600 m district at final art quality | Ground/roof/interior traversal, camera/aim agreement, accessible controls and measured Mac/Android performance |
| E — four districts | Reuse the proven kits across all 16 POIs | Physical traversal, rooftop/metro navigation, balanced loot and 16 → 32 → 64 actor gates |
| F — release validation | Device matrix, thermal runs, online stress and recovery | Repeatable rendering/network tests, stable memory and playable visibility across quality tiers |

The 64-player full map also requires its traversal systems. Do not release a 4 km foot-only map with 16 scattered players as the intended final experience.

### Future benchmark protocol

After the user authorises running the game again, test the same map seed, route and camera sequence on an exact Mac model and at least two named Android phones. Warm shader/asset caches separately, then record a ten-minute normal session and a twenty-minute sustained phone run. Include the busiest street, Glass Quarter roof, wetland vegetation, metro interior, a fast streaming boundary and a multi-player effect burst. Collect median and 95th/99th-percentile frame times, 1% low FPS, CPU/GPU time, resident memory, temperature/throttling state and load spikes. Reject black or missing-asset frames before accepting any performance result. Save the renderer resolution, quality profile and build revision with each capture.

The current instruction to keep the game closed remains in force. Offline design/data and document checks are permitted; GPU sessions and gameplay benchmarks were not performed here.
