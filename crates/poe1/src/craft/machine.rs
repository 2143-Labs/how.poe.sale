//! The policy, run over a distribution of items.
//!
//! The state is a list of `(item, chance)` pairs, because the next draw's pool depends on which
//! modifiers an item already holds: two items in the same stage can have different augmentation
//! pools, so a per-stage count would not know what to draw from next. A distribution also holds
//! several stages at once — one augmentation produces `GoodPrefix`, `GoodPair` and `Other` outcomes —
//! so the policy is consulted per item: in one step a good-prefix item is augmented while a
//! nothing-good item is altered. One step asks the policy for each item's move, applies that item's
//! currency with the exact integer weights the currencies already produce, and folds the results by
//! item. Items whose move is a terminal leave the live distribution; the per-stage summary carries
//! their mass too, so a step's summary accounts for every item that entered it.
//!
//! # Absorption on reach
//!
//! An outcome that already satisfies the goal (`Stage::Desired`) is terminal: its mass is recorded
//! in the step's summary and it does not enter the live state. The old behaviour kept it live, so
//! the next step saw it again, the policy said `Done`, and the same mass was recorded a second time
//! under `Desired` — summing that stage across steps double-counted. With absorption the live state
//! holds only unfinished items, and a stage's mass summed over steps is the mass that left the loop
//! at that stage, which is what a per-depth curve needs.
//!
//! # Per-depth history
//!
//! `chances` is the latest step; `history` keeps every step actually taken, with its depth, so a
//! caller can print the whole curve instead of only its last point. The list grows by one entry per
//! `step` call and is never reordered; the caller bounds it by bounding how many steps it asks for.
//!
//! # No silent mass
//!
//! A currency that produces no outcome at all would have its item vanish: the fold over the empty
//! outcome list runs zero times and the item is in neither the live state nor the summary. So a
//! zero-total set is an error naming the currency, and so is a set whose weights do not sum to its
//! declared total — a set that lies about its own size cannot be folded into a distribution without
//! inventing or losing mass.

use std::collections::BTreeMap;
use std::fmt;

use super::{Action, CurrencyId, Policy, Stage};
use crate::currency::{
    Alteration, Annulment, Augmentation, Currency, Exalted, Fraction, ItemOutputSet, ItemSetFilter,
    Regal, Scouring, Transmutation,
};
use crate::data::Data;
use crate::item::Item;

