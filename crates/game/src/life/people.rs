//! Harmless people and street animals: drawn only near the player, about 16 people and 8 to 12
//! animals at most, none with a weapon. Each is one entity showing one mesh from a flip book
//! (`flipbook`), so a busy lane costs a few draws and no bones.
//!
//! They keep about 6 m from the player and drift away as he nears, so anyone walking straight at
//! him is a warrior. A gunshot, a blast or the Aandhi coming near sends them running for a side
//! lane and out of sight, and they don't come back to that spot for a while.

use std::f32::consts::{PI, TAU};

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, VisibilityRange};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use shared::map::MapLayout;
use shared::map::terrain::ground;
use shared::sim::combat::{Burst, Projectile};
use shared::sim::nav::StreetFloor;
use shared::sim::zone::Zone;
use shared::sim::{Eliminated, LocalPlayer, OfMap, Warrior};

use super::flipbook::{self, Baked, Book, Look, Shot, Spec, Take};
use super::{Dice, Market, Scattered, Stall, StallKind, busy_at};
use crate::batch::probe::Tagged;
use crate::camera::MainCamera;

pub struct PeoplePlugin;

impl Plugin for PeoplePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Books>()
            .init_resource::<Alarms>()
            .init_resource::<Scares>()
            .add_systems(Update, (finish_baking, shadows_near, (hear, listen_ring, populate, walk).chain()));
    }
}

/// The most people near the player, and how far off they stop being drawn.
const PEOPLE: usize = 16;
const ANIMALS: usize = 9;
/// How big the animals are: a street dog about 52 cm at the shoulder, a cow about 1.3 m.
const DOG_SCALE: f32 = 0.27;
const COW_SCALE: f32 = 0.32;
const DRAWN: f32 = 72.0;
const FADE: f32 = 8.0;
/// Past this they are gone altogether, and new ones are found no further out.
const GONE: f32 = DRAWN + FADE + 4.0;
/// A shopper or a stroller this far off and out of view gives way to one nearer.
const STRAYED: f32 = 56.0;
/// How far out shoppers are looked for: sixteen people go further the closer they stay.
const CROWD: f32 = 58.0;
/// How near the centre of the view someone may turn up: past this, in a busy street, nobody sees.
const SEEN_FROM: f32 = 40.0;
/// When the bakes start.
const BAKE_AFTER: f32 = 9.0;
/// How far off someone still casts a shadow: a blob of a figure past this won't be missed.
const SHADOW_TO: f32 = 18.0;
/// The most people moved round to the front of the view each look.
const MOVED_UP: usize = 2;
/// How close the player gets before someone starts to move off.
const KEEP_OFF: f32 = 7.0;
/// Above the ground, the camera is too far up for any of this.
const TOO_HIGH: f32 = 70.0;
/// How far people hear a shot and a blast.
const SHOT_HEARD: f32 = 62.0;
const BLAST_HEARD: f32 = 130.0;
/// How long a spot stays empty after a scare, and how wide.
const SCARED_FOR: f32 = 100.0;
const SCARED_WIDE: f32 = 58.0;
/// How near the ring may come before people leave.
const RING_WARNING: f32 = 30.0;

const PEOPLE_SHOTS: [Shot; 5] = [
    Shot { key: "idle", names: &["idle"], take: Take::Cycle(6), sit: false },
    Shot { key: "walk", names: &["walk_slow", "stroll", "walk", "walk_f"], take: Take::Cycle(12), sit: false },
    Shot { key: "run", names: &["run_f", "jog_f", "run", "sprint_f"], take: Take::Cycle(8), sit: false },
    Shot { key: "sip", names: &["consume", "idle"], take: Take::Cycle(6), sit: true },
    Shot { key: "sit", names: &["idle"], take: Take::Still(0.0), sit: true },
];

const DOG_SHOTS: [Shot; 6] = [
    Shot { key: "idle", names: &["Idle"], take: Take::Cycle(8), sit: false },
    Shot { key: "walk", names: &["Walk"], take: Take::Cycle(10), sit: false },
    Shot { key: "run", names: &["Gallop"], take: Take::Cycle(8), sit: false },
    Shot { key: "eat", names: &["Eating"], take: Take::Cycle(8), sit: false },
    Shot { key: "low", names: &["Idle_2_HeadLow", "Idle_Headlow"], take: Take::Cycle(8), sit: false },
    Shot { key: "sleep", names: &["Death"], take: Take::Still(1.0), sit: false },
];

const DARK: [u8; 3] = [46, 46, 52];
const CREAM: [u8; 3] = [196, 186, 164];
const WHITE: [u8; 3] = [206, 204, 198];
const KHAKI: [u8; 3] = [132, 118, 88];
const SHOE: [u8; 3] = [52, 42, 36];

/// Everyday clothes, muted: a warrior's colours should be what stands out in a lane.
fn men() -> Vec<Look> {
    let dress = |upper: [u8; 3], lower: [u8; 3], skin: f32, hair: f32| Look { upper: Some(upper), lower: Some(lower), shin: Some(lower), feet: Some(SHOE), skin, hair };
    vec![
        dress(WHITE, DARK, 1.0, 1.0),
        dress([128, 146, 168], [58, 58, 66], 0.82, 1.0),
        dress([120, 112, 104], KHAKI, 0.9, 1.0),
        dress(CREAM, CREAM, 0.75, 1.0),
        dress([92, 104, 88], [70, 62, 54], 0.85, 1.0),
        dress(WHITE, WHITE, 0.9, 3.0),
    ]
}

