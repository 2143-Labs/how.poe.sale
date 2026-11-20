//! Shaper's Exalted Orb: a rare item with no influence gains the Shaper influence and one modifier
//! drawn from the Shaper's own modifiers (`currency/shapers_exalted`).
//!
//! The orb's pool is not the item's. It is the modifiers whose first spawn-weight entry the
//! influenced item matches is the Shaper's tag: on this shield one prefix (`MaximumTotemUber1`, the
//! totem) and ten suffixes. The side is drawn first, in equal shares among the sides with room that
//! have a Shaper modifier left, as in every other add here ([`crate::pool::one_add`]), and the
//! modifier then by weight. With the suffixes full the only outcome is the totem.
//!
//! On an item the orb reads the data. On an abstract item it cannot: a [`Law`] is the pool of the
//! item it was built from, and a [`Node`] records no influence. So the orb carries what it adds to
//! an abstract item — each side's Shaper modifiers by grade — built once from the start item a law
//! is built from ([`ShapersExalted::new`]). That is exact because no Shaper modifier shares a
//! family with the start item's own pool, so what the orb adds does not depend on what the item
//! holds; `new` checks it.
//!
//! What the abstract form cannot do is refuse an item that already has an influence: a node does
//! not say. A craft that uses it on nodes must hand it items drawn from a pool with no influence,
//! and draw nothing after it, because no law here knows the pool of an item whose influence changed
//! partway.
//!
//! Fractured, synthesised and eldritch items, which the game also refuses, are not modelled.

use super::Applied;
use super::{Currency, Named};
use crate::classes::{Class, Classes, Grade};
use crate::cost::{Cost, CurrencyId};
use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::fraction::Fraction;
use crate::item::{Affix, Influence, Item, Rarity, Side};
use crate::law::Law;
use crate::node::Node;
use crate::pool::draw_weights;
use crate::stock::{Dist, Lock, Refusal, Stock};

/// The grades, in the order a law lists them.
const GRADES: [Tier; 3] = [Tier::Desired, Tier::Good, Tier::Any];

const OVERFLOW: Refusal = Refusal("the chances do not fit in 128 bits");

/// The Shaper's Exalted Orb, one step.
#[derive(Clone, Debug)]
pub struct ShapersExalted<G: Grade = Tier> {
    /// What the orb adds to an abstract item: per side, each grade with its share of the side's
    /// Shaper weight, in the order the law lists the grades. Empty for a side with no Shaper
    /// modifier.
    grades: [Vec<(G, f64)>; 2],
}

impl ShapersExalted {
    /// The orb for items of the start item's base and item level, graded by this filter.
    ///
    /// Refused when the start item already has an influence, or when a Shaper modifier shares a
    /// family with the start item's own pool — then what the orb adds would depend on what the
    /// item holds, which an abstract item does not say.
    pub fn new(
        data: &Data,
        filter: &ItemSetFilter,
        start: &Item,
    ) -> Result<ShapersExalted, Refusal> {
        build(data, &GRADES, &|m| filter.tier_of(m), start)
    }
}

impl ShapersExalted<Class> {
    /// [`ShapersExalted::new`] with each modifier graded by its class in a run.
    pub fn classed(
        data: &Data,
        classes: &Classes,
        start: &Item,
    ) -> Result<ShapersExalted<Class>, Refusal> {
        build(data, &classes.all(), &|m| classes.of(m), start)
    }
}

/// The orb for a start item, with every Shaper modifier graded by `grade`; `order` lists every
/// grade in the order a law lists them.
fn build<G: Grade>(
    data: &Data,
    order: &[G],
    grade: &dyn Fn(u16) -> G,
    start: &Item,
) -> Result<ShapersExalted<G>, Refusal> {
    if start.influence.is_some() {
        return Err(Refusal(
            "a Shaper's Exalted Orb needs an item with no influence",
        ));
    }
    let rare = start.with_rarity(Rarity::Rare);
    let own: Vec<u16> = Side::BOTH
        .into_iter()
        .flat_map(|side| draw_weights(data, &rare, side))
        .flat_map(|(m, _)| data.mod_at(m).groups.iter().copied())
        .collect();
    let pools = Side::BOTH.map(|side| shaper_pool(data, &rare, side));
    if pools.iter().flatten().any(|&(m, _)| {
        data.mod_at(m)
            .groups
            .iter()
            .any(|group| own.contains(group))
    }) {
        return Err(Refusal(
            "a Shaper modifier shares a family with the item's own pool",
        ));
    }
    let grades: [Vec<(G, f64)>; 2] = pools.map(|pool| {
        let total: u128 = pool.iter().map(|&(_, weight)| weight).sum();
        order
            .iter()
            .filter_map(|&class| {
                let weight: u128 = pool
                    .iter()
                    .filter(|&&(m, _)| grade(m) == class)
                    .map(|&(_, weight)| weight)
                    .sum();
                (weight > 0).then_some((class, weight as f64 / total as f64))
            })
            .collect()
    });
    Ok(ShapersExalted { grades })
}

