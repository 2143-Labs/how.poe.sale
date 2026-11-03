//! The market of goods (plan "Goods", "Processes", "Value of a good"): every item the user cares
//! about is a good, every way to make one is a process, and each good is worth
//! `min(buy, cheapest process)` over the whole graph of goods.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use solver::graph::{BUY_SECONDS, EdgeKind, GOAL, Graph, GraphInput, MAX_NODES, MIN_COST, START};
use solver::{Domain, SolveError, Solved};

use crate::data::Bundle;
use crate::engine::Engine;
use crate::item::{ItemState, Side};
use crate::kernels::{self, Action};
use crate::registry::{self, DonorSpec};
use crate::target::{self, Target};

pub const MAX_GOODS: usize = 64;
pub const MAX_PROCESSES: usize = 256;
/// Gauss–Seidel rounds a cyclic group of goods gets to settle.
pub const MARKET_ROUNDS: u32 = 50;
/// Largest relative change that still counts as settled.
pub const SETTLE_TOL: f64 = 1e-9;
/// Price key of 1,000 Thaumaturgic Dust, in chaos.
pub const DUST_KEY: &str = "thaumaturgic-dust-1k";
/// Price key of 1,000 Gold, in chaos.
pub const GOLD_KEY: &str = "gold-1k";

/// (price key, quantity) of one Recombinator use: its Dust and Gold, times the influenced factor
/// when an input carries an influence (claim recombine.cost).
pub fn recombine_cost(influenced: bool) -> Vec<(String, f64)> {
    let c = crate::claims::recombine_cost();
    let f = if influenced { c.influenced_factor } else { 1.0 };
    vec![
        (DUST_KEY.to_string(), c.dust / 1000.0 * f),
        (GOLD_KEY.to_string(), c.gold / 1000.0 * f),
    ]
}
/// Extra chaos on every trade, as in the single-item graph.
pub const TRADE_PENALTY: f64 = 0.01;

/// An item description with optional typed prices.
#[derive(Clone, Debug, PartialEq)]
pub struct Good {
    pub id: String,
    pub name: String,
    pub spec: Target,
    pub buy: Option<f64>,
    /// What one of these is worth when a process makes it by accident (a brick).
    pub sell: Option<f64>,
}

/// The user's view of one `Craft` process's tree: the v1 plan fields, keyed by state keys of the
/// process's own graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftPlan {
    /// `None`: the forward closure of the start item under the allowed crafts.
    pub expanded: Option<BTreeSet<String>>,
    pub added: Vec<ItemState>,
    pub disabled: BTreeSet<(String, String)>,
    pub buy: BTreeMap<String, f64>,
    pub sell: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessKind {
    /// One item: start from the input good's ideal item (or, without one, from the output's ideal
    /// item as in v1), apply the allowed crafts, stop when the output good is satisfied.
    Craft {
        from: Option<String>,
        /// Contract ids; empty allows every craft.
        allowed: Vec<String>,
        plan: CraftPlan,
    },
    /// Two items, one Recombinator, one attempt per pair of inputs.
    Recombine { a: String, b: String },
    /// A craft the app does not model: each attempt costs `cost` chaos and makes the output
    /// with `chance`; on failure the input is lost, or kept when `keeps_input`.
    Custom {
        from: Option<String>,
        name: String,
        chance: f64,
        cost: f64,
        keeps_input: bool,
    },
}

/// Seconds one attempt of a custom craft takes.
pub const CUSTOM_SECONDS: f64 = 2.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Process {
    pub id: String,
    pub output: String,
    pub kind: ProcessKind,
    pub recipe: Option<String>,
}

/// A validated market: goods, processes and the shared requirement vocabulary.
#[derive(Clone, Debug)]
pub struct Market {
    pub goods: Vec<Good>,
    pub processes: Vec<Process>,
    /// Union of every good's requirements; every state in the market is described against it.
    pub universe: Target,
    pub root: usize,
    /// The approximate recombine rules this market is solved under.
    pub recombine_rules: kernels::RecombineRules,
    index: HashMap<String, usize>,
}

/// Whether two requirements describe the same mod key.
fn same_key(a: &target::Requirement, b: &target::Requirement) -> bool {
    a.group == b.group && a.side == b.side
}

