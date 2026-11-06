//! The inverse of one step: given an item, every (input, method) pair that could have produced it
//! and with what chance.
//!
//! `apply` runs a method forwards; this walks it backwards one op at a time. Undoing an op gives
//! the items it could have started from, each with the chance that the op turns it into the item
//! asked about — for an add, the chance of drawing the modifier that was added; for a removal, one
//! over the number of modifiers that removal could have taken. Multiplying down the op list gives
//! the chance of the whole step, and `apply::apply`'s own `check` decides whether the input the
//! inverse found is one the method would accept at all.
//!
//! Undoing is exact for the single-draw ops: an add (`AddRandom`, `AddMod`, `AddVeiled`), a removal
//! (`RemoveRandom`), a rarity, corruption, influence, value reroll, fracture, quality. An op that
//! throws information away and an op whose result the model does not compute yet (a reforge's
//! `Fill`, a socket table, a clearance that could have started from anything) cannot be undone, and
//! a method with one of those is reported as not invertible rather than guessed at.
//!
//! The answer is grouped by what the input looked like to a pattern: hundreds of concrete inputs
//! collapse into the handful of classes the pattern reads, which is the form the question is asked
//! in ("what state could have been one step short of this?").

use std::collections::BTreeMap;

use crate::apply::{self, Params};
use crate::data::{Cost, Data, ModKind, Rarity, Side};
use crate::item::{Affix, Item, MetaMod, Roll};
use crate::method::{Method, Op, Pool, SideReq};
use crate::pattern::{self, Pattern, PatternState};

/// One input that could have produced the item, with the chance it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub method: String,
    pub name: String,
    pub input: Item,
    pub cost: Vec<Cost>,
    /// The chance the method turns `input` into the item asked about.
    pub p: f64,
    /// Something the inverse had to assume, such as which rarity an item had before a reforge set
    /// it, or a value the target's rolled numbers hide.
    pub note: Option<String>,
}

/// The inputs that could have produced the item, read against a pattern: one group per pattern
/// state, most likely first.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub state: PatternState,
    /// The best chance of any one source in the group. The chances of different inputs are
    /// conditional on that input, so they do not add up.
    pub best: f64,
    pub sources: Vec<Source>,
}

/// What the inverse found, and which methods it could not walk backwards.
#[derive(Clone, Debug, PartialEq)]
pub struct Inversion {
    /// Concrete inputs found before grouping.
    pub found: usize,
    pub groups: Vec<Group>,
    /// (method id, why it cannot be inverted), in method order.
    pub skipped: Vec<(String, String)>,
}

/// Every single-input method that could have produced `target`, grouped by the input's pattern
/// state.
pub fn sources(data: &Data, pattern: &Pattern, target: &Item) -> Inversion {
    let params = Params::default();
    let mut all: Vec<Source> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut seen = 0usize;
    for method in data.methods.iter().filter(|m| m.inputs == 1) {
        match undo_method(data, method, target, &params, &mut seen) {
            Ok(found) => {
                let cost = apply::cost(data, method, &params).unwrap_or_default();
                all.extend(found.into_iter().map(|(input, p, note)| Source {
                    method: method.id.clone(),
                    name: method.name.clone(),
                    input,
                    cost: cost.clone(),
                    p,
                    note,
                }));
            }
            Err(why) => skipped.push((method.id.clone(), why)),
        }
    }
    let mut groups: BTreeMap<PatternState, Group> = BTreeMap::new();
    for source in all.drain(..) {
        let state = most_likely_state(data, pattern, &source.input);
        let group = groups.entry(state.clone()).or_insert_with(|| Group {
            state,
            best: 0.0,
            sources: Vec::new(),
        });
        group.best = group.best.max(source.p);
        group.sources.push(source);
    }
    let mut groups: Vec<Group> = groups.into_values().collect();
    for group in &mut groups {
        group
            .sources
            .sort_by(|a, b| b.p.total_cmp(&a.p).then_with(|| a.method.cmp(&b.method)));
    }
    groups.sort_by(|a, b| {
        b.best
            .total_cmp(&a.best)
            .then_with(|| a.state.cmp(&b.state))
    });
    Inversion {
        found: groups.iter().map(|g| g.sources.len()).sum(),
        groups,
        skipped,
    }
}

