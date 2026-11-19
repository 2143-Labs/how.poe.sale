//! A second craft on a second base: the Viridian Jewel 2+2 of catalogue row 13.
//!
//! Alt and aug until a magic jewel holds % maximum life (the desired prefix) and attack speed (the
//! desired suffix), then one regal and one exalt. A rare jewel side holds two modifiers, and the
//! jewel's pool has a family on both sides (`DamageForm`), so this file also pins the two walls
//! the craft runs into: the caps are the base's, and the law that reads each side apart is shown
//! to cost this craft nothing.
//!
//! The expected numbers come from an independent model (`ai_research/spike-jewel/model.py`: exact
//! fractions over the raw export, a per-side law, and a concrete chain that blocks a family across
//! sides), never from this crate.

use simple::cost::{Cost, CurrencyId};
use simple::currency::{Alteration, Augmentation, Currency, Exalted, ON_NODES, Regal};
use simple::data::Data;
use simple::filter::{ItemSetFilter, Tier};
use simple::fraction::Fraction;
use simple::item::{Affix, Item, Rarity, Side};
use simple::law::{Law, LawError};
use simple::method::{DesiredPair, Kit, Method, Move, run};
use simple::node::{Node, grade};
use simple::pool::{Caps, draw_weights};
use simple::stock::Refusal;
use simple::target::Target;
use simple::weights::Weights;

const JEWEL: &str = include_str!("../data/jewel.json");
const LEVEL: u8 = 83;
const LIFE: &str = "PercentIncreasedLifeJewel";
const SPEED: &str = "AttackSpeedJewel";

/// 3.30 prices (Covens day 4), in chaos.
const PRICES: &str = r#"{ "unit": "chaos", "weights": {
    "currency/transmutation": 0.021, "currency/alteration": 0.091,
    "currency/augmentation": 0.067, "currency/regal": 0.217, "currency/exalted": 0.83,
    "currency/scouring": 0.625, "currency/annulment": 8.6 } }"#;

fn load() -> Data {
    Data::from_json(JEWEL).unwrap_or_else(|why| panic!("crates/simple/data/jewel.json: {why}"))
}

