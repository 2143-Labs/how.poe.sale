//! V9: the fixture `Problem` survives JSON and solves through the wire API.

use std::collections::BTreeMap;

use poe1::api::{self, DonorJson, Problem, Solution};

const PRICES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/defaultPrices.json"
));
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/simplex_target.txt"
));

fn fixture_problem() -> Problem {
    let b = poe1::test_bundle();
    let draft = api::parse_target_api(b, FIXTURE);
    let v: serde_json::Value = serde_json::from_str(PRICES).expect("defaultPrices.json");
    let prices: BTreeMap<String, f64> =
        serde_json::from_value(v["prices"].clone()).expect("prices map");
    let divine = prices["divine"];
    Problem {
        version: 1,
        bundle_patch: b.patch.clone(),
        target: draft.target().expect("fixture has a base"),
        prices,
        donors: vec![DonorJson {
            label: "Warlord amulet, t1 increased Strength".into(),
            base: "Simplex Amulet".into(),
            ilvl: 84,
            influence: "Warlord".into(),
            mod_id: "PercentageStrengthBodyInfluence2_".into(),
            price_chaos: 545.7,
        }],
        added: Vec::new(),
        expanded: vec!["goal".into()],
        disabled: Vec::new(),
        buy: vec![("goal".into(), 800.0 * divine)],
        sell: Vec::new(),
        runs: 20_000,
        seed: 7,
    }
}

#[test]
fn fixture_problem_round_trips_and_solves() {
    let b = poe1::test_bundle();
    let problem = fixture_problem();

    let json = serde_json::to_string(&problem).expect("serialise problem");
    let back: Problem = serde_json::from_str(&json).expect("deserialise problem");
    assert_eq!(back, problem);
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    // The wire names the UI sends.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for field in [
        "bundlePatch",
        "target",
        "prices",
        "donors",
        "added",
        "expanded",
        "disabled",
        "buy",
        "sell",
        "runs",
        "seed",
    ] {
        assert!(v.get(field).is_some(), "missing {field}");
    }
    assert!(v["target"]["ilvlMin"].is_number() && v["target"]["minHits"].is_number());
    assert!(v["target"]["requirements"][0]["minValue"].is_number());
    assert!(v["donors"][0]["mod"].is_string() && v["donors"][0]["priceChaos"].is_number());

    let solution = api::solve(b, &back);
    assert_eq!(solution.version, 1);
    assert!(solution.error.is_none(), "{:?}", solution.error);
    assert!(!solution.edges.is_empty());
    let stats = solution.stats.as_ref().expect("stats");
    assert!(stats.expected_cost.is_finite());

    // The Solution itself survives JSON too.
    let out = serde_json::to_string(&solution).expect("serialise solution");
    let again: Solution = serde_json::from_str(&out).expect("deserialise solution");
    assert_eq!(again, solution);
}

#[test]
fn parse_target_json_matches_contract() {
    let b = poe1::test_bundle();
    let v = serde_json::to_value(api::parse_target_api(b, FIXTURE)).unwrap();
    assert_eq!(v["itemClass"], "Amulets");
    assert_eq!(v["base"], "Simplex Amulet");
    assert_eq!(v["rarity"], "Rare");
    assert_eq!(v["ilvl"], 84);
    assert_eq!(v["influences"], serde_json::json!(["Shaper", "Warlord"]));
    assert_eq!(
        v["catalyst"],
        serde_json::json!({"name": "Intrinsic Catalyst", "quality": 20})
    );
    assert_eq!(v["minHits"], 3);
    assert_eq!(
        v["buyNote"],
        serde_json::json!({"amount": 800.0, "currency": "divine"})
    );
    assert_eq!(v["unmatched"], serde_json::json!([]));
    let reqs = v["requirements"].as_array().unwrap();
    assert_eq!(reqs.len(), 3);
    assert!(reqs.contains(&serde_json::json!({
        "group": "DamagePer15Attributes",
        "side": "Prefix",
        "accepted": ["DamagePer15StrengthInfluence1"],
        "minValue": 2.0,
        "label": "2% increased Damage per 15 Strength",
        "sources": ["Native"]
    })));
}

#[test]
fn influence_mods_lists_one_influence() {
    let b = poe1::test_bundle();
    let warlord = api::influence_mods(b, "Simplex Amulet", "Warlord").expect("mods");
    assert!(
        warlord
            .iter()
            .any(|m| m.id == "PercentageStrengthBodyInfluence2_" && m.side == "Suffix")
    );
    assert!(
        warlord
            .iter()
            .any(|m| m.id == "DamagePer15StrengthInfluence1" && m.side == "Prefix")
    );
    assert!(
        !warlord
            .iter()
            .any(|m| m.id == "PercentageAllAttributesUberShaper1")
    );
    let shaper = api::influence_mods(b, "Simplex Amulet", "Shaper").expect("mods");
    assert!(
        shaper
            .iter()
            .any(|m| m.id == "PercentageAllAttributesUberShaper1")
    );
    assert!(api::influence_mods(b, "Imaginary Amulet", "Warlord").is_err());
    assert!(api::influence_mods(b, "Simplex Amulet", "Nobody").is_err());
}
