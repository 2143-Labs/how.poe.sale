//! Policy iteration over the stochastic shortest-path graph, plus the moment statistics of the
//! solved policy.

use std::collections::VecDeque;

use crate::error::SolveError;
use crate::graph::{EdgeKind, GOAL, Graph, START};

/// Improvement rounds before giving up.
const MAX_ITERATIONS: u32 = 200;
/// Relative slack: a switch must beat the incumbent by more than this to count.
const SWITCH_EPS: f64 = 1e-9;

/// The solution of a [`Graph`]: the cost to reach the goal from each node, and the edge each node
/// should take. `None` marks a node outside the almost-sure reachability set.
#[derive(Clone, Debug)]
pub struct Solved {
    pub value: Vec<Option<f64>>,
    pub policy: Vec<Option<usize>>,
    pub iterations: u32,
}

/// Expected cost, spread and spend of the solved policy from `start`.
#[derive(Clone, Debug, Default)]
pub struct Moments {
    pub expected_cost: f64,
    pub sd_cost: f64,
    pub expected_seconds: f64,
    /// Expected number of each craft line item consumed.
    pub expected_spend: std::collections::BTreeMap<String, f64>,
    /// Expected chaos spent on buys / received from sells, both positive.
    pub expected_buy_chaos: f64,
    pub expected_sell_chaos: f64,
}

/// Moments of a fixed policy given its transient-to-transient block `q`, transient-to-absorbing
/// block `r`, and per-transient cost `c`.
#[derive(Clone, Debug)]
pub struct AbsorbingStats {
    /// `N·R`: probability of each absorbing state.
    pub absorption: Vec<Vec<f64>>,
    /// `N·1`: expected number of steps.
    pub steps: Vec<f64>,
    /// `N·c`: expected accumulated cost.
    pub m1: Vec<f64>,
    /// Standard deviation of the accumulated cost.
    pub sd: Vec<f64>,
}

/// Solve the graph with policy iteration.
pub fn solve<S, A>(g: &Graph<S, A>) -> Result<Solved, SolveError> {
    let n = g.nodes.len();
    let live = almost_sure_set(g);
    let adm = admissible(g, &live);
    let mut row_of = vec![usize::MAX; n];
    let mut rows: Vec<usize> = Vec::new();
    for (v, is_live) in live.iter().enumerate() {
        if *is_live && v != GOAL {
            row_of[v] = rows.len();
            rows.push(v);
        }
    }

    let dist = distances(g, &adm);
    let mut policy = initial_policy(g, &adm, &dist, &rows)?;
    debug_assert!(
        loop_node(g, &policy, &live).is_none(),
        "the distance policy must reach the goal from every node of R"
    );

    let mut value: Vec<Option<f64>> = vec![None; n];
    value[GOAL] = Some(0.0);
    let mut iterations = 0u32;
    loop {
        let (a, c) = system(g, &rows, &row_of, &policy)?;
        let v = solve_linear(a, c);
        for (i, &node) in rows.iter().enumerate() {
            if !v[i].is_finite() {
                return Err(SolveError::diverged(
                    &g.nodes[node].key,
                    "The plan has no finite cost: a state is never left, or its value diverges.",
                ));
            }
            value[node] = Some(v[i]);
        }
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(SolveError::NoConvergence);
        }

        let mut changed = false;
        for &node in &rows {
            let Some(current) = policy[node] else {
                continue;
            };
            let tol = SWITCH_EPS * value[node].unwrap_or(0.0).abs().max(1.0);
            let mut best = current;
            let mut best_q = edge_q(g, current, &value).unwrap_or(f64::INFINITY);
            for &ei in &adm[node] {
                if ei == current {
                    continue;
                }
                if let Some(q) = edge_q(g, ei, &value)
                    && q < best_q - tol
                {
                    best_q = q;
                    best = ei;
                }
            }
            if best != current {
                policy[node] = Some(best);
                changed = true;
            }
        }
        if !changed {
            break;
        }
        if let Some(node) = loop_node(g, &policy, &live) {
            return Err(SolveError::money_loop(&g.nodes[node].key));
        }
    }

    Ok(Solved {
        value,
        policy,
        iterations,
    })
}

