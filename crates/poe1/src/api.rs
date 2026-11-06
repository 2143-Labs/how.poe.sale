//! What the browser shows, as JSON: a pasted item as the engine reads it, a target pattern made
//! from a pasted item, and the one-step report (`solve::next_steps` and `invert::sources`) with
//! every number and word already chosen. The words come from the catalog's own helpers, so the
//! page and `catalog/next-steps.md` describe a step the same way.

use serde::Serialize;

use crate::catalog::{op_sentence, paid_cost, stands_word, state_phrase};
use crate::data::Data;
use crate::invert::{self, Inversion};
use crate::item::{Item, ItemSpec};
use crate::pattern::{self, Pattern, PatternSpec};
use crate::solve::{self, Assessment};
use crate::text;

/// A pasted item as the engine read it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub item: ItemSpec,
    /// The item printed back as game text.
    pub text: String,
    /// Every line the reader could not explain.
    pub warnings: Vec<String>,
}

/// The item as it stands against the pattern.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NowView {
    pub progress: f64,
    pub matches: f64,
    pub blocked: f64,
    /// Whether the item can pass the pattern's item filter at all.
    pub fits: bool,
}

/// One step the item allows.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub method: String,
    pub name: String,
    pub stands: String,
    pub cost: String,
    pub finish: f64,
    pub goal: f64,
    pub better: f64,
    pub worse: f64,
    pub blocked: f64,
    pub results: usize,
    /// What the method does, one sentence per op.
    pub does: String,
    /// Whether the step does exactly what the one before it does.
    pub tied: bool,
}

/// A method that does not apply, or cannot be walked backwards, and why.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnmetView {
    pub method: String,
    pub name: String,
    pub reason: String,
}

/// One input that could have produced the item.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub method: String,
    pub name: String,
    pub p: f64,
    pub note: Option<String>,
}

/// The inputs that read as one pattern state.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub state: String,
    pub best: f64,
    /// The most likely source's input, as game text.
    pub example: String,
    pub sources: Vec<SourceView>,
}

/// What the inverse solver found.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourcesView {
    pub found: usize,
    pub groups: Vec<GroupView>,
    pub skipped: Vec<UnmetView>,
}

/// The one-step report.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SolveView {
    pub now: NowView,
    pub exact: Vec<StepView>,
    pub certain: Vec<StepView>,
    pub blind: Vec<StepView>,
    pub unmet: Vec<UnmetView>,
    pub sources: SourcesView,
}

/// Reads a pasted item: the item, with the warnings of the reader.
fn read(data: &Data, text: &str) -> Result<(Item, Vec<String>), String> {
    let parsed = text::parse(data, text).map_err(|e| format!("item: {e}"))?;
    let item = Item::from_spec(data, &parsed.spec).map_err(|e| format!("item: {e}"))?;
    Ok((item, parsed.warnings))
}

/// A pasted item as the engine reads it.
pub fn read_item(data: &Data, text: &str) -> Result<ItemView, String> {
    let (item, warnings) = read(data, text)?;
    Ok(ItemView {
        item: item.to_spec(data),
        text: text::render(data, &item),
        warnings,
    })
}

/// The pattern that asks for `target` exactly (`pattern::from_item`). An item with no explicit
/// modifier to aim for has no slot, and a pattern with no slot would score every item that fits
/// its filter as finished, so it is refused.
pub fn target_of(data: &Data, target: &Item) -> Result<PatternSpec, String> {
    let spec = pattern::from_item(data, target);
    if spec.prefixes.is_empty() && spec.suffixes.is_empty() {
        return Err("target: the pasted item has no explicit modifier to aim for".into());
    }
    Ok(spec)
}

/// `target_of` a pasted item.
pub fn target(data: &Data, text: &str) -> Result<PatternSpec, String> {
    let (item, _) = read(data, text)?;
    target_of(data, &item)
}

fn name_of(data: &Data, method: &str) -> String {
    data.method(method)
        .map_or_else(|| method.to_string(), |m| m.name.clone())
}

fn unmet(data: &Data, list: &[(String, String)]) -> Vec<UnmetView> {
    list.iter()
        .map(|(method, reason)| UnmetView {
            method: method.clone(),
            name: name_of(data, method),
            reason: reason.clone(),
        })
        .collect()
}

fn steps(data: &Data, group: &[Assessment]) -> Vec<StepView> {
    group
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let w = &a.weighed;
            let does = data
                .method(&a.method)
                .map(|m| {
                    m.ops
                        .iter()
                        .map(|op| op_sentence(data, op))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            StepView {
                method: a.method.clone(),
                name: a.name.clone(),
                stands: stands_word(a.stands()).to_string(),
                cost: paid_cost(data, &a.cost),
                finish: w.finish,
                goal: w.goal,
                better: w.better,
                worse: w.worse,
                blocked: w.blocked,
                results: w.results,
                does,
                tied: i > 0 && group[i - 1].tied(a),
            }
        })
        .collect()
}

fn sources(data: &Data, pattern: &Pattern, inversion: &Inversion) -> SourcesView {
    SourcesView {
        found: inversion.found,
        groups: inversion
            .groups
            .iter()
            .map(|g| GroupView {
                state: state_phrase(pattern, &g.state),
                best: g.best,
                example: g
                    .sources
                    .first()
                    .map(|s| text::render(data, &s.input))
                    .unwrap_or_default(),
                sources: g
                    .sources
                    .iter()
                    .map(|s| SourceView {
                        method: s.method.clone(),
                        name: s.name.clone(),
                        p: s.p,
                        note: s.note.clone(),
                    })
                    .collect(),
            })
            .collect(),
        skipped: unmet(data, &inversion.skipped),
    }
}

/// The one-step report for a pattern (JSON text) and a pasted item.
pub fn solve(data: &Data, pattern_json: &str, item_text: &str) -> Result<SolveView, String> {
    let spec: PatternSpec =
        serde_json::from_str(pattern_json).map_err(|e| format!("pattern: {e}"))?;
    let pattern = Pattern::compile(data, &spec).map_err(|e| e.to_string())?;
    let (item, _) = read(data, item_text)?;
    let report = solve::next_steps(data, &pattern, &item);
    let fits = pattern::evaluate(data, &pattern, &item)
        .iter()
        .any(|(p, s)| *p > 0.0 && s.item_ok);
    let inversion = invert::sources(data, &pattern, &item);
    Ok(SolveView {
        now: NowView {
            progress: report.now.progress,
            matches: report.now.matches,
            blocked: report.now.blocked,
            fits,
        },
        exact: steps(data, &report.exact),
        certain: steps(data, &report.certain),
        blind: steps(data, &report.blind),
        unmet: unmet(data, &report.unmet),
        sources: sources(data, &pattern, &inversion),
    })
}
