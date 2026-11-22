//! A stat's value, as the interval the model knows it in.
//!
//! This crate never rolls a die, so it never knows a value outright: a freshly rolled modifier's
//! stat is somewhere in its range, a fixture may pin one value, and a Divine Orb puts it back to the
//! whole range. Adding intervals is exact — `[a,b] + [c,d] = [a+c, b+d]` — which is why a value can
//! be carried without a random number generator and without a float.
//!
//! The one semantic that keeps the crate exact is therefore the interval itself: a value is never a
//! number, it is the set of numbers the model admits. A pinned value (`Roll::exact`) is the
//! degenerate interval `[v,v]`, a freshly rolled stat is its mod's range `[min,max]`, and a Divine
//! Orb widens a pinned value back to that range. Because the model composes values only by adding
//! stat contributions, and `[a,b] + [c,d] = [a+c, b+d]` is exact for intervals of integers, no die is
//! needed and no float ever enters a value path — `average` exists for display and is fed back into
//! nothing.

use std::fmt;

/// One stat's value: every value it can take, as an interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Roll {
    pub min: i32,
    pub max: i32,
}

impl Roll {
    /// The interval holding one value and nothing else: a value a fixture pins.
    pub const fn exact(value: i32) -> Roll {
        Roll {
            min: value,
            max: value,
        }
    }

    /// The interval `[min, max]`; `None` when `min > max`, because no value can be in it.
    pub fn range(min: i32, max: i32) -> Option<Roll> {
        if min > max {
            None
        } else {
            Some(Roll { min, max })
        }
    }

    /// True when the interval has one value, i.e. the value is pinned.
    pub fn is_exact(self) -> bool {
        self.min == self.max
    }

    /// The sum of two intervals: `[a,b] + [c,d] = [a+c, b+d]`. `None` on overflow of either bound.
    ///
    /// Named like [`crate::fraction::Fraction::checked_add`]: the answer can be absent, so it is
    /// not the `Add` the standard library would give it.
    pub fn checked_add(self, other: Roll) -> Option<Roll> {
        let min = self.min.checked_add(other.min)?;
        let max = self.max.checked_add(other.max)?;
        Roll::range(min, max)
    }

    /// The midpoint of the interval. For display only; never fed back into a value.
    pub fn average(self) -> f64 {
        (f64::from(self.min) + f64::from(self.max)) / 2.0
    }
}

impl fmt::Display for Roll {
    /// An interval of one prints as that value (`"20"`); anything wider prints `"min-max"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_exact() {
            write!(f, "{}", self.min)
        } else {
            write!(f, "{}-{}", self.min, self.max)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Roll;

    #[test]
    fn exact_is_a_degenerate_interval() {
        let roll = Roll::exact(20);
        assert_eq!(roll.min, 20);
        assert_eq!(roll.max, 20);
        assert!(roll.is_exact());
    }

    #[test]
    fn range_keeps_its_bounds_and_refuses_inverted_ones() {
        let roll = Roll::range(145, 159).expect("145 <= 159");
        assert_eq!(roll.min, 145);
        assert_eq!(roll.max, 159);
        assert!(!roll.is_exact());
        assert_eq!(Roll::range(7, 7), Some(Roll::exact(7)));
        assert_eq!(Roll::range(159, 145), None);
    }

    #[test]
    fn add_is_pointwise_and_exact() {
        let life = Roll::range(145, 159).unwrap();
        let pinned = Roll::exact(20);
        let sum = life.checked_add(pinned).unwrap();
        assert_eq!(sum, Roll::range(165, 179).unwrap());
        assert_eq!(sum.min, 165);
        assert_eq!(sum.max, 179);

        let both_ranges = Roll::range(1, 2)
            .unwrap()
            .checked_add(Roll::range(10, 20).unwrap());
        assert_eq!(both_ranges, Some(Roll::range(11, 22).unwrap()));

        // Adding a pinned value to a pinned value stays pinned.
        let pinned_sum = Roll::exact(2).checked_add(Roll::exact(3)).unwrap();
        assert_eq!(pinned_sum, Roll::exact(5));
        assert!(pinned_sum.is_exact());
    }

    #[test]
    fn add_overflows_into_none() {
        assert_eq!(Roll::exact(i32::MAX).checked_add(Roll::exact(1)), None);
        assert_eq!(Roll::exact(i32::MIN).checked_add(Roll::exact(-1)), None);
        assert_eq!(
            Roll::range(i32::MAX - 1, i32::MAX)
                .unwrap()
                .checked_add(Roll::exact(1)),
            None
        );
        assert_eq!(
            Roll::exact(i32::MAX).checked_add(Roll::range(0, 1).unwrap()),
            None
        );
    }

    #[test]
    fn average_is_the_midpoint() {
        assert_eq!(Roll::range(145, 159).unwrap().average(), 152.0);
        assert_eq!(Roll::exact(20).average(), 20.0);
        assert_eq!(Roll::range(-5, 5).unwrap().average(), 0.0);
        assert_eq!(Roll::range(0, 1).unwrap().average(), 0.5);
    }

    #[test]
    fn display_prints_one_value_or_the_range() {
        assert_eq!(Roll::exact(20).to_string(), "20");
        assert_eq!(Roll::range(145, 159).unwrap().to_string(), "145-159");
        assert_eq!(Roll::range(-3, -1).unwrap().to_string(), "-3--1");
    }

    #[test]
    fn intervals_sort_by_min_then_max() {
        // The derived order is the one the crate relies on: narrower intervals that start at the
        // same minimum come first, and a later minimum always wins over a wider earlier interval.
        let mut rolls = vec![
            Roll::range(10, 10).unwrap(),
            Roll::range(5, 20).unwrap(),
            Roll::range(5, 9).unwrap(),
            Roll::range(10, 12).unwrap(),
        ];
        rolls.sort();
        assert_eq!(
            rolls,
            vec![
                Roll::range(5, 9).unwrap(),
                Roll::range(5, 20).unwrap(),
                Roll::range(10, 10).unwrap(),
                Roll::range(10, 12).unwrap(),
            ]
        );
        assert!(Roll::exact(5) < Roll::exact(10));
        assert!(Roll::range(5, 9).unwrap() < Roll::range(5, 20).unwrap());
    }
}
