//! Game text for concrete items: `render` prints an item as the game shows it and `parse` reads
//! a pasted copy back into an `ItemSpec`. Stat lines come from the bundle's `stat_translations`
//! entries in file order, which is the order the game prints them, and a paste's lines are matched
//! back to modifiers with a candidate-and-assign search: each line is read as (part of) one
//! modifier tier, and the assignment that explains the most lines with the fewest modifiers wins.

use std::collections::HashSet;

use crate::data::{
    BenchAction, Condition, Data, Decimals, Influence, ModData, ModKind, Rarity, Side,
};
use crate::item::{AffixSpec, Item, ItemError, ItemSpec, ModSpec, Roll, Socket, display};

/// Everything the reader recovered from a pasted item, plus whatever it could not explain.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub spec: ItemSpec,
    pub warnings: Vec<String>,
}

/// Why a pasted item could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("no known base item in the pasted text")]
    NoBase,
    #[error("unknown rarity \"{0}\"")]
    UnknownRarity(String),
    #[error("unidentified items cannot be read")]
    Unidentified,
    #[error(transparent)]
    Item(#[from] ItemError),
}

fn rarity_text(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Normal => "Normal",
        Rarity::Magic => "Magic",
        Rarity::Rare => "Rare",
        Rarity::Unique => "Unique",
    }
}

fn side_text(side: Side) -> &'static str {
    match side {
        Side::Prefix => "Prefix",
        Side::Suffix => "Suffix",
    }
}

fn socket_letter(socket: Socket) -> char {
    match socket {
        Socket::R => 'R',
        Socket::G => 'G',
        Socket::B => 'B',
        Socket::W => 'W',
        Socket::A => 'A',
    }
}

/// A number as a stat line shows it: `value × factor`, formatted by the slot's decimals.
fn format_value(value: f64, decimals: Decimals) -> String {
    let digits = match decimals {
        Decimals::Fixed(n) => return format!("{value:.n$}", n = n as usize),
        Decimals::UpTo(n) => n as usize,
    };
    let text = format!("{value:.digits$}");
    match text.split_once('.') {
        Some((whole, fraction)) => {
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                whole.to_string()
            } else {
                format!("{whole}.{fraction}")
            }
        }
        None => text,
    }
}

/// The stat lines a set of `(stat, value)` pairs prints, in the bundle's translation order. An
/// entry applies when one of its stats is present; a missing entry stat counts as 0. The first
/// variant whose conditions hold wins; none printing means the entry prints nothing.
pub fn stat_lines(data: &Data, stats: &[(u16, i64)]) -> Vec<String> {
    let mut out = Vec::new();
    for entry in &data.translations {
        if !entry
            .stats
            .iter()
            .any(|s| stats.iter().any(|(t, _)| t == s))
        {
            continue;
        }
        let values: Vec<i64> = entry
            .stats
            .iter()
            .map(|s| stats.iter().find(|(t, _)| t == s).map_or(0, |(_, v)| *v))
            .collect();
        let Some(variant) = entry.variants.iter().find(|v| {
            v.conditions
                .iter()
                .enumerate()
                .all(|(i, c)| c.holds(values.get(i).copied().unwrap_or(0)))
        }) else {
            continue;
        };
        let mut text = String::new();
        let mut slots = variant.slots.iter();
        let mut rest = variant.string.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let Ok(k) = rest[open + 1..open + close].parse::<usize>() else {
                text.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
                continue;
            };
            text.push_str(&rest[..open]);
            let Some(slot) = slots.next() else { break };
            let value = values.get(k).copied().unwrap_or(0) as f64 * slot.factor;
            let mut shown = format_value(value, slot.decimals);
            if slot.plus && value > 0.0 {
                shown.insert(0, '+');
            }
            text.push_str(&shown);
            rest = &rest[open + close + 1..];
        }
        text.push_str(rest);
        out.extend(text.split('\n').map(str::to_string));
    }
    out
}

/// The lines of one modifier: its stats as shown when rolled, else its RePoE text.
fn own_lines(data: &Data, item: &Item, roll: &Roll, implicit: bool) -> Vec<String> {
    let md = data.mod_at(roll.m);
    match &roll.values {
        Some(values) => {
            let effect = item.effect(data, roll, implicit);
            let stats: Vec<(u16, i64)> = md
                .stats
                .iter()
                .zip(values)
                .map(|(s, &v)| (s.stat, i64::from(display(v, effect))))
                .collect();
            stat_lines(data, &stats)
        }
        None => text_lines(md),
    }
}

