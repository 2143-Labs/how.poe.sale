//! The policy, run over a distribution of items.
//!
//! The state is a list of `(item, chance)` pairs, because the next draw's pool depends on which
//! modifiers an item already holds: two items in the same stage can have different augmentation
//! pools, so a per-stage count would not know what to draw from next. A distribution also holds
//! several stages at once — one augmentation produces `GoodPrefix`, `GoodPair` and `Other` outcomes —
//! so the policy is consulted per item: in one step a good-prefix item is augmented while a
//! nothing-good item is altered. One step asks the policy for
//! each item's move, applies that item's currency with the exact integer weights the currencies
//! already produce, and folds the results by item. Items whose move is a terminal leave the live
//! distribution; the per-stage summary carries their mass too, so a step's summary accounts for
//! every item that entered it.

use std::collections::BTreeMap;
use std::fmt;

use super::{Policy, Stage, StepKind};
use crate::currency::{
    Alteration, Augmentation, Currency, Fraction, ItemOutputSet, ItemSetFilter, Regal,
    Transmutation,
};
use crate::data::Data;
use crate::item::Item;

/// One step's result: the items still in the loop, and the per-stage shares of everything that came
/// out of the step. `chances` covers the items in `state` plus the mass that reached a terminal, so
/// it sums to the mass that entered the step — one, until a terminal has carried mass away.
#[derive(Debug)]
pub struct Snapshot<'a> {
    pub state: &'a [(Item, Fraction)],
    pub chances: &'a [(Stage, Fraction)],
}

/// What can stop a step from producing an exact distribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineError {
    /// A chance could not be multiplied or added without leaving `u128`. The distribution is left
    /// untouched: the answer is unavailable, not rounded.
    Overflow,
    /// A currency the policy chose refused the item. The policy is meant to pick only moves the
    /// item can take, so this is a defect in the policy, not a normal outcome.
    Refused(&'static str),
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MachineError::Overflow => write!(f, "the distribution cannot be held exactly in u128"),
            MachineError::Refused(why) => write!(f, "the chosen currency refused the item: {why}"),
        }
    }
}

impl std::error::Error for MachineError {}

/// A craft loop over a distribution.
pub struct Machine<'d> {
    data: &'d Data,
    filter: ItemSetFilter,
    state: Vec<(Item, Fraction)>,
    chances: Vec<(Stage, Fraction)>,
}

impl<'d> Machine<'d> {
    /// A machine whose whole mass is on `start`.
    pub fn new(data: &'d Data, filter: ItemSetFilter, start: Item) -> Machine<'d> {
        Machine {
            data,
            filter,
            state: vec![(start, Fraction::ONE)],
            chances: Vec::new(),
        }
    }

    /// The live distribution: every item the loop may be holding, with its share of one.
    pub fn state(&self) -> &[(Item, Fraction)] {
        &self.state
    }

    /// The last step's per-stage summary; empty before the first step.
    pub fn chances(&self) -> &[(Stage, Fraction)] {
        &self.chances
    }

    /// One step: every live item takes the policy's move, and the summary is replaced. Items whose
    /// move is a terminal leave the live distribution.
    pub fn step<P: Policy + ?Sized>(&mut self, policy: &P) -> Result<Snapshot<'_>, MachineError> {
        let mut next: BTreeMap<Item, Fraction> = BTreeMap::new();
        let mut stages: BTreeMap<Stage, Fraction> = BTreeMap::new();
        for (item, chance) in &self.state {
            let step = policy.step(self.data, item);
            if !step.currency.applies() {
                let stage = match step.currency {
                    StepKind::Reset => Stage::Reset,
                    StepKind::Stop => Stage::Stop,
                    _ => Stage::classify(self.data, &self.filter, item),
                };
                add(&mut stages, stage, *chance)?;
                continue;
            }
            let set = match apply(self.data, step.currency, item) {
                ItemOutputSet::Refused(why) => return Err(MachineError::Refused(why)),
                set => set,
            };
            let total = set.total();
            for outcome in set.outcomes() {
                let share = Fraction::new(outcome.weight, total).ok_or(MachineError::Overflow)?;
                let p = chance.checked_mul(share).ok_or(MachineError::Overflow)?;
                add(&mut next, outcome.item.clone(), p)?;
                let stage = Stage::classify(self.data, &self.filter, &outcome.item);
                add(&mut stages, stage, p)?;
            }
        }
        self.state = next.into_iter().collect();
        self.chances = stages.into_iter().collect();
        Ok(Snapshot {
            state: &self.state,
            chances: &self.chances,
        })
    }

    /// Bounded iteration: at most `steps` steps, stopping early once no item is left.
    pub fn advance<P: Policy + ?Sized>(
        &mut self,
        policy: &P,
        steps: usize,
    ) -> Result<Snapshot<'_>, MachineError> {
        for _ in 0..steps {
            if self.state.is_empty() {
                break;
            }
            self.step(policy)?;
        }
        Ok(Snapshot {
            state: &self.state,
            chances: &self.chances,
        })
    }
}

/// The currency one move applies. This is the single place a currency is built, so a policy decides
/// and the machine applies.
fn apply(data: &Data, kind: StepKind, item: &Item) -> ItemOutputSet {
    match kind {
        StepKind::Transmutation => Transmutation::new(data).apply(item, &None),
        StepKind::Alteration => Alteration::new(data).apply(item, &None),
        StepKind::Augmentation => Augmentation::new(data).apply(item, &None),
        StepKind::Regal => Regal::new(data).apply(item, &None),
        StepKind::Done | StepKind::Reset | StepKind::Stop => {
            ItemOutputSet::refused("the step has no currency to apply")
        }
    }
}

fn add<K: Ord>(
    map: &mut BTreeMap<K, Fraction>,
    key: K,
    chance: Fraction,
) -> Result<(), MachineError> {
    let slot = map.entry(key).or_insert(Fraction::ZERO);
    *slot = slot.checked_add(chance).ok_or(MachineError::Overflow)?;
    Ok(())
}
