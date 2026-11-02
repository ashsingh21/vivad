//! The drive law: arcade kinematics with no physics engine, copied from TukTuk.io's
//! client/src/drive/tuk.ts and put into Rust. The engine pulls hardest off the line and fades with
//! speed, drag grows with the square of it, steering needs speed and calms at the top end. Each
//! vehicle has a row of numbers (`crate::sim::vehicle`); TukTuk.io's own are the auto's.
//!
//! MIT License
//!
//! Copyright (c) 2026 Anjula Karunarathne
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

/// One vehicle's numbers.
#[derive(Clone, Copy, Debug)]
pub struct Law {
    /// Top speed and top speed in reverse, m/s.
    pub top: f32,
    pub reverse: f32,
    /// Engine's pull off the line, m/s^2; it fades to 40% of this at top speed.
    pub pull: f32,
    pub brake: f32,
    /// Air drag, per (m/s)^2.
    pub drag: f32,
    /// Turn rate with full steering, radians per second.
    pub turn: f32,
    /// Below this speed (m/s) steering has less bite.
    pub grip: f32,
    /// How much of the turn rate is gone at top speed.
    pub calm: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DriveInput {
    /// 0..1.
    pub throttle: f32,
    /// 0..1: the brake when moving, reverse when stopped.
    pub brake: f32,
    /// -1..1, right positive.
    pub steer: f32,
}

/// One tick. `heading` is a yaw about +Y, so turning right lowers it.
pub fn step(law: &Law, speed: &mut f32, heading: &mut f32, input: DriveInput, dt: f32) {
    let v = *speed;
    let frac = (v / law.top).clamp(0.0, 1.0);
    let mut a = 0.0;
    if v >= 0.0 {
        // Engine pulls hardest off the line, fades to ~40% at top speed.
        a += law.pull * input.throttle * (1.0 - 0.6 * frac);
        if input.brake > 0.0 && v > 0.5 {
            a -= law.brake * input.brake;
        }
        if input.brake > 0.0 && v <= 0.5 {
            a -= 1.6 * input.brake;
        }
        a -= law.drag * v * v.abs();
        if v > 0.1 {
            a -= 0.35;
        }
        if input.throttle == 0.0 && input.brake == 0.0 && v > 0.1 {
            a -= 0.9;
        }
    } else {
        // Reversing: weak engine, strong drag so it stays slow.
        a += 1.6 * input.throttle;
        a -= 1.6 * input.brake;
        a -= 0.02 * v * v.abs();
        if v < -0.1 {
            a += 0.5;
        }
    }
    *speed = (v + a * dt).clamp(-law.reverse, law.top);
    if input.throttle == 0.0 && input.brake == 0.0 && speed.abs() < 0.25 {
        *speed = 0.0;
    }
    // Calmer at speed; reversing, the nose swings the other way, as it does.
    let dir = if *speed >= 0.0 { 1.0 } else { -1.0 };
    let authority = (speed.abs() / law.grip).min(1.0);
    let calm = 1.0 - law.calm * (speed.abs() / law.top).min(1.0);
    if speed.abs() > 0.4 {
        *heading -= input.steer * dir * law.turn * authority * calm * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUK: Law = Law { top: 12.5, reverse: 3.2, pull: 3.6, brake: 6.5, drag: 0.0042, turn: 2.0, grip: 3.5, calm: 0.45 };
    const DT: f32 = 1.0 / 60.0;

    fn run(speed: &mut f32, heading: &mut f32, input: DriveInput, seconds: f32) {
        for _ in 0..(seconds / DT) as u32 {
            step(&TUK, speed, heading, input, DT);
        }
    }

    #[test]
    fn the_engine_reaches_top_speed_slowly_and_no_further() {
        let (mut v, mut h) = (0.0, 0.0);
        let full = DriveInput { throttle: 1.0, ..default_input() };
        run(&mut v, &mut h, full, 3.0);
        assert!(v > 5.0 && v < 11.0, "after 3 s: {v}");
        run(&mut v, &mut h, full, 30.0);
        assert_eq!(v, TUK.top);
    }

    #[test]
    fn the_brake_stops_it_and_then_it_reverses_no_faster_than_its_limit() {
        let (mut v, mut h) = (12.0, 0.0);
        let brake = DriveInput { brake: 1.0, ..default_input() };
        run(&mut v, &mut h, brake, 2.5);
        assert!(v < 0.0, "braked to {v}");
        run(&mut v, &mut h, brake, 20.0);
        assert!(v >= -TUK.reverse);
    }

    #[test]
    fn it_coasts_to_a_stop_and_stays_there() {
        let (mut v, mut h) = (6.0, 0.0);
        run(&mut v, &mut h, default_input(), 8.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn steering_needs_speed_and_right_lowers_the_heading() {
        let right = DriveInput { steer: 1.0, ..default_input() };
        let (mut v, mut h) = (0.0, 0.0);
        run(&mut v, &mut h, right, 1.0);
        assert_eq!(h, 0.0, "no turning on the spot");
        let (mut v, mut h) = (8.0, 0.0);
        run(&mut v, &mut h, DriveInput { throttle: 1.0, ..right }, 0.5);
        assert!(h < -0.3, "{h}");
    }

    fn default_input() -> DriveInput {
        DriveInput::default()
    }
}
