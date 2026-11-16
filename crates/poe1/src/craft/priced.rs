//! The lumped class graph as a priced Markov decision problem.
//!
//! `craft::lumped` already answers, exactly, what one currency does to one class; `crates/solver`
//! already answers how to reach a goal cheapest over a priced probabilistic graph. This module is
//! the adapter between them: every [`Class`] is a solver state, every registered currency a solver
//! action, and `apply::cost`'s line items are the price of an edge.
//!
//! # What is exact, and where it stops
//!
//! A lumped step's outcome distribution is exact rationals ([`Fraction`]), and every transition
//! keeps its exact chance beside the `f64` the solver reads ([`Transition`]). The one lossy step is
//! `num/den` to `f64`, which the solver's linear algebra requires; a reported chaos figure can be
//! traced back to the exact chance that produced it, but the solve itself is floating point.
//!
//! # Why the closure is built here, not by the solver
//!
//! `GraphInput::expanded` only names the states to expand: the solver interns whatever an outcome
//! refers to but never grows the set itself. So the graph is expanded here, breadth first, up to
//! [`NODE_BUDGET`] classes. `MAX_NODES` is 5 000 and the dense solve is O(n³) per policy round, and
//! the closure of a real base is orders of magnitude past either, so the size is the first number to
//! look at: [`Priced::closure_size`] is what was expanded and [`Priced::truncated`] is what the cut
//! folded into [`Node::Frontier`].
//!
//! # What this is not
//!
//! [`Priced::report`] is the optimum of the graph this adapter built under the prices it was given.
//! It is not a claim about the game, and not a claim that the goal predicate is the finished item:
//! `Stage::Desired` is one desired modifier, which is the owner's pattern, not a finished craft.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use solver::{Domain, Graph, GraphInput, SolveError, moments, solve};

use super::lumped::{Class, Cleared, Lumped};
use super::machine::MachineError;
use super::{Action, CurrencyId, Policy, Stage, Step};
use crate::apply::{self, Params};
use crate::currency::{Fraction, ItemSetFilter};
use crate::data::Data;
use crate::item::Item;

/// The registered currencies the lumped step can take, in a stable order.
///
/// The machine's registry holds seven ids (`craft::machine::currency`). Six are here:
/// `transmutation`, `alteration`, `augmentation`, `regal`, `exalted` and `scouring`.
///
/// `currency/annulment` is refused by the lumped step itself (`LumpError::NotLumpable`): a class
/// records the per-side *union* of modifier groups, not which modifier carries which group, so two
/// members of one class lose different groups to the same annulment and land in different classes.
/// That is a property of the coarse state, not a missing registration, so it cannot be worked
/// around here.
///
/// Everything else is out of reach for a different reason: a parameterised method (essence, fossil)
/// has no `CurrencyId` to name it, and a method the registry does not hold (chaos, divine, bench,
/// beast) cannot be applied by the machine at all.
pub const ACTIONS: [CurrencyId; 6] = [
    CurrencyId::TRANSMUTATION,
    CurrencyId::ALTERATION,
    CurrencyId::AUGMENTATION,
    CurrencyId::REGAL,
    CurrencyId::EXALTED,
    CurrencyId::SCOURING,
];

/// Most class nodes the graph expands.
///
/// The whole reachable closure of a real craft is far past anything the solver can hold: measured on
/// an item level 86 Shaper Archdemon Crown under these six currencies, breadth-first expansion reaches
/// 314 classes at one lumped step, and the closure was still growing when a 500 000-node probe was
/// stopped. That is inherent to the class, which records the per-side unions of modifier groups and
/// tags, so three modifiers a side multiply out.
///
/// `solver::graph::MAX_NODES` is 5 000 and the solve is O(n³) per policy round, so the budget has to
/// be well under that. One thousand solves in under a second and lands on the same expected cost as
/// two thousand to three decimals, and it is a cut rather than a limit a plan is expected to reach:
/// [`Priced::truncated`] counts the outcome classes the cut folded into [`Node::Frontier`].
pub const NODE_BUDGET: usize = 1_000;

