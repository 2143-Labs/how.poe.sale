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
mod eldritch;
mod essence;
mod exalted;
mod fracture;
mod regal;
mod scouring;
mod transmutation;

pub use alteration::Alteration;
pub use annulment::Annulment;
pub use augmentation::Augmentation;
pub use eldritch::Eldritch;
pub use essence::Essence;
pub use exalted::Exalted;
pub use filter::{AbstractMod, Accept, ItemSetFilter, ModSelector, Tier};
pub use fraction::Fraction;
pub use fracture::Fracture;
pub use regal::Regal;
pub use scouring::Scouring;
pub use transmutation::Transmutation;

use std::collections::BTreeMap;

use crate::apply;
use crate::data::{Data, ModData, Rarity, Side};
use crate::item::{Affix, Item};
use crate::method::{Policy, Pool, Protection, SideReq};

/// What every removal method in `rules/methods.json` declares: a lock is respected, and since 3.30
/// no removal respects the Cannot Roll metamods (`remove.cannot-roll-scope`). Scouring,
/// alteration and annulment all clear or take modifiers, so they share this and the
/// `apply::clear_changeable`/`apply::removable` predicates that read it.
pub const REMOVAL_PROTECTION: Protection = Protection {
    locks: Policy::Respect,
    cannot_roll: Policy::Ignore,
};

/// A currency, in one struct and one file.
pub trait Currency {
    /// The method id the rules use, e.g. `currency/alteration`. Borrowed from `self` rather than
    /// `'static` because an essence is parameterised by which essence it is, and its id
    /// (`essence/<name>`) is the one the generator declares for that essence.
    fn id(&self) -> &str;

    /// One step: every item this currency can produce from `item`, with exact chances, after the
    /// optional filter has been applied to the set.
    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet;
}

/// The currency ids this crate models, in the order the craft loop and the page list them.
///
/// Every one has an arm in [`modelled`]. Essences and eldritch embers are parameterised by a
/// sub-kind and fracture is a service rather than a currency, so none of them is registered: the
/// machine could not apply them from an id alone.
pub const MODELLED: [&str; 7] = [
    "currency/transmutation",
    "currency/alteration",
    "currency/augmentation",
    "currency/regal",
    "currency/exalted",
    "currency/scouring",
    "currency/annulment",
];

/// The registry: a modelled id resolved to the currency that id names.
///
/// This is the single place a currency is built, so a policy decides and the machine applies.
/// Adding a currency is one arm here plus its id — nothing in `craft/`'s vocabulary changes, and a
/// policy can name the new id the moment it is registered.
///
/// The lookup is a `match` on the id string, so the same id cannot be registered twice: a second
/// arm for an id is an unreachable pattern, which the workspace's clippy denies. That is a
/// property of the design, not something a test can add to the match to break.
pub fn modelled<'d>(data: &'d Data, id: &str) -> Option<Box<dyn Currency + 'd>> {
    Some(match id {
        "currency/transmutation" => Box::new(Transmutation::new(data)),
        "currency/alteration" => Box::new(Alteration::new(data)),
        "currency/augmentation" => Box::new(Augmentation::new(data)),
        "currency/regal" => Box::new(Regal::new(data)),
        "currency/exalted" => Box::new(Exalted::new(data)),
        "currency/scouring" => Box::new(Scouring::new(data)),
        "currency/annulment" => Box::new(Annulment::new(data)),
        _ => return None,
    })
}

/// A step's result before it is folded into a set: the item, its weight, and the modifier the step
/// added if it added one.
pub type Weighted = (Item, u128, Option<(Side, u32)>);

