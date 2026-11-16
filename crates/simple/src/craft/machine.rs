//! The concrete machine: it holds the actual items and walks the policy one step at a time.
//!
//! One step asks the policy for a move on every live item and replaces the distribution with what
//! that move produces. `Done` mass is absorbed into the goal, `Reset` and `Stop` mass is dropped and
//! reported under its terminal stage, and everything else becomes the next state.
//!
//! The state is a distribution over items, never a stage: a stage is a label a report reads, and an
//! item that rolls well is a different item, not the same item at a later stage. That is why the
//! machine can say exactly what one step produces — and why it cannot take many: a rare shield has
//! three prefixes and three suffixes out of 204 modifiers, so the second step is thousands of items
//! and the third is out of reach. [`super::coarse::Coarse`] is the machine for depth.
//!
//! **What is exact.** Every chance an orb states is an exact [`Fraction`] of the set's weights, and
//! one step's distribution is those chances times the incoming mass. The *masses* are `f64`: a mass is
//! a share of the distribution, and summing them exactly would need a denominator that is the least
//! common multiple of thousands of distinct pool totals, which does not fit in 128 bits. So a row is
//! one within about `1e-15`, not exactly one, and the tests say so. Nothing here feeds back into a
//! chance.

use std::collections::HashMap;

use crate::currency::{ItemOutputSet, registry};
use crate::data::Data;
use crate::filter::ItemSetFilter;
use crate::item::{Influence, Item, Rarity};
use crate::target::Target;

use super::{Action, Policy, Stage};

/// Why a step could not be taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineError {
    /// The policy named an orb this crate does not model.
    UnknownCurrency(&'static str),
    /// The orb produced no outcome at all.
    Empty(&'static str),
    /// The orb refused the item, for the reason given.
    Refused(&'static str),
}

impl std::fmt::Display for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineError::UnknownCurrency(id) => write!(f, "{id} is not modelled"),
            MachineError::Empty(id) => write!(f, "{id} produced no outcome"),
            MachineError::Refused(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for MachineError {}

/// A distribution over actual items, driven one policy step at a time.
pub struct Machine<'a> {
    data: &'a Data,
    filter: &'a ItemSetFilter,
    target: &'a Target,
    live: Vec<(Item, f64)>,
    goal: f64,
    reached: f64,
    history: Vec<Vec<(Stage, f64)>>,
}

impl<'a> Machine<'a> {
    /// A machine starting from one normal item of the given base, item level and influence, at the
    /// whole chance.
    ///
    /// The starting item is built, not drawn: the base's implicits are already on it, because they
    /// are part of the item and never part of a draw.
    pub fn new(
        data: &'a Data,
        filter: &'a ItemSetFilter,
        target: &'a Target,
        base: u16,
        item_level: u8,
        influence: Option<Influence>,
    ) -> Machine<'a> {
        let mut start = Item::of_base(data, base, item_level, Rarity::Normal);
        if let Some(influence) = influence {
            start = start.with_influence(influence);
        }
        Machine {
            data,
            filter,
            target,
            live: vec![(start, 1.0)],
            goal: 0.0,
            reached: 0.0,
            history: Vec::new(),
        }
    }

    /// Takes one step for every live item.
    pub fn step<P: Policy>(&mut self, policy: &P) -> Result<(), MachineError> {
        let current = std::mem::take(&mut self.live);
        let mut live: Vec<(Item, f64)> = Vec::new();
        let mut index: HashMap<Item, usize> = HashMap::new();
        let mut dropped: HashMap<Stage, f64> = HashMap::new();

        for (item, mass) in current {
            let decision = policy.step(self.data, self.filter, &item);
            match decision.action {
                Action::Apply(id) => {
                    let orb =
                        registry(self.data, id.0).ok_or(MachineError::UnknownCurrency(id.0))?;
                    match orb.apply(&item, &None) {
                        ItemOutputSet::Refused(why) => return Err(MachineError::Refused(why)),
                        ItemOutputSet::Outcomes { outcomes, total } => {
                            if outcomes.is_empty() || total == 0 {
                                return Err(MachineError::Empty(id.0));
                            }
                            let total = total as f64;
                            for outcome in outcomes {
                                let share = mass * (outcome.weight as f64 / total);
                                fold(&mut live, &mut index, outcome.item, share);
                            }
                        }
                    }
                }
                Action::Done => {
                    self.goal += mass;
                    if self.target.reached(self.filter, &item) {
                        self.reached += mass;
                    }
                }
                Action::Reset => *dropped.entry(Stage::Reset).or_insert(0.0) += mass,
                Action::Stop => *dropped.entry(Stage::Stop).or_insert(0.0) += mass,
            }
        }

        live.sort_by(|a, b| a.0.cmp(&b.0));
        self.live = live;
        self.history.push(self.row(&dropped));
        Ok(())
    }

    /// The labels a report prints for the step just taken: the new live distribution, the mass the
    /// `Done` decisions absorbed (under [`Stage::Desired`]), and the mass the terminals dropped.
    ///
    /// A row is the mass that entered the step — the live distribution, plus this step's `Done`
    /// mass, plus this step's dropped mass — so a row is the whole chance minus everything earlier
    /// steps took, and no mass is lost between one row and the next.
    fn row(&self, dropped: &HashMap<Stage, f64>) -> Vec<(Stage, f64)> {
        let mut mass: HashMap<Stage, f64> = HashMap::new();
        for (item, share) in &self.live {
            *mass
                .entry(Stage::classify(self.filter, item))
                .or_insert(0.0) += share;
        }
        for (stage, share) in dropped {
            *mass.entry(*stage).or_insert(0.0) += share;
        }
        Stage::ALL
            .into_iter()
            .map(|stage| (stage, mass.get(&stage).copied().unwrap_or(0.0)))
            .collect()
    }

    /// The live distribution, by item, in a stable order.
    pub fn live(&self) -> &[(Item, f64)] {
        &self.live
    }

    /// The mass the loop has finished with: the items holding a desired modifier.
    pub fn goal(&self) -> f64 {
        self.goal
    }

    /// How much of the goal mass is actually the item the craft is for.
    ///
    /// The loop stops at a desired modifier, which a magic item can hold; the target is a rare item
    /// with three modifiers a side. This is the narrower number, and it is why the two are reported
    /// apart.
    pub fn reached(&self) -> f64 {
        self.reached
    }

    /// The live mass, both sides together: what no decision has taken yet.
    pub fn unabsorbed(&self) -> f64 {
        self.live.iter().map(|&(_, share)| share).sum()
    }

    /// One row per step taken, as [`Self::row`] builds them.
    pub fn history(&self) -> &[Vec<(Stage, f64)>] {
        &self.history
    }
}

fn fold(live: &mut Vec<(Item, f64)>, index: &mut HashMap<Item, usize>, item: Item, share: f64) {
    match index.get(&item) {
        Some(&at) => live[at].1 += share,
        None => {
            index.insert(item.clone(), live.len());
            live.push((item, share));
        }
    }
}
