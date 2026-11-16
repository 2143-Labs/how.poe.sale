//! The shield's published numbers, and the rules every orb has to obey.
//!
//! These are the checks the baseline is: the pool is the shield's own, the three chances the plan
//! names come out of it exactly, one orb's step is exact, and nothing can roll past its side's room
//! or repeat a modifier family.

use simple::craft::{LoopPolicy, Machine, Stage};
use simple::currency::{ItemOutputSet, registry};
use simple::data::data;
use simple::filter::{Accept, ItemSetFilter};
use simple::fraction::Fraction;
use simple::item::{Affix, Item, Rarity, Side};
use simple::pool::{draw_weights, side_chance};
use simple::target::Target;

fn totem() -> u16 {
    data().id_of("MaximumTotemUber1").unwrap()
}

fn shield_goal() -> ItemSetFilter {
    ItemSetFilter::from_data(data(), Accept::DesiredOrGoodBothSides)
}

fn weight(pool: &[(u16, u128)]) -> u128 {
    pool.iter().map(|&(_, weight)| weight).sum()
}

/// 1. The pool is the shield's: 73 prefixes over 64800 weight, 131 suffixes over 96475.
#[test]
fn the_pool_is_the_shields() {
    let data = data();
    let prefix = draw_weights(data, &Item::new(Rarity::Magic), Side::Prefix);
    let suffix = draw_weights(data, &Item::new(Rarity::Magic), Side::Suffix);
    assert_eq!(prefix.len(), 73, "73 prefixes roll on the shield");
    assert_eq!(suffix.len(), 131, "131 suffixes roll on the shield");
    assert_eq!(weight(&prefix), 64800);
    assert_eq!(weight(&suffix), 96475);
}

/// 2. The three published chances, read off a fresh magic shield.
#[test]
fn the_three_chances() {
    let data = data();
    let filter = shield_goal();
    let magic = Item::new(Rarity::Magic);

    // The totem modifier is the only desired prefix: 800 of the 64800 weight.
    let desired = side_chance(data, &magic, Side::Prefix, |m| {
        filter.tier_of(m) == simple::filter::Tier::Desired
    })
    .unwrap();
    assert_eq!(desired, Fraction::new(1, 81).unwrap());

    // The good prefixes are IncreasedLife10/9, IncreasedMana12 and both block tiers: 4000 weight.
    let good_prefix = side_chance(data, &magic, Side::Prefix, |m| {
        filter.tier_of(m) == simple::filter::Tier::Good
    })
    .unwrap();
    assert_eq!(good_prefix, Fraction::new(5, 81).unwrap());

    // Twenty good suffixes over the 96475 suffix weight: 14600 / 96475 = 584 / 3859.
    let good_suffix = side_chance(data, &magic, Side::Suffix, |m| {
        filter.tier_of(m) == simple::filter::Tier::Good
    })
    .unwrap();
    assert_eq!(good_suffix, Fraction::new(584, 3859).unwrap());
}

/// The orb a policy would name, applied to one item.
fn apply(id: &str, item: &Item) -> ItemOutputSet {
    registry(data(), id)
        .expect("a registered orb")
        .apply(item, &None)
}

/// 3. One transmutation from a normal shield is 1/108 for the totem prefix.
///
/// Half the time the orb adds one modifier and half the time two. One modifier: the prefix is drawn
/// only when the side comes up prefix (½) and the totem modifier is 1/81 of that side's pool, so
/// ½·1/81 = 1/162. Two modifiers: the totem modifier arrives first on the prefix side (½·1/81) or
/// second, after a suffix (½·1/81), so 1/162 + 1/162 = 1/81. Together ½·1/162 + ½·1/81 = 1/108.
#[test]
fn one_transmute_is_one_in_one_hundred_and_eight() {
    let set = apply("currency/transmutation", &Item::new(Rarity::Normal));
    let totem = totem();
    let chance = set
        .chance(|item| item.affixes(Side::Prefix).iter().any(|a| a.m == totem))
        .expect("a transmutation always produces something");
    assert_eq!(chance, Fraction::new(1, 108).unwrap());
}

/// 4. Every set is exact, and every orb's chances are a whole.
#[test]
fn chances_are_whole_and_exact() {
    let normal = Item::new(Rarity::Normal);
    let mut magic = Item::new(Rarity::Magic);
    magic.affixes_mut(Side::Prefix).push(Affix::new(totem()));

    for (id, item) in [
        ("currency/transmutation", &normal),
        ("currency/alteration", &magic),
    ] {
        let set = apply(id, item);
        assert!(set.exact(), "{id} produced an inexact set");
        let whole = set.chance(|_| true).unwrap();
        assert_eq!(whole, Fraction::ONE, "{id}'s chances do not add up");
    }
}

