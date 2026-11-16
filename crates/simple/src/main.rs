//! The baseline: the shield's pool, the three chances the plan names, and six steps of the loop.
//!
//! Run with no arguments for six steps, or `--depth N` for another number. Every number is computed
//! from `data/shield.json`; nothing here is hard-coded. The pool and the three chances are exact [`Fraction`]s, printed with a percentage
//! beside them. The table is the loop's distribution over depths, as percentages, and it is a mass:
//! its shares are `f64` because an exact sum over thousands of distinct pool totals does not fit in
//! 128 bits — see `craft::machine` for what is exact and what is not.

use simple::craft::{Coarse, LoopPolicy, Stage};
use simple::data::data;
use simple::filter::{Accept, ItemSetFilter, Tier};
use simple::item::{Item, Rarity, Side};
use simple::pool::{draw_weights, side_chance};
use simple::target::Target;

/// How many steps the loop takes unless `--depth` says otherwise.
const DEPTH: usize = 6;

fn main() {
    let depth = arguments();
    let data = data();
    let magic = Item::new(Rarity::Magic);

    let prefixes = draw_weights(data, &magic, Side::Prefix);
    let suffixes = draw_weights(data, &magic, Side::Suffix);
    println!(
        "pool  prefixes {}  suffixes {}",
        pool_total(&prefixes),
        pool_total(&suffixes)
    );
    println!(
        "      {} prefixes, {} suffixes",
        prefixes.len(),
        suffixes.len()
    );

    let filter = ItemSetFilter::from_data(data, Accept::DesiredOrGoodBothSides);
    for (label, side, tier) in [
        ("desired prefix", Side::Prefix, Tier::Desired),
        ("good prefix", Side::Prefix, Tier::Good),
        ("good suffix", Side::Suffix, Tier::Good),
    ] {
        let chance = side_chance(data, &magic, side, |m| filter.tier_of(m) == tier)
            .expect("a fresh magic shield draws on both sides");
        println!("{label} {chance} = {:.4}%", chance.percent());
    }
    println!();

    let target = Target::from_data(data);
    let mut loop_ = Coarse::new(data, &filter, &target);
    let policy = LoopPolicy::new(&target);
    print!("{:>5}", "depth");
    for stage in Stage::ALL {
        print!(" {:>11}", stage.name());
    }
    println!(" {:>11}", "goal");
    for depth in 1..=depth {
        loop_
            .step(&policy)
            .expect("the loop's moves are modellable");
        let row = loop_.history().last().expect("a step was taken");
        print!("{depth:>5}");
        for stage in Stage::ALL {
            print!(" {:>11}", format!("{:.6}", mass(row, stage)));
        }
        println!(" {:>11}", format!("{:.6}", loop_.goal() * 100.0));
    }
    println!();
    println!("goal {:.6}%", loop_.goal() * 100.0);
    println!("unabsorbed {:.6}%", loop_.unabsorbed() * 100.0);
    println!(
        "target {:.6}% of the mass is the item the craft is for",
        loop_.reached() * 100.0
    );
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

fn mass(row: &[(Stage, f64)], stage: Stage) -> f64 {
    row.iter()
        .find(|(label, _)| *label == stage)
        .map(|(_, mass)| *mass * 100.0)
        .unwrap_or(0.0)
}