fn women() -> Vec<Look> {
    let dress = |kurta: [u8; 3], salwar: [u8; 3], skin: f32| Look { upper: Some(kurta), lower: Some(kurta), shin: Some(salwar), feet: Some(SHOE), skin, hair: 1.0 };
    vec![
        dress([150, 96, 80], CREAM, 1.0),
        dress([92, 112, 104], [84, 80, 88], 0.85),
        dress([168, 138, 84], WHITE, 0.9),
        dress([112, 82, 104], [196, 184, 176], 0.8),
        dress([128, 62, 60], [64, 56, 58], 0.95),
    ]
}

/// Everything baked, once.
#[derive(Resource, Default)]
struct Books {
    men: Option<Book>,
    women: Option<Book>,
    dog: Option<Book>,
    cow: Option<Book>,
    /// What is being baked, one at a time, and what's still to come.
    job: Option<(&'static str, Task<Result<Baked, String>>)>,
    next: usize,
    /// When to start: the map's first seconds are the game's memory peak.
    wait: f32,
}

impl Books {
    fn of(&self, role: Role, woman: bool) -> Option<&Book> {
        match role {
            Role::Dog => self.dog.as_ref(),
            Role::Cow => self.cow.as_ref(),
            // Either will do if the other didn't bake.
            _ if woman => self.women.as_ref().or(self.men.as_ref()),
            _ => self.men.as_ref().or(self.women.as_ref()),
        }
    }

    fn ready(&self) -> bool {
        (self.men.is_some() && self.women.is_some()) || (self.next >= specs().len() && self.job.is_none() && (self.men.is_some() || self.women.is_some()))
    }
}

/// What to bake, in order.
fn specs() -> Vec<(&'static str, Spec)> {
    let person = |variant: &str, looks| Spec {
        model: parts_file(variant),
        clips: Some(format!("models/characters/{variant}_anims.glb")),
        shots: PEOPLE_SHOTS.to_vec(),
        atlas: 512,
        looks,
        coats: vec![],
        scale: 1.0,
    };
    let animal = |name: &str, coats: usize, scale: f32| Spec {
        model: format!("models/life/{name}.glb"),
        clips: None,
        shots: DOG_SHOTS.to_vec(),
        atlas: 64,
        looks: vec![],
        coats: (0..coats).map(|k| format!("models/life/{name}_coat_{k}.png")).collect(),
        scale,
    };
    vec![("men", person("male", men())), ("women", person("female", women())), ("dog", animal("dog", 4, DOG_SCALE)), ("cow", animal("cow", 3, COW_SCALE))]
}

/// A character's file: the split one, or the old one if that isn't there.
fn parts_file(variant: &str) -> String {
    let parts = format!("models/characters/{variant}_parts.glb");
    if std::path::Path::new(crate::ASSET_DIR).join(&parts).exists() { parts } else { format!("models/characters/{variant}.glb") }
}

/// Starts each bake in turn, and takes in what's done.
fn finish_baking(mut books: ResMut<Books>, time: Res<Time>, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if cfg!(target_arch = "wasm32") {
        return;
    }
    if let Some((name, task)) = &mut books.job {
        let name = *name;
        let Some(done) = block_on(poll_once(task)) else { return };
        books.job = None;
        match done {
            Ok(baked) => {
                let book = baked.install(&mut meshes, &mut images, &mut materials);
                info!("life: {name} baked: {} looks, clips {:?}", book.looks.len(), book.clips.iter().map(|c| (c.key, c.frames.len(), c.cycle)).collect::<Vec<_>>());
                match name {
                    "men" => books.men = Some(book),
                    "women" => books.women = Some(book),
                    "dog" => books.dog = Some(book),
                    "cow" => books.cow = Some(book),
                    _ => {}
                }
            }
            Err(e) => warn!("life: {name} didn't bake: {e}"),
        }
    }
    books.wait += time.delta_secs();
    // Well after the map has gone to the GPU: a bake's texture reading then adds to no peak.
    if books.wait < BAKE_AFTER {
        return;
    }
    let all = specs();
    if let Some((name, spec)) = all.into_iter().nth(books.next) {
        books.next += 1;
        books.job = Some((name, AsyncComputeTaskPool::get().spawn(async move { flipbook::bake(&spec) })));
    }
}

/// What's going on at a walker this moment.
#[derive(Clone, Copy, Debug)]
enum Doing {
    /// Stays put facing a way, in the clip named, until a time.
    Stay { until: f32, yaw: f32 },
    /// Going somewhere, then stopping there for a while.
    Go { to: Vec2, speed: f32, then: f32, yaw: Option<f32> },
    /// Moving off from the player.
    Shy { from: Vec2 },
    /// Running for it, after a moment's start.
    Flee { to: Vec2, start: f32, began: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Keeper,
    Sitter,
    /// Sits at a spread, with nothing to drink.
    Seller,
    Shopper,
    Dog,
    Cow,
}

impl Role {
    fn animal(self) -> bool {
        matches!(self, Role::Dog | Role::Cow)
    }

    /// Metres a second: strolling, and running for it.
    fn paces(self) -> (f32, f32) {
        match self {
            Role::Dog => (1.1, 7.5),
            Role::Cow => (0.85, 4.8),
            _ => (1.5, 5.2),
        }
    }

