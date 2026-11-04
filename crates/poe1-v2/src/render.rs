//! Markdown renderings the catalog is built from: an item as a fenced game-text block, a line
//! diff between two items, and a method outcome (Chance branches with probabilities, Choices,
//! Pending reasons) nested as blockquotes.

use crate::apply::Outcome;
use crate::data::Data;
use crate::item::Item;

/// How many Chance branches print before the rest are summarised.
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

/// A method outcome in Markdown. Nested outcomes are blockquoted one level deeper.
pub fn outcome(data: &Data, o: &Outcome) -> String {
    match o {
        Outcome::Item(item) => item_block(data, item),
        Outcome::Items(items) => {
            let mut parts = vec![format!("{} items:", items.len())];
            parts.extend(items.iter().map(|i| item_block(data, i)));
            parts.join("\n\n")
        }
        Outcome::Destroyed => "the item is destroyed".into(),
        Outcome::Chance(children) => {
            let mut parts = Vec::new();
            for (p, child) in children.iter().take(SHOWN_BRANCHES) {
                parts.push(format!("p = {p:.6}"));
                parts.push(quote(&outcome(data, child)));
            }
            if children.len() > SHOWN_BRANCHES {
                let rest = &children[SHOWN_BRANCHES..];
                let total: f64 = rest.iter().map(|(p, _)| p).sum();
                parts.push(format!(
                    "… {} more branches (total p = {total:.6})",
                    rest.len()
                ));
            }
            parts.join("\n\n")
        }
        Outcome::Choice(options) => {
            let mut parts = vec!["you choose one of:".to_string()];
            parts.extend(options.iter().map(|o| quote(&outcome(data, o))));
            parts.join("\n\n")
        }
        Outcome::Pending { item, reason, .. } => format!(
            "not computed in this pass: {}; the part that is certain:\n\n{}",
            reason.text(),
            item_block(data, item)
        ),
    }
}

/// The result item with the highest probability (Choice options count equally; Pending results
/// count as their partial item). `None` when nothing survives.
pub fn most_likely_item(o: &Outcome) -> Option<&Item> {
    fn walk<'a>(o: &'a Outcome, p: f64, best: &mut Option<(f64, &'a Item)>) {
        let offer = |p: f64, item: &'a Item, best: &mut Option<(f64, &'a Item)>| {
            if best.is_none_or(|(q, _)| p > q) {
                *best = Some((p, item));
            }
        };
        match o {
            Outcome::Item(i) | Outcome::Pending { item: i, .. } => offer(p, i, best),
            Outcome::Items(items) => {
                if let Some(i) = items.first() {
                    offer(p, i, best);
                }
            }
            Outcome::Destroyed => {}
            Outcome::Chance(c) => c.iter().for_each(|(q, o)| walk(o, p * q, best)),
            Outcome::Choice(c) => {
                let n = c.len().max(1) as f64;
                c.iter().for_each(|o| walk(o, p / n, best));
            }
        }
    }
    let mut best = None;
    walk(o, 1.0, &mut best);
    best.map(|(_, i)| i)
}

/// (exact leaves, pending leaves) of an outcome.
pub fn leaf_counts(o: &Outcome) -> (usize, usize) {
    match o {
        Outcome::Item(_) | Outcome::Destroyed => (1, 0),
        Outcome::Items(v) => (v.len(), 0),
        Outcome::Pending { .. } => (0, 1),
        Outcome::Chance(c) => c.iter().fold((0, 0), |(a, b), (_, o)| {
            let (x, y) = leaf_counts(o);
            (a + x, b + y)
        }),
        Outcome::Choice(c) => c.iter().fold((0, 0), |(a, b), o| {
            let (x, y) = leaf_counts(o);
            (a + x, b + y)
        }),
    }
}
