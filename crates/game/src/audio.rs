//! Sound: positional one-shots for what happens in the world, looping beds for where the local
//! warrior is (vimana, freefall, glide, zone wall, river, the day around them).
//!
//! Most sounds are synthesized by tools/audio/sfx.py; the rifles are cut from free recordings by
//! tools/audio/rifles.py, the street's and the fights' by tools/audio/street.py (crate::street_sound
//! plays the street's). Events come from the simulation's messages
//! and from state changes noticed here (a swing starting, landing, a chest lid going up), so the
//! sim stays free of presentation. Distance attenuation is rodio's 1/d², scaled by
//! `SPATIAL_SCALE` so full volume reaches out to ~6 m; beyond `cull` metres a sound isn't played.

use std::collections::{HashMap, HashSet};

use avian3d::prelude::*;
use bevy::audio::{GlobalVolume, SpatialScale, Volume};
use bevy::prelude::*;
use shared::items::{ItemDb, ItemKind};
use shared::map::{ChestKind, MapLayout, Mat};
use shared::sim::combat::{Burst, FireZone, KillMsg, Projectile};
use shared::sim::melee::{BrokeMsg, GuardMsg};
use shared::sim::flow::{Match, MatchState};
use shared::sim::loot::Chest;
use shared::sim::zone::Zone;
use shared::sim::{Action, Eliminated, HitMsg, Inventory, Kinematic, LocalPlayer, MapBlock, MapTerrain, MoveMode, Warrior, solid_mask};

use crate::animation::Footstep;
use crate::camera::MainCamera;

pub const SPATIAL_SCALE: f32 = 0.16;
/// The street carries further than a fight: full volume out to about 16 m.
const STREET_SCALE: f32 = 0.06;
/// Metres between the listener's ears.
const EAR_GAP: f32 = 0.3;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Played>()
            .add_plugins(crate::street_sound::StreetSoundPlugin)
            .add_systems(Startup, load)
            .add_systems(Update, (listener, footsteps, warrior_events, combat_events, melee_events, world_events, chimes, beds, log_counts, late_sounds));
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

/// Rifles carry across the map. Past `FAR_FROM` a rifle's far take plays instead of its near one.
const RIFLE_REACH: f32 = 500.0;
const FAR_FROM: f32 = 90.0;
/// Metres a second: a far shot is heard late.
const SOUND_SPEED: f32 = 343.0;

const NAMES: [&str; 75] = [
    "step_dirt_0", "step_dirt_1", "step_dirt_2", "step_dirt_3", "step_stone_0", "step_stone_1", "step_stone_2", "step_stone_3",
    "step_wood_0", "step_wood_1", "step_wood_2", "step_wood_3", "step_grass_0", "step_grass_1", "step_grass_2", "step_grass_3",
    "step_water_0", "step_water_1", "step_water_2", "step_water_3", "sword_whoosh_0", "sword_whoosh_1", "sword_whoosh_2",
    "sword_hit_flesh", "sword_hit_stone", "bow_release", "bow_release_heavy", "bow_draw", "arrow_impact_wood",
    "arrow_impact_stone", "gun_tamancha", "gun_dunali", "gun_katta", "gulel_release", "bullet_impact_stone",
    "bullet_impact_wood", "katta_click", "katta_backfire", "chakra_throw", "brahmastra_blast", "hit_flesh", "hit_shield", "hitmarker", "hitmarker_head", "hurt", "jump", "land", "glider_open",
    "glide_loop", "freefall_loop", "vimana_loop", "chest_open", "kalash_open", "pickup", "drink", "reload", "fire_ignite",
    "fire_loop", "wind_gust", "throw", "zone_warn", "zone_loop", "kill", "eliminated", "victory", "amb_day", "amb_river",
    "gun_thri_not_thri", "gun_thaana_slr", "gun_saintalis", "gun_rifle_far", "gun_auto_far", "bullet_crack", "amb_wind", "metro_chime",
];

/// The fights' sounds, cut from recordings.
pub const MELEE_NAMES: [&str; 15] = [
    "swing_light_0", "swing_light_1", "swing_stick", "swing_heavy", "hit_bat", "hit_stick", "hit_pan", "hit_dhal", "hit_blade", "hit_gada",
    "guard_wood", "guard_metal", "guard_pan", "break_wood", "break_metal",
];

