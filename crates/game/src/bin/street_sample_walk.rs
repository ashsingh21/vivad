//! Physics-only traversal check for the visual sample. No window, match or renderer.
use avian3d::prelude::*;
use bevy::{prelude::*, time::TimeUpdateStrategy};
use game::street_sample::{SamplePlan, spawn_colliders, walker_bundle};
use shared::sim::{DamageMsg, HALF_HEIGHT, Kinematic, PlayerIntent};
use std::time::Duration;

fn main() {
    let mut failures = 0;
    for (name, feet, seconds, route) in [
        ("road", Vec3::new(-1.0, 0.02, 38.0), 8.0, 0),
        ("arcade", Vec3::new(-10.5, 0.2, 34.0), 8.0, 1),
        ("shop entrance", Vec3::new(-7.0, 0.2, 40.0), 1.4, 2),
        ("metro stairs", Vec3::new(8.2, 0.2, 16.8), 3.0, 3),
    ] {
        let mut app = App::new();
        app.insert_resource(SamplePlan::load())
            .add_plugins((
                MinimalPlugins,
                TransformPlugin,
                AssetPlugin::default(),
                bevy::mesh::MeshPlugin,
                bevy::diagnostic::DiagnosticsPlugin,
            ))
            .add_plugins(PhysicsPlugins::default())
            .insert_resource(Gravity(Vec3::ZERO))
            .insert_resource(Time::<Fixed>::from_hz(60.0))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1.0 / 60.0,
            )))
            .add_message::<DamageMsg>()
            .add_plugins(shared::sim::movement::MovementPlugin)
            .add_systems(Startup, spawn_colliders);
        app.finish();
        app.cleanup();
        for _ in 0..3 {
            app.update();
        }
        let walker = app.world_mut().spawn(walker_bundle(feet)).id();
        for _ in 0..(seconds * 60.0) as usize {
            let p = app.world().get::<Transform>(walker).unwrap().translation;
            let wish = match route {
                1 => {
                    let z = p.z - 2.0;
                    let x = 0.00065 * (z - 30.0).min(0.0).powi(2) - 10.5;
                    Vec3::new(x - p.x, 0.0, -2.0).normalize()
                }
                2 => Vec3::NEG_X,
                _ => Vec3::NEG_Z,
            };
            let mut intent = app.world_mut().get_mut::<PlayerIntent>(walker).unwrap();
            intent.move_axis = Vec2::Y;
            intent.yaw = (-wish.x).atan2(-wish.z);
            app.update();
        }
        let p = app.world().get::<Transform>(walker).unwrap().translation - Vec3::Y * HALF_HEIGHT;
        let grounded = app.world().get::<Kinematic>(walker).unwrap().grounded;
        let pass = grounded
            && match route {
                0 => p.z < -3.0,
                1 => p.z < -7.0,
                2 => p.x < -14.0,
                _ => p.z < 3.5 && p.y < -2.9,
            };
        println!(
            "{} {name}: feet={p:?}, grounded={grounded}",
            if pass { "PASS" } else { "FAIL" }
        );
        if !pass {
            failures += 1;
        }
    }
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
