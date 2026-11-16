//! The shield's published numbers, and the rules every orb has to obey.
//!
//! These are the checks the baseline is: the pool is the shield's own, the three chances the plan
//! names come out of it exactly, one orb's step is exact, and nothing can roll past its side's room
//! or repeat a modifier family.

use simple::currency::{ItemOutputSet, registry};
use simple::data::data;
use simple::filter::{Accept, ItemSetFilter};
use simple::fraction::Fraction;
use simple::item::{Affix, Item, Rarity, Side};
use simple::pool::{draw_weights, side_chance};

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
