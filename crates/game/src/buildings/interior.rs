//! The hollow buildings drawn (shared::map::interiors). Their paint is the same as every building's,
//! with real holes cut where it shows windows, a door and open shops; behind that, all of a mid
//! cell's rooms are one mesh in one material, in the near tier only, so a street of rooms adds a
//! draw and not a draw a room. The mid and far shells never change.
//!
//! The material is one atlas of tiling strips (tools/textures/room_atlas.py). A strip repeats along
//! its length, so a wall is one quad from end to end; floors are cut into strips as wide as the
//! atlas's row. Colour comes from the vertices, as on every building here.
//!
//! Three things live near the camera only: the furniture of the buildings within reach, sharing the
//! room kit's meshes; a pool of door leaves that swing open for whoever comes near (no collider and
//! no network: bullets pass, and each peer swings its own); and a few shadowless fill lights.

use std::collections::HashMap;

use bevy::asset::AssetPath;
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use shared::map::build::subtract_rect;
use shared::map::interiors::{Cut, Interior, LANE, Piece, Plan, Room, SHELL, SLAB, STAIR_ROOM, Wall, WallKind, rects};
use shared::map::{Footprint, MapLayout};
use shared::sim::{Eliminated, Health, Kinematic, LocalPlayer, MoveMode, OfMap, PlayerIntent, Warrior, map_reloaded};

use super::{MeshBuf, Paint, PendingMips, TILE, hash01, lin, tiers};
use crate::batch::probe::Tagged;
use crate::camera::MainCamera;
use crate::characters::LodRange;
use crate::quality::LodShadow;

/// How far from the camera a building's furniture stands.
const FURNITURE: f32 = 45.0;
/// Door leaves, in pairs, and how close a warrior brings one open.
const DOORS: usize = 24;
const DOOR_NEAR: f32 = 3.2;
const DOOR_SEEN: f32 = 45.0;
/// The fill lights.
const FILLS: usize = 4;
const FILL_LUMENS: f32 = 90_000.0;
const FILL_NEAR: f32 = 16.0;
/// How much of the sky's light reaches a room, of 255.
const SKY_INDOORS: u8 = 96;
/// Metres the wall strip repeats over, and how tall a row of the atlas is.
const STRIP: f32 = 6.4;
const ROW_M: f32 = 3.2;
/// The atlas's rows (tools/textures/room_atlas.py).
const WALL: usize = 0;
const OLD_WALL: usize = 1;
const MOSAIC: usize = 2;
const TILES: usize = 3;
const MARBLE: usize = 4;
const CONCRETE: usize = 5;
const CEILING: usize = 6;
const ROWS: usize = 7;

pub struct InteriorPlugin;

impl Plugin for InteriorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Furniture>()
            .init_resource::<Doors>()
            .add_systems(Update, (reset.run_if(map_reloaded), load_furniture, stream_furniture, swing_doors, place_fills, turn_fans).chain());
        if let Ok(spec) = std::env::var("VIVAD_ROOM_WALK") {
            let (dir, which) = spec.split_once(',').map_or((spec.as_str(), ""), |(d, b)| (d, b));
            app.insert_resource(RoomWalk { dir: dir.to_owned(), building: which.parse().ok(), ring: which == "ring", way: vec![], next: 0, wait: 0.0, shot: 0.0, frame: 0, turn: 0.0 })
                .add_systems(PreUpdate, walk_player.after(crate::input::LocalInput));
        }
    }
}

