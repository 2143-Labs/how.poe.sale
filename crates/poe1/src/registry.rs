//! Costs, timing and applicability of the craft actions.

use crate::data::Bundle;
use crate::item::{INFLUENCE_NAMES, ItemState, Rarity, Side};
use crate::kernels::Action;
use crate::target::Target;

/// Milestone 1: the harvest influenced reforge respects "Cannot Be Changed" metamods.
pub const HARVEST_INFLUENCED_RESPECTS_LOCKS: bool = true;
/// Milestone 1: the harvest influenced reforge's first new mod comes from the influenced pool.
pub const HARVEST_FIRST_DRAW_INFLUENCED: bool = true;
/// Catalysts to take an item from 0% to 20% quality (conservative constant).
pub const CATALYSTS_FOR_FULL_QUALITY: f64 = 20.0;

/// Price key of each influence's exalted orb, by influence bit.
pub const INFLUENCE_EXALT_KEYS: [&str; 6] = [
    "shapers-exalted-orb",
    "elders-exalted-orb",
    "crusaders-exalted-orb",
    "redeemers-exalted-orb",
    "hunters-exalted-orb",
    "warlords-exalted-orb",
];

/// A declared purchase used as an Awakener's Orb donor.
#[derive(Clone, Debug, PartialEq)]
pub struct DonorSpec {
    pub label: String,
    pub base: u16,
    pub ilvl: u8,
    /// Single influence bit.
    pub influence: u8,
    pub mod_index: u32,
    pub price_chaos: f64,
}

impl DonorSpec {
    pub fn influence_name(&self) -> &'static str {
        influence_name(self.influence).unwrap_or("Unknown")
    }
}

pub fn influence_name(bit: u8) -> Option<&'static str> {
    if bit.count_ones() != 1 {
        return None;
    }
    INFLUENCE_NAMES.get(bit.trailing_zeros() as usize).copied()
}

fn bench_mod_id(locked: Side) -> &'static str {
    match locked {
        Side::Suffix => "DexMasterItemGenerationCannotChangeSuffixes",
        Side::Prefix => "StrMasterItemGenerationCannotChangePrefixes",
    }
}

/// The slot the metamod itself occupies: a Suffix lock is a crafted prefix and vice versa.
fn crafted_slot(locked: Side) -> Side {
    locked.other()
}

pub fn side_locked(s: &ItemState, side: Side) -> bool {
    match side {
        Side::Prefix => s.lock & 0b01 != 0,
        Side::Suffix => s.lock & 0b10 != 0,
    }
}

fn lock_bit(side: Side) -> u8 {
    match side {
        Side::Prefix => 0b01,
        Side::Suffix => 0b10,
    }
}

fn class_supports(b: &Bundle, s: &ItemState, bit: u8) -> bool {
    let class = b.base_at(s.base).class.clone();
    match influence_name(bit) {
        Some(name) => b
            .class_by_name(&class)
            .is_some_and(|c| c.influence_tags.contains_key(name)),
        None => false,
    }
}

/// The lock mod's bench entry, if the base's class may use it.
fn bench_entry(b: &Bundle, base: u16, locked: Side) -> Option<&crate::data::BenchEntry> {
    let class = &b.base_at(base).class;
    let entry = b.bench_by_mod.get(bench_mod_id(locked))?;
    let entry = &b.bench[*entry as usize];
    entry.classes.contains(class).then_some(entry)
}

/// Mods the action will remove / reroll: the item's real mods, ignoring the lock metamod.
fn removable_count(s: &ItemState) -> u8 {
    let n = s.prefixes.wanted.len()
        + s.prefixes.blocked.len()
        + s.prefixes.untracked as usize
        + s.suffixes.wanted.len()
        + s.suffixes.blocked.len()
        + s.suffixes.untracked as usize
        + usize::from(s.lock != 0);
    n as u8
}

/// Number of draws a reforge would make at most: `max k - kept`.
fn reforge_capacity(b: &Bundle, s: &ItemState, respect_locks: bool) -> (u8, u8) {
    let (keep_p, keep_s) = if respect_locks {
        (side_locked(s, Side::Prefix), side_locked(s, Side::Suffix))
    } else {
        (false, false)
    };
    let kept = if keep_p { s.prefixes.slots_used() } else { 0 }
        + if keep_s { s.suffixes.slots_used() } else { 0 };
    let (cap_p, cap_s) = s.caps(b);
    let capacity = (cap_p + cap_s).saturating_sub(kept);
    (kept, capacity)
}

