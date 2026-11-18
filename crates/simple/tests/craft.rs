//! The shield's craft, costed step by step.
//!
//! The expected numbers come from an independent implementation of the same rules (Python over
//! `shield.json`), so a disagreement here is a finding about one of the two, never a number to
//! re-pin.

use simple::cost::{Cost, CurrencyId, Spend};
use simple::craft::shield::shield;
use simple::craft::{Craft, CraftError, Report, Step, Wire};
use simple::currency::ON_NODES;
use simple::data::data;
use simple::filter::{ItemSetFilter, Tier};
use simple::item::{Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{AltAugRegal, ExaltTo, ExitSpec, Kit, Method, Move};
use simple::node::Node;
use simple::target::Target;
use simple::weights::weights;

/// The orbs the craft spends, in the order the expected rows list them.
const ORBS: [CurrencyId; 6] = [
    CurrencyId::TRANSMUTATION,
    CurrencyId::ALTERATION,
    CurrencyId::AUGMENTATION,
    CurrencyId::REGAL,
    CurrencyId::EXALTED,
    CurrencyId::SCOURING,
];

fn law() -> Law {
    let data = data();
    let start = Item::of_base(
        data,
        data.base_of("Supreme Spiked Shield"),
        87,
        Rarity::Normal,
    )
    .with_influence(Influence::Shaper);
    Law::new(data, &ItemSetFilter::from_data(data), &start).expect("the shield has a law")
}

fn report() -> Report {
    let law = law();
    let target = Target::from_data(data());
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    shield()
        .report(&kit, weights())
        .unwrap_or_else(|why| panic!("{why}"))
}

/// `got` within `tolerance` of `want`, relative.
fn close(got: f64, want: f64, tolerance: f64, what: &str) {
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

/// The pinned counts are printed to six decimals: `got` within half a unit of the last one, or
/// within `1e-7` relative when that is wider.
fn means(spend: &Spend, want: [f64; 6], what: &str) {
    for (id, want) in ORBS.into_iter().zip(want) {
        let got = spend.mean.get(id);
        assert!(
            (got - want).abs() <= (5e-7f64).max(1e-7 * want.abs()),
            "{what} {}: {got} is not {want}",
            id.name()
        );
    }
}

#[test]
fn the_steps_cost_this() {
    let report = report();
    let names: Vec<&str> = report.rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["alt_aug_regal"]);

    // The goal is a rare shield holding the totem prefix, so the craft is one step: alt, aug and
    // regal until it is there. Per step: chance, one run's counts, the counts to get past it, and
    // its weighted value and sd.
    type Expected = (f64, [f64; 6], [f64; 6], f64, f64);
    let expected: [Expected; 1] = [(
        1.0,
        [1.0, 80.0, 20.5, 1.0, 0.0, 0.0],
        [1.0, 80.0, 20.5, 1.0, 0.0, 0.0],
        17.545000,
        17.107034,
    )];
    for (row, (chance, run, reach, value, sd)) in report.rows.iter().zip(expected) {
        let what = format!("step {} {}", row.step, row.name);
        close(row.chance, chance, 1e-7, &format!("{what} chance"));
        close(row.tries, 1.0 / chance, 1e-7, &format!("{what} tries"));
        means(&row.run, run, &format!("{what} run"));
        means(&row.reach, reach, &format!("{what} reach"));
        let valued = weights().value(&row.reach);
        close(valued.value, value, 1e-7, &format!("{what} weighted"));
        close(valued.sd, sd, 1e-7, &format!("{what} weighted sd"));
        assert_eq!(valued.unweighted, Cost::none());
    }

    for (id, sd) in ORBS.into_iter().zip([0.0, 80.498447, 20.5, 0.0, 0.0, 0.0]) {
        close(
            report.total.sd(id),
            sd,
            1e-7,
            &format!("total {} sd", id.name()),
        );
    }
}

/// One step: getting past it is one run of it, and every step clears on its first try.
#[test]
fn one_step_carries_its_whole_cost() {
    let report = report();
    assert_eq!(report.rows.len(), 1);
    let row = &report.rows[0];
    close(row.chance, 1.0, 1e-12, "chance");
    close(row.tries, 1.0, 1e-12, "tries");
    assert_eq!(report.total, row.reach);
}

/// Solving each method on its own and nesting the answers gives what one chain of single orb uses
/// gives, spread included.
#[test]
fn nested_equals_flat() {
    let report = report();
    let flat = report.flat.spend();
    for a in ORBS {
        close(
            report.total.mean.get(a),
            flat.mean.get(a),
            1e-8,
            &format!("mean {}", a.name()),
        );
        for b in ORBS {
            close(
                report.total.covariance(a, b),
                flat.covariance(a, b),
                1e-8,
                &format!("covariance {} {}", a.name(), b.name()),
            );
        }
    }
}

/// Leaves a magic item through `magic`.
struct HandsOnMagic;

impl Method for HandsOnMagic {
    fn name(&self) -> String {
        "hands_on_magic".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "magic",
            rarity: Rarity::Magic,
        }]
    }

    fn rule(&self, _kit: &Kit, node: &Node) -> Move {
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            _ => Move::Exit("magic"),
        }
    }
}

