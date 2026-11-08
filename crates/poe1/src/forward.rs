//! Forward steps: apply one method to a node's items and group the results by the modifiers the
//! chart tracks.
//!
//! A node stands for a class of items (tracked modifier families, every crafted modifier and the
//! unrevealed veiled modifier, plus a count of the other modifiers per side) and, when it was
//! reached forward, a weighted sample of the items in that class. Applying a method to every
//! member gives the classes it can reach with their chances. `apply` answers every method
//! exactly except a reforge's `Fill`. A reforge that cannot land a tracked or named modifier is
//! counted exactly (only how many modifiers it adds on each side matters); any other reforge is
//! simulated with a fixed seed, so the same node and method always give the same numbers, and
//! its rows carry a 95% range. Claim `reforge.side-first` is the side choice in both; a tag
//! reforge (a Harvest "reforge with a Physical modifier") repeats until its result holds a
//! modifier with the tag (claim `reforge.guarantee-present`); and a Veiled Chaos Orb's veil then
//! replaces one of the modifiers the reforge added (claim `veiled-chaos.replaces-one`).

use std::collections::{BTreeMap, BTreeSet};

use crate::apply::{self, Leaf, Outcome, Params};
use crate::data::{Data, ModKind, Side};
use crate::item::{Affix, Item};
use crate::method::{Fill, Method, Op, Pool, SideReq, Weighting};
use crate::preimages::{AbsItem, Slot};

/// The most items a node keeps.
pub const MEMBER_CAP: usize = 1000;
/// The reforges a simulation runs.
pub const SIMULATIONS: u32 = 200_000;
/// The rerolls one simulation may spend on a tag reforge, over all its runs: a tag the reforge
/// rarely reaches stops the simulation here rather than hold the page.
const REROLLS: u64 = 20 * SIMULATIONS as u64;

/// The 95% range of a chance `p` measured over `n` runs (Wilson score), which stays honest
/// near 0 and 1: a chance no run hit still has an upper bound.
pub fn interval(p: f64, n: u32) -> [f64; 2] {
    let n = f64::from(n);
    let z2 = 1.96 * 1.96;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let half = 1.96 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    [(centre - half).max(0.0), (centre + half).min(1.0)]
}

/// The items a node stands for, with their chances given the node (they sum to 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub members: Vec<(Item, f64)>,
    pub approx: bool,
}

/// What a step reached: an item class, or a reason it stopped short.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Item(AbsItem),
    Destroyed,
    Refused(String),
    NotModelled(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub kind: Kind,
    /// The chance the method reaches this group given the node.
    pub p: f64,
    /// The 95% range when `p` comes from a simulation.
    pub interval: Option<[f64; 2]>,
    /// The items of the group, with their chances inside it.
    pub members: Vec<(Item, f64)>,
    /// The members are a simulation's sample rather than every item the group holds.
    pub sampled: bool,
}

/// Every group one method reaches from a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub method: String,
    pub groups: Vec<Group>,
    /// The reforges run, when the method was simulated.
    pub simulated: Option<u32>,
    /// The method can be used again on every result with the same odds (see `repeats`), so
    /// "how many tries" means something.
    pub repeatable: bool,
}

impl Kind {
    /// The key a page uses for this outcome.
    pub fn key(&self, data: &Data) -> String {
        match self {
            Kind::Item(a) => crate::preimages::abs_key(data, a),
            Kind::Destroyed => "destroyed".into(),
            Kind::Refused(why) => format!("refused:{why}"),
            Kind::NotModelled(why) => format!("pending:{why}"),
        }
    }
}

/// `node_key` and `method_id`, hashed into a seed: same chart, same numbers.
pub fn seed_for(node_key: &str, method_id: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in format!("{node_key}|{method_id}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// SplitMix64: a seeded generator, so a simulation repeats.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.unit() * n as f64) as usize % n
        }
    }
}

/// A weighted list drawn by cumulative weight.
struct Weighted {
    mods: Vec<u32>,
    cumulative: Vec<f64>,
    total: f64,
}

impl Weighted {
    fn new(entries: &[(u32, f64)]) -> Weighted {
        let mut mods = Vec::with_capacity(entries.len());
        let mut cumulative = Vec::with_capacity(entries.len());
        let mut total = 0.0;
        for &(m, w) in entries {
            total += w;
            mods.push(m);
            cumulative.push(total);
        }
        Weighted {
            mods,
            cumulative,
            total,
        }
    }

    fn is_empty(&self) -> bool {
        self.mods.is_empty() || self.total <= 0.0
    }

