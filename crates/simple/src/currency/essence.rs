//! An essence: a normal or rare item is reforged as a rare one holding the essence's own modifier
//! and drawn ones (`essence/<name>`).
//!
//! The rule is the one the repository's other engine encodes and sources (the essence methods
//! `crates/cli/src/methods.rs` generates, run by `Op::AddEssence` and `Op::Fill` in
//! `crates/poe1/src/apply.rs`):
//!
//! - every explicit modifier goes and the item becomes rare;
//! - the essence's modifier for the item's class is put on first, not drawn;
//! - the item ends with 4, 5 or 6 explicit modifiers at 8:3:1, the essence's counted, so 3, 4 or
//!   5 are drawn (`rules/methods.json`, `currency/chaos`; claims `chaos.count`, `reforge.counts`);
//! - each draw is the side first, in equal shares among the sides with room, then the modifier by
//!   weight (claim `reforge.side-first`), which is [`Stock::add`].
//!
//! An essence works on a normal or a rare item: the export describes Essence of Horror as
//! "Upgrades a normal item to rare or reforges a rare item, guaranteeing one property". Which
//! modifier it places depends on the item's class (`essences.json`), so an [`Essence`] names its
//! modifier for the one base a data file holds.
//!
//! On an [`Item`] the modifier is looked up in the data. On a [`Node`] the [`Law`] gives its side
//! and grade, and has none for a modifier whose family a draw could also take, because the law
//! would not hold after it.

use super::{Applied, Currency, Named, adds, mix};
use crate::classes::Grade;
use crate::cost::{Cost, CurrencyId};
use crate::data::Data;
use crate::item::{Affix, Item, Rarity, Side};
use crate::law::Law;
use crate::node::Node;
use crate::stock::{Chance, Dist, Lock, Refusal, Stock};

/// How many explicit modifiers the reforged item ends with, and the weight of each count.
const COUNTS: [(usize, u128); 3] = [(4, 8), (5, 3), (6, 1)];

/// One essence on one base: the id its cost is keyed by, and the modifier it places.
pub struct Essence {
    pub id: CurrencyId,
    /// The export id of the modifier, e.g. `SocketedGemsDealMoreElementalDamageEssence1`.
    pub modifier: &'static str,
}

/// Essence of Horror on a helmet (`essences.json`): the essence the crown's craft uses.
pub static ESSENCE_OF_HORROR: Essence = Essence {
    id: CurrencyId::ESSENCE_OF_HORROR,
    modifier: "SocketedGemsDealMoreElementalDamageEssence1",
};

impl Named for Essence {
    fn id(&self) -> CurrencyId {
        self.id
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Rare]
    }
}

impl Currency<Item> for Essence {
    fn apply(&self, rules: &Data, state: &Item) -> Result<Applied<Item>, Refusal> {
        guard(state)?;
        let m = rules
            .id_of(self.modifier)
            .ok_or(Refusal("the data has no modifier for this essence"))?;
        let side = rules.mod_at(m).kind.side().ok_or(Refusal(
            "the essence's modifier is neither a prefix nor a suffix",
        ))?;
        let mut placed = state.cleared().with_rarity(Rarity::Rare);
        placed.affixes_mut(side).push(Affix::at_range(rules, m));
        Ok(Applied {
            outcomes: fill(rules, &placed)?,
            cost: Cost::of(self.id, 1.0),
        })
    }
}

impl<G: Grade> Currency<Node<G>> for Essence {
    fn apply(&self, rules: &Law<G>, state: &Node<G>) -> Result<Applied<Node<G>>, Refusal> {
        guard(state)?;
        let (side, tier) = rules
            .placed(self.modifier)
            .ok_or(Refusal("the law cannot place this essence's modifier"))?;
        let placed = state
            .cleared()
            .with_rarity(Rarity::Rare)
            .with_placed(side, tier);
        Ok(Applied {
            outcomes: fill(rules, &placed)?,
            cost: Cost::of(self.id, 1.0),
        })
    }
}

/// What every essence checks first: the lock, corrupted before mirrored, then the rarity.
fn guard<S: Stock>(state: &S) -> Result<(), Refusal> {
    if state.lock() == Some(Lock::Corrupted) {
        return Err(Refusal("a corrupted item cannot take an essence"));
    }
    if state.lock() == Some(Lock::Mirrored) {
        return Err(Refusal("a mirrored item cannot take an essence"));
    }
    if !matches!(state.rarity(), Rarity::Normal | Rarity::Rare) {
        return Err(Refusal("an essence needs a normal or rare item"));
    }
    Ok(())
}

/// The draws after the essence's own modifier, for each total in [`COUNTS`] at its share: as many
/// adds in a row as the total lacks. A draw with nothing left to take leaves the item as it is, and
/// when not even the first can be taken the item keeps the essence's modifier alone.
fn fill<S: Stock>(rules: &S::Rules, placed: &S) -> Result<Dist<S, S::Chance>, Refusal> {
    let held: usize = Side::BOTH.into_iter().map(|side| placed.held(side)).sum();
    let whole: u128 = COUNTS.iter().map(|&(_, weight)| weight).sum();
    let mut branches = Vec::with_capacity(COUNTS.len());
    for (total, weight) in COUNTS {
        let share =
            S::Chance::share(weight, whole).ok_or(Refusal("the chances do not fit in 128 bits"))?;
        let mut drawn = adds(rules, placed, total.saturating_sub(held))?;
        if drawn.is_empty() {
            drawn = vec![(placed.clone(), S::Chance::ONE)];
        }
        branches.push((share, drawn));
    }
    mix(branches)
}