impl Market {
    pub fn new(goods: Vec<Good>, processes: Vec<Process>, root: &str) -> Result<Market, String> {
        if goods.len() > MAX_GOODS || processes.len() > MAX_PROCESSES {
            return Err("market too large".to_string());
        }
        let mut index = HashMap::new();
        for (i, g) in goods.iter().enumerate() {
            if index.insert(g.id.clone(), i).is_some() {
                return Err(format!("two goods share the id \"{}\"", g.id));
            }
        }
        let root = *index
            .get(root)
            .ok_or_else(|| format!("the root good \"{root}\" does not exist"))?;
        let mut pids = BTreeSet::new();
        for p in &processes {
            if !pids.insert(p.id.clone()) {
                return Err(format!("two processes share the id \"{}\"", p.id));
            }
            let mut refs = vec![&p.output];
            match &p.kind {
                ProcessKind::Craft { from, .. } => refs.extend(from.iter()),
                ProcessKind::Recombine { a, b } => refs.extend([a, b]),
                ProcessKind::Custom {
                    from, chance, cost, ..
                } => {
                    let id = &p.id;
                    if !chance.is_finite() || *chance <= 0.0 || *chance > 1.0 {
                        return Err(format!(
                            "process \"{id}\": a custom craft's success chance must be above 0 and at most 1"
                        ));
                    }
                    if !cost.is_finite() || *cost < 0.0 {
                        return Err(format!(
                            "process \"{id}\": a custom craft's cost must be 0 chaos or more"
                        ));
                    }
                    if from.is_none() && *cost == 0.0 {
                        return Err(format!(
                            "process \"{id}\": a custom craft without an input must cost more than 0 chaos"
                        ));
                    }
                    if from.as_ref() == Some(&p.output) {
                        return Err(format!(
                            "process \"{id}\": a custom craft cannot use its own output as its input"
                        ));
                    }
                    refs.extend(from.iter());
                }
            }
            for r in refs {
                if !index.contains_key(r) {
                    return Err(format!(
                        "process \"{}\" names the unknown good \"{r}\"",
                        p.id
                    ));
                }
            }
        }

        // One vocabulary: every good agrees on the minimum roll of each (group, side) it shares.
        // Accepted mods are unioned, so goods may want different mods of one group (a Lightning
        // and a Fire gem amulet); `remap` turns a mod a good does not accept into a blocked one.
        let mut requirements: Vec<target::Requirement> = Vec::new();
        let order = std::iter::once(root).chain((0..goods.len()).filter(|i| *i != root));
        for gi in order {
            for r in &goods[gi].spec.requirements {
                match requirements.iter_mut().find(|x| same_key(x, r)) {
                    Some(x) => {
                        if x.min_value != r.min_value {
                            return Err(format!(
                                "goods disagree on {}; use the same tier and roll for this mod in every good",
                                r.label
                            ));
                        }
                        for id in &r.accepted {
                            if !x.accepted.contains(id) {
                                x.accepted.push(*id);
                            }
                        }
                    }
                    None => requirements.push(r.clone()),
                }
            }
        }
        let rs = &goods[root].spec;
        let universe = Target {
            base: rs.base,
            ilvl_min: rs.ilvl_min,
            influences: rs.influences,
            rarity: rs.rarity,
            catalyst: rs.catalyst,
            requirements,
            min_hits: 0,
        };
        Ok(Market {
            goods,
            processes,
            universe,
            root,
            recombine_rules: kernels::RecombineRules::plan(),
            index,
        })
    }

    pub fn good(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    pub fn process(&self, id: &str) -> Option<usize> {
        self.processes.iter().position(|p| p.id == id)
    }

    /// The good's ideal item, described against the universe.
    pub fn ideal(&self, b: &Bundle, g: usize) -> ItemState {
        let spec = &self.goods[g].spec;
        target::remap(b, spec, &self.universe, &target::ideal_state(b, spec))
    }

    /// Does a universe-described state satisfy good `g`?
    pub fn satisfies(&self, b: &Bundle, g: usize, s: &ItemState) -> bool {
        let spec = &self.goods[g].spec;
        target::satisfied(b, spec, &target::remap(b, &self.universe, spec, s))
    }

    /// The first good other than `output` without a sell price that `s` satisfies.
    pub fn unpriced_of(&self, b: &Bundle, output: usize, s: &ItemState) -> Option<usize> {
        (0..self.goods.len())
            .find(|&i| i != output && self.goods[i].sell.is_none() && self.satisfies(b, i, s))
    }

    /// The best-selling good other than `output` that `s` satisfies.
    pub fn brick_of(&self, b: &Bundle, output: usize, s: &ItemState) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, g) in self.goods.iter().enumerate() {
            let Some(price) = g.sell else { continue };
            if i == output || !self.satisfies(b, i, s) {
                continue;
            }
            if best.is_none_or(|(_, p)| price > p) {
                best = Some((i, price));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Input goods of process `p`.
    pub fn inputs(&self, p: usize) -> Vec<usize> {
        match &self.processes[p].kind {
            ProcessKind::Craft { from, .. } | ProcessKind::Custom { from, .. } => {
                from.iter().filter_map(|f| self.good(f)).collect()
            }
            ProcessKind::Recombine { a, b } => {
                vec![self.good(a).unwrap_or(0), self.good(b).unwrap_or(0)]
            }
        }
    }

    fn output(&self, p: usize) -> usize {
        self.good(&self.processes[p].output).unwrap_or(self.root)
    }

    /// Engine inputs for a `Craft` process, owned so an [`Engine`] can borrow them.
    pub fn craft_setup(&self, b: &Bundle, p: usize, from_value: Option<f64>) -> CraftSetup {
        let ProcessKind::Craft {
            from,
            allowed,
            plan,
        } = &self.processes[p].kind
        else {
            panic!("craft_setup on a recombine process");
        };
        let output = self.output(p);
        let bricks: Vec<(Target, f64)> = self
            .goods
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != output)
            .filter_map(|(_, g)| g.sell.map(|s| (g.spec.clone(), s)))
            .collect();
        let allowed = if allowed.is_empty() {
            None
        } else {
            Some(allowed.iter().cloned().collect::<BTreeSet<String>>())
        };
        let s0 = from
            .as_ref()
            .and_then(|f| self.good(f))
            .map(|f| self.ideal(b, f));
        let mut seeds = Vec::new();
        seeds.extend(s0.iter().cloned());
        seeds.push(self.ideal(b, output));
        seeds.extend(plan.added.iter().cloned());
        let mut buy = plan.buy.clone();
        if let (Some(s0), Some(v)) = (&s0, from_value) {
            let key = s0.key();
            let price = buy.get(&key).map_or(v, |x| x.min(v));
            buy.insert(key, price);
        }
        CraftSetup {
            output,
            allowed,
            bricks,
            s0,
            seeds,
            buy,
            sell: plan.sell.clone(),
            disabled: plan.disabled.clone(),
            expanded: plan.expanded.clone(),
        }
    }

    /// The engine of a `Craft` process.
    pub fn engine<'a>(
        &'a self,
        b: &'a Bundle,
        setup: &'a CraftSetup,
        donors: &'a [DonorSpec],
    ) -> Engine<'a> {
        Engine {
            b,
            t: &self.universe,
            donors,
            allowed: setup.allowed.as_ref(),
            bricks: &setup.bricks,
            goal: Some(&self.goods[setup.output].spec),
        }
    }
}

