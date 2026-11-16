//! An exact rational, so a chance never rounds. Every weight the pool gives is an integer, so
//! every probability is a ratio of integers and every sum of them can be kept exact.

use std::fmt;

/// A reduced fraction; the denominator is never zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fraction {
    num: u128,
    den: u128,
}

const fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

impl Fraction {
    pub const ZERO: Fraction = Fraction { num: 0, den: 1 };
    pub const ONE: Fraction = Fraction { num: 1, den: 1 };

    /// `num/den`, reduced; `None` when `den` is zero.
    pub fn new(num: u128, den: u128) -> Option<Fraction> {
        if den == 0 {
            return None;
        }
        let g = gcd(num, den).max(1);
        Some(Fraction {
            num: num / g,
            den: den / g,
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

    pub fn is_one(self) -> bool {
        self.num == self.den
    }

    /// The sum, or `None` if it would overflow: a caller that cannot hold the answer exactly is
    /// told so rather than handed a rounded one.
    pub fn checked_add(self, other: Fraction) -> Option<Fraction> {
        let g = gcd(self.den, other.den);
        let den = self.den / g * other.den;
        let a = self.num.checked_mul(den / self.den)?;
        let b = other.num.checked_mul(den / other.den)?;
        Fraction::new(a.checked_add(b)?, den)
    }

    /// The product, or `None` on overflow. Reduces crosswise first to keep it small.
    pub fn checked_mul(self, other: Fraction) -> Option<Fraction> {
        let g1 = gcd(self.num, other.den);
        let g2 = gcd(other.num, self.den);
        Fraction::new(
            (self.num / g1).checked_mul(other.num / g2)?,
            (self.den / g2).checked_mul(other.den / g1)?,
        )
    }

    /// Display only, for a report: the exact chance as a percentage of one.
    pub fn percent(self) -> f64 {
        self.num as f64 * 100.0 / self.den as f64
    }

    /// Display only, for a report: the chance as a percentage with `places` decimals.
    pub fn percent_text(self, places: usize) -> String {
        format!("{:.*}", places, self.percent())
    }
}

impl fmt::Display for Fraction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

#[cfg(test)]
mod tests {
    use super::Fraction;

    #[test]
    fn reduces_and_adds_exactly() {
        let half = Fraction::new(1, 2).unwrap();
        assert_eq!(half.num(), 1);
        assert_eq!(half.den(), 2);
        assert!(half.checked_add(half).unwrap().is_one());
        let third = Fraction::new(1, 3).unwrap();
        assert_eq!(
            half.checked_add(third).unwrap(),
            Fraction::new(5, 6).unwrap()
        );
        assert_eq!(Fraction::new(2, 4).unwrap(), half);
        assert_eq!(Fraction::new(0, 7).unwrap(), Fraction::ZERO);
        assert!(Fraction::new(1, 0).is_none());
    }

    #[test]
    fn multiplication_cancels_crosswise() {
        let a = Fraction::new(800, 52000).unwrap();
        let b = Fraction::new(52000, 800).unwrap();
        assert!(a.checked_mul(b).unwrap().is_one());
    }

    #[test]
    fn overflow_is_reported_not_rounded() {
        let big = Fraction::new(u128::MAX, 2).unwrap();
        assert!(big.checked_add(big).is_none());
    }
}
