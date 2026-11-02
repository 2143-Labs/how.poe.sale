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
    index_handlers: Option<Vec<Vec<String>>>,
}

#[derive(Deserialize)]
struct RawTradeStat {
    id: String,
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
struct TranslationOut {
    key: String,
    stat: u16,
    negate: bool,
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

    // Classes with influence tags: the only bases a crafting solver models for now.
    let influenced_classes: BTreeMap<String, &RawClass> = classes
        .iter()
        .filter(|(_, c)| c.influence_tags.as_ref().is_some_and(|t| !t.is_empty()))
        .map(|(k, c)| (k.clone(), c))
        .collect();

    let kept_mods: Vec<(&String, &RawMod)> = mods
        .0
        .iter()
        .filter(|(_, m)| {
            matches!(m.domain.as_str(), "item" | "crafted")
                && matches!(m.generation_type.as_str(), "prefix" | "suffix")
                && !m.stats.is_empty()
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
    let mut translations_out: Vec<TranslationOut> = Vec::new();
    let mut trade_stats: BTreeMap<String, String> = BTreeMap::new();
    let mut stat_set: BTreeSet<&str> = BTreeSet::new();
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
        if t.ids().len() != 1 {
            continue;
        }
        let stat = t.ids()[0].as_str();
        for e in t.english.iter().flatten() {
            if e.string.matches("{0}").count() != 1 {
                continue;
            }
            stat_set.insert(stat);
            let negate = e
                .index_handlers
                .as_ref()
                .and_then(|h| h.first())
                .is_some_and(|h| h.iter().any(|x| x == "negate"));
            translations_out.push(TranslationOut {
                key: e.string.replace("{0}", "#"),
                stat: 0, // filled in below once the table is complete
                negate,
            });
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
    // Fill in the stat index now that the table is complete; rows were emitted in this same order.
    {
        let mut gi = 0usize;
        for t in &translations {
            if t.ids().len() != 1 {
                continue;
            }
            let stat = t.ids()[0].as_str();
            for e in t.english.iter().flatten() {
                if e.string.matches("{0}").count() != 1 {
                    continue;
                }
                translations_out[gi].stat = stat_idx[stat];
                gi += 1;
            }
        }
        debug_assert_eq!(gi, translations_out.len());
    }

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
        format: 1,
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
        translations: translations_out,
        trade_stats,
    })
}
