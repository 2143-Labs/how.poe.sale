//! One step of reverse solving: from a concrete item and a pattern, weigh every single crafting
//! step the item allows and say what each one does to the goal.
//!
//! A step is weighed from its own outcome. `apply` runs it, `pattern::lift` reads every leaf at
//! the pattern's level of detail, and a leaf scores as the share of the pattern's slot value it
//! holds (`1.0` once the item matches, `0` when it fails the pattern's filter). A slot's value is
//! the grade of the class that fills it — a best-possible class is worth two, a good-enough class
//! one, a bad class nothing — so an item that lands the veiled prefix is
//! counted further along than one that lands the crafted slot the pattern asks for, and an item
//! whose third prefix is a bad class is not credited for it at all.
//!
//! The report keeps three groups apart, because a number computed from part of an outcome is not
//! comparable with a number computed from all of it:
//!
//! - **exact**: every outcome is computed (`Chance` splits, single draws, unveil, a bench craft);
//! - **certain part**: the random rest of the step is not modelled yet, so the numbers come from
//!   what the step certainly does (a reforge's rarity, clear and guaranteed modifier);
//! - **blind**: nothing about the result is known (the socket tables, rules still being
//!   confirmed), so the step is listed with its reason and not ranked.
//!
//! This is one step of lookahead, not a route planner: it says which single craft moves the item
//! closest to the goal and what that craft risks, and it deliberately does not chain steps or
//! compare costs across currencies (an Exalted Orb and a Chaos Orb can only be compared with a
//! price table the bundle does not have).

use std::cmp::Ordering;

use crate::apply::{self, Params, Unmet};
use crate::data::{Cost, Data, Side};
use crate::item::Item;
use crate::method::{Method, PendingReason};
use crate::pattern::{self, Grade, Lifted, LiftedLeaf, Pattern, PatternState};

/// Two probabilities closer than this are the same number when deciding whether a step gained or
/// lost ground.
const EPS: f64 = 1e-9;

/// A number on the report's grid: rounding noise below `EPS` lands on the same step. Comparing
/// these, rather than testing `|a - b| < EPS`, keeps the order total (`sort_by` needs that; a
/// tolerance alone would make `a ≈ b ≈ c` with `a ≠ c`). Adding zero turns `-0.0` into `0.0`,
/// which `total_cmp` would otherwise rank below it.
fn level(x: f64) -> f64 {
    (x / EPS).round() + 0.0
}

/// Best first, at the report's tolerance: two numbers the report cannot tell apart compare equal,
/// so a later tie-break decides.
fn cmp_desc(a: f64, b: f64) -> Ordering {
    level(b).total_cmp(&level(a))
}

/// Least first, at the same tolerance.
fn cmp_asc(a: f64, b: f64) -> Ordering {
    level(a).total_cmp(&level(b))
}

/// `f64` sums start at `-0.0`, which prints as `-0.000`; a chance of nothing is zero.
fn zero(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x }
}

/// What one slot of a pattern is worth when it holds a class of this grade.
/// The three categories a score uses: the best possible (2), good enough (1), not good enough (0).
/// The pattern's five grades are authoring vocabulary — they say *why* a class is only good
/// enough — and collapse onto these three.
pub fn grade_value(grade: Grade) -> u8 {
    match grade {
        Grade::Desired => 2,
        Grade::Good | Grade::Middling | Grade::Neutral => 1,
        Grade::Bad => 0,
    }
}