/// Owned inputs of one `Craft` process's engine and graph.
#[derive(Clone, Debug)]
pub struct CraftSetup {
    pub output: usize,
    pub allowed: Option<BTreeSet<String>>,
    pub bricks: Vec<(Target, f64)>,
    /// The bought input item, when the process has one.
    pub s0: Option<ItemState>,
    pub seeds: Vec<ItemState>,
    pub buy: BTreeMap<String, f64>,
    pub sell: BTreeMap<String, f64>,
    pub disabled: BTreeSet<(String, String)>,
    pub expanded: Option<BTreeSet<String>>,
}

impl CraftSetup {
    pub fn graph_input(
        &self,
        engine: &Engine,
        prices: &BTreeMap<String, f64>,
    ) -> GraphInput<ItemState> {
        let expanded = match &self.expanded {
            Some(set) => set.clone(),
            None => match &self.s0 {
                Some(s0) => closure(engine, s0, &self.disabled),
                None => BTreeSet::new(),
            },
        };
        GraphInput {
            seeds: self.seeds.clone(),
            expanded,
            disabled: self.disabled.clone(),
            prices: prices.clone(),
            buy: self.buy.clone(),
            sell: self.sell.clone(),
            trade_penalty: TRADE_PENALTY,
        }
    }
}

/// Keys of every non-goal state reachable from `s0` under the engine's crafts, breadth first,
/// stopping once half of [`MAX_NODES`] states are known (the rest stay unexpanded).
pub fn closure(
    engine: &Engine,
    s0: &ItemState,
    disabled: &BTreeSet<(String, String)>,
) -> BTreeSet<String> {
    let mut expanded = BTreeSet::new();
    let mut seen: BTreeSet<String> = BTreeSet::from([s0.key()]);
    let mut queue = VecDeque::from([s0.clone()]);
    while let Some(s) = queue.pop_front() {
        if engine.is_goal(&s) {
            continue;
        }
        if seen.len() > MAX_NODES / 2 {
            break;
        }
        let key = s.key();
        expanded.insert(key.clone());
        for a in engine.actions(&s) {
            let action_key = serde_json::to_string(&a).unwrap_or_default();
            if disabled.contains(&(key.clone(), action_key)) {
                continue;
            }
            for (o, _) in engine.outcomes(&s, &a) {
                if engine.is_goal(&o) {
                    continue;
                }
                if seen.insert(o.key()) {
                    queue.push_back(o);
                }
            }
        }
    }
    expanded
}

// ------------------------------------------------------------------ results

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    Ready,
    Unreachable,
    NeedsInput,
    Error,
}

impl ProcessStatus {
    pub fn name(self) -> &'static str {
        match self {
            ProcessStatus::Ready => "Ready",
            ProcessStatus::Unreachable => "Unreachable",
            ProcessStatus::NeedsInput => "NeedsInput",
            ProcessStatus::Error => "Error",
        }
    }
}

/// One row of an outcome table: the good it satisfies (`None`: nothing priced) and its chance.
#[derive(Clone, Debug, PartialEq)]
pub struct Exit {
    pub good: Option<usize>,
    /// When `good` is `None`: a good without a sell price that this outcome satisfies (a brick
    /// the user has named but not priced; it counts as 0).
    pub unpriced: Option<usize>,
    pub p: f64,
    /// A representative outcome state (universe-described), for "make this brick a good".
    pub state: ItemState,
}

/// One distinct craft along the chosen policy.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub action: Option<Action>,
    pub label: String,
    pub cost: Vec<(String, f64)>,
    pub before: ItemState,
    pub after: ItemState,
}

