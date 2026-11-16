//! The eight crafting orbs, one file each, behind one trait.
//!
//! An orb answers one question: given this item, what can it become, and how often? The answer is
//! an [`ItemOutputSet`] — exact integer weights over one denominator — or a refusal that names why
//! the orb cannot be used. Nothing here rolls dice; the state machines
//! ([`crate::craft`]) walk the sets.
//!
//! [`registry`] names exactly these eight ids and nothing else:
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
//! Corruption and mirroring block all eight: an item either flagged is refused by every orb, with
//! the reason naming the orb's own verb, because none of them may modify an item the game has
//! locked. Each orb checks the two flags itself, in the same two guards, before it looks at the
//! rarity.

use std::collections::HashMap;

pub mod alteration;
pub mod annulment;
pub mod augmentation;
pub mod divine;
pub mod exalted;
pub mod regal;
pub mod scouring;
pub mod transmutation;

pub use crate::pool::{Mass, Weighted};
pub use alteration::Alteration;
pub use annulment::Annulment;
pub use augmentation::Augmentation;
pub use divine::Divine;
pub use exalted::Exalted;
pub use regal::Regal;
pub use scouring::Scouring;
pub use transmutation::Transmutation;

use crate::data::Data;
use crate::filter::ItemSetFilter;
use crate::fraction::Fraction;
use crate::item::Item;

/// One modelled orb, named by the id the game's rules use (`currency/alteration`).
pub trait Currency {
    /// The id a policy names this orb by.
    fn id(&self) -> &'static str;

    /// What one use of this orb can produce.
    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet;
}

/// One way an orb can leave the item, with the weight that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub item: Item,
    /// The outcome's weight over [`ItemOutputSet::total`].
    pub weight: u128,
    /// The modifier this step added, for the first draw of a step that adds one.
    pub added: Option<(crate::item::Side, u16)>,
}

/// Everything one use of an orb can produce.
///
/// `Outcomes` never has fewer than one entry and its weights always sum to `total`; a set that
/// cannot hold its own chances exactly is a [`ItemOutputSet::Refused`] instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemOutputSet {
    /// The orb cannot be used on this item, for the reason given.
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

    /// Folds outcomes that produced the same item and computes the total.
    ///
    /// An empty input is a refusal: a step that produced nothing is not a step. So is a total that
    /// does not fit in 128 bits — an inexact set is worse than no set.
    pub fn new(weighted: Vec<Weighted>) -> ItemOutputSet {
        if weighted.is_empty() {
            return ItemOutputSet::refused("the step produced no outcome");
        }
        let mut folded: Vec<Outcome> = Vec::with_capacity(weighted.len());
        let mut index: HashMap<Item, usize> = HashMap::new();
        for (item, weight, added) in weighted {
            match index.get(&item) {
                Some(&at) => {
                    let Some(sum) = folded[at].weight.checked_add(weight) else {
                        return ItemOutputSet::refused("the weights do not fit in 128 bits");
                    };
                    folded[at].weight = sum;
                }
                None => {
                    index.insert(item.clone(), folded.len());
                    folded.push(Outcome {
                        item,
                        weight,
                        added,
                    });
                }
            }
        }
        let Some(total) = folded
            .iter()
            .try_fold(0u128, |sum, outcome| sum.checked_add(outcome.weight))
        else {
            return ItemOutputSet::refused("the weights do not fit in 128 bits");
        };
        ItemOutputSet::Outcomes {
            outcomes: folded,
            total,
        }
    }

    /// The outcomes, empty for a refusal.
    pub fn outcomes(&self) -> &[Outcome] {
        match self {
            ItemOutputSet::Refused(_) => &[],
            ItemOutputSet::Outcomes { outcomes, .. } => outcomes,
        }
    }

    /// The number every weight is over; zero for a refusal.
    pub fn total(&self) -> u128 {
        match self {
            ItemOutputSet::Refused(_) => 0,
            ItemOutputSet::Outcomes { total, .. } => *total,
        }
    }

    /// Whether the weights add up to the total they claim. Every set this crate builds does; the
    /// tests ask.
    pub fn exact(&self) -> bool {
        match self {
            ItemOutputSet::Refused(_) => true,
            ItemOutputSet::Outcomes { outcomes, total } => {
                outcomes
                    .iter()
                    .try_fold(0u128, |sum, outcome| sum.checked_add(outcome.weight))
                    == Some(*total)
            }
        }
    }

    /// The exact chance that the step produces an item `want` accepts. `None` for a refusal.
    pub fn chance(&self, mut want: impl FnMut(&Item) -> bool) -> Option<Fraction> {
        match self {
            ItemOutputSet::Refused(_) => None,
            ItemOutputSet::Outcomes { outcomes, total } => {
                let kept = outcomes
                    .iter()
                    .filter(|outcome| want(&outcome.item))
                    .try_fold(0u128, |sum, outcome| sum.checked_add(outcome.weight))?;
                Fraction::new(kept, *total)
            }
        }
    }

    /// The outcomes the filter accepts, over their own total.
    ///
    /// The rejected mass is **dropped, not reported**, so a chance read off the result is
    /// conditional on the filter: "given the filter took this step, this outcome's share". Read
    /// [`Self::chance`] off the unfiltered set to learn how much the filter threw away.
    pub fn filtered(&self, filter: &ItemSetFilter) -> ItemOutputSet {
        match self {
            ItemOutputSet::Refused(why) => ItemOutputSet::refused(why),
            ItemOutputSet::Outcomes { outcomes, .. } => {
                let kept: Vec<Weighted> = outcomes
                    .iter()
                    .filter(|outcome| filter.accepts(&outcome.item))
                    .map(|outcome| (outcome.item.clone(), outcome.weight, outcome.added))
                    .collect();
                if kept.is_empty() {
                    return ItemOutputSet::refused("no outcome meets the filter");
                }
                ItemOutputSet::new(kept)
            }
        }
    }
}

