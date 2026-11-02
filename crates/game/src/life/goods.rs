//! The goods: what makes a stall a stall and a shop front a shop. Rows of trestle stalls along the
//! kerbs, heaps on the thelas, stocked shelves and awnings on the shop fronts, spreads on the
//! pavement, parked scooters and handcarts, banners over the road, wires strung from pole to pole,
//! steam off the chai.
//!
//! The tables in the rows are the map's, and solid (`shared::map::stalls`): what is drawn here stands
//! on them. The rest is not on the map: it has no collider, the street grid never hears of it, and
//! bullets pass through it, so what stands in the open stops below a warrior's eyes or hangs above
//! them. It is worked out from the map's kit and footprints only for the patches of street near the
//! camera, and each patch is three meshes with one material (`Level`), merged on a worker. A market
//! is a handful of draws, and with nothing near the player nothing exists at all.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, VisibilityRange};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use shared::map::stalls::hash;
use shared::map::terrain::ground;
use shared::map::{MapLayout, Mat};
use shared::sim::OfMap;

use super::{Market, busy_at};
use crate::batch::probe::Tagged;
use crate::buildings::tiers;
use crate::camera::MainCamera;

pub struct GoodsPlugin;

impl Plugin for GoodsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Goods>()
            .init_resource::<Steam>()
            .add_systems(Startup, load)
            .add_systems(Update, (forget.run_if(shared::sim::map_reloaded), stream, steam).chain());
    }
}

/// Metres a side of a patch of street: the mesh it all goes in.
const PATCH: f32 = 64.0;
/// How far from the camera a patch's middle may be for it to exist, and where it goes.
const REACH: f32 = 164.0;
const LET_GO: f32 = 184.0;
/// Where a patch fades out, from its middle.
const FADE_FROM: f32 = 140.0;
const FADE_TO: f32 = 156.0;
/// Where the full goods hand over to their stand-ins, from a patch's middle, and how far off the
/// full ones are ready and gone. A corner of a patch is 45 m from its middle.
const DETAIL_FROM: f32 = 56.0;
const DETAIL_TO: f32 = 66.0;
const DETAIL_REACH: f32 = 74.0;
const DETAIL_LET_GO: f32 = 92.0;
/// Past this, a patch's overhead stops casting: the sun's far cascade is coarse there anyway.
const SHADE_TO: f32 = 72.0;
/// How far a stall keeps from a parked vehicle's middle.
const PARKED_CLEAR: f32 = 3.2;
/// Above the ground, the camera is too far up for goods to show.
const TOO_HIGH: f32 = 90.0;
/// The middle of the atlas's white tile (tools/blender/life/goods_atlas.py).
const WHITE_TEXEL: [f32; 2] = [0.08, 0.08];
/// The atlas's plain linen tile: where it starts, and how wide.
const LINEN: (f32, f32, f32) = (2.03 * 170.0 / 1024.0, 3.03 * 170.0 / 1024.0, 0.94 * 170.0 / 1024.0);
/// Linear colours for what's strung over a road.
const BANNER_COLOURS: [[f32; 3]; 6] = [[0.56, 0.06, 0.05], [0.72, 0.36, 0.03], [0.04, 0.14, 0.32], [0.05, 0.26, 0.11], [0.56, 0.10, 0.22], [0.62, 0.56, 0.42]];
/// A shop's width along a wall, from the buildings' own window rule.
const BAY: f32 = crate::buildings::BAY;

/// One piece's mesh, read from the file.
#[derive(Default)]
struct Piece {
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

/// A hanging wire: its ends and how far it sags.
#[derive(Clone, Copy)]
struct Wire {
    a: Vec3,
    b: Vec3,
    sag: f32,
}

/// Something strung over the road: a run of little flags, or a long plain cloth.
#[derive(Clone, Copy)]
struct Banner {
    a: Vec3,
    b: Vec3,
    sag: f32,
    cloth: bool,
    tint: f32,
}

#[derive(Resource, Default)]
struct Goods {
    pieces: Option<Arc<HashMap<String, Piece>>>,
    /// The pieces' file being read, and the atlas they are all mapped into.
    reading: Option<Task<Option<(HashMap<String, Piece>, Image)>>>,
    material: Option<Handle<StandardMaterial>>,
    /// The patches that exist.
    live: HashMap<(i32, i32), Patch>,
    /// A patch's mesh being merged.
    job: Option<((i32, i32), Level, Task<Option<(Mesh, Aabb)>>)>,
}

/// The meshes of a patch. What hangs overhead is few faces and throws the shade that matters, and
/// shows at every range; what stands under it is most of the faces and casts no shadow, in full
/// close to and in stand-ins further off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Over,
    Near,
    Far,
}

/// What a patch holds, once worked out, and its meshes.
#[derive(Default)]
struct Patch {
    content: Option<Arc<Content>>,
    over: Option<Entity>,
    far: Option<Entity>,
    near: Option<Entity>,
    /// The meshes asked for since this patch came into reach, so an empty one isn't asked for again.
    tried: Vec<Level>,
    /// Whether its overhead casts a shadow.
    casting: bool,
}

impl Patch {
    fn mesh(&mut self, level: Level) -> &mut Option<Entity> {
        match level {
            Level::Over => &mut self.over,
            Level::Near => &mut self.near,
            Level::Far => &mut self.far,
        }
    }
}

