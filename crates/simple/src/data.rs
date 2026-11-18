//! The shield's data: the tag table, the base, the pool's modifiers and the goal's modifier lists.
//!
//! The file `data/shield.json` is generated once by `data/gen_shield.py` from the pinned export and
//! compiled in, so there is no runtime file to fail on: [`data`] either returns the data or panics
//! with the file's name and the reason it could not be read.
//!
//! There is one base. Its influence is not a property of the base but of the item: the Shaper
//! version of the shield is this base with an [`Influence`] on the item, and an influence
//! contributes one extra tag (`shield_shaper` for the Shaper). A pool is therefore always read for
//! a base *and* an influence: [`Data::weight_on`] reads the modifier's weight off the base's tags
//! plus the influence's tag. That is the whole of the difference between the 204-modifier Shaper
//! pool and the 193-modifier plain one.
//!
//! A modifier's spawn weights are an ordered list, and the **first** entry whose tag the item
//! carries decides: its weight is the modifier's weight, and a zero there means the modifier
//! cannot roll, whatever a later entry says. This is the game's rule as RePoE documents it
//! (`RePoE/docs/mods.md`: "The array elements are evaluated in order, the first element with a
//! tag the item in question has is taken"). The file keeps each modifier's entries in that order,
//! zeros included.
//!
//! The tag table carries `shield_shaper` even though no base lists it, because an influence finds
//! its tag by name.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::item::{Influence, Side};

/// The compiled-in data file.
pub fn data() -> &'static Data {
    static DATA: LazyLock<Data> = LazyLock::new(|| {
        Data::from_json(SOURCE).unwrap_or_else(|why| panic!("{SOURCE_NAME}: {why}"))
    });
    &DATA
}

const SOURCE: &str = include_str!("../data/shield.json");
const SOURCE_NAME: &str = "crates/simple/data/shield.json";

/// The implicit every base of this shield carries.
const BASE_IMPLICIT: &str = "ChanceToDodgeSpellsImplicitShield2";
const BASE_IMPLICIT_STAT: &str = "base_spell_suppression_chance_%";

/// Which of the three buckets a modifier belongs to.
///
/// Only a prefix and a suffix sit on an item's explicit sides; an implicit is a property of the
/// base. [`ModKind::side`] therefore says `None` for an implicit, and [`ModKind::is_implicit`] is
/// the same test spelled for a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModKind {
    Prefix,
    Suffix,
    Implicit,
}

impl ModKind {
    /// The side this kind occupies, or `None` for an implicit.
    pub fn side(self) -> Option<Side> {
        match self {
            ModKind::Prefix => Some(Side::Prefix),
            ModKind::Suffix => Some(Side::Suffix),
            ModKind::Implicit => None,
        }
    }

    pub fn is_implicit(self) -> bool {
        self == ModKind::Implicit
    }

    /// The data file's name for this kind.
    pub fn name(self) -> &'static str {
        match self {
            ModKind::Prefix => "prefix",
            ModKind::Suffix => "suffix",
            ModKind::Implicit => "implicit",
        }
    }

    fn from_name(name: &str) -> Option<ModKind> {
        match name {
            "prefix" => Some(ModKind::Prefix),
            "suffix" => Some(ModKind::Suffix),
            "implicit" => Some(ModKind::Implicit),
            _ => None,
        }
    }
}

/// The item's base. Its influence lives on the item, so this holds only the plain tags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub id: String,
    pub name: String,
    pub item_class: String,
    pub drop_level: u32,
    /// Interned tag ids, in the file's order.
    pub tags: Vec<u16>,
    pub requirements: Requirements,
    /// Armour, evasion and energy shield, each as `(min, max)` or absent.
    pub defences: [Option<(i32, i32)>; 3],
    /// The base's implicit modifiers, as modifier ids.
    pub implicits: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirements {
    pub level: u32,
    pub strength: u32,
    pub dexterity: u32,
    pub intelligence: u32,
}

