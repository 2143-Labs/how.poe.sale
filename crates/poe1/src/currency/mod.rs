//! One file per currency. Each is a single struct implementing [`Currency`]: its [`Currency::apply`]
//! takes an item and an optional [`ItemSetFilter`] and returns the exact distribution of items that
//! step produces.
//!
//! Chances are never floats on the way in or out. The pool hands out integer weights (`spawn ×
//! generation percent`, straight out of data), a distribution is those integers over one shared
//! total, and every reported chance is a [`Fraction`] of them. A set that cannot be held exactly
//! says so instead of rounding.

pub mod filter;
pub mod fraction;

mod alteration;
mod annulment;
mod augmentation;
mod exalted;
mod regal;
mod scouring;
mod transmutation;

pub use alteration::Alteration;
pub use annulment::Annulment;
pub use augmentation::Augmentation;
pub use exalted::Exalted;
pub use filter::{AbstractMod, Accept, ItemSetFilter, ModSelector, Tier};
pub use fraction::Fraction;
pub use regal::Regal;
pub use scouring::Scouring;
pub use transmutation::Transmutation;

use std::collections::BTreeMap;

use crate::apply;
use crate::data::{Data, ModData, Rarity, Side};
use crate::item::{Affix, Item};
use crate::method::{Pool, SideReq};

/// A currency, in one struct and one file.
pub trait Currency {
    /// The method id the rules use, e.g. `currency/alteration`.
    fn id(&self) -> &'static str;

    /// One step: every item this currency can produce from `item`, with exact chances, after the
    /// optional filter has been applied to the set.
    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet;
}

/// A step's result before it is folded into a set: the item, its weight, and the modifier the step
/// added if it added one.
pub type Weighted = (Item, u128, Option<(Side, u32)>);

/// One result of a step: the item, its share of the step as an integer weight, and the modifier the
/// step added, if it added one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub item: Item,
    pub weight: u128,
    pub added: Option<(Side, u32)>,
}

/// What a step produces. The weights of the [`ItemOutputSet::Outcomes`] always sum to `total`, so the
/// chances sum to exactly one — and an inapplicable currency produces no distribution at all, rather
/// than a set of zeroes that would read as "0 %".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemOutputSet {
    /// The currency cannot be applied to this item; the reason is for the reader.
    Refused(&'static str),
    Outcomes {
        outcomes: Vec<Outcome>,
        total: u128,
    },
}

impl ItemOutputSet {
    pub fn refused(why: &'static str) -> ItemOutputSet {
        ItemOutputSet::Refused(why)
    }

    /// Builds a set from weighted results, folding items that came out the same so the set is
    /// canonical and their weights add up.
    pub fn new(data: &Data, weighted: Vec<Weighted>) -> ItemOutputSet {
        let mut folded: BTreeMap<Item, (u128, Option<(Side, u32)>)> = BTreeMap::new();
        for (mut item, weight, added) in weighted {
            // The engine's one form: a fixed roll is filled in, so two results that are the same
            // item fold together and compare equal to what the op-declared path produces.
            item.canonical(data);
            let slot = folded.entry(item).or_insert((0, added));
            slot.0 = slot.0.saturating_add(weight);
        }
        let total = folded.values().map(|&(w, _)| w).sum();
        let outcomes = folded
            .into_iter()
            .map(|(item, (weight, added))| Outcome {
                item,
                weight,
                added,
            })
            .collect();
        ItemOutputSet::Outcomes { outcomes, total }
    }

    /// The outcomes, or an empty slice when the currency was refused.
    pub fn outcomes(&self) -> &[Outcome] {
        match self {
            ItemOutputSet::Refused(_) => &[],
            ItemOutputSet::Outcomes { outcomes, .. } => outcomes,
        }
    }

    pub fn total(&self) -> u128 {
        match self {
            ItemOutputSet::Refused(_) => 0,
            ItemOutputSet::Outcomes { total, .. } => *total,
        }
    }

    /// Whether the weights really do sum to the total — the invariant every report relies on.
    pub fn exact(&self) -> bool {
        match self {
            ItemOutputSet::Refused(_) => true,
            ItemOutputSet::Outcomes { outcomes, total } => {
                *total > 0 && outcomes.iter().map(|o| o.weight).sum::<u128>() == *total
            }
        }
    }

    /// The exact share of the step whose result satisfies `want`; `None` when refused.
    pub fn chance(&self, mut want: impl FnMut(&Item) -> bool) -> Option<Fraction> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let kept: u128 = self
            .outcomes()
            .iter()
            .filter(|o| want(&o.item))
            .map(|o| o.weight)
            .sum();
        Fraction::new(kept, total)
    }

    /// "Mod filtering a crafting step": keeps the outcomes the filter accepts, with the chances
    /// renormalised exactly over what is left.
    pub fn filtered(&self, data: &Data, filter: &ItemSetFilter) -> ItemOutputSet {
        match self {
            ItemOutputSet::Refused(why) => ItemOutputSet::refused(why),
            ItemOutputSet::Outcomes { outcomes, .. } => {
                let kept: Vec<Outcome> = outcomes
                    .iter()
                    .filter(|o| filter.accepts(data, &o.item))
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    return ItemOutputSet::refused("no outcome meets the filter");
                }
                let total = kept.iter().map(|o| o.weight).sum();
                ItemOutputSet::Outcomes {
                    outcomes: kept,
                    total,
                }
            }
        }
    }
}

