//! What a craft holds: a base, its rarity, its origin and the modifiers on it, with the values the
//! model knows them in.
//!
//! An item is a base ([`Item::base`], an index into [`crate::data::Data`]), an item level, a
//! rarity, an optional influence, and three lists of held modifiers: the base's implicits and one
//! list per explicit side. Those lists are private, so a modifier is added or removed through
//! [`Item::affixes_mut`] and nobody can put a fourth prefix on a rare item except through the room
//! rule in [`crate::pool`].
//!
//! Every held modifier is an [`Affix`]: its id into the data, plus one [`Roll`] per stat in the
//! mod's stat order. A value is never a number here, it is an interval — `[min,max]` for a freshly
//! rolled stat, `[v,v]` for a value a fixture pins, and the full range again when a Divine Orb
//! widens it. [`Item::stat_totals`] adds those intervals, which is exact.

use crate::data::Data;
use crate::roll::Roll;

/// The two sides a modifier can sit on.
///
/// The order is the one every side-major loop in this crate uses: prefixes first. `Prefix.index()`
/// is `0` and `Suffix.index()` is `1`, so `[Side::BOTH]`-indexed arrays line up with the affix
/// arrays in [`Item`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Prefix,
    Suffix,
}

impl Side {
    /// Both sides, prefix first.
    pub const BOTH: [Side; 2] = [Side::Prefix, Side::Suffix];

    pub fn index(self) -> usize {
        match self {
            Side::Prefix => 0,
            Side::Suffix => 1,
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Prefix => Side::Suffix,
            Side::Suffix => Side::Prefix,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::Prefix => "prefix",
            Side::Suffix => "suffix",
        }
    }

    /// The side a data file's `type` field names, or `None` for anything else (an implicit, a
    /// corrupted modifier, a crafted one).
    pub fn from_name(name: &str) -> Option<Side> {
        match name {
            "prefix" => Some(Side::Prefix),
            "suffix" => Some(Side::Suffix),
            _ => None,
        }
    }
}

/// An influence: a property of the item, not of the base.
///
/// An influenced item can roll the modifier pool of its influence — a Shaper shield rolls
/// `MaximumTotemUber1`, a plain one does not — because the influence's tag joins the base's tags
/// whenever a pool is read. It is not a base: the same base drops with or without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Influence {
    Shaper,
    Elder,
    Crusader,
    Redeemer,
    Hunter,
    Warlord,
}

impl Influence {
    /// All six, in the game's order.
    pub const ALL: [Influence; 6] = [
        Influence::Shaper,
        Influence::Elder,
        Influence::Crusader,
        Influence::Redeemer,
        Influence::Hunter,
        Influence::Warlord,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Influence::Shaper => "shaper",
            Influence::Elder => "elder",
            Influence::Crusader => "crusader",
            Influence::Redeemer => "redeemer",
            Influence::Hunter => "hunter",
            Influence::Warlord => "warlord",
        }
    }

    /// The item tag an influenced pool matches its spawn weights against.
    pub fn tag(self) -> &'static str {
        match self {
            Influence::Shaper => "shield_shaper",
            Influence::Elder => "shield_elder",
            Influence::Crusader => "shield_crusader",
            Influence::Redeemer => "shield_redeemer",
            Influence::Hunter => "shield_hunter",
            Influence::Warlord => "shield_warlord",
        }
    }
}

/// The three rarities a craft here moves between. Influenced, fractured and corrupted states are
/// not modelled: this crate's data has none of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
}

impl Rarity {
    pub fn name(self) -> &'static str {
        match self {
            Rarity::Normal => "normal",
            Rarity::Magic => "magic",
            Rarity::Rare => "rare",
        }
    }
}

/// One held modifier and the values the model knows its stats in.
///
/// `rolls` follows the mod's stat order in [`crate::data::Data`] and is authoritative: the range a
/// stat *could* roll is `data.mod_at(m).stats`, while the values this affix *does* hold are here.
/// The two are the same ([`Roll::range`]) for a fresh affix, an interval for an accumulating total,
/// and a single point ([`Roll::exact`]) for a pinned fixture.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Affix {
    pub m: u16,
    pub rolls: Vec<Roll>,
}

