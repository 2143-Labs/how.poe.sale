//! Orb of Augmentation: adds one modifier to a magic item that has room (`currency/augmentation`).

use super::Applied;
use super::{Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::item::{Rarity, Side};
use crate::pool;
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The Orb of Augmentation, one step.
pub struct Augmentation;

impl Named for Augmentation {
    fn id(&self) -> CurrencyId {
        CurrencyId::AUGMENTATION
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic]
    }
}

impl<S: Stock> Currency<S> for Augmentation {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be augmented"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be augmented"));
        }
        if state.rarity() != Rarity::Magic {
            return Err(Refusal("an Orb of Augmentation needs a magic item"));
        }
        if !Side::BOTH
            .into_iter()
            .any(|side| pool::rarity_cap(state.rarity()) > state.held(side))
        {
            return Err(Refusal("the item has no room for another modifier"));
        }
        let drawn = state.add(rules)?;
        if drawn.is_empty() {
            return Err(Refusal("no modifier can roll on this item"));
        }
        Ok(Applied {
            outcomes: Dist::new(drawn)?,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
