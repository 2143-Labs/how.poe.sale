//! The craft as text: Markdown, with Mermaid diagrams.
//!
//! Nothing here computes a chance or a cost; it prints a [`Report`], and each method's
//! [`crate::diagram::Diagram`].

use std::fmt::Write;

use crate::cost::{Cost, Spend};
use crate::craft::{Craft, Report, Wire};
use crate::data::Data;
use crate::diagram::{BoxKind, diagram};
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Item, Rarity, Side};
use crate::method::Kit;
use crate::node::Node;
use crate::pool::{draw_weights, side_chance};
use crate::weights::Weights;

/// The whole report. `start` is the cleared item the law was built from.
pub fn render(
    data: &Data,
    start: &Item,
    craft: &Craft,
    kit: &Kit,
    weights: &Weights,
    report: &Report,
) -> String {
    let mut out = String::new();
    header(&mut out, data, start, kit);
    currencies(&mut out, kit, weights);
    steps(&mut out, craft, weights, report);
    for (step, row) in report.rows.iter().enumerate() {
        method(&mut out, craft, kit, report, step, &row.name, weights);
    }
    others(&mut out, weights, report);
    out
}

fn header(out: &mut String, data: &Data, start: &Item, kit: &Kit) {
    let base = data.base_at(start.base);
    let influence = start
        .influence
        .map(|influence| format!(" ({})", influence.name()))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "# {}{influence}, item level {}\n",
        base.name, start.item_level
    );
    let magic = start.with_rarity(Rarity::Magic);
    for side in Side::BOTH {
        let pool = draw_weights(data, &magic, side);
        let total: u128 = pool.iter().map(|&(_, weight)| weight).sum();
        let _ = writeln!(
            out,
            "- {} pool: {} modifiers, total weight {total}",
            side.name(),
            pool.len()
        );
    }
    let filter = ItemSetFilter::from_data(data);
    for (side, tier) in [
        (Side::Prefix, Tier::Desired),
        (Side::Prefix, Tier::Good),
        (Side::Suffix, Tier::Good),
    ] {
        if let Some(chance) = side_chance(data, &magic, side, |m| filter.tier_of(m) == tier) {
            let _ = writeln!(
                out,
                "- one {} {}: {chance} = {:.4}%",
                tier.name(),
                side.name(),
                chance.percent()
            );
        }
    }
    let floors = |side: Side| {
        kit.target
            .floors(side)
            .iter()
            .map(|tier| tier.name())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(
        out,
        "- target: a rare item, prefixes [{}], suffixes [{}]\n",
        floors(Side::Prefix),
        floors(Side::Suffix)
    );
    let _ = writeln!(
        out,
        "The strategy is written down and this says what it costs. A chance is used one step \
         deep; what is carried from step to step is the cost to get there and its spread.\n"
    );
}

fn currencies(out: &mut String, kit: &Kit, weights: &Weights) {
    let _ = writeln!(out, "## Currencies\n");
    let _ = writeln!(
        out,
        "| currency | works on | one use costs | weight ({}) |",
        weights.unit
    );
    let _ = writeln!(out, "|---|---|---|---|");
    for tool in kit.tools {
        let weight = weights
            .of(tool.id())
            .map(|weight| format!("{weight}"))
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(
            out,
            "| {} | {} | 1 {} | {weight} |",
            tool.id().name(),
            rarities(tool.works_on()),
            tool.id().name()
        );
    }
    let _ = writeln!(out);
}