/// Whether a piece hangs overhead.
fn overhead(name: &str) -> bool {
    ["roof_", "tarp_", "awning_", "sheet_", "canopy_", "bamboo_"].iter().any(|k| name.starts_with(k))
}

/// Everything in a patch of street.
#[derive(Default)]
struct Content {
    placed: Vec<(&'static str, Vec3, f32)>,
    wires: Vec<Wire>,
    banners: Vec<Banner>,
}

/// The stand-in for a piece seen from afar, if it needs one: None leaves it out altogether, and a
/// piece light enough stays as it is.
fn far_piece(name: &'static str, roll: f32) -> Option<&'static str> {
    Some(match name {
        "stall_fruit" | "stall_tropic" | "stall_spice" => "far_fruit",
        "stall_veg" | "stall_green" | "stall_grain" => "far_veg",
        "stall_cloth" | "stall_cloth2" | "stall_toys" => "far_cloth",
        "stall_bags" | "stall_shoes" | "stall_steel" | "stall_snack" => "far_mixed",
        "shop_cloth" => "far_shop_b",
        "shop_bartan" => "far_shop_c",
        "shop_grocery" | "shop_general" => "far_shop_a",
        "patri_veg" | "patri_fruit" | "patri_roots" | "greens" => "far_patri_a",
        "patri_clothes" | "patri_shoes" | "patri_steel" => "far_patri_b",
        "scooter_black" | "scooter_grey" | "scooter_blue" | "scooter_maroon" => "far_scooter",
        "thela_LOD1" if roll < 2.0 => "thela_LOD1",
        _ => return None,
    })
}

fn load(mut goods: ResMut<Goods>, mut materials: ResMut<Assets<StandardMaterial>>) {
    goods.material = Some(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.82,
        reflectance: 0.3,
        cull_mode: None,
        double_sided: true,
        ..default()
    }));
    if cfg!(not(target_arch = "wasm32")) {
        goods.reading = Some(AsyncComputeTaskPool::get().spawn(async move { read_pieces().map_err(|e| warn!("life: goods: {e}")).ok() }));
    }
}

/// Another map: its patches went with it.
fn forget(mut goods: ResMut<Goods>) {
    goods.live.clear();
    goods.job = None;
}

fn patch_entity(commands: &mut Commands, mesh: Handle<Mesh>, material: Handle<StandardMaterial>, key: (i32, i32), level: Level, bounds: Aabb) -> Entity {
    let c = patch_centre(key);
    let (range, tag) = match level {
        Level::Over => (VisibilityRange { start_margin: 0.0..0.0, end_margin: FADE_FROM..FADE_TO, use_aabb: false }, "life over"),
        Level::Near => (VisibilityRange { start_margin: 0.0..0.0, end_margin: DETAIL_FROM..DETAIL_TO, use_aabb: false }, "life goods"),
        Level::Far => (VisibilityRange { start_margin: DETAIL_FROM..DETAIL_TO, end_margin: FADE_FROM..FADE_TO, use_aabb: false }, "life far"),
    };
    let mut e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_xyz(c.x, 0.0, c.y), bounds, NoAutoAabb, range, Tagged(tag), OfMap));
    if level != Level::Over {
        e.insert(bevy::light::NotShadowCaster);
    }
    e.id()
}

fn read_pieces() -> Result<(HashMap<String, Piece>, Image), String> {
    let path = std::path::Path::new(crate::ASSET_DIR).join("models/life/goods.glb");
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let atlas = std::fs::read(std::path::Path::new(crate::ASSET_DIR).join("models/life/goods_atlas.png")).map_err(|e| e.to_string())?;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let mut atlas = Image::from_buffer(&atlas, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, tiers::usage()).map_err(|e| e.to_string())?;
    super::flipbook::plain_mips(&mut atlas);
    let file = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
    let blob = file.blob.as_deref().ok_or("no binary chunk")?;
    let mut out = HashMap::new();
    let mut read_file = |file: &gltf::Gltf, blob: &[u8], only: &[&str]| -> Result<(), String> {
        for node in file.document.nodes() {
            let (Some(mesh), Some(name)) = (node.mesh(), node.name()) else { continue };
            if !only.is_empty() && !only.contains(&name) {
                continue;
            }
            let mut piece = Piece::default();
            for prim in mesh.primitives() {
                let reader = prim.reader(|_| Some(blob));
                let base = piece.pos.len() as u32;
                piece.pos.extend(reader.read_positions().ok_or("no positions")?.map(Vec3::from));
                piece.nrm.extend(reader.read_normals().ok_or("no normals")?.map(Vec3::from));
                let n = piece.pos.len() - piece.uv.len();
                match reader.read_tex_coords(0) {
                    Some(uv) => piece.uv.extend(uv.into_f32()),
                    None => piece.uv.extend(std::iter::repeat_n(WHITE_TEXEL, n)),
                }
                piece.col.extend(reader.read_colors(0).ok_or("no colours")?.into_rgba_f32());
                piece.idx.extend(reader.read_indices().ok_or("no indices")?.into_u32().map(|i| i + base));
            }
            out.insert(name.to_owned(), piece);
        }
        Ok(())
    };
    read_file(&file, blob, &[])?;
    // The street kit's own scooter and cart, parked as dressing. A dusty, old one: colour says
    // what can be ridden, and these can't.
    let kit_path = std::path::Path::new(crate::ASSET_DIR).join("models/props/street_kit.glb");
    let kit_bytes = std::fs::read(&kit_path).map_err(|e| e.to_string())?;
    let kit = gltf::Gltf::from_slice(&kit_bytes).map_err(|e| e.to_string())?;
    read_file(&kit, kit.blob.as_deref().ok_or("no binary chunk")?, &["scooter_LOD0", "thela_LOD1"])?;
    // Under this light the goods' plain colours came out as blocks of paint: cloth and tarps fade
    // and dull in a street's sun and dust, and the eye should go to the warriors.
    for (_, piece) in out.iter_mut().filter(|(name, _)| !name.contains("_LOD")) {
        for c in &mut piece.col {
            let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
            for v in c.iter_mut().take(3) {
                *v = (*v * 0.68 + grey * 0.32) * 0.88;
            }
        }
    }
    if let Some(p) = out.get_mut("thela_LOD1") {
        for c in &mut p.col {
            let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
            for v in c.iter_mut().take(3) {
                *v = (*v * 0.5 + grey * 0.5) * 0.7;
            }
        }
    }
    // The kit's scooter in the colours a street's old ones come in: its red body repainted.
    if let Some(red) = out.remove("scooter_LOD0") {
        for (name, paint) in [("scooter_black", [0.02, 0.02, 0.022]), ("scooter_grey", [0.16, 0.17, 0.18]), ("scooter_blue", [0.03, 0.07, 0.14]), ("scooter_maroon", [0.14, 0.025, 0.03])] {
            let col = red.col.iter().map(|c| if c[0] > 3.0 * c[1] { [paint[0], paint[1], paint[2], 1.0] } else { [c[0] * 0.7, c[1] * 0.7, c[2] * 0.7, 1.0] }).collect();
            out.insert(name.to_owned(), Piece { pos: red.pos.clone(), nrm: red.nrm.clone(), uv: red.uv.clone(), col, idx: red.idx.clone() });
        }
    }
    Ok((out, atlas))
}