/// A modifier's RePoE text, one entry per line.
fn text_lines(md: &ModData) -> Vec<String> {
    md.text
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// An item as the game prints it: header, properties, sockets, item level, enchantments,
/// implicits, explicits and the influence and flag lines.
pub fn render(data: &Data, item: &Item) -> String {
    let mut sections: Vec<Vec<String>> = Vec::new();

    sections.push(vec![
        format!("Item Class: {}", item.class(data).name),
        format!("Rarity: {}", rarity_text(item.rarity)),
        data.base_at(item.base).name.clone(),
    ]);

    let mut properties = Vec::new();
    if item.quality > 0 {
        match item.catalyst {
            Some(c) => properties.push(format!(
                "Quality ({} Modifiers): +{}% (augmented)",
                data.catalysts[c as usize].label, item.quality
            )),
            None => properties.push(format!("Quality: +{}% (augmented)", item.quality)),
        }
    }
    if let Some(defences) = item.defences {
        for (i, name) in ["Armour", "Evasion Rating", "Energy Shield", "Ward"]
            .iter()
            .enumerate()
        {
            if defences[i] != 0 {
                properties.push(format!("{name}: {} (augmented)", defences[i]));
            }
        }
    }
    if item.intangibility != 0 {
        // The game's copy carries the stat's display markup on this line.
        properties.push(format!(
            "[Intangibility|Intangibility]: {}%",
            item.intangibility
        ));
    }
    if item.memory_strands != 0 {
        properties.push(format!("Memory Strands: {}", item.memory_strands));
    }
    if !properties.is_empty() {
        sections.push(properties);
    }

    if !item.sockets.is_empty() {
        let notation = item
            .sockets
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|s| socket_letter(*s).to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            })
            .collect::<Vec<_>>()
            .join(" ");
        sections.push(vec![format!("Sockets: {notation}")]);
    }

    sections.push(vec![format!("Item Level: {}", item.item_level)]);

    let mut enchants = Vec::new();
    for roll in &item.enchants {
        for line in own_lines(data, item, roll, true) {
            enchants.push(format!("{line} (enchant)"));
        }
    }
    if !enchants.is_empty() {
        sections.push(enchants);
    }

    let mut implicits = Vec::new();
    for roll in &item.implicits {
        implicits.extend(own_lines(data, item, roll, true));
    }
    if !implicits.is_empty() {
        sections.push(implicits);
    }

    let mut fractured = Vec::new();
    let mut merged: Vec<(u16, i64)> = Vec::new();
    let mut has_merged = false;
    let mut unrolled = Vec::new();
    let mut crafted = Vec::new();
    let mut veiled = Vec::new();
    for affix in item.explicits() {
        let md = data.mod_at(affix.roll.m);
        if md.kind == ModKind::Veiled {
            let side = md.side.unwrap_or(Side::Prefix);
            veiled.push(format!("Veiled {}", side_text(side)));
            continue;
        }
        let tagged = |lines: Vec<String>, tag: &str| -> Vec<String> {
            lines.into_iter().map(|l| format!("{l} ({tag})")).collect()
        };
        if md.kind == ModKind::Crafted {
            crafted.extend(tagged(own_lines(data, item, &affix.roll, false), "crafted"));
            continue;
        }
        if affix.fractured {
            fractured.extend(tagged(
                own_lines(data, item, &affix.roll, false),
                "fractured",
            ));
            continue;
        }
        match &affix.roll.values {
            Some(values) => {
                has_merged = true;
                let effect = item.effect(data, &affix.roll, false);
                for (s, &v) in md.stats.iter().zip(values) {
                    let shown = i64::from(display(v, effect));
                    match merged.iter_mut().find(|(t, _)| *t == s.stat) {
                        Some((_, sum)) => *sum += shown,
                        None => merged.push((s.stat, shown)),
                    }
                }
            }
            None => unrolled.extend(text_lines(md)),
        }
    }
    let mut explicits = fractured;
    if has_merged {
        explicits.extend(stat_lines(data, &merged));
    }
    explicits.extend(unrolled);
    explicits.extend(crafted);
    explicits.extend(veiled);
    if !explicits.is_empty() {
        sections.push(explicits);
    }

    let mut flags = Vec::new();
    for influence in Influence::ALL {
        if item.has_influence(influence) {
            flags.push(format!("{} Item", influence.name()));
        }
    }
    if item.corrupted {
        flags.push("Corrupted".to_string());
    }
    if item.mirrored {
        flags.push("Mirrored".to_string());
    }
    if item.split {
        flags.push("Split".to_string());
    }
    if item.synthesised {
        flags.push("Synthesised Item".to_string());
    }
    if !flags.is_empty() {
        sections.push(flags);
    }

    sections
        .into_iter()
        .map(|s| s.join("\n"))
        .collect::<Vec<_>>()
        .join("\n--------\n")
}

// ---- reading a paste ----

/// Replace `[a|b]` markup with `b`.
fn strip_markup(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        match rest[open + 1..open + close].rsplit_once('|') {
            Some((_, shown)) => {
                out.push_str(&rest[..open]);
                out.push_str(shown);
            }
            None => out.push_str(&rest[..=open]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
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

/// Whether `needle` occurs in `hay` on word boundaries.
fn contains_words(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + needle.len()..].chars().next();
        before.is_none_or(|c| c == ' ') && after.is_none_or(|c| c == ' ')
    })
}

/// One numeric token: the digits, the sign that directly precedes them (when it is a sign rather
/// than a range dash), and its value.
#[derive(Clone, Copy, Debug)]
struct Number {
    digits: usize,
    start: usize,
    end: usize,
    value: f64,
}

fn numbers_in(s: &str) -> Vec<Number> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j + 1 < bytes.len() && bytes[j] == b'.' && bytes[j + 1].is_ascii_digit() {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
        }
        let sign = if i > 0 && (bytes[i - 1] == b'-' || bytes[i - 1] == b'+') {
            let before = i >= 2 && (bytes[i - 2].is_ascii_alphanumeric() || bytes[i - 2] == b'.');
            (!before).then(|| (i - 1, bytes[i - 1]))
        } else {
            None
        };
        let mut value: f64 = s[i..j].parse().unwrap_or(0.0);
        if matches!(sign, Some((_, b'-'))) {
            value = -value;
        }
        out.push(Number {
            digits: i,
            start: sign.map_or(i, |(pos, _)| pos),
            end: j,
            value,
        });
        i = j;
    }
    out
}

/// `s` with each number replaced by `#`; `strip_signs` also swallows the sign in front of it.
fn hash_numbers(s: &str, strip_signs: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for n in numbers_in(s) {
        let from = if strip_signs { n.start } else { n.digits };
        out.push_str(&s[last..from]);
        out.push('#');
        last = n.end;
    }
    out.push_str(&s[last..]);
    out
}

/// `s` with the single number `n` replaced by `#`.
fn hash_one(s: &str, n: Number, strip_signs: bool) -> String {
    let from = if strip_signs { n.start } else { n.digits };
    format!("{}#{}", &s[..from], &s[n.end..])
}

