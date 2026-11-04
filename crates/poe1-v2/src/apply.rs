//! `apply(item(s), method) → outcome`: checks a method's requirements against real items (with
//! the failing reason) and runs its ops. Single-draw randomness becomes exact `Chance` tables
//! (weighted by `pool`), unveiling becomes `Chance` over offer sets of `Choice`s, a reforge's
//! guaranteed part is applied exactly and the rest stays `Pending`, as do socket outcomes and
//! unconfirmed rules.

use std::cmp::Ordering;

use crate::data::{Category, Cost, Data, Eldritch, Influence, ModKind, Rarity, Side};
use crate::item::{Affix, Item, MetaMod, Roll};
use crate::method::{
    Element, EnchantPool, Family, Guarantee, Method, Op, PairRequire, PendingReason, Pool,
    ReforgeSpec, RemoveFilter, Require, SideReq, ValueScope, Weighting,
};
use crate::pool;

/// More leaves than this and `apply` reports `Pending { reason: TooLarge }`.
pub const MAX_BRANCHES: usize = 100_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params {
    /// Fossil indices (`Bundle::fossils`) for the fossil family.
    pub fossils: Vec<u16>,
    /// The essence a Reforge with guarantee Essence places; defaults to the method's own.
    pub essence: Option<String>,
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

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Item(Item),
    Items(Vec<Item>),
    Destroyed,
    Chance(Vec<(f64, Outcome)>),
    Choice(Vec<Outcome>),
    /// Not computed in this pass: `item` is the partial result, `ops` what is left to run.
    Pending {
        item: Item,
        ops: Vec<Op>,
        reason: PendingReason,
    },
}

impl Outcome {
    fn rank(&self) -> u8 {
        match self {
            Outcome::Item(_) => 0,
            Outcome::Items(_) => 1,
            Outcome::Destroyed => 2,
            Outcome::Chance(_) => 3,
            Outcome::Choice(_) => 4,
            Outcome::Pending { .. } => 5,
        }
    }

    /// A total order: variant, then contents.
    pub fn order(&self, other: &Outcome) -> Ordering {
        match (self, other) {
            (Outcome::Item(a), Outcome::Item(b)) => a.cmp(b),
            (Outcome::Items(a), Outcome::Items(b)) => a.cmp(b),
            (Outcome::Chance(a), Outcome::Chance(b)) => {
                for ((pa, oa), (pb, ob)) in a.iter().zip(b) {
                    let o = pb.total_cmp(pa).then_with(|| oa.order(ob));
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                a.len().cmp(&b.len())
            }
            (Outcome::Choice(a), Outcome::Choice(b)) => {
                for (oa, ob) in a.iter().zip(b) {
                    let o = oa.order(ob);
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                a.len().cmp(&b.len())
            }
            (
                Outcome::Pending {
                    item: a,
                    reason: ra,
                    ops: oa,
                },
                Outcome::Pending {
                    item: b,
                    reason: rb,
                    ops: ob,
                },
            ) => a
                .cmp(b)
                .then_with(|| ra.cmp(rb))
                .then_with(|| oa.len().cmp(&ob.len())),
            _ => self.rank().cmp(&other.rank()),
        }
    }

    /// Every item the outcome can end in (Pending partial items included).
    pub fn items(&self) -> Vec<&Item> {
        let mut out = Vec::new();
        fn walk<'a>(o: &'a Outcome, out: &mut Vec<&'a Item>) {
            match o {
                Outcome::Item(i) | Outcome::Pending { item: i, .. } => out.push(i),
                Outcome::Items(v) => out.extend(v),
                Outcome::Destroyed => {}
                Outcome::Chance(c) => c.iter().for_each(|(_, o)| walk(o, out)),
                Outcome::Choice(c) => c.iter().for_each(|o| walk(o, out)),
            }
        }
        walk(self, &mut out);
        out
    }

    fn items_mut(&mut self, f: &mut dyn FnMut(&mut Item)) {
        match self {
            Outcome::Item(i) | Outcome::Pending { item: i, .. } => f(i),
            Outcome::Items(v) => v.iter_mut().for_each(f),
            Outcome::Destroyed => {}
            Outcome::Chance(c) => c.iter_mut().for_each(|(_, o)| o.items_mut(f)),
            Outcome::Choice(c) => c.iter_mut().for_each(|o| o.items_mut(f)),
        }
    }

    /// Merges equal Chance children (summing p), sorts them by p descending then outcome order,
    /// and replaces a Chance left with one child by that child.
    fn normalise(&mut self) {
        match self {
            Outcome::Chance(children) => {
                for (_, o) in children.iter_mut() {
                    o.normalise();
                }
                children.sort_by(|a, b| a.1.order(&b.1));
                let mut merged: Vec<(f64, Outcome)> = Vec::with_capacity(children.len());
                for (p, o) in children.drain(..) {
                    match merged.last_mut() {
                        Some((q, last)) if *last == o => *q += p,
                        _ => merged.push((p, o)),
                    }
                }
                merged.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.order(&b.1)));
                if merged.len() == 1 {
                    *self = merged.pop().map(|(_, o)| o).unwrap_or(Outcome::Destroyed);
                } else {
                    *children = merged;
                }
            }
            Outcome::Choice(options) => options.iter_mut().for_each(Outcome::normalise),
            _ => {}
        }
    }

    /// The Chance masses at this level (1 for a non-Chance outcome).
    pub fn mass(&self) -> f64 {
        match self {
            Outcome::Chance(c) => c.iter().map(|(p, _)| p).sum(),
            _ => 1.0,
        }
    }
}