/// How far a pattern state is toward the goal: `1.0` when it matches (every finished item is
/// equally finished), otherwise the share of the goal's slot value it holds. Zero when the item
/// fails the pattern's filter, which is why a step that corrupts an item the pattern wants
/// uncorrupted scores nothing however many good modifiers it keeps.
pub fn state_progress(pattern: &Pattern, state: &PatternState) -> f64 {
    if !state.item_ok {
        return 0.0;
    }
    if state.matches {
        return 1.0;
    }
    let (mut got, mut most) = (0u32, 0u32);
    for side in Side::BOTH {
        let classes: Vec<u16> = affixes(state, side).iter().map(|a| a.class).collect();
        for (slot, affix) in pattern::placement(pattern, side, &classes)
            .iter()
            .enumerate()
        {
            let best = pattern.slots[side.index()][slot]
                .iter()
                .map(|c| grade_value(pattern.classes[*c as usize].grade))
                .max()
                .unwrap_or(0);
            if best == 0 {
                continue;
            }
            most += best as u32;
            if let Some(i) = affix {
                got += grade_value(pattern.classes[classes[*i] as usize].grade) as u32;
            }
        }
    }
    if most == 0 {
        0.0
    } else {
        got as f64 / most as f64
    }
}

/// The item's expected progress: every state `pattern::evaluate` gives, weighted.
pub fn item_progress(data: &Data, pattern: &Pattern, item: &Item) -> f64 {
    pattern::evaluate(data, pattern, item)
        .iter()
        .map(|(p, s)| p * state_progress(pattern, s))
        .sum()
}

/// Why a slot the pattern wants is empty and no step can fill it as the item stands. The kinds
/// cost the player very differently, which is why the report names them: a fractured modifier
/// cannot be removed at all, a metamod comes off for one Orb of Scouring, and a full side of
/// ordinary modifiers is an annulment's gamble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// A metamod holds the crafted slot the pattern wants: one Orb of Scouring frees it.
    Metamod(Side),
    /// A crafted modifier holds that slot: the bench removes it for a scour.
    Crafted(Side),
    /// The side is full of modifiers that can be removed, at some cost.
    Side(Side),
    /// A fractured modifier sits in the way and cannot be removed.
    Fractured(Side),
}

impl Blocker {
    /// The reason in the report's words.
    pub fn phrase(self) -> String {
        match self {
            Blocker::Metamod(side) => format!(
                "a metamod holds the crafted {} slot; one Orb of Scouring frees it",
                side.name()
            ),
            Blocker::Crafted(side) => format!(
                "a bench craft holds the crafted {} slot; removing it costs one scour",
                side.name()
            ),
            Blocker::Side(side) => format!(
                "the {} side is full of modifiers a removal could take",
                side.name()
            ),
            Blocker::Fractured(side) => {
                format!("a fractured {} modifier cannot be removed", side.name())
            }
        }
    }
}

/// Why the item cannot reach the pattern as it stands. Empty when nothing is stuck.
pub fn blockers(pattern: &Pattern, state: &PatternState) -> Vec<Blocker> {
    let mut out = Vec::new();
    if !state.item_ok || state.matches {
        return out;
    }
    for side in Side::BOTH {
        let i = side.index();
        if state.missing[i].is_empty() {
            continue;
        }
        let crafted_out = state.crafted[0] >= state.crafted[1]
            && state.missing[i].iter().any(|&slot| {
                pattern.slots[i][slot as usize]
                    .iter()
                    .all(|&c| pattern.classes[c as usize].crafted())
            });
        if crafted_out {
            out.push(if state.metamods.is_empty() {
                Blocker::Crafted(side)
            } else {
                Blocker::Metamod(side)
            });
        }
        if state.open[i] == 0 {
            let fractured = affixes(state, side).iter().any(|a| a.fractured);
            out.push(if fractured {
                Blocker::Fractured(side)
            } else {
                Blocker::Side(side)
            });
        }
    }
    out
}

/// Whether a slot the pattern wants is empty and no step can fill it as the item stands: a side
/// is full, or the slot takes crafted modifiers only and the item already holds as many crafted
/// modifiers as it can. Something has to be removed (or rerolled) before that slot can be filled.
pub fn blocked(pattern: &Pattern, state: &PatternState) -> bool {
    !blockers(pattern, state).is_empty()
}

fn affixes(state: &PatternState, side: Side) -> &[pattern::AffixState] {
    match side {
        Side::Prefix => &state.prefixes,
        Side::Suffix => &state.suffixes,
    }
}

