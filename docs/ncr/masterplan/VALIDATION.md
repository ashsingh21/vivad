# Masterplan verification

The checks in this folder validate a **design blueprint**, not the playable engine map.

- The canonical square is 4,000 × 4,000 metres, with a 16 km² area.
- Sixteen unique major POIs are present, with four per district. Their planned footprints remain inside the arena and touch the local road catchments.
- The twelve road polylines form one connected centreline network through shared junctions.
- All five crossing centres remain inside the map. Each above-ground crossing is on a road that crosses the river. The underground floor has vertical separation from the riverbed.
- Twenty-four reproducible micro-compound locations avoid the river corridor, major POI footprints and each other.
- The 600 m production slice and the 128 m streaming-grid arithmetic are consistent with the world bounds.
- The interactive preview passed 48 selection checks across 320, 736 and 1,024 px browser viewports, with no JavaScript errors or horizontal overflow.
- The static atlas and the light/dark interactive previews were rendered and visually inspected.
- Python compilation and `git diff --check` passed.

Regenerate the vector atlas with `just ncr-plan`. Recheck spatial constraints with `just ncr-plan-check`. `validation.json` contains the machine-readable spatial result; `micro-compounds.json` records the derived small-compound positions.

No game executable, GPU gameplay session, headless battle simulation or Android build was run during this task. FPS, physical traversal, network capacity and actual device compatibility remain unverified for the proposed map. The existing playable map was not replaced by these files.
