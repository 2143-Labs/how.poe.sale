//! The v2 game-data bundle: every base, modifier, bench option, essence, fossil, catalyst,
//! currency and stat translation an in-scope item can touch, plus the method catalog, the cited
//! claims and each currency's scope. `datagen-v2` builds exactly these types; there is no second
//! copy of the schema.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Bundle format version; `Data::from_json` rejects any other.
pub const FORMAT: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub format: u32,
    pub patch: String,
    pub repoe_commit: String,
    pub tags: Vec<String>,
    pub groups: Vec<String>,
    pub stats: Vec<String>,
    pub mod_types: Vec<String>,
    pub classes: Vec<ClassData>,
    pub bases: Vec<BaseData>,
    pub mods: Vec<ModData>,
    pub bench: Vec<BenchData>,
    pub essences: Vec<EssenceData>,
    pub fossils: Vec<FossilData>,
    pub catalysts: Vec<CatalystData>,
    pub currencies: Vec<CurrencyData>,
    pub methods: Vec<crate::method::Method>,
    pub claims: Vec<Claim>,
    /// (currency metadata id, its scope rule result), one per currency.
    pub scope: Vec<(String, Scope)>,
    /// RePoE `stat_translations` entries, in file order (the game's line order).
    pub translations: Vec<TranslationData>,
}

/// What a currency does to in-scope items: the methods that use it, or why it has none.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    Methods { ids: Vec<String> },
    Excluded { reason: String },
}

/// One `stat_translations` entry: the stats it reads and the ways its line can read.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TranslationData {
    pub stats: Vec<u16>,
    pub variants: Vec<Variant>,
}

/// One English line form. `string` keeps its `{k}` placeholders.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    /// One per entry stat.
    pub conditions: Vec<Condition>,
    pub string: String,
    /// One per `{k}`, in text order.
    pub slots: Vec<Slot>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub negated: bool,
}

impl Condition {
    /// Whether a stat value meets this condition.
    pub fn holds(&self, v: i64) -> bool {
        let inside = self.min.is_none_or(|m| i64::from(m) <= v)
            && self.max.is_none_or(|m| v <= i64::from(m));
        inside != self.negated
    }
}

/// A `{k}` placeholder: entry stat `k`, shown as `value × factor`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub k: u8,
    pub plus: bool,
    pub factor: f64,
    pub decimals: Decimals,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Decimals {
    Fixed(u8),
    UpTo(u8),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Weapon,
    Armour,
    Jewellery,
    Quiver,
    Jewel,
    AbyssJewel,
    Flask,
    Tincture,
}

/// RePoE base `domain`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Domain {
    Item,
    Misc,
    AbyssJewel,
    Flask,
    Tincture,
}

impl Domain {
    /// The RePoE domain string mods of this domain carry.
    pub fn repoe(self) -> &'static str {
        match self {
            Domain::Item => "item",
            Domain::Misc => "misc",
            Domain::AbyssJewel => "abyss_jewel",
            Domain::Flask => "flask",
            Domain::Tincture => "tincture",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Prefix,
    Suffix,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Prefix, Side::Suffix];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::Prefix => "prefix",
            Side::Suffix => "suffix",
        }
    }
}

/// Bit order 0..5, the order `ClassData::influence_tags` is indexed by.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Influence {
    Shaper,
    Elder,
    Crusader,
    Redeemer,
    Hunter,
    Warlord,
}

impl Influence {
    pub const ALL: [Influence; 6] = [
        Influence::Shaper,
        Influence::Elder,
        Influence::Crusader,
        Influence::Redeemer,
        Influence::Hunter,
        Influence::Warlord,
    ];

    pub fn bit(self) -> u8 {
        1 << (self as u8)
    }

