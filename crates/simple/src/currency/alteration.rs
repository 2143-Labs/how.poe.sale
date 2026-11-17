//! Orb of Alteration: rerolls a magic item into one or two fresh modifiers, half the time each way
//! (`currency/alteration`).

use super::Applied;
use super::{Currency, Named, adds, mix};
use crate::cost::{Cost, CurrencyId};
use crate::item::Rarity;
use crate::stock::{Chance, Lock, Refusal, Stock};

/// The Orb of Alteration, one step.
pub struct Alteration;

impl Named for Alteration {
    fn id(&self) -> CurrencyId {
        CurrencyId::ALTERATION
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic]
    }
}

impl<S: Stock> Currency<S> for Alteration {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be altered"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be altered"));
        }
        if state.rarity() != Rarity::Magic {
            return Err(Refusal("an Orb of Alteration needs a magic item"));
        }
        // Every modifier goes before the new ones are drawn, so the pool the draws see is the pool
        // of an empty magic item, not of the item that was just emptied.
        let fresh = state.cleared();
        let outcomes = mix(vec![
            (S::Chance::half(), adds(rules, &fresh, 1)?),
            (S::Chance::half(), adds(rules, &fresh, 2)?),
        ])?;
        Ok(Applied {
            outcomes,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
