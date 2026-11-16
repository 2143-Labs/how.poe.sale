//! The shield's pool: the base, its 204 rollable modifiers, and the goal's modifier lists.
//!
//! The file `data/shield.json` is generated once by `data/gen_shield.py` from the pinned export and
//! compiled in, so there is no runtime file to fail on: [`data`] either returns the pool or panics
//! with the file's name and the reason it could not be read.
//!
//! A modifier's weight is stored per tag, not as a single number, because a pool is always read
//! for a particular base: [`Data::weight_on`] sums the weights of the tags this base carries. In
//! this export every rollable modifier has exactly one positive weight among those tags, which the
//! generator asserts, so the sum is the weight the game would use.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::item::Side;

/// The compiled-in pool.
pub fn data() -> &'static Data {
    static DATA: LazyLock<Data> = LazyLock::new(|| {
        Data::from_json(SOURCE)
            .unwrap_or_else(|why| panic!("crates/simple/data/shield.json: {why}"))
    });
    &DATA
}

const SOURCE: &str = include_str!("../data/shield.json");

/// The base the pool is read on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub name: String,
    pub item_level: u32,
    /// The tags a modifier's spawn weights are matched against, in the file's order. An index into
    /// this list is how a [`Mod`] stores a weight's tag.
    pub tags: Vec<String>,
}

/// One modifier the base can roll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mod {
    pub id: String,
    pub side: Side,
    pub level: u32,
    /// Interned group ids: two modifiers in one group cannot sit on the same item.
    pub groups: Vec<u16>,
    /// `(tag index into [`Base::tags`], weight)`, positive weights only.
    pub weights: Vec<(u16, u128)>,
    pub text: String,
}

/// The goal, as the data file states it: the ordered grade floors per side, and the modifier ids
/// each grade is made of. The floors are names here and are parsed by [`crate::target::Target`], so
/// this module does not have to know what a grade is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSpec {
    pub prefixes: Vec<String>,
    pub suffixes: Vec<String>,
    pub desired: Vec<u16>,
    pub good: Vec<u16>,
}

/// The pool, indexed by modifier id.
#[derive(Clone, Debug)]
pub struct Data {
    pub base: Base,
    pub mods: Vec<Mod>,
    pub target: TargetSpec,
    ids: HashMap<String, u16>,
    groups: Vec<String>,
}

impl Data {
    /// Reads a data file, interning its ids and group names.
    pub fn from_json(source: &str) -> Result<Data, String> {
        let raw: Raw = serde_json::from_str(source).map_err(|e| e.to_string())?;

        let base = Base {
            name: raw.base.name,
            item_level: raw.base.item_level,
            tags: raw.base.tags,
        };
        if base.tags.is_empty() {
            return Err("the base has no tags".to_string());
        }
        for (i, tag) in base.tags.iter().enumerate() {
            if base.tags[..i].contains(tag) {
                return Err(format!("the base lists tag {tag:?} twice"));
            }
        }

        let mut mods = Vec::with_capacity(raw.mods.len());
        let mut ids: HashMap<String, u16> = HashMap::new();
        let mut groups: Vec<String> = Vec::new();
        for raw_mod in raw.mods {
            let side = Side::from_name(&raw_mod.side)
                .ok_or_else(|| format!("{} is not a prefix or a suffix", raw_mod.id))?;
            let index = u16::try_from(mods.len())
                .map_err(|_| "the pool does not fit in 16-bit modifier ids".to_string())?;
            if ids.insert(raw_mod.id.clone(), index).is_some() {
                return Err(format!("{} appears twice", raw_mod.id));
            }
            if raw_mod.weights.is_empty() {
                return Err(format!("{} has no spawn weight", raw_mod.id));
            }
            let mut weights = Vec::with_capacity(raw_mod.weights.len());
            for (tag, weight) in raw_mod.weights {
                let tag = base.tags.iter().position(|t| *t == tag).ok_or_else(|| {
                    format!("{} has a weight for the unknown tag {tag}", raw_mod.id)
                })?;
                let tag = u16::try_from(tag).map_err(|_| "too many tags".to_string())?;
                if weight == 0 {
                    return Err(format!("{} has a zero spawn weight", raw_mod.id));
                }
                weights.push((tag, weight));
            }
            let mut interned = Vec::with_capacity(raw_mod.groups.len());
            for name in raw_mod.groups {
                let group = match groups.iter().position(|g| *g == name) {
                    Some(index) => index,
                    None => {
                        groups.push(name);
                        groups.len() - 1
                    }
                };
                let group = u16::try_from(group).map_err(|_| "too many groups".to_string())?;
                if !interned.contains(&group) {
                    interned.push(group);
                }
            }
            mods.push(Mod {
                id: raw_mod.id,
                side,
                level: raw_mod.level,
                groups: interned,
                weights,
                text: raw_mod.text,
            });
        }
        if mods.is_empty() {
            return Err("the pool is empty".to_string());
        }

        let lookup = |id: &str, what: &str| {
            ids.get(id)
                .copied()
                .ok_or_else(|| format!("the target names {id} as {what}, but the pool lacks it"))
        };
        let target = TargetSpec {
            prefixes: raw.target.prefixes,
            suffixes: raw.target.suffixes,
            desired: raw
                .target
                .desired
                .iter()
                .map(|id| lookup(id, "desired"))
                .collect::<Result<_, _>>()?,
            good: raw
                .target
                .good
                .iter()
                .map(|id| lookup(id, "good"))
                .collect::<Result<_, _>>()?,
        };

        Ok(Data {
            base,
            mods,
            target,
            ids,
            groups,
        })
    }

