//! Flip books: a skinned model and its clips baked at load, on a worker thread, into plain meshes.
//!
//! The market's people and animals are many, near the player only, and never the subject of the
//! picture, so a bone rig of 53 joints each would be the wrong price: a person here is one entity
//! showing one mesh with one material, and animating it is handing it the next mesh. The meshes of
//! a look are shared by everyone wearing it, so they cost a few megabytes and no bone work at all.
//!
//! Nothing here is authored for the market. The bake reads the game's own files, by name, every
//! start: the lowest LOD in the model, and clips by the names asked for, in whatever rig the file
//! has. A new rig or better clips are picked up with no change here.
//!
//! The vertices come out with the model facing -Z, feet on y = 0, as a warrior stands in the world.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::buildings::tiers;

/// How a clip is taken.
#[derive(Clone, Copy, Debug)]
pub enum Take {
    /// Its whole length, in this many pictures: a loop.
    Cycle(usize),
    /// One picture, this far through it (0 to 1).
    Still(f32),
}

/// One thing a model can be shown doing.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    /// What the game calls it.
    pub key: &'static str,
    /// Clip names in the file, the first that exists being used.
    pub names: &'static [&'static str],
    pub take: Take,
    /// Knees bent over a seat, on top of the clip (a person only: it goes by bone names).
    pub sit: bool,
}

/// A way of dressing a model: the colours its outfit is repainted in, by height on the body.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// sRGB colours, or None to keep the file's own.
    pub upper: Option<[u8; 3]>,
    pub lower: Option<[u8; 3]>,
    pub shin: Option<[u8; 3]>,
    pub feet: Option<[u8; 3]>,
    /// Multiplies skin and hair.
    pub skin: f32,
    pub hair: f32,
}

impl Look {
    pub const PLAIN: Look = Look { upper: None, lower: None, shin: None, feet: None, skin: 1.0, hair: 1.0 };
}

/// What to bake.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Under the assets folder.
    pub model: String,
    /// Where the clips are, if not in the model.
    pub clips: Option<String>,
    pub shots: Vec<Shot>,
    /// Side of each look's texture.
    pub atlas: u32,
    pub looks: Vec<Look>,
    /// Ready-painted textures, under the assets folder, in place of `looks`.
    pub coats: Vec<String>,
    /// Metres to a unit of the file.
    pub scale: f32,
}

pub struct BakedClip {
    pub key: &'static str,
    pub cycle: f32,
    pub frames: Vec<Mesh>,
}

/// A book ready for the renderer, all but handing it over.
pub struct Baked {
    pub clips: Vec<BakedClip>,
    pub images: Vec<Image>,
    pub lo: Vec3,
    pub hi: Vec3,
}

/// A clip in the book.
pub struct Clip {
    pub key: &'static str,
    pub cycle: f32,
    pub frames: Vec<Handle<Mesh>>,
}

/// A model's pictures, and its looks.
pub struct Book {
    pub clips: Vec<Clip>,
    pub looks: Vec<Handle<StandardMaterial>>,
    pub bounds: Aabb,
}

impl Book {
    pub fn clip(&self, key: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.key == key)
    }
}

impl Baked {
    /// Hands the pictures to the renderer.
    pub fn install(self, meshes: &mut Assets<Mesh>, images: &mut Assets<Image>, materials: &mut Assets<StandardMaterial>) -> Book {
        let clips = self.clips.into_iter().map(|c| Clip { key: c.key, cycle: c.cycle, frames: c.frames.into_iter().map(|m| meshes.add(m)).collect() }).collect();
        let looks = self
            .images
            .into_iter()
            .map(|image| {
                materials.add(StandardMaterial {
                    base_color_texture: Some(images.add(image)),
                    perceptual_roughness: 0.88,
                    reflectance: 0.3,
                    ..default()
                })
            })
            .collect();
        Book { clips, looks, bounds: Aabb::from_min_max(self.lo, self.hi) }
    }
}

// ------------------------------------------------------------------ reading the file

/// A bone's or any node's place in the file.
struct Skeleton {
    names: Vec<String>,
    parent: Vec<Option<usize>>,
    rest: Vec<(Vec3, Quat, Vec3)>,
    /// Parents before their children.
    order: Vec<usize>,
    by_name: HashMap<String, usize>,
}