/// One modifier: an explicit prefix or suffix, or a base implicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mod {
    pub id: String,
    /// The modifier's affix name, empty for most implicits in the export.
    pub name: String,
    pub kind: ModKind,
    /// The required item level.
    pub level: u32,
    /// Interned group ids: two modifiers in one group cannot sit on the same item.
    pub groups: Vec<u16>,
    /// `(interned tag id, weight)` in the game's order, zeros included: the first entry whose tag
    /// an item carries is the modifier's weight on it ([`Data::weight_on`]).
    pub weights: Vec<(u16, u128)>,
    /// `(interned stat id, min, max)`, in the export's stat order: a held affix's rolls follow it.
    pub stats: Vec<(u16, i32, i32)>,
    pub implicit_tags: Vec<u16>,
    pub adds_tags: Vec<u16>,
    pub essence_only: bool,
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

/// The whole data file, with its string tables interned to `u16`.
#[derive(Clone, Debug)]
pub struct Data {
    /// The global tag table, in first-seen order. It carries the influence's tag as well as the
    /// base's.
    pub tags: Vec<String>,
    pub bases: Vec<Base>,
    /// Explicit modifiers first, then the base implicits.
    pub mods: Vec<Mod>,
    /// The stat table, interned from every modifier's stats in first-seen order.
    pub stat_ids: Vec<String>,
    pub target: TargetSpec,
    ids: HashMap<String, u16>,
    groups: Vec<String>,
}

