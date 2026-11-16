//! The concrete machine: it holds the actual items and walks the policy one step at a time.
//!
//! One step asks the policy for a move on every live item and replaces the distribution with what
//! that move produces. `Done` mass is absorbed into the goal, `Reset` and `Stop` mass is dropped and
//! reported under its terminal stage, and everything else becomes the next state.
//!
//! The state is a distribution over items, never a stage: a stage is a label a report reads, and an
//! item that rolls well is a different item, not the same item at a later stage. That is why the
//! machine can say exactly what one step produces — and why it cannot take many: a rare shield has
//! three prefixes and three suffixes out of 204 modifiers, so the second step is millions of items
//! and the third is out of reach. [`super::coarse::Coarse`] is the machine for depth.

use std::collections::HashMap;

use crate::currency::{ItemOutputSet, registry};
use crate::data::Data;
use crate::filter::ItemSetFilter;
use crate::fraction::Fraction;
use crate::item::{Item, Rarity};
use crate::target::Target;

use super::{Action, Policy, Stage};

/// Why a step could not be taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineError {
    /// The policy named an orb this crate does not model.
    UnknownCurrency(&'static str),
    /// A chance, a weight or a sum did not fit in 128 bits.
    Overflow,
    /// The orb refused the item, for the reason given.
    Refused(&'static str),
    /// The orb produced no outcome at all.
    Empty(&'static str),
}

impl std::fmt::Display for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineError::UnknownCurrency(id) => write!(f, "{id} is not modelled"),
            MachineError::Overflow => write!(f, "a chance does not fit in 128 bits"),
            MachineError::Refused(why) => f.write_str(why),
            MachineError::Empty(id) => write!(f, "{id} produced no outcome"),
        }
    }
}

impl std::error::Error for MachineError {}

/// A distribution over actual items, driven one policy step at a time.
pub struct Machine<'a> {
    data: &'a Data,
    filter: &'a ItemSetFilter,
    target: &'a Target,
    live: Vec<(Item, Fraction)>,
    goal: Fraction,
    reached: Fraction,
    history: Vec<Vec<(Stage, Fraction)>>,
}

impl<'a> Machine<'a> {
    /// A machine starting from one normal item at the whole chance.
    pub fn new(data: &'a Data, filter: &'a ItemSetFilter, target: &'a Target) -> Machine<'a> {
        Machine {
            data,
            filter,
            target,
            live: vec![(Item::new(Rarity::Normal), Fraction::ONE)],
            goal: Fraction::ZERO,
            reached: Fraction::ZERO,
            history: Vec::new(),
        }
    }

    /// Takes one step for every live item.
    pub fn step<P: Policy>(&mut self, policy: &P) -> Result<(), MachineError> {
        let current = std::mem::take(&mut self.live);
        let mut live: Vec<(Item, Fraction)> = Vec::new();
        let mut index: HashMap<Item, usize> = HashMap::new();
        let mut absorbed = Fraction::ZERO;
        let mut dropped: HashMap<Stage, Fraction> = HashMap::new();

        for (item, mass) in current {
            let decision = policy.step(self.data, self.filter, &item);
            match decision.action {
                Action::Apply(id) => {
                    let orb =
                        registry(self.data, id.0).ok_or(MachineError::UnknownCurrency(id.0))?;
                    match orb.apply(&item, &None) {
                        ItemOutputSet::Refused(why) => {
                            return Err(MachineError::Refused(why));
                        }
                        ItemOutputSet::Outcomes { outcomes, total } => {
                            if outcomes.is_empty() || total == 0 {
                                return Err(MachineError::Empty(id.0));
                            }
                            for outcome in outcomes {
                                let chance = Fraction::new(outcome.weight, total)
                                    .ok_or(MachineError::Overflow)?;
                                let share =
                                    mass.checked_mul(chance).ok_or(MachineError::Overflow)?;
                                fold(&mut live, &mut index, outcome.item, share)?;
                            }
                        }
                    }
                }
                Action::Done => {
                    self.goal = self.goal.checked_add(mass).ok_or(MachineError::Overflow)?;
                    if self.target.reached(self.filter, &item) {
                        self.reached = self
                            .reached
                            .checked_add(mass)
                            .ok_or(MachineError::Overflow)?;
                    }
                    absorbed = absorbed.checked_add(mass).ok_or(MachineError::Overflow)?;
                }
                Action::Reset => add_mass(&mut dropped, Stage::Reset, mass)?,
                Action::Stop => add_mass(&mut dropped, Stage::Stop, mass)?,
            }
        }

        live.sort_by(|a, b| a.0.cmp(&b.0));
        self.live = live;
        self.history.push(self.row(&dropped, absorbed)?);
        Ok(())
    }

    /// The labels a report prints for the step just taken: the new live distribution, the mass a
    /// `Done` decision absorbed (under [`Stage::Desired`]), and the mass the terminals dropped.
    ///
    /// The row is a whole chance every step, which is what makes conservation checkable: no mass is
    /// lost between one row and the next.
    fn row(
        &self,
        dropped: &HashMap<Stage, Fraction>,
        absorbed: Fraction,
    ) -> Result<Vec<(Stage, Fraction)>, MachineError> {
        let mut counts: HashMap<Stage, Fraction> = HashMap::new();
        for (item, mass) in &self.live {
            let stage = Stage::classify(self.filter, item);
            add_mass(&mut counts, stage, *mass)?;
        }
        add_mass(&mut counts, Stage::Desired, absorbed)?;
        for (stage, mass) in dropped {
            add_mass(&mut counts, *stage, *mass)?;
        }
        Ok(Stage::ALL
            .into_iter()
            .map(|stage| (stage, counts.get(&stage).copied().unwrap_or(Fraction::ZERO)))
            .collect())
    }

    /// The live distribution, by item, in a stable order.
    pub fn live(&self) -> &[(Item, Fraction)] {
        &self.live
    }

    /// The mass the loop has finished with: the items holding a desired modifier.
    pub fn goal(&self) -> Fraction {
        self.goal
    }

    /// How much of the goal mass is actually the item the craft is for.
    ///
    /// The loop stops at a desired modifier, which a magic item can hold; the target is a rare item
    /// with three modifiers a side. This is the narrower number, and it is why the two are reported
    /// apart.
    pub fn reached(&self) -> Fraction {
        self.reached
    }

    /// One row per step taken, as [`Self::row`] builds them.
    pub fn history(&self) -> &[Vec<(Stage, Fraction)>] {
        &self.history
    }
}

fn fold(
    live: &mut Vec<(Item, Fraction)>,
    index: &mut HashMap<Item, usize>,
    item: Item,
    mass: Fraction,
) -> Result<(), MachineError> {
    match index.get(&item) {
        Some(&at) => {
            live[at].1 = live[at].1.checked_add(mass).ok_or(MachineError::Overflow)?;
        }
        None => {
            index.insert(item.clone(), live.len());
            live.push((item, mass));
        }
    }
    Ok(())
}

fn add_mass(
    counts: &mut HashMap<Stage, Fraction>,
    stage: Stage,
    mass: Fraction,
) -> Result<(), MachineError> {
    let held = counts.get(&stage).copied().unwrap_or(Fraction::ZERO);
    let sum = held.checked_add(mass).ok_or(MachineError::Overflow)?;
    counts.insert(stage, sum);
    Ok(())
}