/// The node key of the absorbing goal: the mass a lumped step absorbs at `Stage::Desired`.
///
/// A class key is `class:{...}` and the solver reserves `start` and `goal`, so this can never
/// collide with a class.
const DONE_KEY: &str = "craft:done";

/// The node key of the single dead end the node budget folds every unexpanded class into.
const FRONTIER_KEY: &str = "craft:frontier";

/// The finished item's own buy price, and the only key the graph's reserved `goal` node can be
/// priced under. It is not a class key and not this module's `DONE_KEY`: the solver resolves it to
/// its `goal` node, which is where a solved craft ends.
pub const GOAL_KEY: &str = "goal";

/// One state of the priced graph: a lumped class, or the mass that reached the goal.
///
/// `Done` exists because the lumped step absorbs `Stage::Desired` mass instead of producing a class
/// for it, and `Domain::outcomes` has to name a state for that mass. It is the goal, so it is
/// absorbing at zero value: the finished item's worth enters as a buy price, never as a terminal
/// reward, which is why `solver` can price an unfinished craft against simply buying the item.
///
/// Equality, ordering and hashing are the class's, which are its key's: the witness item a class
/// carries is not identity.
// A `Node` is interned at most once per class in a graph bounded by `NODE_BUDGET`, and a `Class`
// already owns a witness item; boxing the variant would add an allocation to every construction and
// every pattern match for no measurable gain in a graph this size.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Node {
    Class(Class),
    Done,
    /// An outcome class the node budget left unexpanded.
    ///
    /// Every class past the budget is folded into this one node, which has no actions and is sold
    /// as a brick. Collapsing them is the honest way to stop: a transition must land somewhere and
    /// its chance must stay on the row, so the choice is one shared dead end, not a dropped outcome
    /// that would make the row stop summing to one.
    Frontier,
}

impl Node {
    /// The stable node key the solver interns and the buy/sell maps are keyed by.
    ///
    /// Class keys read `class:` followed by the class's identity string, so they can never equal
    /// the reserved `start` or `goal`, nor this module's [`DONE_KEY`].
    pub fn key(&self) -> String {
        match self {
            Node::Class(class) => format!("class:{class}"),
            Node::Done => DONE_KEY.to_owned(),
            Node::Frontier => FRONTIER_KEY.to_owned(),
        }
    }
}

impl fmt::Debug for Node {
    /// The key, not the class: a class carries a witness item and printing one dumps the item.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}

/// One outcome of one action from one class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    /// The node reached: the goal, or an index into [`Priced::closure`].
    pub to: Next,
    /// The chance, exactly as the lumped step computed it.
    pub chance: Fraction,
    /// The same chance as the solver reads it. The adapter's only lossy conversion.
    pub probability: f64,
}

/// Where a transition lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Next {
    /// The goal, bought or crafted.
    Done,
    /// The class at this index in [`Priced::closure`].
    Class(usize),
    /// A class the node budget left unexpanded. All of them share one absorbing symbol that can
    /// only be sold, so their mass is conserved without growing the graph.
    Frontier,
}

/// The price of a craft plan, in chaos.
///
/// # `prices.json`
///
/// ```json
/// {
///   "prices": {
///     "Metadata/Items/Currency/CurrencyUpgradeToMagic": 0.1,
///     "Metadata/Items/Currency/CurrencyRerollMagic": 0.07
///   },
///   "buy": { "goal": 40.0, "class:Key { base: 107, ... }": 0.05 },
///   "sell": { "class:Key { base: 107, ... }": 0.2 },
///   "tradePenalty": 0.02
/// }
/// ```
///
/// `prices` is keyed by a *currency metadata id* — the key `apply::cost` produces, not the method id
/// — and a missing key is a hard error, never a zero. `buy` and `sell` are keyed by node key: a
/// class node's key is [`Node::key`] of that class (see [`Priced::start_key`] for the one entry every
/// report needs), plus the reserved `"goal"` for the finished item. `tradePenalty` is charged on
/// every trade; keep it positive, because a free buy/sell loop is an indifferent strategy the
/// solver reports as a money loop rather than one it can rank.
///
/// Every number here is an owner input: this adapter cannot know what a chaos orb is worth, what
/// junk sells for, or what the finished item fetches.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Prices {
    /// Chaos per line-item key (a currency metadata id).
    pub prices: BTreeMap<String, f64>,
    /// Chaos to obtain a node by buying it. `"goal"` prices the finished item.
    pub buy: BTreeMap<String, f64>,
    /// Chaos a node fetches when sold as a brick. A key that is absent sells for nothing.
    pub sell: BTreeMap<String, f64>,
    /// Extra chaos charged on every trade, so an indifferent loop still settles.
    pub trade_penalty: f64,
}

