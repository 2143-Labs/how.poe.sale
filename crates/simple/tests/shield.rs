//! The shield's published numbers, the rules every orb has to obey, and the values the model
//! carries.
//!
//! These are the checks the baseline is: the pool is the shield's own and its influence's, the
//! three chances the plan names come out of it exactly, one orb's step is exact, nothing can roll
//! past its side's room or repeat a modifier family, and a value is an interval so a
//! Divine cannot be confused with a draw.
//!
//! Every chance is an exact [`Fraction`].

use simple::cost::{Cost, CurrencyId};
use simple::currency::{Applied, Currency, ON_ITEMS, find};
use simple::data::{Data, data};
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Affix, Influence, Item, Rarity, Side};
use simple::pool::{draw_weights, side_chance};
use simple::roll::Roll;
use simple::stock::Refusal;
use simple::target::Target;

mod common;

/// The one base this crate models.
const BASE: &str = "Supreme Spiked Shield";

/// The item level the craft runs at: high enough for every modifier this base can roll.
const LEVEL: u8 = 87;

/// The base's index in the data file.
fn base() -> u16 {
    data().base_of(BASE)
}

/// A fresh item of the shield's base, at [`LEVEL`], with or without the Shaper influence.
fn item(rarity: Rarity, influence: Option<Influence>) -> Item {
    let item = Item::of_base(data(), base(), LEVEL, rarity);
    match influence {
        Some(influence) => item.with_influence(influence),
        None => item,
    }
}

/// The item the craft is for: a Shaper shield.
fn shaper(rarity: Rarity) -> Item {
    item(rarity, Some(Influence::Shaper))
}

fn totem() -> u16 {
    data().id_of("MaximumTotemUber1").unwrap()
}

fn shield_goal() -> ItemSetFilter {
    ItemSetFilter::from_data(data())
}

fn weight(pool: &[(u16, u128)]) -> u128 {
    pool.iter().map(|&(_, weight)| weight).sum()
}

/// The one stat total the item holds under this stat id.
fn stat(totals: &[(u16, Roll)], data: &Data, id: &str) -> Roll {
    totals
        .iter()
        .find(|(stat, _)| data.stat_id(*stat) == id)
        .map(|&(_, roll)| roll)
        .unwrap_or_else(|| panic!("{id} is not a stat of the item"))
}

/// The orb a policy would name, applied to one item.
fn apply(id: &'static str, item: &Item) -> Result<Applied<Item>, Refusal> {
    find(&ON_ITEMS, CurrencyId(id))
        .expect("a modelled orb")
        .apply(data(), item)
}

/// Asserts that a use is a refusal with exactly this reason.
fn assert_refused(result: Result<Applied<Item>, Refusal>, why: &str) {
    match result {
        Err(Refusal(got)) => assert_eq!(got, why),
        Ok(_) => panic!("expected a refusal ({why}), got outcomes"),
    }
}

// ---------------------------------------------------------------------------------------------
// The item: its implicit, its item level, its influence, and its values.
// ---------------------------------------------------------------------------------------------

/// 1 (v2). Every item of the base carries the base's implicit, at its full range, and
/// `stat_totals` counts it.
///
/// The implicit is not drawn and never leaves the item, so scouring, divining, transming and
/// quality all keep it: it is a property of the base. `ChanceToDodgeSpellsImplicitShield2` is
/// `5` to `5` spell suppression, so the interval is the single value `5`.
#[test]
fn the_base_implicit_is_on_every_item() {
    let data = data();
    let implicit = data
        .id_of("ChanceToDodgeSpellsImplicitShield2")
        .expect("the base's implicit is in the pool");
    for rarity in [Rarity::Normal, Rarity::Magic, Rarity::Rare] {
        for influence in [None, Some(Influence::Shaper)] {
            let item = item(rarity, influence);
            assert_eq!(
                item.implicits().len(),
                1,
                "a fresh {} shield holds one implicit",
                item.describe(data)
            );
            let held = &item.implicits()[0];
            assert_eq!(held.m, implicit);
            assert_eq!(held.rolls, vec![Roll::range(5, 5).unwrap()]);
            assert_eq!(
                stat(
                    &item.stat_totals(data),
                    data,
                    "base_spell_suppression_chance_%"
                ),
                Roll::range(5, 5).unwrap(),
                "the implicit is part of the item's totals"
            );
        }
    }
}

