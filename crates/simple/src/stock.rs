//! What a currency can be used on.
//!
//! A currency is one transform on a [`Stock`]: a concrete [`Item`](crate::item::Item), or an
//! abstract item later. The stock answers what it holds and what one random modifier would add; the
//! currency answers what that means and what the use costs. A [`Chance`] keeps the arithmetic exact
//! for an item (`Fraction`) and approximate for an abstraction (`f64`).

use std::collections::HashMap;

use crate::fraction::Fraction;
use crate::item::{Rarity, Side};

/// Why a currency cannot be used: the reason, printed as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refusal(pub &'static str);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// A flag the game sets that stops every currency from touching the stock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    Corrupted,
    Mirrored,
}

/// The arithmetic one stock's chances use.
///
/// An exact chance answers `None` when an intermediate would not fit in 128 bits. A chance that
/// cannot overflow (`f64`) always answers.
pub trait Chance: Copy + PartialEq + std::fmt::Debug {
    const ZERO: Self;
    const ONE: Self;

    fn half() -> Self;

    /// `part / whole`.
    fn share(part: u128, whole: u128) -> Option<Self>;

    fn times(self, other: Self) -> Option<Self>;

    fn plus(self, other: Self) -> Option<Self>;

    fn is_one(self) -> bool;
}

impl Chance for Fraction {
    const ZERO: Self = Fraction::ZERO;
    const ONE: Self = Fraction::ONE;

    fn half() -> Self {
        Fraction::new(1, 2).expect("1/2 is a fraction")
    }

    fn share(part: u128, whole: u128) -> Option<Self> {
        Fraction::new(part, whole)
    }

    fn times(self, other: Self) -> Option<Self> {
        self.checked_mul(other)
    }

    fn plus(self, other: Self) -> Option<Self> {
        self.checked_add(other)
    }

    fn is_one(self) -> bool {
        self == Fraction::ONE
    }
}

impl Chance for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;

    fn half() -> Self {
        0.5
    }

    fn share(part: u128, whole: u128) -> Option<Self> {
        Some(part as f64 / whole as f64)
    }

    fn times(self, other: Self) -> Option<Self> {
        Some(self * other)
    }

    fn plus(self, other: Self) -> Option<Self> {
        Some(self + other)
    }

    fn is_one(self) -> bool {
        (self - 1.0).abs() <= 1e-12
    }
}

/// What a currency can be used on.
pub trait Stock: Clone + Eq + std::hash::Hash {
    /// The data a draw reads: `Data` for an item, a law for an abstract one.
    type Rules;
    /// `Fraction` for an item, `f64` for an abstract one.
    type Chance: Chance;

    fn rarity(&self) -> Rarity;

    /// The flag that blocks every currency, corrupted before mirrored.
    fn lock(&self) -> Option<Lock>;

    /// How many modifiers this side holds.
    fn held(&self, side: Side) -> usize;

    fn with_rarity(&self, rarity: Rarity) -> Self;

    /// The same stock with every explicit modifier gone, rarity kept.
    fn cleared(&self) -> Self;

    /// One random modifier added: every way it can come out. Empty when no side has room or
    /// nothing is left to roll.
    fn add(&self, rules: &Self::Rules) -> Result<Vec<(Self, Self::Chance)>, Refusal>;
}

/// Outcomes and their chances: equal outcomes folded, first-seen order kept, never empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Dist<T, C> {
    parts: Vec<(T, C)>,
}

impl<T: Clone + Eq + std::hash::Hash, C: Chance> Dist<T, C> {
    /// Folds outcomes that came out the same and keeps the rest in the order they arrived.
    ///
    /// An empty input is a refusal: a step that produced nothing is not a step. So is a fold that
    /// does not fit in 128 bits.
    pub fn new(parts: Vec<(T, C)>) -> Result<Dist<T, C>, Refusal> {
        if parts.is_empty() {
            return Err(Refusal("the step produced no outcome"));
        }
        let mut folded: Vec<(T, C)> = Vec::with_capacity(parts.len());
        let mut index: HashMap<T, usize> = HashMap::new();
        for (outcome, chance) in parts {
            match index.get(&outcome) {
                Some(&at) => {
                    let Some(sum) = folded[at].1.plus(chance) else {
                        return Err(Refusal("the chances do not fit in 128 bits"));
                    };
                    folded[at].1 = sum;
                }
                None => {
                    index.insert(outcome.clone(), folded.len());
                    folded.push((outcome, chance));
                }
            }
        }
        Ok(Dist { parts: folded })
    }

    /// The one outcome that always happens.
    pub fn one(outcome: T) -> Dist<T, C> {
        Dist {
            parts: vec![(outcome, C::ONE)],
        }
    }

    pub fn parts(&self) -> &[(T, C)] {
        &self.parts
    }

    /// Whether the chances sum to one.
    pub fn is_whole(&self) -> bool {
        let mut sum = C::ZERO;
        for &(_, chance) in &self.parts {
            match sum.plus(chance) {
                Some(next) => sum = next,
                None => return false,
            }
        }
        sum.is_one()
    }

    /// The chance that the step produces an outcome `want` accepts, or `None` when the sum does
    /// not fit.
    pub fn chance(&self, mut want: impl FnMut(&T) -> bool) -> Option<C> {
        let mut sum = C::ZERO;
        for (outcome, chance) in &self.parts {
            if want(outcome) {
                sum = sum.plus(*chance)?;
            }
        }
        Some(sum)
    }

    /// Maps every outcome and folds the results again. One way only: a distribution is not
    /// narrowed back.
    pub fn relabel<U: Clone + Eq + std::hash::Hash>(
        &self,
        mut to: impl FnMut(&T) -> U,
    ) -> Result<Dist<U, C>, Refusal> {
        Dist::new(
            self.parts
                .iter()
                .map(|(outcome, chance)| (to(outcome), *chance))
                .collect(),
        )
    }
}