/// Why the priced graph could not be built, solved, or reported.
#[derive(Clone, Debug, PartialEq)]
pub enum PricedError {
    /// A registered currency names a method the data does not hold.
    UnknownMethod(&'static str),
    /// `apply::cost` could not resolve the method's cost list.
    Cost {
        action: &'static str,
        reason: String,
    },
    /// The method declares no cost at all, so a use of it could not be priced. Named rather than
    /// treated as free: a free currency would make a zero-cost loop the optimal plan.
    NoCost(&'static str),
    /// A lumped step failed for a reason a refused currency does not explain.
    Step(super::lumped::LumpError),
    /// The graph build or the solve failed.
    Solve(SolveError),
    /// The start item already holds a desired modifier, so there is no craft to price.
    AlreadyDone,
    /// The solved policy has no finite expected cost from `start`: the graph cannot fund itself.
    NoFiniteCost,
    /// The buy map prices neither the start class nor the goal, so nothing can enter the craft.
    Unfunded(String),
}

impl fmt::Display for PricedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PricedError::UnknownMethod(id) => {
                write!(f, "no method is registered under the id {id}")
            }
            PricedError::Cost { action, reason } => {
                write!(f, "the cost of {action} cannot be resolved: {reason}")
            }
            PricedError::NoCost(id) => write!(
                f,
                "{id} declares no cost, so its use cannot be priced; a free currency would make \
                 a zero-cost loop optimal"
            ),
            PricedError::Step(err) => write!(f, "a lumped step failed: {err}"),
            PricedError::Solve(err) => err.fmt(f),
            PricedError::AlreadyDone => write!(
                f,
                "the start item already holds a desired modifier; the only plan left is to stop"
            ),
            PricedError::NoFiniteCost => {
                write!(
                    f,
                    "the solved policy has no finite expected cost from the start"
                )
            }
            PricedError::Unfunded(key) => {
                write!(f, "no buy price is set for {key}")
            }
        }
    }
}

impl std::error::Error for PricedError {}

impl From<super::lumped::LumpError> for PricedError {
    fn from(err: super::lumped::LumpError) -> PricedError {
        PricedError::Step(err)
    }
}

impl From<SolveError> for PricedError {
    fn from(err: SolveError) -> PricedError {
        PricedError::Solve(err)
    }
}

/// The edge the solved policy takes out of one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Move {
    /// Apply this registered currency.
    Craft(CurrencyId),
    /// Buy the node at its buy price.
    Buy,
    /// Sell the node as a brick.
    Sell,
}

/// What the solved graph says.
///
/// Numbers are the optimum of *this* graph under *these* prices. `expected_chaos` is what the
/// solved policy spends to reach the goal from `start`; it is not a market price for the item.
#[derive(Clone, Debug)]
pub struct Report {
    /// Class nodes in the reachable closure. Two more nodes — `start` and `goal` — are the solver's.
    pub closure: usize,
    /// Expected chaos from `start` to the goal, under the prices the report was solved with.
    pub expected_chaos: f64,
    /// Standard deviation of that chaos.
    pub sd_chaos: f64,
    /// Expected seconds, which is only the buy edges' [`solver::graph::BUY_SECONDS`]: craft time is
    /// not modelled, so a craft edge costs no time here.
    pub expected_seconds: f64,
    /// Expected number of each line item (currency metadata id) consumed on the way.
    pub expected_spend: BTreeMap<String, f64>,
    /// The policy by node key. Nodes outside the almost-sure reachability set have no entry.
    pub policy: BTreeMap<String, Move>,
    /// How many outcome classes the node budget folded into the frontier. Zero means the graph was
    /// the whole reachable closure; otherwise `expected_chaos` is an upper bound on the whole
    /// space's optimum, because every state past the cut lost its options.
    pub truncated: usize,
    /// Improvement rounds the solve took.
    pub iterations: u32,
}

