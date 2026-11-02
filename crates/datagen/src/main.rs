//! Builds `data/bundle.json` from the pinned repoe export (3.29.3.3).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

const USAGE: &str = "usage: datagen --raw <dir> --out <file>";
const PATCH: &str = "3.29.3.3";
const COMMIT: &str = "d8635430f4005faccfed69c7957ef2fb9f8fb1fb";

fn main() -> anyhow::Result<()> {
    let (mut raw, mut out) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--raw" => raw = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            other => bail!("unknown argument {other}; {USAGE}"),
        }
    }
    let raw = raw.context(USAGE)?;
    let out = out.context(USAGE)?;
    if !raw.is_dir() {
        bail!("--raw {} is not a directory", raw.display());
    }
    let bundle = build(&raw)?;
    let bytes = serde_json::to_vec(&bundle).context("serialising bundle")?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}

// ---------------------------------------------------------------- raw repoe shapes

#[derive(Deserialize)]
struct RawMods(pub BTreeMap<String, RawMod>);

#[derive(Deserialize)]
struct RawMod {
    domain: String,
    generation_type: String,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    implicit_tags: Vec<String>,
    #[serde(default)]
    name: Option<String>,
    required_level: u16,
    #[serde(default)]
    spawn_weights: Vec<RawWeight>,
    #[serde(default)]
    stats: Vec<RawStat>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct RawWeight {
    tag: String,
    weight: u32,
}

#[derive(Deserialize)]
struct RawStat {
    id: String,
    min: f64,
    max: f64,
}

#[derive(Deserialize)]
struct RawBase {
    #[serde(default)]
    implicits: Vec<String>,
    item_class: String,
    name: String,
    drop_level: u8,
    release_state: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct RawClass {
    #[serde(default)]
    influence_tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RawBench {
    actions: RawBenchActions,
    #[serde(default)]
    cost: BTreeMap<String, f64>,
    #[serde(default)]
    item_classes: Vec<String>,
}

#[derive(Deserialize)]
struct RawBenchActions {
    add_explicit_mod: Option<String>,
}

#[derive(Deserialize)]
struct RawTranslation {
    #[serde(rename = "English", default)]
    english: Option<Vec<RawEnglish>>,
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default)]
    trade_stats: Option<Vec<RawTradeStat>>,
}

impl RawTranslation {
    fn ids(&self) -> &[String] {
        self.ids.as_deref().unwrap_or_default()
    }

    fn trade_stats(&self) -> &[RawTradeStat] {
        self.trade_stats.as_deref().unwrap_or_default()
    }
}

#[derive(Deserialize)]
struct RawEnglish {
    string: String,
    #[serde(default)]
    format: Option<Vec<String>>,
    #[serde(default)]
    index_handlers: Option<Vec<Vec<String>>>,
}

/// The `{k}` placeholders of a translation string, in text order. `None` when one is out of
/// range or repeated.
fn placeholders(s: &str, n_ids: usize) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}')? + open;
        let k: usize = rest[open + 1..close].parse().ok()?;
        if k >= n_ids || out.contains(&k) {
            return None;
        }
        out.push(k);
        rest = &rest[close + 1..];
    }
    Some(out)
}

#[derive(Deserialize)]
struct RawTradeStat {
    id: String,
}

#[derive(Deserialize)]
struct RawEssence {
    name: String,
    #[serde(default)]
    mods: BTreeMap<String, Option<String>>,
    #[serde(rename = "type")]
    kind: RawEssenceType,
}

#[derive(Deserialize)]
struct RawEssenceType {
    #[serde(default)]
    tier: u8,
    #[serde(default)]
    is_corruption_only: bool,
}

#[derive(Deserialize)]
struct RawFossil {
    name: String,
    #[serde(default)]
    added_mods: Vec<String>,
    #[serde(default)]
    positive_mod_weights: Vec<RawFossilWeight>,
    #[serde(default)]
    negative_mod_weights: Vec<RawFossilWeight>,
    #[serde(default)]
    forbidden_tags: Vec<String>,
    #[serde(default)]
    allowed_tags: Vec<String>,
}

#[derive(Deserialize)]
struct RawFossilWeight {
    tag: String,
    weight: u32,
}

// ---------------------------------------------------------------- bundle shapes

