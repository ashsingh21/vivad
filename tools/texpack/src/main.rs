//! texpack: PNG or JPEG in, mip-mapped BCn in a zstd-compressed KTX2 out, the one format the game's
//! Ultra level reads. `just textures-pack` runs it over tools/textures/ultra_pack.txt.
//!
//!   texpack pack <manifest>     encode every line of a manifest, in parallel
//!   texpack verify <manifest>   decode every output and measure it against its input
//!   texpack check <file.ktx2>…  read each file back and print what it holds
//!
//! A manifest line is `<kind> <input> <output>`, paths relative to the manifest's own directory
//! or absolute; `#` starts a comment. The kinds:
//!   color   sRGB colour, BC7. Mips are averaged in linear light, or distant walls go dark.
//!   data    linear data (ORM: occlusion, roughness, metalness), BC7.
//!   normal  a tangent-space normal map, +Y up. Mips are renormalised, BC5 keeps X and Y only (the
//!           game's shader rebuilds Z, and Bevy spots the format and does the same).
//!   height  one channel, BC4: parallax depth.
//!
//! The container is written here because no Rust KTX2 writer exists; `check` reads it back with
//! the same `ktx2` crate version Bevy 0.20 uses.

use std::fs;
use std::path::{Path, PathBuf};

use image::{GenericImageView, ImageReader};
use intel_tex_2::{RSurface, RgSurface, RgbaSurface, bc4, bc5, bc7};
use rayon::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Color,
    Data,
    Normal,
    Height,
}

impl Kind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "color" => Some(Kind::Color),
            "data" => Some(Kind::Data),
            "normal" => Some(Kind::Normal),
            "height" => Some(Kind::Height),
            _ => None,
        }
    }

    /// Vulkan format number, as the KTX2 header wants it.
    fn vk_format(self) -> u32 {
        match self {
            Kind::Color => 146,  // VK_FORMAT_BC7_SRGB_BLOCK
            Kind::Data => 145,   // VK_FORMAT_BC7_UNORM_BLOCK
            Kind::Normal => 141, // VK_FORMAT_BC5_UNORM_BLOCK
            Kind::Height => 139, // VK_FORMAT_BC4_UNORM_BLOCK
        }
    }

    /// Bytes per 4x4 block.
    fn block_bytes(self) -> usize {
        if self == Kind::Height { 8 } else { 16 }
    }

    /// The data format descriptor's colour model and its sample channel ids.
    fn dfd(self) -> (u8, &'static [u8]) {
        match self {
            Kind::Color | Kind::Data => (134, &[0]), // KHR_DF_MODEL_BC7
            Kind::Normal => (132, &[0, 1]),          // KHR_DF_MODEL_BC5: R, G
            Kind::Height => (131, &[0]),             // KHR_DF_MODEL_BC4: R
        }
    }
}

/// Floating point RGBA, row by row. Colour is held in linear light.
struct Level {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn load(path: &Path, kind: Kind) -> Result<Level, String> {
    let img = ImageReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?.with_guessed_format().map_err(|e| e.to_string())?.decode().map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = img.dimensions();
    if !w.is_power_of_two() || !h.is_power_of_two() {
        return Err(format!("{}: {w}x{h} is not a power of two", path.display()));
    }
    let rgba = img.to_rgba8();
    let px = rgba
        .pixels()
        .map(|p| {
            let c = [p[0], p[1], p[2], p[3]].map(|v| v as f32 / 255.0);
            match kind {
                Kind::Color => [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0],
                Kind::Data | Kind::Height => c,
                Kind::Normal => {
                    let n = [c[0] * 2.0 - 1.0, c[1] * 2.0 - 1.0, c[2] * 2.0 - 1.0];
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-5);
                    [n[0] / l, n[1] / l, n[2] / l, 1.0]
                }
            }
        })
        .collect();
    Ok(Level { w: w as usize, h: h as usize, px })
}