/// The whoosh of a swing of `item`, and its pitch.
fn swing_sound(item: &str) -> (&'static str, f32) {
    match item {
        "gada" => ("swing_heavy", 1.0),
        "bat" => ("swing_stick", 0.9),
        "lathi" => ("swing_stick", 1.1),
        "hockey_stick" => ("swing_light_1", 1.0),
        "talwar" => ("swing_light_1", 1.25),
        _ => ("swing_light_0", 1.0),
    }
}

fn blow_sound(item: &str) -> &'static str {
    match item {
        "lathi" | "hockey_stick" => "hit_stick",
        "tawa" => "hit_pan",
        "dhal" => "hit_dhal",
        "talwar" => "hit_blade",
        "gada" => "hit_gada",
        _ => "hit_bat",
    }
}

fn guard_sound(item: &str) -> &'static str {
    match item {
        "tawa" => "guard_pan",
        "dhal" | "talwar" => "guard_metal",
        _ => "guard_wood",
    }
}

fn break_sound(item: &str) -> &'static str {
    if item == "gada" { "break_wood" } else { "break_metal" }
}

#[derive(Resource)]
struct Sfx(HashMap<&'static str, Handle<AudioSource>>);

/// Looping beds, faded by `beds` each frame.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
enum Bed {
    /// The park's birds and breeze.
    Day,
    Traffic,
    Market,
    /// The city far off.
    Far,
    River,
    /// Wind over the open land and the roofs.
    Wind,
    Vimana,
    Freefall,
    Glide,
    Zone,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    let names = NAMES.iter().chain(MELEE_NAMES.iter()).chain(crate::street_sound::NAMES.iter());
    let sfx = Sfx(names.map(|n| (*n, assets.load(format!("audio/{n}.ogg")))).collect());
    for (bed, name) in [(Bed::Day, "amb_day"), (Bed::Traffic, "amb_traffic"), (Bed::Market, "amb_market"), (Bed::Far, "amb_far"), (Bed::River, "amb_river"), (Bed::Wind, "amb_wind"), (Bed::Vimana, "vimana_loop"), (Bed::Freefall, "freefall_loop"),
                        (Bed::Glide, "glide_loop"), (Bed::Zone, "zone_loop")] {
        commands.spawn((bed, AudioPlayer(sfx.0[name].clone()), PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0))));
    }
    commands.insert_resource(sfx);
}

/// How many times each sound was triggered (VIVAD_AUDIO_LOG prints it every 30 s).
#[derive(Resource, Default)]
struct Played(HashMap<String, u32>);

fn log_counts(time: Res<Time>, played: Res<Played>, street: Res<crate::street_sound::Ambience>, mut next: Local<f32>) {
    if std::env::var("VIVAD_AUDIO_LOG").is_err() || time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 30.0;
    let mut v: Vec<_> = played.0.iter().collect();
    v.sort();
    info!("sounds played: {v:?}");
    info!("street beds: traffic {:.2}, market {:.2}, far {:.2}, park {:.2}, wind {:.2}, river {:.2}, indoors {}", street.traffic, street.market, street.far, street.park, street.wind, street.river, street.indoors);
}

/// A rifle's far take, and at what pitch.
fn far_take(item: &str) -> Option<(&'static str, f32)> {
    match item {
        "thri_not_thri" => Some(("gun_rifle_far", 1.0)),
        "thaana_slr" => Some(("gun_rifle_far", 1.12)),
        "toradar" => Some(("gun_rifle_far", 0.75)),
        "saintalis" | "trishul_carbine" => Some(("gun_auto_far", 1.0)),
        _ => None,
    }
}

/// A far shot on its way through the air.
#[derive(Component)]
struct LateSound {
    wait: f32,
    sound: Handle<AudioSource>,
    volume: f32,
    speed: f32,
}

fn late_sounds(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut LateSound)>) {
    for (e, mut s) in &mut q {
        s.wait -= time.delta_secs();
        if s.wait <= 0.0 {
            commands.entity(e).remove::<LateSound>().insert((
                AudioPlayer(s.sound.clone()),
                PlaybackSettings::DESPAWN
                    .with_spatial(true)
                    .with_spatial_scale(SpatialScale::new(SPATIAL_SCALE))
                    .with_volume(Volume::Linear(s.volume))
                    .with_speed(s.speed),
            ));
        }
    }
}