fn steps(out: &mut String, craft: &Craft, weights: &Weights, report: &Report) {
    let _ = writeln!(out, "## Craft: {} (forward)\n", craft.name);
    let _ = writeln!(out, "```mermaid\nflowchart LR");
    for (step, row) in report.rows.iter().enumerate() {
        // The rarities the step can hand on, each named once.
        let mut leaves: Vec<&'static str> = Vec::new();
        for spec in craft.steps[step].method.exits() {
            if !leaves.contains(&spec.rarity.name()) {
                leaves.push(spec.rarity.name());
            }
        }
        let leaves = leaves.join(", ");
        let valued = weights.value(&row.reach);
        let _ = writeln!(
            out,
            "  s{step}[\"{}<br/>{} → {leaves}<br/>past this step: {:.1} ± {:.1} {}\"]",
            row.name,
            rarities(row.takes),
            valued.value,
            valued.sd,
            weights.unit
        );
    }
    // Every exit wired out of the craft is one output box, so a craft that can finish three ways
    // says so.
    for (step, row) in report.rows.iter().enumerate() {
        for &(exit, wire, share) in &row.exits {
            if wire == Wire::Done {
                let _ = writeln!(
                    out,
                    "  {}([\"{exit}<br/>{} done\"])",
                    output_id(step, exit),
                    percent(share)
                );
            }
        }
    }
    for (step, row) in report.rows.iter().enumerate() {
        for &(exit, wire, share) in &row.exits {
            let (to, arrow) = match wire {
                Wire::Next => (format!("s{}", step + 1), Some("-->")),
                Wire::Done => (output_id(step, exit), None),
                Wire::To(to) if to > step => (format!("s{to}"), Some("-->")),
                Wire::To(to) => (format!("s{to}"), Some("-.->")),
            };
            match arrow {
                Some(arrow) => {
                    let _ = writeln!(out, "  s{step} {arrow}|\"{exit} {}\"| {to}", percent(share));
                }
                None => {
                    let _ = writeln!(out, "  s{step} --> {to}");
                }
            }
        }
    }
    let _ = writeln!(out, "```\n");

    let _ = writeln!(
        out,
        "| step | method | takes | chance | tries | one run costs | cost to get past it | {} ± sd |",
        weights.unit
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
    for (step, row) in report.rows.iter().enumerate() {
        let valued = weights.value(&row.reach);
        let _ = writeln!(
            out,
            "| {step} | {} | {} | {} | {:.2} | {} | {} | {:.1} ± {:.1} |",
            row.name,
            rarities(row.takes),
            percent(row.chance),
            row.tries,
            counts(&row.run.mean),
            counts(&row.reach.mean),
            valued.value,
            valued.sd
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Whole craft, per orb: {}\n", spread(&report.total));
}

/// The mermaid id of a step's output box: the exit's name, made safe for a node id.
fn output_id(step: usize, exit: &str) -> String {
    let safe: String = exit
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("o{step}_{safe}")
}

/// One box of a method's diagram, as a Mermaid label.
fn label(kind: &BoxKind) -> String {
    match kind {
        // A normal item holds no explicit modifiers, so both sides are empty.
        BoxKind::Forgets {
            rarity: Rarity::Normal,
            currency,
        } => format!(
            "normal<br/>prefixes: -<br/>suffixes: -<br/>{}",
            currency.name()
        ),
        // One box for every item of this rarity: the move throws away what it holds.
        BoxKind::Forgets { rarity, currency } => format!(
            "{}<br/>prefixes: any grades<br/>suffixes: any grades<br/>{}",
            rarity.name(),
            currency.name()
        ),
        BoxKind::Keeps {
            rarity,
            sides,
            currency,
            cost,
            options,
        } => {
            let side = |held: &[Tier]| {
                if held.is_empty() {
                    "-".to_string()
                } else {
                    held.iter()
                        .map(|tier| tier.name())
                        .collect::<Vec<_>>()
                        .join("+")
                }
            };
            let mut label = format!(
                "{}<br/>prefixes: {}<br/>suffixes: {}<br/>→ {}",
                rarity.name(),
                side(&sides[0]),
                side(&sides[1]),
                currency.name()
            );
            if let Some(cost) = cost {
                label.push_str(&format!(" {cost:.1}"));
            }
            // Everything else the item allows, cheapest first, as one line.
            if !options.is_empty() {
                let others: Vec<String> = options
                    .iter()
                    .map(|(at, cost)| format!("{} {cost:.1}", at.name()))
                    .collect();
                label.push_str(&format!("<br/>or {}", others.join(", ")));
            }
            label
        }
        BoxKind::Exit { exit, to } => format!("exit {exit}<br/>to {to}"),
    }
}

fn method(
    out: &mut String,
    craft: &Craft,
    kit: &Kit,
    report: &Report,
    step: usize,
    name: &str,
    weights: &Weights,
) {
    let drawn = diagram(craft, kit, report, step, weights);
    let _ = writeln!(out, "## Method: {name} (step {step})\n");
    let _ = writeln!(
        out,
        "One box per item the strategy passes through: the rarity, the prefixes and the suffixes it \
         holds, the move the strategy makes with what it costs to finish from there, and every \
         other move the item allows. Costs are {}, as elsewhere in this report.\n",
        weights.unit
    );
    let _ = writeln!(out, "```mermaid\nflowchart TD");
    for (at, cell) in drawn.boxes.iter().enumerate() {
        let text = label(&cell.kind);
        match cell.kind {
            BoxKind::Exit { .. } => {
                let _ = writeln!(out, "  b{at}([\"{text}\"])");
            }
            _ => {
                let _ = writeln!(out, "  b{at}[\"{text}\"]");
            }
        }
    }
    for edge in &drawn.edges {
        let _ = writeln!(
            out,
            "  b{} -->|\"{}\"| b{}",
            edge.from,
            percent(edge.share),
            edge.to
        );
    }
    let _ = writeln!(out, "```\n");
}

fn others(out: &mut String, weights: &Weights, report: &Report) {
    let valued = |cost: &Cost| weights.value(&Spend::certain(cost)).value;
    let mut better: Vec<_> = report
        .others
        .iter()
        .filter(|other| other.better)
        .map(|other| {
            let (planned, instead) = (valued(&other.planned_to_go), valued(&other.other_to_go));
            (other.visits * (planned - instead), other, planned, instead)
        })
        .collect();
    better.sort_by(|a, b| b.0.total_cmp(&a.0));

    let _ = writeln!(out, "## Other moves\n");
    let mut places: Vec<(usize, &Node)> = report
        .others
        .iter()
        .map(|other| (other.step, &other.node))
        .collect();
    places.dedup();
    let _ = writeln!(
        out,
        "{} other moves checked at {} places the craft passes through; {} cost less to finish, \
         by the weights.\n",
        report.others.len(),
        places.len(),
        better.len()
    );
    if report.skipped > 0 {
        let _ = writeln!(
            out,
            "{} other moves could not be priced: the craft has no rule for where they land.\n",
            report.skipped
        );
    }
    if better.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "| step | item | visits | planned | to finish ({unit}) | instead | to finish ({unit}) |",
        unit = weights.unit
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|");
    for (_, other, planned, instead) in better {
        let _ = writeln!(
            out,
            "| {} | {} | {:.2} | {} | {:.2} | {} | {:.2} |",
            other.step,
            other.node.label(),
            other.visits,
            other.planned.name(),
            planned,
            other.other.name(),
            instead
        );
    }
    let _ = writeln!(out);
}

fn rarities(rarities: &[Rarity]) -> String {
    rarities
        .iter()
        .map(|rarity| rarity.name())
        .collect::<Vec<_>>()
        .join(", ")
}

fn percent(chance: f64) -> String {
    if chance > 0.0 && chance < 0.0001 {
        "<0.01%".to_string()
    } else {
        format!("{:.2}%", chance * 100.0)
    }
}

/// Mean orb counts, `name count`, the zero ones left out.
fn counts(cost: &Cost) -> String {
    let shown: Vec<String> = cost
        .iter()
        .filter(|&(_, count)| count.abs() >= 0.005)
        .map(|(id, count)| format!("{} {count:.2}", id.name()))
        .collect();
    if shown.is_empty() {
        "-".to_string()
    } else {
        shown.join(", ")
    }
}

/// Mean ± sd per orb.
fn spread(spend: &Spend) -> String {
    spend
        .mean
        .iter()
        .map(|(id, mean)| format!("{} {mean:.2} ± {:.2}", id.name(), spend.sd(id)))
        .collect::<Vec<_>>()
        .join(", ")
}
