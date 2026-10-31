//! Sound: positional one-shots for what happens in the world, looping beds for where the local
//! warrior is (vimana, freefall, glide, zone wall, river, the day around them).
//!
//! Every sound is synthesized by tools/audio/sfx.py. Events come from the simulation's messages
//! and from state changes noticed here (a swing starting, landing, a chest lid going up), so the
//! sim stays free of presentation. Distance attenuation is rodio's 1/d, scaled by
//! `SPATIAL_SCALE` so full volume reaches out to ~6 m; beyond `cull` metres a sound isn't played.

use std::collections::{HashMap, HashSet};

use avian3d::prelude::*;
use bevy::audio::{SpatialScale, Volume};
use bevy::prelude::*;
use shared::items::{ItemDb, ItemKind};
use shared::map::{ChestKind, MapLayout, Mat};
use shared::sim::combat::{Burst, FireZone, KillMsg, Projectile};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::Chest;
use shared::sim::zone::Zone;
use shared::sim::{Action, Eliminated, HitMsg, Inventory, Kinematic, LocalPlayer, MapBlock, MapTerrain, MoveMode, Warrior, solid_mask};

use crate::animation::Footstep;
use crate::camera::MainCamera;

pub const SPATIAL_SCALE: f32 = 0.16;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Played>()
            .add_systems(Startup, load)
            .add_systems(Update, (listener, footsteps, warrior_events, combat_events, world_events, beds, log_counts));
        // Automated runs (autoplay, tours) shouldn't sound through the speakers.
        if std::env::var("VIVAD_MUTE").is_ok() {
            app.insert_resource(GlobalVolume::new(Volume::Linear(0.0)));
        }
    }
}

// Mix levels by category.
const STEP: f32 = 0.35;
const WEAPON: f32 = 0.8;
const IMPACT: f32 = 0.7;
const UI: f32 = 0.5;
const ITEM: f32 = 0.6;
const BED: f32 = 0.45;

const NAMES: [&str; 67] = [
    "step_dirt_0", "step_dirt_1", "step_dirt_2", "step_dirt_3", "step_stone_0", "step_stone_1", "step_stone_2", "step_stone_3",
    "step_wood_0", "step_wood_1", "step_wood_2", "step_wood_3", "step_grass_0", "step_grass_1", "step_grass_2", "step_grass_3",
    "step_water_0", "step_water_1", "step_water_2", "step_water_3", "sword_whoosh_0", "sword_whoosh_1", "sword_whoosh_2",
    "sword_hit_flesh", "sword_hit_stone", "bow_release", "bow_release_heavy", "bow_draw", "arrow_impact_wood",
    "arrow_impact_stone", "gun_tamancha", "gun_dunali", "gun_katta", "gulel_release", "bullet_impact_stone",
    "bullet_impact_wood", "katta_click", "katta_backfire", "chakra_throw", "brahmastra_blast", "hit_flesh", "hit_shield", "hitmarker", "hitmarker_head", "hurt", "jump", "land", "glider_open",
    "glide_loop", "freefall_loop", "vimana_loop", "chest_open", "kalash_open", "pickup", "drink", "reload", "fire_ignite",
    "fire_loop", "wind_gust", "throw", "zone_warn", "zone_loop", "kill", "eliminated", "victory", "amb_day", "amb_river",
];

#[derive(Resource)]
struct Sfx(HashMap<&'static str, Handle<AudioSource>>);

/// Looping beds, faded by `beds` each frame.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
enum Bed {
    Day,
    River,
    Vimana,
    Freefall,
    Glide,
    Zone,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    let sfx = Sfx(NAMES.iter().map(|n| (*n, assets.load(format!("audio/{n}.ogg")))).collect());
    for (bed, name) in [(Bed::Day, "amb_day"), (Bed::River, "amb_river"), (Bed::Vimana, "vimana_loop"), (Bed::Freefall, "freefall_loop"),
                        (Bed::Glide, "glide_loop"), (Bed::Zone, "zone_loop")] {
        commands.spawn((bed, AudioPlayer(sfx.0[name].clone()), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0))));
    }
    commands.insert_resource(sfx);
}

/// How many times each sound was triggered (VIVAD_AUDIO_LOG prints it every 30 s).
#[derive(Resource, Default)]
struct Played(HashMap<String, u32>);

fn log_counts(time: Res<Time>, played: Res<Played>, mut next: Local<f32>) {
    if std::env::var("VIVAD_AUDIO_LOG").is_err() || time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 30.0;
    let mut v: Vec<_> = played.0.iter().collect();
    v.sort();
    info!("sounds played: {v:?}");
}

fn listener(mut commands: Commands, cam: Query<Entity, (With<MainCamera>, Without<SpatialListener>)>) {
    for e in &cam {
        commands.entity(e).insert(SpatialListener::new(0.3));
    }
}

