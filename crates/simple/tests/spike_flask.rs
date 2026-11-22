//! The magic flask: alt-pair on a Quicksilver Flask (spike/flask).
//!
//! A Quicksilver Flask has no rare form, so the target is a magic item holding the desired
//! modifier on both sides. Expected numbers come from `ai_research/spike-flask/model.py`, an exact
//! `Fraction` model of the same rules; they are not read back from the solver.

use simple::cost::CurrencyId;
use simple::currency::{ON_ITEMS, ON_NODES, find};
use simple::data::Data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Item, Rarity, Side};
use simple::law::Law;
use simple::method::{AltPair, Kit, run};
use simple::node::Node;
use simple::target::Target;
use simple::weights::Weights;

/// The Quicksilver Flask, item level 86, generated from the 3.29 export.
const FLASK: &str = include_str!("../data/flask.json");

/// Covens day-4 prices, in chaos, from the research's price table.
const PRICES: &str = r#"{
  "unit": "chaos",
  "weights": {
    "currency/transmutation": 0.021,
    "currency/alteration": 0.091,
    "currency/augmentation": 0.067
  }
}"#;

/// `got` within `1e-7` of `want`, relative.
fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-7 * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

#[test]
fn one_transmutation_gives_the_pair_exactly_five_in_1577() {
    let data = Data::from_json(FLASK).expect("the flask data loads");
    let start = Item::of_base(&data, data.base_of("Quicksilver Flask"), 86, Rarity::Normal);
    let filter = ItemSetFilter::from_data(&data);
    let orb = find(&ON_ITEMS, CurrencyId::TRANSMUTATION).expect("a modelled orb");
    let applied = orb.apply(&data, &start).expect("a normal item transmutes");

    assert!(applied.outcomes.is_whole());
    let holds = |item: &Item, side: Side| {
        item.affixes(side)
            .iter()
            .any(|affix| filter.tier_of(affix.m) == Tier::Desired)
    };
    let both = applied
        .outcomes
        .chance(|item| holds(item, Side::Prefix) && holds(item, Side::Suffix))
        .expect("the chances fit");

    // The prefix pool is 3000/22800 desired, the suffix 3000/62250; the two-mod draw is half of
    // the two adds, so the pair is (1/2)(5/38)(4/83) = 5/1577.
    assert_eq!(both, Fraction::new(5, 1577).unwrap());
}

#[test]
fn alt_pair_costs_this() {
    let data = Data::from_json(FLASK).expect("the flask data loads");
    let start = Item::of_base(&data, data.base_of("Quicksilver Flask"), 86, Rarity::Normal);
    let filter = ItemSetFilter::from_data(&data);
    let law = Law::new(&data, &filter, &start).expect("the flask has a law");
    let target = Target::from_data(&data);
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let solved = run(&AltPair, &kit, &[(Node::fresh(Rarity::Normal), 1.0)])
        .expect("alt-pair solves on the flask");

    // The normal item and seven magic states: four with one side filled (desired or other, on
    // either side) and three with both filled and not the pair. The data grades nothing good, so
    // a non-desired modifier is `any` and there are two grades per side, not three.
    assert_eq!(solved.inside.len(), 8);
    assert_eq!(solved.exits.len(), 1);
    for node in &solved.inside {
        assert_ne!(node.rarity, Rarity::Rare, "{}", node.label());
    }
    let exit = &solved.exits[0];
    assert_eq!(exit.rarity, Rarity::Magic);
    for side in Side::BOTH {
        assert_eq!(exit.side(side).to_vec(), vec![Tier::Desired]);
    }
    close(solved.exit_chances().iter().sum(), 1.0, "the exits");

    // The pair costs 156.7 alterations and 7.0875 augmentations after one transmutation.
    let spend = solved.spend();
    close(
        spend.mean.get(CurrencyId::TRANSMUTATION),
        1.0,
        "transmutations",
    );
    close(spend.mean.get(CurrencyId::ALTERATION), 156.7, "alterations");
    close(
        spend.mean.get(CurrencyId::AUGMENTATION),
        7.0875,
        "augmentations",
    );
    for id in [
        CurrencyId::REGAL,
        CurrencyId::EXALTED,
        CurrencyId::SCOURING,
        CurrencyId::ANNULMENT,
        CurrencyId::DIVINE,
    ] {
        assert_eq!(spend.mean.get(id), 0.0, "{}", id.name());
    }

    let weights = Weights::from_json(PRICES).expect("the prices parse");
    let valued = weights.value(&spend);
    close(valued.value, 14.7555625, "the chaos cost");
    assert_eq!(valued.unweighted.total(), 0.0);
}