#[derive(Clone, Debug)]
pub struct ProcessValue {
    pub id: String,
    pub cost: Option<f64>,
    pub sd: Option<f64>,
    pub seconds: Option<f64>,
    pub p_success: Option<f64>,
    pub attempts: Option<f64>,
    pub spend: BTreeMap<String, f64>,
    pub steps: Vec<Step>,
    pub core: Vec<Exit>,
    pub exits: Vec<Exit>,
    pub status: ProcessStatus,
    pub error: Option<String>,
    /// The claims (`rules/poe1-claims.json`) this process's evaluation read, sorted.
    pub claims: Vec<&'static str>,
    /// Craft only: the solved graph, kept so the API can show it as the focus tree.
    pub graph: Option<Rc<(Graph<ItemState, Action>, Solved)>>,
}

impl ProcessValue {
    fn empty(id: &str, status: ProcessStatus, error: Option<String>) -> ProcessValue {
        ProcessValue {
            id: id.to_string(),
            cost: None,
            sd: None,
            seconds: None,
            p_success: None,
            attempts: None,
            spend: BTreeMap::new(),
            steps: Vec::new(),
            core: Vec::new(),
            exits: Vec::new(),
            status,
            error,
            claims: Vec::new(),
            graph: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoodValue {
    pub id: String,
    pub value: Option<f64>,
    pub sd: Option<f64>,
    pub seconds: Option<f64>,
    /// `"buy"` or the chosen process id.
    pub via: Option<String>,
    /// No value, but some process consumes it.
    pub needed: bool,
}

#[derive(Clone, Debug)]
pub struct MarketSolution {
    pub goods: Vec<GoodValue>,
    pub processes: Vec<ProcessValue>,
    pub warnings: Vec<String>,
    /// Craft evaluations answered from the cache (inputs unchanged since the last round).
    pub cache_hits: u32,
}

// ------------------------------------------------------------------ solve

#[derive(Clone, Copy, Debug, PartialEq)]
struct Val {
    value: f64,
    var: f64,
    seconds: f64,
}

struct Solver<'a> {
    b: &'a Bundle,
    m: &'a Market,
    prices: &'a BTreeMap<String, f64>,
    donors: &'a [DonorSpec],
    vals: Vec<Option<Val>>,
    via: Vec<Option<String>>,
    results: Vec<Option<ProcessValue>>,
    cache: HashMap<(usize, u64, u64), ProcessValue>,
    cache_hits: u32,
    sells_hash: u64,
}

/// Value every good of the market.
pub fn solve(
    b: &Bundle,
    m: &Market,
    prices: &BTreeMap<String, f64>,
    donors: &[DonorSpec],
) -> Result<MarketSolution, SolveError> {
    let mut h = DefaultHasher::new();
    for g in &m.goods {
        g.sell.map(f64::to_bits).hash(&mut h);
    }
    let mut s = Solver {
        b,
        m,
        prices,
        donors,
        vals: vec![None; m.goods.len()],
        via: vec![None; m.goods.len()],
        results: vec![None; m.processes.len()],
        cache: HashMap::new(),
        cache_hits: 0,
        sells_hash: h.finish(),
    };
    for comp in s.components() {
        let cyclic = comp.len() > 1 || s.depends_on(comp[0], comp[0]);
        if !cyclic {
            s.settle(comp[0])?;
            continue;
        }
        for g in &comp {
            s.vals[*g] = m.goods[*g].buy.map(|v| Val {
                value: v,
                var: 0.0,
                seconds: BUY_SECONDS,
            });
            s.via[*g] = m.goods[*g].buy.map(|_| "buy".to_string());
        }
        let mut settled = false;
        for _ in 0..MARKET_ROUNDS {
            let mut change: f64 = 0.0;
            for g in &comp {
                let before = s.vals[*g].map(|v| v.value);
                s.settle(*g)?;
                let after = s.vals[*g].map(|v| v.value);
                change = change.max(match (before, after) {
                    (Some(x), Some(y)) => (x - y).abs() / x.abs().max(1.0),
                    (None, None) => 0.0,
                    _ => f64::INFINITY,
                });
            }
            if change < SETTLE_TOL {
                settled = true;
                break;
            }
        }
        if !settled {
            let names: Vec<&str> = comp.iter().map(|g| m.goods[*g].name.as_str()).collect();
            return Err(SolveError::Diverged {
                node: names.join(", "),
                message: format!(
                    "The values of {} feed each other and did not settle; give one of them a buy price.",
                    names.join(", ")
                ),
            });
        }
    }

    let mut used: BTreeSet<usize> = BTreeSet::new();
    for p in 0..m.processes.len() {
        used.extend(m.inputs(p));
    }
    let goods = m
        .goods
        .iter()
        .enumerate()
        .map(|(i, g)| GoodValue {
            id: g.id.clone(),
            value: s.vals[i].map(|v| v.value),
            sd: s.vals[i].map(|v| v.var.max(0.0).sqrt()),
            seconds: s.vals[i].map(|v| v.seconds),
            via: s.via[i].clone(),
            needed: s.vals[i].is_none() && used.contains(&i),
        })
        .collect();
    let processes = s
        .results
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            r.unwrap_or_else(|| {
                ProcessValue::empty(&m.processes[i].id, ProcessStatus::NeedsInput, None)
            })
        })
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    for (pi, r) in processes.iter().enumerate() {
        let n = r.core.iter().filter(|e| e.unpriced.is_some()).count();
        if n > 0 {
            let what = if n == 1 {
                "1 outcome is an unpriced brick".to_string()
            } else {
                format!("{n} outcomes are unpriced bricks")
            };
            warnings.push(format!("{}: {what}, counted at 0", m.processes[pi].id));
        }
    }
    // A good nothing consumes (a brick) needs no value; only the root and inputs do.
    for (i, g) in m.goods.iter().enumerate() {
        if s.vals[i].is_none() && (i == m.root || used.contains(&i)) {
            warnings.push(format!(
                "{} has no buy price and no working process",
                g.name
            ));
        }
    }
    Ok(MarketSolution {
        goods,
        processes,
        warnings,
        cache_hits: s.cache_hits,
    })
}

