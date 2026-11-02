//! Hand-built domains the solver's own tests need. They are deliberately tiny: the point is the
//! graph/policy machinery, not the game rules.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use solver::{Domain, Graph, GraphInput};

/// `(key, value)` pairs as a price map.
pub fn map(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

/// A graph input with no prices and no trades; the caller fills in what it needs.
pub fn input<S>(seeds: Vec<S>, expanded: &[&str]) -> GraphInput<S> {
    GraphInput {
        seeds,
        expanded: expanded.iter().map(|s| (*s).to_owned()).collect(),
        disabled: BTreeSet::new(),
        prices: BTreeMap::new(),
        buy: BTreeMap::new(),
        sell: BTreeMap::new(),
        trade_penalty: 0.0,
    }
}

/// The transient block `q`, absorbing block `r` and costs `c` of the slam/annul/recraft chain:
/// slam → 0.9 annul / 0.1 goal; annul → ⅓ slam / ⅓ recraft / ⅓ brick; recraft → annul.
pub fn absorbing_stats_inputs() -> (Vec<Vec<f64>>, Vec<Vec<f64>>, Vec<f64>) {
    (
        vec![
            vec![0.0, 0.9, 0.0],
            vec![1.0 / 3.0, 0.0, 1.0 / 3.0],
            vec![0.0, 1.0, 0.0],
        ],
        vec![vec![0.1, 0.0], vec![0.0, 1.0 / 3.0], vec![0.0, 0.0]],
        vec![1.0, 0.5, 2.0],
    )
}

// ---------------------------------------------------------------------------------------------
// Slam → Annul → (slam | recraft | brick), with a 10% goal hit on the slam.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SState {
    Slam,
    Annul,
    Recraft,
    Brick,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SAction {
    Slam,
    Annul,
    Recraft,
}

pub struct SlamDomain;

impl Domain for SlamDomain {
    type State = SState;
    type Action = SAction;

    fn key(&self, s: &SState) -> String {
        match s {
            SState::Slam => "slam",
            SState::Annul => "annul",
            SState::Recraft => "recraft",
            SState::Brick => "brick",
            SState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &SState) -> bool {
        *s == SState::Goal
    }

    fn actions(&self, s: &SState) -> Vec<SAction> {
        match s {
            SState::Slam => vec![SAction::Slam],
            SState::Annul => vec![SAction::Annul],
            SState::Recraft => vec![SAction::Recraft],
            _ => Vec::new(),
        }
    }

    fn outcomes(&self, s: &SState, a: &SAction) -> Vec<(SState, f64)> {
        match (s, a) {
            (SState::Slam, SAction::Slam) => vec![(SState::Annul, 0.9), (SState::Goal, 0.1)],
            (SState::Annul, SAction::Annul) => vec![
                (SState::Slam, 1.0 / 3.0),
                (SState::Recraft, 1.0 / 3.0),
                (SState::Brick, 1.0 / 3.0),
            ],
            (SState::Recraft, SAction::Recraft) => vec![(SState::Annul, 1.0)],
            _ => Vec::new(),
        }
    }

    fn cost(&self, s: &SState, a: &SAction) -> Vec<(String, f64)> {
        let key = match (s, a) {
            (SState::Slam, _) => "slam",
            (SState::Annul, _) => "annul",
            (SState::Recraft, _) => "recraft",
            _ => return Vec::new(),
        };
        vec![(key.to_owned(), 1.0)]
    }

    fn seconds(&self, _s: &SState, _a: &SAction) -> f64 {
        1.0
    }

    fn predecessors(&self, s: &SState) -> Vec<(SAction, SState)> {
        match s {
            SState::Annul => vec![(SAction::Slam, SState::Slam)],
            SState::Recraft => vec![(SAction::Annul, SState::Annul)],
            SState::Brick => vec![(SAction::Annul, SState::Annul)],
            _ => Vec::new(),
        }
    }

    fn label(&self, a: &SAction) -> String {
        match a {
            SAction::Slam => "Slam",
            SAction::Annul => "Annul",
            SAction::Recraft => "Recraft",
        }
        .to_owned()
    }
}

/// The whole chain, expanded, with the caller's prices and trades.
pub fn slam_graph(
    prices: &[(&str, f64)],
    buy: &[(&str, f64)],
    sell: &[(&str, f64)],
    trade_penalty: f64,
) -> Graph<SState, SAction> {
    let mut gi = input(
        vec![SState::Slam, SState::Annul, SState::Recraft, SState::Brick],
        &["slam", "annul", "recraft", "brick"],
    );
    gi.prices = map(prices);
    gi.buy = map(buy);
    gi.sell = map(sell);
    gi.trade_penalty = trade_penalty;
    Graph::build(&SlamDomain, &gi).expect("chain graph builds")
}

// ---------------------------------------------------------------------------------------------
// P -Craft-> {N(ok), N(bad), Far}, N(ok) -Finish-> Goal. Gives p_reach ≠ p_mods and an outcome
// that is not part of the graph at all.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PState {
    P,
    N { ok: bool },
    Far,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum PAction {
    Craft,
    Finish,
}

pub struct PropDomain;

impl Domain for PropDomain {
    type State = PState;
    type Action = PAction;

    fn key(&self, s: &PState) -> String {
        match s {
            PState::P => "p",
            PState::N { ok: true } => "n_ok",
            PState::N { ok: false } => "n_low",
            PState::Far => "far",
            PState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &PState) -> bool {
        *s == PState::Goal
    }

    fn actions(&self, s: &PState) -> Vec<PAction> {
        match s {
            PState::P => vec![PAction::Craft],
            PState::N { ok: true } => vec![PAction::Finish],
            _ => Vec::new(),
        }
    }

    fn outcomes(&self, s: &PState, a: &PAction) -> Vec<(PState, f64)> {
        match (s, a) {
            (PState::P, PAction::Craft) => vec![
                (PState::N { ok: true }, 0.5),
                (PState::N { ok: false }, 0.25),
                (PState::Far, 0.25),
            ],
            (PState::N { ok: true }, PAction::Finish) => vec![(PState::Goal, 1.0)],
            _ => Vec::new(),
        }
    }

    fn cost(&self, s: &PState, a: &PAction) -> Vec<(String, f64)> {
        match (s, a) {
            (PState::P, PAction::Craft) => vec![("craft".to_owned(), 1.0)],
            (PState::N { ok: true }, PAction::Finish) => vec![("finish".to_owned(), 1.0)],
            _ => Vec::new(),
        }
    }

    fn seconds(&self, s: &PState, a: &PAction) -> f64 {
        match (s, a) {
            (PState::P, PAction::Craft) => 3.0,
            _ => 2.0,
        }
    }

    fn predecessors(&self, s: &PState) -> Vec<(PAction, PState)> {
        match s {
            PState::N { ok: true } => vec![(PAction::Craft, PState::P)],
            PState::Goal => vec![
                (PAction::Finish, PState::N { ok: true }),
                (PAction::Craft, PState::P),
            ],
            _ => Vec::new(),
        }
    }

    fn label(&self, a: &PAction) -> String {
        match a {
            PAction::Craft => "Craft",
            PAction::Finish => "Finish",
        }
        .to_owned()
    }

    fn ignore_values(&self, s: &PState) -> PState {
        match s {
            PState::N { .. } => PState::N { ok: true },
            other => other.clone(),
        }
    }
}

/// `n_ok` expanded; trades and penalty from the caller.
pub fn prop_graph(
    prices: &[(&str, f64)],
    buy: &[(&str, f64)],
    sell: &[(&str, f64)],
) -> Graph<PState, PAction> {
    let mut gi = input(vec![PState::N { ok: true }], &["n_ok"]);
    gi.prices = map(prices);
    gi.buy = map(buy);
    gi.sell = map(sell);
    Graph::build(&PropDomain, &gi).expect("prop graph builds")
}

// ---------------------------------------------------------------------------------------------
// A single self-looping state, for the zero-cost floor.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SpinState {
    Spin,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SpinAction {
    Spin,
}

pub struct SpinDomain;

impl Domain for SpinDomain {
    type State = SpinState;
    type Action = SpinAction;

    fn key(&self, s: &SpinState) -> String {
        match s {
            SpinState::Spin => "spin",
            SpinState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &SpinState) -> bool {
        *s == SpinState::Goal
    }

    fn actions(&self, s: &SpinState) -> Vec<SpinAction> {
        if *s == SpinState::Spin {
            vec![SpinAction::Spin]
        } else {
            Vec::new()
        }
    }

    fn outcomes(&self, s: &SpinState, a: &SpinAction) -> Vec<(SpinState, f64)> {
        match (s, a) {
            (SpinState::Spin, SpinAction::Spin) => {
                vec![(SpinState::Spin, 0.5), (SpinState::Goal, 0.5)]
            }
            _ => Vec::new(),
        }
    }

    fn cost(&self, _s: &SpinState, _a: &SpinAction) -> Vec<(String, f64)> {
        vec![("zero".to_owned(), 1.0)]
    }

    fn seconds(&self, _s: &SpinState, _a: &SpinAction) -> f64 {
        1.0
    }

    fn predecessors(&self, _s: &SpinState) -> Vec<(SpinAction, SpinState)> {
        Vec::new()
    }

    fn label(&self, _a: &SpinAction) -> String {
        "Spin".to_owned()
    }
}

// ---------------------------------------------------------------------------------------------
// Item -Idle-> Goal, plus trades: the cheapest graph for edge-level assertions.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum TState {
    Item,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum TAction {
    Idle,
}

pub struct TradeDomain;

impl Domain for TradeDomain {
    type State = TState;
    type Action = TAction;

    fn key(&self, s: &TState) -> String {
        match s {
            TState::Item => "item",
            TState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &TState) -> bool {
        *s == TState::Goal
    }

    fn actions(&self, s: &TState) -> Vec<TAction> {
        if *s == TState::Item {
            vec![TAction::Idle]
        } else {
            Vec::new()
        }
    }

    fn outcomes(&self, s: &TState, a: &TAction) -> Vec<(TState, f64)> {
        match (s, a) {
            (TState::Item, TAction::Idle) => vec![(TState::Goal, 1.0)],
            _ => Vec::new(),
        }
    }

    fn cost(&self, _s: &TState, _a: &TAction) -> Vec<(String, f64)> {
        vec![("x".to_owned(), 1.0)]
    }

    fn seconds(&self, _s: &TState, _a: &TAction) -> f64 {
        1.0
    }

    fn predecessors(&self, _s: &TState) -> Vec<(TAction, TState)> {
        Vec::new()
    }

    fn label(&self, _a: &TAction) -> String {
        "Idle".to_owned()
    }
}

// ---------------------------------------------------------------------------------------------
// Top -Make(1)-> Deep -Push(200)-> Goal, with Deep sellable for 100 and Top buyable for 10:
// selling Deep beats finishing it, so the improved policy loops through `start`.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum LState {
    Top,
    Deep,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum LAction {
    Make,
    Push,
}

pub struct LoopDomain;

impl Domain for LoopDomain {
    type State = LState;
    type Action = LAction;

    fn key(&self, s: &LState) -> String {
        match s {
            LState::Top => "top",
            LState::Deep => "deep",
            LState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &LState) -> bool {
        *s == LState::Goal
    }

    fn actions(&self, s: &LState) -> Vec<LAction> {
        match s {
            LState::Top => vec![LAction::Make],
            LState::Deep => vec![LAction::Push],
            LState::Goal => Vec::new(),
        }
    }

    fn outcomes(&self, s: &LState, a: &LAction) -> Vec<(LState, f64)> {
        match (s, a) {
            (LState::Top, LAction::Make) => vec![(LState::Deep, 1.0)],
            (LState::Deep, LAction::Push) => vec![(LState::Goal, 1.0)],
            _ => Vec::new(),
        }
    }

    fn cost(&self, s: &LState, a: &LAction) -> Vec<(String, f64)> {
        match (s, a) {
            (LState::Top, LAction::Make) => vec![("make".to_owned(), 1.0)],
            (LState::Deep, LAction::Push) => vec![("push".to_owned(), 1.0)],
            _ => Vec::new(),
        }
    }

    fn seconds(&self, _s: &LState, _a: &LAction) -> f64 {
        1.0
    }

    fn predecessors(&self, _s: &LState) -> Vec<(LAction, LState)> {
        Vec::new()
    }

    fn label(&self, a: &LAction) -> String {
        match a {
            LAction::Make => "Make",
            LAction::Push => "Push",
        }
        .to_owned()
    }
}

pub fn loop_graph() -> Graph<LState, LAction> {
    let mut gi = input(vec![LState::Top, LState::Deep], &["top", "deep"]);
    gi.prices = map(&[("make", 1.0), ("push", 200.0)]);
    gi.sell = map(&[("deep", 100.0)]);
    gi.buy = map(&[("top", 10.0)]);
    Graph::build(&LoopDomain, &gi).expect("loop graph builds")
}

// ---------------------------------------------------------------------------------------------
// Rare -Try-> (0.9999 back to Rare | 1e-4 Goal) beside a Pricey node whose sell-and-rebuy detour
// (sell 90, buy 100) is cheaper than its 300,000-chaos craft. A truncated value-iteration seed
// would call that detour a money loop; a distance-seeded policy walks the proper edge first.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum RState {
    Rare,
    Pricey,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum RAction {
    Try,
    Craft,
}

pub struct RareDomain;

impl Domain for RareDomain {
    type State = RState;
    type Action = RAction;

    fn key(&self, s: &RState) -> String {
        match s {
            RState::Rare => "rare",
            RState::Pricey => "pricey",
            RState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &RState) -> bool {
        *s == RState::Goal
    }

    fn actions(&self, s: &RState) -> Vec<RAction> {
        match s {
            RState::Rare => vec![RAction::Try],
            RState::Pricey => vec![RAction::Craft],
            RState::Goal => Vec::new(),
        }
    }

    fn outcomes(&self, s: &RState, a: &RAction) -> Vec<(RState, f64)> {
        match (s, a) {
            (RState::Rare, RAction::Try) => vec![(RState::Rare, 0.9999), (RState::Goal, 0.0001)],
            (RState::Pricey, RAction::Craft) => vec![(RState::Goal, 1.0)],
            _ => Vec::new(),
        }
    }

    fn cost(&self, s: &RState, a: &RAction) -> Vec<(String, f64)> {
        match (s, a) {
            (RState::Rare, RAction::Try) => vec![("chaos".to_owned(), 1.0)],
            (RState::Pricey, RAction::Craft) => vec![("craft".to_owned(), 1.0)],
            _ => Vec::new(),
        }
    }

    fn seconds(&self, _s: &RState, _a: &RAction) -> f64 {
        1.0
    }

    fn predecessors(&self, _s: &RState) -> Vec<(RAction, RState)> {
        Vec::new()
    }

    fn label(&self, a: &RAction) -> String {
        match a {
            RAction::Try => "Try",
            RAction::Craft => "Craft",
        }
        .to_owned()
    }
}

pub fn rare_graph() -> Graph<RState, RAction> {
    let mut gi = input(vec![RState::Rare, RState::Pricey], &["rare", "pricey"]);
    gi.prices = map(&[("chaos", 1.0), ("craft", 300_000.0)]);
    gi.buy = map(&[("goal", 300_000.0), ("pricey", 100.0)]);
    gi.sell = map(&[("pricey", 90.0)]);
    gi.trade_penalty = 0.01;
    Graph::build(&RareDomain, &gi).expect("the rare graph builds")
}

// ---------------------------------------------------------------------------------------------
// Stuck has a p=1 self-loop ("nothing changes") and one edge that reaches the goal.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum IState {
    Stuck,
    Goal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum IAction {
    Nothing,
    Fix,
}

pub struct InertDomain;

impl Domain for InertDomain {
    type State = IState;
    type Action = IAction;

    fn key(&self, s: &IState) -> String {
        match s {
            IState::Stuck => "stuck",
            IState::Goal => "goal",
        }
        .to_owned()
    }

    fn is_goal(&self, s: &IState) -> bool {
        *s == IState::Goal
    }

    fn actions(&self, s: &IState) -> Vec<IAction> {
        match s {
            IState::Stuck => vec![IAction::Nothing, IAction::Fix],
            IState::Goal => Vec::new(),
        }
    }

    fn outcomes(&self, s: &IState, a: &IAction) -> Vec<(IState, f64)> {
        match (s, a) {
            (IState::Stuck, IAction::Nothing) => vec![(IState::Stuck, 1.0)],
            (IState::Stuck, IAction::Fix) => vec![(IState::Goal, 1.0)],
            _ => Vec::new(),
        }
    }

    fn cost(&self, s: &IState, a: &IAction) -> Vec<(String, f64)> {
        match (s, a) {
            (IState::Stuck, IAction::Nothing) => vec![("chaos".to_owned(), 1.0)],
            (IState::Stuck, IAction::Fix) => vec![("fix".to_owned(), 1.0)],
            _ => Vec::new(),
        }
    }

    fn seconds(&self, _s: &IState, _a: &IAction) -> f64 {
        1.0
    }

    fn predecessors(&self, _s: &IState) -> Vec<(IAction, IState)> {
        Vec::new()
    }

    fn label(&self, a: &IAction) -> String {
        match a {
            IAction::Nothing => "Nothing",
            IAction::Fix => "Fix",
        }
        .to_owned()
    }
}

pub fn inert_graph() -> Graph<IState, IAction> {
    let mut gi = input(vec![IState::Stuck], &["stuck"]);
    gi.prices = map(&[("chaos", 1.0), ("fix", 5.0)]);
    gi.buy = map(&[("stuck", 10.0)]);
    Graph::build(&InertDomain, &gi).expect("the inert graph builds")
}