impl Data {
    /// Reads the data file, interning its tags, groups and stat ids and re-checking the pinned
    /// pools. A malformed file is an error naming the reason; [`data`] turns it into a panic.
    pub fn from_json(source: &str) -> Result<Data, String> {
        let Raw {
            tags: raw_tags,
            bases: raw_bases,
            mods: raw_mods,
            target: raw_target,
        } = serde_json::from_str(source).map_err(|e| e.to_string())?;

        if raw_tags.is_empty() {
            return Err("the tag table is empty".to_string());
        }
        if raw_tags.len() > u16::MAX as usize {
            return Err("the tag table does not fit in 16 bits".to_string());
        }
        let mut tag_index: HashMap<String, u16> = HashMap::new();
        for (index, name) in raw_tags.iter().enumerate() {
            if tag_index.insert(name.clone(), index as u16).is_some() {
                return Err(format!("the tag table lists {name:?} twice"));
            }
        }

        // The tags a base can carry: a weight on anything else is not part of its pool.
        let mut base_tags: Vec<u16> = Vec::new();
        for raw_base in &raw_bases {
            for name in &raw_base.tags {
                let tag = resolve_tag(&tag_index, name, &raw_base.id)?;
                if !base_tags.contains(&tag) {
                    base_tags.push(tag);
                }
            }
        }
        if base_tags.is_empty() {
            return Err("the base carries no tags".to_string());
        }

        let mut groups: Vec<String> = Vec::new();
        let mut group_index: HashMap<String, u16> = HashMap::new();
        let mut stat_ids: Vec<String> = Vec::new();
        let mut stat_index: HashMap<String, u16> = HashMap::new();
        let mut mods: Vec<Mod> = Vec::new();
        let mut ids: HashMap<String, u16> = HashMap::new();

        for raw_mod in raw_mods {
            let RawMod {
                id,
                name,
                kind,
                level,
                groups: raw_groups,
                weights: raw_weights,
                stats: raw_stats,
                implicit_tags: raw_implicit_tags,
                adds_tags: raw_adds_tags,
                essence_only,
                text,
            } = raw_mod;

            if id.is_empty() {
                return Err("a modifier has an empty id".to_string());
            }
            let index = u16::try_from(mods.len())
                .map_err(|_| "the pool does not fit in 16-bit modifier ids".to_string())?;
            if ids.insert(id.clone(), index).is_some() {
                return Err(format!("{id} appears twice"));
            }
            let kind = ModKind::from_name(&kind)
                .ok_or_else(|| format!("{id} has the unknown kind {kind:?}"))?;

            if raw_stats.is_empty() {
                return Err(format!("{id} has no stats"));
            }
            let mut stats = Vec::with_capacity(raw_stats.len());
            for raw_stat in raw_stats {
                if raw_stat.id.is_empty() {
                    return Err(format!("{id} has a stat with an empty id"));
                }
                if raw_stat.min > raw_stat.max {
                    return Err(format!(
                        "{id} has the stat {} with min {} above max {}",
                        raw_stat.id, raw_stat.min, raw_stat.max
                    ));
                }
                let stat = intern(&mut stat_ids, &mut stat_index, raw_stat.id, "stat ids")?;
                stats.push((stat, raw_stat.min, raw_stat.max));
            }

            let mut mod_groups = Vec::with_capacity(raw_groups.len());
            for group in raw_groups {
                let group = intern(&mut groups, &mut group_index, group, "groups")?;
                if !mod_groups.contains(&group) {
                    mod_groups.push(group);
                }
            }

            let mut weights = Vec::with_capacity(raw_weights.len());
            for (tag, weight) in raw_weights {
                let name = tag;
                let tag = resolve_tag(&tag_index, &name, &id)?;
                if weights.iter().any(|&(listed, _)| listed == tag) {
                    return Err(format!("{id} lists a spawn weight for {name:?} twice"));
                }
                weights.push((tag, weight));
            }

            let mut implicit_tags = Vec::with_capacity(raw_implicit_tags.len());
            for tag in raw_implicit_tags {
                implicit_tags.push(resolve_tag(&tag_index, &tag, &id)?);
            }
            let mut adds_tags = Vec::with_capacity(raw_adds_tags.len());
            for tag in raw_adds_tags {
                adds_tags.push(resolve_tag(&tag_index, &tag, &id)?);
            }

            mods.push(Mod {
                id,
                name,
                kind,
                level,
                groups: mod_groups,
                weights,
                stats,
                implicit_tags,
                adds_tags,
                essence_only,
                text,
            });
        }
        if mods.is_empty() {
            return Err("the pool is empty".to_string());
        }

        let mut bases: Vec<Base> = Vec::with_capacity(raw_bases.len());
        for raw_base in raw_bases {
            let RawBase {
                id,
                name,
                item_class,
                drop_level,
                tags: raw_base_tags,
                requirements,
                defences,
                implicits: raw_implicits,
            } = raw_base;

            if id.is_empty() {
                return Err("a base has an empty id".to_string());
            }
            let mut base_tag_ids = Vec::with_capacity(raw_base_tags.len());
            for tag in raw_base_tags {
                let tag = resolve_tag(&tag_index, &tag, &id)?;
                if base_tag_ids.contains(&tag) {
                    return Err(format!("{id} lists a tag twice"));
                }
                base_tag_ids.push(tag);
            }
            if base_tag_ids.is_empty() {
                return Err(format!("{id} has no tags"));
            }
            let mut implicits = Vec::with_capacity(raw_implicits.len());
            for implicit in raw_implicits {
                let m = ids
                    .get(&implicit)
                    .copied()
                    .ok_or_else(|| format!("{id} names the unknown implicit {implicit:?}"))?;
                if !mods[m as usize].kind.is_implicit() {
                    return Err(format!(
                        "{id} names the explicit modifier {implicit} as implicit"
                    ));
                }
                if !implicits.contains(&m) {
                    implicits.push(m);
                }
            }
            if implicits.is_empty() {
                return Err(format!("{id} has no implicit modifiers"));
            }

            bases.push(Base {
                id,
                name,
                item_class,
                drop_level,
                tags: base_tag_ids,
                requirements: Requirements {
                    level: requirements.level,
                    strength: requirements.strength,
                    dexterity: requirements.dexterity,
                    intelligence: requirements.intelligence,
                },
                defences: [defences.armour, defences.evasion, defences.energy_shield],
                implicits,
            });
        }
        if bases.len() != 1 {
            return Err(format!(
                "the file holds {} bases, and this data is for exactly one",
                bases.len()
            ));
        }

        let desired = resolve_targets(&ids, &raw_target.desired, "desired")?;
        let good = resolve_targets(&ids, &raw_target.good, "good")?;
        let target = TargetSpec {
            prefixes: raw_target.prefixes,
            suffixes: raw_target.suffixes,
            desired,
            good,
        };

        let data = Data {
            tags: raw_tags,
            bases,
            mods,
            stat_ids,
            target,
            ids,
            groups,
        };
        data.check()?;
        Ok(data)
    }

    pub fn base_at(&self, b: u16) -> &Base {
        &self.bases[b as usize]
    }

    /// The base with this id, if the file has one.
    pub fn base_id(&self, id: &str) -> Option<u16> {
        self.bases
            .iter()
            .position(|base| base.id == id)
            .map(|b| b as u16)
    }

    pub fn mod_at(&self, m: u16) -> &Mod {
        &self.mods[m as usize]
    }

    /// Every modifier, implicit or not.
    pub fn id_of(&self, id: &str) -> Option<u16> {
        self.ids.get(id).copied()
    }

