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
/// minimum shown value per stat; `{"crafted": true}` any crafted modifier; `{"any": true}`
/// everything.
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
    Many(Vec<PatternState>),
    Destroyed,
    Chance(Vec<(f64, Lifted)>),
    Choice(Vec<Lifted>),
    Pending {
        reason: crate::method::PendingReason,
        /// What the partial item already holds.
        known: PatternState,
    },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PatternError {
    #[error("pattern \"{name}\": unknown modifier \"{id}\"")]
    UnknownMod { name: String, id: String },
    #[error("pattern \"{name}\": slot names unknown class \"{id}\"")]
    UnknownClass { name: String, id: String },
    #[error("pattern \"{name}\": class \"{id}\" is a {side} class but sits in a {other} slot")]
    WrongSide {
        name: String,
        id: String,
        side: &'static str,
        other: &'static str,
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
            classes.push(Class {
                id: c.id.clone(),
                label: c.label.clone(),
                side: c.side,
                grade: c.grade,
                matchers: c.matchers.iter().map(matcher).collect::<Result<_, _>>()?,
            });
        }
        let mut slots: [Vec<Vec<u16>>; 2] = [Vec::new(), Vec::new()];
        for (side, list) in [
            (Side::Prefix, &spec.prefixes),
            (Side::Suffix, &spec.suffixes),
        ] {
            for slot in list {
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

/// The chance a matcher accepts an affix: `None` when it does not apply at all.
fn matcher_chance(data: &Data, item: &Item, matcher: &Matcher, affix: &Affix) -> Option<f64> {
    let m = data.mod_at(affix.roll.m);
    match matcher {
        Matcher::Any(any) => any.then_some(1.0),
        Matcher::Crafted(crafted) => (*crafted == (m.kind == ModKind::Crafted)).then_some(1.0),
        Matcher::Mod { m: want, min } => {
            if *want != affix.roll.m {
                return None;
            }
            let Some(min) = min else { return Some(1.0) };
            let effect = item.effect(data, &affix.roll, false);
            let mut p = 1.0;
            for (k, threshold) in min.iter().enumerate() {
                let (Some(t), Some(stat)) = (threshold, m.stats.get(k)) else {
                    continue;
                };
                match &affix.roll.values {
                    Some(values) => {
                        if Item::display(values[k], effect) < *t {
                            return Some(0.0);
                        }
                    }
                    None => {
                        let (lo, hi) = (stat.min.min(stat.max), stat.max.max(stat.min));
                        let ok = (lo..=hi)
                            .filter(|&v| Item::display(v, effect) >= *t)
                            .count();
                        p *= ok as f64 / f64::from(hi - lo + 1);
                    }
                }
            }
            Some(p)
        }
    }
}

/// Which classes of `side` an affix falls into, with the chance of each: classes are tried in
/// list order and each takes its share of the remaining mass (value thresholds on unrolled
/// values split it). The first matcher of a class that applies decides that class's share.
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
        let Some(p) = class
            .matchers
            .iter()
            .find_map(|m| matcher_chance(data, item, m, affix))
        else {
            continue;
        };
        if p > 0.0 {
            out.push((i as u16, remaining * p));
        }
        remaining *= 1.0 - p;
    }
    out
}

fn item_ok(data: &Data, pattern: &Pattern, item: &Item) -> bool {
    let f = &pattern.filter;
    let influences = f.influences.iter().fold(0u8, |acc, i| acc | i.bit());
    f.bases.iter().any(|b| *b == data.base_at(item.base).name)
        && item.item_level >= f.item_level_min
        && item.rarity == f.rarity
        && item.influences == influences
        && f.corrupted.is_none_or(|c| c == item.corrupted)
        && pattern.implicits.iter().all(|m| {
            item.implicits.iter().any(|r| {
                let affix = Affix {
                    roll: r.clone(),
                    fractured: false,
                };
                matcher_chance(data, item, m, &affix).is_some_and(|p| p > 0.0)
            })
        })
}

/// The slot assignment for one side: injective slot → affix maps (or nothing), tried in
/// lexicographic order of the slot → affix-index vector with "nothing" last; the first that
/// fills the most slots wins. Returns (missing slots, unused affix count).
fn assign(slots: &[Vec<u16>], classes: &[u16]) -> (Vec<u8>, u8) {
    let n = classes.len();
    let mut best: Option<(usize, Vec<Option<usize>>)> = None;
    let mut current = Vec::with_capacity(slots.len());
    fn walk(
        slots: &[Vec<u16>],
        classes: &[u16],
        n: usize,
        current: &mut Vec<Option<usize>>,
        best: &mut Option<(usize, Vec<Option<usize>>)>,
    ) {
        let s = current.len();
        if s == slots.len() {
            let filled = current.iter().flatten().count();
            if best.as_ref().is_none_or(|(f, _)| filled > *f) {
                *best = Some((filled, current.clone()));
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
    walk(slots, classes, n, &mut current, &mut best);
    let (_, map) = best.unwrap_or_default();
    let missing = map
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_none())
        .map(|(s, _)| s as u8)
        .collect();
    let used = map.iter().flatten().count();
    (missing, (n - used) as u8)
}

/// Every pattern state the item can be in (several when values are unrolled and a class has a
/// threshold), merged and sorted by probability descending, then state.
pub fn evaluate(data: &Data, pattern: &Pattern, item: &Item) -> Vec<(f64, PatternState)> {
    let ok = item_ok(data, pattern, item);
    // Per affix (prefixes then suffixes) its class distribution.
    let per_affix: Vec<(Side, Vec<(u16, f64)>)> = Side::BOTH
        .into_iter()
        .flat_map(|side| {
            item.affixes(side)
                .iter()
                .map(move |a| (side, classify(data, pattern, item, side, a)))
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
        let (missing_p, unused_p) = assign(&pattern.slots[0], &sides[0]);
        let (missing_s, unused_s) = assign(&pattern.slots[1], &sides[1]);
        let mut state = PatternState {
            item_ok: ok,
            prefixes: sides[0].clone(),
            suffixes: sides[1].clone(),
            open: [item.open(data, Side::Prefix), item.open(data, Side::Suffix)],
            matches: ok && missing_p.is_empty() && missing_s.is_empty(),
            missing: [missing_p, missing_s],
            unused: [unused_p, unused_s],
        };
        state.prefixes.sort_unstable();
        state.suffixes.sort_unstable();
        *merged.entry(state).or_default() += p;
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
