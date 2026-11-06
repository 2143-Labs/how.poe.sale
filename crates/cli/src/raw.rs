//! The raw RePoE export shapes `cli` reads (copied from v1 `datagen` and extended), and
//! the small name helpers both crates share by copy.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct RawMod {
    pub domain: String,
    pub generation_type: String,
    #[serde(default)]
    pub generation_weights: Vec<RawWeight>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub implicit_tags: Vec<String>,
    #[serde(default)]
    pub adds_tags: Vec<String>,
    #[serde(default)]
    pub is_essence_only: bool,
    #[serde(default)]
    pub name: Option<String>,
    pub required_level: u16,
    #[serde(default)]
    pub spawn_weights: Vec<RawWeight>,
    #[serde(default)]
    pub stats: Vec<RawStat>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Deserialize)]
pub struct RawWeight {
    pub tag: String,
    pub weight: u32,
}

#[derive(Deserialize)]
pub struct RawStat {
    pub id: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Deserialize)]
pub struct RawBase {
    pub domain: String,
    #[serde(default)]
    pub implicits: Vec<String>,
    pub item_class: String,
    pub name: String,
    pub drop_level: u8,
    pub release_state: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub requirements: Option<RawRequirements>,
    #[serde(default)]
    pub properties: RawProperties,
}

#[derive(Deserialize, Default)]
pub struct RawRequirements {
    #[serde(default)]
    pub level: u16,
    #[serde(default)]
    pub strength: u16,
    #[serde(default)]
    pub dexterity: u16,
    #[serde(default)]
    pub intelligence: u16,
}

#[derive(Deserialize, Default)]
pub struct RawProperties {
    #[serde(default)]
    pub armour: Option<RawRange>,
    #[serde(default)]
    pub evasion: Option<RawRange>,
    #[serde(default)]
    pub energy_shield: Option<RawRange>,
    #[serde(default)]
    pub ward: Option<RawRange>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub directions: Option<String>,
}

#[derive(Deserialize)]
pub struct RawRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Deserialize)]
pub struct RawClass {
    pub name: String,
    #[serde(default)]
    pub influence_tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct RawBench {
    pub actions: RawBenchActions,
    #[serde(default)]
    pub cost: BTreeMap<String, f64>,
    #[serde(default)]
    pub item_classes: Vec<String>,
    pub master: String,
    pub bench_tier: u8,
}

#[derive(Deserialize)]
pub struct RawBenchActions {
    #[serde(default)]
    pub add_explicit_mod: Option<String>,
    #[serde(default)]
    pub add_enchant_mod: Option<String>,
    #[serde(default)]
    pub color_sockets: Option<String>,
    #[serde(default)]
    pub change_socket_count: Option<u8>,
    #[serde(default)]
    pub link_sockets: Option<u8>,
    #[serde(default)]
    pub reroll_rare_mods: Option<u8>,
    #[serde(default)]
    pub remove_crafted_mods: Option<bool>,
    #[serde(default)]
    pub remove_enchantments: Option<bool>,
    #[serde(default)]
    pub reroll_rarity: Option<bool>,
}

#[derive(Deserialize)]
pub struct RawTranslation {
    #[serde(rename = "English", default)]
    pub english: Option<Vec<RawEnglish>>,
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

impl RawTranslation {
    pub fn ids(&self) -> &[String] {
        self.ids.as_deref().unwrap_or_default()
    }
}

#[derive(Deserialize)]
pub struct RawEnglish {
    pub string: String,
    #[serde(default)]
    pub condition: Option<Vec<RawCondition>>,
    #[serde(default)]
    pub format: Option<Vec<String>>,
    #[serde(default)]
    pub index_handlers: Option<Vec<Vec<String>>>,
}

#[derive(Deserialize)]
pub struct RawCondition {
    #[serde(default)]
    pub min: Option<i32>,
    #[serde(default)]
    pub max: Option<i32>,
    #[serde(default)]
    pub negated: Option<bool>,
}

#[derive(Deserialize)]
pub struct RawEssence {
    pub name: String,
    pub level: u8,
    #[serde(default)]
    pub item_level_restriction: Option<u8>,
    #[serde(default)]
    pub mods: BTreeMap<String, Option<String>>,
    #[serde(rename = "type")]
    pub kind: RawEssenceType,
}

#[derive(Deserialize)]
pub struct RawEssenceType {
    #[serde(default)]
    pub is_corruption_only: bool,
}

#[derive(Deserialize)]
pub struct RawFossil {
    pub name: String,
    #[serde(default)]
    pub added_mods: Vec<String>,
    #[serde(default)]
    pub forced_mods: Vec<String>,
    #[serde(default)]
    pub positive_mod_weights: Vec<RawWeight>,
    #[serde(default)]
    pub negative_mod_weights: Vec<RawWeight>,
    #[serde(default)]
    pub forbidden_tags: Vec<String>,
    #[serde(default)]
    pub allowed_tags: Vec<String>,
    #[serde(default)]
    pub corrupted_essence_chance: u8,
    #[serde(default)]
    pub mirrors: bool,
    #[serde(default)]
    pub rolls_lucky: bool,
    #[serde(default)]
    pub changes_quality: bool,
    #[serde(default)]
    pub rolls_white_sockets: bool,
    #[serde(default)]
    pub descriptions: BTreeMap<String, String>,
}

/// The `{k}` placeholders of a translation string, in text order. `None` when one is out of
/// range. A repeated placeholder reuses its slot: the same stat, shown twice.
pub fn placeholders(s: &str, n_ids: usize) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}')? + open;
        let k: usize = rest[open + 1..close].parse().ok()?;
        if k >= n_ids {
            return None;
        }
        out.push(k);
        rest = &rest[close + 1..];
    }
    Some(out)
}

