//! The Pralaya: a closing ring of cosmic fire. Outside it, warriors burn.

use bevy::prelude::*;
use rand::RngExt;

use super::{DamageCause, DamageMsg, Eliminated, SimSet, Warrior};
use super::flow::{Match, MatchState};

/// (wait seconds, shrink seconds, end radius, damage per second outside)
pub const PHASES: [(f32, f32, f32, f32); 6] = [
    (50.0, 35.0, 190.0, 1.0),
    (40.0, 30.0, 115.0, 2.0),
    (35.0, 25.0, 60.0, 4.0),
    (25.0, 20.0, 28.0, 7.0),
    (20.0, 20.0, 10.0, 10.0),
    (15.0, 25.0, 0.0, 14.0),
];
pub const START_RADIUS: f32 = 360.0;

#[derive(Resource, Clone, Debug, Default)]
pub struct Zone {
    pub phase: usize,
    pub shrinking: bool,
    pub timer: f32,
    pub center: Vec2,
    pub radius: f32,
    pub from_center: Vec2,
    pub from_radius: f32,
    pub target_center: Vec2,
    pub target_radius: f32,
    pub dps: f32,
    tick: f32,
}

impl Zone {
    pub fn new() -> Self {
        let mut z = Zone { radius: START_RADIUS, from_radius: START_RADIUS, dps: 1.0, ..default() };
        z.plan_next();
        z
    }

    fn plan_next(&mut self) {
        let Some(&(wait, _, end_r, dps)) = PHASES.get(self.phase) else { return };
        let mut rng = rand::rng();
        // Next circle sits fully inside the current one.
        let slack = (self.radius - end_r).max(0.0) * 0.8;
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        let d = rng.random_range(0.0..slack.min(self.radius));
        let mut c = self.center + Vec2::new(a.cos(), a.sin()) * d;
        c = c.clamp(Vec2::splat(-200.0), Vec2::splat(200.0));
        self.from_center = self.center;
        self.from_radius = self.radius;
        self.target_center = c;
        self.target_radius = end_r;
        self.timer = wait;
        self.shrinking = false;
        self.dps = dps;
    }

    pub fn outside(&self, p: Vec3) -> bool {
        p.xz().distance(self.center) > self.radius
    }

    /// Seconds until the next change, and whether it is a shrink.
    pub fn countdown(&self) -> (f32, bool) {
        (self.timer, self.shrinking)
    }
}

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Zone::new()).add_systems(FixedUpdate, (advance_zone, burn_outside).chain().in_set(SimSet::Zone));
    }
}

fn advance_zone(mut zone: ResMut<Zone>, m: Res<Match>, time: Res<Time>) {
    if m.state != MatchState::InProgress {
        return;
    }
    let dt = time.delta_secs();
    let Some(&(_, shrink, _, _)) = PHASES.get(zone.phase) else { return };
    zone.timer -= dt;
    if !zone.shrinking {
        if zone.timer <= 0.0 {
            zone.shrinking = true;
            zone.timer = shrink;
        }
    } else {
        let t = 1.0 - (zone.timer / shrink).clamp(0.0, 1.0);
        zone.center = zone.from_center.lerp(zone.target_center, t);
        zone.radius = zone.from_radius + (zone.target_radius - zone.from_radius) * t;
        if zone.timer <= 0.0 {
            zone.center = zone.target_center;
            zone.radius = zone.target_radius;
            zone.phase += 1;
            zone.plan_next();
        }
    }
}

fn burn_outside(
    mut zone: ResMut<Zone>,
    m: Res<Match>,
    q: Query<(Entity, &Transform), (With<Warrior>, Without<Eliminated>)>,
    time: Res<Time>,
    mut damage: MessageWriter<DamageMsg>,
) {
    if m.state != MatchState::InProgress {
        return;
    }
    zone.tick += time.delta_secs();
    if zone.tick < 1.0 {
        return;
    }
    zone.tick -= 1.0;
    for (e, tf) in &q {
        if zone.outside(tf.translation) {
            damage.write(DamageMsg { target: e, amount: zone.dps, source: None, headshot: false, cause: DamageCause::Pralaya });
        }
    }
}
