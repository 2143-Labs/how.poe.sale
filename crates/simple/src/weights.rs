//! What one orb is roughly worth, so two spends in different orbs can be compared.
//!
//! `data/weights.json` is a hand-edited table of rough market figures in one unit. Nothing fetches
//! prices, and nothing here is a chance: a weight only turns orb counts into one number, for the
//! weighted column of a report and for saying which of two moves costs less.
//!
//! A cost key with no weight is never an error and never converted. It stays a count of its own
//! kind ([`Valued::unweighted`]), because some things a craft pays with have no price in orbs.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::cost::{Cost, CurrencyId, Spend};

/// The compiled-in weight table.
pub fn weights() -> &'static Weights {
    static WEIGHTS: LazyLock<Weights> = LazyLock::new(|| {
        Weights::from_json(SOURCE).unwrap_or_else(|why| panic!("{SOURCE_NAME}: {why}"))
    });
    &WEIGHTS
}

const SOURCE: &str = include_str!("../data/weights.json");
const SOURCE_NAME: &str = "crates/simple/data/weights.json";

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Weights {
    /// What the weights are counted in.
    pub unit: String,
    #[serde(rename = "weights")]
    by_id: BTreeMap<String, f64>,
}

/// A spend as one number, with what could not be weighed kept beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct Valued {
    /// The weighted mean.
    pub value: f64,
    /// The weighted spread: `sqrt(wᵀ · Cov · w)`.
    pub sd: f64,
    /// The mean counts of the keys that have no weight.
    pub unweighted: Cost,
}

impl Weights {
    pub fn from_json(source: &str) -> Result<Weights, String> {
        let weights: Weights = serde_json::from_str(source).map_err(|why| why.to_string())?;
        for (id, weight) in &weights.by_id {
            if !weight.is_finite() || *weight < 0.0 {
                return Err(format!("the weight of {id} is {weight}"));
            }
        }
        Ok(weights)
    }

    /// The weight of one of these, or `None` when the table does not list it.
    pub fn of(&self, id: CurrencyId) -> Option<f64> {
        self.by_id.get(id.0).copied()
    }

    pub fn value(&self, spend: &Spend) -> Valued {
        let mut weighed: Vec<(CurrencyId, f64)> = Vec::new();
        let mut unweighted = Cost::none();
        for (id, count) in spend.mean.iter() {
            match self.of(id) {
                Some(weight) => weighed.push((id, weight)),
                None => unweighted.add(id, count),
            }
        }
        let value = weighed
            .iter()
            .map(|&(id, weight)| weight * spend.mean.get(id))
            .sum();
        let variance: f64 = weighed
            .iter()
            .flat_map(|&a| weighed.iter().map(move |&b| (a, b)))
            .map(|((a, wa), (b, wb))| wa * wb * spend.covariance(a, b))
            .sum();
        Valued {
            value,
            sd: variance.max(0.0).sqrt(),
            unweighted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_the_owners_figures() {
        let weights = weights();
        assert_eq!(weights.unit, "chaos");
        assert_eq!(weights.of(CurrencyId::ALTERATION), Some(0.2));
        assert_eq!(weights.of(CurrencyId::EXALTED), Some(2.0));
        assert_eq!(weights.of(CurrencyId::DIVINE), Some(100.0));
        assert_eq!(weights.of(CurrencyId("currency/chaos")), Some(1.0));
        assert_eq!(weights.of(CurrencyId::ANNULMENT), None);
    }

    #[test]
    fn a_negative_weight_is_an_error() {
        let bad = r#"{ "unit": "chaos", "weights": { "currency/regal": -1 } }"#;
        assert!(Weights::from_json(bad).is_err());
    }
}