fn unmet(reason: impl Into<String>) -> Result<(), Unmet> {
    Err(Unmet {
        reason: reason.into(),
    })
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
fn sides(data: &Data, item: &Item, req: SideReq) -> Vec<Side> {
    match req {
        SideReq::Any => Side::BOTH.to_vec(),
        SideReq::Prefix => vec![Side::Prefix],
        SideReq::Suffix => vec![Side::Suffix],
        SideReq::Dominant => dominant_side(data, item).into_iter().collect(),
    }
}

fn open_text(side: Option<Side>) -> &'static str {
    match side {
        Some(Side::Prefix) => "needs an open prefix",
        Some(Side::Suffix) => "needs an open suffix",
        None => "needs an open affix",
    }
}

/// The essence a method's Essence guarantee places.
fn method_essence<'a>(m: &'a Method, params: &'a Params) -> Option<&'a str> {
    params.essence.as_deref().or_else(|| {
        m.requires.iter().find_map(|r| match r {
            Require::EssenceFits { essence } => Some(essence.as_str()),
            _ => None,
        })
    })
}

fn essence_mod(data: &Data, item: &Item, essence: &str) -> Option<u32> {
    let class = data.base_at(item.base).class;
    data.essences
        .iter()
        .find(|e| e.name == essence)?
        .mods
        .iter()
        .find(|(c, _)| *c == class)
        .map(|&(_, m)| m)
}

fn check_one(data: &Data, r: &Require, item: &Item, params: &Params) -> Result<(), Unmet> {
    let class = item.class(data);
    let explicit_count = item.prefixes.len() + item.suffixes.len();
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
                        data.classes
                            .iter()
                            .find(|c| &c.id == id)
                            .map_or(id.as_str(), |c| c.name.as_str())
                    })
                    .collect();
                return unmet(format!("only works on: {}", names.join(", ")));
            }
        }
        Require::NotCorrupted if item.corrupted => return unmet("the item is corrupted"),
        Require::Corrupted if !item.corrupted => return unmet("needs a corrupted item"),
        Require::NotMirrored if item.mirrored => return unmet("the item is mirrored"),
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
        Require::MinExplicits { n } if explicit_count < *n as usize => {
            return unmet(format!("needs at least {n} modifiers"));
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
            if let Some(m) = data.mod_by_id(r#mod) {
                let have = item.explicit_groups(data);
                if let Some(g) = data.mod_at(m).groups.iter().find(|g| have.contains(g)) {
                    return unmet(format!(
                        "the item already has a {} modifier",
                        data.group(*g)
                    ));
                }
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
            let c = data.catalyst_by_name(catalyst);
            if c.is_some() && item.catalyst == c && item.quality >= 20 {
                let label = &data.catalysts[c.unwrap_or_default() as usize].label;
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
        Require::ItemLevelAtMost { n } if item.item_level > *n => {
            return unmet(format!("needs item level {n} or lower"));
        }
        Require::EssenceFits { essence } => {
            // The essence a Reforge places (`Params::essence` overrides the method's own).
            let essence = params.essence.as_deref().unwrap_or(essence);
            if essence_mod(data, item, essence).is_none() {
                return unmet(format!("{essence} has no modifier for {}", class.id));
            }
        }
        Require::Fossils { min, max } => {
            let n = params.fossils.len();
            if n < *min as usize || n > *max as usize {
                return unmet(format!("needs {min} to {max} fossils"));
            }
        }
        Require::ImplicitsChangeable if data.base_at(item.base).implicits_locked => {
            return unmet("the item's implicit modifiers cannot be changed");
        }
        _ => {}
    }
    Ok(())
}

/// Whether the method can be used on these inputs; the first failing requirement's reason if not.
pub fn check(data: &Data, m: &Method, inputs: &[Item], params: &Params) -> Result<(), Unmet> {
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
    Ok(())
}

struct Ctx<'a> {
    data: &'a Data,
    method: &'a Method,
    params: &'a Params,
    last_added: Option<(Side, u32)>,
    leaves: usize,
    /// Set when an op finds the method cannot do its job on this item (an empty tag pool, no
    /// resistance to change); `apply` then reports it as `Unmet`.
    unmet: Option<String>,
}

