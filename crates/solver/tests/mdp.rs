//! Policy iteration, reachability and moments (plan V7).

mod common;

use common::{
    SpinDomain, SpinState, TState, TradeDomain, absorbing_stats_inputs, loop_graph, map,
    prop_graph, slam_graph,
};
use solver::graph::{EdgeKind, GOAL};
use solver::{Graph, GraphInput, START, SolveError, absorbing_stats, moments, quantiles, solve};

#[test]
fn absorbing_stats_slam_annul() {
    let (q, r, c) = absorbing_stats_inputs();
    let st = absorbing_stats(&q, &r, &c);
    assert!(
        (st.absorption[0][0] - 2.0 / 11.0).abs() < 1e-12,
        "absorption into goal: {}",
        st.absorption[0][0]
    );
    assert!(
        (st.absorption[0][1] - 9.0 / 11.0).abs() < 1e-12,
        "the rest ends in the brick: {}",
        st.absorption[0][1]
    );
    assert!(
        (st.steps[0] - 5.090_909_090_909_091).abs() < 1e-6,
        "steps: {}",
        st.steps[0]
    );
    assert!(
        (st.m1[0] - 4.681_818_181_818_182).abs() < 1e-6,
        "m1: {}",
        st.m1[0]
    );
    assert!(
        (st.sd[0] - 4.401_821_561_336_993).abs() < 1e-6,
        "sd: {}",
        st.sd[0]
    );
}

#[test]
fn ssp_slam_annul_with_restart() {
    // The same chain, with the brick sold for 2 and `start` buying the slam state for 10, so
    // V(brick) = -2 + V(start) = 8 + V(slam). The closed form is then
    // V(slam) = (20·c_slam + 27·c_annul + 9·c_recraft + 72) / 22, i.e. 61.75 for (1, 0.5, 2).
    let g = slam_graph(
        &[("slam", 1.0), ("annul", 0.5), ("recraft", 2.0)],
        &[("slam", 10.0)],
        &[("brick", 2.0)],
        0.0,
    );
    let s = solve(&g).expect("chain solves");
    let slam = g.node("slam").expect("slam node");
    let v_slam = s.value[slam].expect("slam reaches the goal");
    let v_start = s.value[START].expect("start reaches the goal");
    assert!((v_slam - 61.75).abs() < 1e-9, "V(slam) = {v_slam}");
    assert!((v_start - 71.75).abs() < 1e-9, "V(start) = {v_start}");

    let m = moments(&g, &s).expect("moments of a solvable plan");
    assert!(
        (m.expected_cost - 71.75).abs() < 1e-9,
        "expected cost {}",
        m.expected_cost
    );
    // start is bought once per attempt, and every brick sends the plan back to it: 5.5 visits.
    assert!(
        (m.expected_buy_chaos - 55.0).abs() < 1e-9,
        "buys {}",
        m.expected_buy_chaos
    );
    assert!(
        (m.expected_sell_chaos - 9.0).abs() < 1e-9,
        "sells {}",
        m.expected_sell_chaos
    );
    assert!(m.sd_cost > 0.0);
    let unit = map(&[("slam", 1.0), ("annul", 0.5), ("recraft", 2.0)]);
    let spend: f64 = m.expected_spend.iter().map(|(k, q)| q * unit[k]).sum();
    assert!(
        (spend + m.expected_buy_chaos - m.expected_sell_chaos - m.expected_cost).abs() < 1e-9,
        "spend {spend} + buys − sells = cost {}",
        m.expected_cost
    );
    assert!((m.expected_spend["slam"] - 10.0).abs() < 1e-9, "10 slams");
    assert!(
        (m.expected_seconds - 358.0).abs() < 1e-9,
        "5.5 buys at 60 s plus 28 crafts at 1 s: {}",
        m.expected_seconds
    );
}

#[test]
fn sell_price_is_clamped_to_buy() {
    let mut gi = GraphInput {
        seeds: vec![TState::Item],
        expanded: ["item".to_owned()].into_iter().collect(),
        disabled: Default::default(),
        prices: map(&[("x", 1.0)]),
        buy: map(&[("item", 3.0)]),
        sell: map(&[("item", 10.0)]),
        trade_penalty: 0.5,
    };
    let g = Graph::build(&TradeDomain, &gi).expect("trade graph builds");
    let item = g.node("item").expect("item node");
    let sell = g
        .edges
        .iter()
        .find(|e| e.from == item && matches!(e.kind, EdgeKind::Sell))
        .expect("every state node is sellable");
    assert!(
        (sell.cost - (-2.5)).abs() < 1e-12,
        "sell cost {}",
        sell.cost
    );

    // Without a sell price the node is still sellable, at a cost of just the penalty.
    gi.sell.clear();
    let g = Graph::build(&TradeDomain, &gi).expect("trade graph builds");
    let sell = g
        .edges
        .iter()
        .find(|e| matches!(e.kind, EdgeKind::Sell))
        .expect("sell edge");
    assert!((sell.cost - 0.5).abs() < 1e-12, "sell cost {}", sell.cost);
}

