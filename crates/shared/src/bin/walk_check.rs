//! Headless traversal checks: drop a test walker at a landmark, hold "forward", and report
//! where it ends up. Catches stairs that don't climb, roofs you fall through, and invisible
//! colliders blocking doorways.
//!
//! cargo run --release -p shared --bin walk_check

use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use shared::map::MapLayout;
use shared::sim::{CAPSULE_LENGTH, CAPSULE_RADIUS, HALF_HEIGHT, Kinematic, Layer, MatchConfig, MoveMode, PlayerIntent, SimPlugin};

#[derive(Component)]
struct Walker;

struct Case {
    name: String,
    start: Vec3,
    /// Direction to walk, in the XZ plane.
    dir: Vec3,
    seconds: f32,
    /// Passes when this holds for the final position and the lowest/highest feet heights seen.
    pass: Box<dyn Fn(Vec3, f32, f32) -> bool>,
    /// Only count feet heights where this holds (default: everywhere).
    track: Box<dyn Fn(Vec3) -> bool>,
    want: String,
}

fn yaw_for(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

fn main() {
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    let mut app = App::new();
    app.insert_resource(MatchConfig { bots: 1, autopilot: true, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    for _ in 0..3 {
        app.update();
    }

    let layout = app.world().resource::<MapLayout>().clone();
    let ground = |x: f32, z: f32| Vec3::new(x, layout.terrain.height(x, z) + HALF_HEIGHT + 0.3, z);
    let everywhere = || Box::new(|_: Vec3| true) as Box<dyn Fn(Vec3) -> bool>;
    let mut cases = vec![
        Case {
            name: "ajanta spur stair".into(),
            start: ground(-161.0, 70.0),
            dir: Vec3::NEG_X,
            seconds: 12.0,
            pass: Box::new(|p, _, _| p.y > 24.5 && p.x < -194.0),
            want: "on the cliff top (y > 24.5, x < -194)".into(),
            track: everywhere(),
        },
        Case {
            name: "ajanta stair down".into(),
            start: Vec3::new(-197.0, 24.0 + HALF_HEIGHT + 0.2, 70.0),
            dir: Vec3::X,
            seconds: 12.0,
            pass: Box::new(|p, _, _| p.x > -162.0),
            want: "back past the foot (x > -162)".into(),
            track: everywhere(),
        },
        Case {
            name: "stupa stair to the medhi".into(),
            start: ground(-178.0, 8.0),
            dir: Vec3::X,
            seconds: 3.0,
            pass: Box::new(|p, _, _| p.y > 3.5 && p.x > -168.5),
            want: "up on the drum terrace (y > 3.5)".into(),
            track: everywhere(),
        },
    ];
    for (i, hz) in [-48.0, -16.0, 16.0, 48.0].iter().enumerate() {
        cases.push(Case {
            name: format!("walk into cave {}", i + 1),
            start: ground(-183.0, hz + 1.8),
            dir: Vec3::NEG_X,
            seconds: 5.0,
            pass: Box::new(|p, _, _| p.x < -203.0),
            want: "deep inside the hall (x < -203)".into(),
            track: everywhere(),
        });
    }
    // Hut roofs: land on the front slope, walk over the ridge and off the back eave.
    let huts: Vec<_> = layout.ornaments.iter().filter(|o| o.model.contains("hut_")).take(4).cloned().collect();
    for (i, h) in huts.iter().enumerate() {
        let back = h.rot * Vec3::NEG_Z;
        let d = 4.5 * h.scale.z;
        let (hw, hd) = (2.6 * h.scale.x, d / 2.0);
        let (pos, inv) = (h.pos, h.rot.inverse());
        cases.push(Case {
            name: format!("over hut roof {} ({})", i + 1, h.model.rsplit('/').next().unwrap()),
            start: h.pos + h.rot * Vec3::new(0.0, 0.0, d / 2.0 + 0.2) + Vec3::Y * 5.5,
            dir: back,
            seconds: 2.4,
            // Feet must reach the ridge (4.35) and never drop inside the walls (2.8).
            pass: Box::new(|_, lo, hi| hi > 4.2 && lo > 2.7),
            want: "reaches the ridge, never sinks below the eave".into(),
            // Over the walls' footprint only (walking off the back eave is expected).
            track: Box::new(move |p| {
                let l = inv * (p - pos);
                l.x.abs() < hw - 0.3 && l.z.abs() < hd - 0.3
            }),
        });
    }

    let mut failed = 0;
    for c in cases {
        let id = app
            .world_mut()
            .spawn((
                Walker,
                Kinematic { mode: MoveMode::Ground, ..default() },
                PlayerIntent { move_axis: Vec2::Y, yaw: yaw_for(c.dir), ..default() },
                Transform::from_translation(c.start),
                RigidBody::Kinematic,
                Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
                CollisionLayers::new(Layer::Character, [Layer::World, Layer::Door]),
            ))
            .id();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let ticks = (c.seconds * 60.0) as u32;
        let mut settled = false;
        for t in 0..ticks {
            app.update();
            let w = app.world();
            let p = w.get::<Transform>(id).unwrap().translation;
            settled |= w.get::<Kinematic>(id).unwrap().grounded;
            // Ignore the initial drop onto the surface.
            if std::env::var("TRACE").is_ok() && t % 30 == 0 {
                println!("    t={:.1} pos ({:.2},{:.2},{:.2}) grounded={}", t as f32 / 60.0, p.x, p.y, p.z, w.get::<Kinematic>(id).unwrap().grounded);
            }
            if settled && t > 20 && (c.track)(p) {
                let feet = p.y - HALF_HEIGHT;
                lo = lo.min(feet);
                hi = hi.max(feet);
            }
        }
        let p = app.world().get::<Transform>(id).unwrap().translation;
        let ok = (c.pass)(p, lo, hi);
        failed += (!ok) as u32;
        println!(
            "{} {:<32} start ({:.1},{:.1},{:.1}) end ({:.1},{:.1},{:.1}) feet {:.2}..{:.2}  want {}",
            if ok { "PASS" } else { "FAIL" }, c.name, c.start.x, c.start.y, c.start.z, p.x, p.y, p.z, lo, hi, c.want
        );
        app.world_mut().despawn(id);
    }
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}