    fn draw(&self, rng: &mut SplitMix64) -> Option<u32> {
        if self.is_empty() {
            return None;
        }
        let x = rng.unit() * self.total;
        let index = self
            .cumulative
            .partition_point(|c| *c < x)
            .min(self.mods.len() - 1);
        Some(self.mods[index])
    }

    /// The list without the modifiers a group bars, built exactly.
    fn without_groups(&self, data: &Data, groups: &BTreeSet<u16>) -> Weighted {
        let mut kept: Vec<(u32, f64)> = Vec::new();
        let mut previous = 0.0;
        for (m, c) in self.mods.iter().zip(&self.cumulative) {
            let w = *c - previous;
            previous = *c;
            if !data.mod_at(*m).groups.iter().any(|g| groups.contains(g)) {
                kept.push((*m, w));
            }
        }
        Weighted::new(&kept)
    }
}

/// The class `item` belongs to: the tracked slots it fills, the crafted and veiled modifiers it
/// holds, and how many other modifiers sit on each side.
pub fn classify(data: &Data, tracked: &[Slot], item: &Item) -> AbsItem {
    let mut out = item.clone();
    out.prefixes.clear();
    out.suffixes.clear();
    out.implicits.clear();
    out.enchants.clear();
    out.quality = 0;
    out.catalyst = None;
    out.defences = None;
    let mut any = [0u8; 2];
    let mut better = BTreeSet::new();
    let mut used: BTreeSet<u32> = BTreeSet::new();
    for side in Side::BOTH {
        for affix in item.affixes(side) {
            let m = affix.roll.m;
            match tracked
                .iter()
                .find(|slot| !used.contains(&slot.m) && slot.matches(data, m))
            {
                Some(slot) => {
                    used.insert(slot.m);
                    out.affixes_mut(slot.side)
                        .push(crate::item::Affix::new(slot.m));
                    if slot.or_better {
                        better.insert(slot.m);
                    }
                }
                None if named(data, m) => out.affixes_mut(side).push(crate::item::Affix::new(m)),
                None => any[side.index()] += 1,
            }
        }
    }
    out.canonical(data);
    AbsItem {
        item: out,
        any,
        better,
    }
}

/// Whether a class names `m` though no slot tracks it: a crafted modifier or the unrevealed veiled
/// placeholder. Both are placed on purpose and change what later methods do (a metamod's lock,
/// the bench's one-craft limit, an Unveil), so two items that differ in one are different nodes.
fn named(data: &Data, m: u32) -> bool {
    matches!(data.mod_at(m).kind, ModKind::Crafted | ModKind::Veiled)
}

/// The fill a method ends with, when the model can run it here.
fn single_fill(method: &Method) -> Option<&Fill> {
    match method.ops.last() {
        Some(Op::Fill { fill }) if fill.weighting == Weighting::Normal => Some(fill),
        _ => None,
    }
}

/// Every group `method` reaches from `node`.
pub fn apply_node(
    data: &Data,
    node: &Node,
    method: &Method,
    tracked: &[Slot],
    seed: u64,
) -> Applied {
    let Taken { reps, refused } = grouped(data, method, node);
    let runs: Vec<(f64, Result<Outcome, String>)> = reps
        .iter()
        .map(|(item, weight)| {
            let outcome =
                apply::apply(data, method, std::slice::from_ref(item), &Params::default())
                    .map_err(|u| u.reason);
            (*weight, outcome)
        })
        .collect();
    let mut applied = match single_fill(method) {
        Some(fill) => simulate(data, &runs, &refused, method, fill, tracked, seed),
        None => exact(data, &runs, &refused, method, tracked),
    };
    applied.repeatable = repeatable(data, method, &reps, &applied.groups);
    applied
}

fn exact(
    data: &Data,
    runs: &[(f64, Result<Outcome, String>)],
    refused: &[(String, f64)],
    method: &Method,
    tracked: &[Slot],
) -> Applied {
    let mut groups: BTreeMap<Kind, Group> = BTreeMap::new();
    for (why, weight) in refused {
        push(&mut groups, Kind::Refused(why.clone()), *weight, None, None);
    }
    for (weight, outcome) in runs {
        match outcome {
            Err(why) => push(&mut groups, Kind::Refused(why.clone()), *weight, None, None),
            Ok(outcome) => {
                for (p, leaf) in &outcome.leaves {
                    let (kind, item) = leaf_group(data, tracked, leaf);
                    push(&mut groups, kind, weight * p, item.as_ref(), None);
                }
            }
        }
    }
    Applied {
        method: method.id.clone(),
        groups: finish(groups),
        simulated: None,
        repeatable: false,
    }
}

