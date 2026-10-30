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
use shared::map::india::{hampi_at, hampi_dir};
use shared::sim::{CAPSULE_LENGTH, CAPSULE_RADIUS, DamageCause, DamageMsg, HALF_HEIGHT, Health, Inventory, Kinematic, Layer, MatchConfig, MoveMode, PlayerIntent, SimPlugin};

#[derive(Component)]
struct Walker;

/// Fall damage the walkers took (other damage, like the closing zone, doesn't count).
#[derive(Resource, Default)]
struct Falls(f32);

fn record_falls(mut msgs: MessageReader<DamageMsg>, walkers: Query<(), With<Walker>>, mut falls: ResMut<Falls>) {
    for m in msgs.read() {
        if m.cause == DamageCause::Fall && walkers.contains(m.target) {
            falls.0 += m.amount;
        }
    }
}

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
    /// Whether the walk should end hurt (a fall); every other walk must end unhurt.
    hurts: bool,
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
        .init_resource::<Falls>()
        .add_systems(Update, record_falls)
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
            hurts: false,
        },
        Case {
            name: "ajanta stair down".into(),
            start: Vec3::new(-197.0, 24.0 + HALF_HEIGHT + 0.2, 70.0),
            dir: Vec3::X,
            seconds: 12.0,
            pass: Box::new(|p, _, _| p.x > -162.0),
            want: "back past the foot (x > -162)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // From the entrance on the east rim down all seven flights (pillars flank the lane).
            name: "stepwell down to the well".into(),
            start: ground(-72.0, 95.0),
            dir: Vec3::NEG_X,
            seconds: 14.0,
            pass: Box::new(|p, _, _| p.y < -20.0 + HALF_HEIGHT + 0.5 && p.x < -129.0),
            want: "on the bottom floor by the well (y ~ -21, x < -129)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "stepwell back up".into(),
            start: Vec3::new(-131.0, -21.0 + HALF_HEIGHT + 0.2, 95.0),
            dir: Vec3::X,
            seconds: 16.0,
            pass: Box::new(|p, _, _| p.y > 0.5 && p.x > -80.0),
            want: "out at the entrance (x > -80, y > 0.5)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Across the first pavilion's roof, rim to rim, between its railings and through
            // the kiosks at both ends.
            name: "stepwell bridge crossing".into(),
            start: ground(-85.1, 81.0),
            dir: Vec3::Z,
            seconds: 5.5,
            pass: Box::new(|p, lo, _| p.z > 108.0 && lo > -0.2),
            want: "over on the south rim, never dropping (z > 108)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Garh Qila: up the hill and through the gate into the court.
            name: "fort gate into the court".into(),
            start: ground(125.0, -103.0),
            dir: Vec3::NEG_Z,
            seconds: 5.0,
            pass: Box::new(|p, _, _| p.z < -125.0 && p.y > 18.5),
            want: "in the court (z < -125, on the summit)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "fort stair to the wall walk".into(),
            start: Vec3::new(136.0, 18.0 + HALF_HEIGHT + 0.3, -153.5),
            dir: Vec3::X,
            seconds: 3.0,
            pass: Box::new(|_, _, hi| hi > 25.7),
            want: "up on the wall walk (feet > 25.7)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Along the back wall's walk, past a mid-wall bastion, into the corner bastion
            // (between its pavilion's pillars).
            name: "fort wall walk to a bastion".into(),
            start: Vec3::new(112.0, 26.0 + HALF_HEIGHT + 0.3, -156.0),
            dir: Vec3::X,
            seconds: 8.0,
            pass: Box::new(|p, lo, _| p.x > 150.5 && lo > 25.7),
            want: "in the corner bastion, never off the walk (x > 150.5)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "fort stair to the diwan roof".into(),
            start: Vec3::new(106.7, 18.0 + HALF_HEIGHT + 0.3, -149.6),
            dir: Vec3::X,
            seconds: 3.5,
            pass: Box::new(|_, _, hi| hi > 23.8),
            want: "up on the Diwan-i-Aam roof (feet > 23.8)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Hampi: up the bazaar street, through the gopuram into the temple courtyard (left
            // of the axis, past the chariot).
            name: "hampi bazaar through the gopuram".into(),
            start: hampi_at(Vec3::new(-1.0, HALF_HEIGHT + 0.4, -56.0)),
            dir: hampi_dir(Vec3::Z),
            seconds: 7.0,
            pass: Box::new(|p, _, _| { let l = p - hampi_at(Vec3::ZERO); hampi_dir(Vec3::Z).dot(l) > -20.0 }),
            want: "in the courtyard (local z > -20)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "hampi stair to the wall walk".into(),
            start: hampi_at(Vec3::new(9.4, HALF_HEIGHT + 0.4, -28.3)),
            dir: hampi_dir(Vec3::X),
            seconds: 4.0,
            pass: Box::new(|_, _, hi| hi > 4.3),
            want: "up on the cloister roof (feet > 4.3)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "hampi bazaar roof stair".into(),
            start: hampi_at(Vec3::new(12.4, HALF_HEIGHT + 0.4, -57.6)),
            dir: hampi_dir(Vec3::Z),
            seconds: 2.4,
            pass: Box::new(|_, _, hi| hi > 4.9),
            want: "up on the gallery roof (feet > 4.9)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Down the stepped tank's flights to the pool's edge, then (separately) back up.
            name: "hampi stepped tank down".into(),
            start: hampi_at(Vec3::new(-40.0, 0.0, -2.0)) + Vec3::new(10.5, HALF_HEIGHT + 0.3, 0.0),
            dir: Vec3::NEG_X,
            seconds: 4.0,
            pass: Box::new(|_, lo, _| lo < -3.1),
            want: "down at the bottom tier (feet < -3.1)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "hampi stepped tank up".into(),
            start: hampi_at(Vec3::new(-40.0, 0.0, -2.0)) + Vec3::new(3.3, -3.18 + HALF_HEIGHT + 0.3, 0.0),
            dir: Vec3::X,
            seconds: 4.0,
            pass: Box::new(|p, _, _| p.y - HALF_HEIGHT > -0.1),
            want: "back up on the rim (feet > -0.1)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Kashi: from the deck at the top of a ghat down all three flights into the river.
            name: "kashi ghat down to the river".into(),
            start: Vec3::new(153.5, 5.02 + HALF_HEIGHT + 0.3, 16.7), // between two landing slots
            dir: Vec3::X,
            seconds: 5.0,
            pass: Box::new(|p, _, _| p.x > 176.0 && p.y - HALF_HEIGHT < -1.4),
            want: "on the river bed past the last step (x > 176)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "kashi river up a ghat".into(),
            start: Vec3::new(176.3, -1.6 + HALF_HEIGHT + 0.3, 47.65), // inside the moored boats
            dir: Vec3::NEG_X,
            seconds: 7.0,
            pass: Box::new(|p, _, _| p.x < 152.0 && p.y - HALF_HEIGHT > 4.9),
            want: "up on the town bank (x < 152, feet > 4.9)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Out along a burj to its prow, between the chhatri's pillars.
            name: "kashi burj prow".into(),
            start: Vec3::new(150.0, 5.0 + HALF_HEIGHT + 0.3, 65.5),
            dir: Vec3::X,
            seconds: 4.0,
            // (One burj per map keeps a chest at its prow.)
            pass: Box::new(|p, lo, _| p.x > 169.5 && lo > 4.9),
            want: "out at the prow, never off the burj (x > 169.5)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // From the char-bagh up the steps, through the arcade's middle bays to the back wall.
            name: "fort palace wing hall".into(),
            start: Vec3::new(131.0, 18.0 + HALF_HEIGHT + 0.3, -139.0),
            dir: Vec3::X,
            seconds: 3.0,
            pass: Box::new(|p, _, _| p.x > 138.5 && p.y - HALF_HEIGHT > 18.55),
            want: "in the east wing's hall (x > 138.5, on the plinth)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "fort palace wing back stair".into(),
            start: Vec3::new(142.5, 18.0 + HALF_HEIGHT + 0.3, -133.5),
            dir: Vec3::NEG_Z,
            seconds: 1.7,
            pass: Box::new(|p, _, _| p.y - HALF_HEIGHT > 22.8),
            want: "up the back stair, on its landing (feet > 22.8)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            name: "fort palace wing landing to the roof".into(),
            start: Vec3::new(142.5, 22.9 + HALF_HEIGHT + 0.3, -142.1),
            dir: Vec3::NEG_X,
            seconds: 2.0,
            pass: Box::new(|p, lo, _| p.x < 139.0 && lo > 22.8),
            want: "through the parapet's gap onto the terrace (x < 139)".into(),
            track: everywhere(),
            hurts: false,
        },
        Case {
            // Over the west rim: a 21 m drop to the bottom floor (the stairs above must not hurt).
            name: "stepwell jump from the rim".into(),
            start: ground(-147.0, 101.0),
            dir: Vec3::X,
            seconds: 3.0,
            pass: Box::new(|p, _, _| p.y < -20.0 + HALF_HEIGHT + 0.5),
            want: "on the bottom floor, hurt".into(),
            track: everywhere(),
            hurts: true,
        },
        Case {
            name: "stupa stair to the medhi".into(),
            start: ground(-178.0, 8.0),
            dir: Vec3::X,
            seconds: 3.0,
            pass: Box::new(|p, _, _| p.y > 3.5 && p.x > -168.5),
            want: "up on the drum terrace (y > 3.5)".into(),
            track: everywhere(),
            hurts: false,
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
            hurts: false,
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
            hurts: false,
        });
    }

    // Havelis: from the street through the gateway into the court (the tulsi platform marks its
    // centre; the gate ornament gives the facing).
    let tulsis: Vec<Vec3> = layout.props.iter().filter(|p| p.model.contains("lantana") && p.pos.y - layout.terrain.height(p.pos.x, p.pos.z) > 0.6 && p.pos.x.abs() < 70.0 && p.pos.z.abs() < 40.0).map(|p| p.pos).collect();
    for (i, c) in tulsis.iter().enumerate() {
        // The gateway: the widest arched surround on this haveli.
        let Some(gate) = layout.ornaments.iter().filter(|o| o.model.contains("arch_") && o.pos.distance(*c) < 12.0).max_by(|a, b| a.scale.x.total_cmp(&b.scale.x)) else { continue };
        let inward = gate.rot * Vec3::NEG_Z;
        let hd = (gate.pos - *c).with_y(0.0).length();
        println!("haveli {} centre ({:.1}, {:.1}) facing ({:.2}, {:.2}) half depth {:.1}", i + 1, c.x, c.z, -inward.x, -inward.z, hd);
        let center = c.with_y(0.0);
        cases.push(Case {
            name: format!("into haveli {} court", i + 1),
            start: ground(gate.pos.x, gate.pos.z) - inward * 3.0,
            dir: inward,
            seconds: 2.5,
            // Stopped by the tulsi platform in the middle of the court.
            pass: Box::new(move |p, _, _| p.with_y(0.0).distance(center) < 2.2),
            want: "at the tulsi platform".into(),
            track: everywhere(),
            hurts: false,
        });
        // The stair flights in the back-left (see `haveli`): the jharokhas sit at +-0.3 w.
        let Some(jh) = layout.ornaments.iter().filter(|o| o.model.contains("jharokha") && o.pos.distance(gate.pos) < 12.0).map(|o| o.pos.with_y(0.0).distance(gate.pos.with_y(0.0))).reduce(f32::max) else { continue };
        let (hw, t, run, lane) = (jh / 0.3 / 2.0, 0.35, 5.0, 1.15);
        let sx = -hw + t + 0.4;
        let lane_z = |k: f32| -hd + t + 0.15 + lane / 2.0 + k * (lane + 0.1);
        let local = move |p: Vec3| center + gate.rot * p;
        let east = gate.rot * Vec3::X;
        let first = local(Vec3::new(sx - 0.7, 0.0, lane_z(0.0)));
        cases.push(Case {
            name: format!("haveli {} stair to floor 1", i + 1),
            start: ground(first.x, first.z),
            dir: east,
            seconds: 1.6,
            pass: Box::new(|_, _, hi| hi > 3.3),
            want: "up on the first floor (feet > 3.3)".into(),
            track: everywhere(),
            hurts: false,
        });
        let second = local(Vec3::new(sx + run + 0.7, 3.4 + 1.2, lane_z(1.0)));
        cases.push(Case {
            name: format!("haveli {} stair to roof", i + 1),
            start: second,
            dir: -east,
            seconds: 1.8,
            pass: Box::new(|_, _, hi| hi > 6.7),
            want: "up in the barsati on the roof (feet > 6.7)".into(),
            track: everywhere(),
            hurts: false,
        });
    }

    // Static geometry only: doors are opened by players, not walked through.
    let doors: Vec<Entity> = app.world_mut().query_filtered::<Entity, With<shared::sim::Door>>().iter(app.world()).collect();
    for d in doors {
        app.world_mut().despawn(d);
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
                Health::default(),
                Inventory::new("khadga"),
                shared::sim::Warrior { name: "walker".into(), is_bot: false, look: 0, kills: 0 },
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
        let fall = std::mem::take(&mut app.world_mut().resource_mut::<Falls>().0);
        let ok = (c.pass)(p, lo, hi) && (fall > 0.0) == c.hurts;
        failed += (!ok) as u32;
        println!(
            "{} {:<32} start ({:.1},{:.1},{:.1}) end ({:.1},{:.1},{:.1}) feet {:.2}..{:.2} fall {:.0}  want {}",
            if ok { "PASS" } else { "FAIL" }, c.name, c.start.x, c.start.y, c.start.z, p.x, p.y, p.z, lo, hi, fall, c.want
        );
        app.world_mut().despawn(id);
    }
    println!("\n{failed} failed");
    std::process::exit(failed as i32);
}