/// The seven ids the crate models, and the only ones it models.
const ORBS: [&str; 7] = [
    "currency/transmutation",
    "currency/alteration",
    "currency/augmentation",
    "currency/regal",
    "currency/exalted",
    "currency/scouring",
    "currency/annulment",
];

#[test]
fn the_registry_is_exactly_the_seven_orbs() {
    for id in ORBS {
        let orb = registry(data(), id).unwrap_or_else(|| panic!("{id} is not registered"));
        assert_eq!(orb.id(), id, "the orb reports a different id than its key");
    }
    for id in [
        "currency/alchemy",
        "currency/chaos",
        "currency/veiled-chaos",
        "currency/divine",
        "currency/essence",
    ] {
        assert!(registry(data(), id).is_none(), "{id} must not be modelled");
    }
}

/// 5. Rarity and room.
///
/// No step puts a second prefix on a magic item, no step puts a fourth modifier on a rare side, and
/// an orb that needs room refuses when there is none.
#[test]
fn no_step_rolls_past_its_sides_room() {
    let normal = Item::new(Rarity::Normal);
    for outcome in apply("currency/transmutation", &normal).outcomes() {
        let item = &outcome.item;
        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.affixes(Side::Prefix).len() <= 1);
        assert!(item.affixes(Side::Suffix).len() <= 1);
    }

    // A magic item with one prefix is regalled into a rare item that may take a second prefix.
    let mut magic = Item::new(Rarity::Magic);
    magic.affixes_mut(Side::Prefix).push(Affix::new(totem()));
    for outcome in apply("currency/regal", &magic).outcomes() {
        let item = &outcome.item;
        assert_eq!(item.rarity, Rarity::Rare);
        assert!(item.affixes(Side::Prefix).len() <= 3);
        assert!(item.affixes(Side::Suffix).len() <= 3);
    }

    // A rare item with three prefixes and three suffixes has no room left.
    let mut full = Item::new(Rarity::Rare);
    for side in Side::BOTH {
        for (m, _) in draw_weights(data(), &full, side).into_iter().take(3) {
            full.affixes_mut(side).push(Affix::new(m));
        }
    }
    assert_eq!(full.affixes(Side::Prefix).len(), 3);
    assert_eq!(full.affixes(Side::Suffix).len(), 3);
    assert!(matches!(
        apply("currency/exalted", &full),
        ItemOutputSet::Refused("the item has no room for another modifier")
    ));
}

/// 6. A modifier family cannot repeat: no orb puts a second life modifier beside the first.
#[test]
fn a_family_cannot_repeat_through_any_orb() {
    let data = data();
    let life = data.id_of("IncreasedLife10").unwrap();
    let lower_life = data.id_of("IncreasedLife9").unwrap();
    let mut rare = Item::new(Rarity::Rare);
    rare.affixes_mut(Side::Prefix).push(Affix::new(life));

    for id in ["currency/exalted", "currency/annulment"] {
        for outcome in apply(id, &rare).outcomes() {
            let item = &outcome.item;
            let held: Vec<u16> = Side::BOTH
                .into_iter()
                .flat_map(|side| item.affixes(side).iter().map(|a| a.m))
                .collect();
            let repeated = held.iter().filter(|m| **m == lower_life).count();
            assert!(repeated <= 1, "{id} put two life modifiers on one item");
        }
    }
    // The rolled pool itself never offers the second tier while the first is held.
    assert!(
        !draw_weights(data, &rare, Side::Prefix)
            .iter()
            .any(|&(m, _)| m == lower_life)
    );
}

/// 7. The filter is a filter.
///
/// It keeps the outcomes holding the desired modifier and reports exactly the mass it kept, so no
/// chance appears or vanishes.
#[test]
fn the_filter_keeps_the_mass_it_reports() {
    let data = data();
    let set = apply("currency/transmutation", &Item::new(Rarity::Normal));
    let wants_desired = ItemSetFilter::from_data(data, Accept::HasDesired);
    let chance = set
        .chance(|item| wants_desired.holds_desired(item))
        .expect("the step always produces something");
    assert_eq!(chance, Fraction::new(1, 108).unwrap());

    let filtered = set.filtered(&wants_desired);
    assert!(filtered.exact());
    assert!(
        filtered
            .outcomes()
            .iter()
            .all(|outcome| wants_desired.holds_desired(&outcome.item))
    );
    // The kept weights over the step's own total are the same chance, read the other way round.
    assert_eq!(
        Fraction::new(filtered.total(), set.total()).unwrap(),
        chance
    );
    // The filtered set is renormalised: over its own total it is a whole chance.
    assert_eq!(filtered.chance(|_| true).unwrap(), Fraction::ONE);
}