/// VIVAD_ROOM_WALK=<dir>[,<hollow building>|ring]: the proof of the rooms from the player's own seat.
/// The player is stood in the street outside a Chandni Chowk shop (or that building, or one of
/// Connaught Place's furnished ring shops), and walks in at the door, through the rooms, up the
/// stairs and out on the roof on its own feet, then turns round to look: the 3D view is saved every
/// half second, so it works with the window hidden.
#[derive(Resource)]
struct RoomWalk {
    dir: String,
    building: Option<usize>,
    ring: bool,
    way: Vec<(Vec2, f32)>,
    next: usize,
    wait: f32,
    shot: f32,
    frame: u32,
    turn: f32,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn walk_player(
    mut commands: Commands,
    mut walk: ResMut<RoomWalk>,
    layout: Res<MapLayout>,
    floor: Res<shared::sim::nav::StreetFloor>,
    view: Res<crate::quality::View3d>,
    time: Res<Time<Real>>,
    pending: Option<Res<PendingMips>>,
    mut q: Query<(&mut Transform, &mut Kinematic, &mut PlayerIntent, &mut Health), With<LocalPlayer>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok((mut tf, mut kin, mut intent, mut health)) = q.single_mut() else { return };
    let dt = time.delta_secs();
    health.hp = 100.0;
    intent.move_axis = Vec2::ZERO;
    intent.pitch = -0.05;
    intent.held = 0;
    if pending.as_ref().is_some_and(|p| !p.0.is_empty()) {
        walk.wait = walk.wait.max(9.0);
    }
    if walk.way.is_empty() {
        let shop = |r: &&Interior| r.plan == Plan::Shop && r.storeys >= 3 && (r.corner - shared::map::compose::PLACES[1].at).abs().max_element() < 250.0;
        let chosen = walk.building.and_then(|k| layout.rooms.get(k)).or(layout.rooms.iter().find(shop));
        let ring = layout.ring_pieces.get(12).filter(|_| walk.ring).map(|p| {
            let out = Quat::from_rotation_y(p.yaw) * Vec3::NEG_Z;
            let at = |d: f32| ((p.pos + out * d).xz(), p.pos.y);
            vec![at(11.0), at(2.2)]
        });
        let Some(way) = ring.or(chosen.and_then(|r| r.way_up(&floor))) else { return };
        info!("room walk: {} points, {:?}", way.len(), chosen.map(|r| (r.plan, r.corner, r.storeys)));
        walk.way = way;
        walk.next = 1;
        walk.wait = 14.0;
    }
    let start = walk.way[0];
    if walk.wait > 0.0 {
        // Held at the start while the near tier comes in around it.
        walk.wait -= dt;
        tf.translation = Vec3::new(start.0.x, start.1 + shared::sim::HALF_HEIGHT + 0.05, start.0.y);
        kin.mode = MoveMode::Ground;
        kin.velocity = Vec3::ZERO;
        let d = walk.way[1].0 - start.0;
        intent.yaw = (-d.x).atan2(-d.y);
        return;
    }
    walk.shot -= dt;
    if walk.shot <= 0.0 {
        walk.shot = 0.5;
        let path = format!("{}/walk_{:03}.png", walk.dir, walk.frame);
        walk.frame += 1;
        commands.spawn(Screenshot::image(view.image.clone())).observe(save_to_disk(path));
    }
    if walk.next < walk.way.len() {
        let d = walk.way[walk.next].0 - tf.translation.xz();
        if d.length() < 0.45 {
            walk.next += 1;
            return;
        }
        // Turns as quickly as a player's hand, stopping to do so at a sharp corner.
        let want = (-d.x).atan2(-d.y);
        let gap = (want - intent.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        intent.yaw += gap * (dt * 14.0).min(1.0);
        intent.move_axis = if gap.abs() < 0.6 { Vec2::Y } else { Vec2::ZERO };
        return;
    }
    // Out on the roof: a slow look round.
    walk.turn += dt;
    intent.yaw += dt * 1.3;
    if walk.turn > 5.2 {
        info!("room walk done: {} frames", walk.frame);
        exit.write(AppExit::Success);
    }
}

/// A band of a wall's paint with its holes left out: `a` and `dir` run along the wall.
#[allow(clippy::too_many_arguments)]
pub(super) fn skin(w: &mut MeshBuf, cuts: &[Cut], (a, dir, len): (Vec2, Vec3, f32), (lo, hi): (f32, f32), u: (f32, f32), v: (f32, f32), col: ([f32; 4], [f32; 4]), out: Vec3) {
    let start = Vec3::new(a.x, 0.0, a.y);
    let at = |s: f32, y: f32| start + dir * s + Vec3::Y * y;
    let uv = |s: f32, y: f32| Vec2::new(u.0 + (u.1 - u.0) * s / len, v.0 + (v.1 - v.0) * (y - lo) / (hi - lo));
    let tint = |y: f32| mix(col.0, col.1, (y - lo) / (hi - lo));
    for [s0, s1, y0, y1] in rects((0.0, len), (lo, hi), cuts) {
        facing(w, [at(s0, y0), at(s1, y0), at(s1, y1), at(s0, y1)], out, [uv(s0, y0), uv(s1, y0), uv(s1, y1), uv(s0, y1)], [tint(y0), tint(y0), tint(y1), tint(y1)]);
    }
}

/// A hollow building's roof, with the stairs coming up through it.
pub(super) fn roof(buf: &mut MeshBuf, r: &Interior, y: f32, col: [f32; 4]) {
    let whole = (Vec2::ZERO, Vec2::new(r.w, r.d));
    let parts = match r.stair {
        Some(s) => subtract_rect(whole, s.well(r.storeys - 1)),
        None => vec![whole],
    };
    for (a, b) in parts {
        let p = [(a.x, a.y), (b.x, a.y), (b.x, b.y), (a.x, b.y)].map(|(x, z)| r.at(x, y, z));
        facing(buf, p, Vec3::Y, p.map(|q| Vec2::new(q.x, q.z) / TILE), [col; 4]);
    }
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 1.0]
}

fn times(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

/// A quad of one normal, whichever way round its corners came.
fn facing(buf: &mut MeshBuf, p: [Vec3; 4], n: Vec3, uv: [Vec2; 4], col: [[f32; 4]; 4]) {
    if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) >= 0.0 {
        buf.quad_col(p, n, uv, col);
    } else {
        buf.quad_col([p[1], p[0], p[3], p[2]], n, [uv[1], uv[0], uv[3], uv[2]], [col[1], col[0], col[3], col[2]]);
    }
}

/// Where on the atlas `frac` of the way up a row is.
fn row_v(row: usize, frac: f32) -> f32 {
    const PAD: f32 = 0.004;
    (row as f32 + 1.0 - PAD - frac.clamp(0.0, 1.0) * (1.0 - 2.0 * PAD)) / ROWS as f32
}

/// What a room is painted with: its walls' row and colour, and its floor's.
struct Look {
    wall_row: usize,
    wall: [f32; 3],
    floor_row: usize,
    floor: [f32; 3],
}

fn look(room: &Room) -> Look {
    // Distemper, as Delhi's rooms are done every Diwali: sky blue, pink, mint, lemon, lilac, cream.
    const DISTEMPER: [[f32; 3]; 6] = [[0.74, 0.84, 0.90], [0.93, 0.78, 0.76], [0.74, 0.88, 0.78], [0.96, 0.90, 0.68], [0.84, 0.79, 0.90], [0.96, 0.91, 0.80]];
    const OFFICE: [[f32; 3]; 3] = [[0.92, 0.91, 0.87], [0.84, 0.88, 0.89], [0.94, 0.90, 0.80]];
    const SHOP: [[f32; 3]; 4] = [[0.96, 0.93, 0.84], [0.78, 0.88, 0.82], [0.96, 0.87, 0.68], [0.80, 0.86, 0.92]];
    let t = room.tone as usize;
    match room.plan {
        _ if room.shaft => Look { wall_row: OLD_WALL, wall: [1.0, 0.97, 0.90], floor_row: MOSAIC, floor: [0.86, 0.84, 0.80] },
        Plan::Flat => Look { wall_row: WALL, wall: DISTEMPER[t % 6], floor_row: MOSAIC, floor: [1.0, 0.97, 0.92] },
        Plan::Office => Look { wall_row: WALL, wall: OFFICE[t % 3], floor_row: MARBLE, floor: [1.0, 0.98, 0.94] },
        Plan::Shop => Look { wall_row: WALL, wall: SHOP[t % 4], floor_row: TILES, floor: [1.0, 0.97, 0.92] },
        Plan::Dhaba => Look { wall_row: WALL, wall: [0.93, 0.84, 0.60], floor_row: TILES, floor: [0.9, 0.86, 0.8] },
        Plan::Godown => Look { wall_row: OLD_WALL, wall: [0.95, 0.91, 0.84], floor_row: CONCRETE, floor: [0.98, 0.94, 0.86] },
        Plan::Lobby => Look { wall_row: WALL, wall: [0.94, 0.91, 0.84], floor_row: MARBLE, floor: [1.0, 1.0, 0.98] },
    }
}

/// A stretch of wall face. `at` is its plane across the wall's axis, `shade` the light low and high
/// on it.
#[allow(clippy::too_many_arguments)]
fn face(buf: &mut MeshBuf, r: &Interior, along_x: bool, at: f32, s: (f32, f32), y: (f32, f32), n: Vec3, cuts: &[Cut], row: usize, tint: [f32; 4], shade: (f32, f32), offset: f32) {
    let p = |a: f32, h: f32| if along_x { r.at(a, h, at) } else { r.at(at, h, a) };
    let uv = |a: f32, h: f32| Vec2::new(a / STRIP + offset, row_v(row, (h - y.0) / ROW_M));
    let col = |h: f32| times(tint, shade.0 + (shade.1 - shade.0) * (h - y.0) / (y.1 - y.0));
    for [s0, s1, y0, y1] in rects(s, y, cuts) {
        facing(buf, [p(s0, y0), p(s1, y0), p(s1, y1), p(s0, y1)], n, [uv(s0, y0), uv(s1, y0), uv(s1, y1), uv(s0, y1)], [col(y0), col(y0), col(y1), col(y1)]);
    }
}

/// A rectangle of floor or ceiling at height `y`, cut into strips as wide as the atlas's row.
fn floor_quad(buf: &mut MeshBuf, r: &Interior, x: (f32, f32), z: (f32, f32), y: f32, n: Vec3, row: usize, col: [f32; 4]) {
    let mut from = z.0;
    while from < z.1 - 1e-3 {
        let to = ((from / ROW_M + 1e-3).floor() + 1.0).mul_add(ROW_M, 0.0).min(z.1);
        let v = |q: f32| {
            let f = (q / ROW_M).fract();
            row_v(row, if f < 1e-4 && q > from { 1.0 } else { f })
        };
        let p = [r.at(x.0, y, from), r.at(x.1, y, from), r.at(x.1, y, to), r.at(x.0, y, to)];
        let uv = [Vec2::new(x.0 / STRIP, v(from)), Vec2::new(x.1 / STRIP, v(from)), Vec2::new(x.1 / STRIP, v(to)), Vec2::new(x.0 / STRIP, v(to))];
        facing(buf, p, n, uv, [col; 4]);
        from = to;
    }
}

/// The sides of every hole, through the wall's thickness.
fn reveals(buf: &mut MeshBuf, r: &Interior, wall: &Wall) {
    let (t0, t1) = (wall.at - wall.thick / 2.0, wall.at + wall.thick / 2.0);
    let p = |s: f32, y: f32, t: f32| if wall.along_x { r.at(s, y, t) } else { r.at(t, y, s) };
    let along = (p(1.0, 0.0, t0) - p(0.0, 0.0, t0)).normalize();
    let tint = lin([0.92, 0.88, 0.80]);
    let v = row_v(CEILING, 0.5);
    let uv = [Vec2::new(0.0, v), Vec2::new(0.1, v), Vec2::new(0.1, v + 0.02), Vec2::new(0.0, v + 0.02)];
    for c in &wall.cuts {
        let (dark, lit) = (times(tint, 0.8), times(tint, 1.0));
        facing(buf, [p(c.s0, c.y0, t0), p(c.s0, c.y0, t1), p(c.s0, c.y1, t1), p(c.s0, c.y1, t0)], along, uv, [dark, dark, lit, lit]);
        facing(buf, [p(c.s1, c.y0, t0), p(c.s1, c.y0, t1), p(c.s1, c.y1, t1), p(c.s1, c.y1, t0)], -along, uv, [dark, dark, lit, lit]);
        facing(buf, [p(c.s0, c.y1, t0), p(c.s1, c.y1, t0), p(c.s1, c.y1, t1), p(c.s0, c.y1, t1)], Vec3::NEG_Y, uv, [times(tint, 0.7); 4]);
        if c.y0 > r.floor + 0.2 {
            facing(buf, [p(c.s0, c.y0, t0), p(c.s1, c.y0, t0), p(c.s1, c.y0, t1), p(c.s0, c.y0, t1)], Vec3::Y, uv, [times(tint, 1.0); 4]);
        }
    }
}

/// The inside of the paint, plain, just behind the room's wall. A third-person camera squeezed along
/// a stair wall has its near plane through that wall, and past it there would otherwise be nothing
/// but the outdoors: this way a wall stays a wall from any seat.
fn shell_cap(buf: &mut MeshBuf, r: &Interior, wall: &Wall) {
    let size = if wall.along_x { r.d } else { r.w };
    let (at, sign) = if wall.at < size / 2.0 { (0.02, 1.0) } else { (size - 0.02, -1.0) };
    let inward = if wall.along_x { r.z } else { r.x } * sign;
    let tint = times(lin([0.72, 0.70, 0.66]), 1.0);
    face(buf, r, wall.along_x, at, wall.s, wall.y, Vec3::new(inward.x, 0.0, inward.y), &wall.cuts, CEILING, tint, (1.0, 1.0), 0.0);
}

/// A wall that belongs to no room: both its faces and its ends. The stair room is in the building's
/// own paint, over plain plaster; the spine between the flights is the stair well's old wall.
fn free_wall(buf: &mut MeshBuf, r: &Interior, wall: &Wall, outside: [f32; 4], inner: [f32; 4]) {
    let (row, tint) = if wall.y.0 >= r.roof - 0.01 { (CEILING, outside) } else { (OLD_WALL, inner) };
    let t = wall.thick / 2.0;
    let side = if wall.along_x { r.z } else { r.x };
    let side = Vec3::new(side.x, 0.0, side.y);
    for (at, n) in [(wall.at - t, -side), (wall.at + t, side)] {
        let mut from = wall.y.0;
        while from < wall.y.1 - 1e-3 {
            let to = (from + ROW_M).min(wall.y.1);
            face(buf, r, wall.along_x, at, wall.s, (from, to), n, &wall.cuts, row, tint, (0.9, 1.0), 0.0);
            from = to;
        }
    }
    reveals(buf, r, wall);
    let along = if wall.along_x { r.x } else { r.z };
    let along = Vec3::new(along.x, 0.0, along.y);
    let p = |s: f32, y: f32, t: f32| if wall.along_x { r.at(s, y, t) } else { r.at(t, y, s) };
    let v = row_v(CEILING, 0.5);
    let uv = [Vec2::new(0.0, v), Vec2::new(0.02, v), Vec2::new(0.02, v + 0.02), Vec2::new(0.0, v + 0.02)];
    for (s, n) in [(wall.s.0, -along), (wall.s.1, along)] {
        let (a, b) = (wall.at - t, wall.at + t);
        facing(buf, [p(s, wall.y.0, a), p(s, wall.y.0, b), p(s, wall.y.1, b), p(s, wall.y.1, a)], n, uv, [times(tint, 0.9); 4]);
    }
}

/// The steps, on the ramp the warrior walks: the ramp goes through each tread's middle, so feet are
/// never more than half a step from it.
fn flight(buf: &mut MeshBuf, r: &Interior, k: u32) {
    let Some(s) = r.stair else { return };
    let (foot, head) = s.flight(k);
    let (y0, y1) = (r.level(k), r.level(k + 1));
    let z = (s.lane(k) - LANE / 2.0, s.lane(k) + LANE / 2.0);
    let steps = ((y1 - y0) / 0.21).round() as i32;
    let (run, rise) = ((head - foot) / steps as f32, (y1 - y0) / steps as f32);
    let along = r.x * run.signum();
    let along = Vec3::new(along.x, 0.0, along.y);
    let (tread, riser) = (lin([0.97, 0.94, 0.88]), lin([0.80, 0.77, 0.71]));
    let v = row_v(MOSAIC, 0.3);
    let uv = |a: f32, b: f32| [Vec2::new(a, v), Vec2::new(b, v), Vec2::new(b, v - 0.012), Vec2::new(a, v - 0.012)];
    for i in 0..steps {
        let (a, b) = (foot + run * i as f32, foot + run * (i + 1) as f32);
        let (lo, hi) = (y0 + rise * (i as f32 - 0.5).max(0.0), y0 + rise * (i as f32 + 0.5));
        facing(buf, [r.at(a, lo, z.0), r.at(a, lo, z.1), r.at(a, hi, z.1), r.at(a, hi, z.0)], -along, uv(0.0, LANE / STRIP), [riser; 4]);
        facing(buf, [r.at(a, hi, z.0), r.at(a, hi, z.1), r.at(b, hi, z.1), r.at(b, hi, z.0)], Vec3::Y, uv(a / STRIP, (a + LANE) / STRIP), [tread; 4]);
    }
    // The edges of the floor it comes up through, at both ends of the well.
    let edge = lin([0.70, 0.67, 0.62]);
    for (x, n) in [(head, -along), (foot, along)] {
        let p = [r.at(x, y1 - SLAB, z.0), r.at(x, y1 - SLAB, z.1), r.at(x, y1, z.1), r.at(x, y1, z.0)];
        facing(buf, p, n, uv(0.0, LANE / STRIP), [edge; 4]);
    }
    // Its underside, from below.
    let under = lin([0.55, 0.53, 0.50]);
    let drop = 0.25;
    let p = [r.at(foot, y0 - drop, z.0), r.at(foot, y0 - drop, z.1), r.at(head, y1 - drop, z.1), r.at(head, y1 - drop, z.0)];
    let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
    let n = if n.y > 0.0 { -n } else { n };
    facing(buf, p, n, uv(0.0, 0.1), [under; 4]);
}

/// All of a building's insides, into its cell's mesh.
fn paint(buf: &mut MeshBuf, r: &Interior, fp: &Footprint) {
    let c = fp.pts.iter().sum::<Vec2>() / fp.pts.len() as f32;
    let outside = Paint::of(c, fp.mat).wall_at(1.02);
    for room in &r.rooms {
        let (y0, top) = (r.level(room.storey), r.level(room.storey + 1));
        // A stair well's walls go on past its ceiling, behind the floor's edge.
        let y1 = if room.shaft { top } else { top - SLAB };
        let l = look(room);
        let wall = lin(l.wall);
        let lit = openings(r, room);
        // In from the street it's dimmer than out in it, and warmer: there's no sky in here but
        // what comes through the windows.
        let shade = if lit { (0.9, 1.0) } else { (0.62, 0.72) };
        let offset = hash01(room.tone, room.storey, 3);
        let sides = [
            (true, room.z.0, room.x, r.z),
            (false, room.x.1, room.z, -r.x),
            (true, room.z.1, room.x, -r.z),
            (false, room.x.0, room.z, r.x),
        ];
        for (along_x, at, s, into) in sides {
            let n = Vec3::new(into.x, 0.0, into.y);
            let cuts = r.cuts_on(along_x, at, s, (y0, y1));
            face(buf, r, along_x, at, s, (y0, y1), n, &cuts, l.wall_row, wall, shade, offset);
        }
        let floor = times(lin(l.floor), if lit { 1.0 } else { 0.7 });
        let ceiling = times(lin([0.93, 0.92, 0.88]), if lit { 0.9 } else { 0.62 });
        let y_floor = if room.storey == 0 { y0 + 0.025 } else { y0 };
        let y_ceiling = top - SLAB;
        if let (true, Some(s)) = (room.shaft, r.stair) {
            // A stair well's floor and ceiling each have the well through them.
            let whole = (Vec2::new(room.x.0, room.z.0), Vec2::new(room.x.1, room.z.1));
            let below = if room.storey > 0 { subtract_rect(whole, s.well(room.storey - 1)) } else { vec![whole] };
            for (a, b) in below {
                floor_quad(buf, r, (a.x, b.x), (a.y, b.y), y_floor, Vec3::Y, l.floor_row, floor);
            }
            for (a, b) in subtract_rect(whole, s.well(room.storey)) {
                floor_quad(buf, r, (a.x, b.x), (a.y, b.y), y_ceiling, Vec3::NEG_Y, CEILING, ceiling);
            }
        } else {
            floor_quad(buf, r, room.x, room.z, y_floor, Vec3::Y, l.floor_row, floor);
            floor_quad(buf, r, room.x, room.z, y_ceiling, Vec3::NEG_Y, CEILING, ceiling);
        }
    }
    let inner = lin([1.0, 0.97, 0.90]);
    for wall in &r.walls {
        match wall.kind {
            WallKind::Shell => {
                reveals(buf, r, wall);
                shell_cap(buf, r, wall);
            }
            WallKind::Inner => reveals(buf, r, wall),
            WallKind::Free => free_wall(buf, r, wall, outside, inner),
            WallKind::Hidden => {}
        }
    }
    for k in 0..r.storeys.min(if r.stair.is_some() { u32::MAX } else { 0 }) {
        flight(buf, r, k);
    }
    // The stair room's lid.
    if r.stair.is_some() {
        let lid = r.slabs.last().filter(|s| s.y > r.roof + STAIR_ROOM);
        if let Some(s) = lid {
            let (a, b) = (r.at(s.x.0, 0.0, s.z.0), r.at(s.x.1, 0.0, s.z.1));
            let c = (a + b) * 0.5 + Vec3::Y * (s.y - s.thick / 2.0);
            buf.solid_box(c, r.rot(), Vec3::new(s.x.1 - s.x.0, s.thick, s.z.1 - s.z.0), times(outside, 0.9));
        }
    }
    let _ = SHELL;
}

/// Whether a room has a window, door or shop bay of its own to the outside.
fn openings(r: &Interior, room: &Room) -> bool {
    let (y0, y1) = (r.level(room.storey), r.level(room.storey + 1) - SLAB);
    [(true, room.z.0, room.x), (true, room.z.1, room.x), (false, room.x.0, room.z), (false, room.x.1, room.z)]
        .iter()
        .any(|&(along_x, at, s)| r.walls.iter().filter(|w| w.kind == WallKind::Shell && w.along_x == along_x && (w.at - at).abs() <= SHELL / 2.0 + 0.03).any(|w| w.cuts.iter().any(|c| c.s1 > s.0 && c.s0 < s.1 && c.y1 > y0 && c.y0 < y1)))
}

fn room_mesh(buf: MeshBuf, origin: Vec3) -> Mesh {
    let pos: Vec<[f32; 3]> = buf.pos.iter().map(|p| (Vec3::from(*p) - origin).to_array()).collect();
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, tiers::usage())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, buf.nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, buf.uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, buf.col)
        .with_inserted_indices(Indices::U32(buf.idx));
    tiers::pack(&mut m);
    m
}