/// The pattern state of an input: the input is read on its own, and its most likely state names
/// the group it falls in.
fn most_likely_state(data: &Data, pattern: &Pattern, item: &Item) -> PatternState {
    pattern::evaluate(data, pattern, item)
        .into_iter()
        .max_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| b.1.matches.cmp(&a.1.matches))
        })
        .map(|(_, s)| s)
        .expect("evaluate always returns at least one state")
}

/// Undoes one method: its ops backwards, then `check` on what it found.
fn undo_method(
    data: &Data,
    method: &Method,
    target: &Item,
    params: &Params,
    seen: &mut usize,
) -> Result<Vec<(Item, f64, Option<String>)>, String> {
    let mut work: Vec<(Item, f64, Option<String>)> = vec![(target.clone(), 1.0, None)];
    for op in method.ops.iter().rev() {
        let mut next: Vec<(Item, f64, Option<String>)> = Vec::new();
        for (item, p, note) in work {
            for (before, q, inner) in undo(data, &item, op)? {
                *seen += 1;
                next.push((before, p * q, note.clone().or(inner)));
            }
        }
        work = next;
        if work.is_empty() {
            return Ok(Vec::new());
        }
    }
    let mut out = Vec::new();
    let mut canonical = target.clone();
    canonical.canonical(data);
    for (item, p, note) in work {
        let mut item = item;
        item.canonical(data);
        // A step that leaves the item exactly as it is says nothing: the input and the result are
        // the same state (a value reroll on an item whose values are unknown, a quality step at
        // zero). Reporting it would only bury the sources that do change something.
        if item == canonical {
            continue;
        }
        if apply::check(data, method, std::slice::from_ref(&item), params).is_err() {
            continue;
        }
        out.push((item, p, note));
    }
    Ok(out)
}