/// The placeholder keys a line can read as, with the shown values they carry: the whole line, the
/// first number alone and the last number alone (a line may show only part of an entry).
fn readings(line: &str) -> Vec<(String, Vec<(f64, f64)>)> {
    let numbers = numbers_in(line);
    if numbers.is_empty() {
        return vec![(line.to_string(), Vec::new())];
    }
    let values: Vec<(f64, f64)> = numbers
        .iter()
        .map(|n| {
            let decimals = line[n.digits..n.end]
                .split_once('.')
                .map_or(0, |(_, d)| d.len());
            (n.value, 10f64.powi(-(decimals as i32)))
        })
        .collect();
    let mut out: Vec<(String, Vec<(f64, f64)>)> = Vec::new();
    let mut push = |key: String, vals: Vec<(f64, f64)>| {
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, vals));
        }
    };
    for strip in [true, false] {
        push(hash_numbers(line, strip), values.clone());
        if numbers.len() > 1 {
            push(hash_one(line, numbers[0], strip), vec![values[0]]);
            push(
                hash_one(line, numbers[numbers.len() - 1], strip),
                vec![values[values.len() - 1]],
            );
        }
    }
    out
}

/// One reading of a line as (part of) one modifier tier: the stats it shows, their values and the
/// stored rolls when they can be recovered.
#[derive(Clone, Debug)]
struct Cand {
    m: u32,
    stats: Vec<u16>,
    values: Vec<f64>,
    stored: Vec<Option<i32>>,
    /// The line is the sum of this mod's stat and another mod's; `lo`..`hi` is what this mod
    /// shows of it.
    partial: bool,
    lo: f64,
    hi: f64,
}

/// Whether some roll in `[lo, hi]` shows as `value` at `scale` (display truncates toward zero).
fn shows_as(lo: f64, hi: f64, value: f64, scale: f64) -> bool {
    if value >= 0.0 {
        lo < (value + 1.0) / scale && hi >= value / scale
    } else {
        lo <= value / scale && hi > (value - 1.0) / scale
    }
}

/// The stored roll in `[min, max]` that shows as `shown` through `display` and the slot's factor.
fn recover(min: i32, max: i32, effect: i16, factor: f64, shown: f64) -> Option<i32> {
    if factor == 0.0 {
        return None;
    }
    let target = (shown / factor * 1e6).round() / 1e6;
    let mut best: Option<(f64, i32)> = None;
    for roll in min..=max {
        let diff = (f64::from(display(roll, effect)) - target).abs();
        if best.is_none_or(|(seen, _)| diff < seen) {
            best = Some((diff, roll));
        }
        if diff == 0.0 {
            break;
        }
    }
    best.filter(|(diff, _)| *diff <= 0.5).map(|(_, roll)| roll)
}

/// Every mod tier passing `reachable` that can show `line`, with the stats the line covers.
fn line_candidates(
    data: &Data,
    effect: &dyn Fn(u32) -> i16,
    line: &str,
    reachable: &dyn Fn(u32) -> bool,
) -> Vec<Cand> {
    let mut out: Vec<Cand> = Vec::new();
    for (key, values) in readings(line) {
        if values.is_empty() {
            continue;
        }
        for &(entry_index, variant_index) in data.translations_keyed(&key) {
            let entry = &data.translations[entry_index as usize];
            let variant = &entry.variants[variant_index as usize];
            if variant.slots.len() != values.len() {
                continue;
            }
            let stats: Vec<u16> = variant
                .slots
                .iter()
                .map(|s| entry.stats[s.k as usize])
                .collect();
            for (index, md) in data.mods.iter().enumerate() {
                let m = index as u32;
                if out.iter().any(|c| c.m == m) || !reachable(m) {
                    continue;
                }
                let eff = effect(m);
                let scale = 1.0 + f64::from(eff) / 100.0;
                let fits = variant
                    .slots
                    .iter()
                    .zip(&values)
                    .all(|(slot, &(shown, q))| {
                        md.stats
                            .iter()
                            .find(|s| s.stat == entry.stats[slot.k as usize])
                            .is_some_and(|s| {
                                let factor = slot.factor;
                                if factor.abs() == 1.0 {
                                    return shows_as(
                                        f64::from(s.min),
                                        f64::from(s.max),
                                        shown,
                                        scale,
                                    );
                                }
                                let (mut a, mut z) = (
                                    (shown - q / 2.0) / (factor * scale),
                                    (shown + q) / (factor * scale),
                                );
                                if factor < 0.0 {
                                    std::mem::swap(&mut a, &mut z);
                                }
                                f64::from(s.min) < z && f64::from(s.max) > a
                            })
                    });
                if !fits {
                    continue;
                }
                let stored: Vec<Option<i32>> = variant
                    .slots
                    .iter()
                    .zip(&values)
                    .map(|(slot, &(shown, _))| {
                        let s = md
                            .stats
                            .iter()
                            .find(|s| s.stat == entry.stats[slot.k as usize])?;
                        recover(s.min, s.max, eff, slot.factor, shown)
                    })
                    .collect();
                out.push(Cand {
                    m,
                    stats: stats.clone(),
                    values: values.iter().map(|(v, _)| *v).collect(),
                    stored,
                    partial: false,
                    lo: 0.0,
                    hi: 0.0,
                });
            }
        }
    }
    out
}

/// Readings of a line with no number as a zero-slot variant: the mods with a stat of that entry
/// whose range meets the variant's conditions.
fn text_candidates(data: &Data, line: &str, reachable: &dyn Fn(u32) -> bool) -> Vec<Cand> {
    let mut out: Vec<Cand> = Vec::new();
    for &(entry_index, variant_index) in data.translations_keyed(line) {
        let entry = &data.translations[entry_index as usize];
        let variant = &entry.variants[variant_index as usize];
        if !variant.slots.is_empty() {
            continue;
        }
        for (index, md) in data.mods.iter().enumerate() {
            let m = index as u32;
            if out.iter().any(|c| c.m == m) || !reachable(m) {
                continue;
            }
            let mut stored = Vec::new();
            let mut ok = true;
            for (k, stat) in entry.stats.iter().enumerate() {
                let Some(range) = md.stats.iter().find(|s| s.stat == *stat) else {
                    ok = false;
                    break;
                };
                if let Some(condition) = variant.conditions.get(k)
                    && !range_meets(condition, range.min, range.max)
                {
                    ok = false;
                    break;
                }
                stored.push((range.min == range.max).then_some(range.min));
            }
            if !ok {
                continue;
            }
            out.push(Cand {
                m,
                stats: entry.stats.clone(),
                values: Vec::new(),
                stored,
                partial: false,
                lo: 0.0,
                hi: 0.0,
            });
        }
    }
    out
}

