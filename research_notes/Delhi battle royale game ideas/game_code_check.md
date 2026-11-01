# Game code check: what the code already supports

Date: 1 Nov 2026, read between about 09:20 and 09:40. Method: reading the code and data only. Nothing was built, run or changed apart from this file. No web research.

Short path forms: `sim/` is `crates/shared/src/sim/`, `map/` is `crates/shared/src/map/`, `shared/` is `crates/shared/src/`, `game/` is `crates/game/src/`, `net/lib.rs` is `crates/net/src/lib.rs`.

Size key, for one developer working with AI: **small** = days, **medium** = weeks, **large** = months. All sizes are estimates from reading the code.

Two warnings:

- Line numbers are for the working tree, which has uncommitted changes. `sim/bots.rs`, `sim/nav.rs` and `sim/zone.rs` changed while this check ran (plan step 7, bot navigation, looks to be in progress), so those numbers can drift. Search by function name if a line looks wrong.
- Claims about how play behaves come from reading the code, not from a match. The ones worth proving first are listed at the end.

## Short answers

| Owner wish | Today | Main gap | Size |
|---|---|---|---|
| Autos, cars, tractor | Parked props only. No driving, no seats, no tractor or Delhi car model | A vehicle system, plus a model and sound per vehicle | Medium: 3 to 5 weeks for four vehicles online, no bot drivers |
| Sniper | The rules work from `items.ron`: damage, drop, headshot, loot, online | Model, sound, scope view, four small code lists, bots that answer past 70 m | Small: 2 to 4 days |
| Crouch | A button constant with nothing behind it | Key, capsule, speed, clips | Small: 3 to 5 days. Prone is medium |
| Hiding from bots | Bots see all round. They have no hearing and no memory | View cone, hearing, last-seen memory | Small: 2 to 4 days, after step 7 |
| Bushes and cover | Bush models exist, but no map places them. Leaves and grass stop nothing | Placement and a hide rule | Small: 3 to 5 days |
| Doors | Can only be broken. Only houses in Chandni Chowk and Hauz Khas have them | Open and close, and bot paths through them | Small to medium: about 1 week |
| Underground | Only on the NCR map, which is out of the rotation. No map has a stepwell | A basement kit and bot paths below street level | Medium: 1 to 4 weeks by scope |
| Slang text | Many Hinglish lines in code. Item lore is never shown. One kill verb | More lines, show the lore | Small: 1 to 3 days |
| Voice lines | None. No voice, barks, chat, ping or language files | Triggers are small. Recording is the real cost | Medium |
| Street fight | One melee weapon, one swing pair, no block or dodge | More melee items, new moves | Small for items, medium for moves |
| Delhi map detail | 4 real-scale maps, 2 built landmarks, 14 street kit pieces | Landmarks, moving life, street sound | Small per piece, medium per landmark |

## Correction to the brief

There is no stepwell in Connaught Place. `map/cp.rs` builds a park, a fountain, a flag and the ring of blocks (`map/cp.rs:52-179`). The stepwell was on the Bharat map, which commit `ca78f57` removed. Its code can still be read at `git show ca78f57^:crates/shared/src/map/india.rs`, from line 941. The model is still on disk at `assets/models/props/stepwell.gltf`. The terrain still has a `pits` option (`map/terrain.rs:181-183`, `map/terrain.rs:250-254`), but the Delhi maps pass an empty list (`map/delhi.rs:123`) and NCR builds its own terrain (`map/ncr.rs:265-298`). Comments that still mention the stepwell are out of date (`sim/movement.rs:24`, `sim/bots.rs:144`, `game/water.rs:1`).

## 1. Vehicles

### What exists

- **No driving code.** A search for vehicle, seat, mount, steer and tractor finds no gameplay code. The only mention is old lore text, "vehicle lane" (`assets/data/items.ron:56`).
- **Parked autos and scooters are scenery.** `place()` adds an invisible static box and a kit entry (`map/kit.rs:52-62`). Sizes: auto 1.4 x 1.8 x 2.6 m, scooter 0.45 x 1.1 x 1.65 m (`map/kit.rs:28-29`). Pieces are chosen at `map/kit.rs:114`. The client merges them into 32 m chunk meshes (`game/street.rs:64-82`), so a parked auto is not an object anyone can take. A drivable one must be a separate object, placed apart from the kit.
- **NCR cars and buses are also scenery**, built from map blocks (`map/ncr_art.rs:192`, `map/ncr_art.rs:291`, placed at `map/ncr_art.rs:366` and `map/ncr_art.rs:377`). `docs/ncr/ART-DIRECTION.md:14` says they are not drivable.
- **The nearest thing to a seat is the drop ride.** `MoveMode::Vimana` (`sim/mod.rs:139-146`) is skipped by movement (`sim/movement.rs:86`). `fly_vimana` places every rider (`sim/flow.rs:302-311`). The body is hidden (`game/visuals.rs:776`), the camera frames it (`game/camera.rs:145-146`), the prompt speaks (`game/hud.rs:277-285`) and a loop plays (`game/audio.rs:407`). It is one-way, scripted and shared by everyone. The Udta Auto is only a look on top (`game/visuals.rs:1111-1125`). It is still the template for a seat.
- **Tractor:** no model and no code. **Delhi cars:** none. The street kit has auto, scooter and thela only.

