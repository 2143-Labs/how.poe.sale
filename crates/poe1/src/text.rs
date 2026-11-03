//! Parsing a pasted item tooltip into a target draft.

use crate::data::Bundle;
use crate::item::{INFLUENCE_NAMES, Rarity};
use crate::target::{Requirement, effect};
use crate::weights::spawn_weight;

/// Everything the parser could recover from a pasted item, before the user edits it.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetDraft {
    pub item_class: Option<String>,
    pub base: Option<u16>,
    pub rarity: Rarity,
    pub ilvl: Option<u8>,
    pub influences: u8,
    pub catalyst: Option<(u8, u8)>,
    pub requirements: Vec<Requirement>,
    pub min_hits: u16,
    pub buy_note: Option<(f64, String)>,
    pub unmatched: Vec<String>,
    pub warnings: Vec<String>,
}

impl TargetDraft {
    /// The target this draft describes, if a base was recognised.
    pub fn to_target(&self) -> Option<crate::target::Target> {
        Some(crate::target::Target {
            base: self.base?,
            ilvl_min: self.ilvl.unwrap_or(1),
            influences: self.influences,
            rarity: self.rarity,
            catalyst: self.catalyst,
            requirements: self.requirements.clone(),
            min_hits: self.min_hits,
        })
    }
}

/// First numeric token in `s`: byte range and value.
fn first_number(s: &str) -> Option<(usize, usize, f64)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text = &s[start..i];
            return text.parse::<f64>().ok().map(|v| (start, i, v));
        }
        i += 1;
    }
    None
}

fn last_number(s: &str) -> Option<(usize, usize, f64)> {
    let mut found = None;
    let mut offset = 0;
    while let Some((start, end, value)) = first_number(&s[offset..]) {
        found = Some((start + offset, end + offset, value));
        offset += end;
        if offset >= s.len() {
            break;
        }
    }
    found
}

/// Every numeric token in `s`, in order.
fn all_numbers(s: &str) -> Vec<(usize, usize, f64)> {
    let mut out = Vec::new();
    let mut offset = 0;
    while let Some((start, end, value)) = first_number(&s[offset..]) {
        out.push((start + offset, end + offset, value));
        offset += end;
        if offset >= s.len() {
            break;
        }
    }
    out
}

/// `s` with every number replaced by `#` (a `+` sign directly before a number goes too).
fn hash_all_numbers(s: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (start, end, _) in all_numbers(s) {
        let lead = if s[..start].ends_with('+') {
            start - 1
        } else {
            start
        };
        out.push_str(&s[last..lead.max(last)]);
        out.push('#');
        last = end;
    }
    out.push_str(&s[last..]);
    out
}

/// Weapon and armour property lines: values of the base, not modifiers.
const PROPERTY_PREFIXES: [&str; 16] = [
    "Physical Damage:",
    "Elemental Damage:",
    "Chaos Damage:",
    "Critical Strike Chance:",
    "Attacks per Second:",
    "Weapon Range:",
    "Armour:",
    "Evasion Rating:",
    "Energy Shield:",
    "Ward:",
    "Chance to Block:",
    "Str:",
    "Dex:",
    "Int:",
    "Sockets:",
    "Stack Size:",
];

/// Replace the first number in `s` with `#`.
fn hash_first_number(s: &str) -> Option<String> {
    first_number(s).map(|(start, end, _)| format!("{}#{}", &s[..start], &s[end..]))
}

/// Replace the last number in `s` with `#`.
fn hash_last_number(s: &str) -> Option<String> {
    last_number(s).map(|(start, end, _)| format!("{}#{}", &s[..start], &s[end..]))
}

fn is_influence_line(line: &str) -> Option<usize> {
    let name = line.strip_suffix(" Item")?;
    INFLUENCE_NAMES.iter().position(|n| *n == name)
}

fn parse_rarity(line: &str) -> Option<Rarity> {
    match line {
        "Normal" => Some(Rarity::Normal),
        "Magic" => Some(Rarity::Magic),
        "Rare" => Some(Rarity::Rare),
        _ => None,
    }
}

