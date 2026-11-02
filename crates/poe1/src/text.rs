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

/// Build a target draft from pasted tooltip text.
pub fn parse_target(b: &Bundle, text: &str) -> TargetDraft {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "--------")
        .collect();

    let base = lines.iter().find_map(|l| b.base_by_name(l));

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
        if Some(*line) == base_name.as_deref() {
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
        if line.starts_with("Allocates ") && line.ends_with("(enchant)") {
            continue;
        }
        if line.starts_with('[') && line.contains('|') {
            continue;
        }
        if implicit_lines.iter().any(|im| im == line) {
            continue;
        }
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

    struct Pending {
        group: u16,
        side: crate::item::Side,
        accepted: Vec<u32>,
        min_value: f64,
        label: String,
    }
    let mut pending: Vec<Pending> = Vec::new();
    for line in &mod_lines {
        let Some((_, _, raw_value)) = first_number(line) else {
            continue;
        };
        // Tooltips show a leading `+` on positive values; the translation keys do not.
        let text = line.strip_prefix('+').unwrap_or(line);
        let rows = hash_first_number(text)
            .and_then(|key| b.translations_by_key.get(&key).cloned())
            .or_else(|| {
                hash_last_number(text).and_then(|key| b.translations_by_key.get(&key).cloned())
            });
        let Some(rows) = rows else {
            draft.unmatched.push((*line).to_string());
            continue;
        };
        let mut any = false;
        for row in rows {
            let translation = &b.translations[row as usize];
            let value = if translation.negate {
                -raw_value
            } else {
                raw_value
            };
            for (i, m) in b.mods.iter().enumerate() {
                let bench_ok = b
                    .bench_by_mod
                    .get(&m.id)
                    .is_some_and(|e| b.bench[*e as usize].classes.contains(&item_class));
                if !bench_ok && spawn_weight(m, &tags) == 0 {
                    continue;
                }
                let Some(stat) = m.stats.iter().find(|s| s.stat == translation.stat) else {
                    continue;
                };
                let scale = 1.0 + effect(b, base, draft.catalyst, i as u32) / 100.0;
                let (lo, hi) = if translation.negate {
                    (-stat.max, -stat.min)
                } else {
                    (stat.min, stat.max)
                };
                if lo < (value + 1.0) / scale && hi >= value / scale {
                    any = true;
                    match pending
                        .iter_mut()
                        .find(|p| p.group == m.group && p.side == m.generation)
                    {
                        Some(p) => {
                            if !p.accepted.contains(&(i as u32)) {
                                p.accepted.push(i as u32);
                            }
                        }
                        None => pending.push(Pending {
                            group: m.group,
                            side: m.generation,
                            accepted: vec![i as u32],
                            min_value: value,
                            label: (*line).to_string(),
                        }),
                    }
                }
            }
        }
        if !any {
            draft.unmatched.push((*line).to_string());
        }
    }

    let requirements: Vec<Requirement> = pending
        .into_iter()
        .map(|p| Requirement {
            group: p.group,
            side: p.side,
            accepted: p.accepted,
            min_value: p.min_value,
            label: p.label,
        })
        .collect();

    draft.requirements = requirements;
    draft.min_hits = draft.requirements.len() as u16;

    if draft.requirements.is_empty() {
        draft.warnings.push(
            "no requirement lines were understood; edit the requirements by hand".to_string(),
        );
    }
    draft
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
