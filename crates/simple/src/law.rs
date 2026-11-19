//! The law of the next grade: on one side of an item, how often the next modifier drawn is
//! desired, good or anything else, given the grades already drawn there.
//!
//! `P(next = g | history h) = P(h + g) / P(h)`, where `P(h)` is the chance that successive weighted
//! draws without replacement — a drawn modifier takes its whole family out of the pool — come out
//! with the grades `h`, in that order. [`Law::new`] computes `P` by carrying, for every history,
//! the mass of each set of families that history could be holding.
//!
//! This is exact for an item that started cleared, and only for such an item: the history says
//! which grades were drawn, and the law averages over the modifiers they could have been, at the
//! odds a cleared item draws them. An item picked up holding a known modifier is not an average.
//! Four things the law checks or relies on:
//!
//! - the side is chosen first, in equal shares among the sides that can take a modifier
//!   ([`crate::pool::one_add`]), so a side's draws do not depend on the other side's weights;
//! - no family is on both sides, so one side's history takes nothing out of the other's pool
//!   (checked: [`LawError::SharedGroup`]);
//! - a side with room never runs out of modifiers within three draws (checked:
//!   [`LawError::DrySide`]);
//! - a modifier's weight depends on the base, the item level and the influence, which the start
//!   item fixes, and on nothing the item holds.
//!
//! A modifier a currency places rather than draws — an essence's own — leaves the law exact when
//! its family is in neither side's pool: it takes a slot, and the draws after it see the pool of a
//! cleared item. [`Law::placed`] names the modifiers that qualify, and a node keeps them out of the
//! history the law reads ([`Node::drawn`]).
//!
//! The chances are `f64`: three draws deep, the exact fraction's denominator has hundreds of bits.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Item, Rarity, Side};
use crate::node::Node;
use crate::pool::{draw_weights, rarity_cap};

/// The grades, in the order a law lists them.
const GRADES: [Tier; 3] = [Tier::Desired, Tier::Good, Tier::Any];

/// One side's table: history → the grades that can come next, each with its chance.
type Table = HashMap<Vec<Tier>, Vec<(Tier, f64)>>;

/// The table of each side, and the modifiers that can be placed without breaking it.
#[derive(Clone, Debug)]
pub struct Law {
    next: [Table; 2],
    /// Essence-only modifier id → its side and grade.
    placed: HashMap<String, (Side, Tier)>,
}

/// Why an item cannot start a law.
#[derive(Clone, Debug, PartialEq)]
pub enum LawError {
    /// The item holds a modifier: its history would be a fact, not an average.
    NotCleared,
    /// The item is corrupted or mirrored: nothing can be drawn on it.
    Locked,
    /// A modifier family both sides can roll, by name.
    SharedGroup(String),
    /// A side that can run out of modifiers while it still has room.
    DrySide(Side),
}

impl fmt::Display for LawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LawError::NotCleared => write!(f, "the item a law starts from holds a modifier"),
            LawError::Locked => write!(f, "the item a law starts from is corrupted or mirrored"),
            LawError::SharedGroup(group) => {
                write!(f, "the family {group} can roll on both sides")
            }
            LawError::DrySide(side) => {
                write!(f, "the {} side can run out of modifiers", side.name())
            }
        }
    }
}

/// One modifier of a side's pool, as the law reads it.
struct Entry {
    weight: f64,
    groups: Vec<u16>,
    tier: Tier,
}

