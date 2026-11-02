//! Backward proposals: the actions that could produce a node, ranked by what they cost given the
//! rest of the solved plan.

use crate::domain::Domain;
use crate::error::SolveError;
use crate::graph::{Graph, MIN_COST, START, price};
use crate::mdp::Solved;

/// Whether a proposal can be used as-is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProposalStatus {
    /// Everything it needs is priced or solved.
    Ready,
    /// A reachable outcome has no value yet: expand it or give it a buy price.
    NeedsInput,
    /// The user switched this action off.
    Disabled,
}

/// The ranked proposals for one node.
pub type Proposals<A, S> = Vec<Proposal<A, S>>;

/// One way to reach a node from a previous state.
#[derive(Clone, Debug)]
pub struct Proposal<A, S> {
    pub action: A,
    /// `serde_json::to_string(&action)`: the string the `disabled` pairs use.
    pub action_key: String,
    pub label: String,
    /// Key of the state the action starts from.
    pub pre: String,
    pub pre_state: S,
    /// Probability of landing exactly on the node (rolled values included).
    pub p_reach: f64,
    /// Probability of landing on the node ignoring every rolled value.
    pub p_mods: f64,
    /// Chaos for one use of the action.
    pub cost: f64,
    /// `cost + Σ p·V'(outcome)`; `None` when some outcome has no value.
    pub q: Option<f64>,
    pub status: ProposalStatus,
}

/// Every predecessor action of `node_key`/`node_state`, best first.
pub fn propose<D>(
    d: &D,
    g: &Graph<D::State, D::Action>,
    s: &Solved,
    node_key: &str,
    node_state: &D::State,
) -> Result<Proposals<D::Action, D::State>, SolveError>
where
    D: Domain,
{
    let at_goal = node_key == "goal";
    let start_value = s.value.get(START).copied().flatten();
    let node_ignored = d.ignore_values(node_state);
    let mut out: Vec<Proposal<D::Action, D::State>> = Vec::new();

    for (action, pre) in d.predecessors(node_state) {
        let pre_key = d.key(&pre);
        let action_key = serde_json::to_string(&action)
            .map_err(|e| SolveError::diverged(&pre_key, e.to_string()))?;
        let cost = price(&d.cost(&pre, &action), &g.prices)?.max(MIN_COST);

        let mut p_reach = 0.0;
        let mut p_mods = 0.0;
        let mut q = Some(cost);
        for (o, p) in d.outcomes(&pre, &action) {
            let is_goal = d.is_goal(&o);
            if at_goal {
                if is_goal {
                    p_reach += p;
                    p_mods += p;
                } else if d.ignore_values(&o) == node_ignored {
                    p_mods += p;
                }
            } else {
                if o == *node_state {
                    p_reach += p;
                }
                if d.ignore_values(&o) == node_ignored {
                    p_mods += p;
                }
            }

            // V'(o): the solved value when the outcome is part of the plan, otherwise what the
            // outcome is worth as a finished item — a fresh start minus what selling it recovers.
            let key = d.key(&o);
            let known = g.node(&key).and_then(|i| s.value[i]);
            let value = match known {
                Some(v) => Some(v),
                None => start_value.map(|sv| sv - g.effective_sell(&key)),
            };
            match value {
                Some(v) => {
                    if let Some(acc) = q.as_mut() {
                        *acc += p * v;
                    }
                }
                None => q = None,
            }
        }

        let status = if g.disabled.contains(&(pre_key.clone(), action_key.clone())) {
            ProposalStatus::Disabled
        } else if q.is_some() {
            ProposalStatus::Ready
        } else {
            ProposalStatus::NeedsInput
        };
        out.push(Proposal {
            label: d.label(&action),
            action,
            action_key,
            pre: pre_key,
            pre_state: pre,
            p_reach,
            p_mods,
            cost,
            q,
            status,
        });
    }

    out.sort_by(|a, b| {
        rank(a.status)
            .cmp(&rank(b.status))
            .then_with(|| match a.status {
                ProposalStatus::Ready => {
                    a.q.unwrap_or(f64::INFINITY)
                        .total_cmp(&b.q.unwrap_or(f64::INFINITY))
                }
                _ => a.cost.total_cmp(&b.cost),
            })
            .then_with(|| a.action_key.cmp(&b.action_key))
    });
    Ok(out)
}

/// Sort rank: Ready, then NeedsInput, then Disabled.
fn rank(status: ProposalStatus) -> u8 {
    match status {
        ProposalStatus::Ready => 0,
        ProposalStatus::NeedsInput => 1,
        ProposalStatus::Disabled => 2,
    }
}