/// One result of a chain of draws: the item, its exact chance, and the first modifier the chain
/// added. [`Weighted`] is the same thing multiplied out to one shared denominator by [`weigh`].
pub type Mass = (Item, Fraction, Option<(Side, u32)>);

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
    /// `before` is the item the step was applied to: a result is settled against it, which drops
    /// the display-only figures and the foresight the step invalidated (`apply::settle`, the rule
    /// the op path already uses). A step that produced nothing is refused rather than returned as
    /// a zero-total set — a set is a distribution, and an empty one silently loses the item's mass.
    ///
    /// A set is not a place to decide "nothing changed": a reroll's one valid outcome can be the
    /// item it started from (an essence on an item already holding that essence's modifier), and
    /// that is a use of the currency. The two cases `empty.refused` cares about are decided where
    /// the step can see them: a currency whose first draw has nothing to take comes back from
    /// [`adds`] empty, and an unconditional add with an empty pool is refused by its own currency.
    ///
    /// The fold is checked rather than wrapped. In practice it cannot overflow: the weights arrive
    /// from [`weigh`] (which checks every weight and their total) or from [`one_add`], whose
    /// per-side products are pool weights summed over a handful of modifiers and sit far below
    /// `u128::MAX`. This function is still the module's one gate on "can the answer be held
    /// exactly", so a caller that one day hands it an overflowing vector is refused here instead
    /// of silently saturating.
    pub fn new(data: &Data, before: &Item, weighted: Vec<Weighted>) -> ItemOutputSet {
        if weighted.is_empty() {
            return ItemOutputSet::refused("the step produced no outcome");
        }
        let mut folded: BTreeMap<Item, (u128, Option<(Side, u32)>)> = BTreeMap::new();
        for (mut item, weight, added) in weighted {
            // The engine's one form: a fixed roll is filled in, so two results that are the same
            // item fold together and compare equal to what the op-declared path produces.
            item.canonical(data);
            // Display figures are a function of the rest of the item, and foresight is lost by any
            // modification: both are settled here, once, for every currency.
            crate::apply::settle(before, &mut item, false);
            let slot = folded.entry(item).or_insert((0, added));
            let Some(weight) = slot.0.checked_add(weight) else {
                return ItemOutputSet::refused("the step's weights do not fit in 128 bits");
            };
            slot.0 = weight;
        }
        let mut total = 0u128;
        for &(weight, _) in folded.values() {
            let Some(sum) = total.checked_add(weight) else {
                return ItemOutputSet::refused("the step's weights do not fit in 128 bits");
            };
            total = sum;
        }
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
    /// The sum is checked: a set whose weights overflow `u128` is not exact, and must not be
    /// mistaken for one whose sum merely wrapped onto the total.
    pub fn exact(&self) -> bool {
        match self {
            ItemOutputSet::Refused(_) => true,
            ItemOutputSet::Outcomes { outcomes, total } => {
                *total > 0
                    && outcomes
                        .iter()
                        .try_fold(0u128, |sum, o| sum.checked_add(o.weight))
                        == Some(*total)
            }
        }
    }

    /// The exact share of the step whose result satisfies `want`; `None` when refused or when the
    /// kept weights do not fit in `u128` (a chance that cannot be held exactly is not reported).
    pub fn chance(&self, mut want: impl FnMut(&Item) -> bool) -> Option<Fraction> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let kept = self
            .outcomes()
            .iter()
            .filter(|o| want(&o.item))
            .try_fold(0u128, |sum, o| sum.checked_add(o.weight))?;
        Fraction::new(kept, total)
    }

    /// "Mod filtering a crafting step": keeps the outcomes the filter accepts, with the chances
    /// renormalised exactly over what is left.
    ///
    /// The rejected outcomes' mass is **dropped, not reported**: the kept outcomes keep their
    /// original weights and the set's total becomes their sum, so a chance read off the filtered
    /// set is conditional on the filter — it says "given the filter accepted this step, this
    /// outcome's share", not "this outcome's share of the whole step". Call
    /// [`Self::chance`] on the *unfiltered* set to learn how much the filter threw away. A
    /// filter that accepts nothing yields a refusal rather than an empty distribution.
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
                let Some(total) = kept
                    .iter()
                    .try_fold(0u128, |sum, o| sum.checked_add(o.weight))
                else {
                    return ItemOutputSet::refused("the filtered weights do not fit in 128 bits");
                };
                ItemOutputSet::Outcomes {
                    outcomes: kept,
                    total,
                }
            }
        }
    }
}

