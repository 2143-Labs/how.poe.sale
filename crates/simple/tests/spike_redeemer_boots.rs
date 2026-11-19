//! A spike on a second base: Redeemer Two-Toned Boots, item level 86 (catalogue row 9). Alteration
//! until the item holds Onslaught, a regal, then exalts until both sides are full; the target is
//! three prefixes of anything and the suffixes Onslaught, good, good. A full item that misses is
//! scoured and started again (policy A; the research's policy B, an annulment, is not modelled).
//!
//! Every expected number comes from `ai_research/spike-boots-annul/model.py`, an independent Python
//! model over the raw export: the abstract item in exact fractions. A disagreement here is a
//! finding about one of the two, never a number to re-pin.

use simple::cost::{CurrencyId, Spend};
use simple::craft::{Craft, Report, Step, Wire};
use simple::currency::ON_NODES;
use simple::data::Data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{AltToMod, ExaltFull, Kit, run};
use simple::node::Node;
use simple::pool::{draw_weights, side_chance};
use simple::target::Target;
use simple::weights::Weights;

const BOOTS: &str = include_str!("../data/redeemer-boots.json");

const LEVEL: u8 = 86;

/// 3.30 prices (Covens of Umbra, day 4), in chaos.
const PRICES: &str = r#"{ "unit": "chaos", "weights": {
    "currency/transmutation": 0.021, "currency/alteration": 0.091,
    "currency/augmentation": 0.067, "currency/regal": 0.217, "currency/exalted": 0.83,
    "currency/scouring": 0.625, "currency/annulment": 8.6 } }"#;

/// The orbs the craft can spend, in the order the pins list them.
const ORBS: [CurrencyId; 7] = [
    CurrencyId::TRANSMUTATION,
    CurrencyId::ALTERATION,
    CurrencyId::AUGMENTATION,
    CurrencyId::REGAL,
    CurrencyId::EXALTED,
    CurrencyId::SCOURING,
    CurrencyId::ANNULMENT,
];

fn boots() -> Data {
    Data::from_json(BOOTS).expect("the Redeemer boots data loads")
}

fn start(data: &Data) -> Item {
    Item::of_base(data, data.base_of("Two-Toned Boots"), LEVEL, Rarity::Normal)
        .with_influence(Influence::Redeemer)
}

fn boots_law(data: &Data) -> Law {
    Law::new(data, &ItemSetFilter::from_data(data), &start(data)).expect("the boots have a law")
}

fn prices() -> Weights {
    Weights::from_json(PRICES).expect("the prices load")
}

/// Policy A: a full item that misses goes back to `alt_to_mod`, which scours it.
fn restart() -> Craft {
    Craft {
        name: "boots_restart",
        start: Rarity::Normal,
        steps: vec![
            Step {
                method: Box::new(AltToMod { augment: false }),
                wires: vec![("magic", Wire::Next)],
            },
            Step {
                method: Box::new(ExaltFull),
                wires: vec![
                    ("done", Wire::Done),
                    ("miss", Wire::To(0)),
                    ("scrap", Wire::To(0)),
                ],
            },
        ],
    }
}

fn report_of(craft: &Craft) -> Report {
    let data = boots();
    let law = boots_law(&data);
    let target = Target::from_data(&data);
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    craft
        .report(&kit, &prices())
        .unwrap_or_else(|why| panic!("{why}"))
}

/// `got` within `1e-7` of `want`, relative.
fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-7 * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

/// Every orb's mean count and spread, in the order of [`ORBS`].
fn pinned(spend: &Spend, mean: [f64; 7], sd: [f64; 7], what: &str) {
    for ((id, mean), sd) in ORBS.into_iter().zip(mean).zip(sd) {
        close(
            spend.mean.get(id),
            mean,
            &format!("{what}: mean {}", id.name()),
        );
        close(spend.sd(id), sd, &format!("{what}: sd {}", id.name()));
    }
}

/// Each method solved on its own and nested gives what one chain of single orb uses gives.
fn nested_is_flat(report: &Report) {
    let flat = report.flat.spend();
    for a in ORBS {
        close(
            report.total.mean.get(a),
            flat.mean.get(a),
            &format!("flat mean {}", a.name()),
        );
        for b in ORBS {
            close(
                report.total.covariance(a, b),
                flat.covariance(a, b),
                &format!("flat covariance {} {}", a.name(), b.name()),
            );
        }
    }
}

/// The chance a law gives a grade; zero when the grade cannot come next.
fn next(law: &Law, side: Side, held: &[Tier], grade: Tier) -> f64 {
    law.next(side, held)
        .iter()
        .find(|(tier, _)| *tier == grade)
        .map(|&(_, chance)| chance)
        .unwrap_or(0.0)
}