### How movement would work

- Warriors use `Kinematic` (`sim/mod.rs:148-158`) and collide-and-slide (`sim/movement.rs:73-179`). The physics solver is off on purpose: zero gravity, one substep, "no dynamic bodies to solve" (`sim/mod.rs:415-417`). A vehicle should follow the same arcade approach. `move_and_slide` already takes any collider and rotation (`sim/movement.rs:130-139`). Real wheel physics would mean turning the solver on, which is a much bigger job.
- The Delhi maps are flat: one pad over everything (`map/delhi.rs:120`), with only water sunk to 1.4 m (`map/delhi.rs:171-178`). So a first vehicle needs no slope handling. NCR has ramps and stairs.
- `Terrain::ground_at` already says road, park, pavement or water under any point (`map/terrain.rs:21-40`), so grip can differ on road and grass.
- `btn::INTERACT` is read only by loot (`sim/loot.rs:232-286`). Entry can go there, with a prompt beside `game/hud.rs:288`.
- Shooting is blocked outside `MoveMode::Ground` (`sim/combat.rs:129-132`). Passengers cannot fire until this changes.
- A new mode must be handled in three `match` blocks the compiler will list (`sim/movement.rs:85`, `sim/bots.rs:367`, `game/camera.rs:143`) and one it will not, because of a catch-all (`game/animation.rs:293`). Head bob needs turning off too (`game/camera.rs:171-179`).
- Vehicle spawn belongs in `start_match` (`sim/flow.rs:220-224`), cleanup in `restart` (`sim/flow.rs:265-271`), and placement in a new stream (`sim/mod.rs:385-393`) so every peer agrees.

### How online sync would work

The owner model fits vehicles well. There is no host: each warrior has one owner peer and the others follow it (`sim/net.rs:1-5`). The driver's peer would own the vehicle while driving.

| Need | What the code has | Note |
|---|---|---|
| Vehicle position | `WarriorState` goes out every tick (`sim/net.rs:390-420`) as unreliable datagrams (`net/lib.rs:925-963`) | The datagram reader only decodes `WarriorState` (`net/lib.rs:203`). Cheapest plan: add a ride field to it (`sim/net.rs:119-135`) and let the vehicle follow its driver |
| Enter, leave, park | Every `NetMsg` goes on the ordered stream (`net/lib.rs:934`) | New messages need no transport work |
| Two players take one seat | Pickups already settle this: earlier take wins (`sim/loot.rs:52-63`, `sim/net.rs:368-381`) | Copy the rule |
| Vehicle damage | Door damage pattern (`sim/combat.rs:567-574`, `sim/net.rs:294-301`) | Copy the pattern |
| Run-over damage | Warriors never collide with each other (`sim/flow.rs:207-209`), so the driver's peer does a sweep like melee (`sim/combat.rs:149-156`) and sends `NetMsg::Hit` | `max_hit` gives 0 for anything except Melee and Bow items (`sim/net.rs:321-330`). Cheapest fix: a hidden Melee item with `drops: []`, which also gets a kill feed line for free |
| Driver quits or dies | `depart` (`sim/net.rs:450-483`) and the elimination code (`sim/combat.rs:613-636`) know nothing of seats | Must free the seat and stop the vehicle |
| Smooth motion on other screens | `follow_remote` with SNAP 4 m and COAST 0.25 s (`sim/net.rs:167-172`, `sim/net.rs:569-580`) | Freefall at up to 42 m/s already goes through this, so it should cope. `mesh_check` counts jumps over 1 m (`crates/net/src/bin/mesh_check.rs:164`) |
| Horn | `held` bits travel in `WarriorState` (`sim/net.rs:129`), and `btn` has 9 free bits (`sim/mod.rs:55-63`) | No new message needed |
| Engine sound | `attach_loop` puts a positional loop on an entity (`game/audio.rs:140-151`) | Sounds are made in `tools/audio/sfx.py` and listed in a fixed array (`game/audio.rs:49`) |

