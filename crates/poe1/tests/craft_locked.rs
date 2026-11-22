//! A lock names the side it protects and sits on the other one: `StrMasterItemGenerationCannot-
//! ChangePrefixes` ("Prefixes Cannot Be Changed") is a *suffix* modifier, and `Item::locked` finds
//! it by searching every explicit. The coarse class has to make the same test, because an
//! alteration's cached reroll is shared by every class whose clear leaves the same facts: a class
//! that misses the lock clears to the same key as a bare item and lends its reroll to it.

use std::collections::BTreeMap;

use poe1::craft::lumped::{Class, Cleared, Lumped};
use poe1::craft::{Action, CurrencyId, Machine, Policy, Stage, Step};
use poe1::currency::{Accept, Fraction, ItemSetFilter};
use poe1::data::{Data, Side};
use poe1::item::{Item, ItemSpec};
use poe1::pattern::{Pattern, PatternSpec};

/// The owner's shield base and goal, the same fixtures `craft_lumped` runs on.
fn shield_item(data: &Data, spec: &str) -> Item {
    let spec: ItemSpec = serde_json::from_str(spec).expect("spec");
    Item::from_spec(data, &spec).expect("the shield")
}

fn bare(data: &Data) -> Item {
    shield_item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
            "influences":["shaper"]}"#,
    )
}

/// A life prefix and the suffix that locks the prefix side.
fn locked(data: &Data) -> Item {
    shield_item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
            "influences":["shaper"],
            "prefixes":[{"id":"IncreasedLife10"}],
            "suffixes":[{"id":"StrMasterItemGenerationCannotChangePrefixes"}]}"#,
    )
}

/// The same prefix with no lock: what the locked item's clear must *not* be confused with.
fn unlocked_prefix(data: &Data) -> Item {
    shield_item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
            "influences":["shaper"],
            "prefixes":[{"id":"IncreasedLife10"}]}"#,
    )
}

/// A prefix and a suffix, neither locked nor fractured: a class that sorts after the locked one.
fn unlocked_pair(data: &Data) -> Item {
    shield_item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
            "influences":["shaper"],
            "prefixes":[{"id":"IncreasedLife10"}],
            "suffixes":[{"id":"AllResistances6"}]}"#,
    )
}

/// A fractured life prefix and the suffix that locks the prefix side: the clear keeps the side and
/// the affix is still fractured afterwards.
fn fractured_locked(data: &Data) -> Item {
    shield_item(
        data,
        r#"{"base":"Supreme Spiked Shield","itemLevel":87,"rarity":"magic",
            "influences":["shaper"],
            "prefixes":[{"id":"IncreasedLife9","fractured":true}],
            "suffixes":[{"id":"StrMasterItemGenerationCannotChangePrefixes"}]}"#,
    )
}

fn pattern(data: &Data) -> Pattern {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/patterns/totem-shield.json"
    );
    let spec: PatternSpec =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the pattern fixture"))
            .expect("pattern spec");
    Pattern::compile(data, &spec).expect("compiles")
}

fn filter(pattern: &Pattern) -> ItemSetFilter {
    ItemSetFilter::from_pattern(pattern, Accept::DesiredOrGoodBothSides)
}

/// The action every test takes, forced: `Lumped` shares an alteration's reroll only when the move
/// is `Action::Apply(CurrencyId::ALTERATION)`.
struct Alter;

impl Policy for Alter {
    fn step(&self, _data: &Data, _item: &Item) -> Step {
        Step {
            action: Action::Apply(CurrencyId::ALTERATION),
            reason: "the test forces an alteration",
        }
    }
}

/// One concrete step's live items folded by class, and its per-stage masses.
fn concrete(
    data: &Data,
    filter: &ItemSetFilter,
    start: Item,
) -> (BTreeMap<Class, Fraction>, Vec<(Stage, Fraction)>) {
    let mut machine = Machine::new(data, filter.clone(), start);
    let snapshot = machine.step(&Alter).expect("an alteration is exact");
    let mut folded: BTreeMap<Class, Fraction> = BTreeMap::new();
    for (item, p) in snapshot.state {
        let slot = folded
            .entry(Class::of(data, filter, item))
            .or_insert(Fraction::ZERO);
        *slot = slot.checked_add(*p).expect("exact");
    }
    (folded, snapshot.chances.to_vec())
}

fn plus(a: BTreeMap<Class, Fraction>, b: BTreeMap<Class, Fraction>) -> BTreeMap<Class, Fraction> {
    let mut out = a;
    for (class, p) in b {
        let slot = out.entry(class).or_insert(Fraction::ZERO);
        *slot = slot.checked_add(p).expect("exact");
    }
    out
}

fn stage_sum(a: &[(Stage, Fraction)], b: &[(Stage, Fraction)]) -> Vec<(Stage, Fraction)> {
    let mut out: BTreeMap<Stage, Fraction> = BTreeMap::new();
    for &(stage, p) in a.iter().chain(b) {
        let slot = out.entry(stage).or_insert(Fraction::ZERO);
        *slot = slot.checked_add(p).expect("exact");
    }
    out.into_iter().collect()
}

/// Whether the item still holds the life prefix.
fn holds_life(data: &Data, item: &Item) -> bool {
    item.affixes(Side::Prefix)
        .iter()
        .any(|a| data.mod_at(a.roll.m).id == "IncreasedLife10")
}