/// 45 prefixes and 82 suffixes (the research counted 46 prefixes: one was the Battle Royale copy
/// `MovementVelocity2Royale`). Onslaught is 1 000 of the 59 600 suffix weight and the seven good
/// families 37 000; no prefix is graded.
#[test]
fn the_pool_and_the_grades_are_the_exports() {
    let data = boots();
    let filter = ItemSetFilter::from_data(&data);
    let rare = start(&data).with_rarity(Rarity::Rare);
    for (side, count, weight) in [(Side::Prefix, 45, 38_750), (Side::Suffix, 82, 59_600)] {
        let pool = draw_weights(&data, &rare, side);
        assert_eq!(pool.len(), count, "{side:?}");
        assert_eq!(
            pool.iter().map(|&(_, w)| w).sum::<u128>(),
            weight,
            "{side:?}"
        );
    }
    let share = |tier: Tier| {
        side_chance(&data, &rare, Side::Suffix, |m| filter.tier_of(m) == tier)
            .expect("the suffixes have a pool")
    };
    assert_eq!(share(Tier::Desired), Fraction::new(5, 298).unwrap());
    assert_eq!(share(Tier::Good), Fraction::new(185, 298).unwrap());
    assert_eq!(
        side_chance(&data, &rare, Side::Prefix, |m| filter.tier_of(m)
            != Tier::Any),
        Some(Fraction::ZERO)
    );
}

#[test]
fn the_suffix_law_is_this() {
    use Tier::{Any, Desired, Good};
    let data = boots();
    let law = boots_law(&data);
    let pins: [(&[Tier], Tier, f64); 7] = [
        (&[], Desired, 0.0167785234899329),
        (&[], Good, 0.620805369127517),
        (&[Desired], Good, 0.631399317406143),
        (&[Desired, Good], Good, 0.584637909220907),
        (&[Desired, Any], Good, 0.695330780071076),
        (&[Good], Desired, 0.0188663798669124),
        (&[Good, Any], Desired, 0.0210470398869936),
    ];
    for (held, grade, want) in pins {
        let got = next(&law, Side::Suffix, held, grade);
        assert!(
            (got - want).abs() <= 1e-12,
            "{held:?} then {grade:?}: {got} is not {want}"
        );
    }
    // Every prefix is `any`, so a prefix side is only a count.
    assert_eq!(law.next(Side::Prefix, &[Any, Any]).len(), 1);
    assert!((next(&law, Side::Prefix, &[Any, Any], Any) - 1.0).abs() <= 1e-12);
}

/// One roll holds Onslaught `3/4 · 5/298 = 15/1192` of the time: a third of the hits hold it alone,
/// two thirds beside a prefix. The alterations are the failed rolls after the transmutation.
#[test]
fn alteration_until_onslaught_costs_this() {
    let data = boots();
    let law = boots_law(&data);
    let target = Target::from_data(&data);
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let solved = run(
        &AltToMod { augment: false },
        &kit,
        &[(Node::fresh(Rarity::Normal), 1.0)],
    )
    .unwrap();
    assert_eq!(solved.exits.len(), 2);
    for (node, chance) in solved.exits.iter().zip(solved.exit_chances()) {
        assert_eq!(
            node.side(Side::Suffix),
            [Tier::Desired].as_slice(),
            "{}",
            node.label()
        );
        let want = if node.side(Side::Prefix).is_empty() {
            1.0 / 3.0
        } else {
            2.0 / 3.0
        };
        close(chance, want, &node.label());
    }
    let spend = solved.spend();
    close(
        spend.mean.get(CurrencyId::TRANSMUTATION),
        1.0,
        "transmutation",
    );
    close(
        spend.mean.get(CurrencyId::ALTERATION),
        1177.0 / 15.0,
        "alteration",
    );
    close(
        spend.sd(CurrencyId::ALTERATION),
        78.9650837044098,
        "alteration sd",
    );
    assert_eq!(spend.mean.get(CurrencyId::AUGMENTATION), 0.0);
}

/// The research's figures, to its two decimals: 2.71 transmutation, 212.57 alteration, 2.71
/// regal, 9.03 exalt, 1.71 scour, 28.55 chaos.
#[test]
fn policy_a_scours_a_miss_and_costs_this() {
    let report = report_of(&restart());
    let names: Vec<&str> = report.rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["alt_to_mod", "exalt_full"]);
    close(
        report.rows[1].chance,
        0.369139976811835,
        "a full item is the target",
    );
    pinned(
        &report.total,
        [
            2.70899946583065,
            212.566158085512,
            0.0,
            2.70899946583065,
            9.0299982194355,
            1.70899946583065,
            0.0,
        ],
        [
            2.15166880351975,
            213.065571412465,
            0.0,
            2.15166880351975,
            7.21407469184503,
            2.15166880351975,
            0.0,
        ],
        "restart",
    );
    let valued = prices().value(&report.total);
    close(valued.value, 28.5512854469249, "restart chaos");
    close(valued.sd, 26.0253700963052, "restart chaos sd");
    nested_is_flat(&report);
}
