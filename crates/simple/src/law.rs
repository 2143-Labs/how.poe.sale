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
//!   (checked by [`Law::new`]: [`LawError::SharedGroup`]; [`Law::sides_apart`] builds the tables
//!   anyway, each side as if it were alone, and names the families it ignored);
//! - a side with room never runs out of modifiers within the draws a rare side holds (checked:
//!   [`LawError::DrySide`]);
//! - a modifier's weight depends on the base, the item level and the influence, which the start
//!   item fixes, and on nothing the item holds.
//!
//! The chances are `f64`: three draws deep, the exact fraction's denominator has hundreds of bits.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Item, Rarity, Side};
use crate::node::Node;
use crate::pool::{Caps, draw_weights};

/// The grades, in the order a law lists them.
const GRADES: [Tier; 3] = [Tier::Desired, Tier::Good, Tier::Any];

/// One side's table: history → the grades that can come next, each with its chance.
type Table = HashMap<Vec<Tier>, Vec<(Tier, f64)>>;

/// The table of each side, and the base's caps it was built for.
#[derive(Clone, Debug)]
pub struct Law {
    next: [Table; 2],
    caps: Caps,
    /// The families both sides can roll, which the tables ignore. Empty unless built by
    /// [`Law::sides_apart`].
    shared: Vec<String>,
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
    ///
    /// Refuses a pool where a family can roll on both sides ([`LawError::SharedGroup`]): there no
    /// table over grades is exact (see [`Law::sides_apart`]).
    pub fn new(data: &Data, filter: &ItemSetFilter, start: &Item) -> Result<Law, LawError> {
        Law::build(data, filter, start, false)
    }

    /// The law with each side read as if it were alone, even where a family can roll on both
    /// sides; [`Law::shared`] names those families.
    ///
    /// On such a pool this is not the game's draw. A modifier of a shared family held on one side
    /// takes the family out of the other side's pool ([`crate::pool::held_groups`]), so the other
    /// side's next grade depends on which side holds it, and, three draws deep, on when it was
    /// drawn — and a [`Node`] records neither. The law's chance is still the game's at a node none
    /// of whose grades includes a modifier of a shared family, because no item that node stands
    /// for holds one. A craft built on this law has to show that the chances its moves turn on are
    /// of that kind, or say what the rest costs.
    pub fn sides_apart(data: &Data, filter: &ItemSetFilter, start: &Item) -> Result<Law, LawError> {
        Law::build(data, filter, start, true)
    }

    fn build(
        data: &Data,
        filter: &ItemSetFilter,
        start: &Item,
        apart: bool,
    ) -> Result<Law, LawError> {
        if start.explicits() > 0 {
            return Err(LawError::NotCleared);
        }
        if start.corrupted || start.mirrored {
            return Err(LawError::Locked);
        }
        let caps = data.base_at(start.base).caps;
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
        let mut shared: Vec<String> = Vec::new();
        for group in pools[1].iter().flat_map(|entry| entry.groups.iter()) {
            if !prefix_groups.contains(group) {
                continue;
            }
            let name = data.group_name(*group).to_string();
            if !apart {
                return Err(LawError::SharedGroup(name));
            }
            if !shared.contains(&name) {
                shared.push(name);
            }
        }
        Ok(Law {
            next: [
                side_law(&pools[0], Side::Prefix, caps.rare)?,
                side_law(&pools[1], Side::Suffix, caps.rare)?,
            ],
            caps,
            shared,
        })
    }

    /// The caps of the base the law was built for.
    pub fn caps(&self) -> Caps {
        self.caps
    }

    /// The families both sides can roll, which this law reads each side without. Empty for a law
    /// from [`Law::new`].
    pub fn shared(&self) -> &[String] {
        &self.shared
    }

    /// The grades that can come next on a side holding these grades, each with its chance, in the
    /// order desired, good, any. Empty for a side as full as a rare side can be.
    pub fn next(&self, side: Side, held: &[Tier]) -> &[(Tier, f64)] {
        self.next[side.index()]
            .get(held)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// One random modifier added to a node: the side in equal shares among the sides that can take
    /// one, then the grade by this law. Empty when no side can take a modifier. This is
    /// [`crate::pool::one_add`] with the modifiers graded.
    pub fn add(&self, node: &Node) -> Vec<(Node, f64)> {
        let open: Vec<Side> = Side::BOTH
            .into_iter()
            .filter(|&side| self.caps.at(node.rarity) > node.side(side).len())
            .filter(|&side| !self.next(side, node.side(side)).is_empty())
            .collect();
        let share = 1.0 / open.len() as f64;
        open.iter()
            .flat_map(|&side| {
                self.next(side, node.side(side))
                    .iter()
                    .map(move |&(tier, chance)| (node.with(side, tier), chance * share))
            })
            .collect()
    }
}

/// One side's table. A level holds, for each history of that length, the mass of every set of
/// families the history could be holding; histories are kept in the order they were first reached,
/// so the sums are taken in the same order on every run.
fn side_law(pool: &[Entry], side: Side, draws: usize) -> Result<Table, LawError> {
    type Held = BTreeMap<Vec<u16>, f64>;
    let mut next = Table::new();
    let mut level: Vec<(Vec<Tier>, Held)> = vec![(Vec::new(), Held::from([(Vec::new(), 1.0)]))];
    for _ in 0..draws {
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