    pub fn tag_name(&self, tag: u16) -> &str {
        &self.tags[tag as usize]
    }

    pub fn group_name(&self, group: u16) -> &str {
        &self.groups[group as usize]
    }

    pub fn stat_id(&self, stat: u16) -> &str {
        &self.stat_ids[stat as usize]
    }

    /// The interned tag an influence contributes to a pool, or `None` when the tag table does not
    /// know it (a modifier can only weigh on a tag the file names).
    pub fn influence_tag(&self, influence: Influence) -> Option<u16> {
        self.tags
            .iter()
            .position(|tag| tag.as_str() == influence.tag())
            .map(|tag| tag as u16)
    }

    /// The modifier's weight on a base with this influence: the weight of its first spawn-weight
    /// entry whose tag the item carries, the item's tags being the base's plus the influence's
    /// (none when `influence` is `None`). It is `0` when no entry matches, and `0` when the first
    /// match is a zero, even if a later entry is positive.
    ///
    /// This is the number a draw uses. A Shaper-only modifier weighs `0` with no influence and its
    /// weight with `Some(Influence::Shaper)`. An influence whose tag the file does not know adds
    /// nothing, because no modifier can weigh on a tag that is not in the table.
    pub fn weight_on(&self, m: u16, base: u16, influence: Option<Influence>) -> u128 {
        let influence_tag = influence.and_then(|influence| self.influence_tag(influence));
        let base_tags = &self.base_at(base).tags;
        first_match(&self.mod_at(m).weights, |tag| {
            base_tags.contains(&tag) || Some(tag) == influence_tag
        })
    }

    /// The base with this name. Panics when the name is absent or names more than one base.
    pub fn base_of(&self, name: &str) -> u16 {
        let mut matches = self
            .bases
            .iter()
            .enumerate()
            .filter(|(_, base)| base.name == name)
            .map(|(b, _)| b as u16);
        let first = matches
            .next()
            .unwrap_or_else(|| panic!("no base is named {name:?}"));
        if matches.next().is_some() {
            panic!("more than one base is named {name:?}");
        }
        first
    }

