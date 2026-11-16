//! The machine's distribution, lumped by an exact coarse state.
//!
//! `craft::machine` keeps the concrete item distribution because a step's next pool depends on what
//! the item holds. That dependency is not the whole item: `apply::candidates` — the one function
//! that builds a draw's pool — reads the item only through a short list of facts. Two items that
//! agree on all of them draw from the same pool with the same weights, so a state that records
//! exactly those facts is lumpable *without* approximation.
//!
//! # What `candidates` reads
//!
//! `apply::candidates` (`crates/poe1/src/apply.rs`) reads:
//!
//! - `item.tags(data)`: the base's tags, its influence tags, and the `adds_tags` of every implicit,
//!   enchant and explicit. It is the whole use of the item's tag set, so the union is all that
//!   matters. This is broader than the audit's "union of held modifiers' `adds_tags`" in one way:
//!   the base's own tags, the influence tags and the implicit/enchant tags are part of the union
//!   too, and a class records them as its fixed tags.
//! - `data.base_at(item.base)`: the base's domain (which modifiers the pool may hold) and its
//!   limits. The base is part of the class.
//! - `item.item_level` and the `MaxModRequiredLevelOverride` metamod, which together cap the level
//!   a draw may use. Neither is on the audit's list as such; both are fields of the class.
//! - `item.explicit_groups(data)`: every group an explicit holds, across both sides. The class
//!   keeps the per-side sets, whose union is this set.
//! - `room(data, item, side)` for each side: the rarity limits (rarity, base) minus the affix
//!   count. Rarity and per-side counts are fields of the class.
//! - `item.has_meta(..)` for the Cannot Roll and lock bits, which bar tags from a draw. The class
//!   keeps the metamod bits per side; per side is one step finer than the audit's "metamod bits",
//!   and it is needed because a lock keeps the whole side it sits on, other metamods included.
//! - `item.memory_strands` and `data.cuts_memory_strands()`, which raise the level floor of the
//!   tiers a draw can reach. Both are fields of the class (the patch is the data's).
//!
//! The operations the machine's currencies run read a little more, and the class records it:
//! `corrupted` and `mirrored` (the currencies refuse those items), and per side which affixes are
//! `fractured` (a fractured modifier never changes, so it survives a clear and its groups, tags and
//! buckets have to survive with it).
//!
//! # Why a lumped step is exact
//!
//! Take two items with the same class. By the list above they reach the same candidate set with the
//! same integer weights, so an add draws the same modifier with the same share from either. The
//! added modifier's own groups, tags and bucket are properties of the modifier, so the two outcome
//! classes are computed from the same inputs and are equal. Folding the (different) items into equal
//! classes therefore loses nothing: the class-level distribution is the concrete distribution folded
//! by class, exactly the same [`Fraction`]s.
//!
//! The same argument covers a clear-based currency once the class records the fractured facts. A
//! clear keeps exactly the affixes a `Guard` accepts: a fractured affix everywhere, and every affix
//! on a locked side. Both selections are class facts, so the post-clear class is determined; the
//! currency's add then reads that class, not the concrete item.
//!
//! The policy has to be a function of the class too. `LoopPolicy` is: it reads the desired bits, the
//! stage (which is the per-side bucket maximum) and the count of good modifiers, all class facts.
//! A policy that inspects a modifier index would break the lumping; the contract is stated here
//! because the type system cannot state it.
//!
//! Each class carries one *witness* item — a member of the class — and every lumped step is run on
//! that witness. The witness is not part of the class identity: two equal classes keep whichever
//! witness was inserted first. That is sound for exactly the reason above: every member of a class
//! gives the same outcome class distribution, so any witness stands for all of them.
//!
//! # What cannot be lumped
//!
//! An Orb of Annulment removes one of the held modifiers, chosen uniformly among the removable ones.
//! The class records the per-side *union* of groups and the per-side bucket counts, not which
//! modifier carries which group, so two members of one class can lose different groups to the same
//! annulment and land in different classes. Enumerating every item a class abstracts is not
//! tractable (the classes exist precisely to avoid that enumeration), so [`lump_step`] refuses
//! annulment rather than approximating it.
//!
//! One more `candidates` input is not recorded because no registered currency asks for it:
//! `sides(data, item, SideReq::Dominant)` reads the eldritch implicits' tiers to pick the dominant
//! side. All seven currencies draw with `SideReq::Any`; a currency that drew a dominant side would
//! also have to add that fact to the class before it could be lumped.