/// The next mip down: a 2x2 box, wrapping nothing (a tile's seam stays where it was).
fn halve(l: &Level, kind: Kind) -> Level {
    let (nw, nh) = ((l.w / 2).max(1), (l.h / 2).max(1));
    let (sx, sy) = (l.w / nw, l.h / nh);
    let mut px = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let mut s = [0.0f32; 4];
            for dy in 0..sy {
                for dx in 0..sx {
                    let p = l.px[(y * sy + dy) * l.w + x * sx + dx];
                    for c in 0..4 {
                        s[c] += p[c];
                    }
                }
            }
            let k = 1.0 / (sx * sy) as f32;
            let mut a = s.map(|v| v * k);
            if kind == Kind::Normal {
                let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-5);
                a = [a[0] / n, a[1] / n, a[2] / n, 1.0];
            }
            px.push(a);
        }
    }
    Level { w: nw, h: nh, px }
}

/// The level as bytes per texel, padded out to whole 4x4 blocks by repeating the edge.
fn to_bytes(l: &Level, kind: Kind) -> (usize, usize, Vec<u8>) {
    let (bw, bh) = (l.w.div_ceil(4) * 4, l.h.div_ceil(4) * 4);
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let channels = match kind {
        Kind::Color | Kind::Data => 4,
        Kind::Normal => 2,
        Kind::Height => 1,
    };
    let mut out = Vec::with_capacity(bw * bh * channels);
    for y in 0..bh {
        for x in 0..bw {
            let p = l.px[y.min(l.h - 1) * l.w + x.min(l.w - 1)];
            match kind {
                Kind::Color => out.extend_from_slice(&[byte(linear_to_srgb(p[0])), byte(linear_to_srgb(p[1])), byte(linear_to_srgb(p[2])), 255]),
                Kind::Data => out.extend_from_slice(&[byte(p[0]), byte(p[1]), byte(p[2]), 255]),
                Kind::Normal => out.extend_from_slice(&[byte(p[0] * 0.5 + 0.5), byte(p[1] * 0.5 + 0.5)]),
                Kind::Height => out.push(byte(p[0])),
            }
        }
    }
    (bw, bh, out)
}

fn encode(l: &Level, kind: Kind) -> Vec<u8> {
    let (width, height, data) = to_bytes(l, kind);
    let (w, h) = (width as u32, height as u32);
    match kind {
        Kind::Color | Kind::Data => bc7::compress_blocks(&bc7::opaque_basic_settings(), &RgbaSurface { data: &data, width: w, height: h, stride: w * 4 }),
        Kind::Normal => bc5::compress_blocks(&RgSurface { data: &data, width: w, height: h, stride: w * 2 }),
        Kind::Height => bc4::compress_blocks(&RSurface { data: &data, width: w, height: h, stride: w }),
    }
}

/// The KTX2 file: header, level index, data format descriptor, then the levels smallest first.
fn container(kind: Kind, w: u32, h: u32, levels: &[(Vec<u8>, usize)]) -> Vec<u8> {
    const ID: [u8; 12] = [0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A];
    let (model, samples) = kind.dfd();
    let block_size = 24 + 16 * samples.len() as u32;
    let mut dfd = Vec::new();
    dfd.extend_from_slice(&(4 + block_size).to_le_bytes());
    dfd.extend_from_slice(&0u32.to_le_bytes()); // vendor 0 (Khronos), descriptor type 0
    dfd.extend_from_slice(&2u16.to_le_bytes()); // descriptor version 2
    dfd.extend_from_slice(&(block_size as u16).to_le_bytes());
    let transfer = if kind == Kind::Color { 2 } else { 1 }; // sRGB or linear
    dfd.extend_from_slice(&[model, 1, transfer, 0]); // model, BT.709 primaries, transfer, flags
    dfd.extend_from_slice(&[3, 3, 0, 0]); // a block is 4x4 (stored as size minus one)
    dfd.extend_from_slice(&[0; 8]); // bytesPlane: 0 under supercompression
    for (i, &channel) in samples.iter().enumerate() {
        dfd.extend_from_slice(&((i as u16) * (kind.block_bytes() as u16 * 8 / samples.len() as u16)).to_le_bytes()); // bit offset
        dfd.push((kind.block_bytes() * 8 / samples.len() - 1) as u8); // bit length minus one
        dfd.push(channel);
        dfd.extend_from_slice(&[0; 4]); // sample position
        dfd.extend_from_slice(&0u32.to_le_bytes()); // sample lower
        dfd.extend_from_slice(&u32::MAX.to_le_bytes()); // sample upper
    }
    let index_end = 80 + 24 * levels.len();
    let data_start = index_end + dfd.len();
    let mut offsets = vec![0usize; levels.len()];
    let mut at = data_start;
    for i in (0..levels.len()).rev() {
        offsets[i] = at;
        at += levels[i].0.len();
    }
    let mut f = Vec::with_capacity(at);
    f.extend_from_slice(&ID);
    for v in [kind.vk_format(), 1, w, h, 0, 0, 1, levels.len() as u32, 2] {
        f.extend_from_slice(&v.to_le_bytes()); // type size 1, depth 0, layers 0, 1 face, 2 = zstd
    }
    f.extend_from_slice(&(index_end as u32).to_le_bytes());
    f.extend_from_slice(&(dfd.len() as u32).to_le_bytes());
    f.extend_from_slice(&[0; 8]); // no key/value data
    f.extend_from_slice(&[0; 16]); // no supercompression global data
    for (i, (bytes, raw)) in levels.iter().enumerate() {
        f.extend_from_slice(&(offsets[i] as u64).to_le_bytes());
        f.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        f.extend_from_slice(&(*raw as u64).to_le_bytes());
    }
    f.extend_from_slice(&dfd);
    for (bytes, _) in levels.iter().rev() {
        f.extend_from_slice(bytes);
    }
    f
}