impl Ctx<'_> {
    fn too_large(&self) -> bool {
        self.leaves > MAX_BRANCHES
    }
}

/// One random-op branch: its chance, the item, and the mod it added (for `keepLastAdded`).
type Branch = (f64, Item, Option<(Side, u32)>);

fn rest_with(head: &[Op], rest: &[Op]) -> Vec<Op> {
    head.iter().chain(rest).cloned().collect()
}

/// Runs each (p, item, last_added) branch through the remaining ops.
fn branch(ctx: &mut Ctx, branches: Vec<Branch>, rest: &[Op]) -> Outcome {
    let saved = ctx.last_added;
    let mut out = Vec::with_capacity(branches.len());
    for (p, item, last) in branches {
        if ctx.too_large() {
            break;
        }
        if last.is_some() {
            ctx.last_added = last;
        }
        out.push((p, run(ctx, item, rest)));
        ctx.last_added = saved;
    }
    Outcome::Chance(out)
}

/// Random-op result: a Chance over the branches, or the item unchanged when there are none.
fn random(ctx: &mut Ctx, item: Item, branches: Vec<Branch>, rest: &[Op]) -> Outcome {
    if branches.is_empty() {
        run(ctx, item, rest)
    } else {
        branch(ctx, branches, rest)
    }
}

/// Candidate mods an AddRandom draws from, with their probabilities.
fn add_candidates(
    data: &Data,
    item: &Item,
    pool: &Pool,
    side: SideReq,
    ignore_limits: bool,
) -> Vec<(u32, f64)> {
    let tags = item.tags(data);
    let base = data.base_at(item.base);
    let groups = item.explicit_groups(data);
    let allowed = sides(data, item, side);
    let level_cap = if item.has_meta(data, MetaMod::Level28) {
        item.item_level.min(28)
    } else {
        item.item_level
    };
    let no_attack = item.has_meta(data, MetaMod::NoAttack);
    let no_caster = item.has_meta(data, MetaMod::NoCaster);
    let attack = data.tag_by_name("attack");
    let caster = data.tag_by_name("caster");
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
            Pool::Natural => true,
            Pool::Influence { influence } => m.influence == Some(*influence),
            Pool::Tag { .. } => tag.is_some_and(|t| m.tags.contains(&t)),
            Pool::Ducat { .. } => true,
        };
        if !pool_ok
            || (kind != ModKind::Ducat && m.domain != base.domain.repoe())
            || m.level > level_cap
            || !allowed.contains(&s)
            || (!ignore_limits && item.open(data, s) == 0)
            || m.groups.iter().any(|g| groups.contains(g))
            || (no_attack && attack.is_some_and(|t| m.tags.contains(&t)))
            || (no_caster && caster.is_some_and(|t| m.tags.contains(&t)))
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

fn add_affix(data: &Data, item: &mut Item, m: u32) -> Option<Side> {
    let side = data.mod_at(m).side?;
    item.affixes_mut(side).push(Affix::new(m));
    Some(side)
}

/// Removes every non-fractured explicit on the sides that can change; with `respect_locks`, a
/// side a metamod locks (decided before anything is removed) is kept whole.
fn clear(data: &Data, item: &mut Item, respect_locks: bool) {
    let locked = [
        respect_locks && item.locked(data, Side::Prefix),
        respect_locks && item.locked(data, Side::Suffix),
    ];
    for side in Side::BOTH {
        if !locked[side.index()] {
            item.affixes_mut(side).retain(|a| a.fractured);
        }
    }
}

