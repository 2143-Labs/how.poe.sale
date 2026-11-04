//! The generated review catalog: `catalog/methods.md` (every method's cost, game text, needs,
//! effect, a worked example and sources; essences as one table), `catalog/bench.md` (the bench
//! crafts that add one modifier, as one table), `catalog/questions.md` (each unconfirmed rule a
//! method relies on, shown on a full input item with the step, the result and the diff),
//! `catalog/items.md` (every fixture and how each pattern reads it) and `catalog/scope.md`
//! (every currency's methods or exclusion reason). Output is deterministic.

use std::collections::BTreeMap;
use std::path::Path;

use crate::apply::{Outcome, Params, apply};
use crate::data::{Category, Data, Scope};
use crate::item::{Item, ItemSpec};
use crate::method::{
    EnchantPool, Family, Guarantee, Method, Op, PairRequire, Pool, ReforgeSpec, RemoveFilter,
    Require, SideReq, ValueScope, Weighting,
};
use crate::pattern::{Pattern, PatternSpec, evaluate};
use crate::render::{item_block, item_diff, leaf_counts, most_likely_item, outcome};

/// The fixtures the catalog demonstrates on.
pub struct Fixtures {
    pub items: BTreeMap<String, (ItemSpec, Item)>,
    pub patterns: BTreeMap<String, Pattern>,
}

fn read_dir_json(dir: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| {
            let path = e.ok()?.path();
            let name = path
                .file_name()?
                .to_str()?
                .strip_suffix(".json")?
                .to_string();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            Some((name, text))
        })
        .collect();
    out.sort();
    out
}

impl Fixtures {
    /// Reads `items/*.json` and `patterns/*.json`; panics on an invalid fixture.
    pub fn load(data: &Data, dir: &Path) -> Fixtures {
        let mut items = BTreeMap::new();
        for (name, text) in read_dir_json(&dir.join("items")) {
            let spec: ItemSpec =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("items/{name}: {e}"));
            let item = Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("items/{name}: {e}"));
            items.insert(name, (spec, item));
        }
        let mut patterns = BTreeMap::new();
        for (name, text) in read_dir_json(&dir.join("patterns")) {
            let spec: PatternSpec =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("patterns/{name}: {e}"));
            let p =
                Pattern::compile(data, &spec).unwrap_or_else(|e| panic!("patterns/{name}: {e}"));
            patterns.insert(name, p);
        }
        Fixtures { items, patterns }
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

fn side_phrase(s: SideReq) -> &'static str {
    match s {
        SideReq::Any => "either side",
        SideReq::Prefix => "prefix",
        SideReq::Suffix => "suffix",
        SideReq::Dominant => "the dominant eldritch side",
    }
}

/// Class names, with a whole category (or all of it but one or two classes) said once.
fn class_list(data: &Data, ids: &[String]) -> String {
    let mut parts = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    for c in &data.classes {
        if done.contains(&c.id.as_str()) || !ids.contains(&c.id) {
            continue;
        }
        let members: Vec<&crate::data::ClassData> = data
            .classes
            .iter()
            .filter(|x| x.category == c.category)
            .collect();
        let missing: Vec<&str> = members
            .iter()
            .filter(|x| !ids.contains(&x.id))
            .map(|x| x.name.as_str())
            .collect();
        if members.len() > 3 && missing.len() <= 2 {
            let mut text = format!("all {}", category_name(c.category));
            if !missing.is_empty() {
                text.push_str(&format!(" but {}", missing.join(" and ")));
            }
            parts.push(text);
            done.extend(members.iter().map(|x| x.id.as_str()));
        } else {
            parts.push(c.name.clone());
            done.push(c.id.as_str());
        }
    }
    parts.join(", ")
}

fn mod_line(data: &Data, id: &str) -> String {
    data.mod_by_id(id)
        .map(|m| data.mod_at(m).text.lines().next().unwrap_or(id).to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| id.to_string())
}