/// Whether any roll in `[min, max]` meets a variant condition.
fn range_meets(condition: &Condition, min: i32, max: i32) -> bool {
    condition.holds(i64::from(min)) || condition.holds(i64::from(max))
}

/// Readings of a one-number line as one share of a sum: every reachable tier with a stat the line
/// shows (factor 1) whose smallest roll is below the shown value.
fn partial_candidates(
    data: &Data,
    effect: &dyn Fn(u32) -> i16,
    line: &str,
    reachable: &dyn Fn(u32) -> bool,
) -> Vec<Cand> {
    let numbers = numbers_in(line);
    let [number] = numbers[..] else {
        return Vec::new();
    };
    let shown = number.value;
    let mut out: Vec<Cand> = Vec::new();
    for (key, _) in readings(line) {
        for &(entry_index, variant_index) in data.translations_keyed(&key) {
            let entry = &data.translations[entry_index as usize];
            let variant = &entry.variants[variant_index as usize];
            let slot = match variant.slots.as_slice() {
                [slot] => slot,
                _ => continue,
            };
            if slot.factor != 1.0 {
                continue;
            }
            let stat = entry.stats[slot.k as usize];
            for (index, md) in data.mods.iter().enumerate() {
                let m = index as u32;
                if out.iter().any(|c| c.m == m) || !reachable(m) {
                    continue;
                }
                let Some(range) = md.stats.iter().find(|s| s.stat == stat) else {
                    continue;
                };
                let eff = effect(m);
                let lo = f64::from(display(range.min, eff));
                let hi = f64::from(display(range.max, eff));
                if lo < shown {
                    out.push(Cand {
                        m,
                        stats: vec![stat],
                        values: vec![shown],
                        stored: vec![None],
                        partial: true,
                        lo,
                        hi,
                    });
                }
            }
        }
    }
    out
}

/// One pool of lines to match: the data, the magnitude bonus each mod's values show with, the
/// two reachability passes and the stats any translation shows.
struct Pools<'a> {
    data: &'a Data,
    effect: &'a dyn Fn(u32) -> i16,
    first: &'a dyn Fn(u32) -> bool,
    second: &'a dyn Fn(u32) -> bool,
    shown: &'a HashSet<u16>,
}

impl Pools<'_> {
    /// Every mod tier passing `reach` that can show `line`.
    fn candidates_with(&self, line: &str, reach: &dyn Fn(u32) -> bool) -> Vec<Cand> {
        if numbers_in(line).is_empty() {
            text_candidates(self.data, line, reach)
        } else {
            line_candidates(self.data, self.effect, line, reach)
        }
    }

    /// The wider pass alone: every modelled source, with a summed-line reading as a last resort.
    fn wide_candidates(&self, line: &str) -> Vec<Cand> {
        let direct = self.candidates_with(line, self.second);
        if !direct.is_empty() || numbers_in(line).is_empty() {
            direct
        } else {
            partial_candidates(self.data, self.effect, line, self.second)
        }
    }

    /// Every mod tier passing either pass that can show `line`.
    fn candidates(&self, line: &str) -> Vec<Cand> {
        let direct = self.candidates_with(line, self.first);
        if !direct.is_empty() {
            direct
        } else {
            self.wide_candidates(line)
        }
    }

    /// Candidate lists for every line, natural tiers before bench crafts.
    fn prepare(&self, lines: &[String], candidates: impl Fn(&str) -> Vec<Cand>) -> Vec<Vec<Cand>> {
        let mut out: Vec<Vec<Cand>> = lines.iter().map(|line| candidates(line)).collect();
        for list in &mut out {
            list.sort_by_key(|c| u8::from(self.data.mod_at(c.m).kind == ModKind::Crafted));
        }
        out
    }

    /// Match the lines and report what the pool could not explain.
    fn match_pool(&self, lines: &[String], warnings: &mut Vec<String>) -> Vec<AffixSpec> {
        let mut cands = self.prepare(lines, |line| self.candidates(line));
        let mut chosen = assign(self.data, self.shown, &cands);
        let unexplained = |chosen: &[Choice]| {
            (0..lines.len())
                .filter(|i| !chosen.iter().any(|c| c.lines.contains(i)))
                .count()
        };
        let best = unexplained(&chosen);
        if best > 0 {
            // A line whose only readings are incomplete hides a later-pass modifier: solve the
            // whole pool again with the wider pass.
            let wide = self.prepare(lines, |line| self.wide_candidates(line));
            let wide_chosen = assign(self.data, self.shown, &wide);
            if unexplained(&wide_chosen) < best {
                cands = wide;
                chosen = wide_chosen;
            }
        }
        let affixes = build_affixes(self.data, &cands, &chosen, lines, warnings);
        for (i, line) in lines.iter().enumerate() {
            if !chosen.iter().any(|c| c.lines.contains(&i)) {
                warnings.push(format!("not understood: \"{line}\""));
            }
        }
        affixes
    }
}