/// Expected cost, spread, time and spend of the solved policy from `start`. `None` when `start` is
/// outside the almost-sure reachability set.
pub fn moments<S, A>(g: &Graph<S, A>, s: &Solved) -> Option<Moments> {
    let n = g.nodes.len();
    let mut row_of = vec![usize::MAX; n];
    let mut rows: Vec<usize> = Vec::new();
    for (v, chosen) in s.policy.iter().enumerate() {
        if chosen.is_some() {
            row_of[v] = rows.len();
            rows.push(v);
        }
    }
    let start_row = *row_of.get(START)?;
    if start_row == usize::MAX {
        return None;
    }
    let m = rows.len();
    let (a, c) = system(g, &rows, &row_of, &s.policy).ok()?;
    let secs: Vec<f64> = rows
        .iter()
        .map(|&v| s.policy[v].map_or(0.0, |ei| g.edges[ei].seconds))
        .collect();

    let m1 = solve_linear(a.clone(), c.clone());
    if m1.iter().any(|x| !x.is_finite()) {
        return None;
    }

    // E[C²] = c² + 2·c·E[C_next] + E[C_next²], i.e. m2 = N·(c² + 2c∘(Q m1)).
    let mut qm1 = vec![0.0; m];
    for (i, &v) in rows.iter().enumerate() {
        let ei = s.policy[v]?;
        for &(o, p) in &g.edges[ei].outcomes {
            if o == GOAL {
                continue;
            }
            let j = row_of[o];
            if j != usize::MAX {
                qm1[i] += p * m1[j];
            }
        }
    }
    let rhs: Vec<f64> = (0..m).map(|i| c[i] * c[i] + 2.0 * c[i] * qm1[i]).collect();
    let m2 = solve_linear(a.clone(), rhs);

    let expected_cost = m1[start_row];
    let sd_cost = if m2[start_row].is_finite() {
        (m2[start_row] - expected_cost * expected_cost)
            .max(0.0)
            .sqrt()
    } else {
        f64::NAN
    };
    let expected_seconds = solve_linear(a.clone(), secs)[start_row];

    // Expected visit counts: (I−Q)ᵀ x = e_start.
    let mut at = vec![vec![0.0; m]; m];
    for i in 0..m {
        for j in 0..m {
            at[j][i] = a[i][j];
        }
    }
    let mut e = vec![0.0; m];
    e[start_row] = 1.0;
    let visits = solve_linear(at, e);
    if visits.iter().any(|x| !x.is_finite()) {
        return None;
    }

    let mut expected_spend = std::collections::BTreeMap::new();
    let mut expected_buy_chaos = 0.0;
    let mut expected_sell_chaos = 0.0;
    for (i, &v) in rows.iter().enumerate() {
        let Some(ei) = s.policy[v] else {
            continue;
        };
        let edge = &g.edges[ei];
        let x = visits[i];
        match &edge.kind {
            EdgeKind::Craft { .. } => {
                for (key, qty) in &edge.line_items {
                    *expected_spend.entry(key.clone()).or_insert(0.0) += qty * x;
                }
            }
            EdgeKind::Buy => expected_buy_chaos += edge.cost * x,
            EdgeKind::Sell => expected_sell_chaos += (-edge.cost).max(0.0) * x,
        }
    }

    Some(Moments {
        expected_cost,
        sd_cost,
        expected_seconds,
        expected_spend,
        expected_buy_chaos,
        expected_sell_chaos,
    })
}

/// Moments of a policy given its transient block `q` (t×t), absorbing block `r` (t×a) and
/// per-transient cost `c`.
pub fn absorbing_stats(q: &[Vec<f64>], r: &[Vec<f64>], c: &[f64]) -> AbsorbingStats {
    let t = c.len();
    let mut a = vec![vec![0.0; t]; t];
    for (i, row) in a.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = f64::from(u8::from(i == j))
                - q.get(i).and_then(|v| v.get(j)).copied().unwrap_or(0.0);
        }
    }
    // N = (I − Q)⁻¹, column by column.
    let mut inv = vec![vec![0.0; t]; t];
    for k in 0..t {
        let mut e = vec![0.0; t];
        e[k] = 1.0;
        let col = solve_linear(a.clone(), e);
        for (i, slot) in inv.iter_mut().enumerate() {
            slot[k] = col[i];
        }
    }
    let mul = |x: &[f64]| -> Vec<f64> {
        (0..t)
            .map(|i| (0..t).map(|j| inv[i][j] * x[j]).sum())
            .collect()
    };
    let steps = mul(&vec![1.0; t]);
    let m1 = mul(c);
    let qm1: Vec<f64> = (0..t)
        .map(|i| {
            (0..t)
                .map(|j| q.get(i).and_then(|row| row.get(j)).copied().unwrap_or(0.0) * m1[j])
                .sum()
        })
        .collect();
    let m2 = mul(&(0..t)
        .map(|i| c[i] * c[i] + 2.0 * c[i] * qm1[i])
        .collect::<Vec<_>>());
    let sd = (0..t)
        .map(|i| (m2[i] - m1[i] * m1[i]).max(0.0).sqrt())
        .collect();
    let cols = r.first().map_or(0, Vec::len);
    let absorption = (0..t)
        .map(|i| {
            (0..cols)
                .map(|j| (0..t).map(|k| inv[i][k] * r[k][j]).sum())
                .collect()
        })
        .collect();
    AbsorbingStats {
        absorption,
        steps,
        m1,
        sd,
    }
}

