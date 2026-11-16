//! The lumped machine: it holds classes of items, so it can walk the loop for depth.
//!
//! A `Class` is exactly what a draw and the goal can read — the rarity, how many modifiers each side
//! holds, how many of them are of each grade, and which groups are held — and nothing else. That is
//! the whole argument for its exactness: a draw reads the rarity (room), the held groups (family
//! exclusion), the side, the item level and the base tags, all of which are fixed or in the class,
//! and the goal reads the grade counts. Two items in one class therefore have the same next
//! distribution and the same goal status, so replacing each of them by one witness item loses no
//! information and no mass.
//!
//! The one orb that is not lumpable here is the Orb of Annulment: taking a modifier off loses which
//! group went, so one class has several successor classes with different pools. [`Coarse::step`]
//! refuses it by name rather than guessing. The loop never names it.
//!
//! The state is large — the baseline craft fills about 953 000 classes by the sixth step — but it
//! closes:
//! from the fifth step on, every class the loop reaches is already known, and a step costs a pass
//! over the tables. Collapsing the classes the policy is finished with brings the same run to
//! 198 656, and the numbers do not move, because a finished item is never drawn from again.
//!
//! Masses are `f64` for the same reason they are in [`super::machine`]: one step's shares are exact
//! chances times the incoming mass, but summing them into a distribution needs a denominator that is
//! the least common multiple of thousands of distinct pool totals. Every chance an orb states stays
//! an exact `Fraction`; only the distribution's shares are floating point, and nothing reads back
//! into a chance.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use crate::currency::{ItemOutputSet, registry};
use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Affix, Item, Rarity, Side};
use crate::target::Target;

use super::{Action, CurrencyId, MachineError, Policy, Stage};

/// What a draw and the goal can read about an item.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Class {
    pub rarity: Rarity,
    /// Modifiers held per side, indexed by [`Side::index`].
    pub counts: [u8; 2],
    /// Held modifiers per side per grade, indexed by grade rank: `tiers[side][2]` desired,
    /// `tiers[side][1]` good, `tiers[side][0]` anything else.
    pub tiers: [[u8; 3]; 2],
    /// Every group held, both sides together, sorted and deduplicated: what the next draw has to
    /// avoid. A list rather than a set because a class holds at most six modifiers' groups and a
    /// step builds millions of classes.
    pub groups: Vec<u16>,
}

impl Class {
    /// The class of one item under one goal.
    pub fn of(data: &Data, filter: &ItemSetFilter, item: &Item) -> Class {
        let mut counts = [0u8; 2];
        let mut tiers = [[0u8; 3]; 2];
        let mut groups: Vec<u16> = Vec::with_capacity(6);
        for side in Side::BOTH {
            for affix in item.affixes(side) {
                counts[side.index()] += 1;
                tiers[side.index()][filter.tier_of(affix.m).rank() as usize] += 1;
                groups.extend_from_slice(&data.mod_at(affix.m).groups);
            }
        }
        groups.sort_unstable();
        groups.dedup();
        Class {
            rarity: item.rarity,
            counts,
            tiers,
            groups,
        }
    }

    /// An item of this class: the same rarity, the same number of modifiers per side and of each
    /// grade, holding exactly the class's groups.
    ///
    /// Which modifiers are chosen does not matter — every item of the class has the same pool and
    /// the same stage — but one has to be found, so the search backtracks over the candidates of
    /// each slot. A class with no witness is a bug in [`Class::of`], which is why this panics rather
    /// than returning a smaller item.
    pub fn witness(&self, data: &Data, filter: &ItemSetFilter) -> Item {
        let mut slots: Vec<(Side, Tier)> = Vec::with_capacity(self.explicits() as usize);
        for side in Side::BOTH {
            for rank in (0..=2u8).rev() {
                let tier = match rank {
                    2 => Tier::Desired,
                    1 => Tier::Good,
                    _ => Tier::Any,
                };
                for _ in 0..self.tiers[side.index()][rank as usize] {
                    slots.push((side, tier));
                }
            }
        }
        let mut fill = Fill {
            data,
            filter,
            slots,
            allowed: &self.groups,
            used: HashSet::new(),
            item: Item::new(self.rarity),
            budget: BUDGET,
        };
        assert!(fill.run(0), "no witness for class {self:?}");
        debug_assert_eq!(Class::of(data, fill.filter, &fill.item), *self);
        fill.item
    }

    /// Whether an item of this class is the item the craft is for.
    pub fn reached(&self, target: &Target) -> bool {
        self.rarity == Rarity::Rare
            && Side::BOTH
                .into_iter()
                .all(|side| target.side_reached(side, self.tiers[side.index()]))
    }

    /// Whether a side still has room for a modifier.
    pub fn has_room(&self) -> bool {
        self.counts.iter().any(|&count| count < 3)
    }