/// All the rooms: a mesh to a mid cell, in the near tier.
#[allow(clippy::too_many_arguments)]
pub(super) fn build(commands: &mut Commands, layout: &MapLayout, assets: &AssetServer, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, pending: &mut PendingMips, scale: f32) {
    if layout.rooms.is_empty() {
        return;
    }
    let atlas: Handle<Image> = assets.load("textures/rooms/room_atlas.png");
    pending.0.push(atlas.clone());
    // Only a little of the sky gets in here, and it is blue: most of a room's light is the sun
    // through its windows and the fills. A constant occlusion leaves those their strength, so the
    // walls can be their true colours and rooms deep in a building still go dim.
    let shut_in = images.add(Image::new_fill(
        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        &[SKY_INDOORS, SKY_INDOORS, SKY_INDOORS, 255],
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    ));
    let material = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(atlas), occlusion_texture: Some(shut_in), perceptual_roughness: 0.88, reflectance: 0.2, ..default() });
    let mut cells: HashMap<(i32, i32), MeshBuf> = HashMap::new();
    for r in &layout.rooms {
        let c = r.at(r.w / 2.0, 0.0, r.d / 2.0);
        paint(cells.entry(tiers::cell_at(c.x, c.z)).or_default(), r, &layout.footprints[r.footprint]);
    }
    let base = LodRange(VisibilityRange { start_margin: 0.0..0.0, end_margin: tiers::mid_range().0.start_margin.clone(), use_aabb: false }, 0);
    let mut tris = 0;
    for (cell, buf) in cells {
        tris += buf.idx.len() / 3;
        let origin = tiers::cell_centre(cell);
        commands.spawn((Mesh3d(meshes.add(room_mesh(buf, origin))), MeshMaterial3d(material.clone()), Transform::from_translation(origin), base.scaled(scale), base.clone(), LodShadow, Tagged("near"), OfMap));
    }
    info!("rooms: {} buildings, {tris} triangles", layout.rooms.len());
}

