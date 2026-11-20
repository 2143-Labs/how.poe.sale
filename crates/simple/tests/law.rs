//! The abstract item against the concrete one.
//!
//! A [`Node`] forgets which modifiers an item holds, and a [`Law`] says how often the next one is
//! desired, good or anything else. These checks are what makes that safe: one step from a cleared
//! item the abstract chances are the concrete orb's exact fractions, and deeper in, the law's
//! chance is the concrete chance averaged over every item the node could be.

use std::collections::HashMap;

use simple::currency::{Alteration, Applied, Currency, ON_ITEMS, ON_NODES};
use simple::data::{Data, data};
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Affix, Influence, Item, Rarity, Side};
use simple::law::{Law, LawError};
use simple::node::{Node, grade};
use simple::pool::{draw_weights, side_chance};
use simple::stock::{Refusal, Stock};

const LEVEL: u8 = 87;

const GRADES: [Tier; 3] = [Tier::Desired, Tier::Good, Tier::Any];

fn shaper(rarity: Rarity) -> Item {
    let data = data();
    Item::of_base(data, data.base_of("Supreme Spiked Shield"), LEVEL, rarity)
        .with_influence(Influence::Shaper)
}

fn filter() -> ItemSetFilter {
    ItemSetFilter::from_data(data())
}

fn law() -> Law {
    Law::new(data(), &filter(), &shaper(Rarity::Normal)).expect("the shield has a law")
}

fn node(rarity: Rarity, prefixes: &[Tier], suffixes: &[Tier]) -> Node {
    let mut node = Node::fresh(rarity);
    for &tier in prefixes {
        node = node.with(Side::Prefix, tier);
    }
    for &tier in suffixes {
        node = node.with(Side::Suffix, tier);
    }
    node
}

fn holding(rarity: Rarity, prefixes: &[&str], suffixes: &[&str]) -> Item {
    let data = data();
    let mut item = shaper(rarity);
    for (side, ids) in [(Side::Prefix, prefixes), (Side::Suffix, suffixes)] {
        for id in ids {
            let m = data
                .id_of(id)
                .unwrap_or_else(|| panic!("{id} is not in the pool"));
            item.affixes_mut(side).push(Affix::at_range(data, m));
        }
    }
    item
}

fn float(chance: Fraction) -> f64 {
    chance.num() as f64 / chance.den() as f64
}

/// The chance a law gives a grade; zero when the grade cannot come next.
fn next(law: &Law, side: Side, held: &[Tier], grade: Tier) -> f64 {
    law.next(side, held)
        .iter()
        .find(|(tier, _)| *tier == grade)
        .map(|&(_, chance)| chance)
        .unwrap_or(0.0)
}

fn refusal<S: Stock>(result: Result<Applied<S>, Refusal>) -> Option<&'static str> {
    result.err().map(|Refusal(why)| why)
}

/// One alteration has eleven outcomes once graded, and their chances are these fractions. The
/// same orb used on a fresh magic node gives the same eleven.
#[test]
fn one_alteration_is_eleven_nodes() {
    use Tier::{Any, Desired, Good};
    let expected: [(&[Tier], &[Tier], u128, u128); 11] = [
        (&[Any], &[Any], 81875, 208386),
        (&[Any], &[], 25, 108),
        (&[], &[Any], 3275, 15436),
        (&[Any], &[Good], 7300, 104193),
        (&[], &[Good], 146, 3859),
        (&[Good], &[Any], 16375, 625158),
        (&[Good], &[], 5, 324),
        (&[Desired], &[Any], 3275, 625158),
        (&[Good], &[Good], 1460, 312579),
        (&[Desired], &[], 1, 324),
        (&[Desired], &[Good], 292, 312579),
    ];

    let on_item = Alteration
        .apply(data(), &shaper(Rarity::Magic))
        .expect("a magic shield can be altered");
    let graded = grade(&filter(), &on_item.outcomes).expect("the graded chances fit");
    assert!(graded.is_whole());
    assert_eq!(graded.parts().len(), expected.len());

    let on_node = Alteration
        .apply(&law(), &Node::fresh(Rarity::Magic))
        .expect("a magic node can be altered");
    assert!(on_node.outcomes.is_whole());
    assert_eq!(on_node.outcomes.parts().len(), expected.len());

    for (prefixes, suffixes, num, den) in expected {
        let want = node(Rarity::Magic, prefixes, suffixes);
        let exact = Fraction::new(num, den).unwrap();
        assert_eq!(
            graded.chance(|got| *got == want),
            Some(exact),
            "{} on an item",
            want.label()
        );
        let abstracted = on_node.outcomes.chance(|got| *got == want).unwrap();
        assert!(
            (abstracted - float(exact)).abs() < 1e-14,
            "{} on a node: {abstracted} is not {exact}",
            want.label()
        );
    }
}