use std::collections::BTreeMap;
use std::fmt;

use super::machine::{apply_checked, MachineError};
use super::{Action, CurrencyId, Policy, Stage};
use crate::currency::{Fraction, ItemSetFilter, Tier};
use crate::data::{Data, Influence, Rarity, Side};
use crate::item::{Affix, Item, MetaMod};

/// The per-side facts a class keeps: how many affixes, how they split across the goal's buckets,
/// which modifier groups they hold, which tags they add, and which metamods sit on the side.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SideFacts {
    count: u8,
    desired: u8,
    good: u8,
    groups: Vec<u16>,
    tags: Vec<u16>,
    metas: u8,
}

/// The class key: every fact a step reads about an item, and nothing else. `Class` pairs it with one
/// witness item; equality and ordering are the key's.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Key {
    base: u16,
    item_level: u8,
    rarity: Rarity,
    influences: u8,
    corrupted: bool,
    mirrored: bool,
    memory_strands: u8,
    /// Every tag the item has that is not an explicit's `adds_tags`: base, influence, implicits,
    /// enchants. Stored sorted and deduped, like every other set here.
    fixed_tags: Vec<u16>,
    sides: [SideFacts; 2],
    /// The affixes a clear keeps on an unlocked side, and the facts those affixes still carry.
    fractured: [SideFacts; 2],
}

impl Key {
    /// The class a `clear_changeable` step leaves: a locked side keeps every affix, an unlocked side
    /// keeps only the fractured ones.
    ///
    /// This is the same selection `apply::clear_changeable` makes through `Guard::can_change`
    /// (fractured, or a respected lock), expressed on the class instead of on an item. It exists so
    /// that two classes whose clears agree can share one reroll: an alteration clears first, so its
    /// outcome distribution depends on the cleared class, not on the class it started from.
    fn cleared(&self) -> Key {
        let mut out = self.clone();
        for side in Side::BOTH {
            let i = side.index();
            let lock = match side {
                Side::Prefix => MetaMod::LockPrefixes,
                Side::Suffix => MetaMod::LockSuffixes,
            };
            let kept = if out.sides[i].metas & meta_bit(lock) != 0 {
                out.sides[i].clone()
            } else {
                out.fractured[i].clone()
            };
            out.sides[i] = kept.clone();
            // Everything left is fractured (a locked side's non-fractured affixes are locked, not
            // fractured, but a later clear keeps the side again, so the distinction never matters).
            out.fractured[i] = kept;
        }
        out
    }
}

fn meta_bit(meta: MetaMod) -> u8 {
    1 << (meta as u8)
}

/// One coarse state: every fact `apply::candidates` and the machine's currencies read about an item,
/// plus one item that has those facts.
///
/// Two items with equal classes are interchangeable for every lumpable step — same pool, same
/// weights, same next classes — so a distribution over classes is an exact restatement of the
/// concrete distribution folded by class. See the module docs for the argument and for the one
/// operation (annulment) that is refused because the class does not record enough to make it exact.
#[derive(Clone, Debug)]
pub struct Class {
    key: Key,
    witness: Item,
}

impl Class {
    /// The class of one item under one goal. The filter is part of the answer because `Good`,
    /// `Any` and `Desired` are the goal's buckets — without a filter there are no buckets to count.
    pub fn of(data: &Data, filter: &ItemSetFilter, item: &Item) -> Class {
        Class {
            key: key_of(data, filter, item),
            witness: item.clone(),
        }
    }

    /// One item in this class; any other member would answer every lumped step the same way.
    pub fn witness(&self) -> &Item {
        &self.witness
    }

    pub fn rarity(&self) -> Rarity {
        self.key.rarity
    }

    /// The stage the class classifies into. It is a class fact (per-side bucket maxima, counts and
    /// rarity), so every member of the class has this stage.
    pub fn stage(&self, data: &Data, filter: &ItemSetFilter) -> Stage {
        Stage::classify(data, filter, &self.witness)
    }

