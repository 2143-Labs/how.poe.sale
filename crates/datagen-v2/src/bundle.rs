//! Builds the v2 `Bundle` from the pinned RePoE export and the hand-authored rules in
//! `rules/v2/`: in-scope classes and bases, every modifier kind an in-scope item can carry, bench
//! options, essences, fossils, catalysts, currencies and the stat translations their lines use.
//! The pinned export splits tincture lines into `stat_translations/tincture.json`; those entries
//! are merged after the main file, which wins where both carry a stat.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, bail};
use poe1_v2::data::{
    BaseData, BenchAction, BenchData, Bundle, CatalystData, Category, Claim, ClassData, Condition,
    Cost, CurrencyData, Decimals, Domain, EssenceData, FORMAT, FossilData, Influence, Limits,
    ModData, ModKind, Side, Slot, StatRange, TranslationData, Variant,
};
use serde::Deserialize;

use crate::raw::{
    RawBase, RawBench, RawClass, RawEssence, RawFossil, RawMod, RawTranslation, RawWeight,
    handler_factor, influence_of_tag, placeholders,
};

pub const PATCH: &str = "3.29.3.3";
pub const COMMIT: &str = "d8635430f4005faccfed69c7957ef2fb9f8fb1fb";

/// Currency item classes in `base_items.json`.
const CURRENCY_CLASSES: [&str; 3] = [
    "StackableCurrency",
    "DelveStackableSocketableCurrency",
    "DelveSocketableCurrency",
];

/// Catalyst tags: a hand-maintained table (copied from v1 `datagen`), not RePoE data. Sinistral
/// and Dextral enhance a side instead of a tag.
const CATALYST_TAGS: &[(&str, &[&str], Option<Side>)] = &[
    ("Abrasive Catalyst", &["attack"], None),
    ("Accelerating Catalyst", &["speed"], None),
    ("Fertile Catalyst", &["life", "mana"], None),
    ("Imbued Catalyst", &["caster"], None),
    ("Intrinsic Catalyst", &["attribute"], None),
    (
        "Noxious Catalyst",
        &["physical_damage", "chaos_damage"],
        None,
    ),
    ("Prismatic Catalyst", &["resistance"], None),
    ("Tempering Catalyst", &["defences"], None),
    ("Turbulent Catalyst", &["elemental_damage"], None),
    ("Unstable Catalyst", &["critical"], None),
    ("Sinistral Catalyst", &[], Some(Side::Prefix)),
    ("Dextral Catalyst", &[], Some(Side::Suffix)),
];