fn pack(kind: Kind, input: &Path, output: &Path) -> Result<(usize, usize), String> {
    let base = load(input, kind)?;
    let (w, h) = (base.w as u32, base.h as u32);
    let mut chain = vec![base];
    while chain.last().is_some_and(|l| l.w > 1 || l.h > 1) {
        let next = halve(chain.last().unwrap(), kind);
        chain.push(next);
    }
    let raw_total: usize = chain.iter().map(|l| l.w.div_ceil(4) * l.h.div_ceil(4) * kind.block_bytes()).sum();
    let levels: Vec<(Vec<u8>, usize)> = chain
        .iter()
        .map(|l| {
            let raw = encode(l, kind);
            let packed = zstd::bulk::compress(&raw, 19).map_err(|e| e.to_string())?;
            Ok((packed, raw.len()))
        })
        .collect::<Result<_, String>>()?;
    let file = container(kind, w, h, &levels);
    if let Some(dir) = output.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(output, &file).map_err(|e| format!("{}: {e}", output.display()))?;
    Ok((file.len(), raw_total))
}

/// Reads a file back with the `ktx2` crate, unpacks each level and checks its size.
fn check(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let r = ktx2::Reader::new(&bytes[..]).map_err(|e| format!("{}: {e:?}", path.display()))?;
    let h = r.header();
    let format = h.format.ok_or_else(|| format!("{}: no format", path.display()))?;
    let block = match format {
        ktx2::Format::BC4_UNORM_BLOCK => 8,
        ktx2::Format::BC5_UNORM_BLOCK | ktx2::Format::BC7_SRGB_BLOCK | ktx2::Format::BC7_UNORM_BLOCK => 16,
        f => return Err(format!("{}: unexpected format {f:?}", path.display())),
    };
    let mut raw = 0usize;
    for (i, level) in r.levels().enumerate() {
        let data = zstd::decode_all(level.data).map_err(|e| format!("{} level {i}: {e}", path.display()))?;
        let (w, hgt) = (((h.pixel_width >> i).max(1)) as usize, ((h.pixel_height >> i).max(1)) as usize);
        let want = w.div_ceil(4) * hgt.div_ceil(4) * block;
        if data.len() != want || level.uncompressed_byte_length as usize != want {
            return Err(format!("{} level {i}: {} bytes, wanted {want}", path.display(), data.len()));
        }
        raw += data.len();
    }
    Ok(format!("{}: {}x{} {:?}, {} levels, {:.2} MB on disk, {:.2} MB in memory", path.display(), h.pixel_width, h.pixel_height, format, h.level_count, bytes.len() as f32 / 1048576.0, raw as f32 / 1048576.0))
}