/// Stats some translation shows; a mod's other stats need no line of their own.
fn shown_stats(data: &Data) -> HashSet<u16> {
    let mut out = HashSet::new();
    for entry in &data.translations {
        for variant in &entry.variants {
            if variant.slots.is_empty() {
                out.extend(entry.stats.iter().copied());
            } else {
                out.extend(variant.slots.iter().map(|s| entry.stats[s.k as usize]));
            }
        }
    }
    out
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
/// mod with several shown stats (a hybrid) claims one line per stat. A line only partial readings
/// show is shared by exactly two hybrids whose ranges add up to its value. Fewest unexplained
/// lines wins, then fewest mods; ties go to the first found in line order.
fn assign(data: &Data, shown: &HashSet<u16>, cands: &[Vec<Cand>]) -> Vec<Choice> {
    struct Search<'a> {
        data: &'a Data,
        shown: &'a HashSet<u16>,
        cands: &'a [Vec<Cand>],
        used: Vec<bool>,
        /// Choices sharing each line (only for `only_partial` lines).
        claims: Vec<u8>,
        /// The line has candidates and all of them are partial.
        only_partial: Vec<bool>,
        groups: Vec<(Option<u16>, Option<Side>)>,
        cur: Vec<Choice>,
        skipped: usize,
        best: Option<(usize, usize, Vec<Choice>)>,
        steps: usize,
    }

    impl Search<'_> {
        /// Whether the choices sharing each summed line add up to its value; every summed line
        /// needs exactly two.
        fn sums_fit(&self) -> bool {
            (0..self.cands.len())
                .filter(|&l| self.claims[l] > 0)
                .all(|l| {
                    if self.claims[l] != 2 {
                        return false;
                    }
                    let value = self.cands[l][0].values[0];
                    let (mut lo, mut hi) = (0.0, 0.0);
                    for choice in self.cur.iter().filter(|c| c.lines.contains(&l)) {
                        let mine = self.cands[l].iter().filter(|x| choice.tiers.contains(&x.m));
                        lo += mine.clone().map(|x| x.lo).fold(f64::INFINITY, f64::min);
                        hi += mine.map(|x| x.hi).fold(f64::NEG_INFINITY, f64::max);
                    }
                    lo <= value && value <= hi
                })
        }

        /// The options for line `i`: (lines claimed, tiers), in first-seen order.
        fn options(&self, i: usize) -> Vec<(Vec<usize>, Vec<u32>)> {
            let mut opts: Vec<(Option<u16>, Vec<usize>, Vec<u32>)> = Vec::new();
            for cand in self.cands[i].iter().filter(|c| !c.partial) {
                let md = self.data.mod_at(cand.m);
                let group = md.groups.first().copied();
                if self.groups.contains(&(group, md.side)) {
                    continue;
                }
                // Each other shown stat of this tier needs a line of its own; try every choice.
                let needed: Vec<u16> = md
                    .stats
                    .iter()
                    .map(|s| s.stat)
                    .filter(|s| self.shown.contains(s) && !cand.stats.contains(s))
                    .collect();
                let mut sets: Vec<Vec<usize>> = vec![vec![i]];
                for stat in needed {
                    let mut next = Vec::new();
                    for set in &sets {
                        let covers = |l: usize| {
                            self.cands[l]
                                .iter()
                                .any(|x| x.m == cand.m && x.stats.contains(&stat))
                        };
                        if set.iter().any(|&l| covers(l)) {
                            next.push(set.clone());
                            continue;
                        }
                        for j in 0..self.cands.len() {
                            // A summed line takes a second claimer; any other line only one.
                            let shareable = self.only_partial[j] && self.claims[j] == 1;
                            if (self.used[j] && !shareable) || set.contains(&j) {
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
                    match opts.iter_mut().find(|(g, l, _)| *g == group && *l == lines) {
                        Some((_, _, tiers)) => {
                            if !tiers.contains(&cand.m) {
                                tiers.push(cand.m);
                            }
                        }
                        None => opts.push((group, lines, vec![cand.m])),
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
            let Some(i) = (0..self.cands.len()).find(|&i| !self.used[i] && !self.only_partial[i])
            else {
                if !self.sums_fit() {
                    return;
                }
                let skipped = self.skipped
                    + (0..self.cands.len())
                        .filter(|&l| self.only_partial[l] && self.claims[l] == 0)
                        .count();
                let better = match &self.best {
                    None => true,
                    Some((s, n, _)) => (skipped, self.cur.len()) < (*s, *n),
                };
                if better {
                    self.best = Some((skipped, self.cur.len(), self.cur.clone()));
                }
                return;
            };
            for (lines, tiers) in self.options(i) {
                let md = self.data.mod_at(tiers[0]);
                for &l in &lines {
                    if self.only_partial[l] {
                        self.claims[l] += 1;
                    }
                    self.used[l] = true;
                }
                self.groups.push((md.groups.first().copied(), md.side));
                self.cur.push(Choice { tiers, lines });
                self.run();
                let choice = self.cur.pop().expect("pushed above");
                self.groups.pop();
                for &l in &choice.lines {
                    if self.only_partial[l] {
                        self.claims[l] -= 1;
                        self.used[l] = self.claims[l] > 0;
                    } else {
                        self.used[l] = false;
                    }
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
        data,
        shown,
        cands,
        used: vec![false; cands.len()],
        claims: vec![0; cands.len()],
        only_partial: cands
            .iter()
            .map(|c| !c.is_empty() && c.iter().all(|x| x.partial))
            .collect(),
        groups: Vec::new(),
        cur: Vec::new(),
        skipped: 0,
        best: None,
        steps: 0,
    };
    search.run();
    let mut out = search.best.map(|b| b.2).unwrap_or_default();
    out.sort_by_key(|c| c.lines[0]);
    out
}

/// Turn the chosen mods into affixes, warning about summed lines and ambiguous tiers.
fn build_affixes(
    data: &Data,
    cands: &[Vec<Cand>],
    chosen: &[Choice],
    lines: &[String],
    warnings: &mut Vec<String>,
) -> Vec<AffixSpec> {
    let mut out = Vec::new();
    let mut summed: Vec<usize> = Vec::new();
    for choice in chosen {
        let m = choice.tiers[0];
        let md = data.mod_at(m);
        let mut values: Option<Vec<i32>> = Some(Vec::new());
        for stat in &md.stats {
            let found = choice
                .lines
                .iter()
                .filter_map(|&l| cands[l].iter().find(|x| x.m == m))
                .find_map(|cand| {
                    let index = cand.stats.iter().position(|t| *t == stat.stat)?;
                    cand.stored.get(index).copied().flatten()
                })
                .or((stat.min == stat.max).then_some(stat.min));
            match (found, &mut values) {
                (Some(value), Some(list)) => list.push(value),
                _ => values = None,
            }
        }
        if values.is_none() {
            for &l in &choice.lines {
                if cands[l].iter().all(|x| x.partial) {
                    summed.push(l);
                }
            }
        }
        if choice.tiers.len() > 1 {
            let ids: Vec<&str> = choice
                .tiers
                .iter()
                .map(|&t| data.mod_at(t).id.as_str())
                .collect();
            warnings.push(format!(
                "\"{}\" fits several tiers ({}); using {}",
                lines[choice.lines[0]],
                ids.join(", "),
                md.id
            ));
        }
        out.push(AffixSpec {
            id: md.id.clone(),
            values,
            fractured: false,
        });
    }
    summed.sort_unstable();
    summed.dedup();
    for l in summed {
        warnings.push(format!(
            "\"{}\" is the sum of two modifiers; their rolls are unknown",
            lines[l]
        ));
    }
    out
}

/// Which pool a line belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pool {
    Explicit,
    Crafted,
    Fractured,
    Implicit,
    Enchant,
}

fn split_tag(line: &str) -> (Option<Pool>, &str) {
    for (suffix, pool) in [
        (" (enchant)", Pool::Enchant),
        (" (implicit)", Pool::Implicit),
        (" (crafted)", Pool::Crafted),
        (" (fractured)", Pool::Fractured),
    ] {
        if let Some(rest) = line.strip_suffix(suffix) {
            return (Some(pool), rest);
        }
    }
    (None, line)
}

/// One line of a paste: a property of the item, or modifier text.
enum Line {
    ItemLevel(u8),
    Quality(Option<String>, u8),
    Defence(usize, i32),
    Sockets(String),
    Intangibility(u8),
    MemoryStrands(u8),
    Influence(Influence),
    Corrupted,
    Mirrored,
    Split,
    Synthesised,
    Unidentified,
    Ignored,
    Mod(String),
}

fn first_number(line: &str) -> Option<f64> {
    numbers_in(line).first().map(|n| n.value)
}

fn classify(line: &str) -> Line {
    if line == "Unidentified" {
        return Line::Unidentified;
    }
    if let Some(rest) = line.strip_prefix("Item Level:")
        && let Ok(level) = rest.trim().parse::<u8>()
    {
        return Line::ItemLevel(level);
    }
    if line.starts_with("Quality (")
        && let Some((label, quality)) = parse_quality(line)
    {
        return Line::Quality(Some(label), quality);
    }
    if line.starts_with("Quality:")
        && let Some(value) = first_number(line)
    {
        return Line::Quality(None, value as u8);
    }
    for (i, name) in ["Armour:", "Evasion Rating:", "Energy Shield:", "Ward:"]
        .iter()
        .enumerate()
    {
        if line.starts_with(name)
            && let Some(value) = first_number(&line[name.len()..])
        {
            return Line::Defence(i, value as i32);
        }
    }
    if let Some(rest) = line.strip_prefix("Sockets:") {
        return Line::Sockets(rest.trim().to_string());
    }
    if let Some(rest) = line.strip_prefix("Intangibility:")
        && let Some(value) = first_number(rest)
    {
        return Line::Intangibility(value as u8);
    }
    if let Some(rest) = line.strip_prefix("Memory Strands:")
        && let Some(value) = first_number(rest)
    {
        return Line::MemoryStrands(value as u8);
    }
    match line {
        "Corrupted" => return Line::Corrupted,
        "Mirrored" => return Line::Mirrored,
        "Split" => return Line::Split,
        "Synthesised Item" => return Line::Synthesised,
        "Fractured Item" | "Searing Exarch Item" | "Eater of Worlds Item" => {
            return Line::Ignored;
        }
        _ => {}
    }
    if let Some(name) = line.strip_suffix(" Item")
        && let Some(influence) = Influence::ALL.into_iter().find(|i| i.name() == name)
    {
        return Line::Influence(influence);
    }
    if line.starts_with("Requirements:")
        || line.starts_with("Level:")
        || line.starts_with("Note:")
        || line.starts_with("Item Class:")
        || line.starts_with("Rarity:")
        || PROPERTY_PREFIXES.iter().any(|p| line.starts_with(p))
    {
        return Line::Ignored;
    }
    Line::Mod(line.to_string())
}

/// `Quality (Attribute Modifiers): +20% (augmented)` -> (label, quality).
fn parse_quality(line: &str) -> Option<(String, u8)> {
    let rest = line.strip_prefix("Quality (")?;
    let close = rest.find(')')?;
    let label = rest[..close].strip_suffix(" Modifiers")?.trim();
    let tail = rest[close + 1..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let tail = tail.strip_prefix('+')?;
    let value = first_number(tail)?;
    Some((label.to_string(), value as u8))
}

fn parse_rarity(text: &str) -> Result<Rarity, ParseError> {
    match text {
        "Normal" => Ok(Rarity::Normal),
        "Magic" => Ok(Rarity::Magic),
        "Rare" => Ok(Rarity::Rare),
        "Unique" => Ok(Rarity::Unique),
        other => Err(ParseError::UnknownRarity(other.to_string())),
    }
}

/// The base a paste names: an exact base name in the header, else the longest base name inside
/// the magic item's name line.
fn find_base(data: &Data, header: &[String], warnings: &mut Vec<String>) -> Option<u16> {
    for line in header.iter().rev() {
        let ids = data.bases_named(line);
        if let Some(&base) = ids
            .iter()
            .find(|&&b| data.classes.get(data.base_at(b).class as usize).is_some())
        {
            if ids.len() > 1 {
                warnings.push(format!(
                    "base name \"{line}\" matches several bases; using {}",
                    data.base_at(base).id
                ));
            }
            return Some(base);
        }
    }
    let position = header.iter().position(|l| l.starts_with("Rarity:"))?;
    let line = header.get(position + 1)?;
    data.bases
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            data.classes.get(b.class as usize).is_some() && contains_words(line, &b.name)
        })
        .max_by_key(|(_, b)| b.name.len())
        .map(|(i, _)| i as u16)
}

/// Read a pasted item into an `ItemSpec`, warning about every line it could not explain.
pub fn parse(data: &Data, text: &str) -> Result<Parsed, ParseError> {
    let mut warnings = Vec::new();

    let mut sections: Vec<Vec<String>> = vec![Vec::new()];
    for raw in text.lines() {
        let line = strip_markup(raw.trim());
        if line.is_empty() {
            continue;
        }
        if line == "--------" {
            sections.push(Vec::new());
            continue;
        }
        sections.last_mut().expect("seeded above").push(line);
    }
    sections.retain(|s| !s.is_empty());
    if sections.iter().flatten().any(|l| l == "Unidentified") {
        return Err(ParseError::Unidentified);
    }
    let Some(header) = sections.first() else {
        return Err(ParseError::NoBase);
    };

    let mut rarity = Rarity::Rare;
    for line in header {
        if let Some(rest) = line.strip_prefix("Rarity:") {
            rarity = parse_rarity(rest.trim())?;
        }
    }
    let Some(base) = find_base(data, header, &mut warnings) else {
        return Err(ParseError::NoBase);
    };

    let mut item = Item {
        base,
        item_level: 1,
        rarity,
        influences: 0,
        quality: 0,
        catalyst: None,
        defences: None,
        implicits: Vec::new(),
        enchants: Vec::new(),
        prefixes: Vec::new(),
        suffixes: Vec::new(),
        sockets: Vec::new(),
        corrupted: false,
        mirrored: false,
        split: false,
        synthesised: false,
        foreseen: false,
        memory_strands: 0,
        intangibility: 0,
        imprint: None,
    };
    let mut sockets_text = String::new();

    let mut untagged_blocks: Vec<Vec<String>> = Vec::new();
    let mut crafted_lines: Vec<String> = Vec::new();
    let mut fractured_lines: Vec<String> = Vec::new();
    let mut implicit_lines: Vec<String> = Vec::new();
    let mut enchant_lines: Vec<String> = Vec::new();

    for (index, section) in sections.iter().enumerate() {
        let mut untagged: Vec<String> = Vec::new();
        for line in section {
            match classify(line) {
                Line::Mod(text) => {
                    if index == 0 {
                        // The header's item name and base name are not modifiers.
                        continue;
                    }
                    match split_tag(&text) {
                        (Some(Pool::Crafted), text) => crafted_lines.push(text.to_string()),
                        (Some(Pool::Fractured), text) => fractured_lines.push(text.to_string()),
                        (Some(Pool::Implicit), text) => implicit_lines.push(text.to_string()),
                        (Some(Pool::Enchant), text) => enchant_lines.push(text.to_string()),
                        (None, text) | (Some(Pool::Explicit), text) => {
                            untagged.push(text.to_string());
                        }
                    }
                }
                Line::Unidentified => return Err(ParseError::Unidentified),
                Line::ItemLevel(level) => item.item_level = level,
                Line::Quality(label, quality) => {
                    item.quality = quality;
                    if let Some(label) = label {
                        match data.catalysts.iter().position(|c| c.label == label) {
                            Some(c) => item.catalyst = Some(c as u8),
                            None => warnings.push(format!("unknown catalyst label \"{label}\"")),
                        }
                    }
                }
                Line::Defence(slot, value) => {
                    item.defences.get_or_insert([0; 4])[slot] = value;
                }
                Line::Sockets(text) => sockets_text = text,
                Line::Intangibility(value) => item.intangibility = value,
                Line::MemoryStrands(value) => item.memory_strands = value,
                Line::Influence(influence) => item.influences |= influence.bit(),
                Line::Corrupted => item.corrupted = true,
                Line::Mirrored => item.mirrored = true,
                Line::Split => item.split = true,
                Line::Synthesised => item.synthesised = true,
                Line::Ignored => {}
            }
        }
        if !untagged.is_empty() {
            untagged_blocks.push(untagged);
        }
    }

    // Untagged sections, latest first: explicits, then implicits, then enchantments.
    let block_count = untagged_blocks.len();
    let mut explicit_lines: Vec<String> = Vec::new();
    for (i, block) in untagged_blocks.into_iter().enumerate() {
        match block_count - i {
            1 => explicit_lines = block,
            2 => implicit_lines.extend(block),
            _ => enchant_lines.extend(block),
        }
    }

    let shown = shown_stats(data);
    let class_index = data.base_at(base).class;
    let tags = item.tags(data);
    let reach0 = |m: u32| {
        let md = data.mod_at(m);
        match md.kind {
            ModKind::Natural => spawn_weight(md, &tags) > 0,
            ModKind::Crafted => bench_has(data, class_index, m),
            _ => false,
        }
    };
    let reach1 = |m: u32| {
        let md = data.mod_at(m);
        reach0(m)
            || matches!(
                md.kind,
                ModKind::EssenceOnly | ModKind::Unveiled | ModKind::Delve | ModKind::Crafted
            )
            || (md.kind == ModKind::Natural && md.elevated)
    };
    let (implicit0, implicit1) = implicit_pool(data, &item);
    let explicit_effect = |m: u32| item.effect(data, &Roll { m, values: None }, false);
    let no_effect = |_: u32| 0i16;
    let enchant_reach = |m: u32| data.mod_at(m).kind.is_enchant();
    let crafted_reach = |m: u32| data.mod_at(m).kind == ModKind::Crafted;

    let implicit_pool_lines = Pools {
        data,
        effect: &no_effect,
        first: &implicit0,
        second: &implicit1,
        shown: &shown,
    };

    // An item with a single untagged block holds implicits when every line reads as one.
    if block_count == 1
        && explicit_lines
            .iter()
            .all(|line| !implicit_pool_lines.candidates(line).is_empty())
    {
        implicit_lines = std::mem::take(&mut explicit_lines);
    }

    let mut veiled: Vec<Side> = Vec::new();
    for list in [
        &mut explicit_lines,
        &mut crafted_lines,
        &mut fractured_lines,
        &mut implicit_lines,
    ] {
        list.retain(|line| match line.as_str() {
            "Veiled Prefix" => {
                veiled.push(Side::Prefix);
                false
            }
            "Veiled Suffix" => {
                veiled.push(Side::Suffix);
                false
            }
            _ => true,
        });
    }

    let mut prefixes: Vec<AffixSpec> = Vec::new();
    let mut suffixes: Vec<AffixSpec> = Vec::new();
    let mut implicits: Vec<ModSpec> = Vec::new();
    let mut enchants: Vec<ModSpec> = Vec::new();

    for (lines, pools, pool) in [
        (
            &explicit_lines,
            Pools {
                data,
                effect: &explicit_effect,
                first: &reach0,
                second: &reach1,
                shown: &shown,
            },
            Pool::Explicit,
        ),
        (
            &crafted_lines,
            Pools {
                data,
                effect: &explicit_effect,
                first: &crafted_reach,
                second: &crafted_reach,
                shown: &shown,
            },
            Pool::Crafted,
        ),
        (
            &fractured_lines,
            Pools {
                data,
                effect: &explicit_effect,
                first: &reach0,
                second: &reach1,
                shown: &shown,
            },
            Pool::Fractured,
        ),
        (&implicit_lines, implicit_pool_lines, Pool::Implicit),
        (
            &enchant_lines,
            Pools {
                data,
                effect: &no_effect,
                first: &enchant_reach,
                second: &enchant_reach,
                shown: &shown,
            },
            Pool::Enchant,
        ),
    ] {
        if lines.is_empty() {
            continue;
        }
        let affixes = pools.match_pool(lines, &mut warnings);
        match pool {
            Pool::Enchant => {
                enchants.extend(affixes.into_iter().map(|a| ModSpec {
                    id: a.id,
                    values: a.values,
                }));
            }
            Pool::Implicit => {
                implicits.extend(affixes.into_iter().map(|a| ModSpec {
                    id: a.id,
                    values: a.values,
                }));
            }
            Pool::Explicit | Pool::Crafted | Pool::Fractured => {
                for mut affix in affixes {
                    affix.fractured = pool == Pool::Fractured;
                    match data.mod_by_id(&affix.id).map(|m| data.mod_at(m).side) {
                        Some(Some(Side::Suffix)) => suffixes.push(affix),
                        _ => prefixes.push(affix),
                    }
                }
            }
        }
    }

    for side in veiled {
        let id = match side {
            Side::Prefix => "VeiledPrefix",
            Side::Suffix => "VeiledSuffix",
        };
        let affix = AffixSpec {
            id: id.to_string(),
            values: None,
            fractured: false,
        };
        match side {
            Side::Prefix => prefixes.push(affix),
            Side::Suffix => suffixes.push(affix),
        }
    }

    let spec = ItemSpec {
        base: data.base_at(base).id.clone(),
        item_level: item.item_level,
        rarity: item.rarity,
        influences: Influence::ALL
            .into_iter()
            .filter(|i| item.has_influence(*i))
            .collect(),
        quality: item.quality,
        catalyst: item
            .catalyst
            .map(|c| data.catalysts[c as usize].name.clone()),
        defences: item.defences,
        implicits,
        enchants,
        prefixes,
        suffixes,
        sockets: sockets_text,
        corrupted: item.corrupted,
        mirrored: item.mirrored,
        split: item.split,
        synthesised: item.synthesised,
        foreseen: item.foreseen,
        memory_strands: item.memory_strands,
        intangibility: item.intangibility,
        imprint: None,
        note: None,
    };
    Item::from_spec(data, &spec)?;
    Ok(Parsed { spec, warnings })
}

/// The first spawn weight whose tag the item has.
fn spawn_weight(md: &ModData, tags: &[u16]) -> u32 {
    for &(tag, weight) in &md.spawn {
        if tags.binary_search(&tag).is_ok() {
            return weight;
        }
    }
    0
}

/// Whether a bench option adds this mod to the class.
fn bench_has(data: &Data, class: u16, m: u32) -> bool {
    data.bench.iter().any(|option| {
        option.classes.contains(&class)
            && matches!(option.action, BenchAction::AddMod { m: added } if added == m)
    })
}

/// The two reachability passes of the implicit pool: the base's own implicits, then the corrupted,
/// eldritch and synthesised ones that can spawn on it.
fn implicit_pool(data: &Data, item: &Item) -> (impl Fn(u32) -> bool, impl Fn(u32) -> bool) {
    let own = data.base_at(item.base).implicits.clone();
    let tags = item.tags(data);
    let first_own = own.clone();
    let first = move |m: u32| first_own.contains(&m);
    let second = move |m: u32| {
        if own.contains(&m) {
            return true;
        }
        let md = data.mod_at(m);
        matches!(
            md.kind,
            ModKind::CorruptedImplicit | ModKind::Exarch | ModKind::Eater | ModKind::Synthesis
        ) && spawn_weight(md, &tags) > 0
    };
    (first, second)
}
