//! The label one item gets relative to a goal filter: what the loop's next move can be read from.
//!
//! A stage is a property of one item and one filter, never of a step, so the same function labels a
//! starting item, an outcome of the machine and a hand-written fixture. It answers only from what
//! the item holds: its rarity, which sides have affixes, and the best bucket on each side.

use crate::currency::{ItemSetFilter, Tier};
use crate::data::{Data, Rarity, Side};
use crate::item::Item;

/// Where an item stands against the goal.
///
/// The first seven labels every item; `Reset` and `Stop` are the loop's terminals and are only ever
/// produced by the machine's summary — no item classifies into them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// A desired modifier is held: the loop is done with this item.
    Desired,
    /// Good on both sides, no desired modifier: the loop keeps it as it is.
    GoodPair,
    /// A good prefix with the suffix side empty: one more draw is worth taking.
    GoodPrefix,
    /// A good suffix with the prefix side empty.
    GoodSuffix,
    /// Has affixes, none of them good: an alteration reroll, not a keep.
    Other,
    /// No explicit affixes: normal, or an item that was never rolled.
    Spare,
    /// A rare item without the desired modifier: its affixes are finished, so only a reset is left.
    ///
    /// Split out of [`Stage::Other`] because the summary must show how much mass is about to be
    /// scoured, and because the owner's loop has a different answer for it ("stop" once three of its
    /// modifiers are good) than for a magic item's ordinary reroll.
    SpentRare,
    /// Terminal: a rare item without the goal that the loop scours and restarts from nothing.
    Reset,
    /// Terminal: the loop must not continue; hand the item to its owner.
    Stop,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Desired => "desired",
            Stage::GoodPair => "good pair",
            Stage::GoodPrefix => "good prefix",
            Stage::GoodSuffix => "good suffix",
            Stage::Other => "other",
            Stage::Spare => "spare",
            Stage::SpentRare => "spent rare",
            Stage::Reset => "reset",
            Stage::Stop => "stop",
        }
    }

    /// Whether the loop has no move left for this mass.
    pub fn is_terminal(self) -> bool {
        matches!(self, Stage::Reset | Stage::Stop)
    }

    /// The stage of one item under one goal.
    ///
    /// An empty side and a good side are facts worth printing, so a rare item can classify as
    /// [`Stage::GoodPrefix`] — the policy is what knows a rare item cannot be augmented, and it
    /// reads the rarity for that. [`Stage::SpentRare`] is therefore "a rare item with affixes on
    /// both sides and no desired modifier", the stage that has nothing left but a reset.
    pub fn classify(data: &Data, filter: &ItemSetFilter, item: &Item) -> Stage {
        let prefix = filter.best(data, item, Side::Prefix);
        let suffix = filter.best(data, item, Side::Suffix);
        if prefix == Some(Tier::Desired) || suffix == Some(Tier::Desired) {
            return Stage::Desired;
        }
        let empty_prefix = item.affixes(Side::Prefix).is_empty();
        let empty_suffix = item.affixes(Side::Suffix).is_empty();
        if prefix == Some(Tier::Good) && suffix == Some(Tier::Good) {
            return Stage::GoodPair;
        }
        if prefix == Some(Tier::Good) && empty_suffix {
            return Stage::GoodPrefix;
        }
        if suffix == Some(Tier::Good) && empty_prefix {
            return Stage::GoodSuffix;
        }
        if empty_prefix && empty_suffix {
            return Stage::Spare;
        }
        if item.rarity == Rarity::Rare {
            return Stage::SpentRare;
        }
        Stage::Other
    }
}