impl Bundle {
    /// `Quality (Attribute Modifiers): +20% (augmented)` -> (label, quality).
    fn parse_quality(&self, line: &str) -> Result<(String, u8), String> {
        let rest = line
            .strip_prefix("Quality (")
            .ok_or_else(|| "quality line is not a labelled catalyst quality".to_string())?;
        let close = rest
            .find(')')
            .ok_or_else(|| "quality line has no closing parenthesis".to_string())?;
        let label = rest[..close]
            .strip_suffix(" Modifiers")
            .ok_or_else(|| "quality line is not a labelled catalyst quality".to_string())?
            .trim();
        let tail = rest[close + 1..].trim_start();
        let tail = tail
            .strip_prefix(':')
            .ok_or_else(|| "quality line has no colon".to_string())?
            .trim_start();
        let tail = tail
            .strip_prefix('+')
            .ok_or_else(|| "quality line has no + sign".to_string())?;
        let (_, _, value) =
            first_number(tail).ok_or_else(|| "quality line has no number".to_string())?;
        Ok((label.to_string(), value as u8))
    }
}

/// Whether `needle` occurs in `hay` on word boundaries.
fn contains_words(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + needle.len()..].chars().next();
        before.is_none_or(|c| c == ' ') && after.is_none_or(|c| c == ' ')
    })
}