/// The weighted pool of one kind for an implicit or enchantment op.
fn kind_pool(
    data: &Data,
    item: &Item,
    kinds: &[ModKind],
    extra: impl Fn(&crate::data::ModData) -> bool,
) -> Vec<(u32, f64)> {
    let tags = item.tags(data);
    let w: Vec<(u32, f64)> = pool::weighted(data, kinds, &tags)
        .filter(|(_, m, _)| m.level <= item.item_level && extra(m))
        .map(|(i, _, w)| (i, w))
        .collect();
    pool::normalise(w)
}

fn veiled_mod(data: &Data, side: Side) -> Option<u32> {
    data.mod_by_id(match side {
        Side::Prefix => "VeiledPrefix",
        Side::Suffix => "VeiledSuffix",
    })
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

fn reforge(ctx: &mut Ctx, mut item: Item, spec: &ReforgeSpec, ops: &[Op]) -> Outcome {
    let data = ctx.data;
    let rest = &ops[1..];
    let pending = |item: Item| Outcome::Pending {
        item,
        ops: ops.to_vec(),
        reason: PendingReason::MultiDraw,
    };
    let guaranteed = matches!(
        spec.guarantee,
        Some(
            Guarantee::Essence
                | Guarantee::Veiled
                | Guarantee::Tag { .. }
                | Guarantee::InfluenceMod
        )
    );
    if !guaranteed || spec.weighting != Weighting::Normal {
        return pending(item);
    }
    item.rarity = spec.rarity;
    clear(data, &mut item, spec.respects_locks);
    let fill: Vec<Op> = std::iter::once(Op::Fill { spec: spec.clone() })
        .chain(rest.iter().cloned())
        .collect();
    let leaf = |item: Item| Outcome::Pending {
        item,
        ops: fill.clone(),
        reason: PendingReason::MultiDraw,
    };
    let branches: Vec<(f64, Item)> = match &spec.guarantee {
        Some(Guarantee::Essence) => {
            let m =
                method_essence(ctx.method, ctx.params).and_then(|e| essence_mod(data, &item, e));
            let groups = item.explicit_groups(data);
            if let Some(m) = m
                && let Some(s) = data.mod_at(m).side
                && item.open(data, s) > 0
                && !data.mod_at(m).groups.iter().any(|g| groups.contains(g))
            {
                add_affix(data, &mut item, m);
            }
            vec![(1.0, item)]
        }
        Some(Guarantee::Veiled) => {
            let room: Vec<Side> = Side::BOTH
                .into_iter()
                .filter(|&s| item.open(data, s) > 0)
                .collect();
            if room.is_empty() {
                vec![(1.0, item)]
            } else {
                let p = 1.0 / room.len() as f64;
                room.into_iter()
                    .filter_map(|s| {
                        let mut it = item.clone();
                        add_affix(data, &mut it, veiled_mod(data, s)?);
                        Some((p, it))
                    })
                    .collect()
            }
        }
        Some(Guarantee::Tag { tag }) => {
            let c = add_candidates(
                data,
                &item,
                &Pool::Tag { tag: tag.clone() },
                SideReq::Any,
                false,
            );
            if c.is_empty() {
                ctx.unmet = Some(format!("no {tag} modifier can roll on this item"));
                return pending(item);
            }
            c.into_iter()
                .map(|(m, p)| {
                    let mut it = item.clone();
                    add_affix(data, &mut it, m);
                    (p, it)
                })
                .collect()
        }
        _ => {
            // InfluenceMod: a Natural mod of one of the item's influences.
            let mut c: Vec<(u32, f64)> = Vec::new();
            for i in Influence::ALL
                .into_iter()
                .filter(|i| item.has_influence(*i))
            {
                let raw = add_candidates(
                    data,
                    &item,
                    &Pool::Influence { influence: i },
                    SideReq::Any,
                    false,
                );
                // Re-weight: candidates of every influence share one pool.
                let tags = item.tags(data);
                c.extend(
                    raw.into_iter()
                        .map(|(m, _)| (m, pool::weight(data.mod_at(m), &tags))),
                );
            }
            let c = pool::normalise(c);
            if c.is_empty() {
                ctx.unmet = Some("no influenced modifier can roll on this item".into());
                return pending(item);
            } else {
                c.into_iter()
                    .map(|(m, p)| {
                        let mut it = item.clone();
                        add_affix(data, &mut it, m);
                        (p, it)
                    })
                    .collect()
            }
        }
    };
    ctx.leaves += branches.len();
    let mut branches = branches;
    if branches.len() == 1 {
        let (_, it) = branches.remove(0);
        return leaf(it);
    }
    Outcome::Chance(branches.into_iter().map(|(p, it)| (p, leaf(it))).collect())
}

fn run(ctx: &mut Ctx, mut item: Item, ops: &[Op]) -> Outcome {
    let data = ctx.data;
    let Some(op) = ops.first() else {
        ctx.leaves += 1;
        return Outcome::Item(item);
    };
    if ctx.too_large() {
        return Outcome::Item(item);
    }
    let rest = &ops[1..];
    match op {
        Op::SetRarity { rarity } => {
            item.rarity = *rarity;
            run(ctx, item, rest)
        }
        Op::ClearExplicits => {
            clear(data, &mut item, true);
            if item.prefixes.is_empty() && item.suffixes.is_empty() {
                item.rarity = Rarity::Normal;
            }
            run(ctx, item, rest)
        }
        Op::RerollMagic => {
            clear(data, &mut item, false);
            run(ctx, item, rest)
        }
        Op::ClearAll => {
            item.prefixes.clear();
            item.suffixes.clear();
            item.rarity = Rarity::Normal;
            run(ctx, item, rest)
        }
        Op::AddRandom {
            pool,
            side,
            ignore_limits,
        } => {
            let branches: Vec<Branch> = add_candidates(data, &item, pool, *side, *ignore_limits)
                .into_iter()
                .map(|(m, p)| {
                    let mut it = item.clone();
                    let s = add_affix(data, &mut it, m);
                    (p, it, s.map(|s| (s, m)))
                })
                .collect();
            // A tag or ducat pool is the method's whole point: with nothing to add it cannot
            // be used on this item.
            if branches.is_empty() {
                match pool {
                    Pool::Tag { tag } => {
                        ctx.unmet = Some(format!("no {tag} modifier can roll on this item"));
                    }
                    Pool::Ducat { .. } => {
                        ctx.unmet =
                            Some("no modifier from this ducat can roll on this item".into());
                    }
                    _ => {}
                }
            }
            random(ctx, item, branches, rest)
        }
        Op::RemoveRandom {
            filter,
            keep_last_added,
            respects_cannot_roll,
        } => {
            let no_attack = *respects_cannot_roll && item.has_meta(data, MetaMod::NoAttack);
            let no_caster = *respects_cannot_roll && item.has_meta(data, MetaMod::NoCaster);
            let attack = data.tag_by_name("attack");
            let caster = data.tag_by_name("caster");
            let filter_sides: Vec<Side> = match filter {
                RemoveFilter::Side { side } => sides(data, &item, *side),
                _ => Side::BOTH.to_vec(),
            };
            let mut candidates = Vec::new();
            for side in Side::BOTH {
                if item.locked(data, side) || !filter_sides.contains(&side) {
                    continue;
                }
                for (i, a) in item.affixes(side).iter().enumerate() {
                    let m = data.mod_at(a.roll.m);
                    let skip = a.fractured
                        || (matches!(filter, RemoveFilter::Influenced) && m.influence.is_none())
                        || (*keep_last_added && ctx.last_added == Some((side, a.roll.m)))
                        || (no_attack && attack.is_some_and(|t| m.tags.contains(&t)))
                        || (no_caster && caster.is_some_and(|t| m.tags.contains(&t)));
                    if !skip {
                        candidates.push((side, i));
                    }
                }
            }
            let p = 1.0 / candidates.len() as f64;
            let branches = candidates
                .into_iter()
                .map(|(side, i)| {
                    let mut it = item.clone();
                    it.affixes_mut(side).remove(i);
                    (p, it, None)
                })
                .collect();
            random(ctx, item, branches, rest)
        }
        Op::AddVeiled => {
            let room: Vec<Side> = Side::BOTH
                .into_iter()
                .filter(|&s| item.open(data, s) > 0)
                .collect();
            let p = 1.0 / room.len() as f64;
            let branches = room
                .into_iter()
                .filter_map(|s| {
                    let m = veiled_mod(data, s)?;
                    let mut it = item.clone();
                    it.affixes_mut(s).push(Affix::new(m));
                    Some((p, it, Some((s, m))))
                })
                .collect();
            random(ctx, item, branches, rest)
        }
        Op::Unveil => unveil(ctx, item, rest),
        Op::AddMod { r#mod } => {
            if let Some(m) = data.mod_by_id(r#mod)
                && let Some(s) = add_affix(data, &mut item, m)
            {
                ctx.last_added = Some((s, m));
            }
            run(ctx, item, rest)
        }
        Op::RemoveCrafted => {
            for side in Side::BOTH {
                item.affixes_mut(side)
                    .retain(|a| data.mod_at(a.roll.m).kind != ModKind::Crafted);
            }
            run(ctx, item, rest)
        }
        Op::SetEnchant { pool: which } => {
            let kind = match which {
                EnchantPool::Lab => ModKind::Enchant,
                EnchantPool::FlaskEnkindling => ModKind::FlaskEnkindling,
                EnchantPool::FlaskInstilling => ModKind::FlaskInstilling,
            };
            let branches = kind_pool(data, &item, &[kind], |_| true)
                .into_iter()
                .map(|(m, p)| {
                    let mut it = item.clone();
                    it.enchants = vec![Roll { m, values: None }];
                    (p, it, None)
                })
                .collect();
            random(ctx, item, branches, rest)
        }
        Op::SetEnchantMod { r#mod } => {
            if let Some(m) = data.mod_by_id(r#mod) {
                item.enchants = vec![Roll { m, values: None }];
            }
            run(ctx, item, rest)
        }
        Op::ClearEnchants => {
            item.enchants.clear();
            run(ctx, item, rest)
        }
        Op::RerollValues { scope } => {
            match scope {
                ValueScope::Explicit => {
                    for side in Side::BOTH {
                        for a in item.affixes_mut(side) {
                            if !a.fractured && data.mod_at(a.roll.m).kind != ModKind::Crafted {
                                a.roll.values = None;
                            }
                        }
                    }
                }
                ValueScope::Implicit => item.implicits.iter_mut().for_each(|r| r.values = None),
                ValueScope::Defences => item.defences = None,
            }
            run(ctx, item, rest)
        }
        Op::Fracture => {
            let candidates: Vec<(Side, usize)> = Side::BOTH
                .into_iter()
                .flat_map(|s| {
                    item.affixes(s)
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| {
                            let k = data.mod_at(a.roll.m).kind;
                            !a.fractured && k != ModKind::Crafted && k != ModKind::Veiled
                        })
                        .map(move |(i, _)| (s, i))
                        .collect::<Vec<_>>()
                })
                .collect();
            let p = 1.0 / candidates.len() as f64;
            let branches = candidates
                .into_iter()
                .map(|(s, i)| {
                    let mut it = item.clone();
                    it.affixes_mut(s)[i].fractured = true;
                    (p, it, None)
                })
                .collect();
            random(ctx, item, branches, rest)
        }
        Op::AddInfluence { influence } => {
            item.influences |= influence.bit();
            run(ctx, item, rest)
        }
        Op::Corrupt => {
            item.corrupted = true;
            run(ctx, item, rest)
        }
        Op::VaalImplicit => {
            if data.base_at(item.base).implicits_locked {
                return run(ctx, item, rest);
            }
            let domain = data.base_at(item.base).domain.repoe();
            let branches = kind_pool(data, &item, &[ModKind::CorruptedImplicit], |m| {
                m.domain == domain
            })
            .into_iter()
            .map(|(m, p)| {
                let mut it = item.clone();
                it.implicits = vec![Roll { m, values: None }];
                (p, it, None)
            })
            .collect();
            random(ctx, item, branches, rest)
        }
        Op::EldritchImplicit { eldritch, tier } => {
            let (kind, other) = match eldritch {
                Eldritch::Exarch => (ModKind::Exarch, ModKind::Eater),
                Eldritch::Eater => (ModKind::Eater, ModKind::Exarch),
            };
            let branches = kind_pool(data, &item, &[kind], |m| m.eldritch_tier == Some(*tier))
                .into_iter()
                .map(|(m, p)| {
                    let mut it = item.clone();
                    it.implicits.retain(|r| data.mod_at(r.m).kind == other);
                    it.implicits.push(Roll { m, values: None });
                    (p, it, None)
                })
                .collect();
            random(ctx, item, branches, rest)
        }
        Op::Dominance => {
            let influenced: Vec<(Side, usize)> = Side::BOTH
                .into_iter()
                .flat_map(|s| {
                    item.affixes(s)
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| !a.fractured && data.mod_at(a.roll.m).influence.is_some())
                        .map(move |(i, _)| (s, i))
                        .collect::<Vec<_>>()
                })
                .collect();
            let n = influenced.len();
            let pairs = n * n.saturating_sub(1);
            let mut branches = Vec::with_capacity(pairs);
            for &(xs, xi) in &influenced {
                for &(ys, yi) in &influenced {
                    if (xs, xi) == (ys, yi) {
                        continue;
                    }
                    let mut it = item.clone();
                    let y = &mut it.affixes_mut(ys)[yi];
                    match upgrade(data, &item, y.roll.m) {
                        Some(up) => {
                            y.roll = Roll {
                                m: up,
                                values: None,
                            }
                        }
                        None => y.roll.values = None,
                    }
                    it.affixes_mut(xs).remove(xi);
                    branches.push((1.0 / pairs as f64, it, None));
                }
            }
            random(ctx, item, branches, rest)
        }
        Op::ChangeResistance { from, to } => {
            if let Err(reason) = change_resistance(data, &mut item, *from, *to) {
                ctx.unmet = Some(reason);
            }
            run(ctx, item, rest)
        }
        Op::AddQuality { by_rarity, max } => {
            let add = by_rarity[item.rarity as usize];
            item.quality = (*max).min(item.quality.saturating_add(add));
            run(ctx, item, rest)
        }
        Op::CatalystQuality { catalyst } => {
            let c = data.catalyst_by_name(catalyst);
            if c.is_some() && item.catalyst == c {
                item.quality = 20.min(item.quality + 1);
            } else {
                item.catalyst = c;
                item.quality = 1;
            }
            run(ctx, item, rest)
        }
        Op::Foresee => {
            item.foreseen = true;
            run(ctx, item, rest)
        }
        Op::Imprint => {
            let mut copy = item.clone();
            copy.imprint = None;
            item.imprint = Some(Box::new(copy));
            run(ctx, item, rest)
        }
        Op::Restore => {
            if let Some(imprint) = item.imprint.take() {
                item = *imprint;
                item.imprint = None;
            }
            run(ctx, item, rest)
        }
        Op::LinkAll => {
            let all: Vec<_> = item.sockets.concat();
            item.sockets = if all.is_empty() {
                Vec::new()
            } else {
                vec![all]
            };
            run(ctx, item, rest)
        }
        Op::Mirror => {
            let mut copy = item.clone();
            copy.mirrored = true;
            ctx.leaves += 2;
            Outcome::Items(vec![item, copy])
        }
        Op::Destroy => {
            ctx.leaves += 1;
            Outcome::Destroyed
        }
        Op::RemoveIntangibility => {
            item.intangibility = 0;
            run(ctx, item, rest)
        }
        Op::Chance { branches } => {
            let saved = ctx.last_added;
            let mut out = Vec::with_capacity(branches.len());
            for b in branches {
                let ops = rest_with(&b.ops, rest);
                out.push((b.p, run(ctx, item.clone(), &ops)));
                ctx.last_added = saved;
            }
            Outcome::Chance(out)
        }
        Op::Reforge { spec } => reforge(ctx, item, spec, ops),
        Op::Fill { .. } => {
            ctx.leaves += 1;
            Outcome::Pending {
                item,
                ops: ops.to_vec(),
                reason: PendingReason::MultiDraw,
            }
        }
        Op::Sockets { .. } => {
            ctx.leaves += 1;
            Outcome::Pending {
                item,
                ops: ops.to_vec(),
                reason: PendingReason::Sockets,
            }
        }
        Op::Unmodelled { reason } => {
            ctx.leaves += 1;
            Outcome::Pending {
                item,
                ops: ops.to_vec(),
                reason: reason.clone(),
            }
        }
    }
}

