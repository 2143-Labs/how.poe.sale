//! The abstract item: a rarity and, per side, the grades of the modifiers it holds, in the order
//! they were drawn.
//!
//! A [`Node`] is what is left of an item once the filter has graded it: it no longer says which
//! modifiers it holds, only what each is worth to the goal. That is one way only — an item becomes
//! a node ([`Node::of`], [`grade`]) and nothing here turns a node back into an item.
//!
//! The order is kept because it matters: holding a modifier removes its whole family from later
//! draws, so which grade comes next on a side depends on the grades already drawn there, and in
//! which order (see [`crate::law`]).
//!
//! A modifier a currency puts on directly — an essence's own — is not a draw. It takes a slot and
//! counts toward the goal, but the law's history on its side starts after it, so a side lists its
//! placed grades first and remembers how many there are ([`Node::with_placed`], [`Node::drawn`]).

use crate::classes::Grade;
use crate::filter::{ItemSetFilter, Tier};
use crate::fraction::Fraction;
use crate::item::{Item, Rarity, Side};
use crate::law::Law;
use crate::stock::{Dist, Lock, Refusal, Stock};
use crate::target::Target;

///
/// The grade is a filter's [`Tier`] unless the node belongs to a run of several filters, where it is
/// the modifier's [`crate::classes::Class`] and a block reads it through its filter's lens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node<G: Grade = Tier> {
    pub rarity: Rarity,
    /// Each side's grades: the placed ones first, then the drawn ones in draw order.
    sides: [Vec<G>; 2],
    /// How many of each side's grades were placed rather than drawn.
    placed: [u8; 2],
    /// Whether one of the held modifiers was placed by the bench rather than drawn. An item holds
    /// at most one, so a flag is the whole of it; which modifier it is, and its family, are not
    /// recorded.
    crafted: bool,
}

impl<G: Grade> Node<G> {
    /// An item of this rarity holding no modifiers.
    pub fn fresh(rarity: Rarity) -> Node<G> {
        Node {
            rarity,
            sides: [Vec::new(), Vec::new()],
            placed: [0, 0],
            crafted: false,
        }
    }

    /// The side's grades: the placed ones, then the drawn ones in draw order.
    pub fn side(&self, side: Side) -> &[G] {
        &self.sides[side.index()]
    }

    /// The side's drawn grades, in draw order: the history the law reads.
    pub fn drawn(&self, side: Side) -> &[G] {
        &self.sides[side.index()][usize::from(self.placed[side.index()])..]
    }

    /// This node with one more modifier of a grade drawn on a side.
    pub fn with(&self, side: Side, tier: G) -> Node<G> {
        let mut node = self.clone();
        node.sides[side.index()].push(tier);
        node
    }

    /// This node with one more modifier of a grade placed on a side: it takes a slot and counts
    /// toward the goal, and it is not part of the side's history.
    pub fn with_placed(&self, side: Side, tier: G) -> Node<G> {
        let mut node = self.clone();
        let at = side.index();
        node.sides[at].insert(usize::from(node.placed[at]), tier);
        node.placed[at] += 1;
        node
    }

    /// This node with one crafted modifier of a grade on a side: placed, not drawn, so the law's
    /// history on that side does not include it. The flag is what stops a second bench use.
    pub fn with_crafted(&self, side: Side, tier: G) -> Node<G> {
        let mut node = self.with_placed(side, tier);
        node.crafted = true;
        node
    }

    /// Whether the bench placed one of the modifiers held.
    pub fn crafted(&self) -> bool {
        self.crafted
    }

    pub fn modifiers(&self) -> usize {
        self.sides.iter().map(Vec::len).sum()
    }