fn skeleton(doc: &gltf::Document) -> Skeleton {
    let n = doc.nodes().count();
    let mut parent = vec![None; n];
    let mut names = vec![String::new(); n];
    let mut rest = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); n];
    for node in doc.nodes() {
        let k = node.index();
        names[k] = node.name().unwrap_or_default().to_owned();
        let (t, r, s) = node.transform().decomposed();
        rest[k] = (Vec3::from(t), Quat::from_array(r), Vec3::from(s));
        for c in node.children() {
            parent[c.index()] = Some(k);
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut stack: Vec<usize> = (0..n).filter(|k| parent[*k].is_none()).collect();
    while let Some(k) = stack.pop() {
        order.push(k);
        stack.extend(doc.nodes().nth(k).into_iter().flat_map(|node| node.children().map(|c| c.index()).collect::<Vec<_>>()));
    }
    let by_name = names.iter().enumerate().map(|(k, n)| (n.clone(), k)).collect();
    Skeleton { names, parent, rest, order, by_name }
}

#[derive(Clone, Copy, PartialEq)]
enum Channel {
    Move,
    Turn,
    Size,
}

/// One bone's motion in a clip.
struct Track {
    node: usize,
    channel: Channel,
    step: bool,
    times: Vec<f32>,
    values: Vec<Vec4>,
}

impl Track {
    fn at(&self, t: f32) -> Vec4 {
        let k = self.times.partition_point(|x| *x <= t);
        if k == 0 {
            return self.values[0];
        }
        if k >= self.times.len() {
            return self.values[self.values.len() - 1];
        }
        let (a, b) = (self.values[k - 1], self.values[k]);
        if self.step {
            return a;
        }
        let s = (t - self.times[k - 1]) / (self.times[k] - self.times[k - 1]).max(1e-6);
        match self.channel {
            Channel::Turn => {
                let (qa, qb) = (Quat::from_vec4(a).normalize(), Quat::from_vec4(b).normalize());
                Vec4::from(qa.slerp(qb, s))
            }
            _ => a.lerp(b, s),
        }
    }
}

struct ClipData {
    tracks: Vec<Track>,
    length: f32,
}

/// The clips of `doc` by name, read against the bones of `skeleton` by their names.
fn clips<'a>(doc: &'a gltf::Document, blob: &'a [u8], skeleton: &Skeleton) -> HashMap<String, ClipData> {
    let mut out = HashMap::new();
    for anim in doc.animations() {
        let mut tracks = vec![];
        let mut length = 0.0f32;
        for ch in anim.channels() {
            let Some(name) = ch.target().node().name() else { continue };
            let Some(&node) = skeleton.by_name.get(name) else { continue };
            let reader = ch.reader(|_| Some(blob));
            let Some(times) = reader.read_inputs() else { continue };
            let times: Vec<f32> = times.collect();
            let Some(outputs) = reader.read_outputs() else { continue };
            use gltf::animation::util::ReadOutputs;
            let (channel, mut values): (Channel, Vec<Vec4>) = match outputs {
                ReadOutputs::Translations(v) => (Channel::Move, v.map(|p| Vec3::from(p).extend(0.0)).collect()),
                ReadOutputs::Rotations(v) => (Channel::Turn, v.into_f32().map(Vec4::from).collect()),
                ReadOutputs::Scales(v) => (Channel::Size, v.map(|p| Vec3::from(p).extend(0.0)).collect()),
                ReadOutputs::MorphTargetWeights(_) => continue,
            };
            use gltf::animation::Interpolation as I;
            let interpolation = ch.sampler().interpolation();
            if interpolation == I::CubicSpline {
                // In tangent, value, out tangent: the values alone are near enough for a low LOD.
                values = values.chunks(3).filter_map(|c| c.get(1).copied()).collect();
            }
            if values.is_empty() || values.len() != times.len() {
                continue;
            }
            length = length.max(times.last().copied().unwrap_or(0.0));
            tracks.push(Track { node, channel, step: interpolation == I::Step, times, values });
        }
        out.insert(anim.name().unwrap_or_default().to_owned(), ClipData { tracks, length });
    }
    out
}

