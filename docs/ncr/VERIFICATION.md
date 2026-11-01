# NCR verification — 2026-10-31

Verified with seed 7 unless specified. This is a playable procedural blockout, not final environment art.

- Release client and shared tools build successfully offline.
- Three shared library tests pass: deterministic/bounded generation (including seed extremes), cross-district navigation without wading, and scaled Pralaya coverage/closure.
- All ten physical traversal checks pass: north/central/south bridges; both underground exits; the full tunnel; mall interior; mall roof stair; viaduct stair; film studio interior.
- The bridge deck uses short invisible collider panels under one continuous visual mesh. This resolves a reproducible capsule sweep stall on the original long central deck collider.
- Cross-process peer check at match seed 1234 passes: identical seeds agree, different seeds produce different matches.
- Final 16-warrior headless match completes at 417 simulation seconds: 10 weapon eliminations, 5 Pralaya eliminations, one survivor. It took approximately 27.3 wall-clock seconds; this is not a rendering frame-rate measurement.
- Ten final in-engine views captured with Mac GPU access and collected in `in-engine-preview.png`. No missing-asset errors were logged. The renderer logged two empty-mesh warnings and a window-close warning; these did not prevent captures.
- `git diff --check` passes.

Outputs: `map-atlas.svg`, `map-atlas.png`, `in-engine-preview.png`, and the approved `world-concept.png`.

Limits: one final seed-7 match is a smoke test, not a balance study. The peer check verifies deterministic setup, not a full online session. Roofs/metro do not yet have bot navigation. No measured GPU frame-rate claim, moving trains, weather/flooding, modern outfits or finished facade art.