Trap: the bot grid is baked once from `solid_mask()` (`sim/nav.rs:505-572`, mask at `sim/nav.rs:537`). A vehicle on `Layer::World` would stay in the grid as a wall after it drives away. Vehicles need their own layer (`sim/mod.rs:34-46`), with one mask for walking and sight and another for the grid. The map fingerprint test will also fail if parked pieces change, so update it on purpose (`map/mod.rs:287-294`).

Bots would not drive. They walk by grid and street router (`sim/bots.rs:225-306`). Teaching them to drive is large work and can be skipped.

### What the maps allow

Sprint is 7.4 m/s (`sim/movement.rs:11`). A 500 m Delhi map takes about 68 s to cross on foot, or about 36 s at 14 m/s. NCR is 2.2 km (`map/ncr.rs:18-19`): about 5 minutes on foot, 2.6 minutes at 14 m/s. So vehicles are fun on today's maps and a real need on a big one. NCR is out of the rotation (`map/mod.rs:35`, `sim/flow.rs:90-92`), but `docs/PLAN.md` names it as the final map.

Road metres inside the play area, from the map JSON. Widths are defaults by road type (`tools/osm/delhi.py:43-47`), not surveyed:

| Map | Road metres | 5 m or wider | Note |
|---|---|---|---|
| Connaught Place | 9,517 | 3,036 (32%) | 6,386 m is 2.5 m footpath. The painted Inner Circle ring is extra (`map/delhi.rs:288-300`) |
| Chandni Chowk | 6,231 | 5,179 (83%) | Mostly 7 m streets, the best driving map |
| Hauz Khas | 3,534 | 1,341 (38%) | 2,193 m is 2.5 m path |
| India Gate | 5,025 | 956 (19%) | 917 m of 14 m road, plus flat open lawns, good for a tractor |

Parked pieces keep to the edge, with at least 1.2 m clear each side of the middle (`map/kit.rs:144-146`).

### Which vehicles fit

| Vehicle | Today | Fit | Note |
|---|---|---|---|
| Auto-rickshaw | Static `auto` piece, 1.4 m wide (`map/kit.rs:28`), and the flying `jugaad.glb` | 5 m roads and wider, lanes with care | First choice: most Delhi, model half done |
| Scooter or bike | Static `scooter` piece, 0.45 m wide (`map/kit.rs:29`) | Also fits the 2.5 m paths, the only one that does | Rider stays exposed. Fastest way through the old city |
| Car | Delhi maps have none. NCR `car()` is a 1.85 x 4.3 m block prop (`map/ncr_art.rs:192-199`) | 5 m roads and wider | New model needed. Solid cover for riders |
| Tractor | Nothing | 7 m roads, parks and lawns | New model. Slow and tough, natural on India Gate lawns and Deer Park, poor in lanes |
| Thela | Static `thela` piece, 1.25 m wide (`map/kit.rs:30`) | Lanes | Comic extra, pushed on foot |
| Bus | NCR static prop only (`map/ncr_art.rs:291`) | Wide roads only | Skip for now |

### Art

`tools/blender/props/street_kit.py` builds `auto()` and `scooter()` by script (lines 156 to 186). Wheels are merged into the body, so they cannot spin yet. `JugaadPart` shows how to animate named parts (`game/visuals.rs:1133-1175`). `mesh_of` can fetch a kit mesh for a first test (`game/street.rs:60-63`). A tractor or car is a new script function of about 20 to 40 lines, with separate wheels.

### Size

| Work | Size |
|---|---|
| One drivable auto, offline, no bots | Small to medium: 1 to 2 weeks |
| The same online | Add about 1 week |
| Each further vehicle (scooter, car, tractor) | Small: 1 to 3 days |
| Passengers who shoot | Small: 3 to 4 days |
| Vehicle health, smoke, wreck | Small to medium: up to 1 week |
| Bots that drive | Large: skip |

## 2. Guns

### Current guns (`assets/data/items.ron:12-53`)

