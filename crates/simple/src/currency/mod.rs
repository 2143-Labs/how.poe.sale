//! The eight crafting orbs, one file each, behind one trait.
//!
//! An orb answers one question: given a [`Stock`], what can it become, how often, and what does the
//! use cost? The answer is an [`Applied`] — outcomes with their chances, plus the [`Cost`] of this
//! use — or a [`Refusal`] naming why the orb cannot be used. Nothing here rolls dice; a caller walks
//! the outcomes.
//!
//! [`ON_ITEMS`] names exactly these eight ids and nothing else:
//!
//! - `currency/transmutation` — normal → magic, one or two modifiers
//! - `currency/alteration` — magic rerolled, one or two fresh modifiers
//! - `currency/augmentation` — one modifier added to a magic item
//! - `currency/regal` — magic → rare, one modifier added
//! - `currency/exalted` — one modifier added to a rare item
//! - `currency/scouring` — everything stripped, back to normal
//! - `currency/annulment` — one explicit modifier removed
//! - `currency/divine` — every explicit value widened to its range
//!
//! Six orbs are written once for every [`Stock`]: transmutation, alteration, augmentation, regal,
//! exalted and scouring. Annulment and Divine are item-only, because one forgets which group left
//! the item and the other changes only values, so neither can be reproduced on an abstract item.
//! The crafting bench ([`Bench`]) is node-only: it places a chosen modifier on an abstract item,
//! and a concrete item has nowhere to record that it did.
//!
//! Corruption and mirroring block all eight: an item either flagged is refused by every orb, with
//! the reason naming the orb's own verb, because none of them may modify an item the game has
//! locked. Each orb asks [`Stock::lock`] first and the rarity second.
//!
//! An essence ([`Essence`]) is a currency too, but in neither list: the modifier it places depends
//! on the base's class, so a caller builds one for the essence and the base it holds.
//!
//! A ninth orb, `currency/shapers_exalted` ([`ShapersExalted`]), sits beside the eight and in
//! neither list: what it adds to an abstract item is not the law's pool, so it is built from the
//! start item a law is built from rather than written as a constant.

pub mod alteration;
pub mod annulment;
pub mod augmentation;
pub mod bench;
pub mod divine;
pub mod essence;
pub mod exalted;
pub mod regal;
pub mod scouring;
pub mod shapers_exalted;
pub mod transmutation;

pub use alteration::Alteration;
pub use annulment::Annulment;
pub use augmentation::Augmentation;
pub use bench::Bench;
pub use divine::Divine;
pub use essence::{ESSENCE_OF_HORROR, Essence};
pub use exalted::Exalted;
pub use regal::Regal;
pub use scouring::Scouring;
pub use shapers_exalted::ShapersExalted;
pub use transmutation::Transmutation;

use crate::cost::{Cost, CurrencyId};
use crate::item::{Item, Rarity};
use crate::node::Node;
use crate::stock::{Chance, Dist, Refusal, Stock};

/// One modelled orb, named by the id the game's rules use (`currency/alteration`).
pub trait Named {
    fn id(&self) -> CurrencyId;

    /// The rarities this currency can be used on. `apply` refuses every other one.
    fn works_on(&self) -> &'static [Rarity];
}

/// One transform on a stock: what it can become, how often, and what the use costs.
pub trait Currency<S: Stock>: Named {
    fn apply(&self, rules: &S::Rules, state: &S) -> Result<Applied<S>, Refusal>;
}

/// One use: the outcomes with their chances, and the cost of this use.
pub struct Applied<S: Stock> {
    pub outcomes: Dist<S, S::Chance>,
    pub cost: Cost,
}

/// Every way a draw can come out, each with its chance.
pub type Ways<S> = Vec<(S, <S as Stock>::Chance)>;

/// `n` adds in a row.
///
/// Each draw is normalised on its own state: a branch keeps the chance it was drawn with, because
/// the next draw's pool depends on what the draw just added. The first draw must happen — an empty
/// result means nothing could be added, and the orb refuses — while a later draw with nothing left
/// to take leaves the state as it is and the draws after it still run.
pub fn adds<S: Stock>(rules: &S::Rules, state: &S, n: usize) -> Result<Ways<S>, Refusal> {
    let mut current: Ways<S> = vec![(state.clone(), S::Chance::ONE)];
    for draw in 0..n {
        let mut next: Ways<S> = Vec::new();
        for (state, chance) in current {
            let branches = state.add(rules)?;
            if branches.is_empty() {
                if draw == 0 {
                    return Ok(Vec::new());
                }
                next.push((state, chance));
                continue;
            }
            for (result, share) in branches {
                let combined = chance
                    .times(share)
                    .ok_or(Refusal("the chances do not fit in 128 bits"))?;
                next.push((result, combined));
            }
        }
        current = next;
    }
    Ok(current)
}

/// Branches with their shares, as one distribution: each branch's chance is multiplied by its
/// share and equal outcomes are folded.
pub fn mix<S: Stock>(branches: Vec<(S::Chance, Ways<S>)>) -> Result<Dist<S, S::Chance>, Refusal> {
    let mut parts = Vec::new();
    for (share, results) in branches {
        for (state, chance) in results {
            let combined = share
                .times(chance)
                .ok_or(Refusal("the chances do not fit in 128 bits"))?;
            parts.push((state, combined));
        }
    }
    Dist::new(parts)
}

/// Every orb this crate models, in the order of the list above.
pub const ON_ITEMS: [&dyn Currency<Item>; 8] = [
    &Transmutation,
    &Alteration,
    &Augmentation,
    &Regal,
    &Exalted,
    &Scouring,
    &Annulment,
    &Divine,
];

/// The orbs that can be used on an abstract item, in the same order.
pub const ON_NODES: [&dyn Currency<Node>; 6] = [
    &Transmutation,
    &Alteration,
    &Augmentation,
    &Regal,
    &Exalted,
    &Scouring,
];

/// The orb a policy's id names, or `None` if this crate does not model it.
pub fn find<'a, S: Stock>(
    set: &[&'a dyn Currency<S>],
    id: CurrencyId,
) -> Option<&'a dyn Currency<S>> {
    set.iter().copied().find(|currency| currency.id() == id)
}
