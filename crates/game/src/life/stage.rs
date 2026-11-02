//! The market on demand, for pictures and clips from the real game: stands the local player in a
//! street as soon as a match is under way, walks him, and fires his gun at set times, so a clip
//! can show what a gunshot does. Nothing here runs unless asked.
//!
//! VIVAD_LIFE_STAGE=x,z,yaw   stand there, facing that way (degrees, 0 is north, 90 west).
//! VIVAD_LIFE_WALK=a,b        walk forward from a to b seconds after arriving.
//! VIVAD_LIFE_FIRE=t,t,...    fire at these seconds after arriving.
//! VIVAD_LIFE_CROUCH=a,b      stay down from a to b seconds after arriving.
//! VIVAD_LIFE_RIFLE=d         a rifleman stands d metres ahead and fires at the player's chest,
//!                            brainless, as a bot behind a stall would never be seen to.
//! VIVAD_LIFE_DRIVE=kind,x,z,speed,t,t,...  the player gets into a new vehicle of that kind, and
//!                            from VIVAD_LIFE_WALK's first time drives it straight at x,z at that
//!                            speed, sounding the horn at these times.
//!
//! Run it with VIVAD_AUTOPILOT=1 (the match then starts by itself and the player's brain is taken
//! out), and VIVAD_CLIP=<dir>,<from>,<to>,<fps> with VIVAD_CLIP_VIEW=1 for frames with the window
//! hidden, or VIVAD_SHOTS with VIVAD_HUD_IMAGE=1 for stills.

use bevy::prelude::*;
use shared::items::{ItemStack, Rarity};
use shared::map::MapLayout;
use shared::sim::bots::BotBrain;
use shared::sim::flow::Match;
use shared::sim::vehicle::{Vehicle, VehicleKind, spawn_vehicle};
use shared::sim::{Action, HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, PosHistory, Warrior, btn, stance::Stance};

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        let Ok(spec) = std::env::var("VIVAD_LIFE_STAGE") else { return };
        let n: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let [x, z, yaw] = n[..] else { return };
        let list = |name: &str| -> Vec<f32> { std::env::var(name).unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect() };
        let walk = list("VIVAD_LIFE_WALK");
        let crouch = list("VIVAD_LIFE_CROUCH");
        let drive = std::env::var("VIVAD_LIFE_DRIVE").ok().and_then(|d| {
            let (kind, rest) = d.split_once(',')?;
            let kind = VehicleKind::ALL.into_iter().find(|k| format!("{k:?}").eq_ignore_ascii_case(kind))?;
            let n: Vec<f32> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            Some(Drive { kind, to: Vec2::new(*n.first()?, *n.get(1)?), speed: *n.get(2)?, horns: n[3..].to_vec(), vehicle: None })
        });
        let rifle = list("VIVAD_LIFE_RIFLE").first().copied();
        app.insert_resource(Stage { at: Vec2::new(x, z), yaw: yaw.to_radians(), walk: (walk.first().copied().unwrap_or(0.0), walk.get(1).copied().unwrap_or(0.0)), fire: list("VIVAD_LIFE_FIRE"), crouch: (crouch.first().copied().unwrap_or(0.0), crouch.get(1).copied().unwrap_or(0.0)), rifle, rifleman: None, shots: 0, drive, log: 0.0, since: None })
            .add_systems(PreUpdate, (stand, rifleman).chain().after(crate::input::LocalInput));
    }
}

/// A drive through the street.
struct Drive {
    kind: VehicleKind,
    to: Vec2,
    speed: f32,
    horns: Vec<f32>,
    vehicle: Option<Entity>,
}

#[derive(Resource)]
struct Stage {
    at: Vec2,
    yaw: f32,
    walk: (f32, f32),
    fire: Vec<f32>,
    crouch: (f32, f32),
    /// How far off the rifleman stands, who he is, and the shots he has fired.
    rifle: Option<f32>,
    rifleman: Option<Entity>,
    shots: u32,
    drive: Option<Drive>,
    /// When it last said where the player is.
    log: f32,
    since: Option<f32>,
}

