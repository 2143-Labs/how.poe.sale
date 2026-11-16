//! The baseline: the shield's pool, the three chances, a sample item's value ranges, and six steps of
//! the loop.
//!
//! Run with no arguments for six steps, or `--depth N` for another number. Every number is computed
//! from `data/shield.json`; nothing here is hard-coded.
//!
//! Three kinds of number are printed, and they are not the same kind of thing:
//!
//! - the pool and the three chances are exact [`Fraction`]s of integer weights, with a percentage
//!   beside them for reading;
//! - `values` are intervals, because this crate never rolls a die: a fresh affix's stat is somewhere
//!   in its range, and intervals add exactly;
//! - the table is a mass — `f64` shares of the distribution, because an exact sum over thousands of
//!   distinct pool totals does not fit in 128 bits. See `craft::machine` for what is exact and what
//!   is not.

use simple::craft::{Coarse, LoopPolicy, Stage};
use simple::data::data;
use simple::filter::{Accept, ItemSetFilter, Tier};
use simple::item::{Affix, Influence, Item, Rarity, Side};
use simple::pool::{draw_weights, side_chance};
use simple::target::Target;

/// How many steps the loop takes unless `--depth` says otherwise.
const DEPTH: usize = 6;

/// The item level the craft runs at: high enough for every modifier this base can roll.
const ITEM_LEVEL: u8 = 87;

fn main() {
    let depth = arguments();
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

    let filter = ItemSetFilter::from_data(data, Accept::DesiredOrGoodBothSides);
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

    let target = Target::from_data(data);
    let mut machine = Coarse::new(
        data,
        &filter,
        &target,
        base,
        ITEM_LEVEL,
        Some(Influence::Shaper),
    );
    let policy = LoopPolicy::new(&target);
    print!("{:>5}", "depth");
    for stage in Stage::ALL {
        print!(" {:>11}", stage.name());
    }
    println!(" {:>11}", "goal");
    for depth in 1..=depth {
        machine
            .step(&policy)
            .expect("the loop's moves are modellable");
        let row = machine.history().last().expect("a step was taken");
        print!("{depth:>5}");
        for stage in Stage::ALL {
            print!(" {:>11}", format!("{:.6}", percent(row, stage)));
        }
        println!(" {:>11}", format!("{:.6}", machine.goal() * 100.0));
    }
    println!();
    println!("goal {:.6}%", machine.goal() * 100.0);
    println!("unabsorbed {:.6}%", machine.unabsorbed() * 100.0);
    println!(
        "target {:.6}% of the mass is the item the craft is for",
        machine.reached() * 100.0
    );
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

/// The one argument the baseline takes: how many steps of the loop to print.
fn arguments() -> usize {
    let mut depth = DEPTH;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--depth" => {
                depth = args
                    .next()
                    .and_then(|count| count.parse().ok())
                    .expect("--depth takes a number of steps");
            }
            other => panic!("{other} is not an argument of the baseline; it takes --depth N"),
        }
    }
    depth
}

/// The side's whole pool, which is the denominator of every chance on it.
fn pool_total(pool: &[(u16, u128)]) -> u128 {
    pool.iter().map(|&(_, weight)| weight).sum()
}

/// One stage's share of the step, in percent.
fn percent(row: &[(Stage, f64)], stage: Stage) -> f64 {
    row.iter()
        .find(|(label, _)| *label == stage)
        .map(|(_, mass)| *mass * 100.0)
        .unwrap_or(0.0)
}