/// The lumped class graph, priced.
///
/// The whole reachable closure is expanded at construction, so [`Domain::outcomes`] is a table look
/// rather than a recomputation, and so the closure size can be reported before anything is solved.
pub struct Priced {
    start: Class,
    /// The prices [`Priced::report`] solves with. Public so the same closure can be re-priced and
    /// re-solved without expanding it again; nothing else about the graph depends on prices.
    pub prices: Prices,
    /// The closure, in discovery order. `Next::Class(i)` indexes this.
    closure: Vec<Class>,
    /// `(class, action)` → that action's exact outcome distribution from that class.
    steps: BTreeMap<(Class, CurrencyId), Vec<Transition>>,
    /// The actions each class can actually take, in [`ACTIONS`] order. A currency the item refuses
    /// is not here, so `Domain::actions` never names an action with no outcome.
    actions: BTreeMap<Class, Vec<CurrencyId>>,
    /// `apply::cost` verbatim: a currency metadata id and a count of the tradeable item.
    costs: BTreeMap<CurrencyId, Vec<(String, f64)>>,
    /// How many outcome classes the node budget routed to the frontier instead of expanding.
    truncated: usize,
}

/// One action's outcome from one class, with classes not yet interned.
#[derive(Clone)]
struct Raw {
    live: Vec<(Class, Fraction)>,
    absorbed: Fraction,
}

/// A policy that answers one fixed action, so `Lumped::step` can take one currency from one class.
struct Fixed(CurrencyId);

impl Policy for Fixed {
    fn step(&self, _data: &Data, _item: &Item) -> Step {
        Step {
            action: Action::Apply(self.0),
            reason: "the priced graph takes one action at a time",
        }
    }
}

impl Priced {
    /// Expand the reachable closure of `start` under a node budget of [`NODE_BUDGET`].
    pub fn build(
        data: &Data,
        filter: ItemSetFilter,
        start: &Item,
        prices: Prices,
    ) -> Result<Priced, PricedError> {
        Priced::with_budget(data, filter, start, prices, NODE_BUDGET)
    }

