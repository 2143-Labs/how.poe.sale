//! The owner's craft loop as a small state machine: normal → transmute → augment / regal → keep or
//! reroll, run over a distribution of items instead of a single one.
//!
//! Three pieces:
//!
//! - [`Stage`] labels one item against the goal filter (`crates/poe1/src/craft/stage.rs`).
//! - [`Policy`] picks the next move for one item, and [`LoopPolicy`] is the owner's loop
//!   (`crates/poe1/src/craft/mod.rs`, below).
//! - [`Machine`] applies the policy to a distribution and reports the per-stage shares of what came
//!   out (`crates/poe1/src/craft/machine.rs`).
//!
//! A move is one of the modelled currencies or one of the loop's terminals. Terminals are values,
//! not an `Option`: "the goal is met", "the rare item is scoured and a new one starts" and "the loop
//! must hand this item to its owner" are three different answers, and a caller that collapses them
//! into "nothing to do" loses the one number the reset case exists to show.
//!
//! # Why the reset is a terminal
//!
//! The owner's loop says: a rare item without the goal is scoured, and a fresh normal item is
//! transmuted again. That is two currencies and a state jump back to a bare base, so it is recorded
//! as [`StepKind::Reset`] with the item's mass; the caller restarts the machine on a new item. The
//! machine never carries a reset forward itself, so its distribution never mixes items from two
//! different rolls, and the cost of a reset cycle stays a separate question the caller can price.
//!
//! # Why there is no `expected_steps`
//!
//! The live distribution is the set of items the loop can be holding, and the next draw's pool
//! depends on which modifiers an item already holds (a full side, a conflicting group). The chain is
//! therefore not lumpable by stage: one "good prefix" item's augmentation pool is not another's, so
//! a per-stage geometric series would be wrong. An exact expected step count would need the whole
//! item distribution as the Markov state — the thing this module already enumerates — so it is
//! left out rather than approximated.
//!
//! The module is separated from `solve`, which weighs a whole method tree; a `Machine` step is one
//! currency applied to every live item, with the exact [`Fraction`](crate::currency::Fraction)
//! weights the currencies already produce.

pub mod machine;
pub mod stage;

pub use machine::{Machine, MachineError, Snapshot};
pub use stage::Stage;

use crate::currency::{ItemSetFilter, Tier};
use crate::data::{Data, Rarity, Side};
use crate::item::Item;

/// What a step does. The first four name the currency to apply; the last three are terminals, kept
/// as values so no caller can mistake "done" for "cannot continue".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StepKind {
    /// Orb of Transmutation: normal → magic.
    Transmutation,
    /// Orb of Alteration: reroll a magic item's modifiers.
    Alteration,
    /// Orb of Augmentation: add one modifier to a magic item with room.
    Augmentation,
    /// Regal Orb: magic → rare with one more modifier.
    Regal,
    /// The item holds a desired modifier; nothing is applied.
    Done,
    /// A rare item without the goal: scour it and start a fresh item.
    Reset,
    /// The loop must not continue; hand the item to its owner.
    Stop,
}

impl StepKind {
    /// Whether this step applies a currency, i.e. whether it changes the item.
    pub fn applies(self) -> bool {
        !matches!(self, StepKind::Done | StepKind::Reset | StepKind::Stop)
    }
}

/// One move and the reason for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub currency: StepKind,
    pub reason: &'static str,
}

impl Step {
    fn new(currency: StepKind, reason: &'static str) -> Step {
        Step { currency, reason }
    }
}

/// The next move for one item.
///
/// The step is a value, not a borrowed currency object: the machine constructs the currency from
/// its data in one place, so a policy stays a pure decision and can be written in a test without a
/// bundle to build it from.
pub trait Policy {
    fn step(&self, data: &Data, item: &Item) -> Step;
}

/// The owner's loop, in the owner's order:
///
/// 1. a desired modifier is held — stop;
/// 2. a good prefix with the suffix side empty — augment;
/// 3. a good suffix with the prefix side empty — augment;
/// 4. good on both sides — regal;
/// 5. a rare item without the goal — stop for three good modifiers, otherwise reset;
/// 6. otherwise — alteration.
///
/// Two clauses are gated on rarity that the loop's own wording does not mention, because the loop
/// only ever reaches them on the item it just made: augmentation and regal are magic-only, and a
/// rare item with an empty side (possible after an annulment) still has only the reset left. Without
/// the gate the policy would name a currency the item refuses.
pub struct LoopPolicy<'a> {
    pub filter: &'a ItemSetFilter,
}

impl<'a> LoopPolicy<'a> {
    pub fn new(filter: &'a ItemSetFilter) -> LoopPolicy<'a> {
        LoopPolicy { filter }
    }
}

impl Policy for LoopPolicy<'_> {
    fn step(&self, data: &Data, item: &Item) -> Step {
        let filter = self.filter;
        if filter.holds_desired(data, item) {
            return Step::new(StepKind::Done, "the item holds a desired modifier");
        }
        let stage = Stage::classify(data, filter, item);
        if item.rarity == Rarity::Magic {
            match stage {
                Stage::GoodPrefix => {
                    return Step::new(
                        StepKind::Augmentation,
                        "a good prefix and an empty suffix side: add a suffix",
                    );
                }
                Stage::GoodSuffix => {
                    return Step::new(
                        StepKind::Augmentation,
                        "a good suffix and an empty prefix side: add a prefix",
                    );
                }
                Stage::GoodPair => {
                    return Step::new(
                        StepKind::Regal,
                        "good on both sides and still magic: make it rare for a third draw",
                    );
                }
                _ => {}
            }
        }
        if item.rarity == Rarity::Rare {
            let good = good_count(data, filter, item);
            if good >= 3 {
                return Step::new(
                    StepKind::Stop,
                    "a rare item with three good modifiers and no desired one: hand it to its owner",
                );
            }
            return Step::new(
                StepKind::Reset,
                "a rare item without the goal: scour it and start a new item",
            );
        }
        if item.rarity == Rarity::Normal {
            return Step::new(StepKind::Transmutation, "a normal item: start it as magic");
        }
        Step::new(
            StepKind::Alteration,
            "the item's modifiers do not move it forward: reroll them",
        )
    }
}

/// How many of an item's modifiers are good. A desired one cannot be counted here: the policy checks
/// for it first and stops.
fn good_count(data: &Data, filter: &ItemSetFilter, item: &Item) -> usize {
    Side::BOTH
        .into_iter()
        .flat_map(|side| item.affixes(side).iter())
        .filter(|a| filter.tier_of(data, a) == Tier::Good)
        .count()
}
