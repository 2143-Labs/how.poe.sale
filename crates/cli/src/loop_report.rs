//! `loop-report`: the owner's craft loop on the exact lumped state, one section per depth.
//!
//! A depth section says what one step did to the whole live mass: how many classes it left, what
//! mass each stage took, and how much stayed live for the next step. The totals section adds up
//! only the mass that *left* the loop — `desired` (absorbed at the goal) and the two policy
//! terminals `reset` and `stop`. Every other stage is carried into the next depth, so summing it
//! over depths would count the same mass once per step it stayed live. The last line checks the
//! account: live plus everything that left is exactly one, in [`Fraction`]s rather than rounded
//! percentages.
//!
//! The state comes from `craft::lumped`, whose class records exactly the facts a step reads, so
//! these numbers are the concrete distribution's, folded — nothing here is an approximation.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::Context;
use poe1::craft::lumped::{Depth, Lumped};
use poe1::craft::{LoopPolicy, Stage};
use poe1::currency::{Accept, Fraction, ItemSetFilter};
use poe1::data::{Data, Influence};
use poe1::item::Item;
use poe1::pattern::{Pattern, PatternSpec};

/// Where the pattern fixtures live, relative to the repository root the command runs from.
const PATTERN_DIR: &str = "fixtures/patterns";

/// Percentages are read, not divided by: six decimals separate the small stages.
const PLACES: usize = 6;

/// The pattern fixture named by `--pattern`, compiled against the bundle.
///
/// The fixture is the same file `solve` and the catalog read, so a loop report and a step report
/// agree on what the goal is; the item comes from the flags rather than a fixture because the loop
/// starts from a white base.
pub fn pattern(data: &Data, name: &str) -> anyhow::Result<Pattern> {
    let path = PathBuf::from(PATTERN_DIR).join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "reading the pattern fixture {} (patterns are the files under {PATTERN_DIR}/)",
            path.display()
        )
    })?;
    let spec: PatternSpec = serde_json::from_str(&text)
        .with_context(|| format!("parsing the pattern fixture {}", path.display()))?;
    Pattern::compile(data, &spec)
        .with_context(|| format!("compiling the pattern {name:?} against the bundle"))
}

/// The whole document, after running `depth` steps (at least one) of the loop from `item`.
///
/// The filter is derived from the pattern the same way the solver derives it, so the stages a
/// depth records are the stages `solve` would grade.
pub fn report(data: &Data, pattern: &Pattern, item: &Item, depth: usize) -> anyhow::Result<String> {
    let filter = ItemSetFilter::from_pattern(pattern, Accept::DesiredOrGoodBothSides);
    let policy = LoopPolicy::new(&filter);
    let mut lumped = Lumped::new(data, filter.clone(), item.clone());
    for step in 1..=depth {
        if lumped.state().is_empty() {
            break;
        }
        lumped
            .step(&policy)
            .with_context(|| format!("lumping depth {step}"))?;
    }
    render(data, pattern, item, lumped.history())
}

