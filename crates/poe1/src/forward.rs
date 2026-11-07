//! Forward steps: apply one method to a node's items and group the results by the modifiers the
//! chart tracks.
//!
//! A node stands for a class of items (tracked modifier families plus a count of untracked
//! modifiers per side) and, when it was reached forward, a weighted sample of the items in that
//! class. Applying a method to every member gives the classes it can reach with their chances.
//! `apply` answers every method exactly except a reforge's `Fill`; a reforge is simulated with a
//! fixed seed, so the same node and method always give the same numbers, and its rows carry a
//! 95% range. Claim `reforge.side-first` is the side choice inside the simulation.

use std::collections::{BTreeMap, BTreeSet};

use crate::apply::{self, Leaf, Outcome, Params};
use crate::data::{Data, Side};
use crate::item::Item;
use crate::method::{Fill, Method, Op, Pool, SideReq, Weighting};
use crate::preimages::{AbsItem, Slot};

/// The most items a node keeps.
pub const MEMBER_CAP: usize = 1000;
/// The reforges a simulation runs.
pub const SIMULATIONS: u32 = 200_000;

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
}

/// Every group one method reaches from a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub method: String,
    pub groups: Vec<Group>,
    /// The reforges run, when the method was simulated.
    pub simulated: Option<u32>,
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

/// The class `item` belongs to: the tracked slots it fills, and how many untracked modifiers sit
/// on each side.
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

/// The fill a method ends with, when the model can run it here.
fn single_fill(method: &Method) -> Option<&Fill> {
    match method.ops.last() {
        Some(Op::Fill { fill })
            if fill.weighting == Weighting::Normal
                && fill.include.is_none()
                && !fill.one_veiled =>
        {
            Some(fill)
        }
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
    match single_fill(method) {
        Some(fill) => simulate(data, node, method, fill, tracked, seed),
        None => exact(data, node, method, tracked),
    }
}

fn exact(data: &Data, node: &Node, method: &Method, tracked: &[Slot]) -> Applied {
    let mut groups: BTreeMap<Kind, Group> = BTreeMap::new();
    for (item, weight) in &node.members {
        let outcome = apply::apply(data, method, std::slice::from_ref(item), &Params::default());
        add_outcome(data, tracked, &mut groups, outcome, *weight);
    }
    Applied {
        method: method.id.clone(),
        groups: finish(groups),
        simulated: None,
    }
}

/// Folds one `apply` outcome into the groups, `weight` being the share of the node that reached
/// it.
fn add_outcome(
    data: &Data,
    tracked: &[Slot],
    groups: &mut BTreeMap<Kind, Group>,
    outcome: Result<Outcome, apply::Unmet>,
    weight: f64,
) {
    match outcome {
        Err(unmet) => push(groups, Kind::Refused(unmet.reason), weight, None, None),
        Ok(outcome) => {
            for (p, leaf) in &outcome.leaves {
                let mass = weight * p;
                match leaf {
                    Leaf::Item(item) => push_item(data, tracked, groups, item, mass),
                    Leaf::Items(items) => match items.first() {
                        Some(item) => push_item(data, tracked, groups, item, mass),
                        None => push(
                            groups,
                            Kind::NotModelled("the method leaves no item".into()),
                            mass,
                            None,
                            None,
                        ),
                    },
                    Leaf::Destroyed => push(groups, Kind::Destroyed, mass, None, None),
                    Leaf::Choice(_) => push(
                        groups,
                        Kind::NotModelled("the player chooses among several results".into()),
                        mass,
                        None,
                        None,
                    ),
                    Leaf::Pending(pending) => push(
                        groups,
                        Kind::NotModelled(pending.reason.text()),
                        mass,
                        None,
                        None,
                    ),
                }
            }
        }
    }
}

fn push_item(
    data: &Data,
    tracked: &[Slot],
    groups: &mut BTreeMap<Kind, Group>,
    item: &Item,
    mass: f64,
) {
    let kind = Kind::Item(classify(data, tracked, item));
    push(groups, kind, mass, Some(item), None);
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
            }
        })
        .collect();
    out.sort_by(|a, b| b.p.total_cmp(&a.p).then_with(|| a.kind.cmp(&b.kind)));
    out
}

