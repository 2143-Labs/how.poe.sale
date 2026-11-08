//! The steps that could have produced an item, read backwards over abstract items.
//!
//! An item's untracked modifiers are counted rather than named: an `AbsItem` holds the tracked
//! modifiers and, per side, how many others sit beside them. Each supported method contributes
//! the inputs that could have produced the item, with the chance it did. A row's chance is
//! conditional on its own input, so different rows are not a partition of one event.
//!
//! A row whose input differs from the item by an untracked slot ("any prefix", "any suffix") is
//! the chance the step acted on that side: the chance the orb added a modifier of that side, or
//! the chance the annulment took one of the untracked modifiers sitting there. Each removable
//! modifier of an annulment's input is equally likely (claim `annul.uniform`), so that chance is
//! the untracked count over every removable modifier.

use std::collections::{BTreeMap, BTreeSet};

use crate::apply::{self, Params};
use crate::data::{Cost, Data, Rarity, Side};
use crate::item::{AbsSpec, AffixSpec, Item, ItemError, ItemSpec, MetaMod, SlotSpec};
use crate::method::{Method, Op, Pool, Require, SideReq};

/// An item whose untracked modifiers are counted, not named: `item` holds only the tracked ones.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbsItem {
    /// Prefixes and suffixes hold the tracked modifiers only.
    pub item: Item,
    /// Untracked modifiers per side, in `Side::index` order (prefix, suffix).
    pub any: [u8; 2],
    /// The tracked modifiers that stand for their family at that tier or higher.
    pub better: BTreeSet<u32>,
}

/// A tracked modifier: an exact tier, or its family at that tier or higher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub m: u32,
    pub side: Side,
    pub mod_type: u16,
    pub level: u8,
    pub or_better: bool,
}

/// The most modifiers one chart can track.
pub const MAX_TRACKED: usize = 8;

impl Slot {
    pub fn from_spec(data: &Data, s: &SlotSpec) -> Result<Slot, String> {
        let m = data
            .mod_by_id(&s.id)
            .ok_or_else(|| format!("unknown modifier \"{}\"", s.id))?;
        let md = data.mod_at(m);
        let side = md
            .side
            .ok_or_else(|| format!("\"{}\" is not a prefix or suffix", s.id))?;
        Ok(Slot {
            m,
            side,
            mod_type: md.mod_type,
            level: md.level,
            or_better: s.or_better,
        })
    }

    /// Whether `m` fills this slot: the same family on the same side, at this tier or higher. A
    /// tier is only ever compared with tiers of its own kind: an essence-only modifier shares its
    /// family with the natural tiers and sits at a high level, yet rolls below the top natural
    /// tier (`LocalAddedPhysicalDamageEssenceNew7` against `LocalAddedPhysicalDamage9`).
    pub fn matches(&self, data: &Data, m: u32) -> bool {
        let md = data.mod_at(m);
        md.side == Some(self.side)
            && md.mod_type == self.mod_type
            && (m == self.m
                || (self.or_better
                    && md.level >= self.level
                    && md.kind == data.mod_at(self.m).kind))
    }

    /// The slot as the page reads it: the tier's text, plus "or better" for a family.
    pub fn text(&self, data: &Data) -> String {
        let text = data.mod_at(self.m).text.lines().next().unwrap_or_default();
        if self.or_better {
            format!("{text} or better")
        } else {
            text.to_string()
        }
    }
}

/// The modifiers a chart tracks: the root's own slots, then the ones watched for.
pub fn tracked(data: &Data, root: &AbsSpec, watch: &[SlotSpec]) -> Result<Vec<Slot>, String> {
    let mut specs: Vec<&SlotSpec> = root
        .prefixes
        .iter()
        .chain(&root.suffixes)
        .chain(watch)
        .collect();
    specs.sort_by(|a, b| a.id.cmp(&b.id));
    specs.dedup_by(|a, b| a.id == b.id);
    if specs.len() > MAX_TRACKED {
        return Err(format!("track at most {MAX_TRACKED} modifiers"));
    }
    let slots = specs
        .into_iter()
        .map(|s| Slot::from_spec(data, s))
        .collect::<Result<Vec<Slot>, String>>()?;
    for (i, a) in slots.iter().enumerate() {
        for b in &slots[i + 1..] {
            if a.side == b.side && a.mod_type == b.mod_type {
                return Err(format!(
                    "\"{}\" and \"{}\" track the same family",
                    data.mod_at(a.m).id,
                    data.mod_at(b.m).id
                ));
            }
        }
    }
    Ok(slots)
}

