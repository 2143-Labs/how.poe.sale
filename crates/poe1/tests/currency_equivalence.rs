//! The currencies written as code must agree with the same currencies declared as ops in
//! `rules/methods.json`. Two implementations of one rule are a bug waiting to happen, so every
//! currency that has both is compared over the same items until they are shown to be the same
//! distribution.
//!
//! The op path is `f64`; the code path is an exact integer weight. Agreement is therefore asserted
//! to within a tolerance that the op path's own arithmetic needs, and any item that appears on one
//! side but not the other fails outright — a shared number with a missing item is not agreement.
//!
//! # What is compared
//!
//! The items are every paste under `fixtures/text` that parses (a paste that does not parse is
//! skipped), plus four edge items the shields below never exercise: a magic item carrying display
//! figures and foresight, a magic item with a fractured modifier, a magic item holding a crafted
//! metamod, and a normal jewel. The currencies are every one this crate implements: transmutation,
//! alteration, augmentation, regal, exalted, scouring, annulment, and one representative essence
//! (`Deafening Essence of Hatred`). Each pair is one case, and each case asserts, in order and
//! reporting only the first failure:
//!
//! 1. the code refuses exactly when the op path refuses;
//! 2. the code set is exact (its weights sum to its total);
//! 3. the two item sets are identical — no item on one side only;
//! 4. every shared item's chance agrees within `1e-12`;
//! 5. every code outcome passes [`Item::validate`].
//!
//! Then, for every first-step case that produced outcomes, the first 25 outcomes in the set's own
//! (sorted) order are fed through every currency again, under the same five assertions. The second
//! step is what reaches states the first step cannot reach in one draw — in particular the
//! cross-draw normalisation on jewels.
//!
//! # Refusals and `Pending`
//!
//! An op path that returns `Err` from [`apply`] is a refusal, and the code must refuse exactly
//! then. A [`Leaf::Pending`] is **not** a refusal: the step ran and the leaf records its certain
//! result. It is compared as `pending.item` at the leaf's own chance, which is how the essence is
//! checked (its generated method always stops at the random fill, so counting `Pending` as a
//! refusal would make every essence case a mismatch by construction rather than a finding).

use std::collections::BTreeMap;
use std::path::PathBuf;

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{
    Alteration, Annulment, Augmentation, Currency, Essence, Exalted, ItemOutputSet, Regal,
    Scouring, Transmutation,
};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::test_data;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[track_caller]
fn spec_item(data: &Data, json: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {json}"));
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{e}: {json}"))
}

/// A pasted item, or `None` when the paste does not parse (fixtures are prose, and a paste the
/// reader cannot explain is not this test's subject).
fn paste_item(data: &Data, text: &str) -> Option<Item> {
    let parsed = poe1::text::parse(data, text).ok()?;
    Item::from_spec(data, &parsed.spec).ok()
}

/// A short, readable item, for failure messages.
fn describe(data: &Data, item: &Item) -> String {
    let affixes: Vec<&str> = item
        .explicits()
        .map(|a| data.mod_at(a.roll.m).id.as_str())
        .collect();
    format!(
        "{} {:?} (ilvl {}, affixes [{}])",
        data.base_at(item.base).name,
        item.rarity,
        item.item_level,
        affixes.join(", ")
    )
}

/// The whole distribution the op-declared method produces, as items with their chances.
fn op_distribution(data: &Data, id: &str, item: &Item) -> BTreeMap<Item, f64> {
    let method = data.method(id).unwrap_or_else(|| panic!("{id} missing"));
    let outcome = apply(data, method, std::slice::from_ref(item), &Params::default())
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    let mut out = BTreeMap::new();
    for (p, leaf) in outcome.leaves {
        if let Leaf::Item(i) = leaf {
            *out.entry(i).or_insert(0.0) += p;
        }
    }
    out
}

/// The same distribution from the currency written as code.
fn code_distribution(set: &ItemOutputSet) -> BTreeMap<Item, f64> {
    assert!(set.exact(), "the code path must sum to its total");
    let total = set.total() as f64;
    set.outcomes()
        .iter()
        .map(|o| (o.item.clone(), o.weight as f64 / total))
        .collect()
}