/// Combines the branches of one step (`½ · one modifier`, `½ · two modifiers`) into one exact
/// set: branch `i` contributes `share_i` of the mass, spread over its own results by chance.
///
/// The shares are fractions, not integers, because each branch's chances already are: a chain of
/// two draws normalises every result against *its own* state's total, so a branch's results do not
/// share one denominator that an integer scale could multiply through. Multiplying the exact
/// chances by the exact share and putting the sum over one denominator keeps the combination as
/// exact as the branches. `None` if a chance, the common denominator, or the total would overflow.
pub fn combine(
    data: &Data,
    before: &Item,
    branches: Vec<(Fraction, Vec<Mass>)>,
) -> Option<ItemOutputSet> {
    let mut mass = Vec::new();
    for (share, results) in branches {
        for (item, chance, added) in results {
            mass.push((item, chance.checked_mul(share)?, added));
        }
    }
    Some(ItemOutputSet::new(data, before, weigh(mass)?))
}

/// One of the two equal branches of a magic-item reroll (`magic.mod-count`): half the time one
/// modifier, half the time two (`currency/transmutation`, `currency/alteration`).
fn half() -> Fraction {
    Fraction::new(1, 2).expect("1/2 is a fraction")
}

/// Puts exact chances over one shared denominator, so a set can hold them as integer weights
/// without rounding: the denominator is the least common multiple of the chances' own, and each
/// weight is its chance multiplied out to it.
///
/// `None` when the common denominator, a weight, or the weights' total would overflow `u128`. A
/// caller that cannot hold the answer exactly must refuse rather than return a rounded
/// distribution, which is why this is checked all the way through.
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
    // The total is the number every report divides by, so it has to fit as well as the weights.
    out.iter()
        .try_fold(0u128, |total, &(_, weight, _)| total.checked_add(weight))?;
    Some(out)
}

/// The least common multiple, or `None` when it does not fit.
fn lcm(a: u128, b: u128) -> Option<u128> {
    (a / gcd(a, b)).checked_mul(b)
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a
}

/// The pools one random add reaches, one entry per side that can take a modifier, with the exact
/// integer weights of the modifiers on it.
///
/// `pool` is the pool the calling method declares — an influence slam draws its influence's pool,
/// not the natural one — so the caller must pass it rather than relying on a default.
fn side_pools(
    data: &Data,
    item: &Item,
    pool: &Pool,
    want: SideReq,
) -> Vec<(Side, Vec<(u32, u128)>)> {
    let mut by_side: [Vec<(u32, u128)>; 2] = Default::default();
    for (m, w) in apply::draw_weights(data, item, pool, want, false, false) {
        if let Some(side) = data.mod_at(m).side {
            by_side[side.index()].push((m, w));
        }
    }
    Side::BOTH.into_iter().zip(by_side).collect()
}

/// One random add, exactly: the side is drawn first, in equal shares among the sides that can take a
/// modifier, and then the modifier on it by weight (claim `add.side-first`). Weights are scaled by
/// the *other* sides' totals so every side shares one denominator, making the sum `k × Π totals`.
///
/// `None` when a side's total or a scaled weight would overflow `u128`: a distribution that
/// cannot be held exactly is refused rather than wrapped. In practice the totals are pool weights
/// summed over a handful of modifiers (real weights are ~1e5–1e8), so this is a guard, not a
/// limit the data reaches.
pub fn one_add(data: &Data, item: &Item, pool: &Pool, want: SideReq) -> Option<Vec<Weighted>> {
    let pools: Vec<(Side, Vec<(u32, u128)>)> = side_pools(data, item, pool, want)
        .into_iter()
        .filter(|(_, mods)| !mods.is_empty())
        .collect();
    let totals: Vec<u128> = pools
        .iter()
        .map(|(_, mods)| {
            mods.iter()
                .try_fold(0u128, |sum, &(_, w)| sum.checked_add(w))
        })
        .collect::<Option<_>>()?;
    let mut out = Vec::new();
    for (i, (side, mods)) in pools.iter().enumerate() {
        let scale = totals
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .try_fold(1u128, |acc, (_, t)| acc.checked_mul(*t))?
            .max(1);
        for &(m, w) in mods {
            let mut it = item.clone();
            it.affixes_mut(*side).push(Affix::new(m));
            out.push((it, w.checked_mul(scale)?, Some((*side, m))));
        }
    }
    Some(out)
}