    /// The speed each clip was made for, so feet stay about on the ground.
    fn beats(self) -> (f32, f32) {
        match self {
            Role::Dog => (1.2, 7.0),
            Role::Cow => (1.0, 5.0),
            _ => (2.2, 5.2),
        }
    }
}

/// One person near the player.
#[derive(Component)]
struct Walker {
    role: Role,
    woman: bool,
    /// Which stall it belongs to, if any.
    stall: Option<u32>,
    /// Where it goes back to.
    home: Vec2,
    home_yaw: f32,
    pos: Vec2,
    yaw: f32,
    doing: Doing,
    clip: usize,
    phase: f32,
    frame: usize,
    /// Seconds a cycle takes of this clip here, by its pace.
    dice: Dice,
    /// How long it's been out of the player's sight while fleeing.
    hidden: f32,
    /// The clip it stands about in.
    resting: &'static str,
}

/// Loud things heard this frame, and how far each carries.
#[derive(Resource, Default)]
struct Alarms(Vec<(Vec3, f32)>);

/// Spots that have been scared lately.
#[derive(Resource, Default)]
struct Scares(Vec<(Vec2, f32)>);

impl Scares {
    fn scared(&self, p: Vec2, now: f32) -> bool {
        self.0.iter().any(|(at, until)| *until > now && at.distance_squared(p) < SCARED_WIDE * SCARED_WIDE)
    }

    fn add(&mut self, at: Vec2, until: f32) {
        // A burst of fire is one scare: it just goes on longer.
        if let Some(s) = self.0.iter_mut().find(|(q, _)| q.distance_squared(at) < 100.0) {
            s.1 = until;
            return;
        }
        self.0.retain(|s| s.1 > until - SCARED_FOR * 2.0);
        self.0.push((at, until));
        if self.0.len() > 32 {
            self.0.remove(0);
        }
    }
}

/// The hook on combat, the one place here that knows its names: every shot and blast on the map
/// as it happens. Combat and sound are being merged and may rename these.
fn hear(mut alarms: ResMut<Alarms>, shots: Query<&Transform, Added<Projectile>>, blasts: Query<&Transform, Added<Burst>>) {
    alarms.0.clear();
    alarms.0.extend(shots.iter().map(|t| (t.translation, SHOT_HEARD)));
    alarms.0.extend(blasts.iter().map(|t| (t.translation, BLAST_HEARD)));
}

/// The Aandhi coming near counts as a noise at the edge it comes from.
fn listen_ring(zone: Res<Zone>, walkers: Query<&Walker>, mut alarms: ResMut<Alarms>, time: Res<Time>, mut every: Local<f32>) {
    *every -= time.delta_secs();
    if *every > 0.0 || walkers.is_empty() {
        return;
    }
    *every = 0.5;
    for w in &walkers {
        let d = w.pos.distance(zone.center);
        if zone.radius - d < RING_WARNING {
            let edge = zone.center + (w.pos - zone.center).normalize_or(Vec2::X) * zone.radius;
            alarms.0.push((Vec3::new(edge.x, 0.0, edge.y), RING_WARNING * 2.0));
            return;
        }
    }
}

/// Whether a person may walk here: the row stalls have no collider, but nobody walks through one.
fn walkable(layout: &MapLayout, floor: &StreetFloor, market: &Market, p: Vec2) -> bool {
    floor.walkable(p) && !market.stall_at(p) && !matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::WATER)) && !layout.is_holy(p)
}

/// Whether the straight walk from `a` to `b` is clear.
fn line_clear(layout: &MapLayout, floor: &StreetFloor, market: &Market, a: Vec2, b: Vec2) -> bool {
    let steps = a.distance(b).ceil() as usize;
    floor.line_clear(a, b) && (0..=steps).all(|s| !market.stall_at(a.lerp(b, s as f32 / steps.max(1) as f32))) && walkable(layout, floor, market, b)
}

fn paved(layout: &MapLayout, p: Vec2) -> bool {
    matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::PAVEMENT | ground::PATH | ground::PLAZA))
}

/// Animals keep to the road ground, but not the open land off it.
fn paved_only(layout: &MapLayout, p: Vec2) -> bool {
    !matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::ROAD | ground::PATH | ground::PAVEMENT | ground::PLAZA | ground::PARKING))
}

fn local(yaw: f32, at: Vec3) -> Vec3 {
    Quat::from_rotation_y(yaw) * at
}

/// The yaw that faces along `d`.
fn facing(d: Vec2) -> f32 {
    (-d.x).atan2(-d.y)
}

/// Only those near the camera cast a shadow.
fn shadows_near(mut commands: Commands, time: Res<Time>, mut every: Local<f32>, camera: Query<&GlobalTransform, With<MainCamera>>, q: Query<(Entity, &Walker, Has<NotShadowCaster>)>) {
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.4;
    let Ok(camera) = camera.single() else { return };
    let at = camera.translation().xz();
    for (e, w, none) in &q {
        let far = w.pos.distance(at) > SHADOW_TO;
        if far && !none {
            commands.entity(e).insert(NotShadowCaster);
        } else if !far && none {
            commands.entity(e).remove::<NotShadowCaster>();
        }
    }
}

