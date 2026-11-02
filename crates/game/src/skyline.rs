//! The city past the map's edge: a ring of distant blocks, towers and water tanks on a ground
//! that runs to the horizon, so a Delhi map looks like part of a city rather than an island
//! in a desert. One mesh, no shadows: a few thousand triangles for the whole horizon.

use std::f32::consts::TAU;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use shared::map::MapLayout;
use shared::sim::{OfMap, map_reloaded};

/// The ground past the map's edge, sRGB.
pub(crate) const LAND: [f32; 3] = [0.42, 0.41, 0.40];
/// How far a big map's ground runs, metres: nearly to the horizon from 500 m up. Further doesn't
/// help: at 120 km, Ultra's atmosphere tinted the last of it in a purple band along the horizon.
const HORIZON_RING: f32 = 31_000.0;

pub struct SkylinePlugin;

impl Plugin for SkylinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_skyline).add_systems(Update, spawn_skyline.run_if(map_reloaded));
    }
}

/// The map's own wall colours (crate::buildings::WALLS), a little greyer for the distance.
fn colour(k: usize) -> (f32, f32, f32) {
    let c = crate::buildings::WALLS[k % crate::buildings::WALLS.len()];
    let g = (c[0] + c[1] + c[2]) / 3.0;
    let m = |x: f32| x * 0.8 + g * 0.2;
    (m(c[0]), m(c[1]), m(c[2]))
}

fn spawn_skyline(mut commands: Commands, layout: Res<MapLayout>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    // Only a map of buildings stands in a city.
    if layout.footprints.is_empty() {
        return;
    }
    let t = &layout.terrain;
    let big = t.n >= crate::visuals::BIG_TERRAIN;
    // The terrain's square ends at its edge: the ground carries on just under its level.
    let edge = t.size * 0.5;
    let mut floor = f32::MAX;
    for k in 0..64 {
        let a = TAU * k as f32 / 64.0;
        let p = Vec2::from_angle(a) * edge * 0.98;
        floor = floor.min(t.height(p.x, p.y));
    }
    let floor = floor - 0.4;
    let mut parts: Vec<Mesh> = vec![];
    // On a big map, far enough that from 500 m up its end is all but on the horizon: under the
    // physical sky of Ultra, what lies past it is the atmosphere's dark underside. It is the one
    // mesh, and costs no more this wide (the camera's far plane culls whole meshes and cuts none).
    let ground_ends = if big { HORIZON_RING } else { 4000.0 };
    let ground = Annulus::new(edge * 0.95, ground_ends).mesh().resolution(96).build();
    parts.push(paint(ground.rotated_by(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).translated_by(Vec3::Y * floor), (LAND[0], LAND[1], LAND[2])));

    // A cheap seeded random stream, so every peer's horizon is the same.
    let mut s = layout.name.bytes().fold(0x9e37_79b9u32, |h, b| h.rotate_left(5) ^ b as u32);
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s % 10_000) as f32 / 10_000.0
    };
    // City blocks on a street grid (lanes between them), lower near the map where they fill gaps
    // between its own buildings, taller and sparser farther out where they make the skyline.
    const CELL: f32 = 34.0;
    let reach = if big { edge + 1100.0 } else { 1100.0 };
    let cells = (reach / CELL) as i32;
    for gj in -cells..=cells {
        for gi in -cells..=cells {
            let centre = Vec2::new(gi as f32 + 0.5, gj as f32 + 0.5) * CELL;
            let r = centre.length();
            // Kept off the playable square, and out of the corners of the terrain past it.
            if r > reach || (centre.x.abs() < edge + CELL * 0.5 && centre.y.abs() < edge + CELL * 0.5) {
                continue;
            }
            let far = ((r - edge) / (reach - edge)).clamp(0.0, 1.0);
            if rnd() > 0.95 - 0.45 * far {
                continue;
            }
            // One to four buildings to a block.
            let split = if rnd() < 0.5 { 1 } else { 2 };
            for sj in 0..split {
                for si in 0..split {
                    let sub = CELL / split as f32;
                    let c = centre + (Vec2::new(si as f32 + 0.5, sj as f32 + 0.5) / split as f32 - Vec2::splat(0.5)) * CELL;
                    let (w, d) = (sub * (0.62 + rnd() * 0.25), sub * (0.62 + rnd() * 0.25));
                    let tall = rnd() < 0.015 + 0.035 * far;
                    let h = if tall { 45.0 + rnd() * 40.0 } else { 7.0 + rnd() * rnd() * (14.0 + 26.0 * far) };
                    // Paler and bluer the farther off, as distant walls look under a bright sky.
                    let col = colour((rnd() * 100.0) as usize);
                    let k = 0.2 + 0.35 * far;
                    let col = (col.0 + (0.74 - col.0) * k, col.1 + (0.78 - col.1) * k, col.2 + (0.85 - col.2) * k);
                    let base = Vec3::new(c.x, floor, c.y);
                    parts.push(paint_keep_uv(block(w, h, d).translated_by(base), col));
                    roof_things(&mut parts, &mut rnd, base, w, d, h, col);
                }
            }
        }
    }
    let mut mesh = parts.pop().expect("ground");
    for p in &parts {
        mesh.merge(p).expect("same attributes");
    }
    mesh.asset_usage = crate::buildings::tiers::usage();
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(images.add({
                let mut img = crate::buildings::facade_image();
                crate::buildings::mip_chain(&mut img);
                img.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
                img
            })),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::default(),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
        OfMap,
    ));
}