/// The document for the depths already taken.
fn render(
    data: &Data,
    pattern: &Pattern,
    item: &Item,
    depths: &[Depth],
) -> anyhow::Result<String> {
    let base = data.base_at(item.base);
    let mut out = String::new();
    writeln!(out, "# Loop report — `{}`", pattern.name).unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Base `{}`, item level {}, influence {}.",
        base.name,
        item.item_level,
        influence_text(item),
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "The owner's loop for {} depth{} on the exact lumped state. Each section is one step: a \
         row per stage that took mass, then the mass left live for the next step.",
        depths.len(),
        if depths.len() == 1 { "" } else { "s" },
    )
    .unwrap();
    writeln!(out).unwrap();

    for depth in depths {
        writeln!(
            out,
            "## Depth {} — {} live classes",
            depth.index,
            depth.classes.len()
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "| stage | exact | percent |").unwrap();
        writeln!(out, "|---|---|---|").unwrap();
        for (stage, _) in &depth.chances {
            let mass = depth.mass(*stage);
            writeln!(out, "| `{}` | {} | {} |", stage.name(), mass, pct(mass)).unwrap();
        }
        writeln!(out).unwrap();
        writeln!(
            out,
            "Live after depth {}: {} ({}).",
            depth.index,
            depth.live(),
            pct(depth.live()),
        )
        .unwrap();
        writeln!(out).unwrap();
    }

    // The mass that left the loop, by stage. `desired` is absorbed at the goal; `reset` and `stop`
    // are the policy's terminals, where the loop has no further move for the mass. Every other
    // stage stays live, so only these three may be summed across depths.
    let stages = [
        (Stage::Desired, total(depths, Stage::Desired)?),
        (Stage::Reset, total(depths, Stage::Reset)?),
        (Stage::Stop, total(depths, Stage::Stop)?),
    ];
    let absorbed = left_loop(depths)?;
    let live = depths.last().map_or(Fraction::ONE, Depth::live);

    writeln!(out, "## Totals").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Live after depth {}: {} ({}).",
        depths.len(),
        live,
        pct(live),
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Mass that left the loop, summed over {} depth{}:",
        depths.len(),
        if depths.len() == 1 { "" } else { "s" },
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "| stage | exact | percent |").unwrap();
    writeln!(out, "|---|---|---|").unwrap();
    for (stage, mass) in stages {
        writeln!(out, "| `{}` | {} | {} |", stage.name(), mass, pct(mass)).unwrap();
    }
    writeln!(out, "| **total** | {} | {} |", absorbed, pct(absorbed)).unwrap();
    writeln!(out).unwrap();

    let total_mass = add(live, absorbed)?;
    let verdict = if total_mass.is_one() {
        "holds exactly"
    } else {
        "DOES NOT HOLD"
    };
    writeln!(
        out,
        "Conservation: live {} + absorbed {} = {} — {}.",
        live, absorbed, total_mass, verdict,
    )
    .unwrap();
    Ok(out)
}

/// One stage's recorded mass summed over the depths, in depth order.
///
/// The order matters to exactness under a fixed width: a depth's denominators share their large
/// factors with the earlier, smaller partial sum, so they reduce before the multiplications meet.
/// Summing the same stages in another order can need a common denominator wider than `u128`.
fn total(depths: &[Depth], stage: Stage) -> anyhow::Result<Fraction> {
    depths
        .iter()
        .try_fold(Fraction::ZERO, |acc, depth| add(acc, depth.mass(stage)))
}

/// The mass that left the loop: each depth's goal absorption, then the `reset` and `stop`
/// terminals paired and added together — the engine's own accounting order. Pairing the terminals
/// first reduces their common denominator, which is what keeps the later sum inside `u128`.
fn left_loop(depths: &[Depth]) -> anyhow::Result<Fraction> {
    depths.iter().try_fold(Fraction::ZERO, |acc, depth| {
        let acc = add(acc, depth.absorbed)?;
        let terminals = add(depth.mass(Stage::Reset), depth.mass(Stage::Stop))?;
        add(acc, terminals)
    })
}

/// Exact addition, refused rather than wrapped when the common denominator outgrows `u128`.
fn add(a: Fraction, b: Fraction) -> anyhow::Result<Fraction> {
    a.checked_add(b)
        .context("the exact mass account does not fit in 128-bit fractions")
}

/// The item's influences as the bundle spells them, or `none` for a plain base.
fn influence_text(item: &Item) -> String {
    let names: Vec<&str> = Influence::ALL
        .iter()
        .filter(|influence| item.influences & influence.bit() != 0)
        .map(|influence| influence.name())
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// A mass as the report reads it: the percentage is scale, the fraction beside it is the number.
fn pct(mass: Fraction) -> String {
    format!("{}%", mass.percent_text(PLACES))
}