/// 2 (v2). Values are intervals and they add exactly.
///
/// The implicit's `5` plus `ChanceToSuppressSpellsHigh4_`'s `17-19` is `22-24`, a pinned `18`
/// makes it the single value `23`, and `IncreasedLife10` (`145-159`) with `AllResistances6`
/// (`17-18`) gives two separate stat totals — a value is carried as the set of numbers the model
/// admits, never as a rolled number.
#[test]
fn values_are_intervals_and_add_exactly() {
    let data = data();
    let suppression = "base_spell_suppression_chance_%";

    let rolled = item_of(
        Some(Influence::Shaper),
        Rarity::Rare,
        &[],
        &["ChanceToSuppressSpellsHigh4_"],
    );
    assert_eq!(
        stat(&rolled.stat_totals(data), data, suppression),
        Roll::range(22, 24).unwrap(),
        "5 plus 17-19 is 22-24"
    );

    let mut pinned = item_of(Some(Influence::Shaper), Rarity::Rare, &[], &[]);
    let high = data.id_of("ChanceToSuppressSpellsHigh4_").unwrap();
    pinned
        .affixes_mut(Side::Suffix)
        .push(Affix::pinned(data, high, &[18]).expect("18 is in the mod's range"));
    assert_eq!(
        stat(&pinned.stat_totals(data), data, suppression),
        Roll::exact(23),
        "5 plus a pinned 18 is exactly 23"
    );

    let both = item_of(
        Some(Influence::Shaper),
        Rarity::Rare,
        &["IncreasedLife10"],
        &["AllResistances6"],
    );
    let totals = both.stat_totals(data);
    assert_eq!(
        stat(&totals, data, "base_maximum_life"),
        Roll::range(145, 159).unwrap()
    );
    assert_eq!(
        stat(&totals, data, "base_resist_all_elements_%"),
        Roll::range(17, 18).unwrap()
    );
}

/// 3 (v2). The item level gates the pool, not the base's drop level.
///
/// At item level 60 the shield rolls 53 prefixes over 48000 weight and 76 suffixes over 62600,
/// every one of them at level 60 or below.
#[test]
fn item_level_gates_the_pool() {
    let data = data();
    for influence in [None, Some(Influence::Shaper)] {
        let item = Item::of_base(data, base(), 60, Rarity::Magic);
        let item = match influence {
            Some(influence) => item.with_influence(influence),
            None => item,
        };
        let prefix = draw_weights(data, &item, Side::Prefix);
        let suffix = draw_weights(data, &item, Side::Suffix);
        assert_eq!(prefix.len(), 53, "53 prefixes roll at item level 60");
        assert_eq!(weight(&prefix), 48000);
        assert_eq!(suffix.len(), 76, "76 suffixes roll at item level 60");
        assert_eq!(weight(&suffix), 62600);
        assert!(
            prefix
                .iter()
                .chain(&suffix)
                .all(|&(m, _)| data.mod_at(m).level <= 60),
            "no modifier above level 60 is in the pool"
        );
        // The totem modifier is level 70, so even an influenced one is gated out here.
        assert!(!prefix.iter().any(|&(m, _)| m == totem()));
    }
}

