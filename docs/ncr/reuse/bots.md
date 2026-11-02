# Bots: what we can reuse (1 Nov 2026)

Read in read-only clones. Nothing was built or run, so no pick has been compiled with our project. Hours saved are guesses.
GitHub search hit its rate limit, so a better repo may exist. `bots.rs` changed while I read it, so I name functions, not lines.

## Short answer
Ring, flight and looting are already built (`zone.rs`, `flow.rs`, `loot.rs`), so copy nothing there. The weak part is
how bots aim, remember and choose: do the picks below in order, about 29 hours saved. A tree alone will not make bots
smarter; picks 1 and 2 do that. Only the owner peer runs a bot's brain (`net.rs`), so nothing here must be deterministic.
Copied code keeps its licence note, and changed Zlib or Apache-2.0 files must say they were changed.

## 1. Aim: s0pk/lgduel
- https://github.com/s0pk/lgduel, `crates/bot/src/lib.rs`. MIT. Last commit 2026-10-25. One star, so judge the code alone.
- Copy `AimSkill` and its presets, the `seen` queue, `perceive`, and the `--- aim ---` block in `think`.
- Change: degrees to our radians, its game types to our query data. Keep our lead, bow drop and aim-ray maths. Its
  `gain` is per tick at 125 Hz, so rescale it. Drop its noise, which re-rolls every tick. Map `Chill` to `EASY`.
- Why: `drive_bots` snaps `intent.yaw` to the target and re-rolls the wobble every tick, with no turn limit. Saves about 4 hours.

## 2. Aim error, memory, goal list: redeclipse/base
- https://github.com/redeclipse/base, `src/game/ai.cpp`, `src/game/ai.h`. Last commit 2026-10-30. C++, so port the logic.
- Licence: Zlib for code (`doc/license.txt`). Its maps, models, sounds and name are not covered.
- `getaimpos`: add aim error on a random timer, not every tick. Change: hold the error until the next roll; theirs lasts one call.
- `hastarget`: fire only when the view is near the aim point. Needed once aim has a turn limit.
- `enemyseen` in `violence` and `process`: stay on an enemy for 1 to 2 seconds after losing sight. Ours drops it at the
  next think. Skip the distance test on that `violence` line: it refuses a closer enemy, which looks like a bug.
- `find` and `items`: score each wanted thing, try the best, and try the next if there is no route. This replaces our fixed chain.
- `avoid` and `avoidset`: route around other bots and blast areas. Leave this until the nav work settles.
- Change: milliseconds to seconds, skill 0-100 to `BotSkill`, waypoints to `heading`. Saves about 10 hours.

## 3. Tree: Sollimann/bonsai, crate `bonsai-bt` 0.14.0
- https://github.com/Sollimann/bonsai, `bonsai/src/bt.rs`, `bonsai/src/behavior.rs`. MIT. Last commit 2026-09-22.
- No Bevy dependency and no required crates, so Bevy 0.20 cannot break it. Add the crate with the `f32` feature; copy nothing.
- Give each bot a `BT` in its own component and tick it inside the `drive_bots` loop, with a `match` calling our existing code.
- Change: root is `Select(...).memory(false)`, so ring and fight are rechecked each tick. Call `reset_bt` when the root ends. Saves about 8 hours.
- Second choice: `hyranno/bevior_tree` 0.12 (MIT or Apache-2.0, 2026-10-15), the only tree crate I found on Bevy 0.20. Its
  `update` takes the whole `World` and each action needs its own system, so `drive_bots` would have to be split up.
- Not on 0.20: `bevy_behave`, `kisya_edbt` (0.19), `seldom_state`, `saddle-ai-*` (0.18, MIT-0), `dogoap`, `big-brain` (0.17). Beehave is Godot GDScript.

## 4. Taking cover: KieranCoppins/Post-Navigation-System
- https://github.com/KieranCoppins/Post-Navigation-System. MIT. Last commit 2024-04-20. Unity C#, so port the logic.
- Copy `Runtime/PostSelector.cs`, `Runtime/Rules/` and `CalculateCoverRaycast` in `Runtime/PostGenerators.cs`: weighted
  rules score spots, and low and high rays find cover with room to peek.
- Change: Unity rays to `SpatialQuery`, NavMesh spots to `NavGrid` spots near the bot. Cap rays per tick. Saves about 6 hours.

## 5. Score curves: zkat/big-brain
- https://github.com/zkat/big-brain, `src/evaluators.rs`, `src/measures.rs`. Apache-2.0. Last commit 2025-10-07. Archived
  here, moved to Codeberg, still on Bevy 0.17, so copy the two files and do not add the crate.
- Change: remove Bevy and `Reflect`, keep the licence note. Use for flee and goal scores. Saves about 1 hour.

## Nothing good exists
- Ring and drop flight: ours is already ahead. `chad/ascii-royale` (MIT, 2026-06-19) has only a 2D grid `zone.rs` and `bot.rs`.
- Fight or flee: nothing to copy. `ai.cpp` has no flee or cover logic, so we write this score ourselves, 3 to 4 hours.
- Open battle royale with good bots: none found. `aratama/magiaforge` uses Bevy, but its licence says it is not open source.

## Read for ideas, never copy
- Unvanquished (GPL): `pkg/unvanquished_src.dpkdir/bots/subroutine_fight_or_flight_humans.bt`, the clearest fight-or-heal tree I saw.
- mrDIMAS/StationIapetus (GPL-3): `game/src/bot/behavior/`, a Rust shooter. xnetcat/BattleRoyaleOS (no licence): `kernel/src/game/bot.rs`.
- Not read: Xonotic, ioquake3, Veloren, survev, suroi (GPL or unclear). AssaultCube says "zlib-like"; I did not read its full licence.