/// `cost + Σ p·V(outcome)` of one edge; `None` when an outcome has no value yet.
fn edge_q<S, A>(g: &Graph<S, A>, ei: usize, value: &[Option<f64>]) -> Option<f64> {
    let e = &g.edges[ei];
    let mut q = e.cost;
    for &(o, p) in &e.outcomes {
        q += p * value.get(o).copied().flatten()?;
    }
    Some(q)
}

/// Reverse-BFS distances to the goal over admissible edges: `dist[goal] = 0`, and a node is
/// labelled `1 + d` as soon as one of its admissible edges has an outcome labelled `d`.
///
/// Taking the *minimum* outcome distance keeps the BFS finite on a cyclic craft — a reforge returns
/// to the state it started from, so ranking by the worst outcome would never settle — and every
/// node of R gets a finite distance, because that is exactly what membership of R means.
fn distances<S, A>(g: &Graph<S, A>, adm: &[Vec<usize>]) -> Vec<Option<u32>> {
    let n = g.nodes.len();
    let mut dist: Vec<Option<u32>> = vec![None; n];
    let mut rev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edges in adm {
        for &ei in edges {
            for &(o, _) in &g.edges[ei].outcomes {
                rev[o].push(ei);
            }
        }
    }
    dist[GOAL] = Some(0);
    let mut queue = VecDeque::from([GOAL]);
    while let Some(m) = queue.pop_front() {
        let d = dist[m].map_or(0, |d| d + 1);
        for &ei in &rev[m] {
            let from = g.edges[ei].from;
            if dist[from].is_none() {
                dist[from] = Some(d);
                queue.push_back(from);
            }
        }
    }
    dist
}

/// The starting policy: for each node of R, an admissible edge whose nearest outcome sits one step
/// closer to the goal (ties by cost, then by creation order).
///
/// This is proper by construction: every node steps to a strictly smaller distance with positive
/// probability, so the goal is reachable from every node of R, the chain is absorbing, and no
/// improvement round can start from a divergent strategy.
fn initial_policy<S, A>(
    g: &Graph<S, A>,
    adm: &[Vec<usize>],
    dist: &[Option<u32>],
    rows: &[usize],
) -> Result<Vec<Option<usize>>, SolveError> {
    let mut policy: Vec<Option<usize>> = vec![None; g.nodes.len()];
    for &v in rows {
        let aimed_at = dist[v].map(|d| d.saturating_sub(1));
        let mut best: Option<(f64, usize)> = None;
        for &ei in &adm[v] {
            let nearest = g.edges[ei]
                .outcomes
                .iter()
                .filter_map(|(o, _)| dist[*o])
                .min();
            if nearest != aimed_at {
                continue;
            }
            let cost = g.edges[ei].cost;
            best = match best {
                None => Some((cost, ei)),
                Some((cheapest, _)) if cost < cheapest => Some((cost, ei)),
                kept => kept,
            };
        }
        policy[v] = best.map(|(_, ei)| ei).or_else(|| adm[v].first().copied());
        if policy[v].is_none() {
            return Err(SolveError::diverged(
                &g.nodes[v].key,
                "No action from this state can reach the goal.",
            ));
        }
    }
    Ok(policy)
}

/// The greatest set of nodes from which the goal is reached with probability one.
fn almost_sure_set<S, A>(g: &Graph<S, A>) -> Vec<bool> {
    let n = g.nodes.len();
    let mut live = vec![true; n];
    if n <= GOAL {
        return live;
    }
    let mut rev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (ei, e) in g.edges.iter().enumerate() {
        for &(o, _) in &e.outcomes {
            rev[o].push(ei);
        }
    }
    loop {
        // Goal reachability inside the live set, over edges whose outcomes all stay live.
        let mut can = vec![false; n];
        can[GOAL] = true;
        let mut stack = vec![GOAL];
        while let Some(o) = stack.pop() {
            for &ei in &rev[o] {
                let e = &g.edges[ei];
                if e.outcomes.is_empty() || !e.outcomes.iter().all(|(x, _)| live[*x]) {
                    continue;
                }
                if !can[e.from] {
                    can[e.from] = true;
                    stack.push(e.from);
                }
            }
        }
        let pruned: Vec<usize> = (0..n).filter(|&i| live[i] && !can[i]).collect();
        if pruned.is_empty() {
            return live;
        }
        for i in pruned {
            live[i] = false;
        }
    }
}

