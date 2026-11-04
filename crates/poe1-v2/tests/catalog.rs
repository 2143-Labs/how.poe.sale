//! The method catalog: every currency scoped, method ids unique and sorted, claims resolve, and
//! the methods the Crown route relies on exist.

use std::collections::BTreeSet;

use poe1_v2::data::{BenchAction, Scope};
use poe1_v2::test_data;

const REASONS: [&str; 7] = [
    "placeholder",
    "shard",
    "maps-atlas",
    "gems",
    "other-item-class",
    "cost-only",
    "not-used-on-items",
];

#[test]
fn every_currency_scoped() {
    let d = test_data();
    assert_eq!(d.scope.len(), d.currencies.len());
    let scoped: BTreeSet<&str> = d.scope.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(scoped.len(), d.currencies.len());
    for c in &d.currencies {
        assert!(scoped.contains(c.id.as_str()), "{} unscoped", c.id);
    }
    for (id, s) in &d.scope {
        match s {
            Scope::Methods { ids } => {
                assert!(!ids.is_empty(), "{id}");
                for m in ids {
                    assert!(d.method(m).is_some(), "{id}: unknown method {m}");
                }
            }
            Scope::Excluded { reason } => {
                assert!(REASONS.contains(&reason.as_str()), "{id}: {reason}")
            }
        }
    }
}

#[test]
fn ids_unique_and_sorted() {
    let d = test_data();
    let ids: Vec<&str> = d.methods.iter().map(|m| m.id.as_str()).collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn claims_resolve() {
    let d = test_data();
    for m in &d.methods {
        for c in &m.claims {
            assert!(d.claim(c).is_some(), "{}: unknown claim {c}", m.id);
        }
    }
    for c in &d.claims {
        if c.checked {
            assert!(!c.quote.is_empty(), "{} is checked without a quote", c.id);
        }
    }
}

#[test]
fn known_methods() {
    let d = test_data();
    let scbc = d
        .mod_by_id("DexMasterItemGenerationCannotChangeSuffixes")
        .unwrap();
    let bench = d
        .bench
        .iter()
        .find(|b| b.action == BenchAction::AddMod { m: scbc })
        .unwrap();
    for id in [
        "currency/veiled-exalted".to_string(),
        "unveil".to_string(),
        format!("bench/{}", bench.index),
        "essence/deafening-essence-of-hatred".to_string(),
    ] {
        assert!(d.method(&id).is_some(), "{id} missing");
    }
}
