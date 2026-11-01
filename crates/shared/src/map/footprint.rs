//! Buildings from footprints (a city map's real outlines): the solid each one is, walls and a flat
//! roof, for the simulation's collider and the client's mesh.

use bevy::prelude::*;

use super::Footprint;

/// Twice the signed area of an outline in (x, z).
fn area2(pts: &[Vec2]) -> f32 {
    pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum()
}

/// Triangles covering a simple polygon (ear clipping), as index triples into `pts`.
pub fn triangulate(pts: &[Vec2]) -> Vec<[u32; 3]> {
    let n = pts.len();
    if n < 3 {
        return vec![];
    }
    let ccw = area2(pts) > 0.0;
    let mut idx: Vec<usize> = (0..n).collect();
    if !ccw {
        idx.reverse();
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let inside = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| cross(a, b, p) > 0.0 && cross(b, c, p) > 0.0 && cross(c, a, p) > 0.0;
    let mut out = vec![];
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (i0, i1, i2) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (pts[i0], pts[i1], pts[i2]);
            if cross(a, b, c) <= 1e-6 {
                continue;
            }
            if idx.iter().any(|&j| j != i0 && j != i1 && j != i2 && inside(pts[j], a, b, c)) {
                continue;
            }
            out.push([i0 as u32, i1 as u32, i2 as u32]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            // Degenerate leftovers (collinear runs): fan the rest.
            break;
        }
    }
    for k in 1..idx.len().saturating_sub(1) {
        out.push([idx[0] as u32, idx[k] as u32, idx[k + 1] as u32]);
    }
    out
}

/// The building's surface: one quad per wall and the roof, wound to face outward (and up).
/// Positions and triangles; vertices are not shared between faces, so normals are flat.
pub fn solid(fp: &Footprint) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let (mut pos, mut tris) = (vec![], vec![]);
    let pts = &fp.pts;
    let n = pts.len();
    let ccw = area2(pts) > 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let e = b - a;
        if e.length() < 1e-4 {
            continue;
        }
        // Outward: to the right of the edge when the outline runs counter-clockwise in (x, z).
        let out = if ccw { Vec2::new(e.y, -e.x) } else { Vec2::new(-e.y, e.x) };
        let k = pos.len() as u32;
        pos.extend([Vec3::new(a.x, fp.base, a.y), Vec3::new(b.x, fp.base, b.y), Vec3::new(b.x, fp.top, b.y), Vec3::new(a.x, fp.top, a.y)]);
        let mut t = [[k, k + 1, k + 2], [k, k + 2, k + 3]];
        let nrm = (pos[k as usize + 1] - pos[k as usize]).cross(pos[k as usize + 2] - pos[k as usize]);
        if nrm.dot(Vec3::new(out.x, 0.0, out.y)) < 0.0 {
            t = [[k, k + 2, k + 1], [k, k + 3, k + 2]];
        }
        tris.extend(t);
    }
    let k = pos.len() as u32;
    pos.extend(pts.iter().map(|p| Vec3::new(p.x, fp.top, p.y)));
    for [a, b, c] in triangulate(pts) {
        let (pa, pb, pc) = (pos[(k + a) as usize], pos[(k + b) as usize], pos[(k + c) as usize]);
        tris.push(if (pb - pa).cross(pc - pa).y >= 0.0 { [k + a, k + b, k + c] } else { [k + a, k + c, k + b] });
    }
    // Standing on a hall (crate::map::delhi): its underside, the hall's ceiling, facing down.
    if fp.base > 0.05 {
        let k = pos.len() as u32;
        pos.extend(pts.iter().map(|p| Vec3::new(p.x, fp.base, p.y)));
        for [a, b, c] in triangulate(pts) {
            let (pa, pb, pc) = (pos[(k + a) as usize], pos[(k + b) as usize], pos[(k + c) as usize]);
            tris.push(if (pb - pa).cross(pc - pa).y <= 0.0 { [k + a, k + b, k + c] } else { [k + a, k + c, k + b] });
        }
    }
    (pos, tris)
}

/// Is (x, z) inside the outline?
pub fn contains(pts: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    for (a, b) in pts.iter().zip(pts.iter().cycle().skip(1)) {
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            inside = !inside;
        }
    }
    inside
}