/// rodio 0.22 gives the nearer ear half the volume (its issue 884), so a negative gap puts each
/// ear on the far side; the tests below fail the day rodio fixes it. Every spatial sound goes
/// through this one listener, near or far.
fn ears() -> SpatialListener {
    SpatialListener::new(-EAR_GAP)
}

fn listener(mut commands: Commands, cam: Query<Entity, (With<MainCamera>, Without<SpatialListener>)>) {
    for e in &cam {
        commands.entity(e).insert(ears());
    }
}

/// Plays sounds; positional ones beyond `cull` metres from the listener are skipped.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Player<'w, 's> {
    commands: Commands<'w, 's>,
    sfx: Res<'w, Sfx>,
    played: ResMut<'w, Played>,
    ear: Query<'w, 's, &'static GlobalTransform, With<SpatialListener>>,
}

impl Player<'_, '_> {
    fn at(&mut self, name: &str, pos: Vec3, volume: f32, cull: f32) {
        self.at_pitched(name, pos, volume, cull, 1.0);
    }

    /// The same sample at another pitch: one gunshot recording serves a second gun.
    fn at_pitched(&mut self, name: &str, pos: Vec3, volume: f32, cull: f32, pitch: f32) {
        let ear = self.ear.iter().next().map(|t| t.translation()).unwrap_or(pos);
        if ear.distance(pos) > cull {
            return;
        }
        let Some(h) = self.sfx.0.get(name) else { return };
        *self.played.0.entry(name.to_owned()).or_default() += 1;
        // A touch of pitch variety so repeats don't sound mechanical.
        let speed = pitch * (1.0 + (pos.x * 12.9898 + pos.z * 78.233).sin().fract() * 0.08);
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

    fn distance(&self, pos: Vec3) -> f32 {
        self.ear.iter().next().map_or(0.0, |t| t.translation().distance(pos))
    }

    /// A shot from far off: distance falloff would lose the near take, so the far take plays close
    /// by the ear from the shot's direction, quieter with distance and late by the speed of sound.
    fn far(&mut self, name: &str, pos: Vec3, volume: f32, reach: f32, pitch: f32) {
        let ear = self.ear.iter().next().map(|t| t.translation()).unwrap_or(pos);
        let d = ear.distance(pos);
        let Some(h) = self.sfx.0.get(name) else { return };
        if d > reach {
            return;
        }
        *self.played.0.entry(name.to_owned()).or_default() += 1;
        self.commands.spawn((
            LateSound { wait: d / SOUND_SPEED, sound: h.clone(), volume: volume * (1.0 - d / (reach * 1.15)), speed: pitch },
            Transform::from_translation(ear + (pos - ear).normalize_or(Vec3::NEG_Z) * 5.0),
        ));
    }

    /// A sound of the street round the listener, at a given pitch.
    pub(crate) fn street(&mut self, name: &str, pos: Vec3, volume: f32, speed: f32) {
        let Some(h) = self.sfx.0.get(name) else { return };
        *self.played.0.entry(name.to_owned()).or_default() += 1;
        self.commands.spawn((
            AudioPlayer(h.clone()),
            PlaybackSettings::DESPAWN
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(STREET_SCALE))
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
        if layout.river.distance(p) == 0.0 && pos.y < -0.3 && pos.y > -5.0 {
            return "water";
        }
        if let Some(class) = layout.terrain.ground_at(p.x, p.y) {
            use shared::map::terrain::ground;
            return match class {
                ground::PARK | ground::OPEN => "grass",
                ground::SAND => "sand",
                // The NCR's rivers are in its ground, not in `layout.river`.
                ground::WATER => "water",
                _ => "stone",
            };
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
        if kind == "sand" {
            // The dirt step, lower and softer: one recording serves both.
            play.at_pitched(&format!("step_dirt_{}", *n % 4), s.pos, v * 0.8, 35.0, 0.8);
        } else {
            play.at(&format!("step_{kind}_{}", *n % 4), s.pos, v, 35.0);
        }
    }
}

/// Per-warrior state from the previous frame, to notice transitions.
#[derive(Component, Default)]
struct Heard {
    grounded: bool,
    mode: Option<MoveMode>,
    vy: f32,
    since_attack: f32,
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
        // Every peer sees this time drop when a swing starts, whoever's warrior it is.
        if let Some((id, ItemKind::Melee { .. })) = &held {
            if act.since_attack < h.since_attack {
                let (name, pitch) = swing_sound(id);
                *n += 1;
                let name = if name == "swing_light_0" && *n % 2 == 0 { "swing_light_1" } else { name };
                play.at_pitched(name, p, WEAPON, 60.0, pitch);
            }
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
        *h = Heard { grounded: kin.grounded, mode: Some(kin.mode), vy: kin.velocity.y, since_attack: act.since_attack, reloading, using, charging };
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
    mut cracked: Local<HashSet<Entity>>,
    time: Res<Time>,
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
                // (sample, reach, pitch, volume): the Ladi-9 is the tamancha's shot pitched up
                // and clipped short.
                let (sound, reach, pitch, vol) = match p.item.as_str() {
                    "tamancha" => ("gun_tamancha", 200.0, 1.0, 1.0),
                    "dunali" => ("gun_dunali", 220.0, 1.0, 1.0),
                    "katta" => ("gun_katta", 220.0, 1.0, 1.0),
                    "gulel" => ("gulel_release", 60.0, 1.0, 1.0),
                    "vajra9" => ("gun_tamancha", 180.0, 1.45, 0.75),
                    "trishul_carbine" => ("gun_saintalis", RIFLE_REACH, 1.15, 0.9),
                    "saintalis" => ("gun_saintalis", RIFLE_REACH, 1.0, 1.0),
                    "thaana_slr" => ("gun_thaana_slr", RIFLE_REACH, 1.0, 1.0),
                    "thri_not_thri" => ("gun_thri_not_thri", RIFLE_REACH, 1.0, 1.1),
                    // Black powder: the dunali's boom, deeper.
                    "toradar" => ("gun_dunali", RIFLE_REACH, 0.72, 1.2),
                    _ if heavy => ("bow_release_heavy", 120.0, 1.0, 1.0),
                    _ => ("bow_release", 120.0, 1.0, 1.0),
                };
                match far_take(&p.item) {
                    Some((far, tone)) if play.distance(tf.translation) > FAR_FROM => play.far(far, tf.translation, WEAPON * vol, reach, tone),
                    _ => play.at_pitched(sound, tf.translation, WEAPON * vol, reach, pitch),
                }
            }
            Some(_) if p.item == "chakra" => play.at("chakra_throw", tf.translation, WEAPON, 80.0),
            Some(_) => play.at("throw", tf.translation, WEAPON, 60.0),
        }
    }
    // Arrows that just stuck into the world.
    let live: HashSet<Entity> = arrows.iter().map(|(e, ..)| e).collect();
    stuck.retain(|e| live.contains(e));
    cracked.retain(|e| live.contains(e));
    // A rifle bullet going past the ear.
    for (e, p, tf) in &arrows {
        let step = p.velocity * time.delta_secs();
        if p.stuck || Some(p.owner) == me || cracked.contains(&e) || p.velocity.length() < 200.0 {
            continue;
        }
        let from = tf.translation - step;
        let along = |at: f32| from + step * at;
        let nearest = (0..=4).map(|k| along(k as f32 / 4.0)).min_by(|a, b| play.distance(*a).total_cmp(&play.distance(*b)));
        if let Some(at) = nearest.filter(|at| play.distance(*at) < 5.0) {
            cracked.insert(e);
            play.at("bullet_crack", at, IMPACT, 8.0);
        }
    }
    for (e, p, tf) in &arrows {
        if p.stuck && stuck.insert(e) {
            let soft = matches!(surface(&spatial, &blocks, &terrain, &layout, tf.translation), "wood" | "grass" | "dirt" | "sand");
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
        let melee = inv.get(h.attacker).ok().and_then(|i| i.current()).filter(|s| matches!(db.get(&s.id).kind, ItemKind::Melee { .. }));
        play.at(melee.map_or(if h.shield { "hit_shield" } else { "hit_flesh" }, |s| blow_sound(&s.id)), h.at, IMPACT, 60.0);
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

/// A Metro-style announcement's two notes.
fn chimes(mut play: Player, mut chimes: MessageReader<crate::say::Chime>) {
    if chimes.read().count() > 0 {
        play.ui("metro_chime", UI * 1.3);
    }
}

/// A guard taking a blow, and an old weapon giving out.
fn melee_events(mut play: Player, mut guards: MessageReader<GuardMsg>, mut broke: MessageReader<BrokeMsg>, inv: Query<&Inventory>) {
    for g in guards.read() {
        let held = inv.get(g.target).ok().and_then(|i| i.current()).map_or("bat", |s| s.id.as_str());
        play.at(guard_sound(held), g.at, IMPACT, 60.0);
    }
    for b in broke.read() {
        play.at(break_sound(&b.item), b.at, IMPACT, 60.0);
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
    layout: Res<MapLayout>,
    street: Res<crate::street_sound::Ambience>,
    global: Res<GlobalVolume>,
    mut sinks: Query<(&Bed, &mut AudioSink)>,
) {
    // At the menu nobody stands anywhere: only the city far off, or the last match's street would
    // play on over Home.
    let here = me.single().ok();
    for (bed, mut sink) in &mut sinks {
        let target = match (bed, here) {
            (Bed::Day, _) => street.park * 0.8,
            (Bed::Traffic, _) => street.traffic,
            (Bed::Market, _) => street.market,
            (Bed::Far, _) => street.far,
            (Bed::Wind, _) => street.wind,
            (_, None) => 0.0,
            (Bed::River, Some((tf, _))) => (1.0 - layout.river.distance(tf.translation.xz()) / 45.0).clamp(0.0, 1.0).max(street.river),
            (Bed::Vimana, Some((_, kin))) => if kin.mode == MoveMode::Vimana { 0.9 } else { 0.0 },
            (Bed::Freefall, Some((_, kin))) => if kin.mode == MoveMode::Freefall { 0.5 + 0.5 * (kin.velocity.length() / 40.0).min(1.0) } else { 0.0 },
            (Bed::Glide, Some((_, kin))) => if kin.mode == MoveMode::Gliding { 0.8 } else { 0.0 },
            (Bed::Zone, Some((tf, _))) => (1.0 - (tf.translation.xz().distance(zone.center) - zone.radius).abs() / 40.0).clamp(0.0, 1.0),
        } * BED * global.volume.to_linear();
        let cur = sink.volume().to_linear();
        let k = (time.delta_secs() * 2.5).min(1.0);
        let v = cur + (target - cur) * k;
        sink.set_volume(Volume::Linear(v));
        // A silent bed is paused, not mixed (in the browser, mixing runs on the game's one thread).
        if v < 0.002 && target < 0.002 {
            if !sink.is_paused() {
                sink.pause();
            }
        } else if sink.is_paused() {
            sink.play();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use rodio::SpatialPlayer;
    use rodio::buffer::SamplesBuffer;
    use rodio::mixer::mixer;

    use super::{SPATIAL_SCALE, ears};

    /// The level each ear gets from a steady tone `x` metres to the listener's right, through the
    /// rodio player Bevy plays spatial sounds with (a mixer read by hand, so no audio device).
    fn heard(x: f32) -> (f32, f32) {
        let rate = NonZero::new(44_100).unwrap();
        let ears = ears();
        let (mix, mut out) = mixer(NonZero::new(2).unwrap(), rate);
        let player = SpatialPlayer::connect_new(&mix, [x, 0.0, -0.5], ears.left_ear_offset.to_array(), ears.right_ear_offset.to_array());
        player.append(SamplesBuffer::new(NonZero::new(1).unwrap(), rate, vec![0.5; 44_100]));
        let (mut left, mut right) = (0.0, 0.0);
        for _ in 0..8_000 {
            left += out.next().unwrap_or(0.0).abs();
            right += out.next().unwrap_or(0.0).abs();
        }
        (left, right)
    }

    #[test]
    fn a_sound_on_the_left_is_louder_in_the_left_ear() {
        let (left, right) = heard(-0.8);
        assert!(left > 0.0 && right > 0.0, "silent: {left} {right}");
        assert!(left > right * 1.2, "sound on the left: left ear {left}, right ear {right}");
    }

    #[test]
    fn a_sound_on_the_right_is_louder_in_the_right_ear() {
        let (left, right) = heard(0.8);
        assert!(right > left * 1.2, "sound on the right: left ear {left}, right ear {right}");
    }

    /// Rifles use the same listener: the far take plays 5 m from the ear, a near shot at its spot.
    #[test]
    fn a_rifle_on_the_left_stays_on_the_left_near_and_far() {
        for metres in [5.0, 60.0, 90.0] {
            let (left, right) = heard(-metres * SPATIAL_SCALE);
            assert!(left > right * 1.2, "a shot {metres} m to the left: left ear {left}, right ear {right}");
        }
    }
}