    /// The key a clear of this class leaves.
    fn cleared(&self) -> Key {
        self.key.cleared()
    }
}

impl PartialEq for Class {
    fn eq(&self, other: &Class) -> bool {
        self.key == other.key
    }
}

impl Eq for Class {}

impl PartialOrd for Class {
    fn partial_cmp(&self, other: &Class) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Class {
    fn cmp(&self, other: &Class) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

/// One class's move: the live classes it produces and the per-stage masses, all over a unit of mass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassStep {
    pub live: Vec<(Class, Fraction)>,
    pub chances: Vec<(Stage, Fraction)>,
    /// The mass that reached [`Stage::Desired`] and was absorbed.
    pub absorbed: Fraction,
}

/// What can stop a lumped step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LumpError {
    /// The concrete engine refused, produced no mass, or could not hold the set exactly.
    Machine(MachineError),
    /// A currency whose outcome classes are not a function of the class. See the module docs.
    NotLumpable(&'static str),
}

impl fmt::Display for LumpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LumpError::Machine(err) => err.fmt(f),
            LumpError::NotLumpable(why) => write!(f, "this class cannot be lumped: {why}"),
        }
    }
}

impl std::error::Error for LumpError {}

impl From<MachineError> for LumpError {
    fn from(err: MachineError) -> LumpError {
        LumpError::Machine(err)
    }
}

/// One depth of a lumped run: the live classes after the step, the per-stage masses it recorded, and
/// how much of the mass was absorbed at the goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Depth {
    /// The step this depth is, starting at one.
    pub index: usize,
    /// The live classes and their shares of one.
    pub classes: Vec<(Class, Fraction)>,
    /// Every stage's mass for this step, live and absorbed alike; it sums to one.
    pub chances: Vec<(Stage, Fraction)>,
    /// The mass that reached the goal this step and left the live state.
    pub absorbed: Fraction,
}

impl Depth {
    /// The mass still in the loop after this step.
    pub fn live(&self) -> Fraction {
        sum(self.classes.iter().map(|(_, p)| p))
    }

    /// The mass this step recorded at `stage`.
    pub fn mass(&self, stage: Stage) -> Fraction {
        sum(self
            .chances
            .iter()
            .filter(|(s, _)| *s == stage)
            .map(|(_, p)| p))
    }
}

fn sum<'a>(chances: impl IntoIterator<Item = &'a Fraction>) -> Fraction {
    chances
        .into_iter()
        .fold(Fraction::ZERO, |acc, &p| acc.checked_add(p).expect("exact"))
}

/// The facts of one item, as a key. Order-independent: every set is sorted and deduped here, so two
/// items with the same affixes in a different order have the same key.
fn key_of(data: &Data, filter: &ItemSetFilter, item: &Item) -> Key {
    let base = data.base_at(item.base);
    let mut fixed: Vec<u16> = base.tags.clone();
    if let Some(tags) = item.class(data).influence_tags {
        if base.all_influences {
            fixed.extend(tags);
        } else {
            for influence in Influence::ALL {
                if item.has_influence(influence) {
                    fixed.push(tags[influence as usize]);
                }
            }
        }
    }
    for roll in item.implicits.iter().chain(&item.enchants) {
        fixed.extend(&data.mod_at(roll.m).adds_tags);
    }
    fixed.sort_unstable();
    fixed.dedup();

    let metas: BTreeMap<u16, u8> = MetaMod::ALL
        .iter()
        .filter_map(|meta| data.group_by_name(meta.group()).map(|g| (g, meta_bit(*meta))))
        .collect();
    let mut sides = [SideFacts::default(), SideFacts::default()];
    let mut fractured = [SideFacts::default(), SideFacts::default()];
    for side in Side::BOTH {
        for affix in item.affixes(side) {
            hold(data, filter, &metas, &mut sides[side.index()], affix);
            if affix.fractured {
                hold(
                    data,
                    filter,
                    &metas,
                    &mut fractured[side.index()],
                    affix,
                );
            }
        }
    }
    for facts in sides.iter_mut().chain(fractured.iter_mut()) {
        facts.groups.sort_unstable();
        facts.groups.dedup();
        facts.tags.sort_unstable();
        facts.tags.dedup();
    }

    Key {
        base: item.base,
        item_level: item.item_level,
        rarity: item.rarity,
        influences: item.influences,
        corrupted: item.corrupted,
        mirrored: item.mirrored,
        memory_strands: item.memory_strands,
        fixed_tags: fixed,
        sides,
        fractured,
    }
}

