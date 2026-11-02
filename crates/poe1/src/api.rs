//! The wire contract with the web UI (plan Step 8, contract §2–3): name-based `Problem` in,
//! `Solution` out, plus the tooltip parser and the donor mod list in their JSON forms.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use solver::graph::{EdgeKind, Graph, START};
use solver::{ProposalStatus, SolveError};

use crate::data::Bundle;
use crate::engine::Engine;
use crate::item::{INFLUENCE_NAMES, ItemState, Rarity, Side};
use crate::kernels::Action;
use crate::market::{self, CraftPlan, Good, Market, Process, ProcessKind};
use crate::recipe;
use crate::registry::{self, DonorSpec};
use crate::target::{self, Requirement, Target};
use crate::text::{self, TargetDraft};
use crate::weights::{ModSource, SourceCtx, spawn_weight};

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
    /// How each mod can reach the item, in source order (drafts only; ignored by the solver).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
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
        sources: None,
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
                .map(|r| hit_text(b, r, hit, s))
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
                text: blocked_text(b, t, side, blocked.group, s),
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

/// Byte range of the first number in `s`.
fn first_number_span(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|c| c.is_ascii_digit())?;
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        end += 1;
    }
    Some((start, end))
}

fn fmt_value(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// The requirement label with its number replaced by the displayed values a roll of this tier
/// takes on this item (its own base and catalyst) that match the hit's `value_ok`.
fn hit_text(b: &Bundle, r: &Requirement, hit: &crate::item::Hit, s: &ItemState) -> String {
    let Some(m) = r.accepted.get(hit.idx as usize).copied() else {
        return r.label.clone();
    };
    let Some(stat) = b.mod_at(m).stats.first() else {
        return r.label.clone();
    };
    let eff = target::effect(b, s.base, s.catalyst, m);
    let (lo, hi) = (stat.min.floor() as i64, stat.max.floor() as i64);
    let shown: Vec<f64> = (lo..=hi.max(lo))
        .map(|roll| target::display(roll as f64, eff))
        .filter(|v| (*v >= r.min_value) == hit.value_ok)
        .collect();
    let (Some(min), Some(max)) = (
        shown.iter().copied().reduce(f64::min),
        shown.iter().copied().reduce(f64::max),
    ) else {
        return r.label.clone();
    };
    let range = if min == max {
        fmt_value(min)
    } else {
        format!("{}–{}", fmt_value(min), fmt_value(max))
    };
    match first_number_span(&r.label) {
        Some((a, z)) => format!("{}{range}{}", &r.label[..a], &r.label[z..]),
        None => r.label.clone(),
    }
}

/// `not wanted: <tier texts>`: the tiers of the group that can roll here and are not accepted.
fn blocked_text(b: &Bundle, t: &Target, side: Side, group: u16, s: &ItemState) -> String {
    let accepted: Vec<u32> = t
        .requirements
        .iter()
        .filter(|r| r.side == side && r.group == group)
        .flat_map(|r| r.accepted.iter().copied())
        .collect();
    let tags = b.item_tags(s.base, s.influences);
    let mut texts: Vec<String> = Vec::new();
    for (i, m) in b.mods.iter().enumerate() {
        if m.group != group
            || m.generation != side
            || accepted.contains(&(i as u32))
            || spawn_weight(m, &tags) == 0
        {
            continue;
        }
        let text = m.text.replace('\n', ", ");
        if !texts.contains(&text) {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        return format!("not wanted: {} (other tier)", b.group(group));
    }
    if texts.len() > 4 {
        texts.truncate(4);
        texts.push("…".to_string());
    }
    format!("not wanted: {}", texts.join(" or "))
}

/// Per-step change lines between two views: removed and added mods, lock and rarity changes.
pub fn changes(before: &NodeView, after: &NodeView) -> Vec<String> {
    let mut out = Vec::new();
    let rarity = |v: &NodeView| v.title.split(' ').next().unwrap_or("").to_string();
    if rarity(before) != rarity(after) {
        out.push(format!("becomes {}", rarity(after)));
    }
    match (&before.lock, &after.lock) {
        (None, Some(l)) => out.push(format!("adds {l}")),
        (Some(l), None) => out.push(format!("removes {l}")),
        _ => {}
    }
    let texts = |v: &NodeView| -> Vec<String> {
        v.prefixes
            .iter()
            .chain(v.suffixes.iter())
            .map(|c| c.text.clone())
            .collect()
    };
    let mut gone = texts(before);
    let mut new = texts(after);
    let mut i = 0;
    while i < gone.len() {
        if let Some(j) = new.iter().position(|x| *x == gone[i]) {
            new.remove(j);
            gone.remove(i);
        } else {
            i += 1;
        }
    }
    out.extend(gone.into_iter().map(|t| format!("− {t}")));
    out.extend(new.into_iter().map(|t| format!("+ {t}")));
    out
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

/// Solve a v1 problem: it is upgraded to a one-good market and its process's tree is returned in
/// the v1 shape. Errors are reported inside the `Solution`, never as a panic.
pub fn solve(b: &Bundle, p: &Problem) -> Solution {
    let mut warnings = Vec::new();
    match solve_v1(b, p, &mut warnings) {
        Ok(solution) => solution,
        Err(error) => Solution::failed(error, warnings),
    }
}

fn check_version(
    b: &Bundle,
    version: u32,
    want: u32,
    patch: &str,
    warnings: &mut Vec<String>,
) -> Result<(), ApiError> {
    if version != want {
        return Err(ApiError::parse(format!(
            "Problem version {version} is not supported (expected {want})."
        )));
    }
    if !patch.is_empty() && patch != b.patch {
        warnings.push(format!(
            "This problem was made for game data {patch}; the loaded data is {}.",
            b.patch
        ));
    }
    Ok(())
}

fn solve_v1(b: &Bundle, p: &Problem, warnings: &mut Vec<String>) -> Result<Solution, ApiError> {
    check_version(b, p.version, VERSION, &p.bundle_patch, warnings)?;
    let v3 = upgrade_v1(p);
    let (m, donors) = build_market(b, &v3)?;
    let values = market::solve(b, &m, &prices_with_donors(&v3.prices, &donors), &donors)
        .map_err(|e| solve_error(b, &m.universe, None, e))?;
    let mut sol = focus_solution(b, &m, 0, &values, &v3, &donors, false, warnings)?;
    sol.version = VERSION;
    Ok(sol)
}

fn prices_with_donors(
    prices: &BTreeMap<String, f64>,
    donors: &[DonorSpec],
) -> BTreeMap<String, f64> {
    let mut prices = prices.clone();
    for (i, d) in donors.iter().enumerate() {
        prices.insert(format!("donor:{i}"), d.price_chaos);
    }
    prices
}

/// The solved tree of one `Craft` process, in the v1 `Solution` shape. `goal_view` adds the
/// finished item's view to the `goal` node (v3 only; v1 keeps it null).
#[allow(clippy::too_many_arguments)]
fn focus_solution(
    b: &Bundle,
    m: &Market,
    pi: usize,
    values: &market::MarketSolution,
    p: &ProblemV3,
    donors: &[DonorSpec],
    goal_view: bool,
    warnings: &mut Vec<String>,
) -> Result<Solution, ApiError> {
    let from_value = match &m.processes[pi].kind {
        ProcessKind::Craft { from: Some(f), .. } => m.good(f).and_then(|g| values.goods[g].value),
        _ => None,
    };
    let setup = m.craft_setup(b, pi, from_value);
    let engine = m.engine(b, &setup, donors);
    let t = &m.universe;
    let prices = prices_with_donors(&p.prices, donors);
    let input = setup.graph_input(&engine, &prices);
    let expanded = input.expanded.clone();
    let g = Graph::build(&engine, &input).map_err(|e| solve_error(b, t, None, e))?;
    let solved = solver::solve(&g).map_err(|e| solve_error(b, t, Some(&g), e))?;
    let ideal = m.ideal(b, setup.output);
    tree_solution(
        b, &engine, t, &ideal, &g, &solved, &expanded, p.runs, p.seed, goal_view, warnings,
    )
}

#[allow(clippy::too_many_arguments)]
fn tree_solution(
    b: &Bundle,
    engine: &Engine,
    t: &Target,
    ideal: &ItemState,
    g: &Graph<ItemState, Action>,
    solved: &solver::Solved,
    expanded: &BTreeSet<String>,
    runs: u32,
    seed: u64,
    goal_view: bool,
    warnings: &mut Vec<String>,
) -> Result<Solution, ApiError> {
    let p_runs = runs;
    let p_seed = seed;
    let stats = match solver::moments(g, solved) {
        Some(m) => {
            let quantiles = if p_runs == 0 {
                None
            } else {
                solver::quantiles(g, solved, p_runs, p_seed)
                    .map_err(|e| solve_error(b, t, Some(g), e))?
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
    let goal_list = solver::propose(engine, g, solved, "goal", ideal)
        .map_err(|e| solve_error(b, t, Some(g), e))?;
    proposals.insert("goal".to_string(), to_json(goal_list));
    for key in expanded {
        if key == "goal" || key == "start" {
            continue;
        }
        let Some(state) = g.node(key).and_then(|i| g.nodes[i].state.clone()) else {
            continue;
        };
        let list = solver::propose(engine, g, solved, key, &state)
            .map_err(|e| solve_error(b, t, Some(g), e))?;
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
                view: match (&n.state, i == solver::GOAL && goal_view) {
                    (Some(s), _) => Some(node_view(b, t, s)),
                    (None, true) => Some(node_view(b, t, ideal)),
                    (None, false) => None,
                },
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

// ------------------------------------------------------------------ v3: the market of goods

/// Version of the market `ProblemV3`/`SolutionV3` shapes.
pub const VERSION_V3: u32 = 3;

/// The crafts a v1 problem could use; an upgraded v1 process keeps exactly these.
pub const V1_CRAFTS: [&str; 10] = [
    "chaos",
    "exalted",
    "influence-exalt",
    "annul",
    "divine",
    "harvest-influenced",
    "harvest-randomise-influence",
    "bench-lock",
    "apply-catalyst",
    "awakener",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodJson {
    pub id: String,
    pub name: String,
    pub target: TargetJson,
    pub buy: Option<f64>,
    pub sell: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessKindJson {
    Craft {
        from: Option<String>,
        #[serde(default)]
        allowed: Vec<String>,
    },
    Recombine {
        a: String,
        b: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessJson {
    pub id: String,
    pub output: String,
    #[serde(default)]
    pub recipe: Option<String>,
    pub kind: ProcessKindJson,
    #[serde(default)]
    pub expanded: Option<Vec<String>>,
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub disabled: Vec<(String, String)>,
    #[serde(default)]
    pub buy: Vec<(String, f64)>,
    #[serde(default)]
    pub sell: Vec<(String, f64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemV3 {
    pub version: u32,
    #[serde(default)]
    pub bundle_patch: String,
    pub goods: Vec<GoodJson>,
    #[serde(default)]
    pub processes: Vec<ProcessJson>,
    pub root: String,
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub prices: BTreeMap<String, f64>,
    #[serde(default)]
    pub donors: Vec<DonorJson>,
    #[serde(default = "default_runs")]
    pub runs: u32,
    #[serde(default)]
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodValueJson {
    pub id: String,
    pub name: String,
    pub value: Option<f64>,
    pub sd: Option<f64>,
    pub seconds: Option<f64>,
    pub via: Option<String>,
    pub needed: bool,
    pub view: NodeView,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepJson {
    pub label: String,
    pub cost: Vec<(String, f64)>,
    pub changes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitJson {
    pub good: Option<String>,
    pub p: f64,
    pub label: String,
    /// The outcome as a good's target ("make this brick a good").
    pub target: TargetJson,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessValueJson {
    pub id: String,
    pub output: String,
    pub recipe: Option<String>,
    pub kind: String,
    pub inputs: Vec<String>,
    pub name: String,
    pub cost: Option<f64>,
    pub sd: Option<f64>,
    pub seconds: Option<f64>,
    pub p_success: Option<f64>,
    pub attempts: Option<f64>,
    pub spend: BTreeMap<String, f64>,
    pub steps: Vec<StepJson>,
    pub core: Vec<ExitJson>,
    pub exits: Vec<ExitJson>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessProposalJson {
    pub process: ProcessJson,
    pub new_goods: Vec<GoodJson>,
    pub name: String,
    pub cost: Option<f64>,
    pub p_success: Option<f64>,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketJson {
    pub goods: Vec<GoodValueJson>,
    pub processes: Vec<ProcessValueJson>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionV3 {
    pub version: u32,
    pub error: Option<ApiError>,
    pub warnings: Vec<String>,
    pub market: MarketJson,
    pub focus: Option<Solution>,
    pub proposals: BTreeMap<String, Vec<ProcessProposalJson>>,
    /// Craft evaluations answered from the market cache (perf diagnostics).
    #[serde(default)]
    pub cache_hits: u32,
}

impl SolutionV3 {
    fn failed(error: ApiError, warnings: Vec<String>) -> SolutionV3 {
        SolutionV3 {
            version: VERSION_V3,
            error: Some(error),
            warnings,
            market: MarketJson {
                goods: Vec::new(),
                processes: Vec::new(),
            },
            focus: None,
            proposals: BTreeMap::new(),
            cache_hits: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractJson {
    pub id: String,
    pub name: String,
    pub requires: String,
    pub effect: String,
    pub cost: Vec<(String, f64)>,
    pub seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierJson {
    pub id: String,
    pub text: String,
    pub ilvl: u16,
    pub min: f64,
    pub max: f64,
}

/// A v1 problem as a one-good market: the target (no buy price of its own) made by one v1-style
/// `Craft` process that keeps the v1 tree, prices and craft set.
pub fn upgrade_v1(p: &Problem) -> ProblemV3 {
    ProblemV3 {
        version: VERSION_V3,
        bundle_patch: p.bundle_patch.clone(),
        goods: vec![GoodJson {
            id: "main".to_string(),
            name: "Target".to_string(),
            target: p.target.clone(),
            buy: None,
            sell: None,
        }],
        processes: vec![ProcessJson {
            id: "p1".to_string(),
            output: "main".to_string(),
            recipe: None,
            kind: ProcessKindJson::Craft {
                from: None,
                allowed: V1_CRAFTS.iter().map(|s| s.to_string()).collect(),
            },
            expanded: Some(p.expanded.clone()),
            added: p.added.clone(),
            disabled: p.disabled.clone(),
            buy: p.buy.clone(),
            sell: p.sell.clone(),
        }],
        root: "main".to_string(),
        focus: Some("p1".to_string()),
        prices: p.prices.clone(),
        donors: p.donors.clone(),
        runs: p.runs,
        seed: p.seed,
    }
}

fn good_from_json(b: &Bundle, g: &GoodJson) -> Result<Good, String> {
    Ok(Good {
        id: g.id.clone(),
        name: g.name.clone(),
        spec: resolve_target(b, &g.target).map_err(|e| format!("{}: {e}", g.name))?,
        buy: g.buy,
        sell: g.sell,
    })
}

fn good_json(b: &Bundle, g: &Good) -> GoodJson {
    GoodJson {
        id: g.id.clone(),
        name: g.name.clone(),
        target: target_json(b, &g.spec),
        buy: g.buy,
        sell: g.sell,
    }
}

fn process_json(p: &Process) -> ProcessJson {
    let mut out = ProcessJson {
        id: p.id.clone(),
        output: p.output.clone(),
        recipe: p.recipe.clone(),
        kind: ProcessKindJson::Recombine {
            a: String::new(),
            b: String::new(),
        },
        expanded: None,
        added: Vec::new(),
        disabled: Vec::new(),
        buy: Vec::new(),
        sell: Vec::new(),
    };
    match &p.kind {
        ProcessKind::Craft {
            from,
            allowed,
            plan,
        } => {
            out.kind = ProcessKindJson::Craft {
                from: from.clone(),
                allowed: allowed.clone(),
            };
            out.expanded = plan.expanded.as_ref().map(|s| s.iter().cloned().collect());
            out.added = plan.added.iter().map(|s| s.key()).collect();
            out.disabled = plan.disabled.iter().cloned().collect();
            out.buy = plan.buy.iter().map(|(k, v)| (k.clone(), *v)).collect();
            out.sell = plan.sell.iter().map(|(k, v)| (k.clone(), *v)).collect();
        }
        ProcessKind::Recombine { a, b } => {
            out.kind = ProcessKindJson::Recombine {
                a: a.clone(),
                b: b.clone(),
            }
        }
    }
    out
}

/// Resolve a v3 problem into a validated market and its donors.
fn build_market(b: &Bundle, p: &ProblemV3) -> Result<(Market, Vec<DonorSpec>), ApiError> {
    let goods = p
        .goods
        .iter()
        .map(|g| good_from_json(b, g))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::parse)?;
    let donors = resolve_donors(b, &p.donors).map_err(ApiError::parse)?;
    let mut processes = Vec::with_capacity(p.processes.len());
    for pj in &p.processes {
        let kind = match &pj.kind {
            ProcessKindJson::Craft { from, allowed } => {
                for a in allowed {
                    if registry::contract_by_id(a).is_none() {
                        return Err(ApiError::parse(format!(
                            "Process {} allows the unknown craft \"{a}\".",
                            pj.id
                        )));
                    }
                }
                let mut added = Vec::with_capacity(pj.added.len());
                for key in &pj.added {
                    let state: ItemState = serde_json::from_str(key).map_err(|e| {
                        ApiError::parse(format!("Added state {key} is not a state key: {e}."))
                    })?;
                    added.push(state);
                }
                ProcessKind::Craft {
                    from: from.clone(),
                    allowed: allowed.clone(),
                    plan: CraftPlan {
                        expanded: pj.expanded.as_ref().map(|v| v.iter().cloned().collect()),
                        added,
                        disabled: pj.disabled.iter().cloned().collect(),
                        buy: pj.buy.iter().cloned().collect(),
                        sell: pj.sell.iter().cloned().collect(),
                    },
                }
            }
            ProcessKindJson::Recombine { a, b } => ProcessKind::Recombine {
                a: a.clone(),
                b: b.clone(),
            },
        };
        processes.push(Process {
            id: pj.id.clone(),
            output: pj.output.clone(),
            kind,
            recipe: pj.recipe.clone(),
        });
    }
    let m = Market::new(goods, processes, &p.root).map_err(ApiError::parse)?;
    for proc_ in &m.processes {
        if let ProcessKind::Craft { plan, .. } = &proc_.kind {
            for s in &plan.added {
                validate_state(b, &m.universe, s).map_err(|why| {
                    ApiError::parse(format!(
                        "Added state {} does not fit this market: {why}.",
                        s.key()
                    ))
                })?;
            }
        }
    }
    Ok((m, donors))
}

/// A universe-described outcome as a good's target: its base, ilvl, rarity, influences and the
/// tracked mods it holds.
fn exit_target(b: &Bundle, m: &Market, s: &ItemState) -> TargetJson {
    let mut requirements = Vec::new();
    for side in Side::ALL {
        for hit in &s.side(side).wanted {
            if let Some(r) = m.universe.requirements.get(hit.req as usize) {
                requirements.push(requirement_json(b, r));
            }
        }
    }
    TargetJson {
        base: b.base_at(s.base).name.clone(),
        ilvl_min: s.ilvl,
        influences: influence_names(s.influences),
        rarity: s.rarity.name().to_string(),
        catalyst: None,
        min_hits: requirements.len() as u16,
        requirements,
    }
}

fn exit_json(b: &Bundle, m: &Market, e: &market::Exit) -> ExitJson {
    ExitJson {
        good: e.good.map(|g| m.goods[g].id.clone()),
        p: e.p,
        label: match (e.good, e.unpriced) {
            (Some(g), _) => m.goods[g].name.clone(),
            (None, Some(g)) => format!("{} (unpriced)", m.goods[g].name),
            (None, None) => "other outcome".to_string(),
        },
        target: exit_target(b, m, &e.state),
    }
}

fn process_value_json(
    b: &Bundle,
    m: &Market,
    pi: usize,
    v: &market::ProcessValue,
) -> ProcessValueJson {
    let p = &m.processes[pi];
    let t = &m.universe;
    let recipe_name = p.recipe.as_ref().and_then(|id| {
        recipe::recipes()
            .iter()
            .find(|r| &r.id == id)
            .map(|r| r.name.clone())
    });
    let (kind, default_name) = match &p.kind {
        ProcessKind::Craft { .. } => ("craft", "Craft"),
        ProcessKind::Recombine { .. } => ("recombine", "Recombine"),
    };
    ProcessValueJson {
        id: p.id.clone(),
        output: p.output.clone(),
        recipe: p.recipe.clone(),
        kind: kind.to_string(),
        inputs: m
            .inputs(pi)
            .iter()
            .map(|g| m.goods[*g].id.clone())
            .collect(),
        name: recipe_name.unwrap_or_else(|| default_name.to_string()),
        cost: v.cost,
        sd: v.sd,
        seconds: v.seconds,
        p_success: v.p_success,
        attempts: v.attempts,
        spend: v.spend.clone(),
        steps: v
            .steps
            .iter()
            .map(|st| StepJson {
                label: st.label.clone(),
                cost: st.cost.clone(),
                changes: changes(&node_view(b, t, &st.before), &node_view(b, t, &st.after)),
            })
            .collect(),
        core: v.core.iter().map(|e| exit_json(b, m, e)).collect(),
        exits: v.exits.iter().map(|e| exit_json(b, m, e)).collect(),
        status: v.status.name().to_string(),
        error: v.error.clone(),
    }
}

fn proposal_json(b: &Bundle, pr: &recipe::Proposal) -> ProcessProposalJson {
    ProcessProposalJson {
        process: process_json(&pr.process),
        new_goods: pr.new_goods.iter().map(|g| good_json(b, g)).collect(),
        name: pr.name.clone(),
        cost: pr.cost,
        p_success: pr.p_success,
        status: pr.status.name().to_string(),
        reason: pr.reason.clone(),
    }
}

/// Solve a v3 market problem. Errors are reported inside the `SolutionV3`, never as a panic.
pub fn solve_market(b: &Bundle, p: &ProblemV3) -> SolutionV3 {
    let mut warnings = Vec::new();
    match solve_market_inner(b, p, &mut warnings) {
        Ok(s) => s,
        Err(e) => SolutionV3::failed(e, warnings),
    }
}

/// Solve a v1 or v3 problem given as JSON; always answers in the v3 shape.
pub fn solve_json(b: &Bundle, problem_json: &str) -> Result<SolutionV3, String> {
    let v: serde_json::Value = serde_json::from_str(problem_json).map_err(|e| e.to_string())?;
    let version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(0);
    let p3 = if version == VERSION as u64 {
        let p1: Problem = serde_json::from_value(v).map_err(|e| e.to_string())?;
        upgrade_v1(&p1)
    } else {
        serde_json::from_value(v).map_err(|e| e.to_string())?
    };
    Ok(solve_market(b, &p3))
}

fn solve_market_inner(
    b: &Bundle,
    p: &ProblemV3,
    warnings: &mut Vec<String>,
) -> Result<SolutionV3, ApiError> {
    check_version(b, p.version, VERSION_V3, &p.bundle_patch, warnings)?;
    let (m, donors) = build_market(b, p)?;
    let prices = prices_with_donors(&p.prices, &donors);
    let values =
        market::solve(b, &m, &prices, &donors).map_err(|e| solve_error(b, &m.universe, None, e))?;
    warnings.extend(values.warnings.iter().cloned());
    for g in &m.goods {
        warnings.extend(requirement_warnings(b, &g.spec));
    }
    warnings.dedup();

    let focus = match p.focus.as_deref().and_then(|id| m.process(id)) {
        Some(pi) if matches!(m.processes[pi].kind, ProcessKind::Craft { .. }) => {
            let mut fw = Vec::new();
            match focus_solution(b, &m, pi, &values, p, &donors, true, &mut fw) {
                Ok(s) => Some(s),
                Err(e) => Some(Solution::failed(e, fw)),
            }
        }
        _ => None,
    };

    let mut proposals = BTreeMap::new();
    for (gi, gv) in values.goods.iter().enumerate() {
        if gi == m.root || gv.needed {
            let list = recipe::propose(b, &m, gi, &prices, &donors);
            proposals.insert(
                m.goods[gi].id.clone(),
                list.iter().map(|pr| proposal_json(b, pr)).collect(),
            );
        }
    }

    let goods = m
        .goods
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let v = &values.goods[i];
            GoodValueJson {
                id: g.id.clone(),
                name: g.name.clone(),
                value: v.value,
                sd: v.sd,
                seconds: v.seconds,
                via: v.via.clone(),
                needed: v.needed,
                view: node_view(b, &m.universe, &m.ideal(b, i)),
            }
        })
        .collect();
    let processes = values
        .processes
        .iter()
        .enumerate()
        .map(|(i, v)| process_value_json(b, &m, i, v))
        .collect();
    Ok(SolutionV3 {
        version: VERSION_V3,
        error: None,
        warnings: std::mem::take(warnings),
        market: MarketJson { goods, processes },
        focus,
        proposals,
        cache_hits: values.cache_hits,
    })
}

/// Requirements only a mechanic no recipe delivers can produce.
fn requirement_warnings(b: &Bundle, t: &Target) -> Vec<String> {
    let ctx = SourceCtx::new(b, t.base, t.influences);
    let mut out = Vec::new();
    for r in &t.requirements {
        let sources: Vec<ModSource> = r.accepted.iter().flat_map(|m| ctx.sources(*m)).collect();
        if !sources.is_empty() && sources.iter().all(|s| *s == ModSource::Bench) {
            out.push(format!(
                "{} is only obtainable by {}, which this milestone does not model",
                r.label,
                ModSource::Bench.describe(b)
            ));
        }
    }
    out
}

/// Recipe instantiations for one good of a v1 or v3 problem (Bridge `propose_processes`).
pub fn propose_processes(
    b: &Bundle,
    problem_json: &str,
    good_id: &str,
) -> Result<Vec<ProcessProposalJson>, String> {
    let v: serde_json::Value = serde_json::from_str(problem_json).map_err(|e| e.to_string())?;
    let p: ProblemV3 = if v.get("version").and_then(|x| x.as_u64()) == Some(VERSION as u64) {
        upgrade_v1(&serde_json::from_value(v).map_err(|e| e.to_string())?)
    } else {
        serde_json::from_value(v).map_err(|e| e.to_string())?
    };
    let (m, donors) = build_market(b, &p).map_err(|e| e.message)?;
    let g = m
        .good(good_id)
        .ok_or_else(|| format!("Unknown good \"{good_id}\"."))?;
    let prices = prices_with_donors(&p.prices, &donors);
    Ok(recipe::propose(b, &m, g, &prices, &donors)
        .iter()
        .map(|pr| proposal_json(b, pr))
        .collect())
}

fn describe_pre(c: &registry::Contract) -> String {
    let r = &c.requires;
    let mut parts = vec![match r.rarity {
        Some(x) => x.name().to_string(),
        None => "any rarity".to_string(),
    }];
    if r.lock_free {
        parts.push("no Cannot Be Changed mod".to_string());
    }
    match r.max_influences {
        Some(0) => parts.push("no influence".to_string()),
        Some(n) => parts.push(format!("at most {n} influence")),
        None => {}
    }
    if r.needs_free > 0 {
        parts.push("a free affix slot".to_string());
    }
    parts.join(", ")
}

fn describe_side(e: registry::SideEffect) -> String {
    use registry::SideEffect::*;
    match e {
        Keep => "kept".to_string(),
        Clear => "removed".to_string(),
        Reroll(n) => format!("rerolled (up to {n})"),
        Add(n) => format!("kept, {n} added over both sides"),
        Remove(n) => format!("kept, {n} removed over both sides"),
    }
}

fn describe_effect(c: &registry::Contract) -> String {
    let e = &c.effect;
    let mut s = format!(
        "prefixes {}; suffixes {}",
        describe_side(e.prefixes),
        describe_side(e.suffixes)
    );
    if let Some(r) = e.rarity {
        s.push_str(&format!("; becomes {}", r.name()));
    }
    s
}

/// The craft table for the UI (Bridge `contracts`).
pub fn contracts_json(b: &Bundle) -> Vec<ContractJson> {
    let sample = |id: &str| -> Option<Action> {
        Some(match id {
            "chaos" => Action::Chaos,
            "exalted" => Action::Exalted,
            "influence-exalt" => Action::InfluenceExalt { influence: 1 },
            "annul" => Action::Annul,
            "divine" => Action::Divine,
            "harvest-influenced" => Action::HarvestInfluenced,
            "harvest-randomise-influence" => Action::HarvestRandomiseInfluence,
            "bench-lock" => Action::BenchLock { side: Side::Suffix },
            "transmute" => Action::Transmute,
            "alteration" => Action::Alteration,
            "augmentation" => Action::Augmentation,
            "regal" => Action::Regal,
            "scour" => Action::Scour,
            "imprint" => Action::Imprint,
            "restore" => Action::Restore,
            _ => return None,
        })
    };
    registry::CONTRACTS
        .iter()
        .map(|c| ContractJson {
            id: c.id.to_string(),
            name: c.name.to_string(),
            requires: describe_pre(c),
            effect: describe_effect(c),
            cost: match c.id {
                "recombine" => vec![(market::RECOMBINER_KEY.to_string(), 1.0)],
                "apply-catalyst" => vec![(
                    "<catalyst>".to_string(),
                    registry::CATALYSTS_FOR_FULL_QUALITY,
                )],
                "awakener" => vec![
                    ("awakeners-orb".to_string(), 1.0),
                    ("<donor>".to_string(), 1.0),
                ],
                "essence" => vec![("<essence>".to_string(), 1.0)],
                "fossil" => vec![
                    ("<fossils>".to_string(), 1.0),
                    ("<resonator>".to_string(), 1.0),
                ],
                id => sample(id)
                    .map(|a| registry::cost(b, &a, &[]))
                    .unwrap_or_default(),
            },
            seconds: c.seconds,
        })
        .collect()
}

/// Tiers of one mod group that can reach `base` on `side`, best first (Bridge `mod_tiers`).
pub fn mod_tiers(b: &Bundle, base: &str, group: &str, side: &str) -> Result<Vec<TierJson>, String> {
    let base_index = b
        .base_by_name(base)
        .ok_or_else(|| format!("Unknown base item \"{base}\"."))?;
    let group_index =
        group_index(b, group).ok_or_else(|| format!("Unknown modifier group \"{group}\"."))?;
    let side = parse_side(side).ok_or_else(|| format!("Unknown side \"{side}\"."))?;
    let ctx = SourceCtx::new(b, base_index, 0);
    let mut out: Vec<TierJson> = Vec::new();
    for (i, m) in b.mods.iter().enumerate() {
        if m.group != group_index || m.generation != side || ctx.sources(i as u32).is_empty() {
            continue;
        }
        let Some(stat) = m.stats.first() else {
            continue;
        };
        let eff = target::effect(b, base_index, None, i as u32);
        out.push(TierJson {
            id: m.id.clone(),
            text: m.text.replace('\n', ", "),
            ilvl: m.ilvl,
            min: target::display(stat.min.floor(), eff),
            max: target::display(stat.max.floor(), eff),
        });
    }
    out.sort_by(|a, b| b.ilvl.cmp(&a.ilvl).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
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
            .map(|r| {
                let mut j = requirement_json(b, r);
                if let Some(base) = d.base {
                    let ctx = SourceCtx::new(b, base, d.influences);
                    let mut names: Vec<String> = Vec::new();
                    let mut all: Vec<ModSource> =
                        r.accepted.iter().flat_map(|m| ctx.sources(*m)).collect();
                    all.sort();
                    all.dedup();
                    for src in all {
                        let name = src.describe(b);
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                    j.sources = Some(names);
                }
                j
            })
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