impl Solver<'_> {
    fn deps(&self, g: usize) -> Vec<usize> {
        let id = &self.m.goods[g].id;
        let mut out = Vec::new();
        for (p, proc_) in self.m.processes.iter().enumerate() {
            if &proc_.output == id {
                out.extend(self.m.inputs(p));
            }
        }
        out
    }

    fn depends_on(&self, g: usize, h: usize) -> bool {
        self.deps(g).contains(&h)
    }

    /// Strongly connected components of good → input-good edges, inputs first (Tarjan).
    fn components(&self) -> Vec<Vec<usize>> {
        struct T {
            index: Vec<Option<usize>>,
            low: Vec<usize>,
            on: Vec<bool>,
            stack: Vec<usize>,
            next: usize,
            out: Vec<Vec<usize>>,
        }
        fn visit(v: usize, adj: &[Vec<usize>], t: &mut T) {
            t.index[v] = Some(t.next);
            t.low[v] = t.next;
            t.next += 1;
            t.stack.push(v);
            t.on[v] = true;
            for &w in &adj[v] {
                match t.index[w] {
                    None => {
                        visit(w, adj, t);
                        t.low[v] = t.low[v].min(t.low[w]);
                    }
                    Some(iw) if t.on[w] => t.low[v] = t.low[v].min(iw),
                    _ => {}
                }
            }
            if Some(t.low[v]) == t.index[v] {
                let mut comp = Vec::new();
                while let Some(w) = t.stack.pop() {
                    t.on[w] = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                comp.sort_unstable();
                t.out.push(comp);
            }
        }
        let n = self.m.goods.len();
        let adj: Vec<Vec<usize>> = (0..n).map(|g| self.deps(g)).collect();
        let mut t = T {
            index: vec![None; n],
            low: vec![0; n],
            on: vec![false; n],
            stack: Vec::new(),
            next: 0,
            out: Vec::new(),
        };
        for v in 0..n {
            if t.index[v].is_none() {
                visit(v, &adj, &mut t);
            }
        }
        t.out
    }

    /// Re-value good `g` from its buy price and every process that makes it.
    fn settle(&mut self, g: usize) -> Result<(), SolveError> {
        let m = self.m;
        let mut best: Option<(Val, String)> = m.goods[g].buy.map(|v| {
            (
                Val {
                    value: v,
                    var: 0.0,
                    seconds: BUY_SECONDS,
                },
                "buy".to_string(),
            )
        });
        for p in 0..m.processes.len() {
            if m.processes[p].output != m.goods[g].id {
                continue;
            }
            let r = self.eval(p)?;
            if let (Some(cost), true) = (r.cost, r.status == ProcessStatus::Ready) {
                if cost < MIN_COST {
                    return Err(money_loop(m, g, &r));
                }
                let val = Val {
                    value: cost,
                    var: r.sd.unwrap_or(0.0).powi(2),
                    seconds: r.seconds.unwrap_or(0.0),
                };
                if best.as_ref().is_none_or(|(v, _)| cost < v.value) {
                    best = Some((val, m.processes[p].id.clone()));
                }
            }
            self.results[p] = Some(r);
        }
        self.vals[g] = best.as_ref().map(|(v, _)| *v);
        self.via[g] = best.map(|(_, id)| id);
        Ok(())
    }

    fn eval(&mut self, p: usize) -> Result<ProcessValue, SolveError> {
        match &self.m.processes[p].kind {
            ProcessKind::Craft { from, .. } => {
                let from_good = from.as_ref().and_then(|f| self.m.good(f));
                if let Some(f) = from_good
                    && self.vals[f].is_none()
                {
                    return Ok(ProcessValue::empty(
                        &self.m.processes[p].id,
                        ProcessStatus::NeedsInput,
                        Some(format!("needs a price for {}", self.m.goods[f].name)),
                    ));
                }
                let from_value = from_good.and_then(|f| self.vals[f]).map(|v| v.value);
                let key = (
                    p,
                    from_value.map_or(u64::MAX, f64::to_bits),
                    self.sells_hash,
                );
                if let Some(hit) = self.cache.get(&key) {
                    self.cache_hits += 1;
                    return Ok(hit.clone());
                }
                let mut r = eval_craft(self.b, self.m, p, from_value, self.prices, self.donors)?;
                let mut claims: Vec<&'static str> = r
                    .steps
                    .iter()
                    .filter_map(|st| st.action.as_ref())
                    .flat_map(|a| registry::action_claims(a).iter().copied())
                    .collect();
                claims.sort_unstable();
                claims.dedup();
                r.claims = claims;
                self.cache.insert(key, r.clone());
                Ok(r)
            }
            ProcessKind::Recombine { a, b } => {
                let (ga, gb) = (self.m.good(a).unwrap_or(0), self.m.good(b).unwrap_or(0));
                let r = eval_recombine(
                    self.b,
                    self.m,
                    p,
                    ga,
                    gb,
                    self.vals[ga],
                    self.vals[gb],
                    self.prices,
                );
                if let Some(e) = r.1 {
                    return Err(e);
                }
                Ok(r.0)
            }
            ProcessKind::Custom { from, chance, .. } => {
                let from_good = from.as_ref().and_then(|f| self.m.good(f));
                if let Some(f) = from_good
                    && self.vals[f].is_none()
                {
                    // The chance is stated, so the edge can show it before the input is priced.
                    let mut r = ProcessValue::empty(
                        &self.m.processes[p].id,
                        ProcessStatus::NeedsInput,
                        Some(format!("needs a price for {}", self.m.goods[f].name)),
                    );
                    r.p_success = Some(*chance);
                    r.attempts = Some(1.0 / *chance);
                    return Ok(r);
                }
                let from_val = from_good.and_then(|f| self.vals[f]);
                Ok(eval_custom(self.b, self.m, p, from_good, from_val))
            }
        }
    }
}