const CATALYST_PREFIX: &str = "Adds quality that enhances ";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ItemClassesFile {
    claims: Vec<String>,
    categories: BTreeMap<String, Limits>,
    classes: Vec<ClassRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClassRule {
    id: String,
    category: Category,
    max_sockets: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimsFile {
    claims: Vec<Claim>,
}

pub fn load_json<T: serde::de::DeserializeOwned>(dir: &Path, file: &str) -> anyhow::Result<T> {
    let path = dir.join(file);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn category_key(c: Category) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The class whose essence column a class uses. RePoE's `essences.json` has no Rune Dagger or
/// Warstaff column; poedb lists them with the Dagger and Staff columns (one-hand and two-hand
/// essences respectively).
fn essence_column(class: &str) -> &str {
    match class {
        "Rune Dagger" => "Dagger",
        "Warstaff" => "Staff",
        other => other,
    }
}

fn base_domain(d: &str) -> Option<Domain> {
    Some(match d {
        "item" => Domain::Item,
        "misc" => Domain::Misc,
        "abyss_jewel" => Domain::AbyssJewel,
        "flask" => Domain::Flask,
        "tincture" => Domain::Tincture,
        _ => return None,
    })
}

/// The kind a raw mod is kept as, per the plan's domain/generation table (first row that holds).
fn mod_kind(id: &str, m: &RawMod, base_implicits: &BTreeSet<&str>) -> Option<ModKind> {
    if id.contains("Royale") {
        return None;
    }
    let affix = matches!(m.generation_type.as_str(), "prefix" | "suffix");
    let d = m.domain.as_str();
    let g = m.generation_type.as_str();
    Some(match (d, g) {
        ("item" | "misc" | "abyss_jewel" | "flask" | "tincture", _) if affix => {
            if m.is_essence_only {
                ModKind::EssenceOnly
            } else {
                ModKind::Natural
            }
        }
        ("crafted", _) if affix => ModKind::Crafted,
        ("delve", _) if affix => ModKind::Delve,
        ("unveiled", _) if affix => ModKind::Unveiled,
        ("veiled", _) if affix => ModKind::Veiled,
        ("mercenary", _) if affix => ModKind::Mercenary,
        ("ducat_crafted", _) if affix => ModKind::Ducat,
        ("item" | "misc" | "abyss_jewel", "corrupted") => ModKind::CorruptedImplicit,
        (_, "searing_exarch_implicit") => ModKind::Exarch,
        (_, "eater_of_worlds_implicit") => ModKind::Eater,
        ("item", "enchantment") => ModKind::Enchant,
        (_, "flask_enchantment_enkindling") => ModKind::FlaskEnkindling,
        (_, "flask_enchantment_instilling") => ModKind::FlaskInstilling,
        (_, "unique") if base_implicits.contains(id) => ModKind::BaseImplicit,
        ("item", "unique") if id.starts_with("SynthesisImplicit") => ModKind::Synthesis,
        _ => return None,
    })
}

/// A stat translation's decimals: the last handler naming a precision wins.
fn decimals(handlers: &[String]) -> Decimals {
    let mut out = Decimals::UpTo(2);
    for h in handlers {
        for n in 0..=2u8 {
            if h.ends_with(&format!("_{n}dp_if_required")) {
                out = Decimals::UpTo(n);
            } else if h.ends_with(&format!("_{n}dp")) {
                out = Decimals::Fixed(n);
            }
        }
    }
    out
}

struct Table {
    names: Vec<String>,
    index: HashMap<String, u16>,
}

impl Table {
    fn new(set: BTreeSet<String>) -> Table {
        let names: Vec<String> = set.into_iter().collect();
        let index = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u16))
            .collect();
        Table { names, index }
    }

    fn get(&self, name: &str, what: &str) -> anyhow::Result<u16> {
        self.index
            .get(name)
            .copied()
            .with_context(|| format!("{what} {name} is not in the table"))
    }

    fn push(&mut self, name: &str) -> u16 {
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        let i = self.names.len() as u16;
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), i);
        i
    }
}

/// One translation entry as the bundle stores it, or `None` when it names no kept mod's stat or
/// every English variant drops out.
fn build_translation(
    t: &RawTranslation,
    stats: &mut Table,
    mod_stat_count: usize,
) -> anyhow::Result<Option<TranslationData>> {
    if t.hidden == Some(true) {
        return Ok(None);
    }
    let ids = t.ids();
    if !ids.iter().any(|s| {
        stats
            .index
            .get(s)
            .is_some_and(|&i| (i as usize) < mod_stat_count)
    }) {
        return Ok(None);
    }
    let mut variants = Vec::new();
    'variants: for e in t.english.iter().flatten() {
        let Some(order) = placeholders(&e.string, ids.len()) else {
            continue;
        };
        let format = |k: usize| e.format.as_ref().and_then(|f| f.get(k)).map(String::as_str);
        let mut slots = Vec::with_capacity(order.len());
        for &k in &order {
            if format(k) == Some("ignore") {
                continue 'variants;
            }
            let handlers: &[String] = e
                .index_handlers
                .as_ref()
                .and_then(|h| h.get(k))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut factor = 1.0;
            for name in handlers {
                match handler_factor(name) {
                    Some(Some(f)) => factor *= f,
                    Some(None) => continue 'variants,
                    None => bail!(
                        "stat translation handler {name} has no known display factor; add it to handler_factor"
                    ),
                }
            }
            slots.push(Slot {
                k: k as u8,
                plus: format(k) == Some("+#"),
                factor,
                decimals: decimals(handlers),
            });
        }
        let conditions = e
            .condition
            .iter()
            .flatten()
            .map(|c| Condition {
                min: c.min,
                max: c.max,
                negated: c.negated.unwrap_or(false),
            })
            .collect();
        variants.push(Variant {
            conditions,
            string: e.string.clone(),
            slots,
        });
    }
    if variants.is_empty() {
        return Ok(None);
    }
    Ok(Some(TranslationData {
        stats: ids.iter().map(|s| stats.push(s)).collect(),
        variants,
    }))
}

