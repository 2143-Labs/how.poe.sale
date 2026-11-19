//! Essence of Horror on a Shaper Archdemon Crown: the data, the essence as a currency on a concrete
//! item and on the abstract one, and the method that uses it until the crown holds Hypothermia.
//!
//! Every expected number comes from `ai_research/spike-crown/model.py`, an exact model that reads
//! the raw export, not the generated data file. The concrete item is tested at item level 5, where
//! one essence makes 18 504 distinct items; at item level 86 it makes about 2.4 billion, which no
//! output set can hold, so the crown itself is tested on the abstract item only.

use simple::cost::{Cost, CurrencyId};
use simple::currency::{Applied, Currency, Essence, Named};
use simple::data::Data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Influence, Item, Rarity, Side};
use simple::law::Law;
use simple::method::{EssenceSpam, Kit, run};
use simple::node::Node;
use simple::pool::draw_weights;
use simple::stock::Refusal;
use simple::target::Target;
use simple::weights::Weights;

const CROWN: &str = include_str!("../data/crown.json");

/// Essence of Horror's modifier on a helmet (`essences.json`).
const HORROR_MOD: &str = "SocketedGemsDealMoreElementalDamageEssence1";

static HORROR: Essence = Essence {
    id: CurrencyId::ESSENCE_OF_HORROR,
    modifier: HORROR_MOD,
};

fn load() -> Data {
    Data::from_json(CROWN).expect("the crown data loads")
}

fn crown(data: &Data, item_level: u8, rarity: Rarity) -> Item {
    Item::of_base(data, data.base_of("Archdemon Crown"), item_level, rarity)
        .with_influence(Influence::Shaper)
}

fn id(data: &Data, name: &str) -> u16 {
    data.id_of(name)
        .unwrap_or_else(|| panic!("{name} is in the data"))
}

fn on_item(essence: &Essence, data: &Data, item: &Item) -> Result<Applied<Item>, Refusal> {
    <Essence as Currency<Item>>::apply(essence, data, item)
}

fn on_node(essence: &Essence, law: &Law, node: &Node) -> Result<Applied<Node>, Refusal> {
    <Essence as Currency<Node>>::apply(essence, law, node)
}

fn refusal<T>(result: Result<T, Refusal>) -> &'static str {
    match result {
        Err(Refusal(why)) => why,
        Ok(_) => panic!("expected a refusal, got outcomes"),
    }
}

fn exact(num: u128, den: u128) -> Fraction {
    Fraction::new(num, den).expect("a fraction")
}

fn value(chance: Fraction) -> f64 {
    chance.num() as f64 / chance.den() as f64
}

