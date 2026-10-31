//! Item definitions (weapons, astras, consumables), loaded from `assets/data/items.ron`.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord, Reflect)]
pub enum Rarity {
    Common,
    Rare,
    Epic,
    Legendary,
    /// Divine tier: mythic weapons of the gods.
    Divya,
}

impl Rarity {
    pub const ALL: [Rarity; 5] = [Rarity::Common, Rarity::Rare, Rarity::Epic, Rarity::Legendary, Rarity::Divya];

    pub fn damage_scale(self) -> f32 {
        match self {
            Rarity::Common => 1.0,
            Rarity::Rare => 1.08,
            Rarity::Epic => 1.16,
            Rarity::Legendary => 1.24,
            Rarity::Divya => 1.35,
        }
    }

    pub fn color(self) -> Color {
        match self {
            Rarity::Common => Color::srgb(0.70, 0.70, 0.68),
            Rarity::Rare => Color::srgb(0.25, 0.60, 0.95),
            Rarity::Epic => Color::srgb(0.70, 0.35, 0.95),
            Rarity::Legendary => Color::srgb(1.0, 0.62, 0.15),
            Rarity::Divya => Color::srgb(1.0, 0.92, 0.45),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub enum ItemKind {
    /// Swept-sphere melee swing.
    Melee { damage: f32, interval: f32, reach: f32, hit_delay: f32 },
    /// Arrows (or bullets): `pellets` per shot spread over `spread_deg`. `charge` > 0 means hold to
    /// draw. A country-made gun can let its owner down: `misfire` is the chance a pull of the
    /// trigger only clicks (the round is spent), `backfire` the chance it blows back in their face.
    Bow {
        damage: f32,
        interval: f32,
        speed: f32,
        gravity: f32,
        pellets: u32,
        spread_deg: f32,
        magazine: u32,
        reload: f32,
        charge: f32,
        headshot: f32,
        #[serde(default)]
        misfire: f32,
        #[serde(default)]
        backfire: f32,
    },
    /// Thrown astra that bursts into an area effect on impact.
    Astra { effect: AstraEffect, speed: f32, count: u32 },
    /// Channelled consumable.
    Consumable { heal: f32, heal_cap: f32, shield: f32, use_time: f32, stack: u32 },
    /// Reserve arrows.
    Arrows { amount: u32 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum AstraEffect {
    /// Burning ground: damage per second inside the radius.
    Fire { radius: f32, dps: f32, duration: f32 },
    /// Wind burst that throws everyone (including the thrower) upward and outward.
    Wind { radius: f32, impulse: f32 },
}

#[derive(Clone, Debug, Deserialize)]
pub struct ItemDef {
    pub name: String,
    pub lore: String,
    pub kind: ItemKind,
    /// Relative drop weights per rarity this item can appear in.
    pub drops: Vec<(Rarity, f32)>,
}

#[derive(Resource, Clone, Debug, Deserialize)]
pub struct ItemDb {
    pub items: BTreeMap<String, ItemDef>,
    /// The weapon every warrior starts with and cannot drop.
    pub default_melee: String,
}

impl ItemDb {
    pub fn load_from_str(src: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(src)
    }

    pub fn get(&self, id: &str) -> &ItemDef {
        self.items.get(id).unwrap_or_else(|| panic!("unknown item `{id}`"))
    }
}

/// One concrete item instance in an inventory or on the ground.
#[derive(Clone, Debug, PartialEq, Reflect, Serialize, Deserialize)]
pub struct ItemStack {
    pub id: String,
    pub rarity: Rarity,
    /// Arrows in the bow, astras held, consumables held.
    pub count: u32,
}
