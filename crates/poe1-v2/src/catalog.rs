//! The generated review catalog: `catalog/methods.md` (every method's cost, game text and usage
//! line, needs, metamod policy, effect, a worked example counted in leaves, and sources;
//! essences as one table), `catalog/bench.md` (the bench crafts that add one modifier, as one
//! table), `catalog/questions.md` (each unconfirmed rule a method relies on, shown on the item
//! inputs that exercise it with the step, the result and the diff; rules no method cites listed
//! with what uses them), `catalog/items.md` (every fixture and how each pattern reads it) and
//! `catalog/scope.md` (every currency's methods or exclusion reason). The question examples come
//! from `fixtures/v2/questions.json`, and generation fails when an unconfirmed rule that a
//! method cites has no example. Output is deterministic.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::apply::{Leaf, Outcome, Params, apply};
use crate::data::{Category, Data, Scope};
use crate::item::{Item, ItemSpec};
use crate::method::{
    EnchantPool, Family, Fill, Method, Op, PairRequire, Policy, Pool, Protection, RemoveFilter,
    Require, SideReq, SocketChange, ValueScope, Weighting,
};
use crate::pattern::{Pattern, PatternSpec, evaluate};
use crate::render::{item_block, item_diff, leaf_counts, most_likely_item, outcome};

/// The fixtures the catalog demonstrates on.
pub struct Fixtures {
    pub items: BTreeMap<String, (ItemSpec, Item)>,
    pub patterns: BTreeMap<String, Pattern>,
    pub examples: BTreeMap<String, Vec<ClaimExample>>,
    pub evidence: BTreeMap<String, ClaimEvidence>,
}

/// One question example: a method, the item fixtures it runs on, and what the example shows.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimExample {
    pub method: String,
    pub fixtures: Vec<String>,
    pub shows: String,
}

/// Evidence for an unconfirmed rule that no method cites: the item fixture that the rule
/// decides, what uses the rule, and what the example shows.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimEvidence {
    pub fixtures: Vec<String>,
    pub users: String,
    pub shows: String,
}

/// `fixtures/v2/questions.json`.
#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuestionExamples {
    #[serde(default)]
    examples: BTreeMap<String, Vec<ClaimExample>>,
    #[serde(default)]
    evidence: BTreeMap<String, ClaimEvidence>,
}

/// A catalog that cannot be generated: a missing or mismatched question example.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CatalogError(pub String);

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
    /// Reads `items/*.json`, `patterns/*.json` and `questions.json`; panics on an invalid
    /// fixture or a malformed questions file. `validate_examples` checks the references against
    /// the bundle.
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
        let path = dir.join("questions.json");
        let examples = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let parsed: QuestionExamples = serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                (parsed.examples, parsed.evidence)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (BTreeMap::new(), BTreeMap::new())
            }
            Err(e) => panic!("{}: {e}", path.display()),
        };
        Fixtures {
            items,
            patterns,
            examples: examples.0,
            evidence: examples.1,
        }
    }
}

