//! The abstract item ("pattern"): an item filter plus per-slot classes of accepted modifiers
//! ("Desired Veiled Prefix", "Good Prefix", "Any Crafted Prefix", "Any Other Affix"), each with a
//! grade. Patterns are JSON data compiled by `Pattern::compile`; a new kind of class is one new
//! `MatcherSpec` variant plus its `classify` arm. `evaluate` reads a concrete item as pattern
//! states (with probabilities where values are unrolled) and `lift` maps method outcomes onto
//! them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::data::{Data, Influence, ModKind, Rarity, Side};
use crate::item::{Affix, Item};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatternSpec {
    pub name: String,
    pub item: ItemFilterSpec,
    /// Every listed matcher must match some implicit.
    #[serde(default)]
    pub implicits: Vec<MatcherSpec>,
    pub classes: Vec<ClassSpec>,
    /// Slots: each slot lists the class ids it accepts.
    pub prefixes: Vec<Vec<String>>,
    pub suffixes: Vec<Vec<String>>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemFilterSpec {
    /// Base names.
    pub bases: Vec<String>,
    pub item_level_min: u8,
    pub rarity: Rarity,
    /// The exact influence set.
    pub influences: Vec<Influence>,
    #[serde(default)]
    pub corrupted: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassSpec {
    pub id: String,
    pub label: String,
    pub side: Side,
    pub grade: Grade,
    pub matchers: Vec<MatcherSpec>,
}

/// Data only in this pass: nothing reads it. ("A middling class opens a cheaper follow-up" is a
/// route-solver rule, not part of this pass.)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Grade {
    Desired,
    Good,
    Middling,
    Neutral,
    Bad,
}

/// What a class accepts. `{"mod": id, "min": [..]}` takes one modifier id, with an optional
/// minimum value per stat: one entry per stat in RePoE stat order, in the units the game shows
/// (the display magnitude, and the stat's translation slot factor multiplied in); `None` leaves
/// that stat unconstrained. `{"crafted": true}` any crafted modifier; `{"any": true}` everything.
/// A class takes the union of its matchers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum MatcherSpec {
    Mod {
        r#mod: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<Vec<Option<i32>>>,
    },
    Crafted {
        crafted: bool,
    },
    Any {
        any: bool,
    },
}

/// A concrete item read against a pattern.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PatternState {
    pub item_ok: bool,
    /// Class indices of the prefixes, sorted.
    pub prefixes: Vec<u16>,
    /// Class indices of the suffixes, sorted.
    pub suffixes: Vec<u16>,
    pub open: [u8; 2],
    pub matches: bool,
    /// Per side, slot indices no affix fills in the chosen assignment.
    pub missing: [Vec<u8>; 2],
    /// Per side, affixes that fill no slot (they take room a slot needs).
    pub unused: [u8; 2],
}

/// A method outcome read against a pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum Lifted {
    State(PatternState),
    /// One entry per item result, each with its own distribution (the items are alternatives of
    /// a multi-item outcome such as Mirror, not further states of one item).
    Many(Vec<Lifted>),
    Destroyed,
    Chance(Vec<(f64, Lifted)>),
    /// Options in canonical order, so equal choices compare equal whatever order the method
    /// produced them in.
    Choice(Vec<Lifted>),
    Pending {
        reason: crate::method::PendingReason,
        /// Every state the partial item can be in, with its probability.
        known: Vec<(f64, PatternState)>,
    },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PatternError {
    #[error("pattern \"{name}\": unknown modifier \"{id}\"")]
    UnknownMod { name: String, id: String },
    #[error("pattern \"{name}\": unknown base \"{base}\"")]
    UnknownBase { name: String, base: String },
    #[error(
        "pattern \"{name}\": modifier \"{id}\" is a {side} modifier but sits in a {other} class"
    )]
    ModWrongSide {
        name: String,
        id: String,
        side: &'static str,
        other: &'static str,
    },
    #[error("pattern \"{name}\": duplicate class \"{id}\"")]
    DuplicateClass { name: String, id: String },
    #[error("pattern \"{name}\": slot names unknown class \"{id}\"")]
    UnknownClass { name: String, id: String },
    #[error("pattern \"{name}\": class \"{id}\" is a {side} class but sits in a {other} slot")]
    WrongSide {
        name: String,
        id: String,
        side: &'static str,
        other: &'static str,
    },
    #[error("pattern \"{name}\": {side} slot {slot} names no class")]
    EmptySlot {
        name: String,
        side: &'static str,
        slot: usize,
    },
    #[error("pattern \"{name}\": {side} slots exceed what {base} allows ({limit})")]
    TooManySlots {
        name: String,
        side: &'static str,
        base: String,
        limit: u8,
    },
    #[error("pattern \"{name}\": the {side} classes need a catch-all class ({{\"any\": true}})")]
    NoCatchAll { name: String, side: &'static str },
}

