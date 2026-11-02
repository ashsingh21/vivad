//! How a vehicle feels, adapted from Kenney's Starter Kit Racing (scripts/vehicle.gd,
//! scripts/vehicle-motorcycle.gd): the body leans into corners and rocks on the throttle, the
//! wheels turn with the speed, the engine's pitch and volume follow speed and throttle.
//!
//! MIT License
//!
//! Copyright (c) 2023 Kenney
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

use bevy::prelude::*;

/// A vehicle's drawn state, eased each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Feel {
    /// Roll into the corner and nose up, radians.
    pub lean: f32,
    pub nose: f32,
    /// The speed a second ago, near enough: the gap to the real speed is the acceleration.
    lag: f32,
    /// Front-wheel steer, radians.
    pub steer: f32,
}

impl Feel {
    pub fn update(&mut self, speed: f32, steer: f32, top: f32, lean: f32, dt: f32) {
        let rate = |r: f32| (dt * r).min(1.0);
        self.lag += (speed - self.lag) * rate(1.0);
        let frac = (speed.abs() / top).min(1.0);
        self.lean += (-steer * frac.sqrt() * lean - self.lean) * rate(5.0);
        self.nose += ((speed - self.lag) / top * 0.5 - self.nose) * rate(10.0);
        self.steer += (-steer / 2.5 - self.steer) * rate(10.0);
    }

    /// The body's turn about the point on the ground under it.
    pub fn body(&self) -> Quat {
        Quat::from_rotation_z(self.lean) * Quat::from_rotation_x(self.nose)
    }
}

/// An engine sound's rate and volume for a fraction of top speed and a throttle.
pub fn engine(speed: f32, throttle: f32) -> (f32, f32) {
    let throttle = throttle.clamp(0.0, 1.0);
    (0.82 + 0.78 * speed + if throttle > 0.1 { 0.08 } else { 0.0 }, 0.3 + 0.32 * speed + 0.18 * throttle)
}
