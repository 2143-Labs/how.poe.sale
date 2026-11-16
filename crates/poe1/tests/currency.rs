//! The per-currency model: one step at a time, with exact chances.

use poe1::apply::draw_weights;
use poe1::currency::{
    Accept, Currency, Fraction, ItemOutputSet, ItemSetFilter, ModSelector, Tier, Transmutation,
    side_chance,
};
use poe1::data::{Data, Rarity, Side};
use poe1::item::{Item, ItemSpec};
use poe1::method::{Pool, SideReq};

/// The owner's shield: a Shaper Supreme Spiked Shield, item level 87, normal to begin with.
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

/// A modifier's printed text with every number blanked, so a family can be named by what it says
/// rather than by an id: `+(145-159) to maximum Life` and `+(3-9) to maximum Life` both become
/// `+# to maximum Life`.
fn key(text: &str) -> String {
    let mut out = String::new();
    let mut last_hash = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !last_hash {
                out.push('#');
                last_hash = true;
            }
        } else {
            last_hash = false;
            out.push(c);
        }
    }
    out.replace("#-#", "#")
}

/// The modifiers of one side that a single draw on this item can reach, by weight.
fn pool(data: &Data, item: &Item, side: Side) -> Vec<(u32, u128)> {
    draw_weights(data, item, &Pool::Natural, SideReq::Any, false, false)
        .into_iter()
        .filter(|&(m, w)| data.mod_at(m).side == Some(side) && w > 0)
        .collect()
}

/// The best `n` tiers of the family whose printed text is `text`, as exact selectors.
fn tiers(data: &Data, pool: &[(u32, u128)], side: Side, text: &str, n: usize) -> Vec<ModSelector> {
    let mut members: Vec<u32> = pool
        .iter()
        .map(|&(m, _)| m)
        .filter(|&m| key(&data.mod_at(m).text) == key(text))
        .collect();
    members.sort_by_key(|&m| std::cmp::Reverse(data.mod_at(m).level));
    assert!(
        members.len() >= n,
        "{text:?} has {} tiers, wanted {n}",
        members.len()
    );
    members
        .into_iter()
        .take(n)
        .map(|m| ModSelector::exact(side, m))
        .collect()
}

fn family_weight(data: &Data, pool: &[(u32, u128)], text: &str) -> u128 {
    pool.iter()
        .filter(|&&(m, _)| key(&data.mod_at(m).text) == key(text))
        .map(|&(_, w)| w)
        .sum()
}

#[test]
fn a_step_is_an_exact_distribution() {
    let data = poe1::test_data();
    let item = shield(data);
    let set = Transmutation::new(data).apply(&item, &None);
    assert!(set.exact(), "weights must sum to the total");
    let one = set.chance(|i| i.explicits().count() == 1).expect("mass");
    let two = set.chance(|i| i.explicits().count() == 2).expect("mass");
    assert_eq!(
        one.checked_add(two).unwrap(),
        Fraction::ONE,
        "the set is whole"
    );
    assert_eq!(one, Fraction::new(1, 2).unwrap(), "the branches are 50/50");
}

#[test]
fn a_currency_refuses_what_it_cannot_do() {
    let data = poe1::test_data();
    let item = shield(data);
    let set = poe1::currency::Alteration::new(data).apply(&item, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(_)),
        "alteration needs a magic item"
    );
}

#[test]
fn the_pool_is_the_published_pool() {
    let data = poe1::test_data();
    let item = shield(data);
    let prefix = pool(data, &item, Side::Prefix);
    let suffix = pool(data, &item, Side::Suffix);
    let prefix_total: u128 = prefix.iter().map(|&(_, w)| w).sum();
    let suffix_total: u128 = suffix.iter().map(|&(_, w)| w).sum();
    // Every family total below is the row the published table gives for this base at item level
    // 87. Weights carry data's per-100 generation factor, so they are the published figures × 100.
    assert_eq!(
        family_weight(data, &prefix, "+(145-159) to maximum Life") / 100,
        11_000
    );
    assert_eq!(
        family_weight(data, &prefix, "+(69-73) to maximum Mana") / 100,
        12_000
    );
    assert_eq!(
        family_weight(data, &prefix, "+1 to maximum number of Summoned Totems") / 100,
        800
    );
    assert_eq!(
        family_weight(data, &suffix, "+(46-48)% to Fire Resistance") / 100,
        8_000
    );
    assert_eq!(prefix_total / 100, 64_800);
    assert_eq!(suffix_total / 100, 96_475);
}