/// The room kit, and which buildings have their furniture out.
#[derive(Resource, Default)]
struct Furniture {
    file: Option<Handle<Gltf>>,
    material: Option<Handle<StandardMaterial>>,
    meshes: HashMap<String, Handle<Mesh>>,
    /// Pieces that come and go together: a building's, or a ring shop's.
    groups: Vec<Group>,
    live: HashMap<u32, Vec<Entity>>,
    /// Where the groups are, a list to every 64 m.
    cells: HashMap<(i32, i32), Vec<u32>>,
    frame: u32,
}

struct Group {
    middle: Vec2,
    /// How far its edge is from its middle.
    reach: f32,
    pieces: Vec<Piece>,
}

fn furniture_groups(layout: &MapLayout) -> Vec<Group> {
    let mut all: Vec<Group> = layout.rooms.iter().map(|r| Group { middle: r.at(r.w / 2.0, 0.0, r.d / 2.0).xz(), reach: r.w.max(r.d) * 0.5, pieces: r.pieces.clone() }).collect();
    for piece in &layout.ring_pieces {
        let at = piece.pos.xz();
        match all.last_mut().filter(|g| g.middle.distance(at) < 8.0) {
            Some(g) => g.pieces.push(*piece),
            None => all.push(Group { middle: at, reach: 4.0, pieces: vec![*piece] }),
        }
    }
    all
}

