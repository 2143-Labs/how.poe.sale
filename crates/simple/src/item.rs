//! What a craft holds: a rarity and the explicit modifiers on each side.

/// The two sides a modifier can sit on.
///
/// The order is the one every side-major loop in this crate uses: prefixes first. `Prefix.index()`
/// is `0` and `Suffix.index()` is `1`, so `[Side::BOTH]`-indexed arrays line up with the affix
/// arrays in [`Item`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Prefix,
    Suffix,
}

impl Side {
    /// Both sides, prefix first.
    pub const BOTH: [Side; 2] = [Side::Prefix, Side::Suffix];

    pub fn index(self) -> usize {
        match self {
            Side::Prefix => 0,
            Side::Suffix => 1,
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Prefix => Side::Suffix,
            Side::Suffix => Side::Prefix,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::Prefix => "prefix",
            Side::Suffix => "suffix",
        }
    }

    /// The side a data file's `type` field names, or `None` for anything else (an implicit, a
    /// corrupted modifier, a crafted one).
    pub fn from_name(name: &str) -> Option<Side> {
        match name {
            "prefix" => Some(Side::Prefix),
            "suffix" => Some(Side::Suffix),
            _ => None,
        }
    }
}

/// The three rarities a craft here moves between. Influenced, fractured and corrupted states are
/// not modelled: this crate's data has none of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
}

impl Rarity {
    pub fn name(self) -> &'static str {
        match self {
            Rarity::Normal => "normal",
            Rarity::Magic => "magic",
            Rarity::Rare => "rare",
        }
    }
}

/// One held modifier, as its index into [`crate::data::Data`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Affix {
    pub m: u16,
}

impl Affix {
    pub fn new(m: u16) -> Affix {
        Affix { m }
    }
}

/// A base, its rarity, and its explicit modifiers per side.
///
/// The affix vectors are private: a modifier is added or removed through [`Item::affixes_mut`], so
/// no caller can put a fourth prefix on a rare item without going through the room rule in
/// [`crate::pool`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Item {
    pub rarity: Rarity,
    affixes: [Vec<Affix>; 2],
}

impl Item {
    pub fn new(rarity: Rarity) -> Item {
        Item {
            rarity,
            affixes: [Vec::new(), Vec::new()],
        }
    }

    pub fn affixes(&self, side: Side) -> &[Affix] {
        &self.affixes[side.index()]
    }

    pub fn affixes_mut(&mut self, side: Side) -> &mut Vec<Affix> {
        &mut self.affixes[side.index()]
    }

    /// How many explicit modifiers the item holds, both sides together.
    pub fn explicits(&self) -> usize {
        Side::BOTH.into_iter().map(|s| self.affixes(s).len()).sum()
    }

    /// The same affixes at another rarity.
    pub fn with_rarity(&self, rarity: Rarity) -> Item {
        Item {
            rarity,
            affixes: self.affixes.clone(),
        }
    }

    /// The same item with every affix gone.
    pub fn cleared(&self) -> Item {
        Item::new(self.rarity)
    }
}