| Item | Damage | Speed m/s | Gravity | Magazine | Headshot | Note |
|---|---|---|---|---|---|---|
| Katta | 60 | 140 | 3 | 1 | x2 | 15% misfire, 3% backfire |
| Tamancha | 24 | 95 | 4 | 20 | x1.75 | |
| Dunali | 13 x 7 | 70 | 9 | 5 | x1.5 | Shotgun |
| Gulel | 95 | 170 | 2 | 1 | x2 | 0.9 s draw, no spread. The nearest thing to a sniper today |
| Vajra-9 | 18 | 120 | 2 | 30 | x1.55 | Shows a bow and flies as an arrow |
| Trishul Carbine | 19 x 3 | 105 | 3 | 9 | x1.45 | Three pellets at once, not a real burst |

Health is 100 and shield up to 100 (`sim/mod.rs:126-130`). Rarity adds up to 35% damage (`shared/items.rs:21-29`).

### Is a sniper possible by data alone?

**The rules yes, the look no.** One new `Bow` entry gets damage, fire rate, speed, drop, magazine, reload, headshot multiplier, rarity, chest and floor loot (`sim/loot.rs:135-137`), online sync (`sim/net.rs:107-116`, `sim/net.rs:321-330`), camera kick (`game/fx.rs:465`), reticle size (`game/hud.rs:365-369`) and the ammo line (`game/hud.rs:238-241`). A test entry, not applied:

```ron
"sniper_test": (
    name: "Sniper Test",
    lore: "",
    kind: Bow(damage: 75.0, interval: 1.4, speed: 320.0, gravity: 4.0, pellets: 1, spread_deg: 0.0,
              magazine: 5, reload: 3.2, charge: 0.0, headshot: 2.5),
    drops: [(Epic, 1.5), (Legendary, 0.8)],
),
```

Data alone leaves these gaps:

| Gap | Where | What happens without a fix |
|---|---|---|
| Model | `model_for` (`game/visuals.rs:801-809`), `WEAPON_MODELS` (`game/visuals.rs:86-91`) | Invisible in the hand and on the ground: `spawn_weapon_model` returns early (`game/visuals.rs:826`). Add a function to `tools/blender/weapons/desi.py`, or point it at the dunali for a test |
| Held pose | `long_gun` lists only the dunali (`game/visuals.rs:930-932`) | Held like a pistol |
| Shot look and impact | `shot_kind` lists three guns (`game/visuals.rs:934-940`) | Flies as an arrow. This already happens to the Vajra-9 and Trishul Carbine |
| Fire sound | `game/audio.rs:291-298` | Bow sound, heard to 120 m |
| Scope | See below | Same zoom as every weapon |
| Bot choice | `choose_weapon` (`sim/bots.rs:708-734`) | Scored like any gun, so bots may use it at 5 m |

Any gun fires on hold (`sim/combat.rs:209-210`), so a bolt-action feel is just a long `interval`. All guns share one ammo pool (`sim/mod.rs:175`). A separate sniper ammo is small work.

### Scope, zoom and aiming

- Right mouse does the same for every weapon: camera to 1.9 m and FOV 48 (`game/camera.rs:148`), spread x0.6 (`sim/combat.rs:237-238`), speed 3.6 m/s (`sim/movement.rs:104-105`), sensitivity x0.55 (`game/input.rs:62`, `game/input.rs:180-181`).
- The only weapon-based zoom is the charged draw, -0.16 radians (`game/fx.rs:476-481`).
- There is no scope overlay, no zoom per weapon and no first-person view. The reticle assumes 70 degrees (`game/hud.rs:367-368`). The wheel is used for the hotbar (`game/input.rs:222-236`), so stepped zoom needs a different key.
- Fix: add a `scope` field with `#[serde(default)]` to `ItemKind::Bow` (`shared/items.rs:49-64`), read it in the camera, input and HUD. No network change.
- Small trap: `aim_ray` uses fixed camera numbers (`sim/mod.rs:573-578`), but the aiming camera sits at 0.85 of that shoulder offset (`game/camera.rs:148`). Shots land about 10 cm right of the crosshair at every range. That does not show today, but under strong zoom it is close to half a head.

### Headshots and bullet drop

- **Headshots exist.** A hit above 0.5 m over the capsule centre counts (`sim/mod.rs:32`, `sim/combat.rs:362-365`), with a multiplier per item. Gold marker, sound and number are in (`game/hud.rs:400`, `game/audio.rs:324`, `game/hud.rs:573-580`). It is only a height test, so crouch needs it changed.
- **Bullet drop exists.** Each weapon has `gravity` (`sim/combat.rs:351`), sent online (`sim/net.rs:111`). At 320 m/s and gravity 4, drop at 200 m is about 0.8 m. Fast shots cannot pass through walls because each tick is a ray (`sim/combat.rs:352-355`). Shots last 4 s (`sim/combat.rs:251`), and the crosshair ray reaches 600 m (`sim/combat.rs:232-235`).
- **Online:** the shooter's peer decides the hit, capped by `max_hit` (`sim/net.rs:270-279`). There is no server check. A one-shot sniper raises the stakes of cheating, though the hole is already there for every weapon.