#[track_caller]
fn agrees(id: &str, set: ItemOutputSet, item: &Item) {
    let data = test_data();
    let old = op_distribution(data, id, item);
    let new = code_distribution(&set);
    let old_total: f64 = old.values().sum();
    assert!(
        (old_total - 1.0).abs() < 1e-9,
        "{id}: the ops sum to {old_total}"
    );
    for (it, p) in &new {
        let q = old
            .get(it)
            .unwrap_or_else(|| panic!("{id}: the code produces an item the ops do not: {it:?}"));
        assert!(
            (p - q).abs() < 1e-12,
            "{id}: one item's chance is {p} as code and {q} as ops"
        );
    }
    assert_eq!(
        new.len(),
        old.len(),
        "{id}: {} items as code, {} as ops",
        new.len(),
        old.len()
    );
}

/// The four shields the per-currency checks pin by name. They are one table now rather than four
/// free-standing constants, so the honest input for each currency is named where it is used.
#[derive(Clone, Copy)]
enum Shield {
    Normal,
    MagicOne,
    MagicTwo,
    RareTwo,
}

fn shield(data: &Data, which: Shield) -> Item {
    let json = match which {
        Shield::Normal => {
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"normal","influences":["shaper"]}"#
        }
        Shield::MagicOne => {
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic","influences":["shaper"],
                "prefixes":[{"id":"IncreasedLife10"}]}"#
        }
        Shield::MagicTwo => {
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic","influences":["shaper"],
                "prefixes":[{"id":"IncreasedLife10"}],"suffixes":[{"id":"FireResist8"}]}"#
        }
        Shield::RareTwo => {
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"rare","influences":["shaper"],
                "prefixes":[{"id":"IncreasedLife10"}],"suffixes":[{"id":"FireResist8"}]}"#
        }
    };
    spec_item(data, json)
}

/// Every item the table walks: the pastes that parse, then the four edge items.
fn items(data: &Data) -> Vec<(String, Item)> {
    let mut out = Vec::new();
    let dir = root().join("fixtures/text");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "txt").then_some(path)
        })
        .collect();
    paths.sort();
    for path in paths {
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if let Some(item) = paste_item(data, &text) {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            out.push((format!("text/{stem}"), item));
        }
    }
    let edges = [
        (
            "edge/display-state",
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
                "defences":[1,2,3,4],"foreseen":true}"#,
        ),
        (
            "edge/fractured-prefix",
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
                "prefixes":[{"id":"IncreasedLife10","fractured":true}]}"#,
        ),
        (
            "edge/metamod",
            r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
                "suffixes":[{"id":"StrMasterItemGenerationCannotChangePrefixes"}]}"#,
        ),
        (
            "edge/normal-jewel",
            r#"{"base":"Viridian Jewel","itemLevel":83,"rarity":"normal"}"#,
        ),
    ];
    for (name, json) in edges {
        out.push((name.to_string(), spec_item(data, json)));
    }
    out
}

/// Every currency this crate implements. `fracture` and `eldritch` have no implementation in
/// `crates/poe1/src/currency/`, so they have no case here.
fn currencies<'d>(data: &'d Data) -> Vec<Box<dyn Currency + 'd>> {
    vec![
        Box::new(Transmutation::new(data)),
        Box::new(Alteration::new(data)),
        Box::new(Augmentation::new(data)),
        Box::new(Regal::new(data)),
        Box::new(Exalted::new(data)),
        Box::new(Scouring::new(data)),
        Box::new(Annulment::new(data)),
        Box::new(
            Essence::named(data, "Deafening Essence of Hatred")
                .expect("the bundle declares this essence"),
        ),
    ]
}

/// What the op path says about one step.
enum OpView {
    Refused(String),
    /// Item → chance. A `Pending` leaf contributes its certain item at the leaf's chance.
    Outcomes(BTreeMap<Item, f64>),
    /// A leaf shape this comparison does not understand; reported as a case failure rather than
    /// aborting the walk.
    Unsupported(String),
}

