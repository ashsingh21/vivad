# Transit reuse: trains, ziplines, platforms, vehicles (1 Nov 2026)

No train or zipline for Bevy or Avian exists to copy, so we join small pieces. Hours saved are my estimates. I built and ran nothing.
GitHub repo search was rate limited: `avian vehicle`, `avian car` and `bevy transit` never ran there. crates.io and web search stood in, so a small repo may be missed.

## How this fits our game
- `move_warriors` already uses Avian's `MoveAndSlide`, so we need no controller crate, only the carry logic. Avian 0.8 has none: `move_and_slide.rs` and its example never mention platforms.
- Every peer runs the whole match, so a train's place comes from `Match.elapsed`, with no train messages. In `SimSet::Flow`, set `LinearVelocity` to (next place - place now) / dt. Avian then moves it on the same tick as the warriors.
- `MoveMode::Vimana => continue` in `movement.rs` is the pattern for a zipline.

## 1. Moving platforms (the train floor)
**Copy: bevy_ahoy.** https://github.com/janhohenheim/bevy_ahoy. MIT or Apache-2.0. Last commit 2026-08-06. Bevy 0.19, Avian 0.7.
- `src/kcc.rs`: `calculate_platform_movement` (lines 1289-1321) finds how far the touch point moves this tick, turning included. Lines 353-377 add that speed, slide, then take it out again.
- Change: `feet_probe` must also return `hit.entity`; keep it in `Kinematic`. Do the same add and take out around our `move_and_slide`, so walls still stop a rider. Saves about 4 hours.
**Copy: happy_feet.** https://github.com/atornity/happy_feet. MIT or Apache-2.0. Last commit 2026-03-23. Bevy 0.18, Avian 0.6.1.
- `src/moving_platform.rs` (205 lines): take the `ColliderOf` lookup from collider to body (a carriage has many colliders) and `apply_inherited_velocity_on_ground_leave` (a rider who jumps off keeps the train's speed).
- Change: do not copy the turn rate in `update_physics_mover`. It subtracts scaled axes, which breaks where the angle wraps. Take the turn rate from the route. Saves about 2 hours.
**Copy for the first test: bevy-tnua.** https://github.com/idanarye/bevy-tnua. MIT or Apache-2.0. Last commit 2026-10-30. Bevy 0.20, Avian 0.8.
- `demos/src/level_mechanics/moving_platform.rs` (87 lines): a kinematic block looping through waypoints. Take only this file: Tnua itself would replace `move_warriors`. Saves about 1 hour.
**Our own work, nothing to copy:** `WarriorState` sends a world `pos`, and peers' `elapsed` may differ. A rider must also send its train and its place on the train, or it will look off the train on other screens.

## 2. Trains
**Best match, read only: Unity Metro.** https://github.com/Unity-Technologies/DOTS-training-samples. Last commit to this folder 2020-08-11.
- `Original/Metro/Assets/src/`: `Train/Train.cs`, `MetroLine.cs`, `Platform.cs`, `Commuter.cs`. It has lines, carriages, doors and commuters, but it is under the Unity Companion License for Unity-dependent projects, so we cannot copy it. Take only the idea of its stop states: arriving, doors open, unloading, loading, doors close, departing.
**Copy: Mini Tokyo 3D.** https://github.com/nagix/mini-tokyo-3d. MIT. Last commit to `src/map.js` 2026-10-25. JavaScript and GLSL.
- `src/gpgpu/compute-fragment.glsl` lines 107-111 turn the clock into progress along a section: speed up, cruise, brake. `src/map.js` lines 1669-1701 get section time from distance, top speed and acceleration, short hops included.
- Change: about 40 lines of Rust on `Match.elapsed`, plus a door wait at each station. Drop Mapbox, live delays and timetable clamping. Saves about 3 hours.
**Use: Bevy's `bevy_curve` 0.20.0.** Already in our `Cargo.lock`. MIT or Apache-2.0.
- `src/cubic_splines/mod.rs`: `LinearSpline`, `CubicCardinalSpline`, `iter_positions`. It has no arc length lookup, so we add a distance table along the route, about 30 lines. Saves about 3 hours.
**Later, for bots who ride: A/B Street.** https://github.com/a-b-street/abstreet. Apache-2.0. Last commit to this file 2023-07-20.
- `sim/src/transit.rs`: `bus_arrived_at_stop` (lines 126-216), `ped_waiting_for_bus` (line 242). Copy the shape: a waiting list per stop, riders with their stop, board and alight. Strip its trip and event code. Humans just walk on. Saves about 1 hour.
Rejected: https://github.com/nickyfoster/train-dispatcher. No licence, 2D, Bevy 0.14, last commit 2024-11-14.

## 3. Ziplines
Nothing in Rust. GitHub found only Godot and Unity scripts.
**Copy: BellaSDK.** https://github.com/Neo-DannyDeTour/BellaSDK. MIT. Last commit to this file 2026-10-26. GDScript. Straight lines only.
- `player/state_zipline.gd` (303 lines): `_calculate_initial_progress` (grab at the nearest point), `_calculate_movement`, `_check_dismount_conditions` (ends, jump, crouch, 0.5 s grace), `_perform_dismount` (keeps speed). Its numbers: 8 m/s, 14.4 fast, 4 climbing, hang 2 m.
- Change: port to a `MoveMode::Zipline` arm in `move_warriors`, about 80 lines. Keep line ends in `MapLayout`, as doors are, so peers agree. Check the `MoveMode` uses at `bots.rs:461` and `flow.rs:421`. Drop tweens, camera code and `print` calls. Saves about 3 hours.
**Copy: bevy_locomotion.** https://github.com/Nub/bevy_locomotion. Last commit 2026-08-18. Bevy 0.19, Avian 0.7. MIT or Apache-2.0 in `Cargo.toml`, but no LICENSE file.
- `src/player/ladder.rs` (126 lines): `shape_intersections` finds a marked sensor and jump leaves it, the same grab and leave in Rust on Avian. Set our `MoveMode`, not its `OnLadder` component. Saves about 1 hour.
Not used: yaelatletl/MovementShooterController `data/prefabs/ZipLine.gd` is MIT but Godot 3 and weaker. hellhop is GPL-3.0. CWaW has a custom licence. Y-Less/Zipline, frkatona/Custom-Godot-Character and geegaz/GClimbingSystem-Ziplines have no licence.

## 4. Vehicles (later)
**Best code, read only.** Neither repo has a LICENSE file, or a licence in `Cargo.toml` or the README, so we cannot copy them unless the author adds one.
- https://github.com/mbuffa/bevy-game-bits `src/vehicle/` (about 2,000 lines): a raycast car made for Avian. Last commit 2026-09-25. Bevy 0.18, Avian 0.6. Worth asking the author for a licence.
- https://github.com/nicolas-found42/rust-racer `src/vehicle.rs`, `core/src/sim.rs`: a drift car. Last commit 2026-10-26. Bevy 0.17, Avian 0.4.
**Copy: Rapier.** https://github.com/dimforge/rapier. Apache-2.0. Last commit 2026-10-10.
- `src/control/ray_cast_vehicle_controller.rs` (823 lines): suspension, engine, brakes and side friction. Wiring is in `examples3d/vehicle_controller3.rs`. The maths already reads like glam.
- Change: swap `cast_ray_and_get_normal` for `SpatialQuery::cast_ray`, `apply_impulse_at_point` for Avian's `apply_linear_impulse_at_point`, and `mprops` for `ComputedMass` and `ComputedAngularInertia`. Keep the Apache licence and mark our changes. Saves about 12 hours.
- Warning: `crates/shared/src/sim/mod.rs:424` sets `SubstepCount(1)` because nothing is a dynamic body today. A dynamic car changes that, so test it first.

## Order of work
Prove each step in the real game before the next. About 30 hours saved in all. Keep the licence notice with any code we copy.
1. Test block plus carry: a warrior stands on it and rides. 2. One train on the Yellow Line walkway. 3. One zipline. 4. Vehicles, when wanted.
