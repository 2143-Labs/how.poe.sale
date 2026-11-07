//! `apply(item(s), method) → outcome`: checks a method's requirements against real items (with
//! the failing reason) and runs its ops over a distribution of item states, merging equal states
//! after every op. The outcome is one flat distribution of distinct leaves: an item, the two items
//! of a Mirror, a destroyed item, a player's choice (unveiling; each option is itself an outcome),
//! or `Pending`. Single draws are exact, weighted by `pool`. A reforge applies its certain part
//! (rarity, the clear, the guaranteed modifier) and stops at its `Fill`; socket outcomes and
//! rules not modelled yet stop at the item before their step. Whether an affix can change is
//! decided in one place (`Guard`), from the method's `Protection`. An op with nothing to do
//! outside a `Chance` branch refuses the method (owner, Q1); inside a branch it leaves that
//! branch's item unchanged. `check` runs the same program as `apply`, so the two agree.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::data::{Category, Cost, Data, Eldritch, ModData, ModKind, Rarity, Side};
use crate::item::{Affix, Item, MetaMod, Roll};
use crate::method::{
    Element, Family, Method, Op, PairRequire, PendingReason, Policy, Pool, Protection,
    RemoveFilter, Require, SideReq, ValueScope, Weighting,
};
use crate::pool;

/// More states or leaves than this and `apply` reports `Pending { reason: TooLarge }`.
pub const MAX_BRANCHES: usize = 100_000;

/// The veiled placeholder modifiers, prefix then suffix.
pub const VEILED: [&str; 2] = ["VeiledPrefix", "VeiledSuffix"];

/// What a method needs besides its items.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Params {
    /// Fossil indices (`Bundle::fossils`) for a fossil method: one to four, each once.
    pub fossils: Vec<u16>,
}

/// Why a method cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmet {
    pub reason: String,
}

impl std::fmt::Display for Unmet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

fn unmet<T>(reason: impl Into<String>) -> Result<T, Unmet> {
    Err(Unmet {
        reason: reason.into(),
    })
}

/// Every distinct result of a method with its probability, most likely first; the masses sum to
/// 1. One item reached through different draw orders is one leaf.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub leaves: Vec<(f64, Leaf)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Leaf {
    Item(Item),
    /// Every item a multi-item result leaves (a Mirror: the item and its mirrored copy).
    Items(Vec<Item>),
    Destroyed,
    /// The player picks one option; each option is the outcome of the ops after the choice.
    Choice(Vec<Outcome>),
    Pending(Pending),
}

/// A result the model stops short of.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    /// Every input the method was given, in order.
    pub inputs: Vec<Item>,
    pub params: Params,
    /// Where the model stopped.
    pub item: Item,
    /// `true`: every result holds `item` as it is plus what the rest adds (a reforge's `Fill`
    /// only adds). `false`: `item` is the item before a step the model does not run, and says
    /// nothing about the result.
    pub certain: bool,
    /// The ops still to run, starting with the one the model stopped at.
    pub ops: Vec<Op>,
    pub reason: PendingReason,
}

impl Pending {
    fn order(&self, other: &Pending) -> Ordering {
        let ops = |p: &Pending| serde_json::to_string(&p.ops).unwrap_or_default();
        self.item
            .cmp(&other.item)
            .then_with(|| self.certain.cmp(&other.certain))
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| ops(self).cmp(&ops(other)))
            .then_with(|| self.inputs.cmp(&other.inputs))
            .then_with(|| self.params.cmp(&other.params))
    }
}

impl Leaf {
    fn rank(&self) -> u8 {
        match self {
            Leaf::Item(_) => 0,
            Leaf::Items(_) => 1,
            Leaf::Destroyed => 2,
            Leaf::Choice(_) => 3,
            Leaf::Pending(_) => 4,
        }
    }

    /// A total order: variant, then contents.
    pub fn order(&self, other: &Leaf) -> Ordering {
        match (self, other) {
            (Leaf::Item(a), Leaf::Item(b)) => a.cmp(b),
            (Leaf::Items(a), Leaf::Items(b)) => a.cmp(b),
            (Leaf::Choice(a), Leaf::Choice(b)) => a
                .iter()
                .zip(b)
                .map(|(x, y)| x.order(y))
                .find(|o| o.is_ne())
                .unwrap_or_else(|| a.len().cmp(&b.len())),
            (Leaf::Pending(a), Leaf::Pending(b)) => a.order(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }

    /// Every item the leaf holds: its item, a Mirror's items, a Pending's item, and every item
    /// of every choice option.
    pub fn items(&self) -> Vec<&Item> {
        match self {
            Leaf::Item(i) => vec![i],
            Leaf::Items(v) => v.iter().collect(),
            Leaf::Destroyed => Vec::new(),
            Leaf::Choice(options) => options.iter().flat_map(Outcome::items).collect(),
            Leaf::Pending(p) => vec![&p.item],
        }
    }
}

impl Outcome {
    /// Merges equal leaves (summing their chances) and sorts them by chance descending, then
    /// leaf order.
    fn from_leaves(raw: Vec<(f64, Leaf)>) -> Outcome {
        let mut items: BTreeMap<Item, f64> = BTreeMap::new();
        let mut other: Vec<(f64, Leaf)> = Vec::new();
        for (p, leaf) in raw {
            match leaf {
                Leaf::Item(i) => *items.entry(i).or_insert(0.0) += p,
                l => match other.iter_mut().find(|(_, x)| *x == l) {
                    Some((q, _)) => *q += p,
                    None => other.push((p, l)),
                },
            }
        }
        let mut leaves: Vec<(f64, Leaf)> = items
            .into_iter()
            .map(|(i, p)| (p, Leaf::Item(i)))
            .chain(other)
            .collect();
        leaves.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.order(&b.1)));
        Outcome { leaves }
    }

    /// The leaves' total chance (1 for every outcome `apply` returns).
    pub fn mass(&self) -> f64 {
        self.leaves.iter().map(|(p, _)| p).sum()
    }

    /// Every item any leaf holds.
    pub fn items(&self) -> Vec<&Item> {
        self.leaves.iter().flat_map(|(_, l)| l.items()).collect()
    }

    /// The item, when the outcome is one certain item.
    pub fn item(&self) -> Option<&Item> {
        match self.leaves.as_slice() {
            [(_, Leaf::Item(i))] => Some(i),
            _ => None,
        }
    }

    /// A total order over outcomes: their leaves in turn (chance, then leaf).
    pub fn order(&self, other: &Outcome) -> Ordering {
        self.leaves
            .iter()
            .zip(&other.leaves)
            .map(|((p, a), (q, b))| q.total_cmp(p).then_with(|| a.order(b)))
            .find(|o| o.is_ne())
            .unwrap_or_else(|| self.leaves.len().cmp(&other.leaves.len()))
    }
}

fn category_name(c: Category) -> &'static str {
    match c {
        Category::Weapon => "weapons",
        Category::Armour => "armour",
        Category::Jewellery => "jewellery",
        Category::Quiver => "quivers",
        Category::Jewel => "jewels",
        Category::AbyssJewel => "abyss jewels",
        Category::Flask => "flasks",
        Category::Tincture => "tinctures",
    }
}

/// The side an eldritch kind's orbs touch: Exarch = prefixes, Eater = suffixes.
fn dominant_side(data: &Data, item: &Item) -> Option<Side> {
    item.dominant(data).map(|e| match e {
        Eldritch::Exarch => Side::Prefix,
        Eldritch::Eater => Side::Suffix,
    })
}

/// The sides a `SideReq` allows on this item.
/// The sides a `SideReq` allows on this item.
pub fn sides(data: &Data, item: &Item, req: SideReq) -> Vec<Side> {
    match req {
        SideReq::Any => Side::BOTH.to_vec(),
        SideReq::Prefix => vec![Side::Prefix],
        SideReq::Suffix => vec![Side::Suffix],
        SideReq::Dominant => dominant_side(data, item).into_iter().collect(),
    }
}

/// The mods of the first Searing Exarch and first Eater of Worlds implicit.
fn conflict_pair(data: &Data, item: &Item) -> Option<(u32, u32)> {
    let find = |kind| {
        item.implicits
            .iter()
            .map(|r| r.m)
            .find(|&m| data.mod_at(m).kind == kind)
    };
    Some((find(ModKind::Exarch)?, find(ModKind::Eater)?))
}

