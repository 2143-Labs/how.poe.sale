//! Markdown renderings the catalog is built from: an item as a fenced game-text block, a line
//! diff between two items, and a method outcome (every leaf with its probability, choices with
//! their options blockquoted, Pending results with what is certain or the item before the step).
//! Every printed probability is of the whole outcome.

use crate::apply::{Leaf, Outcome};
use crate::data::Data;
use crate::item::Item;

/// How many leaves print before the rest are summarised.
pub const SHOWN_BRANCHES: usize = 5;

/// The item as the game shows it, in a ```` ```text ```` fence.
pub fn item_block(data: &Data, item: &Item) -> String {
    format!(
        "```text\n{}\n```",
        crate::text::render(data, item).trim_end()
    )
}

/// A line diff (longest common subsequence): common lines `"  "`, removed `"- "`, added `"+ "`,
/// in a ```` ```diff ```` fence.
pub fn diff(a: &str, b: &str) -> String {
    let a: Vec<&str> = a.lines().collect();
    let b: Vec<&str> = b.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = vec!["```diff".to_string()];
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push(format!("  {}", a[i]));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push(format!("+ {}", b[j]));
            j += 1;
        } else {
            out.push(format!("- {}", a[i]));
            i += 1;
        }
    }
    out.push("```".into());
    out.join("\n")
}

/// The item diff `diff(render(a), render(b))`.
pub fn item_diff(data: &Data, a: &Item, b: &Item) -> String {
    diff(&crate::text::render(data, a), &crate::text::render(data, b))
}

fn quote(block: &str) -> String {
    block
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A method outcome in Markdown.
pub fn outcome(data: &Data, o: &Outcome) -> String {
    scaled(data, o, 1.0)
}

/// `o`'s leaves with each chance multiplied by `scale` (the chance of reaching `o`).
fn scaled(data: &Data, o: &Outcome, scale: f64) -> String {
    let mut parts = Vec::new();
    for (p, leaf) in o.leaves.iter().take(SHOWN_BRANCHES) {
        let p = p * scale;
        match leaf {
            Leaf::Item(item) => {
                parts.push(format!("p = {p:.6}"));
                parts.push(quote(&item_block(data, item)));
            }
            Leaf::Items(items) => {
                parts.push(format!("p = {p:.6}: {} items", items.len()));
                parts.extend(items.iter().map(|i| quote(&item_block(data, i))));
            }
            Leaf::Destroyed => parts.push(format!("p = {p:.6}: the item is destroyed")),
            Leaf::Choice(options) => {
                parts.push(format!(
                    "p = {p:.6}: you choose one of these {} (each option's chances below are of the whole outcome, if you pick it):",
                    options.len()
                ));
                parts.extend(options.iter().map(|opt| quote(&scaled(data, opt, p))));
            }
            Leaf::Pending(pending) => {
                let what = if pending.certain {
                    "the part that is certain"
                } else {
                    "the item before this step; nothing about the result is known"
                };
                parts.push(format!(
                    "p = {p:.6}: not computed in this pass: {}; {what}:",
                    pending.reason.text()
                ));
                parts.push(quote(&item_block(data, &pending.item)));
            }
        }
    }
    if o.leaves.len() > SHOWN_BRANCHES {
        let rest = &o.leaves[SHOWN_BRANCHES..];
        let total: f64 = rest.iter().map(|(p, _)| p * scale).sum();
        parts.push(format!(
            "… {} more outcomes (total p = {total:.6})",
            rest.len()
        ));
    }
    parts.join("\n\n")
}

/// The result item with the highest probability: a choice's options count equally, a Mirror
/// counts as its first item, a reforge as its certain part; Destroyed and a Pending that stopped
/// before its step have no result item. `None` when nothing qualifies.
pub fn most_likely_item(o: &Outcome) -> Option<&Item> {
    fn offer<'a>(p: f64, item: &'a Item, best: &mut Option<(f64, &'a Item)>) {
        if best.is_none_or(|(q, _)| p > q) {
            *best = Some((p, item));
        }
    }
    fn walk<'a>(o: &'a Outcome, scale: f64, best: &mut Option<(f64, &'a Item)>) {
        for (p, leaf) in &o.leaves {
            let p = p * scale;
            match leaf {
                Leaf::Item(i) => offer(p, i, best),
                Leaf::Items(items) => {
                    if let Some(i) = items.first() {
                        offer(p, i, best);
                    }
                }
                Leaf::Pending(pending) if pending.certain => offer(p, &pending.item, best),
                Leaf::Pending(_) | Leaf::Destroyed => {}
                Leaf::Choice(options) => {
                    let n = options.len().max(1) as f64;
                    for opt in options {
                        walk(opt, p / n, best);
                    }
                }
            }
        }
    }
    let mut best = None;
    walk(o, 1.0, &mut best);
    best.map(|(_, i)| i)
}

/// (computed leaves, pending leaves) of an outcome: a choice counts once, as does a Mirror's
/// pair of items.
pub fn leaf_counts(o: &Outcome) -> (usize, usize) {
    o.leaves
        .iter()
        .fold((0, 0), |(exact, pending), (_, leaf)| match leaf {
            Leaf::Pending(_) => (exact, pending + 1),
            _ => (exact + 1, pending),
        })
}