### Long-range problems to expect

- **Bots do not answer.** Sight is 35 or 55 m (`sim/bots.rs:103`). A grudge reaches only 70 m (`sim/bots.rs:445`). A roof sniper past 70 m gets free kills. Bots do not use cover in a fight. A low-health flee is being added now (`sim/bots.rs:557-572`), but it only runs away from a target already seen.
- **Street kit vanishes but still blocks.** It stops drawing at about 160 m on High and Ultra, 120 m on Medium and 90 m on Low (`game/street.rs:27-28`, `game/street.rs:83-84`, `game/quality.rs:100-107`). Its colliders stay. At range, a shot can stop on a crate the player cannot see, and each quality level sees different cover.
- **Warriors stay visible** to about 400 m on every quality (`game/characters.rs:103-104`, `game/characters.rs:199-207`). This is good.
- **Levels of detail go by distance, not zoom.** A strong scope will show coarse models. Check in the real game.
- **Where sniping works.** Connaught Place's ring faces across the park, about 270 m (`map/delhi.rs:98-99`). India Gate is nearly all open. Chandni Chowk is lanes under buildings with a median height of 10.7 m.
- **Getting up high.** Mapped buildings are solid (`map/mod.rs:200-208`) with no stairs, so you reach their roofs only by landing there. Enterable houses have stairs to the roof (`map/india.rs:125-136`) and a 1 m parapet (`map/india.rs:155`). The ring has stairs only to the first floor (`map/cp.rs:156-159`), with windows up there. Its roof needs a glide landing.

### Size

Sniper with model, sound, scope and bot rules: **small, 2 to 4 days**. Bots that react to long-range fire: **small**, best done with the hearing work below.

## 3. Hiding

### Crouch and prone

- `btn::CROUCH` is defined (`sim/mod.rs:62`) and nothing uses it. There is no action (`game/input.rs:12-24`), no key (`game/input.rs:46-58`) and no entry in the intent map (`game/input.rs:191-198`).
- The capsule is fixed at 1.76 m tall (`sim/mod.rs:24-26`, `sim/flow.rs:206`), and `feet_probe` assumes that size (`sim/movement.rs:60-70`).
- There are no crouch clips (`game/animation.rs:36-38`). A missing clip stops the whole animation graph from building (`game/animation.rs:122-124`), so code and re-exported clips must land together. `locomotion()` already has a `crouch` setting (`tools/blender/characters/anims.py:219`), so new cycles are a few calls and `just anims`.
- `held` travels online (`sim/net.rs:129`), so other peers see crouch with no new message.
- Parapets are 1 m and the body 1.76 m, so today nobody can duck behind one.
- Prone: nothing exists. It needs a lying collider, new clips and camera work.

### Footsteps and sound

- Footsteps are client-only. The animation code writes `Footstep` (`game/animation.rs:67-72`, `game/animation.rs:319-321`), and audio plays one of five surfaces with volume by speed, up to 35 m (`game/audio.rs:185-200`).
- Any speed over 0.8 m/s makes a step (`game/animation.rs:319`), so slow aim-walking is still loud at about 80% volume. Walls do not muffle sound. There is no sound marker on the HUD.
- Gunshots carry 200 to 220 m (`game/audio.rs:292-294`).
- The minimap shows only yourself and the ring (`game/hud.rs:430-459`), so the HUD never gives a hider away.

### What bots notice

**Sight and being hit, nothing else.**

- Perception is a distance check plus one ray, every 0.25 s (`sim/bots.rs:434-453`).
- There is **no view cone**: bots see all round.
- There is **no hearing**. `Footstep` lives in the game crate, so the shared simulation never sees it. Gunfire alerts nobody.
- There is **no memory** of where a target was. A hit gives a 6 s grudge (`sim/bots.rs:345-353`).
- Sight stops only at solid colliders (`sim/bots.rs:337`, `sim/mod.rs:44-46`), so anything without a collider hides nobody.
- Hearing can use data the simulation already has: warrior velocity and new `Projectile` entities, on every peer.

### Bushes and concealment