/// The item's implicits after the Orb of Conflict: `raised` loses a tier and `lowered` gains
/// one, a lowered Lesser (tier 6) implicit being removed instead; a move with no such mod
/// leaves its implicit unchanged (claim `conflict.tiers`).
fn conflict_implicits(data: &Data, item: &Item, raised: u32, lowered: u32) -> Vec<Roll> {
    let tags = item.tags(data);
    item.implicits
        .iter()
        .filter_map(|roll| {
            let m = data.mod_at(roll.m);
            let next = if roll.m == raised {
                eldritch_neighbour(data, &tags, m, -1)
            } else if roll.m == lowered {
                if m.eldritch_tier == Some(6) {
                    return None;
                }
                eldritch_neighbour(data, &tags, m, 1)
            } else {
                None
            };
            Some(next.map_or_else(|| roll.clone(), |m| Roll { m, values: None }))
        })
        .collect()
}

/// The eldritch mod one tier stronger (`step` -1) or weaker (+1) with the same kind, mod type
/// and spawn on the item's tags; `None` when there is no such tier.
fn eldritch_neighbour(data: &Data, tags: &[u16], m: &ModData, step: i8) -> Option<u32> {
    let target = i16::from(m.eldritch_tier?) + i16::from(step);
    if !(1..=6).contains(&target) {
        return None;
    }
    data.mods_of_kind(m.kind).iter().copied().find(|&i| {
        let n = data.mod_at(i);
        n.mod_type == m.mod_type
            && n.eldritch_tier == Some(target as u8)
            && pool::weight(n, tags) > 0.0
    })
}

fn open_text(side: Option<Side>) -> &'static str {
    match side {
        Some(Side::Prefix) => "needs an open prefix",
        Some(Side::Suffix) => "needs an open suffix",
        None => "needs an open affix",
    }
}

/// The text of the first explicit carrying the metamod, for refusals that name it.
fn metamod_text(data: &Data, item: &Item, meta: MetaMod) -> Option<String> {
    let g = data.group_by_name(meta.group())?;
    item.explicits()
        .map(|a| data.mod_at(a.roll.m))
        .find(|m| m.groups.contains(&g))
        .map(|m| {
            m.text
                .lines()
                .next()
                .filter(|t| !t.is_empty())
                .unwrap_or(meta.group())
                .to_string()
        })
}

/// A mod's first group, by name, for refusals.
fn group_conflict(data: &Data, item: &Item, m: u32) -> Option<String> {
    let have = item.explicit_groups(data);
    data.mod_at(m)
        .groups
        .iter()
        .find(|g| have.contains(g))
        .map(|&g| format!("the item already has a {} modifier", data.group(g)))
}

/// The essence's modifier for the item's class.
fn essence_mod(data: &Data, item: &Item, essence: &str) -> Option<u32> {
    let class = data.base_at(item.base).class;
    data.essences[data.essence_by_name(essence)? as usize]
        .mods
        .iter()
        .find(|(c, _)| *c == class)
        .map(|&(_, m)| m)
}

/// Why a pool has no candidate, in the same words `AddRandom` refuses with.
fn pool_empty_text(pool: &Pool) -> String {
    match pool {
        Pool::Natural => "no modifier can roll on this item".into(),
        Pool::Influence { influence } => {
            format!("no {} modifier can roll on this item", influence.name())
        }
        Pool::Influenced => "no influenced modifier can roll on this item".into(),
        Pool::Tag { tag } => format!("no {tag} modifier can roll on this item"),
        Pool::Ducat { .. } => "no modifier from this ducat can roll on this item".into(),
    }
}

/// The weighted candidate mods `AddRandom` draws from, at item level, with the attack and
/// caster mods a Cannot Roll metamod bars left out (claim `metamod.cannot-roll`). `ignore_limits`
/// keeps a mod whose side has no room, and `ignore_groups` keeps a mod whose group the item
/// already holds: a requirement asks whether the modifier can roll on the item at all, before the
/// reforge clears it, while an add must respect both. An add draws from this list after
/// `add_candidates` picks the side.
pub fn pool_candidates(
    data: &Data,
    item: &Item,
    pool: &Pool,
    side: SideReq,
    ignore_limits: bool,
    ignore_groups: bool,
) -> Vec<(u32, f64)> {
    let tags = item.tags(data);
    let base = data.base_at(item.base);
    let groups = item.explicit_groups(data);
    let allowed = sides(data, item, side);
    let open = [
        room(data, item, Side::Prefix),
        room(data, item, Side::Suffix),
    ];
    let level_cap = if item.has_meta(data, MetaMod::Level28) {
        item.item_level.min(28)
    } else {
        item.item_level
    };
    let barred: Vec<u16> = [(MetaMod::NoAttack, "attack"), (MetaMod::NoCaster, "caster")]
        .into_iter()
        .filter(|(meta, _)| item.has_meta(data, *meta))
        .filter_map(|(_, name)| data.tag_by_name(name))
        .collect();
    let (kind, tag) = match pool {
        Pool::Ducat { tag } => (ModKind::Ducat, data.tag_by_name(tag)),
        Pool::Tag { tag } => (ModKind::Natural, data.tag_by_name(tag)),
        _ => (ModKind::Natural, None),
    };
    if matches!(pool, Pool::Tag { .. } | Pool::Ducat { .. }) && tag.is_none() {
        return Vec::new();
    }
    let mut weighted = Vec::new();
    for &i in data.mods_of_kind(kind) {
        let m = data.mod_at(i);
        let Some(s) = m.side else { continue };
        let pool_ok = match pool {
            Pool::Natural | Pool::Ducat { .. } => true,
            Pool::Influence { influence } => m.influence == Some(*influence),
            Pool::Influenced => m.influence.is_some_and(|i| item.has_influence(i)),
            Pool::Tag { .. } => tag.is_some_and(|t| m.tags.contains(&t)),
        };
        if !pool_ok
            || (kind != ModKind::Ducat && m.domain != base.domain.repoe())
            || m.level > level_cap
            || !allowed.contains(&s)
            || (!ignore_limits && open[s.index()] == 0)
            || (!ignore_groups && m.groups.iter().any(|g| groups.contains(g)))
            || barred.iter().any(|t| m.tags.contains(t))
        {
            continue;
        }
        let w = match pool {
            Pool::Ducat { .. } => tag.map_or(0.0, |t| pool::weight_of_tag(m, t)),
            _ => pool::weight(m, &tags),
        };
        weighted.push((i, w));
    }
    pool::normalise(weighted)
}

/// The candidates an `AddRandom` draws from, with their probabilities: the pool's mods, a side
/// with room picked first, uniformly among them, and then a modifier on that side by weight
/// (claim `add.side-first`, owner Q1). One side alone leaves the probabilities as the pooled
/// weights. The inverse solver prices what a preimage would have added from the same list.
pub fn add_candidates(
    data: &Data,
    item: &Item,
    pool: &Pool,
    side: SideReq,
    ignore_limits: bool,
) -> Vec<(u32, f64)> {
    side_first(
        data,
        pool_candidates(data, item, pool, side, ignore_limits, false),
    )
}

/// The pooled weights as one share per side, split inside the side by weight.
pub(crate) fn side_first(data: &Data, candidates: Vec<(u32, f64)>) -> Vec<(u32, f64)> {
    let mut by_side: [Vec<(u32, f64)>; 2] = Default::default();
    for (m, w) in candidates {
        if let Some(side) = data.mod_at(m).side {
            by_side[side.index()].push((m, w));
        }
    }
    let sides = by_side.iter().filter(|side| !side.is_empty()).count() as f64;
    let mut out = Vec::new();
    for side in by_side {
        let total: f64 = side.iter().map(|&(_, w)| w).sum();
        for (m, w) in side {
            out.push((m, w / total / sides));
        }
    }
    out
}

