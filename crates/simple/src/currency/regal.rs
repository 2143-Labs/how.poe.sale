//! Orb of Regal: a magic item becomes rare and gains one modifier (`currency/regal`).

use super::Applied;
use super::{Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::item::Rarity;
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The Orb of Regal, one step.
pub struct Regal;

impl Named for Regal {
    fn id(&self) -> CurrencyId {
        CurrencyId::REGAL
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic]
    }
}

impl<S: Stock> Currency<S> for Regal {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be regaled"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be regaled"));
        }
        if state.rarity() != Rarity::Magic {
            return Err(Refusal("an Orb of Regal needs a magic item"));
        }
        // The draw is taken on the *rare* item, which is what the orb hands to the next draw: a
        // magic item with one prefix may gain a second prefix, because a rare side has room for
        // three, and a modifier whose group is already held is still out of the pool.
        let rare = state.with_rarity(Rarity::Rare);
        let drawn = rare.add(rules)?;
        if drawn.is_empty() {
            return Err(Refusal("no modifier can roll on this item"));
        }
        Ok(Applied {
            outcomes: Dist::new(drawn)?,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