/// The items `op` could have turned into `target`, each with the chance it does; `Err` when the op
/// cannot be undone.
fn undo(data: &Data, target: &Item, op: &Op) -> Result<Vec<(Item, f64, Option<String>)>, String> {
    Ok(match op {
        Op::AddRandom {
            pool,
            side,
            ignore_limits,
        } => undo_add_random(data, target, pool, *side, *ignore_limits),
        Op::AddMod { r#mod } => undo_add_mod(data, target, r#mod),
        Op::AddVeiled => undo_add_veiled(data, target),
        Op::RemoveRandom { filter, .. } => undo_removal(data, target, filter),
        Op::SetRarity { rarity } => undo_rarity(target, *rarity),
        Op::Corrupt => {
            if !target.corrupted {
                return Ok(Vec::new());
            }
            let mut before = target.clone();
            before.corrupted = false;
            vec![(before, 1.0, None)]
        }
        Op::AddInfluence { influence } => {
            if !target.has_influence(*influence) {
                return Ok(Vec::new());
            }
            let mut before = target.clone();
            before.influences &= !influence.bit();
            vec![(before, 1.0, None)]
        }
        Op::RerollValues { scope } => undo_reroll(data, target, *scope),
        Op::Fracture => undo_fracture(data, target),
        Op::AddQuality { by_rarity, max } => undo_quality(target, *by_rarity, *max),
        Op::CatalystQuality { catalyst } => undo_catalyst(data, target, catalyst),
        Op::Chance { branches } => {
            let mut out = Vec::new();
            let mut lost = 0usize;
            for branch in branches {
                let mut work = vec![(target.clone(), branch.p, None)];
                let mut undone = true;
                for inner in branch.ops.iter().rev() {
                    let mut next = Vec::new();
                    for (item, p, note) in work {
                        match undo(data, &item, inner) {
                            Ok(found) => {
                                for (before, q, inner_note) in found {
                                    next.push((before, p * q, note.clone().or(inner_note)));
                                }
                            }
                            Err(_) => undone = false,
                        }
                    }
                    work = next;
                    if !undone {
                        break;
                    }
                }
                if undone {
                    out.extend(work);
                } else {
                    lost += 1;
                }
            }
            if out.is_empty() {
                return Err("every branch of its chance cannot be undone".to_string());
            }
            if lost > 0 {
                let note = format!("{lost} of {} branches cannot be undone", branches.len());
                for (_, _, n) in &mut out {
                    *n = Some(match n.take() {
                        Some(prev) => format!("{prev}; {note}"),
                        None => note.clone(),
                    });
                }
            }
            out
        }
        Op::AddEssence { essence } => {
            let Some(essence) = data.essence_by_name(essence) else {
                return Ok(Vec::new());
            };
            let class = data.base_at(target.base).class;
            let Some((_, m)) = data.essences[essence as usize]
                .mods
                .iter()
                .find(|(c, _)| *c == class)
            else {
                return Ok(Vec::new());
            };
            let Some(side) = data.mod_at(*m).side else {
                return Ok(Vec::new());
            };
            let mut out = Vec::new();
            for index in 0..affixes(target, side).len() {
                if affixes(target, side)[index].roll.m != *m {
                    continue;
                }
                let mut before = without(target, side, index);
                before.canonical(data);
                out.push((before, 1.0, None));
            }
            out
        }
        // Ops that throw information away or whose result the model does not compute cannot be
        // undone: a clearance could have started from anything, a socket table has no distribution
        // yet, and a reforge's random modifiers are not modelled in this pass.
        Op::Clear { .. }
        | Op::Scour
        | Op::Unveil
        | Op::RemoveCrafted
        | Op::SetEnchant { .. }
        | Op::SetEnchantMod { .. }
        | Op::ClearEnchants
        | Op::AddInfluences { .. }
        | Op::TakeInput { .. }
        | Op::VaalImplicit
        | Op::EldritchImplicit { .. }
        | Op::Dominance
        | Op::ChangeResistance { .. }
        | Op::Imprint
        | Op::Restore
        | Op::LinkAll
        | Op::Mirror
        | Op::Destroy
        | Op::RemoveIntangibility
        | Op::Fill { .. }
        | Op::Sockets { .. }
        | Op::Unmodelled { .. } => {
            return Err(format!("cannot undo `{}`", op_name(op)));
        }
    })
}

/// The op's name, for the reason a method cannot be inverted.
fn op_name(op: &Op) -> &'static str {
    match op {
        Op::SetRarity { .. } => "setRarity",
        Op::Clear { .. } => "clear",
        Op::Scour => "scour",
        Op::AddRandom { .. } => "addRandom",
        Op::RemoveRandom { .. } => "removeRandom",
        Op::AddVeiled => "addVeiled",
        Op::Unveil => "unveil",
        Op::AddMod { .. } => "addMod",
        Op::AddEssence { .. } => "addEssence",
        Op::RemoveCrafted => "removeCrafted",
        Op::SetEnchant { .. } => "setEnchant",
        Op::SetEnchantMod { .. } => "setEnchantMod",
        Op::ClearEnchants => "clearEnchants",
        Op::RerollValues { .. } => "rerollValues",
        Op::Fracture => "fracture",
        Op::AddInfluence { .. } => "addInfluence",
        Op::AddInfluences { .. } => "addInfluences",
        Op::TakeInput { .. } => "takeInput",
        Op::Corrupt => "corrupt",
        Op::VaalImplicit => "vaalImplicit",
        Op::EldritchImplicit { .. } => "eldritchImplicit",
        Op::Dominance => "dominance",
        Op::ChangeResistance { .. } => "changeResistance",
        Op::AddQuality { .. } => "addQuality",
        Op::CatalystQuality { .. } => "catalystQuality",
        Op::Imprint => "imprint",
        Op::Restore => "restore",
        Op::LinkAll => "linkAll",
        Op::Mirror => "mirror",
        Op::Destroy => "destroy",
        Op::RemoveIntangibility => "removeIntangibility",
        Op::Chance { .. } => "chance",
        Op::Fill { .. } => "fill",
        Op::Sockets { .. } => "sockets",
        Op::Unmodelled { .. } => "unmodelled",
    }
}