/// 4 (v2). The influence is a property of the item, and it is what adds the totem modifier.
///
/// The same base without an influence rolls 72 prefixes over 64000 weight and 121 suffixes over
/// 88475 — eleven modifiers fewer — and `MaximumTotemUber1` has weight 0 on it, so it is out of
/// the pool and its chance is zero. With `Influence::Shaper` the pool is the whole 204 and the
/// totem prefix is 1/81 again.
#[test]
fn the_influence_adds_the_totem_modifier() {
    let data = data();
    let plain = item(Rarity::Magic, None);
    let prefix = draw_weights(data, &plain, Side::Prefix);
    let suffix = draw_weights(data, &plain, Side::Suffix);
    assert_eq!(prefix.len(), 72, "72 prefixes roll without an influence");
    assert_eq!(weight(&prefix), 64000);
    assert_eq!(suffix.len(), 121, "121 suffixes roll without an influence");
    assert_eq!(weight(&suffix), 88475);
    assert!(
        !prefix.iter().any(|&(m, _)| m == totem()),
        "the totem modifier is influence-only"
    );
    assert_eq!(data.weight_on(totem(), base(), None), 0);
    assert_eq!(
        side_chance(data, &plain, Side::Prefix, |m| m == totem()),
        Some(Fraction::ZERO),
        "an absent modifier's chance is zero, not a missing side"
    );

    // The eleven extra modifiers are the influence's.
    let influenced = item(Rarity::Magic, Some(Influence::Shaper));
    assert_eq!(
        draw_weights(data, &influenced, Side::Prefix).len()
            - draw_weights(data, &plain, Side::Prefix).len(),
        1
    );
    assert_eq!(
        draw_weights(data, &influenced, Side::Suffix).len()
            - draw_weights(data, &plain, Side::Suffix).len(),
        10
    );
    assert_eq!(
        side_chance(data, &influenced, Side::Prefix, |m| m == totem()),
        Some(Fraction::new(1, 81).unwrap())
    );
}

// ---------------------------------------------------------------------------------------------
// The published chances.
// ---------------------------------------------------------------------------------------------

/// 1. The pool is the shield's: 73 prefixes over 64800 weight, 131 suffixes over 96475.
#[test]
fn the_pool_is_the_shields() {
    let data = data();
    let magic = shaper(Rarity::Magic);
    let prefix = draw_weights(data, &magic, Side::Prefix);
    let suffix = draw_weights(data, &magic, Side::Suffix);
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
    let magic = shaper(Rarity::Magic);

    // The totem modifier is the only desired prefix: 800 of the 64800 weight.
    let desired = side_chance(data, &magic, Side::Prefix, |m| {
        filter.tier_of(m) == Tier::Desired
    })
    .unwrap();
    assert_eq!(desired, Fraction::new(1, 81).unwrap());

    // The good prefixes are IncreasedLife10/9, IncreasedMana12 and both block tiers: 4000 weight.
    let good_prefix = side_chance(data, &magic, Side::Prefix, |m| {
        filter.tier_of(m) == Tier::Good
    })
    .unwrap();
    assert_eq!(good_prefix, Fraction::new(5, 81).unwrap());

    // Twenty good suffixes over the 96475 suffix weight: 14600 / 96475 = 584 / 3859.
    let good_suffix = side_chance(data, &magic, Side::Suffix, |m| {
        filter.tier_of(m) == Tier::Good
    })
    .unwrap();
    assert_eq!(good_suffix, Fraction::new(584, 3859).unwrap());
}

/// 3. One transmutation from a normal shield is 1/108 for the totem prefix.
///
/// Half the time the orb adds one modifier and half the time two. One modifier: the prefix is drawn
/// only when the side comes up prefix (½) and the totem modifier is 1/81 of that side's pool, so
/// ½·1/81 = 1/162. Two modifiers: the totem modifier arrives first on the prefix side (½·1/81) or
/// second, after a suffix (½·1/81), so 1/162 + 1/162 = 1/81. Together ½·1/162 + ½·1/81 = 1/108.
#[test]
fn one_transmute_is_one_in_one_hundred_and_eight() {
    let applied = apply("currency/transmutation", &shaper(Rarity::Normal))
        .expect("a normal shield is transmuted");
    let totem = totem();
    let chance = applied
        .outcomes
        .chance(|item| item.affixes(Side::Prefix).iter().any(|a| a.m == totem))
        .expect("a transmutation always produces something");
    assert_eq!(chance, Fraction::new(1, 108).unwrap());
}

