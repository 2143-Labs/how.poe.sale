//! What one use costs, in orb counts.
//!
//! A [`Cost`] is keyed by what is paid for, never by what is modelled: a key need not be one of the
//! eight orbs here, and a key with no price stays its own line rather than being converted.

use std::collections::BTreeMap;

/// The id a currency is known by, with or without its `currency/` prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurrencyId(pub &'static str);

impl CurrencyId {
    pub const TRANSMUTATION: CurrencyId = CurrencyId("currency/transmutation");
    pub const ALTERATION: CurrencyId = CurrencyId("currency/alteration");
    pub const AUGMENTATION: CurrencyId = CurrencyId("currency/augmentation");
    pub const REGAL: CurrencyId = CurrencyId("currency/regal");
    pub const EXALTED: CurrencyId = CurrencyId("currency/exalted");
    pub const SCOURING: CurrencyId = CurrencyId("currency/scouring");
    pub const ANNULMENT: CurrencyId = CurrencyId("currency/annulment");
    pub const DIVINE: CurrencyId = CurrencyId("currency/divine");
    pub const SHAPERS_EXALTED: CurrencyId = CurrencyId("currency/shapers_exalted");

    /// The id without its `currency/` prefix.
    pub fn name(self) -> &'static str {
        self.0.strip_prefix("currency/").unwrap_or(self.0)
    }
}

/// Orb counts, keyed by what is paid. A key need not be a modelled currency.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cost(BTreeMap<CurrencyId, f64>);

impl Cost {
    pub fn none() -> Cost {
        Cost::default()
    }

    pub fn of(id: CurrencyId, count: f64) -> Cost {
        Cost::none().with(id, count)
    }

    /// Adds `count` to what is already paid for `id`. A zero count is not stored.
    pub fn add(&mut self, id: CurrencyId, count: f64) {
        if count == 0.0 {
            return;
        }
        let slot = self.0.entry(id).or_insert(0.0);
        *slot += count;
        if *slot == 0.0 {
            self.0.remove(&id);
        }
    }

    pub fn plus(&self, other: &Cost) -> Cost {
        let mut sum = self.clone();
        for (id, count) in other.iter() {
            sum.add(id, count);
        }
        sum
    }

    pub fn times(&self, k: f64) -> Cost {
        let mut scaled = Cost::none();
        for (id, count) in self.iter() {
            scaled.add(id, count * k);
        }
        scaled
    }

    /// The count paid for `id`; zero when it is not paid for.
    pub fn get(&self, id: CurrencyId) -> f64 {
        self.0.get(&id).copied().unwrap_or(0.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (CurrencyId, f64)> + '_ {
        self.0.iter().map(|(&id, &count)| (id, count))
    }

    pub fn total(&self) -> f64 {
        self.0.values().sum()
    }

    fn with(mut self, id: CurrencyId, count: f64) -> Cost {
        self.add(id, count);
        self
    }
}

/// Mean and second moments of orb counts, for something with more than one step.
///
/// A single use has a certain [`Cost`]. A loop does not: how many alterations it takes is a
/// random count, and counts of different orbs move together (every failed attempt pays for a
/// scouring and the alterations of the next one). The second moments are `E[X_a · X_b]` for every
/// pair, so a weighted sum of counts has a spread too.
#[derive(Clone, Debug, PartialEq)]
pub struct Spend {
    pub mean: Cost,
    second: BTreeMap<(CurrencyId, CurrencyId), f64>,
}

impl Spend {
    /// Nothing spent.
    pub fn none() -> Spend {
        Spend::certain(&Cost::none())
    }

    /// A spend with no spread: exactly this cost, every time.
    pub fn certain(cost: &Cost) -> Spend {
        let mut second = BTreeMap::new();
        for (a, x) in cost.iter() {
            for (b, y) in cost.iter() {
                second.insert((a, b), x * y);
            }
        }
        Spend {
            mean: cost.clone(),
            second,
        }
    }

    /// A spend from its moments: `mean[i]` is the mean count of `keys[i]`, and
    /// `second[i · k + j]` is `E[X_i · X_j]`, `k` being the number of keys.
    pub fn from_moments(keys: &[CurrencyId], mean: &[f64], second: &[f64]) -> Spend {
        let mut spend = Spend::none();
        for (i, &a) in keys.iter().enumerate() {
            spend.mean.add(a, mean[i]);
            for (j, &b) in keys.iter().enumerate() {
                let moment = second[i * keys.len() + j];
                if moment != 0.0 {
                    spend.second.insert((a, b), moment);
                }
            }
        }
        spend
    }

    /// `E[X_a · X_b]`.
    pub fn moment(&self, a: CurrencyId, b: CurrencyId) -> f64 {
        self.second.get(&(a, b)).copied().unwrap_or(0.0)
    }

    pub fn covariance(&self, a: CurrencyId, b: CurrencyId) -> f64 {
        self.moment(a, b) - self.mean.get(a) * self.mean.get(b)
    }

    /// The standard deviation of one orb's count.
    pub fn sd(&self, id: CurrencyId) -> f64 {
        self.covariance(id, id).max(0.0).sqrt()
    }
}
