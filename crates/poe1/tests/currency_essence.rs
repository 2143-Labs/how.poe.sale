//! An Essence is one deterministic step: the certain part of its reforge. These tests pin it to the
//! op-declared twin — the generated `essence/<name>` method — item for item and mass for mass, and
//! pin the refusals that method declares.

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{Currency, Essence, ItemOutputSet, Outcome};
use poe1::data::{Data, Rarity};
use poe1::item::{Item, ItemSpec};

fn item(data: &Data, json: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {json}"));
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{e}"))
}

/// The owner's shield: a Shaper Supreme Spiked Shield, item level 87.
const NORMAL: &str =
    r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"normal","influences":["shaper"]}"#;
const MAGIC: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic","influences":["shaper"],
    "prefixes":[{"id":"IncreasedLife10"}]}"#;
const RARE: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"rare","influences":["shaper"],
    "prefixes":[{"id":"IncreasedLife10"}],"suffixes":[{"id":"FireResist8"}]}"#;

/// The one item the op-declared method certainly produces: the reforge stops at its fill, which
/// adds to the item `Pending::item` and changes nothing already on it (`certain`).
fn op_certain(data: &Data, id: &str, item: &Item) -> Item {
    let method = data.method(id).unwrap_or_else(|| panic!("{id} missing"));
    let outcome = apply(data, method, std::slice::from_ref(item), &Params::default())
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    assert_eq!(
        outcome.leaves.len(),
        1,
        "{id}: the certain part is one result"
    );
    match &outcome.leaves[0] {
        (p, Leaf::Pending(pending)) => {
            assert_eq!(*p, 1.0, "{id}: the certain part is the whole mass");
            assert!(
                pending.certain,
                "{id}: the model stopped before the fill, so the item is the result plus additions"
            );
            pending.item.clone()
        }
        other => panic!("{id}: expected the reforge to stop at its fill, got {other:?}"),
    }
}

/// The set's one outcome, asserting the set is whole.
#[track_caller]
fn one(set: &ItemOutputSet) -> &Outcome {
    assert!(set.exact(), "the weights must sum to the total");
    let outcomes = set.outcomes();
    assert_eq!(outcomes.len(), 1, "an essence is deterministic");
    &outcomes[0]
}

/// The essence's declared modifier for `item`'s class, as the bundle holds it.
fn declared_mod(data: &Data, name: &str, item: &Item) -> u32 {
    let index = data
        .essence_by_name(name)
        .unwrap_or_else(|| panic!("{name} missing")) as usize;
    let class = data.base_at(item.base).class;
    data.essences[index]
        .mods
        .iter()
        .find(|(c, _)| *c == class)
        .map(|&(_, m)| m)
        .unwrap_or_else(|| panic!("{name} has no modifier for class {class}"))
}

// (a) The currency and the op-declared method must be the same step. The op path stops at the
// fill, so what it produces with certainty is the item the currency returns — and the whole mass,
// because the currency's one outcome is deterministic.
#[test]
fn an_essence_agrees_with_the_certain_part_of_its_ops() {
    let data = poe1::test_data();
    let name = "Deafening Essence of Hatred";
    let essence = Essence::named(data, name).expect("the bundle declares this essence");
    assert_eq!(
        essence.id(),
        "essence/deafening-essence-of-hatred",
        "the id is the one the generator gives the slugged name"
    );
    let shield = item(data, NORMAL);
    let set = essence.apply(&shield, &None);
    let out = one(&set);
    assert_eq!(out.weight, 1, "one outcome at the whole chance");
    assert_eq!(
        out.item,
        op_certain(data, essence.id(), &shield),
        "the currency and the ops must produce the same item"
    );
    // The generated method's `rarity` requirement is `normal` or `rare`, so a magic item is
    // outside it. Both implementations refuse it: a currency may not be more permissive than the
    // method it stands for.
    let magic = item(data, MAGIC);
    assert!(
        matches!(essence.apply(&magic, &None), ItemOutputSet::Refused(_)),
        "the method refuses a magic item, so the currency does too"
    );
    let method = data.method(essence.id()).expect("method");
    assert!(apply(data, method, &[magic], &Params::default()).is_err());
}

