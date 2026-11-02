//! The goal: a target item shape and the requirements on it.

use crate::data::Bundle;
use crate::item::{Hit, ItemState, Rarity, Side};

/// How a catalyst's quality combines with a base's explicit-modifier effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stacking {
    Additive,
    Multiplicative,
}

/// Milestone 1: the Simplex's `+100%` and a catalyst's quality add.
pub const CATALYST_STACKING: Stacking = Stacking::Additive;

#[derive(Clone, Debug, PartialEq)]
pub struct Requirement {
    pub group: u16,
    pub side: Side,
    /// Accepted mod indices (tiers), in bundle order.
    pub accepted: Vec<u32>,
    pub min_value: f64,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub base: u16,
    pub ilvl_min: u8,
    pub influences: u8,
    pub rarity: Rarity,
    pub catalyst: Option<(u8, u8)>,
    pub requirements: Vec<Requirement>,
    pub min_hits: u16,
}

/// The value a mod shows on this item: its base effect plus the catalyst quality where the
/// catalyst's tags match the mod's.
pub fn effect(b: &Bundle, base: u16, catalyst: Option<(u8, u8)>, mod_index: u32) -> f64 {
    let base_effect = b.base_at(base).explicit_effect as f64;
    let quality = match catalyst {
        Some((c, q)) => {
            let cat = &b.catalysts[c as usize];
            let m = b.mod_at(mod_index);
            if m.implicit_tags_sorted
                .iter()
                .any(|t| cat.tags.iter().any(|ct| ct == t))
            {
                q as f64
            } else {
                0.0
            }
        }
        None => 0.0,
    };
    match CATALYST_STACKING {
        Stacking::Additive => base_effect + quality,
        Stacking::Multiplicative => (100.0 + base_effect) * (100.0 + quality) / 100.0 - 100.0,
    }
}

/// A displayed value: multiply before truncating so integer rolls stay exact.
pub fn display(roll: f64, effect: f64) -> f64 {
    (roll * (100.0 + effect) / 100.0).trunc()
}

/// Probability that a roll of `accepted[idx]` meets the requirement's minimum value.
pub fn p_value_ok(b: &Bundle, t: &Target, req: u16, mod_index: u32) -> f64 {
    let Some(requirement) = t.requirements.get(req as usize) else {
        return 1.0;
    };
    let m = b.mod_at(mod_index);
    let Some(stat) = m.stats.first() else {
        return 1.0;
    };
    let eff = effect(b, t.base, t.catalyst, mod_index);
    let min = stat.min.floor() as i64;
    let max = stat.max.floor() as i64;
    if max < min {
        return 1.0;
    }
    let total = (max - min + 1) as f64;
    let ok = (min..=max)
        .filter(|roll| display(*roll as f64, eff) >= requirement.min_value)
        .count() as f64;
    ok / total
}

/// Does `s` meet the target?
pub fn satisfied(_b: &Bundle, t: &Target, s: &ItemState) -> bool {
    if s.base != t.base || s.ilvl < t.ilvl_min || s.rarity != t.rarity {
        return false;
    }
    if (s.influences & t.influences) != t.influences {
        return false;
    }
    if let Some((c, q)) = t.catalyst {
        match s.catalyst {
            Some((sc, sq)) if sc == c && sq >= q => {}
            _ => return false,
        }
    }
    let mut ok = 0u16;
    for side in [&s.prefixes, &s.suffixes] {
        for hit in &side.wanted {
            if hit.value_ok {
                ok += 1;
            }
        }
    }
    ok >= t.min_hits
}

/// The item the target describes, with every requirement present, all values acceptable, the
/// target catalyst applied and no junk or lock.
pub fn ideal_state(_b: &Bundle, t: &Target) -> ItemState {
    let mut s = ItemState::new(t.base, t.ilvl_min, t.rarity, t.influences);
    s.catalyst = t.catalyst;
    for (i, r) in t.requirements.iter().enumerate() {
        let hit = Hit {
            req: i as u16,
            idx: 0,
            value_ok: true,
        };
        s.side_mut(r.side).wanted.push(hit);
    }
    s.prefixes.wanted.sort();
    s.suffixes.wanted.sort();
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::INFLUENCE_NAMES;

    fn bits(names: &[&str]) -> u8 {
        let mut out = 0;
        for n in names {
            let i = INFLUENCE_NAMES.iter().position(|x| x == n).unwrap();
            out |= 1 << i;
        }
        out
    }

    fn target(b: &Bundle) -> Target {
        let base = b.base_by_name("Simplex Amulet").unwrap();
        let cat = b.catalyst_by_label("Attribute").unwrap();
        let req = |group: &str, side: Side, id: &str, min_value: f64, label: &str| Requirement {
            group: b.groups.iter().position(|g| g == group).unwrap() as u16,
            side,
            accepted: vec![b.mod_by_id(id).unwrap()],
            min_value,
            label: label.into(),
        };
        Target {
            base,
            ilvl_min: 84,
            influences: bits(&["Shaper", "Warlord"]),
            rarity: Rarity::Rare,
            catalyst: Some((cat, 20)),
            requirements: vec![
                req(
                    "DamagePer15Attributes",
                    Side::Prefix,
                    "DamagePer15StrengthInfluence1",
                    2.0,
                    "2% increased Damage per 15 Strength",
                ),
                req(
                    "PercentageStrength",
                    Side::Suffix,
                    "PercentageStrengthBodyInfluence2_",
                    26.0,
                    "26% increased Strength",
                ),
                req(
                    "PercentageAllAttributes",
                    Side::Suffix,
                    "PercentageAllAttributesUberShaper1",
                    19.0,
                    "19% increased Attributes",
                ),
            ],
            min_hits: 3,
        }
    }

    #[test]
    fn display_values() {
        assert_eq!(display(9.0, 120.0), 19.0);
        assert_eq!(display(12.0, 120.0), 26.0);
        assert_eq!(display(8.0, 120.0), 17.0);
        assert_eq!(display(1.0, 100.0), 2.0);
    }

    #[test]
    fn value_ok_probabilities() {
        let b = crate::test_bundle();
        let t = target(b);
        let attr = b.mod_by_id("PercentageAllAttributesUberShaper1").unwrap();
        let str_ = b.mod_by_id("PercentageStrengthBodyInfluence2_").unwrap();
        let dmg = b.mod_by_id("DamagePer15StrengthInfluence1").unwrap();
        assert_eq!(p_value_ok(b, &t, 2, attr), 0.25);
        assert_eq!(p_value_ok(b, &t, 1, str_), 0.25);
        assert_eq!(p_value_ok(b, &t, 0, dmg), 1.0);
    }

    #[test]
    fn ideal_state_satisfies() {
        let b = crate::test_bundle();
        let t = target(b);
        let s = ideal_state(b, &t);
        assert!(satisfied(b, &t, &s));
        assert_eq!(s.prefixes.wanted.len(), 1);
        assert_eq!(s.suffixes.wanted.len(), 2);
        assert_eq!(s.free(b, Side::Prefix), 0);
        assert_eq!(s.free(b, Side::Suffix), 0);
    }

    #[test]
    fn effect_stacks_additively() {
        let b = crate::test_bundle();
        let t = target(b);
        let attr = b.mod_by_id("PercentageAllAttributesUberShaper1").unwrap();
        let dmg = b.mod_by_id("DamagePer15StrengthInfluence1").unwrap();
        assert_eq!(effect(b, t.base, t.catalyst, attr), 120.0);
        assert_eq!(effect(b, t.base, t.catalyst, dmg), 100.0);
        assert_eq!(effect(b, t.base, None, attr), 100.0);
    }
}
