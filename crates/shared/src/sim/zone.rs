//! The Pralaya: a closing ring of cosmic fire. Outside it, warriors burn.

use bevy::prelude::*;
use rand::RngExt;

use super::{DamageCause, DamageMsg, Eliminated, SimSet, Warrior, match_rng, stream};
use super::flow::{Match, MatchState};

/// (wait seconds, shrink seconds, end radius, damage per second outside)
///
/// 520 s from the first wait to the ring's end, so that with the half-minute vimana ride a
/// sixteen-warrior match on a 500 m map runs seven to nine minutes: a minute of looting, the
/// middle game pushed along by the ring, and the last two inside a ring they can see across.
pub const PHASES: [(f32, f32, f32, f32); 6] = [
    (85.0, 45.0, 190.0, 1.0),
    (80.0, 40.0, 115.0, 2.0),
    (65.0, 35.0, 60.0, 4.0),
    (40.0, 30.0, 28.0, 7.0),
    (30.0, 25.0, 10.0, 10.0),
    (20.0, 25.0, 0.0, 14.0),
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
    /// The match seed: each phase's circle comes from it, so every peer closes the same ring.
    seed: u64,
    tick: f32,
    scale: f32,
}

impl Zone {
    pub fn new(seed: u64) -> Self {
        Self::for_extent(seed, 250.0)
    }

    pub fn for_extent(seed: u64, half: f32) -> Self {
        let scale = half / 250.0;
        let mut z = Zone { radius: START_RADIUS * scale, from_radius: START_RADIUS * scale, dps: 1.0, seed, scale, ..default() };
        z.plan_next();
        z
    }

    fn plan_next(&mut self) {
        let Some(&(wait, _, end_r, dps)) = PHASES.get(self.phase) else { return };
        let end_r = end_r * self.scale;
        let mut rng = match_rng(self.seed, stream::ZONE + self.phase as u64);
        // Next circle sits fully inside the current one.
        let slack = (self.radius - end_r).max(0.0) * 0.8;
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        let d = rng.random_range(0.0..slack.min(self.radius));
        let mut c = self.center + Vec2::new(a.cos(), a.sin()) * d;
        c = c.clamp(Vec2::splat(-200.0 * self.scale), Vec2::splat(200.0 * self.scale));
        self.from_center = self.center;
        self.from_radius = self.radius;
        self.target_center = c;
        self.target_radius = end_r;
        self.timer = wait * self.scale.sqrt();
        self.shrinking = false;
        self.dps = dps;
    }

    pub fn outside(&self, p: Vec3) -> bool {
        p.xz().distance(self.center) > self.radius
    }

    /// One tick of the battle: wait, then shrink towards the planned circle, then plan the next.
    pub fn advance(&mut self, dt: f32) {
        let Some(&(_, shrink, _, _)) = PHASES.get(self.phase) else { return };
        let shrink = shrink * self.scale.sqrt();
        self.timer -= dt;
        if !self.shrinking {
            if self.timer <= 0.0 {
                self.shrinking = true;
                self.timer = shrink;
            }
        } else {
            let t = 1.0 - (self.timer / shrink).clamp(0.0, 1.0);
            self.center = self.from_center.lerp(self.target_center, t);
            self.radius = self.from_radius + (self.target_radius - self.from_radius) * t;
            if self.timer <= 0.0 {
                self.center = self.target_center;
                self.radius = self.target_radius;
                self.phase += 1;
                self.plan_next();
            }
        }
    }

    /// Seconds until the next change, and whether it is a shrink.
    pub fn countdown(&self) -> (f32, bool) {
        (self.timer, self.shrinking)
    }
}

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Zone::new(0)).add_systems(FixedUpdate, (advance_zone, burn_outside).chain().in_set(SimSet::Zone));
    }
}

fn advance_zone(mut zone: ResMut<Zone>, m: Res<Match>, time: Res<Time>) {
    if m.state == MatchState::InProgress {
        zone.advance(time.delta_secs());
    }
}

fn burn_outside(
    mut zone: ResMut<Zone>,
    m: Res<Match>,
    q: Query<(Entity, &Transform), (With<Warrior>, Without<Eliminated>, Without<super::net::Remote>)>,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaled_zone_covers_corners_and_finishes() {
        for half in [250.0,500.0] {
            let mut z=Zone::for_extent(7,half);
            assert!(!z.outside(Vec3::new(half,0.0,half)));
            let mut steps=0;
            while z.phase<PHASES.len() && steps<10000 {
                assert!(z.target_center.distance(z.from_center)+z.target_radius<=z.from_radius+0.01);
                z.advance(0.1);steps+=1;
            }
            assert_eq!(z.phase,PHASES.len());
            assert_eq!(z.radius,0.0);
        }
    }
}
