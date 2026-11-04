//! The method catalog: reads the hand-authored methods in `rules/v2/methods.json`, generates the
//! bench, essence, catalyst and fossil families from the bundle, fills each currency method's game
//! text, validates every reference (claims, mods, classes, tags, mod types, costs, Chance
//! masses), and applies `rules/v2/currency-scope.json` to every currency.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, bail};
use poe1_v2::data::{BenchAction, Bundle, Rarity, Scope};
use poe1_v2::method::{
    CostSpec, Family, Guarantee, Method, Op, PendingReason, Pool, ReforgeSpec, Require, SideReq,
    SocketChange, Weighting,
};
use serde::Deserialize;

use crate::bundle::load_json;

/// The Harvest tags a reforge or add-remove craft can name.
pub const HARVEST_TAGS: [&str; 11] = [
    "fire",
    "cold",
    "lightning",
    "physical",
    "defences",
    "life",
    "chaos",
    "attack",
    "caster",
    "speed",
    "critical",
];

/// The exclusion reasons a scope rule may give.
pub const EXCLUSION_REASONS: [&str; 7] = [
    "placeholder",
    "shard",
    "maps-atlas",
    "gems",
    "other-item-class",
    "cost-only",
    "not-used-on-items",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MethodsFile {
    methods: Vec<Method>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeFile {
    rules: Vec<ScopeRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeRule {
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    methods: Option<Vec<String>>,
    #[serde(default)]
    family: Option<Family>,
    #[serde(default)]
    excluded: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    note: Option<String>,
}

/// `price_key` for a currency name: lowercase, spaces to '-', apostrophes dropped.
pub fn price_key(name: &str) -> String {
    name.to_lowercase()
        .replace(['\'', '\u{2019}'], "")
        .replace(' ', "-")
}

fn common(r: &mut Vec<Require>) {
    r.push(Require::NotCorrupted);
    r.push(Require::NotMirrored);
}

fn cost(name: &str, amount: f64) -> CostSpec {
    CostSpec {
        name: name.to_string(),
        amount,
    }
}

/// The methods `datagen-v2` derives from bundle data.
fn generated(b: &Bundle) -> anyhow::Result<Vec<Method>> {
    let currency_name = |id: &str| -> anyhow::Result<String> {
        b.currencies
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .with_context(|| format!("cost currency {id} is not a bundle currency"))
    };
    let mut out = Vec::new();

    // ---- bench
    for o in &b.bench {
        let classes = Require::Class {
            any_of: o
                .classes
                .iter()
                .map(|&c| b.classes[c as usize].id.clone())
                .collect(),
        };
        let mut requires = Vec::new();
        let mut claims = Vec::new();
        let (name, ops) = match &o.action {
            BenchAction::AddMod { m } => {
                let md = &b.mods[*m as usize];
                let side = match md.side {
                    Some(poe1_v2::data::Side::Prefix) => SideReq::Prefix,
                    Some(poe1_v2::data::Side::Suffix) => SideReq::Suffix,
                    None => bail!("bench mod {} has no side", md.id),
                };
                requires.push(Require::Rarity {
                    any_of: vec![Rarity::Magic, Rarity::Rare],
                });
                common(&mut requires);
                requires.push(classes);
                requires.push(Require::OpenAffix { side });
                requires.push(Require::CraftedRoom);
                requires.push(Require::GroupFree {
                    r#mod: md.id.clone(),
                });
                claims = vec!["bench.magic-or-rare".into(), "metamod.slot".into()];
                (
                    format!("Bench: {}", md.text.lines().next().unwrap_or(&md.id)),
                    vec![Op::AddMod {
                        r#mod: md.id.clone(),
                    }],
                )
            }
            BenchAction::AddEnchant { m } => {
                let md = &b.mods[*m as usize];
                requires.push(classes);
                requires.push(Require::NotCorrupted);
                (
                    format!(
                        "Bench enchant: {}",
                        md.text.lines().next().unwrap_or(&md.id)
                    ),
                    vec![Op::SetEnchantMod {
                        r#mod: md.id.clone(),
                    }],
                )
            }
            BenchAction::RemoveCrafted => {
                requires.push(classes);
                requires.push(Require::HasCrafted);
                requires.push(Require::NotCorrupted);
                (
                    "Bench: remove crafted modifiers".into(),
                    vec![Op::RemoveCrafted],
                )
            }
            BenchAction::RemoveEnchants => {
                requires.push(classes);
                requires.push(Require::HasEnchant);
                ("Bench: remove enchantments".into(), vec![Op::ClearEnchants])
            }
            BenchAction::Sockets { count } => {
                requires.push(classes);
                requires.push(Require::CanHaveSockets);
                requires.push(Require::NotCorrupted);
                (
                    format!("Bench: {count} sockets"),
                    vec![Op::Sockets {
                        change: SocketChange::Count,
                    }],
                )
            }
            BenchAction::Links { count } => {
                requires.push(classes);
                requires.push(Require::HasSockets);
                requires.push(Require::NotCorrupted);
                (
                    format!("Bench: link {count} sockets"),
                    vec![Op::Sockets {
                        change: SocketChange::Links,
                    }],
                )
            }
            BenchAction::Colours { spec } => {
                requires.push(classes);
                requires.push(Require::HasSockets);
                requires.push(Require::NotCorrupted);
                (
                    format!("Bench: colour sockets {spec}"),
                    vec![Op::Sockets {
                        change: SocketChange::Colours,
                    }],
                )
            }
            BenchAction::RerollRarity | BenchAction::RerollRareMods { .. } => {
                requires.push(classes);
                requires.push(Require::NotCorrupted);
                claims = vec!["bench.reroll-unknown".into()];
                let name = match &o.action {
                    BenchAction::RerollRareMods { n } => {
                        format!("Bench: reroll {n} rare modifiers")
                    }
                    _ => "Bench: reroll rarity".into(),
                };
                (
                    name,
                    vec![Op::Unmodelled {
                        reason: PendingReason::Unverified,
                    }],
                )
            }
        };
        out.push(Method {
            id: format!("bench/{}", o.index),
            name,
            family: Family::Bench,
            inputs: 1,
            cost: o
                .cost
                .iter()
                .map(|c| Ok(cost(&currency_name(&c.key)?, c.amount)))
                .collect::<anyhow::Result<_>>()?,
            requires,
            pair: Vec::new(),
            ops,
            claims,
            game_text: None,
            example: None,
        });
    }

    // ---- essences (those with a modifier for some in-scope class)
    for e in b.essences.iter().filter(|e| !e.mods.is_empty()) {
        let mut requires = vec![
            Require::Rarity {
                any_of: if e.allows_rare {
                    vec![Rarity::Normal, Rarity::Rare]
                } else {
                    vec![Rarity::Normal]
                },
            },
            Require::EssenceFits {
                essence: e.name.clone(),
            },
        ];
        common(&mut requires);
        if let Some(n) = e.item_level_max {
            requires.push(Require::ItemLevelAtMost { n });
        }
        out.push(Method {
            id: format!("essence/{}", price_key(&e.name)),
            name: e.name.clone(),
            family: Family::Essence,
            inputs: 1,
            cost: vec![cost(&e.name, 1.0)],
            requires,
            pair: Vec::new(),
            ops: vec![Op::Reforge {
                spec: ReforgeSpec {
                    rarity: Rarity::Rare,
                    side: None,
                    respects_locks: true,
                    guarantee: Some(Guarantee::Essence),
                    weighting: Weighting::Normal,
                },
            }],
            claims: vec![
                "game-text".into(),
                "essence.ilvl".into(),
                "reforge.guarantee-first".into(),
            ],
            game_text: None,
            example: None,
        });
    }

    // ---- catalysts
    for c in &b.catalysts {
        let mut requires = vec![Require::Class {
            any_of: vec!["Ring".into(), "Amulet".into(), "Belt".into()],
        }];
        common(&mut requires);
        requires.push(Require::CatalystRoom {
            catalyst: c.name.clone(),
        });
        out.push(Method {
            id: format!("catalyst/{}", price_key(&c.name)),
            name: c.name.clone(),
            family: Family::Catalyst,
            inputs: 1,
            cost: vec![cost(&c.name, 1.0)],
            requires,
            pair: Vec::new(),
            ops: vec![Op::CatalystQuality {
                catalyst: c.name.clone(),
            }],
            claims: vec![
                "game-text".into(),
                "catalyst.amounts".into(),
                "catalyst.switch-resets".into(),
            ],
            game_text: None,
            example: None,
        });
    }

    // ---- fossils: one method per resonator kind; the resonator size follows the fossil count
    for (id, name, rarity) in [
        (
            "fossil/chaotic-resonator",
            "Fossils in a Chaotic Resonator",
            Rarity::Rare,
        ),
        (
            "fossil/alchemical-resonator",
            "Fossils in an Alchemical Resonator",
            Rarity::Normal,
        ),
    ] {
        let mut requires = vec![
            Require::Rarity {
                any_of: vec![rarity],
            },
            Require::Fossils { min: 1, max: 4 },
        ];
        common(&mut requires);
        out.push(Method {
            id: id.into(),
            name: name.into(),
            family: Family::Fossil,
            inputs: 1,
            cost: Vec::new(),
            requires,
            pair: Vec::new(),
            ops: vec![Op::Reforge {
                spec: ReforgeSpec {
                    rarity: Rarity::Rare,
                    side: None,
                    respects_locks: false,
                    guarantee: None,
                    weighting: Weighting::Fossils,
                },
            }],
            claims: vec!["game-text".into(), "chaos.count".into()],
            game_text: None,
            example: None,
        });
    }
    Ok(out)
}

/// Every reference a method makes must resolve against the bundle.
fn validate(b: &Bundle, m: &Method) -> anyhow::Result<()> {
    let ctx = || format!("method {}", m.id);
    let claim_ids: BTreeSet<&str> = b.claims.iter().map(|c| c.id.as_str()).collect();
    for c in &m.claims {
        if !claim_ids.contains(c.as_str()) {
            bail!("{}: unknown claim {c}", ctx());
        }
    }
    for c in &m.cost {
        if !c.name.starts_with("beast:") && !b.currencies.iter().any(|x| x.name == c.name) {
            bail!("{}: cost names unknown currency {:?}", ctx(), c.name);
        }
    }
    let has_mod = |id: &str| b.mods.iter().any(|x| x.id == id);
    let has_tag = |t: &str| b.tags.iter().any(|x| x == t);
    let has_type = |t: &str| b.mod_types.iter().any(|x| x == t);
    for r in &m.requires {
        match r {
            Require::Class { any_of } => {
                for c in any_of {
                    if !b.classes.iter().any(|x| &x.id == c) {
                        bail!("{}: unknown class {c}", ctx());
                    }
                }
            }
            Require::GroupFree { r#mod } if !has_mod(r#mod) => {
                bail!("{}: unknown mod {mod}", ctx(), mod = r#mod)
            }
            Require::HasModType { any_of } => {
                for t in any_of {
                    if !has_type(t) {
                        bail!("{}: unknown mod type {t}", ctx());
                    }
                }
            }
            Require::CatalystRoom { catalyst }
                if !b.catalysts.iter().any(|c| &c.name == catalyst) =>
            {
                bail!("{}: unknown catalyst {catalyst}", ctx())
            }
            Require::EssenceFits { essence } if !b.essences.iter().any(|e| &e.name == essence) => {
                bail!("{}: unknown essence {essence}", ctx())
            }
            _ => {}
        }
    }
    fn walk(ops: &[Op], f: &mut dyn FnMut(&Op) -> anyhow::Result<()>) -> anyhow::Result<()> {
        for op in ops {
            f(op)?;
            if let Op::Chance { branches } = op {
                for br in branches {
                    walk(&br.ops, f)?;
                }
            }
        }
        Ok(())
    }
    walk(&m.ops, &mut |op| {
        match op {
            Op::AddMod { r#mod } | Op::SetEnchantMod { r#mod } if !has_mod(r#mod) => {
                bail!("{}: unknown mod {mod}", ctx(), mod = r#mod)
            }
            Op::AddRandom { pool, .. } => match pool {
                Pool::Tag { tag } if !HARVEST_TAGS.contains(&tag.as_str()) || !has_tag(tag) => {
                    bail!("{}: unknown Harvest tag {tag}", ctx())
                }
                Pool::Ducat { tag } if !has_tag(tag) => bail!("{}: unknown tag {tag}", ctx()),
                _ => {}
            },
            Op::Reforge { spec } | Op::Fill { spec } => {
                if let Some(Guarantee::Tag { tag }) = &spec.guarantee
                    && (!HARVEST_TAGS.contains(&tag.as_str()) || !has_tag(tag))
                {
                    bail!("{}: unknown Harvest tag {tag}", ctx())
                }
            }
            Op::ChangeResistance { from, to } => {
                for t in [from.resistance_type(), to.resistance_type()] {
                    if !has_type(t) {
                        bail!("{}: unknown mod type {t}", ctx());
                    }
                }
            }
            Op::CatalystQuality { catalyst }
                if !b.catalysts.iter().any(|c| &c.name == catalyst) =>
            {
                bail!("{}: unknown catalyst {catalyst}", ctx())
            }
            Op::Chance { branches } => {
                let total: f64 = branches.iter().map(|b| b.p).sum();
                if (total - 1.0).abs() > 1e-9 {
                    bail!("{}: Chance branches sum to {total}, not 1", ctx());
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    if m.inputs == 0 || (m.inputs == 1 && !m.pair.is_empty()) {
        bail!(
            "{}: {} inputs with pair rules {:?}",
            ctx(),
            m.inputs,
            m.pair
        );
    }
    Ok(())
}

/// Applies the scope rules to every currency: the first matching rule wins.
fn scope(b: &Bundle, rules: &ScopeFile) -> anyhow::Result<Vec<(String, Scope)>> {
    let mut compiled = Vec::with_capacity(rules.rules.len());
    for (i, r) in rules.rules.iter().enumerate() {
        let kinds = [
            r.methods.is_some(),
            r.family.is_some(),
            r.excluded.is_some(),
        ]
        .iter()
        .filter(|x| **x)
        .count();
        if kinds != 1 {
            bail!("currency-scope rule {i} needs exactly one of methods, family, excluded");
        }
        if r.names.is_empty() && r.pattern.is_none() {
            bail!("currency-scope rule {i} needs names or a pattern");
        }
        if let Some(reason) = &r.excluded
            && !EXCLUSION_REASONS.contains(&reason.as_str())
        {
            bail!("currency-scope rule {i}: unknown exclusion reason {reason}");
        }
        if let Some(f) = r.family
            && !matches!(f, Family::Essence | Family::Catalyst)
        {
            bail!("currency-scope rule {i}: family must be essence or catalyst");
        }
        for id in r.methods.iter().flatten() {
            if !b.methods.iter().any(|m| &m.id == id) {
                bail!("currency-scope rule {i} names unknown method {id}");
            }
        }
        let pattern = r
            .pattern
            .as_deref()
            .map(regex::Regex::new)
            .transpose()
            .with_context(|| format!("currency-scope rule {i} pattern"))?;
        compiled.push((r, pattern));
    }
    let mut out = Vec::with_capacity(b.currencies.len());
    let mut unmatched = Vec::new();
    for c in &b.currencies {
        let Some((rule, _)) = compiled.iter().find(|(r, p)| {
            r.names.contains(&c.name) || p.as_ref().is_some_and(|p| p.is_match(&c.name))
        }) else {
            unmatched.push(format!("currency \"{}\" matches no scope rule", c.name));
            continue;
        };
        let s = if let Some(ids) = &rule.methods {
            Scope::Methods { ids: ids.clone() }
        } else if let Some(f) = rule.family {
            let prefix = match f {
                Family::Essence => "essence",
                _ => "catalyst",
            };
            let id = format!("{prefix}/{}", price_key(&c.name));
            if !b.methods.iter().any(|m| m.id == id) {
                bail!(
                    "currency \"{}\" is in the {prefix} family but has no method {id}",
                    c.name
                );
            }
            Scope::Methods { ids: vec![id] }
        } else {
            Scope::Excluded {
                reason: rule.excluded.clone().unwrap_or_default(),
            }
        };
        out.push((c.id.clone(), s));
    }
    if !unmatched.is_empty() {
        bail!("{}", unmatched.join("\n"));
    }
    Ok(out)
}

/// Fills `bundle.methods` and `bundle.scope`.
pub fn attach(b: &mut Bundle, rules: &Path) -> anyhow::Result<()> {
    let file: MethodsFile = load_json(rules, "methods.json")?;
    let mut methods = file.methods;
    methods.extend(generated(b)?);
    let mut seen = BTreeMap::new();
    for m in &methods {
        if seen.insert(m.id.clone(), ()).is_some() {
            bail!("method id {} is used twice", m.id);
        }
    }
    for m in &mut methods {
        if m.game_text.is_none()
            && matches!(
                m.family,
                Family::Currency | Family::Essence | Family::Catalyst
            )
            && let Some(first) = m.cost.first()
            && let Some(c) = b.currencies.iter().find(|c| c.name == first.name)
            && !c.description.is_empty()
        {
            m.game_text = Some(c.description.clone());
        }
        validate(b, m)?;
    }
    methods.sort_by(|a, b| a.id.cmp(&b.id));
    b.methods = methods;
    let rules_file: ScopeFile = load_json(rules, "currency-scope.json")?;
    b.scope = scope(b, &rules_file)?;
    Ok(())
}
