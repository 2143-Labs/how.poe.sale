//! The interface a game ruleset implements so the solver can build and solve its state graph.

/// A crafting game: states, the actions applicable to them, and each action's outcome distribution.
pub trait Domain {
    type State: Clone + Eq + std::hash::Hash + Ord;
    type Action: Clone + Eq + Ord + serde::Serialize;

    /// Stable, unique key of a state. `"start"` and `"goal"` are reserved for the graph's own nodes.
    fn key(&self, s: &Self::State) -> String;
    fn is_goal(&self, s: &Self::State) -> bool;
    /// Every action applicable to `s`, in a stable order.
    fn actions(&self, s: &Self::State) -> Vec<Self::Action>;
    /// Outcome distribution of `a` applied to `s`; probabilities sum to 1, identical states merged.
    fn outcomes(&self, s: &Self::State, a: &Self::Action) -> Vec<(Self::State, f64)>;
    /// Line items consumed by one use of `a`: (price key, quantity).
    fn cost(&self, s: &Self::State, a: &Self::Action) -> Vec<(String, f64)>;
    fn seconds(&self, s: &Self::State, a: &Self::Action) -> f64;
    /// Candidate (action, previous state) pairs that can produce `s`; used only to propose new layers.
    fn predecessors(&self, s: &Self::State) -> Vec<(Self::Action, Self::State)>;
    fn label(&self, a: &Self::Action) -> String;
    /// `s` with every rolled-value flag normalised, so states that differ only in how well their
    /// mods rolled compare equal. `Proposal::p_mods` sums outcomes equal to the node under this map.
    fn ignore_values(&self, s: &Self::State) -> Self::State {
        s.clone()
    }
}