    pub fn mod_at(&self, m: u16) -> &Mod {
        &self.mods[m as usize]
    }

    pub fn id_of(&self, id: &str) -> Option<u16> {
        self.ids.get(id).copied()
    }

    /// The interned group's name, for printing and for tests.
    pub fn group_name(&self, group: u16) -> &str {
        &self.groups[group as usize]
    }

    /// The modifier's weight on this base: what it contributes to its side's pool.
    pub fn weight_on(&self, m: u16) -> u128 {
        self.mod_at(m)
            .weights
            .iter()
            .map(|&(_, weight)| weight)
            .sum()
    }
}

#[derive(Deserialize)]
struct Raw {
    base: RawBase,
    mods: Vec<RawMod>,
    target: RawTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBase {
    name: String,
    item_level: u32,
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct RawMod {
    id: String,
    #[serde(rename = "type")]
    side: String,
    level: u32,
    groups: Vec<String>,
    weights: std::collections::BTreeMap<String, u128>,
    text: String,
}

#[derive(Deserialize)]
struct RawTarget {
    prefixes: Vec<String>,
    suffixes: Vec<String>,
    desired: Vec<String>,
    good: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_is_the_shields() {
        let data = data();
        assert_eq!(data.base.name, "Supreme Spiked Shield");
        assert_eq!(data.base.item_level, 87);
        assert!(data.base.tags.iter().any(|t| t == "shield_shaper"));

        let prefixes: Vec<&Mod> = data
            .mods
            .iter()
            .filter(|m| m.side == Side::Prefix)
            .collect();
        let suffixes: Vec<&Mod> = data
            .mods
            .iter()
            .filter(|m| m.side == Side::Suffix)
            .collect();
        assert_eq!(prefixes.len(), 73, "the shield rolls 73 prefixes");
        assert_eq!(suffixes.len(), 131, "the shield rolls 131 suffixes");
        assert_eq!(data.mods.len(), 204);

        let total = |side| -> u128 {
            data.mods
                .iter()
                .filter(|m| m.side == side)
                .map(|m| data.weight_on(data.id_of(&m.id).unwrap()))
                .sum()
        };
        assert_eq!(total(Side::Prefix), 64800);
        assert_eq!(total(Side::Suffix), 96475);
    }

    #[test]
    fn every_target_modifier_is_in_the_pool() {
        let data = data();
        assert_eq!(data.target.desired.len(), 1);
        assert!(!data.target.good.is_empty());
        assert_eq!(data.target.prefixes, ["desired", "good", "any"]);
        assert_eq!(data.target.suffixes, ["good", "good", "any"]);
        for m in data.target.desired.iter().chain(&data.target.good) {
            assert!(
                data.mods.iter().any(|entry| entry.id == data.mod_at(*m).id),
                "a target modifier is not in the pool"
            );
        }
    }
}
