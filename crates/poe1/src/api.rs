//! The wire contract with the web UI (plan Step 8, contract §2–3): name-based `Problem` in,
//! `Solution` out, plus the tooltip parser and the donor mod list in their JSON forms.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use solver::graph::{EdgeKind, Graph, GraphInput, START};
use solver::{ProposalStatus, SolveError};

use crate::data::Bundle;
use crate::engine::Engine;
use crate::item::{INFLUENCE_NAMES, ItemState, Rarity, Side};
use crate::kernels::Action;
use crate::registry::DonorSpec;
use crate::target::{self, Requirement, Target};
use crate::text::{self, TargetDraft};
use crate::weights::spawn_weight;

/// Version of the `Problem`/`Solution` shapes.
pub const VERSION: u32 = 1;
/// Extra chaos charged on every Buy/Sell edge, so an indifferent trade loop still converges.
pub const TRADE_PENALTY: f64 = 0.01;
/// Warning added when `start` is outside the reachability set.
pub const NO_BUY_WARNING: &str =
    "No buy price reaches the goal yet; set a buy price on the goal or on a leaf.";

fn default_runs() -> u32 {
    20_000
}

// ------------------------------------------------------------------ Problem

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementJson {
    pub group: String,
    /// `"Prefix"` or `"Suffix"`.
    pub side: String,
    pub accepted: Vec<String>,
    pub min_value: f64,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalystRef {
    pub name: String,
    pub quality: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetJson {
    pub base: String,
    pub ilvl_min: u8,
    pub influences: Vec<String>,
    /// `"Normal"`, `"Magic"` or `"Rare"`.
    pub rarity: String,
    pub catalyst: Option<CatalystRef>,
    pub min_hits: u16,
    pub requirements: Vec<RequirementJson>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DonorJson {
    pub label: String,
    pub base: String,
    pub ilvl: u8,
    pub influence: String,
    #[serde(rename = "mod")]
    pub mod_id: String,
    pub price_chaos: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub version: u32,
    #[serde(default)]
    pub bundle_patch: String,
    pub target: TargetJson,
    #[serde(default)]
    pub prices: BTreeMap<String, f64>,
    #[serde(default)]
    pub donors: Vec<DonorJson>,
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub expanded: Vec<String>,
    #[serde(default)]
    pub disabled: Vec<(String, String)>,
    #[serde(default)]
    pub buy: Vec<(String, f64)>,
    #[serde(default)]
    pub sell: Vec<(String, f64)>,
    #[serde(default = "default_runs")]
    pub runs: u32,
    #[serde(default)]
    pub seed: u64,
}

// ------------------------------------------------------------------ Solution

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub kind: String,
    pub message: String,
}

impl ApiError {
    fn parse(message: impl Into<String>) -> ApiError {
        ApiError {
            kind: "Parse".to_string(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chip {
    /// `"wanted"`, `"blocked"`, `"other"` or `"otherInfluenced"`.
    pub kind: String,
    pub text: String,
    pub ok: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeView {
    pub title: String,
    pub influences: Vec<String>,
    pub catalyst: Option<String>,
    pub lock: Option<String>,
    pub prefixes: Vec<Chip>,
    pub suffixes: Vec<Chip>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionNode {
    pub key: String,
    /// `"start"`, `"goal"` or `"state"`.
    pub kind: String,
    pub state: Option<ItemState>,
    pub view: Option<NodeView>,
    pub value: Option<f64>,
    pub expanded: bool,
    pub in_r: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionEdge {
    pub from: String,
    /// `"craft"`, `"buy"` or `"sell"`.
    pub kind: String,
    pub action: Option<Action>,
    pub action_key: Option<String>,
    pub label: String,
    pub cost: f64,
    pub seconds: f64,
    pub outcomes: Vec<(String, f64)>,
    pub q: Option<f64>,
    pub chosen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Q4Json {
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuantilesJson {
    pub cost: Q4Json,
    pub seconds: Q4Json,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub expected_cost: f64,
    pub sd_cost: f64,
    pub expected_seconds: f64,
    /// Craft line items: price key -> expected quantity.
    pub expected_spend: BTreeMap<String, f64>,
    pub expected_buy_chaos: f64,
    pub expected_sell_chaos: f64,
    pub quantiles: Option<QuantilesJson>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalJson {
    pub action: Action,
    pub action_key: String,
    pub label: String,
    pub pre: String,
    pub p_reach: f64,
    pub p_mods: f64,
    pub cost: f64,
    pub q: Option<f64>,
    /// `"Ready"`, `"NeedsInput"` or `"Disabled"`.
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub version: u32,
    pub error: Option<ApiError>,
    pub warnings: Vec<String>,
    pub nodes: Vec<SolutionNode>,
    pub edges: Vec<SolutionEdge>,
    pub stats: Option<Stats>,
    pub proposals: BTreeMap<String, Vec<ProposalJson>>,
}

impl Solution {
    fn failed(error: ApiError, warnings: Vec<String>) -> Solution {
        Solution {
            version: VERSION,
            error: Some(error),
            warnings,
            nodes: Vec::new(),
            edges: Vec::new(),
            stats: None,
            proposals: BTreeMap::new(),
        }
    }
}

// ------------------------------------------------------------------ TargetDraft / donor mods

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuyNote {
    pub amount: f64,
    pub currency: String,
}

/// `Bridge::parse_target` JSON (contract §3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDraftJson {
    pub item_class: Option<String>,
    pub base: Option<String>,
    pub rarity: String,
    pub ilvl: Option<u8>,
    pub influences: Vec<String>,
    pub catalyst: Option<CatalystRef>,
    pub requirements: Vec<RequirementJson>,
    pub min_hits: u16,
    pub buy_note: Option<BuyNote>,
    pub unmatched: Vec<String>,
    pub warnings: Vec<String>,
}

impl TargetDraftJson {
    /// `Problem.target` as the UI builds it from a draft; `None` without a base.
    pub fn target(&self) -> Option<TargetJson> {
        Some(TargetJson {
            base: self.base.clone()?,
            ilvl_min: self.ilvl.unwrap_or(1),
            influences: self.influences.clone(),
            rarity: self.rarity.clone(),
            catalyst: self.catalyst.clone(),
            min_hits: self.min_hits,
            requirements: self.requirements.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InfluenceModJson {
    pub id: String,
    pub text: String,
    /// `"Prefix"` or `"Suffix"`.
    pub side: String,
}

// ------------------------------------------------------------------ name <-> index

/// Influence names for a bit set, in bit order.
pub fn influence_names(bits: u8) -> Vec<String> {
    INFLUENCE_NAMES
        .iter()
        .enumerate()
        .filter(|(i, _)| bits & (1 << i) != 0)
        .map(|(_, n)| n.to_string())
        .collect()
}

fn influence_bit(name: &str) -> Option<u8> {
    INFLUENCE_NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(name))
        .map(|i| 1 << i)
}

fn parse_side(name: &str) -> Option<Side> {
    if name.eq_ignore_ascii_case("prefix") {
        Some(Side::Prefix)
    } else if name.eq_ignore_ascii_case("suffix") {
        Some(Side::Suffix)
    } else {
        None
    }
}

fn parse_rarity(name: &str) -> Option<Rarity> {
    [Rarity::Normal, Rarity::Magic, Rarity::Rare]
        .into_iter()
        .find(|r| r.name().eq_ignore_ascii_case(name))
}

fn group_index(b: &Bundle, name: &str) -> Option<u16> {
    b.groups.iter().position(|g| g == name).map(|i| i as u16)
}

pub fn requirement_json(b: &Bundle, r: &Requirement) -> RequirementJson {
    RequirementJson {
        group: b.group(r.group).to_string(),
        side: r.side.name().to_string(),
        accepted: r.accepted.iter().map(|m| b.mod_at(*m).id.clone()).collect(),
        min_value: r.min_value,
        label: r.label.clone(),
    }
}

fn catalyst_ref(b: &Bundle, catalyst: Option<(u8, u8)>) -> Option<CatalystRef> {
    catalyst.map(|(c, q)| CatalystRef {
        name: b.catalysts[c as usize].name.clone(),
        quality: q,
    })
}

/// A resolved target in its name-based JSON form.
pub fn target_json(b: &Bundle, t: &Target) -> TargetJson {
    TargetJson {
        base: b.base_at(t.base).name.clone(),
        ilvl_min: t.ilvl_min,
        influences: influence_names(t.influences),
        rarity: t.rarity.name().to_string(),
        catalyst: catalyst_ref(b, t.catalyst),
        min_hits: t.min_hits,
        requirements: t
            .requirements
            .iter()
            .map(|r| requirement_json(b, r))
            .collect(),
    }
}

/// Resolve a name-based target; any unknown name is a `Parse` error message.
pub fn resolve_target(b: &Bundle, t: &TargetJson) -> Result<Target, String> {
    let base = b
        .base_by_name(&t.base)
        .ok_or_else(|| format!("Unknown base item \"{}\".", t.base))?;
    let mut influences = 0u8;
    for name in &t.influences {
        influences |=
            influence_bit(name).ok_or_else(|| format!("Unknown influence \"{name}\"."))?;
    }
    let rarity =
        parse_rarity(&t.rarity).ok_or_else(|| format!("Unknown rarity \"{}\".", t.rarity))?;
    let catalyst = match &t.catalyst {
        Some(c) => {
            let index = b
                .catalyst_by_name(&c.name)
                .ok_or_else(|| format!("Unknown catalyst \"{}\".", c.name))?;
            Some((index, c.quality.min(20)))
        }
        None => None,
    };
    let mut requirements = Vec::with_capacity(t.requirements.len());
    for r in &t.requirements {
        let group = group_index(b, &r.group)
            .ok_or_else(|| format!("Unknown modifier group \"{}\".", r.group))?;
        let side = parse_side(&r.side)
            .ok_or_else(|| format!("Unknown side \"{}\" (Prefix or Suffix).", r.side))?;
        let mut accepted = Vec::with_capacity(r.accepted.len());
        for id in &r.accepted {
            accepted.push(
                b.mod_by_id(id)
                    .ok_or_else(|| format!("Unknown modifier \"{id}\"."))?,
            );
        }
        if accepted.is_empty() {
            return Err(format!(
                "Requirement \"{}\" accepts no modifier tier.",
                r.label
            ));
        }
        if !r.min_value.is_finite() {
            return Err(format!("Requirement \"{}\" has no minimum value.", r.label));
        }
        requirements.push(Requirement {
            group,
            side,
            accepted,
            min_value: r.min_value,
            label: r.label.clone(),
        });
    }
    Ok(Target {
        base,
        ilvl_min: t.ilvl_min,
        influences,
        rarity,
        catalyst,
        requirements,
        min_hits: t.min_hits,
    })
}

fn resolve_donors(b: &Bundle, donors: &[DonorJson]) -> Result<Vec<DonorSpec>, String> {
    if donors.len() > u8::MAX as usize {
        return Err("Too many donors.".to_string());
    }
    donors
        .iter()
        .map(|d| {
            Ok(DonorSpec {
                label: d.label.clone(),
                base: b.base_by_name(&d.base).ok_or_else(|| {
                    format!("Donor \"{}\": unknown base \"{}\".", d.label, d.base)
                })?,
                ilvl: d.ilvl,
                influence: influence_bit(&d.influence).ok_or_else(|| {
                    format!(
                        "Donor \"{}\": unknown influence \"{}\".",
                        d.label, d.influence
                    )
                })?,
                mod_index: b.mod_by_id(&d.mod_id).ok_or_else(|| {
                    format!("Donor \"{}\": unknown modifier \"{}\".", d.label, d.mod_id)
                })?,
                price_chaos: d.price_chaos,
            })
        })
        .collect()
}

/// A state key from the UI must index into this bundle and this target.
fn validate_state(b: &Bundle, t: &Target, s: &ItemState) -> Result<(), String> {
    if s.base as usize >= b.bases.len() {
        return Err("it names a base that is not in the game data".to_string());
    }
    if s.influences >= 1 << INFLUENCE_NAMES.len() || s.lock > 0b11 {
        return Err("it has invalid influence or lock bits".to_string());
    }
    if let Some((c, q)) = s.catalyst
        && (c as usize >= b.catalysts.len() || q > 20)
    {
        return Err("it has an invalid catalyst".to_string());
    }
    for side in Side::ALL {
        let ss = s.side(side);
        if ss.untracked_influenced > ss.untracked {
            return Err("it counts more influenced mods than mods".to_string());
        }
        for hit in &ss.wanted {
            let ok = t
                .requirements
                .get(hit.req as usize)
                .is_some_and(|r| (hit.idx as usize) < r.accepted.len());
            if !ok {
                return Err("it refers to a requirement the target no longer has".to_string());
            }
        }
        if ss
            .blocked
            .iter()
            .any(|x| x.group as usize >= b.groups.len())
        {
            return Err("it refers to an unknown modifier group".to_string());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ views

/// Display data for one state (contract §2 `view`).
pub fn node_view(b: &Bundle, t: &Target, s: &ItemState) -> NodeView {
    let chips = |side: Side| -> Vec<Chip> {
        let ss = s.side(side);
        let mut out = Vec::new();
        for hit in &ss.wanted {
            let text = t
                .requirements
                .get(hit.req as usize)
                .map(|r| r.label.clone())
                .unwrap_or_else(|| "requirement".to_string());
            out.push(Chip {
                kind: "wanted".to_string(),
                text,
                ok: hit.value_ok,
            });
        }
        for blocked in &ss.blocked {
            out.push(Chip {
                kind: "blocked".to_string(),
                text: format!("{} (other tier)", b.group(blocked.group)),
                ok: false,
            });
        }
        for _ in 0..ss.untracked.saturating_sub(ss.untracked_influenced) {
            out.push(Chip {
                kind: "other".to_string(),
                text: "other mod".to_string(),
                ok: false,
            });
        }
        for _ in 0..ss.untracked_influenced {
            out.push(Chip {
                kind: "otherInfluenced".to_string(),
                text: "other influenced mod".to_string(),
                ok: false,
            });
        }
        out
    };
    let lock = match s.lock {
        0 => None,
        0b01 => Some("Prefixes Cannot Be Changed".to_string()),
        0b10 => Some("Suffixes Cannot Be Changed".to_string()),
        _ => Some("Prefixes Cannot Be Changed, Suffixes Cannot Be Changed".to_string()),
    };
    NodeView {
        title: format!("{} {}", s.rarity.name(), b.base_at(s.base).name),
        influences: influence_names(s.influences),
        catalyst: s.catalyst.map(|(c, q)| {
            let name = b
                .catalysts
                .get(c as usize)
                .map(|x| x.name.as_str())
                .unwrap_or("Catalyst");
            format!("{name} {q}%")
        }),
        lock,
        prefixes: chips(Side::Prefix),
        suffixes: chips(Side::Suffix),
    }
}

/// One-line human description of a state, for messages.
pub fn describe_state(b: &Bundle, t: &Target, s: &ItemState) -> String {
    let view = node_view(b, t, s);
    let mut parts: Vec<String> = Vec::new();
    if !view.influences.is_empty() {
        parts.push(view.influences.join("/"));
    }
    if let Some(c) = &view.catalyst {
        parts.push(c.clone());
    }
    if let Some(l) = &view.lock {
        parts.push(l.clone());
    }
    let mods: Vec<String> = view
        .prefixes
        .iter()
        .chain(view.suffixes.iter())
        .map(|c| {
            if c.kind == "wanted" && !c.ok {
                format!("{} (low roll)", c.text)
            } else {
                c.text.clone()
            }
        })
        .collect();
    if mods.is_empty() {
        parts.push("no mods".to_string());
    } else {
        parts.push(mods.join(", "));
    }
    format!("{} ({})", view.title, parts.join("; "))
}

fn solve_error(
    b: &Bundle,
    t: &Target,
    g: Option<&Graph<ItemState, Action>>,
    e: SolveError,
) -> ApiError {
    let message = match &e {
        SolveError::Diverged { node, message } => {
            let state = g
                .and_then(|g| g.node(node))
                .and_then(|i| g.and_then(|g| g.nodes[i].state.clone()))
                .or_else(|| serde_json::from_str::<ItemState>(node).ok());
            match state {
                Some(s) if validate_state(b, t, &s).is_ok() => {
                    let desc = describe_state(b, t, &s);
                    if message.contains(node.as_str()) {
                        message.replace(node.as_str(), &desc)
                    } else {
                        format!("{message} (at {desc})")
                    }
                }
                _ => message.clone(),
            }
        }
        other => other.to_string(),
    };
    ApiError {
        kind: e.kind().to_string(),
        message,
    }
}

fn status_name(s: ProposalStatus) -> &'static str {
    match s {
        ProposalStatus::Ready => "Ready",
        ProposalStatus::NeedsInput => "NeedsInput",
        ProposalStatus::Disabled => "Disabled",
    }
}

// ------------------------------------------------------------------ entry points

/// Solve a problem. Errors are reported inside the `Solution`, never as a panic.
pub fn solve(b: &Bundle, p: &Problem) -> Solution {
    let mut warnings = Vec::new();
    match solve_inner(b, p, &mut warnings) {
        Ok(solution) => solution,
        Err(error) => Solution::failed(error, warnings),
    }
}

fn solve_inner(b: &Bundle, p: &Problem, warnings: &mut Vec<String>) -> Result<Solution, ApiError> {
    if p.version != VERSION {
        return Err(ApiError::parse(format!(
            "Problem version {} is not supported (expected {VERSION}).",
            p.version
        )));
    }
    if !p.bundle_patch.is_empty() && p.bundle_patch != b.patch {
        warnings.push(format!(
            "This problem was made for game data {}; the loaded data is {}.",
            p.bundle_patch, b.patch
        ));
    }
    let t = resolve_target(b, &p.target).map_err(ApiError::parse)?;
    let donors = resolve_donors(b, &p.donors).map_err(ApiError::parse)?;
    let engine = Engine {
        b,
        t: &t,
        donors: &donors,
    };

    let mut prices = p.prices.clone();
    for (i, d) in donors.iter().enumerate() {
        prices.insert(format!("donor:{i}"), d.price_chaos);
    }
    let ideal = target::ideal_state(b, &t);
    let mut seeds = vec![ideal.clone()];
    for key in &p.added {
        let state: ItemState = serde_json::from_str(key)
            .map_err(|e| ApiError::parse(format!("Added state {key} is not a state key: {e}.")))?;
        validate_state(b, &t, &state).map_err(|why| {
            ApiError::parse(format!(
                "Added state {key} does not fit this target: {why}."
            ))
        })?;
        seeds.push(state);
    }
    let expanded: BTreeSet<String> = p.expanded.iter().cloned().collect();
    let input = GraphInput {
        seeds,
        expanded: expanded.clone(),
        disabled: p.disabled.iter().cloned().collect(),
        prices,
        buy: p.buy.iter().cloned().collect(),
        sell: p.sell.iter().cloned().collect(),
        trade_penalty: TRADE_PENALTY,
    };

    let g = Graph::build(&engine, &input).map_err(|e| solve_error(b, &t, None, e))?;
    let solved = solver::solve(&g).map_err(|e| solve_error(b, &t, Some(&g), e))?;

    let stats = match solver::moments(&g, &solved) {
        Some(m) => {
            let quantiles = if p.runs == 0 {
                None
            } else {
                solver::quantiles(&g, &solved, p.runs, p.seed)
                    .map_err(|e| solve_error(b, &t, Some(&g), e))?
                    .map(|q| QuantilesJson {
                        cost: Q4Json {
                            p50: q.cost.p50,
                            p90: q.cost.p90,
                            p95: q.cost.p95,
                            p99: q.cost.p99,
                        },
                        seconds: Q4Json {
                            p50: q.seconds.p50,
                            p90: q.seconds.p90,
                            p95: q.seconds.p95,
                            p99: q.seconds.p99,
                        },
                    })
            };
            Some(Stats {
                expected_cost: m.expected_cost,
                sd_cost: m.sd_cost,
                expected_seconds: m.expected_seconds,
                expected_spend: m.expected_spend,
                expected_buy_chaos: m.expected_buy_chaos,
                expected_sell_chaos: m.expected_sell_chaos,
                quantiles,
            })
        }
        None => {
            warnings.push(NO_BUY_WARNING.to_string());
            None
        }
    };

    // Proposals: the goal (through the ideal state) and every expanded state in the graph.
    let to_json = |list: Vec<solver::Proposal<Action, ItemState>>| -> Vec<ProposalJson> {
        list.into_iter()
            .map(|pr| ProposalJson {
                action: pr.action,
                action_key: pr.action_key,
                label: pr.label,
                pre: pr.pre,
                p_reach: pr.p_reach,
                p_mods: pr.p_mods,
                cost: pr.cost,
                q: pr.q,
                status: status_name(pr.status).to_string(),
            })
            .collect()
    };
    let mut proposals: BTreeMap<String, Vec<ProposalJson>> = BTreeMap::new();
    let goal_list = solver::propose(&engine, &g, &solved, "goal", &ideal)
        .map_err(|e| solve_error(b, &t, Some(&g), e))?;
    proposals.insert("goal".to_string(), to_json(goal_list));
    for key in &expanded {
        if key == "goal" || key == "start" {
            continue;
        }
        let Some(state) = g.node(key).and_then(|i| g.nodes[i].state.clone()) else {
            continue;
        };
        let list = solver::propose(&engine, &g, &solved, key, &state)
            .map_err(|e| solve_error(b, &t, Some(&g), e))?;
        proposals.insert(key.clone(), to_json(list));
    }

    let nodes: Vec<SolutionNode> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let kind = match i {
                START => "start",
                solver::GOAL => "goal",
                _ => "state",
            };
            SolutionNode {
                key: n.key.clone(),
                kind: kind.to_string(),
                state: n.state.clone(),
                view: n.state.as_ref().map(|s| node_view(b, &t, s)),
                value: solved.value[i],
                expanded: if i == solver::GOAL { true } else { n.expanded },
                in_r: solved.value[i].is_some(),
            }
        })
        .collect();

    let edges: Vec<SolutionEdge> = g
        .edges
        .iter()
        .enumerate()
        .map(|(ei, e)| {
            let mut q = Some(e.cost);
            for (o, prob) in &e.outcomes {
                match (q, solved.value[*o]) {
                    (Some(acc), Some(v)) => q = Some(acc + prob * v),
                    _ => q = None,
                }
            }
            let (kind, action, action_key) = match &e.kind {
                EdgeKind::Craft { action, action_key } => {
                    ("craft", Some(action.clone()), Some(action_key.clone()))
                }
                EdgeKind::Buy => ("buy", None, None),
                EdgeKind::Sell => ("sell", None, None),
            };
            SolutionEdge {
                from: g.nodes[e.from].key.clone(),
                kind: kind.to_string(),
                action,
                action_key,
                label: e.label.clone(),
                cost: e.cost,
                seconds: e.seconds,
                outcomes: e
                    .outcomes
                    .iter()
                    .map(|(o, prob)| (g.nodes[*o].key.clone(), *prob))
                    .collect(),
                q,
                chosen: solved.policy[e.from] == Some(ei),
            }
        })
        .collect();

    Ok(Solution {
        version: VERSION,
        error: None,
        warnings: std::mem::take(warnings),
        nodes,
        edges,
        stats,
        proposals,
    })
}

/// The tooltip parser in its JSON form (contract §3).
pub fn draft_json(b: &Bundle, d: &TargetDraft) -> TargetDraftJson {
    TargetDraftJson {
        item_class: d.item_class.clone(),
        base: d.base.map(|i| b.base_at(i).name.clone()),
        rarity: d.rarity.name().to_string(),
        ilvl: d.ilvl,
        influences: influence_names(d.influences),
        catalyst: catalyst_ref(b, d.catalyst),
        requirements: d
            .requirements
            .iter()
            .map(|r| requirement_json(b, r))
            .collect(),
        min_hits: d.min_hits,
        buy_note: d.buy_note.as_ref().map(|(amount, currency)| BuyNote {
            amount: *amount,
            currency: currency.clone(),
        }),
        unmatched: d.unmatched.clone(),
        warnings: d.warnings.clone(),
    }
}

/// Parse pasted tooltip text into the name-based draft.
pub fn parse_target_api(b: &Bundle, text: &str) -> TargetDraftJson {
    draft_json(b, &text::parse_target(b, text))
}

/// Influence mods that can spawn on `base` carrying exactly the one `influence`.
pub fn influence_mods(
    b: &Bundle,
    base: &str,
    influence: &str,
) -> Result<Vec<InfluenceModJson>, String> {
    let base_index = b
        .base_by_name(base)
        .ok_or_else(|| format!("Unknown base item \"{base}\"."))?;
    let bit =
        influence_bit(influence).ok_or_else(|| format!("Unknown influence \"{influence}\"."))?;
    let tags = b.item_tags(base_index, bit);
    let mut out: Vec<InfluenceModJson> = b
        .mods
        .iter()
        .filter(|m| !m.crafted && m.influence && spawn_weight(m, &tags) > 0)
        .map(|m| InfluenceModJson {
            id: m.id.clone(),
            text: m.text.clone(),
            side: m.generation.name().to_string(),
        })
        .collect();
    out.sort_by(|a, b| {
        a.side
            .cmp(&b.side)
            .then_with(|| a.text.cmp(&b.text))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}