fn check_one(data: &Data, r: &Require, item: &Item, params: &Params) -> Result<(), Unmet> {
    let class = item.class(data);
    let (exarch, eater) = item.eldritch(data);
    match r {
        Require::Rarity { any_of } => {
            if !any_of.contains(&item.rarity) {
                let names: Vec<&str> = any_of.iter().map(|r| r.name()).collect();
                return unmet(format!("needs a {} item", names.join(" or ")));
            }
        }
        Require::Category { any_of } => {
            if !any_of.contains(&class.category) {
                let names: Vec<&str> = any_of.iter().map(|c| category_name(*c)).collect();
                return unmet(format!("only works on: {}", names.join(", ")));
            }
        }
        Require::Class { any_of } => {
            if !any_of.contains(&class.id) {
                let names: Vec<&str> = any_of
                    .iter()
                    .map(|id| {
                        data.class_by_id(id)
                            .map_or(id.as_str(), |c| data.classes[c as usize].name.as_str())
                    })
                    .collect();
                return unmet(format!("only works on: {}", names.join(", ")));
            }
        }
        Require::NotCorrupted if item.corrupted => return unmet("the item is corrupted"),
        Require::Corrupted if !item.corrupted => return unmet("needs a corrupted item"),
        Require::NotMirrored if item.mirrored => return unmet("the item is mirrored"),
        Require::Mirrored if !item.mirrored => return unmet("needs a mirrored item"),
        Require::NotSplit if item.split => return unmet("the item is split"),
        Require::NotSynthesised if item.synthesised => return unmet("the item is synthesised"),
        Require::NotUnique if item.rarity == Rarity::Unique => return unmet("the item is unique"),
        Require::NoFractured if item.explicits().any(|a| a.fractured) => {
            return unmet("the item has a fractured modifier");
        }
        Require::NoEnchant if !item.enchants.is_empty() => return unmet("the item is enchanted"),
        Require::NoInfluence if item.influences != 0 => return unmet("the item is influenced"),
        Require::HasInfluence { influence } => match influence {
            None if item.influences == 0 => return unmet("needs an influenced item"),
            Some(i) if !item.has_influence(*i) => {
                return unmet(format!("needs a {} item", i.name()));
            }
            _ => {}
        },
        Require::SingleInfluence if item.influences.count_ones() != 1 => {
            return unmet("needs exactly one influence");
        }
        Require::Influenceable if class.influence_tags.is_none() => {
            return unmet("this item class cannot be influenced");
        }
        Require::OpenAffix { side } => {
            let (room, text) = match side {
                SideReq::Any => (
                    item.open(data, Side::Prefix) + item.open(data, Side::Suffix) > 0,
                    open_text(None),
                ),
                SideReq::Prefix => (
                    item.open(data, Side::Prefix) > 0,
                    open_text(Some(Side::Prefix)),
                ),
                SideReq::Suffix => (
                    item.open(data, Side::Suffix) > 0,
                    open_text(Some(Side::Suffix)),
                ),
                SideReq::Dominant => match dominant_side(data, item) {
                    Some(s) => (item.open(data, s) > 0, open_text(Some(s))),
                    None => (false, "neither eldritch implicit is dominant"),
                },
            };
            if !room {
                return unmet(text);
            }
        }
        Require::FullSuffixes if item.open(data, Side::Suffix) > 0 => {
            return unmet("needs full suffixes");
        }
        Require::MinExplicits { n, ignore_veiled } => {
            let count = if *ignore_veiled {
                item.explicits()
                    .filter(|a| data.mod_at(a.roll.m).kind != ModKind::Veiled)
                    .count()
            } else {
                item.prefixes.len() + item.suffixes.len()
            };
            if count < *n as usize {
                return unmet(format!("needs at least {n} modifiers"));
            }
        }
        Require::MinInfluencedMods { n } if item.influenced_explicits(data) < *n as usize => {
            return unmet(format!("needs at least {n} influenced modifiers"));
        }
        Require::HasImplicit if item.implicits.is_empty() => {
            return unmet("needs an implicit modifier");
        }
        Require::HasCrafted if item.crafted_count(data) == 0 => {
            return unmet("needs a crafted modifier");
        }
        Require::HasEnchant if item.enchants.is_empty() => return unmet("needs an enchantment"),
        Require::CraftedRoom if item.crafted_count(data) >= item.crafted_limit(data) => {
            return unmet("the item already has as many crafted modifiers as it can hold");
        }
        Require::GroupFree { r#mod } => {
            if let Some(reason) = data
                .mod_by_id(r#mod)
                .and_then(|m| group_conflict(data, item, m))
            {
                return unmet(reason);
            }
        }
        Require::HasVeiled if item.veiled(data).is_none() => {
            return unmet("needs a veiled modifier");
        }
        Require::NoVeiled if item.veiled(data).is_some() => {
            return unmet("the item already has a veiled modifier");
        }
        Require::QualityBelow { max } if item.quality >= *max => {
            return unmet(format!("quality is already {max}%"));
        }
        Require::CatalystRoom { catalyst } => {
            if let Some(c) = data.catalyst_by_name(catalyst)
                && item.catalyst == Some(c)
                && item.quality >= 20
            {
                let label = &data.catalysts[c as usize].label;
                return unmet(format!("the item already has 20% {label} quality"));
            }
        }
        Require::HasImprint if item.imprint.is_none() => return unmet("the item has no imprint"),
        Require::NotForeseen if item.foreseen => {
            return unmet("the item already foresees its next result");
        }
        Require::EldritchDominant if item.dominant(data).is_none() => {
            return unmet("neither eldritch implicit is dominant");
        }
        Require::BothEldritch if exarch.is_none() || eater.is_none() => {
            return unmet("needs both a Searing Exarch and an Eater of Worlds implicit");
        }
        Require::NoEldritch if exarch.is_some() || eater.is_some() => {
            return unmet("the item already has an eldritch implicit");
        }
        Require::HasSockets if item.sockets.is_empty() => return unmet("the item has no sockets"),
        Require::CanHaveSockets if class.max_sockets == 0 => {
            return unmet("this item class has no sockets");
        }
        Require::Intangible if item.intangibility == 0 => {
            return unmet("the item is not intangible");
        }
        Require::HasMemoryStrands if item.memory_strands == 0 => {
            return unmet("the item has no Memory Strands");
        }
        Require::HasModType { any_of } => {
            let found = item.explicits().any(|a| {
                any_of
                    .iter()
                    .any(|t| data.mod_type(data.mod_at(a.roll.m).mod_type) == t)
            });
            if !found {
                return unmet(format!("needs a {} modifier", any_of.join(" or ")));
            }
        }
        Require::BaseTag { tag } => {
            let wanted = data.tag_by_name(tag);
            if !wanted.is_some_and(|t| data.base_at(item.base).tags.contains(&t)) {
                return unmet(format!("needs a {tag} base"));
            }
        }
        Require::NotBaseTag { tag } => {
            let Some(t) = data.tag_by_name(tag) else {
                return unmet(format!("the bundle has no tag {tag}"));
            };
            if data.base_at(item.base).tags.contains(&t) {
                return unmet(format!("the item's base has the {tag} tag"));
            }
        }
        Require::CanRoll { pool } => {
            // Ask whether the pool can roll on the item at all, not whether it has room now: a
            // reforge clears the sides it changes before it fills them.
            if pool_candidates(data, item, pool, SideReq::Any, true, true).is_empty() {
                return unmet(pool_empty_text(pool));
            }
        }
        Require::MinSockets { n } => {
            let count: usize = item.sockets.iter().map(Vec::len).sum();
            if count < *n as usize {
                return unmet(format!("needs at least {n} sockets"));
            }
        }
        Require::EssenceFits { essence } if essence_mod(data, item, essence).is_none() => {
            return unmet(format!("{essence} has no modifier for {}", class.id));
        }
        Require::Fossils { min, max } => {
            let n = params.fossils.len();
            if n < *min as usize || n > *max as usize {
                return unmet(format!("needs {min} to {max} fossils"));
            }
        }
        Require::FossilLimits => {
            for &f in &params.fossils {
                let Some(fossil) = data.fossils.get(f as usize) else {
                    continue;
                };
                let fractures = fossil
                    .descriptions
                    .iter()
                    .any(|d| d.to_lowercase().contains("fracture a random modifier"));
                if fractures && (item.influences != 0 || item.explicits().any(|a| a.fractured)) {
                    return unmet(format!(
                        "{} cannot be used on an influenced or fractured item",
                        fossil.name
                    ));
                }
            }
        }
        Require::ImplicitsChangeable if data.base_at(item.base).implicits_locked => {
            return unmet("the item's implicit modifiers cannot be changed");
        }
        _ => {}
    }
    Ok(())
}

/// Fossil indices are known, distinct, and only given to a method that takes fossils.
fn check_params(data: &Data, m: &Method, params: &Params) -> Result<(), Unmet> {
    let takes = m
        .requires
        .iter()
        .any(|r| matches!(r, Require::Fossils { .. }));
    if !takes && !params.fossils.is_empty() {
        return unmet("this method takes no fossils");
    }
    for (i, &f) in params.fossils.iter().enumerate() {
        let Some(fossil) = data.fossils.get(f as usize) else {
            return unmet(format!("unknown fossil {f}"));
        };
        if params.fossils[..i].contains(&f) {
            return unmet(format!("{} is given twice", fossil.name));
        }
    }
    Ok(())
}

/// Everything `check` decides before the ops run: the method is valid, the item count, every
/// requirement on every input, the pair rules, the params, and the metamods the method's
/// protection forbids.
fn prepare(data: &Data, m: &Method, inputs: &[Item], params: &Params) -> Result<(), Unmet> {
    // A bundle method was validated when the bundle loaded; any other is validated here.
    if !data.methods.as_ptr_range().contains(&std::ptr::from_ref(m)) {
        m.validate(data)
            .or_else(|problem| unmet(format!("method {} is invalid: {problem}", m.id)))?;
    }
    if inputs.len() != m.inputs as usize {
        return unmet(format!("needs {} items", m.inputs));
    }
    for r in &m.requires {
        for item in inputs {
            check_one(data, r, item, params)?;
        }
    }
    if let [a, b] = inputs {
        for p in &m.pair {
            match p {
                PairRequire::SameClass
                    if data.base_at(a.base).class != data.base_at(b.base).class =>
                {
                    return unmet("both items must be the same item class");
                }
                PairRequire::DifferentSingleInfluence
                    if a.influences.count_ones() != 1
                        || b.influences.count_ones() != 1
                        || a.influences == b.influences =>
                {
                    return unmet("each item needs one influence, and they must differ");
                }
                _ => {}
            }
        }
    }
    check_params(data, m, params)?;
    if let Some(p) = m.protect {
        let forbidden = [
            (p.locks, [MetaMod::LockPrefixes, MetaMod::LockSuffixes]),
            (p.cannot_roll, [MetaMod::NoAttack, MetaMod::NoCaster]),
        ];
        for item in inputs {
            for (policy, metas) in forbidden {
                if policy != Policy::Forbid {
                    continue;
                }
                if let Some(text) = metas.iter().find_map(|&mm| metamod_text(data, item, mm)) {
                    return unmet(format!("cannot be used on an item with \"{text}\""));
                }
            }
        }
    }
    Ok(())
}

/// Whether the method can be used on these inputs; the first failing requirement's reason if
/// not. It runs the same program `apply` does, so it is `Ok` exactly when `apply` is.
pub fn check(data: &Data, m: &Method, inputs: &[Item], params: &Params) -> Result<(), Unmet> {
    apply(data, m, inputs, params).map(|_| ())
}

/// Applies a method: checks it, then runs its ops on (a clone of) the first input. `apply`
/// returns `Err` exactly when `check` does.
pub fn apply(data: &Data, m: &Method, inputs: &[Item], params: &Params) -> Result<Outcome, Unmet> {
    prepare(data, m, inputs, params)?;
    let mut run = Run {
        data,
        method: m,
        inputs,
        params,
        size: 0,
        in_choice: 0,
    };
    let start = State {
        item: inputs[0].clone(),
        last_added: None,
    };
    let root = Prog {
        ops: &m.ops,
        conditional: false,
        then: None,
    };
    match run.exec(vec![(1.0, start)], root) {
        Ok(leaves) => Ok(Outcome::from_leaves(leaves)),
        Err(Stop::Unmet(reason)) => Err(Unmet { reason }),
        Err(Stop::TooLarge) => Ok(Outcome {
            leaves: vec![(
                1.0,
                Leaf::Pending(Pending {
                    inputs: inputs.to_vec(),
                    params: params.clone(),
                    item: inputs[0].clone(),
                    certain: false,
                    ops: m.ops.clone(),
                    reason: PendingReason::TooLarge,
                }),
            )],
        }),
    }
}

/// What one use costs: the method's costs resolved to currency ids (or `beast:` keys); for the
/// fossil family, the resonator for the fossil count plus each fossil.
pub fn cost(data: &Data, m: &Method, params: &Params) -> Result<Vec<Cost>, Unmet> {
    check_params(data, m, params)?;
    if m.family == Family::Fossil {
        let n = params.fossils.len();
        let Some(resonator) = n.checked_sub(1).and_then(|i| m.resonators.get(i)) else {
            return unmet(format!("needs 1 to {} fossils", m.resonators.len()));
        };
        let mut out = vec![Cost {
            key: resonator.clone(),
            amount: 1.0,
        }];
        out.extend(params.fossils.iter().map(|&f| Cost {
            key: data.fossils[f as usize].currency.clone(),
            amount: 1.0,
        }));
        return Ok(out);
    }
    m.cost
        .iter()
        .map(|c| {
            let key = if c.name.starts_with("beast:") {
                c.name.clone()
            } else {
                match data.currency_by_name(&c.name) {
                    Some(currency) => currency.id.clone(),
                    None => return unmet(format!("unknown currency {}", c.name)),
                }
            };
            Ok(Cost {
                key,
                amount: c.amount,
            })
        })
        .collect()
}

/// One item in the middle of a method, with what the random ops after it need.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    item: Item,
    /// The mod the last add placed (for `RemoveRandom { keep_last_added }`).
    last_added: Option<(Side, u32)>,
}

