//! Credits, off Home: whose work and data the game stands on, in plain words. OpenStreetMap's ODbL
//! and the CC BY licences ask for this before anyone outside plays. `docs/references.md` holds the
//! reasons, the full source of each line and the offer of the map's files.

use bevy::prelude::*;

use super::theme::{self, CREAM, Fonts, GHOST, MUSTARD, Shortcut, u};
use super::{Press, Screen};

const TITLE: &str = "CREDITS";
const BACK: &str = "Wapas";

const MAP: &str = "Map data © OpenStreetMap contributors, available under the Open Database License (openstreetmap.org/copyright). OpenStreetMap extracts processed by Geofabrik GmbH. Some place data from Overture Maps Foundation and Meta, CDLA Permissive 2.0.";
/// ODbL 4.6: see docs/references.md.
const OFFER: &str = "Panga's map is a database made from OpenStreetMap, and stays under that licence. Anyone may have a free, machine-readable copy of its map files and the tools that make them: github.com/Apc0015/Panga, in assets/data/maps and tools/osm.";
const CHANGED: &str = "Used under CC BY (creativecommons.org/licenses/by/4.0). Changed for the game: scaled, reduced, repainted, makers' marks removed.";
const CLOTHES: &str = "Clothes from the MakeHuman community packs, CC BY 3.0 and 4.0 (static.makehumancommunity.org/assets/assetpacks.html). Changed: recoloured, cut to fit.";

/// A work's name, who made it, its licence, and where it is.
const VEHICLES: [(&str, &str, &str, &str); 9] = [
    ("Tuk Tuk Rikshaw", "alnmathew", "CC BY 4.0", "tuk-tuk-rikshaw-0bdfdf97a0f248008806840f7188367c"),
    ("Jeep Willys", "skudgee (Willy Decarpentrie)", "CC BY 4.0", "jeep-willys-a28600c16b9e44bcb160ede0d199180d"),
    ("honda-smart-dio-z4-2006-1", "AlexanderChen394", "CC BY", "none-ef6060608fe04cacbf6d4c817f494d6c"),
    ("Mini-Truck", "ROY (roy.3dartist)", "CC BY 4.0", "mini-truck-71e499219de548a7a1be74c237b6a2fb"),
    ("Pulsar 135 Ls 2", "EmanuelRestrepoVelez", "CC BY", "none-accbf2048c9344718791588b70d2ebf8"),
    ("Tractor", "le0tard", "CC BY 4.0", "tractor-278fc1e0516c401eba358d3aa2e55635"),
    ("Classic Motorcycle", "DeclanLea14", "CC BY", "none-f246ff1a68d3459892f9420b744a58b6"),
    ("Stylized Suzuki Alto Works", "EddyNL", "CC BY", "none-467f2e28ff334919879841216766903b"),
    ("Hindustan Ambassador", "rSquare", "CC BY 4.0", "hindustan-ambassador-a84ef7126b2b43adb7b4b28a4d42e5fb"),
];
const SHIELD: (&str, &str, &str, &str) = ("Indian dhal (shield), 19th century", "Pedram Ashoori", "CC BY 4.0", "indian-dhal-shield-19th-century-fca1088c468c480a845977189d66bf59");
const CLOTHES_BY: [&str; 3] = [
    "punkduck: \"Male classic jeans\", \"Running shoes 01\", \"Female tight jeans\", \"Tennis shoes\"",
    "Elvaerwyn: \"Mens shirt untucked\", men's trouser, flip-flop sandals",
    "Mindfront: \"Male trousers 1\", Oxford shoes",
];

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Credits), spawn);
}