/// The lock is a suffix that names the prefix side, so the clear keeps that side and drops the lock
/// itself. The cleared key has to say so, because it is the alteration cache's key.
#[test]
fn a_lock_clears_from_the_side_it_names() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));

    let locked_class = Class::of(data, &filter, &locked(data));
    let bare_class = Class::of(data, &filter, &bare(data));
    let prefix_class = Class::of(data, &filter, &unlocked_prefix(data));

    let cleared: Cleared = locked_class.cleared();
    assert_eq!(
        cleared.affixes(Side::Prefix),
        (1, 0),
        "the locked prefix side survives the clear, and it is not fractured"
    );
    assert_eq!(
        cleared.affixes(Side::Suffix),
        (0, 0),
        "the lock sits on the suffix side and locks the prefix side, so the clear takes the lock"
    );

    assert_ne!(
        cleared,
        bare_class.cleared(),
        "a locked shield must not clear to a bare magic shield"
    );
    assert_ne!(
        cleared,
        prefix_class.cleared(),
        "a locked shield must not clear to the same prefix with no lock"
    );
    assert_eq!(
        bare_class.cleared().affixes(Side::Prefix),
        (0, 0),
        "an unlocked side keeps only its fractured affixes, and a bare shield has none"
    );
    assert_eq!(
        prefix_class.cleared().affixes(Side::Prefix),
        (0, 0),
        "an unlocked, unfractured prefix does not survive a clear"
    );

    // The locked item's own alteration is the concrete one, and every outcome keeps the prefix.
    let (folded, chances) = concrete(data, &filter, locked(data));
    let mut lumped = Lumped::new(data, filter.clone(), locked(data));
    let depth = lumped.step(&Alter).expect("the locked item is alterable");
    assert_eq!(
        depth.chances, chances,
        "the lumped step's per-stage masses are the concrete step's"
    );
    assert_eq!(
        depth.classes,
        folded.into_iter().collect::<Vec<_>>(),
        "the concrete distribution folded by class is the lumped distribution"
    );
    assert!(
        !depth.classes.is_empty(),
        "the locked item leaves live outcomes to compare"
    );
    for (class, _) in &depth.classes {
        assert!(
            holds_life(data, class.witness()),
            "a clear that keeps the locked prefix keeps it in every outcome"
        );
    }
}

/// The alteration cache is shared by cleared key, so both visit orders are checked: the class the
/// state visits first populates the entry, and the other must still get its own reroll.
#[test]
fn the_alteration_cache_never_lends_one_class_anothers_reroll() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));

    // The state is a `BTreeMap`, so which class populates the cache is decided by class order, not
    // by insertion order. Each pair names the class that sorts first and the one that borrows.
    let pairs = [
        (bare(data), locked(data)),
        (locked(data), unlocked_pair(data)),
    ];
    for (first, second) in pairs {
        let mut lumped = Lumped::new(data, filter.clone(), first.clone());
        lumped.insert(second.clone(), Fraction::ONE).expect("exact");
        assert_eq!(
            lumped.state().keys().next().expect("two classes").clone(),
            Class::of(data, &filter, &first),
            "the case relies on this class populating the cache first"
        );

        let depth = lumped.step(&Alter).expect("both classes are alterable");
        let (a, chances_a) = concrete(data, &filter, first.clone());
        let (b, chances_b) = concrete(data, &filter, second.clone());
        assert!(
            !a.is_empty() && !b.is_empty(),
            "each concrete run leaves live outcomes"
        );
        assert_eq!(
            depth.chances,
            stage_sum(&chances_a, &chances_b),
            "each class's stage masses are its own"
        );
        assert_eq!(
            depth.classes,
            plus(a, b).into_iter().collect::<Vec<_>>(),
            "each class's live outcomes are its own, whichever one populated the cache"
        );
    }
}

/// A fractured affix under a lock keeps both facts: the side is kept because it is locked, and the
/// affix is still fractured afterwards, so the cleared key records it on both lists.
#[test]
fn a_fractured_prefix_under_a_lock_keeps_both_facts() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));

    let fractured_class = Class::of(data, &filter, &fractured_locked(data));
    let cleared: Cleared = fractured_class.cleared();
    assert_eq!(
        cleared.affixes(Side::Prefix),
        (1, 1),
        "the locked prefix survives the clear and is still fractured"
    );
    assert_eq!(
        cleared.affixes(Side::Suffix),
        (0, 0),
        "the lock itself is on the unlocked suffix side and is removed"
    );
    assert_ne!(
        cleared,
        Class::of(data, &filter, &locked(data)).cleared(),
        "a fractured prefix and an unfractured one are not the same cleared facts"
    );
    assert_ne!(
        cleared,
        Class::of(data, &filter, &bare(data)).cleared(),
        "nor does the fractured shield clear to a bare one"
    );

    let (folded, chances) = concrete(data, &filter, fractured_locked(data));
    let mut lumped = Lumped::new(data, filter.clone(), fractured_locked(data));
    let depth = lumped
        .step(&Alter)
        .expect("the fractured item is alterable");
    assert_eq!(
        depth.chances, chances,
        "the lumped step's per-stage masses are the concrete step's"
    );
    assert_eq!(
        depth.classes,
        folded.into_iter().collect::<Vec<_>>(),
        "the concrete distribution folded by class is the lumped distribution"
    );
    assert!(!depth.classes.is_empty(), "the step leaves live outcomes");
    for (class, _) in &depth.classes {
        let witness = class.witness();
        assert!(
            witness
                .affixes(Side::Prefix)
                .iter()
                .any(|a| a.fractured && data.mod_at(a.roll.m).id == "IncreasedLife9"),
            "the fractured prefix survives every outcome, still fractured"
        );
    }
}