    /// Expand the reachable closure of `start` breadth first, keeping at most `budget` classes.
    ///
    /// The graph interns `budget` class nodes plus three symbols (`start`, `goal`, the frontier), so
    /// a budget within three of `solver::graph::MAX_NODES` is refused by the solver's own size check
    /// rather than here.
    ///
    /// The reachable closure of a real base is far larger than any solvable graph — the class
    /// records per-side group and tag unions, so three modifiers a side multiply out — so the
    /// budget is a cut, not a limit that a plan is expected to hit. Classes nearest the start are
    /// kept, and every outcome past the cut lands on [`Node::Frontier`], which has no actions and is
    /// sold as a brick. [`Priced::truncated`] counts what the cut absorbed, and [`Report`] says
    /// whether the graph is the whole closure: a report over a cut graph can only overstate the cost
    /// of the whole space, because the cut removes options from the states beyond it.
    pub fn with_budget(
        data: &Data,
        filter: ItemSetFilter,
        start: &Item,
        prices: Prices,
        budget: usize,
    ) -> Result<Priced, PricedError> {
        let costs = cost_table(data)?;
        let start_class = Class::of(data, &filter, start);
        // A class whose stage is `Desired` never appears as an outcome — the lumped step absorbs
        // that mass instead of producing a class for it — so the only way to see one is as the
        // start, and pricing further crafting on an already finished item would be nonsense.
        if start_class.stage(data, &filter) == Stage::Desired {
            return Err(PricedError::AlreadyDone);
        }

        let mut closure: Vec<Class> = vec![start_class.clone()];
        let mut index: BTreeMap<Class, usize> = BTreeMap::from([(start_class.clone(), 0)]);
        let mut queue: VecDeque<Class> = VecDeque::from([start_class.clone()]);
        let mut steps: BTreeMap<(Class, CurrencyId), Vec<Transition>> = BTreeMap::new();
        let mut actions: BTreeMap<Class, Vec<CurrencyId>> = BTreeMap::new();
        // An alteration's outcomes depend only on the class its clear leaves, so one computation
        // stands for every class that clears the same way (`Lumped`'s own cache invariant).
        let mut alterations: BTreeMap<Cleared, Option<Raw>> = BTreeMap::new();
        let mut truncated = 0usize;

        while let Some(class) = queue.pop_front() {
            let mut applicable = Vec::new();
            for action in ACTIONS {
                let Some(raw) = step_of(data, &filter, &mut alterations, &class, action)? else {
                    continue;
                };
                let mut trans: Vec<Transition> = Vec::with_capacity(raw.live.len() + 1);
                let mut frontier = Fraction::ZERO;
                for (outcome, chance) in raw.live {
                    let at = match index.get(&outcome) {
                        Some(&i) => Some(i),
                        None if closure.len() < budget => {
                            let i = closure.len();
                            closure.push(outcome.clone());
                            index.insert(outcome, i);
                            queue.push_back(closure[i].clone());
                            Some(i)
                        }
                        // Past the budget the class is not interned: its mass joins the frontier
                        // instead, so the row still sums to one and the graph stays bounded.
                        None => None,
                    };
                    match at {
                        Some(i) => trans.push(Transition {
                            to: Next::Class(i),
                            chance,
                            probability: as_f64(chance),
                        }),
                        None => {
                            // Two exact chances from one row cannot realistically overflow `u128`,
                            // but the sum is checked rather than wrapped: the failure is reported
                            // with the same "cannot be held exactly" wording the lumped step uses.
                            frontier = frontier.checked_add(chance).ok_or(PricedError::Step(
                                super::lumped::LumpError::Machine(MachineError::Overflow),
                            ))?;
                            truncated += 1;
                        }
                    }
                }
                if !frontier.is_zero() {
                    trans.push(Transition {
                        to: Next::Frontier,
                        chance: frontier,
                        probability: as_f64(frontier),
                    });
                }
                if !raw.absorbed.is_zero() {
                    trans.push(Transition {
                        to: Next::Done,
                        chance: raw.absorbed,
                        probability: as_f64(raw.absorbed),
                    });
                }
                applicable.push(action);
                steps.insert((class.clone(), action), trans);
            }
            if !applicable.is_empty() {
                actions.insert(class, applicable);
            }
        }

        Ok(Priced {
            start: start_class,
            prices,
            closure,
            steps,
            actions,
            costs,
            truncated,
        })
    }

    /// The node key a `Prices::buy` entry must use to price acquiring the base item.
    ///
    /// The solver's `start` node ("nothing owned yet") reaches the craft only by buying into it, so
    /// a report without this entry either prices nothing or cannot start.
    pub fn start_key(data: &Data, filter: &ItemSetFilter, start: &Item) -> String {
        Node::Class(Class::of(data, filter, start)).key()
    }

    /// The class every report starts from.
    pub fn start(&self) -> &Class {
        &self.start
    }

    /// The reachable closure, in discovery order.
    pub fn closure(&self) -> &[Class] {
        &self.closure
    }

    /// How many class nodes the reachable closure holds.
    pub fn closure_size(&self) -> usize {
        self.closure.len()
    }

    /// How many outcome classes the node budget folded into [`Node::Frontier`].
    ///
    /// Zero means the graph is the whole reachable closure. Anything else means the closure is at
    /// least `closure_size() + truncated` classes, and the report is about the expanded part only.
    pub fn truncated(&self) -> usize {
        self.truncated
    }