/// Plays sounds; positional ones beyond `cull` metres from the listener are skipped.
#[derive(bevy::ecs::system::SystemParam)]
struct Player<'w, 's> {
    commands: Commands<'w, 's>,
    sfx: Res<'w, Sfx>,
    played: ResMut<'w, Played>,
    ear: Query<'w, 's, &'static GlobalTransform, With<SpatialListener>>,
}

impl Player<'_, '_> {
    fn at(&mut self, name: &str, pos: Vec3, volume: f32, cull: f32) {
        let ear = self.ear.iter().next().map(|t| t.translation()).unwrap_or(pos);
        if ear.distance(pos) > cull {
            return;
        }
        let Some(h) = self.sfx.0.get(name) else { return };
        *self.played.0.entry(name.to_owned()).or_default() += 1;
        // A touch of pitch variety so repeats don't sound mechanical.
        let speed = 1.0 + (pos.x * 12.9898 + pos.z * 78.233).sin().fract() * 0.08;
        self.commands.spawn((
            AudioPlayer(h.clone()),
            PlaybackSettings::DESPAWN
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(SPATIAL_SCALE))
                .with_volume(Volume::Linear(volume))
                .with_speed(speed),
            Transform::from_translation(pos),
        ));
    }

    fn ui(&mut self, name: &str, volume: f32) {
        let Some(h) = self.sfx.0.get(name) else { return };
        *self.played.0.entry(name.to_owned()).or_default() += 1;
        self.commands.spawn((AudioPlayer(h.clone()), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume))));
    }

    /// A looping positional sound riding on `parent` (despawned with it).
    fn attach_loop(&mut self, name: &str, parent: Entity, volume: f32) {
        let Some(h) = self.sfx.0.get(name) else { return };
        self.commands.spawn((
            AudioPlayer(h.clone()),
            PlaybackSettings::LOOP
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(SPATIAL_SCALE))
                .with_volume(Volume::Linear(volume)),
            Transform::default(),
            ChildOf(parent),
        ));
    }
}

/// What the ground under a point is made of, by the collider below it.
fn surface(spatial: &SpatialQuery, blocks: &Query<&MapBlock>, terrain: &Query<(), With<MapTerrain>>, layout: &MapLayout, pos: Vec3) -> &'static str {
    let filter = SpatialQueryFilter::from_mask(solid_mask());
    let Some(hit) = spatial.cast_ray(pos + Vec3::Y * 0.5, Dir3::NEG_Y, 1.5, true, &filter) else { return "dirt" };
    if let Ok(b) = blocks.get(hit.entity) {
        return match b.0.mat {
            Mat::Wood => "wood",
            Mat::Grass | Mat::Foliage | Mat::Thatch | Mat::Cloth => "grass",
            Mat::Dirt => "dirt",
            Mat::Water => "water",
            _ => "stone",
        };
    }
    if terrain.contains(hit.entity) {
        let p = pos.xz();
        if p.x > shared::map::india::RIVER_X.0 && p.x < shared::map::india::RIVER_X.1 && pos.y < -0.3 {
            return "water";
        }
        // Roads are packed earth; the rest is grass.
        let t = &layout.terrain;
        let step = t.step();
        let (i, j) = (((p.x + t.size / 2.0) / step).round() as usize, ((p.y + t.size / 2.0) / step).round() as usize);
        let road = t.road_d.get(i.min(t.n - 1) * t.n + j.min(t.n - 1)).copied().unwrap_or(1e3);
        return if road < 6.0 { "dirt" } else { "grass" };
    }
    "stone"
}

fn footsteps(
    mut steps: MessageReader<Footstep>,
    mut play: Player,
    spatial: SpatialQuery,
    blocks: Query<&MapBlock>,
    terrain: Query<(), With<MapTerrain>>,
    layout: Res<MapLayout>,
    mut n: Local<u32>,
) {
    for s in steps.read() {
        *n += 1;
        let kind = surface(&spatial, &blocks, &terrain, &layout, s.pos);
        let v = STEP * (0.6 + 0.4 * (s.speed / 7.4).min(1.0));
        play.at(&format!("step_{kind}_{}", *n % 4), s.pos, v, 35.0);
    }
}

/// Per-warrior state from the previous frame, to notice transitions.
#[derive(Component, Default)]
struct Heard {
    grounded: bool,
    mode: Option<MoveMode>,
    vy: f32,
    swinging: bool,
    reloading: bool,
    using: bool,
    charging: bool,
}

