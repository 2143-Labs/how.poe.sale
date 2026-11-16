//! Exact chances: a rational number that never rounds.
//!
//! Every chance in this crate comes out of two integers — a modifier's pool weight and the pool's
//! total — and stays one. Nothing here converts to `f64` except [`Fraction::percent`], which exists
//! for printing only.

/// A reduced fraction with a non-zero denominator.
///
/// There is deliberately **no `Ord`**: the derived order would compare numerators before
/// denominators and would rank `1/2` above `3/4`. A caller that needs to order chances compares
/// [`Fraction::percent`] and says why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fraction {
    num: u128,
    den: u128,
}

impl Fraction {
    /// The whole chance: everything happens.
    pub const ONE: Fraction = Fraction { num: 1, den: 1 };

    /// No chance at all.
    pub const ZERO: Fraction = Fraction { num: 0, den: 1 };

    /// `num / den`, reduced, or `None` when the denominator is zero.
    pub fn new(num: u128, den: u128) -> Option<Fraction> {
        if den == 0 {
            return None;
        }
        let divisor = gcd(num, den);
        Some(Fraction {
            num: num / divisor,
            den: den / divisor,
        })
    }

    pub fn num(self) -> u128 {
        self.num
    }

    pub fn den(self) -> u128 {
        self.den
    }

    pub fn is_zero(self) -> bool {
        self.num == 0
    }

    /// The sum, or `None` if a product or the sum does not fit in 128 bits.
    pub fn checked_add(self, other: Fraction) -> Option<Fraction> {
        // a/b + c/d = (a·d + c·b) / (b·d), reduced afterwards. Cross-reducing first would keep the
        // intermediates smaller, but the sum has to be reduced anyway and the overflow check is the
        // same; the callers' denominators are pool totals, far below the limit.
        let left = self.num.checked_mul(other.den)?;
        let right = other.num.checked_mul(self.den)?;
        let num = left.checked_add(right)?;
        let den = self.den.checked_mul(other.den)?;
        Fraction::new(num, den)
    }

    /// The product, or `None` if it does not fit in 128 bits.
    pub fn checked_mul(self, other: Fraction) -> Option<Fraction> {
        // Cross-reduce first: the reduced product is the same and the intermediate stays small.
        let first = gcd(self.num, other.den);
        let second = gcd(other.num, self.den);
        let num = (self.num / first).checked_mul(other.num / second)?;
        let den = (self.den / second).checked_mul(other.den / first)?;
        Fraction::new(num, den)
    }

    /// The chance as a percentage, for printing. Never fed back into a chance.
    pub fn percent(self) -> f64 {
        self.num as f64 / self.den as f64 * 100.0
    }
}

impl std::fmt::Display for Fraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_and_refuses_a_zero_denominator() {
        assert_eq!(Fraction::new(2, 4), Fraction::new(1, 2));
        assert_eq!(Fraction::new(0, 7), Some(Fraction::ZERO));
        assert_eq!(Fraction::new(1, 0), None);
    }

    #[test]
    fn adds_exactly() {
        let third = Fraction::new(1, 3).unwrap();
        assert_eq!(
            third.checked_add(third).unwrap(),
            Fraction::new(2, 3).unwrap()
        );
        assert_eq!(
            Fraction::new(1, 3)
                .unwrap()
                .checked_add(Fraction::new(1, 6).unwrap())
                .unwrap(),
            Fraction::new(1, 2).unwrap()
        );
        assert_eq!(
            Fraction::ZERO.checked_add(Fraction::ONE).unwrap(),
            Fraction::ONE
        );
    }

    #[test]
    fn multiplies_by_cross_reducing() {
        // 3/2 · 2/3 is 1, and the product must not overflow on the way.
        assert_eq!(
            Fraction::new(3, 2)
                .unwrap()
                .checked_mul(Fraction::new(2, 3).unwrap())
                .unwrap(),
            Fraction::ONE
        );
        // The largest numerator over 3, times 3 over the largest numerator: cross-reduction keeps
        // both factors of the product small, so an exact answer that fits is found.
        let big = Fraction::new(u128::MAX, 3).unwrap();
        let small = Fraction::new(3, u128::MAX).unwrap();
        assert_eq!(big.checked_mul(small).unwrap(), Fraction::ONE);
        // A product that genuinely does not fit is refused rather than wrapped.
        let huge = Fraction::new(u128::MAX, 2).unwrap();
        assert!(huge.checked_mul(huge).is_none());
    }

    #[test]
    fn percent_is_display_only() {
        assert_eq!(Fraction::new(1, 81).unwrap().percent(), 100.0 / 81.0);
    }
}