/// Keeps the street near the player peopled.
#[allow(clippy::too_many_arguments)]
fn populate(
    mut commands: Commands,
    time: Res<Time>,
    mut every: Local<f32>,
    mut seed: Local<u32>,
    mut landed: Local<Option<Vec2>>,
    books: Res<Books>,
    market: Res<Market>,
    layout: Res<MapLayout>,
    floor: Res<StreetFloor>,
    zone: Res<Zone>,
    scares: Res<Scares>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    walkers: Query<(Entity, &Walker)>,
) {
    let _spent = super::spent::Timed::start(&super::spent::PEOPLE);
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.5;
    let Ok(camera) = camera.single() else { return };
    let eye = camera.translation();
    let at = eye.xz();
    let aloft = eye.y - layout.terrain.height(at.x, at.y) > TOO_HIGH;
    if aloft || !floor.ready() || !books.ready() {
        for (e, _) in &walkers {
            commands.entity(e).despawn();
        }
        *landed = None;
        return;
    }
    let mut have = 0;
    let mut stalls_taken: Vec<u32> = vec![];
    let forward = camera.forward().xz().normalize_or_zero();
    for (e, w) in &walkers {
        let d = w.pos.distance(at);
        let behind = (w.pos - at).normalize_or_zero().dot(forward) < 0.0;
        if d > GONE || (d > STRAYED && behind && matches!(w.role, Role::Shopper | Role::Keeper | Role::Seller | Role::Sitter)) {
            commands.entity(e).despawn();
            continue;
        }
        have += !w.role.animal() as usize;
        stalls_taken.extend(w.stall);
    }
    // How many this street has room for: the average of where the player is and round about him.
    let spots = [Vec2::ZERO, Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y].map(|d| busy_at(&market, at + d * 36.0));
    let busy = spots.iter().sum::<f32>() / spots.len() as f32;
    let want = (busy * PEOPLE as f32 * 1.25).round().min(PEOPLE as f32) as usize;
    if *seed == 0 {
        *seed = 0x51ed_270b;
    }
    let mut dice = Dice(*seed ^ time.elapsed().as_millis() as u32 | 1);
    let now = time.elapsed_secs();
    // A first fill may stand anywhere: nobody has seen this street yet. After that, people come
    // into a street from out of the player's view.
    let first = landed.is_none_or(|p| p.distance(at) > 30.0);
    *landed = Some(at);
    let ahead = |p: Vec2| (p - at).normalize_or_zero().dot(forward);
    let hidden = |p: Vec2| {
        let d = p.distance(at);
        d >= KEEP_OFF + 1.0 && d < DRAWN + 2.0 && (first || d > SEEN_FROM || ahead(p) < 0.1)
    };
    let inside = |p: Vec2| zone.radius - p.distance(zone.center) > RING_WARNING + 10.0;
    let free = |p: Vec2| hidden(p) && inside(p) && !scares.scared(p, now) && walkable(&layout, &floor, &market, p);
    populate_animals(&mut commands, &books, &layout, busy, at, &walkers, &mut dice, now, &free);
    let mut room = want.saturating_sub(have);
    // Sixteen people are a crowd only if they are where the player is looking: shoppers behind
    // him go, and turn up ahead.
    let in_view = walkers.iter().filter(|(_, w)| !w.role.animal() && w.pos.distance(at) < CROWD && ahead(w.pos) > 0.55).count();
    if in_view < want * 3 / 4 {
        let behind = walkers.iter().filter(|(_, w)| w.role == Role::Shopper && !matches!(w.doing, Doing::Flee { .. }) && ahead(w.pos) < -0.15 && w.pos.distance(at) > 10.0);
        for (e, _) in behind.take(MOVED_UP) {
            commands.entity(e).despawn();
            room += 1;
        }
    }
    if room == 0 {
        *seed = dice.0;
        return;
    }
    // Stalls first, nearest first, a keeper each and up to two at a chai stall.
    let mut near: Vec<u32> = market.stalls_near(at, STRAYED).filter(|id| !stalls_taken.contains(id)).collect();
    // The nearest first, and those in view before those behind.
    let rank = |id: &u32| {
        let p = market.stalls[*id as usize].pos.xz();
        p.distance(at) * if ahead(p) > 0.4 { 0.55 } else { 1.3 }
    };
    near.sort_by(|a, b| rank(a).total_cmp(&rank(b)));
    let keepers = (want * 3 / 10).max(1);
    let mut kept = walkers.iter().filter(|(_, w)| !w.role.animal() && w.role != Role::Shopper).count();
    for id in near {
        if room == 0 || kept >= keepers {
            break;
        }
        let stall = market.stalls[id as usize];
        let placed = stall_people(&stall, &mut dice);
        let mut any = false;
        for (role, spot, yaw) in placed.into_iter().take(room) {
            if !free(spot) {
                continue;
            }
            spawn_walker(&mut commands, &books, &mut dice, role, Some(id), true, spot, yaw, layout.terrain.height(spot.x, spot.y), now);
            room -= 1;
            kept += 1;
            any = true;
        }
        let _ = any;
    }
    // Shoppers at the stalls, in ones and twos: that is what makes a street look like a market.
    let mut spots: Vec<u32> = market.stalls_near(at, CROWD).filter(|id| matches!(market.stalls[*id as usize].kind, StallKind::Thela | StallKind::Patri | StallKind::Chai | StallKind::Row | StallKind::Cart)).collect();
    for k in (1..spots.len()).rev() {
        spots.swap(k, (dice.next() * (k + 1) as f32) as usize);
    }
    spots.sort_by_key(|id| ahead(market.stalls[*id as usize].pos.xz()) < 0.4);
    let strollers = room / 3;
    for id in spots {
        if room <= strollers {
            break;
        }
        let stall = market.stalls[id as usize];
        for _ in 0..1 + (dice.next() * 2.0) as usize {
            let (spot, yaw) = in_front(&stall, &mut dice);
            if room > strollers && free(spot) {
                spawn_walker(&mut commands, &books, &mut dice, Role::Shopper, None, true, spot, yaw, layout.terrain.height(spot.x, spot.y), now);
                room -= 1;
            }
        }
    }
    // The rest stroll, in twos and threes, along the pavement if there is one.
    let mut tries = 0;
    while room > 0 && tries < 24 {
        tries += 1;
        // Up the street ahead of the player, mostly: a street is a road and the ground by it.
        let turn = if dice.next() < 0.8 { dice.range(-0.8, 0.8) } else { dice.range(0.0, TAU) };
        let p = at + Vec2::from_angle(turn).rotate(forward) * dice.range(KEEP_OFF + 2.0, CROWD);
        if !free(p) {
            continue;
        }
        let group = 1 + (dice.next() * 3.0) as usize;
        let leader_yaw = dice.range(0.0, TAU);
        for k in 0..group.min(room) {
            let spot = p + Vec2::from_angle(k as f32 * 2.1 + leader_yaw) * if k == 0 { 0.0 } else { 0.9 };
            if free(spot) {
                spawn_walker(&mut commands, &books, &mut dice, Role::Shopper, None, false, spot, leader_yaw, layout.terrain.height(spot.x, spot.y), now);
                room -= 1;
            }
        }
    }
    *seed = dice.0;
}