#[test]
fn rarity_is_checked_at_wiring_time() {
    let magic_into_exalt = Craft {
        name: "magic into exalt",
        start: Rarity::Normal,
        steps: vec![
            Step {
                method: Box::new(HandsOnMagic),
                wires: vec![("magic", Wire::Next)],
            },
            Step {
                method: Box::new(ExaltTo { modifiers: 4 }),
                wires: vec![("slammed", Wire::Done), ("scrap", Wire::To(0))],
            },
        ],
    };
    let refused = magic_into_exalt.check().unwrap_err();
    assert_eq!(
        refused,
        CraftError::WrongRarity {
            step: 0,
            exit: "magic",
            rarity: Rarity::Magic,
            to: 1,
        }
    );
    assert_eq!(
        refused.to_string(),
        "step 1 does not take a magic item, and step 0's exit `magic` hands it one"
    );

    let unwired = Craft {
        name: "unwired",
        start: Rarity::Normal,
        steps: vec![
            Step {
                method: Box::new(AltAugRegal),
                wires: vec![("rare", Wire::Next)],
            },
            Step {
                method: Box::new(ExaltTo { modifiers: 4 }),
                wires: vec![("slammed", Wire::Done)],
            },
        ],
    };
    assert_eq!(
        unwired.check().unwrap_err(),
        CraftError::Unwired {
            step: 1,
            exit: "scrap"
        }
    );

    let mut from_rare = shield();
    from_rare.start = Rarity::Rare;
    assert_eq!(
        from_rare.check().unwrap_err(),
        CraftError::StartNotTaken {
            rarity: Rarity::Rare
        }
    );
    assert_eq!(shield().check(), Ok(()));
}

/// One move the written strategy does not make would cost less: on a magic item that already holds
/// the totem prefix, the augmentation buys a suffix before the regal, and regaling straight away
/// would do the job for 0.05 chaos less. That is the price of the bonus suffix, and the only place
/// the strategy is not the cheapest.
#[test]
fn the_other_moves_that_are_cheaper() {
    use Tier::Desired;
    let report = report();
    let magic = |prefix: &[Tier], suffix: &[Tier]| {
        let mut node = Node::fresh(Rarity::Magic);
        for &tier in prefix {
            node = node.with(Side::Prefix, tier);
        }
        for &tier in suffix {
            node = node.with(Side::Suffix, tier);
        }
        node
    };
    // 27 other moves were costed at 11 places; this is the one that comes out cheaper.
    assert_eq!(report.others.len(), 27);
    // node, the planned move and its cost to finish, the other move and its cost to finish.
    let expected: [(Node, CurrencyId, f64, CurrencyId, f64); 1] = [(
        magic(&[Desired], &[]),
        CurrencyId::AUGMENTATION,
        0.550,
        CurrencyId::REGAL,
        0.500,
    )];

    let better: Vec<_> = report.others.iter().filter(|other| other.better).collect();
    assert_eq!(better.len(), expected.len(), "{better:#?}");
    for (node, planned_move, planned_cost, other_move, other_cost) in expected {
        let found = better
            .iter()
            .find(|other| other.node == node && other.other == other_move)
            .unwrap_or_else(|| panic!("{} {} is not flagged", node.label(), other_move.name()));
        assert_eq!(found.step, 0);
        assert_eq!(found.planned, planned_move);
        let planned = weights().value(&Spend::certain(&found.planned_to_go)).value;
        assert!(
            (planned - planned_cost).abs() < 1e-3,
            "{} {}: {planned} is not {planned_cost}",
            node.label(),
            planned_move.name()
        );
        let got = weights().value(&Spend::certain(&found.other_to_go)).value;
        assert!(
            (got - other_cost).abs() < 1e-3,
            "{} {}: {got} is not {other_cost}",
            node.label(),
            other_move.name()
        );
    }
}

#[test]
fn a_key_without_a_weight_stays_its_own_line() {
    let imprint = CurrencyId("currency/imprint");
    let cost = Cost::of(imprint, 2.0).plus(&Cost::of(CurrencyId::ALTERATION, 10.0));
    let valued = weights().value(&Spend::certain(&cost));
    assert!((valued.value - 2.0).abs() < 1e-12);
    assert_eq!(valued.unweighted, Cost::of(imprint, 2.0));
}