- `BUSH_LANTANA` and `BUSH_BOUGAINVILLEA` are defined (`map/india.rs:50-51`), the models are on disk and the renderer is ready (`game/batch.rs:27`, `game/batch.rs:100-101`). **No map places them.**
- Trees have a trunk collider only, 0.25 m radius (`map/delhi.rs:397`). NCR trees have none (`map/ncr.rs:153-159`).
- Grass is client-only, at most about 0.46 m tall (`game/grass.rs:409`, `game/grass.rs:416`), and its density changes with quality (`game/quality.rs:115-122`). It cannot be fair cover.
- A hide rule must live in the simulation: inside a bush, a bot sees you only from close range. Bushes fade at 220 m, or about 120 m on Low (`game/batch.rs:100-101`), so hide-bushes need a fixed long range or Low players will see through them.

### Doors

- Doors have 60 HP and can only be broken (`sim/mod.rs:252-257`, `sim/mod.rs:507-517`). Two bat hits do it. Sync is `DoorHit` (`sim/net.rs:294-301`). There is no open, close or interact.
- Only `india::house` places them (`map/india.rs:122-124`), so only Chandni Chowk and Hauz Khas have any (`map/delhi.rs:314`), up to 50 houses a map (`map/delhi.rs:363`).
- **Useful side effect:** the bot grid treats a door as a wall (`sim/nav.rs:537`, `sim/nav.rs:555-557`) and is never sampled again (`sim/nav.rs:509`). `bots.rs` has no door code, and bots drop goals they cannot reach (`sim/bots.rs:261-265`). So by reading the code, bots never walk into a house. Those rooms already work as hiding places from bots. Bots can still see and shoot through windows and a broken doorway. Real players can walk in.
- The same fact means bots never loot those houses. Plan step 7 may change this.

### Hiding places that work today

- Rooms and roofs of enterable houses, as above.
- Roofs of solid buildings, reached by gliding. Rooftop stair rooms give cover (`map/kit.rs:34`, `map/kit.rs:179-188`). A 6.5 m drop is free, and 11 m costs about 30 health (`sim/movement.rs:21-25`).
- Connaught Place's shops and upper floor, which are open to bots too.
- The ring ends at radius 0 (`sim/zone.rs:14-21`), so hiding cannot win a match by itself.

Night is not a hiding option yet: the clock is local to each client, and it runs only on Ultra or with a switch (`game/visuals.rs:23-25`, `game/visuals.rs:165-172`).

### Underground

- **Works only on NCR.** The Blue Line underpass has a trench in the height map (`map/ncr.rs:290-293`), floor, walls and roof (`map/ncr.rs:654-684`), stairs (`map/ncr.rs:685-696`), lamps (`map/ncr.rs:705-712`) and rare chests (`map/ncr.rs:703`). NCR is out of the rotation, and bots do not go there (`sim/bots.rs:237`, `map/navigation.rs:1`).
- **Palika Bazaar and Rajiv Chowk are only names.** The HUD shows them over open ground (`game/hud.rs:422`). The map converter drops underground buildings and stations (`tools/osm/delhi.py:129-131`, `tools/osm/delhi.py:268-269`). Connaught Place's real underground market is an open slot for a set piece.
- **The builder has the parts, but no basement helper.** It has `stairs` (`map/build.rs:218-237`), `slab_with_hole` (`map/build.rs:250-258`), `wall` (`map/build.rs:96-124`) and `Lamp` (`map/mod.rs:189-198`). Editing heights after generation is proven by `water()` (`map/delhi.rs:171-178`). Ground samples are 1 m apart on Delhi maps (`map/delhi.rs:33-34`). A height map cannot have holes, so every room is a pit with a slab over it (`map/ncr.rs:290`).
- **Mapped buildings are solid**, with no inside (`map/footprint.rs:57-88`). Only houses and the ring have interiors.
- **Bots cannot use it yet.** The grid makes levels only from ramps that climb above the street (`sim/nav.rs:110-127`) and joins a stair only if its top level is not the street (`sim/nav.rs:413`). A stair going down from the street has its top at street level, so it is never joined.
- The ring ignores height (`sim/zone.rs:73-75`), so a basement is no shelter from the Aandhi.
- `walk_check` went with the Bharat map, though `docs/testing.md:7` still names `just walk`. The justfile keeps only `ncr-walk` and `street-sample-walk` (`justfile:213`, `justfile:235`). New enterable places on Delhi maps need a new walk check.

### Size