/// A program position: the ops left in this list, then the lists it returns to. Ops in a
/// Chance branch's own list are conditional.
#[derive(Clone, Copy)]
struct Prog<'p> {
    ops: &'p [Op],
    conditional: bool,
    then: Option<&'p Prog<'p>>,
}

impl<'p> Prog<'p> {
    /// The next op, whether it is conditional, and the program after it.
    fn next(self) -> Option<(&'p Op, bool, Prog<'p>)> {
        let mut p = self;
        loop {
            if let Some((op, rest)) = p.ops.split_first() {
                return Some((op, p.conditional, Prog { ops: rest, ..p }));
            }
            p = *p.then?;
        }
    }

    /// Every op left, in order.
    fn remaining(self) -> Vec<Op> {
        let mut out = self.ops.to_vec();
        let mut p = self.then;
        while let Some(frame) = p {
            out.extend_from_slice(frame.ops);
            p = frame.then;
        }
        out
    }
}

enum Stop {
    Unmet(String),
    TooLarge,
}

/// One way an op changes an item: its chance, the item, and the mod it added.
type Branch = (f64, Item, Option<(Side, u32)>);

/// What one op does to one state: further states with their chances, or final leaves.
enum Step {
    Go(Vec<(f64, State)>),
    End(Vec<(f64, Leaf)>),
}

/// Which affixes can change under a method's protection on one item, decided before the op
/// touches anything (scouring removes Suffixes Cannot Be Changed yet keeps the suffixes).
pub(crate) struct Guard {
    locked: [bool; 2],
    attack: Option<u16>,
    caster: Option<u16>,
}

impl Guard {
    /// Fractured modifiers never change; a side a respected lock protects is kept; a modifier
    /// with the tag a respected Cannot Roll metamod names is kept.
    pub(crate) fn can_change(&self, data: &Data, side: Side, a: &Affix) -> bool {
        if a.fractured || self.locked[side.index()] {
            return false;
        }
        let tags = &data.mod_at(a.roll.m).tags;
        ![self.attack, self.caster]
            .into_iter()
            .flatten()
            .any(|t| tags.contains(&t))
    }
}

/// The protection `method` gives an `item`'s affixes. A method with no stated protection keeps
/// nothing (`Policy::Ignore`), which is what a method that changes no explicits needs.
pub(crate) fn guard_for(data: &Data, method: &Method, item: &Item) -> Guard {
    let p = method.protect.unwrap_or(Protection::IGNORE_ALL);
    let locks = p.locks == Policy::Respect;
    let tags = p.cannot_roll == Policy::Respect;
    let tag_of = |meta: MetaMod, name: &str| {
        if tags && item.has_meta(data, meta) {
            data.tag_by_name(name)
        } else {
            None
        }
    };
    Guard {
        locked: [
            locks && item.locked(data, Side::Prefix),
            locks && item.locked(data, Side::Suffix),
        ],
        attack: tag_of(MetaMod::NoAttack, "attack"),
        caster: tag_of(MetaMod::NoCaster, "caster"),
    }
}

