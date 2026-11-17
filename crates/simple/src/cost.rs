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
