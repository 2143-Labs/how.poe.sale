//! The two methods, solved on the shield.
//!
//! The expected numbers come from an independent implementation of the same rules, so a
//! disagreement here is a finding about one of the two, never a number to re-pin.

mod common;

use simple::chain::SolveError;
use simple::cost::CurrencyId;
use simple::currency::{Currency, ON_NODES};
use simple::data::data;
use simple::filter::{ItemSetFilter, Tier};
use simple::item::{Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{AltAugRegal, ExaltTo, ExitSpec, Kit, Method, Move, run};
use simple::node::Node;
use simple::stock::Refusal;
use simple::target::Target;

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

/// `got` within `1e-7` of `want`, relative.
fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-7 * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

#[test]
fn alt_aug_regal_costs_this() {
    let law = law();
    let target = Target::from_data(data());
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let solved = run(&AltAugRegal, &kit, &[(Node::fresh(Rarity::Normal), 1.0)]).unwrap();

    assert_eq!(solved.inside.len(), 13);
    assert_eq!(solved.exits.len(), 7);
    for exit in &solved.exits {
        assert_eq!(exit.rarity, Rarity::Rare);
        assert!(exit.holds(Tier::Desired) && exit.reachable(&target));
    }
    close(solved.exit_chances().iter().sum(), 1.0, "the exits");

    let spend = solved.spend();
    for (id, mean) in [
        (CurrencyId::TRANSMUTATION, 1.560856031),
        (CurrencyId::ALTERATION, 167.011595286),
        (CurrencyId::AUGMENTATION, 0.520285344),
        (CurrencyId::REGAL, 1.560856031),
        (CurrencyId::SCOURING, 0.560856031),
    ] {
        close(spend.mean.get(id), mean, id.name());
    }
    assert_eq!(spend.mean.get(CurrencyId::EXALTED), 0.0);
    close(
        spend.sd(CurrencyId::ALTERATION),
        167.510849,
        "alteration sd",
    );
    // A regal leaves the rare able to become the target 0.640674079 of the time, and the method
    // is run until one does: the transmutations are a geometric count, whose spread is
    // `sqrt(1 - p) / p`. The reference states it as 0.935636.
    let alive = 1.0 / spend.mean.get(CurrencyId::REGAL);
    close(alive, 0.640674079, "alive after the regal");
    let spread = spend.sd(CurrencyId::TRANSMUTATION);
    close(spread, (1.0 - alive).sqrt() / alive, "transmutation sd");
    assert!((spread - 0.935636).abs() < 1e-6, "{spread}");
}

/// Roll for the desired modifier with a currency whose cost depends on what it is used on: the
/// first roll, on a normal item, is a transmutation; every later one pays a scouring as well.
struct RestartForDesired;

impl Method for RestartForDesired {
    fn name(&self) -> String {
        "restart_for_desired".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "rare",
            rarity: Rarity::Rare,
        }]
    }

    fn rule(&self, _target: &Target, node: &Node) -> Move {
        if node.rarity == Rarity::Rare {
            Move::Exit("rare")
        } else if node.rarity == Rarity::Magic && node.holds(Tier::Desired) {
            if Side::BOTH
                .into_iter()
                .any(|side| node.side(side).is_empty())
            {
                Move::Use(CurrencyId::AUGMENTATION)
            } else {
                Move::Use(CurrencyId::REGAL)
            }
        } else {
            Move::Use(CurrencyId("test/restart"))
        }
    }
}

#[test]
fn a_cost_that_depends_on_the_input_is_carried_through_the_solve() {
    let law = law();
    let target = Target::from_data(data());
    let mut tools: Vec<&dyn Currency<Node>> = ON_NODES.to_vec();
    tools.push(&common::Restart);
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &tools,
    };
    let solved = run(
        &RestartForDesired,
        &kit,
        &[(Node::fresh(Rarity::Normal), 1.0)],
    )
    .unwrap();

    // 108 rolls at one in 108, and the first pays no scouring.
    let spend = solved.spend();
    for (id, mean) in [
        (CurrencyId::TRANSMUTATION, 108.0),
        (CurrencyId::SCOURING, 107.0),
        (CurrencyId::REGAL, 1.0),
        (CurrencyId::AUGMENTATION, 1.0 / 3.0),
    ] {
        assert!(
            (spend.mean.get(id) - mean).abs() <= 1e-9 * mean,
            "{}: {} is not {mean}",
            id.name(),
            spend.mean.get(id)
        );
    }
    assert_eq!(spend.mean.get(CurrencyId("test/restart")), 0.0);
}

#[test]
fn a_method_cannot_leave_with_a_rarity_its_exit_does_not_declare() {
    let law = law();
    let target = Target::from_data(data());
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let magic = Node::fresh(Rarity::Magic).with(Side::Prefix, Tier::Any);
    let handed = run(&ExaltTo { modifiers: 4 }, &kit, &[(magic, 1.0)]);
    assert_eq!(
        handed.unwrap_err(),
        SolveError::BadExit {
            at: "exalt_to(4) at magic any / -".to_string(),
            exit: "scrap",
            why: "the item leaves with a rarity the exit does not declare",
        }
    );
}

/// Exalts whatever it is handed.
struct ExaltAnything;

impl Method for ExaltAnything {
    fn name(&self) -> String {
        "exalt_anything".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[]
    }

    fn rule(&self, _target: &Target, _node: &Node) -> Move {
        Move::Use(CurrencyId::EXALTED)
    }
}

#[test]
fn a_currency_refuses_a_rarity_it_does_not_work_on() {
    let law = law();
    let target = Target::from_data(data());
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let magic = Node::fresh(Rarity::Magic).with(Side::Prefix, Tier::Any);
    let exalted = run(&ExaltAnything, &kit, &[(magic, 1.0)]);
    assert_eq!(
        exalted.unwrap_err(),
        SolveError::Refused {
            at: "exalt_anything at magic any / -".to_string(),
            currency: CurrencyId::EXALTED,
            why: Refusal("an Exalted Orb needs a rare item"),
        }
    );
}
