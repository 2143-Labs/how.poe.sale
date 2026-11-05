//! The method catalog: every currency scoped, method ids unique and sorted, claims resolve, the
//! methods the Crown route relies on exist, a bad method stops the bundle loading, the
//! protection policies follow their sources, and the reforge tables and resonators are the
//! data's.

use std::collections::BTreeSet;

use poe1_v2::data::{BenchAction, Data, Scope};
use poe1_v2::method::{CostSpec, Family, Method, Op, Policy, Protection, Require};
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

/// A4/E6: `Data::from_bundle` validates every method, so a bad one stops the bundle loading.
#[test]
fn bad_methods_do_not_load() {
    let d = test_data();
    type Edit = fn(&mut Method);
    let cases: [(Edit, &str); 6] = [
        (
            |m| {
                m.ops = vec![Op::AddMod {
                    r#mod: "NoSuchMod".into(),
                }]
            },
            "unknown mod NoSuchMod",
        ),
        (
            |m| {
                m.cost = vec![CostSpec {
                    name: "No Such Orb".into(),
                    amount: 1.0,
                }]
            },
            "cost names unknown currency \"No Such Orb\"",
        ),
        (
            |m| {
                m.cost = vec![CostSpec {
                    name: "Primitive Chaotic Resonator".into(),
                    amount: 1.0,
                }]
            },
            "cost \"Primitive Chaotic Resonator\" names 2 currencies; use one with a unique name",
        ),
        (
            |m| m.claims.push("no.such-claim".into()),
            "unknown claim no.such-claim",
        ),
        (
            |m| {
                m.requires.push(Require::CatalystRoom {
                    catalyst: "Shiny Catalyst".into(),
                })
            },
            "unknown catalyst Shiny Catalyst",
        ),
        (
            |m| m.ops.push(Op::Scour),
            "changes existing modifiers but states no protection policy",
        ),
    ];
    for (edit, want) in cases {
        let mut bundle = d.bundle.clone();
        let m = bundle
            .methods
            .iter_mut()
            .find(|m| m.id == "currency/exalted")
            .unwrap();
        edit(m);
        let err = Data::from_bundle(bundle).unwrap_err().to_string();
        assert_eq!(err, format!("method currency/exalted: {want}"));
    }
}

/// R1: each method's metamod policy, from Craft of Exile's metamod table (checked against
/// maxroll's metacrafting guide and vhpg's Prefixes Cannot Be Changed table).
#[test]
fn protection_policies_follow_the_sources() {
    use Policy::{Forbid, Ignore, Respect};
    let d = test_data();
    for (id, locks, cannot_roll) in [
        ("currency/chaos", Respect, Ignore),
        ("currency/veiled-chaos", Respect, Ignore),
        ("currency/divine", Respect, Ignore),
        ("currency/orb-of-dominance", Respect, Ignore),
        ("currency/alteration", Respect, Ignore),
        ("harvest/fire-to-cold", Respect, Ignore),
        ("currency/eldritch-chaos", Ignore, Ignore),
        ("currency/annulment", Respect, Respect),
        ("currency/eldritch-annulment", Respect, Respect),
        ("currency/scouring", Respect, Ignore),
        ("currency/veiled-exalted", Respect, Ignore),
        ("harvest/add-life-remove-random", Respect, Ignore),
        ("beast/prefix-to-suffix", Respect, Ignore),
        ("beast/suffix-to-prefix", Respect, Ignore),
        ("fossil/chaotic-resonator", Forbid, Forbid),
        ("fossil/alchemical-resonator", Forbid, Forbid),
    ] {
        assert_eq!(
            d.method(id).unwrap().protect,
            Some(Protection { locks, cannot_roll }),
            "{id}"
        );
    }
    for m in d.methods.iter().filter(|m| m.family == Family::Essence) {
        assert_eq!(
            m.protect,
            Some(Protection {
                locks: Forbid,
                cannot_roll: Forbid
            }),
            "{}",
            m.id
        );
    }
}

/// A3: every reforge's count table is claim `chaos.count`'s, and no essence limits the item
/// level any more (R4): its cap is the Fill's modifier level.
#[test]
fn fill_tables_are_the_claims() {
    let d = test_data();
    let claim: std::collections::BTreeMap<u8, u32> =
        serde_json::from_value(d.claim("chaos.count").unwrap().value.clone().unwrap()).unwrap();
    let table: Vec<(u8, u32)> = claim.into_iter().collect();
    let mut fills = 0;
    for m in &d.methods {
        for op in m.all_ops() {
            if let Op::Fill { fill } = op
                && !fill.counts.is_empty()
            {
                assert_eq!(fill.counts, table, "{}", m.id);
                fills += 1;
            }
        }
    }
    assert!(fills > 100, "{fills}");
    let wailing = d.method("essence/wailing-essence-of-hatred").unwrap();
    let caps: Vec<Option<u8>> = wailing
        .all_ops()
        .into_iter()
        .filter_map(|op| match op {
            Op::Fill { fill } => Some(fill.mod_level_max),
            _ => None,
        })
        .collect();
    assert_eq!(caps, [Some(75)]);
}

/// E6: the resonators are the stackable currencies, one per fossil count.
#[test]
fn resonators_are_data() {
    let d = test_data();
    for (id, kind) in [
        ("fossil/chaotic-resonator", "Chaotic"),
        ("fossil/alchemical-resonator", "Alchemical"),
    ] {
        let m = d.method(id).unwrap();
        assert_eq!(m.resonators.len(), 4);
        for (r, size) in m
            .resonators
            .iter()
            .zip(["Primitive", "Potent", "Powerful", "Prime"])
        {
            let c = d.currency_by_id(r).unwrap();
            assert_eq!(c.name, format!("{size} {kind} Resonator"));
            assert_eq!(c.class, "DelveStackableSocketableCurrency");
        }
    }
}