const NEAR_CELL: f32 = 64.0;

fn near_key(p: Vec2) -> (i32, i32) {
    ((p.x / NEAR_CELL).floor() as i32, (p.y / NEAR_CELL).floor() as i32)
}

/// The buildings within `reach` of a point: their indices.
fn around(cells: &HashMap<(i32, i32), Vec<u32>>, p: Vec2, reach: f32) -> Vec<u32> {
    let (a, b) = (near_key(p - reach), near_key(p + reach));
    (a.0..=b.0).flat_map(|i| (a.1..=b.1).map(move |j| (i, j))).filter_map(|k| cells.get(&k)).flatten().copied().collect()
}

fn index_rooms(layout: &MapLayout) -> HashMap<(i32, i32), Vec<u32>> {
    let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (k, r) in layout.rooms.iter().enumerate() {
        cells.entry(near_key(r.at(r.w / 2.0, 0.0, r.d / 2.0).xz())).or_default().push(k as u32);
    }
    cells
}

/// Everything goes with its map.
fn reset(mut furniture: ResMut<Furniture>, mut doors: ResMut<Doors>) {
    furniture.live.clear();
    furniture.groups.clear();
    furniture.cells.clear();
    *doors = Doors::default();
}

fn load_furniture(mut f: ResMut<Furniture>, layout: Res<MapLayout>, assets: Res<AssetServer>, gltfs: Res<Assets<Gltf>>, gltf_meshes: Res<Assets<GltfMesh>>, gltf_materials: Res<Assets<GltfMaterial>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if layout.rooms.is_empty() || !f.meshes.is_empty() {
        return;
    }
    if f.file.is_none() {
        f.file = Some(assets.load::<Gltf>(AssetPath::from("models/props/room_kit.glb")));
    }
    let Some(file) = f.file.as_ref().and_then(|h| gltfs.get(h)) else { return };
    let mut found = HashMap::new();
    for (name, handle) in &file.named_meshes {
        if let Some(mesh) = gltf_meshes.get(handle).and_then(|m| m.primitives.first()) {
            found.insert(name.to_string(), mesh.mesh.clone());
        }
    }
    let material = file.named_materials.get("room_kit").and_then(|h| gltf_materials.get(h)).map(|m| materials.add(bevy::pbr::gltf::standard_material_from_gltf_material(m)));
    f.material = material;
    f.meshes = found;
}