| Work | Size |
|---|---|
| Crouch with clips | Small: 3 to 5 days |
| Prone | Medium: 1 to 2 weeks |
| Quiet steps by speed and crouch | Small: 1 day |
| Bot view cone, hearing, last-seen memory | Small: 2 to 4 days, after step 7 |
| Bush cover and hide rule | Small: 3 to 5 days |
| Doors that open and close, with bot paths | Small to medium: about 1 week |
| Basements in houses, with bot paths | Medium: 1 to 2 weeks |
| Bring a baoli back from git | Small to medium: about 1 week |
| Palika Bazaar as an underground market | Medium: 2 to 4 weeks |

## 4. Slang and voice

### Where the words live

| Text | Where | Form |
|---|---|---|
| Kill feed | `feed_line` (`sim/combat.rs:640-660`) | Rust. One verb, "tapka diya", plus Aandhi and fall lines |
| Win line, player left | `sim/flow.rs:328`, `sim/net.rs:478` | Rust. The second is English |
| HUD labels | `game/hud.rs:202`, `game/hud.rs:240`, `game/hud.rs:413-423` | Rust. Mixes Jaan, Kavach, Goli, Aandhi with Alive, Kills, Safe zone |
| Prompts | `game/hud.rs:269`, `game/hud.rs:281-291`, `game/hud.rs:298`, `game/hud.rs:303` | Rust |
| End screens and map vote | `game/hud.rs:474-479`, `game/hud.rs:488` | Rust |
| Item names and lore | `assets/data/items.ron`, loaded by `shared/items.rs:88-94` | Data, 16 items |
| Bot names | `sim/flow.rs:120-123` | Rust, 23 names |
| Room code words | `net/lib.rs:241-253` | Rust, 128 words |
| Web menu | `web/index.html:34-57` | HTML, mostly Hinglish |
| Lobby | `game/lobby.rs:59-71` | Rust, English |

### Findings

- **`lore` is loaded but never shown.** No code reads it. All 16 lore lines are invisible to players. The pickup prompt at `game/hud.rs:298` is the easy place to show them.
- **No language files.** All text is inline, so there is no way to swap or review it in one place.
- **No voice or bark system.** The only "bark" in the code is tree bark (`map/mod.rs:109`). All 67 sounds are synthesized (`tools/audio/sfx.py:1-2`, `game/audio.rs:49`). None is a voice.
- **No chat, ping or emote**, and no teams: matches are all against all. Barks can only be taunts or reactions, not squad calls, unless teams are added.
- **Triggers exist.** `HitMsg`, `KillMsg` and `MisfireMsg` already drive sounds (`game/audio.rs:264-338`), and kills also fire on remote peers (`sim/net.rs:290-292`). `Player::at` plays a positional sound (`game/audio.rs:113-131`). The system is the small part. Recording good Delhi voices is the real cost.
- **Random lines are safe online.** Each peer builds its own feed text (`sim/combat.rs:640-643`).
- **Script:** the repo has no font file, and text uses the default font (`game/hud.rs:81-83`). As far as I know that font is Latin only. Roman Hinglish is fine. Devanagari needs a bundled font.
- **Out-of-date text:** "Garuda pankh kholo" (`game/hud.rs:287`) although the glider is now a patang (`game/visuals.rs:740`). English lore on Vajra-9, Trishul Carbine and Brahmastra Mk II (`assets/data/items.ron:42`, `assets/data/items.ron:49`, `assets/data/items.ron:56`).

### Size

| Work | Size |
|---|---|
| Show lore, add kill lines by weapon and cause, fix old text | Small: 1 to 3 days |
| Move all text to one data file | Small: 2 to 3 days |
| Text taunt or quick-chat wheel online | Small: 2 to 3 days |
| Voice bark system | Small: 2 to 3 days |
| Voice lines recorded and mixed | Medium: weeks, depends on casting |
| Devanagari text | Small to medium |

## 5. Street fight

- The bat is the only melee weapon. It is always carried and cannot be dropped (`assets/data/items.ron:4-11`, `sim/mod.rs:171`).
- `ItemKind::Melee` takes damage, interval, reach and hit delay (`shared/items.rs:44-45`), so new melee items are data. But `is_weapon` leaves melee out of loot (`sim/loot.rs:135-137`), and bots use only slot 0 for melee (`sim/bots.rs:708-734`).
- A swing is a swept sphere with no headshot (`sim/combat.rs:146-164`). There is no block, dodge, grab, kick or bare-hand attack. Clips are `melee_guard`, `melee_swing` and `melee_swing_b` (`game/animation.rs:38`).
- A melee hit pauses that peer's whole simulation for 0.05 s (`sim/combat.rs:607-611`). Fine for rare hits. Worth a re-check if melee becomes common.
- Throwables use `Astra` effects: fire, wind, chakra, blast (`shared/items.rs:74-85`). A thrown brick or chappal could be a tiny `Blast` in data, but blasts play the Brahmastra sound out to 500 m (`game/audio.rs:357-360`), so it needs a small code change.

