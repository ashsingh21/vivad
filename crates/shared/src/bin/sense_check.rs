//! Headless proof of crouching, cover and the bots' senses on a real map, with the whole simulation
//! running: nothing here reaches past what a warrior's intent can ask for, except placing warriors.
//!
//! cargo run --release -p shared --bin sense_check
//! VIVAD_MAP=ncr cargo run --release -p shared --bin sense_check     the same on the NCR
//!
//! Prints each check and exits non-zero if one failed. The cover table goes round every piece of the
//! kit on the map from 16 bearings at 15 m, keeping only bearings from which a warrior stepped out
//! beside the piece is in plain sight (other cover in the way proves nothing about this one).

use std::f32::consts::TAU;
use std::time::Duration;

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::items::{ItemDb, ItemKind, ItemStack, Rarity};
use shared::map::{MapId, MapLayout};
use shared::sim::birds::Flock;
use shared::sim::bots::BotBrain;
use shared::sim::combat::Projectile;
use shared::sim::nav::{NavGrid, StreetFloor};
use shared::sim::senses::{self, NoiseMsg, in_sight};
use shared::sim::stance::{CROUCH_DROP, CROUCH_SPEED, Stance};
use shared::sim::zone::Zone;
use shared::sim::{ADS_DISTANCE, CAMERA_DISTANCE, CAMERA_SHOULDER, CROUCH_EYE_DROP, EYE_OFFSET, HALF_HEIGHT, Health, Inventory, Kinematic, Layer, MatchConfig, MoveMode, PlayerIntent, SimPlugin, Warrior, btn, hit_mask, solid_mask};

const COVER: [&str; 11] = ["chai", "thela", "crates", "cooler", "drum", "scooter", "auto", "mumty", "tanki", "gumti", "clothesline"];

struct Check {
    failed: u32,
}

impl Check {
    fn that(&mut self, ok: bool, what: &str) {
        println!("  {} {what}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            self.failed += 1;
        }
    }
}

struct Stage {
    app: App,
    player: Entity,
    bot: Entity,
}

impl Stage {
    fn tick(&mut self, n: u32) {
        for _ in 0..n {
            self.app.update();
        }
    }

    fn ground(&self, p: Vec2) -> f32 {
        self.app.world().resource::<MapLayout>().terrain.height(p.x, p.y)
    }

    fn put(&mut self, who: Entity, feet: Vec3) {
        let world = self.app.world_mut();
        world.get_mut::<Transform>(who).unwrap().translation = feet + Vec3::Y * (HALF_HEIGHT + 0.03);
        let mut kin = world.get_mut::<Kinematic>(who).unwrap();
        kin.mode = MoveMode::Ground;
        kin.velocity = Vec3::ZERO;
        kin.fall_from = None;
    }

    fn pos(&self, who: Entity) -> Vec3 {
        self.app.world().get::<Transform>(who).unwrap().translation
    }

    fn intent(&mut self, who: Entity) -> Mut<'_, PlayerIntent> {
        self.app.world_mut().get_mut::<PlayerIntent>(who).unwrap()
    }

    fn brain(&self) -> &BotBrain {
        self.app.world().get::<BotBrain>(self.bot).unwrap()
    }

    fn crouched(&self) -> bool {
        self.app.world().get::<Stance>(self.player).unwrap().crouched
    }

    /// The top of the player's collider, above his feet, by a ray straight down onto him.
    fn height(&mut self) -> f32 {
        let at = self.pos(self.player);
        self.app
            .world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                let filter = SpatialQueryFilter::from_mask(hit_mask());
                let from = at + Vec3::Y * 1.2;
                let hit = spatial.cast_ray(from, Dir3::NEG_Y, 4.0, true, &filter);
                hit.map_or(0.0, |h| from.y - h.distance - (at.y - HALF_HEIGHT))
            })
            .unwrap()
    }

    fn sees_player(&self) -> bool {
        self.brain().senses.sees(self.player)
    }

    /// Hold the bot where it is, looking along `yaw`, for `n` ticks, with `each` run on the player.
    fn watch(&mut self, n: u32, spot: Vec3, yaw: f32, mut each: impl FnMut(&mut Self)) {
        for _ in 0..n {
            self.put(self.bot, spot);
            self.intent(self.bot).yaw = yaw;
            each(self);
            self.tick(1);
        }
    }
}

/// Somewhere a warrior can stand and get away from, by whichever grid this map has.
fn open(nav: &NavGrid, floor: &StreetFloor, p: Vec2) -> bool {
    if floor.ready() { floor.reachable(p) } else { nav.walkable(p) }
}

/// Well inside the coming circle: a bot the ring is calling drops everything else, as it should, and
/// this is about what it does when it isn't.
fn ring_calm(zone: &Zone, p: Vec2) -> bool {
    zone.target_center.distance(p) < zone.target_radius * 0.6
}

