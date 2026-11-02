//! Policy quantiles (plan V7 / Step 7 `stats.rs`).

mod common;

use common::slam_graph;
use solver::{quantiles, solve};

fn chain() -> (
    solver::Graph<common::SState, common::SAction>,
    solver::Solved,
) {
    let g = slam_graph(
        &[("slam", 1.0), ("annul", 0.5), ("recraft", 2.0)],
        &[("slam", 10.0)],
        &[("brick", 2.0)],
        0.0,
    );
    let s = solve(&g).expect("the chain solves");
    (g, s)
}

#[test]
fn quantiles_are_deterministic_for_a_fixed_seed() {
    let (g, s) = chain();
    let a = quantiles(&g, &s, 4000, 7)
        .expect("runs")
        .expect("start is in R");
    let b = quantiles(&g, &s, 4000, 7)
        .expect("runs")
        .expect("start is in R");
    assert_eq!(a, b, "the same seed must replay the same runs");

    for q in [a.cost, a.seconds] {
        assert!(q.p50.is_finite() && q.p99.is_finite());
        assert!(q.p50 <= q.p90 && q.p90 <= q.p95 && q.p95 <= q.p99, "{q:?}");
    }
    // The plan always buys (10 chaos, 60 s) and every craft is a second.
    assert!(a.cost.p50 >= 10.0, "median cost {}", a.cost.p50);
    assert!(a.seconds.p50 >= 60.0, "median seconds {}", a.seconds.p50);
    assert!(
        a.cost.p99 > a.cost.p50,
        "a long-tailed plan: p50 {} p99 {}",
        a.cost.p50,
        a.cost.p99
    );

    let other = quantiles(&g, &s, 4000, 8).expect("runs").expect("in R");
    assert!(
        (other.cost.p50 - a.cost.p50).abs() < a.cost.p50,
        "another seed gives the same order of magnitude"
    );
}

#[test]
fn quantiles_of_one_run_are_the_single_sample() {
    let (g, s) = chain();
    let q = quantiles(&g, &s, 1, 1).expect("runs").expect("in R");
    assert_eq!(q.cost.p50, q.cost.p99);
    assert_eq!(q.seconds.p50, q.seconds.p99);
    assert!(q.cost.p50 >= 10.0);
}
