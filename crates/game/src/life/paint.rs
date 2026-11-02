//! Paint on the connecting roads and the bridges: edge lines, lane dashes, a double line down the
//! middle of the wide ones. A road with no paint reads as a grey band laid on the land; worn paint
//! is what says highway, on the ground and from a drop. None of it is on the map: it is thin quads a
//! few centimetres over the road, a mesh to a cell, sharing one plain material, with no collider and
//! no shadow.

use std::collections::HashMap;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, VisibilityRange};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use shared::map::stalls::hash;
use shared::map::terrain::ground;
use shared::map::{MapLayout, MapStyle, compose};
use shared::sim::{OfMap, map_reloaded};

use crate::batch::probe::Tagged;
use crate::buildings::tiers;

pub struct PaintPlugin;

impl Plugin for PaintPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, lay).add_systems(Update, lay.run_if(map_reloaded));
    }
}

/// Metres a side of a mesh, and how far from the camera its middle may be.
const CELL: f32 = 128.0;
const SEEN_TO: f32 = 300.0;
/// Over the road, and each line's width.
const LIFT: f32 = 0.035;
const LINE: f32 = 0.13;
/// How long a quad of solid line is, a dash and the gap after it.
const STEP: f32 = 4.0;
const DASH: f32 = 3.0;
const GAP: f32 = 6.0;
/// The narrowest road with paint: anything less is a street.
const NARROWEST: f32 = 11.0;
/// A road this wide has a double line and a lane either side of it.
const DIVIDED: f32 = 18.0;
/// Linear colours of worn road paint, the edge and dashes and the middle's yellow.
const WHITE: [f32; 3] = [0.36, 0.35, 0.33];
const YELLOW: [f32; 3] = [0.34, 0.25, 0.08];

#[derive(Default)]
struct Cell {
    pos: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
    lo: Option<(Vec3, Vec3)>,
}