#[allow(clippy::type_complexity)]
fn stand(
    mut commands: Commands,
    mut stage: ResMut<Stage>,
    m: Res<Match>,
    layout: Res<MapLayout>,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Transform, &mut Kinematic, &mut Inventory, &mut PlayerIntent, &mut Health, &mut Action, &mut PosHistory), With<LocalPlayer>>,
) {
    let Ok((e, mut tf, mut kin, mut inv, mut intent, mut health, mut act, mut hist)) = q.single_mut() else { return };
    if m.elapsed < 0.5 {
        return;
    }
    let now = time.elapsed_secs();
    let Some(since) = stage.since else {
        stage.since = Some(now);
        commands.entity(e).remove::<shared::sim::bots::BotBrain>();
        let p = Vec3::new(stage.at.x, layout.terrain.height(stage.at.x, stage.at.y) + HALF_HEIGHT + 0.05, stage.at.y);
        tf.translation = p;
        hist.prev = p;
        hist.curr = p;
        kin.mode = MoveMode::Ground;
        kin.velocity = Vec3::ZERO;
        kin.fall_from = Some(p.y);
        tf.rotation = Quat::from_rotation_y(stage.yaw);
        intent.yaw = stage.yaw;
        inv.slots[0] = Some(ItemStack { id: "tamancha".into(), rarity: Rarity::Epic, count: 60 });
        inv.arrows = 200;
        inv.selected = 1;
        return;
    };
    let t = now - since;
    if t - stage.log >= 0.5 {
        stage.log = t;
        info!("[life-stage] t {t:.1} at {:.2} hp {:.0} shield {:.0} mode {:?} speed {:.1}", tf.translation, health.hp, health.shield, kin.mode, kin.velocity.length());
    }
    // A rifleman's target lives to show what the shots did.
    if stage.rifle.is_none() {
        health.hp = health.hp.max(90.0);
    }
    act.reload_left = 0.0;
    if let Some(drive) = &mut stage.drive {
        match drive.vehicle {
            None => {
                let (kind, to) = (drive.kind, drive.to);
                let dir = (to - tf.translation.xz()).normalize_or_zero();
                let at = tf.translation.xz() + dir.perp() * (kind.spec().size.x / 2.0 + 0.9);
                let foot = Vec3::new(at.x, layout.terrain.height(at.x, at.y), at.y);
                drive.vehicle = Some(spawn_vehicle(&mut commands, kind, foot, (-dir.x).atan2(-dir.y)));
            }
            Some(_) => {
                if kin.mode == MoveMode::Ground && t > 0.6 {
                    intent.pressed |= btn::INTERACT;
                }
            }
        }
        return;
    }
    intent.yaw = stage.yaw;
    intent.pitch = 0.04;
    intent.select = None;
    intent.move_axis = if t >= stage.walk.0 && t < stage.walk.1 { Vec2::Y * 0.6 } else { Vec2::ZERO };
    let fire = stage.fire.iter().any(|f| t >= *f && t < *f + 0.1);
    let was = intent.held(btn::FIRE);
    intent.held = if fire { btn::FIRE } else { 0 };
    if fire && !was {
        intent.pressed |= btn::FIRE;
    }
    if t >= stage.crouch.0 && t < stage.crouch.1 {
        intent.held |= btn::CROUCH;
    }
}