    /// Re-checks the generator's invariants, so a hand-edited or truncated data file cannot load.
    fn check(&self) -> Result<(), String> {
        // The file's shape: explicit modifiers first, then the implicits.
        let mut implicit_seen = false;
        for entry in &self.mods {
            if entry.kind.is_implicit() {
                implicit_seen = true;
            } else if implicit_seen {
                return Err(format!("{} follows an implicit modifier", entry.id));
            }
        }

        let base = 0u16;
        let shaper_tag = self.influence_tag(Influence::Shaper).ok_or_else(|| {
            format!(
                "the tag table lacks the Shaper tag {:?}",
                Influence::Shaper.tag()
            )
        })?;
        let base_tags = &self.base_at(base).tags;

        // Every explicit modifier in the file can roll on the Shaper shield, and none of its
        // weights is on a tag the shield could never carry.
        for (m, entry) in self.mods.iter().enumerate() {
            if entry.kind.side().is_none() {
                continue;
            }
            if self.weight_on(m as u16, base, Some(Influence::Shaper)) == 0 {
                return Err(format!("{} cannot roll on the Shaper shield", entry.id));
            }
            for (tag, _) in &entry.weights {
                if !base_tags.contains(tag) && *tag != shaper_tag {
                    return Err(format!(
                        "{} weighs on {}, which is neither a base tag nor the Shaper tag",
                        entry.id,
                        self.tag_name(*tag)
                    ));
                }
            }
        }

        let mut counts = [0usize; 2];
        let mut shaper_totals = [0u128; 2];
        let mut plain_totals = [0u128; 2];
        for (m, entry) in self.mods.iter().enumerate() {
            let Some(side) = entry.kind.side() else {
                continue;
            };
            let index = side.index();
            let m = m as u16;
            counts[index] += 1;
            shaper_totals[index] = shaper_totals[index]
                .checked_add(self.weight_on(m, base, Some(Influence::Shaper)))
                .ok_or_else(|| format!("{}'s weight overflows", entry.id))?;
            plain_totals[index] = plain_totals[index]
                .checked_add(self.weight_on(m, base, None))
                .ok_or_else(|| format!("{}'s weight overflows", entry.id))?;
        }
        if counts != [73, 131] {
            return Err(format!(
                "the pool has {} prefixes and {} suffixes, expected 73 and 131",
                counts[0], counts[1]
            ));
        }
        if shaper_totals != [64_800, 96_475] {
            return Err(format!(
                "the Shaper weights are {} prefixes and {} suffixes, expected 64800 and 96475",
                shaper_totals[0], shaper_totals[1]
            ));
        }
        if plain_totals != [64_000, 88_475] {
            return Err(format!(
                "the plain weights are {} prefixes and {} suffixes, expected 64000 and 88475",
                plain_totals[0], plain_totals[1]
            ));
        }

        let implicit = self
            .id_of(BASE_IMPLICIT)
            .ok_or_else(|| format!("{BASE_IMPLICIT} is not in the pool"))?;
        let entry = self.mod_at(implicit);
        if entry.kind != ModKind::Implicit {
            return Err(format!("{BASE_IMPLICIT} is not an implicit"));
        }
        let stat = self
            .stat_ids
            .iter()
            .position(|id| id == BASE_IMPLICIT_STAT)
            .ok_or_else(|| format!("{BASE_IMPLICIT_STAT} is not a stat id"))?
            as u16;
        if entry.stats.as_slice() != [(stat, 5, 5)] {
            return Err(format!(
                "{BASE_IMPLICIT} has stats {:?}, expected [({BASE_IMPLICIT_STAT}, 5, 5)]",
                entry.stats
            ));
        }
        if !self.base_at(base).implicits.contains(&implicit) {
            return Err(format!("the base does not carry {BASE_IMPLICIT}"));
        }

        // Without its influence the base rolls strictly less, and only because of the 11 modifiers
        // whose one positive weight is the Shaper tag.
        let pool_size = |influence: Option<Influence>| {
            self.mods
                .iter()
                .enumerate()
                .filter(|(m, entry)| {
                    entry.kind.side().is_some() && self.weight_on(*m as u16, base, influence) > 0
                })
                .count()
        };
        let shaper_pool = pool_size(Some(Influence::Shaper));
        let plain_pool = pool_size(None);
        if plain_pool >= shaper_pool {
            return Err(format!(
                "the plain base rolls {plain_pool} modifiers against the Shaper base's {shaper_pool}"
            ));
        }
        if shaper_pool != 204 {
            return Err(format!(
                "the Shaper base rolls {shaper_pool} modifiers, expected 204"
            ));
        }
        let dropped = shaper_pool - plain_pool;
        if dropped != 11 {
            return Err(format!(
                "the influence drops {dropped} modifiers, expected 11"
            ));
        }

        Ok(())
    }
}

/// The weight of the first entry whose tag the item carries; `0` when no entry matches.
fn first_match(weights: &[(u16, u128)], carries: impl Fn(u16) -> bool) -> u128 {
    weights
        .iter()
        .find(|&&(tag, _)| carries(tag))
        .map_or(0, |&(_, weight)| weight)
}

fn resolve_tag(index: &HashMap<String, u16>, name: &str, owner: &str) -> Result<u16, String> {
    index
        .get(name)
        .copied()
        .ok_or_else(|| format!("{owner} names the unknown tag {name:?}"))
}

fn intern(
    table: &mut Vec<String>,
    index: &mut HashMap<String, u16>,
    name: String,
    what: &str,
) -> Result<u16, String> {
    if let Some(&id) = index.get(&name) {
        return Ok(id);
    }
    let id = u16::try_from(table.len())
        .map_err(|_| format!("there are more than {} {what}", u16::MAX))?;
    index.insert(name.clone(), id);
    table.push(name);
    Ok(id)
}

fn resolve_targets(
    ids: &HashMap<String, u16>,
    names: &[String],
    what: &str,
) -> Result<Vec<u16>, String> {
    names
        .iter()
        .map(|id| {
            ids.get(id)
                .copied()
                .ok_or_else(|| format!("the target names {id} as {what}, but the pool lacks it"))
        })
        .collect()
}

