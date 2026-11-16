//! The currencies written as code must agree with the same currencies declared as ops in
//! `rules/methods.json`. Two implementations of one rule are a bug waiting to happen, so every
//! currency that has both is compared over the same items until they are shown to be the same
//! distribution.
//!
//! The op path is `f64`; the code path is an exact integer weight. Agreement is therefore asserted
//! to within a tolerance that the op path's own arithmetic needs, and any item that appears on one
//! side but not the other fails outright — a shared number with a missing item is not agreement.

use std::collections::BTreeMap;

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{
    Alteration, Augmentation, Currency, Exalted, ItemOutputSet, Regal, Scouring, Transmutation,
};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::test_data;

fn shield(data: &Data, json: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {json}"));
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{e}"))
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

const NORMAL: &str =
    r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"normal","influences":["shaper"]}"#;
const MAGIC_ONE: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic","influences":["shaper"],
    "prefixes":[{"id":"IncreasedLife10"}]}"#;
const MAGIC_TWO: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic","influences":["shaper"],
    "prefixes":[{"id":"IncreasedLife10"}],"suffixes":[{"id":"FireResist8"}]}"#;
const RARE_TWO: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"rare","influences":["shaper"],
    "prefixes":[{"id":"IncreasedLife10"}],"suffixes":[{"id":"FireResist8"}]}"#;

#[test]
fn transmutation_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, NORMAL);
    agrees(
        "currency/transmutation",
        Transmutation::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn alteration_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, MAGIC_ONE);
    agrees(
        "currency/alteration",
        Alteration::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn augmentation_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, MAGIC_ONE);
    agrees(
        "currency/augmentation",
        Augmentation::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn regal_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, MAGIC_TWO);
    agrees(
        "currency/regal",
        Regal::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn exalted_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, RARE_TWO);
    agrees(
        "currency/exalted",
        Exalted::new(data).apply(&item, &None),
        &item,
    );
}

#[test]
fn scouring_agrees_with_its_ops() {
    let data = test_data();
    let item = shield(data, MAGIC_TWO);
    agrees(
        "currency/scouring",
        Scouring::new(data).apply(&item, &None),
        &item,
    );
}