/// Admissible edge indices per node: every outcome stays inside the live set.
fn admissible<S, A>(g: &Graph<S, A>, live: &[bool]) -> Vec<Vec<usize>> {
    let mut adm = vec![Vec::new(); g.nodes.len()];
    for (ei, e) in g.edges.iter().enumerate() {
        if live[e.from] && !e.outcomes.is_empty() && e.outcomes.iter().all(|(o, _)| live[*o]) {
            adm[e.from].push(ei);
        }
    }
    adm
}

/// The row system `(I − Q)·V = c` of the current policy. The diagonal is the outgoing mass, never
/// `1 − Q_ii`, so a near-certain self-loop cannot lose precision.
fn system<S, A>(
    g: &Graph<S, A>,
    rows: &[usize],
    row_of: &[usize],
    policy: &[Option<usize>],
) -> Result<(Vec<Vec<f64>>, Vec<f64>), SolveError> {
    let m = rows.len();
    let mut a = vec![vec![0.0; m]; m];
    let mut c = vec![0.0; m];
    for (i, &v) in rows.iter().enumerate() {
        let Some(ei) = policy[v] else {
            return Err(SolveError::diverged(
                &g.nodes[v].key,
                "No action from this state can reach the goal.",
            ));
        };
        let e = &g.edges[ei];
        c[i] = e.cost;
        let mut outgoing = 0.0;
        for &(o, p) in &e.outcomes {
            if o == v {
                continue;
            }
            outgoing += p;
            if o != GOAL {
                let j = row_of[o];
                if j != usize::MAX {
                    a[i][j] -= p;
                }
            }
        }
        a[i][i] = outgoing;
    }
    Ok((a, c))
}

/// A node on a policy loop, if the policy no longer reaches the goal from every live node.
fn loop_node<S, A>(g: &Graph<S, A>, policy: &[Option<usize>], live: &[bool]) -> Option<usize> {
    let n = g.nodes.len();
    let mut rev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (v, chosen) in policy.iter().enumerate() {
        let Some(ei) = chosen else {
            continue;
        };
        for &(o, _) in &g.edges[*ei].outcomes {
            rev[o].push(v);
        }
    }
    let mut can = vec![false; n];
    can[GOAL] = true;
    let mut stack = vec![GOAL];
    while let Some(o) = stack.pop() {
        for &from in &rev[o] {
            if !can[from] {
                can[from] = true;
                stack.push(from);
            }
        }
    }
    let stuck = (0..n).find(|&v| live[v] && !can[v])?;
    // The stuck region is closed under the policy's outcomes, so walking it must repeat.
    let mut seen: Vec<usize> = Vec::new();
    let mut cur = stuck;
    loop {
        if let Some(pos) = seen.iter().position(|&x| x == cur) {
            let cycle = &seen[pos..];
            return Some(
                cycle
                    .iter()
                    .copied()
                    .filter(|&v| v != START && v != GOAL)
                    .min()
                    .unwrap_or_else(|| *cycle.iter().min().unwrap_or(&cur)),
            );
        }
        seen.push(cur);
        let Some(ei) = policy[cur] else {
            return Some(cur);
        };
        let next = g.edges[ei]
            .outcomes
            .iter()
            .map(|(o, _)| *o)
            .filter(|&o| o != GOAL && !can[o])
            .min();
        match next {
            Some(v) => cur = v,
            None => return Some(cur),
        }
    }
}

/// Dense Gaussian elimination with partial pivoting. A singular system yields non-finite entries,
/// which the callers turn into [`SolveError::Diverged`].
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Vec<f64> {
    let n = b.len();
    for col in 0..n {
        let mut piv = col;
        for r in (col + 1)..n {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if piv != col {
            a.swap(col, piv);
            b.swap(col, piv);
        }
        let d = a[col][col];
        if d == 0.0 {
            continue;
        }
        for r in (col + 1)..n {
            let f = a[r][col] / d;
            if f == 0.0 {
                continue;
            }
            let (head, tail) = a.split_at_mut(r);
            let pivot = &head[col];
            for (k, cell) in tail[0].iter_mut().enumerate().skip(col) {
                *cell -= f * pivot[k];
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut acc = b[i];
        for j in (i + 1)..n {
            acc -= a[i][j] * x[j];
        }
        x[i] = if a[i][i] == 0.0 {
            f64::NAN
        } else {
            acc / a[i][i]
        };
    }
    x
}