/// The group a leaf the model finished falls in, and the item it leaves, in canonical form. A
/// pending leaf is not modelled here; `simulate` runs the reforges it can.
fn leaf_group(data: &Data, tracked: &[Slot], leaf: &Leaf) -> (Kind, Option<Item>) {
    let item = match leaf {
        Leaf::Item(item) => item,
        Leaf::Items(items) => match items.first() {
            Some(item) => item,
            None => return (Kind::NotModelled("the method leaves no item".into()), None),
        },
        Leaf::Destroyed => return (Kind::Destroyed, None),
        Leaf::Choice(_) => {
            return (
                Kind::NotModelled("the player chooses among several results".into()),
                None,
            );
        }
        Leaf::Pending(pending) => return (Kind::NotModelled(pending.reason.text()), None),
    };
    let mut item = item.clone();
    item.canonical(data);
    (Kind::Item(classify(data, tracked, &item)), Some(item))
}

fn push_item(
    data: &Data,
    tracked: &[Slot],
    groups: &mut BTreeMap<Kind, Group>,
    item: &Item,
    mass: f64,
) {
    let mut item = item.clone();
    item.canonical(data);
    let kind = Kind::Item(classify(data, tracked, &item));
    push(groups, kind, mass, Some(&item), None);
}

fn push(
    groups: &mut BTreeMap<Kind, Group>,
    kind: Kind,
    mass: f64,
    item: Option<&Item>,
    interval: Option<[f64; 2]>,
) {
    let group = groups.entry(kind).or_insert_with(|| Group {
        kind: Kind::Destroyed,
        p: 0.0,
        interval,
        members: Vec::new(),
        sampled: false,
    });
    group.p += mass;
    if interval.is_some() {
        group.interval = interval;
    }
    if let Some(item) = item {
        group.members.push((item.clone(), mass));
    }
}

/// Merges a group's members by item, keeps their chances inside the group, and sorts the groups.
fn finish(mut groups: BTreeMap<Kind, Group>) -> Vec<Group> {
    let mut out: Vec<Group> = groups
        .iter_mut()
        .map(|(kind, group)| {
            let mut by_item: BTreeMap<Item, f64> = BTreeMap::new();
            for (item, mass) in group.members.drain(..) {
                *by_item.entry(item).or_default() += mass;
            }
            let total: f64 = by_item.values().sum();
            let members = by_item
                .into_iter()
                .map(|(item, mass)| (item, if total > 0.0 { mass / total } else { 0.0 }))
                .collect();
            Group {
                kind: kind.clone(),
                p: group.p,
                interval: group.interval,
                members,
                sampled: group.sampled,
            }
        })
        .collect();
    out.sort_by(|a, b| b.p.total_cmp(&a.p).then_with(|| a.kind.cmp(&b.kind)));
    out
}

fn simulate(
    data: &Data,
    runs: &[(f64, Result<Outcome, String>)],
    refused: &[(String, f64)],
    method: &Method,
    fill: &Fill,
    tracked: &[Slot],
    seed: u64,
) -> Applied {
    let mut rng = SplitMix64::new(seed);
    // What each run draws from: a member's outcome, or why the method refused it.
    let entries: Vec<(f64, Result<&Outcome, &str>)> = runs
        .iter()
        .map(|(weight, outcome)| (*weight, outcome.as_ref().map_err(String::as_str)))
        .chain(
            refused
                .iter()
                .map(|(why, weight)| (*weight, Err(why.as_str()))),
        )
        .collect();
    let mut cumulative = Vec::with_capacity(entries.len());
    let mut total = 0.0;
    for (weight, _) in &entries {
        total += weight;
        cumulative.push(total);
    }
    let mut groups: BTreeMap<Kind, Group> = BTreeMap::new();
    let mut lists: BTreeMap<Item, [Weighted; 2]> = BTreeMap::new();
    let mut includes: BTreeMap<Item, Option<BTreeSet<u32>>> = BTreeMap::new();
    let mut rerolls = 0u64;
    let one = 1.0 / f64::from(SIMULATIONS);
    for _ in 0..SIMULATIONS {
        let x = rng.unit() * total;
        let index = cumulative
            .partition_point(|c| *c < x)
            .min(entries.len() - 1);
        let outcome = match entries[index].1 {
            Err(why) => {
                push(&mut groups, Kind::Refused(why.to_string()), one, None, None);
                continue;
            }
            Ok(outcome) => outcome,
        };
        let pick = rng.unit();
        let mut acc = 0.0;
        let Some((_, leaf)) = outcome.leaves.iter().find(|(p, _)| {
            acc += p;
            acc >= pick
        }) else {
            continue;
        };
        match leaf {
            Leaf::Pending(pending) if reforges(pending) => {
                let include = match &fill.include {
                    Some(pool) => includes
                        .entry(pending.item.clone())
                        .or_insert_with(|| include_set(data, &pending.item, fill, pool))
                        .as_ref()
                        .map(Some)
                        .ok_or("no modifier with the reforge's tag can roll on this item"),
                    None => Ok(None),
                };
                let reforged = include.map_err(String::from).and_then(|include| {
                    reforge(
                        data,
                        &pending.item,
                        fill,
                        &mut lists,
                        include,
                        &mut rerolls,
                        &mut rng,
                    )
                });
                match reforged {
                    Ok(item) => push_item(data, tracked, &mut groups, &item, one),
                    Err(why) => push(&mut groups, Kind::NotModelled(why), one, None, None),
                }
            }
            leaf => {
                let (kind, item) = leaf_group(data, tracked, leaf);
                push(&mut groups, kind, one, item.as_ref(), None);
            }
        }
    }
    let mut out = finish(groups);
    for group in &mut out {
        group.sampled = true;
    }
    // The simulation still gives the members; the exact counts, when they hold, give the chances.
    let exact = exact_split(data, runs, refused, fill, tracked)
        .filter(|exact| out.iter().all(|g| exact.contains_key(&g.kind)));
    let simulated = match exact {
        Some(exact) => {
            for group in &mut out {
                group.p = exact[&group.kind];
                group.interval = None;
            }
            for (kind, p) in exact {
                if p > 0.0 && !out.iter().any(|g| g.kind == kind) {
                    out.push(Group {
                        kind,
                        p,
                        interval: None,
                        members: Vec::new(),
                        sampled: true,
                    });
                }
            }
            None
        }
        None => {
            for group in &mut out {
                group.interval = Some(interval(group.p, SIMULATIONS));
            }
            Some(SIMULATIONS)
        }
    };
    out.sort_by(|a, b| b.p.total_cmp(&a.p).then_with(|| a.kind.cmp(&b.kind)));
    Applied {
        method: method.id.clone(),
        groups: out,
        simulated,
        repeatable: false,
    }
}

