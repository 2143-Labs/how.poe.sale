//! `loop-report`: the owner's craft loop on the exact lumped state, one section per depth.
//!
//! A depth section says what one step did to the whole live mass: how many classes it left, what
//! mass each stage took, and how much stayed live for the next step. The totals section adds up
//! only the mass that *left* the loop — `desired` (absorbed at the goal) and the two policy
//! terminals `reset` and `stop`. Every other stage is carried into the next depth, so summing it
//! over depths would count the same mass once per step it stayed live. The last line checks the
//! account: live plus everything that left is exactly one, in [`Fraction`]s rather than rounded
//! percentages. A report whose account does not hold is refused (a non-zero exit) rather than
//! printed as if it were a result.
//!
//! The state comes from `craft::lumped`, whose class records exactly the facts a step reads, so
//! these numbers are the concrete distribution's, folded — nothing here is an approximation.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;
use poe1::craft::lumped::{Depth, Lumped};
use poe1::craft::{LoopPolicy, Stage};
use poe1::currency::{Accept, Fraction, ItemSetFilter};
use poe1::data::{Data, Influence};
use poe1::item::Item;
use poe1::pattern::{Pattern, PatternSpec};

/// Percentages are read, not divided by: six decimals separate the small stages.
const PLACES: usize = 6;

/// The pattern fixture named by `--pattern`, read from the `--fixtures` tree and compiled against
/// the bundle.
///
/// The fixture is the goal the page's default also uses; the item comes from the flags rather than
/// a fixture because the loop starts from a white base.
pub fn pattern(data: &Data, fixtures: &Path, name: &str) -> anyhow::Result<Pattern> {
    let path = fixtures.join("patterns").join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "reading the pattern fixture {} (patterns are the files under {})",
            path.display(),
            fixtures.join("patterns").display(),
        )
    })?;
    let spec: PatternSpec = serde_json::from_str(&text)
        .with_context(|| format!("parsing the pattern fixture {}", path.display()))?;
    Pattern::compile(data, &spec)
        .with_context(|| format!("compiling the pattern {name:?} against the bundle"))
}

/// A report and, when a depth stopped the loop, the depth that did — the document still carries
/// every depth that ran.
pub struct Report {
    /// The document, covering the depths that ran.
    pub text: String,
    /// The depth the exact state could not hold, if one was reached; the document has a section
    /// saying which depth it was and why.
    pub stopped: Option<Stop>,
}

/// The depth the lumped state could not hold to the end, and the engine's reason.
pub struct Stop {
    /// The depth that failed, one past the last section in the document.
    pub depth: usize,
    /// What stopped it.
    pub reason: anyhow::Error,
}

/// The whole document, after running up to `depth` steps (at least one) of the loop from `item`.
///
/// The filter is derived from the pattern by `ItemSetFilter::from_pattern`, so a depth's stages
/// are the goal filter's own buckets.
///
/// A depth too deep for the exact state does not throw the earlier depths away: they are rendered,
/// the document says which depth stopped it and why, and [`Report::stopped`] carries the failure so
/// the command can still exit non-zero.
pub fn report(data: &Data, pattern: &Pattern, item: &Item, depth: usize) -> anyhow::Result<Report> {
    let filter = ItemSetFilter::from_pattern(pattern, Accept::DesiredOrGoodBothSides);
    let policy = LoopPolicy::new(&filter);
    let mut lumped = Lumped::new(data, filter.clone(), item.clone());
    let mut stopped = None;
    for step in 1..=depth {
        if lumped.state().is_empty() {
            break;
        }
        if let Err(err) = lumped.step(&policy) {
            stopped = Some(Stop {
                depth: step,
                reason: err.into(),
            });
            break;
        }
    }
    let text = render(
        data,
        pattern,
        item,
        lumped.history(),
        depth,
        stopped.as_ref(),
    )?;
    Ok(Report { text, stopped })
}

/// The document for the depths already taken.
fn render(
    data: &Data,
    pattern: &Pattern,
    item: &Item,
    depths: &[Depth],
    requested: usize,
    stopped: Option<&Stop>,
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
    // Asking for more depths than the loop has mass for is not a failure: it emptied.
    if stopped.is_none() && depths.len() < requested {
        writeln!(
            out,
            "The loop emptied after depth {}: no live class was left to take depth {}.",
            depths.len(),
            depths.len() + 1,
        )
        .unwrap();
        writeln!(out).unwrap();
    }

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

    // A depth that did not fit keeps the ones that did, and says so here rather than in a bare
    // error: the numbers above are still the exact state's.
    if let Some(stop) = stopped {
        writeln!(out, "## Stopped at depth {}", stop.depth).unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "Depth {} did not fit: {}. The {} depth{} above ran to completion on the exact state; \
             this report asked for {} and stops here. Those numbers are exact, but the command \
             exits non-zero: this is what computed, not the result asked for.",
            stop.depth,
            stop.reason,
            depths.len(),
            if depths.len() == 1 { "" } else { "s" },
            requested,
        )
        .unwrap();
        writeln!(out).unwrap();
    }

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
    // Name every term rather than one word for the sum: `desired` is the goal's absorption alone,
    // while the total that left the loop also includes the two policy terminals.
    let (desired, reset, stop) = (stages[0].1, stages[1].1, stages[2].1);
    writeln!(
        out,
        "Conservation: live {} + left the loop (desired {}, reset {}, stop {}) = {} — {}.",
        live, desired, reset, stop, total_mass, verdict,
    )
    .unwrap();
    // A report whose account does not hold is a bug in the lumped model, not a result: the caller
    // gets a non-zero exit so a script cannot read it as success.
    ensure_account(live, total_mass)?;
    Ok(out)
}

/// Conservation, refused rather than printed and forgotten. A loop whose mass does not add up is a
/// bug in the lumped model, and a script must see a non-zero exit rather than a report.
fn ensure_account(live: Fraction, total: Fraction) -> anyhow::Result<()> {
    anyhow::ensure!(
        total.is_one(),
        "the loop's mass account does not hold: live {live} + absorbed the rest = {total}; the \
         report's conservation line says so"
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{Fraction, ensure_account};

    /// The account is enforced, not merely printed: a broken one is an error, so the command exits
    /// non-zero and a script cannot read the report as success.
    #[test]
    fn a_broken_account_is_an_error() {
        assert!(ensure_account(Fraction::new(1, 2).expect("1/2"), Fraction::ONE).is_ok());
        let err = ensure_account(
            Fraction::new(1, 2).expect("1/2"),
            Fraction::new(5, 6).expect("5/6"),
        )
        .expect_err("5/6 of the mass is not a whole one");
        assert!(err.to_string().contains("does not hold"), "{err}");
    }
}
