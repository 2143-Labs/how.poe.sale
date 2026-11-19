//! Hunter Two-Toned Boots (`BootsAtlas1`, item level 86): alter until Tailwind, regal, bench a
//! life prefix.
//!
//! The expected numbers come from `ai_research/spike-boots/model.py`, an exact-fraction model of
//! the same rules over `data/boots.json`, so a disagreement here is a finding about one of the
//! two, never a number to re-pin.

use simple::cost::{Cost, CurrencyId, Spend};
use simple::craft::{Craft, Step, Wire};
use simple::currency::{Bench, Currency, Named, ON_NODES, Transmutation};
use simple::data::Data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{AltToMod, ExitSpec, Kit, Method, Move, run};
use simple::node::{Node, grade};
use simple::pool;
use simple::stock::Refusal;
use simple::target::Target;
use simple::weights::Weights;

const BOOTS: &str = include_str!("../data/boots.json");

/// The 3.30 prices the spike is costed at, in chaos. `currency/alchemy` is absent on purpose: the
/// bench is priced in it and nothing here prices an orb of alchemy.
const PRICES: &str = r#"{
  "unit": "chaos",
  "weights": {
    "currency/transmutation": 0.021,
    "currency/alteration": 0.091,
    "currency/augmentation": 0.067,
    "currency/regal": 0.217,
    "currency/exalted": 0.83,
    "currency/scouring": 0.625,
    "currency/annulment": 8.6
  }
}"#;

fn boots_data() -> Data {
    Data::from_json(BOOTS).expect("the boots data loads")
}

/// The base at item level 86 with a Hunter influence and this rarity, holding nothing explicit.
fn item(data: &Data, rarity: Rarity) -> Item {
    Item::of_base(data, data.base_of("Two-Toned Boots"), 86, rarity)
        .with_influence(Influence::Hunter)
}

fn boots() -> (Law, Target) {
    let data = boots_data();
    let start = item(&data, Rarity::Normal);
    let filter = ItemSetFilter::from_data(&data);
    let law = Law::new(&data, &filter, &start).expect("the boots have a law");
    (law, Target::from_data(&data))
}

/// The currencies the methods may name: the six random orbs, plus the bench. The bench is not in
/// [`ON_NODES`] (it is not a random orb), so a craft that uses it adds it to the tools itself.
fn currencies() -> Vec<&'static dyn Currency<Node>> {
    let mut tools: Vec<&'static dyn Currency<Node>> = ON_NODES.to_vec();
    tools.push(&Bench);
    tools
}

/// `got` within `tolerance` of `want`, relative, with an absolute floor of one unit so a zero is
/// compared absolutely.
fn close(got: f64, want: f64, tolerance: f64, what: &str) {
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

fn means(spend: &Spend, want: &[(CurrencyId, f64)], what: &str) {
    for &(id, value) in want {
        close(
            spend.mean.get(id),
            value,
            1e-7,
            &format!("{what}, {}", id.name()),
        );
    }
}

#[test]
fn the_data_files_target_is_the_tailwind_suffix() {
    let (_, target) = boots();
    assert!(target.prefixes.is_empty());
    assert_eq!(target.suffixes, [Tier::Desired]);

    // On the abstract item, the floor is the desired suffix: a rare prefix alone does not clear
    // it, the Tailwind suffix does.
    let rare = Node::fresh(Rarity::Rare);
    assert!(!rare.with(Side::Prefix, Tier::Any).reached(&target));
    assert!(rare.with(Side::Suffix, Tier::Desired).reached(&target));
}

#[test]
fn stage_one_alters_until_tailwind() {
    let (law, target) = boots();
    let tools = currencies();
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &tools,
    };
    let solved =
        run(&AltToMod, &kit, &[(Node::fresh(Rarity::Normal), 1.0)]).expect("the boots solve");

    // Four places: the normal item, and the three misses a magic roll leaves — a prefix alone, a
    // suffix alone, and both sides.
    assert_eq!(solved.inside.len(), 4);
    assert_eq!(solved.exits.len(), 2);

    let alone = Node::fresh(Rarity::Magic).with(Side::Suffix, Tier::Desired);
    let paired = alone.with(Side::Prefix, Tier::Any);
    let chance_to = |node: &Node| -> f64 {
        let at = solved
            .exits
            .iter()
            .position(|exit| exit == node)
            .expect("an exit");
        solved.exit_chances()[at]
    };
    // The share of finished runs that end in each exit: a run that misses comes back and can end
    // there later, so this is not the chance one roll holds Tailwind (that is pinned exactly in
    // `one_transmutation_holds_tailwind_three_times_in_2468`).
    close(chance_to(&alone), 0.25, 1e-12, "the lone tailwind");
    close(chance_to(&paired), 0.75, 1e-12, "with a prefix");
    close(solved.exit_chances().iter().sum(), 1.0, 1e-12, "the exits");

    let spend = solved.spend();
    means(
        &spend,
        &[
            (CurrencyId::TRANSMUTATION, 1.0),
            (CurrencyId::ALTERATION, 616.0),
            (CurrencyId::AUGMENTATION, 154.25),
            (CurrencyId::REGAL, 0.0),
            (CurrencyId::SCOURING, 0.0),
        ],
        "stage 1",
    );
    close(
        spend.sd(CurrencyId::ALTERATION),
        616.4997972424646,
        1e-7,
        "alteration sd",
    );
    close(
        spend.sd(CurrencyId::AUGMENTATION),
        154.49979773449544,
        1e-7,
        "augmentation sd",
    );
    close(
        spend.covariance(CurrencyId::ALTERATION, CurrencyId::AUGMENTATION),
        95018.0,
        1e-7,
        "the alterations and the augmentations move together",
    );
}

