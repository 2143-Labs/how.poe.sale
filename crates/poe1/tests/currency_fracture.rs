//! Beastcraft fracture: the code currency must agree with its op-declared twin, it must choose
//! among exactly the modifiers the op can fracture, and it must refuse where the method refuses.

use std::collections::BTreeMap;

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{Currency, Fracture, ItemOutputSet};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::test_data;

fn item(data: &Data, json: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {json}"));
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{e}: {json}"))
}

fn fracture(data: &Data) -> Fracture<'_> {
    Fracture::new(data).expect("the bundle declares beast/fracture-talisman")
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

/// Two prefixes and two suffixes: every modifier can be fractured, so the beast fractures one of
/// four, equally likely. The base is a Talisman because `beast/fracture-talisman` requires one
/// (`baseTag`), and is uninfluenced because the method's `noInfluence` refuses an influenced item.
const TALISMAN_TWO_TWO: &str = r#"{
    "base": "Chieftain Talisman",
    "itemLevel": 87,
    "rarity": "rare",
    "prefixes": [
        {"id": "IncreasedMana8"},
        {"id": "AddedPhysicalDamage8"}
    ],
    "suffixes": [
        {"id": "Strength8"},
        {"id": "Intelligence6"}
    ]
}"#;

#[test]
fn fracture_agrees_with_its_ops() {
    let data = test_data();
    let talisman = item(data, TALISMAN_TWO_TWO);
    let set = fracture(data).apply(&talisman, &None);

    assert_eq!(set.outcomes().len(), 4, "one outcome per fracturable affix");
    assert_eq!(set.total(), 4, "each choice has weight one");
    for outcome in set.outcomes() {
        assert_eq!(outcome.weight, 1, "a uniform draw");
        assert_eq!(
            outcome.item.explicits().filter(|a| a.fractured).count(),
            1,
            "exactly one modifier becomes fractured"
        );
    }

    let old = op_distribution(data, "beast/fracture-talisman", &talisman);
    assert_eq!(old.len(), 4, "the ops produce four items");
    let old_total: f64 = old.values().sum();
    assert!((old_total - 1.0).abs() < 1e-9, "the ops sum to {old_total}");
    let new_total = set.total() as f64;
    for outcome in set.outcomes() {
        let p = outcome.weight as f64 / new_total;
        let q = old.get(&outcome.item).unwrap_or_else(|| {
            panic!(
                "the code produces an item the ops do not: {:?}",
                outcome.item
            )
        });
        assert!(
            (p - q).abs() < 1e-12,
            "one item's chance is {p} as code and {q} as ops"
        );
        assert!((q - 0.25).abs() < 1e-12, "four choices, one quarter each");
    }
}

/// A veiled modifier is not one the beast may fracture: it is excluded from the choice, and every
/// outcome still holds it unrevealed and unfractured.
#[test]
fn fracture_does_not_offer_a_veiled_modifier() {
    let data = test_data();
    let talisman = item(
        data,
        r#"{
            "base": "Chieftain Talisman",
            "itemLevel": 87,
            "rarity": "rare",
            "prefixes": [
                {"id": "IncreasedMana8"},
                {"id": "AddedPhysicalDamage8"}
            ],
            "suffixes": [
                {"id": "Strength8"},
                {"id": "Intelligence6"},
                {"id": "VeiledSuffix"}
            ]
        }"#,
    );
    let set = fracture(data).apply(&talisman, &None);

    assert_eq!(
        set.total(),
        4,
        "only the four revealed modifiers can be fractured"
    );
    for outcome in set.outcomes() {
        let veiled = outcome
            .item
            .suffixes
            .iter()
            .find(|a| data.mod_at(a.roll.m).id == "VeiledSuffix")
            .expect("the veiled modifier stays on the item");
        assert!(!veiled.fractured, "a veiled modifier cannot be fractured");
        let fractured: Vec<&str> = outcome
            .item
            .explicits()
            .filter(|a| a.fractured)
            .map(|a| data.mod_at(a.roll.m).id.as_str())
            .collect();
        assert_eq!(fractured.len(), 1, "one modifier becomes fractured");
        assert_ne!(fractured[0], "VeiledSuffix");
    }

    let old = op_distribution(data, "beast/fracture-talisman", &talisman);
    assert_eq!(old.len(), set.outcomes().len(), "same items as the ops");
}

/// With every modifier already fractured the method's `noFractured` refuses, and there is nothing
/// left for the op either: both refuse rather than returning an empty distribution.
#[test]
fn fracture_refuses_when_every_modifier_is_fractured() {
    let data = test_data();
    let talisman = item(
        data,
        r#"{
            "base": "Chieftain Talisman",
            "itemLevel": 87,
            "rarity": "rare",
            "prefixes": [
                {"id": "IncreasedMana8", "fractured": true},
                {"id": "AddedPhysicalDamage8", "fractured": true}
            ],
            "suffixes": [
                {"id": "Strength8", "fractured": true},
                {"id": "Intelligence6", "fractured": true}
            ]
        }"#,
    );
    let set = fracture(data).apply(&talisman, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(_)),
        "nothing can be fractured, so the step is refused: {set:?}"
    );
    assert!(
        apply(
            data,
            data.method("beast/fracture-talisman").expect("declared"),
            std::slice::from_ref(&talisman),
            &Params::default()
        )
        .is_err(),
        "the op path refuses the same item"
    );
}

/// The declared requirements are the method's, not this file's: an influenced item this beastcraft
/// cannot touch is refused even though it has modifiers that could be fractured. (This is why the
/// equivalence cases use a Talisman: a Shaper shield is refused by `noInfluence`.)
#[test]
fn fracture_refuses_an_item_the_method_refuses() {
    let data = test_data();
    let shield = item(
        data,
        r#"{
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
        }"#,
    );
    let set = fracture(data).apply(&shield, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(_)),
        "the method refuses an influenced item: {set:?}"
    );
}

/// The integer weights really do add up to the total, which is the invariant every report reads.
#[test]
fn fracture_set_is_exact() {
    let data = test_data();
    let talisman = item(data, TALISMAN_TWO_TWO);
    assert!(fracture(data).apply(&talisman, &None).exact());
}