#[test]
fn the_totem_shield_chances() {
    let data = poe1::test_data();
    let item = shield(data);
    let magic = {
        let mut it = item.clone();
        it.rarity = Rarity::Magic;
        it
    };
    let prefix = pool(data, &magic, Side::Prefix);
    let suffix = pool(data, &magic, Side::Suffix);

    // The filter, as the owner wrote it: one desired prefix, then the good ones by tier.
    let desired = tiers(
        data,
        &prefix,
        Side::Prefix,
        "+1 to maximum number of Summoned Totems",
        1,
    );
    let mut good = Vec::new();
    good.extend(tiers(
        data,
        &prefix,
        Side::Prefix,
        "+(145-159) to maximum Life",
        2,
    )); // t1/t2 life
    good.extend(tiers(
        data,
        &prefix,
        Side::Prefix,
        "+(69-73) to maximum Mana",
        1,
    )); // t1 mana
    good.extend(tiers(
        data,
        &prefix,
        Side::Prefix,
        "(76-81)% increased Chance to Block",
        2,
    )); // t1/t2 block
    good.extend(tiers(
        data,
        &suffix,
        Side::Suffix,
        "+(17-18)% to all Elemental Resistances",
        2,
    ));
    for resist in [
        "+(46-48)% to Fire Resistance",
        "+(46-48)% to Cold Resistance",
        "+(46-48)% to Lightning Resistance",
        "(90-109)% increased Spell Critical Strike Chance",
    ] {
        good.extend(tiers(data, &suffix, Side::Suffix, resist, 2));
    }
    good.extend(tiers(
        data,
        &suffix,
        Side::Suffix,
        "Recover (3-5)% of Energy Shield when you Block",
        1,
    ));
    good.extend(tiers(
        data,
        &suffix,
        Side::Suffix,
        "Recover (3-5)% of Life when you Block",
        1,
    ));
    for max in [
        "+3% to maximum Fire Resistance",
        "+3% to maximum Cold Resistance",
        "+3% to maximum Lightning Resistance",
        "+3% to maximum Chaos Resistance",
        "+2% to all maximum Resistances",
    ] {
        good.extend(tiers(data, &suffix, Side::Suffix, max, 1));
    }
    good.extend(tiers(
        data,
        &suffix,
        Side::Suffix,
        "(31-35)% chance to Avoid Elemental Ailments",
        1,
    ));
    good.extend(tiers(
        data,
        &suffix,
        Side::Suffix,
        "+(20-22)% chance to Suppress Spell Damage",
        2,
    ));
    let filter = ItemSetFilter::new(desired, good, Accept::DesiredOrGoodBothSides);

    let share = |side: Side, tier: Tier| {
        side_chance(data, &magic, side, |m, _| {
            filter.tier_of_mod(data, m) == tier
        })
        .expect("a side with a pool")
    };
    for side in Side::BOTH {
        let desired = share(side, Tier::Desired);
        let good = share(side, Tier::Good);
        println!(
            "{:6} draw: desired {:>9} ({:>7.4} %)   good {:>9} ({:>7.3} %)",
            side.name(),
            desired,
            desired.percent(),
            good,
            good.percent()
        );
    }
    // The numbers the model must reproduce, exactly.
    assert_eq!(
        share(Side::Prefix, Tier::Desired),
        Fraction::new(800, 64_800).unwrap()
    );
    assert_eq!(
        share(Side::Prefix, Tier::Good),
        Fraction::new(4_000, 64_800).unwrap()
    );
    assert_eq!(share(Side::Suffix, Tier::Desired), Fraction::ZERO);
    assert_eq!(
        share(Side::Suffix, Tier::Good),
        Fraction::new(14_600, 96_475).unwrap()
    );
}