fn op_view(data: &Data, id: &str, item: &Item) -> OpView {
    let Some(method) = data.method(id) else {
        return OpView::Unsupported(format!("no method named {id}"));
    };
    let outcome = match apply(data, method, std::slice::from_ref(item), &Params::default()) {
        Ok(outcome) => outcome,
        Err(e) => return OpView::Refused(e.to_string()),
    };
    let mut map = BTreeMap::new();
    for (p, leaf) in outcome.leaves {
        match leaf {
            Leaf::Item(i) => *map.entry(i).or_insert(0.0) += p,
            Leaf::Pending(pending) => *map.entry(pending.item).or_insert(0.0) += p,
            Leaf::Items(items) => {
                for i in items {
                    *map.entry(i).or_insert(0.0) += p;
                }
            }
            other => {
                return OpView::Unsupported(format!("a {} leaf", leaf_name(&other)));
            }
        }
    }
    OpView::Outcomes(map)
}

fn leaf_name(leaf: &Leaf) -> &'static str {
    match leaf {
        Leaf::Item(_) => "Item",
        Leaf::Items(_) => "Items",
        Leaf::Destroyed => "Destroyed",
        Leaf::Choice(_) => "Choice",
        Leaf::Pending(_) => "Pending",
    }
}

/// The five assertions, in order, returning the first failure.
fn compare(data: &Data, currency: &str, item: &Item, set: &ItemOutputSet) -> Result<(), String> {
    let op = op_view(data, currency, item);
    let code_refused = matches!(set, ItemOutputSet::Refused(_));

    // 1. The code refuses exactly when the op path refuses.
    match &op {
        OpView::Unsupported(why) => return Err(format!("the op path returned {why}")),
        OpView::Refused(why) => {
            if !code_refused {
                return Err(format!(
                    "the ops refuse ({why}) but the code produces {} outcomes",
                    set.outcomes().len()
                ));
            }
            return Ok(());
        }
        OpView::Outcomes(map) => {
            if code_refused {
                let why = match set {
                    ItemOutputSet::Refused(why) => *why,
                    _ => unreachable!(),
                };
                return Err(format!(
                    "the code refuses ({why}) but the ops produce {} items",
                    map.len()
                ));
            }
        }
    }
    let OpView::Outcomes(op) = &op else {
        unreachable!("handled above");
    };

    // 2. The code set is exact.
    if !set.exact() {
        let sum: u128 = set.outcomes().iter().map(|o| o.weight).sum();
        return Err(format!(
            "the code set is not exact: {} outcomes sum to {sum} over a total of {}",
            set.outcomes().len(),
            set.total()
        ));
    }

    // 3. The item sets are identical.
    let total = set.total() as f64;
    let code: Vec<(&Item, f64)> = set
        .outcomes()
        .iter()
        .map(|o| (&o.item, o.weight as f64 / total))
        .collect();
    let shared = code.iter().filter(|(it, _)| op.contains_key(*it)).count();
    if shared != code.len() || code.len() != op.len() {
        let only_code = code.len() - shared;
        let only_op = op.len() - shared;
        let example = code
            .iter()
            .find(|(it, _)| !op.contains_key(*it))
            .map(|(it, _)| describe(data, it));
        return Err(format!(
            "item sets differ by {} items ({only_code} only as code, {only_op} only as ops){}",
            only_code + only_op,
            example
                .map(|e| format!("; first code-only: {e}"))
                .unwrap_or_default()
        ));
    }

    // 4. Every chance matches within 1e-12.
    let mut worst = (0.0f64, None);
    let mut worst_relative = (0.0f64, None);
    for (it, p) in &code {
        let q = op[*it];
        let delta = (p - q).abs();
        if delta > worst.0 {
            worst = (delta, Some(*it));
        }
        if q > 0.0 && delta / q > worst_relative.0 {
            worst_relative = (delta / q, Some(*it));
        }
    }
    if worst.0 >= 1e-12 {
        let it = worst.1.expect("a nonzero error has an item");
        let p = code
            .iter()
            .find_map(|(i, p)| (*i == it).then_some(*p))
            .expect("present");
        let relative = worst_relative
            .1
            .map(|rel| {
                format!(
                    "; the worst relative error is {:.2}% on {}",
                    worst_relative.0 * 100.0,
                    describe(data, rel)
                )
            })
            .unwrap_or_default();
        return Err(format!(
            "the worst chance error is {:.3e} on {} (code {p:.17}, ops {:.17}){relative}",
            worst.0,
            describe(data, it),
            op[it]
        ));
    }

    // 5. Every outcome is a legal item.
    for outcome in set.outcomes() {
        if let Err(e) = outcome.item.validate(data) {
            return Err(format!(
                "an outcome does not validate ({e}): {}",
                describe(data, &outcome.item)
            ));
        }
    }

    Ok(())
}

