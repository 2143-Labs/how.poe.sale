//! Classes: a run of several filters, checked against the one-filter law and against the items.
//!
//! The Redeemer boots, with two filters: Onslaught alone (`a`), and Onslaught or 30% movement speed
//! (`b`). Their classes are Onslaught, movement speed, and the rest.

use std::collections::HashMap;

use simple::classes::{Class, Classes, Grade};
use simple::currency::{Alteration, Currency, ON_CLASSED, Transmutation};
use simple::data::Data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{AltToMod, Kit, Through, run};
use simple::node::Node;
use simple::target::Target;

const LEVEL: u8 = 86;

fn data() -> &'static Data {
    simple::bases::data("boots-redeemer").expect("the Redeemer boots load")
}

fn ids(names: &[&str]) -> Vec<u16> {
    names
        .iter()
        .map(|name| data().id_of(name).expect("a boots modifier"))
        .collect()
}

/// Onslaught alone.
fn onslaught() -> ItemSetFilter {
    let desired = ids(&["OnslaughtOnKillInfluence1", "OnslaughtOnKillInfluence2_"]);
    ItemSetFilter::new(data(), &desired, &[])
}

/// Onslaught or 30% movement speed.
fn speed() -> ItemSetFilter {
    let desired = ids(&[
        "OnslaughtOnKillInfluence1",
        "OnslaughtOnKillInfluence2_",
        "MovementVelocity5",
        "MovementVelocity6",
    ]);
    ItemSetFilter::new(data(), &desired, &[])
}

fn start() -> Item {
    Item::of_base(data(), 0, LEVEL, Rarity::Normal).with_influence(Influence::Redeemer)
}

fn fraction(chance: Fraction) -> f64 {
    chance.num() as f64 / chance.den() as f64
}

/// The chance of every history up to `depth` draws on a side, by the law's next-grade chances.
fn histories<G: Grade>(law: &Law<G>, side: Side, depth: usize) -> HashMap<Vec<G>, f64> {
    let mut all: HashMap<Vec<G>, f64> = HashMap::new();
    let mut level: Vec<(Vec<G>, f64)> = vec![(Vec::new(), 1.0)];
    for _ in 0..depth {
        let mut deeper = Vec::new();
        for (history, chance) in &level {
            for &(grade, next) in law.next(side, history) {
                let mut longer = history.clone();
                longer.push(grade);
                *all.entry(longer.clone()).or_default() += chance * next;
                deeper.push((longer, chance * next));
            }
        }
        level = deeper;
    }
    all
}

#[test]
fn one_filter_makes_its_three_tiers_in_law_order() {
    let classes = Classes::new(data(), &[&onslaught()]).expect("the classes");
    assert_eq!(classes.all(), [Class(0), Class(1)]);
    let lens = classes.lens(0);
    assert_eq!(lens.tier(Class(0)), Tier::Desired);
    assert_eq!(lens.tier(Class(1)), Tier::Any);
    assert_eq!(classes.unlisted(), Class(1));
}

/// Summed over every history of classes that reads as one history of tiers, the law over classes
/// gives that history the chance the one-filter law gives it, for each filter.
#[test]
fn the_law_over_classes_reads_as_each_filters_law() {
    let (a, b) = (onslaught(), speed());
    let classes = Classes::new(data(), &[&a, &b]).expect("the classes");
    assert_eq!(
        classes.all().len(),
        3,
        "Onslaught, movement speed, and the rest"
    );
    let law = Law::classed(data(), &classes, &start()).expect("the law over classes");
    for (at, filter) in [&a, &b].into_iter().enumerate() {
        let lens = classes.lens(at);
        let alone = Law::new(data(), filter, &start()).expect("the filter's own law");
        for side in Side::BOTH {
            let want = histories(&alone, side, 3);
            let mut got: HashMap<Vec<Tier>, f64> = HashMap::new();
            for (history, chance) in histories(&law, side, 3) {
                let seen = history.iter().map(|&class| lens.tier(class)).collect();
                *got.entry(seen).or_default() += chance;
            }
            assert_eq!(got.len(), want.len(), "filter {at}, {side:?}");
            for (history, chance) in &want {
                let found = got.get(history).copied().unwrap_or(0.0);
                assert!(
                    (found - chance).abs() < 1e-12,
                    "filter {at}, {side:?} {history:?}: got {found}, want {chance}"
                );
            }
        }
    }
}

/// Alteration until Onslaught, read by filter `a`, hands on magic items that filter `b` grades: the
/// run over classes gives each of `b`'s grades the chance the items do.
#[test]
fn a_block_hands_on_what_another_filter_sees_in_the_items() {
    let (a, b) = (onslaught(), speed());
    let classes = Classes::new(data(), &[&a, &b]).expect("the classes");
    let law = Law::classed(data(), &classes, &start()).expect("the law over classes");
    let alt_until = Through {
        method: Box::new(AltToMod { augment: false }),
        lens: classes.lens(0),
        target: Target {
            prefixes: Vec::new(),
            suffixes: vec![Tier::Desired],
            caps: data().base_at(0).caps,
        },
    };
    let kit = Kit {
        law: &law,
        target: &alt_until.target,
        tools: &ON_CLASSED,
    };
    let solved = run(
        &alt_until,
        &kit,
        &[(Node::<Class>::fresh(Rarity::Normal), 1.0)],
    )
    .expect("alteration until Onslaught solves");
    let seen = classes.lens(1);
    let mut got: HashMap<String, f64> = HashMap::new();
    for (node, chance) in solved.exits.iter().zip(solved.exit_chances()) {
        *got.entry(seen.view(node).label()).or_default() += chance;
    }

    // The items: the transmutation's outcome when it holds Onslaught, else the first alteration
    // that does. An alteration rerolls the item, so its outcomes do not depend on what it held.
    let transmuted = Transmutation
        .apply(data(), &start())
        .expect("a normal item transmutes")
        .outcomes;
    let missed = transmuted
        .parts()
        .iter()
        .find(|(item, _)| !a.holds_desired(item))
        .map(|(item, _)| item.clone())
        .expect("a transmutation can miss");
    let altered = Alteration
        .apply(data(), &missed)
        .expect("a magic item alters")
        .outcomes;
    let hit = |set: &[(Item, Fraction)]| -> f64 {
        set.iter()
            .filter(|(item, _)| a.holds_desired(item))
            .map(|(_, chance)| fraction(*chance))
            .sum()
    };
    let (first, again) = (hit(transmuted.parts()), hit(altered.parts()));
    let mut want: HashMap<String, f64> = HashMap::new();
    for (set, weight) in [
        (transmuted.parts(), 1.0),
        (altered.parts(), (1.0 - first) / again),
    ] {
        for (item, chance) in set {
            if a.holds_desired(item) {
                *want.entry(Node::of(&b, item).label()).or_default() += weight * fraction(*chance);
            }
        }
    }

    assert!(want.len() > 1, "filter b tells the exits apart: {want:?}");
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
    for (label, chance) in &want {
        let found = got.get(label).copied().unwrap_or(0.0);
        assert!(
            (found - chance).abs() < 1e-12,
            "{label}: got {found}, want {chance}"
        );
    }
}
