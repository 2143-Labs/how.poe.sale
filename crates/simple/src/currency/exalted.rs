//! Exalted Orb: adds one modifier to a rare item that has room (`currency/exalted`).

use super::Applied;
use super::{Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::item::{Rarity, Side};
use crate::pool;
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The Exalted Orb, one step.
pub struct Exalted;

impl Named for Exalted {
    fn id(&self) -> CurrencyId {
        CurrencyId::EXALTED
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Rare]
    }
}

impl<S: Stock> Currency<S> for Exalted {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be exalted"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be exalted"));
        }
        if state.rarity() != Rarity::Rare {
            return Err(Refusal("an Exalted Orb needs a rare item"));
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