#[derive(Serialize)]
struct Bundle {
    format: u32,
    patch: String,
    source: Source,
    tags: Vec<String>,
    groups: Vec<String>,
    stat_ids: Vec<String>,
    mods: Vec<ModOut>,
    bases: Vec<BaseOut>,
    classes: Vec<ClassOut>,
    bench: Vec<BenchOut>,
    catalysts: Vec<CatalystOut>,
    essences: Vec<EssenceOut>,
    fossils: Vec<FossilOut>,
    translations: Vec<TranslationOut>,
    trade_stats: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Source {
    repo: &'static str,
    commit: &'static str,
}

#[derive(Serialize)]
struct ModOut {
    id: String,
    #[serde(rename = "gen")]
    generation: &'static str,
    #[serde(skip_serializing_if = "is_false")]
    crafted: bool,
    group: u16,
    name: String,
    ilvl: u16,
    stats: Vec<StatOut>,
    text: String,
    implicit_tags: Vec<String>,
    influence: bool,
    weights: Vec<(u16, u32)>,
    #[serde(skip_serializing_if = "is_false")]
    fossil_only: bool,
}

#[derive(Serialize)]
struct StatOut {
    stat: u16,
    min: f64,
    max: f64,
}

#[derive(Serialize)]
struct BaseOut {
    key: String,
    name: String,
    class: String,
    drop_level: u8,
    tags: Vec<u16>,
    prefix_delta: i16,
    suffix_delta: i16,
    explicit_effect: i16,
    implicit_text: Vec<String>,
}

#[derive(Serialize)]
struct ClassOut {
    name: String,
    influence_tags: BTreeMap<String, u16>,
}

#[derive(Serialize)]
struct BenchOut {
    #[serde(rename = "mod")]
    mod_id: String,
    classes: Vec<String>,
    divines: u32,
}

#[derive(Serialize)]
struct CatalystOut {
    name: &'static str,
    price_key: &'static str,
    label: &'static str,
    tags: &'static [&'static str],
}

