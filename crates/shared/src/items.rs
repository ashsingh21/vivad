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
    /// A swing (see `sim::melee`): `windup` seconds, then the blade sweeps `arc_deg` of the front out
    /// to `reach` in `active` seconds and may strike `max_hits` warriors; ready again after
    /// `interval`. A blow pushes its target at `knock` m/s (below zero pulls it in) and dazes it for
    /// `stun` seconds. Aim held is a guard: from the front it stops `guard` of a blow and
    /// `shot_guard` of a bullet. Old things wear out: `durability` blows and it breaks (0: never),
    /// counted down in its stack's `count`.
    Melee {
        damage: f32,
        interval: f32,
        reach: f32,
        windup: f32,
        active: f32,
        arc_deg: f32,
        max_hits: u32,
        guard: f32,
        #[serde(default)]
        knock: f32,
        #[serde(default)]
        stun: f32,
        #[serde(default)]
        shot_guard: f32,
        #[serde(default)]
        durability: u32,
    },
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
        /// Degrees of spread each shot adds, and the most they add up to: spray and the cone
        /// opens, let go and it closes again (2.5 degrees a second).
        #[serde(default)]
        bloom: f32,
        #[serde(default)]
        bloom_max: f32,
        /// Magnification of a fixed scope, looked through while aiming; none below 1.
        #[serde(default)]
        scope: f32,
        /// Extra degrees of spread when not looking through the scope: rifles are poor from the hip.
        #[serde(default)]
        hip_spread: f32,
        /// Goli spent per round loaded (0 counts as 1): the big rifles ration the one shared pool.
        #[serde(default)]
        ammo_cost: u32,
    },
    /// Thrown astra that bursts into an area effect on impact.
    Astra { effect: AstraEffect, speed: f32, count: u32 },
    /// Channelled consumable.
    Consumable { heal: f32, heal_cap: f32, shield: f32, use_time: f32, stack: u32 },
    /// Reserve arrows.
    Arrows { amount: u32 },
}

impl ItemKind {
    /// The magnification of this weapon's fixed scope, if it has one.
    pub fn scope(&self) -> Option<f32> {
        match self {
            ItemKind::Bow { scope, .. } if *scope > 1.0 => Some(*scope),
            _ => None,
        }
    }

    /// The goli it takes to load one round.
    pub fn round_cost(&self) -> u32 {
        match self {
            ItemKind::Bow { ammo_cost, .. } => (*ammo_cost).max(1),
            _ => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum AstraEffect {
    /// Burning ground: damage per second inside the radius.
    Fire { radius: f32, dps: f32, duration: f32 },
    /// Wind burst that throws everyone (including the thrower) upward and outward.
    Wind { radius: f32, impulse: f32 },
    /// The Sudarshan Chakra: flies straight out to `range`, cutting through everyone it passes
    /// (`damage` each, once per pass), and comes back to the thrower's hand to be thrown again.
    Chakra { damage: f32, range: f32 },
    /// The Brahmastra: a blast that hurts everyone within `radius` (`damage` at the centre, falling
    /// off to nothing at the edge, the thrower included) and hurls them outward.
    Blast { radius: f32, damage: f32, impulse: f32 },
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
    /// Arrows in the bow, astras held, consumables held, blows left in an old melee weapon.
    pub count: u32,
}