Size: each new melee or thrown item is **small**, 1 to 2 days with a model. Block or dodge is **medium**.

## 6. Delhi map content

From the fixed test at `map/mod.rs:289-293`, seed 7:

| Map | Blocks | Buildings | Chests | Floor loot | Props | Kit pieces | Places named |
|---|---|---|---|---|---|---|---|
| Connaught Place | 6,808 | 60 | 154 | 154 | 379 | 725 | 11 |
| Chandni Chowk | 14,376 | 879 | 124 | 185 | 69 | 2,371 | 12 |
| Hauz Khas | 6,431 | 134 | 95 | 127 | 350 | 479 | 6 |
| India Gate | 630 | 6 | 70 | 90 | 585 | 774 | 5 |
| NCR (not in rotation) | 18,591 | 0 | 191 | 72 | 156 | 0 | 15 |

- All four Delhi maps are 500 m square, from OpenStreetMap (`map/delhi.rs:1-10`, `map/delhi.rs:32`). The rotation holds those four (`map/mod.rs:35`).
- **Built landmarks: only two.** The Connaught Place ring (`map/cp.rs:52-179`) and India Gate (`map/delhi.rs:187-214`). Everything else is an extruded outline.
- **Place names are HUD labels only** (`game/hud.rs:422`). Paranthe Wali Gali, Dariba Kalan, Kinari Bazar, Moti Bazar, Palika Bazaar, Rajiv Chowk, Deer Park and the Hauz Khas madrasa have no matching built set piece.
- **Enterable houses:** Chandni Chowk and Hauz Khas only (`map/delhi.rs:314`, `map/delhi.rs:325-338`).
- **Street kit, 14 pieces:** auto, scooter, thela, chai, drum, crates, chairs, pot, pole on streets, plus cooler, dish, solar, clothesline and mumty on roofs (`map/kit.rs:114`, `map/kit.rs:166`, `tools/blender/props/street_kit.py:356`). A new piece is a script function, a weight, an optional collider and `just street-kit`. Hours to a day each.
- **Street feel already in:** 32 shop signs (commit `d38e26b`), kites and pigeons (`game/ambient.rs:1-3`), patang glider, Udta Auto, Aandhi ring, chai and lassi heals, Anaar and Sutli Bomb.
- **Missing:** no people, traffic, animals or moving street life. No horns, vendors or crowd sound: the only ambient beds are day and river (`game/audio.rs:64-72`). No music.

Size: a kit piece is small. A built landmark is about 1 to 2 weeks. A new OSM map is small to generate, and medium to bring up to Connaught Place's standard.

## 7. Fit with the plan

- `docs/PLAN.md:3-6` says it is the only plan. None of these wishes is in it yet, so they must be added there.
- Owner rule: one step at a time, each proven in the real game (`docs/PLAN.md` working rule, and owner memory). Connaught Place comes first (`docs/PLAN.md:11-13`).
- Phase 1 steps 6 to 9 are open. Step 7 is bot navigation, which touches the same `bots.rs` and `nav.rs` that bot hearing, doors and basements need. Those wishes should wait for it or be built on top of it.

Suggested order, cheapest and safest first:

1. Show lore, more kill lines, fix old text.
2. Sniper and scope, with a bot reply past 70 m.
3. Crouch, quiet steps, bot hearing and view cone (after step 7).
4. Bushes and working doors.
5. One drivable auto offline, then online, then scooter, car and tractor.
6. An underground set piece, Palika Bazaar being the natural one.
7. Voice lines.

## 8. Not proven yet

Nothing here was run. A `sim_check` run was active on this Mac during the check, and the owner has said not to run work side by side.

1. Bots never enter closed houses. Check with `VIVAD_MAP=chandni cargo run --release -p shared --bin nav_check` and a `sim` run.
2. The 10 cm aim offset under zoom.
3. How coarse far models look through a scope.
4. Whether a kinematic vehicle slides cleanly along building meshes. A note at `map/ncr.rs:588-589` shows the capsule sweep has already caught on long thin colliders.