    pub fn name(self) -> &'static str {
        match self {
            Influence::Shaper => "Shaper",
            Influence::Elder => "Elder",
            Influence::Crusader => "Crusader",
            Influence::Redeemer => "Redeemer",
            Influence::Hunter => "Hunter",
            Influence::Warlord => "Warlord",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

impl Rarity {
    pub fn name(self) -> &'static str {
        match self {
            Rarity::Normal => "normal",
            Rarity::Magic => "magic",
            Rarity::Rare => "rare",
            Rarity::Unique => "unique",
        }
    }
}

/// The two eldritch implicit kinds.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Eldritch {
    Exarch,
    Eater,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClassData {
    pub id: String,
    pub name: String,
    pub category: Category,
    /// Indexed by `Influence` order; `None` = cannot be influenced.
    pub influence_tags: Option<[u16; 6]>,
    pub limits: Limits,
    pub max_sockets: u8,
}

/// `[prefixes, suffixes]` per rarity.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub magic: [u8; 2],
    pub rare: [u8; 2],
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BaseData {
    pub id: String,
    pub name: String,
    pub class: u16,
    pub domain: Domain,
    pub tags: Vec<u16>,
    pub implicits: Vec<u32>,
    pub drop_level: u8,
    /// Level, Str, Dex, Int.
    pub requirements: [u16; 4],
    /// Armour, evasion, energy shield, ward as (min, max).
    pub defences: [Option<[i32; 2]>; 4],
    pub prefix_delta: i8,
    pub suffix_delta: i8,
    /// `local_explicit_mod_effect_+%` of the base implicits (Simplex Amulet: 100).
    pub explicit_effect: i16,
    /// A base implicit carries `local_implicit_mod_cannot_be_changed`.
    pub implicits_locked: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ModKind {
    Natural,
    EssenceOnly,
    Crafted,
    Delve,
    Unveiled,
    Veiled,
    Mercenary,
    Ducat,
    BaseImplicit,
    CorruptedImplicit,
    Exarch,
    Eater,
    Synthesis,
    Enchant,
    FlaskEnkindling,
    FlaskInstilling,
}

impl ModKind {
    /// Kinds that sit in a prefix or suffix slot.
    pub fn is_explicit(self) -> bool {
        matches!(
            self,
            ModKind::Natural
                | ModKind::EssenceOnly
                | ModKind::Crafted
                | ModKind::Delve
                | ModKind::Unveiled
                | ModKind::Veiled
                | ModKind::Mercenary
                | ModKind::Ducat
        )
    }

    /// Kinds that sit among the implicits.
    pub fn is_implicit(self) -> bool {
        matches!(
            self,
            ModKind::BaseImplicit
                | ModKind::CorruptedImplicit
                | ModKind::Exarch
                | ModKind::Eater
                | ModKind::Synthesis
        )
    }

    /// Kinds that sit among the enchantments.
    pub fn is_enchant(self) -> bool {
        matches!(
            self,
            ModKind::Enchant | ModKind::FlaskEnkindling | ModKind::FlaskInstilling
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModData {
    pub id: String,
    pub name: String,
    pub kind: ModKind,
    pub side: Option<Side>,
    pub domain: String,
    pub groups: Vec<u16>,
    pub mod_type: u16,
    /// RePoE `implicit_tags`: the tags the attack/caster metamods and catalysts test.
    pub tags: Vec<u16>,
    pub adds_tags: Vec<u16>,
    /// Spawn weights in scan order: the first tag the item has wins.
    pub spawn: Vec<(u16, u32)>,
    /// Generation-weight percentages in scan order.
    pub generation: Vec<(u16, u32)>,
    pub level: u8,
    pub influence: Option<Influence>,
    pub elevated: bool,
    pub eldritch_tier: Option<u8>,
    pub stats: Vec<StatRange>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatRange {
    pub stat: u16,
    pub min: i32,
    pub max: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BenchAction {
    AddMod { m: u32 },
    AddEnchant { m: u32 },
    RemoveCrafted,
    RemoveEnchants,
    Sockets { count: u8 },
    Links { count: u8 },
    Colours { spec: String },
    RerollRarity,
    RerollRareMods { n: u8 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BenchData {
    /// Position in RePoE `crafting_bench_options.json`.
    pub index: u16,
    pub master: String,
    pub tier: u8,
    pub classes: Vec<u16>,
    pub cost: Vec<Cost>,
    pub action: BenchAction,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EssenceData {
    pub currency: String,
    pub name: String,
    pub level: u8,
    pub corruption_only: bool,
    pub allows_rare: bool,
    pub item_level_max: Option<u8>,
    /// (class, mod).
    pub mods: Vec<(u16, u32)>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FossilData {
    pub currency: String,
    pub name: String,
    pub more: Vec<(u16, u32)>,
    pub less: Vec<(u16, u32)>,
    pub added_mods: Vec<u32>,
    pub forced_mods: Vec<u32>,
    pub allowed_tags: Vec<u16>,
    pub forbidden_tags: Vec<u16>,
    pub corrupted_essence_chance: u8,
    pub mirrors: bool,
    pub rolls_lucky: bool,
    pub changes_quality: bool,
    pub rolls_white_sockets: bool,
    pub descriptions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatalystData {
    pub currency: String,
    pub name: String,
    /// The tooltip label: `Quality ({label} Modifiers)`.
    pub label: String,
    pub tags: Vec<u16>,
    /// Sinistral and Dextral catalysts enhance one side instead of a tag.
    pub side: Option<Side>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyData {
    pub id: String,
    pub name: String,
    pub class: String,
    pub description: String,
}

/// `key` = a currency metadata id, or `beast:<name>`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub key: String,
    pub amount: f64,
}

/// `Coe` = craftofexile.com; `Experiment` = an in-game test.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    GameText,
    Datamined,
    Wiki,
    Guide,
    PatchNotes,
    Owner,
    Coe,
    Experiment,
}

impl SourceKind {
    pub fn name(self) -> &'static str {
        match self {
            SourceKind::GameText => "game-text",
            SourceKind::Datamined => "datamined",
            SourceKind::Wiki => "wiki",
            SourceKind::Guide => "guide",
            SourceKind::PatchNotes => "patch-notes",
            SourceKind::Owner => "owner",
            SourceKind::Coe => "coe",
            SourceKind::Experiment => "experiment",
        }
    }
}

/// A game rule the model relies on, with where it comes from.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub id: String,
    pub statement: String,
    pub source_kind: SourceKind,
    pub url: String,
    pub quote: String,
    pub patch: String,
    pub checked: bool,
    /// Numeric payloads (count tables, ratios) copied as-is.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("bundle JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("bundle format {found}, expected {expected}; regenerate it")]
    Format { found: u32, expected: u32 },
}

/// The bundle plus lookup indexes.
#[derive(Clone, Debug)]
pub struct Data {
    pub bundle: Bundle,
    mod_index: HashMap<String, u32>,
    base_index: HashMap<String, u16>,
    base_names: HashMap<String, Vec<u16>>,
    tag_index: HashMap<String, u16>,
    group_index: HashMap<String, u16>,
    type_index: HashMap<String, u16>,
    /// A variant's `string` with each `{k}` replaced by `#` → (entry, variant).
    translation_keys: HashMap<String, Vec<(u32, u8)>>,
    /// The first entry listing the stat.
    translation_of_stat: HashMap<u16, u32>,
    method_index: HashMap<String, usize>,
    claim_index: HashMap<String, usize>,
    currency_by_name: HashMap<String, usize>,
    currency_by_id: HashMap<String, usize>,
}

impl std::ops::Deref for Data {
    type Target = Bundle;

    fn deref(&self) -> &Bundle {
        &self.bundle
    }
}

/// A translation string with every `{k}` replaced by `#`.
pub fn translation_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        match rest[open..].find('}') {
            Some(close) if rest[open + 1..open + close].parse::<u8>().is_ok() => {
                out.push_str(&rest[..open]);
                out.push('#');
                rest = &rest[open + close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

impl Data {
    pub fn from_json(text: &str) -> Result<Data, DataError> {
        Data::from_bundle(serde_json::from_str(text)?)
    }

    pub fn from_bundle(bundle: Bundle) -> Result<Data, DataError> {
        if bundle.format != FORMAT {
            return Err(DataError::Format {
                found: bundle.format,
                expected: FORMAT,
            });
        }
        let index = |names: &[String]| -> HashMap<String, u16> {
            names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), i as u16))
                .collect()
        };
        let mut base_names: HashMap<String, Vec<u16>> = HashMap::new();
        for (i, b) in bundle.bases.iter().enumerate() {
            base_names.entry(b.name.clone()).or_default().push(i as u16);
        }
        let mut translation_keys: HashMap<String, Vec<(u32, u8)>> = HashMap::new();
        let mut translation_of_stat = HashMap::new();
        for (e, t) in bundle.translations.iter().enumerate() {
            for (v, var) in t.variants.iter().enumerate() {
                translation_keys
                    .entry(translation_key(&var.string))
                    .or_default()
                    .push((e as u32, v as u8));
            }
            for &s in &t.stats {
                translation_of_stat.entry(s).or_insert(e as u32);
            }
        }
        let mut currency_by_name = HashMap::new();
        for (i, c) in bundle.currencies.iter().enumerate() {
            currency_by_name.entry(c.name.clone()).or_insert(i);
        }
        Ok(Data {
            mod_index: bundle
                .mods
                .iter()
                .enumerate()
                .map(|(i, m)| (m.id.clone(), i as u32))
                .collect(),
            base_index: bundle
                .bases
                .iter()
                .enumerate()
                .map(|(i, b)| (b.id.clone(), i as u16))
                .collect(),
            base_names,
            tag_index: index(&bundle.tags),
            group_index: index(&bundle.groups),
            type_index: index(&bundle.mod_types),
            translation_keys,
            translation_of_stat,
            method_index: bundle
                .methods
                .iter()
                .enumerate()
                .map(|(i, m)| (m.id.clone(), i))
                .collect(),
            claim_index: bundle
                .claims
                .iter()
                .enumerate()
                .map(|(i, c)| (c.id.clone(), i))
                .collect(),
            currency_by_name,
            currency_by_id: bundle
                .currencies
                .iter()
                .enumerate()
                .map(|(i, c)| (c.id.clone(), i))
                .collect(),
            bundle,
        })
    }

    pub fn mod_at(&self, m: u32) -> &ModData {
        &self.bundle.mods[m as usize]
    }

    pub fn mod_by_id(&self, id: &str) -> Option<u32> {
        self.mod_index.get(id).copied()
    }

    pub fn base_at(&self, b: u16) -> &BaseData {
        &self.bundle.bases[b as usize]
    }

    pub fn base_by_id(&self, id: &str) -> Option<u16> {
        self.base_index.get(id).copied()
    }

    /// Every base with this display name, in bundle order.
    pub fn bases_named(&self, name: &str) -> &[u16] {
        self.base_names
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn class_of(&self, base: u16) -> &ClassData {
        &self.bundle.classes[self.base_at(base).class as usize]
    }

    pub fn class_by_id(&self, id: &str) -> Option<u16> {
        self.bundle
            .classes
            .iter()
            .position(|c| c.id == id)
            .map(|i| i as u16)
    }

    pub fn tag(&self, t: u16) -> &str {
        &self.bundle.tags[t as usize]
    }

    pub fn tag_by_name(&self, name: &str) -> Option<u16> {
        self.tag_index.get(name).copied()
    }

    pub fn group(&self, g: u16) -> &str {
        &self.bundle.groups[g as usize]
    }

    pub fn group_by_name(&self, name: &str) -> Option<u16> {
        self.group_index.get(name).copied()
    }

    pub fn mod_type(&self, t: u16) -> &str {
        &self.bundle.mod_types[t as usize]
    }

    pub fn mod_type_by_name(&self, name: &str) -> Option<u16> {
        self.type_index.get(name).copied()
    }

    pub fn stat(&self, s: u16) -> &str {
        &self.bundle.stats[s as usize]
    }

    pub fn stat_by_name(&self, name: &str) -> Option<u16> {
        self.bundle
            .stats
            .iter()
            .position(|s| s == name)
            .map(|i| i as u16)
    }

    /// Every (entry, variant) whose string reads as `key` (values replaced by `#`).
    pub fn translations_keyed(&self, key: &str) -> &[(u32, u8)] {
        self.translation_keys
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The first translation entry that lists `stat`.
    pub fn translation_of_stat(&self, stat: u16) -> Option<u32> {
        self.translation_of_stat.get(&stat).copied()
    }

    pub fn method(&self, id: &str) -> Option<&crate::method::Method> {
        self.method_index.get(id).map(|&i| &self.bundle.methods[i])
    }

    pub fn claim(&self, id: &str) -> Option<&Claim> {
        self.claim_index.get(id).map(|&i| &self.bundle.claims[i])
    }

    /// The first currency with this name.
    pub fn currency_by_name(&self, name: &str) -> Option<&CurrencyData> {
        self.currency_by_name
            .get(name)
            .map(|&i| &self.bundle.currencies[i])
    }

    pub fn currency_by_id(&self, id: &str) -> Option<&CurrencyData> {
        self.currency_by_id
            .get(id)
            .map(|&i| &self.bundle.currencies[i])
    }

    pub fn catalyst_by_name(&self, name: &str) -> Option<u8> {
        self.bundle
            .catalysts
            .iter()
            .position(|c| c.name == name)
            .map(|i| i as u8)
    }
}
