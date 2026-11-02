//! The game-data bundle produced by `datagen`.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::item::Side;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("bundle JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub repo: String,
    pub commit: String,
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ModStat {
    pub stat: u16,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModData {
    pub id: String,
    #[serde(rename = "gen")]
    pub generation: Side,
    #[serde(default)]
    pub crafted: bool,
    pub group: u16,
    pub name: String,
    pub ilvl: u16,
    pub stats: Vec<ModStat>,
    pub text: String,
    pub implicit_tags: Vec<String>,
    pub influence: bool,
    /// Spawn weights in the order the game scans them: first tag present in the item's tag set wins.
    pub weights: Vec<(u16, u32)>,
    /// `implicit_tags`, sorted, for binary-search matching against a catalyst's tags.
    #[serde(skip)]
    pub implicit_tags_sorted: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaseData {
    pub key: String,
    pub name: String,
    pub class: String,
    pub drop_level: u8,
    pub tags: Vec<u16>,
    pub prefix_delta: i16,
    pub suffix_delta: i16,
    pub explicit_effect: i16,
    #[serde(default)]
    pub implicit_text: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClassData {
    pub name: String,
    pub influence_tags: BTreeMap<String, u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BenchEntry {
    #[serde(rename = "mod")]
    pub mod_id: String,
    pub classes: Vec<String>,
    pub divines: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalyst {
    pub name: String,
    pub price_key: String,
    pub label: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Translation {
    pub key: String,
    pub stat: u16,
    pub negate: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bundle {
    pub format: u32,
    pub patch: String,
    pub source: Source,
    pub tags: Vec<String>,
    pub groups: Vec<String>,
    pub stat_ids: Vec<String>,
    pub mods: Vec<ModData>,
    pub bases: Vec<BaseData>,
    pub classes: Vec<ClassData>,
    pub bench: Vec<BenchEntry>,
    pub catalysts: Vec<Catalyst>,
    pub translations: Vec<Translation>,
    pub trade_stats: BTreeMap<String, String>,

    #[serde(skip)]
    pub mods_by_id: HashMap<String, u32>,
    #[serde(skip)]
    pub bases_by_name: HashMap<String, u16>,
    #[serde(skip)]
    pub bases_by_key: HashMap<String, u16>,
    #[serde(skip)]
    pub classes_by_name: HashMap<String, u16>,
    #[serde(skip)]
    pub translations_by_key: HashMap<String, Vec<u32>>,
    #[serde(skip)]
    pub bench_by_mod: HashMap<String, u16>,
}

impl Bundle {
    pub fn from_json(json: &str) -> Result<Bundle, DataError> {
        let mut bundle: Bundle = serde_json::from_str(json)?;
        bundle.build_indexes();
        Ok(bundle)
    }

    fn build_indexes(&mut self) {
        for (i, m) in self.mods.iter_mut().enumerate() {
            m.implicit_tags_sorted = m.implicit_tags.clone();
            m.implicit_tags_sorted.sort();
            m.implicit_tags_sorted.dedup();
            self.mods_by_id.insert(m.id.clone(), i as u32);
        }
        for (i, b) in self.bases.iter().enumerate() {
            self.bases_by_name.insert(b.name.clone(), i as u16);
            self.bases_by_key.insert(b.key.clone(), i as u16);
        }
        for (i, c) in self.classes.iter().enumerate() {
            self.classes_by_name.insert(c.name.clone(), i as u16);
        }
        for (i, t) in self.translations.iter().enumerate() {
            self.translations_by_key
                .entry(t.key.clone())
                .or_default()
                .push(i as u32);
        }
        for (i, b) in self.bench.iter().enumerate() {
            self.bench_by_mod.insert(b.mod_id.clone(), i as u16);
        }
    }

    pub fn tag(&self, index: u16) -> &str {
        self.tags
            .get(index as usize)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn group(&self, index: u16) -> &str {
        self.groups
            .get(index as usize)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn stat_id(&self, index: u16) -> &str {
        self.stat_ids
            .get(index as usize)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn mod_at(&self, index: u32) -> &ModData {
        &self.mods[index as usize]
    }

    pub fn base_at(&self, index: u16) -> &BaseData {
        &self.bases[index as usize]
    }

    pub fn mod_by_id(&self, id: &str) -> Option<u32> {
        self.mods_by_id.get(id).copied()
    }

    pub fn base_by_name(&self, name: &str) -> Option<u16> {
        self.bases_by_name.get(name).copied()
    }

    pub fn class_by_name(&self, name: &str) -> Option<&ClassData> {
        self.classes_by_name
            .get(name)
            .map(|i| &self.classes[*i as usize])
    }

    pub fn catalyst_by_label(&self, label: &str) -> Option<u8> {
        self.catalysts
            .iter()
            .position(|c| c.label.eq_ignore_ascii_case(label))
            .map(|i| i as u8)
    }

    pub fn catalyst_by_name(&self, name: &str) -> Option<u8> {
        self.catalysts
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
            .map(|i| i as u8)
    }
}