/// 4. Every set is exact, and every orb's chances are a whole.
///
/// One item each orb accepts: a normal shield for the transmutation, a magic one for the
/// alteration, a magic one with a free side for augmentation and regal, a rare one with room for
/// the exalted orb, a rare one holding something for scouring and annulment, and a rare one whose
/// values a Divine still has room to widen.
#[test]
fn chances_are_whole_and_exact() {
    for (id, item) in accepted_items() {
        let applied = apply(id, &item).expect("an item the orb accepts");
        assert!(applied.outcomes.is_whole(), "{id} produced an inexact set");
        assert!(
            !applied.outcomes.parts().is_empty(),
            "{id} produced no outcome"
        );
    }
}

/// One item each orb accepts: a normal shield for the transmutation, a magic one for the
/// alteration, a magic one with a free side for augmentation and regal, a rare one with room for
/// the exalted orb, a rare one holding something for scouring and annulment, and a rare one whose
/// values a Divine still has room to widen.
fn accepted_items() -> Vec<(&'static str, Item)> {
    let data = data();
    let mut filled = shaper(Rarity::Magic);
    filled
        .affixes_mut(Side::Prefix)
        .push(Affix::at_range(data, totem()));

    let mut rare = shaper(Rarity::Rare);
    rare.affixes_mut(Side::Prefix)
        .push(Affix::at_range(data, totem()));

    let life = data.id_of("IncreasedLife10").unwrap();
    let mut pinned = shaper(Rarity::Rare);
    pinned
        .affixes_mut(Side::Prefix)
        .push(Affix::pinned(data, life, &[150]).expect("150 is in the mod's range"));

    vec![
        ("currency/transmutation", shaper(Rarity::Normal)),
        ("currency/alteration", filled.clone()),
        ("currency/augmentation", filled.clone()),
        ("currency/regal", filled.clone()),
        ("currency/exalted", rare.clone()),
        ("currency/scouring", rare.clone()),
        ("currency/annulment", rare),
        ("currency/divine", pinned),
    ]
}

/// Every use of an orb the item accepts states one orb of its own kind as the cost.
#[test]
fn every_use_states_its_cost() {
    for (id, item) in accepted_items() {
        let applied = apply(id, &item).expect("an item the orb accepts");
        assert_eq!(
            applied.cost,
            Cost::of(CurrencyId(id), 1.0),
            "{id} states another cost"
        );
    }
}

/// The rarity an orb refuses a stock for, one reason each.
const RARITY_REASONS: [(&str, &str); 8] = [
    (
        "currency/transmutation",
        "an Orb of Transmutation needs a normal item",
    ),
    (
        "currency/alteration",
        "an Orb of Alteration needs a magic item",
    ),
    (
        "currency/augmentation",
        "an Orb of Augmentation needs a magic item",
    ),
    ("currency/regal", "an Orb of Regal needs a magic item"),
    ("currency/exalted", "an Exalted Orb needs a rare item"),
    (
        "currency/scouring",
        "an Orb of Scouring needs a magic or rare item",
    ),
    (
        "currency/annulment",
        "an Orb of Annulment needs a magic or rare item",
    ),
    ("currency/divine", "a Divine Orb needs a magic or rare item"),
];

/// An orb's `works_on` and its rarity guard say the same thing: a shield of each rarity is refused
/// with the orb's rarity reason exactly when the rarity is not in `works_on`.
#[test]
fn works_on_and_apply_agree() {
    let data = data();
    let life = data.id_of("IncreasedLife10").unwrap();
    for (id, reason) in RARITY_REASONS {
        let orb = find(&ON_ITEMS, CurrencyId(id)).expect("a modelled orb");
        for rarity in [Rarity::Normal, Rarity::Magic, Rarity::Rare] {
            let mut held = shaper(rarity);
            if rarity != Rarity::Normal {
                held.affixes_mut(Side::Prefix)
                    .push(Affix::pinned(data, life, &[150]).expect("150 is in the mod's range"));
            }
            let accepted = orb.works_on().contains(&rarity);
            match orb.apply(data, &held) {
                Ok(_) => assert!(
                    accepted,
                    "{id} took a {} item it does not work on",
                    rarity.name()
                ),
                Err(Refusal(why)) => {
                    assert!(
                        !accepted,
                        "{id} refused a {} item it works on",
                        rarity.name()
                    );
                    assert_eq!(why, reason, "{id} on a {} item", rarity.name());
                }
            }
        }
    }
}