fn spawn(mut commands: Commands, fonts: Res<Fonts>) {
    let heading = |c: &mut ChildSpawnerCommands, s: &str| {
        c.spawn(theme::line(s, &fonts.title, 27.0, MUSTARD));
    };
    let model = |c: &mut ChildSpawnerCommands, (name, by, licence, page): (&str, &str, &str, &str)| {
        c.spawn(theme::line(format!("\"{name}\" by {by}, {licence}"), &fonts.body, 19.0, CREAM));
        c.spawn((theme::line(format!("sketchfab.com/3d-models/{page}"), &fonts.body, 14.0, CREAM.with_alpha(0.8)), Node { margin: UiRect::bottom(u(3.0)), ..default() }));
    };
    commands.spawn((theme::layer(5), BackgroundColor(Color::srgba(0.078, 0.047, 0.031, 0.9)), DespawnOnExit(Screen::Credits))).with_children(|p| {
        p.spawn((Node { position_type: PositionType::Absolute, left: u(72.0), top: u(40.0), right: u(40.0), column_gap: u(44.0), ..default() }, Pickable::IGNORE)).with_children(|row| {
            row.spawn((Node { width: u(500.0), flex_direction: FlexDirection::Column, row_gap: u(12.0), ..default() }, Pickable::IGNORE)).with_children(|c| {
                c.spawn(theme::painted(TITLE, &fonts.title, 62.0, MUSTARD));
                heading(c, "Naksha");
                c.spawn(theme::line(MAP, &fonts.body, 20.0, CREAM));
                c.spawn(theme::line(OFFER, &fonts.body, 20.0, CREAM));
                heading(c, "Kapde");
                c.spawn(theme::line(CLOTHES, &fonts.body, 18.0, CREAM));
                for line in CLOTHES_BY {
                    c.spawn(theme::line(line, &fonts.body, 18.0, CREAM));
                }
            });
            row.spawn((Node { flex_grow: 1.0, flex_basis: u(0.0), flex_direction: FlexDirection::Column, row_gap: u(2.0), ..default() }, Pickable::IGNORE)).with_children(|c| {
                heading(c, "Gaadiyan aur dhal");
                c.spawn((theme::line(CHANGED, &fonts.body, 17.0, CREAM), Node { margin: UiRect::bottom(u(8.0)), ..default() }));
                for work in VEHICLES {
                    model(c, work);
                }
                model(c, SHIELD);
            });
        });
        p.spawn((Node { position_type: PositionType::Absolute, left: u(72.0), bottom: u(44.0), width: u(300.0), ..default() }, Pickable::IGNORE)).with_children(|c| {
            theme::small_button(c, &fonts, BACK, Press::Home, GHOST, Some(Shortcut::Esc));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line of the vehicles' table in docs/references.md, and each CC BY author outside it.
    #[test]
    fn every_cc_by_author_is_credited() {
        let docs = include_str!("../../../../docs/references.md");
        let shown: String = VEHICLES.iter().chain([&SHIELD]).map(|w| format!("{} {} {} {}\n", w.0, w.1, w.2, w.3)).chain(CLOTHES_BY.iter().map(|l| l.to_string())).collect();
        let rows: Vec<&str> = docs.lines().filter(|l| l.starts_with('|') && (l.contains("CC-BY") || l.contains("CC BY"))).collect();
        assert!(rows.len() >= 13, "{}", rows.len());
        for row in rows {
            let named = shown.split('\n').any(|l| {
                let author = l.split(['(', ':', ' ']).next().unwrap_or_default();
                !author.is_empty() && row.contains(author) || VEHICLES.iter().chain([&SHIELD]).any(|w| row.contains(w.1.split(' ').next().unwrap_or_default()))
            });
            assert!(named, "no credit for {row}");
        }
        for page in VEHICLES.iter().chain([&SHIELD]).map(|w| w.3) {
            assert!(docs.contains(page), "{page}");
        }
    }

    #[test]
    fn the_map_credit_and_the_offer_are_there() {
        let docs = include_str!("../../../../docs/references.md");
        assert!(MAP.contains("OpenStreetMap contributors") && MAP.contains("Open Database License") && MAP.contains("Geofabrik") && MAP.contains("Overture"));
        assert!(docs.contains(OFFER), "the offer in docs/references.md is what the screen says");
    }
}