/// Build a target draft from pasted tooltip text.
pub fn parse_target(b: &Bundle, text: &str) -> TargetDraft {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "--------")
        .collect();

    // A rare's base has a line of its own; a magic item's name wraps it in affix words
    // ("Thaumaturgist's Lapis Amulet of the Fox"), so fall back to the longest base name inside
    // the line after "Rarity:".
    let mut name_line: Option<&str> = None;
    let base = lines.iter().find_map(|l| b.base_by_name(l)).or_else(|| {
        let i = lines.iter().position(|l| l.starts_with("Rarity:"))?;
        let line = *lines.get(i + 1)?;
        let found = b
            .bases
            .iter()
            .enumerate()
            .filter(|(_, x)| contains_words(line, &x.name))
            .max_by_key(|(_, x)| x.name.len())
            .map(|(i, _)| i as u16)?;
        name_line = Some(line);
        Some(found)
    });

    let mut draft = TargetDraft {
        item_class: None,
        base,
        rarity: Rarity::Rare,
        ilvl: None,
        influences: 0,
        catalyst: None,
        requirements: Vec::new(),
        min_hits: 0,
        buy_note: None,
        unmatched: Vec::new(),
        warnings: Vec::new(),
    };

    let base_name = base.map(|i| b.base_at(i).name.clone());
    let implicit_lines: Vec<String> = base
        .map(|i| b.base_at(i).implicit_text.clone())
        .unwrap_or_default();

    let mut mod_lines: Vec<&str> = Vec::new();
    for line in &lines {
        if Some(*line) == base_name.as_deref() || Some(*line) == name_line {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Item Class:") {
            draft.item_class = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Rarity:") {
            if let Some(r) = parse_rarity(rest.trim()) {
                draft.rarity = r;
            } else {
                draft
                    .warnings
                    .push(format!("unknown rarity {:?}", rest.trim()));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Item Level:") {
            draft.ilvl = rest.trim().parse::<u8>().ok();
            continue;
        }
        if line.starts_with("Quality (") {
            match b.parse_quality(line) {
                Ok((label, quality)) => match b.catalyst_by_label(&label) {
                    Some(c) => draft.catalyst = Some((c, quality.min(20))),
                    None => draft
                        .warnings
                        .push(format!("unknown catalyst label {label:?}")),
                },
                Err(err) => draft.warnings.push(err),
            }
            continue;
        }
        if line.starts_with("Quality:") {
            draft
                .warnings
                .push("plain quality without a catalyst type; no catalyst assumed".to_string());
            continue;
        }
        if let Some(bit) = is_influence_line(line) {
            draft.influences |= 1 << bit;
            continue;
        }
        if *line == "Requirements:" || line.starts_with("Level:") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Note:") {
            let rest = rest.trim();
            let rest = rest
                .strip_prefix("~b/o")
                .or_else(|| rest.strip_prefix("~price"))
                .unwrap_or(rest)
                .trim();
            let mut parts = rest.split_whitespace();
            if let (Some(amount), Some(currency)) = (parts.next(), parts.next())
                && let Ok(amount) = amount.parse::<f64>()
            {
                let currency = currency.trim_end_matches('?').to_ascii_lowercase();
                let currency = match currency.as_str() {
                    "divine" => "divine".to_string(),
                    "chaos" => "chaos".to_string(),
                    "exalted" => "exalted".to_string(),
                    other => other.to_string(),
                };
                draft.buy_note = Some((amount, currency));
            }
            continue;
        }
        if line.ends_with("(enchant)") || line.ends_with("(implicit)") {
            continue;
        }
        if PROPERTY_PREFIXES.iter().any(|p| line.starts_with(p)) {
            continue;
        }
        if line.starts_with('[') && line.contains('|') {
            continue;
        }
        if implicit_lines.iter().any(|im| im == line) {
            continue;
        }
        let line = line
            .strip_suffix(" (crafted)")
            .or_else(|| line.strip_suffix(" (fractured)"))
            .unwrap_or(line);
        mod_lines.push(line);
    }

    let Some(base) = base else {
        draft.warnings.push(
            "no known base item name in the pasted text; requirements cannot be matched"
                .to_string(),
        );
        draft.unmatched = mod_lines.iter().map(|l| l.to_string()).collect();
        return draft;
    };

    let tags = b.item_tags(base, draft.influences);
    let item_class = b.base_at(base).class.clone();
    let ctx = crate::weights::SourceCtx::new(b, base, draft.influences);

    let candidates: Vec<Vec<Cand>> = mod_lines
        .iter()
        .map(|line| {
            // Pass 0: tiers that roll on the item as it is (or bench crafts). Pass 1, only when
            // pass 0 found nothing: any tier some modelled mechanic can place (influence,
            // essence, fossil, recombine), per `weights::sources`.
            let native = line_candidates(b, base, draft.catalyst, line, |_, m| {
                b.bench_by_mod
                    .get(&m.id)
                    .is_some_and(|e| b.bench[*e as usize].classes.contains(&item_class))
                    || (!m.fossil_only && spawn_weight(m, &tags) > 0)
            });
            if !native.is_empty() {
                return native;
            }
            line_candidates(b, base, draft.catalyst, line, |i, _| {
                !ctx.sources(i).is_empty()
            })
        })
        .collect();
    let shown = shown_stats(b);
    let chosen = assign(b, &shown, &candidates);

    let mut explained = vec![false; mod_lines.len()];
    let mut requirements: Vec<Requirement> = Vec::new();
    for c in &chosen {
        for &l in &c.lines {
            explained[l] = true;
        }
        // The requirement's value is the roll of the mod's first stat, as `p_value_ok` reads it.
        let first = b.mod_at(c.tiers[0]).stats.first().map(|s| s.stat);
        let min_value = c
            .lines
            .iter()
            .flat_map(|&l| candidates[l].iter())
            .find(|x| x.m == c.tiers[0] && first.is_some_and(|f| x.stats.contains(&f)))
            .and_then(|x| first.and_then(|f| x.value_of(f)))
            .unwrap_or_else(|| first_number(mod_lines[c.lines[0]]).map_or(0.0, |n| n.2));
        let m = b.mod_at(c.tiers[0]);
        requirements.push(Requirement {
            group: m.group,
            side: m.generation,
            accepted: c.tiers.clone(),
            min_value,
            label: mod_lines[c.lines[0]].to_string(),
        });
    }
    for (l, line) in mod_lines.iter().enumerate() {
        if !explained[l] && first_number(line).is_some() {
            draft.unmatched.push((*line).to_string());
        }
    }

    draft.requirements = requirements;
    draft.min_hits = draft.requirements.len() as u16;

    if draft.requirements.is_empty() {
        draft.warnings.push(
            "no requirement lines were understood; edit the requirements by hand".to_string(),
        );
    }
    draft
}