#[test]
fn one_transmutation_holds_tailwind_three_times_in_2468() {
    let data = boots_data();
    let filter = ItemSetFilter::from_data(&data);
    let applied = Transmutation
        .apply(&data, &item(&data, Rarity::Normal))
        .expect("a normal item is transmuted");
    let graded = grade(&filter, &applied.outcomes).expect("the outcomes grade");

    let alone = Node::fresh(Rarity::Magic).with(Side::Suffix, Tier::Desired);
    let paired = alone.with(Side::Prefix, Tier::Any);
    let chance_of = |want: &Node| graded.chance(|node| node == want);

    // One roll, exactly: a suffix on its own 1/2468, a suffix beside a prefix 1/1234 — three
    // quarters of one suffix draw, and the same on a magic item (the alteration).
    assert_eq!(chance_of(&alone), Fraction::new(1, 2468));
    assert_eq!(chance_of(&paired), Fraction::new(1, 1234));
    let holds = graded.chance(|node| node.holds(Tier::Desired));
    assert_eq!(holds, Fraction::new(3, 2468));
    assert!(graded.is_whole());
}

/// The same goal without the augmentation: transmute once, then alter. This is the rule the
/// research's catalogue reports (821.7 alterations), kept here as the independent check of it.
struct AltOnly;

impl Method for AltOnly {
    fn name(&self) -> String {
        "alt_only".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "magic",
            rarity: Rarity::Magic,
        }]
    }

    fn rule(&self, _target: &Target, node: &Node) -> Move {
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic if node.holds(Tier::Desired) => Move::Exit("magic"),
            Rarity::Magic => Move::Use(CurrencyId::ALTERATION),
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}

#[test]
fn without_the_augmentation_the_alteration_count_is_the_catalogue_figure() {
    let (law, target) = boots();
    let tools = currencies();
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &tools,
    };
    let solved =
        run(&AltOnly, &kit, &[(Node::fresh(Rarity::Normal), 1.0)]).expect("the boots solve");
    let spend = solved.spend();

    // 2465/3 = 821.666…: one transmutation and then (1 - p) / p alterations, with
    // p = 75/61700 — three quarters of one tailwind draw per roll.
    means(
        &spend,
        &[
            (CurrencyId::TRANSMUTATION, 1.0),
            (CurrencyId::ALTERATION, 821.6666666666666),
            (CurrencyId::AUGMENTATION, 0.0),
        ],
        "without the augmentation",
    );
    close(
        spend.sd(CurrencyId::ALTERATION),
        822.1665146293528,
        1e-7,
        "alteration sd",
    );

    let prices = Weights::from_json(PRICES).expect("the prices load");
    let valued = prices.value(&spend);
    close(valued.value, 74.79266666666666, 1e-7, "the chaos");
    close(valued.sd, 74.8171528312711, 1e-7, "its spread");
}

/// Regal a magic item into a rare one, and leave. A normal item reaching this step (a flat-chain
/// alternative) is handed back to the first step.
struct RegalTo;

impl Method for RegalTo {
    fn name(&self) -> String {
        "regal_to".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "rare",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "normal",
                rarity: Rarity::Normal,
            },
        ]
    }

    fn rule(&self, _target: &Target, node: &Node) -> Move {
        match node.rarity {
            Rarity::Magic => Move::Use(CurrencyId::REGAL),
            Rarity::Rare => Move::Exit("rare"),
            Rarity::Normal => Move::Exit("normal"),
        }
    }
}

/// Bench the life prefix, and leave. The crafted flag is what says the bench has already run: a
/// second use is refused by the currency, so the rule has to exit instead of naming it.
struct BenchLife;

impl Method for BenchLife {
    fn name(&self) -> String {
        "bench_life".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "done",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "scrap",
                rarity: Rarity::Normal,
            },
        ]
    }

    fn rule(&self, _target: &Target, node: &Node) -> Move {
        if node.rarity != Rarity::Rare {
            return Move::Exit("scrap");
        }
        let room = node.side(Side::Prefix).len() < pool::rarity_cap(Rarity::Rare);
        if !node.crafted() && room {
            Move::Use(CurrencyId::BENCH)
        } else {
            Move::Exit("done")
        }
    }
}

