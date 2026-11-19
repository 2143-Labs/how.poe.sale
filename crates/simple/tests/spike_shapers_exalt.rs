//! The Shaper's Exalted Orb, and research report 11's route to the totem shield with it: a rare
//! shield with no influence is prepared until its suffixes are full and one prefix is open, then
//! the orb adds the totem, the only Shaper prefix this base has.
//!
//! The item is report 11's: prefixes clearing `[desired, good, any]` and suffixes clearing
//! `[good, good, any]`, so the totals can be held against its 188 chaos. Every expected number
//! comes from `ai_research/spike-shapers-exalt/model.py`, an independent model over `shield.json`;
//! a disagreement is a finding about one of the two, never a number to re-pin.

use simple::cost::{Cost, CurrencyId};
use simple::craft::{Craft, Step, Wire};
use simple::currency::{Applied, Currency, Named, ON_NODES, ShapersExalted};
use simple::data::data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Affix, Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{Kit, ShaperSlam, run};
use simple::node::{Node, grade};
use simple::stock::{Refusal, Stock};
use simple::target::Target;
use simple::weights::{Weights, weights};

const BASE: &str = "Supreme Spiked Shield";

const LEVEL: u8 = 87;

/// The 3.30 prices report 11 priced its routes at (03's Covens snapshot), and its 89.6c orb.
const PRICES: &str = r#"{
    "unit": "chaos",
    "weights": {
        "currency/transmutation": 0.021,
        "currency/alteration": 0.095,
        "currency/augmentation": 0.064,
        "currency/regal": 0.232,
        "currency/exalted": 0.788,
        "currency/scouring": 0.612,
        "currency/shapers_exalted": 89.6
    }
}"#;

/// Per finished item, each ordinary orb's mean count and its sd. The orb itself is one, always.
const ROUTE: [(CurrencyId, f64, f64); 6] = [
    (CurrencyId::TRANSMUTATION, 7.787928805, 7.270756925),
    (CurrencyId::ALTERATION, 840.257610933, 840.757462257),
    (CurrencyId::AUGMENTATION, 44.402426779, 44.402426779),
    (CurrencyId::REGAL, 7.787928805, 7.270756925),
    (CurrencyId::EXALTED, 12.181893208, 10.983659286),
    (CurrencyId::SCOURING, 6.787928805, 7.270756925),
];

fn filter() -> ItemSetFilter {
    ItemSetFilter::from_data(data())
}

/// A shield holding these modifiers, by id, in draw order.
fn shield(
    rarity: Rarity,
    level: u8,
    influence: Option<Influence>,
    prefixes: &[&str],
    suffixes: &[&str],
) -> Item {
    let data = data();
    let mut item = Item::of_base(data, data.base_of(BASE), level, rarity);
    if let Some(influence) = influence {
        item = item.with_influence(influence);
    }
    for (side, ids) in [(Side::Prefix, prefixes), (Side::Suffix, suffixes)] {
        for id in ids {
            let m = data
                .id_of(id)
                .unwrap_or_else(|| panic!("{id} is not in the data"));
            item.affixes_mut(side).push(Affix::at_range(data, m));
        }
    }
    item
}

/// A shield with no influence and no modifiers.
fn plain(rarity: Rarity) -> Item {
    shield(rarity, LEVEL, None, &[], &[])
}

/// A rare ready for the slam: a good and an anything prefix, two good suffixes and an anything.
fn ready(influence: Option<Influence>) -> Item {
    shield(
        Rarity::Rare,
        LEVEL,
        influence,
        &["IncreasedLife10", "AttackerTakesDamage1"],
        &["FireResist8", "ColdResist8", "Dexterity1"],
    )
}

fn orb() -> ShapersExalted {
    ShapersExalted::new(data(), &filter(), &plain(Rarity::Normal))
        .expect("a plain shield can take the orb")
}

/// The law of the pool without an influence: every draw before the slam.
fn law() -> Law {
    Law::new(data(), &filter(), &plain(Rarity::Normal)).expect("the plain shield has a law")
}

/// Report 11's item.
fn target() -> Target {
    Target {
        prefixes: vec![Tier::Desired, Tier::Good, Tier::Any],
        suffixes: vec![Tier::Good, Tier::Good, Tier::Any],
        caps: data().base_at(0).caps,
    }
}

fn prices() -> Weights {
    Weights::from_json(PRICES).expect("the prices parse")
}

