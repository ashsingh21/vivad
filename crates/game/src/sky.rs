//! The sky: a painted one, not a simulated one. A dome coloured per vertex (warm pale haze at the
//! horizon to clear blue overhead, a glow round the sun), a bright sun disc for the bloom to catch,
//! and a ring of puffy clouds lit by the sun. It all rides with the camera, so it is always "at
//! infinity".
//!
//! Replaces Bevy's physical atmosphere, whose lookup tables were compute passes every frame: this is
//! three draws of a few thousand triangles at any quality, which suits the tile-based phone GPUs
//! (docs/rendering-guidelines.md §4), and reads as the polished stylised look we want.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::visuals::Sun;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_sky.after(crate::visuals::setup_sky))
            .add_systems(PostUpdate, follow_camera.after(crate::camera::CameraSystems).before(TransformSystems::Propagate));
    }
}

/// Inside the camera's far plane (Bevy's default 1000 m).
const RADIUS: f32 = 900.0;
/// Sky colours (sRGB): the haze at the horizon, the blue above it, and overhead.
pub const HORIZON: [f32; 3] = [0.92, 0.88, 0.84];
const MID: [f32; 3] = [0.62, 0.77, 0.93];
const ZENITH: [f32; 3] = [0.34, 0.56, 0.88];
const SUN_GLOW: [f32; 3] = [1.0, 0.86, 0.62];

/// The sky rig's root, kept on the camera's position.
#[derive(Component)]
struct SkyRig;

fn lin(c: [f32; 3]) -> Vec3 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

fn spawn_sky(mut commands: Commands, sun: Query<&Transform, With<Sun>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Toward the sun: the light shines along its -Z.
    let to_sun = sun.single().map(|t| t.back().as_vec3()).unwrap_or(Vec3::new(0.7, 0.5, 0.3).normalize());
    let rig = commands.spawn((SkyRig, Transform::default(), Visibility::default())).id();
    let unlit = |materials: &mut Assets<StandardMaterial>, emissive: LinearRgba| {
        materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, fog_enabled: false, emissive, ..default() })
    };
    let plain = unlit(&mut materials, LinearRgba::BLACK);
    commands.spawn((Mesh3d(meshes.add(dome(to_sun))), MeshMaterial3d(plain), sky_flags(), ChildOf(rig)));
    // The sun: a disc bright enough to bloom.
    let disc = Circle::new(26.0).mesh().resolution(32).build();
    commands.spawn((
        Mesh3d(meshes.add(disc)),
        MeshMaterial3d(unlit(&mut materials, LinearRgba::rgb(14.0, 12.0, 9.0))),
        Transform::from_translation(to_sun * RADIUS * 0.95).looking_to(-to_sun, Vec3::Y),
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
        ChildOf(rig),
    ));
    // Painted, not lit: the shading is in the vertex colours (clouds()), so they stay soft and
    // luminous rather than plastic lumps with hard dark undersides.
    let cloud = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, fog_enabled: false, ..default() });
    commands.spawn((Mesh3d(meshes.add(clouds(to_sun))), MeshMaterial3d(cloud), sky_flags(), ChildOf(rig)));
}

fn sky_flags() -> (Transform, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver) {
    (Transform::default(), bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver)
}