#[derive(Clone, Debug, PartialEq)]
enum Matcher {
    Mod {
        m: u32,
        min: Option<Vec<Option<i32>>>,
    },
    Crafted(bool),
    Any(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    pub id: String,
    pub label: String,
    pub side: Side,
    pub grade: Grade,
    matchers: Vec<Matcher>,
}

/// A compiled pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub name: String,
    pub filter: ItemFilterSpec,
    implicits: Vec<Matcher>,
    pub classes: Vec<Class>,
    /// Per side, per slot, the accepted class indices.
    pub slots: [Vec<Vec<u16>>; 2],
    pub notes: Vec<String>,
}

impl Pattern {
    pub fn compile(data: &Data, spec: &PatternSpec) -> Result<Pattern, PatternError> {
        let name = &spec.name;
        for base in &spec.item.bases {
            if data.bases_named(base).is_empty() {
                return Err(PatternError::UnknownBase {
                    name: name.clone(),
                    base: base.clone(),
                });
            }
        }
        let matcher = |m: &MatcherSpec| -> Result<Matcher, PatternError> {
            Ok(match m {
                MatcherSpec::Mod { r#mod, min } => Matcher::Mod {
                    m: data
                        .mod_by_id(r#mod)
                        .ok_or_else(|| PatternError::UnknownMod {
                            name: name.clone(),
                            id: r#mod.clone(),
                        })?,
                    min: min.clone(),
                },
                MatcherSpec::Crafted { crafted } => Matcher::Crafted(*crafted),
                MatcherSpec::Any { any } => Matcher::Any(*any),
            })
        };
        let implicits = spec
            .implicits
            .iter()
            .map(matcher)
            .collect::<Result<Vec<_>, _>>()?;
        let mut classes = Vec::with_capacity(spec.classes.len());
        for c in &spec.classes {
            if classes.iter().any(|x: &Class| x.id == c.id) {
                return Err(PatternError::DuplicateClass {
                    name: name.clone(),
                    id: c.id.clone(),
                });
            }
            let matchers = c
                .matchers
                .iter()
                .map(matcher)
                .collect::<Result<Vec<_>, _>>()?;
            for m in &matchers {
                let Matcher::Mod { m: index, .. } = m else {
                    continue;
                };
                let m = data.mod_at(*index);
                let Some(side) = m.side else { continue };
                if side != c.side {
                    return Err(PatternError::ModWrongSide {
                        name: name.clone(),
                        id: m.id.clone(),
                        side: side.name(),
                        other: c.side.name(),
                    });
                }
            }
            classes.push(Class {
                id: c.id.clone(),
                label: c.label.clone(),
                side: c.side,
                grade: c.grade,
                matchers,
            });
        }
        let mut slots: [Vec<Vec<u16>>; 2] = [Vec::new(), Vec::new()];
        for (side, list) in [
            (Side::Prefix, &spec.prefixes),
            (Side::Suffix, &spec.suffixes),
        ] {
            for (n, slot) in list.iter().enumerate() {
                if slot.is_empty() {
                    return Err(PatternError::EmptySlot {
                        name: name.clone(),
                        side: side.name(),
                        slot: n + 1,
                    });
                }
                let mut accepted = Vec::with_capacity(slot.len());
                for id in slot {
                    let i = classes.iter().position(|c| &c.id == id).ok_or_else(|| {
                        PatternError::UnknownClass {
                            name: name.clone(),
                            id: id.clone(),
                        }
                    })?;
                    if classes[i].side != side {
                        return Err(PatternError::WrongSide {
                            name: name.clone(),
                            id: id.clone(),
                            side: classes[i].side.name(),
                            other: side.name(),
                        });
                    }
                    accepted.push(i as u16);
                }
                slots[side.index()].push(accepted);
            }
        }
        // A side cannot ask for more slots than any listed base can hold, counting a rare item's
        // limits and the base's own prefix/suffix delta.
        for side in Side::BOTH {
            let count = slots[side.index()].len() as u16;
            let mut worst: Option<(u8, &str)> = None;
            for base in &spec.item.bases {
                for &b in data.bases_named(base) {
                    let class = data.class_of(b);
                    let delta = match side {
                        Side::Prefix => data.base_at(b).prefix_delta,
                        Side::Suffix => data.base_at(b).suffix_delta,
                    };
                    let limit = (i16::from(class.limits.rare[side.index()]) + i16::from(delta))
                        .max(0) as u8;
                    if u16::from(limit) < count && worst.is_none_or(|(l, _)| limit < l) {
                        worst = Some((limit, base));
                    }
                }
            }
            if let Some((limit, base)) = worst {
                return Err(PatternError::TooManySlots {
                    name: name.clone(),
                    side: side.name(),
                    base: base.to_string(),
                    limit,
                });
            }
        }
        for side in Side::BOTH {
            let catch_all = classes
                .iter()
                .any(|c| c.side == side && c.matchers.contains(&Matcher::Any(true)));
            if !catch_all {
                return Err(PatternError::NoCatchAll {
                    name: name.clone(),
                    side: side.name(),
                });
            }
        }
        Ok(Pattern {
            name: name.clone(),
            filter: spec.item.clone(),
            implicits,
            classes,
            slots,
            notes: spec.notes.clone(),
        })
    }

    /// The class index with this id.
    pub fn class_index(&self, id: &str) -> Option<u16> {
        self.classes
            .iter()
            .position(|c| c.id == id)
            .map(|i| i as u16)
    }
}

/// The factor a stat is shown with: the translation slot factor of the slot that carries it.
/// A stat with no translation entry, or whose slots disagree, is shown unchanged (factor 1).
fn shown_factor(data: &Data, stat: u16) -> f64 {
    let Some(e) = data.translation_of_stat(stat) else {
        return 1.0;
    };
    let entry = &data.translations[e as usize];
    let mut factor: Option<f64> = None;
    for variant in &entry.variants {
        for slot in &variant.slots {
            if entry.stats.get(slot.k as usize) != Some(&stat) {
                continue;
            }
            match factor {
                None => factor = Some(slot.factor),
                Some(f) if f == slot.factor => {}
                Some(_) => return 1.0,
            }
        }
    }
    factor.unwrap_or(1.0)
}

/// The value the game shows for a roll of `v`.
fn shown(data: &Data, stat: u16, v: i32, effect: i16) -> f64 {
    f64::from(Item::display(v, effect)) * shown_factor(data, stat)
}

/// The share of a stat's roll grid whose shown value meets `threshold`.
fn grid_share(data: &Data, stat: u16, lo: i32, hi: i32, threshold: i32, effect: i16) -> f64 {
    let (lo, hi) = (lo.min(hi), hi.max(lo));
    let ok = (lo..=hi)
        .filter(|&v| shown(data, stat, v, effect) >= f64::from(threshold))
        .count();
    ok as f64 / f64::from(hi - lo + 1)
}

/// The chance a single matcher accepts an affix: `None` when it does not apply at all. A value
/// threshold splits the roll grid; several stats are taken as independent.
fn matcher_chance(
    data: &Data,
    item: &Item,
    matcher: &Matcher,
    affix: &Affix,
    implicit: bool,
) -> Option<f64> {
    let m = data.mod_at(affix.roll.m);
    match matcher {
        Matcher::Any(any) => Some(if *any { 1.0 } else { 0.0 }),
        Matcher::Crafted(crafted) => Some(if *crafted == (m.kind == ModKind::Crafted) {
            1.0
        } else {
            0.0
        }),
        Matcher::Mod { m: want, min } => {
            if *want != affix.roll.m {
                return None;
            }
            let Some(min) = min else { return Some(1.0) };
            let effect = item.effect(data, &affix.roll, implicit);
            let mut p = 1.0;
            for (k, threshold) in min.iter().enumerate() {
                let (Some(t), Some(stat)) = (threshold, m.stats.get(k)) else {
                    continue;
                };
                match &affix.roll.values {
                    Some(values) => {
                        let Some(v) = values.get(k) else { continue };
                        if shown(data, stat.stat, *v, effect) < f64::from(*t) {
                            return Some(0.0);
                        }
                    }
                    None => {
                        p *= grid_share(data, stat.stat, stat.min, stat.max, *t, effect);
                    }
                }
            }
            Some(p)
        }
    }
}

/// Whether a value vector meets every threshold of a `min` list.
fn box_met(
    data: &Data,
    m: &crate::data::ModData,
    thresholds: &[Option<i32>],
    values: &[i32],
    effect: i16,
) -> bool {
    thresholds.iter().enumerate().all(|(k, t)| {
        let (Some(t), Some(stat), Some(v)) = (t, m.stats.get(k), values.get(k)) else {
            return true;
        };
        shown(data, stat.stat, *v, effect) >= f64::from(*t)
    })
}

/// The probability a roll grid meets `thresholds`: a product over the stats the list constrains.
fn box_share(
    data: &Data,
    m: &crate::data::ModData,
    thresholds: &[Option<i32>],
    effect: i16,
) -> f64 {
    let mut p = 1.0;
    for (k, t) in thresholds.iter().enumerate() {
        let (Some(t), Some(stat)) = (t, m.stats.get(k)) else {
            continue;
        };
        p *= grid_share(data, stat.stat, stat.min, stat.max, *t, effect);
    }
    p
}

/// The chance a class accepts an affix: the union of its matchers. A matcher that accepts
/// everything (a matching `{"any": true}` or `{"crafted": true}`) makes it certain; otherwise
/// the value grids of its same-mod thresholds are unioned, so overlapping thresholds do not
/// double-count and no matcher is ignored. `None` when no matcher applies at all.
fn class_chance(
    data: &Data,
    item: &Item,
    class: &Class,
    affix: &Affix,
    implicit: bool,
) -> Option<f64> {
    let m = data.mod_at(affix.roll.m);
    let mut applies = false;
    let mut certain = false;
    let mut boxes: Vec<&[Option<i32>]> = Vec::new();
    for matcher in &class.matchers {
        match matcher {
            Matcher::Any(any) => {
                applies = true;
                certain |= *any;
            }
            Matcher::Crafted(crafted) => {
                applies = true;
                certain |= *crafted == (m.kind == ModKind::Crafted);
            }
            Matcher::Mod { m: want, min } => {
                if *want != affix.roll.m {
                    continue;
                }
                applies = true;
                match min {
                    None => certain = true,
                    Some(min) => boxes.push(min),
                }
            }
        }
    }
    if !applies {
        return None;
    }
    if certain {
        return Some(1.0);
    }
    let effect = item.effect(data, &affix.roll, implicit);
    Some(match &affix.roll.values {
        Some(values) => f64::from(u8::from(
            boxes.iter().any(|b| box_met(data, m, b, values, effect)),
        )),
        None => {
            if boxes.is_empty() {
                0.0
            } else {
                union_share(data, m, &boxes, effect)
            }
        }
    })
}

/// The probability of the union of threshold boxes (inclusion-exclusion), exact for overlapping
/// thresholds.
fn union_share(
    data: &Data,
    m: &crate::data::ModData,
    boxes: &[&[Option<i32>]],
    effect: i16,
) -> f64 {
    struct Union<'a> {
        data: &'a Data,
        m: &'a crate::data::ModData,
        boxes: &'a [&'a [Option<i32>]],
        effect: i16,
        thresholds: Vec<Option<i32>>,
        total: f64,
    }
    impl Union<'_> {
        /// Inclusion-exclusion over the boxes: every non-empty subset contributes its
        /// intersection, with alternating sign.
        fn walk(&mut self, j: usize, size: usize) {
            if j == self.boxes.len() {
                if size > 0 {
                    let p = box_share(self.data, self.m, &self.thresholds, self.effect);
                    self.total += if size % 2 == 1 { p } else { -p };
                }
                return;
            }
            self.walk(j + 1, size);
            let saved = self.thresholds.clone();
            for (k, t) in self.boxes[j].iter().enumerate() {
                if let (Some(t), Some(slot)) = (t, self.thresholds.get_mut(k)) {
                    *slot = Some(slot.map_or(*t, |s| s.max(*t)));
                }
            }
            self.walk(j + 1, size + 1);
            self.thresholds = saved;
        }
    }
    let mut union = Union {
        data,
        m,
        boxes,
        effect,
        thresholds: vec![None; m.stats.len()],
        total: 0.0,
    };
    union.walk(0, 0);
    union.total.clamp(0.0, 1.0)
}