fn craft() -> Craft {
    Craft {
        name: "boots",
        start: Rarity::Normal,
        steps: vec![
            Step {
                method: Box::new(AltToMod),
                wires: vec![("magic", Wire::Next)],
            },
            Step {
                method: Box::new(RegalTo),
                wires: vec![("rare", Wire::Next), ("normal", Wire::To(0))],
            },
            Step {
                method: Box::new(BenchLife),
                wires: vec![("done", Wire::Done), ("scrap", Wire::To(0))],
            },
        ],
    }
}

#[test]
fn the_craft_costs_this() {
    let (law, target) = boots();
    let tools = currencies();
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &tools,
    };
    let prices = Weights::from_json(PRICES).expect("the prices load");
    let boot_craft = craft();
    let report = boot_craft
        .report(&kit, &prices)
        .unwrap_or_else(|why| panic!("{why}"));

    let names: Vec<&str> = report.rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["alt_to_mod", "regal_to", "bench_life"]);
    for row in &report.rows {
        close(row.chance, 1.0, 1e-12, "the share that goes on");
    }

    means(
        &report.rows[0].run,
        &[
            (CurrencyId::TRANSMUTATION, 1.0),
            (CurrencyId::ALTERATION, 616.0),
            (CurrencyId::AUGMENTATION, 154.25),
        ],
        "step 0",
    );
    close(
        report.rows[1].run.mean.get(CurrencyId::REGAL),
        1.0,
        1e-12,
        "step 1's regal",
    );
    close(
        report.rows[2].run.mean.get(CurrencyId::ALCHEMY),
        4.0,
        1e-12,
        "step 2's bench",
    );

    let total = &report.total;
    means(
        total,
        &[
            (CurrencyId::TRANSMUTATION, 1.0),
            (CurrencyId::ALTERATION, 616.0),
            (CurrencyId::AUGMENTATION, 154.25),
            (CurrencyId::REGAL, 1.0),
            (CurrencyId::SCOURING, 0.0),
        ],
        "the craft",
    );
    close(
        total.mean.get(CurrencyId::ALCHEMY),
        4.0,
        1e-12,
        "the bench's alchemy",
    );
    close(
        total.sd(CurrencyId::ALTERATION),
        616.4997972424646,
        1e-7,
        "alteration sd",
    );

    let valued = prices.value(total);
    close(valued.value, 66.62875, 1e-7, "the craft's weighted cost");
    close(valued.sd, 66.43176194929275, 1e-7, "the craft's spread");
    close(
        valued.unweighted.get(CurrencyId::ALCHEMY),
        4.0,
        1e-12,
        "the bench is its own line",
    );

    // The flat chain — one orb per place — agrees with the nested one, means and spread.
    let flat = report.flat.spend();
    for (id, want) in [
        (CurrencyId::TRANSMUTATION, 1.0),
        (CurrencyId::ALTERATION, 616.0),
        (CurrencyId::AUGMENTATION, 154.25),
        (CurrencyId::REGAL, 1.0),
        (CurrencyId::SCOURING, 0.0),
        (CurrencyId::ALCHEMY, 4.0),
    ] {
        close(flat.mean.get(id), want, 1e-8, id.name());
    }
    close(
        flat.covariance(CurrencyId::ALTERATION, CurrencyId::AUGMENTATION),
        total.covariance(CurrencyId::ALTERATION, CurrencyId::AUGMENTATION),
        1e-8,
        "the covariance",
    );
}

#[test]
fn the_bench_adds_one_chosen_prefix_with_certainty() {
    let (law, _) = boots();
    assert_eq!(Bench.id(), CurrencyId::BENCH);
    assert_eq!(Bench.works_on(), &[Rarity::Magic, Rarity::Rare]);

    let rare = Node::fresh(Rarity::Rare).with(Side::Suffix, Tier::Desired);
    let applied = Bench.apply(&law, &rare).expect("the bench applies");
    assert!(applied.outcomes.is_whole());
    assert_eq!(applied.outcomes.parts().len(), 1);
    let (node, chance) = &applied.outcomes.parts()[0];
    assert_eq!(*chance, 1.0);
    assert_eq!(*node, rare.with_crafted(Side::Prefix, Tier::Any));
    assert_eq!(node.side(Side::Prefix).to_vec(), vec![Tier::Any]);
    assert!(node.crafted());
    assert_eq!(applied.cost, Cost::of(CurrencyId::ALCHEMY, 4.0));
}

#[test]
fn the_bench_refuses_what_it_cannot_do() {
    let (law, _) = boots();
    assert_eq!(
        Bench.apply(&law, &Node::fresh(Rarity::Normal)).err(),
        Some(Refusal("the crafting bench needs a magic or rare item"))
    );
    let crafted = Node::fresh(Rarity::Rare).with_crafted(Side::Prefix, Tier::Any);
    assert_eq!(
        Bench.apply(&law, &crafted).err(),
        Some(Refusal("the item already holds a crafted modifier"))
    );
    let full = Node::fresh(Rarity::Rare)
        .with(Side::Prefix, Tier::Any)
        .with(Side::Prefix, Tier::Any)
        .with(Side::Prefix, Tier::Any);
    assert_eq!(
        Bench.apply(&law, &full).err(),
        Some(Refusal("the item has no room for another prefix"))
    );
}