/// A sphere seen from inside, its colour the sky's at each vertex.
fn dome(to_sun: Vec3) -> Mesh {
    let (rings, segs) = (48u32, 96u32);
    let (horizon, mid, zenith, glow) = (lin(HORIZON), lin(MID), lin(ZENITH), lin(SUN_GLOW));
    let (mut pos, mut nrm, mut col, mut idx) = (vec![], vec![], vec![], vec![]);
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32;
        for s in 0..=segs {
            let theta = TAU * s as f32 / segs as f32;
            let d = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            let h = d.y;
            let c = if h >= 0.0 {
                // Haze sits low: most of the change in the first 15 degrees.
                let a = (h / 0.25).clamp(0.0, 1.0).powf(0.7);
                let b = ((h - 0.25) / 0.75).clamp(0.0, 1.0);
                horizon.lerp(mid, a).lerp(zenith, b)
            } else {
                // Below the horizon (seen past the map's edge): the haze, a little darker.
                horizon * (1.0 + h * 0.3)
            };
            let s = d.dot(to_sun).max(0.0);
            let c = c + glow * (0.55 * s.powf(48.0) + 0.18 * s.powf(6.0));
            pos.push((d * RADIUS).to_array());
            nrm.push((-d).to_array());
            col.push([c.x, c.y, c.z, 1.0]);
        }
    }
    let w = segs + 1;
    for r in 0..rings {
        for s in 0..segs {
            let (a, b, c, d) = (r * w + s, r * w + s + 1, (r + 1) * w + s + 1, (r + 1) * w + s);
            // Wound to face inward.
            idx.extend_from_slice(&[a, c, b, a, d, c]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(col))
        .with_inserted_indices(Indices::U32(idx))
}

/// Cumulus round the horizon, painted: each a heap of many puffs on a broad flat base, smaller
/// toward the top, shaded in their vertex colours (lit warm white toward the sun and on top, a
/// soft lavender-grey underneath, hazier near the horizon) and drawn unlit. Fewer toward the sun
/// so its glow stays clear. One merged mesh.
fn clouds(to_sun: Vec3) -> Mesh {
    let mut s = 0x2545_f491u32;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s % 10_000) as f32 / 10_000.0
    };
    let ball = Sphere::new(1.0).mesh().ico(2).expect("ico");
    // Brighter than white: unlit, they'd otherwise sit dimmer than the sky they hang in.
    let (lit, shade, haze) = (Vec3::new(1.0, 0.97, 0.92) * 2.0, Vec3::new(0.86, 0.87, 0.98) * 1.35, Vec3::from(lin(HORIZON)) * 1.2);
    let mut out: Option<Mesh> = None;
    let sun_az = to_sun.z.atan2(to_sun.x);
    for k in 0..22 {
        let az = TAU * (k as f32 + rnd() * 0.7) / 22.0;
        let off = ((az - sun_az + PI).rem_euclid(TAU) - PI).abs();
        if off < 0.35 {
            continue;
        }
        let elev = 0.05 + rnd() * rnd() * 0.32;
        let dist = RADIUS * 0.82;
        let centre = Vec3::new(az.cos() * elev.cos(), elev.sin(), az.sin() * elev.cos()) * dist;
        // Facing the camera: spread along the horizon, little depth.
        let across = Vec3::new(-az.sin(), 0.0, az.cos());
        let toward = -Vec3::new(az.cos(), 0.0, az.sin());
        let size = 30.0 + rnd() * 40.0;
        let base = centre.y - size * 0.15;
        // Hazier the lower it sits.
        let fade = (1.0 - (elev - 0.05) / 0.15).clamp(0.0, 1.0) * 0.25;
        let puffs = 10 + (rnd() * 6.0) as usize;
        for p in 0..puffs {
            // A heap: the first puffs a wide base along the horizon, later ones smaller and higher,
            // drawn toward the middle.
            let tier = p as f32 / puffs as f32;
            let spread = 1.25 * (1.0 - 0.65 * tier);
            let t = (rnd() - 0.5) * 2.0 * spread;
            let r = size * (0.68 - 0.28 * tier) * (0.75 + 0.5 * rnd()) * (1.0 - 0.3 * t.abs());
            let at = centre + across * t * size + Vec3::Y * (size * 0.55 * tier + r * 0.2) + toward * (rnd() - 0.5) * size * 0.5;
            let mut m = ball.clone().scaled_by(Vec3::new(r, r * 0.8, r)).translated_by(at);
            if let Some(VertexAttributeValues::Float32x3(ps)) = m.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
                // A flat underside, as cumulus have.
                for q in ps.iter_mut() {
                    q[1] = q[1].max(base);
                }
            }
            let ps: Vec<Vec3> = match m.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(ps)) => ps.iter().map(|q| Vec3::from(*q)).collect(),
                _ => vec![],
            };
            let col: Vec<[f32; 4]> = ps
                .iter()
                .map(|q| {
                    // The puff's own normal (a sphere's), the sun's side and the top lit, the
                    // underside in soft shade; then haze near the horizon.
                    let n = ((*q - at) / Vec3::new(r, r * 0.8, r)).normalize_or(Vec3::Y);
                    let sun = 0.5 + 0.5 * n.dot(to_sun);
                    let up = ((q.y - base) / (size * 0.9)).clamp(0.0, 1.0);
                    let k = (0.45 * sun + 0.55 * up.powf(0.6)).clamp(0.0, 1.0);
                    let c = shade.lerp(lit, k).lerp(haze, fade);
                    [c.x, c.y, c.z, 1.0]
                })
                .collect();
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            match &mut out {
                Some(o) => o.merge(&m).expect("same attributes"),
                None => out = Some(m),
            }
        }
    }
    let mut m = out.expect("clouds");
    m.remove_attribute(Mesh::ATTRIBUTE_UV_0);
    m.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    m
}

fn follow_camera(cam: Query<&Transform, (With<MainCamera>, Without<SkyRig>)>, mut rig: Query<&mut Transform, With<SkyRig>>) {
    let (Ok(c), Ok(mut r)) = (cam.single(), rig.single_mut()) else { return };
    r.translation = c.translation;
}