/// Runs `f` with the plain law, report 11's target, and the six orbs plus the Shaper's.
fn with_kit<T>(f: impl FnOnce(&Kit<'_>) -> T) -> T {
    let law = law();
    let target = target();
    let orb = orb();
    let mut tools: Vec<&dyn Currency<Node>> = ON_NODES.to_vec();
    tools.push(&orb);
    f(&Kit {
        law: &law,
        target: &target,
        tools: &tools,
    })
}

fn craft() -> Craft {
    Craft {
        name: "shaper_slam",
        start: Rarity::Normal,
        steps: vec![Step {
            method: Box::new(ShaperSlam),
            wires: vec![("shaped", Wire::Done)],
        }],
    }
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

fn float(chance: Fraction) -> f64 {
    chance.num() as f64 / chance.den() as f64
}

fn refusal<S: Stock>(result: Result<Applied<S>, Refusal>) -> Option<&'static str> {
    result.err().map(|Refusal(why)| why)
}

/// `got` within `tolerance` of `want`, relative.
fn close(got: f64, want: f64, tolerance: f64, what: &str) {
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

/// On a cleared rare both sides are open: the totem half the time, and each of the ten Shaper
/// suffixes a twentieth. Every outcome is a Shaper item holding one modifier the plain pool
/// cannot roll.
#[test]
fn the_orb_draws_only_from_the_shaper_modifiers() {
    let data = data();
    let totem = data.id_of("MaximumTotemUber1").unwrap();
    let applied = orb()
        .apply(data, &plain(Rarity::Rare))
        .expect("a plain rare can take the orb");
    assert!(applied.outcomes.is_whole());
    assert_eq!(applied.cost, Cost::of(CurrencyId::SHAPERS_EXALTED, 1.0));
    let parts = applied.outcomes.parts();
    assert_eq!(parts.len(), 11);
    for (item, chance) in parts {
        assert_eq!(item.influence, Some(Influence::Shaper));
        assert_eq!(item.explicits(), 1);
        if let Some(prefix) = item.affixes(Side::Prefix).first() {
            assert_eq!(prefix.m, totem);
            assert_eq!(*chance, Fraction::new(1, 2).unwrap());
        } else {
            let m = item.affixes(Side::Suffix)[0].m;
            assert_eq!(
                data.weight_on(m, item.base, None),
                0,
                "{}",
                data.mod_at(m).id
            );
            assert_eq!(*chance, Fraction::new(1, 20).unwrap());
        }
    }
}

/// With the suffixes full the orb can only add a prefix, and the only Shaper prefix is the
/// totem: one outcome, at chance one, and it is report 11's finished item.
#[test]
fn with_the_suffixes_full_the_totem_is_certain() {
    let data = data();
    let applied = orb()
        .apply(data, &ready(None))
        .expect("a plain rare can take the orb");
    let parts = applied.outcomes.parts();
    assert_eq!(parts.len(), 1);
    let (item, chance) = &parts[0];
    assert_eq!(*chance, Fraction::ONE);
    assert_eq!(item.influence, Some(Influence::Shaper));
    assert_eq!(
        item.affixes(Side::Prefix)[2].m,
        data.id_of("MaximumTotemUber1").unwrap()
    );
    assert!(target().reached(&filter(), item));
}

#[test]
fn the_orb_refuses_what_the_game_refuses() {
    let data = data();
    let orb = orb();
    assert_eq!(orb.id().name(), "shapers_exalted");
    assert_eq!(orb.works_on(), &[Rarity::Rare]);
    for rarity in [Rarity::Normal, Rarity::Magic] {
        assert_eq!(
            refusal(orb.apply(data, &plain(rarity))),
            Some("a Shaper's Exalted Orb needs a rare item")
        );
    }
    assert_eq!(
        refusal(orb.apply(data, &ready(Some(Influence::Shaper)))),
        Some("a Shaper's Exalted Orb needs an item with no influence")
    );
    let full = shield(
        Rarity::Rare,
        LEVEL,
        None,
        &["IncreasedLife10", "AttackerTakesDamage1", "IncreasedMana12"],
        &["FireResist8", "ColdResist8", "Dexterity1"],
    );
    assert_eq!(
        refusal(orb.apply(data, &full)),
        Some("the item has no room for another modifier")
    );
    // Every Shaper modifier needs item level 68 or more (the totem 70).
    let low = shield(Rarity::Rare, 60, None, &[], &[]);
    assert_eq!(
        refusal(orb.apply(data, &low)),
        Some("no Shaper modifier can roll on this item")
    );
    let mut corrupted = plain(Rarity::Rare);
    corrupted.corrupted = true;
    assert_eq!(
        refusal(orb.apply(data, &corrupted)),
        Some("a corrupted item cannot be exalted")
    );
}

/// The orb on an item, graded, against the orb on that item's abstract form.
fn agrees(item: &Item, expected: &[(&[Tier], &[Tier], u128, u128)]) {
    let filter = filter();
    let orb = orb();
    let on_item = orb.apply(data(), item).expect("the item can take the orb");
    let graded = grade(&filter, &on_item.outcomes).expect("the graded chances fit");
    let on_node = orb
        .apply(&law(), &Node::of(&filter, item))
        .expect("the node can take the orb");
    assert!(graded.is_whole());
    assert!(on_node.outcomes.is_whole());
    assert_eq!(graded.parts().len(), expected.len());
    assert_eq!(on_node.outcomes.parts().len(), expected.len());
    for &(prefixes, suffixes, num, den) in expected {
        let want = node(Rarity::Rare, prefixes, suffixes);
        let exact = Fraction::new(num, den).unwrap();
        assert_eq!(
            graded.chance(|got| *got == want),
            Some(exact),
            "{} on the item",
            want.label()
        );
        let chance = on_node.outcomes.chance(|got| *got == want).unwrap();
        assert!(
            (chance - float(exact)).abs() <= 1e-14,
            "{} on the node: {chance}",
            want.label()
        );
    }
}

/// Two of the ten Shaper suffixes are good under this grading, so an open suffix side takes a good
/// one a fifth of the time it is drawn. The prefix half is always the totem.
#[test]
fn on_an_abstract_item_the_orb_is_the_item_orb_graded() {
    use Tier::{Any, Desired, Good};
    agrees(
        &plain(Rarity::Rare),
        &[
            (&[Desired], &[], 1, 2),
            (&[], &[Good], 1, 10),
            (&[], &[Any], 2, 5),
        ],
    );
    agrees(
        &shield(
            Rarity::Rare,
            LEVEL,
            None,
            &["IncreasedLife10"],
            &["FireResist8", "ColdResist8"],
        ),
        &[
            (&[Good, Desired], &[Good, Good], 1, 2),
            (&[Good], &[Good, Good, Good], 1, 10),
            (&[Good], &[Good, Good, Any], 2, 5),
        ],
    );
    agrees(
        &ready(None),
        &[(&[Good, Any, Desired], &[Good, Good, Any], 1, 1)],
    );
}

/// What the abstract item cannot do. A Shaper rare is refused by the item orb because it has an
/// influence; its abstract form is the same node as the plain rare's, so the node orb takes it.
#[test]
fn an_abstract_item_cannot_say_it_already_has_an_influence() {
    let filter = filter();
    let shaped = ready(Some(Influence::Shaper));
    let orb = orb();
    assert_eq!(
        refusal(orb.apply(data(), &shaped)),
        Some("a Shaper's Exalted Orb needs an item with no influence")
    );
    let as_node = Node::of(&filter, &shaped);
    assert_eq!(as_node, Node::of(&filter, &ready(None)));
    let slammed = orb
        .apply(&law(), &as_node)
        .expect("a node does not say it has an influence");
    assert_eq!(slammed.outcomes.parts().len(), 1);
    // The orb cannot be built for a start that already has one.
    assert_eq!(
        ShapersExalted::new(
            data(),
            &filter,
            &plain(Rarity::Normal).with_influence(Influence::Shaper)
        )
        .err(),
        Some(Refusal(
            "a Shaper's Exalted Orb needs an item with no influence"
        ))
    );
}

/// Report 11's cheapest route at 3.30 prices, solved on the abstract item: 7.79 attempts, 840
/// alterations, 44.4 augmentations, 12.2 exalts and one Shaper's Exalted Orb per finished item,
/// 188.0 chaos all in.
#[test]
fn the_route_costs_this() {
    let solved =
        with_kit(|kit| run(&ShaperSlam, kit, &[(Node::fresh(Rarity::Normal), 1.0)]).unwrap());

    // 35 places before the slam and the 6 it is used at; 6 finished items, each with the totem
    // as its last prefix.
    assert_eq!(solved.inside.len(), 41);
    assert_eq!(solved.exits.len(), 6);
    let target = target();
    for exit in &solved.exits {
        assert!(exit.reached(&target), "{}", exit.label());
        assert_eq!(
            exit.side(Side::Prefix).last(),
            Some(&Tier::Desired),
            "{}",
            exit.label()
        );
    }
    close(solved.exit_chances().iter().sum(), 1.0, 1e-12, "the exits");

    let spend = solved.spend();
    for (id, mean, sd) in ROUTE {
        close(spend.mean.get(id), mean, 1e-7, id.name());
        close(spend.sd(id), sd, 1e-7, &format!("{} sd", id.name()));
    }
    close(
        spend.mean.get(CurrencyId::SHAPERS_EXALTED),
        1.0,
        1e-9,
        "the orb",
    );

    let valued = prices().value(&spend);
    close(valued.value, 187.990118617, 1e-7, "chaos");
    close(valued.sd, 96.729071852, 1e-7, "chaos sd");
    assert_eq!(valued.unweighted, Cost::none());

    // The crate's own table has no price for the orb: it stays a count of its own kind.
    let crate_table = weights().value(&spend);
    close(crate_table.value, 202.079117322, 1e-7, "crate weights");
    let unweighted: Vec<CurrencyId> = crate_table.unweighted.iter().map(|(id, _)| id).collect();
    assert_eq!(unweighted, [CurrencyId::SHAPERS_EXALTED]);
    close(
        crate_table.unweighted.get(CurrencyId::SHAPERS_EXALTED),
        1.0,
        1e-9,
        "the unpriced orb",
    );
}

/// The same route as a one-step craft: the wiring checks, and the craft's cost is the method's.
#[test]
fn the_craft_carries_the_same_cost() {
    with_kit(|kit| {
        let craft = craft();
        assert_eq!(craft.check(), Ok(()));
        let report = craft
            .report(kit, &prices())
            .unwrap_or_else(|why| panic!("{why}"));
        assert_eq!(report.rows.len(), 1);
        close(report.rows[0].chance, 1.0, 1e-12, "chance");
        for (id, mean, _) in ROUTE {
            close(report.total.mean.get(id), mean, 1e-7, id.name());
        }
        let flat = report.flat.spend();
        let keys: Vec<CurrencyId> = ROUTE
            .iter()
            .map(|&(id, _, _)| id)
            .chain([CurrencyId::SHAPERS_EXALTED])
            .collect();
        for &a in &keys {
            close(
                report.total.mean.get(a),
                flat.mean.get(a),
                1e-8,
                &format!("mean {}", a.name()),
            );
            for &b in &keys {
                close(
                    report.total.covariance(a, b),
                    flat.covariance(a, b),
                    1e-8,
                    &format!("covariance {} {}", a.name(), b.name()),
                );
            }
        }
    });
}

/// Where the abstract item breaks. The report's other-moves check tries the orb early, at every
/// rare the route exalts or scours, and the node it lands on does not record the slam: the route
/// carries on as if it had not happened and slams again at the end. Every one of the 26 early
/// slams is costed with a second orb the game would refuse. At a rare holding a good prefix and two
/// good suffixes it comes to exactly two orbs and one exalt.
#[test]
fn the_other_moves_cost_an_early_slam_as_if_it_could_be_repeated() {
    with_kit(|kit| {
        let report = craft()
            .report(kit, &prices())
            .unwrap_or_else(|why| panic!("{why}"));
        assert_eq!(report.others.len(), 84);
        let early: Vec<_> = report
            .others
            .iter()
            .filter(|other| other.other == CurrencyId::SHAPERS_EXALTED)
            .collect();
        assert_eq!(early.len(), 26);
        for other in &early {
            assert!(
                other.other_to_go.get(CurrencyId::SHAPERS_EXALTED) > 1.0 + 1e-9,
                "{}",
                other.node.label()
            );
        }

        let place = node(Rarity::Rare, &[Tier::Good], &[Tier::Good, Tier::Good]);
        let found = early
            .iter()
            .find(|other| other.node == place)
            .unwrap_or_else(|| panic!("no early slam at {}", place.label()));
        assert_eq!(found.planned, CurrencyId::EXALTED);
        close(
            found.other_to_go.get(CurrencyId::SHAPERS_EXALTED),
            2.0,
            1e-9,
            "orbs",
        );
        close(
            found.other_to_go.get(CurrencyId::EXALTED),
            1.0,
            1e-9,
            "exalts",
        );
        let rest: f64 = found
            .other_to_go
            .iter()
            .filter(|(id, _)| ![CurrencyId::SHAPERS_EXALTED, CurrencyId::EXALTED].contains(id))
            .map(|(_, count)| count.abs())
            .sum();
        assert!(rest < 1e-9, "{:?}", found.other_to_go);

        // The route as written, from the same place.
        for (id, mean) in [
            (CurrencyId::TRANSMUTATION, 1.946982201),
            (CurrencyId::ALTERATION, 210.064402733),
            (CurrencyId::AUGMENTATION, 11.100606695),
            (CurrencyId::REGAL, 1.946982201),
            (CurrencyId::EXALTED, 5.045473302),
            (CurrencyId::SCOURING, 1.946982201),
            (CurrencyId::SHAPERS_EXALTED, 1.0),
        ] {
            close(found.planned_to_go.get(id), mean, 1e-7, id.name());
        }
    });
}
