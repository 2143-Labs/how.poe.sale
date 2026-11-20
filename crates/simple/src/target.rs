//! The item the craft is for: the grade floors each side has to clear, best first.
//!
//! A floor list is read best-first. `[desired]` says the side must hold the desired modifier and
//! asks nothing else of it; `[desired, good]` says the best modifier on that side must be the
//! desired one and a second must at least be good. A side with no floors is free, and everything
//! on the item above the floors is a bonus — the floors are what the craft has to reach, not what
//! the item may hold.
//!
//! The comparison is written once here, over per-grade counts, so a concrete item (whose grades a
//! filter reads) and its abstract form (which holds nothing but grades) cannot disagree about what
//! "reached" means.

use crate::data::Data;
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Item, Rarity, Side};
use crate::pool::Caps;

/// The floors the craft has to clear, per side, best first, and how many modifiers a side of the
/// item may hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub prefixes: Vec<Tier>,
    pub suffixes: Vec<Tier>,
    /// The base's caps ([`crate::data::Base::caps`]): what [`crate::node::Node::reachable`] counts
    /// the room left against.
    pub caps: Caps,
}

impl Target {
    /// The shield's goal, as the data file states it.
    pub fn from_data(data: &Data) -> Target {
        let target = Target {
            prefixes: floors(&data.target.prefixes, "prefixes"),
            suffixes: floors(&data.target.suffixes, "suffixes"),
            caps: data.base_at(0).caps,
        };
        for side in Side::BOTH {
            let floors = target.floors(side);
            assert!(
                floors.len() <= target.caps.rare,
                "crates/simple/data/shield.json: the {} floors ask for {} modifiers, and a rare \
                 item holds {} at most",
                side.name(),
                floors.len(),
                target.caps.rare
            );
        }
        target
    }

    pub fn floors(&self, side: Side) -> &[Tier] {
        match side {
            Side::Prefix => &self.prefixes,
            Side::Suffix => &self.suffixes,
        }
    }

    /// Whether a side holding these grade counts clears that side's floors.
    ///
    /// `counts` is indexed by grade — `counts[2]` desired, `counts[1]` good, `counts[0]` anything —
    /// and the side's grades are read best first, so a side holding three modifiers is credited for
    /// its best ones and the rest are free.
    pub fn side_reached(&self, side: Side, counts: [u8; 3]) -> bool {
        let floors = self.floors(side);
        let held = best_first(counts);
        held.len() >= floors.len() && clears(floors, &held)
    }

    /// Whether a side holding these grade counts can still clear its floors, if every slot it has
    /// left — `cap` modifiers a side at most — rolls the best grade there is.
    ///
    /// This reads the counts and nothing else — not which groups are held — so it is optimistic: a
    /// held low-tier life prefix blocks the good life prefixes, and this check does not know it.
    /// Reading only grade counts is what gives one answer for an item and for its abstract form.
    pub fn side_reachable(&self, side: Side, counts: [u8; 3], cap: usize) -> bool {
        let floors = self.floors(side);
        let held: usize = counts.iter().map(|&n| n as usize).sum();
        let mut available = best_first(counts);
        for _ in held..cap {
            available.push(Tier::Desired);
        }
        available.sort_unstable_by_key(|tier| std::cmp::Reverse(tier.rank()));
        available.truncate(floors.len());
        clears(floors, &available)
    }

    /// Whether the item is the item the craft is for.
    pub fn reached(&self, filter: &ItemSetFilter, item: &Item) -> bool {
        if item.rarity != Rarity::Rare {
            return false;
        }
        Side::BOTH.into_iter().all(|side| {
            let mut tiers = filter.tiers(item, side);
            tiers.sort_unstable_by_key(|tier| std::cmp::Reverse(tier.rank()));
            let floors = self.floors(side);
            tiers.len() >= floors.len() && clears(floors, &tiers)
        })
    }
}

/// Whether the best grades held clear the floors, both read best first.
fn clears(floors: &[Tier], held: &[Tier]) -> bool {
    floors
        .iter()
        .zip(held)
        .all(|(floor, held)| held.rank() >= floor.rank())
}

