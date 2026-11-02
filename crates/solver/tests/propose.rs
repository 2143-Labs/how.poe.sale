//! Backward proposals (plan Step 7 `propose.rs`).

mod common;

use common::{PAction, PState, PropDomain, input, map, prop_graph};
use solver::{Graph, ProposalStatus, SolveError, propose, solve};

#[test]
fn propose_reports_p_reach_p_mods_and_q() {
    let g = prop_graph(
        &[("craft", 1.0), ("finish", 5.0)],
        &[("n_ok", 100.0)],
        &[("far", 5.0)],
    );
    let s = solve(&g).expect("the graph solves");
    let node = PState::N { ok: true };
    let props = propose(&PropDomain, &g, &s, "n_ok", &node).expect("proposals");
    assert_eq!(props.len(), 1);
    let p = &props[0];
    assert_eq!(p.action, PAction::Craft);
    assert_eq!(p.action_key, "\"Craft\"");
    assert_eq!(p.label, "Craft");
    assert_eq!(p.pre, "p");
    assert_eq!(p.pre_state, PState::P);
    assert!((p.cost - 1.0).abs() < 1e-12);
    assert!((p.p_reach - 0.5).abs() < 1e-12, "p_reach {}", p.p_reach);
    assert!((p.p_mods - 0.75).abs() < 1e-12, "p_mods {}", p.p_mods);
    // V(n_ok) = 5, V(start) = 105; `n_low` is unknown (worth a fresh start) and `far` sells for 5.
    let q = p.q.expect("every outcome is valued through V(start)");
    assert!((q - 54.75).abs() < 1e-9, "q {q}");
    assert_eq!(p.status, ProposalStatus::Ready);
}

#[test]
fn propose_without_a_sell_price_values_the_restart_at_v_start() {
    let g = prop_graph(&[("craft", 1.0), ("finish", 5.0)], &[("n_ok", 100.0)], &[]);
    let s = solve(&g).expect("the graph solves");
    let node = PState::N { ok: true };
    let props = propose(&PropDomain, &g, &s, "n_ok", &node).expect("proposals");
    let q = props[0].q.expect("every outcome is valued");
    // 1 + 0.5·5 + 0.25·105 + 0.25·105
    assert!((q - 56.0).abs() < 1e-9, "q {q}");
}

#[test]
fn propose_for_the_goal_counts_goal_outcomes() {
    let g = prop_graph(
        &[("craft", 1.0), ("finish", 5.0)],
        &[("n_ok", 100.0)],
        &[("far", 5.0)],
    );
    let s = solve(&g).expect("the graph solves");
    let props = propose(&PropDomain, &g, &s, "goal", &PState::Goal).expect("proposals");
    assert_eq!(props.len(), 2);
    let finish = &props[0];
    assert_eq!(finish.action, PAction::Finish);
    assert!((finish.p_reach - 1.0).abs() < 1e-12);
    assert!((finish.p_mods - 1.0).abs() < 1e-12);
    assert!((finish.q.expect("goal is worth 0") - 5.0).abs() < 1e-9);
    let craft = &props[1];
    assert_eq!(craft.action, PAction::Craft);
    assert_eq!(craft.p_reach, 0.0, "no craft outcome is a goal");
    assert_eq!(craft.p_mods, 0.0);
    assert_eq!(craft.status, ProposalStatus::Ready);
    assert!(
        props[0].q.unwrap() < props[1].q.unwrap(),
        "Ready proposals are ordered by q"
    );
}

#[test]
fn propose_marks_disabled_actions() {
    let mut gi = input(vec![PState::N { ok: true }], &["n_ok"]);
    gi.prices = map(&[("craft", 1.0), ("finish", 5.0)]);
    gi.buy = map(&[("n_ok", 100.0)]);
    gi.disabled.insert(("p".to_owned(), "\"Craft\"".to_owned()));
    let g = Graph::build(&PropDomain, &gi).expect("the graph builds");
    let s = solve(&g).expect("the graph solves");
    let node = PState::N { ok: true };
    let props = propose(&PropDomain, &g, &s, "n_ok", &node).expect("proposals");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].status, ProposalStatus::Disabled);
    assert!(
        props[0].q.is_some(),
        "a disabled proposal still shows what it would cost"
    );
}

#[test]
fn propose_needs_input_when_start_is_unreachable() {
    // No buy price at all: `start` is outside R, so an unknown outcome has no value to fall back on.
    let g = prop_graph(&[("craft", 1.0), ("finish", 5.0)], &[], &[]);
    let s = solve(&g).expect("the node itself still solves");
    assert_eq!(s.value[solver::START], None);
    let props = propose(&PropDomain, &g, &s, "n_ok", &PState::N { ok: true }).expect("proposals");
    assert_eq!(props.len(), 1);
    let p = &props[0];
    assert_eq!(p.status, ProposalStatus::NeedsInput);
    assert!(p.q.is_none(), "an unvalued outcome leaves q unknown");
    assert!(
        (p.p_reach - 0.5).abs() < 1e-12,
        "the probability is still known"
    );
    assert!((p.p_mods - 0.75).abs() < 1e-12);
}

#[test]
fn propose_reports_a_missing_price() {
    let g = prop_graph(&[("finish", 5.0)], &[("n_ok", 100.0)], &[]);
    let s = solve(&g).expect("the graph solves");
    match propose(&PropDomain, &g, &s, "n_ok", &PState::N { ok: true }) {
        Err(err) => {
            assert_eq!(err.kind(), "MissingPrice");
            assert!(matches!(&err, SolveError::MissingPrice(k) if k == "craft"));
        }
        Ok(_) => panic!("pricing the predecessor action must fail"),
    }
}