    /// Every expanded class and, per action it can take, that action's exact outcomes.
    ///
    /// This is the whole graph the solver sees; `Domain::outcomes` converts this table to the
    /// `f64`s it needs.
    pub fn steps(&self) -> impl Iterator<Item = (&Class, CurrencyId, &[Transition])> {
        self.steps
            .iter()
            .map(|((class, action), trans)| (class, *action, trans.as_slice()))
    }

    /// `apply::cost`'s line items for one action, in chaos-per-unit keys.
    pub fn cost(&self, action: CurrencyId) -> &[(String, f64)] {
        self.costs.get(&action).map_or(&[], Vec::as_slice)
    }

    /// Build the solver's graph, solve it, and report what the optimum is.
    ///
    /// The graph is the closure expanded at construction, priced by [`Priced::prices`]. Its `start`
    /// node is the solver's "nothing owned yet"; [`Priced::start_key`] and [`GOAL_KEY`] must both be
    /// priced in `buy`, or the report fails as unfunded rather than silently pricing a craft that
    /// cannot begin.
    pub fn report(&self) -> Result<Report, PricedError> {
        let start_key = Node::Class(self.start.clone()).key();
        for key in [start_key.as_str(), GOAL_KEY] {
            if !self.prices.buy.contains_key(key) {
                return Err(PricedError::Unfunded(key.to_owned()));
            }
        }
        let input = GraphInput {
            seeds: vec![Node::Class(self.start.clone())],
            expanded: self
                .closure
                .iter()
                .map(|class| Node::Class(class.clone()).key())
                .collect::<BTreeSet<String>>(),
            disabled: BTreeSet::new(),
            prices: self.prices.prices.clone(),
            buy: self.prices.buy.clone(),
            sell: self.prices.sell.clone(),
            trade_penalty: self.prices.trade_penalty,
        };
        let graph: Graph<Node, CurrencyId> = Graph::build(self, &input)?;
        let solved = solve(&graph)?;
        let moments = moments(&graph, &solved).ok_or(PricedError::NoFiniteCost)?;

        let mut policy = BTreeMap::new();
        for (i, chosen) in solved.policy.iter().enumerate() {
            let Some(ei) = chosen else { continue };
            let step = match &graph.edges[*ei].kind {
                solver::graph::EdgeKind::Craft { action, .. } => Move::Craft(*action),
                solver::graph::EdgeKind::Buy => Move::Buy,
                solver::graph::EdgeKind::Sell => Move::Sell,
            };
            policy.insert(graph.nodes[i].key.clone(), step);
        }

        Ok(Report {
            closure: self.closure.len(),
            expected_chaos: moments.expected_cost,
            sd_chaos: moments.sd_cost,
            expected_seconds: moments.expected_seconds,
            expected_spend: moments.expected_spend,
            policy,
            truncated: self.truncated,
            iterations: solved.iterations,
        })
    }
}

impl Domain for Priced {
    type State = Node;
    type Action = CurrencyId;

    fn key(&self, s: &Node) -> String {
        s.key()
    }

    fn is_goal(&self, s: &Node) -> bool {
        matches!(s, Node::Done)
    }

    fn actions(&self, s: &Node) -> Vec<CurrencyId> {
        match s {
            Node::Done | Node::Frontier => Vec::new(),
            Node::Class(class) => self.actions.get(class).cloned().unwrap_or_default(),
        }
    }

    fn outcomes(&self, s: &Node, a: &CurrencyId) -> Vec<(Node, f64)> {
        let Node::Class(class) = s else {
            return Vec::new();
        };
        let Some(trans) = self.steps.get(&(class.clone(), *a)) else {
            return Vec::new();
        };
        trans
            .iter()
            .map(|t| {
                let to = match t.to {
                    Next::Done => Node::Done,
                    Next::Frontier => Node::Frontier,
                    Next::Class(i) => Node::Class(self.closure[i].clone()),
                };
                (to, t.probability)
            })
            .collect()
    }

    fn cost(&self, _s: &Node, a: &CurrencyId) -> Vec<(String, f64)> {
        // The six currencies are priced per use, not per item: `apply::cost` reads the method, so
        // the state cannot change the answer. A method whose price depended on the item (an essence
        // on a base with a cost modifier) would have to price per state instead.
        self.costs.get(a).cloned().unwrap_or_default()
    }