/// One step's result: the items still in the loop, and the per-stage shares of everything that came
/// out of the step. `chances` covers the items in `state` plus the mass that reached a terminal —
/// a [`Stage::Desired`] item included, since it is absorbed the moment it appears — so it sums to
/// the mass that entered the step, one, until a terminal has carried mass away.
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
    /// The policy named a currency id the registry does not hold. This is a defect in the caller,
    /// not a refusal: the id is either misspelt or names a currency the crate does not model.
    UnknownCurrency(&'static str),
    /// The currency produced a set with no mass at all. Folding it would drop the item's mass
    /// without recording it anywhere, so the step reports the currency that did it instead.
    ///
    /// No currency in this crate can reach this: one that has nothing to draw refuses the step
    /// itself, and `ItemOutputSet::new` and `ItemOutputSet::filtered` both refuse an empty outcome
    /// list rather than return a zero-total set. It stays as the machine's guard against a set that
    /// was built by hand (the unit test below does exactly that), because a set is a distribution
    /// and an empty one silently loses the item's mass.
    Empty(&'static str),
    /// The currency's set does not sum to its declared total. Folding it would report shares that
    /// do not add up, so the set is rejected rather than normalised silently.
    Inexact(&'static str),
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MachineError::Overflow => write!(f, "the distribution cannot be held exactly in u128"),
            MachineError::Refused(why) => write!(f, "the chosen currency refused the item: {why}"),
            MachineError::UnknownCurrency(id) => {
                write!(f, "no currency is registered under the id {id}")
            }
            MachineError::Empty(id) => write!(
                f,
                "{id} produced no outcome at all; the item's mass would vanish from the state"
            ),
            MachineError::Inexact(id) => write!(
                f,
                "the outcome set of {id} does not sum to its total, so it cannot be folded exactly"
            ),
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
    /// One entry per step taken: the depth (starting at one) and that step's per-stage summary.
    history: Vec<(usize, Vec<(Stage, Fraction)>)>,
}

impl<'d> Machine<'d> {
    /// A machine whose whole mass is on `start`.
    pub fn new(data: &'d Data, filter: ItemSetFilter, start: Item) -> Machine<'d> {
        Machine {
            data,
            filter,
            state: vec![(start, Fraction::ONE)],
            chances: Vec::new(),
            history: Vec::new(),
        }
    }

    /// The live distribution: every unfinished item the loop may be holding, with its share of one.
    /// An item that satisfied the goal left this list the step it appeared (absorb on reach).
    pub fn state(&self) -> &[(Item, Fraction)] {
        &self.state
    }

    /// The last step's per-stage summary; empty before the first step and after a step with nothing
    /// live (a step that moves no item records no summary and adds no depth).
    pub fn chances(&self) -> &[(Stage, Fraction)] {
        &self.chances
    }

    /// Every step taken so far: `(depth, per-stage summary)`, depth starting at one.
    ///
    /// The summaries are the same numbers `chances` reports for the last step; keeping the list
    /// turns "the loop converged" into a curve a caller can print, which is the question the
    /// machine exists to answer.
    pub fn history(&self) -> &[(usize, Vec<(Stage, Fraction)>)] {
        &self.history
    }

    /// One step: every live item takes the policy's move, and the summary is replaced. Items whose
    /// move is a terminal leave the live distribution, and so does every outcome that already
    /// holds a desired modifier.
    pub fn step<P: Policy + ?Sized>(&mut self, policy: &P) -> Result<Snapshot<'_>, MachineError> {
        // A step with nothing live is not a step: it records no summary and adds no depth, so a
        // caller that keeps stepping past an empty machine does not manufacture empty history.
        if self.state.is_empty() {
            self.chances = Vec::new();
            return Ok(Snapshot {
                state: &self.state,
                chances: &self.chances,
            });
        }
        let mut next: BTreeMap<Item, Fraction> = BTreeMap::new();
        let mut stages: BTreeMap<Stage, Fraction> = BTreeMap::new();
        for (item, chance) in &self.state {
            let step = policy.step(self.data, item);
            let Action::Apply(currency) = step.action else {
                let stage = terminal_stage(self.data, &self.filter, item, step.action);
                add(&mut stages, stage, *chance)?;
                continue;
            };
            let set = apply_checked(self.data, currency, item)?;
            let total = set.total();
            for outcome in set.outcomes() {
                let share = Fraction::new(outcome.weight, total).ok_or(MachineError::Overflow)?;
                let p = chance.checked_mul(share).ok_or(MachineError::Overflow)?;
                let stage = Stage::classify(self.data, &self.filter, &outcome.item);
                add(&mut stages, stage, p)?;
                // A desired item is what the loop was working towards, so it leaves the live
                // distribution the step it appears. Keeping it would record the same mass again
                // under `Desired` in every later step.
                if stage != Stage::Desired {
                    add(&mut next, outcome.item.clone(), p)?;
                }
            }
        }
        self.state = next.into_iter().collect();
        self.chances = stages.into_iter().collect();
        let depth = self.history.len() + 1;
        self.history.push((depth, self.chances.clone()));
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

/// The stage a terminal move carries: the loop's own two terminals name themselves, and `Done` is
/// read from the item (which is how it reads as `Desired`).
fn terminal_stage(data: &Data, filter: &ItemSetFilter, item: &Item, action: Action) -> Stage {
    match action {
        Action::Reset => Stage::Reset,
        Action::Stop => Stage::Stop,
        Action::Done => {
            let stage = Stage::classify(data, filter, item);
            // `Done` says the item holds the goal, and the stage has to agree: an item classified
            // into a live stage would have its mass recorded under that stage and then dropped from
            // the state, so the step's summary would not add up. `LoopPolicy` cannot do this (it
            // tests `holds_desired` first, like `Stage::Desired`); a custom policy can, and this is
            // the assertion that catches it in tests while a release build keeps the behaviour.
            debug_assert_eq!(
                stage,
                Stage::Desired,
                "a policy's `Done` must name an item that holds the goal"
            );
            stage
        }
        // The step handles an applied currency before asking for a terminal's stage; classifying
        // here is the harmless reading for a caller that asks anyway.
        Action::Apply(_) => Stage::classify(data, filter, item),
    }
}

/// The currency registry: a [`CurrencyId`] resolved to the currency that id names.
///
/// This is the single place a currency is built, so a policy decides and the machine applies. Adding
/// a currency is one arm here plus its id — nothing in `craft/`'s vocabulary changes, and a policy
/// can name the new id the moment it is registered.
///
/// The lookup is a `match` on the id string, so the same id cannot be registered twice: a second arm
/// for an id is an unreachable pattern, which the workspace's clippy denies. That is a property of
/// the design, not something a test can add to the match to break.
fn currency<'d>(data: &'d Data, id: CurrencyId) -> Option<Box<dyn Currency + 'd>> {
    Some(match id.0 {
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

/// One move, applied: the registered currency's exact distribution of the item.
fn apply(data: &Data, id: CurrencyId, item: &Item) -> Result<ItemOutputSet, MachineError> {
    let modelled = currency(data, id).ok_or(MachineError::UnknownCurrency(id.0))?;
    Ok(modelled.apply(item, &None))
}

/// One move, checked: a refusal, an empty set and a set that lies about its total are all errors
/// rather than a distribution.
///
/// The empty case is the dangerous one. Its outcome list is empty, so the fold that adds each
/// outcome's share runs zero times and the item's mass is added nowhere: the step would "succeed"
/// while silently shrinking the distribution. `exact` already answers false for it, so the check is
/// the same one every report relies on; this is the place that stops the machine from ignoring it.
pub(crate) fn apply_checked(
    data: &Data,
    id: CurrencyId,
    item: &Item,
) -> Result<ItemOutputSet, MachineError> {
    checked(id, apply(data, id, item)?)
}

fn checked(id: CurrencyId, set: ItemOutputSet) -> Result<ItemOutputSet, MachineError> {
    if let ItemOutputSet::Refused(why) = &set {
        return Err(MachineError::Refused(why));
    }
    if set.total() == 0 {
        return Err(MachineError::Empty(id.0));
    }
    if !set.exact() {
        return Err(MachineError::Inexact(id.0));
    }
    Ok(set)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::currency::{ItemOutputSet, Outcome};
    use crate::item::ItemSpec;

    fn normal_shield(data: &Data) -> Item {
        let spec: ItemSpec = serde_json::from_str(
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"normal"}"#,
        )
        .expect("spec");
        Item::from_spec(data, &spec).expect("the shield")
    }

    /// A currency that produces no outcome at all is an error: the fold over its empty outcome
    /// list would run zero times, and the item's mass would be added to neither the live state nor
    /// the summary.
    #[test]
    fn an_empty_set_is_an_error_naming_the_currency() {
        let set = ItemOutputSet::Outcomes {
            outcomes: Vec::new(),
            total: 0,
        };
        assert_eq!(
            checked(CurrencyId::ALTERATION, set),
            Err(MachineError::Empty("currency/alteration"))
        );
    }

    /// A set whose weights do not sum to its total cannot be folded into a distribution: each
    /// outcome's share is its weight over the total, so the shares would not sum to one.
    #[test]
    fn a_set_that_lies_about_its_total_is_an_error() {
        let data = crate::test_data();
        let set = ItemOutputSet::Outcomes {
            outcomes: vec![Outcome {
                item: normal_shield(data),
                weight: 1,
                added: None,
            }],
            total: 5,
        };
        assert_eq!(
            checked(CurrencyId::TRANSMUTATION, set),
            Err(MachineError::Inexact("currency/transmutation"))
        );
    }
}