/// The grades a side holds from its counts, best first.
fn best_first(counts: [u8; 3]) -> Vec<Tier> {
    let mut out = Vec::with_capacity(3);
    for rank in (0..=2u8).rev() {
        for _ in 0..counts[rank as usize] {
            out.push(match rank {
                2 => Tier::Desired,
                1 => Tier::Good,
                _ => Tier::Any,
            });
        }
    }
    out
}

fn floors(names: &[String], side: &str) -> Vec<Tier> {
    names
        .iter()
        .map(|name| {
            Tier::from_name(name).unwrap_or_else(|| {
                panic!("crates/simple/data/shield.json: {name} is not a grade, in {side}")
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data;

    /// Counts are indexed by `Tier::rank`: `[any, good, desired]`.
    #[test]
    fn a_side_is_reachable_while_its_floors_can_still_be_cleared() {
        let target = Target::from_data(data());
        assert_eq!(target.prefixes, vec![Tier::Desired]);
        assert_eq!(target.suffixes, Vec::new());

        // One desired prefix is the whole goal. A rare prefix side with room can still gain it,
        // whatever it holds now; a full one that lacks it cannot.
        for counts in [[0, 0, 0], [1, 0, 0], [2, 0, 0], [0, 0, 1], [0, 0, 3]] {
            assert!(target.side_reachable(Side::Prefix, counts, 3), "{counts:?}");
        }
        assert!(!target.side_reachable(Side::Prefix, [3, 0, 0], 3));

        // A magic side holds one modifier: it is out of room, so only the desired one counts.
        assert!(!target.side_reachable(Side::Prefix, [1, 0, 0], 1));
        assert!(!target.side_reachable(Side::Prefix, [0, 1, 0], 1));
        assert!(target.side_reachable(Side::Prefix, [0, 0, 1], 1));
        assert!(target.side_reachable(Side::Prefix, [0, 0, 0], 1));

        // No floors on the suffixes: every suffix side is reached and reachable.
        for counts in [[0, 0, 0], [3, 0, 0], [1, 1, 1], [0, 0, 3]] {
            assert!(target.side_reached(Side::Suffix, counts), "{counts:?}");
            for cap in [1, 3] {
                assert!(
                    target.side_reachable(Side::Suffix, counts, cap),
                    "{counts:?}"
                );
            }
        }
    }

    #[test]
    fn a_full_side_is_reachable_exactly_when_it_is_reached() {
        let target = Target::from_data(data());
        for side in Side::BOTH {
            for any in 0..=3u8 {
                for good in 0..=3 - any {
                    let counts = [any, good, 3 - any - good];
                    assert_eq!(
                        target.side_reachable(side, counts, 3),
                        target.side_reached(side, counts),
                        "{side:?} {counts:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn floors_of_more_than_one_grade_still_read_best_first() {
        let target = Target {
            prefixes: vec![Tier::Desired, Tier::Good],
            suffixes: vec![Tier::Good, Tier::Good, Tier::Good],
            caps: data().base_at(0).caps,
        };
        // Two prefixes, one desired and one good, clear the prefix floors; a desired alone does
        // not, because the second floor has no modifier under it yet.
        assert!(target.side_reached(Side::Prefix, [0, 1, 1]));
        assert!(!target.side_reached(Side::Prefix, [0, 0, 1]));
        assert!(target.side_reachable(Side::Prefix, [0, 0, 1], 2));
        assert!(!target.side_reachable(Side::Prefix, [1, 0, 1], 2));
        // Extra modifiers above the floors are free: a good one in third place changes nothing.
        assert!(target.side_reached(Side::Prefix, [1, 1, 1]));
        assert!(!target.side_reached(Side::Prefix, [2, 0, 1]));
        // Three good suffixes clear three good floors; two and an anything do not.
        assert!(target.side_reached(Side::Suffix, [0, 3, 0]));
        assert!(!target.side_reached(Side::Suffix, [1, 2, 0]));
        assert!(!target.side_reached(Side::Suffix, [3, 0, 0]));
        assert!(!target.side_reachable(Side::Suffix, [3, 0, 0], 3));
    }
}