/// Every node's place in the model at time `t` of `clip`.
fn pose(s: &Skeleton, clip: &ClipData, t: f32, sit: bool) -> Vec<Mat4> {
    let mut local = s.rest.clone();
    for track in &clip.tracks {
        let v = track.at(t);
        let slot = &mut local[track.node];
        match track.channel {
            Channel::Move => slot.0 = v.truncate(),
            Channel::Turn => slot.1 = Quat::from_vec4(v).normalize(),
            Channel::Size => slot.2 = v.truncate(),
        }
    }
    let mut global = vec![Mat4::IDENTITY; local.len()];
    for &k in &s.order {
        let (t, r, z) = local[k];
        let own = Mat4::from_scale_rotation_translation(z, r, t);
        let g = s.parent[k].map_or(own, |p| global[p] * own);
        global[k] = if sit { seated(&s.names[k], g) } else { g };
    }
    global
}

/// Knees bent: the thighs swing forward (the model faces +Z) about the hips, then the shins back
/// to upright about the knees. The clip's bones above carry over.
fn seated(name: &str, g: Mat4) -> Mat4 {
    const BEND: f32 = 1.45;
    let about = |g: Mat4, turn: f32| {
        let at = g.w_axis.truncate();
        Mat4::from_translation(at) * Mat4::from_rotation_x(turn) * Mat4::from_translation(-at) * g
    };
    match name {
        "thigh_l" | "thigh_r" => about(g, -BEND),
        "calf_l" | "calf_r" => about(g, BEND),
        _ => g,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Outfit,
    Skin,
    Hair,
    Other,
}

/// A mesh of the model with its skin.
struct Piece {
    part: Part,
    image: Option<usize>,
    colour: [f32; 4],
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    uv: Vec<Vec2>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    idx: Vec<u32>,
    /// The vertices of a warrior's pack, flattened (`flatten_pack`): their cloth is painted over.
    pack: Vec<bool>,
    /// The body under the clothes.
    body: bool,
}

/// Whether a joint moves skin that shows past the clothes.
fn bare(name: &str) -> bool {
    ["hand", "lowerarm", "foot", "ball", "index", "middle", "ring", "pinky", "thumb", "neck", "head"].iter().any(|k| name.contains(k))
}

/// The body's faces that clothes cover cost the most and are never seen. Nothing is lost by
/// leaving them out of every picture.
fn strip_covered(piece: &mut Piece, joint_bare: &[bool]) {
    let shown = |v: u32| {
        let (j, w) = (piece.joints[v as usize], piece.weights[v as usize]);
        let top = (0..4).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
        joint_bare.get(j[top] as usize).copied().unwrap_or(true)
    };
    let kept: Vec<u32> = piece.idx.chunks_exact(3).filter(|t| t.iter().any(|v| shown(*v))).flatten().copied().collect();
    piece.idx = kept;
}

/// The lowest detail the model has: its highest `..._LOD<n>` nodes, or all of it if it has none.
fn lowest_lod(doc: &gltf::Document) -> Option<usize> {
    doc.nodes().filter(|n| n.mesh().is_some()).filter_map(|n| n.name()?.rsplit("LOD").next()?.parse::<usize>().ok()).max()
}

fn pieces(doc: &gltf::Document, blob: &[u8]) -> Result<Vec<Piece>, String> {
    let lod = lowest_lod(doc);
    let mut out = vec![];
    for node in doc.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        if node.skin().is_none() {
            continue;
        }
        if let Some(lod) = lod {
            let here = node.name().and_then(|n| n.rsplit("LOD").next()?.parse::<usize>().ok());
            if here != Some(lod) {
                continue;
            }
        }
        for prim in mesh.primitives() {
            let material = prim.material();
            let name = material.name().unwrap_or_default().to_lowercase();
            let part = if name.contains("outfit") {
                Part::Outfit
            } else if name.contains("head") || name.contains("body") {
                Part::Skin
            } else if name.contains("hair") {
                Part::Hair
            } else {
                Part::Other
            };
            let pbr = material.pbr_metallic_roughness();
            let reader = prim.reader(|_| Some(blob));
            let pos: Vec<Vec3> = reader.read_positions().ok_or("no positions")?.map(Vec3::from).collect();
            let nrm: Vec<Vec3> = reader.read_normals().ok_or("no normals")?.map(Vec3::from).collect();
            let uv: Vec<Vec2> = reader.read_tex_coords(0).map_or_else(|| vec![Vec2::ZERO; pos.len()], |c| c.into_f32().map(Vec2::from).collect());
            let joints: Vec<[u16; 4]> = reader.read_joints(0).ok_or("no joints")?.into_u16().collect();
            let weights: Vec<[f32; 4]> = reader.read_weights(0).ok_or("no weights")?.into_f32().collect();
            let idx: Vec<u32> = reader.read_indices().ok_or("no indices")?.into_u32().collect();
            let image = pbr.base_color_texture().map(|t| t.texture().source().index());
            let pack = vec![false; pos.len()];
            let body = name.contains("body");
            out.push(Piece { part, image, colour: pbr.base_color_factor(), pos, nrm, uv, joints, weights, idx, pack, body });
        }
    }
    if out.is_empty() {
        return Err("no skinned mesh".into());
    }
    Ok(out)
}

