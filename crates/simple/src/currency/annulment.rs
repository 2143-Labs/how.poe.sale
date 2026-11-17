//! Orb of Annulment: removes one explicit modifier from a magic or rare item, in equal shares among
//! the modifiers it may take (`currency/annulment`).
//!
//! It is item-only: an abstract item does not remember which group the removed modifier belonged
//! to, so the same orb on it could not say what is left.

use super::Applied;
use super::{Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::data::Data;
use crate::fraction::Fraction;
use crate::item::{Item, Rarity, Side};
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The Orb of Annulment, one step: one modifier goes, each in the same share.
pub struct Annulment;

impl Named for Annulment {
    fn id(&self) -> CurrencyId {
        CurrencyId::ANNULMENT
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic, Rarity::Rare]
    }
}

impl Currency<Item> for Annulment {
    fn apply(&self, _rules: &Data, state: &Item) -> Result<Applied<Item>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be annulled"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be annulled"));
        }
        if !matches!(state.rarity(), Rarity::Magic | Rarity::Rare) {
            return Err(Refusal("an Orb of Annulment needs a magic or rare item"));
        }
        if state.explicits() == 0 {
            return Err(Refusal("the item has no explicit modifiers to annul"));
        }
        // One outcome per held modifier, each at the same share: the orb picks a modifier, not a
        // side, so a side holding two of the item's four modifiers loses one twice as often. The
        // rarity is left as it was — an annulment does not recompute it.
        let held = state.explicits();
        let share = Fraction::new(1, held as u128).expect("the item holds a modifier");
        let mut parts = Vec::with_capacity(held);
        for side in Side::BOTH {
            for index in 0..state.affixes(side).len() {
                let mut result = state.clone();
                result.affixes_mut(side).remove(index);
                parts.push((result, share));
            }
        }
        Ok(Applied {
            outcomes: Dist::new(parts)?,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
