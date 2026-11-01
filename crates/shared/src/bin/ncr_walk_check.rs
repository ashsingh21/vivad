//! Physics-backed traversal checks for the NCR map. Run with VIVAD_MAP=ncr.
use avian3d::prelude::*;
use bevy::{prelude::*, time::TimeUpdateStrategy};
use shared::{
    map::{MapLayout, ncr},
    sim::*,
};
use std::time::Duration;
fn main() {
    assert!(ncr::enabled(), "run with VIVAD_MAP=ncr");
    let mut app = App::new();
    app.insert_resource(MatchConfig {
        bots: 0,
        online: true,
        ..default()
    })
    .add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        bevy::diagnostic::DiagnosticsPlugin,
    ))
    .add_plugins(PhysicsPlugins::default())
    .add_plugins(SimPlugin {
        items_ron: include_str!("../../../../assets/data/items.ron").into(),
    })
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 60.0,
    )));
    app.finish();
    app.cleanup();
    for _ in 0..3 {
        app.update();
    }
    let mut cases: Vec<(&str, Vec3, Vec3, f32, Vec3)> = vec![];
    for (name, z) in [
        ("north bridge", -270.0 * ncr::WORLD_SCALE),
        ("central bridge", 0.0),
        ("south bridge", 270.0 * ncr::WORLD_SCALE),
    ] {
        cases.push((
            name,
            Vec3::new(-90.0 * ncr::WORLD_SCALE, 1.2, z),
            Vec3::X,
            44.0,
            Vec3::new(85.0 * ncr::WORLD_SCALE, 0.0, z),
        ));
    }
    cases.extend([
        (
            "metro east exit",
            Vec3::new(125.0 * ncr::WORLD_SCALE, -6.9, 0.0),
            Vec3::X,
            10.0,
            Vec3::new(174.0 * ncr::WORLD_SCALE, 0.0, 0.0),
        ),
        (
            "metro west exit",
            Vec3::new(-125.0 * ncr::WORLD_SCALE, -6.9, 0.0),
            Vec3::NEG_X,
            10.0,
            Vec3::new(-174.0 * ncr::WORLD_SCALE, 0.0, 0.0),
        ),
        (
            "metro tunnel",
            Vec3::new(-120.0 * ncr::WORLD_SCALE, -6.9, 0.0),
            Vec3::X,
            42.0,
            Vec3::new(110.0 * ncr::WORLD_SCALE, -8.0, 0.0),
        ),
        (
            "mall interior",
            Vec3::new(-218.0 * ncr::WORLD_SCALE, 1.2, 305.0 * ncr::WORLD_SCALE),
            Vec3::Z,
            10.0,
            Vec3::new(-218.0 * ncr::WORLD_SCALE, 0.0, 355.0 * ncr::WORLD_SCALE),
        ),
        (
            "mall roof stair",
            Vec3::new(-275.0 * ncr::WORLD_SCALE, 1.2, 347.0 * ncr::WORLD_SCALE),
            Vec3::X,
            4.0,
            Vec3::new(-254.0 * ncr::WORLD_SCALE, 9.0, 347.0 * ncr::WORLD_SCALE),
        ),
        (
            "viaduct stair",
            Vec3::new(-114.0 * ncr::WORLD_SCALE, 1.2, 310.0 * ncr::WORLD_SCALE),
            Vec3::X,
            6.0,
            Vec3::new(-84.0 * ncr::WORLD_SCALE, 8.3, 310.0 * ncr::WORLD_SCALE),
        ),
        (
            "film studio",
            Vec3::new(367.0 * ncr::WORLD_SCALE, 1.2, 368.0 * ncr::WORLD_SCALE),
            Vec3::Z,
            12.0,
            Vec3::new(367.0 * ncr::WORLD_SCALE, 0.0, 432.0 * ncr::WORLD_SCALE),
        ),
    ]);
    let mut failed = 0;
    // ONLY=<substring> runs the matching cases; TRACE=<substring> prints their walker every 2 s.
    let only = std::env::var("ONLY").unwrap_or_default();
    let trace = std::env::var("TRACE").unwrap_or_default();
    for (name, start, dir, seconds, target) in cases {
        if !name.contains(&only) {
            continue;
        }
        let id = app
            .world_mut()
            .spawn((
                Transform::from_translation(start),
                Kinematic {
                    mode: MoveMode::Ground,
                    ..default()
                },
                PlayerIntent {
                    move_axis: Vec2::Y,
                    yaw: (-dir.x).atan2(-dir.z),
                    ..default()
                },
                RigidBody::Kinematic,
                Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
                CollisionLayers::new(Layer::Character, [Layer::World, Layer::Door]),
            ))
            .id();
        // The budgets were set for the 1 km blockout: the walks are WORLD_SCALE times longer now.
        for tick in 0..(seconds * ncr::WORLD_SCALE * 60.0) as usize {
            app.update();
            let p = app.world().get::<Transform>(id).unwrap().translation;
            if !trace.is_empty() && name.contains(&trace) && tick % 120 == 0 {
                let kin = app.world().get::<Kinematic>(id).unwrap();
                println!(
                    "t {} p {:?} vel {:?} grounded {} normal {:?}",
                    tick / 60,
                    p,
                    kin.velocity,
                    kin.grounded,
                    kin.ground_normal
                );
            }
            if (p - start).dot(dir) >= (target - start).dot(dir)
                && (p.y - HALF_HEIGHT - target.y).abs() < 1.0
            {
                break;
            }
        }
        let pos = app.world().get::<Transform>(id).unwrap().translation;
        let progress = (pos - start).dot(dir);
        let want = (target - start).dot(dir);
        let ok = progress >= want && (pos.y - HALF_HEIGHT - target.y).abs() < 1.0;
        println!(
            "{} {name}: end {pos:.2}, progress {progress:.1}/{want:.1}",
            if ok { "PASS" } else { "FAIL" }
        );
        if !ok {
            let map = app.world().resource::<MapLayout>();
            for b in map.blocks.iter().filter(|b| b.collide) {
                if let shared::map::Shape::Box(size) = b.shape {
                    let q = b.rot.inverse() * (pos - b.pos);
                    if q.x.abs() < size.x * 0.5 + 1.0
                        && q.z.abs() < size.z * 0.5 + 1.0
                        && q.y.abs() < size.y * 0.5 + 1.0
                    {
                        println!("  nearby solid {:?} pos {:?} size {:?}", b.mat, b.pos, size);
                    }
                }
            }
        }
        failed += usize::from(!ok);
        app.world_mut().despawn(id);
    }
    let m = app.world().resource::<MapLayout>();
    println!(
        "{} blocks / {} chests / {} POIs; {failed} failed",
        m.blocks.len(),
        m.chests.len(),
        m.pois.len()
    );
    assert_eq!(failed, 0);
}
