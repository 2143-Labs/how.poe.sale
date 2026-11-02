//! Item state: the crafting-relevant summary of one item.

use serde::{Deserialize, Serialize};

use crate::data::Bundle;

/// The two affix sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Prefix,
    Suffix,
}

impl Side {
    pub const ALL: [Side; 2] = [Side::Prefix, Side::Suffix];

    /// Human-facing name, as `poe1::api` emits it.
    pub fn name(self) -> &'static str {
        match self {
            Side::Prefix => "Prefix",
            Side::Suffix => "Suffix",
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Prefix => Side::Suffix,
            Side::Suffix => Side::Prefix,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Side::Prefix => 0,
            Side::Suffix => 1,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Rarity {
    #[default]
    Normal,
    Magic,
    Rare,
}

impl Rarity {
    pub fn name(self) -> &'static str {
        match self {
            Rarity::Normal => "Normal",
            Rarity::Magic => "Magic",
            Rarity::Rare => "Rare",
        }
    }
}

/// A requirement mod present on the item, at the tier `requirements[req].accepted[idx]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hit {
    pub req: u16,
    pub idx: u8,
    pub value_ok: bool,
}

/// A requirement group held by a tier that is not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Blocked {
    pub group: u16,
    pub influence: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SideState {
    /// Requirement mods present, sorted.
    pub wanted: Vec<Hit>,
    /// Requirement groups held by a non-accepted tier, sorted.
    pub blocked: Vec<Blocked>,
    /// Other mods.
    pub untracked: u8,
    /// How many of `untracked` are influence mods (≤ `untracked`).
    pub untracked_influenced: u8,
}

impl SideState {
    pub fn slots_used(&self) -> u8 {
        self.wanted.len() as u8 + self.blocked.len() as u8 + self.untracked
    }

    pub fn is_empty(&self) -> bool {
        self.wanted.is_empty()
            && self.blocked.is_empty()
            && self.untracked == 0
            && self.untracked_influenced == 0
    }
}

/// The one item the crafting state describes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemState {
    pub base: u16,
    pub ilvl: u8,
    pub rarity: Rarity,
    pub influences: u8,
    /// (bundle.catalysts index, quality 0..=20)
    pub catalyst: Option<(u8, u8)>,
    pub prefixes: SideState,
    pub suffixes: SideState,
    /// bit0 Prefixes Cannot Be Changed, bit1 Suffixes Cannot Be Changed
    pub lock: u8,
    /// A Craicic Chimeral imprint of this item's (prefixes, suffixes), taken while it was magic.
    /// Omitted from the key when absent, so states without an imprint keep their v1 keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imprint: Option<Box<(SideState, SideState)>>,
}

pub const INFLUENCE_NAMES: [&str; 6] = [
    "Shaper", "Elder", "Crusader", "Redeemer", "Hunter", "Warlord",
];

/// Influence-name suffix in `item_classes.json` tags, in bit order.
pub const INFLUENCE_TAG_SUFFIXES: [&str; 6] = [
    "shaper",
    "elder",
    "crusader",
    "eyrie",
    "basilisk",
    "adjudicator",
];

impl ItemState {
    pub fn new(base: u16, ilvl: u8, rarity: Rarity, influences: u8) -> Self {
        ItemState {
            base,
            ilvl,
            rarity,
            influences,
            catalyst: None,
            prefixes: SideState::default(),
            suffixes: SideState::default(),
            lock: 0,
            imprint: None,
        }
    }

    pub fn side(&self, side: Side) -> &SideState {
        match side {
            Side::Prefix => &self.prefixes,
            Side::Suffix => &self.suffixes,
        }
    }

    pub fn side_mut(&mut self, side: Side) -> &mut SideState {
        match side {
            Side::Prefix => &mut self.prefixes,
            Side::Suffix => &mut self.suffixes,
        }
    }

    /// Stable key: the JSON encoding of the struct (field order is fixed by the struct).
    pub fn key(&self) -> String {
        serde_json::to_string(self).expect("ItemState serialises")
    }

    /// (prefix slots, suffix slots) for this rarity and base.
    pub fn caps(&self, b: &Bundle) -> (u8, u8) {
        let base = b.base_at(self.base);
        let (p, s) = match self.rarity {
            Rarity::Rare => (3i16, 3i16),
            Rarity::Magic => (1, 1),
            Rarity::Normal => (0, 0),
        };
        (
            (p + base.prefix_delta).max(0) as u8,
            (s + base.suffix_delta).max(0) as u8,
        )
    }

    /// Slots used on each side, including the crafted metamod that occupies one.
    pub fn used(&self) -> (u8, u8) {
        let p = self.prefixes.slots_used() + u8::from(self.lock & 0b10 != 0);
        let s = self.suffixes.slots_used() + u8::from(self.lock & 0b01 != 0);
        (p, s)
    }

    /// Free slots on each side (saturating).
    pub fn free(&self, b: &Bundle, side: Side) -> u8 {
        let (cap_p, cap_s) = self.caps(b);
        let (used_p, used_s) = self.used();
        let (cap, used) = match side {
            Side::Prefix => (cap_p, used_p),
            Side::Suffix => (cap_s, used_s),
        };
        cap.saturating_sub(used)
    }

    pub fn any_free(&self, b: &Bundle) -> bool {
        Side::ALL.iter().any(|s| self.free(b, *s) > 0)
    }

    /// Number of mods on both sides (the lock metamod is not a real mod).
    pub fn mod_count(&self) -> u8 {
        self.prefixes.slots_used() + self.suffixes.slots_used()
    }

    /// How many of this state's mods carry an influence tag.
    pub fn influenced_mods(&self) -> u8 {
        self.prefixes.untracked_influenced + self.suffixes.untracked_influenced
    }
}