/// The factor a RePoE `index_handlers` entry applies to a stored value before display.
/// `Some(None)`: the handler shows something other than a number. `None`: an unknown handler.
pub fn handler_factor(name: &str) -> Option<Option<f64>> {
    let f = match name {
        "negate" => -1.0,
        "canonical_stat" => 1.0,
        "divide_by_one_hundred"
        | "divide_by_one_hundred_2dp_if_required"
        | "divide_by_one_hundred_2dp" => 0.01,
        "divide_by_one_hundred_and_negate" => -0.01,
        "milliseconds_to_seconds"
        | "milliseconds_to_seconds_2dp_if_required"
        | "milliseconds_to_seconds_2dp"
        | "milliseconds_to_seconds_0dp"
        | "milliseconds_to_seconds_1dp"
        | "divide_by_one_thousand" => 0.001,
        "per_minute_to_per_second"
        | "per_minute_to_per_second_2dp_if_required"
        | "per_minute_to_per_second_2dp"
        | "per_minute_to_per_second_1dp"
        | "per_minute_to_per_second_0dp" => 1.0 / 60.0,
        "permyriad_per_minute_to_%_per_second" => 1.0 / 6000.0,
        "divide_by_ten_1dp_if_required"
        | "divide_by_ten_0dp"
        | "divide_by_ten_1dp"
        | "locations_to_metres"
        | "deciseconds_to_seconds"
        | "divide_by_twenty_then_double_0dp" => 0.1,
        "double" => 2.0,
        "negate_and_double" => -2.0,
        "times_twenty" => 20.0,
        "times_one_point_five" => 1.5,
        "old_leech_percent" | "divide_by_five" => 0.2,
        "old_leech_permyriad" => 0.002,
        "divide_by_four" => 0.25,
        "divide_by_three" => 1.0 / 3.0,
        "divide_by_six" => 1.0 / 6.0,
        "divide_by_twelve" => 1.0 / 12.0,
        "divide_by_fifteen_0dp" => 1.0 / 15.0,
        "divide_by_twenty" => 0.05,
        "divide_by_two_0dp" => 0.5,
        "30%_of_value" => 0.3,
        "60%_of_value" => 0.6,
        "passive_hash"
        | "affliction_reward_type"
        | "display_indexable_non_active_support"
        | "display_indexable_support"
        | "display_indexable_skill"
        | "mod_value_to_item_class"
        | "multiplicative_damage_modifier"
        | "plus_two_hundred"
        | "tree_expansion_jewel_passive"
        | "weapon_tree_unique_base_type_name" => return Some(None),
        _ => return None,
    };
    Some(Some(f))
}

/// tag suffix -> influence, per the class influence tags in `item_classes.json`.
pub fn influence_of_tag(tag: &str) -> Option<poe1::data::Influence> {
    use poe1::data::Influence;
    Some(match tag.rsplit('_').next()? {
        "crusader" => Influence::Crusader,
        "elder" => Influence::Elder,
        "basilisk" => Influence::Hunter,
        "eyrie" => Influence::Redeemer,
        "shaper" => Influence::Shaper,
        "adjudicator" => Influence::Warlord,
        _ => return None,
    })
}