/// One of the two equal branches of a magic reroll: half the time one modifier, half the time two.
pub(crate) fn half() -> Fraction {
    Fraction::new(1, 2).expect("1/2 is a fraction")
}

/// Puts exact chances over one denominator, so a set can hold them as integer weights without
/// rounding: the denominator is the least common multiple of the chances' own, and each weight is
/// its chance multiplied out to it.
///
/// `None` when the common denominator, a weight or the total would not fit in 128 bits — a caller
/// that cannot hold the answer exactly must refuse rather than round.
pub fn weigh(mass: Vec<Mass>) -> Option<Vec<Weighted>> {
    let mut common = 1u128;
    for &(_, chance, _) in &mass {
        common = lcm(common, chance.den())?;
    }
    let mut out = Vec::with_capacity(mass.len());
    for (item, chance, added) in mass {
        out.push((
            item,
            chance.num().checked_mul(common / chance.den())?,
            added,
        ));
    }
    out.iter()
        .try_fold(0u128, |total, &(_, weight, _)| total.checked_add(weight))?;
    Some(out)
}

/// Combines the branches of one step — `½ · one modifier`, `½ · two modifiers` — into one exact set.
///
/// A branch's results do not share a denominator with the other branch's, because each draw inside
/// a branch was normalised against its own state. Multiplying each exact chance by the exact share
/// and putting the sum over one denominator keeps the combination as exact as the branches.
pub fn combine(branches: Vec<(Fraction, Vec<Mass>)>) -> Option<ItemOutputSet> {
    let mut mass = Vec::new();
    for (share, results) in branches {
        for (item, chance, added) in results {
            mass.push((item, chance.checked_mul(share)?, added));
        }
    }
    Some(ItemOutputSet::new(weigh(mass)?))
}

/// The orb a policy's id names, or `None` if this crate does not model it.
pub fn registry<'d>(data: &'d Data, id: &str) -> Option<Box<dyn Currency + 'd>> {
    match id {
        "currency/transmutation" => Some(Box::new(Transmutation::new(data))),
        "currency/alteration" => Some(Box::new(Alteration::new(data))),
        "currency/augmentation" => Some(Box::new(Augmentation::new(data))),
        "currency/regal" => Some(Box::new(Regal::new(data))),
        "currency/exalted" => Some(Box::new(Exalted::new(data))),
        "currency/scouring" => Some(Box::new(Scouring::new(data))),
        "currency/annulment" => Some(Box::new(Annulment::new(data))),
        "currency/divine" => Some(Box::new(Divine::new(data))),
        _ => None,
    }
}

fn lcm(a: u128, b: u128) -> Option<u128> {
    (a / gcd(a, b)).checked_mul(b)
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a.max(1)
}