/// Whether a pending leaf is a reforge the model runs: nothing left but the fill, which only
/// adds to the item as it stands.
fn reforges(pending: &apply::Pending) -> bool {
    pending.certain && matches!(pending.ops.as_slice(), [Op::Fill { .. }])
}

/// The item a method's leading clear leaves, and whether it removed anything: `Some` when the
/// method changes nothing but the rarity before it clears both sides, so what follows depends on
/// that item alone.
fn cleared_key(data: &Data, method: &Method, item: &Item) -> Option<(Item, bool)> {
    let clear = method
        .ops
        .iter()
        .position(|op| matches!(op, Op::Clear { side: SideReq::Any }))?;
    if !method.ops[..clear]
        .iter()
        .all(|op| matches!(op, Op::SetRarity { .. }))
    {
        return None;
    }
    let guard = apply::guard_for(data, method, item);
    let mut out = item.clone();
    for side in Side::BOTH {
        out.affixes_mut(side)
            .retain(|a| !guard.can_change(data, side, a));
    }
    let removed = out.explicits().count() != item.explicits().count();
    out.canonical(data);
    Some((out, removed))
}

/// A node's members as one method takes them (`grouped`).
struct Taken {
    /// One member per item the method's leading clear leaves, with the weight of every member
    /// that clears to it.
    reps: Vec<(Item, f64)>,
    /// The weight the method refuses, by reason.
    refused: Vec<(String, f64)>,
}

/// The node's members `method` will take, merged where its leading clear leaves the same item,
/// and the weight it refuses. The requirements are checked on every member; the ops then run
/// once per representative.
fn grouped(data: &Data, method: &Method, node: &Node) -> Taken {
    let mut reps: Vec<(Item, f64)> = Vec::new();
    let mut index: BTreeMap<(Item, bool), usize> = BTreeMap::new();
    let mut refused: BTreeMap<String, f64> = BTreeMap::new();
    for (item, weight) in &node.members {
        let inputs = std::slice::from_ref(item);
        if let Err(unmet) = apply::requirements(data, method, inputs, &Params::default()) {
            *refused.entry(unmet.reason).or_default() += weight;
            continue;
        }
        match cleared_key(data, method, item) {
            Some(key) => match index.get(&key) {
                Some(&i) => reps[i].1 += weight,
                None => {
                    index.insert(key, reps.len());
                    reps.push((item.clone(), *weight));
                }
            },
            None => reps.push((item.clone(), *weight)),
        }
    }
    Taken {
        reps,
        refused: refused.into_iter().collect(),
    }
}

/// Whether `method` clears every modifier it may change before it adds any.
fn clears(method: &Method) -> bool {
    method
        .ops
        .iter()
        .any(|op| matches!(op, Op::Clear { side: SideReq::Any }))
}