fn pick<'a>(from: &[&'a str], roll: f32) -> &'a str {
    from[((roll * from.len() as f32) as usize).min(from.len() - 1)]
}

fn patch_at(p: Vec2) -> (i32, i32) {
    ((p.x / PATCH).floor() as i32, (p.y / PATCH).floor() as i32)
}

fn patch_centre(k: (i32, i32)) -> Vec2 {
    (Vec2::new(k.0 as f32, k.1 as f32) + 0.5) * PATCH
}

fn on_street(layout: &MapLayout, p: Vec2) -> bool {
    matches!(layout.terrain.ground_at(p.x, p.y), Some(ground::ROAD | ground::PAVEMENT | ground::PATH | ground::PLAZA | ground::PARKING))
}

/// What a row stall sells: it goes by where the stall stands, so it is the same every time.
fn row_goods(s: &super::Stall) -> &'static str {
    const STALLS: [&str; 13] = ["stall_fruit", "stall_tropic", "stall_veg", "stall_green", "stall_cloth", "stall_cloth2", "stall_bags", "stall_shoes", "stall_steel", "stall_spice", "stall_grain", "stall_toys", "stall_snack"];
    pick(&STALLS, hash(s.pos.x.to_bits(), s.pos.z.to_bits(), 1))
}

/// Everything in a patch of street, from the map.
fn dress(layout: &MapLayout, market: &Market, key: (i32, i32), parked: &[Vec2], out: &mut Vec<(&'static str, Vec3, f32)>, wires: &mut Vec<Wire>, banners: &mut Vec<Banner>) {
    let inside = |p: Vec2| patch_at(p) == key;
    let ground_y = |p: Vec2| layout.terrain.height(p.x, p.y);
    let mid = patch_centre(key);

    // The stalls.
    for id in market.stalls_near(mid, PATCH * 0.5 + 1.0) {
        let s = market.stalls[id as usize];
        let at = s.pos.xz();
        if !inside(at) {
            continue;
        }
        // A match's parked vehicle has the spot. A table is solid, and the map keeps it off the bays.
        if !matches!(s.kind, super::StallKind::Thela | super::StallKind::Row) && parked.iter().any(|p| p.distance_squared(at) < PARKED_CLEAR * PARKED_CLEAR) {
            continue;
        }
        let (a, b) = (at.x.to_bits(), at.y.to_bits());
        let busy = busy_at(market, at);
        let local = |x: f32, y: f32, z: f32| s.pos + Quat::from_rotation_y(s.yaw) * Vec3::new(x, y, z);
        match s.kind {
            super::StallKind::Thela => {
                out.push((pick(&["heap_orange_apple", "heap_banana_guava", "heap_tomato_onion", "heap_veg", "heap_cauli"], hash(a, b, 1)), s.pos, s.yaw));
                if hash(a, b, 2) < 0.4 + busy * 0.5 {
                    out.push((pick(&["canopy_blue", "canopy_orange", "canopy_white"], hash(a, b, 3)), s.pos, s.yaw));
                }
                // Stock under the cart, inside its collider.
                out.push((pick(&["crate_onion", "crate_tomato", "crate_coconut", "cartons"], hash(a, b, 4)), local(0.0, 0.0, 0.2), s.yaw + std::f32::consts::FRAC_PI_2));
            }
            super::StallKind::Crates => {
                out.push((pick(&["crate_tomato", "crate_onion", "crate_cauli", "crate_coconut"], hash(a, b, 1)), local(0.32, 0.3, 0.0), s.yaw));
            }
            super::StallKind::Patri => {
                out.push((pick(&["patri_veg", "patri_fruit", "patri_roots", "patri_clothes", "patri_shoes", "patri_steel", "greens"], hash(a, b, 1)), s.pos, s.yaw));
                if hash(a, b, 2) < 0.25 + busy * 0.35 {
                    out.push((pick(&["sheet_blue", "sheet_orange", "sheet_white"], hash(a, b, 3)), s.pos, s.yaw));
                }
            }
            super::StallKind::Row => {
                out.push((row_goods(&s), s.pos, s.yaw));
                out.push((pick(&["roof_blue", "roof_orange", "roof_white", "roof_green", "roof_red", "roof_blue", "roof_white"], hash(a, b, 2)), s.pos, s.yaw));
            }
            super::StallKind::Cart => {
                out.push(("thela_LOD1", s.pos, s.yaw));
                out.push((pick(&["heap_orange_apple", "heap_banana_guava", "heap_tomato_onion", "heap_veg", "heap_cauli"], hash(a, b, 1)), s.pos, s.yaw));
                if hash(a, b, 2) < 0.5 {
                    out.push((pick(&["canopy_blue", "canopy_orange", "canopy_white"], hash(a, b, 3)), s.pos, s.yaw));
                }
            }
            super::StallKind::Scooters => {
                // A bunch nose to the kerb, each at its own slant, a gap here and there.
                let bunch = 2 + (hash(a, b, 1) * 5.0) as i32;
                for k in 0..bunch {
                    if hash(a, b, 40 + k as u32) < 0.15 {
                        continue;
                    }
                    let lean = (hash(a, b, 50 + k as u32) - 0.5) * 0.5;
                    let x = (k as f32 - bunch as f32 / 2.0) * 0.85;
                    out.push((pick(&["scooter_black", "scooter_grey", "scooter_blue", "scooter_maroon", "scooter_black"], hash(a, b, 60 + k as u32)), local(x, 0.0, 0.0), s.yaw + 0.5 + lean));
                }
            }
            _ => {}
        }
    }

    // The shop fronts.
    for i in market.buildings_near(mid, PATCH * 0.5 + 2.0) {
        let fp = &layout.footprints[i as usize];
        if fp.mat == Mat::Water || fp.top - fp.base < 4.5 {
            continue;
        }
        let theme = hash(i, 11, 0);
        let area: f32 = fp.pts.iter().zip(fp.pts.iter().cycle().skip(1)).map(|(a, b)| a.perp_dot(*b)).sum();
        let n = fp.pts.len();
        for e in 0..n {
            let (a, b) = (fp.pts[e], fp.pts[(e + 1) % n]);
            let len = a.distance(b);
            if len < 3.0 {
                continue;
            }
            let dir = (b - a) / len;
            let out2 = if area > 0.0 { Vec2::new(dir.y, -dir.x) } else { Vec2::new(-dir.y, dir.x) };
            let bays = (len / BAY).floor().max(1.0) as i32;
            for s in 0..bays {
                let at = a + dir * ((s as f32 + 0.5) * len / bays as f32);
                if !inside(at) || layout.is_holy(at) {
                    continue;
                }
                let front = at + out2 * 2.0;
                if !on_street(layout, front) || market.built(layout, at + out2 * 1.0) || market.built(layout, at + out2 * 2.5) {
                    continue;
                }
                let y = ground_y(front);
                if (y - fp.base).abs() > 0.7 {
                    continue;
                }
                let busy = busy_at(market, front);
                // Awnings over a way in, but nothing in it: a room's doors and open bays stay clear.
                let open = layout.rooms_of(i as usize).is_some_and(|r| r.doors.iter().any(|d| d.pos.xz().distance(at) < d.width * 0.5 + BAY * 0.5 && d.out.dot(out2) > 0.7));
                let yaw = (-out2.x).atan2(-out2.y);
                let seed = (e as u32) * 97 + s as u32;
                let at3 = |off: f32| Vec3::new(at.x + out2.x * off, y, at.y + out2.y * off);
                // The shop's stock out at its door, floor to lintel.
                if !open && hash(i, seed, 2) < (busy * 2.4).min(0.96) {
                    let (front, floor): (&str, &[&str]) = if theme < 0.34 {
                        ("shop_cloth", &["bolts", "rack_short", "rack_cloth"])
                    } else if theme < 0.66 {
                        ("shop_grocery", &["sacks_spice", "sacks_grain", "sacks_stack", "stand_veg", "stand_fruit", "greens", "crate_cauli"])
                    } else if theme < 0.84 {
                        ("shop_bartan", &["pots", "buckets", "cartons"])
                    } else {
                        ("shop_general", &["cartons", "buckets", "sacks_stack", "stand_fruit"])
                    };
                    out.push((front, at3(0.02), yaw));
                    // And more at its feet, out on the pavement.
                    if hash(i, seed, 3) < (busy * 1.5).min(0.85) {
                        let name = pick(floor, hash(i, seed, 6));
                        let off = if name.starts_with("rack") { 0.5 } else { 0.72 };
                        out.push((name, at3(off), yaw));
                    }
                }
                let roll = hash(i, seed, 4);
                if roll < (busy * 1.4).min(0.8) {
                    out.push((pick(&["awning_red", "awning_blue", "awning_green", "awning_orange"], hash(i, e as u32, 5)), at3(0.02), yaw));
                } else if s % 2 == 0 && roll < 0.96 {
                    out.push((pick(&["tarp_blue", "tarp_orange", "tarp_grey", "tarp_green"], hash(i, seed, 7)), at3(0.02), yaw));
                }
            }
        }
    }

    // Banners and flags over the road.
    for (k, (a, b)) in market.banners.iter().enumerate() {
        if !inside(a.xz()) {
            continue;
        }
        let k = k as u32;
        out.push(("bamboo_tall", *a, 0.0));
        out.push(("bamboo_tall", *b, 0.0));
        let ropes = [(5.9, false), (5.2, true)];
        for (r, (height, cloth)) in ropes.iter().enumerate() {
            let (ta, tb) = (*a + Vec3::Y * *height, *b + Vec3::Y * *height);
            let sag = 0.05 * a.distance(*b);
            wires.push(Wire { a: ta, b: tb, sag });
            banners.push(Banner { a: ta, b: tb, sag, cloth: *cloth, tint: hash(k, r as u32, 12) });
        }
    }

    // The wires, from pole to pole, with a bundle dropping to the nearest wall.
    for (k, pole) in market.poles.iter().enumerate() {
        let at = pole.pos.xz();
        if !inside(at) {
            continue;
        }
        let busy = busy_at(market, at);
        let rot = Quat::from_rotation_y(pole.yaw);
        let arm = |x: f32, y: f32| pole.pos + rot * Vec3::new(x, y, 0.0);
        let mut near: Vec<&super::Stall> = market.poles.iter().filter(|q| { let d = q.pos.xz().distance(at); d > 8.0 && d < 46.0 }).collect();
        near.sort_by(|a, b| a.pos.xz().distance_squared(at).total_cmp(&b.pos.xz().distance_squared(at)));
        for (n, other) in near.iter().take(2).enumerate() {
            // Each span once, and none across a holy site.
            if other.pos.x + other.pos.z < pole.pos.x + pole.pos.z || layout.is_holy(pole.pos.lerp(other.pos, 0.5).xz()) {
                continue;
            }
            let other_rot = Quat::from_rotation_y(other.yaw);
            for w in 0..4 {
                let side = -0.8 + 0.53 * w as f32;
                let a = arm(side, 8.5);
                let b = other.pos + other_rot * Vec3::new(side, 8.5, 0.0);
                wires.push(Wire { a, b, sag: 0.5 + hash(k as u32, n as u32 * 7 + w, 1) * 0.5 });
            }
            // The low bundle every street grows.
            for w in 0..(1.0 + busy * 5.0) as u32 {
                let (ha, hb) = (5.2 + hash(k as u32, w, 2) * 1.8, 5.2 + hash(k as u32, w, 3) * 1.8);
                wires.push(Wire { a: pole.pos + Vec3::Y * ha, b: other.pos + Vec3::Y * hb, sag: 0.5 + hash(k as u32, w, 4) * 1.1 });
            }
            if hash(k as u32, n as u32, 9) < 0.3 {
                let m = pole.pos.lerp(other.pos, 0.3 + hash(k as u32, n as u32, 10) * 0.4);
                out.push(("crow", Vec3::new(m.x, 8.3 + m.y, m.z), (other.pos - pole.pos).xz().to_angle()));
            }
        }
        // To the nearest wall.
        let mut best: Option<(f32, Vec2)> = None;
        for t in 0..12 {
            let dir = Vec2::from_angle(t as f32 / 12.0 * std::f32::consts::TAU);
            for d in 3..20 {
                let p = at + dir * d as f32;
                if market.built(layout, p) {
                    if best.is_none_or(|b| (d as f32) < b.0) {
                        best = Some((d as f32, p));
                    }
                    break;
                }
            }
        }
        if let Some((_, wall)) = best.filter(|b| !layout.is_holy(b.1)) {
            for w in 0..(1.0 + busy * 3.5) as u32 {
                let h = 4.2 + hash(k as u32, w, 6) * 2.2;
                let shift = Vec3::new(hash(k as u32, w, 7) - 0.5, 0.0, hash(k as u32, w, 8) - 0.5);
                wires.push(Wire { a: pole.pos + Vec3::Y * (5.0 + hash(k as u32, w, 5) * 2.5), b: Vec3::new(wall.x, pole.pos.y + h, wall.y) + shift, sag: 0.3 + hash(k as u32, w, 9) * 0.5 });
            }
        }
    }
}

/// Merges a patch's pieces and wires into one mesh, its origin at the patch's middle.
fn merge(pieces: &HashMap<String, Piece>, placed: &[(&'static str, Vec3, f32)], wires: &[Wire], banners: &[Banner], origin: Vec3) -> Option<(Mesh, Aabb)> {
    let (mut pos, mut nrm, mut uv, mut col, mut idx): (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>, Vec<u32>) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
    for (name, at, yaw) in placed {
        let Some(p) = pieces.get(*name) else { continue };
        let rot = Quat::from_rotation_y(*yaw);
        let base = pos.len() as u32;
        for (v, n) in p.pos.iter().zip(&p.nrm) {
            let q = rot * *v + *at - origin;
            lo = lo.min(q);
            hi = hi.max(q);
            pos.push(q.to_array());
            nrm.push((rot * *n).to_array());
        }
        uv.extend_from_slice(&p.uv);
        col.extend_from_slice(&p.col);
        idx.extend(p.idx.iter().map(|i| i + base));
    }
    const STEPS: usize = 7;
    const WIDTH: f32 = 0.011;
    for w in wires {
        // Two crossed ribbons along a sagging curve: it reads as a line from any side.
        let along = (w.b - w.a).normalize_or(Vec3::X);
        let flat = along.cross(Vec3::Y).normalize_or(Vec3::X);
        for ribbon in [flat, Vec3::Y] {
            let base = pos.len() as u32;
            for s in 0..=STEPS {
                let t = s as f32 / STEPS as f32;
                let p = w.a.lerp(w.b, t) - Vec3::Y * w.sag * 4.0 * t * (1.0 - t) - origin;
                for side in [-WIDTH, WIDTH] {
                    let q = p + ribbon * side;
                    lo = lo.min(q);
                    hi = hi.max(q);
                    pos.push(q.to_array());
                    nrm.push([0.0, 1.0, 0.0]);
                    uv.push(WHITE_TEXEL);
                    col.push([0.012, 0.012, 0.013, 1.0]);
                }
            }
            for s in 0..STEPS as u32 {
                let k = base + s * 2;
                idx.extend_from_slice(&[k, k + 1, k + 2, k + 1, k + 3, k + 2]);
            }
        }
    }
    for b in banners {
        let curve = |t: f32| b.a.lerp(b.b, t) - Vec3::Y * b.sag * 4.0 * t * (1.0 - t) - origin;
        let len = b.a.distance(b.b);
        let across = (b.b - b.a).normalize_or(Vec3::X);
        let face = across.cross(Vec3::Y).normalize_or(Vec3::Z).to_array();
        if b.cloth {
            // A long plain cloth hung from the rope, in folds, no words on it.
            let c = BANNER_COLOURS[(b.tint * BANNER_COLOURS.len() as f32) as usize % BANNER_COLOURS.len()];
            let base = pos.len() as u32;
            let steps = ((len / 1.2) as usize).clamp(4, 24);
            for s in 0..=steps {
                let t = 0.12 + 0.76 * s as f32 / steps as f32;
                let top = curve(t);
                let fold = (s as f32 * 1.9).sin() * 0.03;
                for (v, drop) in [(0.0, 0.0), (1.0, 0.95)] {
                    let q = top - Vec3::Y * drop + Vec3::from_array(face) * fold * v;
                    lo = lo.min(q);
                    hi = hi.max(q);
                    pos.push(q.to_array());
                    nrm.push(face);
                    uv.push([LINEN.0 + LINEN.2 * (s % 2) as f32, LINEN.1 + LINEN.2 * v]);
                    let shade = 0.82 + 0.18 * (s as f32 * 1.9).cos().abs();
                    col.push([c[0] * shade, c[1] * shade, c[2] * shade, 1.0]);
                }
            }
            for s in 0..steps as u32 {
                let k = base + s * 2;
                idx.extend_from_slice(&[k, k + 1, k + 2, k + 1, k + 3, k + 2]);
            }
        } else {
            // Paper flags on the rope.
            let flags = ((len / 0.62) as usize).clamp(6, 60);
            for f in 0..flags {
                let t = (f as f32 + 0.5) / flags as f32;
                let (l, r, top) = (curve(t - 0.17 / len), curve(t + 0.17 / len), curve(t));
                let c = BANNER_COLOURS[f % BANNER_COLOURS.len()];
                let base = pos.len() as u32;
                for q in [l, r, top - Vec3::Y * 0.42] {
                    lo = lo.min(q);
                    hi = hi.max(q);
                    pos.push(q.to_array());
                    nrm.push(face);
                    uv.push(WHITE_TEXEL);
                    col.push([c[0], c[1], c[2], 1.0]);
                }
                idx.extend_from_slice(&[base, base + 1, base + 2]);
            }
        }
    }
    if pos.is_empty() {
        return None;
    }
    let count = pos.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, tiers::usage());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(if count > u16::MAX as usize { Indices::U32(idx) } else { Indices::U16(idx.iter().map(|i| *i as u16).collect()) });
    tiers::pack(&mut mesh);
    Some((mesh, Aabb::from_min_max(lo, hi)))
}

/// Keeps the patches near the camera in being, and only those.
#[allow(clippy::too_many_arguments)]
fn stream(
    mut commands: Commands,
    mut goods: ResMut<Goods>,
    time: Res<Time>,
    mut every: Local<f32>,
    market: Res<Market>,
    layout: Res<MapLayout>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    vehicles: Query<&Transform, With<shared::sim::vehicle::Vehicle>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let _spent = super::spent::Timed::start(&super::spent::GOODS);
    let goods = &mut *goods;
    if let Some(task) = &mut goods.reading
        && let Some(done) = block_on(poll_once(task))
    {
        goods.reading = None;
        if let Some((pieces, atlas)) = done {
            info!("life: {} kinds of goods", pieces.len());
            goods.pieces = Some(Arc::new(pieces));
            if let Some(mut m) = goods.material.as_ref().and_then(|m| materials.get_mut(m)) {
                m.base_color_texture = Some(images.add(atlas));
            }
        }
    }
    // A merge finished.
    if let Some((key, level, task)) = &mut goods.job
        && let Some(done) = block_on(poll_once(task))
    {
        let (key, level) = (*key, *level);
        goods.job = None;
        if let (Some((mesh, bounds)), Some(material), Some(patch)) = (done, goods.material.clone(), goods.live.get_mut(&key)) {
            *patch.mesh(level) = Some(patch_entity(&mut commands, meshes.add(mesh), material, key, level, bounds));
            patch.casting = patch.casting || level == Level::Over;
        }
    }
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.05;
    if !market.ready {
        return;
    }
    let Ok(camera) = camera.single() else { return };
    let eye = camera.translation();
    let at = eye.xz();
    let aloft = eye.y - layout.terrain.height(at.x, at.y) > TOO_HIGH;
    goods.live.retain(|key, patch| {
        let d = patch_centre(*key).distance(at);
        if aloft || d > DETAIL_LET_GO {
            if let Some(e) = patch.near.take() {
                commands.entity(e).despawn();
            }
            patch.tried.retain(|l| *l != Level::Near);
        }
        let keep = !aloft && d < LET_GO;
        if !keep {
            for e in [patch.far.take(), patch.over.take()].into_iter().flatten() {
                commands.entity(e).despawn();
            }
        } else if (d < SHADE_TO) != patch.casting
            && let Some(e) = patch.over
        {
            patch.casting = d < SHADE_TO;
            if patch.casting {
                commands.entity(e).remove::<bevy::light::NotShadowCaster>();
            } else {
                commands.entity(e).insert(bevy::light::NotShadowCaster);
            }
        }
        keep
    });
    if aloft || goods.job.is_some() {
        return;
    }
    let Some(pieces) = goods.pieces.clone() else { return };
    // The nearest thing missing, one at a time.
    let reach = (REACH / PATCH).ceil() as i32;
    let (ci, cj) = patch_at(at);
    let mut best: Option<(f32, (i32, i32), Level)> = None;
    for i in ci - reach..=ci + reach {
        for j in cj - reach..=cj + reach {
            let key = (i, j);
            let d = patch_centre(key).distance(at);
            if d >= REACH {
                continue;
            }
            let patch = goods.live.get(&key);
            // Overhead first: it is what shows from furthest. A patch that came out empty stays so.
            let wanted = [(Level::Over, d - 30.0, d < REACH), (Level::Near, d - 15.0, d < DETAIL_REACH), (Level::Far, d, d < REACH)];
            for (level, rank, needed) in wanted {
                let missing = patch.is_none_or(|p| match level {
                    Level::Over => p.over.is_none(),
                    Level::Near => p.near.is_none(),
                    Level::Far => p.far.is_none(),
                });
                let tried = patch.is_some_and(|p| p.tried.contains(&level));
                if needed && missing && !tried && best.is_none_or(|b| rank < b.0) {
                    best = Some((rank, key, level));
                }
            }
        }
    }
    let Some((_, key, level)) = best else { return };
    let patch = goods.live.entry(key).or_default();
    patch.tried.push(level);
    let content = patch
        .content
        .get_or_insert_with(|| {
            let mut c = Content::default();
            let parked: Vec<Vec2> = vehicles.iter().map(|t| t.translation.xz()).collect();
            dress(&layout, &market, key, &parked, &mut c.placed, &mut c.wires, &mut c.banners);
            if std::env::var("VIVAD_LIFE_LOG").is_ok() {
                let tris: usize = c.placed.iter().filter_map(|(n, ..)| pieces.get(*n)).map(|p| p.idx.len() / 3).sum();
                info!("goods patch {key:?}: {} pieces, {tris} tris, {} wires, {} banners", c.placed.len(), c.wires.len(), c.banners.len());
            }
            Arc::new(c)
        })
        .clone();
    let c = patch_centre(key);
    let origin = Vec3::new(c.x, 0.0, c.y);
    goods.job = Some((
        key,
        level,
        AsyncComputeTaskPool::get().spawn(async move {
            let of = |placed: Vec<(&'static str, Vec3, f32)>, wires: &[Wire], banners: &[Banner]| merge(&pieces, &placed, wires, banners, origin);
            match level {
                Level::Over => of(content.placed.iter().filter(|p| overhead(p.0)).copied().collect(), &content.wires, &content.banners),
                Level::Near => of(content.placed.iter().filter(|p| !overhead(p.0)).copied().collect(), &[], &[]),
                Level::Far => of(content.placed.iter().filter(|p| !overhead(p.0)).filter_map(|(n, p, y)| Some((far_piece(n, p.x.fract().abs())?, *p, *y))).collect(), &[], &[]),
            }
        }),
    ));
}

// ------------------------------------------------------------------ steam

/// A wisp over a kettle.
#[derive(Component)]
struct Puff {
    age: f32,
    life: f32,
    from: Vec3,
    drift: Vec2,
}

#[derive(Resource, Default)]
struct Steam {
    mesh: Option<Handle<Mesh>>,
    /// By how far gone a wisp is.
    fades: Vec<Handle<StandardMaterial>>,
    /// Kettles near the camera.
    kettles: Vec<Vec3>,
    next: f32,
    look: f32,
    /// Which kettle's turn it is.
    turn: usize,
    dice: u32,
}

const NEAR_KETTLE: f32 = 26.0;
const FADES: usize = 5;

fn puff_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let n = 32u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let d = (Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / n as f32 * 2.0 - Vec2::ONE).length();
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.4);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn steam(
    mut commands: Commands,
    mut state: ResMut<Steam>,
    time: Res<Time>,
    market: Res<Market>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let _spent = super::spent::Timed::start(&super::spent::GOODS);
    let dt = time.delta_secs();
    let state = &mut *state;
    let Ok(camera) = camera.single() else { return };
    let eye = camera.translation();
    state.look -= dt;
    if state.look <= 0.0 {
        state.look = 0.4;
        state.kettles.clear();
        for id in market.stalls_near(eye.xz(), NEAR_KETTLE) {
            let s = market.stalls[id as usize];
            if s.pos.distance(eye) >= NEAR_KETTLE {
                continue;
            }
            // A kettle, or a fry stall's pan.
            if s.kind == super::StallKind::Chai {
                state.kettles.push(s.pos + Quat::from_rotation_y(s.yaw) * Vec3::new(-0.4, 1.44, 0.0));
            } else if s.kind == super::StallKind::Row && row_goods(&s) == "stall_snack" {
                state.kettles.push(s.pos + Quat::from_rotation_y(s.yaw) * Vec3::new(-0.72, 1.12, 0.0));
            }
        }
        state.kettles.truncate(4);
    }
    if !state.kettles.is_empty() {
        if state.mesh.is_none() {
            state.mesh = Some(meshes.add(Rectangle::new(1.0, 1.0)));
            let texture = images.add(puff_image());
            state.fades = (0..FADES)
                .map(|k| {
                    let alpha = 0.34 * (1.0 - k as f32 / FADES as f32);
                    materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, alpha), base_color_texture: Some(texture.clone()), alpha_mode: AlphaMode::Blend, unlit: true, ..default() })
                })
                .collect();
        }
        state.next -= dt;
        if state.next <= 0.0 {
            state.next = 0.22;
            state.turn = state.turn.wrapping_add(1);
            state.dice = state.dice.wrapping_mul(1664525).wrapping_add(1013904223);
            let r = ((state.dice >> 16) & 0xff) as f32 / 255.0 - 0.5;
            let from = state.kettles[state.turn % state.kettles.len()];
            if let Some(mesh) = state.mesh.clone() {
                commands.spawn((
                    Puff { age: 0.0, life: 1.7 + r, from, drift: Vec2::new(r * 0.5, 0.18 - r * 0.2) },
                    Mesh3d(mesh),
                    MeshMaterial3d(state.fades[0].clone()),
                    Transform::from_translation(from).with_scale(Vec3::splat(0.1)),
                    bevy::light::NotShadowCaster,
                    bevy::light::NotShadowReceiver,
                    Tagged("life steam"),
                    OfMap,
                ));
            }
        }
    }
    let turn = camera.compute_transform().rotation;
    for (e, mut p, mut tf, mut mat) in &mut puffs {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        let k = p.age / p.life;
        let wobble = (p.age * 3.0 + p.life * 9.0).sin() * 0.06;
        tf.translation = p.from + Vec3::new(p.drift.x * p.age + wobble, 0.55 * p.age, p.drift.y * p.age);
        tf.rotation = turn;
        tf.scale = Vec3::splat(0.12 + 0.55 * k);
        let fade = ((k * FADES as f32) as usize).min(FADES - 1);
        if mat.0 != state.fades[fade] {
            mat.0 = state.fades[fade].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::map::{MapId, generate};
    use shared::sim::nav::StreetFloor;

    use super::*;

    /// Every piece the dressing asks for is in the files, in every patch of the busiest streets:
    /// a name that isn't would just leave a hole in a market.
    #[test]
    fn every_piece_asked_for_exists_and_merges() {
        let (pieces, _) = read_pieces().expect("the goods");
        let layout = generate(MapId::Ncr, 7);
        let market = Market::build(&layout, &StreetFloor::new(&layout));
        let mut seen = std::collections::HashSet::new();
        for centre in [Vec2::new(-1472.0, -1176.0), Vec2::new(-353.0, -1263.0), Vec2::new(-865.0, -762.0), Vec2::new(-604.0, 334.0)] {
            let key = patch_at(centre);
            for dj in -3..=3 {
                for di in -3..=3 {
                    let k = (key.0 + di, key.1 + dj);
                    let mut c = Content::default();
                    dress(&layout, &market, k, &[], &mut c.placed, &mut c.wires, &mut c.banners);
                    for (name, ..) in &c.placed {
                        assert!(pieces.contains_key(*name), "{name} isn't in the files");
                        seen.insert(*name);
                        if let Some(far) = far_piece(name, 0.0) {
                            assert!(pieces.contains_key(far), "{far} isn't in the files");
                        }
                    }
                    let origin = Vec3::ZERO;
                    if !c.placed.is_empty() {
                        assert!(merge(&pieces, &c.placed, &c.wires, &c.banners, origin).is_some());
                    }
                }
            }
        }
        assert!(seen.len() > 30, "only {} kinds of goods out", seen.len());
    }

    #[test]
    fn what_hangs_overhead_clears_a_warriors_head() {
        let (pieces, _) = read_pieces().expect("the goods");
        for (name, p) in pieces.iter().filter(|(n, _)| overhead(n)) {
            // Its cloth: the poles run to the ground, thin enough to walk past.
            let lowest_cloth = p.pos.iter().filter(|v| v.xz().length() > 0.0).map(|v| v.y).filter(|y| *y > 1.0).fold(f32::MAX, f32::min);
            assert!(lowest_cloth >= 1.9 || name.starts_with("bamboo"), "{name} hangs to {lowest_cloth}");
        }
    }
}