/// Which classes of `side` an affix falls into, with the chance of each: classes are tried in
/// list order and each takes its share of the remaining mass. A class's share is the union of its
/// matchers (see `class_chance`), so a class with several thresholds never loses mass it should
/// have taken.
pub fn classify(
    data: &Data,
    pattern: &Pattern,
    item: &Item,
    side: Side,
    affix: &Affix,
) -> Vec<(u16, f64)> {
    let mut remaining = 1.0;
    let mut out = Vec::new();
    for (i, class) in pattern.classes.iter().enumerate() {
        if class.side != side || remaining <= 0.0 {
            continue;
        }
        let Some(p) = class_chance(data, item, class, affix, false) else {
            continue;
        };
        if p > 0.0 {
            out.push((i as u16, remaining * p));
        }
        remaining *= 1.0 - p;
    }
    out
}

/// The chance the item passes every deterministic filter check and every implicit matcher.
fn item_ok_probability(data: &Data, pattern: &Pattern, item: &Item) -> f64 {
    let f = &pattern.filter;
    let influences = f.influences.iter().fold(0u8, |acc, i| acc | i.bit());
    let deterministic = f.bases.iter().any(|b| *b == data.base_at(item.base).name)
        && item.item_level >= f.item_level_min
        && item.rarity == f.rarity
        && item.influences == influences
        && f.corrupted.is_none_or(|c| c == item.corrupted);
    if !deterministic {
        return 0.0;
    }
    let mut ok = 1.0;
    for matcher in &pattern.implicits {
        let mut miss = 1.0;
        for roll in &item.implicits {
            let affix = Affix {
                roll: roll.clone(),
                fractured: false,
            };
            if let Some(p) = matcher_chance(data, item, matcher, &affix, true) {
                miss *= 1.0 - p.clamp(0.0, 1.0);
            }
        }
        ok *= 1.0 - miss;
    }
    ok
}

