//! Divine Orb: widens a magic or rare item's explicit values to the full range of their stats
//! (`currency/divine`).
//!
//! A Divine touches values and nothing else. The base, the item level, the influence, the quality,
//! the flags and the implicits come out of the orb exactly as they went in, and not one explicit
//! modifier is added, removed or replaced — each held modifier keeps its id and takes the
//! `[min, max]` its stat rolls in. That is why the set is one item at the whole chance: the crate
//! knows a value as an interval, and the full interval is the widest a Divine can make it.

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::data::Data;
use crate::item::{Affix, Item, Rarity, Side};

/// The Divine Orb, one step.
pub struct Divine<'d> {
    data: &'d Data,
}

impl<'d> Divine<'d> {
    pub fn new(data: &'d Data) -> Divine<'d> {
        Divine { data }
    }
}

impl Currency for Divine<'_> {
    fn id(&self) -> &'static str {
        "currency/divine"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be divined");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be divined");
        }
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("a Divine Orb needs a magic or rare item");
        }
        // Every explicit affix is rewritten at its stat's full range, in place, so the outcome is the
        // same item with new values: same ids on the same sides in the same order, same implicit,
        // same base, level, influence, quality and flags.
        let mut widened = item.clone();
        let mut changed = false;
        for side in Side::BOTH {
            for (held, affix) in item.affixes(side).iter().enumerate() {
                let full = Affix::at_range(self.data, affix.m);
                if full.rolls != affix.rolls {
                    changed = true;
                }
                widened.affixes_mut(side)[held] = full;
            }
        }
        // An item every affix of which already sits at its full range has nothing left to gain; a
        // step that can only return its own input is not a step.
        if !changed {
            return ItemOutputSet::refused(
                "the item's values are already as wide as a Divine can make them",
            );
        }
        let set = ItemOutputSet::new(vec![(widened, 1, None)]);
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