fn yaw_to(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

fn setup() -> Stage {
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    let mut app = App::new();
    let map = if std::env::var("VIVAD_MAP").is_ok() { MapId::from_env() } else { MapId::Cp };
    println!("on {map:?}");
    app.insert_resource(MatchConfig { bots: 1, autopilot: false, match_seed: Some(7), rotate: false, map, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    // The NCR's floor is ready from the start, with the world not yet up.
    for _ in 0..3 {
        app.update();
    }
    while !app.world().resource::<NavGrid>().ready() && !app.world().resource::<StreetFloor>().ready() {
        app.update();
    }
    let world = app.world_mut();
    let (mut player, mut bot) = (None, None);
    for (e, w) in world.query::<(Entity, &Warrior)>().iter(world) {
        if w.is_bot {
            bot = Some(e);
        } else {
            player = Some(e);
        }
    }
    Stage { app, player: player.expect("a player"), bot: bot.expect("a bot") }
}

/// An open place on the street, with `room` metres free every way.
fn open_spot(stage: &mut Stage, room: f32) -> Vec2 {
    stage
        .app
        .world_mut()
        .run_system_once(move |layout: Res<MapLayout>, nav: Res<NavGrid>, floor: Res<StreetFloor>, zone: Res<Zone>| {
            layout
                .kit
                .iter()
                .filter(|k| k.pos.y < layout.terrain.height(k.pos.x, k.pos.z) + 1.0 && ring_calm(&zone, k.pos.xz()))
                .flat_map(|k| (0..8).map(move |i| k.pos.xz() + Vec2::from_angle(i as f32 * TAU / 8.0) * 8.0))
                .find(|p| (0..24).all(|i| (1..=4).all(|r| open(&nav, &floor, *p + Vec2::from_angle(i as f32 * TAU / 24.0) * room * r as f32 / 4.0))))
                .expect("somewhere open")
        })
        .unwrap()
}

fn crouching(stage: &mut Stage, check: &mut Check) {
    println!("crouch");
    let spot = open_spot(stage, 6.0);
    let feet = spot.extend(stage.ground(spot)).xzy();
    let player = stage.player;
    stage.put(player, feet);
    stage.tick(5);
    let tall = stage.height();
    check.that((tall - 1.76).abs() < 0.08, &format!("standing he is {tall:.2} m tall"));
    stage.intent(player).held = btn::CROUCH;
    stage.tick(3);
    let low = stage.height();
    check.that(stage.crouched() && (low - 1.10).abs() < 0.08, &format!("crouched he is {low:.2} m tall"));
    let from = stage.pos(player);
    stage.intent(player).move_axis = Vec2::Y;
    stage.tick(60);
    let went = stage.pos(player).xz().distance(from.xz());
    check.that((went - CROUCH_SPEED).abs() < 0.4, &format!("crouched he goes {went:.1} m in a second"));
    stage.intent(player).move_axis = Vec2::ZERO;
    // A slab over his head: he stays down until it is gone.
    let at = stage.pos(player);
    let ground = at.y - HALF_HEIGHT;
    let slab = stage
        .app
        .world_mut()
        .spawn((Transform::from_translation(Vec3::new(at.x, ground + 1.4, at.z)), RigidBody::Static, Collider::cuboid(3.0, 0.2, 3.0), CollisionLayers::new(Layer::World, LayerMask::ALL)))
        .id();
    stage.tick(4);
    for _ in 0..30 {
        stage.intent(player).held = 0;
        stage.tick(1);
    }
    check.that(stage.crouched(), "under a 1.3 m ceiling he can't stand");
    stage.app.world_mut().entity_mut(slab).despawn();
    stage.tick(4);
    stage.intent(player).held = 0;
    stage.tick(4);
    check.that(!stage.crouched() && (stage.height() - 1.76).abs() < 0.08, "with it gone he stands");
}

fn shutter(stage: &mut Stage, check: &mut Check) {
    println!("half shutter");
    let found = stage
        .app
        .world_mut()
        .run_system_once(|layout: Res<MapLayout>, nav: Res<NavGrid>, floor: Res<StreetFloor>| {
            layout.kit.iter().filter(|k| k.name == "gumti").find_map(|k| {
                let rot = Quat::from_rotation_y(k.yaw);
                let out = rot * Vec3::NEG_Z;
                open(&nav, &floor, k.pos.xz() + out.xz() * 3.0).then_some((k.pos, out))
            })
        })
        .unwrap();
    let Some((at, out)) = found else {
        check.that(false, "no gumti on the map with its front clear");
        return;
    };
    println!("  {} gumti on the map", stage.app.world().resource::<MapLayout>().kit.iter().filter(|k| k.name == "gumti").count());
    let player = stage.player;
    let inside = |p: Vec3| (p - at).xz().dot(out.xz()) < 1.1;
    // Standing: the shutter stops him. Crouched: he goes under it.
    let front = at + out * 2.3;
    for (crouch, want_in, what) in [(false, false, "walking upright the shutter stops him"), (true, true, "crouched he goes under it")] {
        stage.put(player, front);
        stage.tick(6);
        let yaw = yaw_to(stage.pos(player), at);
        let mut i = stage.intent(player);
        i.yaw = yaw;
        i.move_axis = Vec2::Y;
        i.held = if crouch { btn::CROUCH } else { 0 };
        stage.tick(150);
        check.that(inside(stage.pos(player)) == want_in, what);
    }
    // Under the shutter he can't stand; further in he can.
    let mut i = stage.intent(player);
    i.move_axis = Vec2::ZERO;
    stage.put(player, at + out * 1.25);
    stage.tick(4);
    stage.intent(player).held = 0;
    stage.tick(10);
    check.that(stage.crouched(), "right under the shutter he stays down");
    stage.put(player, at);
    stage.intent(player).held = 0;
    stage.tick(10);
    check.that(!stage.crouched(), "inside he can stand");
}

/// The cover table: from how many bearings each piece hides a warrior, crouched and standing.
fn cover(stage: &mut Stage, check: &mut Check) {
    println!("cover, from 15 m, of the bearings on which beside the piece is in plain sight");
    println!("  {:<12} {:>6} {:>8} {:>16} {:>16}", "piece", "placed", "bearings", "crouched hidden", "standing hidden");
    let rows = stage
        .app
        .world_mut()
        .run_system_once(|layout: Res<MapLayout>, spatial: SpatialQuery| {
            let body = |p: Vec2, feet: f32| Vec3::new(p.x, feet + HALF_HEIGHT, p.y);
            COVER
                .iter()
                .map(|name| {
                    let pieces: Vec<_> = layout.kit.iter().filter(|k| k.name == *name).collect();
                    let (mut bearings, mut crouched, mut standing) = (0, 0, 0);
                    for k in &pieces {
                        let c = k.pos.xz();
                        // The piece's own half-width: a warrior hard up behind it.
                        let back = if *name == "gumti" { 0.0 } else if *name == "mumty" { 2.0 } else if *name == "auto" { 1.4 } else { 1.0 };
                        for b in 0..16 {
                            let d = Vec2::from_angle(b as f32 * TAU / 16.0);
                            let eye = body(c + d * 15.0, k.pos.y) + Vec3::Y * shared::sim::EYE_OFFSET;
                            let side = c + d * 0.5 + d.perp() * (back + 2.0);
                            // A warrior beside it in plain sight, or this bearing proves nothing.
                            if !in_sight(&spatial, eye, body(side, k.pos.y), 0.0) {
                                continue;
                            }
                            bearings += 1;
                            let hid = body(c - d * back, k.pos.y);
                            crouched += !in_sight(&spatial, eye, hid, CROUCH_DROP) as u32;
                            standing += !in_sight(&spatial, eye, hid, 0.0) as u32;
                        }
                    }
                    (*name, pieces.len(), bearings, crouched, standing)
                })
                .collect::<Vec<_>>()
        })
        .unwrap();
    for (name, placed, bearings, crouched, standing) in rows {
        let pct = |n: u32| 100.0 * n as f32 / (bearings as f32).max(1.0);
        println!("  {name:<12} {placed:>6} {bearings:>8} {:>15.0}% {:>15.0}%", pct(crouched), pct(standing));
        // Crates and drums stop short of a crouched head; washing leaves a crouch's legs showing; a
        // gumti's open front shows a crouch in the gap under its shutter, and not a man standing.
        match name {
            "crates" | "drum" => {}
            "clothesline" => check.that(pct(standing) >= 90.0, "washing hides a warrior standing behind it"),
            "gumti" => check.that(pct(standing) >= 99.0 && pct(crouched) >= 70.0, "a gumti hides whoever stands up inside"),
            _ => check.that(bearings > 0 && pct(crouched) >= 90.0, &format!("{name} hides a crouched warrior")),
        }
    }
}

/// The market's tables, on a map that has them: from before and behind each, at 15 m, on the bearings
/// where a warrior stepped out from the row is in plain sight, a warrior hard up behind it.
fn table_cover(stage: &mut Stage, check: &mut Check) {
    let (bearings, crouched, standing, tables) = stage
        .app
        .world_mut()
        .run_system_once(|layout: Res<MapLayout>, spatial: SpatialQuery| {
            let (mut bearings, mut crouched, mut standing) = (0, 0, 0);
            for s in layout.stalls.tables().step_by(5) {
                let c = s.pos.xz();
                let body = |p: Vec2| Vec3::new(p.x, s.pos.y + HALF_HEIGHT, p.y);
                for side in [1.0, -1.0] {
                    for turn in [-0.6f32, -0.3, 0.0, 0.3, 0.6] {
                        let d = Vec2::from_angle(turn).rotate(s.facing() * side);
                        let eye = body(c + d * 15.0) + Vec3::Y * EYE_OFFSET;
                        // Past the row's end the same line of sight proves the way is open.
                        if !in_sight(&spatial, eye, body(c + d * 0.5 + d.perp() * 3.0), 0.0) {
                            continue;
                        }
                        let hid = body(c - s.facing() * side * (s.half().y + 0.42));
                        bearings += 1;
                        crouched += !in_sight(&spatial, eye, hid, CROUCH_DROP) as u32;
                        standing += !in_sight(&spatial, eye, hid, 0.0) as u32;
                    }
                }
            }
            (bearings, crouched, standing, layout.stalls.tables().count())
        })
        .unwrap();
    if tables == 0 {
        return;
    }
    let pct = |n: u32| 100.0 * n as f32 / (bearings as f32).max(1.0);
    println!("  {:<12} {tables:>6} {bearings:>8} {:>15.0}% {:>15.0}%", "table", pct(crouched), pct(standing));
    check.that(bearings > 1000 && pct(crouched) >= 90.0, "a market table hides a crouched warrior");
    check.that(pct(standing) < 10.0, "and not one standing");
}

/// The real thing: a warrior behind a market table and a rifle firing at him from 15 m, with the
/// rifleman's brain out of the way. Crouched, every shot stops in the table. Standing, the same aim
/// a little higher finds him. Last of all: the bot is no use for anything else after this.
fn table_shots(stage: &mut Stage, check: &mut Check) {
    println!("a market table under fire");
    let (player, bot) = (stage.player, stage.bot);
    let found = stage
        .app
        .world_mut()
        .run_system_once(|layout: Res<MapLayout>, floor: Res<StreetFloor>, zone: Res<Zone>, spatial: SpatialQuery| {
            layout.stalls.tables().filter(|s| ring_calm(&zone, s.pos.xz())).find_map(|s| {
                let c = s.pos.xz();
                let f = s.facing();
                let body = |p: Vec2| Vec3::new(p.x, layout.terrain.height(p.x, p.y) + HALF_HEIGHT, p.y);
                let (front, behind) = (c + f * 15.0, c - f * (s.half().y + 0.42));
                let eye = body(front) + Vec3::Y * EYE_OFFSET;
                // Nothing else between them, on open street.
                let beside = body(c + f * 0.8 + f.perp() * 2.2);
                let clear = floor.reachable(front) && in_sight(&spatial, eye, beside, 0.0) && (0..4).all(|k| layout.stalls.tables().all(|o| !o.covers(behind + f.perp() * k as f32 * 0.25)));
                clear.then_some((s.pos, f, front, behind))
            })
        })
        .unwrap();
    let Some((table, f, front, behind)) = found else {
        check.that(false, "a table with open street in front of it");
        return;
    };
    println!("  table at {:.1}, rifle 15 m off at {:.1}", table, front);
    stage.app.world_mut().entity_mut(bot).remove::<BotBrain>();
    let feet = |s: &Stage, p: Vec2| Vec3::new(p.x, s.ground(p), p.y);
    let (at, from) = (feet(stage, behind), feet(stage, front));
    let life = |s: &Stage| s.app.world().get::<Health>(player).map_or(0.0, |h| h.hp + h.shield);
    // `aim` above the target's feet; `crouch`: how he holds himself.
    let volley = |stage: &mut Stage, crouch: bool, aim: f32| {
        stage.intent(player).held = if crouch { btn::CROUCH } else { 0 };
        stage.intent(player).move_axis = Vec2::ZERO;
        {
            let world = stage.app.world_mut();
            let mut h = world.get_mut::<Health>(player).unwrap();
            h.hp = h.max_hp;
            h.shield = h.max_shield;
            let mut inv = world.get_mut::<Inventory>(bot).unwrap();
            inv.slots[0] = Some(ItemStack { id: "saintalis".into(), rarity: Rarity::Rare, count: 30 });
            inv.selected = 1;
            inv.arrows = 90;
        }
        let target = at + Vec3::Y * aim;
        let (mut hits, mut stopped, mut fired) = (0, 0, 0);
        for k in 0..240 {
            stage.put(player, at);
            stage.put(bot, from);
            let gun = stage.pos(bot);
            let mut yaw_pitch = (0.0, 0.0);
            let mut origin = gun + Vec3::Y * EYE_OFFSET;
            for _ in 0..3 {
                let d = target - origin;
                yaw_pitch = ((-d.x).atan2(-d.z), d.y.atan2(d.xz().length()));
                let probe = PlayerIntent { yaw: yaw_pitch.0, pitch: yaw_pitch.1, held: btn::AIM, ..default() };
                origin = shared::sim::aim_ray(gun, &probe).0 + (Quat::from_euler(EulerRot::YXZ, yaw_pitch.0, yaw_pitch.1, 0.0) * Vec3::NEG_Z) * (CAMERA_DISTANCE - ADS_DISTANCE);
            }
            let before = life(stage);
            let mut i = stage.intent(bot);
            (i.yaw, i.pitch) = yaw_pitch;
            i.held = if k > 60 && k % 12 == 0 { btn::AIM | btn::FIRE } else { btn::AIM };
            fired += (k > 60 && k % 12 == 0) as u32;
            stage.tick(1);
            hits += (life(stage) < before) as u32;
        }
        stage.intent(bot).held = 0;
        stage.tick(30);
        let world = stage.app.world_mut();
        stopped += world.query::<(&Transform, &Projectile)>().iter(world).filter(|(t, p)| p.owner == bot && p.stuck && t.translation.xz().distance(table.xz()) < 1.8).count();
        let spent: Vec<Entity> = world.query::<(Entity, &Projectile)>().iter(world).filter(|(_, p)| p.owner == bot).map(|(e, _)| e).collect();
        for e in spent {
            world.despawn(e);
        }
        println!("  crouched {crouch}, aiming {aim:.2} m over his feet: {fired} fired, {hits} hit him, {stopped} stuck in the table");
        (hits, stopped)
    };
    let (hits, stopped) = volley(stage, true, 0.6);
    check.that(hits == 0 && stopped >= 3, "crouched behind it, a rifle's shots stop in the table");
    let (hits, _) = volley(stage, false, 1.5);
    check.that(hits >= 3, "standing behind it, his head and shoulders are a target");
    let _ = f;
}

/// Leaves on the NCR: a bush hides a crouch from 15 m and not from 4 m, a thicket hides anyone.
fn leaves(stage: &mut Stage, check: &mut Check) {
    let rows = stage
        .app
        .world_mut()
        .run_system_once(|layout: Res<MapLayout>, spatial: SpatialQuery| {
            let (mut far, mut hid_low, mut hid_tall, mut close, mut seen_close) = (0, 0, 0, 0, 0);
            for s in layout.soft.iter().step_by(7).filter(|s| s.size.y < 2.0) {
                let c = s.pos.xz();
                let (feet, body) = (layout.terrain.height(c.x, c.y), Vec3::new(c.x, layout.terrain.height(c.x, c.y) + HALF_HEIGHT, c.y));
                let solid = SpatialQueryFilter::from_mask(shared::sim::solid_mask());
                for b in 0..8 {
                    let d = Vec2::from_angle(b as f32 * TAU / 8.0);
                    let eye_at = |r: f32| {
                        let p = c + d * r;
                        Vec3::new(p.x, layout.terrain.height(p.x, p.y).max(feet) + HALF_HEIGHT + shared::sim::EYE_OFFSET, p.y)
                    };
                    // Only bearings with nothing solid in the way: those prove the leaves.
                    if shared::sim::senses::clear_line(&spatial, &solid, eye_at(15.0), body) {
                        far += 1;
                        hid_low += !in_sight(&spatial, eye_at(15.0), body, CROUCH_DROP) as u32;
                        hid_tall += !in_sight(&spatial, eye_at(15.0), body, 0.0) as u32;
                    }
                    if shared::sim::senses::clear_line(&spatial, &solid, eye_at(4.0), body) {
                        close += 1;
                        seen_close += in_sight(&spatial, eye_at(4.0), body, CROUCH_DROP) as u32;
                    }
                }
            }
            let (mut thick, mut thick_hid) = (0, 0);
            for s in layout.soft.iter().filter(|s| s.size.y > 2.0).step_by(5) {
                let c = s.pos.xz();
                let body = Vec3::new(c.x, layout.terrain.height(c.x, c.y) + HALF_HEIGHT, c.y);
                let p = c + Vec2::X * 26.0;
                let eye = Vec3::new(p.x, layout.terrain.height(p.x, p.y) + HALF_HEIGHT + shared::sim::EYE_OFFSET, p.y);
                let solid = SpatialQueryFilter::from_mask(shared::sim::solid_mask());
                if shared::sim::senses::clear_line(&spatial, &solid, eye, body) {
                    thick += 1;
                    thick_hid += !in_sight(&spatial, eye, body, 0.0) as u32;
                }
            }
            (layout.soft.len(), far, hid_low, hid_tall, close, seen_close, thick, thick_hid)
        })
        .unwrap();
    let (covers, far, hid_low, hid_tall, close, seen_close, thick, thick_hid) = rows;
    if covers == 0 {
        return;
    }
    let pct = |n: u32, of: u32| 100.0 * n as f32 / (of as f32).max(1.0);
    println!("leaves: {covers} covers; bush or hedge from 15 m ({far} bearings): crouch hidden {:.0}%, standing {:.0}%; from 4 m ({close}): crouch seen {:.0}%; thicket from 26 m ({thick}): standing hidden {:.0}%",
        pct(hid_low, far), pct(hid_tall, far), pct(seen_close, close), pct(thick_hid, thick));
    check.that(pct(hid_low, far) >= 90.0, "a bush hides a crouched warrior from 15 m");
    check.that(pct(seen_close, close) >= 90.0, "a bot at 4 m sees into a bush");
    check.that(pct(thick_hid, thick) >= 90.0, "a thicket hides a warrior standing in it");
}

fn senses_of_a_bot(stage: &mut Stage, check: &mut Check) {
    println!("sight");
    let spot = open_spot(stage, 26.0);
    let feet = spot.extend(stage.ground(spot)).xzy();
    let (player, bot) = (stage.player, stage.bot);
    // Past looting, armed: the bot acts on what it senses.
    {
        let world = stage.app.world_mut();
        let mut brain = world.get_mut::<BotBrain>(bot).unwrap();
        brain.landed = Some(-1000.0);
        brain.calm = false;
        let mut inv = world.get_mut::<Inventory>(bot).unwrap();
        inv.slots[0] = Some(ItemStack { id: "tamancha".into(), rarity: Rarity::Common, count: 6 });
        inv.arrows = 40;
    }
    let bot_at = feet + Vec3::X * 0.0;
    let ahead = Vec3::new(0.0, 0.0, -20.0);
    let fresh = |stage: &mut Stage| {
        let mut b = stage.app.world_mut().get_mut::<BotBrain>(bot).unwrap();
        b.senses = default();
        b.target = None;
    };
    // Standing in plain sight ahead: noticed, not at once.
    stage.intent(player).held = 0;
    stage.put(player, bot_at + ahead);
    stage.watch(2, bot_at, 0.0, |_| {});
    let first = stage.sees_player();
    stage.watch(90, bot_at, 0.0, |s| {
        s.put(player, bot_at + ahead);
    });
    check.that(!first && stage.sees_player(), "standing 20 m ahead he is noticed after a moment, not at once");
    // Crouched at 30 m: beyond a chill bot's crouch range (35 m * 0.6).
    fresh(stage);
    let chill_range = stage.brain().sight.0;
    let far = ahead.normalize() * (chill_range * 0.75);
    stage.intent(player).held = btn::CROUCH;
    stage.tick(3);
    stage.watch(90, bot_at, 0.0, |s| {
        s.put(player, bot_at + far);
    });
    check.that(!stage.sees_player(), &format!("crouched {:.0} m ahead he goes unnoticed", far.length()));
    // Behind the bot, close: a crouch passes, a run does not.
    fresh(stage);
    let behind = Vec3::new(3.0, 0.0, 1.5);
    stage.watch(60, bot_at, 0.0, |s| {
        s.put(player, bot_at + behind * 1.6);
    });
    check.that(!stage.sees_player(), "crouched just behind its back it stays calm");
    fresh(stage);
    stage.intent(player).held = 0;
    stage.tick(3);
    stage.watch(30, bot_at, 0.0, |s| {
        s.put(player, bot_at + behind);
    });
    check.that(stage.sees_player(), "standing that close it knows");

    println!("hearing");
    // Moving across, 12 m behind its back.
    let pace = |s: &mut Stage, crouch: bool, speed: f32| {
        s.put(player, bot_at + Vec3::new(0.0, 0.0, 12.0));
        s.intent(player).held = if crouch { btn::CROUCH } else { 0 };
        s.watch(3, bot_at, 0.0, |_| {});
        fresh(s);
        s.watch(30, bot_at, 0.0, |s| {
            s.put(player, bot_at + Vec3::new(0.0, 0.0, 12.0));
            s.app.world_mut().get_mut::<Kinematic>(player).unwrap().velocity = Vec3::X * speed;
        });
        s.brain().senses.known.iter().any(|k| k.who == player)
    };
    check.that(pace(stage, false, 7.4), "a sprint behind it at 12 m is heard");
    check.that(!pace(stage, true, 2.6), "a crouch at 12 m is not");
    stage.intent(player).held = 0;
    // Out of its range, on the far side of the shot.
    stage.put(player, bot_at + Vec3::new(-80.0, 0.0, 0.0));
    stage.watch(3, bot_at, 0.0, |_| {});
    fresh(stage);
    let shot = bot_at + Vec3::new(28.0, 1.0, 28.0);
    stage.app.world_mut().write_message(NoiseMsg { at: shot, radius: senses::SHOT_NOISE, source: Some(player) });
    stage.tick(30);
    let spot_of = stage.brain().senses.search.map(|s| s.spot);
    check.that(spot_of.is_some(), &format!("a shot 40 m off is heard, and it goes to look: {spot_of:?}"));
    fresh(stage);
    stage.app.world_mut().write_message(NoiseMsg { at: bot_at + Vec3::new(60.0, 1.0, 60.0), radius: senses::SHOT_NOISE, source: Some(player) });
    stage.watch(30, bot_at, 0.0, |_| {});
    let heard = stage.brain().senses.known.iter().any(|k| k.who == player);
    let went = stage.brain().senses.search.is_some();
    check.that(heard && !went, "a shot 85 m off is heard and left alone");

    println!("memory");
    fresh(stage);
    stage.put(bot, bot_at);
    stage.intent(player).held = 0;
    // The bot's fight, as it would start: facing the player, the brain free to act.
    let seen_at = bot_at + Vec3::new(10.0, 0.0, -12.0);
    stage.watch(90, bot_at, yaw_to(bot_at, seen_at), |s| s.put(player, seen_at));
    check.that(stage.brain().target == Some(player), "it picks the fight");
    // Gone: out of sight of it, and of the place it saw him.
    let far_side = stage
        .app
        .world_mut()
        .run_system_once(move |spatial: SpatialQuery, nav: Res<NavGrid>, floor: Res<StreetFloor>, layout: Res<MapLayout>| {
            let eye = seen_at + Vec3::Y * (HALF_HEIGHT + EYE_OFFSET);
            senses::hiding_spot(&spatial, &|p| open(&nav, &floor, p), &layout, eye, seen_at.xz(), 60.0, CROUCH_DROP)
        })
        .unwrap();
    let Some(far_side) = far_side else {
        check.that(false, "a hiding place from the bot");
        return;
    };
    let last = stage.pos(player);
    let gone = far_side.extend(stage.ground(far_side)).xzy();
    let hide_player = move |s: &mut Stage| {
        s.put(player, gone);
    };
    let mut lost_at = None;
    let mut arrived = None;
    let mut gave_up = None;
    for k in 0..60 * 40 {
        hide_player(stage);
        stage.intent(player).held = btn::CROUCH;
        stage.tick(1);
        let t = k as f32 / 60.0;
        let b = stage.brain();
        if lost_at.is_none() && b.target.is_none() && b.senses.search.is_some() {
            lost_at = Some((t, b.senses.search.unwrap().spot));
        }
        if arrived.is_none() && b.senses.search.is_some_and(|s| s.looking.is_some()) {
            arrived = Some(t);
        }
        if arrived.is_some() && b.senses.search.is_none() {
            gave_up = Some(t);
            break;
        }
    }
    println!("  lost at {lost_at:?}, arrived {arrived:?}, gave up {gave_up:?}");
    let spot = lost_at.map(|l| l.1).unwrap_or(Vec3::ZERO);
    check.that(lost_at.is_some(), "losing him it sets off to search");
    check.that(spot.xz().distance(last.xz()) < 5.0, &format!("it goes to where it last saw him ({:.1} m off), not where he is ({:.0} m)", spot.xz().distance(last.xz()), last.distance(gone)));
    check.that(arrived.is_some(), "it gets there and looks round");
    check.that(gave_up.is_some(), "and gives up");
    check.that(stage.brain().senses.known.iter().all(|k| k.who != player), "it has forgotten him");
}

/// A bot losing a fight runs for somewhere its enemy can't see, on whatever ground this map has.
fn running_away(stage: &mut Stage, check: &mut Check) {
    println!("running away");
    let (player, bot) = (stage.player, stage.bot);
    // A street where the player is in plain sight of the bot, with a corner to get round.
    let found = stage
        .app
        .world_mut()
        .run_system_once(|spatial: SpatialQuery, nav: Res<NavGrid>, floor: Res<StreetFloor>, layout: Res<MapLayout>, zone: Res<Zone>| {
            let walkable = |p: Vec2| open(&nav, &floor, p);
            let body = |p: Vec2| Vec3::new(p.x, layout.terrain.height(p.x, p.y) + HALF_HEIGHT, p.y);
            layout
                .kit
                .iter()
                .filter(|k| k.pos.y < layout.terrain.height(k.pos.x, k.pos.z) + 1.0 && ring_calm(&zone, k.pos.xz()))
                .flat_map(|k| (0..8).map(move |i| k.pos.xz() + Vec2::from_angle(i as f32 * TAU / 8.0) * 6.0))
                .find_map(|at| {
                    let foe = at + Vec2::X * 18.0;
                    let eye = body(foe) + Vec3::Y * EYE_OFFSET;
                    if !walkable(at) || !walkable(foe) || !in_sight(&spatial, eye, body(at), 0.0) {
                        return None;
                    }
                    senses::hiding_spot(&spatial, &walkable, &layout, eye, at, 24.0, 0.0).map(|_| (body(at), body(foe)))
                })
        })
        .unwrap();
    let Some((at, foe)) = found else {
        check.that(false, "a street with a corner to run round");
        return;
    };
    let feet = |p: Vec3| p - Vec3::Y * HALF_HEIGHT;
    stage.intent(player).held = 0;
    {
        let world = stage.app.world_mut();
        let mut b = world.get_mut::<BotBrain>(bot).unwrap();
        b.senses = default();
        b.target = None;
        b.goal = None;
        b.flee = 0.0;
    }
    stage.watch(90, feet(at), yaw_to(at, foe), |s| {
        s.put(player, feet(foe));
        let mut h = s.app.world_mut().get_mut::<Health>(player).unwrap();
        h.hp = h.max_hp;
    });
    check.that(stage.brain().target == Some(player), "it picks the fight");
    let before = stage.brain().senses.tally;
    stage.app.world_mut().get_mut::<Health>(bot).unwrap().hp = 30.0;
    let mut held = 0;
    while stage.brain().flee <= 0.0 && held < 30 {
        held += 1;
        stage.put(player, feet(foe));
        stage.tick(1);
    }
    let tally = stage.brain().senses.tally;
    check.that(tally.flights == before.flights + 1, "with its health gone it breaks off and runs");
    check.that(tally.hid == before.hid + 1, "to a place out of the enemy's sight");
    let goal = stage.brain().goal.expect("somewhere to run to");
    let eye = foe + Vec3::Y * EYE_OFFSET;
    let hidden_there = stage
        .app
        .world_mut()
        .run_system_once(move |spatial: SpatialQuery, layout: Res<MapLayout>| !in_sight(&spatial, eye, Vec3::new(goal.x, layout.terrain.height(goal.x, goal.z) + HALF_HEIGHT, goal.z), 0.0))
        .unwrap();
    check.that(hidden_there, &format!("the place it chose, {:.0} m off, is one he can't see", goal.xz().distance(at.xz())));
    let mut clear = false;
    for _ in 0..60 * 6 {
        stage.put(player, feet(foe));
        stage.tick(1);
        let pos = stage.pos(bot);
        let seen = stage.app.world_mut().run_system_once(move |spatial: SpatialQuery| in_sight(&spatial, eye, pos, 0.0)).unwrap();
        if !seen {
            clear = true;
            break;
        }
    }
    check.that(clear, "and it gets out of his sight");
    println!("  out of sight {:.0} m from where it started", stage.pos(bot).xz().distance(at.xz()));
}

/// Rifle shots as a player fires them through a scope, real bullets across open ground and off a
/// roof: past what a bot takes in, the blow alone tells it. Nearer than the answer range (120 m) it
/// turns on the shooter it can see and goes looking for one it can't; past that it runs for cover.
fn long_shots(stage: &mut Stage, check: &mut Check) {
    println!("long shots");
    const NEAR: f32 = 100.0;
    const FAR: f32 = 150.0;
    /// `Projectile::ttl` as a bullet leaves.
    const BULLET_LIFE: f32 = 4.0;
    let (player, bot) = (stage.player, stage.bot);
    let (speed, gravity) = match stage.app.world().resource::<ItemDb>().get("thri_not_thri").kind {
        ItemKind::Bow { speed, gravity, .. } => (speed, gravity),
        _ => panic!("the rifle is a gun"),
    };
    let (ground, roof) = stage
        .app
        .world_mut()
        .run_system_once(|spatial: SpatialQuery, nav: Res<NavGrid>, floor: Res<StreetFloor>, layout: Res<MapLayout>, zone: Res<Zone>| {
            let walkable = |p: Vec2| open(&nav, &floor, p);
            let feet = |p: Vec2| Vec3::new(p.x, layout.terrain.height(p.x, p.y), p.y);
            let eye = |feet: Vec3| feet + Vec3::Y * (HALF_HEIGHT + EYE_OFFSET);
            let body = |feet: Vec3| feet + Vec3::Y * HALF_HEIGHT;
            let bearings = || (0..16).map(|k| Vec2::from_angle(k as f32 * TAU / 16.0));
            let around = |p: Vec2, r: f32| (0..8).map(move |i| p + Vec2::from_angle(i as f32 * TAU / 8.0) * r);
            // Open ground, the shooter in plain sight from both ranges.
            let on_ground = layout.kit.iter().filter(|k| ring_calm(&zone, k.pos.xz())).flat_map(|k| around(k.pos.xz(), 6.0)).filter(|at| walkable(*at)).find_map(|at| {
                let clear = |dir: Vec2, d: f32| walkable(at + dir * d) && in_sight(&spatial, eye(feet(at + dir * d)), body(feet(at)), 0.0);
                bearings().find(|dir| clear(*dir, NEAR) && clear(*dir, FAR)).map(|dir| (feet(at), dir))
            });
            // Beside something standing on a roof, with a street in sight far below.
            let filter = SpatialQueryFilter::from_mask(solid_mask());
            let on_roof = layout.kit.iter().filter(|k| k.pos.y > layout.terrain.height(k.pos.x, k.pos.z) + 5.0).flat_map(|k| around(k.pos.xz(), 2.5).map(move |p| (k.pos.y, p))).find_map(|(y, p)| {
                let from = Vec3::new(p.x, y + 3.0, p.y);
                let top = from.y - spatial.cast_ray(from, Dir3::NEG_Y, 6.0, true, &filter)?.distance;
                let stand = Vec3::new(p.x, top, p.y);
                let seen = |dir: &Vec2| walkable(p + *dir * FAR) && in_sight(&spatial, eye(stand), body(feet(p + *dir * FAR)), 0.0);
                ((top - y).abs() < 0.3).then(|| bearings().find(seen).map(|dir| (stand, feet(p + dir * FAR)))).flatten()
            });
            (on_ground, on_roof)
        })
        .unwrap();
    // Fires from `from` at a bot held at `at`, both feet. True when the bullet told.
    let shoot = |stage: &mut Stage, from: Vec3, at: Vec3, facing: f32| {
        {
            let world = stage.app.world_mut();
            let mut brain = world.get_mut::<BotBrain>(bot).unwrap();
            brain.senses = default();
            brain.target = None;
            brain.goal = None;
            brain.flee = 0.0;
            brain.provoked = None;
            let mut inv = world.get_mut::<Inventory>(player).unwrap();
            inv.slots[0] = Some(ItemStack { id: "thri_not_thri".into(), rarity: Rarity::Rare, count: 5 });
            inv.selected = 1;
            inv.arrows = 40;
            for who in [player, bot] {
                let mut h = world.get_mut::<Health>(who).unwrap();
                h.hp = h.max_hp;
                // A rifle's one hit would otherwise leave it losing, and running for that reason.
                h.shield = 100.0;
            }
        }
        let (eye, chest) = (from + Vec3::Y * (HALF_HEIGHT + 0.03 + EYE_OFFSET), at + Vec3::Y * (HALF_HEIGHT + 0.03 + 0.3));
        let flight = eye.distance(chest) / speed;
        let d = chest + Vec3::Y * (0.5 * gravity * flight * flight) - eye;
        let (yaw, pitch) = (yaw_to(eye, eye + d), d.y.atan2(d.xz().length()));
        let shooter = |s: &mut Stage, held: u16| {
            s.put(player, from);
            let mut intent = s.intent(player);
            (intent.yaw, intent.pitch, intent.held) = (yaw, pitch, held);
        };
        stage.watch(45, at, facing, |s| shooter(s, btn::AIM));
        stage.watch(1, at, facing, |s| shooter(s, btn::AIM | btn::FIRE));
        let life = |s: &Stage| s.app.world().get::<Health>(bot).map_or(0.0, |h| h.hp + h.shield);
        let before = life(stage);
        stage.watch((flight * 60.0) as u32 + 40, at, facing, |s| shooter(s, 0));
        life(stage) < before
    };
    if let Some((at, dir)) = ground {
        let foe = |d: f32| Vec3::new(at.x + dir.x * d, stage.ground(at.xz() + dir * d), at.z + dir.y * d);
        let (near, far) = (foe(NEAR), foe(FAR));
        let hit = shoot(stage, near, at, yaw_to(at, near));
        check.that(hit && stage.brain().target == Some(player), "hit from 100 m across open ground, it shoots back at the man it sees");
        let hit = shoot(stage, near, at, yaw_to(near, at));
        let search = stage.brain().senses.search.map(|s| s.spot);
        check.that(hit && search.is_some_and(|s| s.xz().distance(near.xz()) < 26.0) && stage.brain().flee <= 0.0, "hit from 100 m in the back, it goes to the place the shot came from");
        let before = stage.brain().senses.tally.flights;
        let hit = shoot(stage, far, at, yaw_to(at, far));
        check.that(hit && stage.brain().flee > 0.0 && stage.brain().target.is_none(), "hit from 150 m, it runs for cover");
        check.that(stage.brain().senses.tally.flights == before + 1, "just the one flight");
    } else {
        println!("  no line that long in the middle of this map");
    }
    if let Some((from, at)) = roof {
        let hit = shoot(stage, from, at, 0.0);
        check.that(hit, &format!("a shot from a roof {:.0} m up lands {:.0} m off", from.y - at.y, from.xz().distance(at.xz())));
    } else {
        println!("  no roof with a long view on this map");
    }
    // The height a shot leaves at, through the scope and from the hip, standing and crouched: up at
    // the sky, with nothing to stop it short, and worked back along its flight, as spread tilts it.
    let spot = ground.map_or(open_spot(stage, 6.0), |g| g.0.xz());
    let feet = spot.extend(stage.ground(spot)).xzy();
    let shot_at = |stage: &mut Stage, crouch: u16, aim: u16| {
        let world = stage.app.world_mut();
        world.get_mut::<Inventory>(player).unwrap().slots[0] = Some(ItemStack { id: "thri_not_thri".into(), rarity: Rarity::Rare, count: 5 });
        for _ in 0..100 {
            stage.put(player, feet);
            let mut intent = stage.intent(player);
            (intent.pitch, intent.held) = (0.5, crouch | aim);
            stage.tick(1);
        }
        stage.intent(player).held = crouch | aim | btn::FIRE;
        stage.tick(1);
        stage.intent(player).held = crouch;
        let world = stage.app.world_mut();
        world.query::<(&Transform, &Projectile)>().iter(world).filter(|(_, p)| p.owner == player).max_by(|a, b| a.1.ttl.total_cmp(&b.1.ttl)).map_or(0.0, |(t, p)| t.translation.y - p.velocity.y * (BULLET_LIFE - p.ttl))
    };
    // The hands drop with the capsule's top; the eye, which a scope fires from, a little less.
    let dropped = |standing: f32, down: f32, by: f32| (standing - down - by).abs() < 0.03;
    let (eye, low_eye) = (shot_at(stage, 0, btn::AIM), shot_at(stage, btn::CROUCH, btn::AIM));
    let (hip, low_hip) = (shot_at(stage, 0, 0), shot_at(stage, btn::CROUCH, 0));
    check.that(dropped(eye, low_eye, shared::sim::CROUCH_EYE_DROP), &format!("crouched, a scoped shot leaves {:.2} m lower", eye - low_eye));
    check.that(dropped(hip, low_hip, CROUCH_DROP), &format!("crouched, a shot from the hip leaves {:.2} m lower", hip - low_hip));
    // Where a rifle's shots land on a wall 8 m ahead, against the middle of the player's view: the
    // camera worked out as crate::camera does, 30 shots to even out the spread.
    let wall_z = feet.z - 8.0;
    let wall = stage.app.world_mut().spawn((RigidBody::Static, Collider::cuboid(14.0, 14.0, 0.4), Transform::from_xyz(feet.x, feet.y + 1.0, wall_z), CollisionLayers::new(Layer::World, LayerMask::ALL))).id();
    stage.tick(2);
    let off_centre = |stage: &mut Stage, held: u16, eye: f32, back: f32| {
        const SHOTS: u32 = 30;
        let (mut landed, mut fired) = (Vec2::ZERO, 0);
        let mut centre = Vec3::ZERO;
        for _ in 0..SHOTS {
            stage.app.world_mut().get_mut::<Inventory>(player).unwrap().slots[0] = Some(ItemStack { id: "saintalis".into(), rarity: Rarity::Rare, count: 30 });
            for _ in 0..50 {
                stage.put(player, feet);
                let mut intent = stage.intent(player);
                (intent.pitch, intent.yaw, intent.held) = (0.0, 0.0, held);
                stage.tick(1);
            }
            stage.intent(player).held = held | btn::FIRE;
            stage.tick(1);
            stage.intent(player).held = held;
            let world = stage.app.world_mut();
            centre = world.get::<Transform>(player).unwrap().translation;
            // Just the shot that's still on its way to the wall, if the gun went off this tick.
            let shot = world.query::<(&Transform, &Projectile)>().iter(world).filter(|(t, p)| p.owner == player && p.item == "saintalis" && t.translation.z > wall_z + 0.5).max_by(|a, b| a.1.ttl.total_cmp(&b.1.ttl));
            if let Some((t, p)) = shot {
                let at = t.translation + p.velocity * (wall_z + 0.2 - t.translation.z) / p.velocity.z;
                landed += at.xy();
                fired += 1;
            }
        }
        let camera = centre + Vec3::Y * eye + Vec3::new(CAMERA_SHOULDER, 0.0, back);
        (fired, (landed / fired.max(1) as f32 - camera.xy()).length())
    };
    let low_eye = EYE_OFFSET - CROUCH_EYE_DROP;
    for (what, held, eye, back) in [
        ("crouched and aiming", btn::CROUCH | btn::AIM, low_eye, ADS_DISTANCE),
        ("crouched from the hip", btn::CROUCH, low_eye, CAMERA_DISTANCE),
        ("standing and aiming", btn::AIM, EYE_OFFSET, ADS_DISTANCE),
    ] {
        let (fired, off) = off_centre(stage, held, eye, back);
        check.that(fired >= 10 && off < 0.12, &format!("{what}, {fired} of a rifle's shots land {off:.2} m off the crosshair at 8 m"));
    }
    stage.app.world_mut().despawn(wall);
    stage.intent(player).held = 0;
}

fn flocks(stage: &mut Stage, check: &mut Check) {
    println!("birds");
    let player = stage.player;
    let world = stage.app.world_mut();
    let count = world.query::<&Flock>().iter(world).count();
    let found = world.query::<(Entity, &Flock)>().iter(world).find(|(_, f)| f.at.y > 6.0).map(|(e, f)| (e, f.at));
    let Some((flock, at)) = found else {
        check.that(false, "flocks on the map");
        return;
    };
    println!("  {count} flocks on the map");
    let foot = Vec3::new(at.x + 3.0, stage.ground(at.xz() + Vec2::X * 3.0), at.z);
    let away = |s: &Stage| s.app.world().get::<Flock>(flock).unwrap().away;
    stage.put(player, foot);
    stage.intent(player).held = btn::CROUCH;
    stage.tick(3);
    for _ in 0..30 {
        stage.put(player, foot);
        stage.app.world_mut().get_mut::<Kinematic>(player).unwrap().velocity = Vec3::X * 2.6;
        stage.tick(1);
    }
    check.that(away(stage) <= 0.0, "a crouch passes under them");
    stage.intent(player).held = 0;
    stage.tick(3);
    // A fifth of a second at least: each flock looks in its turn.
    for _ in 0..15 {
        stage.put(player, foot);
        stage.app.world_mut().get_mut::<Kinematic>(player).unwrap().velocity = Vec3::X * 5.6;
        stage.tick(1);
    }
    check.that(away(stage) > 0.0, "a walk puts them up");
    stage.put(player, foot);
    stage.tick(3);
}

fn main() {
    let mut stage = setup();
    let mut check = Check { failed: 0 };
    crouching(&mut stage, &mut check);
    shutter(&mut stage, &mut check);
    cover(&mut stage, &mut check);
    table_cover(&mut stage, &mut check);
    leaves(&mut stage, &mut check);
    flocks(&mut stage, &mut check);
    stage.intent(stage.player).held = 0;
    stage.intent(stage.player).move_axis = Vec2::ZERO;
    stage.tick(4);
    senses_of_a_bot(&mut stage, &mut check);
    running_away(&mut stage, &mut check);
    long_shots(&mut stage, &mut check);
    if !stage.app.world().resource::<MapLayout>().stalls.stalls.is_empty() {
        table_shots(&mut stage, &mut check);
    }
    println!("{}", if check.failed == 0 { "all good".to_string() } else { format!("{} failed", check.failed) });
    std::process::exit((check.failed > 0) as i32);
}
