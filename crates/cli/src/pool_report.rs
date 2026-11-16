//! `pool-report`: one item's whole modifier pool, as a document a person can read against the
//! published tables. The numbers are the export's exact integer weights — spawn weight times the
//! generation percentage — never floats; percentages are printed for reading only.
//!
//! This is the natural explicit pool the currency model draws from (`apply::draw_weights` with
//! `Pool::Natural`), so a family's weight here is the denominator a currency divides by. The Vaal
//! Orb's corrupted-implicit pool is listed too when the data has one for the base: it is a
//! different pool, but it is the other set of modifiers the item can end up with.

use std::fmt::Write as _;

use anyhow::Context;
use poe1::currency::Fraction;
use poe1::data::{Data, Influence, ModKind, Side};
use poe1::item::{Item, ItemSpec};
use poe1::method::{Pool, SideReq};

/// `--influence`: an influence name as the bundle spells it, in any case.
pub fn influence(name: &str) -> Option<Influence> {
    Influence::ALL
        .into_iter()
        .find(|i| i.name().eq_ignore_ascii_case(name))
}

/// The item a pool report describes: a normal item of this base, at this level, carrying this
/// influence and nothing else. Building it through `ItemSpec` keeps the report on the same path
/// as a fixture or a paste, so a base the model does not know is refused here too.
pub fn item(
    data: &Data,
    base: &str,
    item_level: u8,
    influence: Option<Influence>,
) -> anyhow::Result<Item> {
    let spec: ItemSpec = serde_json::from_value(serde_json::json!({
        "base": base,
        "itemLevel": item_level,
        "rarity": "normal",
        "influences": influence.into_iter().collect::<Vec<_>>(),
    }))
    .context("building the item spec")?;
    Item::from_spec(data, &spec).with_context(|| format!("no base {base:?}"))
}

/// The whole document.
pub fn render(data: &Data, item: &Item) -> String {
    let base = data.base_at(item.base);
    let weights = poe1::apply::draw_weights(data, item, &Pool::Natural, SideReq::Any, false, false);
    let side = |side: Side| -> Vec<(u32, u128)> {
        weights
            .iter()
            .copied()
            .filter(|&(m, w)| w > 0 && data.mod_at(m).side == Some(side))
            .collect()
    };
    let prefixes = side(Side::Prefix);
    let suffixes = side(Side::Suffix);
    let corrupted = corrupted(data, item);
    // The export stores generation percentages in hundredths, so every weight carries a common
    // factor of 100. Removing it gives the numbers the published tables use; when a pool has a
    // weight that does not divide by 100, keep every weight exact instead.
    let scale = scale(
        prefixes
            .iter()
            .chain(&suffixes)
            .chain(&corrupted)
            .map(|&(_, w)| w),
    );

    let mut out = String::new();
    writeln!(
        out,
        "# {} (item level {}, {}) — modifier pool\n",
        base.name,
        item.item_level,
        influence_text(item),
    )
    .unwrap();
    write!(out, "`{}`, {}.\n\n", base.id, data.class_of(item.base).name,).unwrap();
    writeln!(
        out,
        "Every number below comes from the pinned RePoE export (patch {}, commit `{}`). \
         A weight is the export's spawn weight times its generation percentage, kept as an exact \
         integer{}; a share is a ratio of those integers, printed to three decimals for reading \
         only. A modifier's chance is its weight over its side's total — the division the \
         currency code makes.\n",
        data.patch,
        data.repoe_commit,
        if scale == 100 {
            ", with the common factor of 100 removed so the numbers match the published tables"
        } else {
            ""
        },
    )
    .unwrap();

    side_report(&mut out, data, "Prefixes", &prefixes, scale);
    side_report(&mut out, data, "Suffixes", &suffixes, scale);
    if !corrupted.is_empty() {
        corrupted_report(&mut out, data, item, &corrupted, scale);
    }
    out
}

/// The Vaal Orb pool: the exported corrupted implicits this base's domain can roll at this level,
/// with the same exact weight the `VaalImplicit` op draws by.
fn corrupted(data: &Data, item: &Item) -> Vec<(u32, u128)> {
    let base = data.base_at(item.base);
    if base.implicits_locked {
        return Vec::new();
    }
    let tags = item.tags(data);
    data.mods_of_kind(ModKind::CorruptedImplicit)
        .iter()
        .filter_map(|&m| {
            let md = data.mod_at(m);
            (md.domain == base.domain.repoe()
                && md.level <= item.item_level
                && poe1::pool::spawn(md, &tags) > 0)
                .then(|| (m, poe1::pool::weight_exact(md, &tags)))
        })
        .filter(|&(_, w)| w > 0)
        .collect()
}

fn scale(weights: impl Iterator<Item = u128>) -> u128 {
    let mut seen = false;
    for weight in weights {
        seen = true;
        if weight % 100 != 0 {
            return 1;
        }
    }
    if seen { 100 } else { 1 }
}