impl Law {
    /// The law for the start item's base, item level and influence, under this grading.
    pub fn new(data: &Data, filter: &ItemSetFilter, start: &Item) -> Result<Law, LawError> {
        if start.explicits() > 0 {
            return Err(LawError::NotCleared);
        }
        if start.corrupted || start.mirrored {
            return Err(LawError::Locked);
        }
        let rare = start.with_rarity(Rarity::Rare);
        let pools = Side::BOTH.map(|side| {
            draw_weights(data, &rare, side)
                .into_iter()
                .map(|(m, weight)| Entry {
                    weight: weight as f64,
                    groups: data.mod_at(m).groups.clone(),
                    tier: filter.tier_of(m),
                })
                .collect::<Vec<Entry>>()
        });
        let prefix_groups: HashSet<u16> = pools[0]
            .iter()
            .flat_map(|entry| entry.groups.iter().copied())
            .collect();
        if let Some(shared) = pools[1]
            .iter()
            .flat_map(|entry| entry.groups.iter())
            .find(|group| prefix_groups.contains(group))
        {
            return Err(LawError::SharedGroup(data.group_name(*shared).to_string()));
        }
        // An essence-only modifier whose family no pool modifier belongs to takes nothing out of
        // either pool, so the draws after it are a cleared item's.
        let pooled: HashSet<u16> = pools
            .iter()
            .flatten()
            .flat_map(|entry| entry.groups.iter().copied())
            .collect();
        let placed = data
            .mods
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.essence_only)
            .filter(|(_, entry)| !entry.groups.iter().any(|group| pooled.contains(group)))
            .filter_map(|(m, entry)| {
                let side = entry.kind.side()?;
                Some((entry.id.clone(), (side, filter.tier_of(m as u16))))
            })
            .collect();
        Ok(Law {
            next: [
                side_law(&pools[0], Side::Prefix)?,
                side_law(&pools[1], Side::Suffix)?,
            ],
            placed,
        })
    }

    /// The grades that can come next on a side holding these grades, each with its chance, in the
    /// order desired, good, any. Empty for a side holding three.
    pub fn next(&self, side: Side, held: &[Tier]) -> &[(Tier, f64)] {
        self.next[side.index()]
            .get(held)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The side and grade of an essence-only modifier that can be placed on a cleared item with
    /// the law still exact after it, or `None` when the data lacks it or its family is in a pool.
    pub fn placed(&self, modifier: &str) -> Option<(Side, Tier)> {
        self.placed.get(modifier).copied()
    }

    /// One random modifier added to a node: the side in equal shares among the sides that can take
    /// one, then the grade by this law, read from the side's drawn grades. Empty when no side can
    /// take a modifier. This is [`crate::pool::one_add`] with the modifiers graded.
    pub fn add(&self, node: &Node) -> Vec<(Node, f64)> {
        let open: Vec<Side> = Side::BOTH
            .into_iter()
            .filter(|&side| rarity_cap(node.rarity) > node.side(side).len())
            .filter(|&side| !self.next(side, node.drawn(side)).is_empty())
            .collect();
        let share = 1.0 / open.len() as f64;
        open.iter()
            .flat_map(|&side| {
                self.next(side, node.drawn(side))
                    .iter()
                    .map(move |&(tier, chance)| (node.with(side, tier), chance * share))
            })
            .collect()
    }
}

/// One side's table. A level holds, for each history of that length, the mass of every set of
/// families the history could be holding; histories are kept in the order they were first reached,
/// so the sums are taken in the same order on every run.
fn side_law(pool: &[Entry], side: Side) -> Result<Table, LawError> {
    type Held = BTreeMap<Vec<u16>, f64>;
    let mut next = Table::new();
    let mut level: Vec<(Vec<Tier>, Held)> = vec![(Vec::new(), Held::from([(Vec::new(), 1.0)]))];
    for _ in 0..rarity_cap(Rarity::Rare) {
        let mut deeper: Vec<(Vec<Tier>, Held)> = Vec::new();
        for (history, held) in &level {
            let whole: f64 = held.values().sum();
            let mut drawn: [Held; 3] = Default::default();
            for (groups, mass) in held {
                let open: Vec<&Entry> = pool
                    .iter()
                    .filter(|entry| !entry.groups.iter().any(|group| groups.contains(group)))
                    .collect();
                let total: f64 = open.iter().map(|entry| entry.weight).sum();
                if total <= 0.0 {
                    return Err(LawError::DrySide(side));
                }
                for entry in open {
                    let mut now = groups.clone();
                    now.extend(&entry.groups);
                    now.sort_unstable();
                    now.dedup();
                    let slot = GRADES
                        .iter()
                        .position(|grade| *grade == entry.tier)
                        .expect("every grade is listed");
                    *drawn[slot].entry(now).or_insert(0.0) += mass * entry.weight / total;
                }
            }
            let mut odds = Vec::new();
            for (grade, held) in GRADES.into_iter().zip(drawn) {
                let mass: f64 = held.values().sum();
                if mass > 0.0 {
                    odds.push((grade, mass / whole));
                    let mut longer = history.clone();
                    longer.push(grade);
                    deeper.push((longer, held));
                }
            }
            next.insert(history.clone(), odds);
        }
        level = deeper;
    }
    Ok(next)
}
