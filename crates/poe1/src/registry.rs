//! Costs, timing and applicability of the craft actions.

use crate::data::Bundle;
use crate::item::{INFLUENCE_NAMES, ItemState, Rarity, Side};
use crate::kernels::Action;
use crate::target::Target;

/// Milestone 1: the harvest influenced reforge respects "Cannot Be Changed" metamods.
/// Claim: harvest.influenced_respects_locks
pub const HARVEST_INFLUENCED_RESPECTS_LOCKS: bool = true;
/// Milestone 1: the harvest influenced reforge's first new mod comes from the influenced pool.
/// Claim: harvest.first_draw_influenced
pub const HARVEST_FIRST_DRAW_INFLUENCED: bool = true;
/// Catalysts to take an item from 0% to 20% quality (conservative constant).
/// Claim: catalyst.full_quality_count
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

/// Price keys of the chaotic resonators, by socket count 1..=4.
pub const RESONATOR_KEYS: [&str; 4] = [
    "primitive-chaotic-resonator",
    "potent-chaotic-resonator",
    "powerful-chaotic-resonator",
    "prime-chaotic-resonator",
];

/// What a craft does to one affix side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideEffect {
    /// Unchanged.
    Keep,
    /// Emptied, unless the item's own lock protects it.
    Clear,
    /// Replaced by at most `n` fresh mods (a locked side is kept).
    Reroll(u8),
    /// Kept, and the action adds exactly `n` mods over both sides together.
    Add(u8),
    /// Kept minus exactly `n` mods over both sides together (or the lock).
    Remove(u8),
}

/// What a craft does to an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub prefixes: SideEffect,
    pub suffixes: SideEffect,
    /// The rarity afterwards, when the craft sets one.
    pub rarity: Option<Rarity>,
    pub consumes_quality: bool,
}

/// What an item must be for a craft to apply. Action-specific numeric checks (donor class,
/// essence class, fossil tags, reforge capacity) stay in [`applicable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pre {
    pub rarity: Option<Rarity>,
    /// No "Cannot Be Changed" metamod on the item.
    pub lock_free: bool,
    pub max_influences: Option<u8>,
    /// Free affix slots needed (on either side).
    pub needs_free: u8,
}

/// The declared behaviour of one craft: the single table the UI, `applicable` and the tests read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contract {
    pub id: &'static str,
    pub name: &'static str,
    pub requires: Pre,
    pub effect: Effect,
    pub seconds: f64,
}

const fn pre(
    rarity: Option<Rarity>,
    lock_free: bool,
    max_influences: Option<u8>,
    needs_free: u8,
) -> Pre {
    Pre {
        rarity,
        lock_free,
        max_influences,
        needs_free,
    }
}

const fn eff(prefixes: SideEffect, suffixes: SideEffect, rarity: Option<Rarity>) -> Effect {
    Effect {
        prefixes,
        suffixes,
        rarity,
        consumes_quality: true,
    }
}

const RARE: Option<Rarity> = Some(Rarity::Rare);
const MAGIC: Option<Rarity> = Some(Rarity::Magic);
const R3: SideEffect = SideEffect::Reroll(3);
const R1: SideEffect = SideEffect::Reroll(1);
const KEEP: SideEffect = SideEffect::Keep;