/// A Viridian Jewel of this rarity holding these modifiers, in this order.
fn item(data: &Data, rarity: Rarity, prefixes: &[&str], suffixes: &[&str]) -> Item {
    let mut item = Item::of_base(data, data.base_of("Viridian Jewel"), LEVEL, rarity);
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

/// The jewel's law, each side read apart.
fn law(data: &Data) -> Law {
    let start = item(data, Rarity::Normal, &[], &[]);
    Law::sides_apart(data, &ItemSetFilter::from_data(data), &start)
        .expect("the jewel has a law with its sides apart")
}

fn float(chance: Fraction) -> f64 {
    chance.num() as f64 / chance.den() as f64
}

/// `got` within `tolerance` of `want`, relative.
fn close(got: f64, want: f64, tolerance: f64, what: &str) {
    assert!(
        (got - want).abs() <= tolerance * want.abs().max(1.0),
        "{what}: {got} is not {want}"
    );
}

/// The chance a law gives a grade; zero when the grade cannot come next.
fn next(law: &Law, side: Side, held: &[Tier], wanted: Tier) -> f64 {
    law.next(side, held)
        .iter()
        .find(|(tier, _)| *tier == wanted)
        .map(|&(_, chance)| chance)
        .unwrap_or(0.0)
}

/// The file loads, its pool is the export's, and its base's caps are a jewel's: two a side when
/// rare. Every rule that counts room reads them; the shield's are what they always were.
#[test]
fn the_jewel_has_a_jewels_caps() {
    let data = load();
    let base = data.base_at(0);
    assert_eq!(base.name, "Viridian Jewel");
    assert_eq!(base.item_class, "Jewel");
    assert_eq!(base.caps, Caps { magic: 1, rare: 2 });
    assert_eq!(
        simple::data::data().base_at(0).caps,
        Caps { magic: 1, rare: 3 }
    );

    // 52 prefixes over 19 550 and 62 suffixes over 19 370, on the first matching spawn tag.
    let pool = |item: &Item, side: Side| {
        let pool = draw_weights(&data, item, side);
        (
            pool.len(),
            pool.iter().map(|&(_, weight)| weight).sum::<u128>(),
        )
    };
    let magic = item(&data, Rarity::Magic, &[], &[]);
    assert_eq!(pool(&magic, Side::Prefix), (52, 19_550));
    assert_eq!(pool(&magic, Side::Suffix), (62, 19_370));

    // Two prefixes fill a rare jewel's prefix side. The Mine Damage prefix also takes the three
    // `DamageForm` suffixes (1 350 of weight) out of the suffix pool: the draw blocks a family
    // across sides.
    let rare = item(&data, Rarity::Rare, &[LIFE, "MineDamageJewel"], &[]);
    assert_eq!(pool(&rare, Side::Prefix), (0, 0));
    assert_eq!(pool(&rare, Side::Suffix), (59, 18_020));

    // Four modifiers fill a rare jewel, on an item and on a node alike.
    let no_room = Some(Refusal("the item has no room for another modifier"));
    let full = item(
        &data,
        Rarity::Rare,
        &[LIFE, "MineDamageJewel"],
        &[SPEED, "ColdResistanceJewel"],
    );
    assert_eq!(Exalted.apply(&data, &full).err(), no_room);
    let law = law(&data);
    let finished = node(
        Rarity::Rare,
        &[Tier::Desired, Tier::Any],
        &[Tier::Desired, Tier::Any],
    );
    assert_eq!(Exalted.apply(&law, &finished).err(), no_room);

    // With the prefix side full, an exalt can only add a suffix.
    let three = node(Rarity::Rare, &[Tier::Desired, Tier::Any], &[Tier::Desired]);
    let exalted = Exalted
        .apply(&law, &three)
        .expect("a rare jewel with room can be exalted");
    assert_eq!(exalted.outcomes.parts().len(), 1);
    assert_eq!(exalted.outcomes.parts()[0].0, finished);
    close(
        exalted.outcomes.parts()[0].1,
        1.0,
        1e-12,
        "the forced exalt",
    );

    // The target's floors, and its caps decide what is still reachable: a rare jewel whose two
    // prefixes are not the desired one cannot become the target; with three a side it could.
    let target = Target::from_data(&data);
    assert_eq!(target.prefixes, [Tier::Desired, Tier::Any]);
    assert_eq!(target.suffixes, [Tier::Desired, Tier::Any]);
    assert_eq!(target.caps, base.caps);
    let stuck = node(Rarity::Rare, &[Tier::Any, Tier::Any], &[Tier::Desired]);
    assert!(!stuck.reachable(&target));
    let three_a_side = Target {
        caps: Caps { magic: 1, rare: 3 },
        ..target.clone()
    };
    assert!(stuck.reachable(&three_a_side));
}

/// `DamageForm` rolls on both sides. The strict law refuses the pool by name; the law with its
/// sides apart builds, names the family it reads past, and gives each side's own chances.
#[test]
fn the_shared_family_is_named_not_hidden() {
    let data = load();
    let filter = ItemSetFilter::from_data(&data);
    let start = item(&data, Rarity::Normal, &[], &[]);
    assert_eq!(
        Law::new(&data, &filter, &start).err(),
        Some(LawError::SharedGroup("DamageForm".to_string()))
    );
    let law = law(&data);
    assert_eq!(law.shared(), ["DamageForm"]);
    assert_eq!(law.caps(), Caps { magic: 1, rare: 2 });

    let pins: [(Side, &[Tier], Tier, f64); 6] = [
        // 350 / 19 550, and after an `any` prefix took its family out of the pool.
        (Side::Prefix, &[], Tier::Desired, 0.017902813299232736),
        (
            Side::Prefix,
            &[Tier::Any],
            Tier::Desired,
            0.019270363904735063,
        ),
        (Side::Prefix, &[Tier::Desired], Tier::Any, 1.0),
        // 500 / 19 370, likewise.
        (Side::Suffix, &[], Tier::Desired, 0.02581311306143521),
        (
            Side::Suffix,
            &[Tier::Any],
            Tier::Desired,
            0.0266073036130163,
        ),
        (Side::Suffix, &[Tier::Desired], Tier::Any, 1.0),
    ];
    for (side, held, tier, want) in pins {
        close(
            next(&law, side, held, tier),
            want,
            1e-12,
            &format!("{side:?} {held:?} {tier:?}"),
        );
    }
    // Nothing comes after two: a rare jewel side is full.
    assert!(law.next(Side::Prefix, &[Tier::Any, Tier::Any]).is_empty());
    assert!(
        law.next(Side::Suffix, &[Tier::Desired, Tier::Any])
            .is_empty()
    );
}

/// The craft: 2 162.9 alterations, 23.6 augmentations, one each of the rest, 199.48 chaos at 3.30
/// prices.
#[test]
fn the_desired_pair_costs_this() {
    let data = load();
    let law = law(&data);
    let target = Target::from_data(&data);
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let solved = run(&DesiredPair, &kit, &[(Node::fresh(Rarity::Normal), 1.0)]).unwrap();

    // A normal item, eight magic ones, and the two rares a regal can make; one way out.
    assert_eq!(solved.inside.len(), 11);
    let finished = node(
        Rarity::Rare,
        &[Tier::Desired, Tier::Any],
        &[Tier::Desired, Tier::Any],
    );
    assert_eq!(solved.exits, [finished]);
    close(solved.exit_chances()[0], 1.0, 1e-9, "the exit");

    let spend = solved.spend();
    for (id, mean, sd) in [
        (
            CurrencyId::ALTERATION,
            2162.905714285714,
            2163.4056565064448,
        ),
        // The augmentations' sd equals their mean: in this chain their variance is exactly the
        // mean squared.
        (
            CurrencyId::AUGMENTATION,
            23.649285714285714,
            23.649285714285714,
        ),
    ] {
        close(spend.mean.get(id), mean, 1e-7, id.name());
        close(spend.sd(id), sd, 1e-7, &format!("{} sd", id.name()));
    }
    // One transmutation, one regal and one exalt, every time.
    for id in [
        CurrencyId::TRANSMUTATION,
        CurrencyId::REGAL,
        CurrencyId::EXALTED,
    ] {
        close(spend.mean.get(id), 1.0, 1e-9, id.name());
        assert!(
            spend.covariance(id, id).abs() < 1e-6,
            "{} varies",
            id.name()
        );
    }
    assert_eq!(spend.mean.get(CurrencyId::SCOURING), 0.0);

    let valued = Weights::from_json(PRICES).unwrap().value(&spend);
    close(valued.value, 199.47692214285712, 1e-7, "chaos");
    close(valued.sd, 198.4215372744611, 1e-7, "chaos sd");
    assert_eq!(valued.unweighted, Cost::none());
}

/// The game blocks a family across sides and the law with its sides apart does not. One
/// alteration shows where that bites: on the items holding an `any` modifier, which may be a
/// `DamageForm` one. The rest shows it costs this craft nothing, because every move it makes reads
/// only chances no `DamageForm` modifier is part of — and they are the game's exact fractions.
#[test]
fn reading_the_sides_apart_costs_this_craft_nothing() {
    use Tier::{Any, Desired};
    let data = load();
    let filter = ItemSetFilter::from_data(&data);
    let target = Target::from_data(&data);
    let law = law(&data);
    let exact = |num, den| Fraction::new(num, den).unwrap();

    let magic = item(&data, Rarity::Magic, &[], &[]);
    let rolled = grade(
        &filter,
        &Alteration
            .apply(&data, &magic)
            .expect("a magic jewel can be altered")
            .outcomes,
    )
    .expect("the graded chances fit");
    let on_node = Alteration
        .apply(&law, &Node::fresh(Rarity::Magic))
        .expect("a magic node can be altered")
        .outcomes;

    // The game's exact chance, and the law's: equal unless an `any` modifier is held beside a
    // modifier on the other side. A row is the prefixes, the suffixes, the game's chance as a
    // fraction, and the law's.
    type Row<'a> = (&'a [Tier], &'a [Tier], u128, u128, f64);
    let expected: [Row; 8] = [
        (&[Desired], &[Desired], 175, 757_367, 0.00023106367190543027),
        (&[Desired], &[], 7, 1_564, 0.004475703324808184),
        (&[], &[Desired], 25, 3_874, 0.006453278265358802),
        (&[Any], &[], 96, 391, 0.24552429667519182),
        (&[], &[Any], 1_887, 7_748, 0.2435467217346412),
        (
            &[Any],
            &[Desired],
            8_666_475,
            682_387_667,
            0.012675492858812174,
        ),
        (
            &[Desired],
            &[Any],
            701_427,
            80_280_902,
            0.008720342977710937,
        ),
        (
            &[Any],
            &[Any],
            326_407_554,
            682_387_667,
            0.47837310049157145,
        ),
    ];
    assert_eq!(rolled.parts().len(), expected.len());
    assert_eq!(on_node.parts().len(), expected.len());
    for (prefixes, suffixes, num, den, by_law) in expected {
        let want = node(Rarity::Magic, prefixes, suffixes);
        assert_eq!(
            rolled.chance(|got| *got == want),
            Some(exact(num, den)),
            "{} in the game",
            want.label()
        );
        let got = on_node.chance(|got| *got == want).unwrap();
        close(got, by_law, 1e-12, &format!("{} by the law", want.label()));
    }

    // The craft reads three chances: a roll making the pair, and an augmentation on a lone
    // desired modifier making it. In the game they are these fractions.
    let pair = node(Rarity::Magic, &[Desired], &[Desired]);
    let augmented = |state: &Item| {
        let outcomes = Augmentation
            .apply(&data, state)
            .expect("a lone modifier leaves room")
            .outcomes;
        grade(&filter, &outcomes)
            .expect("the graded chances fit")
            .chance(|got| *got == pair)
            .expect("the chance fits")
    };
    let from_life = augmented(&item(&data, Rarity::Magic, &[LIFE], &[]));
    let from_speed = augmented(&item(&data, Rarity::Magic, &[], &[SPEED]));
    assert_eq!(from_life, exact(50, 1_937));
    assert_eq!(from_speed, exact(7, 391));
    let rolled_to = |prefixes: &[Tier], suffixes: &[Tier]| {
        let want = node(Rarity::Magic, prefixes, suffixes);
        rolled.chance(|got| *got == want).unwrap()
    };
    let life_alone = rolled_to(&[Desired], &[]);
    let speed_alone = rolled_to(&[], &[Desired]);
    let per_roll = rolled_to(&[Desired], &[Desired])
        .checked_add(life_alone.checked_mul(from_life).unwrap())
        .and_then(|sum| sum.checked_add(speed_alone.checked_mul(from_speed).unwrap()))
        .unwrap();
    assert_eq!(per_roll, exact(350, 757_367));

    // Every rare the regal makes of the pair is exalted once, into the target, whichever modifier
    // the regal added.
    let regaled = Regal
        .apply(&data, &item(&data, Rarity::Magic, &[LIFE], &[SPEED]))
        .expect("the pair can be regaled");
    assert!(regaled.outcomes.is_whole());
    for (rare, _) in regaled.outcomes.parts() {
        assert_eq!(
            DesiredPair.rule(&target, &Node::of(&filter, rare)),
            Move::Use(CurrencyId::EXALTED)
        );
        let exalted = Exalted.apply(&data, rare).expect("one side has room");
        assert!(exalted.outcomes.is_whole());
        for (done, _) in exalted.outcomes.parts() {
            assert!(
                Node::of(&filter, done).reached(&target),
                "{}",
                done.describe(&data)
            );
        }
    }

    // So in the game: a roll makes the pair 350 times in 757 367 (the first roll is the
    // transmutation), and every roll that leaves a lone desired modifier is augmented once. The
    // law's chain spends exactly that.
    let kit = Kit {
        law: &law,
        target: &target,
        tools: &ON_NODES,
    };
    let spend = run(&DesiredPair, &kit, &[(Node::fresh(Rarity::Normal), 1.0)])
        .unwrap()
        .spend();
    let rolls = per_roll.den() as f64 / per_roll.num() as f64;
    close(
        spend.mean.get(CurrencyId::ALTERATION),
        rolls - 1.0,
        1e-9,
        "alterations",
    );
    close(
        spend.mean.get(CurrencyId::AUGMENTATION),
        rolls * float(life_alone.checked_add(speed_alone).unwrap()),
        1e-9,
        "augmentations",
    );
}