impl Cell {
    /// A thin quad from `a` to `b`, each end `shade` of `colour`.
    fn quad(&mut self, a: Vec3, b: Vec3, half: f32, colour: [f32; 3], shade: [f32; 2]) {
        let side = (b - a).xz().normalize_or(Vec2::X).perp() * half;
        let side = Vec3::new(side.x, 0.0, side.y);
        let base = self.pos.len() as u32;
        for (p, k) in [(a - side, shade[0]), (a + side, shade[0]), (b + side, shade[1]), (b - side, shade[1])] {
            self.pos.push(p.to_array());
            self.col.push([colour[0] * k, colour[1] * k, colour[2] * k, 1.0]);
            let (lo, hi) = self.lo.get_or_insert((p, p));
            *lo = lo.min(p);
            *hi = hi.max(p);
        }
        // The quad faces up whichever way the road runs.
        self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// How worn the paint is here: a stretch fades, and comes back.
fn worn(p: Vec2, k: u32) -> f32 {
    let (a, b) = ((p.x / 3.0).floor() as i32 as u32, (p.y / 3.0).floor() as i32 as u32);
    0.62 + 0.38 * hash(a, b, k)
}

/// Runs along a polyline: calls `each` with the point every `STEP` metres, for as long as it lasts.
fn along(line: &[Vec2], mut each: impl FnMut(Vec2, Vec2, f32)) {
    let mut walked = 0.0;
    for s in line.windows(2) {
        let (a, c) = (s[0], s[1]);
        let len = a.distance(c);
        if len < 1e-3 {
            continue;
        }
        let dir = (c - a) / len;
        let mut t = 0.0;
        while t < len {
            each(a + dir * t, dir, walked + t);
            t += STEP / 2.0;
        }
        walked += len;
    }
}

/// One road's paint. `y` is the height of the road at a point, or None where this paint stops.
fn road(cells: &mut HashMap<(i32, i32), Cell>, width: f32, line: &[Vec2], y: &impl Fn(Vec2) -> Option<f32>, open: &impl Fn(Vec2) -> bool) {
    let half = width * 0.5;
    // Samples along the middle: where, which way, how far along.
    let mut at = vec![];
    along(line, |p, d, s| at.push((p, d, s)));
    let mut put = |a: Vec2, b: Vec2, half_line: f32, colour: [f32; 3], k: u32| {
        let (Some(ya), Some(yb)) = (y(a), y(b)) else { return };
        let mid = (a + b) * 0.5;
        let key = ((mid.x / CELL).floor() as i32, (mid.y / CELL).floor() as i32);
        cells.entry(key).or_default().quad(Vec3::new(a.x, ya + LIFT, a.y), Vec3::new(b.x, yb + LIFT, b.y), half_line, colour, [worn(a, k), worn(b, k)]);
    };
    for w in at.windows(3) {
        let ((p, d, s), (q, e, _)) = (w[0], w[2]);
        let (n0, n1) = (d.perp(), e.perp());
        // The edge lines, broken at a side road.
        for side in [-1.0f32, 1.0] {
            let off = side * (half - 0.45);
            if !open(p + n0 * side * (half + 2.5)) && !open(q + n1 * side * (half + 2.5)) {
                put(p + n0 * off, q + n1 * off, LINE * 0.5, WHITE, 1);
            }
        }
        let dash = s.rem_euclid(DASH + GAP) < DASH;
        if width >= DIVIDED {
            // A double line down the middle, and a lane's dashes either side of it.
            for side in [-0.16f32, 0.16] {
                put(p + n0 * side, q + n1 * side, LINE * 0.5, YELLOW, 2);
            }
            if dash {
                for side in [-1.0f32, 1.0] {
                    let off = side * half * 0.5;
                    put(p + n0 * off, q + n1 * off, LINE * 0.5, WHITE, 3);
                }
            }
        } else if dash {
            put(p, q, LINE * 0.5, WHITE, 3);
        }
    }
}

fn lay(mut commands: Commands, layout: Res<MapLayout>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if layout.style != MapStyle::Ncr {
        return;
    }
    let t = &layout.terrain;
    let on_road = |p: Vec2| (t.ground_at(p.x, p.y) == Some(ground::ROAD)).then(|| t.height(p.x, p.y));
    let open = |p: Vec2| t.ground_at(p.x, p.y) == Some(ground::ROAD);
    let mut cells: HashMap<(i32, i32), Cell> = HashMap::new();
    for (width, line) in compose::network() {
        if width >= NARROWEST {
            road(&mut cells, width, &line, &on_road, &open);
        }
    }
    for s in &compose::SPANS {
        let top = s.top;
        let deck = |_: Vec2| Some(top);
        road(&mut cells, s.width, &[s.a, s.c], &deck, &|_| false);
    }
    let material = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.7, reflectance: 0.3, ..default() });
    let mut quads = 0;
    for (key, cell) in cells {
        let Some((lo, hi)) = cell.lo else { continue };
        let centre = Vec3::new((key.0 as f32 + 0.5) * CELL, 0.0, (key.1 as f32 + 0.5) * CELL);
        quads += cell.idx.len() / 6;
        let count = cell.pos.len();
        let pos: Vec<[f32; 3]> = cell.pos.iter().map(|p| (Vec3::from(*p) - centre).to_array()).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, tiers::usage());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, cell.col);
        mesh.insert_indices(if count > u16::MAX as usize { Indices::U32(cell.idx) } else { Indices::U16(cell.idx.iter().map(|i| *i as u16).collect()) });
        tiers::pack(&mut mesh);
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(centre),
            Aabb::from_min_max(lo - centre, hi - centre),
            NoAutoAabb,
            VisibilityRange::abrupt(0.0, SEEN_TO),
            NotShadowCaster,
            bevy::light::NotShadowReceiver,
            Tagged("road paint"),
            OfMap,
        ));
    }
    info!("road paint: {quads} quads");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_faces_up_whichever_way_it_runs() {
        for to in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z, Vec3::new(3.0, 0.0, -4.0)] {
            let mut c = Cell::default();
            c.quad(Vec3::ZERO, to, 0.1, WHITE, [1.0, 1.0]);
            for t in c.idx.chunks_exact(3) {
                let [a, b, d] = [t[0], t[1], t[2]].map(|i| Vec3::from(c.pos[i as usize]));
                assert!((b - a).cross(d - a).y > 0.0, "{to}");
            }
        }
    }
}
