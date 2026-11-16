//! The mod filter a step can be read through: which modifiers count as desired, which as good, and
//! the rest as any. The same three buckets the craft loop branches on.
//!
//! Modifiers are also representable in the abstract — a specific modifier, its bucket, or anything on
//! its side — and abstraction only ever widens: a specific modifier coerces to "desired prefix" and
//! to "any prefix", never the other way round.

use crate::data::{Data, Side};
use crate::item::{Affix, Item};
use crate::pattern::{Grade, Pattern};

/// The three buckets a modifier falls into for a given goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Desired,
    Good,
    Any,
}

impl Tier {
    /// Higher is better, so `max` picks the best modifier on a side.
    fn rank(self) -> u8 {
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
}

/// A modifier referred to at some level of abstraction. Coercion only widens, so the levels form a
/// chain per side: `Specific → Tier → Any`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbstractMod {
    Specific(Side, u32),
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

    fn level(self) -> u8 {
        match self {
            AbstractMod::Specific(..) => 0,
            AbstractMod::Tier(..) => 1,
            AbstractMod::Any(_) => 2,
        }
    }

    /// Coerces this modifier to a wider one, or `None` when that would narrow it: a specific
    /// prefix coerces to "a desired prefix" and to "any prefix"; "any prefix" coerces to nothing
    /// narrower, and a desired prefix does not coerce to a good one — those are different buckets,
    /// not rungs on one ladder.
    pub fn coerce(self, to: AbstractMod) -> Option<AbstractMod> {
        if self.side() != to.side() || self.level() > to.level() {
            return None;
        }
        match (self, to) {
            (_, AbstractMod::Any(_)) => Some(to),
            (_, AbstractMod::Tier(..)) if self.level() == 0 => Some(to),
            (AbstractMod::Tier(_, a), AbstractMod::Tier(_, b)) if a == b => Some(to),
            _ => None,
        }
    }
}

/// One modifier the goal names: a family at a tier or better, optionally with value floors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModSelector {
    pub side: Side,
    /// The modifier that names the family; usually its best tier.
    pub m: u32,
    /// Every modifier of the same family at this one's level or above.
    pub or_better: bool,
    /// Per-stat floors, in the modifier's stat order. A freshly drawn modifier is unrolled, so a
    /// floor is treated as met there: the value is not chosen yet, only its family is.
    pub min: Option<Vec<Option<i32>>>,
}

impl ModSelector {
    /// A single modifier, exactly as named.
    pub fn exact(side: Side, m: u32) -> ModSelector {
        ModSelector {
            side,
            m,
            or_better: false,
            min: None,
        }
    }

    /// The family of `m` at its tier or better.
    pub fn or_better(side: Side, m: u32) -> ModSelector {
        ModSelector {
            side,
            m,
            or_better: true,
            min: None,
        }
    }

    pub fn with_min(mut self, min: Vec<Option<i32>>) -> ModSelector {
        self.min = Some(min);
        self
    }

    pub fn matches(&self, data: &Data, affix: &Affix) -> bool {
        let md = data.mod_at(affix.roll.m);
        if md.side != Some(self.side) {
            return false;
        }
        if affix.roll.m != self.m {
            if !self.or_better {
                return false;
            }
            let want = data.mod_at(self.m);
            let same_family = md.mod_type == want.mod_type
                && md.kind == want.kind
                && md.influence == want.influence;
            if !same_family || md.level < want.level {
                return false;
            }
        }
        let Some(floors) = &self.min else {
            return true;
        };
        let Some(values) = &affix.roll.values else {
            return true;
        };
        floors
            .iter()
            .enumerate()
            .all(|(i, floor)| match (floor, values.get(i)) {
                (Some(floor), Some(value)) => value >= floor,
                _ => true,
            })
    }
}

/// What a filtered set must contain to be kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accept {
    /// Keep only items that already hold a desired modifier.
    HasDesired,
    /// A desired modifier, or a good one on each side.
    DesiredOrGoodBothSides,
    /// Keep everything.
    Any,
}

/// The goal, as the three buckets over one item's modifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSetFilter {
    pub desired: Vec<ModSelector>,
    pub good: Vec<ModSelector>,
    pub accept: Accept,
}

impl ItemSetFilter {
    pub fn new(desired: Vec<ModSelector>, good: Vec<ModSelector>, accept: Accept) -> ItemSetFilter {
        ItemSetFilter {
            desired,
            good,
            accept,
        }
    }

