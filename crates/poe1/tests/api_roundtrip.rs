//! V9: the fixture `Problem` survives JSON and solves through the wire API.

mod common;

use std::collections::BTreeMap;
use std::time::Instant;

use poe1::api::{
    self, DonorJson, GoodJson, Problem, ProblemV3, ProcessJson, ProcessKindJson, Solution,
};
use poe1::item::Rarity;

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

/// V-A2 (3x T1 physical Jewelled Foil) as a wire problem: every answer names the approximate rules
/// its route rests on and how far the root's value moves under their alternative readings.
#[test]
fn market_answer_names_approximate_rules_and_range() {
    let b = poe1::test_bundle();
    let foil = |ids: &[&str], min_hits: u16| {
        let mut t = common::target(
            "Jewelled Foil",
            84,
            Rarity::Rare,
            ids.iter().map(|id| common::req(&[id])).collect(),
        );
        t.min_hits = min_hits;
        api::target_json(b, &t)
    };
    let (phys, hybrid, flat, fourth) = (
        "LocalIncreasedPhysicalDamagePercent8",
        "LocalIncreasedPhysicalDamagePercentAndAccuracyRating8",
        "LocalAddedPhysicalDamage9",
        "LocalAddedFireDamage9",
    );
    let good = |id: &str, target, buy| GoodJson {
        id: id.into(),
        name: id.into(),
        target,
        buy,
        sell: None,
    };
    let recombine = |id: &str, output: &str, a: &str, c: &str| ProcessJson {
        id: id.into(),
        output: output.into(),
        recipe: Some("recombine-split".into()),
        kind: ProcessKindJson::Recombine {
            a: a.into(),
            b: c.into(),
        },
        expanded: None,
        added: Vec::new(),
        disabled: Vec::new(),
        buy: Vec::new(),
        sell: Vec::new(),
    };
    let problem = ProblemV3 {
        version: 3,
        bundle_patch: b.patch.clone(),
        goods: vec![
            good("main", foil(&[phys, hybrid, flat], 3), None),
            good("a2", foil(&[phys, hybrid], 2), None),
            good("b2", foil(&[flat, fourth], 2), None),
            good("phys", foil(&[phys], 1), Some(10.0)),
            good("hybrid", foil(&[hybrid], 1), Some(10.0)),
            good("flat", foil(&[flat], 1), Some(10.0)),
            good("fourth", foil(&[fourth], 1), Some(10.0)),
        ],
        processes: vec![
            recombine("ra", "a2", "phys", "hybrid"),
            recombine("rb", "b2", "flat", "fourth"),
            recombine("rm", "main", "a2", "b2"),
        ],
        root: "main".into(),
        focus: None,
        prices: common::prices(),
        donors: Vec::new(),
        runs: 1000,
        seed: 7,
    };
    let json = serde_json::to_string(&problem).expect("serialise problem");
    // For the manual UI check: lz-compress this into a `#p=` link.
    println!("{json}");

    let start = Instant::now();
    let sol = api::solve_json(b, &json).expect("solves");
    let ms = start.elapsed().as_secs_f64() * 1e3;
    println!("api::solve_json {ms:.1} ms");
    assert!(ms < 2000.0, "V-A5: {ms} ms");
    assert!(sol.error.is_none(), "{:?}", sol.error);

    let ids: Vec<&str> = sol.approximate.iter().map(|c| c.id.as_str()).collect();
    println!("approximate {ids:?}");
    for want in [
        "recombine.count_table",
        "recombine.selection_alpha",
        "recombine.output_rarity",
        "recombine.cost",
    ] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    // No craft on the route, so no magic-orb rule.
    assert!(!ids.contains(&"magic.mod_count"));
    assert!(
        sol.approximate
            .iter()
            .all(|c| !c.statement.is_empty() && !c.reason.is_empty() && !c.url.is_empty())
    );

    let root = sol
        .market
        .goods
        .iter()
        .find(|g| g.id == "main")
        .and_then(|g| g.value)
        .expect("root value");
    let range = sol.range.as_ref().expect("range");
    println!("V(main) {root:.2}, range {range:?}");
    let reading = |label: &str| {
        range
            .readings
            .iter()
            .find(|r| r.label == label)
            .and_then(|r| r.value)
            .unwrap_or_else(|| panic!("no {label} reading"))
    };
    let (low, high, wiki) = (
        reading("α 0.4"),
        reading("α 1.6"),
        reading("wiki count table"),
    );
    assert!(wiki.is_finite());
    assert!(range.low <= root && root <= range.high);
    // A flatter selection keeps the rare T1 mods more often, a steeper one less.
    assert!(low < root && root < high, "{low} {root} {high}");

    // The wire names the UI reads.
    let v = serde_json::to_value(&sol).expect("serialise solution");
    assert!(v["approximate"][0]["statement"].is_string());
    assert!(v["range"]["readings"][0]["label"].is_string());
    assert!(v["market"]["processes"][0]["claims"].is_array());
}

#[test]
fn custom_process_round_trips() {
    let b = poe1::test_bundle();
    let main = common::target(
        "Lapis Amulet",
        84,
        Rarity::Magic,
        vec![common::req(&["GlobalLightningGemLevel1"])],
    );
    let input = common::target("Lapis Amulet", 84, Rarity::Normal, Vec::new());
    let problem = ProblemV3 {
        version: 3,
        bundle_patch: b.patch.clone(),
        goods: vec![
            GoodJson {
                id: "main".into(),
                name: "main".into(),
                target: api::target_json(b, &main),
                buy: None,
                sell: None,
            },
            GoodJson {
                id: "in".into(),
                name: "in".into(),
                target: api::target_json(b, &input),
                buy: Some(100.0),
                sell: None,
            },
        ],
        processes: vec![ProcessJson {
            id: "c".into(),
            output: "main".into(),
            recipe: None,
            kind: ProcessKindJson::Custom {
                from: Some("in".into()),
                name: "Veiled chaos and unveil".into(),
                chance: 0.25,
                cost: 10.0,
                keeps_input: false,
            },
            expanded: None,
            added: Vec::new(),
            disabled: Vec::new(),
            buy: Vec::new(),
            sell: Vec::new(),
        }],
        root: "main".into(),
        focus: None,
        prices: common::prices(),
        donors: Vec::new(),
        runs: 1000,
        seed: 7,
    };
    let v = serde_json::to_value(&problem).expect("serialise problem");
    assert_eq!(
        v["processes"][0]["kind"]["custom"]["keepsInput"],
        serde_json::json!(false),
        "{v}"
    );
    let sol = api::solve_json(b, &v.to_string()).expect("solves");
    assert!(sol.error.is_none(), "{:?}", sol.error);
    let c = sol
        .market
        .processes
        .iter()
        .find(|p| p.id == "c")
        .expect("the custom process");
    assert_eq!(c.kind, "custom");
    assert_eq!(c.name, "Veiled chaos and unveil");
    let cost = c.cost.expect("cost");
    assert!((cost - 440.0).abs() < 1e-9, "cost {cost}");
}