/// Feeds up to 25 of `set`'s outcomes through every currency, under the same five assertions.
// One call site and one test: the parameters are the case's identity, and a struct around them would
// only move the same list somewhere else.
#[allow(clippy::too_many_arguments)]
fn second_step(
    data: &Data,
    label: &str,
    set: &ItemOutputSet,
    currencies: &[Box<dyn Currency + '_>],
    failures: &mut Vec<String>,
    total: &mut usize,
    passed: &mut usize,
    stats: &mut BTreeMap<String, (usize, usize)>,
) {
    let ItemOutputSet::Outcomes { outcomes, .. } = set else {
        return;
    };
    for (i, outcome) in outcomes.iter().take(25).enumerate() {
        for currency in currencies {
            *total += 1;
            let case = format!(
                "{label} -> outcome #{i} [{}] x {}",
                describe(data, &outcome.item),
                currency.id()
            );
            let next = currency.apply(&outcome.item, &None);
            let tally = stats.entry(currency.id().to_string()).or_default();
            tally.0 += 1;
            match compare(data, currency.id(), &outcome.item, &next) {
                Ok(()) => {
                    *passed += 1;
                    tally.1 += 1;
                }
                Err(e) => failures.push(format!("{case}: {e}")),
            }
        }
    }
}

// The shields, one test per currency, as they have always been.

#[test]
fn transmutation_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, Shield::Normal);
    agrees(
        "currency/transmutation",
        Transmutation::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn alteration_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, Shield::MagicOne);
    agrees(
        "currency/alteration",
        Alteration::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn augmentation_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, Shield::MagicOne);
    agrees(
        "currency/augmentation",
        Augmentation::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn regal_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, Shield::MagicTwo);
    agrees(
        "currency/regal",
        Regal::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn exalted_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, Shield::RareTwo);
    agrees(
        "currency/exalted",
        Exalted::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn scouring_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, Shield::MagicTwo);
    agrees(
        "currency/scouring",
        Scouring::new(data).apply(&item, &None),
        &item,
    );
}

/// The table: every item against every currency, then up to 25 outcomes of each first step
/// against every currency again.
#[test]
fn every_currency_agrees_with_its_ops_over_every_item() {
    let data = test_data();
    let currencies = currencies(data);
    let items = items(data);
    let mut failures = Vec::new();
    let mut total = 0usize;
    let mut passed = 0usize;
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for (label, item) in &items {
        for currency in &currencies {
            total += 1;
            let case = format!("{label} [{}] x {}", describe(data, item), currency.id());
            let set = currency.apply(item, &None);
            let tally = stats.entry(currency.id().to_string()).or_default();
            tally.0 += 1;
            match compare(data, currency.id(), item, &set) {
                Ok(()) => {
                    passed += 1;
                    tally.1 += 1;
                }
                Err(e) => failures.push(format!("{case}: {e}")),
            }
            second_step(
                data,
                &case,
                &set,
                &currencies,
                &mut failures,
                &mut total,
                &mut passed,
                &mut stats,
            );
        }
    }

    let mut summary = String::new();
    for (id, (n, ok)) in &stats {
        summary.push_str(&format!("  {id}: {ok}/{n} cases pass\n"));
    }
    if !failures.is_empty() {
        panic!(
            "{} of {total} currency-equivalence cases fail ({} pass)\n{}\n{}",
            failures.len(),
            passed,
            summary,
            failures.join("\n")
        );
    }
    println!("all {total} currency-equivalence cases pass\n{summary}");
}
