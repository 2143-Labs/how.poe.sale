//! The goal, as three grades and an acceptance rule.
//!
//! A modifier is **desired**, **good**, or anything else — and "anything else" is not bad, it is
//! simply not counted by the goal. The grades are the only vocabulary here: a filter is a list of
//! desired ids, a list of good ids, and a rule saying which items it will take. The abstractions
//! ([`AbstractMod`]) exist so a caller can widen a question — one modifier, then one modifier of a
//! grade, then any modifier on a side — and never narrow it back.

use crate::data::Data;
use crate::item::{Item, Side};

/// What one held modifier is worth to the goal.
///
/// There is no `Ord`: the grades are named, and [`Tier::rank`] is the only order, so a comparison
/// has to say which order it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tier {
    /// The modifier the craft is for.
    Desired,
    /// A modifier worth keeping the item for.
    Good,
    /// A modifier the goal does not count.
    Any,
}

impl Tier {
    /// The grade's height: desired 2, good 1, anything 0.
    pub fn rank(self) -> u8 {
        match self {
            Tier::Desired => 2,
            Tier::Good => 1,
            Tier::Any => 0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tier::Desired => "desired",
            Tier::Good => "good",
            Tier::Any => "any",
        }
    }

    pub fn from_name(name: &str) -> Option<Tier> {
        match name {
            "desired" => Some(Tier::Desired),
            "good" => Some(Tier::Good),
            "any" => Some(Tier::Any),
            _ => None,
        }
    }
}

/// A question about a modifier, at three widths.
///
/// [`AbstractMod::coerce`] widens — specific, then grade, then any on the side — and never the
/// other way: a caller who asked about "any suffix" cannot be handed back "FireResist8".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbstractMod {
    Specific(Side, u16),
    Tier(Side, Tier),
    Any(Side),
}

impl AbstractMod {
    pub fn side(self) -> Side {
        match self {
            AbstractMod::Specific(side, _)
            | AbstractMod::Tier(side, _)
            | AbstractMod::Any(side) => side,
        }
    }

    fn width(self) -> u8 {
        match self {
            AbstractMod::Specific(..) => 2,
            AbstractMod::Tier(..) => 1,
            AbstractMod::Any(_) => 0,
        }
    }

    /// `self` widened to `to`, or `None` when `to` is narrower, or on the other side.
    pub fn coerce(self, to: AbstractMod) -> Option<AbstractMod> {
        (self.side() == to.side() && to.width() <= self.width()).then_some(to)
    }
}

/// One entry in a filter's list: a named modifier, or "any".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModSelector {
    Mod(u16),
    Any,
}

/// What the filter will take a finished step's item for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accept {
    /// Anything holding a desired modifier.
    HasDesired,
    /// Anything holding a desired modifier, or a good modifier on both sides.
    DesiredOrGoodBothSides,
    /// Anything.
    Any,
}

/// The goal's modifier lists and acceptance rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSetFilter {
    pub desired: Vec<ModSelector>,
    pub good: Vec<ModSelector>,
    pub accept: Accept,
}

impl ItemSetFilter {
    /// The shield's goal: the data file's desired and good ids, under the given acceptance rule.
    pub fn from_data(data: &Data, accept: Accept) -> ItemSetFilter {
        let selector = |m: u16| ModSelector::Mod(m);
        ItemSetFilter {
            desired: data.target.desired.iter().copied().map(selector).collect(),
            good: data.target.good.iter().copied().map(selector).collect(),
            accept,
        }
    }

    fn lists(&self, selectors: &[ModSelector], m: u16) -> bool {
        selectors.iter().any(|selector| match selector {
            ModSelector::Mod(id) => *id == m,
            ModSelector::Any => true,
        })
    }

    /// The grade of one modifier. Anything the filter does not name is [`Tier::Any`].
    pub fn tier_of(&self, m: u16) -> Tier {
        if self.lists(&self.desired, m) {
            Tier::Desired
        } else if self.lists(&self.good, m) {
            Tier::Good
        } else {
            Tier::Any
        }
    }

    /// The best grade on one side, or `None` when the side is empty.
    pub fn best(&self, item: &Item, side: Side) -> Option<Tier> {
        item.affixes(side)
            .iter()
            .map(|affix| self.tier_of(affix.m))
            .max_by_key(|tier| tier.rank())
    }

    /// Whether the item holds the modifier the craft is for.
    pub fn holds_desired(&self, item: &Item) -> bool {
        Side::BOTH
            .into_iter()
            .flat_map(|side| item.affixes(side).iter())
            .any(|affix| self.tier_of(affix.m) == Tier::Desired)
    }

    /// Whether both sides carry at least a good modifier.
    pub fn good_both_sides(&self, item: &Item) -> bool {
        Side::BOTH.into_iter().all(|side| {
            self.best(item, side)
                .is_some_and(|tier| tier.rank() >= Tier::Good.rank())
        })
    }

    /// Whether the filter takes this item.
    pub fn accepts(&self, item: &Item) -> bool {
        match self.accept {
            Accept::HasDesired => self.holds_desired(item),
            Accept::DesiredOrGoodBothSides => {
                self.holds_desired(item) || self.good_both_sides(item)
            }
            Accept::Any => true,
        }
    }

    /// The grades on one side, best first.
    pub fn tiers(&self, item: &Item, side: Side) -> Vec<Tier> {
        let mut tiers: Vec<Tier> = item
            .affixes(side)
            .iter()
            .map(|affix| self.tier_of(affix.m))
            .collect();
        tiers.sort_by_key(|tier| std::cmp::Reverse(tier.rank()));
        tiers
    }
}