    /// Whether the prefix side holds three modifiers.
    pub fn full_prefix(&self) -> bool {
        self.counts[Side::Prefix.index()] == 3
    }

    /// Whether the best prefix is the desired one.
    pub fn best_prefix_is_desired(&self) -> bool {
        self.tiers[Side::Prefix.index()][Tier::Desired.rank() as usize] > 0
    }

    /// The stage of every item in this class.
    pub fn stage(&self) -> Stage {
        let best = |side: Side| {
            let tiers = &self.tiers[side.index()];
            [Tier::Desired, Tier::Good, Tier::Any]
                .into_iter()
                .find(|tier| tiers[tier.rank() as usize] > 0)
        };
        Stage::of_facts(
            self.rarity,
            best(Side::Prefix),
            best(Side::Suffix),
            self.counts[Side::Prefix.index()] == 0,
            self.counts[Side::Suffix.index()] == 0,
        )
    }

    /// The number of modifiers this class holds, both sides together.
    pub fn explicits(&self) -> u8 {
        self.counts.iter().sum()
    }
}

/// How many choices the witness search may try before it gives up.
///
/// Six slots of at most a few hundred candidates each, with the group test cutting most of them:
/// the search finds a witness in a handful of steps for every class this pool can reach. The bound
/// is a guard against a rule change that makes the search explode, not a limit a real class meets.
const BUDGET: u32 = 2_000_000;

/// The witness search's state: one modifier per slot, chosen so that the union of their groups is
/// exactly the class's.
struct Fill<'a> {
    data: &'a Data,
    filter: &'a ItemSetFilter,
    slots: Vec<(Side, Tier)>,
    /// The groups the class holds: a candidate may hold only these.
    allowed: &'a [u16],
    /// The groups the choices so far hold: a candidate may hold none of these.
    used: HashSet<u16>,
    item: Item,
    budget: u32,
}

impl Fill<'_> {
    /// Depth-first: fill the slots from `at` on, and keep the first choice that fills them all.
    ///
    /// A class can need the backtracking — one group set can have more than one decomposition into
    /// modifiers, and only some of them leave a later slot fillable.
    fn run(&mut self, at: usize) -> bool {
        if at == self.slots.len() {
            return true;
        }
        if self.budget == 0 {
            panic!("the witness search ran out of budget for {:?}", self.item);
        }
        self.budget -= 1;
        let (side, tier) = self.slots[at];
        for (m, entry) in self.data.mods.iter().enumerate() {
            let m = m as u16;
            if entry.side != side || self.filter.tier_of(m) != tier {
                continue;
            }
            if !entry
                .groups
                .iter()
                .all(|group| self.allowed.contains(group))
            {
                continue;
            }
            if entry.groups.iter().any(|group| self.used.contains(group)) {
                continue;
            }
            for group in &entry.groups {
                self.used.insert(*group);
            }
            self.item.affixes_mut(side).push(Affix::new(m));
            if self.run(at + 1) {
                return true;
            }
            self.item.affixes_mut(side).pop();
            for group in &entry.groups {
                self.used.remove(group);
            }
        }
        false
    }
}

/// A distribution over classes, driven one policy step at a time.
pub struct Coarse<'a> {
    data: &'a Data,
    filter: &'a ItemSetFilter,
    target: &'a Target,
    live: BTreeMap<Class, f64>,
    goal: f64,
    reached: f64,
    history: Vec<Vec<(Stage, f64)>>,
    /// Each class's next classes and their shares, computed once.
    ///
    /// The machine is driven by one policy: the cache is keyed by the class and the orb, and the
    /// collapse of finished classes is what that policy says. Driving one machine with two policies
    /// would read the first policy's tables, which is why `step` is not meant for that.
    ///
    /// What a class becomes under an orb cannot change between steps — the orb reads the rarity,
    /// the held groups and the base, all of which are the class — so the second depth onwards would
    /// otherwise recompute the same twelve thousand draws for every class.
    transitions: Transitions,
}