/// The explicit modifiers `method` keeps on `item` (fractured, or on a side a respected
/// metamod locks), as sorted (side, modifier) pairs.
fn kept(data: &Data, method: &Method, item: &Item) -> Vec<(usize, u32)> {
    let guard = apply::guard_for(data, method, item);
    let guard = &guard;
    let mut out: Vec<(usize, u32)> = Side::BOTH
        .into_iter()
        .flat_map(|side| {
            item.affixes(side)
                .iter()
                .filter(move |a| !guard.can_change(data, side, a))
                .map(move |a| (side.index(), a.roll.m))
        })
        .collect();
    out.sort_unstable();
    out
}

/// Whether `method` used on `before` gave `after` an item it can be used on again with the same
/// odds: it clears every modifier it may change before it adds, `after` meets its requirements,
/// and `after` keeps exactly `before`'s protected modifiers.
pub fn repeats(data: &Data, method: &Method, before: &Item, after: &Item) -> bool {
    clears(method)
        && apply::requirements(
            data,
            method,
            std::slice::from_ref(after),
            &Params::default(),
        )
        .is_ok()
        && kept(data, method, before) == kept(data, method, after)
}

/// Whether every result of the step repeats it (`repeats`), and every group is an item. Every
/// representative and every member must keep the same protected modifiers. The requirements are
/// checked on each group's heaviest member: a group is one class, which fixes the rarity, the
/// flags, the named modifiers and the counts they read, and a simulated group can hold 200,000
/// sampled items.
fn repeatable(data: &Data, method: &Method, reps: &[(Item, f64)], groups: &[Group]) -> bool {
    let Some((first, _)) = reps.first() else {
        return false;
    };
    let same = kept(data, method, first);
    let meets = |item: &Item| {
        apply::requirements(data, method, std::slice::from_ref(item), &Params::default()).is_ok()
    };
    clears(method)
        && groups.iter().all(|g| matches!(g.kind, Kind::Item(_)))
        && reps.iter().all(|(rep, _)| kept(data, method, rep) == same)
        && groups
            .iter()
            .flat_map(|g| &g.members)
            .all(|(item, _)| kept(data, method, item) == same)
        && groups.iter().all(|g| {
            g.members
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .is_none_or(|(item, _)| meets(item))
        })
}

/// The exact chance of every group a reforge reaches, when no modifier it can add is tracked or
/// named (so each one only counts toward its side) and its lists cannot run out: `None`
/// otherwise, and the step stays simulated. It follows `fill_once` draw for draw: the total by
/// weight, then each addition on a side picked evenly among those with room and candidates.
fn exact_split(
    data: &Data,
    runs: &[(f64, Result<Outcome, String>)],
    refused: &[(String, f64)],
    fill: &Fill,
    tracked: &[Slot],
) -> Option<BTreeMap<Kind, f64>> {
    if fill.include.is_some() || fill.side != SideReq::Any {
        return None;
    }
    let total: u32 = fill.counts.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return None;
    }
    let mut out: BTreeMap<Kind, f64> = BTreeMap::new();
    for (why, weight) in refused {
        *out.entry(Kind::Refused(why.clone())).or_default() += weight;
    }
    for (weight, outcome) in runs {
        let outcome = match outcome {
            Err(why) => {
                *out.entry(Kind::Refused(why.clone())).or_default() += weight;
                continue;
            }
            Ok(outcome) => outcome,
        };
        for (q, leaf) in &outcome.leaves {
            let mass = weight * q;
            let pending = match leaf {
                Leaf::Pending(pending) if reforges(pending) => pending,
                leaf => {
                    *out.entry(leaf_group(data, tracked, leaf).0).or_default() += mass;
                    continue;
                }
            };
            let item = &pending.item;
            let lists = exact_lists(data, item, fill, tracked)?;
            let base = classify(data, tracked, item);
            for &(n, w) in &fill.counts {
                let share = mass * f64::from(w) / f64::from(total);
                let additions = fill.additions(data, item, n);
                for (added, d) in spread(data, item, &lists, additions) {
                    for (kind, x) in reforged(data, &base, added, fill.one_veiled) {
                        *out.entry(kind).or_default() += share * d * x;
                    }
                }
            }
        }
    }
    Some(out)
}

