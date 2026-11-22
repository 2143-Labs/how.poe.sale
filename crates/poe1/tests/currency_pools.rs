//! The draw helpers take the pool from the calling method. A currency whose method declares an
//! influence pool must reach that influence's modifiers, and a report must be able to price that
//! same pool exactly — neither is possible if the helpers are hard-wired to the natural pool.

use std::collections::BTreeSet;

use poe1::currency::{one_add, side_chance};
use poe1::data::{Data, Influence, Rarity, Side};
use poe1::item::{Item, ItemSpec};
use poe1::method::{Pool, SideReq};

/// The owner's Shaper shield at item level 87, magic so a prefix draw is a single explicit.
fn shield(data: &Data) -> Item {
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "magic",
            "influences": ["shaper"],
            "sockets": "G-W W"
        }"#,
    )
    .expect("spec");
    let item = Item::from_spec(data, &spec).expect("the shield");
    assert_eq!(item.rarity, Rarity::Magic);
    item
}

/// The ids one prefix draw can add, so two pools compare by what they actually offer.
fn drawn_ids<'d>(data: &'d Data, item: &Item, pool: &Pool) -> BTreeSet<&'d str> {
    one_add(data, item, pool, SideReq::Prefix)
        .expect("the pool's weights fit in 128 bits")
        .into_iter()
        .map(|(_, _, added)| {
            let (side, m) = added.expect("a prefix draw adds a modifier");
            assert_eq!(side, Side::Prefix);
            data.mod_at(m).id.as_str()
        })
        .collect()
}

#[test]
fn an_influence_pool_draws_only_that_influences_prefixes() {
    let data = poe1::test_data();
    let item = shield(data);
    let shaper = Pool::Influence {
        influence: Influence::Shaper,
    };

    let drawn =
        one_add(data, &item, &shaper, SideReq::Prefix).expect("the pool's weights fit in 128 bits");
    assert!(!drawn.is_empty(), "the Shaper pool has prefixes to draw");
    for (_, _, added) in &drawn {
        let (side, m) = added.expect("a prefix draw adds a modifier");
        assert_eq!(side, Side::Prefix);
        assert_eq!(
            data.mod_at(m).influence,
            Some(Influence::Shaper),
            "only the pool's influence may be drawn"
        );
    }
    assert!(
        drawn_ids(data, &item, &shaper).contains("MaximumTotemUber1"),
        "the Shaper totem prefix is in the Shaper pool"
    );
}

#[test]
fn the_natural_pool_is_a_strict_superset_of_the_shaper_pool() {
    let data = poe1::test_data();
    let item = shield(data);
    let natural = drawn_ids(data, &item, &Pool::Natural);
    let shaper = drawn_ids(
        data,
        &item,
        &Pool::Influence {
            influence: Influence::Shaper,
        },
    );

    // Every Shaper modifier is a natural modifier; the natural pool also holds the uninfluenced
    // ones, so the containment is strict in both directions.
    assert!(shaper.is_subset(&natural));
    assert!(natural.len() > shaper.len());
}

#[test]
fn side_chance_prices_the_influence_pool_exactly() {
    let data = poe1::test_data();
    let item = shield(data);
    let shaper = Pool::Influence {
        influence: Influence::Shaper,
    };
    let totem = data.mod_by_id("MaximumTotemUber1").expect("the totem mod");

    let chance = side_chance(data, &item, &shaper, Side::Prefix, |m, _| m == totem)
        .expect("the Shaper pool has a prefix side");
    assert!(!chance.is_zero(), "the totem modifier has a weight");
    // The natural pool holds strictly more prefixes, so the same weight is a smaller share there.
    let natural = side_chance(data, &item, &Pool::Natural, Side::Prefix, |m, _| m == totem)
        .expect("the natural pool has a prefix side");
    // Compare the shares as percentages: `Fraction` has no ordering of its own.
    assert!(chance.percent() > natural.percent());
}
