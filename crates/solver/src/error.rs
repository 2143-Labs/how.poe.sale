//! Errors a graph build or solve can produce. Every variant is user-facing: `poe1::api`
//! forwards `kind()` and the `Display` text straight into the `Solution` JSON.

/// Something that makes the craft plan unsolvable as stated.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SolveError {
    /// A line item has no price in the problem.
    #[error("No price is set for \"{0}\".")]
    MissingPrice(String),
    /// The expanded graph exceeded [`crate::graph::MAX_NODES`].
    #[error("This plan needs {0} nodes, more than the solver handles; expand fewer states.")]
    TooLarge(usize),
    /// Policy iteration ran out of improvement rounds.
    #[error("The strategy did not settle after many rounds; check the buy and sell prices.")]
    NoConvergence,
    /// A simulated run of the policy never reached the goal.
    #[error("A simulated run never finished; the strategy contains a loop.")]
    NoTermination,
    /// The strategy has no finite cost: a node's value diverges.
    #[error("{message}")]
    Diverged { node: String, message: String },
}

impl SolveError {
    /// Stable machine-readable discriminant, used as `Solution.error.kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            SolveError::MissingPrice(_) => "MissingPrice",
            SolveError::TooLarge(_) => "TooLarge",
            SolveError::NoConvergence => "NoConvergence",
            SolveError::NoTermination => "NoTermination",
            SolveError::Diverged { .. } => "Diverged",
        }
    }

    /// A divergence whose cost is a money loop through `node_key`.
    pub(crate) fn money_loop(node_key: &str) -> Self {
        SolveError::Diverged {
            node: node_key.to_owned(),
            message: format!(
                "Selling {node_key} for more than it costs to make creates a money loop; lower its sell price."
            ),
        }
    }

    /// A divergence with a caller-chosen explanation.
    pub(crate) fn diverged(node_key: &str, message: impl Into<String>) -> Self {
        SolveError::Diverged {
            node: node_key.to_owned(),
            message: message.into(),
        }
    }
}
