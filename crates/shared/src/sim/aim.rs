//! How a bot turns to aim, and what it sees of its target: a moment late, as a person does.
//!
//! Adapted from LGDuel's bot, `crates/bot/src/lib.rs` at https://github.com/s0pk/lgduel: `AimSkill`
//! and its presets, the `seen` queue, `perceive` and the turning in `think`. Changed: radians for
//! degrees, seconds for ticks, our own types for theirs, `gain` scaled to any tick rate, and its
//! noise left out, as `bots` holds its own aim error. Its licence:
//!
//! MIT License
//!
//! Copyright (c) 2026 LGDuel contributors
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

/// Ticks a second LGDuel's `gain` was tuned at.
const GAIN_RATE: f32 = 125.0;
/// The view stops short of straight up and down.
const MAX_PITCH: f32 = 1.55;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimSkill {
    /// How long ago the target stood where the bot sees it, seconds.
    pub reaction: f32,
    /// Fraction (0..1) of the target's velocity the bot leads by over its reaction time.
    pub lead: f32,
    /// Fastest the view turns, radians a second.
    pub turn_speed: f32,
    /// Fraction of the remaining aim error corrected each of LGDuel's ticks (0..1).
    pub gain: f32,
}

impl AimSkill {
    pub const MEDIUM: AimSkill = AimSkill { reaction: 0.18, lead: 0.4, turn_speed: 600f32.to_radians(), gain: 0.3 };
    pub const EASY: AimSkill = AimSkill { reaction: 0.25, lead: 0.0, turn_speed: 360f32.to_radians(), gain: 0.18 };
}

/// What a bot has seen of its target lately, oldest first: when, where, and how it was moving.
#[derive(Clone, Debug, Default)]
pub struct Seen(VecDeque<(f32, Vec3, Vec3)>);

impl Seen {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Remembers the target as it is at `now`, and forgets what is too old to matter.
    pub fn push(&mut self, now: f32, pos: Vec3, velocity: Vec3, skill: &AimSkill) {
        self.0.push_back((now, pos, velocity));
        while self.0.front().is_some_and(|s| s.0 < now - skill.reaction - 0.1) {
            self.0.pop_front();
        }
    }

    /// The target as perceived now: where it was `reaction` ago plus a partial lead, and how it
    /// was moving then.
    pub fn perceive(&self, skill: &AimSkill) -> Option<(Vec3, Vec3)> {
        let &(latest, _, _) = self.0.back()?;
        let want = latest - skill.reaction;
        let &(_, pos, velocity) = self.0.iter().find(|s| s.0 >= want).or(self.0.back())?;
        Some((pos + velocity * skill.reaction * skill.lead, velocity))
    }
}

/// The shortest way round from angle `b` to angle `a`.
fn angle_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(TAU);
    if d > PI { d - TAU } else { d }
}

/// A view (yaw, pitch) turned towards the one wanted, for `dt` seconds: a share of the way, and
/// never faster than the skill allows.
pub fn turn(view: (f32, f32), want: (f32, f32), skill: &AimSkill, dt: f32) -> (f32, f32) {
    let step = skill.turn_speed * dt;
    let gain = 1.0 - (1.0 - skill.gain).powf(GAIN_RATE * dt);
    let yaw = (angle_diff(want.0, view.0) * gain).clamp(-step, step);
    let pitch = ((want.1 - view.1) * gain).clamp(-step, step);
    (view.0 + yaw, (view.1 + pitch).clamp(-MAX_PITCH, MAX_PITCH))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A turn is limited in speed, closes on its target, and goes the short way round.
    #[test]
    fn turns_at_a_limited_speed() {
        let skill = AimSkill::EASY;
        let dt = 1.0 / 64.0;
        let once = turn((0.0, 0.0), (3.0, 0.0), &skill, dt);
        assert!(once.0 > 0.0 && once.0 <= skill.turn_speed * dt + 1e-6);
        let mut view = (0.0, 0.0);
        for _ in 0..64 {
            view = turn(view, (1.0, 0.3), &skill, dt);
        }
        assert!((view.0 - 1.0).abs() < 0.02 && (view.1 - 0.3).abs() < 0.02, "{view:?}");
        assert!(turn((3.0, 0.0), (-3.0, 0.0), &skill, dt).0 > 3.0);
    }

    /// A bot sees its target as it was a moment ago, and only the sharper skill leads it.
    #[test]
    fn sees_a_moment_late() {
        let mut seen = Seen::default();
        let walking = Vec3::X * 4.0;
        for tick in 0..=64 {
            let t = tick as f32 / 64.0;
            seen.push(t, walking * t, walking, &AimSkill::EASY);
        }
        let (at, _) = seen.perceive(&AimSkill::EASY).unwrap();
        assert!((at.x - 3.0).abs() < 0.1, "{at}");
        let (at, _) = seen.perceive(&AimSkill::MEDIUM).unwrap();
        assert!((at.x - 3.57).abs() < 0.1, "{at}");
    }
}