/// An assignment's score (slots filled, flexibility left unfilled) and its slot → affix map.
type Assignment = ((usize, usize), Vec<Option<usize>>);

/// The slot assignment for one side: injective slot → affix maps (or nothing), tried in
/// lexicographic order of the slot → affix-index vector with "nothing" last. Among maps that
/// fill the most slots, the first one that leaves the slots accepting the most classes unfilled
/// wins (they are the easiest to fill later). Returns (missing slots, unused affix count).
fn assign(slots: &[Vec<u16>], classes: &[u16], affixes: usize) -> (Vec<u8>, u8) {
    let n = classes.len();
    let mut best: Option<Assignment> = None;
    fn walk(
        slots: &[Vec<u16>],
        classes: &[u16],
        n: usize,
        current: &mut Vec<Option<usize>>,
        best: &mut Option<Assignment>,
    ) {
        let s = current.len();
        if s == slots.len() {
            let filled = current.iter().flatten().count();
            let flexible: usize = current
                .iter()
                .enumerate()
                .filter(|(_, a)| a.is_none())
                .map(|(s, _)| slots[s].len())
                .sum();
            let key = (filled, flexible);
            if best.as_ref().is_none_or(|(k, _)| key > *k) {
                *best = Some((key, current.clone()));
            }
            return;
        }
        for a in 0..n {
            if !current.contains(&Some(a)) && slots[s].contains(&classes[a]) {
                current.push(Some(a));
                walk(slots, classes, n, current, best);
                current.pop();
            }
        }
        current.push(None);
        walk(slots, classes, n, current, best);
        current.pop();
    }
    walk(slots, classes, n, &mut Vec::new(), &mut best);
    let map = best.map(|(_, m)| m).unwrap_or_default();
    let missing = map
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_none())
        .map(|(s, _)| s as u8)
        .collect();
    let used = map.iter().flatten().count();
    (missing, affixes.saturating_sub(used) as u8)
}