/// The affixes of one side, in order.
fn affixes(item: &Item, side: Side) -> &[Affix] {
    match side {
        Side::Prefix => &item.prefixes,
        Side::Suffix => &item.suffixes,
    }
}

fn affixes_mut(item: &mut Item, side: Side) -> &mut Vec<Affix> {
    match side {
        Side::Prefix => &mut item.prefixes,
        Side::Suffix => &mut item.suffixes,
    }
}

fn without(item: &Item, side: Side, index: usize) -> Item {
    let mut before = item.clone();
    affixes_mut(&mut before, side).remove(index);
    before
}

/// An add: the target without the modifier it added, with the chance the pool draws that modifier.
fn undo_add_random(
    data: &Data,
    target: &Item,
    pool: &Pool,
    side: SideReq,
    ignore_limits: bool,
) -> Vec<(Item, f64, Option<String>)> {
    let mut out = Vec::new();
    for s in Side::BOTH {
        for index in 0..affixes(target, s).len() {
            let mut before = without(target, s, index);
            before.canonical(data);
            let allowed = apply::sides(data, &before, side);
            if !allowed.contains(&s) {
                continue;
            }
            let candidates =
                apply::pool_candidates(data, &before, pool, side, ignore_limits, false);
            let added = affixes(target, s)[index].roll.m;
            if let Some((_, p)) = candidates.iter().find(|(m, _)| *m == added) {
                out.push((before, *p, None));
            }
        }
    }
    out
}

/// A named add (a bench craft, a beastcraft): the target without that modifier.
fn undo_add_mod(data: &Data, target: &Item, id: &str) -> Vec<(Item, f64, Option<String>)> {
    let Some(m) = data.mod_by_id(id) else {
        return Vec::new();
    };
    let Some(side) = data.mod_at(m).side else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for index in 0..affixes(target, side).len() {
        if affixes(target, side)[index].roll.m != m {
            continue;
        }
        let mut before = without(target, side, index);
        before.canonical(data);
        // The op adds the modifier unrolled, so a rolled target hides the roll it added.
        let note = (affixes(target, side)[index].roll.values.is_some())
            .then(|| "the added modifier's values are not the ones the craft gives".to_string());
        out.push((before, 1.0, note));
    }
    out
}

/// A veiled add: the target without the placeholder, with the half-or-certain side chance.
fn undo_add_veiled(data: &Data, target: &Item) -> Vec<(Item, f64, Option<String>)> {
    let mut out = Vec::new();
    for side in Side::BOTH {
        for index in 0..affixes(target, side).len() {
            let m = affixes(target, side)[index].roll.m;
            if !apply::VEILED.contains(&data.mod_at(m).id.as_str()) {
                continue;
            }
            let mut before = without(target, side, index);
            before.canonical(data);
            let both = apply::room(data, &before, Side::Prefix) > 0
                && apply::room(data, &before, Side::Suffix) > 0;
            out.push((before, if both { 0.5 } else { 1.0 }, None));
        }
    }
    out
}