/// A node's id: its class, as the spec the page round-trips. `to_spec` sorts the slots, so
/// equal classes give equal keys and forward and backward steps meet in one node.
pub fn abs_key(data: &Data, a: &AbsItem) -> String {
    serde_json::to_string(&a.to_spec(data)).unwrap_or_default()
}

/// One input that could have produced the item, and the chance the method turns it into it.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub method: String,
    pub name: String,
    pub p: f64,
    /// The 95% range when `p` comes from a simulation.
    pub interval: Option<[f64; 2]>,
    pub input: AbsItem,
    pub note: Option<String>,
    pub cost: Vec<Cost>,
}

/// Every step found, and the asked-for methods that show no input.
#[derive(Clone, Debug, PartialEq)]
pub struct Preimages {
    pub steps: Vec<Step>,
    /// (method id, why it shows no input), in bundle order, then the ids the bundle lacks.
    pub skipped: Vec<(String, String)>,
}

impl AbsItem {
    pub fn from_spec(data: &Data, spec: &AbsSpec) -> Result<AbsItem, ItemError> {
        let affix = |s: &SlotSpec| AffixSpec {
            id: s.id.clone(),
            values: None,
            fractured: false,
        };
        let mut better = BTreeSet::new();
        for s in spec.prefixes.iter().chain(&spec.suffixes) {
            if s.or_better
                && let Some(m) = data.mod_by_id(&s.id)
            {
                better.insert(m);
            }
        }
        let item = Item::from_spec(
            data,
            &ItemSpec {
                base: spec.base.clone(),
                item_level: spec.item_level,
                rarity: spec.rarity,
                influences: spec.influences.clone(),
                quality: 0,
                catalyst: None,
                defences: None,
                implicits: Vec::new(),
                enchants: Vec::new(),
                prefixes: spec.prefixes.iter().map(affix).collect(),
                suffixes: spec.suffixes.iter().map(affix).collect(),
                sockets: String::new(),
                corrupted: spec.corrupted,
                mirrored: false,
                split: false,
                synthesised: false,
                foreseen: false,
                memory_strands: 0,
                intangibility: 0,
                imprint: None,
                note: None,
            },
        )?;
        Ok(AbsItem {
            item,
            any: [spec.any_prefixes, spec.any_suffixes],
            better,
        })
    }

    pub fn to_spec(&self, data: &Data) -> AbsSpec {
        let spec = self.item.to_spec(data);
        let slots = |list: Vec<AffixSpec>| -> Vec<SlotSpec> {
            let mut out: Vec<SlotSpec> = list
                .into_iter()
                .filter_map(|a| {
                    let m = data.mod_by_id(&a.id)?;
                    Some(SlotSpec {
                        id: a.id,
                        or_better: self.better.contains(&m),
                    })
                })
                .collect();
            out.sort_by(|a, b| a.id.cmp(&b.id));
            out
        };
        AbsSpec {
            base: spec.base,
            item_level: spec.item_level,
            rarity: spec.rarity,
            influences: spec.influences,
            prefixes: slots(spec.prefixes),
            suffixes: slots(spec.suffixes),
            any_prefixes: self.any[0],
            any_suffixes: self.any[1],
            corrupted: self.item.corrupted,
        }
    }

    /// Tracked plus untracked modifiers.
    pub fn mods(&self) -> usize {
        self.counts().iter().sum()
    }

    /// Tracked plus untracked modifiers per side.
    fn counts(&self) -> [usize; 2] {
        Side::BOTH.map(|s| self.item.affixes(s).len() + usize::from(self.any[s.index()]))
    }

    /// Whether a side holds as many modifiers as the rarity allows; an untracked slot takes
    /// room even though it names no modifier.
    fn full(&self, data: &Data, side: Side) -> bool {
        self.counts()[side.index()] >= usize::from(limits(data, &self.item)[side.index()])
    }