/// The explicit modifiers per side `method` may remove from `item`: what `Guard` accepts. This
/// is the count `uniform` removal divides by, so an Orb of Annulment's chance of taking a named
/// modifier is one over the sum.
pub(crate) fn changeable(data: &Data, method: &Method, item: &Item) -> [usize; 2] {
    let guard = guard_for(data, method, item);
    Side::BOTH.map(|side| {
        item.affixes(side)
            .iter()
            .filter(|a| guard.can_change(data, side, a))
            .count()
    })
}

struct Run<'a> {
    data: &'a Data,
    method: &'a Method,
    inputs: &'a [Item],
    params: &'a Params,
    /// States and leaves made so far, against `MAX_BRANCHES`.
    size: usize,
    /// Set while the ops after a player's choice run; they are conditional too.
    in_choice: usize,
}

/// The display-only and one-shot fields after an op: the pasted defence figures go once
/// anything that feeds them changes (explicits, implicits, quality, catalyst), so within one
/// method they are a function of the rest of the item and never split equal states; foresight
/// goes once the item changes at all (game text: "Modifying the item in any way removes the
/// ability to foresee").
fn settle(before: &Item, after: &mut Item) {
    let fed = before.prefixes != after.prefixes
        || before.suffixes != after.suffixes
        || before.implicits != after.implicits
        || before.quality != after.quality
        || before.catalyst != after.catalyst;
    if fed {
        after.defences = None;
    }
    if after.foreseen {
        let mut same = before.clone();
        same.defences = after.defences;
        if same != *after {
            after.foreseen = false;
        }
    }
}

/// The next tier up a Dominance upgrade gives `m`, or `None` when `m` is elevated already.
fn upgrade(data: &Data, item: &Item, m: u32) -> Option<u32> {
    let md = data.mod_at(m);
    if md.elevated {
        return None;
    }
    let tags = item.tags(data);
    let key = |i: u32| {
        let x = data.mod_at(i);
        (x.level, x.stats.first().map_or(0, |s| s.max), i)
    };
    let mut chain: Vec<u32> = data
        .mods_of_kind(ModKind::Natural)
        .iter()
        .copied()
        .filter(|&i| {
            let x = data.mod_at(i);
            !x.elevated
                && x.mod_type == md.mod_type
                && x.groups == md.groups
                && x.influence == md.influence
                && (i == m || pool::spawn(x, &tags) > 0)
        })
        .collect();
    chain.sort_by_key(|&i| key(i));
    let pos = chain.iter().position(|&i| i == m)?;
    chain.get(pos + 1).copied().or_else(|| elevated_of(data, m))
}

/// The elevated version of a top-tier mod: same groups and influence, and either the chain's id
/// stem followed by `Maven` or the same mod type (RePoE gives elevated mods a `…Maven` type).
fn elevated_of(data: &Data, m: u32) -> Option<u32> {
    let md = data.mod_at(m);
    let stem = md
        .id
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '_');
    let base_type = data.mod_type(md.mod_type);
    let candidates: Vec<u32> = data
        .mods_of_kind(ModKind::Natural)
        .iter()
        .copied()
        .filter(|&i| {
            let x = data.mod_at(i);
            x.elevated && x.groups == md.groups && x.influence == md.influence
        })
        .collect();
    let by_id = candidates.iter().copied().find(|&i| {
        data.mod_at(i)
            .id
            .strip_prefix(stem)
            .is_some_and(|r| r.starts_with("Maven"))
    });
    by_id.or_else(|| {
        candidates.iter().copied().find(|&i| {
            let t = data.mod_type(data.mod_at(i).mod_type);
            t == base_type || t.strip_suffix("Maven") == Some(base_type)
        })
    })
}

/// Spawnable mods of one mod type on this item, highest level first.
fn resistance_ranks(data: &Data, item: &Item, mod_type: &str) -> Vec<u32> {
    let Some(t) = data.mod_type_by_name(mod_type) else {
        return Vec::new();
    };
    let tags = item.tags(data);
    let mut list: Vec<u32> = data
        .mods_of_kind(ModKind::Natural)
        .iter()
        .copied()
        .filter(|&i| {
            let x = data.mod_at(i);
            x.mod_type == t && pool::spawn(x, &tags) > 0
        })
        .collect();
    list.sort_by_key(|&i| (std::cmp::Reverse(data.mod_at(i).level), i));
    list
}

/// Puts `m` on its side, unrolled; a normal item that gains an explicit turns magic (owner,
/// Q3, for a bench craft; every add follows it, so no result is a normal item with explicits).
fn add_affix(data: &Data, item: &mut Item, m: u32) -> Option<Side> {
    let side = data.mod_at(m).side?;
    item.affixes_mut(side).push(Affix::new(m));
    if item.rarity == Rarity::Normal {
        item.rarity = Rarity::Magic;
    }
    Some(side)
}

/// The room on `side` for an added explicit: a normal item turns magic when it gains one, so
/// it has a magic item's room.
/// The room a side has for an add: a normal item is counted as magic, because one add makes it
/// magic (owner, "Turns it magic").
pub fn room(data: &Data, item: &Item, side: Side) -> u8 {
    if item.rarity == Rarity::Normal {
        let mut magic = item.clone();
        magic.rarity = Rarity::Magic;
        magic.open(data, side)
    } else {
        item.open(data, side)
    }
}

fn has_range(data: &Data, m: u32) -> bool {
    data.mod_at(m).stats.iter().any(|s| s.min != s.max)
}

