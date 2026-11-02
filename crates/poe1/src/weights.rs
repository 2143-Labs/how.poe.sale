//! Spawn weights and mod pools.

use crate::data::{Bundle, ModData};
use crate::item::{INFLUENCE_NAMES, Side};

/// Whether catalyst quality steers spawn weights (user-stated 3.29 behaviour).
pub const CATALYST_AFFECTS_WEIGHTS: bool = true;

/// A sorted, deduplicated set of tag indices: the tags that decide a mod's spawn weight.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagSet(Vec<u16>);

impl TagSet {
    pub fn new(iter: impl IntoIterator<Item = u16>) -> TagSet {
        let mut v: Vec<u16> = iter.into_iter().collect();
        v.sort_unstable();
        v.dedup();
        TagSet(v)
    }

    pub fn contains(&self, tag: u16) -> bool {
        self.0.binary_search(&tag).is_ok()
    }

    pub fn indices(&self) -> &[u16] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The game scans `spawn_weights` in order and the first tag present in the item's tag set wins;
/// `default` is the catch-all and a weight of 0 blocks the mod.
pub fn spawn_weight(m: &ModData, tags: &TagSet) -> u32 {
    for (tag, weight) in &m.weights {
        if tags.contains(*tag) {
            return *weight;
        }
    }
    0
}

impl Bundle {
    /// The influence bits whose class influence tag the mod spawns from (weight > 0).
    pub fn mod_influences(&self, base: u16, mod_index: u32) -> u8 {
        let b = self.base_at(base);
        let Some(class) = self.class_by_name(&b.class) else {
            return 0;
        };
        let m = self.mod_at(mod_index);
        let mut bits = 0;
        for (bit, name) in INFLUENCE_NAMES.iter().enumerate() {
            if let Some(tag) = class.influence_tags.get(*name)
                && m.weights.iter().any(|(t, w)| t == tag && *w > 0)
            {
                bits |= 1 << bit;
            }
        }
        bits
    }

    /// The base's tags plus the class's influence tags for every influence on the item.
    pub fn item_tags(&self, base: u16, influences: u8) -> TagSet {
        let b = self.base_at(base);
        let mut tags = b.tags.clone();
        if let Some(class) = self.class_by_name(&b.class) {
            for (bit, name) in INFLUENCE_NAMES.iter().enumerate() {
                if influences & (1 << bit) != 0
                    && let Some(tag) = class.influence_tags.get(*name)
                {
                    tags.push(*tag);
                }
            }
        }
        TagSet::new(tags)
    }
}

pub struct PoolFilter<'a> {
    pub side: Side,
    pub tags: &'a TagSet,
    pub ilvl: u8,
    pub excluded_groups: &'a [u16],
    pub influence_only: bool,
    /// (catalyst tags, quality 0..=20)
    pub catalyst: Option<(&'a [String], u8)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoolEntry {
    pub mod_index: u32,
    pub group: u16,
    pub weight: f64,
    pub influence: bool,
}

/// Every mod that can spawn on the side under the filter, in bundle order, with its effective weight.
pub fn pool(b: &Bundle, f: &PoolFilter) -> Vec<PoolEntry> {
    let mut out = Vec::new();
    for (i, m) in b.mods.iter().enumerate() {
        if m.crafted || m.generation != f.side || m.ilvl > f.ilvl as u16 {
            continue;
        }
        if f.excluded_groups.contains(&m.group) {
            continue;
        }
        if f.influence_only && !m.influence {
            continue;
        }
        let base = spawn_weight(m, f.tags);
        if base == 0 {
            continue;
        }
        let mut weight = base as f64;
        if CATALYST_AFFECTS_WEIGHTS
            && let Some((tags, quality)) = f.catalyst
            && m.implicit_tags_sorted
                .iter()
                .any(|t| tags.iter().any(|ct| ct == t))
        {
            weight *= 1.0 + quality as f64 / 100.0;
        }
        out.push(PoolEntry {
            mod_index: i as u32,
            group: m.group,
            weight,
            influence: m.influence,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::INFLUENCE_NAMES;

    fn simplex() -> (u16, u8) {
        let b = crate::test_bundle();
        (b.base_by_name("Simplex Amulet").expect("base"), 84)
    }

    fn influence_bits(names: &[&str]) -> u8 {
        let mut bits = 0;
        for n in names {
            let i = INFLUENCE_NAMES
                .iter()
                .position(|x| x == n)
                .expect("influence");
            bits |= 1 << i;
        }
        bits
    }

    fn total(
        b: &Bundle,
        side: Side,
        ilvl: u8,
        influences: u8,
        catalyst: Option<(&[String], u8)>,
    ) -> f64 {
        let base = simplex().0;
        let tags = b.item_tags(base, influences);
        pool(
            b,
            &PoolFilter {
                side,
                tags: &tags,
                ilvl,
                excluded_groups: &[],
                influence_only: false,
                catalyst,
            },
        )
        .iter()
        .map(|e| e.weight)
        .sum()
    }

    #[test]
    fn simplex_influenced_prefix_pool() {
        let b = crate::test_bundle();
        let (base, ilvl) = simplex();
        let tags = b.item_tags(base, influence_bits(&["Shaper", "Warlord"]));
        let entries = pool(
            b,
            &PoolFilter {
                side: Side::Prefix,
                tags: &tags,
                ilvl,
                excluded_groups: &[],
                influence_only: true,
                catalyst: None,
            },
        );
        let total: f64 = entries.iter().map(|e| e.weight).sum();
        assert_eq!(total, 11_700.0);
        assert_eq!(entries.len(), 19);
        let target = b.mod_by_id("DamagePer15StrengthInfluence1").expect("mod");
        let share = entries
            .iter()
            .find(|e| e.mod_index == target)
            .expect("target in pool")
            .weight
            / total;
        assert!((share - 500.0 / 11_700.0).abs() < 1e-12, "{share}");
    }

    #[test]
    fn simplex_all_prefix_pool_ilvl84() {
        let b = crate::test_bundle();
        assert_eq!(
            total(
                b,
                Side::Prefix,
                84,
                influence_bits(&["Shaper", "Warlord"]),
                None
            ),
            99_000.0
        );
    }

    #[test]
    fn simplex_all_suffix_pool_ilvl84() {
        let b = crate::test_bundle();
        assert_eq!(
            total(
                b,
                Side::Suffix,
                84,
                influence_bits(&["Shaper", "Warlord"]),
                None
            ),
            143_700.0
        );
    }

    #[test]
    fn simplex_all_suffix_pool_ilvl84_intrinsic20() {
        let b = crate::test_bundle();
        let tags = vec!["attribute".to_string()];
        assert_eq!(
            total(
                b,
                Side::Suffix,
                84,
                influence_bits(&["Shaper", "Warlord"]),
                Some((&tags, 20))
            ),
            150_740.0
        );
    }

    #[test]
    fn simplex_all_prefix_pool_ilvl84_intrinsic20() {
        let b = crate::test_bundle();
        let tags = vec!["attribute".to_string()];
        assert_eq!(
            total(
                b,
                Side::Prefix,
                84,
                influence_bits(&["Shaper", "Warlord"]),
                Some((&tags, 20))
            ),
            99_000.0
        );
    }
}