/// The exact share of one side's next draw that `want` would take: the weights of the modifiers it
/// accepts over the side's whole pool. `None` when that side has nothing to draw, or when a total
/// would overflow `u128` (the share cannot then be held exactly).
///
/// This is the number a report wants ("a 0.63 % chance the added prefix is the totem modifier"),
/// and it is exact: the pool's weights are integers and the share is their ratio.
pub fn side_chance(
    data: &Data,
    item: &Item,
    pool: &Pool,
    side: Side,
    mut want: impl FnMut(u32, &ModData) -> bool,
) -> Option<Fraction> {
    let pool = side_pools(data, item, pool, SideReq::Any)
        .into_iter()
        .find(|(s, _)| *s == side)?
        .1;
    let total = pool
        .iter()
        .try_fold(0u128, |sum, &(_, w)| sum.checked_add(w))?;
    let kept = pool
        .iter()
        .filter(|&&(m, _)| want(m, data.mod_at(m)))
        .try_fold(0u128, |sum, &(_, w)| sum.checked_add(w))?;
    Fraction::new(kept, total)
}

/// `n` random adds in a row, as exact chances. `None` if a chance would overflow.
///
/// Each draw is normalised against the state it is drawn on: a result of that draw keeps
/// `weight / that state's total`, because the next draw's pool depends on what was just added, so
/// different states have different totals. Multiplying raw weights across draws is only correct
/// when every first-draw result leaves the same second-draw total, which is not true in general.
///
/// The first draw must happen: a step with nothing to take it has drawn no modifier, which is a
/// refusal (claim `empty.refused`) — an empty vector, which [`combine`] turns into a refusal.
///
/// A *later* draw with nothing to take leaves the item as it is and the draws after it still run:
/// that is what the op path does inside a `chance` branch (`currency/transmutation`,
/// `currency/alteration`), where a dead end is a conditional no-op and not a lost path — an
/// alteration on a magic item whose free side has no room keeps what the first draw gave it.
///
/// The reported modifier is the *first* one added; the later ones are the draws this chain exists
/// to count.
pub fn adds(data: &Data, item: &Item, pool: &Pool, n: usize, want: SideReq) -> Option<Vec<Mass>> {
    let mut acc: Vec<Mass> = vec![(item.clone(), Fraction::ONE, None)];
    for draw in 0..n {
        let mut next: Vec<Mass> = Vec::new();
        for (state, chance, first) in acc {
            let branches = one_add(data, &state, pool, want)?;
            if branches.is_empty() {
                if draw == 0 {
                    // Nothing at all can be drawn, so the currency has produced no modifier.
                    return Some(Vec::new());
                }
                next.push((state, chance, first));
                continue;
            }
            let total = branches
                .iter()
                .try_fold(0u128, |sum, &(_, weight, _)| sum.checked_add(weight))?;
            for (result, weight, added) in branches {
                let share = Fraction::new(weight, total)?;
                next.push((result, chance.checked_mul(share)?, first.or(added)));
            }
        }
        acc = next;
    }
    Some(acc)
}

/// One add that cannot be skipped, for the currencies whose `addRandom` is not inside a `chance`
/// branch (`currency/augmentation`, `-exalted`, `-regal`): an empty pool there refuses the method
/// rather than adding nothing, so an item with room but nothing to draw yields `None` and the
/// caller refuses with the reason.
fn add_required(data: &Data, item: &Item, pool: &Pool) -> Option<Vec<Weighted>> {
    let drawn = one_add(data, item, pool, SideReq::Any)?;
    (!drawn.is_empty()).then_some(drawn)
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
