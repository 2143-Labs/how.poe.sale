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
//! as [`Action::Reset`] with the item's mass; the caller restarts the machine on a new item. The
//! machine never carries a reset forward itself, so its distribution never mixes items from two
//! different rolls, and the cost of a reset cycle stays a separate question the caller can price.
//!
//! # Why `Stage` is not the Markov state
//!
//! The live distribution is the set of items the loop can be holding, and the next draw's pool
//! depends on which modifiers an item already holds. Four facts of the engine make two items in the
//! same stage draw differently, so a per-stage geometric series would be wrong:
//!
//! - Regal's group exclusion: a draw may not repeat a modifier group the item already holds
//!   (`apply::candidates` filters those out), so a magic and a rare `GoodPair` can reach different
//!   pools even before their room is considered.
//! - Rarity and room: a magic and a rare `GoodPair` have different limits for their rarity (one
//!   prefix and one suffix against three of each), so the same stage has a different number of open
//!   slots and therefore a different next draw.
//! - The metamod bits: `MaxModRequiredLevelOverride` alone cuts the pool from 204 modifiers to 58
//!   by capping the level a draw may use, and the lock and Cannot Roll bits change what may be
//!   drawn or removed.
//! - Fractured or locked affixes survive a clear, so two items in the same stage can differ in what
//!   a scour or an alteration leaves behind.
//!
//! The chain is therefore not lumpable by stage. It *is* lumpable by a coarse state that keeps
//! exactly what `apply::candidates` reads: [`lumped::Class`] is that state, and a geometric series
//! over the lumped chain is exact rather than approximate. An expected step count from that
//! lumped chain is still a separate question, which is why the machine reports distributions and
//! not a single number; `Machine` keeps the concrete path, [`lumped::Lumped`] the coarse one.
//!
//! The module is separated from `solve`, which weighs a whole method tree; a `Machine` step is one
//! currency applied to every live item, with the exact [`Fraction`](crate::currency::Fraction)
//! weights the currencies already produce.

pub mod lumped;
pub mod machine;
pub mod stage;

pub use lumped::{Class, Depth, LumpError, Lumped};
pub use machine::{Machine, MachineError, Snapshot};
pub use stage::Stage;

use crate::currency::{ItemSetFilter, Tier};
use crate::data::{Data, Rarity, Side};
use crate::item::Item;

/// A modelled currency, named by the method id the rules use (`currency/alteration`).
///
/// The id is the contract, not the const: the machine resolves an id through its registry, so a
/// currency reaches the machine by being registered there. The consts below are convenience for the
/// seven ids this crate implements today; a policy may name any registered id, and a new currency
/// needs a registry arm rather than a change to this vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurrencyId(pub &'static str);

impl CurrencyId {
    pub const TRANSMUTATION: CurrencyId = CurrencyId("currency/transmutation");
    pub const ALTERATION: CurrencyId = CurrencyId("currency/alteration");
    pub const AUGMENTATION: CurrencyId = CurrencyId("currency/augmentation");
    pub const REGAL: CurrencyId = CurrencyId("currency/regal");
    pub const EXALTED: CurrencyId = CurrencyId("currency/exalted");
    pub const SCOURING: CurrencyId = CurrencyId("currency/scouring");
    pub const ANNULMENT: CurrencyId = CurrencyId("currency/annulment");
}

/// What a step does: apply a currency, or one of three terminals. The terminals stay values so no
/// caller can mistake "done" for "cannot continue".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Apply this currency to the item.
    Apply(CurrencyId),
    /// The item holds a desired modifier; nothing is applied.
    Done,
    /// A rare item without the goal: scour it and start a fresh item.
    Reset,
    /// The loop must not continue; hand the item to its owner.
    Stop,
}

impl Action {
    /// Whether this action applies a currency, i.e. whether it changes the item.
    pub fn applies(self) -> bool {
        matches!(self, Action::Apply(_))
    }
}

/// One move and the reason for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    pub reason: &'static str,
}

impl Step {
    fn new(action: Action, reason: &'static str) -> Step {
        Step { action, reason }
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
///
/// The policy names currencies by [`CurrencyId`], so the ids are the whole contract between a
/// policy and the engine: a currency added to the machine's registry is usable from a policy
/// without touching this module.
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
            return Step::new(Action::Done, "the item holds a desired modifier");
        }
        let stage = Stage::classify(data, filter, item);
        if item.rarity == Rarity::Magic {
            match stage {
                Stage::GoodPrefix => {
                    return Step::new(
                        Action::Apply(CurrencyId::AUGMENTATION),
                        "a good prefix and an empty suffix side: add a suffix",
                    );
                }
                Stage::GoodSuffix => {
                    return Step::new(
                        Action::Apply(CurrencyId::AUGMENTATION),
                        "a good suffix and an empty prefix side: add a prefix",
                    );
                }
                Stage::GoodPair => {
                    return Step::new(
                        Action::Apply(CurrencyId::REGAL),
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
                    Action::Stop,
                    "a rare item with three good modifiers and no desired one: hand it to its owner",
                );
            }
            return Step::new(
                Action::Reset,
                "a rare item without the goal: scour it and start a new item",
            );
        }
        if item.rarity == Rarity::Normal {
            return Step::new(
                Action::Apply(CurrencyId::TRANSMUTATION),
                "a normal item: start it as magic",
            );
        }
        Step::new(
            Action::Apply(CurrencyId::ALTERATION),
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
