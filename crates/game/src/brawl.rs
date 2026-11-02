//! A staged fight for previews, on any map. VIVAD_BRAWL=<x>,<z> with VIVAD_AUTOPILOT=1: at the
//! start of a match everyone stands in a ring round that point on the ground with a melee weapon
//! in hand, ready to fight, and nobody falls, so a clip (VIVAD_CLIP) can show one at once.
//! VIVAD_BOT_SKILL=normal fights without pauses. The camera stands off and circles it slowly. Never
//! set in play.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use shared::items::{ItemDb, ItemStack, Rarity};
use shared::map::MapLayout;
use shared::sim::bots::BotBrain;
use shared::sim::{HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MoveMode, Warrior};

use crate::camera::{CameraSystems, MainCamera};

/// Who gets what, the local player first. The gada starts near the end of its life, to show a break.
const WEAPONS: [(&str, u32); 6] = [("lathi", 1), ("gada", 3), ("tawa", 1), ("hockey_stick", 1), ("talwar", 24), ("dhal", 30)];
/// The ring everyone starts on, metres.
const RING: f32 = 3.0;
/// Nobody falls: a warrior this hurt is whole again (above where a bot would run from the fight).
const LOW: f32 = 45.0;

pub struct BrawlPlugin;

#[derive(Resource)]
struct Brawl {
    at: Vec2,
    started: bool,
}

impl Plugin for BrawlPlugin {
    fn build(&self, app: &mut App) {
        let Ok(spot) = std::env::var("VIVAD_BRAWL") else { return };
        let (x, z) = spot.split_once(',').expect("VIVAD_BRAWL=<x>,<z>");
        let at = Vec2::new(x.parse().expect("x"), z.parse().expect("z"));
        app.insert_resource(Brawl { at, started: false })
            .add_systems(FixedUpdate, stage.before(shared::sim::SimSet::Brains))
            .add_systems(PostUpdate, watch.after(CameraSystems).before(TransformSystems::Propagate));
    }
}

/// Stands the camera back from the player's own fight, over the follow camera's work.
fn watch(
    mut brawl: ResMut<Brawl>,
    layout: Res<MapLayout>,
    time: Res<Time>,
    me: Query<&Transform, (With<LocalPlayer>, Without<MainCamera>)>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(me) = me.single() else { return };
    let at = brawl.at + (me.translation.xz() - brawl.at) * (time.delta_secs() * 2.0).min(1.0);
    brawl.at = at;
    let ground = layout.terrain.height(at.x, at.y);
    let round = at + Vec2::from_angle(time.elapsed_secs() * 0.07) * 4.8;
    for mut t in &mut cam {
        *t = Transform::from_xyz(round.x, ground + 2.1, round.y).looking_at(Vec3::new(at.x, ground + 0.9, at.y), Vec3::Y);
    }
}

#[allow(clippy::type_complexity)]
fn stage(
    mut brawl: ResMut<Brawl>,
    db: Res<ItemDb>,
    layout: Res<MapLayout>,
    mut q: Query<(&mut Transform, &mut Kinematic, &mut Inventory, &mut Health, Option<&mut BotBrain>, Has<LocalPlayer>), With<Warrior>>,
) {
    if brawl.started || q.is_empty() {
        for (.., mut health, _, _) in &mut q {
            if health.hp < LOW {
                health.hp = health.max_hp;
            }
        }
        return;
    }
    brawl.started = true;
    let n = q.iter().count();
    let mut order: Vec<_> = q.iter_mut().collect();
    order.sort_by_key(|w| !w.5);
    for (k, (tf, kin, inv, _, brain, _)) in order.iter_mut().enumerate() {
        let (id, count) = WEAPONS[k % WEAPONS.len()];
        assert!(db.items.contains_key(id));
        let p = brawl.at + Vec2::from_angle(k as f32 / n as f32 * std::f32::consts::TAU) * RING;
        tf.translation = Vec3::new(p.x, layout.terrain.height(p.x, p.y) + HALF_HEIGHT + 0.05, p.y);
        kin.mode = MoveMode::Ground;
        kin.velocity = Vec3::ZERO;
        inv.slots[0] = Some(ItemStack { id: id.into(), rarity: Rarity::Epic, count });
        inv.selected = 1;
        if let Some(b) = brain.as_mut() {
            b.landed = Some(-1.0e3);
            b.react = 0.1;
            b.calm = true;
        }
    }
}