/// Combines the branches of one step (`0.5 · one modifier`, `0.5 · two modifiers`) into one exact
/// set: branch `i` contributes `branch_i` of the mass, spread over its own results by weight.
///
/// Branch `i`'s results are multiplied by `branch_i × Π_{j≠i} total_j`, so every branch shares one
/// denominator and the total is `Π_j total_j × Σ branch_j`. `None` if that would overflow.
pub fn combine(data: &Data, branches: Vec<(u128, Vec<Weighted>)>) -> Option<ItemOutputSet> {
    let totals: Vec<u128> = branches
        .iter()
        .map(|(_, results)| results.iter().map(|&(_, w, _)| w).sum())
        .collect();
    let mut weighted = Vec::new();
    for (i, (share, results)) in branches.into_iter().enumerate() {
        let mut scale = share;
        for (j, total) in totals.iter().enumerate() {
            if j != i {
                scale = scale.checked_mul(*total)?;
            }
        }
        for (item, weight, added) in results {
            weighted.push((item, weight.checked_mul(scale)?, added));
        }
    }
    Some(ItemOutputSet::new(data, weighted))
}

/// The pools one random add reaches, one entry per side that can take a modifier, with the exact
/// integer weights of the modifiers on it.
fn side_pools(data: &Data, item: &Item, want: SideReq) -> Vec<(Side, Vec<(u32, u128)>)> {
    let mut by_side: [Vec<(u32, u128)>; 2] = Default::default();
    for (m, w) in apply::draw_weights(data, item, &Pool::Natural, want, false, false) {
        if let Some(side) = data.mod_at(m).side {
            by_side[side.index()].push((m, w));
        }
    }
    Side::BOTH.into_iter().zip(by_side).collect()
}

/// One random add, exactly: the side is drawn first, in equal shares among the sides that can take a
/// modifier, and then the modifier on it by weight (claim `add.side-first`). Weights are scaled by
/// the *other* sides' totals so every side shares one denominator, making the sum `k × Π totals`.
pub fn one_add(data: &Data, item: &Item, want: SideReq) -> Vec<Weighted> {
    let pools: Vec<(Side, Vec<(u32, u128)>)> = side_pools(data, item, want)
        .into_iter()
        .filter(|(_, mods)| !mods.is_empty())
        .collect();
    let totals: Vec<u128> = pools
        .iter()
        .map(|(_, mods)| mods.iter().map(|&(_, w)| w).sum())
        .collect();
    let mut out = Vec::new();
    for (i, (side, mods)) in pools.iter().enumerate() {
        let scale = totals
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, t)| *t)
            .product::<u128>()
            .max(1);
        for &(m, w) in mods {
            let mut it = item.clone();
            it.affixes_mut(*side).push(Affix::new(m));
            out.push((it, w * scale, Some((*side, m))));
        }
    }
    out
}

/// The exact share of one side's next draw that `want` would take: the weights of the modifiers it
/// accepts over the side's whole pool. `None` when that side has nothing to draw.
///
/// This is the number a report wants ("a 0.63 % chance the added prefix is the totem modifier"),
/// and it is exact: the pool's weights are integers and the share is their ratio.
pub fn side_chance(
    data: &Data,
    item: &Item,
    side: Side,
    mut want: impl FnMut(u32, &ModData) -> bool,
) -> Option<Fraction> {
    let pool = side_pools(data, item, SideReq::Any)
        .into_iter()
        .find(|(s, _)| *s == side)?
        .1;
    let total: u128 = pool.iter().map(|&(_, w)| w).sum();
    let kept: u128 = pool
        .iter()
        .filter(|&&(m, _)| want(m, data.mod_at(m)))
        .map(|&(_, w)| w)
        .sum();
    Fraction::new(kept, total)
}

/// `n` random adds in a row, as one exact set; `None` if a weight would overflow.
pub fn adds(data: &Data, item: &Item, n: usize, want: SideReq) -> Option<Vec<Weighted>> {
    let mut acc = vec![(item.clone(), 1u128, None)];
    for _ in 0..n {
        let mut next = Vec::new();
        for (it, w, first) in acc {
            for (result, w2, added) in one_add(data, &it, want) {
                next.push((result, w.checked_mul(w2)?, first.or(added)));
            }
        }
        acc = next;
    }
    Some(acc)
}

/// The item with its rarity changed, leaving the affixes alone.
pub(crate) fn with_rarity(item: &Item, rarity: Rarity) -> Item {
    let mut out = item.clone();
    out.rarity = rarity;
    out
}

/// Whether a side can still take a modifier, by the same room count every other step uses.
pub(crate) fn has_room(data: &Data, item: &Item, side: Side) -> bool {
    apply::room(data, item, side) > 0
}
