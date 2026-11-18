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

    assert_eq!(solved.inside.len(), 12);
    assert_eq!(solved.exits.len(), 8);
    for exit in &solved.exits {
        assert_eq!(exit.rarity, Rarity::Rare);
        assert!(exit.holds(Tier::Desired) && exit.reached(&target));
    }
    close(solved.exit_chances().iter().sum(), 1.0, "the exits");

    // 80 alterations, 20.5 augmentations, one transmutation and one regal: 17.545 chaos of orbs.
    let spend = solved.spend();
    for (id, mean) in [
        (CurrencyId::TRANSMUTATION, 1.0),
        (CurrencyId::ALTERATION, 80.0),
        (CurrencyId::AUGMENTATION, 20.5),
        (CurrencyId::REGAL, 1.0),
    ] {
        close(spend.mean.get(id), mean, id.name());
    }
    for id in [CurrencyId::EXALTED, CurrencyId::SCOURING] {
        assert_eq!(spend.mean.get(id), 0.0, "{}", id.name());
    }
    close(
        spend.sd(CurrencyId::ALTERATION),
        80.498447190,
        "alteration sd",
    );
    // Nothing is exalted, and nothing is scoured: the item is finished the moment it holds the
    // desired modifier.
    //
    // One transmutation starts the craft and every item is regaled exactly once, so both are
    // certain and have no spread.
    for id in [CurrencyId::REGAL, CurrencyId::TRANSMUTATION] {
        close(spend.mean.get(id), 1.0, id.name());
        assert_eq!(spend.sd(id), 0.0, "{}", id.name());
    }
    // A transmutation or an alteration leaves one modifier half the time and two the other half,
    // and a one-modifier item has the other side empty. A quarter of the rolls therefore leaves a
    // suffix and nothing else, and every one of those is augmented, because the augmentation can
    // be the totem prefix. A prefix and nothing else is altered instead: an augmentation there
    // would add a suffix, which cannot be the totem and would be altered away. The one exception
    // is the quarter-of-a-roll in 81 that holds the totem prefix alone: it is augmented to buy a
    // second modifier before the regal.
    let rolls = 1.0 + spend.mean.get(CurrencyId::ALTERATION);
    let suffix_only = rolls / 4.0;
    close(
        spend.mean.get(CurrencyId::AUGMENTATION),
        suffix_only * (1.0 + 1.0 / 81.0),
        "augmentations",
    );
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

    fn rule(&self, _kit: &Kit, node: &Node) -> Move {
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

    fn rule(&self, _kit: &Kit, _node: &Node) -> Move {
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