/// The modifiers a reforge of `item` can add on each side, when counting them is enough: none is
/// tracked or named, no group spans both sides, and no side's list can run out before its room
/// does (the room − 1 modifiers that bar the most leave one candidate).
fn exact_lists(data: &Data, item: &Item, fill: &Fill, tracked: &[Slot]) -> Option<[Vec<u32>; 2]> {
    let held = item.explicit_groups(data);
    let lists: [Vec<u32>; 2] = side_entries(data, item, fill).map(|entries| {
        entries
            .into_iter()
            .filter(|(m, w)| *w > 0.0 && !data.mod_at(*m).groups.iter().any(|g| held.contains(g)))
            .map(|(m, _)| m)
            .collect()
    });
    let counted = |m: u32| named(data, m) || tracked.iter().any(|t| t.matches(data, m));
    if lists.iter().flatten().any(|&m| counted(m)) {
        return None;
    }
    let groups = |list: &[u32]| -> BTreeSet<u16> {
        list.iter()
            .flat_map(|&m| data.mod_at(m).groups.iter().copied())
            .collect()
    };
    if !groups(&lists[0]).is_disjoint(&groups(&lists[1])) {
        return None;
    }
    for side in Side::BOTH {
        let list = &lists[side.index()];
        let room = usize::from(item.open(data, side));
        if room == 0 || list.is_empty() {
            continue;
        }
        let mut barred: Vec<usize> = list
            .iter()
            .map(|&m| {
                let own = &data.mod_at(m).groups;
                list.iter()
                    .filter(|&&o| o == m || data.mod_at(o).groups.iter().any(|g| own.contains(g)))
                    .count()
            })
            .collect();
        barred.sort_unstable_by(|a, b| b.cmp(a));
        if barred.iter().take(room - 1).sum::<usize>() >= list.len() {
            return None;
        }
    }
    Some(lists)
}

/// The chance of each (prefixes, suffixes) count `additions` draws add to `item`: each draw
/// picks a side evenly among those with room and candidates, and stops when none has both
/// (claim `reforge.side-first`).
fn spread(
    data: &Data,
    item: &Item,
    lists: &[Vec<u32>; 2],
    additions: usize,
) -> BTreeMap<[u8; 2], f64> {
    let open = Side::BOTH.map(|side| item.open(data, side));
    let mut dist: BTreeMap<[u8; 2], f64> = BTreeMap::from([([0, 0], 1.0)]);
    for _ in 0..additions {
        let mut next: BTreeMap<[u8; 2], f64> = BTreeMap::new();
        for (added, p) in dist {
            let sides: Vec<usize> = [0usize, 1]
                .into_iter()
                .filter(|&s| open[s] > added[s] && !lists[s].is_empty())
                .collect();
            if sides.is_empty() {
                *next.entry(added).or_default() += p;
                continue;
            }
            let share = p / sides.len() as f64;
            for s in sides {
                let mut more = added;
                more[s] += 1;
                *next.entry(more).or_default() += share;
            }
        }
        dist = next;
    }
    dist
}

/// The classes a reforge that added `added` (prefixes, suffixes) untracked modifiers to `base`'s
/// item reaches, with their chances: one class, or with a Veiled Chaos Orb's veil one per side
/// it can land on, in proportion to the modifiers added there (`veil`).
fn reforged(data: &Data, base: &AbsItem, added: [u8; 2], one_veiled: bool) -> Vec<(Kind, f64)> {
    let mut plain = base.clone();
    plain.any[0] += added[0];
    plain.any[1] += added[1];
    if !one_veiled {
        return vec![(Kind::Item(plain), 1.0)];
    }
    let total = f64::from(added[0]) + f64::from(added[1]);
    if total == 0.0 {
        let why = "the reforge added no modifier for the veil to replace";
        return vec![(Kind::NotModelled(why.into()), 1.0)];
    }
    Side::BOTH
        .into_iter()
        .filter(|side| added[side.index()] > 0)
        .map(|side| {
            let p = f64::from(added[side.index()]) / total;
            let Some(placeholder) = data.mod_by_id(apply::VEILED[side.index()]) else {
                let why = "the bundle lacks a veiled placeholder modifier";
                return (Kind::NotModelled(why.into()), p);
            };
            let mut class = plain.clone();
            class.any[side.index()] -= 1;
            class.item.affixes_mut(side).push(Affix::new(placeholder));
            class.item.canonical(data);
            (Kind::Item(class), p)
        })
        .collect()
}

/// One reforge of a pending item: the fill, drawn again while a tag reforge's result holds no
/// modifier of its tag (claim `reforge.guarantee-present`: the reforge repeats rather than place
/// the tagged modifier first), then a Veiled Chaos Orb's veil.
fn reforge(
    data: &Data,
    item: &Item,
    fill: &Fill,
    lists: &mut BTreeMap<Item, [Weighted; 2]>,
    include: Option<&BTreeSet<u32>>,
    rerolls: &mut u64,
    rng: &mut SplitMix64,
) -> Result<Item, String> {
    loop {
        let (mut out, added) = fill_once(data, item, fill, lists, rng);
        if include.is_none_or(|set| out.explicits().any(|a| set.contains(&a.roll.m))) {
            if fill.one_veiled {
                veil(data, &mut out, &added, rng)?;
            }
            return Ok(out);
        }
        *rerolls += 1;
        if *rerolls > REROLLS {
            return Err(format!(
                "the reforge rarely reaches its tag: the simulation stopped after {REROLLS} rerolls"
            ));
        }
    }
}