#[test]
fn zero_price_loop_terminates() {
    let gi = GraphInput {
        seeds: vec![SpinState::Spin],
        expanded: ["spin".to_owned()].into_iter().collect(),
        disabled: Default::default(),
        prices: map(&[("zero", 0.0)]),
        buy: map(&[("spin", 0.0)]),
        sell: Default::default(),
        // A trade penalty is what stops the free sell/re-buy loop; poe1 always passes one.
        trade_penalty: 0.01,
    };
    let g = Graph::build(&SpinDomain, &gi).expect("spin graph builds");
    let craft = g
        .edges
        .iter()
        .find(|e| matches!(e.kind, EdgeKind::Craft { .. }))
        .expect("craft edge");
    assert!((craft.cost - 1e-6).abs() < 1e-15, "the cost floor applies");
    let s = solve(&g).expect("a free action still converges");
    let v = s.value[START].expect("start reaches the goal");
    assert!(v.is_finite());
    let spin = g.node("spin").expect("spin node");
    let v_spin = s.value[spin].expect("the spin state reaches the goal");
    assert!((v_spin - 2e-6).abs() < 1e-12, "V(spin) = {v_spin}");
    assert!((v - (0.01 + 2e-6)).abs() < 1e-12, "V(start) = {v}");
}

#[test]
fn unreachable_goal_is_reported() {
    // Nothing can be bought, so `start` has no way in at all.
    let g = slam_graph(
        &[("slam", 1.0), ("annul", 0.5), ("recraft", 2.0)],
        &[],
        &[("brick", 2.0)],
        0.0,
    );
    let s = solve(&g).expect("the states themselves still solve");
    assert_eq!(
        s.value[START], None,
        "start is outside the reachability set"
    );
    assert_eq!(s.policy[START], None);
    // Without a buy price `start` is a sink, so a brick is fatal: nothing is worth solving.
    assert_eq!(s.value[g.node("slam").unwrap()], None);
    assert!(moments(&g, &s).is_none());
    assert!(quantiles(&g, &s, 100, 1).expect("quantiles run").is_none());
}

#[test]
fn money_loop_is_reported() {
    let g = loop_graph();
    match solve(&g) {
        Err(SolveError::Diverged { node, message }) => {
            assert!(
                message.contains("money loop"),
                "message should name the money loop: {message}"
            );
            assert!(
                node == "top" || node == "deep",
                "the loop is reported on a state node, got {node}"
            );
        }
        other => panic!("expected a divergence, got {other:?}"),
    }
}

#[test]
fn missing_price_names_the_key() {
    let mut gi = GraphInput {
        seeds: vec![TState::Item],
        expanded: ["item".to_owned()].into_iter().collect(),
        disabled: Default::default(),
        prices: Default::default(),
        buy: Default::default(),
        sell: Default::default(),
        trade_penalty: 0.0,
    };
    match Graph::<TState, _>::build(&TradeDomain, &gi) {
        Err(err) => {
            assert_eq!(err.kind(), "MissingPrice");
            assert!(err.to_string().contains('x'), "{err}");
            assert!(matches!(&err, SolveError::MissingPrice(k) if k == "x"));
        }
        Ok(_) => panic!("a missing price must be an error"),
    }
    gi.prices = map(&[("x", 1.0)]);
    assert!(Graph::build(&TradeDomain, &gi).is_ok());
}

#[test]
fn goal_has_no_craft_edges_and_start_only_buys() {
    let g = prop_graph(&[("craft", 1.0), ("finish", 5.0)], &[("n_ok", 100.0)], &[]);
    let start_edges: Vec<&_> = g.edges.iter().filter(|e| e.from == START).collect();
    assert!(!start_edges.is_empty());
    assert!(start_edges.iter().all(|e| matches!(e.kind, EdgeKind::Buy)));
    assert!(
        !g.edges.iter().any(|e| e.from == GOAL),
        "the goal vertex never leaves"
    );
    for (ei, e) in g.edges.iter().enumerate() {
        assert!(
            g.out[e.from].contains(&ei),
            "out must list every edge leaving a node"
        );
    }
}

#[test]
fn rare_success_seeds_a_proper_policy() {
    // 1e-4 per try at 1 chaos, next to a node that is cheaper to sell and re-buy than to craft, and
    // a 300,000-chaos goal. Seeding from a distance BFS is exact and instant; a truncated value
    // iteration either needs ~1e5 sweeps or mistakes the detour for a money loop.
    let started = std::time::Instant::now();
    let g = common::rare_graph();
    let s = solve(&g).expect("a cheap sell/re-buy detour is not a money loop");
    let elapsed = started.elapsed();
    assert!(elapsed < std::time::Duration::from_secs(1), "{elapsed:?}");

    let rare = s.value[g.node("rare").expect("rare node")].expect("rare reaches the goal");
    let pricey = s.value[g.node("pricey").expect("pricey node")].expect("pricey reaches the goal");
    let start = s.value[START].expect("start reaches the goal");
    assert!(
        (rare - 10_000.0).abs() < 1e-6,
        "one chaos per try at p=1e-4: {rare}"
    );
    assert!(
        (pricey - 299_910.02).abs() < 1e-6,
        "sell for 90 then buy the goal for 300,000: {pricey}"
    );
    assert!((start - 300_000.01).abs() < 1e-6, "V(start) {start}");
    let moments = moments(&g, &s).expect("moments");
    assert!(moments.expected_cost.is_finite());
}

#[test]
fn a_self_looping_craft_does_not_seed_the_policy() {
    // `Nothing` is a p=1 self-loop: picking it would make the linear system singular.
    let g = common::inert_graph();
    let s = solve(&g).expect("the goal-reaching edge is available");
    let stuck = g.node("stuck").expect("stuck node");
    assert!((s.value[stuck].expect("stuck reaches the goal") - 5.0).abs() < 1e-12);
    let chosen = &g.edges[s.policy[stuck].expect("a policy edge")];
    assert_eq!(
        chosen.label, "Fix",
        "the p=1 self-loop must never be the policy"
    );
    assert!((s.value[START].expect("start reaches the goal") - 15.0).abs() < 1e-12);
}