/// `got` within `tolerance` of `want`, relative.
fn close(got: f64, want: f64, tolerance: f64, what: &str) {
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

/// The item, graded, with its essence-only modifier read as placed rather than drawn: what
/// [`Node::of`] would have to know to grade an essence's item.
fn graded(data: &Data, filter: &ItemSetFilter, item: &Item) -> Node {
    let mut node = Node::fresh(item.rarity);
    for side in Side::BOTH {
        for affix in item.affixes(side) {
            let tier = filter.tier_of(affix.m);
            node = if data.mod_at(affix.m).essence_only {
                node.with_placed(side, tier)
            } else {
                node.with(side, tier)
            };
        }
    }
    node
}

#[test]
fn the_crown_loads_with_the_essence_modifier() {
    let data = load();
    assert_eq!(data.bases.len(), 1);
    assert_eq!(data.base_at(0).name, "Archdemon Crown");
    assert_eq!(
        data.mods.len(),
        178,
        "67 prefixes, 109 suffixes, the essence's modifier and one implicit"
    );

    // The essence's modifier is in the file, a suffix that rolls nowhere.
    let horror = id(&data, HORROR_MOD);
    let entry = data.mod_at(horror);
    assert_eq!(entry.kind.side(), Some(Side::Suffix));
    assert!(entry.essence_only);
    assert_eq!(data.weight_on(horror, 0, Some(Influence::Shaper)), 0);

    // The pool a rare Shaper crown draws from at item level 86, without it.
    let rare = crown(&data, 86, Rarity::Rare);
    for (side, count, total) in [(Side::Prefix, 67, 55_375), (Side::Suffix, 109, 85_000)] {
        let pool = draw_weights(&data, &rare, side);
        assert_eq!(pool.len(), count, "{side:?}");
        assert_eq!(
            pool.iter().map(|&(_, w)| w).sum::<u128>(),
            total,
            "{side:?}"
        );
        assert!(pool.iter().all(|&(m, _)| m != horror));
    }

    // The goal: Hypothermia 18 or 20 is desired; the essence's modifier, spell critical strike
    // and the life or area prefixes are good.
    let filter = ItemSetFilter::from_data(&data);
    assert_eq!(filter.tier_of(horror), Tier::Good);
    assert_eq!(
        filter.tier_of(id(&data, "IncreasedChillEffectSupportedUber2")),
        Tier::Desired
    );
    assert_eq!(
        filter.tier_of(id(&data, "IncreasedChillEffectSupportedUber1")),
        Tier::Any
    );
    let target = Target::from_data(&data);
    assert_eq!(target.prefixes, vec![Tier::Good]);
    assert_eq!(target.suffixes, vec![Tier::Desired, Tier::Good, Tier::Good]);

    // Its family is in neither pool, so the law can place it.
    let law = Law::new(&data, &filter, &crown(&data, 86, Rarity::Normal)).expect("a law");
    assert_eq!(law.placed(HORROR_MOD), Some((Side::Suffix, Tier::Good)));
    assert_eq!(law.placed("NotAModifier"), None);
}

#[test]
fn the_essence_on_an_item_is_exact() {
    let data = load();
    let start = crown(&data, 5, Rarity::Normal);
    let applied = on_item(&HORROR, &data, &start).expect("the essence works on a normal item");
    assert_eq!(applied.cost, Cost::of(CurrencyId::ESSENCE_OF_HORROR, 1.0));
    let outcomes = &applied.outcomes;
    assert!(outcomes.is_whole());
    assert_eq!(outcomes.parts().len(), 18_504);

    // Every outcome is rare and holds the essence's modifier, placed first on its side.
    let horror = id(&data, HORROR_MOD);
    for (item, _) in outcomes.parts() {
        assert_eq!(item.rarity, Rarity::Rare);
        assert_eq!(item.affixes(Side::Suffix)[0].m, horror);
    }

    // Four, five or six modifiers at 8:3:1.
    for (count, num, den) in [(4, 2, 3), (5, 1, 4), (6, 1, 12)] {
        assert_eq!(
            outcomes.chance(|item| item.explicits() == count),
            Some(exact(num, den)),
            "{count} modifiers"
        );
    }

    let cold = id(&data, "ColdResist1");
    assert_eq!(
        outcomes.chance(|item| item
            .affixes(Side::Suffix)
            .iter()
            .any(|affix| affix.m == cold)),
        Some(exact(38_846_771, 209_796_480))
    );
    let life = [id(&data, "IncreasedLife0"), id(&data, "IncreasedLife1")];
    assert_eq!(
        outcomes.chance(|item| item
            .affixes(Side::Prefix)
            .iter()
            .any(|affix| life.contains(&affix.m))),
        Some(exact(55, 112))
    );

    // One whole item, in draw order on each side.
    let prefixes: Vec<u16> = [
        "IncreasedLife0",
        "IncreasedMana1",
        "LocalBaseArmourAndEnergyShield1",
    ]
    .iter()
    .map(|name| id(&data, name))
    .collect();
    let suffixes: Vec<u16> = [HORROR_MOD, "ColdResist1", "Strength1"]
        .iter()
        .map(|name| id(&data, name))
        .collect();
    let ids = |item: &Item, side: Side| -> Vec<u16> {
        item.affixes(side).iter().map(|affix| affix.m).collect()
    };
    assert_eq!(
        outcomes.chance(
            |item| ids(item, Side::Prefix) == prefixes && ids(item, Side::Suffix) == suffixes
        ),
        Some(exact(5, 966_168))
    );

    // A rare item is reforged from scratch: what it held changes nothing.
    let rare = outcomes.parts()[0].0.clone();
    let again = on_item(&HORROR, &data, &rare).expect("the essence works on a rare item");
    assert_eq!(&again.outcomes, outcomes);
}

#[test]
fn the_essence_on_a_node_is_the_item_graded() {
    // A grading of the item level 5 pool: one desired suffix, two life prefixes of one family and
    // a light suffix as good, and the essence's own modifier as good.
    let mut data = load();
    data.target.desired = vec![id(&data, "ColdResist1")];
    data.target.good = ["IncreasedLife0", "IncreasedLife1", "Strength1", HORROR_MOD]
        .iter()
        .map(|name| id(&data, name))
        .collect();
    let filter = ItemSetFilter::from_data(&data);
    let start = crown(&data, 5, Rarity::Normal);
    let law = Law::new(&data, &filter, &start).expect("a law at item level 5");

    let items = on_item(&HORROR, &data, &start).expect("the essence on the item");
    let from_items = items
        .outcomes
        .relabel(|item| graded(&data, &filter, item))
        .expect("the graded chances fit");
    let nodes = on_node(&HORROR, &law, &Node::fresh(Rarity::Normal))
        .expect("the essence on the node")
        .outcomes;
    assert!(nodes.is_whole());
    assert_eq!(from_items.parts().len(), 88);
    assert_eq!(nodes.parts().len(), 88);
    for (node, chance) in nodes.parts() {
        let want = from_items
            .chance(|other| other == node)
            .expect("the sum fits");
        close(*chance, value(want), 1e-12, &node.label());
        // The essence's grade is placed, and the law's history starts after it.
        assert_eq!(node.side(Side::Suffix)[0], Tier::Good, "{}", node.label());
        assert_eq!(
            node.drawn(Side::Suffix).len() + 1,
            node.side(Side::Suffix).len()
        );
    }
    close(
        nodes.chance(|node| node.holds(Tier::Desired)).unwrap(),
        value(exact(38_846_771, 209_796_480)),
        1e-12,
        "the desired suffix",
    );
}

#[test]
fn essence_spam_for_hypothermia_costs_this() {
    let data = load();
    let filter = ItemSetFilter::from_data(&data);
    let target = Target::from_data(&data);
    let law = Law::new(&data, &filter, &crown(&data, 86, Rarity::Normal)).expect("a law");
    let tools: [&dyn Currency<Node>; 1] = [&HORROR];
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &tools,
    };

    // One essence: Hypothermia 18 or 20 one time in 34.99.
    let one = on_node(&HORROR, &law, &Node::fresh(Rarity::Normal)).expect("one essence");
    assert_eq!(one.outcomes.parts().len(), 131);
    close(
        one.outcomes
            .chance(|node| node.holds(Tier::Desired))
            .unwrap(),
        0.0285773174593339,
        1e-7,
        "Hypothermia per essence",
    );

    let method = EssenceSpam {
        essence: CurrencyId::ESSENCE_OF_HORROR,
    };
    let solved = run(&method, &kit, &[(Node::fresh(Rarity::Normal), 1.0)]).unwrap();
    // The normal item and every miss are reforged again; every hit leaves.
    assert_eq!(solved.inside.len(), 69);
    assert_eq!(solved.exits.len(), 63);
    for exit in &solved.exits {
        assert_eq!(exit.rarity, Rarity::Rare);
        assert!(exit.holds(Tier::Desired), "{}", exit.label());
    }
    assert_eq!(solved.keys, vec![CurrencyId::ESSENCE_OF_HORROR]);

    // A geometric count: mean 1/p, spread sqrt(1-p)/p.
    let spend = solved.spend();
    close(
        spend.mean.get(CurrencyId::ESSENCE_OF_HORROR),
        34.9927875988717,
        1e-7,
        "essences",
    );
    close(
        spend.sd(CurrencyId::ESSENCE_OF_HORROR),
        34.4891634624686,
        1e-7,
        "essences sd",
    );

    // Of the crowns it hands over, how many also hold spell critical strike: a second good suffix
    // beside the essence's own. And how many are the whole natural goal, a good prefix as well.
    let chances = solved.exit_chances();
    close(chances.iter().sum(), 1.0, 1e-9, "the exits");
    let crit: f64 = solved
        .exits
        .iter()
        .zip(&chances)
        .filter(|(node, _)| node.counts(Side::Suffix)[Tier::Good.rank() as usize] >= 2)
        .map(|(_, chance)| chance)
        .sum();
    close(crit, 0.0113517683361141, 1e-7, "spell crit as well");
    let goal: f64 = solved
        .exits
        .iter()
        .zip(&chances)
        .filter(|(node, _)| node.reached(&target))
        .map(|(_, chance)| chance)
        .sum();
    close(goal, 0.00141161680898771, 1e-7, "the natural goal");

    // The essence has no price in the table, so it stays a count of its own.
    let prices = Weights::from_json(
        r#"{ "unit": "chaos", "weights": {
            "currency/alteration": 0.091, "currency/augmentation": 0.067,
            "currency/transmutation": 0.021, "currency/regal": 0.217,
            "currency/exalted": 0.83, "currency/scouring": 0.625,
            "currency/annulment": 8.6 } }"#,
    )
    .expect("the prices load");
    let valued = prices.value(&spend);
    assert_eq!(valued.value, 0.0);
    close(
        valued.unweighted.get(CurrencyId::ESSENCE_OF_HORROR),
        34.9927875988717,
        1e-7,
        "unweighted essences",
    );
}

