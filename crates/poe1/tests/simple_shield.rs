//! The Simple game carries the export's shield data: the Shaper Supreme Spiked Shield at item
//! level 87, with every modifier the pinned export rolls on it, so the per-currency model can be
//! exercised without the 63 MB export. The chances must come out identical to the export's.

mod common;

use poe1::apply::draw_weights;
use poe1::currency::{Accept, Fraction, ItemSetFilter, Tier, side_chance};
use poe1::data::{Data, Rarity, Side};
use poe1::item::{Item, ItemSpec};
use poe1::method::{Pool, SideReq};
use poe1::pattern::{Pattern, PatternSpec};

/// The owner's shield, normal to begin with.
fn shield(data: &Data) -> Item {
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "normal",
            "influences": ["shaper"],
            "sockets": "G-W W"
        }"#,
    )
    .expect("spec");
    Item::from_spec(data, &spec).expect("the shield")
}

/// The modifiers of one side that a single draw on this item can reach, by weight.
fn pool(data: &Data, item: &Item, side: Side) -> Vec<(u32, u128)> {
    draw_weights(data, item, &Pool::Natural, SideReq::Any, false, false)
        .into_iter()
        .filter(|&(m, w)| data.mod_at(m).side == Some(side) && w > 0)
        .collect()
}

/// The goal, as the owner wrote it: the same fixture the export's tests use, compiled against
/// simple's data rather than a filter built here.
fn pattern(data: &Data) -> Pattern {
    let path = common::root().join("fixtures/patterns/totem-shield.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let spec: PatternSpec = serde_json::from_str(&text).expect("pattern spec");
    Pattern::compile(data, &spec).expect("the pattern compiles against simple's data")
}

/// The pool is the export's: 73 prefixes and 131 suffixes, the published weights ×100 (data's
/// per-100 generation factor, which cancels in every ratio).
#[test]
fn the_pool_is_the_published_pool() {
    let data = common::simple_data();
    let item = shield(data);
    let prefix = pool(data, &item, Side::Prefix);
    let suffix = pool(data, &item, Side::Suffix);
    assert_eq!(prefix.len(), 73);
    assert_eq!(suffix.len(), 131);
    let weight = |p: &[(u32, u128)]| p.iter().map(|&(_, w)| w).sum::<u128>() / 100;
    assert_eq!(weight(&prefix), 64_800);
    assert_eq!(weight(&suffix), 96_475);
}

/// One draw on the magic item: the totem prefix at 800/64 800, the good prefixes at 4 000/64 800,
/// and the good suffixes at 14 600/96 475 — the export's numbers, from simple's data.
#[test]
fn the_totem_shield_chances() {
    let data = common::simple_data();
    let mut magic = shield(data);
    magic.rarity = Rarity::Magic;
    let filter = ItemSetFilter::from_pattern(&pattern(data), Accept::DesiredOrGoodBothSides);
    let share = |side: Side, tier: Tier| {
        side_chance(data, &magic, side, |m, _| {
            filter.tier_of_mod(data, m) == tier
        })
        .expect("a side with a pool")
    };
    assert_eq!(
        share(Side::Prefix, Tier::Desired),
        Fraction::new(1, 81).unwrap()
    );
    assert_eq!(
        share(Side::Prefix, Tier::Good),
        Fraction::new(5, 81).unwrap()
    );
    assert_eq!(share(Side::Suffix, Tier::Desired), Fraction::ZERO);
    assert_eq!(
        share(Side::Suffix, Tier::Good),
        Fraction::new(584, 3859).unwrap()
    );
}
