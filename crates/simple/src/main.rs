//! The baseline: the shield's pool, the three chances, and a sample item's value ranges.
//!
//! Every number is computed from `data/shield.json`; nothing here is hard-coded.
//!
//! Two kinds of number are printed, and they are not the same kind of thing:
//!
//! - the pool and the three chances are exact [`Fraction`]s of integer weights, with a percentage
//!   beside them for reading;
//! - `values` are intervals, because this crate never rolls a die: a fresh affix's stat is somewhere
//!   in its range, and intervals add exactly.

use simple::data::data;
use simple::filter::{ItemSetFilter, Tier};
use simple::item::{Affix, Influence, Item, Rarity, Side};
use simple::pool::{draw_weights, side_chance};

/// The item level the craft runs at: high enough for every modifier this base can roll.
const ITEM_LEVEL: u8 = 87;

fn main() {
    let data = data();
    let base = data.base_of("Supreme Spiked Shield");
    let influenced =
        Item::of_base(data, base, ITEM_LEVEL, Rarity::Magic).with_influence(Influence::Shaper);
    let plain = Item::of_base(data, base, ITEM_LEVEL, Rarity::Magic);

    // The same base rolls two different pools: the Shaper influence adds its tag, and with it the
    // eleven modifiers only an influenced shield can roll.
    for (label, item) in [("(shaper)", &influenced), ("(no influence)", &plain)] {
        let prefix = draw_weights(data, item, Side::Prefix);
        let suffix = draw_weights(data, item, Side::Suffix);
        println!(
            "pool {:<55} prefixes {:6} {:>7}   suffixes {:6} {:>8}",
            label,
            pool_total(&prefix),
            prefix.len(),
            pool_total(&suffix),
            suffix.len()
        );
    }
    println!();

    let filter = ItemSetFilter::from_data(data);
    for (label, side, tier) in [
        ("desired prefix", Side::Prefix, Tier::Desired),
        ("good prefix", Side::Prefix, Tier::Good),
        ("good suffix", Side::Suffix, Tier::Good),
    ] {
        let chance = side_chance(data, &influenced, side, |m| filter.tier_of(m) == tier)
            .expect("a fresh magic shield draws on both sides");
        println!("{label} {chance} = {:.4}%", chance.percent());
    }
    println!();

    // What the model knows about a stat's value: the range the mod can roll, summed over the mods
    // that contribute to it. A fresh affix is carried at its whole range, because the crate never
    // rolls a die.
    let sample = sample_item(data);
    print!("values");
    for (stat, roll) in sample.stat_totals(data) {
        print!("  {} {roll}", data.stat_id(stat));
    }
    println!();
    println!();
}

/// A rare shield holding the base's implicit plus two modifiers, for the value line.
fn sample_item(data: &simple::data::Data) -> Item {
    let base = data.base_of("Supreme Spiked Shield");
    let mut item =
        Item::of_base(data, base, ITEM_LEVEL, Rarity::Rare).with_influence(Influence::Shaper);
    for (side, id) in [
        (Side::Prefix, "IncreasedLife10"),
        (Side::Suffix, "AllResistances6"),
    ] {
        let m = data
            .id_of(id)
            .unwrap_or_else(|| panic!("{id} is not in the pool"));
        item.affixes_mut(side).push(Affix::at_range(data, m));
    }
    item
}

/// The side's whole pool, which is the denominator of every chance on it.
fn pool_total(pool: &[(u16, u128)]) -> u128 {
    pool.iter().map(|&(_, weight)| weight).sum()
}