/// Dogs and cows on the road near the player: as many as the street has road for.
#[allow(clippy::too_many_arguments)]
fn populate_animals(commands: &mut Commands, books: &Books, layout: &MapLayout, busy: f32, at: Vec2, walkers: &Query<(Entity, &Walker)>, dice: &mut Dice, now: f32, free: &impl Fn(Vec2) -> bool) {
    if books.dog.is_none() || books.cow.is_none() {
        return;
    }
    let have = walkers.iter().filter(|(_, w)| w.role.animal()).count();
    let cows = walkers.iter().filter(|(_, w)| w.role == Role::Cow).count();
    let want = ((3.0 + busy * 10.0).round() as usize).min(ANIMALS);
    let mut left = want.saturating_sub(have);
    let mut tries = 0;
    while left > 0 && tries < 12 {
        tries += 1;
        let p = at + Vec2::from_angle(dice.range(0.0, TAU)) * dice.range(12.0, DRAWN - 8.0);
        let on_road = matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::ROAD | ground::PATH | ground::PAVEMENT | ground::PLAZA | ground::PARKING));
        if !on_road || !free(p) {
            continue;
        }
        let role = if cows < 2 + (want > 6) as usize && dice.next() < 0.35 { Role::Cow } else { Role::Dog };
        let yaw = dice.range(0.0, TAU);
        spawn_walker(commands, books, dice, role, None, true, p, yaw, layout.terrain.height(p.x, p.y), now);
        left -= 1;
    }
}