/// How a step's outcome was read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Model {
    /// Every outcome is computed.
    #[default]
    Exact,
    /// Some outcomes are computed and the rest are listed with the reason.
    CertainPart,
    /// Nothing about the result is known.
    Blind,
}

/// What a step can do for the goal, best first. A step that only holds the item where it is ranks
/// above one that only loses ground, and below everything that can move it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stands {
    /// It can finish the item.
    Finishes,
    /// It can bring the item further along.
    Advances,
    /// It cannot bring it along but can free a side that is full while a slot is missing.
    Unblocks,
    /// It leaves the item where it is.
    Holds,
    /// All it can do is lose ground.
    SetsBack,
}

/// Every number a report shows about one step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weighed {
    /// The chance of the outcomes this model computes (exact plus certain part).
    pub computed: f64,
    /// The chance of the outcomes it does not compute.
    pub uncomputed: f64,
    /// The chance the item ends matching the pattern.
    pub finish: f64,
    /// The expected progress of the computed outcomes; `1.0` is the goal.
    pub goal: f64,
    /// The chance the step leaves the item further along, and further behind.
    pub better: f64,
    pub worse: f64,
    /// The chance it leaves a side full with a slot the pattern wants still empty, and the chance
    /// the item was in that position before the step.
    pub blocked: f64,
    pub blocked_before: f64,
    /// The chance the item is destroyed.
    pub destroyed: f64,
    /// Distinct results: one per item, one per choice, one per pending tail.
    pub results: usize,
    pub model: Model,
}

/// A step's outcome weighed against a pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct Assessment {
    pub method: String,
    pub name: String,
    pub params: Params,
    pub cost: Vec<Cost>,
    pub weighed: Weighed,
    /// Why the model stopped before this step's result, when it did and said one reason. `None`
    /// for a computed step and for one whose leaves stop for several reasons.
    pub pending: Option<PendingReason>,
}

/// The item as it stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Now {
    /// The chance it already matches.
    pub matches: f64,
    /// Its progress, over every state `evaluate` gives.
    pub progress: f64,
    /// The chance a side is full with a slot still missing.
    pub blocked: f64,
}

impl Now {
    pub fn of(data: &Data, pattern: &Pattern, item: &Item) -> Now {
        let states = pattern::evaluate(data, pattern, item);
        Now {
            matches: zero(
                states
                    .iter()
                    .filter(|(_, s)| s.matches)
                    .map(|(p, _)| p)
                    .sum(),
            ),
            progress: zero(
                states
                    .iter()
                    .map(|(p, s)| p * state_progress(pattern, s))
                    .sum(),
            ),
            blocked: zero(
                states
                    .iter()
                    .filter(|(_, s)| blocked(pattern, s))
                    .map(|(p, _)| p)
                    .sum(),
            ),
        }
    }
}

/// Every step the item allows, in three groups, plus the methods it does not allow and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub now: Now,
    /// Every outcome computed, best first (`Weighed::rank`).
    pub exact: Vec<Assessment>,
    /// Ranked among themselves from the certain part only.
    pub certain: Vec<Assessment>,
    /// Not ranked: nothing about the result is known.
    pub blind: Vec<Assessment>,
    /// (method id, reason it does not apply), in method order.
    pub unmet: Vec<(String, String)>,
    /// Why the item is stuck as it stands, each kind once.
    pub blockers: Vec<Blocker>,
}

/// Sums one lifted outcome against a pattern.
#[derive(Clone, Copy, Debug, Default)]
struct Part {
    exact: f64,
    certain: f64,
    uncomputed: f64,
    finish: f64,
    goal: f64,
    better: f64,
    worse: f64,
    blocked: f64,
    destroyed: f64,
    results: usize,
}

impl Part {
    fn add(&mut self, p: f64, v: f64, and_blocked: bool, base: f64) {
        self.goal += p * v;
        if and_blocked {
            self.blocked += p;
        }
        if v > base + EPS {
            self.better += p;
        } else if v < base - EPS {
            self.worse += p;
        }
    }