/// A cost is a property of the use: `Restart` produces the same outcomes either way and states a
/// different cost for a normal item than for anything else.
#[test]
fn a_cost_can_depend_on_the_input() {
    let normal = shaper(Rarity::Normal);
    let rare = shaper(Rarity::Rare);
    let from_normal = common::Restart
        .apply(data(), &normal)
        .expect("a normal item restarts");
    let from_rare = common::Restart
        .apply(data(), &rare)
        .expect("a rare item restarts");

    assert_eq!(
        from_normal.cost,
        Cost::of(CurrencyId::TRANSMUTATION, 1.0),
        "a normal item costs one transmutation"
    );
    assert_eq!(
        from_rare.cost,
        Cost::of(CurrencyId::SCOURING, 1.0).plus(&Cost::of(CurrencyId::TRANSMUTATION, 1.0)),
        "anything else costs a scouring and a transmutation"
    );
    assert_eq!(
        from_normal.outcomes, from_rare.outcomes,
        "the same restart produces the same items either way"
    );
}

/// The eight ids the crate models, and the only ones it models.
const ORBS: [(&str, &str); 8] = [
    ("currency/transmutation", "transmuted"),
    ("currency/alteration", "altered"),
    ("currency/augmentation", "augmented"),
    ("currency/regal", "regaled"),
    ("currency/exalted", "exalted"),
    ("currency/scouring", "scoured"),
    ("currency/annulment", "annulled"),
    ("currency/divine", "divined"),
];

#[test]
fn the_registry_is_exactly_the_eight_orbs() {
    let modelled: Vec<&'static str> = ON_ITEMS.iter().map(|orb| orb.id().0).collect();
    let expected: Vec<&'static str> = ORBS.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        modelled, expected,
        "the modelled orbs are exactly the eight"
    );
    for (id, _) in ORBS {
        let orb = find(&ON_ITEMS, CurrencyId(id)).unwrap_or_else(|| panic!("{id} is not modelled"));
        assert_eq!(
            orb.id(),
            CurrencyId(id),
            "the orb reports a different id than its key"
        );
    }
    assert!(
        find(&ON_ITEMS, CurrencyId("currency/chaos")).is_none(),
        "currency/chaos must not be modelled"
    );
}