/// Every craft, in `Action` declaration order, followed by the two-item recombine.
pub const CONTRACTS: [Contract; 20] = [
    Contract {
        id: "chaos",
        name: "Chaos Orb",
        requires: pre(RARE, false, None, 0),
        effect: eff(R3, R3, None),
        seconds: 2.0,
    },
    Contract {
        id: "exalted",
        name: "Exalted Orb",
        requires: pre(RARE, false, None, 1),
        effect: eff(SideEffect::Add(1), SideEffect::Add(1), None),
        seconds: 2.0,
    },
    Contract {
        id: "influence-exalt",
        name: "Influenced Exalted Orb",
        requires: pre(RARE, false, Some(0), 1),
        effect: eff(SideEffect::Add(1), SideEffect::Add(1), None),
        seconds: 3.0,
    },
    Contract {
        id: "annul",
        name: "Orb of Annulment",
        requires: pre(RARE, false, None, 0),
        effect: eff(SideEffect::Remove(1), SideEffect::Remove(1), None),
        seconds: 2.0,
    },
    Contract {
        id: "divine",
        name: "Divine Orb",
        requires: pre(RARE, false, None, 0),
        effect: eff(KEEP, KEEP, None),
        seconds: 2.0,
    },
    Contract {
        id: "harvest-influenced",
        name: "Harvest: reforge influenced rare",
        requires: pre(RARE, false, None, 0),
        effect: eff(R3, R3, None),
        seconds: 12.0,
    },
    Contract {
        id: "harvest-randomise-influence",
        name: "Harvest: randomise influence",
        requires: pre(RARE, true, Some(1), 0),
        effect: eff(R3, R3, None),
        seconds: 12.0,
    },
    Contract {
        id: "bench-lock",
        name: "Bench: Cannot Be Changed",
        requires: pre(RARE, true, None, 0),
        effect: eff(KEEP, KEEP, None),
        seconds: 8.0,
    },
    Contract {
        id: "apply-catalyst",
        name: "Catalyst",
        requires: pre(RARE, false, None, 0),
        effect: Effect {
            prefixes: KEEP,
            suffixes: KEEP,
            rarity: None,
            consumes_quality: false,
        },
        seconds: 4.0,
    },
    Contract {
        id: "awakener",
        name: "Awakener's Orb",
        requires: pre(RARE, false, Some(1), 0),
        effect: eff(R3, R3, None),
        seconds: 10.0,
    },
    Contract {
        id: "transmute",
        name: "Orb of Transmutation",
        requires: pre(Some(Rarity::Normal), false, None, 0),
        effect: eff(R1, R1, MAGIC),
        seconds: 2.0,
    },
    Contract {
        id: "alteration",
        name: "Orb of Alteration",
        requires: pre(MAGIC, false, None, 0),
        effect: eff(R1, R1, MAGIC),
        seconds: 1.0,
    },
    Contract {
        id: "augmentation",
        name: "Orb of Augmentation",
        requires: pre(MAGIC, false, None, 1),
        effect: eff(SideEffect::Add(1), SideEffect::Add(1), MAGIC),
        seconds: 1.0,
    },
    Contract {
        id: "regal",
        name: "Regal Orb",
        requires: pre(MAGIC, false, None, 0),
        effect: eff(SideEffect::Add(1), SideEffect::Add(1), RARE),
        seconds: 2.0,
    },
    Contract {
        id: "scour",
        name: "Orb of Scouring",
        requires: pre(None, false, None, 0),
        effect: eff(SideEffect::Clear, SideEffect::Clear, None),
        seconds: 2.0,
    },
    Contract {
        id: "imprint",
        name: "Craicic Chimeral: imprint",
        requires: pre(MAGIC, false, None, 0),
        effect: eff(KEEP, KEEP, MAGIC),
        seconds: 30.0,
    },
    Contract {
        id: "restore",
        name: "Restore imprint",
        requires: pre(None, false, None, 0),
        effect: eff(R1, R1, MAGIC),
        seconds: 5.0,
    },
    Contract {
        id: "essence",
        name: "Essence",
        requires: pre(None, false, None, 0),
        effect: eff(R3, R3, RARE),
        seconds: 3.0,
    },
    Contract {
        id: "fossil",
        name: "Fossil reforge",
        requires: pre(None, false, None, 0),
        effect: eff(R3, R3, RARE),
        seconds: 6.0,
    },
    Contract {
        id: "recombine",
        name: "Recombinator",
        requires: pre(None, false, None, 0),
        effect: eff(R3, R3, None),
        seconds: 20.0,
    },
];

/// The recombine's contract (it is a market process, not an [`Action`]).
pub fn recombine_contract() -> &'static Contract {
    &CONTRACTS[19]
}

/// The declared contract of `a`.
pub fn contract(a: &Action) -> &'static Contract {
    let i = match a {
        Action::Chaos => 0,
        Action::Exalted => 1,
        Action::InfluenceExalt { .. } => 2,
        Action::Annul => 3,
        Action::Divine => 4,
        Action::HarvestInfluenced => 5,
        Action::HarvestRandomiseInfluence => 6,
        Action::BenchLock { .. } => 7,
        Action::ApplyCatalyst { .. } => 8,
        Action::Combine { .. } => 9,
        Action::Transmute => 10,
        Action::Alteration => 11,
        Action::Augmentation => 12,
        Action::Regal => 13,
        Action::Scour => 14,
        Action::Imprint => 15,
        Action::Restore => 16,
        Action::Essence { .. } => 17,
        Action::Fossil { .. } => 18,
    };
    &CONTRACTS[i]
}

