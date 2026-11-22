//! Orb of Transmutation: a normal item becomes magic holding one or two modifiers, half the time
//! each way (`currency/transmutation`).

use super::Applied;
use super::{Currency, Named, adds, mix};
use crate::cost::{Cost, CurrencyId};
use crate::item::Rarity;
use crate::stock::{Chance, Lock, Refusal, Stock};

/// The Orb of Transmutation, one step.
pub struct Transmutation;

impl Named for Transmutation {
    fn id(&self) -> CurrencyId {
        CurrencyId::TRANSMUTATION
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Normal]
    }
}

impl<S: Stock> Currency<S> for Transmutation {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be transmuted"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be transmuted"));
        }
        if state.rarity() != Rarity::Normal {
            return Err(Refusal("an Orb of Transmutation needs a normal item"));
        }
        let magic = state.with_rarity(Rarity::Magic);
        let outcomes = mix(vec![
            (S::Chance::half(), adds(rules, &magic, 1)?),
            (S::Chance::half(), adds(rules, &magic, 2)?),
        ])?;
        Ok(Applied {
            outcomes,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
