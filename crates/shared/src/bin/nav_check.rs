//! Headless check of the bots' street grid on a map: how much is walkable, which chests can be
//! reached, whether every Aandhi circle can be walked to, and what a path costs.
//!
//! VIVAD_MAP=cp cargo run --release -p shared --bin nav_check -- [out.pgm]

use std::time::{Duration, Instant};

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use rand::{RngExt, SeedableRng};
use rand::rngs::SmallRng;
use shared::map::MapLayout;
use shared::sim::nav::{NavGrid, NoPath};
use shared::sim::zone::Zone;
use shared::sim::{MatchConfig, SimPlugin};

#[derive(Default)]
struct Tally {
    ok: u32,
    there: u32,
    here: u32,
    far: f32,
    worst: f32,
    secs: f32,
}

impl Tally {
    fn add(&mut self, nav: &NavGrid, a: Vec3, b: Vec3) {
        let t = Instant::now();
        let r = nav.path(a, b);
        let s = t.elapsed().as_secs_f32();
        self.secs += s;
        self.worst = self.worst.max(s);
        match r {
            Ok(path) => {
                self.ok += 1;
                let mut at = a.xz();
                let mut len = 0.0;
                for p in &path {
                    len += at.distance(*p);
                    at = *p;
                }
                self.far += len / a.xz().distance(b.xz()).max(1.0);
            }
            Err(NoPath::There) => self.there += 1,
            Err(NoPath::Here) => self.here += 1,
        }
    }

    fn print(&self, what: &str) {
        let n = self.ok + self.there + self.here;
        println!(
            "{what}: {}/{n} ok, {} no-way-there, {} lost, detour x{:.2}, mean {:.2} ms, worst {:.1} ms",
            self.ok, self.there, self.here, self.far / self.ok.max(1) as f32,
            1000.0 * self.secs / n.max(1) as f32, 1000.0 * self.worst
        );
    }
}

fn main() {
    let items = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data/items.ron")).unwrap();
    let out = std::env::args().nth(1);
    let mut app = App::new();
    app.insert_resource(MatchConfig { bots: 0, ..default() })
        .add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, bevy::diagnostic::DiagnosticsPlugin))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(SimPlugin { items_ron: items })
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)));
    app.finish();
    app.cleanup();
    let t0 = Instant::now();
    let mut ticks = 0;
    while !app.world().resource::<NavGrid>().ready() {
        app.update();
        ticks += 1;
        assert!(ticks < 5000, "nav grid never finished");
    }
    println!("grid ready after {ticks} ticks, {:.1} s wall", t0.elapsed().as_secs_f32());
    app.world_mut()
        .run_system_once(move |nav: Res<NavGrid>, layout: Res<MapLayout>| {
            let half = layout.half_extent;
            let n = (2.0 * half) as usize;
            let mut rng = SmallRng::seed_from_u64(1);
            let mut free = vec![];
            let mut pgm = format!("P5\n{n} {n}\n255\n").into_bytes();
            for j in 0..n {
                for i in 0..n {
                    let p = Vec2::new(i as f32 + 0.5 - half, j as f32 + 0.5 - half);
                    let w = nav.walkable(p);
                    if w {
                        free.push(p);
                    }
                    pgm.push(if w { 255 } else { 0 });
                }
            }
            println!("walkable {:.1}%", 100.0 * free.len() as f32 / (n * n) as f32);
            for c in &layout.chests {
                let p = (c.pos.xz() + Vec2::splat(half)).as_uvec2();
                let k = (p.y as usize * n + p.x as usize).min(n * n - 1) + pgm.len() - n * n;
                pgm[k] = 128;
            }
            if let Some(out) = &out {
                std::fs::write(out, pgm).unwrap();
            }
            let (joined, stairs) = nav.stair_count();
            println!("stairs joined {joined} of {stairs}");
            for (foot, head) in nav.stair_ends() {
                println!("  stair {:.1},{:.1},{:.1} -> {:.1},{:.1},{:.1}", foot.x, foot.y, foot.z, head.x, head.y, head.z);
            }
            // SKETCH=level,x0,z0,x1,z1 draws that part of the grid.
            if let Ok(s) = std::env::var("SKETCH") {
                let v: Vec<f32> = s.split(',').map(|x| x.parse().unwrap()).collect();
                println!("{}", nav.sketch(v[0] as usize, Vec2::new(v[1], v[2]), Vec2::new(v[3], v[4])));
            }
            let pick = |rng: &mut SmallRng| free[rng.random_range(0..free.len())].extend(0.9).xzy();
            // From the park's middle to every chest.
            let start = layout.chests.iter().map(|c| c.pos).find(|p| p.length() < 100.0).unwrap_or(Vec3::ZERO);
            let (mut ground, mut upper) = (Tally::default(), Tally::default());
            let mut low = 0;
            for c in &layout.chests {
                if c.pos.y > 2.5 {
                    upper.add(&nav, start, c.pos);
                } else {
                    low += 1;
                    let before = ground.there + ground.here;
                    ground.add(&nav, start, c.pos);
                    if ground.there + ground.here > before {
                        println!("  no way to the chest at {:.0},{:.0}", c.pos.x, c.pos.z);
                    }
                }
            }
            ground.print(&format!("{low} ground chests from the park"));
            upper.print("upstairs chests, by the stairs");
            let mut anywhere = Tally::default();
            for _ in 0..300 {
                let (a, b) = (pick(&mut rng), pick(&mut rng));
                anywhere.add(&nav, a, b);
            }
            anywhere.print("anywhere to anywhere");
            // Every Aandhi circle of a dozen matches, from a warrior outside it.
            let mut zones = Tally::default();
            let mut buried = 0;
            let mut circles = 0;
            for seed in 0..12 {
                let mut z = Zone::for_extent(1000 + seed, half);
                let mut phase = 99;
                for _ in 0..4000 {
                    z.advance(0.5);
                    if z.phase != phase {
                        phase = z.phase;
                        circles += 1;
                        buried += (!nav.walkable(z.target_center)) as u32;
                        for _ in 0..8 {
                            zones.add(&nav, pick(&mut rng), z.target_center.extend(0.0).xzy());
                        }
                    }
                }
            }
            println!("{circles} circles, {buried} centred inside something");
            zones.print("anywhere to a circle's centre");
        })
        .unwrap();
}
