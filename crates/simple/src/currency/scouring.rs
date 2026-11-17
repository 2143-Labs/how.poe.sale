//! Orb of Scouring: strips a magic or rare item's explicit modifiers and returns it to normal
//! rarity, keeping the item itself — base, item level, influence, quality, flags and implicits
//! (`currency/scouring`).

use super::Applied;
use super::{Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::item::{Rarity, Side};
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The Orb of Scouring, one step. It is deterministic, so its distribution is one item at the whole
/// chance.
pub struct Scouring;

impl Named for Scouring {
    fn id(&self) -> CurrencyId {
        CurrencyId::SCOURING
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic, Rarity::Rare]
    }
}

impl<S: Stock> Currency<S> for Scouring {
    fn apply(&self, _rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        if state.lock() == Some(Lock::Corrupted) {
            return Err(Refusal("a corrupted item cannot be scoured"));
        }
        if state.lock() == Some(Lock::Mirrored) {
            return Err(Refusal("a mirrored item cannot be scoured"));
        }
        if !matches!(state.rarity(), Rarity::Magic | Rarity::Rare) {
            return Err(Refusal("an Orb of Scouring needs a magic or rare item"));
        }
        if state.held(Side::Prefix) + state.held(Side::Suffix) == 0 {
            return Err(Refusal("the item has no explicit modifiers to scour"));
        }
        // Everything but the explicits survives: the same base, item level, influence, quality,
        // flags and implicits, back at normal rarity. A scouring is not a fresh item.
        Ok(Applied {
            outcomes: Dist::one(state.cleared().with_rarity(Rarity::Normal)),
            cost: Cost::of(self.id(), 1.0),
        })
    }
}