/// Checks the question examples against the bundle: every unconfirmed rule a method cites has an
/// example, every example names a method that cites the rule and lists exactly as many item
/// fixtures as the method takes, every fixture name loads, and every piece of evidence names an
/// unconfirmed rule that no method cites.
pub fn validate_examples(data: &Data, fx: &Fixtures) -> Result<(), CatalogError> {
    let mut errors: Vec<String> = Vec::new();
    for (claim_id, examples) in &fx.examples {
        if data.claim(claim_id).is_none() {
            errors.push(format!("questions.json names unknown rule `{claim_id}`"));
            continue;
        }
        if examples.is_empty() {
            errors.push(format!("questions.json lists no example for `{claim_id}`"));
        }
        for example in examples {
            if example.shows.trim().is_empty() {
                errors.push(format!(
                    "the example for `{claim_id}` has no `shows` sentence"
                ));
            }
            let Some(method) = data.method(&example.method) else {
                errors.push(format!(
                    "the example for `{claim_id}` names unknown method `{}`",
                    example.method
                ));
                continue;
            };
            if !method.claims.contains(claim_id) {
                errors.push(format!(
                    "the example for `{claim_id}` uses `{}`, which does not cite it",
                    example.method
                ));
            }
            let wanted = method.inputs as usize;
            if example.fixtures.len() != wanted {
                errors.push(format!(
                    "`{}` takes {wanted} item{}, but the example for `{claim_id}` lists {}",
                    example.method,
                    if wanted == 1 { "" } else { "s" },
                    example.fixtures.len()
                ));
            }
            for name in &example.fixtures {
                if !fx.items.contains_key(name) {
                    errors.push(format!(
                        "the example for `{claim_id}` names unknown fixture `{name}`"
                    ));
                }
            }
        }
    }
    let mut missing: Vec<String> = Vec::new();
    for m in &data.methods {
        for id in &m.claims {
            let Some(claim) = data.claim(id) else {
                continue;
            };
            if !claim.checked && !fx.examples.contains_key(id) && !missing.contains(id) {
                missing.push(id.clone());
                errors.push(format!(
                    "no example for the unconfirmed rule `{id}`, which `{}` cites (add one to fixtures/v2/questions.json)",
                    m.id
                ));
            }
        }
    }
    for (claim_id, evidence) in &fx.evidence {
        if data.claim(claim_id).is_none() {
            errors.push(format!(
                "questions.json evidence names unknown rule `{claim_id}`"
            ));
            continue;
        }
        if data.methods.iter().any(|m| m.claims.contains(claim_id)) {
            errors.push(format!(
                "`{claim_id}` is cited by a method, so it needs an example rather than evidence"
            ));
        }
        if evidence.shows.trim().is_empty() || evidence.users.trim().is_empty() {
            errors.push(format!(
                "the evidence for `{claim_id}` needs both `shows` and `users`"
            ));
        }
        for name in &evidence.fixtures {
            if !fx.items.contains_key(name) {
                errors.push(format!(
                    "the evidence for `{claim_id}` names unknown fixture `{name}`"
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CatalogError(errors.join("\n")))
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
        Require::Mirrored => "mirrored".into(),
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
        Require::BaseTag { tag } => format!("a base with the {tag} tag"),
        Require::NotBaseTag { tag } => format!("a base without the {tag} tag"),
        Require::CanRoll { pool } => match pool {
            Pool::Natural => "at least one random modifier it can roll".into(),
            Pool::Influence { influence } => {
                format!("at least one {} modifier it can roll", influence.name())
            }
            Pool::Influenced => "at least one influenced modifier it can roll".into(),
            Pool::Tag { tag } => format!("at least one {tag}-tagged modifier it can roll"),
            Pool::Ducat { tag } => format!("at least one ducat {tag} modifier it can roll"),
        },
        Require::MinSockets { n } => format!("at least {n} sockets"),
        Require::EssenceFits { essence } => format!("a class {essence} has a modifier for"),
        Require::Fossils { min, max } => format!("{min} to {max} fossils"),
        Require::FossilLimits => "fossils whose own limits allow this item".into(),
        Require::ImplicitsChangeable => "implicit modifiers that can be changed".into(),
    }
}

fn pool_phrase(p: &Pool) -> String {
    match p {
        Pool::Natural => "any".into(),
        Pool::Influence { influence } => format!("{} influence", influence.name()),
        Pool::Influenced => "one of the item's influences".into(),
        Pool::Tag { tag } => format!("{tag} tag"),
        Pool::Ducat { tag } => format!("ducat pool {tag}"),
    }
}

fn fill_phrase(f: &Fill) -> String {
    let mut out = "fills the rest of the reforge with random modifiers".to_string();
    if !f.counts.is_empty() {
        let counts: Vec<String> = f.counts.iter().map(|(n, _)| n.to_string()).collect();
        let weights: Vec<String> = f.counts.iter().map(|(_, w)| w.to_string()).collect();
        out.push_str(&format!(
            " ({} modifiers at {})",
            counts.join(" / "),
            weights.join(":")
        ));
    }
    if f.side == SideReq::Dominant {
        out.push_str(", on the dominant eldritch side only");
    }
    if let Some(n) = f.mod_level_max {
        out.push_str(&format!(", modifiers of level {n} and below"));
    }
    if let Some(include) = &f.include {
        out.push_str(&format!(
            ", repeating until at least one modifier comes from the {} pool",
            pool_phrase(include)
        ));
    }
    if f.one_veiled {
        out.push_str(
            ", the veiled modifier replacing one of the reforge's modifiers and taking that modifier's side",
        );
    }
    out.push_str(match f.weighting {
        Weighting::Normal => "",
        Weighting::Fossils => ", weighted by the fossils",
        Weighting::MoreLikelySame => ", much more likely to give the first item's modifiers",
        Weighting::LessLikelySame => ", much less likely to give the first item's modifiers",
        Weighting::Recombine => ", recombining the two inputs",
    });
    out
}

/// What a method's protection policy means, in one sentence.
pub fn protection_phrase(p: Protection) -> String {
    let locks = match p.locks {
        Policy::Respect => "keeps the side Prefixes/Suffixes Cannot Be Changed locks",
        Policy::Ignore => "changes a side Prefixes/Suffixes Cannot Be Changed locks",
        Policy::Forbid => "cannot be used with Prefixes/Suffixes Cannot Be Changed",
    };
    let cannot_roll = match p.cannot_roll {
        Policy::Respect => {
            "keeps the attack or caster modifiers Cannot Roll Attack/Caster Modifiers names"
        }
        Policy::Ignore => {
            "removes or changes attack and caster modifiers even with Cannot Roll Attack/Caster Modifiers"
        }
        Policy::Forbid => "cannot be used with Cannot Roll Attack/Caster Modifiers",
    };
    format!("{locks}; {cannot_roll}")
}

fn socket_phrase(c: &SocketChange) -> String {
    match c {
        SocketChange::RandomCount => "rerolls the socket count".into(),
        SocketChange::Count { n } => format!("sets {n} sockets"),
        SocketChange::RandomLinks => "rerolls the links".into(),
        SocketChange::Links { n } => format!("links {n} sockets"),
        SocketChange::RandomColours => "rerolls the socket colours".into(),
        SocketChange::Colours { spec } => format!("colours the sockets {spec}"),
        SocketChange::LinkUpToFour => "links up to four sockets".into(),
        SocketChange::TaintedCount => "adds or removes a socket (Tainted)".into(),
        SocketChange::TaintedLinks => "adds or removes a link (Tainted)".into(),
        SocketChange::TaintedColours => "rerolls the socket colours (Tainted)".into(),
        SocketChange::Dewhiten => "turns a random white socket non-white (3.29)".into(),
        SocketChange::Reforge => "rerolls the sockets, links and colours".into(),
    }
}

/// One sentence per op.
pub fn op_sentence(data: &Data, op: &Op) -> String {
    match op {
        Op::SetRarity { rarity } => format!("sets the rarity to {}", rarity.name()),
        Op::Clear { side } => match side {
            SideReq::Any => "removes every modifier that can change".into(),
            SideReq::Prefix => "removes every prefix that can change".into(),
            SideReq::Suffix => "removes every suffix that can change".into(),
            SideReq::Dominant => "removes every modifier on the dominant eldritch side that can change".into(),
        },
        Op::Scour => "removes every modifier that can change (nothing left: normal; only fractured left: magic for one, rare for more; a side a metamod kept leaves the rarity)".into(),
        Op::AddRandom { pool, side, ignore_limits } => format!(
            "adds one random modifier ({}, {}){}",
            pool_phrase(pool),
            side_phrase(*side),
            if *ignore_limits { ", ignoring the affix limits" } else { "" }
        ),
        Op::RemoveRandom { filter, keep_last_added, optional } => {
            let mut s = "removes one random modifier that can change".to_string();
            match filter {
                RemoveFilter::Any => {}
                RemoveFilter::Side { side } => s.push_str(&format!(" ({})", side_phrase(*side))),
                RemoveFilter::Influenced => s.push_str(" (an influenced one)"),
            }
            if *keep_last_added {
                s.push_str(", never the one just added");
            }
            if *optional {
                s.push_str("; with none that can change it removes nothing");
            }
            s
        }
        Op::AddVeiled => "adds a veiled modifier on a side with room (half each when both have room)".into(),
        Op::Unveil => "reveals the veiled modifier: up to three weighted options, never two of one group, and you pick one".into(),
        Op::AddMod { r#mod } => format!("adds {} (`{mod}`; a normal item turns magic)", mod_line(data, r#mod), mod = r#mod),
        Op::AddEssence { essence } => format!("adds {essence}'s modifier for the item's class"),
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
            ValueScope::Explicit => "rerolls the values of the modifiers that can change and are not crafted".into(),
            ValueScope::Implicit => "rerolls the values of the implicit modifiers".into(),
            ValueScope::Defences => "rerolls the base defences".into(),
        },
        Op::Fracture => "fractures one random modifier (not a veiled one)".into(),
        Op::AddInfluence { influence } => format!("adds {} influence", influence.name()),
        Op::AddInfluences { input } => format!("adds the influences of item {}", input + 1),
        Op::TakeInput { input } => format!("carries on with item {}", input + 1),
        Op::Corrupt => "corrupts the item".into(),
        Op::VaalImplicit => "replaces one random implicit with a random corrupted implicit (with none, adds one)".into(),
        Op::EldritchImplicit { eldritch, tier } => format!(
            "replaces the {} implicit with a random tier {tier} one",
            match eldritch {
                crate::data::Eldritch::Exarch => "Searing Exarch",
                crate::data::Eldritch::Eater => "Eater of Worlds",
            }
        ),
        Op::Conflict => "raises one eldritch implicit one tier and lowers the other one tier, the two assignments being equally likely, and a lowered Lesser (tier 6) implicit is removed instead".into(),
        Op::Dominance => "removes one influenced modifier that can change and upgrades another one tier".into(),
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
        Op::Fill { fill } => fill_phrase(fill),
        Op::Sockets { change } => socket_phrase(change),
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
    for (_, leaf) in &o.leaves {
        match leaf {
            Leaf::Pending(p) => {
                let t = p.reason.text();
                if !out.contains(&t) {
                    out.push(t);
                }
            }
            Leaf::Choice(options) => options.iter().for_each(|opt| collect_reasons(opt, out)),
            _ => {}
        }
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

const ESSENCE_CLAIMS: [&str; 5] = [
    "game-text",
    "essence.ilvl",
    "reforge.guarantee-first",
    "reforge.counts",
    "metamod.multicraft-does-not-block",
];

/// A generated essence method: the rarities it takes and the level its reforge's modifiers are
/// capped at.
fn standard_essence(m: &Method) -> Option<(&[crate::data::Rarity], Option<u8>)> {
    let [
        Op::SetRarity { .. },
        Op::Clear { side: SideReq::Any },
        Op::AddEssence { .. },
        Op::Fill { fill },
    ] = m.ops.as_slice()
    else {
        return None;
    };
    let [
        Require::NotCorrupted,
        Require::NotMirrored,
        Require::Rarity { any_of },
        Require::EssenceFits { .. },
    ] = m.requires.as_slice()
    else {
        return None;
    };
    let standard = m.family == Family::Essence
        && m.protect
            == Some(Protection {
                locks: Policy::Forbid,
                cannot_roll: Policy::Forbid,
            })
        && fill.weighting == Weighting::Normal
        && m.claims == ESSENCE_CLAIMS
        && m.example.is_none();
    standard.then_some((any_of.as_slice(), fill.mod_level_max))
}

fn essence_table(data: &Data, methods: &[&Method]) -> String {
    let first = methods[0];
    let mut texts: Vec<&str> = methods
        .iter()
        .filter_map(|m| m.game_text.as_deref())
        .collect();
    texts.sort_unstable();
    texts.dedup();
    // The ops in the essence's own words, without the first essence's name or level cap.
    let does: Vec<String> = first
        .ops
        .iter()
        .map(|o| match o {
            Op::AddEssence { .. } => "adds the essence's modifier for the item's class".to_string(),
            Op::Fill { fill } => fill_phrase(&Fill {
                mod_level_max: None,
                ..fill.clone()
            }),
            o => op_sentence(data, o),
        })
        .collect();
    let mut out = vec![
        format!(
            "Each essence in this table costs one of itself and needs: the rarity listed; a class the essence has a modifier for; not corrupted; not mirrored. It {}; where a level is listed, the random modifiers are of that level and below. Metamods: it {}. Game text: {}. Sources: {}.",
            does.join("; then "),
            first.protect.map(protection_phrase).unwrap_or_default(),
            texts
                .iter()
                .map(|t| format!("\"{}\"", t.replace('\n', " / ")))
                .collect::<Vec<_>>()
                .join(" or "),
            sources(data, first)
        ),
        String::new(),
        "|Method|Rarity|Modifier level at most|".to_string(),
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

const BENCH_ADD_CLAIMS: [&str; 3] = [
    "bench.normal-to-magic",
    "metamod.slot",
    "metamod.crafted-room",
];

/// A generated bench craft that adds one modifier with the standard requirements: its mod id,
/// side and classes. Those print as one table row instead of a full entry.
fn standard_bench_add(m: &Method) -> Option<(&str, SideReq, &[String])> {
    let [Op::AddMod { r#mod }] = m.ops.as_slice() else {
        return None;
    };
    let [
        Require::NotCorrupted,
        Require::NotMirrored,
        Require::Rarity { any_of },
        Require::Class { any_of: classes },
        Require::OpenAffix { side },
        Require::CraftedRoom,
        Require::GroupFree { r#mod: group_mod },
    ] = m.requires.as_slice()
    else {
        return None;
    };
    let standard = m.family == Family::Bench
        && any_of
            == &[
                crate::data::Rarity::Normal,
                crate::data::Rarity::Magic,
                crate::data::Rarity::Rare,
            ]
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
            "Each bench craft in this table needs: not corrupted; not mirrored; {}; one of the classes listed; an open affix on the modifier's side; {}; no modifier of the added one's group yet. It adds that modifier, and a normal item turns magic. Sources: {}.",
            require_phrase(data, &first.requires[2]),
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
    if m.family == Family::Currency
        && let Some(directions) = m
            .cost
            .first()
            .and_then(|c| data.currency_by_name(&c.name))
            .map(|c| c.directions.trim())
            .filter(|d| !d.is_empty())
    {
        lines.push(format!("- Directions: {directions}"));
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
    if let Some(p) = m.protect {
        lines.push(format!("- Metamods: it {}.", protection_phrase(p)));
    }
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

/// The methods that cite a rule, by name and id, to at most eight.
fn method_list(users: &[&Method]) -> String {
    let shown: Vec<String> = users
        .iter()
        .take(8)
        .map(|m| format!("{} (`{}`)", m.name, m.id))
        .collect();
    let more = users.len().saturating_sub(8);
    format!(
        "{} method{}: {}{}",
        users.len(),
        if users.len() == 1 { "" } else { "s" },
        shown.join(", "),
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        }
    )
}

/// The probability mass of the outcomes that add a prefix and of those that add a suffix,
/// counted against the first input. A choice counts each of its options equally, as in
/// `most_likely_item`.
fn change_mass(input: &Item, o: &Outcome, scale: f64, prefix: &mut f64, suffix: &mut f64) {
    for (p, leaf) in &o.leaves {
        let p = p * scale;
        let item = match leaf {
            Leaf::Item(item) => Some(item),
            Leaf::Items(items) => items.first(),
            Leaf::Choice(options) => {
                let n = options.len().max(1) as f64;
                for option in options {
                    change_mass(input, option, p / n, prefix, suffix);
                }
                None
            }
            Leaf::Pending(_) | Leaf::Destroyed => None,
        };
        if let Some(item) = item {
            if item.prefixes.len() > input.prefixes.len() {
                *prefix += p;
            }
            if item.suffixes.len() > input.suffixes.len() {
                *suffix += p;
            }
        }
    }
}

/// The probability mass of the outcomes by their number of explicit modifiers, counted against
/// the first input. A choice counts each of its options equally, as in `most_likely_item`.
fn count_mass(o: &Outcome, scale: f64, counts: &mut BTreeMap<usize, f64>) {
    for (p, leaf) in &o.leaves {
        let p = p * scale;
        match leaf {
            Leaf::Item(item) => {
                *counts.entry(item.explicits().count()).or_default() += p;
            }
            Leaf::Items(items) => {
                if let Some(item) = items.first() {
                    *counts.entry(item.explicits().count()).or_default() += p;
                }
            }
            Leaf::Choice(options) => {
                let n = options.len().max(1) as f64;
                for option in options {
                    count_mass(option, p / n, counts);
                }
            }
            Leaf::Pending(_) | Leaf::Destroyed => {}
        }
    }
}

/// One question example: what it shows, every input item, the step with its cost, the result and
/// the diff between the first input and the most likely result.
fn question_example(
    data: &Data,
    fx: &Fixtures,
    index: usize,
    count: usize,
    example: &ClaimExample,
) -> Vec<String> {
    let method = data
        .method(&example.method)
        .expect("validated examples name a method");
    let mut out = Vec::new();
    if count > 1 {
        out.push(format!("**Example {index}**"));
        out.push(String::new());
    }
    out.push(format!("**What this shows:** {}", example.shows));
    out.push(String::new());
    for (i, name) in example.fixtures.iter().enumerate() {
        let (_, item) = fx
            .items
            .get(name)
            .expect("validated examples name a fixture");
        let label = if example.fixtures.len() == 1 {
            "Input".to_string()
        } else {
            format!("Input {}", i + 1)
        };
        out.push(format!("**{label}** (`{name}`)"));
        out.push(String::new());
        out.push(item_block(data, item));
        out.push(String::new());
    }
    out.push(format!(
        "**Step**: {} (`{}`; cost: {})",
        method.name,
        method.id,
        cost_text(method)
    ));
    out.push(String::new());
    out.push("**Result**".into());
    out.push(String::new());
    let items: Vec<Item> = example
        .fixtures
        .iter()
        .map(|name| fx.items[name].1.clone())
        .collect();
    match apply(data, method, &items, &Params::default()) {
        Ok(o) => {
            out.push(outcome(data, &o));
            let (mut prefix, mut suffix) = (0.0, 0.0);
            change_mass(&items[0], &o, 1.0, &mut prefix, &mut suffix);
            if prefix > 0.0 || suffix > 0.0 {
                out.push(String::new());
                out.push(format!(
                    "**Where the result changes:** {prefix:.6} of the outcomes add a prefix and {suffix:.6} add a suffix; a result that adds both is counted in both numbers."
                ));
            }
            let mut counts: BTreeMap<usize, f64> = BTreeMap::new();
            count_mass(&o, 1.0, &mut counts);
            counts.retain(|_, p| *p > 0.0);
            let unchanged = counts.len() == 1 && counts.contains_key(&items[0].explicits().count());
            if !counts.is_empty() && !unchanged {
                let parts: Vec<String> = counts
                    .iter()
                    .map(|(n, p)| {
                        format!(
                            "{p:.6} end with {n} explicit modifier{}",
                            if *n == 1 { "" } else { "s" }
                        )
                    })
                    .collect();
                out.push(String::new());
                out.push(format!("**Modifier counts:** {}.", parts.join(", ")));
            }
            out.push(String::new());
            match most_likely_item(&o) {
                Some(best) => {
                    let label = if items.len() == 1 {
                        "**Diff** (input vs the most likely result)"
                    } else {
                        "**Diff** (input 1 vs the most likely result)"
                    };
                    out.push(label.into());
                    out.push(String::new());
                    out.push(item_diff(data, &items[0], best));
                }
                None => out.push(
                    "**Diff**: none; this pass does not compute a result item for this step."
                        .into(),
                ),
            }
        }
        Err(reason) => out.push(format!("**Refused**: {reason}")),
    }
    out
}

fn questions_md(data: &Data, fx: &Fixtures) -> Result<String, CatalogError> {
    validate_examples(data, fx)?;
    let mut out = vec![
        "# Questions for the owner".to_string(),
        String::new(),
        "Generated by `datagen-v2 catalog`; do not edit. Each section is one rule that a method relies on and that no published source confirmed. The section shows a full item input, the step, the result and the difference between the input and the most likely result, so the rule can be checked without opening any other file. Confirm or correct each proposed rule.".into(),
    ];
    for claim in data.claims.iter().filter(|c| !c.checked) {
        let users: Vec<&Method> = data
            .methods
            .iter()
            .filter(|m| m.claims.contains(&claim.id))
            .collect();
        out.push(String::new());
        out.push(format!("## `{}`", claim.id));
        out.push(String::new());
        out.push(format!("**Proposed rule:** {}", claim.statement));
        if !claim.url.is_empty() {
            out.push(String::new());
            out.push(format!(
                "Current source ({}): {}",
                claim.source_kind.name(),
                claim.url
            ));
        }
        if users.is_empty() {
            out.push(String::new());
            let evidence = fx.evidence.get(&claim.id);
            out.push(format!(
                "**Used by:** {}",
                evidence.map_or(
                    "no method or data file in this bundle records a user of this rule",
                    |e| e.users.as_str()
                )
            ));
            match evidence {
                None => {
                    out.push(String::new());
                    out.push("No item fixture exercises this rule yet.".into());
                }
                Some(evidence) => {
                    out.push(String::new());
                    out.push(format!("**What this shows:** {}", evidence.shows));
                    for (i, name) in evidence.fixtures.iter().enumerate() {
                        let (_, item) = fx
                            .items
                            .get(name)
                            .expect("validated evidence names a fixture");
                        let label = if evidence.fixtures.len() == 1 {
                            "Input".to_string()
                        } else {
                            format!("Input {}", i + 1)
                        };
                        out.push(String::new());
                        out.push(format!("**{label}** (`{name}`)"));
                        out.push(String::new());
                        out.push(item_block(data, item));
                        out.push(String::new());
                        let limits = &item.class(data).limits;
                        out.push(format!(
                            "**Limits**: the {} class allows at most {} prefix{} and {} suffix{} at magic rarity, and {} and {} at rare rarity.",
                            item.class(data).name,
                            limits.magic[0],
                            if limits.magic[0] == 1 { "" } else { "es" },
                            limits.magic[1],
                            if limits.magic[1] == 1 { "" } else { "es" },
                            limits.rare[0],
                            limits.rare[1]
                        ));
                    }
                }
            }
            continue;
        }
        let examples = fx
            .examples
            .get(&claim.id)
            .expect("validated methods' unconfirmed rules have examples");
        let count = examples.len();
        for (i, example) in examples.iter().enumerate() {
            out.push(String::new());
            out.extend(question_example(data, fx, i + 1, count, example));
        }
        out.push(String::new());
        out.push(format!("**Used by** {}", method_list(&users)));
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
    Ok(out.join("\n"))
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
                let classes = |list: &[crate::pattern::AffixState]| {
                    let ids: Vec<&str> = list
                        .iter()
                        .map(|a| p.classes[a.class as usize].id.as_str())
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
pub fn render_all(data: &Data, fixtures: &Path) -> Result<BTreeMap<String, String>, CatalogError> {
    let fx = Fixtures::load(data, fixtures);
    let mut out = BTreeMap::new();
    out.insert("methods.md".to_string(), methods_md(data, &fx));
    out.insert("bench.md".to_string(), bench_md(data));
    out.insert("questions.md".to_string(), questions_md(data, &fx)?);
    out.insert("items.md".to_string(), items_md(data, &fx));
    out.insert("scope.md".to_string(), scope_md(data));
    Ok(out)
}
