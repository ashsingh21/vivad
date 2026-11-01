//! Export the actual seeded map geometry as a standalone, labelled tactical SVG.
use bevy::prelude::*;
use shared::map::{Mat, Shape, ncr};
use std::fmt::Write;
fn main() {
    let map = ncr::generate(shared::sim::map_seed());
    // The drawing is 1000 px for the map's width: one px is WORLD_SCALE metres.
    let px = |v: f32| v / ncr::WORLD_SCALE + 500.0;
    let metres = Thousands((2.0_f32 * ncr::HALF).round() as u32);
    let mut s = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1440 1260"><rect width="1440" height="1260" fill="#101d25"/><style>text{{font-family:Arial,sans-serif;fill:#ebeee6}}.small{{font-size:14px;fill:#a9bdc5}}.label{{font-size:15px;paint-order:stroke;stroke:#14252d;stroke-width:5px;stroke-linejoin:round}}</style><text x="55" y="60" font-size="34" letter-spacing="4">VIVAD / NCR</text><text x="55" y="92" class="small">KALI YUGA  ·  PLAYABLE MAP ATLAS  ·  {metres} × {metres} METRES</text><text x="1040" y="60" class="small">N ↑  /  NORTH = −Z</text><g transform="translate(55 135)"><rect width="1000" height="1000" fill="#33423f"/>"##,
    );
    for i in 0..=10 {
        let k = i * 100;
        write!(
            s,
            r##"<path d="M{k} 0V1000M0 {k}H1000" stroke="#42514b" stroke-width="0.7"/>"##
        )
        .unwrap();
    }
    let mut blocks: Vec<_> = map.blocks.iter().filter(|b| b.visible).collect();
    blocks.sort_by(|a, b| a.pos.y.total_cmp(&b.pos.y));
    for b in blocks {
        let color = match b.mat {
            Mat::Water => "#398e9b",
            Mat::Asphalt => "#1d2d35",
            Mat::Glass => "#689ca9",
            Mat::Concrete => "#a4aaa0",
            Mat::Plaster | Mat::Marble => "#e2d9ba",
            Mat::PlasterOchre => "#b6a077",
            Mat::Brick | Mat::ContainerRed => "#b17a61",
            Mat::Steel => "#59737c",
            Mat::TransitBlue => "#41b5d7",
            Mat::TransitYellow => "#e5c45a",
            Mat::Grass => "#698157",
            _ => continue,
        };
        if let Shape::Box(v) = b.shape {
            if v.x < 0.4 || v.z < 0.4 {
                continue;
            }
            let corners = [
                Vec3::new(-v.x / 2.0, 0.0, -v.z / 2.0),
                Vec3::new(v.x / 2.0, 0.0, -v.z / 2.0),
                Vec3::new(v.x / 2.0, 0.0, v.z / 2.0),
                Vec3::new(-v.x / 2.0, 0.0, v.z / 2.0),
            ];
            let points = corners
                .map(|p| {
                    let p = b.pos + b.rot * p;
                    format!("{:.1},{:.1}", px(p.x), px(p.z))
                })
                .join(" ");
            write!(s, "<polygon points=\"{points}\" fill=\"{color}\"/>").unwrap();
        }
    }
    for p in &map.props {
        write!(
            s,
            r##"<circle cx="{}" cy="{}" r="4" fill="#7b9861"/>"##,
            px(p.pos.x),
            px(p.pos.z)
        )
        .unwrap();
    }
    write!(s,r##"<path d="M330 500H670" stroke="#41b5d7" stroke-width="3" stroke-dasharray="7 5"/><path d="M420 565V955" stroke="#e5c45a" stroke-width="3"/>"##).unwrap();
    for (i, p) in map.pois.iter().enumerate() {
        let x = px(p.center.x);
        let y = px(p.center.z);
        write!(s,r##"<circle cx="{x}" cy="{y}" r="10" fill="#14252d" stroke="#e6c57a"/><text x="{x}" y="{}" font-size="11" text-anchor="middle">{}</text>"##,y+4.0,i+1).unwrap();
    }
    s.push_str(r##"</g><text x="1090" y="155" font-size="19">DISTRICTS &amp; ROUTES</text>"##);
    for (i, p) in map.pois.iter().enumerate() {
        write!(
            s,
            "<text x=\"1090\" y=\"{}\" class=\"small\">{:02}  {}</text>",
            195 + i * 36,
            i + 1,
            p.name.replace('&', "&amp;").replace(" / ", " · ")
        )
        .unwrap();
    }
    s.push_str(r##"<text x="1090" y="800" font-size="19">READ THE MAP</text><text x="1090" y="838" class="small">Cyan dash · underground route</text><text x="1090" y="870" class="small">Yellow · elevated metro walkway</text><text x="1090" y="902" class="small">Teal · Yamuna and wetlands</text><text x="1090" y="934" class="small">Cream blocks · arcades &amp; homes</text><text x="1090" y="966" class="small">Three bridges · exposed rotations</text><text x="1090" y="1025" class="small">Geometry exported from the game.</text><text x="1090" y="1057" class="small">Fictional compression, not a survey.</text><path d="M55 1175H255" stroke="#e6c57a" stroke-width="3"/><text x="55" y="1204" class="small">0</text><text x="225" y="1204" class="small">440 m</text><text x="530" y="1204" class="small">Modern streets. Divine arsenal. One closing Pralaya.</text></svg>"##);
    let out = std::env::args()
        .nth(1)
        .unwrap_or("docs/ncr/map-atlas.svg".into());
    std::fs::write(&out, s).unwrap();
    println!(
        "{out}: {} blocks / {} POIs / {} chests",
        map.blocks.len(),
        map.pois.len(),
        map.chests.len()
    );
}

/// A number with thousands separators, for the atlas heading.
struct Thousands(u32);
impl std::fmt::Display for Thousands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let digits = self.0.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        f.write_str(&out)
    }
}
