//! The crafting bench: one chosen modifier added with certainty, at the price the export lists for
//! the option (`bench/EinharMasterIncreasedLife4`).
//!
//! The bench is not an orb. It does not roll, and it is bought by option rather than held in the
//! inventory; it is a [`Currency`] all the same, because it changes an item by a known amount for a
//! known cost. The cost stays its own line ([`CurrencyId::ALCHEMY`]) rather than being folded into
//! chaos: nothing here prices an orb of alchemy.
//!
//! The option is `EinharMasterIncreasedLife4` in `crafting_bench_options.json`: a prefix (required
//! level 75, family `IncreasedLife`, stat `base_maximum_life`) listed for the `Boots` item class.
//! The export prices it as `CurrencyUpgradeToRare: 4`, and `base_items.json` names that base "Orb
//! of Alchemy".
//!
//! What a [`Node`] cannot say, and this currency therefore cannot honour:
//!
//! - the chosen modifier's family. `IncreasedLife` is held after the use, and later random draws
//!   must avoid it; a node records grades, not families, so it does not.
//! - the chosen modifier's identity. It is a `crafted`-domain modifier and is not in the
//!   item-domain pool, so the filter cannot grade it: it is recorded as [`Tier::Any`], and only the
//!   crafted flag says it is there at all.
//!
//! The flag is what an item does record: at most one crafted modifier. A second use is refused.

use super::{Applied, Currency, Named};
use crate::cost::{Cost, CurrencyId};
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::law::Law;
use crate::node::Node;
use crate::stock::{Dist, Refusal, Stock};

/// The bench, as one currency: the best life prefix it sells for boots, added to the prefix side.
pub struct Bench;

impl Named for Bench {
    fn id(&self) -> CurrencyId {
        CurrencyId::BENCH
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Magic, Rarity::Rare]
    }
}

impl Currency<Node> for Bench {
    fn apply(&self, rules: &Law, state: &Node) -> Result<Applied<Node>, Refusal> {
        if !matches!(state.rarity(), Rarity::Magic | Rarity::Rare) {
            return Err(Refusal("the crafting bench needs a magic or rare item"));
        }
        if state.crafted() {
            return Err(Refusal("the item already holds a crafted modifier"));
        }
        if state.held(Side::Prefix) >= state.cap(rules) {
            return Err(Refusal("the item has no room for another prefix"));
        }
        Ok(Applied {
            outcomes: Dist::one(state.with_crafted(Side::Prefix, Tier::Any)),
            cost: Cost::of(CurrencyId::ALCHEMY, 4.0),
        })
    }
}
