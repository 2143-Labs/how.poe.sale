//! Monte Carlo quantiles of the solved policy. Crafting cost distributions are long-tailed, so a
//! p90/p99 pair says more about a plan than any single "confidence" number.

use rand::{RngExt, SeedableRng};
use rand_pcg::Pcg64Mcg;

use crate::error::SolveError;
use crate::graph::{GOAL, Graph, START};
use crate::mdp::Solved;

/// Simulated runs of one policy; above this the policy is not terminating.
const MAX_STEPS: u64 = 1_000_000;

/// Four quantiles of one quantity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Q4 {
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
}

/// Quantiles of the cost and the time of a plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantiles {
    pub cost: Q4,
    pub seconds: Q4,
}

/// Walk the solved policy from `start` `runs` times and report the cost and time quantiles.
/// `None` when `start` is outside the almost-sure reachability set.
pub fn quantiles<S, A>(
    g: &Graph<S, A>,
    s: &Solved,
    runs: u32,
    seed: u64,
) -> Result<Option<Quantiles>, SolveError> {
    if s.policy.get(START).copied().flatten().is_none() {
        return Ok(None);
    }
    let runs = runs.max(1);
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    let mut costs: Vec<f64> = Vec::with_capacity(runs as usize);
    let mut times: Vec<f64> = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        let (cost, seconds) = run_once(g, s, &mut rng)?;
        costs.push(cost);
        times.push(seconds);
    }
    Ok(Some(Quantiles {
        cost: q4(&mut costs),
        seconds: q4(&mut times),
    }))
}

/// One Monte Carlo path; `NoTermination` when it never reaches the goal.
fn run_once<S, A>(
    g: &Graph<S, A>,
    s: &Solved,
    rng: &mut Pcg64Mcg,
) -> Result<(f64, f64), SolveError> {
    let mut node = START;
    let mut cost = 0.0;
    let mut seconds = 0.0;
    let mut steps = 0u64;
    while node != GOAL {
        steps += 1;
        if steps > MAX_STEPS {
            return Err(SolveError::NoTermination);
        }
        let Some(ei) = s.policy.get(node).copied().flatten() else {
            return Err(SolveError::NoTermination);
        };
        let edge = &g.edges[ei];
        cost += edge.cost;
        seconds += edge.seconds;
        let u: f64 = rng.random();
        let mut acc = 0.0;
        let mut next = edge.outcomes.last().map_or(GOAL, |(o, _)| *o);
        for &(o, p) in &edge.outcomes {
            acc += p;
            if u < acc {
                next = o;
                break;
            }
        }
        node = next;
    }
    Ok((cost, seconds))
}

/// Nearest-rank quantiles: index `ceil(p·n)`, clamped into the sample.
fn q4(samples: &mut [f64]) -> Q4 {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let at = |p: f64| -> f64 {
        if n == 0 {
            return 0.0;
        }
        let rank = (p * n as f64).ceil() as usize;
        samples[rank.clamp(1, n) - 1]
    };
    Q4 {
        p50: at(0.50),
        p90: at(0.90),
        p95: at(0.95),
        p99: at(0.99),
    }
}
