//! The craft: a policy decides one move per item, and a machine walks the moves.
//!
//! Two machines are here, one file each. [`machine::Machine`] holds the actual items, so it can say
//! exactly what each step produces — and it cannot go far, because a rare shield has three prefixes
//! and three suffixes out of 204 modifiers. [`coarse::Coarse`] holds classes of items instead, so it
//! can walk the loop for as many steps as asked; it is exact for the same reason the concrete
//! machine is, and the two agree step for step.
//!
//! Both read the same [`Policy`], so a decision is written once and cannot drift between them.

pub mod machine;

pub use machine::{Machine, MachineError};

use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Item, Rarity, Side};

/// A modelled orb, named by the id the rules use (`currency/alteration`).
///
/// The id is the whole contract between a policy and the engine: a policy names an id, the machine
/// resolves it through [`crate::currency::registry`].
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

/// What a step does: use an orb, or stop in one of three ways.
///
/// The terminals are values, not absences, so no caller can mistake "the loop is done" for "the
/// loop cannot continue".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Use this orb on the item.
    Apply(CurrencyId),
    /// The item holds the desired modifier: the loop is finished with it.
    Done,
    /// A rare item without the goal: scour it and start again from nothing.
    Reset,
    /// Hand the item to its owner; the loop must not go on.
    Stop,
}

/// One move and the reason for it. The reason is what a report prints, so it is part of the value.
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
/// A policy is a pure decision: it reads the item, the goal and the pool, and returns a move. It
/// never touches a distribution, which is why the two machines can share it.
pub trait Policy {
    fn step(&self, data: &Data, filter: &ItemSetFilter, item: &Item) -> Step;
}

/// The owner's loop, in the owner's order:
///
/// 1. a desired modifier is held — stop;
/// 2. a magic item good on one side with the other side empty — augment;
/// 3. a magic item good on both sides — regal for a third draw;
/// 4. a rare item with three good modifiers — stop and hand it over;
/// 5. any other rare item — reset;
/// 6. a normal item — transmute;
/// 7. anything else — alter.
///
/// Augmentation and regal are gated on the item being magic, because the loop only ever reaches
/// those clauses on the item it just made, and both orbs are magic-only: without the gate the policy
/// would name an orb the item refuses.
pub struct LoopPolicy;

impl LoopPolicy {
    pub fn new() -> LoopPolicy {
        LoopPolicy
    }
}

impl Default for LoopPolicy {
    fn default() -> LoopPolicy {
        LoopPolicy::new()
    }
}

impl Policy for LoopPolicy {
    fn step(&self, _data: &Data, filter: &ItemSetFilter, item: &Item) -> Step {
        if filter.holds_desired(item) {
            return Step::new(Action::Done, "the item holds a desired modifier");
        }
        if item.rarity == Rarity::Magic {
            match Stage::classify(filter, item) {
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
            if good_count(filter, item) >= 3 {
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

/// How many of an item's modifiers are good.
///
/// A desired modifier cannot be counted here: the policy checks for it first and stops, so an item
/// with a desired modifier never reaches this count.
fn good_count(filter: &ItemSetFilter, item: &Item) -> usize {
    Side::BOTH
        .into_iter()
        .flat_map(|side| item.affixes(side).iter())
        .filter(|affix| filter.tier_of(affix.m) == Tier::Good)
        .count()
}

/// Where an item stands against the goal: what the loop's next move can be read from.
///
/// A stage is a property of one item and one filter, never of a step, so the same function labels a
/// starting item, an outcome of a machine and an item written by hand in a test. The first seven
/// labels cover every item; `Reset` and `Stop` are the loop's terminals and label the mass a step
/// drops, never an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// A desired modifier is held: the loop is done with this item.
    Desired,
    /// Good on both sides, no desired modifier.
    GoodPair,
    /// A good prefix with the suffix side empty.
    GoodPrefix,
    /// A good suffix with the prefix side empty.
    GoodSuffix,
    /// Has modifiers, none of them good, and not rare.
    Other,
    /// No explicit modifiers at all.
    Spare,
    /// A rare item without the desired modifier that no other label claims: the loop has nothing
    /// left for it but a reset.
    SpentRare,
    /// Terminal: a rare item without the goal, scoured to start again.
    Reset,
    /// Terminal: handed to its owner.
    Stop,
}

impl Stage {
    /// Every stage, in the order a report prints them.
    pub const ALL: [Stage; 9] = [
        Stage::Desired,
        Stage::GoodPair,
        Stage::GoodPrefix,
        Stage::GoodSuffix,
        Stage::Other,
        Stage::SpentRare,
        Stage::Spare,
        Stage::Reset,
        Stage::Stop,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Desired => "desired",
            Stage::GoodPair => "good pair",
            Stage::GoodPrefix => "good prefix",
            Stage::GoodSuffix => "good suffix",
            Stage::Other => "other",
            Stage::Spare => "spare",
            Stage::SpentRare => "spent rare",
            Stage::Reset => "reset",
            Stage::Stop => "stop",
        }
    }

    /// The stage of one item under one goal.
    pub fn classify(filter: &ItemSetFilter, item: &Item) -> Stage {
        let prefix = filter.best(item, Side::Prefix);
        let suffix = filter.best(item, Side::Suffix);
        if prefix == Some(Tier::Desired) || suffix == Some(Tier::Desired) {
            return Stage::Desired;
        }
        let empty_prefix = item.affixes(Side::Prefix).is_empty();
        let empty_suffix = item.affixes(Side::Suffix).is_empty();
        match (prefix, suffix) {
            (Some(Tier::Good), Some(Tier::Good)) => return Stage::GoodPair,
            (Some(Tier::Good), _) if empty_suffix => return Stage::GoodPrefix,
            (_, Some(Tier::Good)) if empty_prefix => return Stage::GoodSuffix,
            _ => {}
        }
        if empty_prefix && empty_suffix {
            return Stage::Spare;
        }
        if item.rarity == Rarity::Rare {
            return Stage::SpentRare;
        }
        Stage::Other
    }
}