/// One reading of a line as (part of) one mod tier: the stats it shows and their values.
#[derive(Clone, Debug)]
struct Cand {
    m: u32,
    stats: Vec<u16>,
    values: Vec<f64>,
}

impl Cand {
    fn value_of(&self, stat: u16) -> Option<f64> {
        self.stats
            .iter()
            .position(|s| *s == stat)
            .map(|i| self.values[i])
    }
}

/// Whether some roll in `[lo, hi]` shows as `value` at `scale` (display truncates toward zero).
fn shows_as(lo: f64, hi: f64, value: f64, scale: f64) -> bool {
    if value >= 0.0 {
        lo < (value + 1.0) / scale && hi >= value / scale
    } else {
        lo <= value / scale && hi > (value - 1.0) / scale
    }
}

/// Every mod tier (passing `reachable`) that can show `line`, with the stats the line covers and
/// their values in stored units.
fn line_candidates(
    b: &Bundle,
    base: u16,
    catalyst: Option<(u8, u8)>,
    line: &str,
    reachable: impl Fn(u32, &crate::data::ModData) -> bool,
) -> Vec<Cand> {
    let text = line.strip_prefix('+').unwrap_or(line);
    // (value, precision): "3" reads to the unit, "50.3" to a tenth.
    let numbers: Vec<(f64, f64)> = all_numbers(text)
        .into_iter()
        .map(|(start, end, v)| {
            let decimals = text[start..end].split_once('.').map_or(0, |(_, d)| d.len());
            (v, 10f64.powi(-(decimals as i32)))
        })
        .collect();
    let mut readings: Vec<(String, Vec<(f64, f64)>)> =
        vec![(hash_all_numbers(text), numbers.clone())];
    if numbers.len() > 1 {
        if let Some(k) = hash_first_number(text) {
            readings.push((k, vec![numbers[0]]));
        }
        if let Some(k) = hash_last_number(text) {
            readings.push((k, vec![numbers[numbers.len() - 1]]));
        }
    }
    let mut out: Vec<Cand> = Vec::new();
    for (key, values) in readings {
        let Some(rows) = b.translations_by_key.get(&key) else {
            continue;
        };
        for &row in rows {
            let slots = &b.translations[row as usize].slots;
            if slots.len() != values.len() {
                continue;
            }
            let stats: Vec<u16> = slots.iter().map(|s| s.0).collect();
            // Rounded to 6 decimals so 0.7 / 0.01 reads as 70, not 69.99999999999999.
            let stored: Vec<f64> = slots
                .iter()
                .zip(&values)
                .map(|(s, (v, _))| ((v / s.1) * 1e6).round() / 1e6)
                .collect();
            for (i, m) in b.mods.iter().enumerate() {
                let i = i as u32;
                if out.iter().any(|c| c.m == i) || !reachable(i, m) {
                    continue;
                }
                let scale = 1.0 + effect(b, base, catalyst, i) / 100.0;
                let fits = slots
                    .iter()
                    .zip(&values)
                    .zip(&stored)
                    .all(|((slot, &(v, q)), &sv)| {
                        m.stats.iter().find(|s| s.stat == slot.0).is_some_and(|s| {
                            let f = slot.1;
                            if f.abs() == 1.0 {
                                return shows_as(s.min, s.max, sv, scale);
                            }
                            // Rounded or truncated to the shown precision: either way the stored
                            // value lies in (v - q/2, v + q) once scaled back.
                            let (mut a, mut z) =
                                ((v - q / 2.0) / (f * scale), (v + q) / (f * scale));
                            if f < 0.0 {
                                std::mem::swap(&mut a, &mut z);
                            }
                            s.min < z && s.max > a
                        })
                    });
                if fits {
                    out.push(Cand {
                        m: i,
                        stats: stats.clone(),
                        values: stored.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Stats some translation shows; a mod's other stats need no line of their own.
fn shown_stats(b: &Bundle) -> std::collections::HashSet<u16> {
    b.translations
        .iter()
        .flat_map(|t| t.slots.iter().map(|s| s.0))
        .collect()
}

/// One mod group read off the item: the tiers that fit every line it covers.
#[derive(Clone, Debug)]
struct Choice {
    tiers: Vec<u32>,
    lines: Vec<usize>,
}

/// Most search steps [`assign`] takes before settling for the best assignment found.
const ASSIGN_BUDGET: usize = 200_000;

/// Read the lines as mods: each line belongs to at most one mod, each group appears once, and a
/// mod with several shown stats (a hybrid) claims one line per stat. Fewest unexplained lines
/// wins, then fewest mods; ties go to the first found in line order.
fn assign(b: &Bundle, shown: &std::collections::HashSet<u16>, cands: &[Vec<Cand>]) -> Vec<Choice> {
    struct Search<'a> {
        b: &'a Bundle,
        shown: &'a std::collections::HashSet<u16>,
        cands: &'a [Vec<Cand>],
        used: Vec<bool>,
        groups: Vec<(u16, crate::item::Side)>,
        cur: Vec<Choice>,
        skipped: usize,
        best: Option<(usize, usize, Vec<Choice>)>,
        steps: usize,
    }
    impl Search<'_> {
        /// The options for line `i`: (lines claimed, tiers), in first-seen order.
        fn options(&self, i: usize) -> Vec<(Vec<usize>, Vec<u32>)> {
            let mut opts: Vec<(u16, Vec<usize>, Vec<u32>)> = Vec::new();
            for c in &self.cands[i] {
                let m = self.b.mod_at(c.m);
                if self.groups.contains(&(m.group, m.generation)) {
                    continue;
                }
                // Each other shown stat of this tier needs a line of its own; try every choice.
                let needed: Vec<u16> = m
                    .stats
                    .iter()
                    .map(|s| s.stat)
                    .filter(|s| self.shown.contains(s) && !c.stats.contains(s))
                    .collect();
                let mut sets: Vec<Vec<usize>> = vec![vec![i]];
                for s in needed {
                    let mut next = Vec::new();
                    for set in &sets {
                        let covers = |l: usize| {
                            self.cands[l]
                                .iter()
                                .any(|x| x.m == c.m && x.stats.contains(&s))
                        };
                        if set.iter().any(|&l| covers(l)) {
                            next.push(set.clone());
                            continue;
                        }
                        for j in 0..self.cands.len() {
                            if self.used[j] || set.contains(&j) {
                                continue;
                            }
                            if covers(j) {
                                let mut more = set.clone();
                                more.push(j);
                                next.push(more);
                            }
                        }
                    }
                    sets = next;
                }
                for mut lines in sets {
                    lines.sort_unstable();
                    match opts
                        .iter_mut()
                        .find(|(g, l, _)| *g == m.group && *l == lines)
                    {
                        Some((_, _, tiers)) => {
                            if !tiers.contains(&c.m) {
                                tiers.push(c.m)
                            }
                        }
                        None => opts.push((m.group, lines, vec![c.m])),
                    }
                }
            }
            opts.into_iter().map(|(_, l, t)| (l, t)).collect()
        }

        fn run(&mut self) {
            self.steps += 1;
            if self.steps > ASSIGN_BUDGET {
                return;
            }
            if let Some((skipped, _, _)) = &self.best
                && self.skipped > *skipped
            {
                return;
            }
            let Some(i) = (0..self.cands.len()).find(|&i| !self.used[i]) else {
                let better = match &self.best {
                    None => true,
                    Some((s, n, _)) => (self.skipped, self.cur.len()) < (*s, *n),
                };
                if better {
                    self.best = Some((self.skipped, self.cur.len(), self.cur.clone()));
                }
                return;
            };
            for (lines, tiers) in self.options(i) {
                let m = self.b.mod_at(tiers[0]);
                for &l in &lines {
                    self.used[l] = true;
                }
                self.groups.push((m.group, m.generation));
                self.cur.push(Choice { tiers, lines });
                self.run();
                let c = self.cur.pop().expect("pushed above");
                self.groups.pop();
                for &l in &c.lines {
                    self.used[l] = false;
                }
            }
            self.used[i] = true;
            self.skipped += 1;
            self.run();
            self.skipped -= 1;
            self.used[i] = false;
        }
    }
    let mut search = Search {
        b,
        shown,
        cands,
        used: vec![false; cands.len()],
        groups: Vec::new(),
        cur: Vec::new(),
        skipped: 0,
        best: None,
        steps: 0,
    };
    search.run();
    let mut out = search.best.map(|x| x.2).unwrap_or_default();
    out.sort_by_key(|c| c.lines[0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_magic_item_name() {
        let b = crate::test_bundle();
        let d = parse_target(b, include_str!("../../../fixtures/amulet_gem_mods.txt"));
        assert_eq!(
            d.base.map(|i| b.base_at(i).name.clone()),
            Some("Lapis Amulet".to_string())
        );
        assert_eq!(d.rarity, Rarity::Magic);
        assert!(d.unmatched.is_empty(), "{:?}", d.unmatched);
        assert_eq!(d.requirements.len(), 1);
        assert!(
            d.requirements[0]
                .accepted
                .iter()
                .any(|i| b.mod_at(*i).id == "GlobalLightningGemLevel1")
        );
    }

    #[test]
    fn parses_the_simplex_fixture() {
        let b = crate::test_bundle();
        let text = include_str!("../../../fixtures/simplex_target.txt");
        let d = parse_target(b, text);
        assert_eq!(d.item_class.as_deref(), Some("Amulets"));
        assert_eq!(
            d.base.map(|i| b.base_at(i).name.clone()),
            Some("Simplex Amulet".to_string())
        );
        assert_eq!(d.rarity, Rarity::Rare);
        assert_eq!(d.ilvl, Some(84));
        assert_eq!(d.influences, 0b100001);
        assert_eq!(
            d.catalyst
                .map(|(c, q)| (b.catalysts[c as usize].name.clone(), q)),
            Some(("Intrinsic Catalyst".to_string(), 20))
        );
        assert_eq!(d.buy_note, Some((800.0, "divine".to_string())));
        assert_eq!(d.min_hits, 3);
        assert!(d.unmatched.is_empty(), "{:?}", d.unmatched);
        let ids = |r: &Requirement| {
            r.accepted
                .iter()
                .map(|i| b.mod_at(*i).id.clone())
                .collect::<Vec<_>>()
        };
        let found: Vec<(String, Vec<String>)> = d
            .requirements
            .iter()
            .map(|r| (b.group(r.group).to_string(), ids(r)))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    "PercentageAllAttributes".to_string(),
                    vec!["PercentageAllAttributesUberShaper1".to_string()]
                ),
                (
                    "PercentageStrength".to_string(),
                    vec!["PercentageStrengthBodyInfluence2_".to_string()]
                ),
                (
                    "DamagePer15Attributes".to_string(),
                    vec!["DamagePer15StrengthInfluence1".to_string()]
                ),
            ]
        );
        assert_eq!(d.requirements[0].min_value, 19.0);
        assert_eq!(d.requirements[1].min_value, 26.0);
        assert_eq!(d.requirements[2].min_value, 2.0);
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    }

    /// (group, accepted mod ids, min value) per requirement.
    fn found(b: &Bundle, d: &TargetDraft) -> Vec<(String, Vec<String>, f64)> {
        d.requirements
            .iter()
            .map(|r| {
                let ids = r.accepted.iter().map(|i| b.mod_at(*i).id.clone()).collect();
                (b.group(r.group).to_string(), ids, r.min_value)
            })
            .collect()
    }

    #[test]
    fn parses_a_physical_weapon() {
        let b = crate::test_bundle();
        let d = parse_target(b, include_str!("../../../fixtures/vaal_axe_phys.txt"));
        assert_eq!(
            d.base.map(|i| b.base_at(i).name.clone()),
            Some("Vaal Axe".to_string())
        );
        assert!(d.unmatched.is_empty(), "{:?}", d.unmatched);
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        let f = found(b, &d);
        for r in &f {
            println!("{r:?}");
        }
        // 74% can only be the hybrid (153% cannot), so the accuracy line is its second half.
        assert_eq!(f.len(), 5, "{f:?}");
        assert_eq!(
            f[0].1,
            vec!["LocalIncreasedPhysicalDamagePercentAndAccuracyRating7".to_string()]
        );
        assert_eq!(f[0].2, 74.0);
        assert_eq!(
            f[1].1,
            vec!["LocalIncreasedPhysicalDamagePercent6".to_string()]
        );
        assert_eq!(f[1].2, 153.0);
        assert_eq!(f[2].1, vec!["LocalAddedPhysicalDamageTwoHand9".to_string()]);
        assert_eq!(f[2].2, 37.0);
        assert_eq!(d.min_hits, 5);
        // Battle Royale copies are not real tiers, so no line may resolve to one.
        assert!(
            f.iter()
                .all(|r| r.1.iter().all(|id| !id.contains("Royale"))),
            "{f:?}"
        );
    }

    #[test]
    fn parses_a_hybrid_beside_pure_percent_physical() {
        let b = crate::test_bundle();
        let text = "Item Class: Thrusting One Hand Swords\nRarity: Rare\nGrim Needle\nJewelled Foil\nItem Level: 84\n179% increased Physical Damage\n79% increased Physical Damage\n+200 to Accuracy Rating\nAdds 29 to 52 Physical Damage\n";
        let d = parse_target(b, text);
        assert!(d.unmatched.is_empty(), "{:?}", d.unmatched);
        let f = found(b, &d);
        for r in &f {
            println!("{r:?}");
        }
        assert_eq!(f.len(), 3, "{f:?}");
        assert_eq!(
            f[0].1,
            vec!["LocalIncreasedPhysicalDamagePercent8".to_string()]
        );
        assert_eq!(
            f[1].1,
            vec!["LocalIncreasedPhysicalDamagePercentAndAccuracyRating8".to_string()]
        );
        assert!(
            f[2].1
                .iter()
                .all(|id| id.starts_with("LocalAddedPhysicalDamage"))
        );
    }

    #[test]
    fn strips_a_leading_plus() {
        let b = crate::test_bundle();
        let text = "Item Class: Amulets\nRarity: Rare\nSimplex Amulet\nItem Level: 84\nShaper Item\nWarlord Item\n+2% increased Damage per 15 Strength\n";
        let d = parse_target(b, text);
        assert!(d.unmatched.is_empty(), "{:?}", d.unmatched);
        assert_eq!(d.requirements.len(), 1);
        assert_eq!(d.requirements[0].min_value, 2.0);
        assert_eq!(
            d.requirements[0].label,
            "+2% increased Damage per 15 Strength"
        );
    }

    #[test]
    fn parses_values_shown_in_other_units() {
        let b = crate::test_bundle();
        let one = |text: &str| {
            let d = parse_target(b, text);
            assert!(d.unmatched.is_empty(), "{text}: {:?}", d.unmatched);
            let f = found(b, &d);
            assert_eq!(f.len(), 1, "{text}: {f:?}");
            f.into_iter().next().expect("one requirement")
        };
        let crit = one(
            "Item Class: Helmets\nRarity: Rare\nX\nArchdemon Crown\nItem Level: 87\nShaper Item\nSocketed Spells have +3% to Critical Strike Chance\n",
        );
        assert_eq!(
            crit.1,
            vec!["SocketedSpellCriticalStrikeChanceUber3".to_string()]
        );
        assert_eq!(crit.2, 300.0);

        let amulet = "Item Class: Amulets\nRarity: Rare\nX\nLapis Amulet\nItem Level: 84\n";
        let regen = one(&format!("{amulet}Regenerate 50.3 Life per second\n"));
        assert_eq!(regen.1, vec!["LifeRegeneration7".to_string()]);
        assert!((regen.2 - 3018.0).abs() < 1e-9, "{}", regen.2);
        let regen = one(&format!("{amulet}Regenerate 64.1 Life per second\n"));
        assert_eq!(regen.1, vec!["LifeRegeneration8_".to_string()]);
        let leech = one(&format!(
            "{amulet}0.7% of Physical Attack Damage Leeched as Life\n"
        ));
        assert_eq!(leech.1, vec!["LifeLeechPermyriadSuffix2".to_string()]);
        assert_eq!(leech.2, 70.0);
    }
}
