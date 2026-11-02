//! The market on demand, for pictures and clips from the real game: stands the local player in a
//! street as soon as a match is under way, walks him, and fires his gun at set times, so a clip
//! can show what a gunshot does. Nothing here runs unless asked.
//!
//! VIVAD_LIFE_STAGE=x,z,yaw   stand there, facing that way (degrees, 0 is north, 90 west).
//! VIVAD_LIFE_WALK=a,b        walk forward from a to b seconds after arriving.
//! VIVAD_LIFE_FIRE=t,t,...    fire at these seconds after arriving.
//!
//! Run it with VIVAD_AUTOPILOT=1 (the match then starts by itself and the player's brain is taken
//! out), and VIVAD_CLIP=<dir>,<from>,<to>,<fps> with VIVAD_CLIP_VIEW=1 for frames with the window
//! hidden, or VIVAD_SHOTS with VIVAD_HUD_IMAGE=1 for stills.

use bevy::prelude::*;
use shared::items::{ItemStack, Rarity};
use shared::map::MapLayout;
use shared::sim::flow::Match;
use shared::sim::{Action, HALF_HEIGHT, Health, Inventory, Kinematic, LocalPlayer, MoveMode, PlayerIntent, PosHistory, btn};

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        let Ok(spec) = std::env::var("VIVAD_LIFE_STAGE") else { return };
        let n: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let [x, z, yaw] = n[..] else { return };
        let list = |name: &str| -> Vec<f32> { std::env::var(name).unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect() };
        let walk = list("VIVAD_LIFE_WALK");
        app.insert_resource(Stage { at: Vec2::new(x, z), yaw: yaw.to_radians(), walk: (walk.first().copied().unwrap_or(0.0), walk.get(1).copied().unwrap_or(0.0)), fire: list("VIVAD_LIFE_FIRE"), since: None })
            .add_systems(PreUpdate, stand.after(crate::input::LocalInput));
    }
}

#[derive(Resource)]
struct Stage {
    at: Vec2,
    yaw: f32,
    walk: (f32, f32),
    fire: Vec<f32>,
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
        inv.slots[0] = Some(ItemStack { id: "tamancha".into(), rarity: Rarity::Epic, count: 60 });
        inv.arrows = 200;
        inv.selected = 1;
        return;
    };
    let t = now - since;
    health.hp = health.hp.max(90.0);
    act.reload_left = 0.0;
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
}
