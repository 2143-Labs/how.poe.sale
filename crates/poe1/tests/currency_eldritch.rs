//! Eldritch embers and ichors: the code currency must agree with its op-declared twin, replace
//! the opposite kind's implicit, draw from the pool's own exact weights, and ignore item level.

use std::collections::BTreeMap;

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{Currency, Eldritch, ItemOutputSet};
use poe1::data::{Data, Eldritch as Kind, ModKind};
use poe1::item::{Item, ItemSpec};
use poe1::test_data;

fn item(data: &Data, json: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {json}"));
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{e}: {json}"))
}

/// The greater ember, resolved by the method whose single op is a tier-5 Exarch implicit.
fn greater_ember(data: &Data) -> Eldritch<'_> {
    Eldritch::new(data, Kind::Exarch, 5)
        .expect("the bundle declares currency/greater-eldritch-ember")
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

/// A pair of boots: the class the ember works on, uninfluenced, with changeable implicits.
const BOOTS: &str = r#"{
    "base": "Slink Boots",
    "itemLevel": 87,
    "rarity": "normal"
}"#;

#[test]
fn greater_ember_agrees_with_its_ops() {
    let data = test_data();
    let boots = item(data, BOOTS);
    let orb = greater_ember(data);
    assert_eq!(orb.id(), "currency/greater-eldritch-ember");

    let set = orb.apply(&boots, &None);
    assert!(!set.outcomes().is_empty(), "the tier has candidates");
    for outcome in set.outcomes() {
        assert_eq!(
            outcome.item.implicits.len(),
            1,
            "the draw replaces the item's implicits"
        );
        assert_eq!(
            data.mod_at(outcome.item.implicits[0].m).kind,
            ModKind::Exarch
        );
    }

    let old = op_distribution(data, "currency/greater-eldritch-ember", &boots);
    assert_eq!(old.len(), set.outcomes().len(), "same items as the ops");
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
    }
}

/// An ember replaces its own side's implicit and leaves the other side as it was (claim
/// `eldritch.ember-tier`, owner): an Eater implicit survives, and the Exarch implicit that was
/// there is gone. (The op's `retain` keeps exactly the opposite kind before pushing the draw; it
/// does not remove the opposite kind.)
#[test]
fn greater_ember_replaces_its_own_kind() {
    let data = test_data();
    let boots = item(
        data,
        r#"{
            "base": "Slink Boots",
            "itemLevel": 87,
            "rarity": "normal",
            "implicits": [
                {"id": "MovementVelocityEldritchImplicit2"},
                {"id": "LifeRegenerationRateEldritchImplicit1"}
            ]
        }"#,
    );
    let eater = data
        .mod_by_id("LifeRegenerationRateEldritchImplicit1")
        .expect("the bundle declares the Eater implicit");
    assert_eq!(data.mod_at(eater).kind, ModKind::Eater);
    let old_exarch = data
        .mod_by_id("MovementVelocityEldritchImplicit2")
        .expect("the bundle declares the Exarch implicit");
    assert_eq!(data.mod_at(old_exarch).kind, ModKind::Exarch);

    let set = greater_ember(data).apply(&boots, &None);
    assert!(!set.outcomes().is_empty());
    for outcome in set.outcomes() {
        assert_eq!(outcome.item.implicits.len(), 2, "one implicit per kind");
        let kept: Vec<ModKind> = outcome
            .item
            .implicits
            .iter()
            .map(|r| data.mod_at(r.m).kind)
            .collect();
        assert!(kept.contains(&ModKind::Eater), "the Eater implicit stays");
        let exarch = outcome
            .item
            .implicits
            .iter()
            .find(|r| data.mod_at(r.m).kind == ModKind::Exarch)
            .expect("the ember pushes an Exarch implicit");
        assert_ne!(exarch.m, old_exarch, "the old Exarch implicit is replaced");
        assert_eq!(data.mod_at(exarch.m).eldritch_tier, Some(5));
        assert_eq!(
            outcome
                .item
                .implicits
                .iter()
                .find(|r| data.mod_at(r.m).kind == ModKind::Eater)
                .map(|r| r.m),
            Some(eater),
            "the Eater implicit is untouched"
        );
    }

    let old = op_distribution(data, "currency/greater-eldritch-ember", &boots);
    assert_eq!(old.len(), set.outcomes().len(), "same items as the ops");
}

/// Embers and ichors have no item level requirement: a level-1 item draws the same tier-5 pool as
/// a level-87 one, and the op path does the same.
#[test]
fn greater_ember_ignores_item_level() {
    let data = test_data();
    let low = item(
        data,
        r#"{
            "base": "Slink Boots",
            "itemLevel": 1,
            "rarity": "normal"
        }"#,
    );
    let set = greater_ember(data).apply(&low, &None);
    assert!(!set.outcomes().is_empty());
    for outcome in set.outcomes() {
        let m = data.mod_at(outcome.item.implicits[0].m);
        assert!(
            m.level > low.item_level,
            "a level-{} modifier is offered on a level-{} item",
            m.level,
            low.item_level
        );
    }

    let old = op_distribution(data, "currency/greater-eldritch-ember", &low);
    assert_eq!(old.len(), set.outcomes().len(), "same items as the ops");
}

/// The declared requirements are the method's: an influenced shield (a class no ember works on,
/// and influenced besides) is refused.
#[test]
fn greater_ember_refuses_an_item_the_method_refuses() {
    let data = test_data();
    let shield = item(
        data,
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "rare",
            "influences": ["shaper"],
            "prefixes": [{"id": "IncreasedLife10"}],
            "suffixes": [{"id": "FireResist8"}]
        }"#,
    );
    let set = greater_ember(data).apply(&shield, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(_)),
        "the method refuses a shield: {set:?}"
    );
}

/// The integer weights really do add up to the total, which is the invariant every report reads.
#[test]
fn greater_ember_set_is_exact() {
    let data = test_data();
    let boots = item(data, BOOTS);
    let set = greater_ember(data).apply(&boots, &None);
    assert!(set.exact(), "the weights must sum to the total");
}
