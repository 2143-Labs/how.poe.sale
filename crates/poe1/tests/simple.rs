//! The Simple game: a Bow and a Helmet with toy modifiers, built from `games/simple` by the same
//! bundle pipeline and run by the same engine. Every test goes through `api::Chart`, the calls the
//! page makes.

mod common;

use std::sync::LazyLock;

use poe1::api;
use poe1::data::Data;

const LIFE4: &str = "SimpleLife4";
const ALTERATION: &str = "currency/alteration";
const AUGMENTATION: &str = "currency/augmentation";
const LOCK_SUFFIXES: &str = "bench/add/DexMasterItemGenerationCannotChangeSuffixes";

static DATA: LazyLock<Data> = LazyLock::new(|| {
    let path = common::root().join("data/simple.json");
    let build = "nix develop -c cargo run -p cli --release -- bundle --raw games/simple/raw \
                 --rules games/simple/rules --claims rules/claims.json --out data/simple.json";
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nbuild it with:\n  {build}",
            path.display()
        )
    });
    Data::from_json(&text).unwrap_or_else(|e| {
        panic!(
            "{} does not load ({e})\nbuild it with:\n  {build}",
            path.display()
        )
    })
});

fn simple() -> &'static Data {
    &DATA
}
fn close(got: f64, want: f64) -> bool {
    (got - want).abs() < 1e-12
}

/// The outcome chances of a step, largest first.
fn chances(applied: &api::AppliedView) -> Vec<f64> {
    let mut p: Vec<f64> = applied.outcomes.iter().map(|o| o.p).collect();
    p.sort_by(|a, b| b.total_cmp(a));
    p
}

fn assert_chances(applied: &api::AppliedView, want: &[f64]) {
    let got = chances(applied);
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(close(*g, *w), "{got:?} != {want:?}");
    }
}

#[test]
fn the_simple_game_has_a_bow_and_a_helmet() {
    assert_eq!(api::bases(simple()), ["Bow", "Helmet"]);
}

/// A magic helmet with one prefix and one suffix, rerolled by Alteration: one or two modifiers at
/// even odds, a side picked first, and +40 to maximum Life is 1/30 of the helmet prefixes.
#[test]
fn alteration_spam_on_a_magic_helmet() {
    let d = simple();
    let root = serde_json::json!({
        "base": "Helmet", "itemLevel": 87, "rarity": "magic", "anyPrefixes": 1, "anySuffixes": 1,
    })
    .to_string();
    let watch = serde_json::json!([{ "id": LIFE4 }]).to_string();
    let (mut chart, node) = api::Chart::start(d, &root, &watch).expect("a chart");
    let applied = chart.forward(d, &node.id, ALTERATION).expect("a step");
    assert_chances(
        &applied,
        &[
            29.0 / 60.0,
            1.0 / 4.0,
            29.0 / 120.0,
            1.0 / 60.0,
            1.0 / 120.0,
        ],
    );
    let back = applied
        .outcomes
        .iter()
        .find(|o| close(o.p, 29.0 / 60.0))
        .expect("the root's class");
    assert_eq!(back.key, node.id);
    assert_eq!(applied.simulated, None);
}

/// A magic helmet with only a suffix takes an Augmentation: one prefix, +40 to maximum Life at
/// 1/30.
#[test]
fn an_augmentation_fills_the_prefix() {
    let d = simple();
    let root = serde_json::json!({
        "base": "Helmet", "itemLevel": 87, "rarity": "magic", "anySuffixes": 1,
    })
    .to_string();
    let watch = serde_json::json!([{ "id": LIFE4 }]).to_string();
    let (mut chart, node) = api::Chart::start(d, &root, &watch).expect("a chart");
    let applied = chart.forward(d, &node.id, AUGMENTATION).expect("a step");
    assert_chances(&applied, &[29.0 / 30.0, 1.0 / 30.0]);
}

/// The metacraft: lock the three suffixes, then a Veiled Chaos Orb adds one, two or three prefixes
/// at 8:3:1 (claim `chaos.count`), one of them veiled.
#[test]
#[ignore = "exact once Step 3 lands"]
fn the_veiled_chaos_metacraft() {
    let d = simple();
    let root = serde_json::json!({
        "base": "Helmet", "itemLevel": 87, "rarity": "rare", "anySuffixes": 3,
    })
    .to_string();
    let (mut chart, node) = api::Chart::start(d, &root, "[]").expect("a chart");
    let lock = chart.forward(d, &node.id, LOCK_SUFFIXES).expect("the lock");
    assert_chances(&lock, &[1.0]);
    let pinned = chart
        .pin_forward(d, &node.id, LOCK_SUFFIXES, &lock.outcomes[0].key)
        .expect("the lock pins");
    let veiled = chart
        .forward(d, &pinned.node.id, "currency/veiled-chaos")
        .expect("a veiled chaos");
    assert_eq!(veiled.simulated, None);
    for (any, want) in [(0u8, 2.0 / 3.0), (1, 1.0 / 4.0), (2, 1.0 / 12.0)] {
        let outcome = veiled
            .outcomes
            .iter()
            .find(|o| o.view.as_ref().is_some_and(|v| v.any_prefixes == any))
            .unwrap_or_else(|| panic!("no outcome with {any} other prefixes"));
        assert!(close(outcome.p, want), "{any}: {}", outcome.p);
        assert_eq!(outcome.interval, None);
        let view = outcome.view.as_ref().expect("an item");
        assert_eq!(view.prefixes, ["Veiled Prefix"]);
    }
    assert_eq!(veiled.outcomes.len(), 3);
}

/// A Chaos Orb on a rare bow, watching every top physical prefix: simulated, with a row for each
/// and one for any of them.
#[test]
fn a_bow_chaos_watches_any_top_physical_prefix() {
    let d = simple();
    let root = serde_json::json!({ "base": "Bow", "itemLevel": 87, "rarity": "rare" }).to_string();
    let watch = serde_json::json!([
        { "id": "SimplePhysical4" },
        { "id": "SimpleAddedPhysical4" },
        { "id": "SimpleHybridPhysical4" },
    ])
    .to_string();
    let (mut chart, node) = api::Chart::start(d, &root, &watch).expect("a chart");
    let applied = chart
        .forward(d, &node.id, "currency/chaos")
        .expect("a step");
    assert_eq!(applied.simulated, Some(200_000));
    assert_eq!(applied.watched.len(), 4);
    assert_eq!(applied.watched[3].label, "at least one of these");
    let any = applied.watched[3].p;
    assert!(applied.watched[..3].iter().all(|w| w.p <= any));
}

/// The copied currencies are the Path of Exile 1 methods, unchanged.
#[test]
fn simple_methods_match_poe1() {
    let d = simple();
    let copied: Vec<&str> = d
        .methods
        .iter()
        .map(|m| m.id.as_str())
        .filter(|id| !id.starts_with("bench/"))
        .collect();
    assert_eq!(copied.len(), 11, "{copied:?}");
    for id in copied {
        assert_eq!(d.method(id), poe1::test_data().method(id), "{id}");
    }
}
