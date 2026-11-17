//! The item the craft is for: three ordered grade floors on each side.
//!
//! A floor list is read best-first, so `[desired, good, any]` on the prefixes says the best prefix
//! must be the desired modifier and one more prefix must at least be good; `[good, good, any]` on
//! the suffixes says two of the three suffixes must be good. Everything else on the item is free.
//!
//! The comparison is written once here, over per-grade counts, so a concrete item (whose grades a
//! filter reads) and its abstract form (which holds nothing but grades) cannot disagree about what
//! "reached" means.

use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Item, Rarity, Side};

/// The floors the craft has to clear, per side, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub prefixes: [Tier; 3],
    pub suffixes: [Tier; 3],
}

impl Target {
    /// The shield's goal, as the data file states it.
    pub fn from_data(data: &Data) -> Target {
        Target {
            prefixes: floors(&data.target.prefixes, "prefixes"),
            suffixes: floors(&data.target.suffixes, "suffixes"),
        }
    }

    pub fn floors(&self, side: Side) -> [Tier; 3] {
        match side {
            Side::Prefix => self.prefixes,
            Side::Suffix => self.suffixes,
        }
    }

    /// Whether a rare item holding three modifiers a side clears that side's floors.
    ///
    /// `counts` is indexed by grade — `counts[2]` desired, `counts[1]` good, `counts[0]` anything —
    /// and its sum has to be three: the goal is a six-modifier rare item.
    pub fn side_reached(&self, side: Side, counts: [u8; 3]) -> bool {
        let floors = self.floors(side);
        if counts.iter().sum::<u8>() as usize != floors.len() {
            return false;
        }
        let mut available: Vec<Tier> = Vec::with_capacity(3);
        for rank in (0..=2u8).rev() {
            for _ in 0..counts[rank as usize] {
                available.push(match rank {
                    2 => Tier::Desired,
                    1 => Tier::Good,
                    _ => Tier::Any,
                });
            }
        }
        floors
            .iter()
            .zip(&available)
            .all(|(floor, held)| held.rank() >= floor.rank())
    }

    /// Whether a side holding these grade counts can still clear its floors, if every slot it has
    /// left rolls the best grade there is.
    ///
    /// This reads the counts and nothing else — not which groups are held — so it is optimistic: a
    /// held low-tier life prefix blocks the good life prefixes, and this check does not know it.
    /// Reading only grade counts is what gives one answer for an item and for its abstract form.
    pub fn side_reachable(&self, side: Side, counts: [u8; 3]) -> bool {
        let floors = self.floors(side);
        let held = counts.iter().sum::<u8>() as usize;
        if held > floors.len() {
            return false;
        }
        let mut available: Vec<Tier> = vec![Tier::Desired; floors.len() - held];
        for rank in (0..=2u8).rev() {
            for _ in 0..counts[rank as usize] {
                available.push(match rank {
                    2 => Tier::Desired,
                    1 => Tier::Good,
                    _ => Tier::Any,
                });
            }
        }
        floors
            .iter()
            .zip(&available)
            .all(|(floor, held)| held.rank() >= floor.rank())
    }

    /// Whether the item is the item the craft is for.
    pub fn reached(&self, filter: &ItemSetFilter, item: &Item) -> bool {
        if item.rarity != Rarity::Rare {
            return false;
        }
        Side::BOTH.into_iter().all(|side| {
            let tiers = filter.tiers(item, side);
            if tiers.len() != 3 {
                return false;
            }
            self.floors(side)
                .iter()
                .zip(&tiers)
                .all(|(floor, held)| held.rank() >= floor.rank())
        })
    }
}

fn floors(names: &[String], side: &str) -> [Tier; 3] {
    let parsed: Vec<Tier> = names
        .iter()
        .map(|name| {
            Tier::from_name(name)
                .unwrap_or_else(|| panic!("crates/simple/data/shield.json: {name} is not a grade"))
        })
        .collect();
    parsed.try_into().unwrap_or_else(|_| {
        panic!("crates/simple/data/shield.json: the {side} floors are not three grades")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data;

    /// Counts are indexed by `Tier::rank`: `[any, good, desired]`.
    #[test]
    fn a_side_is_reachable_while_its_floors_can_still_be_cleared() {
        let target = Target::from_data(data());
        assert_eq!(target.prefixes, [Tier::Desired, Tier::Good, Tier::Any]);
        assert_eq!(target.suffixes, [Tier::Good, Tier::Good, Tier::Any]);

        assert!(target.side_reachable(Side::Prefix, [0, 0, 0]));
        assert!(!target.side_reachable(Side::Prefix, [3, 0, 0]));
        assert!(target.side_reachable(Side::Prefix, [1, 0, 1]));
        assert!(!target.side_reachable(Side::Prefix, [2, 0, 1]));
        assert!(!target.side_reachable(Side::Suffix, [2, 0, 0]));
        assert!(target.side_reachable(Side::Suffix, [1, 1, 0]));
    }

    #[test]
    fn a_full_side_is_reachable_exactly_when_it_is_reached() {
        let target = Target::from_data(data());
        for side in Side::BOTH {
            for any in 0..=3u8 {
                for good in 0..=3 - any {
                    let counts = [any, good, 3 - any - good];
                    assert_eq!(
                        target.side_reachable(side, counts),
                        target.side_reached(side, counts),
                        "{side:?} {counts:?}"
                    );
                }
            }
        }
    }
}