    /// The same filter, taken from a pattern's classes: its `desired` classes become the desired
    /// selectors and its `good` ones the good selectors, so a goal is written once, in the pattern
    /// language the rest of the repository already speaks, and the step filter reads it.
    pub fn from_pattern(pattern: &Pattern, accept: Accept) -> ItemSetFilter {
        let mut desired = Vec::new();
        let mut good = Vec::new();
        for class in &pattern.classes {
            let target = if matches!(class.grade, Grade::Desired) {
                &mut desired
            } else if matches!(class.grade, Grade::Good) {
                &mut good
            } else {
                continue;
            };
            for (m, or_better, min) in class.mods() {
                target.push(ModSelector {
                    side: class.side,
                    m,
                    or_better,
                    min,
                });
            }
        }
        ItemSetFilter {
            desired,
            good,
            accept,
        }
    }

    /// Which bucket one modifier falls into.
    pub fn tier_of(&self, data: &Data, affix: &Affix) -> Tier {
        if self.desired.iter().any(|s| s.matches(data, affix)) {
            return Tier::Desired;
        }
        if self.good.iter().any(|s| s.matches(data, affix)) {
            return Tier::Good;
        }
        Tier::Any
    }

    /// The best bucket on a side, or `None` when that side is empty.
    pub fn best(&self, data: &Data, item: &Item, side: Side) -> Option<Tier> {
        item.affixes(side)
            .iter()
            .map(|a| self.tier_of(data, a))
            .max_by_key(|t| t.rank())
    }

    /// Which bucket a modifier would fall into if it were drawn — the same classification, applied
    /// to a candidate the item does not hold yet.
    pub fn tier_of_mod(&self, data: &Data, m: u32) -> Tier {
        self.tier_of(data, &Affix::new(m))
    }

    pub fn holds_desired(&self, data: &Data, item: &Item) -> bool {
        Side::BOTH
            .into_iter()
            .any(|side| self.best(data, item, side) == Some(Tier::Desired))
    }

    /// A good modifier on each side.
    pub fn good_both_sides(&self, data: &Data, item: &Item) -> bool {
        Side::BOTH.into_iter().all(|side| {
            matches!(
                self.best(data, item, side),
                Some(Tier::Good | Tier::Desired)
            )
        })
    }

    pub fn accepts(&self, data: &Data, item: &Item) -> bool {
        match self.accept {
            Accept::HasDesired => self.holds_desired(data, item),
            Accept::DesiredOrGoodBothSides => {
                self.holds_desired(data, item) || self.good_both_sides(data, item)
            }
            Accept::Any => true,
        }
    }

    /// Every modifier on the item, referred to at the widest level the goal needs: the specific
    /// modifier when the goal names it exactly, its bucket when the goal names the family, and
    /// "any" otherwise. This is the item-side half of the coercion — it only ever widens.
    pub fn view(&self, data: &Data, item: &Item) -> Vec<AbstractMod> {
        let mut out = Vec::new();
        for side in Side::BOTH {
            for affix in item.affixes(side) {
                let specific = AbstractMod::Specific(side, affix.roll.m);
                let tier = self.tier_of(data, affix);
                out.push(specific);
                if let Some(wider) = specific.coerce(AbstractMod::Tier(side, tier)) {
                    out.push(wider);
                }
                out.push(AbstractMod::Any(side));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{AbstractMod, Tier};
    use crate::data::Side;

    #[test]
    fn abstraction_only_widens() {
        let specific = AbstractMod::Specific(Side::Prefix, 7);
        let desired = AbstractMod::Tier(Side::Prefix, Tier::Desired);
        let any = AbstractMod::Any(Side::Prefix);
        assert_eq!(specific.coerce(desired), Some(desired));
        assert_eq!(specific.coerce(any), Some(any));
        assert_eq!(desired.coerce(any), Some(any));
        assert_eq!(any.coerce(specific), None);
        assert_eq!(any.coerce(desired), None);
        assert_eq!(
            desired.coerce(AbstractMod::Tier(Side::Prefix, Tier::Good)),
            None
        );
        // a prefix never coerces to a suffix
        assert_eq!(specific.coerce(AbstractMod::Any(Side::Suffix)), None);
    }
}