fn influence_text(item: &Item) -> String {
    let names: Vec<&str> = Influence::ALL
        .into_iter()
        .filter(|i| item.has_influence(*i))
        .map(Influence::name)
        .collect();
    match names.as_slice() {
        [] => "uninfluenced".into(),
        [one] => format!("{one} influence"),
        many => format!("{} influence", many.join(" + ")),
    }
}

/// One side of the item: the family summary, then every tier in it.
fn side_report(out: &mut String, data: &Data, heading: &str, pool: &[(u32, u128)], scale: u128) {
    let total: u128 = pool.iter().map(|&(_, w)| w).sum();
    let families = families(data, pool);
    writeln!(
        out,
        "\n## {heading} — total weight {}\n",
        grouped(total / scale)
    )
    .unwrap();
    out.push_str("| family | tiers | weight | share |\n|:--|--:|--:|--:|\n");
    for family in &families {
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            family.name,
            family.tiers.len(),
            grouped(family.total / scale),
            share(family.total, total),
        )
        .unwrap();
    }
    writeln!(out, "\n### {heading} by tier\n").unwrap();
    out.push_str(
        "| tier | level | family | id | modifier | weight | share |\n\
         |--:|--:|:--|:--|:--|--:|--:|\n",
    );
    for family in &families {
        for (rank, &(m, weight)) in family.tiers.iter().enumerate() {
            let md = data.mod_at(m);
            writeln!(
                out,
                "| {} | {} | {} | `{}` | {} | {} | {} |",
                rank + 1,
                md.level,
                family.name,
                md.id,
                cell(&md.text),
                grouped(weight / scale),
                share(weight, total),
            )
            .unwrap();
        }
    }
}

/// The Vaal pool has no sides and no family ladder: its tiers are listed by family and level.
fn corrupted_report(out: &mut String, data: &Data, item: &Item, pool: &[(u32, u128)], scale: u128) {
    let total: u128 = pool.iter().map(|&(_, w)| w).sum();
    let mut rows: Vec<(u32, u128)> = pool.to_vec();
    rows.sort_by(|a, b| {
        data.mod_type(data.mod_at(a.0).mod_type)
            .cmp(data.mod_type(data.mod_at(b.0).mod_type))
            .then(data.mod_at(b.0).level.cmp(&data.mod_at(a.0).level))
            .then_with(|| data.mod_at(a.0).id.cmp(&data.mod_at(b.0).id))
    });
    writeln!(
        out,
        "\n## Corrupted implicits — total weight {}\n",
        grouped(total / scale)
    )
    .unwrap();
    writeln!(
        out,
        "The Vaal Orb pool: the exported corrupted implicits this base's domain can roll at item \
         level {}. They sit among the implicits and never fill a prefix or suffix slot.\n",
        item.item_level
    )
    .unwrap();
    out.push_str(
        "| level | family | id | modifier | weight | share |\n|--:|:--|:--|:--|--:|--:|\n",
    );
    for (m, weight) in rows {
        let md = data.mod_at(m);
        writeln!(
            out,
            "| {} | {} | `{}` | {} | {} | {} |",
            md.level,
            data.mod_type(md.mod_type),
            md.id,
            cell(&md.text),
            grouped(weight / scale),
            share(weight, total),
        )
        .unwrap();
    }
}

struct Family {
    name: String,
    total: u128,
    /// (mod index, weight), best tier first: the highest required level is tier 1.
    tiers: Vec<(u32, u128)>,
}

/// Families are the export's modifier `type` (`modTypes`), the grouping the game's own tables use.
fn families(data: &Data, pool: &[(u32, u128)]) -> Vec<Family> {
    let mut by_name: std::collections::BTreeMap<String, Family> = Default::default();
    for &(m, weight) in pool {
        let md = data.mod_at(m);
        let name = data.mod_type(md.mod_type).to_string();
        let family = by_name.entry(name.clone()).or_insert_with(|| Family {
            name,
            total: 0,
            tiers: Vec::new(),
        });
        family.total += weight;
        family.tiers.push((m, weight));
    }
    let mut out: Vec<Family> = by_name.into_values().collect();
    for family in &mut out {
        family.tiers.sort_by(|a, b| {
            let (a, b) = (data.mod_at(a.0), data.mod_at(b.0));
            b.level.cmp(&a.level).then_with(|| a.id.cmp(&b.id))
        });
    }
    out.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
    out
}

/// A weight with thousands separators, for reading. Integer arithmetic only: this is formatting,
/// not the number anyone divides by.
fn grouped(n: u128) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A share of `total`, exact until the three-decimal print.
fn share(weight: u128, total: u128) -> String {
    Fraction::new(weight, total)
        .map(|f| format!("{}%", f.percent_text(3)))
        .unwrap_or_else(|| "—".into())
}

/// A modifier's printed text as one table cell: the game's line breaks become `<br>`, and a pipe
/// that would split the cell is escaped.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', "<br>")
}