/// Folds one affix into the side's facts.
fn hold(
    data: &Data,
    filter: &ItemSetFilter,
    metas: &BTreeMap<u16, u8>,
    facts: &mut SideFacts,
    affix: &Affix,
) {
    let md = data.mod_at(affix.roll.m);
    facts.count += 1;
    match filter.tier_of(data, affix) {
        Tier::Desired => facts.desired += 1,
        Tier::Good => facts.good += 1,
        Tier::Any => {}
    }
    facts.groups.extend(md.groups.iter().copied());
    facts.tags.extend(md.adds_tags.iter().copied());
    for group in &md.groups {
        if let Some(&bit) = metas.get(group) {
            facts.metas |= bit;
        }
    }
}

/// One policy move over one class, exactly: the live outcome classes and the per-stage masses.
///
/// The move is the policy's, read from the class's witness; a policy that is a function of the class
/// (the contract in the module docs) makes this the class's move rather than the witness's.
pub fn lump_step<P: Policy + ?Sized>(
    data: &Data,
    filter: &ItemSetFilter,
    class: &Class,
    policy: &P,
) -> Result<ClassStep, LumpError> {
    let action = policy.step(data, class.witness()).action;
    step_action(data, filter, class, action)
}

/// One currency applied to one class: the class-level distribution `lump_step` would give a policy
/// that chose `currency`, without a policy value. This is the "class and currency in, classes and
/// Fractions out" step; `lump_step` is the same thing driven by a decision.
pub fn lump_apply(
    data: &Data,
    filter: &ItemSetFilter,
    class: &Class,
    currency: CurrencyId,
) -> Result<ClassStep, LumpError> {
    step_action(data, filter, class, Action::Apply(currency))
}

/// One move, already decided: the currency's exact distribution folded by class, or the terminal's
/// single stage.
fn step_action(
    data: &Data,
    filter: &ItemSetFilter,
    class: &Class,
    action: Action,
) -> Result<ClassStep, LumpError> {
    let Action::Apply(id) = action else {
        let stage = match action {
            Action::Reset => Stage::Reset,
            Action::Stop => Stage::Stop,
            _ => class.stage(data, filter),
        };
        return Ok(ClassStep {
            live: Vec::new(),
            chances: vec![(stage, Fraction::ONE)],
            absorbed: if stage == Stage::Desired {
                Fraction::ONE
            } else {
                Fraction::ZERO
            },
        });
    };
    if id == CurrencyId::ANNULMENT {
        return Err(LumpError::NotLumpable(
            "an annulment removes one of the held modifiers, and a class records the per-side \
             union of groups rather than which modifier carries which group",
        ));
    }
    let set = apply_checked(data, id, class.witness())?;
    let total = set.total();
    let mut live: BTreeMap<Key, (Item, Fraction)> = BTreeMap::new();
    let mut stages: BTreeMap<Stage, Fraction> = BTreeMap::new();
    let mut absorbed = Fraction::ZERO;
    for outcome in set.outcomes() {
        let p = Fraction::new(outcome.weight, total).ok_or(MachineError::Overflow)?;
        let stage = Stage::classify(data, filter, &outcome.item);
        add(&mut stages, stage, p)?;
        if stage == Stage::Desired {
            absorbed = absorbed
                .checked_add(p)
                .ok_or(MachineError::Overflow)?;
        } else {
            // Build the key first and the class (with its witness item) only for a key that is new:
            // the same outcome class shows up many times in one set, and cloning the item each time
            // would cost more than the fold.
            let key = key_of(data, filter, &outcome.item);
            match live.entry(key) {
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    let (_, mass) = slot.get_mut();
                    *mass = mass.checked_add(p).ok_or(MachineError::Overflow)?;
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert((outcome.item.clone(), p));
                }
            }
        }
    }
    Ok(ClassStep {
        live: live
            .into_iter()
            .map(|(key, (witness, p))| (Class { key, witness }, p))
            .collect(),
        chances: stages.into_iter().collect(),
        absorbed,
    })
}

