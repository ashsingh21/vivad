//! The style guide's colour tokens (docs/style-guide.md): `assets/style/palette.json`, built in.
//! `style::srgb("architecture.stucco")` is that colour as sRGB 0..1; a name that isn't in the
//! file panics on first use (a colour that isn't a token is a bug), and the test below reads every
//! one.

use std::collections::HashMap;
use std::sync::OnceLock;

const PALETTE: &str = include_str!("../../../assets/style/palette.json");

fn table() -> &'static HashMap<String, [f32; 3]> {
    static T: OnceLock<HashMap<String, [f32; 3]>> = OnceLock::new();
    T.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(PALETTE).expect("assets/style/palette.json");
        let mut out = HashMap::new();
        for (group, colours) in v.as_object().into_iter().flatten() {
            for (name, hex) in colours.as_object().into_iter().flatten() {
                let Some(hex) = hex.as_str().and_then(|s| s.strip_prefix('#')) else { continue };
                let c = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("palette hex") as f32 / 255.0;
                out.insert(format!("{group}.{name}"), [c(0), c(2), c(4)]);
            }
        }
        out
    })
}

/// A palette token as sRGB (0..1).
pub fn srgb(name: &str) -> [f32; 3] {
    *table().get(name).unwrap_or_else(|| panic!("no colour `{name}` in assets/style/palette.json"))
}

/// A palette token as linear RGB.
pub fn linear(name: &str) -> [f32; 3] {
    srgb(name).map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_token_reads() {
        assert!(super::table().len() > 50);
        assert_eq!(super::srgb("ground.kerb_black"), [0x2b as f32 / 255.0, 0x2b as f32 / 255.0, 0x2e as f32 / 255.0]);
    }
}