/// The positive phrase of a requirement.
pub fn require_phrase(data: &Data, r: &Require) -> String {
    match r {
        Require::Rarity { any_of } => {
            let names: Vec<&str> = any_of.iter().map(|r| r.name()).collect();
            format!("a {} item", names.join(" or "))
        }
        Require::Category { any_of } => {
            let names: Vec<&str> = any_of.iter().map(|c| category_name(*c)).collect();
            format!("one of: {}", names.join(", "))
        }
        Require::Class { any_of } => format!("one of: {}", class_list(data, any_of)),
        Require::NotCorrupted => "not corrupted".into(),
        Require::Corrupted => "corrupted".into(),
        Require::NotMirrored => "not mirrored".into(),
        Require::NotSplit => "not split".into(),
        Require::NotSynthesised => "not synthesised".into(),
        Require::NotUnique => "not unique".into(),
        Require::NoFractured => "no fractured modifier".into(),
        Require::NoEnchant => "no enchantment".into(),
        Require::NoInfluence => "no influence".into(),
        Require::HasInfluence { influence } => match influence {
            Some(i) => format!("{} influence", i.name()),
            None => "an influence".into(),
        },
        Require::SingleInfluence => "exactly one influence".into(),
        Require::Influenceable => "a class that can be influenced".into(),
        Require::OpenAffix { side } => match side {
            SideReq::Any => "an open affix".into(),
            SideReq::Prefix => "an open prefix".into(),
            SideReq::Suffix => "an open suffix".into(),
            SideReq::Dominant => "an open affix on the dominant eldritch side".into(),
        },
        Require::FullSuffixes => "full suffixes".into(),
        Require::MinExplicits { n } => format!("at least {n} modifiers"),
        Require::MinInfluencedMods { n } => format!("at least {n} influenced modifiers"),
        Require::HasImplicit => "an implicit modifier".into(),
        Require::HasCrafted => "a crafted modifier".into(),
        Require::HasEnchant => "an enchantment".into(),
        Require::CraftedRoom => "room for another crafted modifier".into(),
        Require::GroupFree { r#mod } => {
            let group = data
                .mod_by_id(r#mod)
                .and_then(|m| data.mod_at(m).groups.first().copied())
                .map_or(r#mod.as_str(), |g| data.group(g));
            format!("no {group} modifier yet")
        }
        Require::HasVeiled => "a veiled modifier".into(),
        Require::NoVeiled => "no veiled modifier".into(),
        Require::QualityBelow { max } => format!("quality below {max}%"),
        Require::CatalystRoom { catalyst } => {
            let label = data
                .catalysts
                .iter()
                .find(|c| &c.name == catalyst)
                .map_or(catalyst.as_str(), |c| c.label.as_str());
            format!("less than 20% {label} quality")
        }
        Require::HasImprint => "an imprint".into(),
        Require::NotForeseen => "not already foreseeing".into(),
        Require::EldritchDominant => "a dominant eldritch implicit".into(),
        Require::BothEldritch => "both a Searing Exarch and an Eater of Worlds implicit".into(),
        Require::NoEldritch => "no eldritch implicit".into(),
        Require::HasSockets => "sockets".into(),
        Require::CanHaveSockets => "a class that has sockets".into(),
        Require::Intangible => "intangibility".into(),
        Require::HasMemoryStrands => "Memory Strands".into(),
        Require::HasModType { any_of } => format!("a {} modifier", any_of.join(" or ")),
        Require::ItemLevelAtMost { n } => format!("item level {n} or lower"),
        Require::EssenceFits { essence } => format!("a class {essence} has a modifier for"),
        Require::Fossils { min, max } => format!("{min} to {max} fossils"),
        Require::ImplicitsChangeable => "implicit modifiers that can be changed".into(),
    }
}

fn pool_phrase(p: &Pool) -> String {
    match p {
        Pool::Natural => "any".into(),
        Pool::Influence { influence } => format!("{} influence", influence.name()),
        Pool::Tag { tag } => format!("{tag} tag"),
        Pool::Ducat { tag } => format!("ducat pool {tag}"),
    }
}

fn reforge_phrase(s: &ReforgeSpec) -> String {
    let mut out = format!("reforges the item as {}", s.rarity.name());
    if let Some(g) = &s.guarantee {
        out.push_str(match g {
            Guarantee::Essence => ", guaranteeing the essence's modifier",
            Guarantee::Veiled => ", guaranteeing a veiled modifier",
            Guarantee::Tag { .. } => "",
            Guarantee::Infamous => ", guaranteeing an infamous modifier",
            Guarantee::InfluenceMod => ", guaranteeing an influenced modifier",
            Guarantee::InfluenceFromEachInput => ", keeping one influence from each input",
        });
        if let Guarantee::Tag { tag } = g {
            out.push_str(&format!(", guaranteeing a {tag} modifier"));
        }
    }
    if s.respects_locks {
        out.push_str(", keeping a side a metamod locks");
    }
    if s.side == Some(SideReq::Dominant) {
        out.push_str(", rerolling only the dominant eldritch side");
    }
    out.push_str(match s.weighting {
        Weighting::Normal => "",
        Weighting::Fossils => ", weighted by the fossils",
        Weighting::MoreLikelySame => ", much more likely to give the same modifiers",
        Weighting::LessLikelySame => ", much less likely to give the same modifiers",
        Weighting::Recombine => ", recombining the two inputs",
    });
    out
}

/// One sentence per op.
pub fn op_sentence(data: &Data, op: &Op) -> String {
    match op {
        Op::SetRarity { rarity } => format!("sets the rarity to {}", rarity.name()),
        Op::ClearExplicits => "removes every modifier that can change (a side a metamod locks is kept; with nothing left the item turns normal)".into(),
        Op::RerollMagic => "removes every modifier that is not fractured".into(),
        Op::ClearAll => "removes every modifier and makes the item normal".into(),
        Op::AddRandom { pool, side, ignore_limits } => format!(
            "adds one random modifier ({}, {}){}",
            pool_phrase(pool),
            side_phrase(*side),
            if *ignore_limits { ", ignoring the affix limits" } else { "" }
        ),
        Op::RemoveRandom { filter, keep_last_added, respects_cannot_roll } => {
            let mut s = "removes one random modifier that can change".to_string();
            match filter {
                RemoveFilter::Any => {}
                RemoveFilter::Side { side } => s.push_str(&format!(" ({})", side_phrase(*side))),
                RemoveFilter::Influenced => s.push_str(" (an influenced one)"),
            }
            if *keep_last_added {
                s.push_str(", never the one just added");
            }
            if *respects_cannot_roll {
                s.push_str(", never one Cannot Roll Attack/Caster Modifiers protects");
            }
            s
        }
        Op::AddVeiled => "adds a veiled modifier on a side with room (half each when both have room)".into(),
        Op::Unveil => "reveals the veiled modifier: three weighted options, you pick one".into(),
        Op::AddMod { r#mod } => format!("adds {} (`{mod}`)", mod_line(data, r#mod), mod = r#mod),
        Op::RemoveCrafted => "removes every crafted modifier".into(),
        Op::SetEnchant { pool } => format!(
            "sets a random {} enchantment",
            match pool {
                EnchantPool::Lab => "Labyrinth",
                EnchantPool::FlaskEnkindling => "enkindling",
                EnchantPool::FlaskInstilling => "instilling",
            }
        ),
        Op::SetEnchantMod { r#mod } => format!("sets the enchantment {}", mod_line(data, r#mod)),
        Op::ClearEnchants => "removes the enchantments".into(),
        Op::RerollValues { scope } => match scope {
            ValueScope::Explicit => "rerolls the values of the modifiers that are not crafted or fractured".into(),
            ValueScope::Implicit => "rerolls the values of the implicit modifiers".into(),
            ValueScope::Defences => "rerolls the base defences".into(),
        },
        Op::Fracture => "fractures one random modifier (not crafted or veiled)".into(),
        Op::AddInfluence { influence } => format!("adds {} influence", influence.name()),
        Op::Corrupt => "corrupts the item".into(),
        Op::VaalImplicit => "replaces the implicits with a random corrupted implicit (no change when the implicits cannot be changed)".into(),
        Op::EldritchImplicit { eldritch, tier } => format!(
            "replaces the {} implicit with a random tier {tier} one",
            match eldritch {
                crate::data::Eldritch::Exarch => "Searing Exarch",
                crate::data::Eldritch::Eater => "Eater of Worlds",
            }
        ),
        Op::Dominance => "removes one influenced modifier and upgrades another one tier".into(),
        Op::ChangeResistance { from, to } => format!(
            "turns a {} modifier into the {} modifier of the same rank",
            from.resistance_type(),
            to.resistance_type()
        ),
        Op::AddQuality { by_rarity, max } => format!(
            "adds {}% / {}% / {}% / {}% quality to a normal / magic / rare / unique item, up to {max}%",
            by_rarity[0], by_rarity[1], by_rarity[2], by_rarity[3]
        ),
        Op::CatalystQuality { catalyst } => {
            let label = data
                .catalysts
                .iter()
                .find(|c| &c.name == catalyst)
                .map_or(catalyst.as_str(), |c| c.label.as_str());
            format!("adds 1% {label} quality (another catalyst type is replaced and starts at 1%)")
        }
        Op::Foresee => "makes the item foresee the result of the next currency".into(),
        Op::Imprint => "stores an imprint of the item".into(),
        Op::Restore => "restores the item to its imprint".into(),
        Op::LinkAll => "links every socket".into(),
        Op::Mirror => "makes a mirrored copy".into(),
        Op::Destroy => "destroys the item".into(),
        Op::RemoveIntangibility => "removes intangibility".into(),
        Op::Chance { branches } => {
            let parts: Vec<String> = branches
                .iter()
                .map(|b| {
                    let what = if b.ops.is_empty() {
                        "nothing else".to_string()
                    } else {
                        b.ops.iter().map(|o| op_sentence(data, o)).collect::<Vec<_>>().join(", then ")
                    };
                    format!("{:.0}%: {what}", b.p * 100.0)
                })
                .collect();
            format!("by chance, one of: {}", parts.join("; "))
        }
        Op::Reforge { spec } => reforge_phrase(spec),
        Op::Fill { .. } => "fills the rest of the reforge".into(),
        Op::Sockets { change } => format!(
            "changes the sockets ({})",
            serde_json::to_value(change)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        ),
        Op::Unmodelled { reason } => format!("is not modelled yet ({})", reason.text()),
    }
}

fn family_title(f: Family) -> &'static str {
    match f {
        Family::Currency => "Currency",
        Family::Bench => "Crafting bench",
        Family::Essence => "Essences",
        Family::Fossil => "Fossils",
        Family::Catalyst => "Catalysts",
        Family::Harvest => "Harvest",
        Family::Beast => "Beastcrafting",
        Family::Unveil => "Unveiling",
    }
}

fn cost_text(m: &Method) -> String {
    if m.family == Family::Fossil {
        return "a resonator sized for the fossil count (Primitive 1, Potent 2, Powerful 3, Prime 4) plus each fossil".into();
    }
    if m.cost.is_empty() {
        return "none".into();
    }
    m.cost
        .iter()
        .map(|c| {
            let name = c
                .name
                .strip_prefix("beast:")
                .map_or(c.name.clone(), |b| format!("{b} (beast)"));
            format!("{} × {name}", c.amount)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn sources(data: &Data, m: &Method) -> String {
    if m.claims.is_empty() {
        return "none".into();
    }
    m.claims
        .iter()
        .map(|id| match data.claim(id) {
            Some(c) => {
                let status = if c.checked {
                    "checked"
                } else {
                    "needs the owner"
                };
                if c.url.is_empty() {
                    format!("`{id}` ({status}, {})", c.source_kind.name())
                } else {
                    format!("`{id}` ({status}, {}: {})", c.source_kind.name(), c.url)
                }
            }
            None => format!("`{id}` (unknown claim)"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn apply_on(data: &Data, m: &Method, item: &Item) -> Result<Outcome, String> {
    apply(data, m, std::slice::from_ref(item), &Params::default()).map_err(|e| e.reason)
}

fn summary(data: &Data, m: &Method, item: &Item) -> String {
    match apply_on(data, m, item) {
        Err(reason) => format!("does not apply: {reason}"),
        Ok(o) => {
            let (exact, pending) = leaf_counts(&o);
            let mut reasons: Vec<String> = Vec::new();
            collect_reasons(&o, &mut reasons);
            match (exact, pending) {
                (n, 0) => format!("{n} exact outcome{}", if n == 1 { "" } else { "s" }),
                (0, _) => format!("not computed: {}", reasons.join(", ")),
                (n, k) => format!(
                    "{n} exact outcomes; {k} not computed ({})",
                    reasons.join(", ")
                ),
            }
        }
    }
}

fn collect_reasons(o: &Outcome, out: &mut Vec<String>) {
    match o {
        Outcome::Pending { reason, .. } => {
            let t = reason.text();
            if !out.contains(&t) {
                out.push(t);
            }
        }
        Outcome::Chance(c) => c.iter().for_each(|(_, o)| collect_reasons(o, out)),
        Outcome::Choice(c) => c.iter().for_each(|o| collect_reasons(o, out)),
        _ => {}
    }
}

/// `bench/761` sorts after `bench/99`: the numeric tail of an id compares as a number.
fn natural_key(id: &str) -> (String, u64) {
    match id.rsplit_once('/') {
        Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => {
            (head.to_string(), tail.parse().unwrap_or(0))
        }
        _ => (id.to_string(), 0),
    }
}

fn methods_md(data: &Data, fx: &Fixtures) -> String {
    let mut out = vec![
        "# Crafting methods".to_string(),
        String::new(),
        "Generated by `datagen-v2 catalog`; do not edit. Each method lists what it needs, what it does, how it fares on its example fixture, and the claims it rests on. The bench crafts that add one modifier are listed in [bench.md](bench.md).".into(),
    ];
    for family in Family::ALL {
        let mut methods: Vec<&Method> =
            data.methods.iter().filter(|m| m.family == family).collect();
        methods.sort_by_key(|m| natural_key(&m.id));
        if methods.is_empty() {
            continue;
        }
        out.push(String::new());
        out.push(format!(
            "## {} ({} methods)",
            family_title(family),
            methods.len()
        ));
        let (bench, rest): (Vec<&Method>, Vec<&Method>) = methods
            .into_iter()
            .partition(|m| standard_bench_add(m).is_some());
        if !bench.is_empty() {
            out.push(String::new());
            out.push(format!(
                "The {} crafts that add one modifier are in [bench.md](bench.md); the others follow.",
                bench.len()
            ));
        }
        let (essences, full): (Vec<&Method>, Vec<&Method>) = rest
            .into_iter()
            .partition(|m| standard_essence(m).is_some());
        if !essences.is_empty() {
            out.push(String::new());
            out.push(essence_table(data, &essences));
        }
        for m in full {
            out.push(String::new());
            out.extend(method_entry(data, fx, m));
        }
    }
    out.push(String::new());
    out.join("\n")
}

fn bench_md(data: &Data) -> String {
    let mut methods: Vec<&Method> = data
        .methods
        .iter()
        .filter(|m| standard_bench_add(m).is_some())
        .collect();
    methods.sort_by_key(|m| natural_key(&m.id));
    let mut out = vec![
        "# Bench crafts that add a modifier".to_string(),
        String::new(),
        "Generated by `datagen-v2 catalog`; do not edit. Every other method is in [methods.md](methods.md).".into(),
    ];
    if !methods.is_empty() {
        out.push(String::new());
        out.push(bench_table(data, &methods));
    }
    out.push(String::new());
    out.join("\n")
}

const ESSENCE_CLAIMS: [&str; 3] = ["game-text", "essence.ilvl", "reforge.guarantee-first"];

/// A generated essence method: the rarities it takes and its item level cap.
fn standard_essence(m: &Method) -> Option<(&[crate::data::Rarity], Option<u8>)> {
    let [Op::Reforge { spec }] = m.ops.as_slice() else {
        return None;
    };
    let (rarities, cap) = match m.requires.as_slice() {
        [
            Require::Rarity { any_of },
            Require::EssenceFits { .. },
            Require::NotCorrupted,
            Require::NotMirrored,
        ] => (any_of, None),
        [
            Require::Rarity { any_of },
            Require::EssenceFits { .. },
            Require::NotCorrupted,
            Require::NotMirrored,
            Require::ItemLevelAtMost { n },
        ] => (any_of, Some(*n)),
        _ => return None,
    };
    let standard = m.family == Family::Essence
        && spec.guarantee == Some(Guarantee::Essence)
        && spec.respects_locks
        && m.claims == ESSENCE_CLAIMS
        && m.example.is_none();
    standard.then_some((rarities.as_slice(), cap))
}

fn essence_table(data: &Data, methods: &[&Method]) -> String {
    let first = methods[0];
    let mut texts: Vec<&str> = methods
        .iter()
        .filter_map(|m| m.game_text.as_deref())
        .collect();
    texts.sort_unstable();
    texts.dedup();
    let mut out = vec![
        format!(
            "Each essence in this table costs one of itself and needs: the rarity listed; a class the essence has a modifier for; not corrupted; not mirrored; and, when listed, an item level at most the cap. It {}. Game text: {}. Sources: {}.",
            first
                .ops
                .iter()
                .map(|o| op_sentence(data, o))
                .collect::<Vec<_>>()
                .join("; then "),
            texts
                .iter()
                .map(|t| format!("\"{}\"", t.replace('\n', " / ")))
                .collect::<Vec<_>>()
                .join(" or "),
            sources(data, first)
        ),
        String::new(),
        "|Method|Rarity|Item level at most|".to_string(),
        "|---|---|---|".to_string(),
    ];
    for m in methods {
        let Some((rarities, cap)) = standard_essence(m) else {
            continue;
        };
        let names: Vec<&str> = rarities.iter().map(|r| r.name()).collect();
        out.push(format!(
            "|{} — `{}`|{}|{}|",
            m.name,
            m.id,
            names.join(" or "),
            cap.map_or("—".to_string(), |n| n.to_string())
        ));
    }
    out.join("\n")
}

const BENCH_ADD_CLAIMS: [&str; 2] = ["bench.magic-or-rare", "metamod.slot"];

/// A generated bench craft that adds one modifier with the standard requirements: its mod id,
/// side and classes. Those print as one table row instead of a full entry.
fn standard_bench_add(m: &Method) -> Option<(&str, SideReq, &[String])> {
    let [Op::AddMod { r#mod }] = m.ops.as_slice() else {
        return None;
    };
    let [
        Require::Rarity { any_of },
        Require::NotCorrupted,
        Require::NotMirrored,
        Require::Class { any_of: classes },
        Require::OpenAffix { side },
        Require::CraftedRoom,
        Require::GroupFree { r#mod: group_mod },
    ] = m.requires.as_slice()
    else {
        return None;
    };
    let standard = m.family == Family::Bench
        && any_of == &[crate::data::Rarity::Magic, crate::data::Rarity::Rare]
        && group_mod == r#mod
        && m.claims == BENCH_ADD_CLAIMS
        && m.inputs == 1
        && m.pair.is_empty()
        && m.example.is_none()
        && m.game_text.is_none();
    standard.then_some((r#mod.as_str(), *side, classes.as_slice()))
}

fn bench_table(data: &Data, methods: &[&Method]) -> String {
    let first = methods[0];
    let mut out = vec![
        format!(
            "Each bench craft in this table needs: {}; not corrupted; not mirrored; one of the classes listed; an open affix on the modifier's side; {}; no modifier of the added one's group yet. It adds that modifier. Sources: {}.",
            require_phrase(data, &first.requires[0]),
            require_phrase(data, &Require::CraftedRoom),
            sources(data, first)
        ),
        String::new(),
        "|Method|Adds|Side|Cost|Classes|".to_string(),
        "|---|---|---|---|---|".to_string(),
    ];
    for m in methods {
        let Some((id, side, classes)) = standard_bench_add(m) else {
            continue;
        };
        out.push(format!(
            "|`{}`|{}|{}|{}|{}|",
            m.id,
            mod_line(data, id).replace('|', "\\|"),
            side_phrase(side),
            cost_text(m),
            class_list(data, classes)
        ));
    }
    out.join("\n")
}

fn method_entry(data: &Data, fx: &Fixtures, m: &Method) -> Vec<String> {
    let mut lines = vec![
        format!("### {} — `{}`", m.name, m.id),
        String::new(),
        format!("- Cost: {}", cost_text(m)),
    ];
    if let Some(t) = &m.game_text {
        lines.push(format!("- Game text: {}", t.replace('\n', " / ")));
    }
    if m.inputs > 1 {
        lines.push(format!("- Takes {} items.", m.inputs));
    }
    let mut needs: Vec<String> = m.requires.iter().map(|r| require_phrase(data, r)).collect();
    needs.extend(m.pair.iter().map(|p| match p {
        PairRequire::SameClass => "both items of the same class".to_string(),
        PairRequire::DifferentSingleInfluence => {
            "one influence on each item, and different ones".to_string()
        }
    }));
    lines.push(format!(
        "- Needs: {}",
        if needs.is_empty() {
            "nothing".to_string()
        } else {
            needs.join("; ")
        }
    ));
    let does: Vec<String> = m.ops.iter().map(|o| op_sentence(data, o)).collect();
    lines.push(format!("- Does: {}.", does.join("; then ")));
    if let Some(ex) = &m.example {
        let line = match fx.items.get(ex) {
            Some((_, item)) => summary(data, m, item),
            None => format!("unknown fixture {ex}"),
        };
        lines.push(format!("- On `{ex}`: {line}"));
    }
    lines.push(format!("- Sources: {}", sources(data, m)));
    lines
}

/// The worked example for a claim: the first method (by id) citing it that names an example
/// fixture, else the first (method, fixture) pair where the method applies.
fn worked_example<'a>(
    data: &Data,
    fx: &'a Fixtures,
    users: &[&'a Method],
) -> Option<(&'a Method, &'a str, &'a Item)> {
    for m in users {
        if let Some(ex) = &m.example
            && let Some((name, (_, item))) = fx.items.get_key_value(ex)
        {
            return Some((m, name.as_str(), item));
        }
    }
    for m in users {
        for (name, (_, item)) in &fx.items {
            if apply_on(data, m, item).is_ok() {
                return Some((m, name.as_str(), item));
            }
        }
    }
    None
}

fn questions_md(data: &Data, fx: &Fixtures) -> String {
    let mut out = vec![
        "# Questions for the owner".to_string(),
        String::new(),
        "Generated by `datagen-v2 catalog`; do not edit. Each section is a rule a method relies on that no published source confirmed. It shows the rule on a full input item: the step, the full result and the diff between the input and the most likely result. Confirm or correct the proposed rule.".into(),
    ];
    for claim in data.claims.iter().filter(|c| !c.checked) {
        let users: Vec<&Method> = data
            .methods
            .iter()
            .filter(|m| m.claims.contains(&claim.id))
            .collect();
        if users.is_empty() {
            continue;
        }
        out.push(String::new());
        out.push(format!("## `{}`", claim.id));
        out.push(String::new());
        out.push(claim.statement.clone());
        if !claim.url.is_empty() {
            out.push(String::new());
            out.push(format!(
                "Current source ({}): {}",
                claim.source_kind.name(),
                claim.url
            ));
        }
        out.push(String::new());
        match worked_example(data, fx, &users) {
            Some((m, name, item)) => {
                out.push(format!("**Input** (`{name}`)"));
                out.push(String::new());
                out.push(item_block(data, item));
                out.push(String::new());
                out.push(format!("**Step**: {} (`{}`)", m.name, m.id));
                out.push(String::new());
                out.push("**Result**".into());
                out.push(String::new());
                match apply_on(data, m, item) {
                    Ok(o) => {
                        out.push(outcome(data, &o));
                        if let Some(best) = most_likely_item(&o) {
                            out.push(String::new());
                            out.push("**Diff** (input vs the most likely result)".into());
                            out.push(String::new());
                            out.push(item_diff(data, item, best));
                        }
                    }
                    Err(reason) => out.push(format!("does not apply: {reason}")),
                }
            }
            None => out.push("No fixture demonstrates this rule yet.".into()),
        }
        out.push(String::new());
        out.push(format!("**Proposed rule:** {}", claim.statement));
        out.push(String::new());
        let shown: Vec<String> = users
            .iter()
            .take(8)
            .map(|m| format!("`{}`", m.id))
            .collect();
        let more = users.len().saturating_sub(8);
        out.push(format!(
            "Used by {} method{}: {}{}",
            users.len(),
            if users.len() == 1 { "" } else { "s" },
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ));
    }
    out.push(String::new());
    out.push("## Pattern notes".into());
    for (name, p) in &fx.patterns {
        for note in &p.notes {
            out.push(String::new());
            out.push(format!("- `{name}`: {note}"));
        }
    }
    out.push(String::new());
    out.join("\n")
}

fn items_md(data: &Data, fx: &Fixtures) -> String {
    let mut out = vec![
        "# Item fixtures".to_string(),
        String::new(),
        "Generated by `datagen-v2 catalog`; do not edit. Every fixture in `fixtures/v2/items`, as game text, and how each pattern in `fixtures/v2/patterns` reads it.".into(),
    ];
    for (name, (spec, item)) in &fx.items {
        out.push(String::new());
        out.push(format!("## `{name}`"));
        if let Some(note) = &spec.note {
            out.push(String::new());
            out.push(format!("Note: {note}"));
        }
        out.push(String::new());
        out.push(item_block(data, item));
        out.push(String::new());
        out.push("|Pattern|p|Item fits|Prefix classes|Suffix classes|Missing slots (prefix / suffix)|Matches|".into());
        out.push("|---|---|---|---|---|---|---|".into());
        for (pname, p) in &fx.patterns {
            for (prob, s) in evaluate(data, p, item) {
                let classes = |list: &[u16]| {
                    let ids: Vec<&str> = list
                        .iter()
                        .map(|&c| p.classes[c as usize].id.as_str())
                        .collect();
                    if ids.is_empty() {
                        "—".to_string()
                    } else {
                        ids.join(", ")
                    }
                };
                let slots = |list: &[u8]| {
                    if list.is_empty() {
                        "none".to_string()
                    } else {
                        list.iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                };
                out.push(format!(
                    "|`{pname}`|{prob:.6}|{}|{}|{}|{} / {}|{}|",
                    if s.item_ok { "yes" } else { "no" },
                    classes(&s.prefixes),
                    classes(&s.suffixes),
                    slots(&s.missing[0]),
                    slots(&s.missing[1]),
                    if s.matches { "yes" } else { "no" },
                ));
            }
        }
    }
    out.push(String::new());
    out.join("\n")
}

fn scope_md(data: &Data) -> String {
    let mut out = vec![
        "# Currency scope".to_string(),
        String::new(),
        "Generated by `datagen-v2 catalog`; do not edit. Every released currency in the pinned game data and what v2 does with it (`rules/v2/currency-scope.json`).".into(),
        String::new(),
    ];
    let (mut used, mut excluded) = (0, 0);
    let mut rows = Vec::new();
    for (id, s) in &data.scope {
        let name = data
            .currency_by_id(id)
            .map(|c| c.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or("(no name)");
        let what = match s {
            Scope::Methods { ids } => {
                used += 1;
                ids.iter()
                    .map(|i| format!("`{i}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            Scope::Excluded { reason } => {
                excluded += 1;
                format!("excluded: {reason}")
            }
        };
        rows.push(format!("|{}|`{id}`|{what}|", name.replace('|', "\\|")));
    }
    out.push(format!(
        "{used} currencies have methods; {excluded} are excluded."
    ));
    out.push(String::new());
    out.push("|Currency|Metadata id|Methods or exclusion|".into());
    out.push("|---|---|---|".into());
    out.extend(rows);
    out.push(String::new());
    out.join("\n")
}

/// Every catalog file name and its content.
pub fn render_all(data: &Data, fixtures: &Path) -> BTreeMap<String, String> {
    let fx = Fixtures::load(data, fixtures);
    let mut out = BTreeMap::new();
    out.insert("methods.md".to_string(), methods_md(data, &fx));
    out.insert("bench.md".to_string(), bench_md(data));
    out.insert("questions.md".to_string(), questions_md(data, &fx));
    out.insert("items.md".to_string(), items_md(data, &fx));
    out.insert("scope.md".to_string(), scope_md(data));
    out
}
