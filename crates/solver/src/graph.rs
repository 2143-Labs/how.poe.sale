//! The expanded state graph: interned nodes, one craft edge per enabled action of every expanded
//! state, and the permanent Buy/Sell edges that keep every node connected to `start`.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use crate::domain::Domain;
use crate::error::SolveError;

/// Node index of the abstract `start` node: the state "nothing bought, nothing owned yet".
pub const START: usize = 0;
/// Node index of the `goal` node: the target item, reached at zero cost.
pub const GOAL: usize = 1;
/// Largest graph the solver will build; above this the dense evaluation is not viable.
pub const MAX_NODES: usize = 5000;
/// Floor on a craft edge's cost, so a free action cannot create a zero-cost absorbing loop.
pub const MIN_COST: f64 = 1e-6;
/// Seconds a Buy edge costs (a trade).
pub const BUY_SECONDS: f64 = 60.0;

/// Everything the graph builder needs from the problem.
#[derive(Clone, Debug)]
pub struct GraphInput<S> {
    /// States to intern first, in order. They are the anchors the user's plan starts from.
    pub seeds: Vec<S>,
    /// Keys of the states whose actions should be expanded into craft edges.
    pub expanded: BTreeSet<String>,
    /// `(state key, action key)` pairs the user has switched off.
    pub disabled: BTreeSet<(String, String)>,
    /// Price per line-item key, in chaos.
    pub prices: BTreeMap<String, f64>,
    /// Buy price per state key, in chaos.
    pub buy: BTreeMap<String, f64>,
    /// Sell price per state key, in chaos.
    pub sell: BTreeMap<String, f64>,
    /// Extra chaos charged on every trade, so an indifferent loop still converges.
    pub trade_penalty: f64,
}

/// One vertex of the graph. `start` and `goal` carry no state.
#[derive(Clone, Debug)]
pub struct Node<S> {
    pub key: String,
    pub state: Option<S>,
    pub expanded: bool,
}

/// What produced an edge.
#[derive(Clone, Debug)]
pub enum EdgeKind<A> {
    Craft { action: A, action_key: String },
    Buy,
    Sell,
}

/// A directed, priced, probabilistic edge.
#[derive(Clone, Debug)]
pub struct Edge<A> {
    pub from: usize,
    pub kind: EdgeKind<A>,
    pub label: String,
    /// Chaos per use: the summed line items (floored at [`MIN_COST`]) for crafts, `buy + penalty`
    /// for a buy, `-min(sell, buy) + penalty` for a sell.
    pub cost: f64,
    pub seconds: f64,
    /// Craft only: `Domain::cost` verbatim, for expected-spend reporting.
    pub line_items: Vec<(String, f64)>,
    /// `(node index, probability)`; goal outcomes point at [`GOAL`].
    pub outcomes: Vec<(usize, f64)>,
}

/// The solved problem's shape, retained so proposals can price and filter without the input.
#[derive(Clone, Debug)]
pub struct Graph<S, A> {
    pub nodes: Vec<Node<S>>,
    pub edges: Vec<Edge<A>>,
    /// Edge indices leaving each node, in creation order.
    pub out: Vec<Vec<usize>>,
    pub prices: BTreeMap<String, f64>,
    pub buy: BTreeMap<String, f64>,
    pub sell: BTreeMap<String, f64>,
    pub disabled: BTreeSet<(String, String)>,
    pub trade_penalty: f64,
    index: HashMap<String, usize>,
}

