//! The per-currency model's chains of draws: a step whose op path draws more than once must be the
//! same distribution as the ops, item for item.
//!
//! Two things the single-draw model got wrong are pinned here.
//!
//! A dead end inside a `chance` branch is a conditional no-op in the op path (`Run::nothing`), not
//! a lost path. An alteration on a magic item whose free side has no room — a fractured modifier,
//! or a modifier a lock protects — draws nothing on the second add, and the item still comes out
//! holding what it held. Multiplying that empty draw into the set zeroed the whole step.
//!
//! A chain of draws must normalise each draw against the state it is drawn on, because the second
//! draw's pool depends on which modifier the first draw added (families, groups, room). On a
//! jewel every first-draw result leaves a different second-draw total, which is where the error is
//! largest: the old code was 5.46 % relative out on a Viridian Jewel and still reported `exact()`.

use std::collections::BTreeMap;

use poe1::apply::{Leaf, Params, apply};
use poe1::currency::{
    Alteration, Augmentation, Currency, Fraction, ItemOutputSet, Mass, Transmutation, weigh,
};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::test_data;

/// The agreement the op path's own `f64` arithmetic allows. The code path is exact; the op path
/// sums `f64` probabilities, so anything inside this is the op path's rounding, not a difference in
/// the model.
const TOLERANCE: f64 = 1e-12;

fn item(data: &Data, json: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {json}"));
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{e}: {json}"))
}

/// The op-declared method's whole distribution, as items with their chances.
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

/// Asserts the two distributions are the same set of items with the same chances, returning the
/// largest absolute difference so a caller can report it. Any item on one side only is a failure:
/// a shared number with a missing item is not agreement.
#[track_caller]
fn agrees(label: &str, set: &ItemOutputSet, op: &BTreeMap<Item, f64>) -> f64 {
    let code = code_distribution(set);
    let op_total: f64 = op.values().sum();
    assert!(
        (op_total - 1.0).abs() < 1e-9,
        "{label}: the ops sum to {op_total}"
    );
    for (it, p) in &code {
        let q = op
            .get(it)
            .copied()
            .unwrap_or_else(|| panic!("{label}: the code produces an item the ops do not: {it:?}"));
        assert!(
            (p - q).abs() < TOLERANCE,
            "{label}: one item's chance is {p} as code and {q} as ops"
        );
    }
    for it in op.keys() {
        assert!(
            code.contains_key(it),
            "{label}: the ops produce an item the code does not: {it:?}"
        );
    }
    code.iter()
        .map(|(it, p)| (p - op[it]).abs())
        .fold(0.0f64, f64::max)
}

/// A magic jewel's reroll, compared against `currency/alteration`.
fn alteration_jewel(data: &Data, base: &str) {
    let plain = item(
        data,
        &format!(r#"{{"base":"{base}","itemLevel":83,"rarity":"magic"}}"#),
    );
    let set = Alteration::new(data).apply(&plain, &None);
    assert!(set.exact(), "{base}: the alteration set must be exact");
    assert!(set.total() > 0, "{base}: the alteration set lost its mass");
    let worst = agrees(
        &format!("alteration on a {base}"),
        &set,
        &op_distribution(data, "currency/alteration", &plain),
    );
    assert!(
        worst < TOLERANCE,
        "{base}: worst absolute error {worst:e} exceeds {TOLERANCE:e}"
    );
}

/// A normal jewel's transmutation, compared against `currency/transmutation`.
fn transmutation_jewel(data: &Data, base: &str) {
    let normal = item(
        data,
        &format!(r#"{{"base":"{base}","itemLevel":83,"rarity":"normal"}}"#),
    );
    let set = Transmutation::new(data).apply(&normal, &None);
    assert!(set.exact(), "{base}: the transmutation set must be exact");
    let worst = agrees(
        &format!("transmutation on a {base}"),
        &set,
        &op_distribution(data, "currency/transmutation", &normal),
    );
    assert!(
        worst < TOLERANCE,
        "{base}: worst absolute error {worst:e} exceeds {TOLERANCE:e}"
    );
}

/// The case that exposed the missing per-state normalisation: on these jewels the second draw's
/// total depends on the first draw's result, and the old integer scaling was out by 5.46 %
/// (Viridian) and 3.76 % (Cobalt) on the worst outcome while `exact()` still reported true.
#[test]
fn a_chain_of_draws_normalises_each_draw() {
    let data = test_data();
    for base in ["Viridian Jewel", "Cobalt Jewel"] {
        alteration_jewel(data, base);
        transmutation_jewel(data, base);
    }
}

/// The fixture that exposed the lost mass: the fractured prefix holds the item's only prefix slot,
/// so the second draw has no room anywhere and the op path keeps what the first draw gave the item
/// instead of dropping the path.
const FRACTURED_SHIELD: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
    "influences":["shaper"],"prefixes":[{"id":"IncreasedLife10","fractured":true}]}"#;

/// The same shape without the fracture: the lock protects the prefix rather than making it
/// unremovable, so the prefix stays and the suffix side is the only one with room — and the
/// alteration's own clear removes the lock first, leaving one free suffix slot and no second draw.
const LOCKED_SHIELD: &str = r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
    "influences":["shaper"],"prefixes":[{"id":"IncreasedLife10"}],
    "suffixes":[{"id":"StrMasterItemGenerationCannotChangePrefixes"}]}"#;