    /// `Item::validate`, and the untracked slots fitting beside the tracked modifiers.
    pub fn validate(&self, data: &Data) -> Result<(), String> {
        self.item.validate(data).map_err(|e| e.to_string())?;
        let limit = limits(data, &self.item);
        let counts = self.counts();
        for side in Side::BOTH {
            let (n, held) = (counts[side.index()], limit[side.index()]);
            if n > usize::from(held) {
                let name = match side {
                    Side::Prefix => "prefixes",
                    Side::Suffix => "suffixes",
                };
                return Err(format!("{name}: {n} modifiers, the rarity holds {held}"));
            }
        }
        Ok(())
    }
}

/// The explicit modifiers per side the item's rarity holds: none on a normal item.
fn limits(data: &Data, item: &Item) -> [u8; 2] {
    if item.rarity == Rarity::Normal {
        [0, 0]
    } else {
        item.limits(data)
    }
}

/// An input a rule found: the item, the chance, the 95% range of a simulated chance, and what
/// the rule had to assume.
type Found = (AbsItem, f64, Option<[f64; 2]>, Option<String>);

/// Every input the asked-for methods could have turned into `target`, and why the others show
/// none.
pub fn preimages(data: &Data, target: &AbsItem, tracked: &[Slot], methods: &[&str]) -> Preimages {
    let mut out = Preimages {
        steps: Vec::new(),
        skipped: Vec::new(),
    };
    for method in data
        .methods
        .iter()
        .filter(|m| methods.contains(&m.id.as_str()))
    {
        let Some((found, empty)) = rule(data, method, target, tracked) else {
            out.skipped
                .push((method.id.clone(), "this page does not model it yet".into()));
            continue;
        };
        let cost = apply::cost(data, method, &Params::default()).unwrap_or_default();
        let mut rejected: Option<String> = None;
        let mut kept = false;
        for (input, p, interval, note) in found {
            match admits(data, method, &input) {
                Ok(()) => {
                    kept = true;
                    out.steps.push(Step {
                        method: method.id.clone(),
                        name: method.name.clone(),
                        p,
                        interval,
                        input,
                        note,
                        cost: cost.clone(),
                    });
                }
                Err(why) => {
                    rejected.get_or_insert(why);
                }
            }
        }
        if !kept {
            out.skipped
                .push((method.id.clone(), rejected.unwrap_or(empty)));
        }
    }
    for id in methods {
        if data.method(id).is_none() {
            out.skipped
                .push((id.to_string(), "this method is not in the bundle".into()));
        }
    }
    out
}

/// The inputs one method could have started from, and why it found none; `None` for a method
/// this page does not model.
fn rule(
    data: &Data,
    method: &Method,
    target: &AbsItem,
    tracked: &[Slot],
) -> Option<(Vec<Found>, String)> {
    Some(match method.id.as_str() {
        "currency/exalted" => exalted(data, target, tracked),
        "currency/annulment" => annulment(data, method, target),
        "bench/add/StrMasterItemGenerationCannotChangePrefixes"
        | "bench/add/DexMasterItemGenerationCannotChangeSuffixes" => bench(data, method, target),
        id if id.starts_with("essence/") => essence(data, method, target, tracked),
        _ => return None,
    })
}