impl<G: Grade> Named for ShapersExalted<G> {
    fn id(&self) -> CurrencyId {
        CurrencyId::SHAPERS_EXALTED
    }

    fn works_on(&self) -> &'static [Rarity] {
        &[Rarity::Rare]
    }
}

impl<G: Grade> Currency<Item> for ShapersExalted<G> {
    fn apply(&self, rules: &Data, state: &Item) -> Result<Applied<Item>, Refusal> {
        usable(state)?;
        if state.influence.is_some() {
            return Err(Refusal(
                "a Shaper's Exalted Orb needs an item with no influence",
            ));
        }
        if !Side::BOTH
            .into_iter()
            .any(|side| has_room(rules, state, side))
        {
            return Err(Refusal("the item has no room for another modifier"));
        }
        let sides: Vec<(Side, Vec<(u16, u128)>)> = Side::BOTH
            .into_iter()
            .map(|side| (side, shaper_pool(rules, state, side)))
            .filter(|(_, pool)| !pool.is_empty())
            .collect();
        if sides.is_empty() {
            return Err(Refusal("no Shaper modifier can roll on this item"));
        }
        let shaped = state.with_influence(Influence::Shaper);
        let mut parts = Vec::new();
        for (side, pool) in &sides {
            let total = pool
                .iter()
                .try_fold(0u128, |sum, &(_, weight)| sum.checked_add(weight))
                .ok_or(OVERFLOW)?;
            let whole = total.checked_mul(sides.len() as u128).ok_or(OVERFLOW)?;
            for &(m, weight) in pool {
                let mut result = shaped.clone();
                result.affixes_mut(*side).push(Affix::at_range(rules, m));
                parts.push((result, Fraction::new(weight, whole).ok_or(OVERFLOW)?));
            }
        }
        Ok(Applied {
            outcomes: Dist::new(parts)?,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}

/// The abstract item: the side in equal shares among the sides with room that have a Shaper
/// modifier, then the grade at the share [`ShapersExalted::new`] found. The law is not read: the
/// orb does not draw from the item's pool.
impl<G: Grade> Currency<Node<G>> for ShapersExalted<G> {
    fn apply(&self, rules: &Law<G>, state: &Node<G>) -> Result<Applied<Node<G>>, Refusal> {
        usable(state)?;
        if !Side::BOTH
            .into_iter()
            .any(|side| has_room(rules, state, side))
        {
            return Err(Refusal("the item has no room for another modifier"));
        }
        let open: Vec<Side> = Side::BOTH
            .into_iter()
            .filter(|&side| has_room(rules, state, side) && !self.grades[side.index()].is_empty())
            .collect();
        if open.is_empty() {
            return Err(Refusal("no Shaper modifier can roll on this item"));
        }
        let share = 1.0 / open.len() as f64;
        let parts: Vec<(Node<G>, f64)> = open
            .iter()
            .flat_map(|&side| {
                self.grades[side.index()]
                    .iter()
                    .map(move |&(tier, chance)| (state.with(side, tier), chance * share))
            })
            .collect();
        Ok(Applied {
            outcomes: Dist::new(parts)?,
            cost: Cost::of(self.id(), 1.0),
        })
    }
}

/// The refusals an item and an abstract item share: the lock, then the rarity.
fn usable<S: Stock>(state: &S) -> Result<(), Refusal> {
    if state.lock() == Some(Lock::Corrupted) {
        return Err(Refusal("a corrupted item cannot be exalted"));
    }
    if state.lock() == Some(Lock::Mirrored) {
        return Err(Refusal("a mirrored item cannot be exalted"));
    }
    if state.rarity() != Rarity::Rare {
        return Err(Refusal("a Shaper's Exalted Orb needs a rare item"));
    }
    Ok(())
}

fn has_room<S: Stock>(rules: &S::Rules, state: &S, side: Side) -> bool {
    state.cap(rules) > state.held(side)
}

/// The Shaper modifiers one side of this item could gain: the side's pool once the item has the
/// Shaper influence, kept to the modifiers whose first spawn-weight entry the item matches is the
/// Shaper's tag. Empty when the data has no Shaper tag.
fn shaper_pool(data: &Data, item: &Item, side: Side) -> Vec<(u16, u128)> {
    let Some(tag) = data.influence_tag(Influence::Shaper) else {
        return Vec::new();
    };
    let base = &data.base_at(item.base).tags;
    draw_weights(data, &item.with_influence(Influence::Shaper), side)
        .into_iter()
        .filter(|&(m, _)| {
            data.mod_at(m)
                .weights
                .iter()
                .find(|(on, _)| base.contains(on) || *on == tag)
                .is_some_and(|&(on, _)| on == tag)
        })
        .collect()
}