fn add<K: Ord>(map: &mut BTreeMap<K, Fraction>, key: K, p: Fraction) -> Result<(), MachineError> {
    let slot = map.entry(key).or_insert(Fraction::ZERO);
    *slot = slot.checked_add(p).ok_or(MachineError::Overflow)?;
    Ok(())
}

/// A bounded multi-step run over classes.
///
/// The state is a map from class to mass, and every step folds each class's outcome classes back
/// together, so the state stays at the coarse size instead of growing with the concrete item count.
/// `history` keeps every depth: per-class masses, the per-stage masses and the absorbed share, all
/// exact [`Fraction`]s.
pub struct Lumped<'d> {
    data: &'d Data,
    filter: ItemSetFilter,
    state: BTreeMap<Class, Fraction>,
    history: Vec<Depth>,
    /// An alteration's outcome distribution, keyed by the class its clear leaves. Every class with
    /// the same cleared class rerolls the same way, so one computation stands for all of them — the
    /// difference is exactly what makes the concrete machine explode.
    alterations: BTreeMap<Key, ClassStep>,
}

impl<'d> Lumped<'d> {
    /// A lumped run whose whole mass is on `start`.
    pub fn new(data: &'d Data, filter: ItemSetFilter, start: Item) -> Lumped<'d> {
        let class = Class::of(data, &filter, &start);
        let mut state = BTreeMap::new();
        state.insert(class, Fraction::ONE);
        Lumped {
            data,
            filter,
            state,
            history: Vec::new(),
            alterations: BTreeMap::new(),
        }
    }

    /// The live classes and their shares after the last step.
    pub fn state(&self) -> &BTreeMap<Class, Fraction> {
        &self.state
    }

    /// Every depth taken so far.
    pub fn history(&self) -> &[Depth] {
        &self.history
    }

    /// One step: every live class takes the policy's move, and the outcome classes are folded back
    /// together with their exact masses.
    pub fn step<P: Policy + ?Sized>(&mut self, policy: &P) -> Result<&Depth, LumpError> {
        let mut next: BTreeMap<Class, Fraction> = BTreeMap::new();
        let mut stages: BTreeMap<Stage, Fraction> = BTreeMap::new();
        let mut absorbed = Fraction::ZERO;
        for (class, mass) in &self.state {
            let action = policy.step(self.data, class.witness()).action;
            let shared = action == Action::Apply(CurrencyId::ALTERATION);
            let local: ClassStep;
            let one: &ClassStep = if shared {
                let key = class.cleared();
                if !self.alterations.contains_key(&key) {
                    let computed = step_action(self.data, &self.filter, class, action)?;
                    self.alterations.insert(key.clone(), computed);
                }
                self.alterations.get(&key).expect("inserted above")
            } else {
                local = step_action(self.data, &self.filter, class, action)?;
                &local
            };
            for (outcome, p) in &one.live {
                let scaled = mass.checked_mul(*p).ok_or(MachineError::Overflow)?;
                add(&mut next, outcome.clone(), scaled)?;
            }
            for (stage, p) in &one.chances {
                let scaled = mass.checked_mul(*p).ok_or(MachineError::Overflow)?;
                add(&mut stages, *stage, scaled)?;
            }
            absorbed = absorbed
                .checked_add(mass.checked_mul(one.absorbed).ok_or(MachineError::Overflow)?)
                .ok_or(MachineError::Overflow)?;
        }
        self.state = next;
        let index = self.history.len() + 1;
        self.history.push(Depth {
            index,
            classes: self
                .state
                .iter()
                .map(|(class, p)| (class.clone(), *p))
                .collect(),
            chances: stages.into_iter().collect(),
            absorbed,
        });
        Ok(self.history.last().expect("just pushed"))
    }

    /// Bounded iteration: at most `steps` steps, stopping early once no class is left.
    pub fn advance<P: Policy + ?Sized>(
        &mut self,
        policy: &P,
        steps: usize,
    ) -> Result<&[Depth], LumpError> {
        for _ in 0..steps {
            if self.state.is_empty() {
                break;
            }
            self.step(policy)?;
        }
        Ok(&self.history)
    }
}