/// Deeper than one step, the law's chance for a history is the concrete chance averaged over
/// every run of modifiers that history could be, each at the chance it is drawn with.
#[test]
fn the_law_is_the_concrete_chance_averaged_over_the_history() {
    let data = data();
    let filter = filter();
    let law = law();
    let rare = shaper(Rarity::Rare);

    for side in Side::BOTH {
        // history -> (the chance of drawing it, the chance-weighted sum of each next grade)
        let mut seen: HashMap<Vec<Tier>, (f64, [f64; 3])> = HashMap::new();
        let mut record = |item: &Item, history: Vec<Tier>, chance: f64| {
            let entry = seen.entry(history).or_insert((0.0, [0.0; 3]));
            entry.0 += chance;
            for (sum, grade) in entry.1.iter_mut().zip(GRADES) {
                let concrete = side_chance(data, item, side, |m| filter.tier_of(m) == grade)
                    .expect("a side with room draws");
                *sum += chance * float(concrete);
            }
        };

        let first = draw_weights(data, &rare, side);
        let first_total: u128 = first.iter().map(|&(_, weight)| weight).sum();
        for &(m1, w1) in &first {
            let mut one = rare.clone();
            one.affixes_mut(side).push(Affix::at_range(data, m1));
            let chance1 = w1 as f64 / first_total as f64;
            record(&one, vec![filter.tier_of(m1)], chance1);

            let second = draw_weights(data, &one, side);
            let second_total: u128 = second.iter().map(|&(_, weight)| weight).sum();
            for &(m2, w2) in &second {
                let mut two = one.clone();
                two.affixes_mut(side).push(Affix::at_range(data, m2));
                record(
                    &two,
                    vec![filter.tier_of(m1), filter.tier_of(m2)],
                    chance1 * w2 as f64 / second_total as f64,
                );
            }
        }

        assert!(seen.len() >= 6, "{side:?}: histories of both lengths");
        for (history, (chance, sums)) in &seen {
            for (sum, grade) in sums.iter().zip(GRADES) {
                let averaged = sum / chance;
                let stated = next(&law, side, history, grade);
                assert!(
                    (averaged - stated).abs() < 1e-13,
                    "{side:?} after {history:?}, next {grade:?}: the law says {stated}, the items \
                     average {averaged}"
                );
            }
        }
    }
}

/// What the suffixes hold changes nothing about the next prefix.
#[test]
fn sides_do_not_interact() {
    let data = data();
    let filter = filter();
    let alone = holding(Rarity::Rare, &["IncreasedLife10"], &[]);
    let beside = holding(
        Rarity::Rare,
        &["IncreasedLife10"],
        &["AllResistances6", "StunRecovery1"],
    );
    for grade in GRADES {
        assert_eq!(
            side_chance(data, &alone, Side::Prefix, |m| filter.tier_of(m) == grade),
            side_chance(data, &beside, Side::Prefix, |m| filter.tier_of(m) == grade),
            "{grade:?}"
        );
    }
}