#[allow(clippy::type_complexity)]
fn warrior_events(
    mut commands: Commands,
    mut play: Player,
    db: Res<ItemDb>,
    mut q: Query<(Entity, &Transform, &Kinematic, &Action, &Inventory, Option<&mut Heard>, Has<LocalPlayer>), (With<Warrior>, Without<Eliminated>)>,
    mut n: Local<u32>,
) {
    for (e, tf, kin, act, inv, heard, local) in &mut q {
        let Some(mut h) = heard else {
            commands.entity(e).insert(Heard { grounded: kin.grounded, mode: Some(kin.mode), ..default() });
            continue;
        };
        let p = tf.translation;
        let held = inv.current().map(|s| (s.id.clone(), db.get(&s.id).kind.clone()));
        // Swings, draws, reloads, drinking.
        let swinging = act.swing_hit_in.is_some();
        if swinging && !h.swinging {
            *n += 1;
            play.at(&format!("sword_whoosh_{}", *n % 3), p, WEAPON, 60.0);
        }
        let charging = act.charge > 0.0;
        if charging && !h.charging {
            play.at("bow_draw", p, WEAPON * 0.6, 40.0);
        }
        let reloading = act.reload_left > 0.0;
        if reloading && !h.reloading {
            play.at("reload", p, ITEM, 30.0);
        }
        let using = act.using.is_some();
        if using && !h.using && matches!(held, Some((_, ItemKind::Consumable { .. }))) {
            play.at("drink", p, ITEM, 25.0);
        }
        // Movement: take-off, landing, the glider opening.
        if kin.mode == MoveMode::Ground {
            if h.grounded && !kin.grounded && kin.velocity.y > 2.0 {
                play.at("jump", p, STEP * 1.4, 30.0);
            }
            if !h.grounded && kin.grounded && h.vy < -5.0 {
                play.at("land", p, STEP * 1.6 * (-h.vy / 12.0).clamp(0.5, 1.5), 35.0);
            }
        }
        if kin.mode == MoveMode::Gliding && h.mode != Some(MoveMode::Gliding) {
            play.at("glider_open", p, if local { 0.7 } else { 0.4 }, 60.0);
        }
        *h = Heard { grounded: kin.grounded, mode: Some(kin.mode), vy: kin.velocity.y, swinging, reloading, using, charging };
    }
}

#[allow(clippy::type_complexity)]
fn combat_events(
    mut play: Player,
    mut hits: MessageReader<HitMsg>,
    mut kills: MessageReader<KillMsg>,
    mut misfires: MessageReader<shared::sim::combat::MisfireMsg>,
    db: Res<ItemDb>,
    arrows: Query<(Entity, &Projectile, &Transform)>,
    added: Query<(&Projectile, &Transform), Added<Projectile>>,
    inv: Query<&Inventory>,
    local: Query<Entity, With<LocalPlayer>>,
    spatial: SpatialQuery,
    blocks: Query<&MapBlock>,
    terrain: Query<(), With<MapTerrain>>,
    layout: Res<MapLayout>,
    mut stuck: Local<HashSet<Entity>>,
) {
    let me = local.iter().next();
    // One report per shot, however many pellets it fires.
    let mut fired: Vec<Entity> = vec![];
    for (p, tf) in &added {
        if fired.contains(&p.owner) {
            continue;
        }
        fired.push(p.owner);
        match p.astra {
            None => {
                let heavy = matches!(db.get(&p.item).kind, ItemKind::Bow { charge, .. } if charge > 0.0);
                let (sound, reach) = match p.item.as_str() {
                    "tamancha" => ("gun_tamancha", 200.0),
                    "dunali" => ("gun_dunali", 220.0),
                    "katta" => ("gun_katta", 220.0),
                    "gulel" => ("gulel_release", 60.0),
                    _ if heavy => ("bow_release_heavy", 120.0),
                    _ => ("bow_release", 120.0),
                };
                play.at(sound, tf.translation, WEAPON, reach);
            }
            Some(_) if p.item == "chakra" => play.at("chakra_throw", tf.translation, WEAPON, 80.0),
            Some(_) => play.at("throw", tf.translation, WEAPON, 60.0),
        }
    }
    // Arrows that just stuck into the world.
    let live: HashSet<Entity> = arrows.iter().map(|(e, ..)| e).collect();
    stuck.retain(|e| live.contains(e));
    for (e, p, tf) in &arrows {
        if p.stuck && stuck.insert(e) {
            let soft = matches!(surface(&spatial, &blocks, &terrain, &layout, tf.translation), "wood" | "grass" | "dirt");
            let sound = match (crate::visuals::shot_kind(&p.item), soft) {
                (crate::visuals::Shot::Bullet, true) => "bullet_impact_wood",
                (crate::visuals::Shot::Bullet, false) => "bullet_impact_stone",
                (_, true) => "arrow_impact_wood",
                (_, false) => "arrow_impact_stone",
            };
            play.at(sound, tf.translation, IMPACT, 60.0);
        }
    }
    for h in hits.read() {
        let melee = inv.get(h.attacker).ok().and_then(|i| i.current()).is_some_and(|s| matches!(db.get(&s.id).kind, ItemKind::Melee { .. }));
        play.at(if h.shield { "hit_shield" } else if melee { "sword_hit_flesh" } else { "hit_flesh" }, h.at, IMPACT, 60.0);
        if Some(h.attacker) == me {
            play.ui(if h.headshot { "hitmarker_head" } else { "hitmarker" }, UI);
        }
        if Some(h.target) == me {
            play.ui("hurt", UI * 1.2);
        }
    }
    for m in misfires.read() {
        play.at(if m.backfire { "katta_backfire" } else { "katta_click" }, m.at, WEAPON, if m.backfire { 120.0 } else { 25.0 });
    }
    for k in kills.read() {
        if Some(k.killer) == me {
            play.ui("kill", UI);
        }
    }
}