/// What sits on Delhi's roofs: water tanks and stair heads. No domes: invented ones read as mosques
/// and temples that are not there.
fn roof_things(parts: &mut Vec<Mesh>, rnd: &mut impl FnMut() -> f32, base: Vec3, w: f32, d: f32, h: f32, col: (f32, f32, f32)) {
    let roll = rnd();
    if roll < 0.35 {
        let tank = Cylinder::new(1.2, 2.0).mesh().resolution(8).build();
        let off = Vec3::new((rnd() - 0.5) * w * 0.6, 0.0, (rnd() - 0.5) * d * 0.6);
        parts.push(paint(tank.translated_by(base + off + Vec3::Y * (h + 1.0)), (0.12, 0.12, 0.13)));
    } else if roll < 0.55 {
        let (pw, pd, ph) = (w * 0.3, d * 0.3, 2.8);
        let off = Vec3::new((rnd() - 0.5) * w * 0.4, 0.0, (rnd() - 0.5) * d * 0.4);
        parts.push(paint(Cuboid::new(pw, ph, pd).mesh().build().translated_by(base + off + Vec3::Y * (h + ph * 0.5)), col));
    }
}

/// A block standing on the origin, its walls mapped with the city's window texture (two bays by
/// two storeys a repeat, as crate::buildings), its roof on a plain patch of that texture.
fn block(w: f32, h: f32, d: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let (bay, storey) = (2.0 * crate::buildings::BAY, 2.0 * crate::buildings::STOREY);
    let (mut pos, mut nrm, mut uv, mut idx) = (vec![], vec![], vec![], vec![]);
    let (x, z) = (w * 0.5, d * 0.5);
    // Each wall from its left corner seen from outside, and its width.
    for (a, b, n, len) in [
        (Vec3::new(-x, 0.0, z), Vec3::new(x, 0.0, z), Vec3::Z, w),
        (Vec3::new(x, 0.0, z), Vec3::new(x, 0.0, -z), Vec3::X, d),
        (Vec3::new(x, 0.0, -z), Vec3::new(-x, 0.0, -z), Vec3::NEG_Z, w),
        (Vec3::new(-x, 0.0, -z), Vec3::new(-x, 0.0, z), Vec3::NEG_X, d),
    ] {
        let k = pos.len() as u32;
        let (u1, v1) = (len / bay, -h / storey);
        pos.extend([a, b, b + Vec3::Y * h, a + Vec3::Y * h].map(|p| p.to_array()));
        nrm.extend([n.to_array(); 4]);
        uv.extend([[0.0, 0.0], [u1, 0.0], [u1, v1], [0.0, v1]]);
        idx.extend([k, k + 1, k + 2, k, k + 2, k + 3]);
    }
    let k = pos.len() as u32;
    pos.extend([[-x, h, z], [x, h, z], [x, h, -z], [-x, h, -z]]);
    nrm.extend([[0.0, 1.0, 0.0]; 4]);
    uv.extend([PLAIN; 4]);
    idx.extend([k, k + 1, k + 2, k, k + 2, k + 3]);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

/// A point of the window texture that is bare wall, for everything that isn't a block's wall.
const PLAIN: [f32; 2] = [0.04, 0.4];

/// A part with only the attributes every part shares, all one colour, on bare wall.
fn paint(mut m: Mesh, col: (f32, f32, f32)) -> Mesh {
    let n = m.count_vertices();
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![PLAIN; n]);
    paint_keep_uv(m, col)
}

fn paint_keep_uv(mut m: Mesh, (r, g, b): (f32, f32, f32)) -> Mesh {
    m.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    let n = m.count_vertices();
    let c = Color::srgb(r, g, b).to_linear();
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(vec![[c.red, c.green, c.blue, 1.0]; n]));
    m
}