/// A warrior's pack is what a warrior looks like from behind, so a shopper's goes flat against the
/// back, where it is only more shirt. Model space: +Z is forward, feet at y = 0.
fn flatten_pack(piece: &mut Piece) {
    const BACK: f32 = -0.1;
    for ((p, n), pack) in piece.pos.iter_mut().zip(&mut piece.nrm).zip(&mut piece.pack) {
        if p.y > 0.8 && p.z < BACK {
            p.z = BACK + (p.z - BACK) * 0.1;
            // Lit as the flat back it now is: its old sides would still draw its outline.
            *n = (*n * 0.15 + Vec3::NEG_Z).normalize();
            *pack = true;
        }
    }
}

// ------------------------------------------------------------------ the texture

/// A square of the atlas.
#[derive(Clone, Copy)]
struct Cell {
    x: u32,
    y: u32,
    side: u32,
}

/// The file's image as RGBA bytes, with its size.
fn decode(image: &gltf::Image, blob: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let gltf::image::Source::View { view, mime_type } = image.source() else { return None };
    let bytes = blob.get(view.offset()..view.offset() + view.length())?;
    let img = Image::from_buffer(bytes, ImageType::MimeType(mime_type), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::MAIN_WORLD).ok()?;
    let (w, h) = (img.width(), img.height());
    let data = img.data?;
    (data.len() == (w * h * 4) as usize).then_some((data, w, h))
}

/// Which way up the body a joint is: 0 chest and arms, 1 hips and thighs, 2 shins, 3 feet.
fn height_class(name: &str) -> u8 {
    if name.contains("foot") || name.contains("ball") || name.contains("toe") {
        3
    } else if name.contains("calf") {
        2
    } else if name.contains("pelvis") || name.contains("thigh") {
        1
    } else {
        0
    }
}

/// Shrinks `src` into a cell of the atlas: alpha-weighted, so a card's gaps don't darken its edges.
fn blit(atlas: &mut [u8], side: u32, cell: Cell, src: &[u8], w: u32, h: u32) {
    for y in 0..cell.side {
        for x in 0..cell.side {
            let (x0, x1) = (x * w / cell.side, ((x + 1) * w / cell.side).max(x * w / cell.side + 1));
            let (y0, y1) = (y * h / cell.side, ((y + 1) * h / cell.side).max(y * h / cell.side + 1));
            let (mut sum, mut alpha, mut n) = ([0u32; 3], 0u32, 0u32);
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let p = &src[((sy * w + sx) * 4) as usize..][..4];
                    let a = p[3] as u32;
                    for c in 0..3 {
                        sum[c] += p[c] as u32 * a;
                    }
                    alpha += a;
                    n += 1;
                }
            }
            let o = (((cell.y + y) * side + cell.x + x) * 4) as usize;
            for c in 0..3 {
                atlas[o + c] = if alpha > 0 { (sum[c] / alpha) as u8 } else { 128 };
            }
            // The low LOD draws its hair solid, so a card's alpha goes no further than this.
            atlas[o + 3] = 255;
            let _ = n;
        }
    }
}