/// Decodes a file's top level and measures it against the source image: the encoder's error, in
/// dB, on the channels the game reads.
fn verify(kind: Kind, input: &Path, output: &Path) -> Result<String, String> {
    let bytes = fs::read(output).map_err(|e| format!("{}: {e}", output.display()))?;
    let r = ktx2::Reader::new(&bytes[..]).map_err(|e| format!("{}: {e:?}", output.display()))?;
    let top = r.levels().next().ok_or("no levels")?;
    let data = zstd::decode_all(top.data).map_err(|e| e.to_string())?;
    let source = load(input, kind)?;
    let (want_w, want_h, want) = to_bytes(&source, kind);
    let (bw, bh) = (want_w / 4, want_h / 4);
    let channels = want.len() / (want_w * want_h);
    let mut got = vec![0u8; want.len()];
    let mut sum = 0.0f64;
    for by in 0..bh {
        for bx in 0..bw {
            let block = &data[(by * bw + bx) * kind.block_bytes()..];
            let at = (by * 4 * want_w + bx * 4) * channels;
            match kind {
                Kind::Color | Kind::Data => bcdec_rs::bc7(block, &mut got[at..], want_w * 4),
                Kind::Normal => bcdec_rs::bc5(block, &mut got[at..], want_w * 2, false),
                Kind::Height => bcdec_rs::bc4(block, &mut got[at..], want_w, false),
            }
        }
    }
    let read = if channels == 4 { 3 } else { channels };
    for (a, b) in got.chunks_exact(channels).zip(want.chunks_exact(channels)) {
        for c in 0..read {
            let d = a[c] as f64 - b[c] as f64;
            sum += d * d;
        }
    }
    let mse = sum / (want_w * want_h * read) as f64;
    let psnr = if mse > 0.0 { 10.0 * (255.0f64 * 255.0 / mse).log10() } else { 99.0 };
    if psnr < 34.0 {
        return Err(format!("{}: only {psnr:.1} dB against its source", output.display()));
    }
    Ok(format!("{}: {psnr:.1} dB against its source", output.display()))
}

/// A manifest's lines as (kind, input, output).
fn read_manifest(manifest: &Path) -> Result<Vec<(Kind, PathBuf, PathBuf)>, String> {
    let dir = manifest.parent().unwrap_or(Path::new(".")).to_path_buf();
    let text = fs::read_to_string(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let mut jobs = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [kind, input, output] = parts[..] else { return Err(format!("{}:{}: want <kind> <input> <output>", manifest.display(), n + 1)) };
        let kind = Kind::parse(kind).ok_or_else(|| format!("{}:{}: unknown kind {kind}", manifest.display(), n + 1))?;
        jobs.push((kind, dir.join(input), dir.join(output)));
    }
    Ok(jobs)
}

/// Runs a job over a manifest, in parallel; prints each result and fails if any did.
fn each(jobs: &[(Kind, PathBuf, PathBuf)], job: impl Fn(Kind, &Path, &Path) -> Result<String, String> + Sync) -> Result<(), String> {
    let results: Vec<Result<String, String>> = jobs.par_iter().map(|(kind, input, output)| job(*kind, input, output)).collect();
    let mut failed = false;
    for r in results {
        match r {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    if failed { Err("some textures failed".into()) } else { Ok(()) }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("pack") if args.len() == 2 => each(&read_manifest(Path::new(&args[1]))?, |kind, input, output| {
            let (disk, raw) = pack(kind, input, output)?;
            Ok(format!("{}: {:?}, {:.2} MB on disk, {:.2} MB in memory", output.display(), kind, disk as f32 / 1048576.0, raw as f32 / 1048576.0))
        }),
        Some("verify") if args.len() == 2 => each(&read_manifest(Path::new(&args[1]))?, verify),
        Some("check") if args.len() > 1 => {
            for f in &args[1..] {
                println!("{}", check(Path::new(f))?);
            }
            Ok(())
        }
        _ => Err("usage: texpack pack <manifest> | texpack verify <manifest> | texpack check <file.ktx2>...".into()),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("texpack: {e}");
        std::process::exit(1);
    }
}