// (b) The essence's own modifier is the only explicit the step leaves, on the side the bundle
// declares for it, and the reforge makes the item rare.
#[test]
fn the_essence_places_exactly_its_own_modifier() {
    let data = poe1::test_data();
    let name = "Deafening Essence of Hatred";
    let essence = Essence::named(data, name).expect("the essence");
    let shield = item(data, NORMAL);
    let m = declared_mod(data, name, &shield);
    let set = essence.apply(&shield, &None);
    let out = one(&set);
    let side = data
        .mod_at(m)
        .side
        .expect("an explicit modifier has a side");
    assert_eq!(out.added, Some((side, m)), "the step reports what it added");
    assert_eq!(
        out.item.explicits().count(),
        1,
        "the reforge clears the sides, leaving exactly the essence's modifier"
    );
    assert_eq!(out.item.affixes(side).len(), 1);
    assert_eq!(out.item.affixes(side)[0].roll.m, m);
    assert_eq!(out.item.rarity, Rarity::Rare);
}

// (c) The item-level cap is not a refusal: claim `essence.ilvl` says it caps the levels of the
// reforge's random modifiers, not the item level the essence may be used on, and the generated
// method's `requires` says nothing about it either.
#[test]
fn an_item_level_cap_does_not_refuse_a_higher_item() {
    let data = poe1::test_data();
    let name = "Wailing Essence of Anger";
    let essence = Essence::named(data, name).expect("the essence");
    let index = data.essence_by_name(name).expect("the essence") as usize;
    let cap = data.essences[index]
        .item_level_max
        .expect("a capped essence");
    assert!(cap < 87, "the cap {cap} is below the shield's item level");
    let shield = item(data, NORMAL);
    let m = declared_mod(data, name, &shield);
    let set = essence.apply(&shield, &None);
    let out = one(&set);
    assert_eq!(
        out.added,
        Some((data.mod_at(m).side.expect("a side"), m)),
        "the essence still adds its modifier; the cap is the fill's"
    );
}

// (c) `corruption_only` is where the essence drops, not what it may be used on: nothing in the
// generated method's `requires` reads it, and the four corruption-only essences are ordinary
// essences otherwise. Essence of Delirium has a Shield modifier, so the owner's shield can take it.
#[test]
fn a_corruption_only_essence_works_on_an_uncorrupted_item() {
    let data = poe1::test_data();
    let name = "Essence of Delirium";
    let essence = Essence::named(data, name).expect("the essence");
    let index = data.essence_by_name(name).expect("the essence") as usize;
    assert!(data.essences[index].corruption_only, "the fixture essence");
    let shield = item(data, NORMAL);
    assert!(!shield.corrupted);
    let m = declared_mod(data, name, &shield);
    let set = essence.apply(&shield, &None);
    let out = one(&set);
    assert_eq!(out.added, Some((data.mod_at(m).side.expect("a side"), m)));
}

// (d) `allowsRare` is the generated method's rarity requirement: an essence that cannot reforge a
// rare item requires `normal` and refuses anything else.
#[test]
fn an_essence_that_cannot_reforge_a_rare_item_refuses_one() {
    let data = poe1::test_data();
    let name = "Wailing Essence of Anger";
    let essence = Essence::named(data, name).expect("the essence");
    let index = data.essence_by_name(name).expect("the essence") as usize;
    assert!(!data.essences[index].allows_rare);
    let rare = item(data, RARE);
    let refused = essence.apply(&rare, &None);
    assert!(
        matches!(refused, ItemOutputSet::Refused(why) if why.contains("normal")),
        "a rare item is refused: {refused:?}"
    );
    // The same essence accepts the item it does reforge.
    let normal = item(data, NORMAL);
    assert_eq!(essence.apply(&normal, &None).outcomes().len(), 1);
}

// Claim `essence.ignores-metamods`: the reforge clears a side a lock protects as if the craft were
// not there, and a Cannot Roll metamod never stops the essence's own modifier. A `Suffixes Cannot
// Be Changed` prefix is the interesting case: the essence's modifier is a suffix, and the lock
// would forbid changing that side.
#[test]
fn a_lock_does_not_stop_the_essence() {
    let data = poe1::test_data();
    let name = "Deafening Essence of Hatred";
    let essence = Essence::named(data, name).expect("the essence");
    let locked = item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"rare","influences":["shaper"],
            "prefixes":[{"id":"DexMasterItemGenerationCannotChangeSuffixes"}],
            "suffixes":[{"id":"FireResist8"}]}"#,
    );
    let set = essence.apply(&locked, &None);
    let out = one(&set);
    assert_eq!(out.item.crafted_count(data), 0, "the craft is cleared too");
    assert_eq!(out.item.explicits().count(), 1);
    let m = declared_mod(data, name, &locked);
    assert_eq!(out.added, Some((data.mod_at(m).side.expect("a side"), m)));
    assert_eq!(
        out.item,
        op_certain(data, essence.id(), &locked),
        "the ops ignore the lock the same way"
    );
}