/// Each class's next classes and shares under one orb.
type Transitions = HashMap<(Class, &'static str), Rc<Vec<(Class, f64)>>>;

impl<'a> Coarse<'a> {
    /// A machine starting from one normal item at the whole chance.
    pub fn new(data: &'a Data, filter: &'a ItemSetFilter, target: &'a Target) -> Coarse<'a> {
        let mut live = BTreeMap::new();
        live.insert(Class::of(data, filter, &Item::new(Rarity::Normal)), 1.0);
        Coarse {
            data,
            filter,
            target,
            live,
            goal: 0.0,
            reached: 0.0,
            history: Vec::new(),
            transitions: HashMap::new(),
        }
    }

    /// Takes one step for every live class.
    ///
    /// The policy is asked once per class, on the class's witness: every item in the class answers
    /// the same, because the loop's clauses read nothing but the rarity and the grade counts.
    pub fn step<P: Policy>(&mut self, policy: &P) -> Result<(), MachineError> {
        let current = std::mem::take(&mut self.live);
        let mut live: BTreeMap<Class, f64> = BTreeMap::new();
        let mut dropped: HashMap<Stage, f64> = HashMap::new();

        for (class, mass) in current {
            // A class the policy is finished with is applied without a witness: its held modifiers
            // were forgotten when it was stored, because nothing reads them again.
            if let Some(action) = policy.finished(self.filter, &class) {
                match action {
                    Action::Done => {
                        self.goal += mass;
                        if class.reached(self.target) {
                            self.reached += mass;
                        }
                    }
                    Action::Reset => *dropped.entry(Stage::Reset).or_insert(0.0) += mass,
                    Action::Stop => *dropped.entry(Stage::Stop).or_insert(0.0) += mass,
                    Action::Apply(id) => {
                        return Err(MachineError::UnknownCurrency(id.0));
                    }
                }
                continue;
            }
            let item = class.witness(self.data, self.filter);
            let decision = policy.step(self.data, self.filter, &item);
            match decision.action {
                Action::Apply(id) => {
                    if id == CurrencyId::ANNULMENT {
                        return Err(MachineError::Refused("currency/annulment is not lumpable"));
                    }
                    let key = (class.clone(), id.0);
                    if !self.transitions.contains_key(&key) {
                        let table = self.transitions_of(&item, id.0, policy)?;
                        self.transitions.insert(key.clone(), Rc::new(table));
                    }
                    let table = self.transitions[&key].clone();
                    for (next, share) in table.iter() {
                        *live.entry(next.clone()).or_insert(0.0) += mass * share;
                    }
                }
                Action::Done => {
                    self.goal += mass;
                    if class.reached(self.target) {
                        self.reached += mass;
                    }
                }
                Action::Reset => *dropped.entry(Stage::Reset).or_insert(0.0) += mass,
                Action::Stop => *dropped.entry(Stage::Stop).or_insert(0.0) += mass,
            }
        }

        self.live = live;
        self.history.push(self.row(&dropped));
        Ok(())
    }

    /// One orb's effect on one item, as classes and shares.
    fn transitions_of<P: Policy>(
        &self,
        item: &Item,
        id: &'static str,
        policy: &P,
    ) -> Result<Vec<(Class, f64)>, MachineError> {
        let orb = registry(self.data, id).ok_or(MachineError::UnknownCurrency(id))?;
        match orb.apply(item, &None) {
            ItemOutputSet::Refused(why) => Err(MachineError::Refused(why)),
            ItemOutputSet::Outcomes { outcomes, total } => {
                if outcomes.is_empty() || total == 0 {
                    return Err(MachineError::Empty(id));
                }
                let total = total as f64;
                let mut folded: BTreeMap<Class, f64> = BTreeMap::new();
                for outcome in outcomes {
                    let mut next = Class::of(self.data, self.filter, &outcome.item);
                    // A class the policy is finished with keeps no groups: nothing reads them
                    // again, and forgetting them collapses thousands of dead ends into one state.
                    if policy.finished(self.filter, &next).is_some() {
                        next.groups.clear();
                    }
                    *folded.entry(next).or_insert(0.0) += outcome.weight as f64 / total;
                }
                Ok(folded.into_iter().collect())
            }
        }
    }

    /// The step's row, by stage: the live distribution plus the mass this step's terminals dropped.
    /// The goal is reported beside the row, not inside it.
    fn row(&self, dropped: &HashMap<Stage, f64>) -> Vec<(Stage, f64)> {
        let mut mass: HashMap<Stage, f64> = HashMap::new();
        for (class, share) in &self.live {
            *mass.entry(class.stage()).or_insert(0.0) += share;
        }
        for (stage, share) in dropped {
            *mass.entry(*stage).or_insert(0.0) += share;
        }
        Stage::ALL
            .into_iter()
            .map(|stage| (stage, mass.get(&stage).copied().unwrap_or(0.0)))
            .collect()
    }

    /// The live classes and their mass.
    pub fn live(&self) -> &BTreeMap<Class, f64> {
        &self.live
    }

    /// The mass the loop has finished with: the items holding a desired modifier.
    pub fn goal(&self) -> f64 {
        self.goal
    }

    /// How much of the goal mass is the item the craft is for.
    pub fn reached(&self) -> f64 {
        self.reached
    }

    /// The live mass: what no decision has taken yet.
    pub fn unabsorbed(&self) -> f64 {
        self.live.values().sum()
    }

    /// One row per step taken.
    pub fn history(&self) -> &[Vec<(Stage, f64)>] {
        &self.history
    }
}