/// The class of a texel on a warrior's pack and straps.
const PACK: u8 = 4;

/// Which body class each texel of an outfit's cell is, from the triangles laid over it.
fn outfit_classes(piece: &Piece, joint_class: &[u8], side: u32) -> Vec<u8> {
    let mut grid = vec![255u8; (side * side) as usize];
    let class_of = |v: usize| {
        let (j, w) = (piece.joints[v], piece.weights[v]);
        let top = (0..4).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
        joint_class.get(j[top] as usize).copied().unwrap_or(0)
    };
    for tri in piece.idx.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let classes = [class_of(a), class_of(b), class_of(c)];
        let class = if piece.pack[a] || piece.pack[b] || piece.pack[c] {
            PACK
        } else if classes[1] == classes[2] {
            classes[1]
        } else {
            classes[0]
        };
        let wrap = |v: usize| Vec2::new(piece.uv[v].x.rem_euclid(1.0), piece.uv[v].y.rem_euclid(1.0)) * side as f32;
        let (pa, pb, pc) = (wrap(a), wrap(b), wrap(c));
        let (lo, hi) = (pa.min(pb).min(pc).floor(), pa.max(pb).max(pc).ceil());
        let area = (pb - pa).perp_dot(pc - pa);
        if area.abs() < 1e-5 {
            continue;
        }
        for y in lo.y as u32..(hi.y as u32).min(side) {
            for x in lo.x as u32..(hi.x as u32).min(side) {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let (u, v) = ((p - pa).perp_dot(pc - pa) / area, (pb - pa).perp_dot(p - pa) / area);
                if u >= -0.02 && v >= -0.02 && u + v <= 1.04 {
                    grid[(y * side + x) as usize] = class;
                }
            }
        }
    }
    grid
}

/// A look's copy of the atlas.
fn dress(atlas: &[u8], side: u32, cells: &[(Part, Cell)], classes: &[Vec<u8>], look: &Look) -> Vec<u8> {
    let mut out = atlas.to_vec();
    let mut outfits = 0;
    for (part, cell) in cells {
        let classes = (*part == Part::Outfit).then(|| {
            outfits += 1;
            &classes[outfits - 1]
        });
        // The cloth's own folds stay: each class's mean brightness maps to the new colour.
        let mut mean = [(0.0f32, 0u32); 4];
        if let Some(classes) = classes {
            for y in 0..cell.side {
                for x in 0..cell.side {
                    let class = classes[(y * cell.side + x) as usize];
                    if class < 4 {
                        let p = &atlas[(((cell.y + y) * side + cell.x + x) * 4) as usize..][..3];
                        mean[class as usize].0 += luma(p);
                        mean[class as usize].1 += 1;
                    }
                }
            }
        }
        for y in 0..cell.side {
            for x in 0..cell.side {
                let o = (((cell.y + y) * side + cell.x + x) * 4) as usize;
                match (part, classes) {
                    (Part::Outfit, Some(classes)) => {
                        let class = classes[(y * cell.side + x) as usize];
                        // A pack is plain shirt now, with none of its own shading.
                        let colour = [look.upper, look.lower, look.shin, look.feet, look.upper].get(class as usize).copied().flatten();
                        if let Some(colour) = colour {
                            let (sum, n) = mean.get(class as usize).copied().unwrap_or((0.0, 0));
                            let (lo, hi) = if class == 0 { (0.8, 1.15) } else { (0.6, 1.3) };
                            let shade = if class == PACK { 0.92 } else { (luma(&out[o..o + 3]) / (sum / n.max(1) as f32).max(0.02)).clamp(lo, hi) };
                            for c in 0..3 {
                                out[o + c] = (colour[c] as f32 * shade).min(255.0) as u8;
                            }
                        }
                    }
                    (Part::Skin, _) => out[o..o + 3].iter_mut().for_each(|c| *c = (*c as f32 * look.skin).min(255.0) as u8),
                    (Part::Hair, _) => out[o..o + 3].iter_mut().for_each(|c| *c = (*c as f32 * look.hair).min(255.0) as u8),
                    _ => {}
                }
            }
        }
    }
    out
}