fn money_loop(m: &Market, g: usize, r: &ProcessValue) -> SolveError {
    let brick = r
        .exits
        .iter()
        .filter_map(|e| e.good)
        .filter(|x| *x != g)
        .max_by(|x, y| {
            m.goods[*x]
                .sell
                .unwrap_or(0.0)
                .total_cmp(&m.goods[*y].sell.unwrap_or(0.0))
        })
        .map(|x| m.goods[x].name.clone())
        .unwrap_or_else(|| "a brick".to_string());
    SolveError::Diverged {
        node: m.goods[g].name.clone(),
        message: format!(
            "Making {} pays for itself: {brick} sells for more than its inputs cost (a money loop); lower its sell price.",
            m.goods[g].name
        ),
    }
}

/// Group `(state, p)` outcomes into exit rows: the output good, the best brick good, or none.
fn group_exits(
    b: &Bundle,
    m: &Market,
    output: usize,
    outs: impl IntoIterator<Item = (ItemState, f64, bool)>,
) -> Vec<Exit> {
    let mut rows: Vec<Exit> = Vec::new();
    for (s, p, goal) in outs {
        let good = if goal {
            Some(output)
        } else {
            m.brick_of(b, output, &s)
        };
        let unpriced = match good {
            Some(_) => None,
            None => m.unpriced_of(b, output, &s),
        };
        match rows
            .iter_mut()
            .find(|e| e.good == good && e.unpriced == unpriced)
        {
            Some(e) => e.p += p,
            None => rows.push(Exit {
                good,
                unpriced,
                p,
                state: s,
            }),
        }
    }
    rows.sort_by(|a, b| b.p.total_cmp(&a.p));
    rows
}

fn eval_craft(
    b: &Bundle,
    m: &Market,
    p: usize,
    from_value: Option<f64>,
    prices: &BTreeMap<String, f64>,
    donors: &[DonorSpec],
) -> Result<ProcessValue, SolveError> {
    let id = &m.processes[p].id;
    let setup = m.craft_setup(b, p, from_value);
    let engine = m.engine(b, &setup, donors);
    let input = setup.graph_input(&engine, prices);
    let g = match Graph::build(&engine, &input) {
        Ok(g) => g,
        Err(SolveError::MissingPrice(key)) => {
            return Ok(ProcessValue::empty(
                id,
                ProcessStatus::NeedsInput,
                Some(SolveError::MissingPrice(key).to_string()),
            ));
        }
        Err(e @ SolveError::TooLarge(_)) => {
            return Ok(ProcessValue::empty(
                id,
                ProcessStatus::Error,
                Some(e.to_string()),
            ));
        }
        Err(e) => return Err(e),
    };
    let solved = solver::solve(&g)?;
    let mut r = ProcessValue::empty(id, ProcessStatus::Unreachable, None);
    let Some(moments) = solver::moments(&g, &solved) else {
        r.status = if setup.s0.is_some() {
            ProcessStatus::Unreachable
        } else {
            ProcessStatus::NeedsInput
        };
        r.graph = Some(Rc::new((g, solved)));
        return Ok(r);
    };
    r.status = ProcessStatus::Ready;
    r.cost = Some(moments.expected_cost);
    r.sd = Some(moments.sd_cost);
    r.seconds = Some(moments.expected_seconds);
    r.spend = moments.expected_spend.clone();

    if let Some(s0) = &setup.s0
        && let Some(n0) = g.node(&s0.key())
    {
        // Per input item: absorbed into the goal or sold.
        let (p_goal, sold, steps) = absorption(&g, &solved, n0);
        r.p_success = Some(p_goal);
        r.attempts = Some(steps);
        let mut outs: Vec<(ItemState, f64, bool)> = Vec::new();
        if p_goal > 0.0 {
            outs.push((m.ideal(b, setup.output), p_goal, true));
        }
        for (node, pn) in sold {
            if let Some(s) = &g.nodes[node].state {
                outs.push((s.clone(), pn, false));
            }
        }
        r.exits = group_exits(b, m, setup.output, outs);

        r.steps = walk(&engine, &g, &solved, n0, m.ideal(b, setup.output));
        if let Some(last) = r.steps.last()
            && let Some(a) = &last.action
        {
            let outs = engine.outcomes(&last.before, a).into_iter().map(|(s, pr)| {
                let goal = engine.is_goal(&s);
                (s, pr, goal)
            });
            r.core = group_exits(b, m, setup.output, outs);
        }
    }
    r.graph = Some(Rc::new((g, solved)));
    Ok(r)
}