#[derive(Serialize)]
struct EssenceOut {
    name: String,
    price_key: String,
    tier: u8,
    /// bundle class name -> bundle mod id.
    mods: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct FossilOut {
    name: String,
    price_key: String,
    /// `positive_mod_weights` then `negative_mod_weights`, in file order.
    weights: Vec<(String, u32)>,
    added_mods: Vec<String>,
    forbidden_tags: Vec<String>,
    allowed_tags: Vec<String>,
    supported: bool,
}

/// One way a line can read: `key` with each value replaced by `#`, and per `#` in text order the
/// stat it shows and whether the shown number is the stat negated.
#[derive(Serialize)]
struct TranslationOut {
    key: String,
    slots: Vec<(u16, bool)>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// tag suffix -> influence name, per the class influence tags in `item_classes.json`.
fn influence_name(tag: &str) -> Option<&'static str> {
    Some(match tag.rsplit('_').next()? {
        "crusader" => "Crusader",
        "elder" => "Elder",
        "basilisk" => "Hunter",
        "eyrie" => "Redeemer",
        "shaper" => "Shaper",
        "adjudicator" => "Warlord",
        _ => return None,
    })
}

/// `price_key` for an essence or fossil: lowercase, spaces to '-', apostrophes dropped.
fn price_key(name: &str) -> String {
    name.to_lowercase()
        .replace(['\'', '\u{2019}'], "")
        .replace(' ', "-")
}

/// Essences whose corruption-only mods are still modelled as sources.
const CORRUPTION_ESSENCES: [&str; 4] = ["Hysteria", "Horror", "Delirium", "Insanity"];

/// Every essence is a modelled source: RePoE's `type.tier` is the essence *family* tier
/// (1-6 in this export), not the ladder tier, and the engine counts 45 essence-only amulet
/// mods only when all of them are present. Corruption-only essences are skipped except
/// Hysteria/Horror/Delirium/Insanity, which the plan models.
fn keep_essence(name: &str, corruption_only: bool) -> bool {
    !corruption_only || CORRUPTION_ESSENCES.iter().any(|s| name.contains(s))
}

/// Fossils whose crafting behaviour is not modelled; they still get a name and price.
const UNSUPPORTED_FOSSILS: [&str; 6] = [
    "bloodstained",
    "fractured",
    "gilded",
    "glyphic",
    "hollow",
    "sanctified",
];

/// The catalyst table is a hand-maintained constant, not repoe data.
const CATALYSTS: &[CatalystOut] = &[
    CatalystOut {
        name: "Abrasive Catalyst",
        price_key: "abrasive-catalyst",
        label: "Attack",
        tags: &["attack"],
    },
    CatalystOut {
        name: "Accelerating Catalyst",
        price_key: "accelerating-catalyst",
        label: "Speed",
        tags: &["speed"],
    },
    CatalystOut {
        name: "Fertile Catalyst",
        price_key: "fertile-catalyst",
        label: "Life and Mana",
        tags: &["life", "mana"],
    },
    CatalystOut {
        name: "Imbued Catalyst",
        price_key: "imbued-catalyst",
        label: "Caster",
        tags: &["caster"],
    },
    CatalystOut {
        name: "Intrinsic Catalyst",
        price_key: "intrinsic-catalyst",
        label: "Attribute",
        tags: &["attribute"],
    },
    CatalystOut {
        name: "Noxious Catalyst",
        price_key: "noxious-catalyst",
        label: "Physical and Chaos Damage",
        tags: &["physical_damage", "chaos_damage"],
    },
    CatalystOut {
        name: "Prismatic Catalyst",
        price_key: "prismatic-catalyst",
        label: "Resistance",
        tags: &["resistance"],
    },
    CatalystOut {
        name: "Tempering Catalyst",
        price_key: "tempering-catalyst",
        label: "Defence",
        tags: &["defences"],
    },
    CatalystOut {
        name: "Turbulent Catalyst",
        price_key: "turbulent-catalyst",
        label: "Elemental Damage",
        tags: &["elemental_damage"],
    },
    CatalystOut {
        name: "Unstable Catalyst",
        price_key: "unstable-catalyst",
        label: "Critical",
        tags: &["critical"],
    },
];

fn load_json<T: serde::de::DeserializeOwned>(raw: &Path, file: &str) -> anyhow::Result<T> {
    let path = raw.join(file);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn build(raw: &Path) -> anyhow::Result<Bundle> {
    let mods: RawMods = load_json(raw, "mods.json")?;
    let bases: BTreeMap<String, RawBase> = load_json(raw, "base_items.json")?;
    let classes: BTreeMap<String, RawClass> = load_json(raw, "item_classes.json")?;
    let bench: Vec<RawBench> = load_json(raw, "crafting_bench_options.json")?;
    let translations: Vec<RawTranslation> = load_json(raw, "stat_translations.json")?;
    let essences: BTreeMap<String, RawEssence> = load_json(raw, "essences.json")?;
    let fossils: BTreeMap<String, RawFossil> = load_json(raw, "fossils.json")?;

    // Classes with influence tags: the only bases a crafting solver models for now.
    let influenced_classes: BTreeMap<String, &RawClass> = classes
        .iter()
        .filter(|(_, c)| c.influence_tags.as_ref().is_some_and(|t| !t.is_empty()))
        .map(|(k, c)| (k.clone(), c))
        .collect();

    // ---- essences and fossils (before the mod filter: their mod ids must be kept)
    let referenced_mods: BTreeSet<&str> = essences
        .values()
        .flat_map(|e| e.mods.values().flatten().map(String::as_str))
        .chain(
            fossils
                .values()
                .flat_map(|f| f.added_mods.iter().map(String::as_str)),
        )
        .collect();

    let mut essences_out: Vec<EssenceOut> = Vec::new();
    for e in essences.values() {
        if !keep_essence(&e.name, e.kind.is_corruption_only) {
            continue;
        }
        let class_mods: BTreeMap<String, String> = e
            .mods
            .iter()
            .filter_map(|(class, id)| {
                let id = id.as_deref()?;
                (influenced_classes.contains_key(class) && mods.0.contains_key(id))
                    .then(|| (class.clone(), id.to_string()))
            })
            .collect();
        if class_mods.is_empty() {
            continue;
        }
        essences_out.push(EssenceOut {
            name: e.name.clone(),
            price_key: price_key(&e.name),
            tier: e.kind.tier,
            mods: class_mods,
        });
    }
    essences_out.sort_by(|a, b| a.name.cmp(&b.name));

    let mut fossils_out: Vec<FossilOut> = Vec::new();
    for f in fossils.values() {
        // `RandomFossilOutcome*` placeholders (Tangled Fossil's random rolls) carry no name.
        if f.name.trim().is_empty() {
            continue;
        }
        let lower = f.name.to_lowercase();
        let supported = !UNSUPPORTED_FOSSILS.iter().any(|w| lower.contains(w));
        let mut weights: Vec<(String, u32)> =
            Vec::with_capacity(f.positive_mod_weights.len() + f.negative_mod_weights.len());
        weights.extend(
            f.positive_mod_weights
                .iter()
                .map(|w| (w.tag.clone(), w.weight)),
        );
        weights.extend(
            f.negative_mod_weights
                .iter()
                .map(|w| (w.tag.clone(), w.weight)),
        );
        fossils_out.push(FossilOut {
            name: f.name.clone(),
            price_key: price_key(&f.name),
            weights,
            added_mods: f
                .added_mods
                .iter()
                .filter(|id| mods.0.contains_key(*id))
                .cloned()
                .collect(),
            forbidden_tags: f.forbidden_tags.clone(),
            allowed_tags: f.allowed_tags.clone(),
            supported,
        });
    }
    fossils_out.sort_by(|a, b| a.name.cmp(&b.name));

    let fossil_only: BTreeSet<&str> = fossils_out
        .iter()
        .filter(|f| f.supported)
        .flat_map(|f| f.added_mods.iter().map(String::as_str))
        .collect();

    let kept_mods: Vec<(&String, &RawMod)> = mods
        .0
        .iter()
        .filter(|(id, m)| {
            let modelled = matches!(m.domain.as_str(), "item" | "crafted")
                && matches!(m.generation_type.as_str(), "prefix" | "suffix")
                && !m.stats.is_empty();
            // Essence and fossil mods may live outside the modelled domain (delve), but the
            // bundle must still be able to resolve them by id.
            let referenced = referenced_mods.contains(id.as_str())
                && matches!(m.generation_type.as_str(), "prefix" | "suffix");
            modelled || referenced
        })
        .collect();

    let kept_bases: Vec<(&String, &RawBase)> = bases
        .iter()
        .filter(|(_, b)| {
            influenced_classes.contains_key(&b.item_class) && b.release_state == "released"
        })
        .collect();

    // ---- string tables
    let mut tag_set: BTreeSet<String> = BTreeSet::new();
    tag_set.insert("default".into());
    for (_, m) in &kept_mods {
        for w in &m.spawn_weights {
            tag_set.insert(w.tag.clone());
        }
    }
    for (_, b) in &kept_bases {
        for t in &b.tags {
            tag_set.insert(t.clone());
        }
    }
    for c in influenced_classes.values() {
        for t in c.influence_tags.iter().flatten() {
            tag_set.insert(t.clone());
        }
    }
    let tags: Vec<String> = tag_set.into_iter().collect();
    let tag_idx: BTreeMap<&str, u16> = tags
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), i as u16))
        .collect();

    let mut group_set: BTreeSet<&str> = BTreeSet::new();
    for (_, m) in &kept_mods {
        if let Some(g) = m.groups.first() {
            group_set.insert(g.as_str());
        }
    }
    let groups: Vec<String> = group_set.iter().map(|g| (*g).to_string()).collect();
    let group_idx: BTreeMap<&str, u16> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.as_str(), i as u16))
        .collect();

    // Translations first, so their stat ids join the table the mods index into.
    let mut trade_stats: BTreeMap<String, String> = BTreeMap::new();
    let mut stat_set: BTreeSet<&str> = BTreeSet::new();
    let mut rows: Vec<(String, Vec<(&str, bool)>)> = Vec::new();
    for t in &translations {
        for ts in t.trade_stats() {
            if ts.id.starts_with("explicit.")
                && let Some(stat) = t.ids().first()
            {
                trade_stats
                    .entry(stat.clone())
                    .or_insert_with(|| ts.id.clone());
            }
        }
        let ids = t.ids();
        for e in t.english.iter().flatten() {
            let Some(order) = placeholders(&e.string, ids.len()) else {
                continue;
            };
            let shown = |k: usize| e.format.as_ref().and_then(|f| f.get(k)).map(String::as_str);
            if order.is_empty() || order.iter().any(|&k| shown(k) == Some("ignore")) {
                continue;
            }
            let mut key = e.string.clone();
            let mut slots = Vec::with_capacity(order.len());
            for &k in &order {
                key = key.replace(&format!("{{{k}}}"), "#");
                let negate = e
                    .index_handlers
                    .as_ref()
                    .and_then(|h| h.get(k))
                    .is_some_and(|h| h.iter().any(|x| x == "negate"));
                stat_set.insert(ids[k].as_str());
                slots.push((ids[k].as_str(), negate));
            }
            rows.push((key, slots));
        }
    }
    for (_, m) in &kept_mods {
        for s in &m.stats {
            stat_set.insert(s.id.as_str());
        }
    }
    let stat_ids: Vec<String> = stat_set.iter().map(|s| (*s).to_string()).collect();
    let stat_idx: BTreeMap<&str, u16> = stat_ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u16))
        .collect();
    let translations_out: Vec<TranslationOut> = rows
        .into_iter()
        .map(|(key, slots)| TranslationOut {
            key,
            slots: slots
                .into_iter()
                .map(|(stat, negate)| (stat_idx[stat], negate))
                .collect(),
        })
        .collect();

    // ---- mods
    let mut mods_out = Vec::with_capacity(kept_mods.len());
    for (id, m) in &kept_mods {
        let Some(group) = m.groups.first().and_then(|g| group_idx.get(g.as_str())) else {
            bail!("mod {id} has no known group");
        };
        let weights = m
            .spawn_weights
            .iter()
            .map(|w| {
                tag_idx
                    .get(w.tag.as_str())
                    .map(|i| (*i, w.weight))
                    .with_context(|| format!("mod {id} spawn tag {} not in tag table", w.tag))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        mods_out.push(ModOut {
            id: (*id).clone(),
            generation: if m.generation_type == "prefix" {
                "prefix"
            } else {
                "suffix"
            },
            crafted: m.domain == "crafted",
            group: *group,
            name: m.name.clone().unwrap_or_default(),
            ilvl: m.required_level,
            stats: m
                .stats
                .iter()
                .map(|s| StatOut {
                    stat: stat_idx[s.id.as_str()],
                    min: s.min,
                    max: s.max,
                })
                .collect(),
            text: m.text.clone().unwrap_or_default(),
            implicit_tags: m.implicit_tags.clone(),
            influence: m.implicit_tags.iter().any(|t| t == "influence_mod"),
            weights,
            fossil_only: fossil_only.contains(id.as_str()),
        });
    }

    // ---- bases (implicits are read from the full mod table, which excludes implicit mods)
    let mut bases_out = Vec::with_capacity(kept_bases.len());
    for (key, b) in &kept_bases {
        let (mut prefix_delta, mut suffix_delta, mut explicit_effect) = (0i16, 0i16, 0i16);
        let mut implicit_text = Vec::new();
        for id in &b.implicits {
            let Some(m) = mods.0.get(id) else { continue };
            let text = m.text.as_deref().unwrap_or("");
            implicit_text.extend(text.split('\n').map(|s| s.to_string()));
            for s in &m.stats {
                match s.id.as_str() {
                    "local_maximum_prefixes_allowed_+" => prefix_delta = s.min as i16,
                    "local_maximum_suffixes_allowed_+" => suffix_delta = s.min as i16,
                    "local_explicit_mod_effect_+%" => explicit_effect = s.min as i16,
                    _ => {}
                }
            }
        }
        bases_out.push(BaseOut {
            key: (*key).clone(),
            name: b.name.clone(),
            class: b.item_class.clone(),
            drop_level: b.drop_level,
            tags: b.tags.iter().map(|t| tag_idx[t.as_str()]).collect(),
            prefix_delta,
            suffix_delta,
            explicit_effect,
            implicit_text,
        });
    }

    // ---- classes
    let mut classes_out = Vec::with_capacity(influenced_classes.len());
    for (class_name, c) in &influenced_classes {
        let mut map = BTreeMap::new();
        for t in c.influence_tags.iter().flatten() {
            let influence = influence_name(t)
                .with_context(|| format!("class {class_name} has unknown influence tag {t}"))?;
            map.insert(influence.to_string(), tag_idx[t.as_str()]);
        }
        classes_out.push(ClassOut {
            name: (*class_name).clone(),
            influence_tags: map,
        });
    }

    // ---- bench lock mods
    let lock_mods = [
        "DexMasterItemGenerationCannotChangeSuffixes",
        "StrMasterItemGenerationCannotChangePrefixes",
    ];
    let mut bench_out = Vec::new();
    for b in &bench {
        let Some(id) = b.actions.add_explicit_mod.as_deref() else {
            continue;
        };
        if !lock_mods.contains(&id) {
            continue;
        }
        let divines = b
            .cost
            .get("Metadata/Items/Currency/CurrencyModValues")
            .copied()
            .unwrap_or(0.0)
            .round() as u32;
        bench_out.push(BenchOut {
            mod_id: id.to_string(),
            classes: b.item_classes.clone(),
            divines,
        });
    }

    Ok(Bundle {
        format: 3,
        patch: PATCH.to_string(),
        source: Source {
            repo: "repoe-fork",
            commit: COMMIT,
        },
        tags,
        groups,
        stat_ids,
        mods: mods_out,
        bases: bases_out,
        classes: classes_out,
        bench: bench_out,
        catalysts: CATALYSTS
            .iter()
            .map(|c| CatalystOut {
                name: c.name,
                price_key: c.price_key,
                label: c.label,
                tags: c.tags,
            })
            .collect(),
        essences: essences_out,
        fossils: fossils_out,
        translations: translations_out,
        trade_stats,
    })
}
