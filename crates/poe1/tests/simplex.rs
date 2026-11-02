//! V8: the Simplex fixture through the wire API — parse, propose, Use, solve.

use std::collections::BTreeMap;
use std::time::Instant;

use poe1::api::{self, DonorJson, Problem, ProposalJson, Solution};
use poe1::item::{ItemState, Side};
use poe1::kernels::Action;

const PRICES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/defaultPrices.json"
));
const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/simplex_target.txt"
));

fn prices() -> BTreeMap<String, f64> {
    let v: serde_json::Value = serde_json::from_str(PRICES).expect("defaultPrices.json");
    serde_json::from_value(v["prices"].clone()).expect("prices map")
}

fn fixture_problem() -> Problem {
    let b = poe1::test_bundle();
    let draft = api::parse_target_api(b, FIXTURE);
    let prices = prices();
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

fn timed_solve(p: &Problem) -> Solution {
    let b = poe1::test_bundle();
    let start = Instant::now();
    let s = api::solve(b, p);
    println!(
        "api::solve: {:.1} ms ({} nodes, {} edges)",
        start.elapsed().as_secs_f64() * 1e3,
        s.nodes.len(),
        s.edges.len()
    );
    assert!(s.error.is_none(), "{:?}", s.error);
    s
}

fn use_proposal(p: &mut Problem, pr: &ProposalJson) {
    p.added.push(pr.pre.clone());
    p.expanded.push(pr.pre.clone());
}

fn state(key: &str) -> ItemState {
    serde_json::from_str(key).expect("state key")
}

/// `serde_json::to_string(&action)` for an action read back from the solution JSON.
fn action_key_of(v: &serde_json::Value) -> String {
    let action: Action = serde_json::from_value(v.clone()).expect("action");
    serde_json::to_string(&action).unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn simplex_v8() {
    let mut p = fixture_problem();

    // Goal: the only craft proposal is the target's catalyst.
    let s = timed_solve(&p);
    let goal = &s.proposals["goal"];
    assert_eq!(goal.len(), 1, "{goal:#?}");
    let apply = &goal[0];
    assert!(matches!(apply.action, Action::ApplyCatalyst { .. }));
    assert!(close(apply.cost, 20.0 * 2.92, 1e-9), "{}", apply.cost);
    assert_eq!(apply.p_reach, 1.0);
    // A goal outcome is worth 0, so the only cost of reaching the goal from here is the catalysts.
    let q = apply.q.expect("a goal outcome always has a value");
    assert!(close(q, 20.0 * 2.92, 1e-9), "goal proposal q {q}");
    println!("goal <- {} cost {:.4}", apply.label, apply.cost);

    // Use it: the uncatalysed ideal state.
    use_proposal(&mut p, apply);
    let uncat_key = apply.pre.clone();
    assert_eq!(state(&uncat_key).catalyst, None);
    let s = timed_solve(&p);
    let props = &s.proposals[&uncat_key];
    for pr in props {
        println!(
            "  {:<48} p_reach={:.10} p_mods={:.10} cost={:.4} status={}",
            pr.label, pr.p_reach, pr.p_mods, pr.cost, pr.status
        );
    }

    let divine = props
        .iter()
        .find(|pr| matches!(pr.action, Action::Divine))
        .expect("Divine proposal");
    assert!(
        close(divine.p_reach, 1.0 / 16.0, 1e-12),
        "{}",
        divine.p_reach
    );
    assert!(close(divine.cost, 389.5, 1e-9));
    let low = state(&divine.pre);
    assert!(low.suffixes.wanted.iter().all(|h| !h.value_ok));

    let harvest_target = 500.0 / 11_700.0;
    let harvests: Vec<&ProposalJson> = props
        .iter()
        .filter(|pr| {
            matches!(pr.action, Action::HarvestInfluenced)
                && close(pr.p_mods, harvest_target, 1e-12)
        })
        .collect();
    assert_eq!(harvests.len(), 2, "plain and Intrinsic-steered");
    for h in &harvests {
        assert!(close(h.p_reach, harvest_target, 1e-12), "{}", h.p_reach);
        assert!(close(h.cost, 273.46, 0.01), "{}", h.cost);
    }
    assert!(harvests.iter().any(|h| state(&h.pre).catalyst.is_none()));
    assert!(harvests.iter().any(|h| state(&h.pre).catalyst.is_some()));

    let combine = props
        .iter()
        .find(|pr| {
            matches!(pr.action, Action::Combine { donor: 0 }) && state(&pr.pre).catalyst.is_none()
        })
        .expect("Combine proposal");
    let combine_mods = 500.0 / 99_000.0;
    assert!(
        close(combine.p_mods, combine_mods, 1e-12),
        "{}",
        combine.p_mods
    );
    assert!(
        close(combine.p_reach, combine_mods / 16.0, 1e-12),
        "{}",
        combine.p_reach
    );
    let receiver = state(&combine.pre);
    assert_eq!(receiver.influences, 1, "Shaper only");
    assert!(receiver.prefixes.wanted.is_empty());
    assert_eq!(
        receiver.suffixes.wanted.len(),
        1,
        "only the attribute suffix"
    );

    // Use the (uncatalysed) harvest reforge: its locked predecessor proposes the bench lock.
    let harvest = harvests
        .iter()
        .find(|h| state(&h.pre).catalyst.is_none())
        .expect("plain harvest");
    use_proposal(&mut p, harvest);
    let locked_key = harvest.pre.clone();
    assert_eq!(state(&locked_key).lock, 0b10);
    let s = timed_solve(&p);
    let bench = s.proposals[&locked_key]
        .iter()
        .find(|pr| matches!(pr.action, Action::BenchLock { side: Side::Suffix }))
        .expect("BenchLock{Suffix} proposal");
    assert!(close(bench.cost, 779.0, 1e-9), "{}", bench.cost);

    let stats = s.stats.as_ref().expect("stats");
    println!(
        "expectedCost {:.4} sd {:.4} seconds {:.1} quantiles {:?}",
        stats.expected_cost, stats.sd_cost, stats.expected_seconds, stats.quantiles
    );
    assert!(
        stats.expected_cost <= 311_600.01 + 1e-6,
        "{}",
        stats.expected_cost
    );
}

#[test]
fn unknown_base_is_a_parse_error() {
    let mut p = fixture_problem();
    p.target.base = "Imaginary Amulet".into();
    let s = api::solve(poe1::test_bundle(), &p);
    let error = s.error.expect("error");
    assert_eq!(error.kind, "Parse");
    assert!(
        error.message.contains("Imaginary Amulet"),
        "{}",
        error.message
    );
    assert!(s.nodes.is_empty() && s.edges.is_empty());
}

#[test]
fn every_state_has_a_view_and_every_action_a_key() {
    let mut p = fixture_problem();
    let s = timed_solve(&p);
    let apply = s.proposals["goal"][0].clone();
    use_proposal(&mut p, &apply);
    let s = timed_solve(&p);
    let harvest = s.proposals[&apply.pre]
        .iter()
        .find(|pr| matches!(pr.action, Action::HarvestInfluenced))
        .expect("harvest")
        .clone();
    use_proposal(&mut p, &harvest);
    let s = timed_solve(&p);

    let json: serde_json::Value = serde_json::to_value(&s).expect("solution json");
    let nodes = json["nodes"].as_array().expect("nodes");
    let mut states = 0;
    for n in nodes {
        if n["kind"] == "state" {
            states += 1;
            let view = &n["view"];
            assert!(view.is_object(), "{n}");
            assert!(
                view["title"]
                    .as_str()
                    .is_some_and(|t| t.contains("Simplex Amulet"))
            );
            assert!(view["prefixes"].is_array() && view["suffixes"].is_array());
        } else {
            assert!(n["view"].is_null());
        }
    }
    assert!(states > 2);

    let edges = json["edges"].as_array().expect("edges");
    let mut crafts = 0;
    for e in edges {
        match e["kind"].as_str() {
            Some("craft") => {
                crafts += 1;
                let key = e["actionKey"].as_str().expect("craft actionKey");
                assert_eq!(key, action_key_of(&e["action"]));
            }
            Some("buy") | Some("sell") => {
                assert!(e["actionKey"].is_null() && e["action"].is_null());
            }
            other => panic!("edge kind {other:?}"),
        }
    }
    assert!(crafts > 0);

    let proposals = json["proposals"].as_object().expect("proposals");
    assert!(proposals.len() >= 3);
    for list in proposals.values() {
        for pr in list.as_array().expect("list") {
            let key = pr["actionKey"].as_str().expect("proposal actionKey");
            assert_eq!(key, action_key_of(&pr["action"]));
        }
    }

    // A disabled pair uses exactly that key.
    let mut p2 = p.clone();
    p2.disabled
        .push((apply.pre.clone(), harvest.action_key.clone()));
    let s2 = timed_solve(&p2);
    let st = s2.proposals[&apply.pre]
        .iter()
        .find(|pr| pr.pre == harvest.pre && pr.action_key == harvest.action_key)
        .expect("same proposal");
    assert_eq!(
        st.status, "Ready",
        "disabling a craft on the node leaves predecessors alone"
    );
    p2.disabled = vec![(harvest.pre.clone(), harvest.action_key.clone())];
    let s3 = timed_solve(&p2);
    let st = s3.proposals[&apply.pre]
        .iter()
        .find(|pr| pr.pre == harvest.pre && pr.action_key == harvest.action_key)
        .expect("same proposal");
    assert_eq!(st.status, "Disabled");

    println!(
        "sample node: {}",
        serde_json::to_string(
            nodes
                .iter()
                .find(|n| n["key"] == apply.pre.as_str())
                .expect("uncatalysed ideal node")
        )
        .unwrap()
    );
    println!(
        "sample edge: {}",
        serde_json::to_string(
            edges
                .iter()
                .find(|e| e["kind"] == "craft" && e["action"]["kind"] == "HarvestInfluenced")
                .expect("harvest edge")
        )
        .unwrap()
    );
    println!(
        "sample proposal: {}",
        serde_json::to_string(&proposals[&apply.pre][0]).unwrap()
    );
}

#[test]
fn money_loop_names_the_state_readably() {
    let mut p = fixture_problem();
    let s = timed_solve(&p);
    let apply = s.proposals["goal"][0].clone();
    use_proposal(&mut p, &apply);
    let s = timed_solve(&p);
    let divine = s.proposals[&apply.pre]
        .iter()
        .find(|pr| matches!(pr.action, Action::Divine))
        .expect("Divine proposal")
        .clone();
    // Buy the ideal state cheaply, Divine it, sell the low roll for a fortune: a money loop.
    p.buy.push((apply.pre.clone(), 100.0));
    p.sell.push((divine.pre.clone(), 1_000_000.0));
    let s = api::solve(poe1::test_bundle(), &p);
    let error = s.error.expect("money loop error");
    println!("{}: {}", error.kind, error.message);
    assert_eq!(error.kind, "Diverged");
    assert!(
        error.message.contains("Rare Simplex Amulet"),
        "{}",
        error.message
    );
    assert!(
        !error.message.contains("{\""),
        "raw key leaked: {}",
        error.message
    );
}