/// From node `n0` under the solved policy: P(goal), each sold node's probability, and the expected
/// number of craft steps, treating Sell as absorbing.
fn absorption(
    g: &Graph<ItemState, Action>,
    s: &Solved,
    n0: usize,
) -> (f64, Vec<(usize, f64)>, f64) {
    let is_craft =
        |v: usize| s.policy[v].is_some_and(|ei| matches!(g.edges[ei].kind, EdgeKind::Craft { .. }));
    // Transient nodes: reachable from n0 through craft edges.
    let mut transient: Vec<usize> = Vec::new();
    let mut absorbing: Vec<usize> = Vec::new();
    let mut seen = BTreeSet::from([n0]);
    let mut queue = VecDeque::from([n0]);
    while let Some(v) = queue.pop_front() {
        if !is_craft(v) {
            absorbing.push(v);
            continue;
        }
        transient.push(v);
        let ei = s.policy[v].unwrap_or(0);
        for &(o, _) in &g.edges[ei].outcomes {
            if o != GOAL && o != START && seen.insert(o) {
                queue.push_back(o);
            }
        }
    }
    if transient.is_empty() {
        return (0.0, vec![(n0, 1.0)], 0.0);
    }
    let row: HashMap<usize, usize> = transient.iter().enumerate().map(|(i, v)| (*v, i)).collect();
    let col: HashMap<usize, usize> = absorbing
        .iter()
        .enumerate()
        .map(|(i, v)| (*v, i + 1))
        .collect();
    let t = transient.len();
    let mut q = vec![vec![0.0; t]; t];
    let mut r = vec![vec![0.0; absorbing.len() + 1]; t];
    for (i, v) in transient.iter().enumerate() {
        let ei = s.policy[*v].unwrap_or(0);
        for &(o, p) in &g.edges[ei].outcomes {
            if o == GOAL {
                r[i][0] += p;
            } else if let Some(j) = row.get(&o) {
                q[i][*j] += p;
            } else if let Some(j) = col.get(&o) {
                r[i][*j] += p;
            }
        }
    }
    let stats = solver::absorbing_stats(&q, &r, &vec![0.0; t]);
    let i0 = row[&n0];
    let p_goal = stats.absorption[i0][0];
    let sold = absorbing
        .iter()
        .enumerate()
        .map(|(j, v)| (*v, stats.absorption[i0][j + 1]))
        .filter(|(_, p)| *p > 0.0)
        .collect();
    (p_goal, sold, stats.steps[i0])
}

/// Distinct crafts along the chosen policy from `n0`, following each step's most likely non-goal
/// outcome; `after` is the goal item when the step can reach it.
fn walk(
    engine: &Engine,
    g: &Graph<ItemState, Action>,
    s: &Solved,
    n0: usize,
    ideal: ItemState,
) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut v = n0;
    while seen.insert(v) {
        let Some(ei) = s.policy[v] else { break };
        let e = &g.edges[ei];
        let EdgeKind::Craft { action, .. } = &e.kind else {
            break;
        };
        let Some(before) = g.nodes[v].state.clone() else {
            break;
        };
        let reaches_goal = e.outcomes.iter().any(|(o, _)| *o == GOAL);
        let next = e
            .outcomes
            .iter()
            .filter(|(o, _)| *o != GOAL)
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(o, _)| *o);
        let after = if reaches_goal {
            ideal.clone()
        } else {
            next.and_then(|o| g.nodes[o].state.clone())
                .unwrap_or_else(|| before.clone())
        };
        if steps.last().is_none_or(|l| l.label != e.label) {
            steps.push(Step {
                action: Some(action.clone()),
                label: e.label.clone(),
                cost: engine.cost(&before, action),
                before,
                after,
            });
        }
        match next {
            Some(o) => v = o,
            None => break,
        }
    }
    steps
}