/// Every modifier that could sit as an explicit on this item: the natural pool, and the crafted,
/// essence, delve, unveiled, veiled, mercenary and ducat modifiers that can spawn on its tags.
fn eligible(data: &Data, item: &Item) -> Vec<u32> {
    let tags = item.tags(data);
    let base = data.base_at(item.base);
    let groups = item.explicit_groups(data);
    let bench: Vec<u32> = data
        .bench
        .iter()
        .filter(|b| b.classes.contains(&base.class))
        .filter_map(|b| match b.action {
            crate::data::BenchAction::AddMod { m } => Some(m),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for kind in [
        ModKind::Natural,
        ModKind::EssenceOnly,
        ModKind::Crafted,
        ModKind::Delve,
        ModKind::Unveiled,
        ModKind::Veiled,
        ModKind::Mercenary,
        ModKind::Ducat,
    ] {
        for &m in data.mods_of_kind(kind) {
            let md = data.mod_at(m);
            if md.side.is_none()
                || (kind != ModKind::Ducat && md.domain != base.domain.repoe())
                || md.level > item.item_level
                || md.groups.iter().any(|g| groups.contains(g))
            {
                continue;
            }
            let spawnable = crate::pool::spawn(md, &tags) > 0 || bench.contains(&m);
            if spawnable {
                out.push(m);
            }
        }
    }
    out
}

/// A removal: the target with one more modifier, at one over the number of modifiers the removal
/// could have taken from that input.
fn undo_removal(
    data: &Data,
    target: &Item,
    filter: &crate::method::RemoveFilter,
) -> Vec<(Item, f64, Option<String>)> {
    let mut out = Vec::new();
    for m in eligible(data, target) {
        let md = data.mod_at(m);
        let Some(side) = md.side else { continue };
        // The input held one more modifier than the result, so the result must have had room for
        // it: an item already at its affix limit cannot have lost a modifier of that side.
        if apply::room(data, target, side) == 0 {
            continue;
        }
        let mut before = target.clone();
        affixes_mut(&mut before, side).push(Affix {
            roll: Roll { m, values: None },
            fractured: false,
        });
        before.canonical(data);
        // The removal picks uniformly among the explicits it can take, in this input.
        let removable = removable_count(data, &before, filter);
        if removable == 0 {
            continue;
        }
        let note = (before.has_meta(data, MetaMod::NoAttack)
            || before.has_meta(data, MetaMod::NoCaster))
        .then(|| {
            "the item carries Cannot Roll, which narrows the removal and this pass does not count"
                .to_string()
        });
        out.push((before, 1.0 / removable as f64, note));
    }
    out
}

/// How many explicits a removal could take from this item: not fractured, not a veiled
/// placeholder, not on a side a metamod locks, and matching the op's filter.
fn removable_count(data: &Data, item: &Item, filter: &crate::method::RemoveFilter) -> usize {
    use crate::method::{RemoveFilter, SideReq};
    Side::BOTH
        .into_iter()
        .filter(|side| !item.locked(data, *side))
        .flat_map(|side| affixes(item, side).iter().map(move |a| (side, a)))
        .filter(|(_, a)| !a.fractured && data.mod_at(a.roll.m).kind != ModKind::Veiled)
        .filter(|(side, a)| match *filter {
            RemoveFilter::Any => true,
            RemoveFilter::Side { side: want } => match want {
                SideReq::Any | SideReq::Dominant => true,
                SideReq::Prefix => *side == Side::Prefix,
                SideReq::Suffix => *side == Side::Suffix,
            },
            RemoveFilter::Influenced => data.mod_at(a.roll.m).influence.is_some(),
        })
        .count()
}

/// A rarity set: the same item at any other rarity. Nothing in the model says which one it was, so
/// every rarity the method's requirements accept is offered at p = 1 each.
fn undo_rarity(target: &Item, rarity: Rarity) -> Vec<(Item, f64, Option<String>)> {
    if target.rarity != rarity {
        return Vec::new();
    }
    [Rarity::Normal, Rarity::Magic, Rarity::Rare, Rarity::Unique]
        .into_iter()
        .filter(|&r| r != rarity)
        .map(|r| {
            let mut before = target.clone();
            before.rarity = r;
            (
                before,
                1.0,
                Some(format!(
                    "the item was {} before the step set its rarity",
                    r.name().to_lowercase()
                )),
            )
        })
        .collect()
}

/// A value reroll: the target with the values unrolled.
fn undo_reroll(
    data: &Data,
    target: &Item,
    scope: crate::method::ValueScope,
) -> Vec<(Item, f64, Option<String>)> {
    use crate::method::ValueScope;
    let mut before = target.clone();
    let rolled = match scope {
        ValueScope::Explicit => before
            .prefixes
            .iter()
            .chain(&before.suffixes)
            .any(|a| a.roll.values.is_some()),
        ValueScope::Implicit => before.implicits.iter().any(|r| r.values.is_some()),
        ValueScope::Defences => before.defences.is_some(),
    };
    if rolled {
        // The forward reroll leaves the values unrolled, so a target with values did not come
        // from one.
        return Vec::new();
    }
    match scope {
        ValueScope::Explicit => {
            // The forward reroll leaves crafted and fractured modifiers alone.
            let crafted: Vec<usize> = before
                .prefixes
                .iter()
                .enumerate()
                .filter(|(_, a)| data.mod_at(a.roll.m).kind == ModKind::Crafted)
                .map(|(i, _)| i)
                .collect();
            for (i, affix) in before.prefixes.iter_mut().enumerate() {
                if !affix.fractured && !crafted.contains(&i) {
                    affix.roll.values = None;
                }
            }
            let crafted: Vec<usize> = before
                .suffixes
                .iter()
                .enumerate()
                .filter(|(_, a)| data.mod_at(a.roll.m).kind == ModKind::Crafted)
                .map(|(i, _)| i)
                .collect();
            for (i, affix) in before.suffixes.iter_mut().enumerate() {
                if !affix.fractured && !crafted.contains(&i) {
                    affix.roll.values = None;
                }
            }
        }
        ValueScope::Implicit => before.implicits.iter_mut().for_each(|r| r.values = None),
        ValueScope::Defences => before.defences = None,
    }
    vec![(
        before,
        1.0,
        Some("the values before the reroll are not known".to_string()),
    )]
}

/// A fracture: the target with one fractured modifier unfractured.
fn undo_fracture(data: &Data, target: &Item) -> Vec<(Item, f64, Option<String>)> {
    let mut out = Vec::new();
    for side in Side::BOTH {
        for index in 0..affixes(target, side).len() {
            if !affixes(target, side)[index].fractured {
                continue;
            }
            let mut before = target.clone();
            affixes_mut(&mut before, side)[index].fractured = false;
            before.canonical(data);
            let candidates = before
                .prefixes
                .iter()
                .chain(&before.suffixes)
                .filter(|a| {
                    !a.fractured
                        && !matches!(
                            data.mod_at(a.roll.m).kind,
                            ModKind::Crafted | ModKind::Veiled
                        )
                })
                .count();
            if candidates == 0 {
                continue;
            }
            out.push((before, 1.0 / candidates as f64, None));
        }
    }
    out
}

/// A quality step: the target with less quality.
fn undo_quality(target: &Item, by_rarity: [u8; 4], max: u8) -> Vec<(Item, f64, Option<String>)> {
    if target.quality == 0 {
        return Vec::new();
    }
    let index = match target.rarity {
        Rarity::Normal => 0,
        Rarity::Magic => 1,
        Rarity::Rare => 2,
        Rarity::Unique => 3,
    };
    let step = by_rarity[index].max(1);
    let mut before = target.clone();
    let note = if target.quality >= max {
        before.quality = max.saturating_sub(step);
        Some(format!(
            "the quality was anywhere from {}% to {}%: the step caps at {max}%",
            before.quality,
            max - 1
        ))
    } else {
        before.quality -= step.min(before.quality);
        None
    };
    vec![(before, 1.0, note)]
}

/// A catalyst step: the target with one less quality when the catalyst did not change.
fn undo_catalyst(data: &Data, target: &Item, catalyst: &str) -> Vec<(Item, f64, Option<String>)> {
    let Some(wanted) = data.catalyst_by_name(catalyst) else {
        return Vec::new();
    };
    let Some(have) = target.catalyst else {
        return Vec::new();
    };
    if have != wanted || target.quality == 0 {
        // A different catalyst before means the quality type and its value both changed, which
        // the target does not say.
        return Vec::new();
    }
    let mut before = target.clone();
    before.quality -= 1;
    vec![(before, 1.0, None)]
}
