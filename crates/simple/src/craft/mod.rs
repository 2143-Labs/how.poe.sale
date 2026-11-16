//! The craft: a policy decides one move per item, and a machine walks the moves.
//!
//! Two machines are here, one file each. [`machine::Machine`] holds the actual items, so it can say
//! exactly what each step produces — and it cannot go far, because a rare shield has three prefixes
//! and three suffixes out of 204 modifiers. [`coarse::Coarse`] holds classes of items instead, so
//! it can walk the loop for depth; it is exact for the same reason the concrete machine is, and the
//! two agree step for step at the first. What it costs is state: the baseline craft fills about 953 000
//! classes by the sixth step, and 198 656 once the policy's finished classes are collapsed.
//!
//! Both read the same [`Policy`], so a decision is written once and cannot drift between them.

pub mod coarse;
pub mod machine;

pub use coarse::Coarse;
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

    /// The move for a class the policy is finished with, from the class's own facts — or `None`
    /// when the class has to be expanded.
    ///
    /// This is what lets the lumped machine forget which modifiers a dead end holds: an item that
    /// is about to be handed over or scoured is never drawn from again, so two dead ends with the
    /// same rarity, counts and grades are one state. It is sound only because the answer is a
    /// function of exactly those facts, which a collapsed class keeps.
    ///
    /// The default is `None`, so a policy that does not answer keeps the state as fine as the items.
    /// A policy that answers must answer `Done`, `Reset` or `Stop`: anything else is a bug, and the
    /// machine will say so rather than guess.
    fn finished(
        &self,
        _filter: &ItemSetFilter,
        _class: &crate::craft::coarse::Class,
    ) -> Option<Action> {
        None
    }
}

/// The loop that builds the target: keep an item while it can still become the thing the craft is
/// for, and give up on it when it cannot.
///
/// 1. the item is the target — done;
/// 2. a magic item good on one side with the other side empty — augment, for the second good side;
/// 3. a magic item good on both sides — regal, for a third draw;
/// 4. a rare item whose prefix side is full and whose best prefix is not the desired one — it can
///    never reach the target, so reset;
/// 5. a rare item with room — exalt, one more modifier;
/// 6. a full rare item that is not the target — reset;
/// 7. a normal item — transmute;
/// 8. anything else — alter.
///
/// Augmentation and regal are gated on the item being magic because both orbs are magic-only, and
/// the loop only reaches those clauses on the item it just made. The annulment is never named: it is
/// the one orb the lumped machine cannot follow, and removing one modifier is not a move this loop
/// wants — a wrong modifier is cheaper to reroll than to annul.
///
/// The target is what "done" means here, not the desired modifier on its own: a magic item holding
/// the totem prefix is a promising item, not a finished one.
pub struct LoopPolicy<'a> {
    pub target: &'a crate::target::Target,
}

impl<'a> LoopPolicy<'a> {
    pub fn new(target: &'a crate::target::Target) -> LoopPolicy<'a> {
        LoopPolicy { target }
    }
}

impl Policy for LoopPolicy<'_> {
    fn finished(
        &self,
        _filter: &ItemSetFilter,
        class: &crate::craft::coarse::Class,
    ) -> Option<Action> {
        // The three answers the step gives without drawing anything, read off the class: the item
        // is the target, or it is a rare item that can no longer become the target — a full prefix
        // side whose best prefix is not the desired one, or no room left at all.
        if class.reached(self.target) {
            return Some(Action::Done);
        }
        if class.rarity == Rarity::Rare
            && (!class.has_room() || (class.full_prefix() && !class.best_prefix_is_desired()))
        {
            return Some(Action::Reset);
        }
        None
    }

    fn step(&self, _data: &Data, filter: &ItemSetFilter, item: &Item) -> Step {
        if self.target.reached(filter, item) {
            return Step::new(Action::Done, "the item is the one the craft is for");
        }
        match item.rarity {
            Rarity::Magic => match Stage::classify(filter, item) {
                Stage::GoodPrefix => Step::new(
                    Action::Apply(CurrencyId::AUGMENTATION),
                    "a good prefix and an empty suffix side: add a suffix",
                ),
                Stage::GoodSuffix => Step::new(
                    Action::Apply(CurrencyId::AUGMENTATION),
                    "a good suffix and an empty prefix side: add a prefix",
                ),
                Stage::GoodPair => Step::new(
                    Action::Apply(CurrencyId::REGAL),
                    "good on both sides and still magic: make it rare for a third draw",
                ),
                _ => Step::new(
                    Action::Apply(CurrencyId::ALTERATION),
                    "the item's modifiers do not move it forward: reroll them",
                ),
            },
            Rarity::Rare => {
                let full_prefix = item.affixes(Side::Prefix).len() == 3;
                if full_prefix && filter.best(item, Side::Prefix) != Some(Tier::Desired) {
                    return Step::new(
                        Action::Reset,
                        "a full prefix side without the desired modifier cannot reach the target",
                    );
                }
                if Side::BOTH
                    .into_iter()
                    .any(|side| item.affixes(side).len() < 3)
                {
                    Step::new(
                        Action::Apply(CurrencyId::EXALTED),
                        "a rare item with room: add one more modifier",
                    )
                } else {
                    Step::new(
                        Action::Reset,
                        "a full rare item that is not the target: scour it and start again",
                    )
                }
            }
            Rarity::Normal => Step::new(
                Action::Apply(CurrencyId::TRANSMUTATION),
                "a normal item: start it as magic",
            ),
        }
    }
}

/// Where an item stands against the goal: what the loop's next move can be read from.
///
/// A stage is a property of one item and one filter, never of a step, so the same function labels a
/// starting item, an outcome of a machine and an item written by hand in a test. The first seven
/// labels cover every item; `Reset` and `Stop` are the loop's terminals and label the mass a step
/// drops, never an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// A desired modifier is held. That does not finish the item — the target is a full rare item,
    /// not the desired modifier alone — so this is a promising item, not a finished one.
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
    /// Terminal: handed to its owner without further work. The baseline loop never uses it — it
    /// keeps working until the item is the target or cannot become it — but a policy that stops
    /// early reports its mass here.
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
        Stage::of_facts(
            item.rarity,
            filter.best(item, Side::Prefix),
            filter.best(item, Side::Suffix),
            item.affixes(Side::Prefix).is_empty(),
            item.affixes(Side::Suffix).is_empty(),
        )
    }

    /// The stage, from the four facts it is made of.
    ///
    /// The concrete machine reads them off an item; the lumped machine reads them off a class's
    /// grade counts. Writing the ladder once means the two cannot disagree about a label.
    pub(crate) fn of_facts(
        rarity: Rarity,
        prefix: Option<Tier>,
        suffix: Option<Tier>,
        empty_prefix: bool,
        empty_suffix: bool,
    ) -> Stage {
        if prefix == Some(Tier::Desired) || suffix == Some(Tier::Desired) {
            return Stage::Desired;
        }
        match (prefix, suffix) {
            (Some(Tier::Good), Some(Tier::Good)) => return Stage::GoodPair,
            (Some(Tier::Good), _) if empty_suffix => return Stage::GoodPrefix,
            (_, Some(Tier::Good)) if empty_prefix => return Stage::GoodSuffix,
            _ => {}
        }
        if empty_prefix && empty_suffix {
            return Stage::Spare;
        }
        if rarity == Rarity::Rare {
            return Stage::SpentRare;
        }
        Stage::Other
    }
}