pub fn build(raw: &Path, rules: &Path) -> anyhow::Result<Bundle> {
    let raw_mods: BTreeMap<String, RawMod> = load_json(raw, "mods.json")?;
    let raw_bases: BTreeMap<String, RawBase> = load_json(raw, "base_items.json")?;
    let raw_classes: BTreeMap<String, RawClass> = load_json(raw, "item_classes.json")?;
    let raw_bench: Vec<RawBench> = load_json(raw, "crafting_bench_options.json")?;
    let raw_essences: BTreeMap<String, RawEssence> = load_json(raw, "essences.json")?;
    let raw_fossils: BTreeMap<String, RawFossil> = load_json(raw, "fossils.json")?;
    let class_rules: ItemClassesFile = load_json(rules, "item-classes.json")?;
    let claims: ClaimsFile = load_json(rules, "claims.json")?;

    for c in &class_rules.claims {
        if !claims.claims.iter().any(|k| &k.id == c) {
            bail!("item-classes.json cites unknown claim {c}");
        }
    }

    // ---- classes in scope, in rule order
    let class_idx: HashMap<&str, u16> = class_rules
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i as u16))
        .collect();

    // ---- bases
    let kept_bases: Vec<(&String, &RawBase, Domain)> = raw_bases
        .iter()
        .filter_map(|(id, b)| {
            let domain = base_domain(&b.domain)?;
            let keep = b.release_state == "released"
                && class_idx.contains_key(b.item_class.as_str())
                && !(b.item_class == "Jewel" && domain != Domain::Misc)
                // Battle Royale copies of real bases carry `Royale` in their id and only Royale implicits.
                && !id.contains("Royale");
            keep.then_some((id, b, domain))
        })
        .collect();
    let base_implicits: BTreeSet<&str> = kept_bases
        .iter()
        .flat_map(|(_, b, _)| b.implicits.iter().map(String::as_str))
        .collect();

    // ---- currencies (released currency-class bases, by metadata id)
    let currencies: Vec<CurrencyData> = raw_bases
        .iter()
        .filter(|(_, b)| {
            b.release_state == "released" && CURRENCY_CLASSES.contains(&b.item_class.as_str())
        })
        .map(|(id, b)| CurrencyData {
            id: id.clone(),
            name: b.name.clone(),
            class: b.item_class.clone(),
            description: b
                .properties
                .description
                .clone()
                .unwrap_or_default()
                .replace("\r\n", "\n"),
            directions: b
                .properties
                .directions
                .clone()
                .unwrap_or_default()
                .replace("\r\n", "\n"),
        })
        .collect();
    let currency_ids: BTreeSet<&str> = currencies.iter().map(|c| c.id.as_str()).collect();

    // ---- mods
    let kept_mods: Vec<(&String, &RawMod, ModKind)> = raw_mods
        .iter()
        .filter_map(|(id, m)| mod_kind(id, m, &base_implicits).map(|k| (id, m, k)))
        .collect();
    for imp in &base_implicits {
        if !kept_mods.iter().any(|(id, _, _)| id.as_str() == *imp) {
            bail!("base implicit {imp} is not a kept mod");
        }
    }

    let fossils_kept: Vec<(&String, &RawFossil)> = raw_fossils
        .iter()
        .filter(|(id, f)| currency_ids.contains(id.as_str()) && !f.name.trim().is_empty())
        .collect();
    let catalyst_currencies: Vec<&CurrencyData> = currencies
        .iter()
        .filter(|c| c.description.starts_with(CATALYST_PREFIX) && !c.name.starts_with("Tainted"))
        .collect();

    // ---- string tables
    let mut tag_set: BTreeSet<String> = BTreeSet::new();
    tag_set.insert("default".into());
    for (_, m, _) in &kept_mods {
        for w in m.spawn_weights.iter().chain(&m.generation_weights) {
            tag_set.insert(w.tag.clone());
        }
        tag_set.extend(m.implicit_tags.iter().cloned());
        tag_set.extend(m.adds_tags.iter().cloned());
    }
    for (_, b, _) in &kept_bases {
        tag_set.extend(b.tags.iter().cloned());
    }
    for c in &class_rules.classes {
        let raw = raw_classes
            .get(&c.id)
            .with_context(|| format!("item-classes.json names unknown class {}", c.id))?;
        tag_set.extend(raw.influence_tags.iter().flatten().cloned());
    }
    for (_, f) in &fossils_kept {
        for w in f.positive_mod_weights.iter().chain(&f.negative_mod_weights) {
            tag_set.insert(w.tag.clone());
        }
        tag_set.extend(f.allowed_tags.iter().cloned());
        tag_set.extend(f.forbidden_tags.iter().cloned());
    }
    for (_, tags, _) in CATALYST_TAGS {
        tag_set.extend(tags.iter().map(|t| t.to_string()));
    }
    let tags = Table::new(tag_set);

    let groups = Table::new(
        kept_mods
            .iter()
            .flat_map(|(_, m, _)| m.groups.iter().cloned())
            .collect(),
    );
    let mod_types = Table::new(kept_mods.iter().map(|(_, m, _)| m.kind.clone()).collect());
    let mut stats = Table::new(
        kept_mods
            .iter()
            .flat_map(|(_, m, _)| m.stats.iter().map(|s| s.id.clone()))
            .collect(),
    );
    let mod_stat_count = stats.names.len();

    // ---- translations: entries naming a kept mod's stat, in file order. Tincture lines live in
    // `stat_translations/tincture.json` upstream; the main file wins where both carry a stat.
    let main_translations: Vec<RawTranslation> = load_json(raw, "stat_translations.json")?;
    let tincture_translations: Vec<RawTranslation> =
        load_json(raw, "stat_translations/tincture.json").context(
            "run scripts/fetch-data.sh to download stat_translations/tincture.json into data/raw",
        )?;
    let main_stats: BTreeSet<&str> = main_translations
        .iter()
        .flat_map(RawTranslation::ids)
        .map(String::as_str)
        .collect();
    let mut translations = Vec::new();
    for t in &main_translations {
        if let Some(tr) = build_translation(t, &mut stats, mod_stat_count)? {
            translations.push(tr);
        }
    }
    for t in &tincture_translations {
        // `tincture.json` repeats the general entries the main file already carries verbatim;
        // drop those, since two entries for one stat would print the same line twice.
        if t.ids().iter().any(|s| main_stats.contains(s.as_str())) {
            continue;
        }
        if let Some(tr) = build_translation(t, &mut stats, mod_stat_count)? {
            translations.push(tr);
        }
    }

    // ---- mods out
    let tag_list = |list: &[String], what: &str| -> anyhow::Result<Vec<u16>> {
        list.iter().map(|t| tags.get(t, what)).collect()
    };
    let weights = |list: &[RawWeight], what: &str| -> anyhow::Result<Vec<(u16, u32)>> {
        list.iter()
            .map(|w| Ok((tags.get(&w.tag, what)?, w.weight)))
            .collect()
    };
    let mut mods = Vec::with_capacity(kept_mods.len());
    for (id, m, kind) in &kept_mods {
        let side = match m.generation_type.as_str() {
            "prefix" => Some(Side::Prefix),
            "suffix" => Some(Side::Suffix),
            _ => None,
        };
        let eldritch_tier = if matches!(kind, ModKind::Exarch | ModKind::Eater) {
            let digits: String = id
                .chars()
                .rev()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            match digits.parse::<u8>() {
                Ok(t @ 1..=6) => Some(t),
                _ => bail!("eldritch mod {id} has no trailing tier 1-6"),
            }
        } else {
            None
        };
        let influence = m
            .spawn_weights
            .iter()
            .filter(|w| w.weight > 0)
            .find_map(|w| influence_of_tag(&w.tag));
        let stats_out = m
            .stats
            .iter()
            .map(|s| {
                Ok(StatRange {
                    stat: stats.get(&s.id, "stat")?,
                    min: s.min as i32,
                    max: s.max as i32,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        mods.push(ModData {
            id: (*id).clone(),
            name: m.name.clone().unwrap_or_default(),
            kind: *kind,
            side,
            domain: m.domain.clone(),
            groups: m
                .groups
                .iter()
                .map(|g| groups.get(g, "group"))
                .collect::<anyhow::Result<_>>()?,
            mod_type: mod_types.get(&m.kind, "mod type")?,
            tags: tag_list(&m.implicit_tags, "implicit tag")?,
            adds_tags: tag_list(&m.adds_tags, "added tag")?,
            spawn: weights(&m.spawn_weights, "spawn tag")?,
            generation: weights(&m.generation_weights, "generation tag")?,
            level: u8::try_from(m.required_level)
                .with_context(|| format!("mod {id} level {}", m.required_level))?,
            influence,
            elevated: *kind == ModKind::Natural && id.contains("Maven"),
            eldritch_tier,
            stats: stats_out,
            text: m.text.clone().unwrap_or_default(),
        });
    }
    // Elevated mods have no spawn weights: they take the influence of their base chain. RePoE
    // gives elevated mods their own `…Maven` mod type, so the chain is found by id first (the
    // elevated id's stem before `Maven` plus a tier suffix, same groups), then by mod type with
    // `Maven` removed; only a single influence counts.
    let natural_chain: Vec<&ModData> = mods
        .iter()
        .filter(|m| m.kind == ModKind::Natural && !m.elevated)
        .collect();
    let single = |list: Vec<&&ModData>| -> Option<Influence> {
        let set: BTreeSet<Influence> = list.iter().filter_map(|m| m.influence).collect();
        let mut it = set.into_iter();
        match (it.next(), it.next()) {
            (Some(i), None) => Some(i),
            _ => None,
        }
    };
    let elevated_influence: Vec<(usize, Option<Influence>)> = mods
        .iter()
        .enumerate()
        .filter(|(_, m)| m.elevated)
        .map(|(i, e)| {
            let stem = &e.id[..e.id.find("Maven").unwrap_or(e.id.len())];
            let by_id = natural_chain
                .iter()
                .filter(|m| {
                    m.groups == e.groups
                        && m.id.strip_prefix(stem).is_some_and(|rest| {
                            rest.chars().all(|c| c.is_ascii_digit() || c == '_')
                        })
                })
                .collect();
            let elevated_type = &mod_types.names[e.mod_type as usize];
            let base_type = elevated_type.replace("Maven", "");
            let inf = single(by_id).or_else(|| {
                single(
                    natural_chain
                        .iter()
                        .filter(|m| {
                            let t = &mod_types.names[m.mod_type as usize];
                            m.groups == e.groups && (t == elevated_type || *t == base_type)
                        })
                        .collect(),
                )
            });
            (i, inf)
        })
        .collect();
    for (i, inf) in elevated_influence {
        mods[i].influence = inf;
    }
    let mod_idx: HashMap<&str, u32> = mods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.as_str(), i as u32))
        .collect();
    let mod_ref = |id: &str, what: &str| -> anyhow::Result<u32> {
        mod_idx
            .get(id)
            .copied()
            .with_context(|| format!("{what} names unknown mod {id}"))
    };

    // ---- classes out
    let mut classes = Vec::with_capacity(class_rules.classes.len());
    for c in &class_rules.classes {
        let raw = &raw_classes[&c.id];
        let influence_tags = match raw.influence_tags.as_deref() {
            Some(list) if !list.is_empty() => {
                let mut out = [u16::MAX; 6];
                for t in list {
                    let inf = influence_of_tag(t)
                        .with_context(|| format!("class {} has unknown influence tag {t}", c.id))?;
                    out[inf as usize] = tags.get(t, "influence tag")?;
                }
                if out.contains(&u16::MAX) {
                    bail!("class {} lacks an influence tag", c.id);
                }
                Some(out)
            }
            _ => None,
        };
        let key = category_key(c.category);
        let limits = class_rules
            .categories
            .get(&key)
            .with_context(|| format!("item-classes.json has no limits for category {key}"))?
            .clone();
        classes.push(ClassData {
            id: c.id.clone(),
            name: raw.name.clone(),
            category: c.category,
            influence_tags,
            limits,
            max_sockets: c.max_sockets,
        });
    }

    // ---- bases out
    let mut bases = Vec::with_capacity(kept_bases.len());
    for (id, b, domain) in &kept_bases {
        let (mut prefix_delta, mut suffix_delta, mut explicit_effect) = (0i8, 0i8, 0i16);
        let (mut prefix_effect, mut suffix_effect) = (0i16, 0i16);
        let mut implicits_locked = false;
        let mut all_influences = false;
        let mut implicits = Vec::with_capacity(b.implicits.len());
        for imp in &b.implicits {
            implicits.push(mod_ref(imp, &format!("base {id}"))?);
            for s in &raw_mods[imp].stats {
                match s.id.as_str() {
                    "local_maximum_prefixes_allowed_+" => prefix_delta = s.min as i8,
                    "local_maximum_suffixes_allowed_+" => suffix_delta = s.min as i8,
                    "local_explicit_mod_effect_+%" => explicit_effect = s.min as i16,
                    "local_prefix_mod_effect_+%" => prefix_effect = s.min as i16,
                    "local_suffix_mod_effect_+%" => suffix_effect = s.min as i16,
                    "local_implicit_mod_cannot_be_changed" => implicits_locked = true,
                    "local_item_can_roll_all_influences" => all_influences = true,
                    _ => {}
                }
            }
        }
        let range =
            |r: &Option<crate::raw::RawRange>| r.as_ref().map(|r| [r.min as i32, r.max as i32]);
        let p = &b.properties;
        let req = b.requirements.as_ref();
        bases.push(BaseData {
            id: (*id).clone(),
            name: b.name.clone(),
            class: class_idx[b.item_class.as_str()],
            domain: *domain,
            tags: tag_list(&b.tags, "base tag")?,
            implicits,
            drop_level: b.drop_level,
            requirements: [
                req.map_or(0, |r| r.level),
                req.map_or(0, |r| r.strength),
                req.map_or(0, |r| r.dexterity),
                req.map_or(0, |r| r.intelligence),
            ],
            defences: [
                range(&p.armour),
                range(&p.evasion),
                range(&p.energy_shield),
                range(&p.ward),
            ],
            prefix_delta,
            suffix_delta,
            explicit_effect,
            prefix_effect,
            suffix_effect,
            implicits_locked,
            all_influences,
        });
    }

    // ---- bench
    let mut bench = Vec::new();
    for (index, b) in raw_bench.iter().enumerate() {
        let classes_in: Vec<u16> = b
            .item_classes
            .iter()
            .filter_map(|c| class_idx.get(c.as_str()).copied())
            .collect();
        if classes_in.is_empty() {
            continue;
        }
        let a = &b.actions;
        let what = format!("bench option {index}");
        let mut actions = Vec::new();
        if let Some(m) = &a.add_explicit_mod {
            actions.push(BenchAction::AddMod {
                m: mod_ref(m, &what)?,
            });
        }
        if let Some(m) = &a.add_enchant_mod {
            actions.push(BenchAction::AddEnchant {
                m: mod_ref(m, &what)?,
            });
        }
        if a.remove_crafted_mods == Some(true) {
            actions.push(BenchAction::RemoveCrafted);
        }
        if a.remove_enchantments == Some(true) {
            actions.push(BenchAction::RemoveEnchants);
        }
        if let Some(count) = a.change_socket_count {
            actions.push(BenchAction::Sockets { count });
        }
        if let Some(count) = a.link_sockets {
            actions.push(BenchAction::Links { count });
        }
        if let Some(spec) = &a.color_sockets {
            actions.push(BenchAction::Colours { spec: spec.clone() });
        }
        if a.reroll_rarity == Some(true) {
            actions.push(BenchAction::RerollRarity);
        }
        if let Some(n) = a.reroll_rare_mods {
            actions.push(BenchAction::RerollRareMods { n });
        }
        let action = match <[BenchAction; 1]>::try_from(actions) {
            Ok([a]) => a,
            Err(v) => bail!("{what} has {} actions, expected one", v.len()),
        };
        bench.push(BenchData {
            index: index as u16,
            master: b.master.clone(),
            tier: b.bench_tier,
            classes: classes_in,
            cost: b
                .cost
                .iter()
                .map(|(k, &amount)| Cost {
                    key: k.clone(),
                    amount,
                })
                .collect(),
            action,
        });
    }

    // ---- essences
    let mut essences = Vec::new();
    for (id, e) in &raw_essences {
        let Some(cur) = currencies.iter().find(|c| &c.id == id) else {
            continue;
        };
        let mut class_mods = Vec::new();
        for (ci, rule) in class_rules.classes.iter().enumerate() {
            let m = e.mods.get(&rule.id).and_then(|m| m.as_deref()).or_else(|| {
                e.mods
                    .get(essence_column(&rule.id))
                    .and_then(|m| m.as_deref())
            });
            let Some(m) = m else {
                continue;
            };
            class_mods.push((ci as u16, mod_ref(m, &format!("essence {}", e.name))?));
        }
        class_mods.sort();
        essences.push(EssenceData {
            currency: id.clone(),
            name: e.name.clone(),
            level: e.level,
            corruption_only: e.kind.is_corruption_only,
            allows_rare: cur.description.contains("or reforges a rare item"),
            item_level_max: e.item_level_restriction,
            mods: class_mods,
        });
    }

    // A class the bench can add a modifier to needs an essence modifier whenever the other classes
    // of its category have one. Equipment without a bench craft (Fishing Rods) and the categories
    // essences do not touch (jewels, flasks, tinctures) are exempt.
    let bench_classes: BTreeSet<u16> = bench
        .iter()
        .filter(|b| matches!(b.action, BenchAction::AddMod { .. }))
        .flat_map(|b| b.classes.iter().copied())
        .collect();
    for ess in &essences {
        for (ci, rule) in class_rules.classes.iter().enumerate() {
            let ci = ci as u16;
            if rule.id == "FishingRod"
                || !bench_classes.contains(&ci)
                || ess.mods.iter().any(|&(c, _)| c == ci)
            {
                continue;
            }
            let siblings: Vec<&str> = class_rules
                .classes
                .iter()
                .enumerate()
                .filter(|(i, r)| {
                    r.category == rule.category && ess.mods.iter().any(|&(c, _)| c == *i as u16)
                })
                .map(|(_, r)| r.id.as_str())
                .collect();
            if !siblings.is_empty() {
                bail!(
                    "essence {} has no modifier for {}, although it has one for {}",
                    ess.name,
                    rule.id,
                    siblings.join(", ")
                );
            }
        }
    }

    // ---- fossils
    let mut fossils = Vec::new();
    for (id, f) in &fossils_kept {
        let what = format!("fossil {}", f.name);
        fossils.push(FossilData {
            currency: (*id).clone(),
            name: f.name.clone(),
            more: weights(&f.positive_mod_weights, "fossil tag")?,
            less: weights(&f.negative_mod_weights, "fossil tag")?,
            added_mods: f
                .added_mods
                .iter()
                .map(|m| mod_ref(m, &what))
                .collect::<anyhow::Result<_>>()?,
            forced_mods: f
                .forced_mods
                .iter()
                .map(|m| mod_ref(m, &what))
                .collect::<anyhow::Result<_>>()?,
            allowed_tags: tag_list(&f.allowed_tags, "fossil tag")?,
            forbidden_tags: tag_list(&f.forbidden_tags, "fossil tag")?,
            corrupted_essence_chance: f.corrupted_essence_chance,
            mirrors: f.mirrors,
            rolls_lucky: f.rolls_lucky,
            changes_quality: f.changes_quality,
            rolls_white_sockets: f.rolls_white_sockets,
            descriptions: f.descriptions.values().cloned().collect(),
        });
    }

    // ---- catalysts
    let mut catalysts = Vec::new();
    for c in &catalyst_currencies {
        let Some((_, tag_names, side)) = CATALYST_TAGS.iter().find(|(n, _, _)| *n == c.name) else {
            bail!("catalyst {} has no entry in CATALYST_TAGS", c.name);
        };
        let label = c.description[CATALYST_PREFIX.len()..]
            .split(" modifiers")
            .next()
            .unwrap_or_default()
            .to_string();
        catalysts.push(CatalystData {
            currency: c.id.clone(),
            name: c.name.clone(),
            label,
            tags: tag_names
                .iter()
                .map(|t| tags.get(t, "catalyst tag"))
                .collect::<anyhow::Result<_>>()?,
            side: *side,
        });
    }
    if catalysts.len() != CATALYST_TAGS.len() {
        bail!(
            "found {} catalysts, expected {}",
            catalysts.len(),
            CATALYST_TAGS.len()
        );
    }

    Ok(Bundle {
        format: FORMAT,
        patch: PATCH.to_string(),
        repoe_commit: COMMIT.to_string(),
        tags: tags.names,
        groups: groups.names,
        stats: stats.names,
        mod_types: mod_types.names,
        classes,
        bases,
        mods,
        bench,
        essences,
        fossils,
        catalysts,
        currencies,
        methods: Vec::new(),
        claims: claims.claims,
        scope: Vec::new(),
        translations,
    })
}
