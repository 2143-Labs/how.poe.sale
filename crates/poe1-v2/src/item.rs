//! A concrete item: base, item level, rarity, influences, quality and catalyst, implicits,
//! enchantments, prefixes and suffixes (with rolled values or unrolled), sockets and every flag
//! crafting reads (corrupted, mirrored, split, synthesised, foreseen, Memory Strands,
//! intangibility, imprint). `ItemSpec` is the serde form with string ids used by fixtures and
//! the wire; `Item` is the index form the engine uses. `Item::from_spec` validates.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::data::{Category, ClassData, Data, ModKind};
pub use crate::data::{Eldritch, Influence, Rarity, Side};

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemSpec {
    /// A metadata id, or a base name that is unique.
    pub base: String,
    pub item_level: u8,
    pub rarity: Rarity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub influences: Vec<Influence>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub quality: u8,
    /// A catalyst currency name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalyst: Option<String>,
    /// Armour, evasion, energy shield, ward as shown on a pasted item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defences: Option<[i32; 4]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implicits: Vec<ModSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enchants: Vec<ModSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefixes: Vec<AffixSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suffixes: Vec<AffixSpec>,
    /// `"B-B-B-B"`, `"R-G B"`, `"A A"`: `-` links, a space separates groups.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sockets: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub corrupted: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mirrored: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub split: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthesised: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub foreseen: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub memory_strands: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub intangibility: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imprint: Option<Box<ItemSpec>>,
    /// Fixture prose only; `Item` does not keep it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An implicit or enchantment; `values: None` = unrolled.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModSpec {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<i32>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AffixSpec {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<i32>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub fractured: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Item {
    pub base: u16,
    pub item_level: u8,
    pub rarity: Rarity,
    /// Bit `Influence::bit` per influence.
    pub influences: u8,
    pub quality: u8,
    /// Index into `Bundle::catalysts`.
    pub catalyst: Option<u8>,
    /// Display-only figures copied from a paste; `apply` clears them when the item changes.
    pub defences: Option<[i32; 4]>,
    pub implicits: Vec<Roll>,
    pub enchants: Vec<Roll>,
    pub prefixes: Vec<Affix>,
    pub suffixes: Vec<Affix>,
    pub sockets: Vec<Vec<Socket>>,
    pub corrupted: bool,
    pub mirrored: bool,
    pub split: bool,
    pub synthesised: bool,
    pub foreseen: bool,
    pub memory_strands: u8,
    pub intangibility: u8,
    pub imprint: Option<Box<Item>>,
}

/// A modifier and its rolled values, always the mod's own roll inside `[min, max]`; `None` =
/// unrolled.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Roll {
    pub m: u32,
    pub values: Option<Vec<i32>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Affix {
    pub roll: Roll,
    pub fractured: bool,
}