impl Affix {
    pub fn new(m: u16, rolls: Vec<Roll>) -> Affix {
        Affix { m, rolls }
    }

    /// The affix just after it rolled: every stat at its full range.
    pub fn at_range(data: &Data, m: u16) -> Affix {
        let rolls = data
            .mod_at(m)
            .stats
            .iter()
            .map(|&(_, min, max)| Roll::range(min, max).expect("data.rs rejects min above max"))
            .collect();
        Affix { m, rolls }
    }

    /// An affix whose stats hold exactly `values`, in the mod's stat order. `None` when the count
    /// differs from the mod's stat count or a value lies outside its stat's range — a fixture that
    /// pins something the game could not roll is a bug in the fixture, not an item.
    pub fn pinned(data: &Data, m: u16, values: &[i32]) -> Option<Affix> {
        let stats = &data.mod_at(m).stats;
        if stats.len() != values.len() {
            return None;
        }
        let rolls = stats
            .iter()
            .zip(values)
            .map(|(&(_, min, max), &value)| {
                (min..=max).contains(&value).then_some(Roll::exact(value))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Affix { m, rolls })
    }
}

/// An item: its base, level, rarity, influence, quality and flags, plus its held modifiers.
///
/// Identity includes the values and the quality and the flags: two items that differ in any of
/// them are different items. Folding items that a draw cannot tell apart is the job of the craft's
/// lumped class, not of equality.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Item {
    /// Index into [`crate::data::Data::bases`].
    pub base: u16,
    /// The item level the pool's modifier levels are gated against.
    pub item_level: u8,
    pub rarity: Rarity,
    /// The influence, if any: its tag joins the base's tags when a pool is read.
    pub influence: Option<Influence>,
    /// Quality, as a percentage. The pool here does not read it; it is carried because an item
    /// that has quality is not the same item as one that does not.
    pub quality: u8,
    pub corrupted: bool,
    pub mirrored: bool,
    implicits: Vec<Affix>,
    affixes: [Vec<Affix>; 2],
}

impl Item {
    /// A fresh item of this base and item level: no influence, no quality, not corrupted or
    /// mirrored, no explicit modifiers, and the base's implicits at their full ranges.
    pub fn of_base(data: &Data, base: u16, item_level: u8, rarity: Rarity) -> Item {
        let implicits = data
            .base_at(base)
            .implicits
            .iter()
            .map(|&m| Affix::at_range(data, m))
            .collect();
        Item {
            base,
            item_level,
            rarity,
            influence: None,
            quality: 0,
            corrupted: false,
            mirrored: false,
            implicits,
            affixes: [Vec::new(), Vec::new()],
        }
    }

    /// The same item under an influence. The base does not change — the influence is a property of
    /// the item, and only widens the pool.
    pub fn with_influence(&self, influence: Influence) -> Item {
        Item {
            influence: Some(influence),
            ..self.clone()
        }
    }

    pub fn affixes(&self, side: Side) -> &[Affix] {
        &self.affixes[side.index()]
    }

    pub fn affixes_mut(&mut self, side: Side) -> &mut Vec<Affix> {
        &mut self.affixes[side.index()]
    }

    pub fn implicits(&self) -> &[Affix] {
        &self.implicits
    }

    /// How many explicit modifiers the item holds, both sides together.
    pub fn explicits(&self) -> usize {
        Side::BOTH.into_iter().map(|s| self.affixes(s).len()).sum()
    }

    /// The same affixes and values at another rarity.
    pub fn with_rarity(&self, rarity: Rarity) -> Item {
        Item {
            rarity,
            ..self.clone()
        }
    }