impl<S: Clone, A: Clone> Graph<S, A> {
    /// Intern the seeds, expand every state listed in `input.expanded`, then add the Sell and Buy edges.
    pub fn build<D>(d: &D, input: &GraphInput<S>) -> Result<Self, SolveError>
    where
        D: Domain<State = S, Action = A>,
        A: serde::Serialize,
    {
        let mut nodes: Vec<Node<S>> = vec![
            Node {
                key: "start".to_owned(),
                state: None,
                expanded: false,
            },
            Node {
                key: "goal".to_owned(),
                state: None,
                expanded: false,
            },
        ];
        let mut index: HashMap<String, usize> =
            HashMap::from([("start".to_owned(), START), ("goal".to_owned(), GOAL)]);
        let mut edges: Vec<Edge<A>> = Vec::new();

        for seed in &input.seeds {
            if d.is_goal(seed) {
                continue;
            }
            intern(
                &mut nodes,
                &mut index,
                &input.expanded,
                d.key(seed),
                seed.clone(),
            )?;
        }

        let mut queue: VecDeque<usize> = (2..nodes.len()).collect();
        while let Some(ni) = queue.pop_front() {
            if !nodes[ni].expanded {
                continue;
            }
            let Some(state) = nodes[ni].state.clone() else {
                continue;
            };
            let state_key = nodes[ni].key.clone();
            let actions = d.actions(&state);
            for a in actions {
                let action_key = serde_json::to_string(&a)
                    .map_err(|e| SolveError::diverged(&state_key, e.to_string()))?;
                if input
                    .disabled
                    .contains(&(state_key.clone(), action_key.clone()))
                {
                    continue;
                }
                let line_items = d.cost(&state, &a);
                let cost = price(&line_items, &input.prices)?.max(MIN_COST);
                let before = nodes.len();
                let outcomes = resolve(d, &mut nodes, &mut index, &input.expanded, &state, &a)?;
                if outcomes.is_empty() {
                    continue;
                }
                queue.extend(before..nodes.len());
                edges.push(Edge {
                    label: d.label(&a),
                    seconds: d.seconds(&state, &a),
                    from: ni,
                    kind: EdgeKind::Craft {
                        action: a,
                        action_key,
                    },
                    cost,
                    line_items,
                    outcomes,
                });
            }
        }

        // Every state node can be sold, which is what makes a dead end a recovery rather than a loss.
        for (ni, node) in nodes.iter().enumerate().skip(2) {
            let key = node.key.clone();
            let sell = input.sell.get(&key).copied().unwrap_or(0.0);
            let buy = input.buy.get(&key).copied().unwrap_or(f64::INFINITY);
            edges.push(Edge {
                from: ni,
                kind: EdgeKind::Sell,
                label: "Sell".to_owned(),
                cost: -sell.min(buy) + input.trade_penalty,
                seconds: 0.0,
                line_items: Vec::new(),
                outcomes: vec![(START, 1.0)],
            });
        }

        // `start` reaches the graph only by buying into it.
        for (key, price) in &input.buy {
            let Some(&target) = index.get(key) else {
                continue;
            };
            if target == START {
                continue;
            }
            edges.push(Edge {
                from: START,
                kind: EdgeKind::Buy,
                label: "Buy".to_owned(),
                cost: price + input.trade_penalty,
                seconds: BUY_SECONDS,
                line_items: Vec::new(),
                outcomes: vec![(target, 1.0)],
            });
        }

        let mut out: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
        for (ei, e) in edges.iter().enumerate() {
            out[e.from].push(ei);
        }

        Ok(Graph {
            nodes,
            edges,
            out,
            prices: input.prices.clone(),
            buy: input.buy.clone(),
            sell: input.sell.clone(),
            disabled: input.disabled.clone(),
            trade_penalty: input.trade_penalty,
            index,
        })
    }

    /// Node index of a state or of `"start"`/`"goal"`.
    pub fn node(&self, key: &str) -> Option<usize> {
        self.index.get(key).copied()
    }

    /// The Sell price the graph charges for `key`: the clamped `min(sell, buy)`.
    pub fn effective_sell(&self, key: &str) -> f64 {
        let sell = self.sell.get(key).copied().unwrap_or(0.0);
        let buy = self.buy.get(key).copied().unwrap_or(f64::INFINITY);
        sell.min(buy)
    }
}

/// Intern `state` under `key`, appending it to `nodes`; the reserved keys fall back to their node.
fn intern<S>(
    nodes: &mut Vec<Node<S>>,
    index: &mut HashMap<String, usize>,
    expanded: &BTreeSet<String>,
    key: String,
    state: S,
) -> Result<usize, SolveError> {
    if let Some(&i) = index.get(&key) {
        // "start"/"goal" are reserved; a domain must never key a real state with them.
        debug_assert!(i >= 2, "domain produced the reserved state key {key:?}");
        return Ok(i);
    }
    let i = nodes.len();
    nodes.push(Node {
        expanded: expanded.contains(&key),
        key: key.clone(),
        state: Some(state),
    });
    index.insert(key, i);
    if nodes.len() > MAX_NODES {
        return Err(SolveError::TooLarge(nodes.len()));
    }
    Ok(i)
}

/// The outcomes of `a` on `state`, as `(node index, probability)` with duplicates merged.
fn resolve<S, A, D>(
    d: &D,
    nodes: &mut Vec<Node<S>>,
    index: &mut HashMap<String, usize>,
    expanded: &BTreeSet<String>,
    state: &S,
    a: &A,
) -> Result<Vec<(usize, f64)>, SolveError>
where
    D: Domain<State = S, Action = A>,
{
    let mut out: Vec<(usize, f64)> = Vec::new();
    for (o, p) in d.outcomes(state, a) {
        if p <= 0.0 || p.is_nan() {
            continue;
        }
        let idx = if d.is_goal(&o) {
            GOAL
        } else {
            intern(nodes, index, expanded, d.key(&o), o)?
        };
        match out.iter_mut().find(|(j, _)| *j == idx) {
            Some(slot) => slot.1 += p,
            None => out.push((idx, p)),
        }
    }
    Ok(out)
}

/// Chaos cost of a set of line items; a key with no price is a hard error.
pub(crate) fn price(
    line_items: &[(String, f64)],
    prices: &BTreeMap<String, f64>,
) -> Result<f64, SolveError> {
    let mut cost = 0.0;
    for (key, qty) in line_items {
        let unit = prices
            .get(key)
            .ok_or_else(|| SolveError::MissingPrice(key.clone()))?;
        cost += qty * unit;
    }
    Ok(cost)
}
