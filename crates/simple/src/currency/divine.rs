//! Divine Orb: widens a magic or rare item's explicit values to the full range of their stats
//! (`currency/divine`).
//!
//! A Divine touches values and nothing else. The base, the item level, the influence, the quality,
//! the flags and the implicits come out of the orb exactly as they went in, and not one explicit
//! modifier is added, removed or replaced — each held modifier keeps its id and takes the
//! `[min, max]` its stat rolls in. That is why the distribution is one item at the whole chance:
//! the crate knows a value as an interval, and the full interval is the widest a Divine can make it.
//!
//! It is item-only: an abstract item has no values to widen.

use super::Applied;
use super::{Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::data::Data;
use crate::item::{Affix, Item, Rarity, Side};
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The Divine Orb, one step.
pub struct Divine;

impl Named for Divine {
    fn id(&self) -> CurrencyId {
        CurrencyId::DIVINE
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic, Rarity::Rare]
    }
}

impl Currency<Item> for Divine {
    fn apply(&self, rules: &Data, state: &Item) -> Result<Applied<Item>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be divined"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be divined"));
        }
        if !matches!(state.rarity(), Rarity::Magic | Rarity::Rare) {
            return Err(Refusal("a Divine Orb needs a magic or rare item"));
        }
        // Every explicit affix is rewritten at its stat's full range, in place, so the outcome is
        // the same item with new values: same ids on the same sides in the same order, same
        // implicit, same base, level, influence, quality and flags.
        let mut widened = state.clone();
        let mut changed = false;
        for side in Side::BOTH {
            for (held, affix) in state.affixes(side).iter().enumerate() {
                let full = Affix::at_range(rules, affix.m);
                if full.rolls != affix.rolls {
                    changed = true;
                }
                widened.affixes_mut(side)[held] = full;
            }
        }
        // An item every affix of which already sits at its full range has nothing left to gain; a
        // step that can only return its own input is not a step.
        if !changed {
            return Err(Refusal(
                "the item's values are already as wide as a Divine can make them",
            ));
        }
        Ok(Applied {
            outcomes: Dist::one(widened),
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