/// The rest of the stage's cast: the rifleman, and the wheel of the drive.
#[allow(clippy::type_complexity)]
fn rifleman(
    mut commands: Commands,
    mut stage: ResMut<Stage>,
    layout: Res<MapLayout>,
    time: Res<Time>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    me: Query<(&Transform, &Stance), With<LocalPlayer>>,
    mut intents: Query<&mut PlayerIntent, With<LocalPlayer>>,
    vehicles: Query<(&Transform, &Vehicle)>,
    mut bots: Query<(Entity, &mut Transform, &mut Kinematic, &mut PosHistory, &mut Inventory, &mut PlayerIntent), (With<Warrior>, Without<LocalPlayer>, Without<Vehicle>)>,
) {
    let Some(since) = stage.since else { return };
    let now = time.elapsed_secs();
    let t = now - since;
    let stage = &mut *stage;
    if let Some(drive) = &mut stage.drive
        && let (Some(ride), Ok(mut intent)) = (drive.vehicle, intents.single_mut())
        && let Ok((tf, v)) = vehicles.get(ride)
        && v.driver.is_some()
    {
        let d = drive.to - tf.translation.xz();
        let off = ((-d.x).atan2(-d.y) - v.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let pedal = if t < stage.walk.0 {
            0.0
        } else if d.length() < 8.0 || v.speed > drive.speed + 0.5 {
            -1.0
        } else if v.speed < drive.speed {
            1.0
        } else {
            0.0
        };
        intent.move_axis = Vec2::new((-off * 3.0).clamp(-1.0, 1.0), pedal);
        keys.release(KeyCode::KeyH);
        if drive.horns.first().is_some_and(|h| t >= *h) {
            drive.horns.remove(0);
            keys.press(KeyCode::KeyH);
        }
    }
    let Some(reach) = stage.rifle else { return };
    let Ok((me, stance)) = me.single() else { return };
    let forward = Vec2::new(-stage.yaw.sin(), -stage.yaw.cos());
    // A little to the side: straight ahead he stands behind the player's back in every picture.
    let spot = stage.at + forward * reach + forward.perp() * 2.2;
    let from = Vec3::new(spot.x, layout.terrain.height(spot.x, spot.y) + HALF_HEIGHT + 0.05, spot.y);
    if stage.rifleman.is_none() {
        // Not before the player stands where he is to stand.
        let Some((e, ..)) = bots.iter().next() else { return };
        commands.entity(e).remove::<BotBrain>();
        stage.rifleman = Some(e);
        return;
    }
    let Some(Ok((_, mut tf, mut kin, mut hist, mut inv, mut intent))) = stage.rifleman.map(|e| bots.get_mut(e)) else { return };
    tf.translation = from;
    hist.prev = from;
    hist.curr = from;
    kin.mode = MoveMode::Ground;
    kin.velocity = Vec3::ZERO;
    inv.slots[0] = Some(ItemStack { id: "saintalis".into(), rarity: Rarity::Rare, count: 30 });
    inv.selected = 1;
    inv.arrows = 90;
    // The aim that lands on him from over this one's shoulder: his chest, over the table if he stands.
    let target = me.translation + Vec3::Y * if stance.crouched { -0.25 } else { 0.5 };
    let (mut yaw, mut pitch) = (0.0, 0.0);
    let mut origin = from + Vec3::Y * 0.72;
    for _ in 0..3 {
        let d = target - origin;
        (yaw, pitch) = ((-d.x).atan2(-d.z), d.y.atan2(d.xz().length()));
        let probe = PlayerIntent { yaw, pitch, ..default() };
        origin = shared::sim::aim_ray(from, &probe).0;
    }
    intent.yaw = yaw;
    intent.pitch = pitch;
    intent.move_axis = Vec2::ZERO;
    let fire = t > 2.5 && (t * 3.0).fract() < 0.1;
    intent.held = if fire { btn::FIRE } else { 0 };
    if fire {
        stage.shots += 1;
    }
    if (t * 2.0).fract() < 0.02 {
        info!("[life-stage] rifleman at {:.1} mode {:?} held {} shots asked {} rounds {:?}", tf.translation, kin.mode, intent.held, stage.shots, inv.slots[0].as_ref().map(|s| s.count));
    }
}
