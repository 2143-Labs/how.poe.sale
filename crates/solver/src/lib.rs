//! Game-agnostic crafting solver: state graph, policy iteration, statistics and proposals.

pub mod domain;
pub mod error;
pub mod graph;
pub mod mdp;
pub mod propose;
pub mod stats;

pub use domain::Domain;
pub use error::SolveError;
pub use graph::{GOAL, Graph, GraphInput, START};
pub use mdp::{AbsorbingStats, Moments, Solved, absorbing_stats, moments, solve};
pub use propose::{Proposal, ProposalStatus, propose};
pub use stats::{Q4, Quantiles, quantiles};