fn simulate(
    data: &Data,
    node: &Node,
    method: &Method,
    fill: &Fill,
    tracked: &[Slot],
    seed: u64,
) -> Applied {
    let mut rng = SplitMix64::new(seed);
    // Each member's outcome, computed once.
    let runs: Vec<(f64, Result<Outcome, String>)> = node
        .members
        .iter()
        .map(|(item, weight)| {
            let outcome =
                apply::apply(data, method, std::slice::from_ref(item), &Params::default())
                    .map_err(|u| u.reason);
            (*weight, outcome)
        })
        .collect();
    let mut cumulative = Vec::with_capacity(runs.len());
    let mut total = 0.0;
    for (weight, _) in &runs {
        total += weight;
        cumulative.push(total);
    }
    let mut groups: BTreeMap<Kind, Group> = BTreeMap::new();
    let mut lists: BTreeMap<Item, [Weighted; 2]> = BTreeMap::new();
    let one = 1.0 / f64::from(SIMULATIONS);
    for _ in 0..SIMULATIONS {
        let x = rng.unit() * total;
        let index = cumulative.partition_point(|c| *c < x).min(runs.len() - 1);
        let outcome = match &runs[index].1 {
            Err(why) => {
                push(&mut groups, Kind::Refused(why.clone()), one, None, None);
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
            Leaf::Item(item) => push_item(data, tracked, &mut groups, item, one),
            Leaf::Items(items) => match items.first() {
                Some(item) => push_item(data, tracked, &mut groups, item, one),
                None => push(
                    &mut groups,
                    Kind::NotModelled("the method leaves no item".into()),
                    one,
                    None,
                    None,
                ),
            },
            Leaf::Destroyed => push(&mut groups, Kind::Destroyed, one, None, None),
            Leaf::Choice(_) => push(
                &mut groups,
                Kind::NotModelled("the player chooses among several results".into()),
                one,
                None,
                None,
            ),
            Leaf::Pending(pending) => {
                let only_fill = matches!(pending.ops.as_slice(), [Op::Fill { .. }]);
                if pending.certain && only_fill {
                    let item = fill_once(data, &pending.item, fill, &mut lists, &mut rng);
                    push_item(data, tracked, &mut groups, &item, one);
                } else {
                    push(
                        &mut groups,
                        Kind::NotModelled(pending.reason.text()),
                        one,
                        None,
                        None,
                    );
                }
            }
        }
    }
    let n = f64::from(SIMULATIONS);
    let mut out = finish(groups);
    for group in &mut out {
        let p = group.p;
        let sd = (p * (1.0 - p) / n).sqrt();
        group.interval = Some([(p - 1.96 * sd).max(0.0), (p + 1.96 * sd).min(1.0)]);
    }
    out.sort_by(|a, b| b.p.total_cmp(&a.p).then_with(|| a.kind.cmp(&b.kind)));
    Applied {
        method: method.id.clone(),
        groups: out,
        simulated: Some(SIMULATIONS),
    }
}

/// One reforge: draw the total, then fill the open slots one modifier at a time, each draw
/// picking a side with room uniformly (claim `reforge.side-first`).
fn fill_once(
    data: &Data,
    item: &Item,
    fill: &Fill,
    lists: &mut BTreeMap<Item, [Weighted; 2]>,
    rng: &mut SplitMix64,
) -> Item {
    let n = draw_count(&fill.counts, rng);
    let additions = fill.additions(data, item, n);
    let mut cur = item.clone();
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
            placed = true;
            break;
        }
        if !placed {
            break;
        }
    }
    cur
}

/// The fill's pool on `item`, split by side, with the level cap applied.
fn data_sides(data: &Data, item: &Item, fill: &Fill) -> [Weighted; 2] {
    let entries = apply::pool_candidates(data, item, &Pool::Natural, fill.side, true, true);
    Side::BOTH.map(|side| {
        let side_entries: Vec<(u32, f64)> = entries
            .iter()
            .copied()
            .filter(|(m, _)| data.mod_at(*m).side == Some(side))
            .filter(|(m, _)| {
                fill.mod_level_max
                    .is_none_or(|cap| data.mod_at(*m).level <= cap)
            })
            .collect();
        Weighted::new(&side_entries)
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
            approx: group.interval.is_some(),
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
