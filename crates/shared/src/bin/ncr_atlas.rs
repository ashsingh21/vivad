//! Draw the one NCR map from the game's own data: an SVG atlas with a picture of the ground, hills,
//! water and buildings inside it, and the names, bridges and loot on top.
//!
//!   cargo run --release -p shared --bin ncr_atlas -- [out.svg]     (or `just ncr-atlas`)
//!
//! Open the SVG in any browser. Map data (c) OpenStreetMap contributors, ODbL 1.0.
use bevy::{
    asset::RenderAssetUsages,
    image::Image,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use shared::map::{MapLayout, Mat, Shape, compose, footprint, terrain::ground};
use std::fmt::Write;

/// The picture's side in pixels: 2 m a pixel on the 4 km map.
const SIDE: usize = 2000;
/// Where the map sits in the page, and the page itself.
const LEFT: f32 = 50.0;
const TOP: f32 = 130.0;
const PAGE_W: f32 = 2520.0;
const PAGE_H: f32 = 2200.0;
/// The rivers and the ridge: (name, x, z, turn in degrees, size).
const LAND: [(&str, f32, f32, f32, f32); 5] = [
    ("YAMUNA", 225.0, -370.0, 84.0, 34.0),
    ("YAMUNA", 840.0, 1230.0, 62.0, 34.0),
    ("HINDON", 1600.0, -640.0, 83.0, 22.0),
    ("DELHI RIDGE", -1225.0, -320.0, 90.0, 22.0),
    ("DELHI RIDGE", -640.0, 1450.0, 26.0, 22.0),
];
/// Main roads, named where they run clear of everything else: (name, x, z, turn, size).
const ROADS: [(&str, f32, f32, f32, f32); 7] = [
    ("RING ROAD", -1120.0, -1800.0, -6.0, 15.0),
    ("RING ROAD", -1800.0, -1090.0, 86.0, 15.0),
    ("NH-48", -1400.0, 460.0, -68.0, 15.0),
    ("MATHURA ROAD", 20.0, 1320.0, 90.0, 15.0),
    ("NH-9", 1180.0, -312.0, 0.0, 15.0),
    ("NOIDA EXPRESSWAY", 1507.0, 830.0, 88.0, 15.0),
    ("MG ROAD", -1040.0, 1150.0, -10.0, 15.0),
];

/// The ground, hills, water and buildings, RGBA.
fn picture(map: &MapLayout) -> Vec<u8> {
    let (t, half) = (&map.terrain, map.half_extent);
    let at = |k: usize| (k as f32 + 0.5) / SIDE as f32 * 2.0 * half - half;
    let mut px = vec![255u8; SIDE * SIDE * 4];
    let light = Vec3::new(-0.55, 0.62, -0.55).normalize();
    for j in 0..SIDE {
        for i in 0..SIDE {
            let (x, z) = (at(i), at(j));
            let base = match t.ground_at(x, z) {
                Some(ground::ROAD) => [70.0, 74.0, 80.0],
                Some(ground::PARK) => [172.0, 186.0, 136.0],
                Some(ground::WATER) => [76.0, 146.0, 168.0],
                Some(ground::PATH) => [194.0, 174.0, 134.0],
                Some(ground::PARKING) => [136.0, 136.0, 136.0],
                Some(ground::PLAZA) => [205.0, 200.0, 190.0],
                Some(ground::OPEN) => [186.0, 184.0, 138.0],
                Some(ground::SAND) => [212.0, 194.0, 148.0],
                _ => [216.0, 210.0, 196.0],
            };
            // Hills: lit from the north-west, greener and darker with height.
            let h = t.height(x, z).clamp(0.0, 40.0);
            let shade = (t.normal(x, z).dot(light) / light.y).clamp(0.72, 1.22);
            let green = 1.0 - h / 90.0;
            let c = [base[0] * shade * green, base[1] * shade * (green + 0.06), base[2] * shade * green];
            let k = (j * SIDE + i) * 4;
            for n in 0..3 {
                px[k + n] = c[n].clamp(0.0, 255.0) as u8;
            }
        }
    }
    let cell = |c: f32| (((c + half) / (2.0 * half) * SIDE as f32) as isize).clamp(0, SIDE as isize - 1) as usize;
    // Trees: a dark fleck each.
    for p in &map.props {
        let k = (cell(p.pos.z) * SIDE + cell(p.pos.x)) * 4;
        px[k] = 104;
        px[k + 1] = 128;
        px[k + 2] = 84;
    }
    // Roofs and thick walls built of blocks rather than outlines, such as Connaught Place's ring.
    for b in map.blocks.iter().filter(|b| b.collide && b.visible) {
        let Shape::Box(v) = b.shape else { continue };
        let floor = map.terrain.height(b.pos.x, b.pos.z);
        if v.x.min(v.z) < 1.5 || b.pos.y + v.y / 2.0 < floor + 2.0 || b.pos.y - v.y / 2.0 > floor + 14.0 {
            continue;
        }
        let (u, w) = (b.rot * Vec3::X, b.rot * Vec3::Z);
        let reach = Vec2::new(v.x, v.z).length() / 2.0;
        for j in cell(b.pos.z - reach)..=cell(b.pos.z + reach) {
            for i in cell(b.pos.x - reach)..=cell(b.pos.x + reach) {
                let d = Vec3::new(at(i) - b.pos.x, 0.0, at(j) - b.pos.z);
                if d.dot(u).abs() <= v.x / 2.0 && d.dot(w).abs() <= v.z / 2.0 {
                    let k = (j * SIDE + i) * 4;
                    px[k..k + 3].copy_from_slice(&[96, 102, 112]);
                }
            }
        }
    }
    for f in map.footprints.iter().filter(|f| f.mat != Mat::Water) {
        let (lo, hi) = f.pts.iter().fold((Vec2::MAX, Vec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        // Taller buildings are darker, so towers stand out from a colony's houses.
        let d = 1.0 - (f.top / 160.0).clamp(0.0, 0.5);
        let c = [66.0 * d, 72.0 * d, 84.0 * d];
        for j in cell(lo.y)..=cell(hi.y) {
            for i in cell(lo.x)..=cell(hi.x) {
                if footprint::contains(&f.pts, Vec2::new(at(i), at(j))) {
                    let k = (j * SIDE + i) * 4;
                    for n in 0..3 {
                        px[k + n] = c[n] as u8;
                    }
                }
            }
        }
    }
    px
}

/// The picture as PNG bytes, by way of a file: Bevy's image type does the encoding.
fn png(rgba: Vec<u8>, path: &str) -> Vec<u8> {
    let size = Extent3d { width: SIDE as u32, height: SIDE as u32, depth_or_array_layers: 1 };
    let image = Image::new(size, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.try_into_dynamic().expect("an RGBA picture").to_rgb8().save(path).expect("write the picture");
    let bytes = std::fs::read(path).expect("read the picture back");
    std::fs::remove_file(path).ok();
    bytes
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(ABC[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A tier's colour on the atlas.
fn tier_colour(t: compose::Tier) -> &'static str {
    match t {
        compose::Tier::Hot => "#ff6a4d",
        compose::Tier::Warm => "#ffc15a",
        compose::Tier::Quiet => "#8fd3ff",
        compose::Tier::Small => "#d8dde0",
    }
}

fn main() {
    let map = shared::map::compose::generate(shared::sim::map_seed());
    let out = std::env::args().nth(1).unwrap_or("docs/ncr/map-atlas.svg".into());
    let half = map.half_extent;
    // A world position on the page.
    let page = |x: f32, z: f32| (LEFT + (x + half) / (2.0 * half) * SIDE as f32, TOP + (z + half) / (2.0 * half) * SIDE as f32);
    let picture = png(picture(&map), &format!("{out}.base.png"));
    println!("base picture {} KB", picture.len() / 1024);
    let mut s = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {PAGE_W} {PAGE_H}"><rect width="{PAGE_W}" height="{PAGE_H}" fill="#101d25"/><style>text{{font-family:Avenir Next,Helvetica Neue,Arial,sans-serif;fill:#ebeee6}}.name{{font-weight:700;paint-order:stroke;stroke:#0d1a21;stroke-width:6px;stroke-linejoin:round}}.spot{{font-size:19px;paint-order:stroke;stroke:#0d1a21;stroke-width:4px;stroke-linejoin:round}}.land{{fill:#eaf6f8;font-weight:600;letter-spacing:.45em;opacity:.8;paint-order:stroke;stroke:#0d1a2199;stroke-width:3px}}.road{{fill:#fff;font-weight:600;letter-spacing:2px;opacity:.85;paint-order:stroke;stroke:#2c3036;stroke-width:4px}}.small{{font-size:21px;fill:#a9bdc5}}.head{{font-size:27px;letter-spacing:3px}}</style><text x="{LEFT}" y="72" font-size="52" font-weight="700" letter-spacing="6">NCR</text><text x="{LEFT}" y="108" class="small">THE ONE MAP  ·  REAL DISTRICTS FROM OPENSTREETMAP  ·  {w} × {w} METRES  ·  EVERY DISTRICT AT TRUE SIZE, IN ITS TRUE DIRECTION FROM CONNAUGHT PLACE</text>"##,
        w = thousands((2.0 * half) as u32),
    );
    write!(s, r##"<image x="{LEFT}" y="{TOP}" width="{SIDE}" height="{SIDE}" href="data:image/png;base64,{}"/>"##, base64(&picture)).unwrap();
    // One kilometre squares, marked in kilometres from the middle of the map.
    for k in -2..=2 {
        let (x, _) = page(k as f32 * 1000.0, 0.0);
        let (_, y) = page(0.0, k as f32 * 1000.0);
        let (end_x, end_y) = (LEFT + SIDE as f32, TOP + SIDE as f32);
        write!(s, r##"<path d="M{x} {TOP}V{end_y}M{LEFT} {y}H{end_x}" stroke="#ffffff" stroke-opacity=".16" stroke-width="1.5"/>"##).unwrap();
        write!(s, r##"<text x="{x}" y="{}" class="small" text-anchor="middle">{k} km</text><text x="{}" y="{}" class="small" text-anchor="end">{k}</text>"##, TOP + SIDE as f32 + 30.0, LEFT - 8.0, y + 7.0).unwrap();
    }
    for (name, x, z, turn, size) in LAND {
        let (px, py) = page(x, z);
        write!(s, r##"<text class="land" font-size="{size}" text-anchor="middle" transform="translate({px} {py}) rotate({turn})">{name}</text>"##).unwrap();
    }
    for (name, x, z, turn, size) in ROADS {
        let (px, py) = page(x, z);
        write!(s, r##"<text class="road" font-size="{size}" text-anchor="middle" transform="translate({px} {py}) rotate({turn})">{name}</text>"##).unwrap();
    }
    // Bridges: drawn over the water, numbered as in the legend.
    for (k, b) in compose::SPANS.iter().enumerate() {
        let (ax, ay) = page(b.a.x, b.a.y);
        let (cx, cy) = page(b.c.x, b.c.y);
        let w = (b.width / 2.0).max(7.0);
        write!(s, r##"<line x1="{ax}" y1="{ay}" x2="{cx}" y2="{cy}" stroke="#101d25" stroke-width="{}"/><line x1="{ax}" y1="{ay}" x2="{cx}" y2="{cy}" stroke="#f3efe2" stroke-width="{w}"/>"##, w + 5.0).unwrap();
        let (mx, my) = ((ax + cx) / 2.0, (ay + cy) / 2.0);
        write!(s, r##"<circle cx="{mx}" cy="{my}" r="17" fill="#101d25" stroke="#f3efe2" stroke-width="2.5"/><text x="{mx}" y="{}" font-size="20" font-weight="700" text-anchor="middle">{}</text>"##, my + 7.0, k + 1).unwrap();
    }
    // Loot: every chest a match can have out.
    for c in &map.chests {
        let (x, y) = page(c.pos.x, c.pos.z);
        write!(s, r##"<circle cx="{x:.1}" cy="{y:.1}" r="3.4" fill="#ffd34d" stroke="#3a2a00" stroke-width="1"/>"##).unwrap();
    }
    // Smaller named spots, a dot and a name.
    for p in map.pois.iter().filter(|p| !compose::PLACES.iter().any(|d| d.name == p.name) && !compose::BRIDGES.contains(&p.name.as_str())) {
        let (x, y) = page(p.center.x, p.center.z);
        write!(s, r##"<circle cx="{x}" cy="{y}" r="6" fill="#101d25" stroke="#ebeee6" stroke-width="2.5"/><text x="{}" y="{}" class="spot">{}</text>"##, x + 11.0, y + 7.0, p.name).unwrap();
    }
    // Districts: an outline of the square kept, and the name.
    for d in &compose::PLACES {
        let (x, y) = page(d.at.x, d.at.y);
        let w = d.clip / (2.0 * half) * SIDE as f32;
        let colour = tier_colour(d.tier);
        write!(s, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{colour}" stroke-opacity=".5" stroke-width="2" stroke-dasharray="10 8"/>"##, x - w, y - w, w * 2.0, w * 2.0).unwrap();
        write!(s, r##"<text x="{x}" y="{}" class="name" font-size="31" fill="{colour}" style="fill:{colour}" text-anchor="middle">{}</text>"##, y + 11.0, d.name).unwrap();
    }
    // North, and a scale.
    let (bar_x, bar_y) = (LEFT + 24.0, TOP + SIDE as f32 - 34.0);
    write!(s, r##"<path d="M{bar_x} {bar_y}h500" stroke="#101d25" stroke-width="11"/><path d="M{bar_x} {bar_y}h500" stroke="#f3efe2" stroke-width="6"/><text x="{bar_x}" y="{}" class="spot">1 km, about 2 minutes 15 at a sprint</text>"##, bar_y - 14.0).unwrap();
    let (nx, ny) = (LEFT + SIDE as f32 - 56.0, TOP + 66.0);
    write!(s, r##"<path d="M{nx} {}l20 54l-20 -12l-20 12z" fill="#f3efe2" stroke="#101d25" stroke-width="3"/><text x="{nx}" y="{}" font-size="26" font-weight="700" class="spot" text-anchor="middle">N</text>"##, ny - 42.0, ny + 42.0).unwrap();
    // The side column: districts by tier with their loot, bridges, and how to read the map.
    let one_match = map.chests.iter().filter(|c| c.set.is_none_or(|s| s.1 == 0));
    let col = LEFT + SIDE as f32 + 50.0;
    let mut y = TOP + 26.0;
    write!(s, r##"<text x="{col}" y="{y}" class="head">DISTRICTS</text><text x="{}" y="{y}" class="small" text-anchor="end">chests · finds</text>"##, col + 390.0).unwrap();
    y += 44.0;
    for d in &compose::PLACES {
        let inside = |p: Vec3| (p.xz() - d.at).abs().max_element() <= d.clip;
        let chests = one_match.clone().filter(|c| inside(c.pos)).count();
        let finds = map.floor_loot.iter().filter(|p| inside(**p)).count();
        write!(s, r##"<circle cx="{}" cy="{}" r="8" fill="{}"/><text x="{}" y="{y}" font-size="23">{}</text><text x="{}" y="{y}" class="small" text-anchor="end">{chests} · {finds}</text>"##, col + 8.0, y - 8.0, tier_colour(d.tier), col + 26.0, d.name, col + 390.0).unwrap();
        y += 34.0;
    }
    y += 26.0;
    for (name, colour) in [("Hot: most loot, most drops", "#ff6a4d"), ("Warm", "#ffc15a"), ("Quiet", "#8fd3ff"), ("Small", "#d8dde0")] {
        write!(s, r##"<circle cx="{}" cy="{}" r="8" fill="{colour}"/><text x="{}" y="{y}" class="small">{name}</text>"##, col + 8.0, y - 8.0, col + 26.0).unwrap();
        y += 30.0;
    }
    y += 34.0;
    write!(s, r##"<text x="{col}" y="{y}" class="head">BRIDGES</text>"##).unwrap();
    y += 42.0;
    for (k, name) in compose::BRIDGES.iter().enumerate() {
        write!(s, r##"<text x="{col}" y="{y}" font-size="23"><tspan font-weight="700">{}</tspan>   {name}</text>"##, k + 1).unwrap();
        y += 32.0;
    }
    y += 34.0;
    write!(s, r##"<text x="{col}" y="{y}" class="head">THIS BUILD</text>"##).unwrap();
    y += 40.0;
    let roofs = map.footprints.iter().filter(|f| f.mat != Mat::Water).count();
    for line in [
        format!("{} buildings", thousands(roofs as u32)),
        format!("{} chests a match, of {} spots (gold dots)", one_match.clone().count(), map.chests.len()),
        format!("{} floor finds", thousands(map.floor_loot.len() as u32)),
        format!("{} trees and props", thousands(map.props.len() as u32)),
        "Between districts the distance is".to_string(),
        "cut down and stays open land.".to_string(),
    ] {
        write!(s, r##"<text x="{col}" y="{y}" class="small">{line}</text>"##).unwrap();
        y += 30.0;
    }
    let foot = TOP + SIDE as f32 + 34.0;
    write!(s, r##"<text x="{col}" y="{}" class="small">Map data © OpenStreetMap</text><text x="{col}" y="{foot}" class="small">contributors, ODbL 1.0.</text></svg>"##, foot - 28.0).unwrap();
    std::fs::write(&out, s).unwrap();
    println!("{out}: {} places, {} buildings, {} chests a match, of {} spots", map.pois.len(), roofs, one_match.clone().count(), map.chests.len());
}

/// A number with thousands separators.
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
