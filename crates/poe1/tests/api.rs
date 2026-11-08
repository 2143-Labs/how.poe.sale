//! The browser's views: a pasted item, the steps that could have produced an abstract item, and
//! the modifiers a target can name, as JSON.

mod common;

use common::root;
use poe1::api;
use poe1::test_data;

const METHODS: [&str; 5] = [
    "currency/exalted",
    "currency/annulment",
    "bench/add/StrMasterItemGenerationCannotChangePrefixes",
    "bench/add/DexMasterItemGenerationCannotChangeSuffixes",
    "essence/essence-of-horror",
];

fn text(name: &str) -> String {
    let path = root().join(format!("fixtures/text/{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn methods() -> Vec<String> {
    METHODS.iter().map(|m| m.to_string()).collect()
}

/// The Essence of Horror outcome on an Archdemon Crown: its suffix and two untracked prefixes and
/// suffixes.
fn horror_root() -> String {
    serde_json::json!({
        "base": "Archdemon Crown",
        "itemLevel": 87,
        "rarity": "rare",
        "suffixes": [{ "id": "SocketedGemsDealMoreElementalDamageEssence1" }],
        "anyPrefixes": 2,
        "anySuffixes": 2,
    })
    .to_string()
}

/// The chart grows both ways: the essence explains the root, its input is a node of its own, and
/// pushing that node forward reaches the root again, in the same node.
#[test]
fn a_chart_grows_both_ways() {
    let d = test_data();
    let (mut chart, root) = api::Chart::start(d, &horror_root(), "[]").expect("a chart");
    let json = serde_json::to_value(&root).unwrap();
    assert_eq!(json["view"]["anySuffixes"], 2);
    assert_eq!(
        json["view"]["suffixes"][0],
        "Socketed Gems deal 30% more Elemental Damage"
    );
    assert_eq!(json["members"], serde_json::Value::Null);

    let made = chart.made_by(d, &root.id, &methods()).expect("a view");
    let essence = made
        .inputs
        .iter()
        .find(|s| s.method == "essence/essence-of-horror")
        .expect("the essence step");
    assert!((essence.p - 11.0 / 64.0).abs() < 1e-12, "{}", essence.p);
    assert_eq!(essence.cost, "1 × Essence of Horror");
    for m in METHODS {
        let shown =
            made.inputs.iter().any(|s| s.method == m) || made.skipped.iter().any(|s| s.method == m);
        assert!(shown, "{m}");
    }

    let pinned = chart
        .pin_backward(d, &root.id, "essence/essence-of-horror", &essence.key)
        .expect("a node");
    assert_eq!(pinned.node.members, Some(1));
    assert_eq!(pinned.edge.from, pinned.node.id);
    assert_eq!(pinned.edge.to, root.id);

    let applied = chart
        .forward(d, &pinned.node.id, "essence/essence-of-horror")
        .expect("an outcome");
    let back = applied
        .outcomes
        .iter()
        .find(|o| o.key == root.id)
        .expect("the root's class");
    let tolerance = 4.0 * (essence.p * (1.0 - essence.p) / 200_000.0).sqrt();
    assert!((back.p - essence.p).abs() < tolerance, "{}", back.p);

    let again = chart
        .pin_forward(d, &pinned.node.id, "essence/essence-of-horror", &root.id)
        .expect("a node");
    assert_eq!(again.node.id, root.id);
    // The root began as a class with no items; the forward path gives it the ones it reaches.
    assert!(again.node.members.is_some(), "{:?}", again.node.members);
}

/// A reforge that can land a tracked modifier is simulated, so the view says how many runs and
/// carries the range: the watched life family is in the essence's pool.
#[test]
fn a_reforge_reports_its_simulation() {
    let d = test_data();
    let root = serde_json::json!({
        "base": "Archdemon Crown",
        "itemLevel": 87,
        "rarity": "normal",
    })
    .to_string();
    let watch = serde_json::json!([{ "id": "IncreasedLife9", "orBetter": true }]).to_string();
    let (mut chart, node) = api::Chart::start(d, &root, &watch).expect("a chart");
    let applied = chart
        .forward(d, &node.id, "essence/essence-of-horror")
        .expect("an outcome");
    assert_eq!(applied.simulated, Some(200_000));
    assert!(applied.outcomes[0].interval.is_some());
}

/// Two tiers of one family cannot both be tracked, and neither can a ninth modifier.
#[test]
fn track_refuses_one_family_twice() {
    let d = test_data();
    let watched = serde_json::json!([
        { "id": "ColdResist6", "orBetter": true },
        { "id": "ColdResist8" }
    ])
    .to_string();
    let err = api::Chart::start(d, &horror_root(), &watched)
        .err()
        .expect("a refusal");
    assert!(err.starts_with("track: "), "{err}");
    assert!(err.contains("track the same family"), "{err}");
}

#[test]
fn a_chart_refuses_a_bad_root() {
    let d = test_data();
    let err = api::Chart::start(d, "{", "[]").err().expect("a refusal");
    assert!(err.starts_with("root: "), "{err}");
    let wide = serde_json::json!({
        "base": "Archdemon Crown",
        "itemLevel": 87,
        "rarity": "rare",
        "anyPrefixes": 4,
    })
    .to_string();
    assert_eq!(
        api::Chart::start(d, &wide, "[]").err().expect("a refusal"),
        "root: the item prefixes: 4 modifiers, the rarity holds 3"
    );
}

#[test]
fn mods_list_the_natural_and_essence_modifiers() {
    let d = test_data();
    let list = api::mods(d, "Archdemon Crown", 87, &[]).expect("a list");
    let life = list
        .iter()
        .find(|m| m.id == "IncreasedLife9")
        .expect("IncreasedLife9");
    assert_eq!(
        (life.side.as_str(), life.source.as_str()),
        ("prefix", "natural")
    );
    let horror = list
        .iter()
        .find(|m| m.id == "SocketedGemsDealMoreElementalDamageEssence1")
        .expect("Essence of Horror's modifier");
    assert_eq!(horror.side, "suffix");
    assert_eq!(horror.source, "Essence of Horror");
    let first_suffix = list
        .iter()
        .position(|m| m.side == "suffix")
        .expect("suffixes");
    assert!(list[..first_suffix].iter().all(|m| m.side == "prefix"));
    assert!(list[first_suffix..].iter().all(|m| m.side == "suffix"));
    assert_eq!(
        api::mods(d, "Nope", 87, &[]).unwrap_err(),
        "mods: unknown base \"Nope\""
    );
    assert!(api::bases(d).iter().any(|b| b == "Archdemon Crown"));
}

#[test]
fn read_item_reports_its_warnings() {
    let d = test_data();
    let view = api::read_item(d, &text("archdemon-crown")).expect("an item");
    assert!(
        view.warnings
            .iter()
            .any(|w| w.contains("is the sum of two modifiers")),
        "{:?}",
        view.warnings
    );
}

/// The metacraft: boots with three suffixes and no prefix, Suffixes Cannot Be Changed, then a
/// Veiled Chaos Orb. The lock is named on its card, so it is never the node an Exalted Orb's
/// random prefix reaches; the reforge keeps the suffixes, removes the lock, and adds one, two or
/// three prefixes at 8:3:1 (claims `reforge.counts`, `chaos.count`), one of them veiled.
#[test]
fn a_meta_craft_runs_forward() {
    const LOCK: &str = "bench/add/DexMasterItemGenerationCannotChangeSuffixes";
    let data = test_data();
    let root = serde_json::json!({
        "base": "Slink Boots",
        "itemLevel": 86,
        "rarity": "rare",
        "suffixes": [{ "id": "FireResist8" }, { "id": "ColdResist8" }, { "id": "LightningResist8" }],
    })
    .to_string();
    let (mut chart, node) = api::Chart::start(data, &root, "[]").expect("a chart");
    assert_eq!(node.members, Some(1));
    let lock = chart.forward(data, &node.id, LOCK).expect("the lock");
    assert_eq!(lock.outcomes.len(), 1);
    let locked = lock.outcomes[0].clone();
    let view = locked.view.as_ref().expect("an item");
    assert_eq!(view.prefixes, ["Suffixes Cannot Be Changed (crafted)"]);
    assert_eq!(view.any_prefixes, 0);
    let exalt = chart
        .forward(data, &node.id, "currency/exalted")
        .expect("an exalt");
    assert!(exalt.outcomes.iter().all(|o| o.key != locked.key));
    let pinned = chart
        .pin_forward(data, &node.id, LOCK, &locked.key)
        .expect("the lock pins");
    let veiled = chart
        .forward(data, &pinned.node.id, "currency/veiled-chaos")
        .expect("a veiled chaos");
    // Nothing is tracked, so the reforge's counts are exact.
    assert_eq!(veiled.simulated, None);
    assert!(!veiled.repeatable);
    let cases = [(0u8, 2.0 / 3.0), (1, 1.0 / 4.0), (2, 1.0 / 12.0)];
    assert_eq!(veiled.outcomes.len(), cases.len());
    for (any, want) in cases {
        let outcome = veiled
            .outcomes
            .iter()
            .find(|o| o.view.as_ref().is_some_and(|v| v.any_prefixes == any))
            .unwrap_or_else(|| panic!("no outcome with {any} other prefixes"));
        let view = outcome.view.as_ref().expect("an item");
        assert_eq!(view.prefixes, ["Veiled Prefix"]);
        assert_eq!(view.suffixes.len(), 3);
        assert_eq!(view.any_suffixes, 0);
        assert!((outcome.p - want).abs() < 1e-12, "{any}: {}", outcome.p);
        assert_eq!(outcome.interval, None);
    }
}

/// The chance of "any T1 physical prefix": a step lists, for each watched modifier, the chance
/// its result holds it, and the chance it holds at least one, which is the sum of the outcomes
/// naming one. A Harvest physical reforge reaches them more often than a Chaos Orb.
#[test]
fn a_watch_list_sums_its_hits() {
    let data = test_data();
    let foil = |rarity: &str| {
        serde_json::json!({ "base": "Jewelled Foil", "itemLevel": 86, "rarity": rarity })
            .to_string()
    };
    let watch = serde_json::json!([
        { "id": "LocalAddedPhysicalDamage9" },
        { "id": "LocalIncreasedPhysicalDamagePercent8" },
        { "id": "LocalIncreasedPhysicalDamagePercentAndAccuracyRating8" },
    ])
    .to_string();
    let check = |rarity: &str, method: &str| -> api::AppliedView {
        let (mut chart, node) = api::Chart::start(data, &foil(rarity), &watch).expect("a chart");
        let applied = chart.forward(data, &node.id, method).expect("a step");
        assert_eq!(applied.watched.len(), 4, "{method}");
        assert_eq!(applied.watched[3].label, "at least one of these");
        let any = applied.watched[3].p;
        let named: f64 = applied
            .outcomes
            .iter()
            .filter(|o| o.view.as_ref().is_some_and(|v| !v.prefixes.is_empty()))
            .map(|o| o.p)
            .sum();
        assert!((any - named).abs() < 1e-12, "{method}: {any} != {named}");
        let marginals: f64 = applied.watched[..3].iter().map(|w| w.p).sum();
        assert!(
            applied.watched[..3]
                .iter()
                .all(|w| w.p > 0.0 && w.p <= any + 1e-12)
        );
        assert!(any <= marginals + 1e-12, "{method}");
        let simulated = applied.simulated.is_some();
        assert!(
            applied
                .watched
                .iter()
                .all(|w| w.interval.is_some() == simulated)
        );
        applied
    };
    check("magic", "currency/alteration");
    let chaos = check("rare", "currency/chaos");
    let harvest = check("rare", "harvest/reforge-physical");
    assert!(harvest.watched[3].p > chaos.watched[3].p);
}