#[derive(Deserialize)]
struct Raw {
    tags: Vec<String>,
    bases: Vec<RawBase>,
    mods: Vec<RawMod>,
    target: RawTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBase {
    id: String,
    name: String,
    item_class: String,
    drop_level: u32,
    tags: Vec<String>,
    requirements: RawRequirements,
    defences: RawDefences,
    implicits: Vec<String>,
}

#[derive(Deserialize)]
struct RawRequirements {
    level: u32,
    strength: u32,
    dexterity: u32,
    intelligence: u32,
}

/// `ward` is in the file but not in the model, so it is simply not deserialized.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDefences {
    armour: Option<(i32, i32)>,
    evasion: Option<(i32, i32)>,
    energy_shield: Option<(i32, i32)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMod {
    id: String,
    name: String,
    kind: String,
    level: u32,
    groups: Vec<String>,
    weights: Vec<(String, u128)>,
    stats: Vec<RawStat>,
    implicit_tags: Vec<String>,
    adds_tags: Vec<String>,
    essence_only: bool,
    text: String,
}

#[derive(Deserialize)]
struct RawStat {
    id: String,
    min: i32,
    max: i32,
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

    /// The pool with no influence or with Shaper influence, as modifier ids, per-side counts and
    /// per-side weight totals.
    fn pool(data: &Data, shaper: bool) -> (Vec<u16>, (usize, usize), (u128, u128)) {
        let influence = || {
            if shaper {
                Some(Influence::Shaper)
            } else {
                None
            }
        };
        let mut mods = Vec::new();
        let mut counts = [0usize; 2];
        let mut totals = [0u128; 2];
        for (m, entry) in data.mods.iter().enumerate() {
            let Some(side) = entry.kind.side() else {
                continue;
            };
            let weight = data.weight_on(m as u16, 0, influence());
            if weight == 0 {
                continue;
            }
            mods.push(m as u16);
            counts[side.index()] += 1;
            totals[side.index()] += weight;
        }
        (mods, (counts[0], counts[1]), (totals[0], totals[1]))
    }

    #[test]
    fn the_shaper_pool_is_the_shields() {
        let data = data();
        assert_eq!(
            data.mods.len(),
            205,
            "204 explicit modifiers and one implicit"
        );
        let (mods, counts, weights) = pool(data, true);
        assert_eq!(
            (mods.len(), counts, weights),
            (204, (73, 131), (64_800, 96_475))
        );
    }

    #[test]
    fn without_influence_the_pool_is_strictly_less() {
        let data = data();
        let (shaper_mods, _, _) = pool(data, true);
        let (plain_mods, counts, weights) = pool(data, false);
        assert_eq!(
            (plain_mods.len(), counts, weights),
            (193, (72, 121), (64_000, 88_475))
        );

        let missing: Vec<u16> = shaper_mods
            .into_iter()
            .filter(|m| !plain_mods.contains(m))
            .collect();
        assert_eq!(missing.len(), 11, "exactly the Shaper-only modifiers");
        assert!(missing.iter().all(|m| {
            let (first, _) = data.mod_at(*m).weights[0];
            data.tag_name(first) == "shield_shaper"
        }));

        let totem = data.id_of("MaximumTotemUber1").expect("the totem prefix");
        assert_eq!(data.weight_on(totem, 0, Some(Influence::Shaper)), 800);
        assert_eq!(data.weight_on(totem, 0, None), 0);
    }

    #[test]
    fn the_base_implicit_resolves() {
        let data = data();
        let implicit = data.id_of(BASE_IMPLICIT).expect("the base implicit");
        let entry = data.mod_at(implicit);
        assert_eq!(entry.kind, ModKind::Implicit);
        assert_eq!(entry.kind.side(), None);
        assert!(entry.kind.is_implicit());
        let stat = data
            .stat_ids
            .iter()
            .position(|id| id == BASE_IMPLICIT_STAT)
            .expect("the implicit's stat") as u16;
        assert_eq!(entry.stats, vec![(stat, 5, 5)]);
        assert_eq!(data.stat_id(stat), BASE_IMPLICIT_STAT);
        assert!(data.base_at(0).implicits.contains(&implicit));
    }

    #[test]
    fn every_target_modifier_resolves() {
        let data = data();
        assert_eq!(data.target.prefixes, ["desired"]);
        assert!(data.target.suffixes.is_empty());
        assert!(!data.target.desired.is_empty());
        assert!(!data.target.good.is_empty());
        for m in data.target.desired.iter().chain(&data.target.good) {
            assert!(data.mods.get(*m as usize).is_some());
            assert!(data.id_of(&data.mod_at(*m).id).is_some());
        }
    }