    fn seconds(&self, _s: &Node, _a: &CurrencyId) -> f64 {
        // Craft time is not modelled. Only the buy edges carry time, so `Report::expected_seconds`
        // reads as the number of trades times the solver's own trade time.
        0.0
    }

    fn predecessors(&self, _s: &Node) -> Vec<(CurrencyId, Node)> {
        // The solver uses this only to propose new layers to expand, and the graph is expanded at
        // build time as far as the budget allows, so there is nothing to propose.
        Vec::new()
    }

    fn label(&self, a: &CurrencyId) -> String {
        a.0.to_owned()
    }

    fn sell_value(&self, s: &Node) -> Option<f64> {
        match s {
            // The goal is the finished item, priced as a buy, not as a brick; the frontier is a
            // symbol standing for many classes, so it has no key a sell price could name.
            Node::Done | Node::Frontier => None,
            Node::Class(_) => self.prices.sell.get(&s.key()).copied(),
        }
    }
}

/// The exact chance as the solver reads it.
///
/// `Fraction` keeps `num/den` exact and refuses to compare values; the solver needs a float. This is
/// the adapter's one conversion, so it lives in one place and stays beside the exact value.
fn as_f64(chance: Fraction) -> f64 {
    chance.num() as f64 / chance.den() as f64
}

/// `apply::cost` for every action, rejecting a method that declares no cost at all.
fn cost_table(data: &Data) -> Result<BTreeMap<CurrencyId, Vec<(String, f64)>>, PricedError> {
    let mut out = BTreeMap::new();
    for &action in &ACTIONS {
        let method = data
            .method(action.0)
            .ok_or(PricedError::UnknownMethod(action.0))?;
        let items =
            apply::cost(data, method, &Params::default()).map_err(|err| PricedError::Cost {
                action: action.0,
                reason: err.reason,
            })?;
        if items.is_empty() {
            return Err(PricedError::NoCost(action.0));
        }
        out.insert(
            action,
            items
                .into_iter()
                .map(|cost| (cost.key, cost.amount))
                .collect(),
        );
    }
    Ok(out)
}

/// One action's outcome from one class, or `None` when the currency refuses the class.
fn step_of(
    data: &Data,
    filter: &ItemSetFilter,
    alterations: &mut BTreeMap<Cleared, Option<Raw>>,
    class: &Class,
    action: CurrencyId,
) -> Result<Option<Raw>, PricedError> {
    if action != CurrencyId::ALTERATION {
        return compute(data, filter, class, action);
    }
    // Every class with the same cleared class rerolls an alteration the same way, exactly as
    // `Lumped`'s own cache assumes; the difference is what keeps the closure from recomputing one
    // reroll for every magic item it holds.
    let key = class.cleared();
    if let Some(step) = alterations.get(&key) {
        return Ok(step.clone());
    }
    let computed = compute(data, filter, class, action)?;
    alterations.insert(key, computed.clone());
    Ok(computed)
}

/// Run one currency on one class through the lumped step and fold its outcomes.
fn compute(
    data: &Data,
    filter: &ItemSetFilter,
    class: &Class,
    action: CurrencyId,
) -> Result<Option<Raw>, PricedError> {
    let mut run = Lumped::new(data, filter.clone(), class.witness().clone());
    let depth = match run.step(&Fixed(action)) {
        Ok(depth) => depth,
        // A currency the item refuses is not applicable to it, which is a missing edge, not an
        // error. Every other failure means the step could not be held exactly, and pricing a
        // rounded distribution would be worse than refusing.
        Err(super::lumped::LumpError::Machine(MachineError::Refused(_))) => return Ok(None),
        Err(err) => return Err(PricedError::Step(err)),
    };
    let live = depth
        .classes
        .iter()
        .filter(|(_, chance)| !chance.is_zero())
        .map(|(outcome, chance)| (outcome.clone(), *chance))
        .collect();
    Ok(Some(Raw {
        live,
        absorbed: depth.absorbed,
    }))
}