/// The modifiers a tag reforge on `item` counts as its tag: the tag's pool on the base, within
/// the fill's level cap. `None` when the item holds none and none can roll into its open slots,
/// so the reforge could never stop.
fn include_set(data: &Data, item: &Item, fill: &Fill, pool: &Pool) -> Option<BTreeSet<u32>> {
    let set: BTreeSet<u32> = apply::pool_candidates(data, item, pool, fill.side, true, true)
        .into_iter()
        .map(|(m, _)| m)
        .filter(|m| {
            fill.mod_level_max
                .is_none_or(|cap| data.mod_at(*m).level <= cap)
        })
        .collect();
    let held = item.explicits().any(|a| set.contains(&a.roll.m));
    let rollable = set
        .iter()
        .any(|m| data.mod_at(*m).side.is_some_and(|s| item.open(data, s) > 0));
    (held || rollable).then_some(set)
}

/// One reforge: draw the total, then fill the open slots one modifier at a time, each draw
/// picking a side with room uniformly (claim `reforge.side-first`). Returns the item and the
/// modifiers it added, in draw order.
fn fill_once(
    data: &Data,
    item: &Item,
    fill: &Fill,
    lists: &mut BTreeMap<Item, [Weighted; 2]>,
    rng: &mut SplitMix64,
) -> (Item, Vec<u32>) {
    let n = draw_count(&fill.counts, rng);
    let additions = fill.additions(data, item, n);
    let mut cur = item.clone();
    let mut added = Vec::new();
    if lists.get(item).is_none() {
        let all = data_sides(data, item, fill);
        lists.insert(item.clone(), all);
    }
    for _ in 0..additions {
        let mut open: Vec<usize> = [0usize, 1]
            .into_iter()
            .filter(|&s| cur.open(data, Side::BOTH[s]) > 0)
            .collect();
        let mut placed = false;
        while !open.is_empty() {
            let s = open[rng.below(open.len())];
            let pool = &lists[item][s];
            if pool.is_empty() {
                open.retain(|x| *x != s);
                continue;
            }
            let groups = cur.explicit_groups(data);
            let mut chosen = None;
            for _ in 0..64 {
                let m = pool.draw(rng).expect("a non-empty pool draws");
                if !data.mod_at(m).groups.iter().any(|g| groups.contains(g)) {
                    chosen = Some(m);
                    break;
                }
            }
            let m = match chosen {
                Some(m) => m,
                None => {
                    let filtered = pool.without_groups(data, &groups);
                    match filtered.draw(rng) {
                        Some(m) => m,
                        None => {
                            open.retain(|x| *x != s);
                            continue;
                        }
                    }
                }
            };
            apply::add_affix(data, &mut cur, m);
            added.push(m);
            placed = true;
            break;
        }
        if !placed {
            break;
        }
    }
    (cur, added)
}

/// A Veiled Chaos Orb's veil (claim `veiled-chaos.replaces-one`): one of the modifiers the
/// reforge added becomes the veiled placeholder of that modifier's side. The claim names no rule
/// for which one, so each added modifier is taken as equally likely.
fn veil(
    data: &Data,
    item: &mut Item,
    added: &[u32],
    rng: &mut SplitMix64,
) -> Result<(), &'static str> {
    if added.is_empty() {
        return Err("the reforge added no modifier for the veil to replace");
    }
    let m = added[rng.below(added.len())];
    let Some(side) = data.mod_at(m).side else {
        return Err("the veiled modifier has no side");
    };
    let Some(placeholder) = data.mod_by_id(apply::VEILED[side.index()]) else {
        return Err("the bundle lacks a veiled placeholder modifier");
    };
    item.affixes_mut(side).retain(|a| a.roll.m != m);
    apply::add_affix(data, item, placeholder);
    Ok(())
}

/// The fill's pool on `item`, split by side, with the level cap applied.
fn data_sides(data: &Data, item: &Item, fill: &Fill) -> [Weighted; 2] {
    side_entries(data, item, fill).map(|entries| Weighted::new(&entries))
}