    /// The same item with every explicit modifier gone. The rarity, base, item level, influence,
    /// quality, flags and implicits are kept: scouring strips the explicits, nothing else.
    pub fn cleared(&self) -> Item {
        Item {
            affixes: [Vec::new(), Vec::new()],
            ..self.clone()
        }
    }

    /// The held affix for this modifier, implicit or explicit.
    pub fn affix(&self, m: u16) -> Option<&Affix> {
        self.implicits
            .iter()
            .chain(Side::BOTH.into_iter().flat_map(|side| self.affixes(side)))
            .find(|affix| affix.m == m)
    }

    /// Every stat the item holds, summed over its affixes, one entry per stat id.
    ///
    /// Summing intervals is exact (`[a,b] + [c,d] = [a+c, b+d]`), so the result is the exact set of
    /// totals the model admits — never an average and never a rolled value. The entries are sorted
    /// by stat id, i.e. the order of [`crate::data::Data::stat_ids`].
    pub fn stat_totals(&self, data: &Data) -> Vec<(u16, Roll)> {
        let mut totals: std::collections::BTreeMap<u16, Roll> = std::collections::BTreeMap::new();
        for affix in self.implicits.iter().chain(
            Side::BOTH
                .into_iter()
                .flat_map(|side| self.affixes(side).iter()),
        ) {
            let stats = &data.mod_at(affix.m).stats;
            assert_eq!(
                stats.len(),
                affix.rolls.len(),
                "affix {} holds {} rolls for {} stats",
                data.mod_at(affix.m).id,
                affix.rolls.len(),
                stats.len()
            );
            for (&(stat, _, _), &roll) in stats.iter().zip(&affix.rolls) {
                totals
                    .entry(stat)
                    .and_modify(|total| {
                        *total = total
                            .checked_add(roll)
                            .expect("two stat totals overflowed i32")
                    })
                    .or_insert(roll);
            }
        }
        totals.into_iter().collect()
    }