    /// `rare desired+good / good`: the rarity, then the prefixes and the suffixes in draw order,
    /// `-` for a side that holds nothing. A placed grade is in brackets: `[good]+desired`.
    pub fn label(&self) -> String {
        let side = |side: Side| {
            if self.side(side).is_empty() {
                "-".to_string()
            } else {
                let placed = usize::from(self.placed[side.index()]);
                self.side(side)
                    .iter()
                    .enumerate()
                    .map(|(at, grade)| {
                        if at < placed {
                            format!("[{}]", grade.name())
                        } else {
                            grade.name()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("+")
            }
        };
        format!(
            "{} {} / {}",
            self.rarity.name(),
            side(Side::Prefix),
            side(Side::Suffix)
        )
    }

    /// The node with every grade replaced: what one filter sees in a node graded by class.
    pub fn map<H: Grade>(&self, grade: impl Fn(G) -> H) -> Node<H> {
        Node {
            rarity: self.rarity,
            sides: [
                self.sides[0].iter().map(|&held| grade(held)).collect(),
                self.sides[1].iter().map(|&held| grade(held)).collect(),
            ],
            placed: self.placed,
            crafted: self.crafted,
        }
    }
}

/// What a method reads: the questions a filter's tiers answer.
impl Node<Tier> {
    /// The item, graded. An item's affix order on a side is its draw order, so every modifier it
    /// holds is read as drawn — an essence's own modifier included.
    ///
    /// A node of an item is never crafted: the [`Item`] type does not record a bench modifier.
    pub fn of(filter: &ItemSetFilter, item: &Item) -> Node {
        Node {
            rarity: item.rarity,
            sides: Side::BOTH.map(|side| {
                item.affixes(side)
                    .iter()
                    .map(|affix| filter.tier_of(affix.m))
                    .collect()
            }),
            placed: [0, 0],
            crafted: false,
        }
    }

    /// How many modifiers of each grade the side holds, indexed by [`Tier::rank`].
    pub fn counts(&self, side: Side) -> [u8; 3] {
        let mut counts = [0u8; 3];
        for tier in self.side(side) {
            counts[tier.rank() as usize] += 1;
        }
        counts
    }

    /// Whether a modifier of this grade is held on either side.
    pub fn holds(&self, tier: Tier) -> bool {
        self.sides.iter().flatten().any(|held| *held == tier)
    }

    /// Whether this is the item the craft is for: rare, and both sides clear their floors.
    pub fn reached(&self, target: &Target) -> bool {
        self.rarity == Rarity::Rare && self.meets(target)
    }

    /// Whether this can still become the item the craft is for
    /// ([`Target::side_reachable`] on each side, with the room this rarity gives the target's base).
    pub fn reachable(&self, target: &Target) -> bool {
        let cap = target.caps.at(self.rarity);
        Side::BOTH
            .into_iter()
            .all(|side| target.side_reachable(side, self.counts(side), cap))
    }

    /// Whether both sides clear the target's floors, whatever the rarity: what a finished item is
    /// checked against when a craft's goal is read at its end.
    pub fn meets(&self, target: &Target) -> bool {
        Side::BOTH
            .into_iter()
            .all(|side| target.side_reached(side, self.counts(side)))
    }
}

/// A node as one kind of [`Stock`]: one add is the [`Law`]'s, and its chances are `f64`, because
/// the chance of a grade deep in a rare item has no exact form that fits in 128 bits.
impl<G: Grade> Stock for Node<G> {
    type Rules = Law<G>;
    type Chance = f64;

    fn rarity(&self) -> Rarity {
        self.rarity
    }

    /// A node records no flag: a locked item cannot start a [`Law`].
    fn lock(&self) -> Option<Lock> {
        None
    }

    fn held(&self, side: Side) -> usize {
        self.side(side).len()
    }

    fn cap(&self, rules: &Law<G>) -> usize {
        rules.caps().at(self.rarity)
    }

    fn with_rarity(&self, rarity: Rarity) -> Node<G> {
        Node {
            rarity,
            sides: self.sides.clone(),
            placed: self.placed,
            crafted: self.crafted,
        }
    }

    fn cleared(&self) -> Node<G> {
        Node::fresh(self.rarity)
    }

    fn add(&self, rules: &Law<G>) -> Result<Vec<(Node<G>, f64)>, Refusal> {
        Ok(rules.add(self))
    }
}

/// The filter applied to an output set: every item becomes its abstract form, and items that grade
/// alike are folded into one outcome. One way only.
pub fn grade(
    filter: &ItemSetFilter,
    set: &Dist<Item, Fraction>,
) -> Result<Dist<Node, Fraction>, Refusal> {
    set.relabel(|item| Node::of(filter, item))
}
