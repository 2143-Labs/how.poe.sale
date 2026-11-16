//! The mod filter a step can be read through: which modifiers count as desired, which as good, and
//! the rest as any. The same three buckets the craft loop branches on.
//!
//! Modifiers are also representable in the abstract — a specific modifier, its bucket, or anything on
//! its side — and abstraction only ever widens: a specific modifier coerces to "desired prefix" and
//! to "any prefix", never the other way round.

use crate::data::{Data, Side};
use crate::item::{Affix, Item};
use crate::pattern::{self, Pattern};

/// The three buckets a modifier falls into for a given goal.
///
/// Deliberately unordered: the variants are declared `Desired, Good, Any`, so a derived `Ord`
/// would rank `Desired` *lowest* while [`Tier::rank`] ranks it highest. Nothing orders buckets —
/// callers that need the best one use `max_by_key(Tier::rank)` — so no `Ord` is offered rather
/// than one that disagrees with the only notion of order the module has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// One modifier the goal names. A goal written in the pattern language names families mostly —
/// "this modifier, or its tier or better" — but a class may also be written by category
/// (`{"crafted": true}`), which names no id at all, so a selector says which of the two it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModSelector {
    /// A family, named by one of its members.
    Mod {
        side: Side,
        /// The modifier that names the family; usually its best tier.
        m: u32,
        /// Every modifier of the same family at this one's level or above.
        or_better: bool,
        /// Per-stat floors, in the modifier's stat order. A freshly drawn modifier is unrolled, so
        /// a floor is treated as met there: the value is not chosen yet, only its family is.
        min: Option<Vec<Option<i32>>>,
    },
    /// Every modifier of a kind on one side, carrying the sense the pattern's matcher has: `true`
    /// takes a crafted modifier that is not a metamod — the same test the pattern applies, since a
    /// metamod is a tool rather than a modifier a goal wants — and `false` takes anything that is
    /// not crafted.
    Crafted { side: Side, crafted: bool },
}

impl ModSelector {
    /// A single modifier, exactly as named.
    pub fn exact(side: Side, m: u32) -> ModSelector {
        ModSelector::Mod {
            side,
            m,
            or_better: false,
            min: None,
        }
    }

    /// The family of `m` at its tier or better.
    pub fn or_better(side: Side, m: u32) -> ModSelector {
        ModSelector::Mod {
            side,
            m,
            or_better: true,
            min: None,
        }
    }

    /// Adds value floors. Only a family has values to floor; asking a category for floors is a
    /// caller bug rather than a value nobody asked about.
    pub fn with_min(mut self, min: Vec<Option<i32>>) -> ModSelector {
        match &mut self {
            ModSelector::Mod { min: floors, .. } => *floors = Some(min),
            ModSelector::Crafted { .. } => panic!("a category selector has no value floors"),
        }
        self
    }

    /// Whether this selector takes `affix` on `item`: the same test the pattern makes, family and
    /// value floors read through `pattern::mod_matches` and `pattern::floors_met`, so a goal
    /// cannot bucket a modifier one way here and another in `pattern::evaluate`. The floors are in
    /// the units the game shows, which is why the item is needed: its effect magnitudes scale a
    /// stored roll into the number on the item's tooltip.
    pub fn matches(&self, data: &Data, item: &Item, affix: &Affix) -> bool {
        self.matches_with(data, item.effect(data, &affix.roll, false), affix)
    }

    fn matches_with(&self, data: &Data, effect: i16, affix: &Affix) -> bool {
        let md = data.mod_at(affix.roll.m);
        match self {
            ModSelector::Crafted { side, crafted } => {
                md.side == Some(*side) && pattern::crafted_match(data, *crafted, affix.roll.m)
            }
            ModSelector::Mod {
                side,
                m,
                or_better,
                min,
            } => {
                if md.side != Some(*side) {
                    return false;
                }
                if !pattern::mod_matches(data, *m, *or_better, affix.roll.m) {
                    return false;
                }
                match min {
                    None => true,
                    Some(floors) => pattern::floors_met(data, affix, effect, floors),
                }
            }
        }
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

    /// The same filter, taken from a pattern's classes: its `Desired` classes become the desired
    /// selectors, and its `Good`, `Middling` and `Neutral` ones the good selectors, so a goal is
    /// written once, in the pattern language the rest of the repository already speaks, and the
    /// step filter reads it. The buckets come from `solve::grade_value` — the score's own
    /// collapse of the five authoring grades onto best (2), good enough (1) and not good enough
    /// (0) — rather than restating which grades pair up, so the filter and the score cannot
    /// disagree about whether a class is good enough. A `Bad` class needs no selector: its
    /// modifiers are `Tier::Any`, the fallback bucket.
    pub fn from_pattern(pattern: &Pattern, accept: Accept) -> ItemSetFilter {
        let mut desired = Vec::new();
        let mut good = Vec::new();
        for class in &pattern.classes {
            let target = match crate::solve::grade_value(class.grade) {
                2 => &mut desired,
                1 => &mut good,
                _ => continue,
            };
            for (m, or_better, min) in class.mods() {
                target.push(ModSelector::Mod {
                    side: class.side,
                    m,
                    or_better,
                    min,
                });
            }
            // A class written by category names no ids, so `mods` cannot report it. `{"any": true}`
            // still needs nothing here: a class that takes anything *is* the fallback bucket, and
            // anything no selector claims already lands in `Tier::Any`.
            for crafted in class.crafted_matchers() {
                target.push(ModSelector::Crafted {
                    side: class.side,
                    crafted,
                });
            }
        }
        ItemSetFilter {
            desired,
            good,
            accept,
        }
    }

    /// Which bucket one modifier falls into, for a caller that has only the affix: a freshly
    /// drawn modifier, or a class fact that is not about one item's magnitudes. A value floor is
    /// read with effect zero, which is what every base in the bundle carries; use [`Self::tier_of_item`]
    /// when the item is at hand.
    pub fn tier_of(&self, data: &Data, affix: &Affix) -> Tier {
        self.tier_of_with(data, 0, affix)
    }

    /// Which bucket one modifier on `item` falls into: the same reading `pattern::evaluate` makes,
    /// with the item's effect magnitudes applied to any value floor.
    pub fn tier_of_item(&self, data: &Data, item: &Item, affix: &Affix) -> Tier {
        self.tier_of_with(data, item.effect(data, &affix.roll, false), affix)
    }

    fn tier_of_with(&self, data: &Data, effect: i16, affix: &Affix) -> Tier {
        if self
            .desired
            .iter()
            .any(|s| s.matches_with(data, effect, affix))
        {
            return Tier::Desired;
        }
        if self
            .good
            .iter()
            .any(|s| s.matches_with(data, effect, affix))
        {
            return Tier::Good;
        }
        Tier::Any
    }

    /// The best bucket on a side, or `None` when that side is empty.
    pub fn best(&self, data: &Data, item: &Item, side: Side) -> Option<Tier> {
        item.affixes(side)
            .iter()
            .map(|a| self.tier_of_item(data, item, a))
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
                let tier = self.tier_of_item(data, item, affix);
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
