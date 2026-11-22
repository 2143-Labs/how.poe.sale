//! Orb of Annulment: the code currency must agree with its op-declared twin, and it must only ever
//! take a modifier a removal may take — a fractured one never goes.

use std::collections::BTreeMap;

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{Annulment, Currency, ItemOutputSet};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::test_data;

fn item(data: &Data, json: &str) -> Item {
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

/// Two prefixes and two suffixes: every modifier is removable, so the annulment has four equally
/// likely results.
const RARE_TWO_TWO: &str = r#"{
    "base": "Supreme Spiked Shield",
    "itemLevel": 87,
    "rarity": "rare",
    "influences": ["shaper"],
    "prefixes": [
        {"id": "IncreasedLife10"},
        {"id": "IncreasedMana12"}
    ],
    "suffixes": [
        {"id": "FireResist8"},
        {"id": "ColdResist8"}
    ]
}"#;

#[test]
fn annulment_agrees_with_its_ops() {
    let data = test_data();
    let shield = item(data, RARE_TWO_TWO);
    let set = Annulment::new(data).apply(&shield, &None);

    assert_eq!(
        set.outcomes().len(),
        4,
        "one outcome per removable modifier"
    );
    assert_eq!(set.total(), 4, "each draw has weight one");
    for outcome in set.outcomes() {
        assert_eq!(outcome.weight, 1, "a uniform draw");
    }

    let old = op_distribution(data, "currency/annulment", &shield);
    assert_eq!(old.len(), 4, "the ops produce four items");
    let old_total: f64 = old.values().sum();
    assert!((old_total - 1.0).abs() < 1e-9, "the ops sum to {old_total}");
    let new = code_distribution(&set);
    for (it, p) in &new {
        let q = old
            .get(it)
            .unwrap_or_else(|| panic!("the code produces an item the ops do not: {it:?}"));
        assert!(
            (p - q).abs() < 1e-12,
            "one item's chance is {p} as code and {q} as ops"
        );
        assert!((q - 0.25).abs() < 1e-12, "four draws, one quarter each");
    }
    assert_eq!(new.len(), old.len(), "same number of items");
}

/// A fractured modifier cannot be removed, so it stays in every outcome.
#[test]
fn annulment_never_removes_a_fractured_modifier() {
    let data = test_data();
    let shield = item(
        data,
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "rare",
            "influences": ["shaper"],
            "prefixes": [
                {"id": "IncreasedLife10", "fractured": true},
                {"id": "IncreasedMana12"}
            ],
            "suffixes": [{"id": "FireResist8"}]
        }"#,
    );
    let set = Annulment::new(data).apply(&shield, &None);
    assert_eq!(set.total(), 2, "only the plain prefix and the suffix go");
    for outcome in set.outcomes() {
        assert_eq!(outcome.weight, 1);
        assert!(
            outcome.item.prefixes.iter().any(|a| a.fractured),
            "the fractured prefix is still there: {:?}",
            outcome.item.prefixes
        );
    }
}

/// With every modifier fractured there is nothing to remove, which is a refusal rather than an
/// empty distribution.
#[test]
fn annulment_refuses_an_item_with_nothing_removable() {
    let data = test_data();
    let shield = item(
        data,
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "rare",
            "influences": ["shaper"],
            "prefixes": [{"id": "IncreasedLife10", "fractured": true}],
            "suffixes": [{"id": "FireResist8", "fractured": true}]
        }"#,
    );
    let set = Annulment::new(data).apply(&shield, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(_)),
        "nothing can be removed, so the step is refused: {set:?}"
    );
}

/// The integer weights really do add up to the total, which is the invariant every report reads.
#[test]
fn annulment_set_is_exact() {
    let data = test_data();
    let shield = item(data, RARE_TWO_TWO);
    let set = Annulment::new(data).apply(&shield, &None);
    assert!(set.exact(), "the weights must sum to the total");
}