    fn add_lifted(&mut self, pattern: &Pattern, lifted: &Lifted, base: f64) {
        for (p, leaf) in &lifted.leaves {
            self.results += 1;
            match leaf {
                LiftedLeaf::State(s) => {
                    self.exact += p;
                    self.add_state(pattern, *p, s, base);
                    if s.matches {
                        self.finish += p;
                    }
                }
                // A Mirror keeps both items; the goal is the item that was crafted.
                LiftedLeaf::Many(items) => match items.first() {
                    Some(first) => {
                        for (q, s) in first {
                            self.exact += p * q;
                            self.add_state(pattern, p * q, s, base);
                            if s.matches {
                                self.finish += p * q;
                            }
                        }
                    }
                    None => self.exact += p,
                },
                LiftedLeaf::Destroyed => {
                    self.exact += p;
                    self.destroyed += p;
                    self.worse += p;
                }
                // The player picks the best option, so the step is read by that option alone.
                LiftedLeaf::Choice(options) => {
                    let best = options
                        .iter()
                        .map(|o| {
                            let mut part = Part::default();
                            part.add_lifted(pattern, o, base);
                            part
                        })
                        .filter(|part| part.exact + part.certain > 0.0)
                        .max_by(|a, b| {
                            let rate = |p: &Part| p.goal / (p.exact + p.certain);
                            a.finish
                                .total_cmp(&b.finish)
                                .then_with(|| a.exact.total_cmp(&b.exact))
                                .then_with(|| rate(a).total_cmp(&rate(b)))
                        });
                    match best {
                        Some(best) => {
                            self.exact += p * best.exact;
                            self.certain += p * best.certain;
                            self.uncomputed += p * best.uncomputed;
                            self.finish += p * best.finish;
                            self.goal += p * best.goal;
                            self.better += p * best.better;
                            self.worse += p * best.worse;
                            self.blocked += p * best.blocked;
                            self.destroyed += p * best.destroyed;
                            self.results += best.results;
                        }
                        None => self.uncomputed += p,
                    }
                }
                LiftedLeaf::Pending { known: Some(s), .. } => {
                    self.certain += p;
                    self.add_state(pattern, *p, s, base);
                    if s.matches {
                        self.finish += p;
                    }
                }
                LiftedLeaf::Pending { known: None, .. } => self.uncomputed += p,
            }
        }
    }

    fn add_state(&mut self, pattern: &Pattern, p: f64, s: &PatternState, base: f64) {
        self.add(p, state_progress(pattern, s), blocked(pattern, s), base);
    }
}

/// Weighs a lifted outcome against a pattern, with `now` the item before the step.
pub fn weigh(pattern: &Pattern, lifted: &Lifted, now: &Now) -> Weighed {
    let mut part = Part::default();
    part.add_lifted(pattern, lifted, now.progress);
    let computed = part.exact + part.certain;
    Weighed {
        computed: zero(computed),
        uncomputed: zero(part.uncomputed),
        finish: zero(part.finish),
        goal: if computed > 0.0 {
            part.goal / computed
        } else {
            0.0
        },
        better: zero(part.better),
        worse: zero(part.worse),
        blocked: zero(part.blocked),
        blocked_before: now.blocked,
        destroyed: zero(part.destroyed),
        results: part.results,
        model: if part.uncomputed <= EPS && part.certain <= EPS {
            Model::Exact
        } else if computed > 0.0 {
            Model::CertainPart
        } else {
            Model::Blind
        },
    }
}

impl Assessment {
    /// What the step can do for the goal.
    pub fn stands(&self) -> Stands {
        let w = &self.weighed;
        if w.finish > EPS {
            Stands::Finishes
        } else if w.better > EPS {
            Stands::Advances
        } else if w.blocked + EPS < w.blocked_before && w.goal > EPS {
            // An item that no longer fits the pattern is not "unblocked": without a goal left,
            // the step only loses ground.
            Stands::Unblocks
        } else if w.worse > EPS || w.destroyed > EPS {
            Stands::SetsBack
        } else {
            Stands::Holds
        }
    }