/// Whether the method takes `input`. A concrete input runs `apply::check`; with an untracked
/// slot that check would miscount the explicits and the room, so only the requirements an
/// untracked slot cannot change are read, and the rule's own construction stands for the rest.
fn admits(data: &Data, method: &Method, input: &AbsItem) -> Result<(), String> {
    if input.any == [0, 0] {
        return apply::check(
            data,
            method,
            std::slice::from_ref(&input.item),
            &Params::default(),
        )
        .map_err(|u| u.reason);
    }
    let name = &method.name;
    let item = &input.item;
    let class = item.class(data);
    for require in &method.requires {
        match require {
            Require::Rarity { any_of } if !any_of.contains(&item.rarity) => {
                let names: Vec<&str> = any_of.iter().map(|r| r.name()).collect();
                return Err(format!("{name} needs a {} item", names.join(" or ")));
            }
            Require::Category { any_of } if !any_of.contains(&class.category) => {
                return Err(format!("{name} does not work on {}", class.id));
            }
            Require::Class { any_of } if !any_of.contains(&class.id) => {
                return Err(format!("{name} does not work on {}", class.id));
            }
            Require::NotCorrupted if item.corrupted => {
                return Err("a corrupted item cannot be changed".into());
            }
            Require::NotMirrored if item.mirrored => {
                return Err("a mirrored item cannot be changed".into());
            }
            Require::NotUnique if item.rarity == Rarity::Unique => {
                return Err(format!("{name} does not work on a unique item"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Why a rule finds nothing on an item whose every side is full, when the rarity is the cause.
fn rarity_reason(data: &Data, target: &AbsItem) -> Option<&'static str> {
    match limits(data, &target.item) {
        [0, 0] => Some("a normal item holds no explicit modifiers"),
        [1, 1] if target.counts() == [1, 1] => Some("a magic item holds one prefix and one suffix"),
        _ => None,
    }
}

/// Whether both sides are full.
fn full_both(data: &Data, target: &AbsItem) -> bool {
    Side::BOTH.into_iter().all(|s| target.full(data, s))
}

/// The Exalted Orb's pool on `item` (`apply::pool_candidates`), less any side that untracked
/// slots fill.
fn add_pool(data: &Data, item: &AbsItem) -> Vec<(u32, f64)> {
    apply::pool_candidates(data, &item.item, &Pool::Natural, SideReq::Any, false, false)
        .into_iter()
        .filter(|(m, _)| data.mod_at(*m).side.is_some_and(|s| !item.full(data, s)))
        .collect()
}

/// The chance the Exalted Orb draws each candidate from `item`: `apply::add_candidates` over
/// `add_pool`. With no untracked slot the two are the same list.
fn add_chances(data: &Data, item: &AbsItem) -> Vec<(u32, f64)> {
    apply::side_first(data, add_pool(data, item))
}

/// The chance the orb draws a modifier of `side` that no tracked slot names: the sum of that
/// side's `add_chances` over the untracked candidates. A tracked modifier that is not on the
/// item yet is still tracked, so it counts as a named outcome rather than an untracked one.
fn untracked_share(data: &Data, item: &AbsItem, tracked: &[Slot], side: Side) -> f64 {
    add_chances(data, item)
        .iter()
        .filter(|(m, _)| data.mod_at(*m).side == Some(side))
        .filter(|(m, _)| !tracked.iter().any(|slot| slot.matches(data, *m)))
        .map(|(_, q)| *q)
        .sum()
}

/// An Exalted Orb: the item without a tracked modifier, at the chance of drawing it, or with
/// one untracked slot fewer, at the chance of drawing that side.
fn exalted(data: &Data, target: &AbsItem, tracked: &[Slot]) -> (Vec<Found>, String) {
    let mut out = Vec::new();
    for side in Side::BOTH {
        for (index, affix) in target.item.affixes(side).iter().enumerate() {
            // An add places an unfractured modifier: a fractured one was made fractured later.
            if affix.fractured {
                continue;
            }
            let mut item = target.item.clone();
            item.affixes_mut(side).remove(index);
            item.canonical(data);
            let before = AbsItem {
                better: target.better.clone(),
                item,
                any: target.any,
            };
            let added = affix.roll.m;
            let slot = tracked.iter().find(|slot| slot.m == added);
            let chances = add_chances(data, &before);
            let p: f64 = match slot {
                // A family slot is filled by any tier of its family at or above its own.
                Some(slot) => chances
                    .iter()
                    .filter(|(m, _)| slot.matches(data, *m))
                    .map(|(_, q)| *q)
                    .sum(),
                None => chances
                    .iter()
                    .find(|(m, _)| *m == added)
                    .map_or(0.0, |(_, q)| *q),
            };
            if p > 0.0 {
                out.push((before, p, None, None));
            }
        }
    }
    for side in Side::BOTH {
        if target.any[side.index()] == 0 {
            continue;
        }
        let mut before = target.clone();
        before.any[side.index()] -= 1;
        // The tracked modifiers stayed on `before`, so every candidate left is untracked: their
        // summed chance is the chance the orb filled one of the untracked slots.
        let p = untracked_share(data, &before, tracked, side);
        if p > 0.0 {
            out.push((before, p, None, None));
        }
    }
    // A full item can still be an Exalted Orb's result (its last slot), so fullness is no reason
    // unless the rarity holds nothing to add.
    let empty = match rarity_reason(data, target) {
        Some(why) if full_both(data, target) => why,
        _ => "no modifier on the item could have been the one an Exalted Orb added",
    };
    (out, empty.to_string())
}

/// An Orb of Annulment: the item with one more untracked modifier on a side, at the chance the
/// orb took one of the untracked modifiers on that side: each removable modifier of the input is
/// equally likely (claim `annul.uniform`).
fn annulment(data: &Data, method: &Method, target: &AbsItem) -> (Vec<Found>, String) {
    // The input held one more modifier than the result on the side the orb took from, so that
    // side had room in the result, and a side a metamod locks gives nothing up.
    let locked = Side::BOTH.map(|s| target.item.locked(data, s));
    let open = Side::BOTH.map(|s| !locked[s.index()] && !target.full(data, s));
    let removable = apply::changeable(data, method, &target.item);
    // Untracked slots are counted as removable: the model does not name them.
    let untracked: usize = Side::BOTH
        .into_iter()
        .filter(|s| !locked[s.index()])
        .map(|s| usize::from(target.any[s.index()]))
        .sum();
    let total = removable[0] + removable[1] + untracked + 1;
    let note = (target.item.has_meta(data, MetaMod::NoAttack)
        || target.item.has_meta(data, MetaMod::NoCaster))
    .then(|| "untracked modifiers are counted as removable".to_string());
    let mut out = Vec::new();
    for side in Side::BOTH {
        let i = side.index();
        if !open[i] {
            continue;
        }
        let mut before = target.clone();
        before.any[i] += 1;
        let taken = before.any[i] as usize;
        out.push((before, taken as f64 / total as f64, None, note.clone()));
    }
    let empty = if full_both(data, target) {
        rarity_reason(data, target).unwrap_or(
            "the item already holds every modifier it can, so nothing could have been removed from it",
        )
    } else {
        "the item has no modifier an Orb of Annulment could have left"
    };
    (out, empty.to_string())
}

/// A bench craft of one named modifier: the item without it, for certain.
fn bench(data: &Data, method: &Method, target: &AbsItem) -> (Vec<Found>, String) {
    let id = method.ops.iter().find_map(|op| match op {
        Op::AddMod { r#mod } => Some(r#mod.as_str()),
        _ => None,
    });
    let Some(m) = id.and_then(|id| data.mod_by_id(id)) else {
        return (Vec::new(), "the bench craft names no modifier".into());
    };
    let mut out = Vec::new();
    for side in Side::BOTH {
        for (index, affix) in target.item.affixes(side).iter().enumerate() {
            if affix.roll.m != m || affix.fractured {
                continue;
            }
            let mut item = target.item.clone();
            item.affixes_mut(side).remove(index);
            item.canonical(data);
            out.push((
                AbsItem {
                    better: target.better.clone(),
                    item,
                    any: target.any,
                },
                1.0,
                None,
                None,
            ));
        }
    }
    let text = data.mod_at(m).text.lines().next().unwrap_or_default();
    (out, format!("the item does not hold {text}"))
}

/// An essence reforge: a normal item of the same base, at the chance the reforge leaves the
/// target. With nothing tracked but the essence's own modifier the chance is the fill's exact
/// distribution; with other modifiers tracked it is a seeded simulation of the whole reforge.
fn essence(
    data: &Data,
    method: &Method,
    target: &AbsItem,
    tracked: &[Slot],
) -> (Vec<Found>, String) {
    let none = |why: &str| (Vec::new(), why.to_string());
    let item = &target.item;
    if item.rarity != Rarity::Rare {
        return none("an essence leaves a rare item");
    }
    let essence = method.ops.iter().find_map(|op| match op {
        Op::AddEssence { essence } => data.essence_by_name(essence),
        _ => None,
    });
    let class = data.base_at(item.base).class;
    let guaranteed = essence.and_then(|e| {
        data.essences[e as usize]
            .mods
            .iter()
            .find(|(c, _)| *c == class)
            .map(|&(_, m)| m)
    });
    let held = guaranteed
        .and_then(|m| Some((m, data.mod_at(m).side?)))
        .filter(|&(m, side)| item.affixes(side).iter().any(|a| a.roll.m == m));
    let Some((_, side)) = held else {
        let name = essence.map_or(method.name.as_str(), |e| {
            data.essences[e as usize].name.as_str()
        });
        return none(&format!("the item does not hold {name}'s modifier"));
    };
    let counts = target.counts();
    let n = counts[0] + counts[1];
    if !(4..=6).contains(&n) {
        return none(&format!(
            "the item has {n} explicit modifiers; a reforge leaves 4, 5 or 6"
        ));
    }
    let mut input = item.clone();
    input.rarity = Rarity::Normal;
    input.prefixes.clear();
    input.suffixes.clear();
    let base = &data.base_at(item.base).name;
    let note = format!(
        "the reforge clears the item first, so any normal or rare {base} at this item level gives this outcome"
    );
    let start = AbsItem {
        better: BTreeSet::new(),
        item: input,
        any: [0, 0],
    };
    // The essence's modifier is the only tracked one: the fill's own distribution answers it.
    if item.explicits().count() == 1 {
        let fill = method.ops.iter().find_map(|op| match op {
            Op::Fill { fill } => Some(fill.counts.clone()),
            _ => None,
        });
        let fill = fill.unwrap_or_else(|| vec![(4, 8), (5, 3), (6, 1)]);
        let mut placed = [0u8; 2];
        placed[side.index()] = 1;
        let want = [counts[0] as u8, counts[1] as u8];
        let p = compositions(&fill, placed, limits(data, item))
            .into_iter()
            .find(|(c, _)| *c == want)
            .map_or(0.0, |(_, p)| p);
        if p <= 0.0 {
            return none("the reforge cannot leave that many modifiers on each side");
        }
        return (vec![(start, p, None, Some(note))], String::new());
    }
    // Other tracked modifiers: simulate the reforge from the cleared item and read the class off.
    let (p, interval) = simulated_class(data, &start, method, target, tracked);
    if p <= 0.0 {
        return none(&format!(
            "the reforge did not reach this item in {} runs",
            crate::forward::SIMULATIONS
        ));
    }
    (vec![(start, p, interval, Some(note))], String::new())
}

/// The simulated chance that `method` turned `start` into `target`'s class, and its 95% range.
fn simulated_class(
    data: &Data,
    start: &AbsItem,
    method: &Method,
    target: &AbsItem,
    tracked: &[Slot],
) -> (f64, Option<[f64; 2]>) {
    let node = crate::forward::Node {
        members: vec![(start.item.clone(), 1.0)],
        approx: false,
    };
    let seed = crate::forward::seed_for(&abs_key(data, target), &method.id);
    let applied = crate::forward::apply_node(data, &node, method, tracked, seed);
    let want = abs_key(data, target);
    applied
        .groups
        .iter()
        .find(|g| match &g.kind {
            crate::forward::Kind::Item(a) => abs_key(data, a) == want,
            _ => false,
        })
        .map_or((0.0, None), |g| (g.p, g.interval))
}

/// The total-modifier distribution of a reforge whose guaranteed modifier is already placed on
/// `placed[side]`: (final counts per side, probability). Claim `reforge.side-first`.
fn compositions(counts: &[(u8, u32)], placed: [u8; 2], limits: [u8; 2]) -> Vec<([u8; 2], f64)> {
    let total: u32 = counts.iter().map(|(_, w)| *w).sum();
    let mut out: BTreeMap<[u8; 2], f64> = BTreeMap::new();
    for &(n, w) in counts {
        if n < 1 {
            continue;
        }
        let mut dist: BTreeMap<[u8; 2], f64> = BTreeMap::from([(placed, 1.0)]);
        for _ in 0..n.saturating_sub(1) {
            let mut next: BTreeMap<[u8; 2], f64> = BTreeMap::new();
            for (state, mass) in &dist {
                let open: Vec<usize> = [0usize, 1]
                    .into_iter()
                    .filter(|s| state[*s] < limits[*s])
                    .collect();
                for &s in &open {
                    let mut after = *state;
                    after[s] += 1;
                    *next.entry(after).or_default() += mass / open.len() as f64;
                }
            }
            dist = next;
        }
        for (state, mass) in dist {
            *out.entry(state).or_default() += mass * f64::from(w) / f64::from(total);
        }
    }
    out.into_iter().collect()
}