/// The contract with this id.
pub fn contract_by_id(id: &str) -> Option<&'static Contract> {
    CONTRACTS.iter().find(|c| c.id == id)
}

/// The claims (`rules/poe1-claims.json`) the kernel of `a` reads.
pub fn action_claims(a: &Action) -> &'static [&'static str] {
    match a {
        Action::Chaos
        | Action::Essence { .. }
        | Action::Fossil { .. }
        | Action::HarvestRandomiseInfluence
        | Action::Combine { .. } => &["affix.chaos_count_ratio"],
        Action::HarvestInfluenced => &[
            "affix.chaos_count_ratio",
            "harvest.influenced_respects_locks",
            "harvest.first_draw_influenced",
        ],
        Action::Transmute | Action::Alteration => &["magic.mod_count"],
        Action::ApplyCatalyst { .. } => {
            &["catalyst.full_quality_count", "catalyst.affects_weights"]
        }
        _ => &[],
    }
}

/// Fossil sets the engine offers per state, best single-draw share first.
pub const FOSSIL_TOP_K: usize = 3;

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
        Action::Transmute => vec![("transmute".into(), 1.0)],
        Action::Alteration => vec![("alt".into(), 1.0)],
        Action::Augmentation => vec![("aug".into(), 1.0)],
        Action::Regal => vec![("regal".into(), 1.0)],
        Action::Scour => vec![("scour".into(), 1.0)],
        Action::Imprint => vec![("craicic-croaker".into(), 1.0)],
        Action::Restore => Vec::new(),
        Action::Essence { essence } => vec![(
            b.essences
                .get(*essence as usize)
                .map(|e| e.price_key.clone())
                .unwrap_or_else(|| "essence".into()),
            1.0,
        )],
        Action::Fossil { fossils } => {
            let mut out: Vec<(String, f64)> = fossils
                .iter()
                .filter_map(|f| b.fossils.get(*f as usize))
                .map(|f| (f.price_key.clone(), 1.0))
                .collect();
            out.push((RESONATOR_KEYS[fossils.len().clamp(1, 4) - 1].into(), 1.0));
            out
        }
    }
}

pub fn seconds(a: &Action) -> f64 {
    contract(a).seconds
}

pub fn applicable(b: &Bundle, t: &Target, s: &ItemState, a: &Action, donors: &[DonorSpec]) -> bool {
    let c = contract(a);
    if let Some(r) = c.requires.rarity
        && s.rarity != r
    {
        return false;
    }
    if c.requires.lock_free && s.lock != 0 {
        return false;
    }
    if let Some(max) = c.requires.max_influences
        && s.influences.count_ones() as u8 > max
    {
        return false;
    }
    if c.requires.needs_free > 0 && !s.any_free(b) {
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
        Action::Transmute | Action::Augmentation | Action::Regal => true,
        Action::Alteration => true,
        Action::Scour => s.mod_count() > 0 || s.lock != 0,
        Action::Imprint => s.imprint.is_none() && s.mod_count() > 0,
        Action::Restore => s.imprint.is_some(),
        Action::Essence { essence } => {
            let class = &b.base_at(s.base).class;
            let Some(m) = b.essence_mod(*essence, class) else {
                return false;
            };
            matches!(s.rarity, Rarity::Normal | Rarity::Rare)
                && !side_locked(s, b.mod_at(m).generation)
                && b.mod_at(m).ilvl <= s.ilvl as u16
        }
        Action::Fossil { fossils } => {
            if !matches!(s.rarity, Rarity::Normal | Rarity::Rare)
                || fossils.is_empty()
                || fossils.len() > 4
            {
                return false;
            }
            let tags = &b.base_at(s.base).tags;
            fossils.iter().all(|f| {
                b.fossils.get(*f as usize).is_some_and(|fossil| {
                    fossil.supported
                        && fossil
                            .forbidden_tags
                            .iter()
                            .all(|t| !tags.iter().any(|x| b.tag(*x) == t))
                        && (fossil.allowed_tags.is_empty()
                            || fossil
                                .allowed_tags
                                .iter()
                                .any(|t| tags.iter().any(|x| b.tag(*x) == t)))
                })
            })
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