fn alteration_of_a_shield(data: &Data, label: &str, json: &str, kept: &str) {
    let shield = item(data, json);
    let set = Alteration::new(data).apply(&shield, &None);
    assert!(
        set.exact(),
        "{label}: the dead-end draw zeroed the set instead of leaving the item alone"
    );
    assert!(set.total() > 0, "{label}: the set lost its mass");
    assert_eq!(
        set.outcomes().len(),
        131,
        "{label}: one draw of the free side's pool, no second draw"
    );
    for outcome in set.outcomes() {
        assert!(
            outcome
                .item
                .prefixes
                .iter()
                .any(|a| data.mod_at(a.roll.m).id == kept),
            "{label}: an outcome lost {kept}: {outcome:?}"
        );
    }
    let worst = agrees(
        label,
        &set,
        &op_distribution(data, "currency/alteration", &shield),
    );
    assert!(
        worst < TOLERANCE,
        "{label}: worst absolute error {worst:e} exceeds {TOLERANCE:e}"
    );
}

/// A fractured modifier cannot be removed, so the alteration keeps it and has one free side; the
/// second draw has no room and must be a no-op, not a zero-weight path.
#[test]
fn a_dead_end_keeps_the_item() {
    let data = test_data();
    alteration_of_a_shield(
        data,
        "alteration on a fractured shield",
        FRACTURED_SHIELD,
        "IncreasedLife10",
    );
}

/// A modifier a lock protects is the same dead end: the alteration clears the lock (a suffix lock
/// does not stop a suffix being removed), keeps the locked prefix, and has one free side.
#[test]
fn a_lock_protected_dead_end_keeps_the_item() {
    let data = test_data();
    alteration_of_a_shield(
        data,
        "alteration on a lock-protected prefix",
        LOCKED_SHIELD,
        "IncreasedLife10",
    );
}

/// An augmentation adds unconditionally, so an empty pool is a refusal in the op path and must be
/// one here too — not a set holding the unchanged item at chance one.
///
/// The item is a magic Hybrid Flask at item level 1: its class allows one suffix, but the flask's
/// suffix pool is empty at that level, so the suffix slot has room and nothing can fill it.
#[test]
fn an_unconditional_add_refuses_an_empty_pool() {
    let data = test_data();
    let flask = item(
        data,
        r#"{"base":"Small Hybrid Flask","itemLevel":1,"rarity":"magic",
            "prefixes":[{"id":"FlaskIncreasedRecoveryAmount1"}]}"#,
    );
    assert!(
        poe1::apply::room(data, &flask, poe1::data::Side::Suffix) > 0,
        "the test item must have room, or it is testing the wrong refusal"
    );
    let reason = match Augmentation::new(data).apply(&flask, &None) {
        ItemOutputSet::Refused(reason) => reason,
        other => panic!("an empty pool must refuse, not return a set: {other:?}"),
    };
    assert!(
        reason.contains("no modifier"),
        "the refusal must say the pool is empty, not something else: {reason}"
    );
    // The ops refuse the same step for the same reason: the pool, not the room.
    let method = data.method("currency/augmentation").expect("a method");
    let op = apply(
        data,
        method,
        std::slice::from_ref(&flask),
        &Params::default(),
    );
    assert!(op.is_err(), "the op path must refuse this item too");
}

/// The conversion to integer weights is where an inexact set would be born: if the common
/// denominator does not fit, the caller must be told rather than handed a rounded distribution.
/// Two chances with coprime denominators whose product exceeds `u128` cannot share a denominator.
#[test]
fn an_overflowing_common_denominator_is_refused() {
    let data = test_data();
    let first = item(
        data,
        r#"{"base":"Viridian Jewel","itemLevel":83,"rarity":"magic"}"#,
    );
    // 2^127 and 3^80 are coprime and each fits in u128; their least common multiple is 2^127·3^80,
    // far past u128::MAX (2^128 - 1).
    let huge = Fraction::new(1, 2u128.pow(127)).expect("1/2^127");
    let other = Fraction::new(1, 3u128.pow(80)).expect("1/3^80");
    assert!(other.den() < u128::MAX);
    let mass: Vec<Mass> = vec![(first.clone(), huge, None), (first, other, None)];
    assert_eq!(
        weigh(mass),
        None,
        "an unrepresentable common denominator must not be rounded"
    );
}

/// The other side of the dead-end rule, and the reason the pair is load-bearing: the *first* draw
/// is what makes a step a use of the currency, so a step whose first draw has nothing to take is
/// refused and the currency is not consumed (claim `empty.refused`). A magic item whose two affixes
/// are both fractured has no room at all: nothing can be drawn, and the refusal is not a set
/// holding the item at 100 %.
#[test]
fn a_first_draw_that_cannot_happen_is_a_refusal() {
    let data = test_data();
    let both = item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic","influences":["shaper"],
            "prefixes":[{"id":"IncreasedLife10","fractured":true}],
            "suffixes":[{"id":"FireResist8","fractured":true}]}"#,
    );
    match Alteration::new(data).apply(&both, &None) {
        ItemOutputSet::Refused(why) => assert!(
            why.contains("no outcome"),
            "the refusal must say the step produced nothing: {why}"
        ),
        other => panic!(
            "a step that changes nothing must be refused, got {} outcomes / total {}",
            other.outcomes().len(),
            other.total()
        ),
    }
}