#[allow(clippy::type_complexity)]
fn world_events(
    mut play: Player,
    fires: Query<(Entity, &Transform), Added<FireZone>>,
    bursts: Query<(&Burst, &Transform), Added<Burst>>,
    chests: Query<(&Chest, &Transform), Changed<Chest>>,
    me_dead: Query<(), (With<LocalPlayer>, Added<Eliminated>)>,
    local: Query<(Entity, &Inventory), (With<LocalPlayer>, Changed<Inventory>)>,
    zone: Res<Zone>,
    m: Res<Match>,
    mut was: Local<(bool, Option<MatchState>, usize)>,
) {
    for (e, tf) in &fires {
        play.at("fire_ignite", tf.translation, IMPACT, 80.0);
        play.attach_loop("fire_loop", e, IMPACT * 0.8);
    }
    for (b, tf) in &bursts {
        if b.blast {
            play.at("brahmastra_blast", tf.translation, IMPACT * 1.6, 500.0);
            continue;
        }
        play.at("wind_gust", tf.translation, IMPACT, 80.0);
    }
    for (c, tf) in &chests {
        if c.opened {
            play.at(if c.kind == ChestKind::Kalash { "kalash_open" } else { "chest_open" }, tf.translation, ITEM, 40.0);
        }
    }
    if !me_dead.is_empty() {
        play.ui("eliminated", UI * 1.2);
    }
    // Picking things up: the local inventory gaining items.
    for (_, inv) in &local {
        let count = inv.slots.iter().flatten().map(|s| s.count as usize).sum::<usize>() + inv.arrows as usize;
        if count > was.2 {
            play.ui("pickup", ITEM * 0.8);
        }
        was.2 = count;
    }
    // The Pralaya starting to close: a conch.
    if zone.shrinking && !was.0 {
        play.ui("zone_warn", UI * 1.4);
    }
    was.0 = zone.shrinking;
    if m.state == MatchState::Ended && was.1 != Some(MatchState::Ended) && m.winner.is_some_and(|w| local.iter().any(|(e, _)| e == w) || m.alive <= 1) {
        play.ui("victory", UI * 1.2);
    }
    was.1 = Some(m.state);
}

/// Fade the looping beds by where the local warrior is and what it's doing.
fn beds(
    time: Res<Time>,
    me: Query<(&Transform, &Kinematic), With<LocalPlayer>>,
    zone: Res<Zone>,
    mut sinks: Query<(&Bed, &mut AudioSink)>,
) {
    let Ok((tf, kin)) = me.single() else { return };
    let p = tf.translation;
    let (rx0, rx1) = shared::map::india::RIVER_X;
    let river_d = if p.x < rx0 { rx0 - p.x } else if p.x > rx1 { p.x - rx1 } else { 0.0 };
    let wall_d = (p.xz().distance(zone.center) - zone.radius).abs();
    let speed = kin.velocity.length();
    for (bed, mut sink) in &mut sinks {
        let target = match bed {
            Bed::Day => if kin.mode == MoveMode::Vimana { 0.25 } else { 0.8 },
            Bed::River => (1.0 - river_d / 45.0).clamp(0.0, 1.0),
            Bed::Vimana => if kin.mode == MoveMode::Vimana { 0.9 } else { 0.0 },
            Bed::Freefall => if kin.mode == MoveMode::Freefall { 0.5 + 0.5 * (speed / 40.0).min(1.0) } else { 0.0 },
            Bed::Glide => if kin.mode == MoveMode::Gliding { 0.8 } else { 0.0 },
            Bed::Zone => (1.0 - wall_d / 40.0).clamp(0.0, 1.0),
        } * BED;
        let cur = sink.volume().to_linear();
        let k = (time.delta_secs() * 2.5).min(1.0);
        sink.set_volume(Volume::Linear(cur + (target - cur) * k));
    }
}