/// The furniture of the buildings within reach. By the building, and not by the cell, as there are
/// about 70 pieces in one: 15 near buildings is 1,000 entities, a whole near tier of cells 20,000.
/// The room kit's one material and its meshes draw as a few instanced batches.
fn stream_furniture(mut commands: Commands, mut f: ResMut<Furniture>, layout: Res<MapLayout>, camera: Query<&GlobalTransform, With<MainCamera>>) {
    f.frame = f.frame.wrapping_add(1);
    if layout.rooms.is_empty() || f.material.is_none() || f.frame % 12 != 0 {
        return;
    }
    if f.groups.is_empty() {
        f.groups = furniture_groups(&layout);
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (k, g) in f.groups.iter().enumerate() {
            cells.entry(near_key(g.middle)).or_default().push(k as u32);
        }
        f.cells = cells;
    }
    let Ok(camera) = camera.single() else { return };
    let at = camera.translation();
    let f = &mut *f;
    let groups = &f.groups;
    let near = |k: u32| {
        let g = &groups[k as usize];
        g.middle.distance(at.xz()) - g.reach
    };
    f.live.retain(|k, list| {
        if near(*k) < FURNITURE + 8.0 {
            return true;
        }
        for e in list.drain(..) {
            commands.entity(e).despawn();
        }
        false
    });
    let Some(material) = f.material.clone() else { return };
    for k in around(&f.cells, at.xz(), FURNITURE + 40.0) {
        if near(k) >= FURNITURE || f.live.contains_key(&k) {
            continue;
        }
        let mut list = vec![];
        for p in &f.groups[k as usize].pieces {
            let Some(mesh) = f.meshes.get(p.name) else { continue };
            let tf = Transform::from_translation(p.pos).with_rotation(Quat::from_rotation_y(p.yaw));
            let mut e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), tf, NotShadowCaster, Tagged("furniture"), OfMap));
            if p.name == "fan" {
                e.insert(Fan);
            }
            list.push(e.id());
        }
        f.live.insert(k, list);
    }
}