/// Turns the first changeable (natural, not fractured) `from` resistance into the `to` mod of
/// the same rank among the mods that can spawn on the item (the nearest lower rank when `to` has
/// fewer tiers).
fn change_resistance(
    data: &Data,
    item: &mut Item,
    from: Element,
    to: Element,
) -> Result<(), String> {
    let from_ranks = resistance_ranks(data, item, from.resistance_type());
    let to_ranks = resistance_ranks(data, item, to.resistance_type());
    let found = Side::BOTH.into_iter().find_map(|side| {
        item.affixes(side)
            .iter()
            .position(|a| !a.fractured && from_ranks.contains(&a.roll.m))
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
    let groups = item.explicit_groups(data);
    if data
        .mod_at(target)
        .groups
        .iter()
        .any(|g| groups.contains(g))
    {
        return Err(format!(
            "the item already has a {} modifier",
            to.resistance_type()
        ));
    }
    item.affixes_mut(side).remove(i);
    if let Some(s) = data.mod_at(target).side {
        item.affixes_mut(s).push(Affix::new(target));
    }
    Ok(())
}

/// Unveiling: three options drawn by weight without replacement; the player picks one.
fn unveil(ctx: &mut Ctx, item: Item, rest: &[Op]) -> Outcome {
    let data = ctx.data;
    let Some((side, index)) = item.veiled(data) else {
        return run(ctx, item, rest);
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
    options.sort_by_key(|(m, _)| *m);
    let with = |ctx: &mut Ctx, m: u32| {
        let mut it = item.clone();
        let fractured = it.affixes(side)[index].fractured;
        it.affixes_mut(side)[index] = Affix {
            roll: Roll { m, values: None },
            fractured,
        };
        run(ctx, it, rest)
    };
    if options.is_empty() {
        return run(ctx, item, rest);
    }
    if options.len() <= 3 {
        let choices = options.iter().map(|&(m, _)| with(ctx, m)).collect();
        return Outcome::Choice(choices);
    }
    let total: f64 = options.iter().map(|(_, w)| w).sum();
    let n = options.len();
    let mut sets = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            for c in b + 1..n {
                let idx = [a, b, c];
                let mut p = 0.0;
                for perm in [
                    [0, 1, 2],
                    [0, 2, 1],
                    [1, 0, 2],
                    [1, 2, 0],
                    [2, 0, 1],
                    [2, 1, 0],
                ] {
                    let w = |k: usize| options[idx[perm[k]]].1;
                    p += w(0) / total * w(1) / (total - w(0)) * w(2) / (total - w(0) - w(1));
                }
                if ctx.too_large() {
                    break;
                }
                let choices = idx.iter().map(|&k| with(ctx, options[k].0)).collect();
                sets.push((p, Outcome::Choice(choices)));
            }
        }
    }
    Outcome::Chance(sets)
}

/// Applies a method: checks it, runs its ops on (a clone of) the first input, then clears the
/// foresight, consumes catalyst quality, drops stale defence figures, canonicalises items and
/// merges equal Chance children.
pub fn apply(data: &Data, m: &Method, inputs: &[Item], params: &Params) -> Result<Outcome, Unmet> {
    check(data, m, inputs, params)?;
    let input = &inputs[0];
    let mut ctx = Ctx {
        data,
        method: m,
        params,
        last_added: None,
        leaves: 0,
        unmet: None,
    };
    let mut out = run(&mut ctx, input.clone(), &m.ops);
    if let Some(reason) = ctx.unmet {
        return Err(Unmet { reason });
    }
    if ctx.too_large() {
        return Ok(Outcome::Pending {
            item: input.clone(),
            ops: m.ops.clone(),
            reason: PendingReason::TooLarge,
        });
    }
    let foresees = m.ops.contains(&Op::Foresee);
    // A mirror copies the item exactly, catalyst quality included.
    let consumes = input.catalyst.is_some()
        && matches!(
            m.family,
            Family::Currency | Family::Essence | Family::Fossil
        )
        && !m.ops.contains(&Op::Mirror);
    out.items_mut(&mut |it| {
        if !foresees {
            it.foreseen = false;
        }
        // Claim catalyst.consumed.
        if consumes {
            it.catalyst = None;
            it.quality = 0;
        }
        if it.prefixes != input.prefixes
            || it.suffixes != input.suffixes
            || it.implicits != input.implicits
            || it.quality != input.quality
            || it.rarity != input.rarity
        {
            it.defences = None;
        }
        it.canonical(data);
    });
    out.normalise();
    Ok(out)
}

/// What one use costs: the method's costs, resolved to currency ids (or `beast:` keys); for the
/// fossil family, the resonator for the fossil count plus each fossil.
pub fn cost(data: &Data, m: &Method, params: &Params) -> Vec<Cost> {
    let resolve = |name: &str, amount: f64| Cost {
        key: if name.starts_with("beast:") {
            name.to_string()
        } else {
            data.currency_by_name(name)
                .map_or_else(|| name.to_string(), |c| c.id.clone())
        },
        amount,
    };
    if m.family == Family::Fossil {
        let size = match params.fossils.len() {
            1 => "Primitive",
            2 => "Potent",
            3 => "Powerful",
            _ => "Prime",
        };
        let kind = if m.id.contains("alchemical") {
            "Alchemical"
        } else {
            "Chaotic"
        };
        let mut out = vec![resolve(&format!("{size} {kind} Resonator"), 1.0)];
        for &f in &params.fossils {
            if let Some(fossil) = data.fossils.get(f as usize) {
                out.push(Cost {
                    key: fossil.currency.clone(),
                    amount: 1.0,
                });
            }
        }
        return out;
    }
    m.cost.iter().map(|c| resolve(&c.name, c.amount)).collect()
}