#[allow(clippy::too_many_arguments)]
fn eval_recombine(
    b: &Bundle,
    m: &Market,
    p: usize,
    ga: usize,
    gb: usize,
    va: Option<Val>,
    vb: Option<Val>,
    prices: &BTreeMap<String, f64>,
) -> (ProcessValue, Option<SolveError>) {
    let id = &m.processes[p].id;
    let output = m.output(p);
    let x = m.ideal(b, ga);
    let y = m.ideal(b, gb);
    let mut r = ProcessValue::empty(id, ProcessStatus::Ready, None);
    if let Some(why) = kernels::recombine_blocker(b, &x, &y) {
        r.status = ProcessStatus::Error;
        r.error = Some(why.to_string());
        return (r, None);
    }
    let outs = kernels::recombine(b, &m.universe, &x, &y, &m.recombine_rules);
    r.claims = kernels::recombine_claims(b, &m.universe, &x, &y);
    r.core = group_exits(
        b,
        m,
        output,
        outs.iter()
            .map(|(s, pr)| (s.clone(), *pr, m.satisfies(b, output, s))),
    );
    r.exits = r.core.clone();
    let contract = registry::recombine_contract();
    let cost = recombine_cost(x.influences != 0 || y.influences != 0);
    r.steps = vec![Step {
        action: None,
        label: contract.name.to_string(),
        cost: cost.clone(),
        before: x.clone(),
        after: m.ideal(b, output),
    }];
    let p_ok: f64 = r
        .core
        .iter()
        .filter(|e| e.good == Some(output))
        .map(|e| e.p)
        .sum();
    r.p_success = Some(p_ok);
    if p_ok <= 0.0 {
        r.status = ProcessStatus::Unreachable;
        return (r, None);
    }
    r.attempts = Some(1.0 / p_ok);
    let (Some(va), Some(vb)) = (va, vb) else {
        let missing = if va.is_none() { ga } else { gb };
        r.status = ProcessStatus::NeedsInput;
        r.error = Some(format!("needs a price for {}", m.goods[missing].name));
        return (r, None);
    };
    let mut unit = 0.0;
    for (key, qty) in &cost {
        let Some(price) = prices.get(key).copied() else {
            r.status = ProcessStatus::NeedsInput;
            r.error = Some(SolveError::MissingPrice(key.clone()).to_string());
            return (r, None);
        };
        unit += qty * price;
    }
    let mut credit = 0.0;
    let mut credit_sq = 0.0;
    for e in &r.core {
        if e.good == Some(output) {
            continue;
        }
        let brick = e.good.and_then(|g| m.goods[g].sell).unwrap_or(0.0);
        credit += e.p * brick;
        credit_sq += e.p * brick * brick;
    }
    let x_cost = va.value + vb.value + unit;
    let net = x_cost - credit;
    if net <= 0.0 {
        return (r.clone(), Some(money_loop(m, output, &r)));
    }
    let ek = 1.0 / p_ok;
    let var_k = (1.0 - p_ok) / (p_ok * p_ok);
    let var_x1 = va.var + vb.var + credit_sq - credit * credit;
    r.cost = Some(net / p_ok);
    r.sd = Some((ek * var_x1 + var_k * net * net).max(0.0).sqrt());
    r.seconds = Some((va.seconds + vb.seconds + contract.seconds) / p_ok);
    r.spend = cost
        .iter()
        .map(|(key, qty)| (key.clone(), qty * ek))
        .collect();
    (r, None)
}

/// A custom craft's closed form: attempts are geometric with the stated chance. A lost input is
/// bought again for every attempt; a kept one once.
fn eval_custom(
    b: &Bundle,
    m: &Market,
    p: usize,
    from_good: Option<usize>,
    from_val: Option<Val>,
) -> ProcessValue {
    let process = &m.processes[p];
    let ProcessKind::Custom {
        name,
        chance,
        cost,
        keeps_input,
        ..
    } = &process.kind
    else {
        unreachable!("eval_custom on a non-custom process");
    };
    let (chance, cost) = (*chance, *cost);
    let output = m.output(p);
    let after = m.ideal(b, output);
    let before = from_good.map_or_else(|| after.clone(), |g| m.ideal(b, g));
    let (vi, vari, si) = from_val.map_or((0.0, 0.0, 0.0), |v| (v.value, v.var, v.seconds));
    let ek = 1.0 / chance;
    let var_k = (1.0 - chance) / (chance * chance);
    let mut r = ProcessValue::empty(&process.id, ProcessStatus::Ready, None);
    if *keeps_input {
        r.cost = Some(vi + cost * ek);
        r.sd = Some((vari + var_k * cost * cost).max(0.0).sqrt());
        r.seconds = Some(si + CUSTOM_SECONDS * ek);
    } else {
        let net = vi + cost;
        r.cost = Some(net * ek);
        r.sd = Some((ek * vari + var_k * net * net).max(0.0).sqrt());
        r.seconds = Some((si + CUSTOM_SECONDS) * ek);
    }
    r.p_success = Some(chance);
    r.attempts = Some(ek);
    r.spend = BTreeMap::from([("chaos".to_string(), cost * ek)]);
    r.steps = vec![Step {
        action: None,
        label: name.clone(),
        cost: vec![("chaos".to_string(), cost)],
        before: before.clone(),
        after: after.clone(),
    }];
    r.core = vec![Exit {
        good: Some(output),
        unpriced: None,
        p: chance,
        state: after,
    }];
    if chance < 1.0 {
        r.core.push(Exit {
            good: None,
            unpriced: None,
            p: 1.0 - chance,
            state: before,
        });
    }
    r.exits = r.core.clone();
    r
}

/// A side's requirement indices, for splitting a good in two.
pub fn side_requirements(t: &Target, side: Side) -> Vec<usize> {
    t.requirements
        .iter()
        .enumerate()
        .filter(|(_, r)| r.side == side)
        .map(|(i, _)| i)
        .collect()
}