    /// Whether two steps do the same thing to the goal, to the last digit: the report shows them
    /// together, because the ranking's only say between them is their id.
    pub fn tied(&self, other: &Assessment) -> bool {
        let (a, b) = (&self.weighed, &other.weighed);
        (a.finish - b.finish).abs() < EPS
            && (a.goal - b.goal).abs() < EPS
            && (a.worse - b.worse).abs() < EPS
            && (a.blocked - b.blocked).abs() < EPS
            && (a.better - b.better).abs() < EPS
    }

    /// The order the report shows: the best chance of finishing, then the expected goal, then the
    /// least chance of losing ground, then the least chance of ending blocked, then the method id
    /// so a report is reproducible.
    pub fn rank(&self, other: &Assessment) -> Ordering {
        let (a, b) = (&self.weighed, &other.weighed);
        self.stands()
            .cmp(&other.stands())
            .then_with(|| cmp_desc(a.finish, b.finish))
            .then_with(|| cmp_desc(a.goal, b.goal))
            .then_with(|| cmp_asc(a.worse, b.worse))
            .then_with(|| cmp_asc(a.blocked, b.blocked))
            .then_with(|| self.method.cmp(&other.method))
    }
}

/// Applies one step and weighs it. `Err` when the method does not apply to the item.
pub fn assess(
    data: &Data,
    pattern: &Pattern,
    item: &Item,
    method: &Method,
    params: &Params,
) -> Result<Assessment, Unmet> {
    let now = Now::of(data, pattern, item);
    assess_with(data, pattern, item, method, params, &now)
}

/// `assess` with the item already read, so a report reads it once.
fn assess_with(
    data: &Data,
    pattern: &Pattern,
    item: &Item,
    method: &Method,
    params: &Params,
    now: &Now,
) -> Result<Assessment, Unmet> {
    let outcome = apply::apply(data, method, std::slice::from_ref(item), params)?;
    // The reason the model stopped, when the leaves that stopped agree on it. A choice's option
    // is not looked into: the step is described by what its own leaves say.
    let mut reasons: Vec<&PendingReason> = Vec::new();
    for (_, leaf) in &outcome.leaves {
        if let apply::Leaf::Pending(p) = leaf {
            reasons.push(&p.reason);
        }
    }
    reasons.sort();
    reasons.dedup();
    let pending = match reasons.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    };
    let cost = apply::cost(data, method, params)?;
    let lifted = pattern::lift(data, pattern, &outcome);
    Ok(Assessment {
        method: method.id.clone(),
        name: method.name.clone(),
        params: params.clone(),
        cost,
        weighed: weigh(pattern, &lifted, now),
        pending,
    })
}

/// Every single-input method weighed against the item, in three groups sorted best first.
pub fn next_steps(data: &Data, pattern: &Pattern, item: &Item) -> Report {
    let params = Params::default();
    let now = Now::of(data, pattern, item);
    let mut reasons: Vec<Blocker> = Vec::new();
    for (p, s) in pattern::evaluate(data, pattern, item) {
        if p <= 0.0 {
            continue;
        }
        for b in blockers(pattern, &s) {
            if !reasons.contains(&b) {
                reasons.push(b);
            }
        }
    }
    let mut report = Report {
        now,
        exact: Vec::new(),
        certain: Vec::new(),
        blind: Vec::new(),
        unmet: Vec::new(),
        blockers: reasons,
    };
    for method in data.methods.iter().filter(|m| m.inputs == 1) {
        match assess_with(data, pattern, item, method, &params, &now) {
            Ok(a) => match a.weighed.model {
                Model::Exact => report.exact.push(a),
                Model::CertainPart => report.certain.push(a),
                Model::Blind => report.blind.push(a),
            },
            Err(unmet) => report.unmet.push((method.id.clone(), unmet.reason)),
        }
    }
    for group in [&mut report.exact, &mut report.certain, &mut report.blind] {
        group.sort_by(Assessment::rank);
    }
    report
}