/// The fill's weighted candidates on `item` for each side, within its level cap.
fn side_entries(data: &Data, item: &Item, fill: &Fill) -> [Vec<(u32, f64)>; 2] {
    let entries = apply::pool_candidates(data, item, &Pool::Natural, fill.side, true, true);
    Side::BOTH.map(|side| {
        entries
            .iter()
            .copied()
            .filter(|(m, _)| data.mod_at(*m).side == Some(side))
            .filter(|(m, _)| {
                fill.mod_level_max
                    .is_none_or(|cap| data.mod_at(*m).level <= cap)
            })
            .collect()
    })
}

/// A total from `counts` (`[(4, 8), (5, 3), (6, 1)]`) by weight.
fn draw_count(counts: &[(u8, u32)], rng: &mut SplitMix64) -> u8 {
    let total: u32 = counts.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return 4;
    }
    let mut x = rng.unit() * f64::from(total);
    for &(n, w) in counts {
        x -= f64::from(w);
        if x <= 0.0 {
            return n;
        }
    }
    counts.last().map_or(4, |(n, _)| *n)
}

/// The node a group becomes: its members, resampled to `MEMBER_CAP` when there are more.
pub fn child(group: &Group, seed: u64) -> Node {
    let mut members = group.members.clone();
    let total: f64 = members.iter().map(|(_, w)| *w).sum();
    if total > 0.0 {
        for (_, w) in &mut members {
            *w /= total;
        }
    }
    if members.len() <= MEMBER_CAP {
        return Node {
            members,
            approx: group.sampled,
        };
    }
    let mut rng = SplitMix64::new(seed);
    let step = 1.0 / MEMBER_CAP as f64;
    let mut cumulative = Vec::with_capacity(members.len());
    let mut acc = 0.0;
    for (_, w) in &members {
        acc += w;
        cumulative.push(acc);
    }
    let mut out: BTreeMap<Item, f64> = BTreeMap::new();
    for i in 0..MEMBER_CAP {
        let x = (i as f64 + rng.unit()) * step;
        let index = cumulative
            .partition_point(|c| *c < x)
            .min(members.len() - 1);
        *out.entry(members[index].0.clone()).or_default() += step;
    }
    Node {
        members: out.into_iter().collect(),
        approx: true,
    }
}

/// A node for a class with no members of its own (one reached backward): `MEMBER_CAP` samples
/// whose untracked modifiers and family tiers are drawn from the natural pool.
pub fn realise(data: &Data, abs: &AbsItem, tracked: &[Slot], seed: u64) -> Node {
    let mut base = abs.item.clone();
    base.prefixes.clear();
    base.suffixes.clear();
    base.quality = 0;
    base.catalyst = None;
    base.defences = None;
    let pool = apply::pool_candidates(data, &base, &Pool::Natural, SideReq::Any, true, true);
    let mut rng = SplitMix64::new(seed);
    let mut out: BTreeMap<Item, f64> = BTreeMap::new();
    let step = 1.0 / MEMBER_CAP as f64;
    for _ in 0..MEMBER_CAP {
        let mut cur = base.clone();
        for side in Side::BOTH {
            for affix in abs.item.affixes(side) {
                let m = affix.roll.m;
                let slot = tracked.iter().find(|slot| slot.m == m);
                let pick = match slot {
                    Some(slot) if slot.or_better => {
                        let entries: Vec<(u32, f64)> = pool
                            .iter()
                            .copied()
                            .filter(|(m, _)| slot.matches(data, *m))
                            .collect();
                        Weighted::new(&entries).draw(&mut rng).unwrap_or(m)
                    }
                    _ => m,
                };
                apply::add_affix(data, &mut cur, pick);
            }
        }
        for side in Side::BOTH {
            for _ in 0..abs.any[side.index()] {
                let entries: Vec<(u32, f64)> = pool
                    .iter()
                    .copied()
                    .filter(|(m, _)| data.mod_at(*m).side == Some(side))
                    .filter(|(m, _)| !tracked.iter().any(|slot| slot.matches(data, *m)))
                    .collect();
                let groups = cur.explicit_groups(data);
                let allowed: Vec<(u32, f64)> = entries
                    .into_iter()
                    .filter(|(m, _)| !data.mod_at(*m).groups.iter().any(|g| groups.contains(g)))
                    .collect();
                match Weighted::new(&allowed).draw(&mut rng) {
                    Some(m) => {
                        apply::add_affix(data, &mut cur, m);
                    }
                    None => break,
                }
            }
        }
        cur.canonical(data);
        if classify(data, tracked, &cur) == *abs {
            *out.entry(cur).or_default() += step;
        }
    }
    if out.is_empty() {
        return Node {
            members: vec![(abs.item.clone(), 1.0)],
            approx: true,
        };
    }
    Node {
        members: out.into_iter().collect(),
        approx: true,
    }
}