/// Who stands at a stall, and where: role, spot and which way they face.
fn stall_people(s: &Stall, dice: &mut Dice) -> Vec<(Role, Vec2, f32)> {
    let at = |x: f32, z: f32| s.pos.xz() + local(s.yaw, Vec3::new(x, 0.0, z)).xz();
    match s.kind {
        // Beside his cart, facing across it.
        StallKind::Thela => {
            let side = if dice.next() < 0.5 { 1.05 } else { -1.05 };
            vec![(Role::Keeper, at(side, 0.0), s.yaw + if side > 0.0 { PI / 2.0 } else { -PI / 2.0 })]
        }
        // Behind the counter, and a customer or two on the bench in front.
        StallKind::Chai => {
            let mut out = vec![(Role::Keeper, at(0.0, 0.72), s.yaw)];
            out.push((Role::Sitter, at(-0.4, -1.15), s.yaw + PI));
            if dice.next() < 0.5 {
                out.push((Role::Sitter, at(0.45, -1.15), s.yaw + PI));
            }
            out
        }
        StallKind::Crates => vec![(Role::Keeper, at(0.0, 0.75), s.yaw)],
        StallKind::Chairs => vec![(Role::Sitter, at(-0.35, 0.0), s.yaw + PI + 0.3)],
        // On his stool behind the sheet.
        StallKind::Patri => vec![(Role::Seller, at(0.0, 0.98), s.yaw)],
        // Behind the table, facing the road.
        StallKind::Row => vec![(Role::Keeper, at(dice.range(-0.7, 0.7), 0.86), s.yaw)],
        StallKind::Cart => {
            let side = if dice.next() < 0.5 { 1.05 } else { -1.05 };
            vec![(Role::Keeper, at(side, 0.0), s.yaw + if side > 0.0 { PI / 2.0 } else { -PI / 2.0 })]
        }
        StallKind::Scooters => vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_walker(commands: &mut Commands, books: &Books, dice: &mut Dice, role: Role, stall: Option<u32>, at_stall: bool, spot: Vec2, yaw: f32, y: f32, now: f32) {
    let woman = role == Role::Shopper && dice.next() < 0.5;
    let Some(book) = books.of(role, woman) else { return };
    let look = (dice.next() * book.looks.len() as f32) as usize % book.looks.len();
    let clip_name = match role {
        Role::Sitter => "sip",
        Role::Seller => "sit",
        _ => "idle",
    };

    let Some(clip) = book.clip(clip_name) else { return };
    // Those not at a stall are on their way somewhere.
    let until = now + if role == Role::Shopper && !at_stall { dice.range(0.0, 0.5) } else { dice.range(2.0, 9.0) };
    let phase = dice.next();
    commands.spawn((
        Walker {
            role,
            woman,
            stall,
            home: spot,
            home_yaw: yaw,
            pos: spot,
            yaw,
            doing: Doing::Stay { until, yaw },
            clip,
            phase,
            frame: 0,
            dice: Dice(dice.0 ^ 0x9e37_79b9),
            hidden: 0.0,
            resting: clip_name,
        },
        Mesh3d(book.clips[clip].frames[0].clone()),
        MeshMaterial3d(book.looks[look].clone()),
        Transform::from_xyz(spot.x, y, spot.y).with_rotation(Quat::from_rotation_y(yaw)),
        Aabb::from(book.bounds),
        NoAutoAabb,
        VisibilityRange { start_margin: 0.0..0.0, end_margin: DRAWN..DRAWN + FADE, use_aabb: false },
        Tagged("life"),
        OfMap,
    ));
}

/// Where a shopper stands to look over a stall, and which way.
fn in_front(s: &Stall, dice: &mut Dice) -> (Vec2, f32) {
    let (x, z) = match s.kind {
        StallKind::Thela | StallKind::Cart => (if dice.next() < 0.5 { 1.2 } else { -1.2 }, dice.range(-0.8, 0.8)),
        StallKind::Chai => (dice.range(-0.8, 0.8), -1.7),
        StallKind::Row => (dice.range(-1.0, 1.0), -1.15),
        _ => (dice.range(-0.9, 0.9), dice.range(-1.2, -1.0)),
    };
    let spot = s.pos.xz() + local(s.yaw, Vec3::new(x, 0.0, z)).xz();
    (spot, facing(s.pos.xz() - spot) + dice.range(-0.3, 0.3))
}

/// A way to run from `from`, away from `danger`: as far as the street goes that way, up to 40 m.
fn escape(layout: &MapLayout, floor: &StreetFloor, market: &Market, from: Vec2, danger: Vec2, dice: &mut Dice) -> Vec2 {
    let away = (from - danger).normalize_or(Vec2::X);
    let mut best = (f32::MIN, from + away * 6.0);
    let start = dice.range(0.0, TAU);
    for k in 0..12 {
        let dir = Vec2::from_angle(start + k as f32 / 12.0 * TAU);
        let mut run = 0.0;
        while run < 40.0 && walkable(layout, floor, market, from + dir * (run + 2.0)) {
            run += 2.0;
        }
        if run < 6.0 {
            continue;
        }
        // Side lanes make the best way out: a lane's ground beats the open road.
        let end = from + dir * run;
        let lane = if paved(layout, end) { 6.0 } else { 0.0 };
        let score = run * 0.6 + dir.dot(away) * 26.0 + lane + dice.range(0.0, 8.0);
        if score > best.0 {
            best = (score, end);
        }
    }
    best.1
}

/// Each walker's frame: moves, chooses, runs away, and turns the page.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn walk(
    mut commands: Commands,
    time: Res<Time>,
    books: Res<Books>,
    market: Res<Market>,
    layout: Res<MapLayout>,
    floor: Res<StreetFloor>,
    alarms: Res<Alarms>,
    mut scares: ResMut<Scares>,
    mut scattered: MessageWriter<Scattered>,
    me: Query<&Transform, (With<LocalPlayer>, Without<Walker>)>,
    warriors: Query<&Transform, (With<Warrior>, Without<Eliminated>, Without<LocalPlayer>, Without<Walker>)>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    spatial: avian3d::prelude::SpatialQuery,
    mut q: Query<(Entity, &mut Walker, &mut Transform, &mut Mesh3d)>,
) {
    let _spent = super::spent::Timed::start(&super::spent::PEOPLE);
    if q.is_empty() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    let Ok(camera) = camera.single() else { return };
    let eye = camera.translation();
    let player = me.single().map_or(eye, |t| t.translation).xz();
    // Warriors near enough to matter to anyone here.
    let others: Vec<Vec2> = warriors.iter().map(|t| t.translation.xz()).filter(|p| p.distance_squared(player) < 90.0 * 90.0).collect();
    // Scares first: everyone in earshot is off.
    for (at, reach) in &alarms.0 {
        let spot = at.xz();
        let mut ran = 0;
        let mut any_near = false;
        for (_, mut w, _, _) in &mut q {
            let d = w.pos.distance(spot);
            if d > *reach {
                continue;
            }
            any_near = true;
            if matches!(w.doing, Doing::Flee { .. }) {
                continue;
            }
            let to = {
                let mut dice = w.dice;
                let to = escape(&layout, &floor, &market, w.pos, spot, &mut dice);
                w.dice = dice;
                to
            };
            let wait = 0.08 + d / 90.0 + w.dice.next() * 0.25;
            w.doing = Doing::Flee { to, start: now + wait, began: now };
            ran += 1;
        }
        if any_near {
            scares.add(spot, now + SCARED_FOR);
        }
        if ran > 0 {
            scattered.write(Scattered { at: *at, people: ran });
        }
    }
    for (e, mut w, mut tf, mut mesh) in &mut q {
        let w = &mut *w;
        let Some(book) = books.of(w.role, w.woman) else { continue };
        // The nearest warrior, player or not.
        let nearest = others.iter().copied().chain(std::iter::once(player)).min_by(|a, b| a.distance_squared(w.pos).total_cmp(&b.distance_squared(w.pos))).unwrap_or(player);
        let dist = nearest.distance(w.pos);
        if !matches!(w.doing, Doing::Flee { .. } | Doing::Shy { .. }) && dist < KEEP_OFF {
            w.doing = Doing::Shy { from: nearest };
        }
        let mut pace = 0.0;
        let mut want = "idle";
        let mut turn_to = None;
        let (stroll, bolt) = w.role.paces();
        match w.doing {
            Doing::Stay { until, yaw } => {
                want = w.resting;
                turn_to = Some(yaw);
                if now > until {
                    w.doing = next_step(&layout, &floor, &market, w, now);
                }
            }
            Doing::Go { to, speed, then, yaw } => {
                let d = to - w.pos;
                if d.length() < 0.3 {
                    w.doing = Doing::Stay { until: now + then, yaw: yaw.unwrap_or(w.yaw) };
                } else {
                    let dir = d.normalize();
                    w.pos += dir * speed * dt;
                    turn_to = Some(facing(dir));
                    want = "walk";
                    pace = speed;
                }
            }
            Doing::Shy { from } => {
                let away = (w.pos - from).normalize_or(Vec2::X);
                // Along the wall if the wall's in the way.
                let dir = [0.0f32, 0.7, -0.7, 1.4, -1.4].iter().map(|a| Vec2::from_angle(*a).rotate(away)).find(|d| walkable(&layout, &floor, &market, w.pos + *d * 1.2));
                let speed = stroll * 1.5;
                match dir {
                    Some(dir) if dist < KEEP_OFF + 2.0 => {
                        w.pos += dir * speed * dt;
                        turn_to = Some(facing(dir));
                        want = "walk";
                        pace = speed;
                    }
                    Some(_) => {
                        let (home, yaw) = (w.home, w.home_yaw);
                        w.doing = if w.role == Role::Shopper {
                            Doing::Stay { until: now + 1.5, yaw: w.yaw }
                        } else {
                            Doing::Go { to: home, speed: 1.4, then: 6.0, yaw: Some(yaw) }
                        };
                    }
                    None => {}
                }
            }
            Doing::Flee { to, start, began } => {
                if now >= start {
                    let d = to - w.pos;
                    let speed = bolt;
                    if d.length() < 0.8 {
                        // The end of that run: on, if there's more street, or gone if out of sight.
                        let mut dice = w.dice;
                        let next = escape(&layout, &floor, &market, w.pos, player, &mut dice);
                        w.dice = dice;
                        w.doing = Doing::Flee { to: next, start, began };
                    } else {
                        let dir = d.normalize();
                        w.pos += dir * speed * dt;
                        turn_to = Some(facing(dir));
                        want = "run";
                        pace = speed;
                    }
                    // Gone for good once the player can't see them.
                    let head = Vec3::new(w.pos.x, tf.translation.y + 1.5, w.pos.y);
                    let to_eye = eye - head;
                    let ahead = (head - eye).normalize_or_zero().dot(*camera.forward()) > 0.5;
                    let hit = Dir3::new(to_eye).ok().and_then(|dir| spatial.cast_ray(head, dir, to_eye.length(), true, &avian3d::prelude::SpatialQueryFilter::from_mask(shared::sim::solid_mask())));
                    w.hidden = if !ahead || hit.is_some() { w.hidden + dt } else { 0.0 };
                    if (w.hidden > 0.25 && now - began > 1.0) || now - began > 14.0 {
                        commands.entity(e).despawn();
                        continue;
                    }
                }
            }
        }
        // Turn the page.
        let Some(clip) = book.clip(want) else { continue };
        if clip != w.clip {
            w.clip = clip;
            w.frame = usize::MAX;
        }
        let pages = &book.clips[clip];
        // The clip's own beat when walking at its speed; a slow stroll stretches it.
        let (walk_beat, run_beat) = w.role.beats();
        let cycle = match want {
            "walk" => pages.cycle * (walk_beat / pace.max(0.5)).clamp(0.8, 1.8),
            "run" => pages.cycle * (run_beat / pace.max(1.0)).clamp(0.8, 1.3),
            _ => pages.cycle,
        };
        w.phase = (w.phase + dt / cycle).fract();
        let frame = ((w.phase * pages.frames.len() as f32) as usize).min(pages.frames.len() - 1);
        if frame != w.frame {
            w.frame = frame;
            mesh.0 = pages.frames[frame].clone();
        }
        if let Some(goal) = turn_to {
            let d = (goal - w.yaw + PI).rem_euclid(TAU) - PI;
            let step = (if pace > 3.0 { 10.0 } else { 5.0 }) * dt;
            w.yaw += d.clamp(-step, step);
        }
        tf.translation = Vec3::new(w.pos.x, layout.terrain.height(w.pos.x, w.pos.y), w.pos.y);
        tf.rotation = Quat::from_rotation_y(w.yaw);
    }
}

/// What someone does next after standing about.
fn next_step(layout: &MapLayout, floor: &StreetFloor, market: &Market, w: &mut Walker, now: f32) -> Doing {
    match w.role {
        Role::Keeper | Role::Sitter | Role::Seller => Doing::Stay { until: now + w.dice.range(4.0, 14.0), yaw: w.home_yaw },
        Role::Dog | Role::Cow => {
            // Lie down, nose about, or move on along the road.
            let roll = w.dice.next();
            let (stroll, _) = w.role.paces();
            w.resting = if w.role == Role::Dog && roll < 0.3 {
                "sleep"
            } else if roll < 0.55 {
                "eat"
            } else if roll < 0.75 {
                "low"
            } else {
                "idle"
            };
            if w.dice.next() < 0.5 {
                for _ in 0..6 {
                    let to = w.pos + Vec2::from_angle(w.dice.range(0.0, TAU)) * w.dice.range(6.0, 26.0);
                    if line_clear(layout, floor, market, w.pos, to) && !paved_only(layout, to) {
                        return Doing::Go { to, speed: stroll * w.dice.range(0.85, 1.15), then: w.dice.range(3.0, 16.0), yaw: None };
                    }
                }
            }
            Doing::Stay { until: now + w.dice.range(5.0, 20.0), yaw: w.yaw }
        }
        Role::Shopper => {
            // Mostly to the next stall along; now and then just on down the street.
            if w.dice.next() < 0.7 {
                let stalls: Vec<u32> = market.stalls_near(w.pos, 28.0).filter(|id| !matches!(market.stalls[*id as usize].kind, StallKind::Chairs | StallKind::Scooters)).collect();
                for _ in 0..4 {
                    let Some(id) = stalls.get((w.dice.next() * stalls.len() as f32) as usize) else { break };
                    let stall = market.stalls[*id as usize];
                    let mut dice = w.dice;
                    let (to, yaw) = in_front(&stall, &mut dice);
                    w.dice = dice;
                    if to.distance(w.pos) > 3.0 && line_clear(layout, floor, market, w.pos, to) {
                        return Doing::Go { to, speed: w.dice.range(1.25, 1.7), then: w.dice.range(4.0, 14.0), yaw: Some(yaw) };
                    }
                }
            }
            for _ in 0..8 {
                let to = w.pos + Vec2::from_angle(w.dice.range(0.0, TAU)) * w.dice.range(8.0, 34.0);
                if line_clear(layout, floor, market, w.pos, to) {
                    return Doing::Go { to, speed: w.dice.range(1.25, 1.7), then: w.dice.range(2.0, 9.0), yaw: None };
                }
            }
            Doing::Stay { until: now + w.dice.range(2.0, 6.0), yaw: w.yaw }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scare_keeps_people_away_for_a_while_and_then_lets_them_back() {
        let mut scares = Scares::default();
        let spot = Vec2::new(10.0, 20.0);
        scares.add(spot, 100.0);
        assert!(scares.scared(spot + Vec2::X * (SCARED_WIDE - 1.0), 50.0));
        assert!(!scares.scared(spot + Vec2::X * (SCARED_WIDE + 1.0), 50.0));
        assert!(!scares.scared(spot, 101.0));
        // A burst of fire stays one scare, and lasts as long as the last shot.
        scares.add(spot + Vec2::X, 130.0);
        assert_eq!(scares.0.len(), 1);
        assert!(scares.scared(spot, 120.0));
    }

    #[test]
    fn nobody_fills_more_than_the_street_allows() {
        let mut scares = Scares::default();
        for k in 0..100 {
            scares.add(Vec2::new(k as f32 * 100.0, 0.0), 1000.0);
        }
        assert!(scares.0.len() <= 32);
    }

    #[test]
    fn a_stall_has_its_people_beside_it_and_never_in_it() {
        let mut dice = Dice(7);
        for kind in [StallKind::Thela, StallKind::Chai, StallKind::Crates, StallKind::Chairs, StallKind::Row, StallKind::Cart, StallKind::Patri] {
            for yaw in [0.0, 1.0, 2.5] {
                let stall = Stall { kind, pos: Vec3::new(5.0, 0.0, 9.0), yaw };
                for (role, spot, _) in stall_people(&stall, &mut dice) {
                    let d = spot.distance(stall.pos.xz());
                    assert!(d < 1.6, "{kind:?}: {role:?} {d} m off");
                    assert!(kind == StallKind::Chairs || d > 0.6, "{kind:?}: {role:?} in the stall");
                }
            }
        }
    }

    #[test]
    fn faces_the_way_it_goes() {
        for d in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y, Vec2::new(0.6, -0.8)] {
            let forward = Quat::from_rotation_y(facing(d)) * Vec3::NEG_Z;
            assert!((forward.xz() - d).length() < 1e-3, "{d}");
        }
    }

    /// Every file the market needs bakes, and comes out standing on the ground, facing -Z.
    #[test]
    fn every_book_bakes_and_stands_up() {
        for (name, spec) in specs() {
            let baked = flipbook::bake(&spec).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(baked.clips.len(), spec.shots.len(), "{name}");
            assert_eq!(baked.images.len(), spec.looks.len() + spec.coats.len(), "{name}");
            for clip in &baked.clips {
                assert!(!clip.frames.is_empty() && clip.cycle > 0.05, "{name} {}", clip.key);
                let verts = clip.frames[0].count_vertices();
                assert!(verts > 300, "{name} {}: {verts}", clip.key);
                assert!(clip.frames.iter().all(|f| f.count_vertices() == verts), "{name} {}", clip.key);
            }
            let height = baked.hi.y - baked.lo.y;
            let (low, high) = match name {
                "men" | "women" => (1.45, 2.1),
                "dog" => (0.45, 1.1),
                _ => (1.0, 2.1),
            };
            assert!(height > low && height < high, "{name}: {height} m tall");
            assert!(baked.lo.y.abs() < 0.02, "{name}: feet at {}", baked.lo.y);
        }
    }
}