/// 5 (v2). A corrupted or a mirrored item is locked: every one of the eight orbs refuses it, and
/// the reason names the orb's own verb.
///
/// The two flags are checked before the rarity, so each orb answers the same way for an item it
/// would otherwise accept — an item the game has locked may not be modified by any orb at all.
#[test]
fn a_corrupted_or_mirrored_item_is_locked() {
    for (id, verb) in ORBS {
        let mut corrupted = shaper(Rarity::Rare);
        corrupted.corrupted = true;
        assert_refused(
            apply(id, &corrupted),
            &format!("a corrupted item cannot be {verb}"),
        );

        let mut mirrored = shaper(Rarity::Rare);
        mirrored.mirrored = true;
        assert_refused(
            apply(id, &mirrored),
            &format!("a mirrored item cannot be {verb}"),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The orbs' rules: room, family exclusion, and what a Divine does.
// ---------------------------------------------------------------------------------------------

/// 5. Rarity and room.
///
/// No step puts a second prefix on a magic item, no step puts a fourth modifier on a rare side, and
/// an orb that needs room refuses when there is none.
#[test]
fn no_step_rolls_past_its_sides_room() {
    let data = data();
    let transmuted = apply("currency/transmutation", &shaper(Rarity::Normal))
        .expect("a normal shield is transmuted");
    for (item, _) in transmuted.outcomes.parts() {
        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.affixes(Side::Prefix).len() <= 1);
        assert!(item.affixes(Side::Suffix).len() <= 1);
    }

    let mut magic = shaper(Rarity::Magic);
    magic
        .affixes_mut(Side::Prefix)
        .push(Affix::at_range(data, totem()));
    let augmented =
        apply("currency/augmentation", &magic).expect("a magic shield with a free side");
    for (item, _) in augmented.outcomes.parts() {
        assert_eq!(item.rarity, Rarity::Magic);
        assert!(item.affixes(Side::Suffix).len() <= 1);
    }

    // A magic item with one prefix is regalled into a rare item that may take a second prefix.
    let mut magic = shaper(Rarity::Magic);
    magic
        .affixes_mut(Side::Prefix)
        .push(Affix::at_range(data, totem()));
    let regaled = apply("currency/regal", &magic).expect("a magic shield is regalled");
    for (item, _) in regaled.outcomes.parts() {
        assert_eq!(item.rarity, Rarity::Rare);
        assert!(item.affixes(Side::Prefix).len() <= 3);
        assert!(item.affixes(Side::Suffix).len() <= 3);
    }

    // A rare item with three prefixes and three suffixes has no room left.
    let mut full = shaper(Rarity::Rare);
    for side in Side::BOTH {
        while full.affixes(side).len() < 3 {
            let pool = draw_weights(data, &full, side);
            let (m, _) = *pool
                .first()
                .expect("a rare side with room has something to draw");
            full.affixes_mut(side).push(Affix::at_range(data, m));
        }
    }
    assert_eq!(full.affixes(Side::Prefix).len(), 3);
    assert_eq!(full.affixes(Side::Suffix).len(), 3);
    assert_refused(
        apply("currency/exalted", &full),
        "the item has no room for another modifier",
    );
}

/// 6. A modifier family cannot repeat: no orb puts a second life modifier beside the first.
#[test]
fn a_family_cannot_repeat_through_any_orb() {
    let data = data();
    let life = data.id_of("IncreasedLife10").unwrap();
    let lower_life = data.id_of("IncreasedLife9").unwrap();
    let mut rare = shaper(Rarity::Rare);
    rare.affixes_mut(Side::Prefix)
        .push(Affix::at_range(data, life));

    for id in ["currency/exalted", "currency/annulment"] {
        let applied = apply(id, &rare).expect("a rare shield with a free side");
        for (item, _) in applied.outcomes.parts() {
            let held: Vec<u16> = Side::BOTH
                .into_iter()
                .flat_map(|side| item.affixes(side).iter().map(|a| a.m))
                .collect();
            let repeated = held.iter().filter(|m| **m == lower_life).count();
            assert_eq!(repeated, 0, "{id} put a second life modifier on one item");
        }
    }
    // The rolled pool itself never offers the second tier while the first is held.
    assert!(
        !draw_weights(data, &rare, Side::Prefix)
            .iter()
            .any(|&(m, _)| m == lower_life)
    );
}

/// 7. The filter is a question, and the distribution answers it without hiding mass.
///
/// The chance that a transmutation's outcome holds the desired modifier is exactly `1/108`, and
/// every outcome's chance is counted once: the set is a whole.
#[test]
fn the_filter_keeps_the_mass_it_reports() {
    let wants_desired = ItemSetFilter::from_data(data());
    let applied = apply("currency/transmutation", &shaper(Rarity::Normal))
        .expect("a normal shield is transmuted");
    let chance = applied
        .outcomes
        .chance(|item| wants_desired.holds_desired(item))
        .expect("the step always produces something");
    assert_eq!(chance, Fraction::new(1, 108).unwrap());
    assert!(
        applied.outcomes.is_whole(),
        "every outcome's chance is counted once"
    );
}

/// 6 (v2). The Divine Orb widens values and touches nothing else.
///
/// On a rare item whose `IncreasedLife10` is pinned at 150 the one outcome holds that same
/// modifier at its full range `145-159`, at the whole chance. A second Divine has nothing left to
/// widen, so it refuses; a normal item is not a magic or rare item, so it refuses that too.
#[test]
fn the_divine_widens_values_and_nothing_else() {
    let data = data();
    let life = data.id_of("IncreasedLife10").unwrap();
    let mut rare = shaper(Rarity::Rare);
    rare.affixes_mut(Side::Prefix)
        .push(Affix::pinned(data, life, &[150]).expect("150 is in the mod's range"));

    let applied = apply("currency/divine", &rare).expect("a pinned value is widened");
    assert!(applied.outcomes.is_whole());
    assert_eq!(
        applied.outcomes.parts().len(),
        1,
        "a Divine is deterministic"
    );
    let widened = &applied.outcomes.parts()[0].0;
    assert_eq!(
        widened
            .affix(life)
            .expect("the modifier survived the orb")
            .rolls,
        vec![Roll::range(145, 159).unwrap()],
        "150 is widened to the whole range"
    );
    // Nothing but the values moved: same rarity, base, level, influence and implicit.
    assert_eq!(widened.rarity, rare.rarity);
    assert_eq!(widened.base, rare.base);
    assert_eq!(widened.item_level, rare.item_level);
    assert_eq!(widened.influence, rare.influence);
    assert_eq!(widened.implicits(), rare.implicits());

    assert_refused(
        apply("currency/divine", widened),
        "the item's values are already as wide as a Divine can make them",
    );
    assert_refused(
        apply("currency/divine", &shaper(Rarity::Normal)),
        "a Divine Orb needs a magic or rare item",
    );
}

// ---------------------------------------------------------------------------------------------
// The goal and the filters.
// ---------------------------------------------------------------------------------------------

/// An item built from named modifiers, in the order given, on the shield's base at [`LEVEL`].
fn item_of(
    influence: Option<Influence>,
    rarity: Rarity,
    prefixes: &[&str],
    suffixes: &[&str],
) -> Item {
    let data = data();
    let mut item = Item::of_base(data, base(), LEVEL, rarity);
    if let Some(influence) = influence {
        item = item.with_influence(influence);
    }
    for (side, ids) in [(Side::Prefix, prefixes), (Side::Suffix, suffixes)] {
        for id in ids {
            let m = data
                .id_of(id)
                .unwrap_or_else(|| panic!("{id} is not in the shield's pool"));
            item.affixes_mut(side).push(Affix::at_range(data, m));
        }
    }
    item
}

/// 8. The target: any rare shield that holds the totem prefix. Everything else is a bonus.
#[test]
fn the_target_is_the_totem_prefix_and_nothing_else() {
    let data = data();
    let filter = shield_goal();
    let target = Target::from_data(data);
    assert_eq!(target.prefixes, [Tier::Desired]);
    assert!(target.suffixes.is_empty());

    // The totem prefix and nothing worth calling a second modifier: this is the item the craft
    // is for, and the rest of the item is free.
    let bare = item_of(
        Some(Influence::Shaper),
        Rarity::Rare,
        &["MaximumTotemUber1", "AttackerTakesDamage1"],
        &["LifeRegeneration1"],
    );
    assert!(
        target.reached(&filter, &bare),
        "the goal is the totem prefix"
    );

    // Good modifiers beside it are the bonus the alt and aug are for, and they change nothing.
    let good_beside_it = item_of(
        Some(Influence::Shaper),
        Rarity::Rare,
        &["MaximumTotemUber1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "FireResist8", "ColdResist8"],
    );
    assert!(target.reached(&filter, &good_beside_it));

    // Without the totem prefix nothing else can make up for it.
    let no_totem = item_of(
        Some(Influence::Shaper),
        Rarity::Rare,
        &["AttackerTakesDamage1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "FireResist8", "ColdResist8"],
    );
    assert!(!target.reached(&filter, &no_totem));

    // A magic item never reaches it, whatever it holds.
    let magic = item_of(
        Some(Influence::Shaper),
        Rarity::Magic,
        &["MaximumTotemUber1"],
        &["AllResistances6"],
    );
    assert!(!target.reached(&filter, &magic));
}