#[test]
fn the_essence_refuses_what_it_cannot_take() {
    let data = load();
    assert_eq!(HORROR.works_on(), &[Rarity::Normal, Rarity::Rare]);
    let law = Law::new(
        &data,
        &ItemSetFilter::from_data(&data),
        &crown(&data, 86, Rarity::Normal),
    )
    .expect("a law");

    let magic = crown(&data, 86, Rarity::Magic);
    assert_eq!(
        refusal(on_item(&HORROR, &data, &magic)),
        "an essence needs a normal or rare item"
    );
    assert_eq!(
        refusal(on_node(&HORROR, &law, &Node::fresh(Rarity::Magic))),
        "an essence needs a normal or rare item"
    );

    let mut corrupted = crown(&data, 5, Rarity::Normal);
    corrupted.corrupted = true;
    corrupted.mirrored = true;
    assert_eq!(
        refusal(on_item(&HORROR, &data, &corrupted)),
        "a corrupted item cannot take an essence"
    );
    let mut mirrored = crown(&data, 5, Rarity::Normal);
    mirrored.mirrored = true;
    assert_eq!(
        refusal(on_item(&HORROR, &data, &mirrored)),
        "a mirrored item cannot take an essence"
    );

    let unknown = Essence {
        id: CurrencyId::ESSENCE_OF_HORROR,
        modifier: "NotAModifier",
    };
    assert_eq!(
        refusal(on_item(&unknown, &data, &crown(&data, 5, Rarity::Normal))),
        "the data has no modifier for this essence"
    );
    assert_eq!(
        refusal(on_node(&unknown, &law, &Node::fresh(Rarity::Normal))),
        "the law cannot place this essence's modifier"
    );
}