/// An item built from named modifiers, in the order given.
fn item_of(rarity: Rarity, prefixes: &[&str], suffixes: &[&str]) -> Item {
    let data = data();
    let mut item = Item::new(rarity);
    for (side, ids) in [(Side::Prefix, prefixes), (Side::Suffix, suffixes)] {
        for id in ids {
            let m = data
                .id_of(id)
                .unwrap_or_else(|| panic!("{id} is not in the shield's pool"));
            item.affixes_mut(side).push(Affix::new(m));
        }
    }
    item
}

/// 8. The target: the totem prefix, and two good suffixes beside it.
#[test]
fn the_target_needs_the_totem_prefix_and_two_good_suffixes() {
    let data = data();
    let filter = shield_goal();
    let target = Target::from_data(data);
    assert_eq!(target.prefixes.len(), 3);
    assert_eq!(target.prefixes[0], simple::filter::Tier::Desired);

    let reached = item_of(
        Rarity::Rare,
        &["MaximumTotemUber1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "FireResist8", "ColdResist8"],
    );
    assert!(
        target.reached(&filter, &reached),
        "this is the item the craft is for"
    );

    // Two suffixes is not three: the goal is a six-modifier rare item.
    let short = item_of(
        Rarity::Rare,
        &["MaximumTotemUber1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "FireResist8"],
    );
    assert!(!target.reached(&filter, &short));

    // Without the totem prefix the first floor is missed, however good the rest is.
    let no_totem = item_of(
        Rarity::Rare,
        &["AttackerTakesDamage1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "FireResist8", "ColdResist8"],
    );
    assert!(!target.reached(&filter, &no_totem));

    // One good suffix is not two.
    let one_good_suffix = item_of(
        Rarity::Rare,
        &["MaximumTotemUber1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "LifeRegeneration1", "StunRecovery1"],
    );
    assert!(!target.reached(&filter, &one_good_suffix));

    // A magic item never reaches it, whatever it holds.
    let magic = item_of(Rarity::Magic, &["MaximumTotemUber1"], &["AllResistances6"]);
    assert!(!target.reached(&filter, &magic));
}

/// The machine's row for one step, by stage.
fn row(machine: &simple::craft::Machine<'_>) -> Vec<(simple::craft::Stage, Fraction)> {
    machine.history().last().expect("a step was taken").clone()
}

fn mass(row: &[(simple::craft::Stage, Fraction)], stage: simple::craft::Stage) -> Fraction {
    row.iter()
        .find(|(label, _)| *label == stage)
        .map(|(_, mass)| *mass)
        .unwrap_or(Fraction::ZERO)
}

/// 3, again, through the machine: the first step from a normal shield is one transmutation, and it
/// puts exactly 1/108 of the mass in the desired stage.
#[test]
fn the_first_machine_step_is_one_transmutation() {
    let data = data();
    let filter = shield_goal();
    let target = Target::from_data(data);
    let mut machine = Machine::new(data, &filter, &target);
    machine.step(&LoopPolicy::new()).unwrap();

    assert_eq!(machine.history().len(), 1);
    let first = row(&machine);
    assert_eq!(
        mass(&first, Stage::Desired),
        Fraction::new(1, 108).unwrap(),
        "the totem prefix, one step in"
    );
    // One step is a whole chance: nothing is absorbed and nothing is dropped yet.
    let whole = first
        .iter()
        .try_fold(Fraction::ZERO, |sum, &(_, mass)| sum.checked_add(mass))
        .unwrap();
    assert_eq!(whole, Fraction::ONE);
    assert_eq!(machine.goal(), Fraction::ZERO);

    // A normal item has two modifiers at most, so every outcome is magic with one or two.
    assert!(
        machine
            .live()
            .iter()
            .all(|(item, _)| item.rarity == Rarity::Magic && item.explicits() <= 2)
    );
    // The desired stage really is the totem prefix, not a coincidence of labels.
    assert!(machine.live().iter().all(|(item, _)| {
        (Stage::classify(&filter, item) == Stage::Desired)
            == item.affixes(Side::Prefix).iter().any(|a| a.m == totem())
    }));
}