impl Run<'_> {
    fn grow(&mut self, n: usize) -> Result<(), Stop> {
        self.size += n;
        if self.size > MAX_BRANCHES {
            Err(Stop::TooLarge)
        } else {
            Ok(())
        }
    }

    fn guard(&self, item: &Item) -> Guard {
        guard_for(self.data, self.method, item)
    }

    /// An op with nothing to do: refuse the method, or inside a Chance branch (or after a
    /// player's choice) leave the state as it is.
    fn nothing(&self, conditional: bool, state: State, reason: String) -> Result<Step, Stop> {
        if conditional {
            Ok(Step::Go(vec![(1.0, state)]))
        } else {
            Err(Stop::Unmet(reason))
        }
    }

    fn pending(&self, item: Item, certain: bool, ops: Vec<Op>, reason: PendingReason) -> Step {
        Step::End(vec![(
            1.0,
            Leaf::Pending(Pending {
                inputs: self.inputs.to_vec(),
                params: self.params.clone(),
                item,
                certain,
                ops,
                reason,
            }),
        )])
    }

    /// Runs `prog` over the distribution, merging equal states after every op.
    fn exec(&mut self, dist: Vec<(f64, State)>, prog: Prog<'_>) -> Result<Vec<(f64, Leaf)>, Stop> {
        let mut dist = dist;
        let mut prog = prog;
        let mut leaves = Vec::new();
        loop {
            let Some((op, conditional, rest)) = prog.next() else {
                leaves.extend(dist.into_iter().map(|(p, s)| (p, Leaf::Item(s.item))));
                return Ok(leaves);
            };
            let conditional = conditional || self.in_choice > 0;
            if let Op::Chance { branches } = op {
                for b in branches {
                    let frame = Prog {
                        ops: &b.ops,
                        conditional: true,
                        then: Some(&rest),
                    };
                    let sub: Vec<(f64, State)> =
                        dist.iter().map(|(p, s)| (p * b.p, s.clone())).collect();
                    leaves.extend(self.exec(sub, frame)?);
                }
                return Ok(leaves);
            }
            let mut next: BTreeMap<State, f64> = BTreeMap::new();
            for (p, state) in dist {
                match self.step(op, state, rest, conditional)? {
                    Step::Go(states) => {
                        self.grow(states.len())?;
                        for (q, s) in states {
                            *next.entry(s).or_insert(0.0) += p * q;
                        }
                    }
                    Step::End(ends) => {
                        self.grow(ends.len())?;
                        leaves.extend(ends.into_iter().map(|(q, l)| (p * q, l)));
                    }
                }
            }
            dist = next.into_iter().map(|(s, p)| (p, s)).collect();
            prog = rest;
        }
    }

    /// One op on one state.
    fn step(
        &mut self,
        op: &Op,
        state: State,
        rest: Prog<'_>,
        conditional: bool,
    ) -> Result<Step, Stop> {
        let data = self.data;
        let here = || -> Vec<Op> {
            std::iter::once(op.clone())
                .chain(rest.remaining())
                .collect()
        };
        // Ops that end the program, replace the item, or branch on their own.
        match op {
            Op::Mirror => {
                let mut copy = state.item.clone();
                copy.mirrored = true;
                copy.imprint = None;
                return Ok(Step::End(vec![(1.0, Leaf::Items(vec![state.item, copy]))]));
            }
            Op::Destroy => return Ok(Step::End(vec![(1.0, Leaf::Destroyed)])),
            Op::Fill { fill } => {
                let certain = fill.weighting != Weighting::Recombine;
                let reason = if fill.one_veiled {
                    PendingReason::MultiDrawVeiled
                } else {
                    PendingReason::MultiDraw
                };
                return Ok(self.pending(state.item, certain, here(), reason));
            }
            Op::Sockets { .. } => {
                return Ok(self.pending(state.item, false, here(), PendingReason::Sockets));
            }
            Op::Unmodelled { reason } => {
                return Ok(self.pending(state.item, false, here(), reason.clone()));
            }
            Op::AddQuality { by_rarity, .. } if by_rarity[state.item.rarity as usize].is_none() => {
                let rarity = state.item.rarity.name();
                return Ok(self.pending(
                    state.item,
                    false,
                    here(),
                    PendingReason::NeedsData {
                        what: format!(
                            "the amount of quality a {rarity} item gains is not known and depends on the item's item level"
                        ),
                    },
                ));
            }
            Op::Unveil => return self.unveil(state, rest, conditional),
            Op::TakeInput { input } => {
                let item = self.inputs[*input as usize].clone();
                return Ok(Step::Go(vec![(
                    1.0,
                    State {
                        item,
                        last_added: None,
                    },
                )]));
            }
            Op::Restore => {
                let item = &state.item;
                let reason = if item.imprint.is_none() {
                    Some("the item has no imprint")
                } else if item.corrupted {
                    Some("the item is corrupted")
                } else if item.mirrored {
                    Some("the item is mirrored")
                } else {
                    None
                };
                if let Some(r) = reason {
                    return self.nothing(conditional, state, r.into());
                }
                // The imprint comes back as it was, defence figures included.
                let mut item = state.item.imprint.clone().map(|b| *b).unwrap_or(state.item);
                item.imprint = None;
                return Ok(Step::Go(vec![(
                    1.0,
                    State {
                        item,
                        last_added: None,
                    },
                )]));
            }
            _ => {}
        }
        let before = state.item.clone();
        let branches = match self.change(op, &state) {
            Ok(b) => b,
            Err(reason) => return self.nothing(conditional, state, reason),
        };
        Ok(Step::Go(
            branches
                .into_iter()
                .map(|(p, mut item, added)| {
                    item.canonical(data);
                    settle(&before, &mut item);
                    let last_added = added.or(state.last_added);
                    (p, State { item, last_added })
                })
                .collect(),
        ))
    }

    /// The ops that change the item: (chance, item, the mod it added) per branch, or why the op
    /// has nothing to do.
    fn change(&self, op: &Op, state: &State) -> Result<Vec<Branch>, String> {
        let data = self.data;
        let item = &state.item;
        let one = |it: Item| Ok(vec![(1.0, it, None)]);
        let uniform = |items: Vec<Item>| {
            let p = 1.0 / items.len() as f64;
            items
                .into_iter()
                .map(|it| (p, it, None))
                .collect::<Vec<_>>()
        };
        match op {
            Op::SetRarity { rarity } => {
                let mut it = item.clone();
                it.rarity = *rarity;
                one(it)
            }
            Op::Clear { side } => {
                let guard = self.guard(item);
                let mut it = item.clone();
                for s in sides(data, item, *side) {
                    it.affixes_mut(s).retain(|a| !guard.can_change(data, s, a));
                }
                one(it)
            }
            Op::Scour => {
                let guard = self.guard(item);
                let mut it = item.clone();
                for s in Side::BOTH {
                    it.affixes_mut(s).retain(|a| !guard.can_change(data, s, a));
                }
                let removed = item.prefixes.len() + item.suffixes.len()
                    - it.prefixes.len()
                    - it.suffixes.len();
                if removed == 0 {
                    return Err("no modifier can be removed".into());
                }
                // vhpg (fractured items): one fractured modifier left makes the item magic, two
                // or more rare; nothing left, normal. A side a metamod kept leaves the rarity.
                let left: Vec<&Affix> = it.explicits().collect();
                if left.iter().all(|a| a.fractured) {
                    it.rarity = match left.len() {
                        0 => Rarity::Normal,
                        1 => Rarity::Magic,
                        _ => Rarity::Rare,
                    };
                }
                one(it)
            }
            Op::AddRandom {
                pool,
                side,
                ignore_limits,
            } => {
                let candidates = self.add_candidates(item, pool, *side, *ignore_limits);
                if candidates.is_empty() {
                    return Err(pool_empty_text(pool));
                }
                Ok(candidates
                    .into_iter()
                    .map(|(m, p)| {
                        let mut it = item.clone();
                        let side = add_affix(data, &mut it, m);
                        (p, it, side.map(|s| (s, m)))
                    })
                    .collect())
            }
            Op::RemoveRandom {
                filter,
                keep_last_added,
                optional,
            } => {
                let guard = self.guard(item);
                let allowed = match filter {
                    RemoveFilter::Side { side } => sides(data, item, *side),
                    _ => Side::BOTH.to_vec(),
                };
                let mut items = Vec::new();
                for &s in &allowed {
                    for (i, a) in item.affixes(s).iter().enumerate() {
                        let influenced = data.mod_at(a.roll.m).influence.is_some();
                        if guard.can_change(data, s, a)
                            && (influenced || !matches!(filter, RemoveFilter::Influenced))
                            && !(*keep_last_added && state.last_added == Some((s, a.roll.m)))
                        {
                            let mut it = item.clone();
                            it.affixes_mut(s).remove(i);
                            items.push(it);
                        }
                    }
                }
                if items.is_empty() {
                    if *optional {
                        return one(item.clone());
                    }
                    let what = match (filter, allowed.as_slice()) {
                        (RemoveFilter::Influenced, _) => "no influenced modifier can be removed",
                        (RemoveFilter::Side { .. }, []) => "neither eldritch implicit is dominant",
                        (RemoveFilter::Side { .. }, [Side::Prefix]) => "no prefix can be removed",
                        (RemoveFilter::Side { .. }, [Side::Suffix]) => "no suffix can be removed",
                        _ => "no modifier can be removed",
                    };
                    return Err(what.into());
                }
                Ok(uniform(items))
            }
            Op::AddVeiled => {
                let open: Vec<Side> = Side::BOTH
                    .into_iter()
                    .filter(|&s| room(data, item, s) > 0)
                    .collect();
                if open.is_empty() {
                    return Err("no side has room for a veiled modifier".into());
                }
                let p = 1.0 / open.len() as f64;
                Ok(open
                    .into_iter()
                    .filter_map(|s| {
                        let m = data.mod_by_id(VEILED[s.index()])?;
                        let mut it = item.clone();
                        add_affix(data, &mut it, m);
                        Some((p, it, Some((s, m))))
                    })
                    .collect())
            }
            Op::AddMod { r#mod } => {
                let Some(m) = data.mod_by_id(r#mod) else {
                    return Err(format!("unknown modifier {mod}", mod = r#mod));
                };
                if let Some(reason) = self.cannot_place(item, m) {
                    return Err(reason);
                }
                let mut it = item.clone();
                let side = add_affix(data, &mut it, m);
                Ok(vec![(1.0, it, side.map(|s| (s, m)))])
            }
            Op::AddEssence { essence } => {
                let Some(m) = essence_mod(data, item, essence) else {
                    return Err(format!(
                        "{essence} has no modifier for {}",
                        item.class(data).id
                    ));
                };
                if let Some(reason) = self.cannot_place(item, m) {
                    return Err(reason);
                }
                let mut it = item.clone();
                let side = add_affix(data, &mut it, m);
                Ok(vec![(1.0, it, side.map(|s| (s, m)))])
            }
            Op::RemoveCrafted => {
                if item.crafted_count(data) == 0 {
                    return Err("needs a crafted modifier".into());
                }
                let mut it = item.clone();
                for s in Side::BOTH {
                    it.affixes_mut(s)
                        .retain(|a| data.mod_at(a.roll.m).kind != ModKind::Crafted);
                }
                one(it)
            }
            Op::SetEnchant { pool: which } => {
                let kind = which.kind();
                let candidates = self.kind_pool(item, kind, true, |_| true);
                if candidates.is_empty() {
                    return Err("no enchantment can roll on this item".into());
                }
                Ok(candidates
                    .into_iter()
                    .map(|(m, p)| {
                        let mut it = item.clone();
                        it.enchants = vec![Roll { m, values: None }];
                        (p, it, None)
                    })
                    .collect())
            }
            Op::SetEnchantMod { r#mod } => {
                let Some(m) = data.mod_by_id(r#mod) else {
                    return Err(format!("unknown modifier {mod}", mod = r#mod));
                };
                let mut it = item.clone();
                it.enchants = vec![Roll { m, values: None }];
                one(it)
            }
            Op::ClearEnchants => {
                if item.enchants.is_empty() {
                    return Err("needs an enchantment".into());
                }
                let mut it = item.clone();
                it.enchants.clear();
                one(it)
            }
            Op::RerollValues { scope } => {
                let mut it = item.clone();
                let rerolled = match scope {
                    ValueScope::Explicit => {
                        let guard = self.guard(item);
                        let mut n = 0;
                        for s in Side::BOTH {
                            for a in it.affixes_mut(s) {
                                if guard.can_change(data, s, a)
                                    && data.mod_at(a.roll.m).kind != ModKind::Crafted
                                    && has_range(data, a.roll.m)
                                {
                                    a.roll.values = None;
                                    n += 1;
                                }
                            }
                        }
                        (n > 0)
                            .then_some(())
                            .ok_or("no modifier has values to reroll")
                    }
                    ValueScope::Implicit => {
                        let mut n = 0;
                        for r in &mut it.implicits {
                            if has_range(data, r.m) {
                                r.values = None;
                                n += 1;
                            }
                        }
                        (n > 0)
                            .then_some(())
                            .ok_or("no implicit modifier has values to reroll")
                    }
                    ValueScope::Defences => {
                        let base = data.base_at(item.base);
                        it.defences = None;
                        base.defences
                            .iter()
                            .flatten()
                            .any(|[lo, hi]| lo != hi)
                            .then_some(())
                            .ok_or("the base has no defence values to reroll")
                    }
                };
                match rerolled {
                    Ok(()) => one(it),
                    Err(reason) => Err(reason.into()),
                }
            }
            Op::Fracture => {
                let mut items = Vec::new();
                for s in Side::BOTH {
                    for (i, a) in item.affixes(s).iter().enumerate() {
                        if !a.fractured && data.mod_at(a.roll.m).kind != ModKind::Veiled {
                            let mut it = item.clone();
                            it.affixes_mut(s)[i].fractured = true;
                            items.push(it);
                        }
                    }
                }
                if items.is_empty() {
                    return Err("no modifier can be fractured".into());
                }
                Ok(uniform(items))
            }
            Op::AddInfluence { influence } => {
                if item.class(data).influence_tags.is_none() {
                    return Err("this item class cannot be influenced".into());
                }
                let mut it = item.clone();
                it.influences |= influence.bit();
                one(it)
            }
            Op::AddInfluences { input } => {
                if item.class(data).influence_tags.is_none() {
                    return Err("this item class cannot be influenced".into());
                }
                let mut it = item.clone();
                it.influences |= self.inputs[*input as usize].influences;
                one(it)
            }
            Op::Corrupt => {
                let mut it = item.clone();
                it.corrupted = true;
                one(it)
            }
            Op::VaalImplicit => {
                if data.base_at(item.base).implicits_locked {
                    return Err("the item's implicit modifiers cannot be changed".into());
                }
                let domain = data.base_at(item.base).domain.repoe();
                let candidates = self.kind_pool(item, ModKind::CorruptedImplicit, true, |m| {
                    m.domain == domain
                });
                if candidates.is_empty() {
                    return Err("no corrupted implicit can roll on this item".into());
                }
                // One random implicit is replaced (maxroll, corruption); with none, one is added.
                let n = item.implicits.len().max(1);
                let mut out = Vec::new();
                for slot in 0..n {
                    for &(m, p) in &candidates {
                        let mut it = item.clone();
                        let roll = Roll { m, values: None };
                        match it.implicits.get_mut(slot) {
                            Some(r) => *r = roll,
                            None => it.implicits.push(roll),
                        }
                        out.push((p / n as f64, it, None));
                    }
                }
                Ok(out)
            }
            Op::EldritchImplicit { eldritch, tier } => {
                let (kind, other, name) = match eldritch {
                    Eldritch::Exarch => (ModKind::Exarch, ModKind::Eater, "Searing Exarch"),
                    Eldritch::Eater => (ModKind::Eater, ModKind::Exarch, "Eater of Worlds"),
                };
                // Embers and ichors have no item level requirement (maxroll, eldritch crafting).
                let candidates =
                    self.kind_pool(item, kind, false, |m| m.eldritch_tier == Some(*tier));
                if candidates.is_empty() {
                    return Err(format!(
                        "no tier {tier} {name} implicit can roll on this item"
                    ));
                }
                Ok(candidates
                    .into_iter()
                    .map(|(m, p)| {
                        let mut it = item.clone();
                        it.implicits.retain(|r| data.mod_at(r.m).kind == other);
                        it.implicits.push(Roll { m, values: None });
                        (p, it, None)
                    })
                    .collect())
            }
            Op::Conflict => {
                let Some((exarch, eater)) = conflict_pair(data, item) else {
                    return Err("needs a Searing Exarch and an Eater of Worlds implicit".into());
                };
                Ok([(exarch, eater), (eater, exarch)]
                    .into_iter()
                    .map(|(raised, lowered)| {
                        let mut it = item.clone();
                        it.implicits = conflict_implicits(data, item, raised, lowered);
                        (0.5, it, None)
                    })
                    .collect())
            }
            Op::Dominance => {
                let guard = self.guard(item);
                let influenced: Vec<(Side, usize)> = Side::BOTH
                    .into_iter()
                    .flat_map(|s| {
                        item.affixes(s)
                            .iter()
                            .enumerate()
                            .filter(|(_, a)| {
                                guard.can_change(data, s, a)
                                    && data.mod_at(a.roll.m).influence.is_some()
                            })
                            .map(move |(i, _)| (s, i))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                if influenced.len() < 2 {
                    return Err("needs at least 2 influenced modifiers that can change".into());
                }
                let mut items = Vec::new();
                for &(xs, xi) in &influenced {
                    for &(ys, yi) in &influenced {
                        if (xs, xi) == (ys, yi) {
                            continue;
                        }
                        let mut it = item.clone();
                        let y = &mut it.affixes_mut(ys)[yi];
                        match upgrade(data, item, y.roll.m) {
                            Some(up) => {
                                y.roll = Roll {
                                    m: up,
                                    values: None,
                                }
                            }
                            None => y.roll.values = None,
                        }
                        it.affixes_mut(xs).remove(xi);
                        items.push(it);
                    }
                }
                Ok(uniform(items))
            }
            Op::ChangeResistance { from, to } => {
                let guard = self.guard(item);
                self.change_resistance(item, &guard, *from, *to)
                    .map(|it| vec![(1.0, it, None)])
            }
            Op::AddQuality { by_rarity, max } => {
                if item.quality >= *max {
                    return Err(format!("quality is already {max}%"));
                }
                let Some(add) = by_rarity[item.rarity as usize] else {
                    return Err(format!(
                        "the amount of quality a {} item gains is not known",
                        item.rarity.name()
                    ));
                };
                let mut it = item.clone();
                it.quality = (*max).min(item.quality.saturating_add(add));
                one(it)
            }
            Op::CatalystQuality { catalyst } => {
                let Some(c) = data.catalyst_by_name(catalyst) else {
                    return Err(format!("unknown catalyst {catalyst}"));
                };
                let mut it = item.clone();
                if item.catalyst == Some(c) {
                    if item.quality >= 20 {
                        let label = &data.catalysts[c as usize].label;
                        return Err(format!("the item already has 20% {label} quality"));
                    }
                    it.quality += 1;
                } else {
                    it.catalyst = Some(c);
                    it.quality = 1;
                }
                one(it)
            }
            Op::Imprint => {
                let mut copy = item.clone();
                copy.imprint = None;
                let mut it = item.clone();
                it.imprint = Some(Box::new(copy));
                one(it)
            }
            Op::LinkAll => {
                if item.sockets.is_empty() {
                    return Err("the item has no sockets".into());
                }
                if item.sockets.len() == 1 {
                    return Err("every socket is already linked".into());
                }
                let mut it = item.clone();
                it.sockets = vec![item.sockets.concat()];
                one(it)
            }
            Op::RemoveIntangibility => {
                if item.intangibility == 0 {
                    return Err("the item is not intangible".into());
                }
                let mut it = item.clone();
                it.intangibility = 0;
                one(it)
            }
            Op::Mirror
            | Op::Destroy
            | Op::Fill { .. }
            | Op::Sockets { .. }
            | Op::Unmodelled { .. }
            | Op::Unveil
            | Op::TakeInput { .. }
            | Op::Restore
            | Op::Chance { .. } => unreachable!("handled by step and exec"),
        }
    }

    /// Why `m` cannot go on the item: a mod of another base domain, no room on its side, a
    /// group the item has, or no crafted room for a crafted modifier.
    fn cannot_place(&self, item: &Item, m: u32) -> Option<String> {
        let data = self.data;
        let md = data.mod_at(m);
        let side = md.side?;
        let own_domain = matches!(
            md.domain.as_str(),
            "item" | "misc" | "abyss_jewel" | "flask" | "tincture"
        );
        if own_domain && md.domain != data.base_at(item.base).domain.repoe() {
            return Some(format!(
                "{} cannot go on a {}",
                md.id,
                item.class(data).name
            ));
        }
        if room(data, item, side) == 0 {
            return Some(open_text(Some(side)).into());
        }
        if let Some(reason) = group_conflict(data, item, m) {
            return Some(reason);
        }
        (md.kind == ModKind::Crafted && item.crafted_count(data) >= item.crafted_limit(data))
            .then(|| "the item already has as many crafted modifiers as it can hold".into())
    }

    /// The weighted pool of one implicit or enchantment kind, normalised; `level` caps mod level
    /// at the item level.
    fn kind_pool(
        &self,
        item: &Item,
        kind: ModKind,
        level: bool,
        extra: impl Fn(&crate::data::ModData) -> bool,
    ) -> Vec<(u32, f64)> {
        let data = self.data;
        let tags = item.tags(data);
        let w: Vec<(u32, f64)> = pool::weighted(data, &[kind], &tags)
            .filter(|(_, m, _)| (!level || m.level <= item.item_level) && extra(m))
            .map(|(i, _, w)| (i, w))
            .collect();
        pool::normalise(w)
    }

    /// Candidate mods an AddRandom draws from, with their probabilities. Every add leaves out
    /// the attack or caster modifiers a Cannot Roll metamod names (claim `metamod.cannot-roll`),
    /// and it picks a side first, then a modifier on that side by weight (claim
    /// `add.side-first`, owner Q1).
    fn add_candidates(
        &self,
        item: &Item,
        pool: &Pool,
        side: SideReq,
        ignore_limits: bool,
    ) -> Vec<(u32, f64)> {
        add_candidates(self.data, item, pool, side, ignore_limits)
    }

    /// Turns the `from` resistance that can change into the `to` mod of the same rank among the
    /// mods that can spawn on the item (the nearest lower rank when `to` has fewer tiers).
    fn change_resistance(
        &self,
        item: &Item,
        guard: &Guard,
        from: Element,
        to: Element,
    ) -> Result<Item, String> {
        let data = self.data;
        let from_ranks = resistance_ranks(data, item, from.resistance_type());
        let to_ranks = resistance_ranks(data, item, to.resistance_type());
        let found = Side::BOTH.into_iter().find_map(|side| {
            item.affixes(side)
                .iter()
                .position(|a| guard.can_change(data, side, a) && from_ranks.contains(&a.roll.m))
                .map(|i| (side, i))
        });
        let Some((side, i)) = found else {
            return Err(format!(
                "no {} modifier can be changed",
                from.resistance_type()
            ));
        };
        let Some(&last) = to_ranks.last() else {
            return Err(format!(
                "no {} modifier can roll on this item",
                to.resistance_type()
            ));
        };
        let m = item.affixes(side)[i].roll.m;
        let rank = from_ranks.iter().position(|&x| x == m).unwrap_or(0);
        let target = to_ranks.get(rank).copied().unwrap_or(last);
        if group_conflict(data, item, target).is_some() {
            return Err(format!(
                "the item already has a {} modifier",
                to.resistance_type()
            ));
        }
        let mut it = item.clone();
        it.affixes_mut(side).remove(i);
        if let Some(s) = data.mod_at(target).side {
            it.affixes_mut(s).push(Affix::new(target));
        }
        Ok(it)
    }

    /// Unveiling: up to three options drawn by weight without replacement, each draw leaving
    /// out the options that share a group with one already drawn (an offer never holds two mods
    /// of one group); the player picks one, and the ops after the unveil run on the pick.
    fn unveil(&mut self, state: State, rest: Prog<'_>, conditional: bool) -> Result<Step, Stop> {
        let data = self.data;
        let item = &state.item;
        let Some((side, index)) = item.veiled(data) else {
            return self.nothing(conditional, state, "needs a veiled modifier".into());
        };
        let tags = item.tags(data);
        let placeholder = item.affixes(side)[index].roll.m;
        let mut groups = item.explicit_groups(data);
        for g in &data.mod_at(placeholder).groups {
            groups.remove(g);
        }
        let mut options: Vec<(u32, f64)> = pool::weighted(data, &[ModKind::Unveiled], &tags)
            .filter(|(_, m, _)| {
                m.side == Some(side)
                    && m.level <= item.item_level
                    && !m.groups.iter().any(|g| groups.contains(g))
            })
            .map(|(i, m, _)| (i, f64::from(pool::spawn(m, &tags))))
            .collect();
        if options.is_empty() {
            return self.nothing(
                conditional,
                state,
                "no unveiled modifier can be offered".into(),
            );
        }
        options.sort_by_key(|(m, _)| *m);
        let mut sets: BTreeMap<Vec<usize>, f64> = BTreeMap::new();
        self.draw(&options, &mut Vec::new(), 1.0, &mut sets)?;
        let mut ends = Vec::with_capacity(sets.len());
        for (set, p) in sets {
            let mut picks = Vec::with_capacity(set.len());
            for k in set {
                let mut it = item.clone();
                let fractured = it.affixes(side)[index].fractured;
                it.affixes_mut(side)[index] = Affix {
                    roll: Roll {
                        m: options[k].0,
                        values: None,
                    },
                    fractured,
                };
                it.canonical(data);
                settle(item, &mut it);
                self.in_choice += 1;
                let leaves = self.exec(
                    vec![(
                        1.0,
                        State {
                            item: it,
                            last_added: None,
                        },
                    )],
                    rest,
                );
                self.in_choice -= 1;
                picks.push(Outcome::from_leaves(leaves?));
            }
            // A single option is no choice at all.
            if picks.len() == 1 {
                let only = picks.remove(0);
                ends.extend(only.leaves.into_iter().map(|(q, l)| (p * q, l)));
            } else {
                ends.push((p, Leaf::Choice(picks)));
            }
        }
        Ok(Step::End(ends))
    }

    /// Every ordered draw of up to three options, summed per unordered set.
    fn draw(
        &mut self,
        options: &[(u32, f64)],
        drawn: &mut Vec<usize>,
        p: f64,
        sets: &mut BTreeMap<Vec<usize>, f64>,
    ) -> Result<(), Stop> {
        let data = self.data;
        let shares = |a: usize, b: usize| {
            let ga = &data.mod_at(options[a].0).groups;
            data.mod_at(options[b].0)
                .groups
                .iter()
                .any(|g| ga.contains(g))
        };
        let eligible: Vec<usize> = (0..options.len())
            .filter(|&k| !drawn.iter().any(|&d| d == k || shares(d, k)))
            .collect();
        if drawn.len() == 3 || eligible.is_empty() {
            let mut key = drawn.clone();
            key.sort_unstable();
            *sets.entry(key).or_insert(0.0) += p;
            return self.grow(1);
        }
        let total: f64 = eligible.iter().map(|&k| options[k].1).sum();
        for &k in &eligible {
            drawn.push(k);
            self.draw(options, drawn, p * options[k].1 / total, sets)?;
            drawn.pop();
        }
        Ok(())
    }
}