    /// A one-line description, for panics and tests: `"rare Supreme Spiked Shield (shaper)"`.
    pub fn describe(&self, data: &Data) -> String {
        let base = data.base_at(self.base);
        match self.influence {
            Some(influence) => format!(
                "{} {} ({})",
                self.rarity.name(),
                base.name,
                influence.name()
            ),
            None => format!("{} {}", self.rarity.name(), base.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Data, data};

    const BASE: &str = "Supreme Spiked Shield";

    fn rare(data: &Data) -> Item {
        Item::of_base(data, data.base_of(BASE), 87, Rarity::Rare)
    }

    #[test]
    fn a_fresh_item_holds_the_bases_implicit_at_its_range() {
        let data = data();
        let item = rare(data);
        assert_eq!(item.rarity, Rarity::Rare);
        assert_eq!(item.item_level, 87);
        assert_eq!(item.influence, None);
        assert_eq!(item.quality, 0);
        assert!(!item.corrupted && !item.mirrored);
        assert_eq!(item.explicits(), 0);
        assert_eq!(item.describe(data), "rare Supreme Spiked Shield");

        let implicit = data.id_of("ChanceToDodgeSpellsImplicitShield2").unwrap();
        assert_eq!(item.implicits().len(), 1);
        assert_eq!(
            item.implicits()[0],
            Affix::new(implicit, vec![Roll::exact(5)])
        );
        assert_eq!(item.affix(implicit), Some(&item.implicits()[0]));
    }

    #[test]
    fn an_affix_at_range_holds_every_stat_at_its_full_range() {
        let data = data();
        let two_stats = data.id_of("AttackerTakesDamage1").unwrap();
        let affix = Affix::at_range(data, two_stats);
        assert_eq!(
            affix.rolls,
            vec![Roll::range(1, 4).unwrap(), Roll::exact(0)]
        );
    }

    #[test]
    fn a_pinned_affix_rejects_a_wrong_count_or_an_out_of_range_value() {
        let data = data();
        let life = data.id_of("IncreasedLife10").unwrap();
        assert_eq!(
            Affix::pinned(data, life, &[150]),
            Some(Affix::new(life, vec![Roll::exact(150)]))
        );
        // One stat, so two values are the wrong count.
        assert_eq!(Affix::pinned(data, life, &[150, 150]), None);
        // The range is 145-159.
        assert_eq!(Affix::pinned(data, life, &[144]), None);
        assert_eq!(Affix::pinned(data, life, &[160]), None);
        // Both ends are inside.
        assert!(Affix::pinned(data, life, &[145]).is_some());
        assert!(Affix::pinned(data, life, &[159]).is_some());
    }

    #[test]
    fn stat_totals_add_the_implicit_and_two_explicits() {
        let data = data();
        let mut item = rare(data).with_influence(Influence::Shaper);
        let life = data.id_of("IncreasedLife10").unwrap();
        let all_resistances = data.id_of("AllResistances6").unwrap();
        item.affixes_mut(Side::Prefix)
            .push(Affix::at_range(data, life));
        item.affixes_mut(Side::Suffix)
            .push(Affix::at_range(data, all_resistances));

        let totals = item.stat_totals(data);
        assert!(
            totals.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "totals are sorted by stat id"
        );
        let stat = |name: &str| {
            let index = data
                .stat_ids
                .iter()
                .position(|id| id == name)
                .unwrap_or_else(|| panic!("the data has no stat {name:?}"));
            index as u16
        };
        let value = |name: &str| {
            let id = stat(name);
            totals
                .iter()
                .find(|&&(s, _)| s == id)
                .map(|&(_, roll)| roll)
                .unwrap_or_else(|| panic!("the totals have no {name:?}"))
        };

        assert_eq!(value("base_spell_suppression_chance_%"), Roll::exact(5));
        assert_eq!(value("base_maximum_life"), Roll::range(145, 159).unwrap());
        assert_eq!(
            value("base_resist_all_elements_%"),
            Roll::range(17, 18).unwrap()
        );
        assert_eq!(totals.len(), 3);
        assert_eq!(item.describe(data), "rare Supreme Spiked Shield (shaper)");
    }

    #[test]
    fn stat_totals_sum_intervals_and_pinned_values() {
        let data = data();
        // The base's implicit and this suffix share the spell-suppression stat, so an item holding
        // both sums two intervals: 5-5 plus 17-19.
        let suppression = data.id_of("ChanceToSuppressSpellsHigh4_").unwrap();
        let stat = data
            .stat_ids
            .iter()
            .position(|id| id == "base_spell_suppression_chance_%")
            .unwrap() as u16;

        let mut item = rare(data);
        item.affixes_mut(Side::Suffix)
            .push(Affix::at_range(data, suppression));
        assert_eq!(
            item.stat_totals(data),
            vec![(stat, Roll::range(22, 24).unwrap())]
        );

        let mut pinned = rare(data);
        pinned
            .affixes_mut(Side::Suffix)
            .push(Affix::pinned(data, suppression, &[18]).unwrap());
        assert_eq!(pinned.stat_totals(data), vec![(stat, Roll::exact(23))]);
    }

    #[test]
    fn cleared_keeps_everything_but_the_explicits() {
        let data = data();
        let mut item = rare(data).with_influence(Influence::Shaper);
        let life = data.id_of("IncreasedLife10").unwrap();
        item.affixes_mut(Side::Prefix)
            .push(Affix::at_range(data, life));
        let cleared = item.cleared();
        assert_eq!(cleared.explicits(), 0);
        assert_eq!(cleared.implicits(), item.implicits());
        assert_eq!(cleared.base, item.base);
        assert_eq!(cleared.item_level, item.item_level);
        assert_eq!(cleared.rarity, item.rarity);
        assert_eq!(cleared.influence, item.influence);
        assert_eq!(cleared.quality, item.quality);
        assert_eq!(cleared.corrupted, item.corrupted);
        assert_eq!(cleared.mirrored, item.mirrored);
    }
}