/// Every pattern state the item can be in (several when values are unrolled and a class or an
/// implicit has a threshold), merged and sorted by probability descending, then state.
pub fn evaluate(data: &Data, pattern: &Pattern, item: &Item) -> Vec<(f64, PatternState)> {
    let ok = item_ok_probability(data, pattern, item);
    let counts = [item.prefixes.len(), item.suffixes.len()];
    // Per affix (prefixes then suffixes) its class distribution; an affix no class takes (only
    // possible without a catch-all) contributes no state and counts as unused.
    let per_affix: Vec<(Side, Vec<(u16, f64)>)> = Side::BOTH
        .into_iter()
        .flat_map(|side| {
            item.affixes(side)
                .iter()
                .map(|a| (side, classify(data, pattern, item, side, a)))
                .filter(|(_, d)| !d.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut merged: BTreeMap<PatternState, f64> = BTreeMap::new();
    let mut choice = vec![0usize; per_affix.len()];
    loop {
        let mut p = 1.0;
        let mut sides: [Vec<u16>; 2] = [Vec::new(), Vec::new()];
        for (i, (side, dist)) in per_affix.iter().enumerate() {
            let (class, q) = dist[choice[i]];
            p *= q;
            sides[side.index()].push(class);
        }
        let (missing_p, unused_p) = assign(&pattern.slots[0], &sides[0], counts[0]);
        let (missing_s, unused_s) = assign(&pattern.slots[1], &sides[1], counts[1]);
        let state = |item_ok: bool| {
            let mut s = PatternState {
                item_ok,
                prefixes: sides[0].clone(),
                suffixes: sides[1].clone(),
                open: [item.open(data, Side::Prefix), item.open(data, Side::Suffix)],
                matches: item_ok && missing_p.is_empty() && missing_s.is_empty(),
                missing: [missing_p.clone(), missing_s.clone()],
                unused: [unused_p, unused_s],
            };
            s.prefixes.sort_unstable();
            s.suffixes.sort_unstable();
            s
        };
        if ok > 0.0 {
            *merged.entry(state(true)).or_default() += p * ok;
        }
        if ok < 1.0 {
            *merged.entry(state(false)).or_default() += p * (1.0 - ok);
        }
        // Next combination.
        let mut i = 0;
        loop {
            if i == choice.len() {
                let mut out: Vec<(f64, PatternState)> =
                    merged.into_iter().map(|(s, p)| (p, s)).collect();
                out.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                return out;
            }
            choice[i] += 1;
            if choice[i] < per_affix[i].1.len() {
                break;
            }
            choice[i] = 0;
            i += 1;
        }
    }
}

/// The chance the item matches the pattern.
pub fn matches_probability(data: &Data, pattern: &Pattern, item: &Item) -> f64 {
    evaluate(data, pattern, item)
        .iter()
        .filter(|(_, s)| s.matches)
        .map(|(p, _)| p)
        .sum()
}

/// The full distribution of an item, with a bare state when it is certain.
fn lift_item(data: &Data, pattern: &Pattern, item: &Item) -> Lifted {
    let mut states = evaluate(data, pattern, item);
    if states.len() == 1 && states[0].0 == 1.0 {
        Lifted::State(states.remove(0).1)
    } else {
        Lifted::Chance(
            states
                .into_iter()
                .map(|(p, s)| (p, Lifted::State(s)))
                .collect(),
        )
    }
}

/// Sorts every Choice's options canonically, so two choices with the same options in different
/// orders compare equal.
fn canonicalise(lifted: Lifted) -> Lifted {
    match lifted {
        Lifted::Choice(options) => {
            let mut options: Vec<Lifted> = options.into_iter().map(canonicalise).collect();
            options.sort_by_key(|l| format!("{l:?}"));
            Lifted::Choice(options)
        }
        Lifted::Chance(children) => Lifted::Chance(
            children
                .into_iter()
                .map(|(p, l)| (p, canonicalise(l)))
                .collect(),
        ),
        Lifted::Many(items) => Lifted::Many(items.into_iter().map(canonicalise).collect()),
        other => other,
    }
}

/// Reads a method outcome as pattern states: an item becomes its full distribution (or a bare
/// state when certain), several items one distribution per item, and a Pending outcome the full
/// distribution of what the partial item already holds. Equal Chance children merge.
pub fn lift(data: &Data, pattern: &Pattern, outcome: &crate::apply::Outcome) -> Lifted {
    use crate::apply::Outcome;
    canonicalise(match outcome {
        Outcome::Item(item) => lift_item(data, pattern, item),
        Outcome::Items(items) => {
            Lifted::Many(items.iter().map(|i| lift_item(data, pattern, i)).collect())
        }
        Outcome::Destroyed => Lifted::Destroyed,
        Outcome::Choice(options) => {
            Lifted::Choice(options.iter().map(|o| lift(data, pattern, o)).collect())
        }
        Outcome::Pending { item, reason, .. } => Lifted::Pending {
            reason: reason.clone(),
            known: evaluate(data, pattern, item),
        },
        Outcome::Chance(children) => {
            let mut merged: Vec<(f64, Lifted)> = Vec::with_capacity(children.len());
            for (p, o) in children {
                let l = lift(data, pattern, o);
                match merged.iter_mut().find(|(_, x)| *x == l) {
                    Some((q, _)) => *q += p,
                    None => merged.push((*p, l)),
                }
            }
            Lifted::Chance(merged)
        }
    })
}
