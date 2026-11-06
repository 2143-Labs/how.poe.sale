//! The method catalog: reads the hand-authored methods in `rules/v2/methods.json`, generates the
//! bench, essence, catalyst and fossil families from the bundle, fills each currency method's game
//! text, validates every method with the library's own `Method::validate` (by loading the bundle
//! as `Data`), checks what only the authoring side knows (every Fill count table is claim
//! `chaos.count`'s, every tag pool names a Harvest tag, and a currency method never requires less
//! than its currency's in-game `directions` line states), and applies
//! `rules/v2/currency-scope.json` to every currency.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, bail};
use poe1_v2::data::{BenchAction, Bundle, Category, Rarity, Scope};
use poe1_v2::method::{
    CostSpec, Family, Fill, Method, Op, PendingReason, Policy, Pool, Protection, Require, SideReq,
    SocketChange, Weighting,
};
use serde::Deserialize;

use crate::bundle::load_json;

/// The Harvest tags a reforge or add-remove craft can name: the eleven the 3.24 list has plus the
/// five reforges that also cost Crystallised Rancour (claim `harvest.list`).
pub const HARVEST_TAGS: [&str; 16] = [
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
    "minion",
    "elemental",
    "attribute",
    "mana",
    "drop",
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

fn cost(name: &str, amount: f64) -> CostSpec {
    CostSpec {
        name: name.to_string(),
        amount,
    }
}

/// Claim `chaos.count`'s table: explicit totals of a reforge and their relative weights, by
/// total.
fn chaos_counts(b: &Bundle) -> anyhow::Result<Vec<(u8, u32)>> {
    let claim = b
        .claims
        .iter()
        .find(|c| c.id == "chaos.count")
        .context("claim chaos.count is missing")?;
    let value = claim
        .value
        .clone()
        .context("claim chaos.count has no count table")?;
    let table: BTreeMap<u8, u32> =
        serde_json::from_value(value).context("claim chaos.count's value is not a count table")?;
    Ok(table.into_iter().collect())
}

/// The stackable resonator for each fossil count (the legacy socketable ones share the names).
fn resonators(b: &Bundle, kind: &str) -> anyhow::Result<Vec<String>> {
    ["Primitive", "Potent", "Powerful", "Prime"]
        .iter()
        .map(|size| {
            let name = format!("{size} {kind} Resonator");
            let found: Vec<&str> = b
                .currencies
                .iter()
                .filter(|c| c.name == name && c.class == "DelveStackableSocketableCurrency")
                .map(|c| c.id.as_str())
                .collect();
            match found.as_slice() {
                [id] => Ok(id.to_string()),
                _ => bail!("{name}: {} stackable currencies", found.len()),
            }
        })
        .collect()
}

/// The protection of a method metamods forbid: Craft of Exile's metamod table marks essences
/// and fossils "Prevents usage" under all four metamods, and the owner settled that the
/// multi-craft metamod alone does not (Q9).
const FORBIDDEN: Protection = Protection {
    locks: Policy::Forbid,
    cannot_roll: Policy::Forbid,
};

/// The methods `datagen-v2` derives from bundle data.
fn generated(b: &Bundle) -> anyhow::Result<Vec<Method>> {
    let currency_name = |id: &str| -> anyhow::Result<String> {
        b.currencies
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .with_context(|| format!("cost currency {id} is not a bundle currency"))
    };
    let counts = chaos_counts(b)?;
    let mut out = Vec::new();

    // ---- bench. Ids name the action, not the file position, so a RePoE refresh keeps them.
    for o in &b.bench {
        let class_ids = |max_sockets: Option<u8>| -> Vec<String> {
            o.classes
                .iter()
                .map(|&c| &b.classes[c as usize])
                .filter(|c| max_sockets.is_none_or(|n| c.max_sockets >= n))
                .map(|c| c.id.clone())
                .collect()
        };
        let classes = |max_sockets: Option<u8>| Require::Class {
            any_of: class_ids(max_sockets),
        };
        let (id, name, ops, claims) = match &o.action {
            BenchAction::AddMod { m } => {
                let md = &b.mods[*m as usize];
                (
                    format!("bench/add/{}", md.id),
                    format!("Bench: {}", md.text.lines().next().unwrap_or(&md.id)),
                    vec![Op::AddMod {
                        r#mod: md.id.clone(),
                    }],
                    vec![
                        "bench.normal-to-magic".to_string(),
                        "metamod.slot".into(),
                        "metamod.crafted-room".into(),
                    ],
                )
            }
            BenchAction::AddEnchant { m } => {
                let md = &b.mods[*m as usize];
                (
                    format!("bench/enchant/{}", md.id),
                    format!(
                        "Bench enchant: {}",
                        md.text.lines().next().unwrap_or(&md.id)
                    ),
                    vec![Op::SetEnchantMod {
                        r#mod: md.id.clone(),
                    }],
                    Vec::new(),
                )
            }
            BenchAction::RemoveCrafted => (
                "bench/remove-crafted".into(),
                "Bench: remove crafted modifiers".into(),
                vec![Op::RemoveCrafted],
                Vec::new(),
            ),
            BenchAction::RemoveEnchants => (
                "bench/remove-enchants".into(),
                "Bench: remove enchantments".into(),
                vec![Op::ClearEnchants],
                Vec::new(),
            ),
            BenchAction::Sockets { count } => (
                format!("bench/sockets/{count}"),
                format!("Bench: {count} sockets"),
                vec![Op::Sockets {
                    change: SocketChange::Count { n: *count },
                }],
                vec!["bench.corrupted-sockets".into()],
            ),
            BenchAction::Links { count } => (
                format!("bench/links/{count}"),
                format!("Bench: link {count} sockets"),
                vec![Op::Sockets {
                    change: SocketChange::Links { n: *count },
                }],
                vec!["bench.corrupted-sockets".into()],
            ),
            BenchAction::Colours { spec } => (
                format!("bench/colours/{spec}"),
                format!("Bench: colour sockets {spec}"),
                vec![Op::Sockets {
                    change: SocketChange::Colours { spec: spec.clone() },
                }],
                vec!["bench.corrupted-sockets".into()],
            ),
            BenchAction::RerollRarity => (
                "bench/reroll-rarity".into(),
                "Bench: reroll rarity".into(),
                vec![Op::Unmodelled {
                    reason: PendingReason::Unverified,
                }],
                vec!["bench.reroll-unknown".into()],
            ),
            BenchAction::RerollRareMods { n } => (
                format!("bench/reroll-rare-mods/{n}"),
                format!("Bench: reroll {n} rare modifiers"),
                vec![Op::Unmodelled {
                    reason: PendingReason::NeedsData {
                        what: "what the bench reroll picks is not stated".into(),
                    },
                }],
                vec!["bench.reroll-mods".into(), "bench.reroll-unknown".into()],
            ),
        };
        let mut requires = Vec::new();
        match &o.action {
            BenchAction::AddMod { m } => {
                let md = &b.mods[*m as usize];
                let side = match md.side {
                    Some(poe1_v2::data::Side::Prefix) => SideReq::Prefix,
                    Some(poe1_v2::data::Side::Suffix) => SideReq::Suffix,
                    None => bail!("bench mod {} has no side", md.id),
                };
                // A craft on a normal item turns it magic (owner, Q3), and a metamod craft
                // needs the crafted slot free: the player removes the old craft first (Q2).
                requires.push(Require::Rarity {
                    any_of: vec![Rarity::Normal, Rarity::Magic, Rarity::Rare],
                });
                requires.push(classes(None));
                requires.push(Require::OpenAffix { side });
                requires.push(Require::CraftedRoom);
                requires.push(Require::GroupFree {
                    r#mod: md.id.clone(),
                });
            }
            BenchAction::AddEnchant { .. } => requires.push(classes(None)),
            BenchAction::RemoveCrafted => {
                requires.push(classes(None));
                requires.push(Require::HasCrafted);
            }
            BenchAction::RemoveEnchants => {
                requires.push(classes(None));
                requires.push(Require::HasEnchant);
            }
            BenchAction::Sockets { count } => {
                requires.push(classes(Some(*count)));
                requires.push(Require::CanHaveSockets);
            }
            BenchAction::Links { count } => {
                requires.push(classes(Some(*count)));
                requires.push(Require::HasSockets);
                requires.push(Require::MinSockets { n: *count });
            }
            BenchAction::Colours { spec } => {
                requires.push(classes(Some(spec.len() as u8)));
                requires.push(Require::HasSockets);
                requires.push(Require::MinSockets {
                    n: spec.len() as u8,
                });
            }
            BenchAction::RerollRarity => requires.push(classes(None)),
            BenchAction::RerollRareMods { .. } => {
                requires.push(classes(None));
                requires.push(Require::Rarity {
                    any_of: vec![Rarity::Rare],
                });
            }
        }
        // A bench craft changes no existing explicit except the remove-crafted option, which
        // needs no protection policy.
        requires.insert(0, Require::NotCorrupted);
        requires.insert(1, Require::NotMirrored);
        // Socket, link and colour crafts are the exception: the bench offers them on a
        // corrupted item at an extra Vaal Orb cost (claim bench.corrupted-sockets).
        if matches!(
            o.action,
            BenchAction::Sockets { .. } | BenchAction::Links { .. } | BenchAction::Colours { .. }
        ) {
            requires.remove(0);
        }
        out.push(Method {
            id,
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
            protect: None,
            ops,
            claims,
            game_text: None,
            example: None,
            resonators: Vec::new(),
        });
    }

    // ---- essences (those with a modifier for some in-scope class)
    for e in b.essences.iter().filter(|e| !e.mods.is_empty()) {
        let requires = vec![
            Require::NotCorrupted,
            Require::NotMirrored,
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
        out.push(Method {
            id: format!("essence/{}", price_key(&e.name)),
            name: e.name.clone(),
            family: Family::Essence,
            inputs: 1,
            cost: vec![cost(&e.name, 1.0)],
            requires,
            pair: Vec::new(),
            protect: Some(FORBIDDEN),
            ops: vec![
                Op::SetRarity {
                    rarity: Rarity::Rare,
                },
                Op::Clear { side: SideReq::Any },
                Op::AddEssence {
                    essence: e.name.clone(),
                },
                // "Properties restricted to level N and below" caps the reforge's modifiers, not
                // the item (claim essence.ilvl).
                Op::Fill {
                    fill: Fill {
                        counts: counts.clone(),
                        weighting: Weighting::Normal,
                        side: SideReq::Any,
                        mod_level_max: e.item_level_max,
                        include: None,
                        one_veiled: false,
                    },
                },
            ],
            claims: vec![
                "game-text".into(),
                "essence.ilvl".into(),
                "reforge.guarantee-first".into(),
                "reforge.counts".into(),
                "metamod.multicraft-does-not-block".into(),
            ],
            game_text: None,
            example: None,
            resonators: Vec::new(),
        });
    }

    // ---- catalysts
    for c in &b.catalysts {
        let requires = vec![
            Require::NotCorrupted,
            Require::NotMirrored,
            Require::Class {
                any_of: vec!["Ring".into(), "Amulet".into(), "Belt".into()],
            },
            Require::CatalystRoom {
                catalyst: c.name.clone(),
            },
        ];
        out.push(Method {
            id: format!("catalyst/{}", price_key(&c.name)),
            name: c.name.clone(),
            family: Family::Catalyst,
            inputs: 1,
            cost: vec![cost(&c.name, 1.0)],
            requires,
            pair: Vec::new(),
            protect: None,
            // Since 3.25 the amount a catalyst adds depends on the item's item level.
            ops: vec![Op::Unmodelled {
                reason: PendingReason::NeedsData {
                    what: "the catalyst quality amount depends on item level".into(),
                },
            }],
            claims: vec![
                "game-text".into(),
                "quality.per-level".into(),
                "catalyst.switch-resets".into(),
            ],
            game_text: None,
            example: None,
            resonators: Vec::new(),
        });
    }

    // ---- fossils: one method per resonator kind; the resonator size follows the fossil count
    for (id, name, rarity, kind) in [
        (
            "fossil/chaotic-resonator",
            "Fossils in a Chaotic Resonator",
            Rarity::Rare,
            "Chaotic",
        ),
        (
            "fossil/alchemical-resonator",
            "Fossils in an Alchemical Resonator",
            Rarity::Normal,
            "Alchemical",
        ),
    ] {
        // A fossil reforge only applies to item classes that can be rare.
        let requires = vec![
            Require::NotCorrupted,
            Require::NotMirrored,
            Require::Category {
                any_of: vec![
                    Category::Weapon,
                    Category::Armour,
                    Category::Jewellery,
                    Category::Quiver,
                    Category::Jewel,
                    Category::AbyssJewel,
                ],
            },
            Require::Rarity {
                any_of: vec![rarity],
            },
            Require::Fossils { min: 1, max: 4 },
            Require::FossilLimits,
        ];
        let mut ops = vec![Op::SetRarity {
            rarity: Rarity::Rare,
        }];
        // A rare loses what can change first; a normal item has nothing to lose.
        if rarity == Rarity::Rare {
            ops.push(Op::Clear { side: SideReq::Any });
        }
        ops.push(Op::Fill {
            fill: Fill {
                counts: counts.clone(),
                weighting: Weighting::Fossils,
                side: SideReq::Any,
                mod_level_max: None,
                include: None,
                one_veiled: false,
            },
        });
        out.push(Method {
            id: id.into(),
            name: name.into(),
            family: Family::Fossil,
            inputs: 1,
            cost: Vec::new(),
            requires,
            pair: Vec::new(),
            protect: Some(FORBIDDEN),
            ops,
            claims: vec![
                "game-text".into(),
                "chaos.count".into(),
                "reforge.counts".into(),
                "fossil.fractured-limits".into(),
                "metamod.multicraft-does-not-block".into(),
            ],
            game_text: None,
            example: None,
            resonators: resonators(b, kind)?,
        });
    }
    Ok(out)
}

/// What only the authoring side checks: every Fill count table is claim `chaos.count`'s value,
/// and every tag pool names one of the Harvest tags.
fn authoring_checks(m: &Method, counts: &[(u8, u32)]) -> anyhow::Result<()> {
    for op in m.all_ops() {
        match op {
            Op::Fill { fill } if !fill.counts.is_empty() && fill.counts != *counts => {
                bail!(
                    "method {}: Fill counts {:?} differ from claim chaos.count {:?}",
                    m.id,
                    fill.counts,
                    counts
                );
            }
            Op::AddRandom {
                pool: Pool::Tag { tag },
                ..
            } if !HARVEST_TAGS.contains(&tag.as_str()) => {
                bail!("method {}: {tag} is not a Harvest tag", m.id);
            }
            _ => {}
        }
    }
    Ok(())
}

/// The reviewed pairs where a currency's directions name a limit the method deliberately does
/// not repeat, with the reason. The three quality currencies say "non-unique" in their
/// directions because the 3.25 item-level scaling applies only to non-unique items (claim
/// `quality.per-level`); a unique item is still a legal target (claim `quality.unique`).
const DIRECTION_EXCEPTIONS: [(&str, &str); 3] = [
    ("currency/armourers-scrap", "non-unique"),
    ("currency/blacksmiths-whetstone", "non-unique"),
    ("currency/glassblowers-bauble", "non-unique"),
];

/// The phrases in a currency's in-game `directions` line that limit its target, as the
/// requirement each implies. The wording is the game's own, so a new currency whose directions
/// state a limit lands here instead of passing silently.
fn direction_rules(dirs: &str) -> Vec<(&'static str, Require)> {
    let mut out = Vec::new();
    let has = |p: &str| dirs.contains(p);
    if has("non-unique") || has("non-Unique") {
        out.push(("non-unique", Require::NotUnique));
    }
    let cannot_influence = has("Cannot be used on") && has("Influenced");
    if cannot_influence || has("with no influence") {
        out.push(("no-influence", Require::NoInfluence));
    }
    if has("Influenced") && !cannot_influence && !has("with no influence") {
        out.push(("influenced", Require::MinInfluencedMods { n: 2 }));
    }
    if has("Fractured") {
        out.push(("no-fractured", Require::NoFractured));
    }
    if has("Cannot be used on Veiled") {
        out.push(("no-veiled", Require::NoVeiled));
    }
    if has("normal, magic or rare") {
        out.push((
            "normal-magic-rare",
            Require::Rarity {
                any_of: vec![Rarity::Normal, Rarity::Magic, Rarity::Rare],
            },
        ));
    } else if has("magic or rare item") {
        out.push((
            "magic-or-rare",
            Require::Rarity {
                any_of: vec![Rarity::Magic, Rarity::Rare],
            },
        ));
    } else if has("magic, rare or unique") {
        out.push((
            "magic-rare-unique",
            Require::Rarity {
                any_of: vec![Rarity::Magic, Rarity::Rare, Rarity::Unique],
            },
        ));
    } else if has("rare item") {
        out.push((
            "rare",
            Require::Rarity {
                any_of: vec![Rarity::Rare],
            },
        ));
    }
    if has("normal item") {
        out.push((
            "normal",
            Require::Rarity {
                any_of: vec![Rarity::Normal],
            },
        ));
    }
    if has("corrupted") {
        out.push(("corrupted", Require::Corrupted));
    }
    out
}

/// Whether `m` meets the limits its costing currency's directions state. A method may be
/// stricter than the directions, never looser.
fn check_directions(b: &Bundle, m: &Method) -> anyhow::Result<()> {
    if m.family != Family::Currency {
        return Ok(());
    }
    for c in &m.cost {
        if c.name.starts_with("beast:") {
            continue;
        }
        let Some(currency) = b.currencies.iter().find(|x| x.name == c.name) else {
            continue;
        };
        for (key, want) in direction_rules(&currency.directions) {
            if DIRECTION_EXCEPTIONS.contains(&(m.id.as_str(), key)) || satisfies(m, &want) {
                continue;
            }
            bail!(
                "method {} costs {} whose directions state \"{key}\", but the method does not require it",
                m.id,
                c.name
            );
        }
    }
    Ok(())
}

/// Whether the method's requirements imply `want`.
fn satisfies(m: &Method, want: &Require) -> bool {
    m.requires.iter().any(|have| match (have, want) {
        (Require::NotUnique, Require::NotUnique)
        | (Require::NoInfluence, Require::NoInfluence)
        | (Require::NoFractured, Require::NoFractured)
        | (Require::NoVeiled, Require::NoVeiled)
        | (Require::NotCorrupted, Require::NotCorrupted)
        | (Require::Corrupted, Require::Corrupted) => true,
        (Require::Rarity { any_of: a }, Require::Rarity { any_of: b }) => {
            b.iter().all(|r| a.contains(r))
        }
        (Require::HasInfluence { .. } | Require::SingleInfluence, Require::HasInfluence { .. }) => {
            true
        }
        (Require::MinInfluencedMods { n }, Require::MinInfluencedMods { n: want }) => n >= want,
        _ => false,
    })
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
    }
    let counts = chaos_counts(b)?;
    for m in &methods {
        authoring_checks(m, &counts)?;
        check_directions(b, m)?;
    }
    methods.sort_by(|a, b| a.id.cmp(&b.id));
    b.methods = methods;
    // Every method must load the way the library loads it: `Data::from_bundle` runs
    // `Method::validate` on each.
    poe1_v2::data::Data::from_bundle(b.clone()).context("the methods do not validate")?;
    let rules_file: ScopeFile = load_json(rules, "currency-scope.json")?;
    b.scope = scope(b, &rules_file)?;
    Ok(())
}