fn luma(p: &[u8]) -> f32 {
    (0.3 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32) / 255.0
}

// ------------------------------------------------------------------ the bake

/// Bakes a model. Runs for a second or so: on a worker.
pub fn bake(spec: &Spec) -> Result<Baked, String> {
    let read = |path: &str| std::fs::read(std::path::Path::new(crate::ASSET_DIR).join(path)).map_err(|e| format!("{path}: {e}"));
    let model = read(&spec.model)?;
    let file = gltf::Gltf::from_slice(&model).map_err(|e| e.to_string())?;
    let blob = file.blob.as_deref().ok_or("no binary chunk")?;
    let doc = &file.document;
    let skel = skeleton(doc);
    let skin = doc.skins().next().ok_or("no skin")?;
    let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
    let binds: Vec<Mat4> = skin.reader(|_| Some(blob)).read_inverse_bind_matrices().ok_or("no bind matrices")?.map(|m| Mat4::from_cols_array_2d(&m)).collect();
    let joint_class: Vec<u8> = joints.iter().map(|j| height_class(&skel.names[*j])).collect();
    let mut parts = pieces(doc, blob)?;
    if spec.coats.is_empty() {
        parts.iter_mut().filter(|p| p.part == Part::Outfit).for_each(flatten_pack);
        let joint_bare: Vec<bool> = joints.iter().map(|j| bare(&skel.names[*j])).collect();
        parts.iter_mut().filter(|p| p.body).for_each(|p| strip_covered(p, &joint_bare));
    }
    // Biggest first: it gets the big square of the atlas.
    parts.sort_by_key(|p| std::cmp::Reverse(p.idx.len() + if p.part == Part::Outfit { 1 << 24 } else { 0 }));

    // The clips' file, if it isn't this one.
    let clip_bytes = spec.clips.as_ref().map(|p| read(p)).transpose()?;
    let clip_file = clip_bytes.as_ref().map(|b| gltf::Gltf::from_slice(b)).transpose().map_err(|e| e.to_string())?;
    let all = match &clip_file {
        Some(f) => clips(&f.document, f.blob.as_deref().ok_or("no binary chunk")?, &skel),
        None => clips(doc, blob, &skel),
    };

    // The atlas.
    let side = spec.atlas.max(4);
    let mut atlas = vec![128u8; (side * side * 4) as usize];
    let images: Vec<gltf::Image> = doc.images().collect();
    let mut cells: Vec<(Part, Cell)> = vec![];
    let (big, small) = (side / 4 * 3, side / 4);
    for (k, part) in parts.iter().enumerate() {
        let cell = match (k, parts.len()) {
            (_, 1) => Cell { x: 0, y: 0, side },
            (0, _) => Cell { x: 0, y: 0, side: big },
            (k, _) if k <= 4 => Cell { x: big, y: (k as u32 - 1) * small, side: small },
            (k, _) => Cell { x: ((k as u32 - 5) % 3) * small, y: big, side: small },
        };
        // One image at a time: each is 16 MB, and a model has several.
        let src = if spec.coats.is_empty() { part.image.and_then(|i| decode(&images[i], blob)) } else { None };
        match src {
            Some((data, w, h)) => blit(&mut atlas, side, cell, &data, w, h),
            None => {
                let c = part.colour.map(|v| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8);
                for y in 0..cell.side {
                    for x in 0..cell.side {
                        let o = (((cell.y + y) * side + cell.x + x) * 4) as usize;
                        atlas[o..o + 3].copy_from_slice(&c[..3]);
                    }
                }
            }
        }
        cells.push((part.part, cell));
    }
    let classes: Vec<Vec<u8>> = parts.iter().zip(&cells).filter(|(p, _)| p.part == Part::Outfit).map(|(p, (_, c))| outfit_classes(p, &joint_class, c.side)).collect();
    let coats = spec.coats.iter().map(|path| {
        let bytes = read(path)?;
        let mut image = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, tiers::usage()).map_err(|e| format!("{path}: {e}"))?;
        crate::buildings::mip_chain(&mut image);
        Ok::<Image, String>(image)
    });
    let coats: Vec<Image> = coats.collect::<Result<_, _>>()?;
    let looks = spec
        .looks
        .iter()
        .map(|look| {
            let mut image = Image::new(
                Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                TextureDimension::D2,
                dress(&atlas, side, &cells, &classes, look),
                TextureFormat::Rgba8UnormSrgb,
                tiers::usage(),
            );
            crate::buildings::mip_chain(&mut image);
            image
        });
    let images: Vec<Image> = coats.into_iter().chain(looks).collect();

    // One vertex list for the whole model.
    let (mut pos, mut nrm, mut uv, mut skin_j, mut skin_w, mut idx) = (vec![], vec![], vec![], vec![], vec![], vec![]);
    for (part, (_, cell)) in parts.iter().zip(&cells) {
        let base = pos.len() as u32;
        pos.extend_from_slice(&part.pos);
        nrm.extend_from_slice(&part.nrm);
        let (at, size) = (Vec2::new(cell.x as f32, cell.y as f32) / side as f32, cell.side as f32 / side as f32);
        uv.extend(part.uv.iter().map(|t| {
            // Half a texel in, so a cell's neighbour doesn't bleed in.
            let wrapped = Vec2::new(t.x.rem_euclid(1.0), t.y.rem_euclid(1.0));
            let inset = 1.0 / cell.side as f32;
            (at + wrapped.clamp(Vec2::splat(inset), Vec2::splat(1.0 - inset)) * size).to_array()
        }));
        skin_j.extend_from_slice(&part.joints);
        skin_w.extend_from_slice(&part.weights);
        idx.extend(part.idx.iter().map(|i| i + base));
    }
    let wide = pos.len() > u16::MAX as usize;
    let indices = || if wide { Indices::U32(idx.clone()) } else { Indices::U16(idx.iter().map(|i| *i as u16).collect()) };

    // The pictures.
    let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
    let mut baked = vec![];
    for shot in &spec.shots {
        let clip = shot.names.iter().find_map(|n| all.get(*n)).ok_or_else(|| format!("{}: none of {:?}", spec.model, shot.names))?;
        let times: Vec<f32> = match shot.take {
            Take::Cycle(n) => (0..n).map(|k| k as f32 / n as f32 * clip.length).collect(),
            Take::Still(at) => vec![at.clamp(0.0, 1.0) * clip.length],
        };
        let mut posed = vec![];
        let mut floor = f32::MAX;
        for t in &times {
            let globals = pose(&skel, clip, *t, shot.sit);
            let matrices: Vec<Mat4> = joints.iter().zip(&binds).map(|(j, b)| globals[*j] * *b).collect();
            let mut out_pos = Vec::with_capacity(pos.len());
            let mut out_nrm = Vec::with_capacity(pos.len());
            for k in 0..pos.len() {
                let (j, w) = (skin_j[k], skin_w[k]);
                let (mut p, mut n) = (Vec3::ZERO, Vec3::ZERO);
                for i in 0..4 {
                    if w[i] > 0.0 {
                        let m = matrices.get(j[i] as usize).copied().unwrap_or(Mat4::IDENTITY);
                        p += m.transform_point3(pos[k]) * w[i];
                        n += m.transform_vector3(nrm[k]) * w[i];
                    }
                }
                // Turned to face -Z, as everything in the world does.
                let p = Vec3::new(-p.x, p.y, -p.z) * spec.scale;
                floor = floor.min(p.y);
                out_pos.push(p);
                out_nrm.push(Vec3::new(-n.x, n.y, -n.z).normalize_or(Vec3::Y).to_array());
            }
            posed.push((out_pos, out_nrm));
        }
        let mut frames = vec![];
        for (mut p, n) in posed {
            // Feet (or a sitter's) on the ground.
            for q in &mut p {
                q.y -= floor;
                lo = lo.min(*q);
                hi = hi.max(*q);
            }
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, tiers::usage());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, p.iter().map(|q| q.to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, n);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
            mesh.insert_indices(indices());
            tiers::pack(&mut mesh);
            frames.push(mesh);
        }
        baked.push(BakedClip { key: shot.key, cycle: clip.length.max(0.05), frames });
    }
    Ok(Baked { clips: baked, images, lo, hi })
}
