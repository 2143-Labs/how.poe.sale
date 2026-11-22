//! Test-only helpers shared by the integration tests.
//!
//! [`Restart`] is a currency whose outcomes do not depend on the input but whose cost does: it
//! proves that a cost is a property of the use, not of the step.

use simple::cost::{Cost, CurrencyId};
use simple::currency::{Applied, Currency, Named, Transmutation};
use simple::item::Rarity;
use simple::stock::{Refusal, Stock};

/// Restarts a stock: a scouring and a transmutation the first time, a transmutation alone after.
pub struct Restart;

impl Named for Restart {
    fn id(&self) -> CurrencyId {
        CurrencyId("test/restart")
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic, Rarity::Rare]
    }
}

impl<S: Stock> Currency<S> for Restart {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal> {
        let normal = state.rarity() == Rarity::Normal;
        let fresh = state.cleared().with_rarity(Rarity::Normal);
        let entry = if normal { state } else { &fresh };
        let mut applied = Transmutation.apply(rules, entry)?;
        applied.cost = if normal {
            Cost::of(CurrencyId::TRANSMUTATION, 1.0)
        } else {
            Cost::of(CurrencyId::SCOURING, 1.0).plus(&Cost::of(CurrencyId::TRANSMUTATION, 1.0))
        };
        Ok(applied)
    }
}
