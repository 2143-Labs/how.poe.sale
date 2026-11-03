//! Spawn weights and mod pools.

use crate::data::{Bundle, ModData};
use crate::item::{INFLUENCE_NAMES, Side};

/// Whether catalyst quality steers spawn weights (user-stated 3.29 behaviour).
/// Claim: catalyst.affects_weights
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

/// The mod's generation-weight multiplier on an item with `tags`: the first listed tag the item
/// has scales its spawn weight by `weight / 100`; no list or no match leaves it at 1.
// Claim: pool.generation_weights
pub fn generation_multiplier(m: &ModData, tags: &TagSet) -> f64 {
    m.gen_weights
        .iter()
        .find(|(tag, _)| tags.contains(*tag))
        .map_or(1.0, |(_, w)| *w as f64 / 100.0)
}

/// The weight a mod rolls with on an item with `tags`: spawn weight times generation multiplier.
pub fn effective_weight(m: &ModData, tags: &TagSet) -> f64 {
    spawn_weight(m, tags) as f64 * generation_multiplier(m, tags)
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

/// Changes a reforge makes to the pool beyond the filter: fossil weights and fossil-added mods.
#[derive(Clone, Debug, Default)]
pub struct PoolMod {
    /// Combined fossil multiplier per implicit tag (`0.0` blocks); empty means no fossils.
    pub tag_multipliers: Vec<(String, f64)>,
    /// Fossil-only mods the fossils add to the pool.
    pub added: Vec<u32>,
}

impl PoolMod {
    /// Fossil weights combine additively per tag: each fossil adds `weight / 100 - 1` to a base
    /// of 1 (so 1000 is ×10 alone), and any fossil listing a tag at 0 blocks it.
    pub fn fossils(b: &Bundle, fossils: &[u16]) -> PoolMod {
        let mut mult: Vec<(String, f64, bool)> = Vec::new();
        let mut added: Vec<u32> = Vec::new();
        for f in fossils {
            let Some(fossil) = b.fossils.get(*f as usize) else {
                continue;
            };
            for (tag, weight) in &fossil.weights {
                let slot = match mult.iter().position(|(t, _, _)| t == tag) {
                    Some(i) => i,
                    None => {
                        mult.push((tag.clone(), 1.0, false));
                        mult.len() - 1
                    }
                };
                if *weight == 0 {
                    mult[slot].2 = true;
                } else {
                    mult[slot].1 += *weight as f64 / 100.0 - 1.0;
                }
            }
            for id in &fossil.added_mods {
                if let Some(i) = b.mod_by_id(id)
                    && !added.contains(&i)
                {
                    added.push(i);
                }
            }
        }
        PoolMod {
            tag_multipliers: mult
                .into_iter()
                .map(|(t, m, blocked)| (t, if blocked { 0.0 } else { m.max(0.0) }))
                .collect(),
            added,
        }
    }

    fn multiplier(&self, m: &ModData) -> f64 {
        let mut out = 1.0;
        for (tag, factor) in &self.tag_multipliers {
            if m.implicit_tags_sorted.binary_search(tag).is_ok() {
                out *= factor;
            }
        }
        out
    }
}

/// Every mod that can spawn on the side under the filter, in bundle order, with its effective weight.
pub fn pool(b: &Bundle, f: &PoolFilter) -> Vec<PoolEntry> {
    pool_with(b, f, &PoolMod::default())
}

/// [`pool`] with fossil weights applied and fossil-only mods admitted when a fossil adds them.
pub fn pool_with(b: &Bundle, f: &PoolFilter, pm: &PoolMod) -> Vec<PoolEntry> {
    let mut out = Vec::new();
    for (i, m) in b.mods.iter().enumerate() {
        if m.crafted || m.generation != f.side || m.ilvl > f.ilvl as u16 {
            continue;
        }
        if m.fossil_only && !pm.added.contains(&(i as u32)) {
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
        let mut weight = effective_weight(m, f.tags) * pm.multiplier(m);
        if weight <= 0.0 {
            continue;
        }
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

/// How a mod can reach an item: the mechanic that can place it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub enum ModSource {
    /// Rolls naturally on the item as it is.
    Native,
    /// Rolls once the item gains this influence bit index (0 Shaper .. 5 Warlord).
    Influence(u8),
    /// Placed by this essence (`Bundle::essences` index).
    Essence(u16),
    /// Added to the pool by this fossil (`Bundle::fossils` index).
    Fossil(u16),
    /// A crafting-bench option for the item's class.
    Bench,
    /// Rolls on this other base of the same item class; reaches this item only by recombining.
    Foreign(u16),
}

impl ModSource {
    /// Sort rank for recipe ordering: the scarcer mechanic first, plain rolling last.
    pub fn rank(self) -> u8 {
        match self {
            ModSource::Essence(_) => 0,
            ModSource::Influence(_) => 1,
            ModSource::Fossil(_) => 2,
            ModSource::Foreign(_) => 3,
            ModSource::Bench => 4,
            ModSource::Native => 5,
        }
    }

    /// Display name, as the target editor's badge shows it.
    pub fn describe(self, b: &Bundle) -> String {
        match self {
            ModSource::Native => "Native".to_string(),
            ModSource::Influence(bit) => format!(
                "{} influence",
                INFLUENCE_NAMES
                    .get(bit as usize)
                    .copied()
                    .unwrap_or("Unknown")
            ),
            ModSource::Essence(e) => format!(
                "Essence: {}",
                b.essences
                    .get(e as usize)
                    .map(|x| x.name.as_str())
                    .unwrap_or("essence")
            ),
            ModSource::Fossil(f) => format!(
                "Fossil: {}",
                b.fossils
                    .get(f as usize)
                    .map(|x| x.name.as_str())
                    .unwrap_or("fossil")
            ),
            ModSource::Bench => "Bench".to_string(),
            ModSource::Foreign(base) => {
                format!("Recombine with {}", b.base_at(base).name)
            }
        }
    }
}

/// Every mechanic that can place `mod_index` on `base` carrying `influences`, in [`ModSource`]
/// order. Empty means no modelled mechanic can; only then is a requirement "not understood".
pub fn sources(b: &Bundle, base: u16, influences: u8, mod_index: u32) -> Vec<ModSource> {
    SourceCtx::new(b, base, influences).sources(mod_index)
}

/// The tag sets [`sources`] compares against, built once per (base, influences).
pub struct SourceCtx<'a> {
    b: &'a Bundle,
    base: u16,
    class: String,
    tags: TagSet,
    /// (influence bit index, tags with that influence added) for influences the item lacks and
    /// its class supports.
    with_influence: Vec<(u8, TagSet)>,
    /// Other bases of the same class, with their plain tags.
    siblings: Vec<(u16, TagSet)>,
}

impl<'a> SourceCtx<'a> {
    pub fn new(b: &'a Bundle, base: u16, influences: u8) -> SourceCtx<'a> {
        let class = b.base_at(base).class.clone();
        let tags = b.item_tags(base, influences);
        let with_influence = (0..INFLUENCE_NAMES.len() as u8)
            .filter(|bit| influences & (1 << bit) == 0)
            .map(|bit| (bit, b.item_tags(base, influences | (1 << bit))))
            .filter(|(_, with)| *with != tags)
            .collect();
        let siblings = (0..b.bases.len() as u16)
            .filter(|i| *i != base && b.base_at(*i).class == class)
            .map(|i| (i, b.item_tags(i, 0)))
            .collect();
        SourceCtx {
            b,
            base,
            class,
            tags,
            with_influence,
            siblings,
        }
    }

    pub fn base(&self) -> u16 {
        self.base
    }

    pub fn sources(&self, mod_index: u32) -> Vec<ModSource> {
        let b = self.b;
        let m = b.mod_at(mod_index);
        let mut out = Vec::new();
        let natural = !m.crafted && !m.fossil_only;
        let native = natural && spawn_weight(m, &self.tags) > 0;
        if native {
            out.push(ModSource::Native);
        }
        if natural && !native {
            for (bit, with) in &self.with_influence {
                if spawn_weight(m, with) > 0 {
                    out.push(ModSource::Influence(*bit));
                }
            }
        }
        for e in b.essences_for_mod(mod_index, &self.class) {
            out.push(ModSource::Essence(e));
        }
        // A fossil adds its mods to the pool, but they still need this item's tags to roll.
        if m.fossil_only && spawn_weight(m, &self.tags) > 0 {
            for f in b.fossils_adding(mod_index) {
                out.push(ModSource::Fossil(f));
            }
        }
        if b.bench_by_mod
            .get(&m.id)
            .is_some_and(|e| b.bench[*e as usize].classes.contains(&self.class))
        {
            out.push(ModSource::Bench);
        }
        if natural
            && !native
            && let Some((other, _)) = self
                .siblings
                .iter()
                .find(|(_, tags)| spawn_weight(m, tags) > 0)
        {
            out.push(ModSource::Foreign(*other));
        }
        out
    }
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
            // 143,700 less the Battle Royale cast-speed copy's amulet weight of 800.
            143_700.0 - 800.0
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
            // 150,740 less the same 800: the copy has no `attribute` tag, so the catalyst
            // never scaled it.
            150_740.0 - 800.0
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

    fn base_pool(name: &str, side: Side) -> f64 {
        let b = crate::test_bundle();
        let tags = b.item_tags(b.base_by_name(name).expect("base"), 0);
        pool(
            b,
            &PoolFilter {
                side,
                tags: &tags,
                ilvl: 84,
                excluded_groups: &[],
                influence_only: false,
                catalyst: None,
            },
        )
        .iter()
        .map(|e| e.weight)
        .sum()
    }

    #[test]
    fn generation_weights_halve_staff_attack_mods() {
        // A staff rolls the generation-weighted attack mods at half weight...
        assert_eq!(base_pool("Eclipse Staff", Side::Prefix), 77_870.0);
        assert_eq!(base_pool("Eclipse Staff", Side::Suffix), 78_075.0);
        // ...a warstaff at full weight, so its pools are the plain spawn-weight sums.
        assert_eq!(base_pool("Ezomyte Staff", Side::Prefix), 52_982.0);
        assert_eq!(base_pool("Ezomyte Staff", Side::Suffix), 87_400.0);
    }
}