/// The numbers, pinned. Order matters: a good prefix after `[good, any]` is not as likely as
/// after `[any, good]`.
#[test]
fn the_law_states_these_chances() {
    use Tier::{Any, Desired, Good};
    let law = law();
    let pinned: [(Side, &[Tier], Tier, f64); 9] = [
        (Side::Prefix, &[], Desired, 1.0 / 81.0),
        (Side::Prefix, &[], Good, 5.0 / 81.0),
        (Side::Prefix, &[Desired], Good, 1.0 / 16.0),
        (Side::Prefix, &[Good], Desired, 0.014485465766417),
        (Side::Prefix, &[Good], Good, 0.045026816326666),
        (Side::Prefix, &[Good, Any], Good, 0.043869385646164),
        (Side::Prefix, &[Any, Good], Good, 0.043532627248738),
        (Side::Suffix, &[], Good, 584.0 / 3859.0),
        (Side::Suffix, &[Good], Good, 0.142751308683577),
    ];
    for (side, held, grade, want) in pinned {
        let got = next(&law, side, held, grade);
        assert!(
            (got - want).abs() < 1e-12,
            "{side:?} after {held:?}, next {grade:?}: {got} is not {want}"
        );
    }
    // There is one desired prefix, so it cannot come twice, and no desired suffix at all.
    assert_eq!(next(&law, Side::Prefix, &[Desired], Desired), 0.0);
    assert_eq!(next(&law, Side::Suffix, &[], Desired), 0.0);
    assert!(law.next(Side::Prefix, &[Any, Any, Any]).is_empty());
}

/// The six orbs that work on a node give whole outcomes there, and refuse a node for the same
/// reason they refuse the item it stands for.
#[test]
fn the_orbs_treat_a_node_as_they_treat_its_item() {
    let data = data();
    let filter = filter();
    let law = law();
    let full = holding(
        Rarity::Rare,
        &["MaximumTotemUber1", "IncreasedLife10", "IncreasedMana12"],
        &["AllResistances6", "LifeRegeneration1", "StunRecovery1"],
    );
    let items = [
        shaper(Rarity::Normal),
        shaper(Rarity::Magic),
        holding(Rarity::Magic, &["IncreasedLife10"], &[]),
        holding(Rarity::Magic, &["IncreasedLife10"], &["AllResistances6"]),
        shaper(Rarity::Rare),
        holding(Rarity::Rare, &["IncreasedLife10"], &[]),
        full,
    ];

    let mut taken = 0;
    let mut refused = 0;
    for (on_node, on_item) in ON_NODES.iter().zip(ON_ITEMS) {
        assert_eq!(on_node.id(), on_item.id());
        for item in &items {
            let node = Node::of(&filter, item);
            let abstracted = on_node.apply(&law, &node);
            if let Ok(applied) = &abstracted {
                assert!(
                    applied.outcomes.is_whole(),
                    "{} on {}",
                    on_node.id().0,
                    node.label()
                );
                taken += 1;
            } else {
                refused += 1;
            }
            assert_eq!(
                refusal(abstracted),
                refusal(on_item.apply(data, item)),
                "{} on {}",
                on_node.id().0,
                node.label()
            );
        }
    }
    assert!(
        taken >= 10 && refused >= 20,
        "{taken} taken, {refused} refused"
    );
}

/// A law starts from a cleared item: a held modifier is a fact, and the law is an average.
#[test]
fn a_law_needs_a_cleared_item() {
    let data: &Data = data();
    let filter = filter();
    let held = holding(Rarity::Magic, &["IncreasedLife10"], &[]);
    assert_eq!(
        Law::new(data, &filter, &held).err(),
        Some(LawError::NotCleared)
    );
    let mut corrupted = shaper(Rarity::Normal);
    corrupted.corrupted = true;
    assert_eq!(
        Law::new(data, &filter, &corrupted).err(),
        Some(LawError::Locked)
    );
}