/// A ceiling fan, going round.
#[derive(Component)]
struct Fan;

fn turn_fans(time: Res<Time>, mut fans: Query<&mut Transform, With<Fan>>) {
    const TURNS: f32 = 7.0;
    for mut tf in &mut fans {
        tf.rotate_y(TURNS * time.delta_secs());
    }
}

/// One doorway's leaves.
#[derive(Clone, Copy)]
struct Spot {
    pos: Vec3,
    out: Vec2,
    width: f32,
}

#[derive(Component)]
struct Leaf {
    slot: usize,
    side: f32,
    swing: f32,
}

#[derive(Resource, Default)]
struct Doors {
    spots: Vec<Spot>,
    cells: HashMap<(i32, i32), Vec<u32>>,
    slots: Vec<Option<u32>>,
    built: bool,
    frame: u32,
    /// Where the rooms are, for the fill lights; empty until they are spawned.
    rooms: HashMap<(i32, i32), Vec<u32>>,
}

/// The pool of leaves: each slot two leaves, hung at the jambs, a door the camera is near given to
/// each. A warrior close by swings them in. Client only, so online peers need agree on nothing.
fn swing_doors(
    mut commands: Commands,
    mut doors: ResMut<Doors>,
    layout: Res<MapLayout>,
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    warriors: Query<&Transform, (With<Warrior>, Without<Eliminated>, Without<Leaf>)>,
    mut leaves: Query<(&mut Transform, &mut Visibility, &mut Leaf)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if layout.rooms.is_empty() {
        return;
    }
    if doors.spots.is_empty() {
        doors.spots = layout.rooms.iter().flat_map(|r| r.doors.iter().filter(|d| d.leaves).map(|d| Spot { pos: d.pos, out: d.out, width: d.width })).collect();
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (k, d) in doors.spots.iter().enumerate() {
            cells.entry(near_key(d.pos.xz())).or_default().push(k as u32);
        }
        doors.cells = cells;
        doors.slots = vec![None; DOORS];
    }
    if !doors.built {
        doors.built = true;
        let leaf = meshes.add(Cuboid::new(0.79, 2.38, 0.05).mesh().build().translated_by(Vec3::new(0.395, 1.19, 0.0)));
        let wood = materials.add(StandardMaterial { base_color: Color::srgb(0.20, 0.33, 0.30), perceptual_roughness: 0.6, ..default() });
        for slot in 0..DOORS {
            for side in [-1.0, 1.0] {
                commands.spawn((Mesh3d(leaf.clone()), MeshMaterial3d(wood.clone()), Transform::default(), Visibility::Hidden, Leaf { slot, side, swing: 0.0 }, Tagged("door"), OfMap));
            }
        }
    }
    let Ok(camera) = camera.single() else { return };
    let at = camera.translation();
    doors.frame = doors.frame.wrapping_add(1);
    if doors.frame % 15 == 1 {
        let mut near: Vec<(f32, u32)> = around(&doors.cells, at.xz(), DOOR_SEEN).into_iter().map(|k| (doors.spots[k as usize].pos.distance(at), k)).filter(|(d, _)| *d < DOOR_SEEN).collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(DOORS);
        // A door keeps its slot while it stays near, so it doesn't jump.
        let keep: Vec<u32> = near.iter().map(|n| n.1).collect();
        let doors = &mut *doors;
        for s in doors.slots.iter_mut() {
            *s = s.filter(|k| keep.contains(k));
        }
        for k in keep {
            if !doors.slots.contains(&Some(k))
                && let Some(free) = doors.slots.iter_mut().find(|s| s.is_none())
            {
                *free = Some(k);
            }
        }
    }
    let dt = time.delta_secs();
    for (mut tf, mut vis, mut leaf) in &mut leaves {
        let Some(spot) = doors.slots.get(leaf.slot).copied().flatten().map(|k| doors.spots[k as usize]) else {
            *vis = Visibility::Hidden;
            continue;
        };
        *vis = Visibility::Inherited;
        let open = warriors.iter().any(|w| w.translation.xz().distance(spot.pos.xz()) < DOOR_NEAR);
        let target = if open { 1.0 } else { 0.0 };
        leaf.swing += (target - leaf.swing).clamp(-dt * 4.0, dt * 4.0);
        // Hung at its jamb, a little in from the paint, swinging inward.
        let along = spot.out.perp();
        let hinge = spot.pos.xz() + along * leaf.side * spot.width * 0.5 - spot.out * 0.15;
        let yaw = (-(along.y * -leaf.side)).atan2(along.x * -leaf.side);
        let swing = leaf.swing * 1.75 * leaf.side * if spot.out.perp_dot(along) > 0.0 { 1.0 } else { -1.0 };
        tf.translation = Vec3::new(hinge.x, spot.pos.y, hinge.y);
        tf.rotation = Quat::from_rotation_y(yaw + swing);
    }
}