/// Every action the engine offers on this target, catalysts included.
pub fn offered_catalysts(b: &Bundle, t: &Target) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    if let Some((c, _)) = t.catalyst {
        out.push(c);
    }
    for r in &t.requirements {
        for mod_index in &r.accepted {
            let m = b.mod_at(*mod_index);
            for (i, cat) in b.catalysts.iter().enumerate() {
                if cat
                    .tags
                    .iter()
                    .any(|ct| m.implicit_tags_sorted.iter().any(|t| t == ct))
                    && !out.contains(&(i as u8))
                {
                    out.push(i as u8);
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// (price key, quantity) for one use of `a`.
pub fn cost(b: &Bundle, a: &Action, _donors: &[DonorSpec]) -> Vec<(String, f64)> {
    match a {
        Action::Chaos => vec![("chaos".into(), 1.0)],
        Action::Exalted => vec![("exalted".into(), 1.0)],
        Action::InfluenceExalt { influence } => {
            let i = (0..6).find(|i| *influence == 1 << i).unwrap_or(0);
            vec![(INFLUENCE_EXALT_KEYS[i].into(), 1.0)]
        }
        Action::Annul => vec![("annul".into(), 1.0)],
        Action::Divine => vec![("divine".into(), 1.0)],
        Action::HarvestInfluenced => {
            vec![
                ("primal-lifeforce".into(), 5_000.0),
                ("sacred-lifeforce".into(), 1.0),
            ]
        }
        Action::HarvestRandomiseInfluence => vec![("primal-lifeforce".into(), 1_000.0)],
        Action::BenchLock { .. } => vec![("divine".into(), 2.0)],
        Action::ApplyCatalyst { catalyst } => vec![(
            b.catalysts[*catalyst as usize].price_key.clone(),
            CATALYSTS_FOR_FULL_QUALITY,
        )],
        Action::Combine { donor } => vec![
            (format!("donor:{donor}"), 1.0),
            ("awakeners-orb".into(), 1.0),
        ],
    }
}

pub fn seconds(a: &Action) -> f64 {
    match a {
        Action::Chaos | Action::Exalted | Action::Annul | Action::Divine => 2.0,
        Action::InfluenceExalt { .. } => 3.0,
        Action::HarvestInfluenced | Action::HarvestRandomiseInfluence => 12.0,
        Action::BenchLock { .. } => 8.0,
        Action::ApplyCatalyst { .. } => 4.0,
        Action::Combine { .. } => 10.0,
    }
}

pub fn applicable(b: &Bundle, t: &Target, s: &ItemState, a: &Action, donors: &[DonorSpec]) -> bool {
    if s.rarity != Rarity::Rare {
        return false;
    }
    match a {
        Action::Chaos => true,
        Action::Exalted => s.any_free(b),
        Action::InfluenceExalt { influence } => {
            s.influences == 0 && s.any_free(b) && class_supports(b, s, *influence)
        }
        Action::Annul => removable_count(s) > 0,
        Action::Divine => Side::ALL
            .iter()
            .any(|side| !side_locked(s, *side) && !s.side(*side).wanted.is_empty()),
        Action::HarvestInfluenced => {
            if s.influences == 0 {
                return false;
            }
            let (kept, capacity) = reforge_capacity(b, s, HARVEST_INFLUENCED_RESPECTS_LOCKS);
            capacity > 0 && kept < 6
        }
        Action::HarvestRandomiseInfluence => {
            s.influences.count_ones() == 1 && s.lock == 0 && !prefix_has_influence(b, t, s)
        }
        Action::BenchLock { side } => {
            s.lock == 0
                && bench_entry(b, s.base, *side).is_some()
                && s.free(b, crafted_slot(*side)) > 0
        }
        Action::ApplyCatalyst { catalyst } => s.catalyst != Some((*catalyst, 20)),
        Action::Combine { donor } => {
            let Some(d) = donors.get(*donor as usize) else {
                return false;
            };
            s.influences.count_ones() == 1
                && d.influence.count_ones() == 1
                && d.influence & s.influences == 0
                && b.base_at(d.base).class == b.base_at(s.base).class
        }
    }
}

pub fn prefix_has_influence(b: &Bundle, t: &Target, s: &ItemState) -> bool {
    let p = &s.prefixes;
    p.untracked_influenced > 0
        || p.blocked.iter().any(|x| x.influence)
        || p.wanted.iter().any(|h| hit_is_influence_t(b, t, h))
}

/// Whether the hit's accepted mod carries an influence tag.
pub fn hit_is_influence_t(b: &Bundle, t: &Target, hit: &crate::item::Hit) -> bool {
    t.requirements
        .get(hit.req as usize)
        .and_then(|r| r.accepted.get(hit.idx as usize))
        .is_some_and(|i| b.mod_at(*i).influence)
}

/// Lock bit for a locked side (bit0 Prefixes Cannot Be Changed, bit1 Suffixes Cannot Be Changed).
pub fn lock_bit_for(side: Side) -> u8 {
    lock_bit(side)
}

/// The target's requirements grouped for quick lookup: (side, group) -> requirement index.
pub fn requirement_for(t: &Target, side: Side, group: u16) -> Option<u16> {
    t.requirements
        .iter()
        .position(|r| r.side == side && r.group == group)
        .map(|i| i as u16)
}