    #[test]
    fn weight_on_differs_with_the_influence() {
        let data = data();
        let totem = data.id_of("MaximumTotemUber1").expect("the totem prefix");
        assert_eq!(data.weight_on(totem, 0, Some(Influence::Shaper)), 800);
        assert_ne!(
            data.weight_on(totem, 0, Some(Influence::Shaper)),
            data.weight_on(totem, 0, None)
        );

        // A modifier both pools carry weighs the same with or without the influence.
        let life = data.id_of("IncreasedLife10").expect("the life prefix");
        assert_eq!(data.weight_on(life, 0, Some(Influence::Shaper)), 1000);
        assert_eq!(data.weight_on(life, 0, None), 1000);

        // An influence whose tag the file does not know adds nothing.
        assert_eq!(data.weight_on(totem, 0, Some(Influence::Elder)), 0);

        let shaper_tag = data
            .influence_tag(Influence::Shaper)
            .expect("the Shaper tag");
        assert_eq!(data.tag_name(shaper_tag), "shield_shaper");
        assert_eq!(data.influence_tag(Influence::Elder), None);
    }

    #[test]
    fn there_is_one_base_and_it_is_the_shield() {
        let data = data();
        assert_eq!(data.bases.len(), 1);
        let base = data.base_at(0);
        assert_eq!(base.name, "Supreme Spiked Shield");
        assert_eq!(base.item_class, "Shield");
        assert_eq!(base.drop_level, 70);
        assert_eq!(base.defences, [None, Some((242, 278)), Some((49, 57))]);
        assert_eq!(data.base_of("Supreme Spiked Shield"), 0);
        assert_eq!(data.base_id(&base.id), Some(0));
        assert!(
            !base
                .tags
                .iter()
                .any(|tag| data.tag_name(*tag) == "shield_shaper")
        );
        assert!(data.tags.iter().any(|tag| tag == "shield_shaper"));
    }

    #[test]
    #[should_panic(expected = "no base is named")]
    fn base_of_refuses_an_unknown_name() {
        data().base_of("Not a Shield");
    }

    #[test]
    fn the_tag_and_group_tables_round_trip() {
        let data = data();
        assert!(!data.tags.is_empty());
        for base in &data.bases {
            for tag in &base.tags {
                assert!(!data.tag_name(*tag).is_empty());
            }
        }
        for entry in &data.mods {
            for group in &entry.groups {
                assert!(!data.group_name(*group).is_empty());
            }
            for (stat, min, max) in &entry.stats {
                assert!(!data.stat_id(*stat).is_empty());
                assert!(min <= max);
            }
        }
    }

    #[test]
    fn the_first_matching_tag_decides_the_weight() {
        // The suppression family on this shield: `shield: 0` comes before `dex_int_armour: 500`,
        // so the modifier cannot roll, though summing the matches would give 500.
        let blocked = [(3, 0), (0, 500), (5, 0)];
        assert_eq!(first_match(&blocked, |tag| tag == 3 || tag == 0), 0);
        // An item without the blocking tag reaches the next entry.
        assert_eq!(first_match(&blocked, |tag| tag == 0 || tag == 5), 500);

        // Two positive matches: the earlier one is the weight, not their sum.
        let two = [(1, 500), (2, 1000)];
        assert_eq!(first_match(&two, |_| true), 500);
        assert_eq!(first_match(&two, |tag| tag == 2), 1000);

        // No entry on a tag the item carries.
        assert_eq!(first_match(&two, |tag| tag == 9), 0);
        assert_eq!(first_match(&[], |_| true), 0);
    }

    #[test]
    fn every_modifier_in_the_file_resolves_by_its_first_match() {
        // The file lists zeros, so a sum and a first match can only agree if no modifier has a
        // positive weight behind its first matching tag: pin that this file has none.
        let data = data();
        for (m, entry) in data.mods.iter().enumerate() {
            if entry.kind.side().is_none() {
                continue;
            }
            let first = data.weight_on(m as u16, 0, Some(Influence::Shaper));
            let sum: u128 = entry.weights.iter().map(|&(_, weight)| weight).sum();
            assert_eq!(first, sum, "{}", entry.id);
        }
    }

    #[test]
    fn a_malformed_file_is_an_error() {
        assert!(Data::from_json("{}").is_err());
        assert!(Data::from_json("not json").is_err());
    }
}