impl Affix {
    pub fn new(m: u32) -> Affix {
        Affix {
            roll: Roll { m, values: None },
            fractured: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Socket {
    R,
    G,
    B,
    W,
    A,
}

impl Socket {
    fn parse(c: char) -> Option<Socket> {
        Some(match c {
            'R' => Socket::R,
            'G' => Socket::G,
            'B' => Socket::B,
            'W' => Socket::W,
            'A' => Socket::A,
            _ => return None,
        })
    }

    fn letter(self) -> char {
        match self {
            Socket::R => 'R',
            Socket::G => 'G',
            Socket::B => 'B',
            Socket::W => 'W',
            Socket::A => 'A',
        }
    }
}

/// Bench metamods, recognised by mod group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetaMod {
    LockPrefixes,
    LockSuffixes,
    NoAttack,
    NoCaster,
    MultiCraft,
    Level28,
}

impl MetaMod {
    pub fn group(self) -> &'static str {
        match self {
            MetaMod::LockPrefixes => "ItemGenerationCannotChangePrefixes",
            MetaMod::LockSuffixes => "ItemGenerationCannotChangeSuffixes",
            MetaMod::NoAttack => "ItemGenerationCannotRollAttackAffixes",
            MetaMod::NoCaster => "ItemGenerationCannotRollCasterAffixes",
            MetaMod::MultiCraft => "ItemGenerationCanHaveMultipleCraftedMods",
            MetaMod::Level28 => "MaxModRequiredLevelOverride",
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ItemError {
    #[error("unknown base \"{0}\"")]
    UnknownBase(String),
    #[error("base name \"{base}\" matches several bases; use one of: {}", ids.join(", "))]
    AmbiguousBase { base: String, ids: Vec<String> },
    #[error("item class {0} is not covered by v2")]
    ClassNotCovered(String),
    #[error("unknown modifier \"{0}\"")]
    UnknownMod(String),
    #[error("modifier \"{id}\" cannot be a {place}")]
    WrongPlace { id: String, place: &'static str },
    #[error("modifier \"{id}\" needs {n} values within its ranges")]
    BadValues { id: String, n: usize },
    #[error("a normal item cannot have explicit modifiers")]
    NormalExplicits,
    #[error("too many {sides} for a {rarity} {class}")]
    TooMany {
        sides: &'static str,
        rarity: &'static str,
        class: String,
    },
    #[error("two modifiers share the group {0}")]
    SharedGroup(String),
    #[error("more crafted modifiers than the item allows")]
    TooManyCrafted,
    #[error("catalyst quality only exists on rings, amulets and belts")]
    CatalystNotJewellery,
    #[error("unknown catalyst \"{0}\"")]
    UnknownCatalyst(String),
    #[error("cannot read sockets \"{0}\"")]
    BadSockets(String),
    #[error("an item holds at most one veiled modifier")]
    TwoVeiled,
}

/// Parses socket notation; `None` when it is not `^([RGBWA](-[RGBWA])*)?( [RGBWA](-[RGBWA])*)*$`.
fn parse_sockets(s: &str) -> Option<Vec<Vec<Socket>>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for (i, group) in s.split(' ').enumerate() {
        if group.is_empty() && i == 0 {
            continue;
        }
        let sockets = group
            .split('-')
            .map(|c| {
                let mut chars = c.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Socket::parse(c),
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()?;
        out.push(sockets);
    }
    Some(out)
}

fn socket_string(groups: &[Vec<Socket>]) -> String {
    groups
        .iter()
        .map(|g| {
            g.iter()
                .map(|s| s.letter().to_string())
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where a mod sits on an item.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Prefix,
    Suffix,
    Implicit,
    Enchant,
}

impl Place {
    fn name(self) -> &'static str {
        match self {
            Place::Prefix => "prefix",
            Place::Suffix => "suffix",
            Place::Implicit => "implicit",
            Place::Enchant => "enchant",
        }
    }

    fn accepts(self, kind: ModKind, side: Option<Side>) -> bool {
        match self {
            Place::Prefix => kind.is_explicit() && side == Some(Side::Prefix),
            Place::Suffix => kind.is_explicit() && side == Some(Side::Suffix),
            Place::Implicit => kind.is_implicit(),
            Place::Enchant => kind.is_enchant(),
        }
    }
}

/// A displayed value: multiply before truncating so integer rolls stay exact.
pub fn display(v: i32, effect: i16) -> i32 {
    (i64::from(v) * (100 + i64::from(effect)) / 100) as i32
}

impl Item {
    pub fn from_spec(data: &Data, spec: &ItemSpec) -> Result<Item, ItemError> {
        // 1. base
        let base = match data.base_by_id(&spec.base) {
            Some(b) => b,
            None => match data.bases_named(&spec.base) {
                [] => return Err(ItemError::UnknownBase(spec.base.clone())),
                [b] => *b,
                many => {
                    return Err(ItemError::AmbiguousBase {
                        base: spec.base.clone(),
                        ids: many.iter().map(|&b| data.base_at(b).id.clone()).collect(),
                    });
                }
            },
        };
        // 2. class: every bundle class is in scope; a base whose class the bundle lacks is not.
        let class_index = data.base_at(base).class;
        let Some(class) = data.classes.get(class_index as usize) else {
            return Err(ItemError::ClassNotCovered(class_index.to_string()));
        };

        // 3. ids and places
        let resolve = |id: &str, place: Place| -> Result<u32, ItemError> {
            let m = data
                .mod_by_id(id)
                .ok_or_else(|| ItemError::UnknownMod(id.to_string()))?;
            let md = data.mod_at(m);
            if !place.accepts(md.kind, md.side) {
                return Err(ItemError::WrongPlace {
                    id: id.to_string(),
                    place: place.name(),
                });
            }
            Ok(m)
        };
        struct Entry<'a> {
            place: Place,
            m: u32,
            id: &'a str,
            values: &'a Option<Vec<i32>>,
            fractured: bool,
        }
        let listed = spec
            .implicits
            .iter()
            .map(|s| (Place::Implicit, &s.id, &s.values, false))
            .chain(
                spec.enchants
                    .iter()
                    .map(|s| (Place::Enchant, &s.id, &s.values, false)),
            )
            .chain(
                spec.prefixes
                    .iter()
                    .map(|s| (Place::Prefix, &s.id, &s.values, s.fractured)),
            )
            .chain(
                spec.suffixes
                    .iter()
                    .map(|s| (Place::Suffix, &s.id, &s.values, s.fractured)),
            );
        let mut entries = Vec::new();
        for (place, id, values, fractured) in listed {
            entries.push(Entry {
                place,
                m: resolve(id, place)?,
                id,
                values,
                fractured,
            });
        }
        // 4. values
        for e in &entries {
            if let Some(v) = e.values {
                let stats = &data.mod_at(e.m).stats;
                let ok = v.len() == stats.len()
                    && v.iter()
                        .zip(stats)
                        .all(|(&x, s)| s.min.min(s.max) <= x && x <= s.max.max(s.min));
                if !ok {
                    return Err(ItemError::BadValues {
                        id: e.id.to_string(),
                        n: stats.len(),
                    });
                }
            }
        }

        let mut item = Item {
            base,
            item_level: spec.item_level,
            rarity: spec.rarity,
            influences: spec.influences.iter().fold(0, |acc, i| acc | i.bit()),
            quality: spec.quality,
            catalyst: None,
            defences: spec.defences,
            implicits: Vec::new(),
            enchants: Vec::new(),
            prefixes: Vec::new(),
            suffixes: Vec::new(),
            sockets: Vec::new(),
            corrupted: spec.corrupted,
            mirrored: spec.mirrored,
            split: spec.split,
            synthesised: spec.synthesised,
            foreseen: spec.foreseen,
            memory_strands: spec.memory_strands,
            intangibility: spec.intangibility,
            imprint: None,
        };
        for e in entries {
            let roll = Roll {
                m: e.m,
                values: e.values.clone(),
            };
            let fractured = e.fractured;
            match e.place {
                Place::Implicit => item.implicits.push(roll),
                Place::Enchant => item.enchants.push(roll),
                Place::Prefix => item.prefixes.push(Affix { roll, fractured }),
                Place::Suffix => item.suffixes.push(Affix { roll, fractured }),
            }
        }

        // 5. normal items
        if item.rarity == Rarity::Normal && !(item.prefixes.is_empty() && item.suffixes.is_empty())
        {
            return Err(ItemError::NormalExplicits);
        }
        // 6. counts
        let limits = item.limits(data);
        for side in Side::BOTH {
            if item.affixes(side).len() > limits[side.index()] as usize {
                return Err(ItemError::TooMany {
                    sides: match side {
                        Side::Prefix => "prefixes",
                        Side::Suffix => "suffixes",
                    },
                    rarity: item.rarity.name(),
                    class: class.id.clone(),
                });
            }
        }
        // 7. groups
        let mut seen = BTreeSet::new();
        for a in item.explicits() {
            for &g in &data.mod_at(a.roll.m).groups {
                if !seen.insert(g) {
                    return Err(ItemError::SharedGroup(data.group(g).to_string()));
                }
            }
        }
        // 8. crafted
        if item.crafted_count(data) > item.crafted_limit(data) {
            return Err(ItemError::TooManyCrafted);
        }
        // 9. catalyst
        if let Some(name) = &spec.catalyst {
            if class.category != Category::Jewellery {
                return Err(ItemError::CatalystNotJewellery);
            }
            item.catalyst = Some(
                data.catalyst_by_name(name)
                    .ok_or_else(|| ItemError::UnknownCatalyst(name.clone()))?,
            );
        }
        // 10. sockets
        item.sockets = parse_sockets(&spec.sockets)
            .ok_or_else(|| ItemError::BadSockets(spec.sockets.clone()))?;
        // 11. veiled
        if item
            .explicits()
            .filter(|a| data.mod_at(a.roll.m).kind == ModKind::Veiled)
            .count()
            > 1
        {
            return Err(ItemError::TwoVeiled);
        }
        if let Some(imprint) = &spec.imprint {
            item.imprint = Some(Box::new(Item::from_spec(data, imprint)?));
        }
        item.canonical();
        Ok(item)
    }

    pub fn to_spec(&self, data: &Data) -> ItemSpec {
        let base = data.base_at(self.base);
        let mod_spec = |r: &Roll| ModSpec {
            id: data.mod_at(r.m).id.clone(),
            values: r.values.clone(),
        };
        let affix_spec = |a: &Affix| AffixSpec {
            id: data.mod_at(a.roll.m).id.clone(),
            values: a.roll.values.clone(),
            fractured: a.fractured,
        };
        ItemSpec {
            base: if data.bases_named(&base.name).len() == 1 {
                base.name.clone()
            } else {
                base.id.clone()
            },
            item_level: self.item_level,
            rarity: self.rarity,
            influences: Influence::ALL
                .into_iter()
                .filter(|i| self.influences & i.bit() != 0)
                .collect(),
            quality: self.quality,
            catalyst: self
                .catalyst
                .map(|c| data.catalysts[c as usize].name.clone()),
            defences: self.defences,
            implicits: self.implicits.iter().map(mod_spec).collect(),
            enchants: self.enchants.iter().map(mod_spec).collect(),
            prefixes: self.prefixes.iter().map(affix_spec).collect(),
            suffixes: self.suffixes.iter().map(affix_spec).collect(),
            sockets: socket_string(&self.sockets),
            corrupted: self.corrupted,
            mirrored: self.mirrored,
            split: self.split,
            synthesised: self.synthesised,
            foreseen: self.foreseen,
            memory_strands: self.memory_strands,
            intangibility: self.intangibility,
            imprint: self.imprint.as_ref().map(|i| Box::new(i.to_spec(data))),
            note: None,
        }
    }

    /// `display(v, effect)`: a stored roll as the item shows it.
    pub fn display(v: i32, effect: i16) -> i32 {
        display(v, effect)
    }

    pub fn class<'d>(&self, data: &'d Data) -> &'d ClassData {
        data.class_of(self.base)
    }

    pub fn category(&self, data: &Data) -> Category {
        self.class(data).category
    }

    pub fn has_influence(&self, i: Influence) -> bool {
        self.influences & i.bit() != 0
    }

    pub fn affixes(&self, side: Side) -> &Vec<Affix> {
        match side {
            Side::Prefix => &self.prefixes,
            Side::Suffix => &self.suffixes,
        }
    }

    pub fn affixes_mut(&mut self, side: Side) -> &mut Vec<Affix> {
        match side {
            Side::Prefix => &mut self.prefixes,
            Side::Suffix => &mut self.suffixes,
        }
    }

    /// Prefixes then suffixes.
    pub fn explicits(&self) -> impl Iterator<Item = &Affix> {
        self.prefixes.iter().chain(&self.suffixes)
    }

    /// `[prefixes, suffixes]` the item can hold: the class limits for its rarity (rare limits
    /// for normal and unique items) plus the base's deltas, floored at 0.
    pub fn limits(&self, data: &Data) -> [u8; 2] {
        let class = self.class(data);
        let base = data.base_at(self.base);
        let l = match self.rarity {
            Rarity::Magic => class.limits.magic,
            _ => class.limits.rare,
        };
        [
            (i16::from(l[0]) + i16::from(base.prefix_delta)).max(0) as u8,
            (i16::from(l[1]) + i16::from(base.suffix_delta)).max(0) as u8,
        ]
    }

    /// The magnitude bonus (percent) a mod's values show with: 0 for implicits; for explicits
    /// the base's explicit effect plus the catalyst quality when the catalyst enhances the mod.
    pub fn effect(&self, data: &Data, roll: &Roll, implicit: bool) -> i16 {
        if implicit {
            return 0;
        }
        let base = data.base_at(self.base).explicit_effect;
        let quality = match self.catalyst {
            Some(c) => {
                let cat = &data.catalysts[c as usize];
                let m = data.mod_at(roll.m);
                let applies = cat.side.is_some_and(|s| m.side == Some(s))
                    || m.tags.iter().any(|t| cat.tags.contains(t));
                if applies { i16::from(self.quality) } else { 0 }
            }
            None => 0,
        };
        base + quality
    }

    pub fn open(&self, data: &Data, side: Side) -> u8 {
        self.limits(data)[side.index()].saturating_sub(self.affixes(side).len() as u8)
    }

    /// Sorted, deduped: base tags, influence tags, and every mod's `adds_tags`.
    pub fn tags(&self, data: &Data) -> Vec<u16> {
        let mut out = data.base_at(self.base).tags.clone();
        if let Some(inf) = self.class(data).influence_tags {
            for i in Influence::ALL {
                if self.has_influence(i) {
                    out.push(inf[i as usize]);
                }
            }
        }
        let rolls = self
            .implicits
            .iter()
            .chain(&self.enchants)
            .chain(self.explicits().map(|a| &a.roll));
        for r in rolls {
            out.extend(&data.mod_at(r.m).adds_tags);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Groups of the explicits (implicit groups never block explicits).
    pub fn explicit_groups(&self, data: &Data) -> BTreeSet<u16> {
        self.explicits()
            .flat_map(|a| data.mod_at(a.roll.m).groups.iter().copied())
            .collect()
    }

    pub fn crafted_count(&self, data: &Data) -> usize {
        self.explicits()
            .filter(|a| data.mod_at(a.roll.m).kind == ModKind::Crafted)
            .count()
    }

    pub fn crafted_limit(&self, data: &Data) -> usize {
        if self.has_meta(data, MetaMod::MultiCraft) {
            3
        } else {
            1
        }
    }

    pub fn has_meta(&self, data: &Data, meta: MetaMod) -> bool {
        let Some(g) = data.group_by_name(meta.group()) else {
            return false;
        };
        self.explicits()
            .any(|a| data.mod_at(a.roll.m).groups.contains(&g))
    }

    /// Whether a metamod keeps this side from changing.
    pub fn locked(&self, data: &Data, side: Side) -> bool {
        match side {
            Side::Prefix => self.has_meta(data, MetaMod::LockPrefixes),
            Side::Suffix => self.has_meta(data, MetaMod::LockSuffixes),
        }
    }

    /// The unrevealed veiled placeholder: its side and index in that side's list.
    pub fn veiled(&self, data: &Data) -> Option<(Side, usize)> {
        Side::BOTH.into_iter().find_map(|side| {
            self.affixes(side)
                .iter()
                .position(|a| data.mod_at(a.roll.m).kind == ModKind::Veiled)
                .map(|i| (side, i))
        })
    }

    /// The highest Searing Exarch and Eater of Worlds implicit tiers.
    pub fn eldritch(&self, data: &Data) -> (Option<u8>, Option<u8>) {
        let tier = |kind: ModKind| {
            self.implicits
                .iter()
                .map(|r| data.mod_at(r.m))
                .filter(|m| m.kind == kind)
                .filter_map(|m| m.eldritch_tier)
                .max()
        };
        (tier(ModKind::Exarch), tier(ModKind::Eater))
    }

    /// The eldritch kind with the strictly higher tier (the present one when only one exists);
    /// `None` on equal tiers.
    pub fn dominant(&self, data: &Data) -> Option<Eldritch> {
        match self.eldritch(data) {
            (Some(x), Some(e)) if x > e => Some(Eldritch::Exarch),
            (Some(x), Some(e)) if e > x => Some(Eldritch::Eater),
            (Some(_), None) => Some(Eldritch::Exarch),
            (None, Some(_)) => Some(Eldritch::Eater),
            _ => None,
        }
    }

    pub fn influenced_explicits(&self, data: &Data) -> usize {
        self.explicits()
            .filter(|a| data.mod_at(a.roll.m).influence.is_some())
            .count()
    }

    /// Sorts every mod list by (mod index, values, fractured).
    pub fn canonical(&mut self) {
        self.prefixes.sort();
        self.suffixes.sort();
        self.implicits.sort();
        self.enchants.sort();
        if let Some(imprint) = &mut self.imprint {
            imprint.canonical();
        }
    }
}