#[derive(Component)]
struct Fill(usize);

/// A few warm lights in the rooms nearest the camera: no light per room, and none of them casts a
/// shadow. They do what a bounce off the floor would.
fn place_fills(mut commands: Commands, layout: Res<MapLayout>, mut doors: ResMut<Doors>, camera: Query<&GlobalTransform, With<MainCamera>>, mut fills: Query<(&mut Transform, &mut PointLight, &Fill)>, mut frame: Local<u32>) {
    if layout.rooms.is_empty() {
        return;
    }
    if doors.rooms.is_empty() {
        doors.rooms = index_rooms(&layout);
        for k in 0..FILLS {
            let light = PointLight { color: Color::srgb(1.0, 0.86, 0.68), intensity: 0.0, range: 9.0, shadow_maps_enabled: false, ..default() };
            commands.spawn((light, Transform::default(), Fill(k), OfMap));
        }
    }
    *frame = frame.wrapping_add(1);
    if *frame % 10 != 0 {
        return;
    }
    let Ok(camera) = camera.single() else { return };
    let at = camera.translation();
    let mut near: Vec<(f32, Vec3)> = vec![];
    for k in around(&doors.rooms, at.xz(), FILL_NEAR + 30.0) {
        let r = &layout.rooms[k as usize];
        for room in &r.rooms {
            let (mx, mz) = ((room.x.0 + room.x.1) / 2.0, (room.z.0 + room.z.1) / 2.0);
            let p = r.at(mx, r.level(room.storey) + 2.5, mz);
            let d = p.distance(at);
            if d < FILL_NEAR {
                near.push((d, p));
            }
        }
    }
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (mut tf, mut light, fill) in &mut fills {
        match near.get(fill.0) {
            Some((d, p)) => {
                tf.translation = *p;
                light.intensity = FILL_LUMENS * (1.0 - d / FILL_NEAR).clamp(0.0, 1.0);
            }
            None => light.intensity = 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::map::Mat;
    use shared::map::interiors::{BAY, STOREY, has_shops};

    /// The holes and the paint are cut by one rule: the facade's numbers, and which walls are shops.
    #[test]
    fn the_paint_and_the_holes_agree() {
        assert_eq!(BAY, crate::buildings::BAY);
        assert_eq!(STOREY, crate::buildings::STOREY);
        for k in 0..400 {
            let c = Vec2::new(k as f32 * 13.37 - 2000.0, k as f32 * -7.11 + 1500.0);
            let height = 3.4 + (k % 5) as f32 * 3.4;
            let painted = height > crate::buildings::STOREY * 1.6 && Paint::of(c, Mat::Plaster).r(3) < 0.8;
            assert_eq!(painted, has_shops(c, height), "at {c}");
        }
    }

    /// A cut wall keeps its paint where it is: the texture stays on the wall's own coordinates.
    #[test]
    fn a_cut_wall_keeps_its_texture_in_place() {
        let mut w = MeshBuf::default();
        let cuts = [Cut { s0: 1.0, s1: 2.0, y0: 1.0, y1: 2.0 }];
        skin(&mut w, &cuts, (Vec2::ZERO, Vec3::X, 4.0), (0.0, 3.0), (0.0, 1.0), (0.0, -1.0), ([1.0; 4], [0.5, 0.5, 0.5, 1.0]), Vec3::NEG_Z);
        assert_eq!(w.pos.len(), w.uv.len());
        assert_eq!(w.pos.len(), w.col.len());
        for (p, uv) in w.pos.iter().zip(&w.uv) {
            assert!((uv[0] - p[0] / 4.0).abs() < 1e-5 && (uv[1] + p[1] / 3.0).abs() < 1e-5, "{p:?} {uv:?}");
        }
        let area: f32 = w.idx.chunks_exact(3).map(|t| (Vec3::from(w.pos[t[1] as usize]) - Vec3::from(w.pos[t[0] as usize])).cross(Vec3::from(w.pos[t[2] as usize]) - Vec3::from(w.pos[t[0] as usize])).length() / 2.0).sum();
        assert!((area - 11.0).abs() < 1e-3, "{area}");
    }
}
