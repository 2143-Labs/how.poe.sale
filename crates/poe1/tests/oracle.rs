//! Oracles: odds published outside this code base, typed here from their sources, against the
//! kernels. No expected value below is computed by a kernel; each assertion names its source and
//! the patch it describes, and the tolerance is the source's own precision (±0.005 for a whole
//! percent).

mod common;

use common::*;
use poe1::item::{ItemState, Rarity};
use poe1::kernels::{self, Action, RecombineRules};

const GUIDE_325: &str =
    "https://www.reddit.com/r/pathofexile/comments/1exyavx/325_updated_guide_to_recombinators/";
const GUIDE_TRICKSTER: &str = "https://www.reddit.com/r/pathofexile/comments/1f5k4uq/guide_how_to_craft_bis_trickster_body_armour_for/";
const GUIDE_329: &str = "https://www.reddit.com/r/pathofexile/comments/1w9wdj0/";
const GUIDE_326: &str =
    "https://www.reddit.com/r/pathofexile/comments/1lfyxxd/326_recombinators_analysisguide/";
const COE_BASICS: &str = "https://www.craftofexile.com/basics";

/// A whole published percent.
const PERCENT: f64 = 0.005;

fn amulet_prefixes(n: usize) -> poe1::target::Target {
    target(
        "Lapis Amulet",
        84,
        Rarity::Rare,
        distinct_prefixes("Lapis Amulet", n),
    )
}

/// Recombine a rare holding the first `na` prefixes of `t` with one holding the next `nb`.
fn recombine_rares(t: &poe1::target::Target, na: usize, nb: usize) -> Vec<(ItemState, f64)> {
    let a = state(t, Rarity::Rare, &(0..na).collect::<Vec<_>>());
    let c = state(t, Rarity::Rare, &(na..na + nb).collect::<Vec<_>>());
    kernels::recombine(b(), t, &a, &c, &RecombineRules::plan())
}

fn p_kept(outs: &[(ItemState, f64)], n: usize) -> f64 {
    outs.iter()
        .filter(|(o, _)| o.prefixes.wanted.len() == n)
        .map(|(_, p)| p)
        .sum()
}

#[test]
fn guide_two_prefix_feeder_one_third() {
    // "33%": a rare with one prefix and a rare with another keep both.
    let got = p_kept(&recombine_rares(&amulet_prefixes(2), 1, 1), 2);
    assert!(
        (got - 0.33).abs() <= PERCENT,
        "{GUIDE_TRICKSTER} (3.25) says 33% for both of two prefixes; the kernel gives {got}"
    );
}

#[test]
fn guide_three_prefix_pool_ten_percent() {
    // "10% with only three prefixes in the pool": 2 prefixes + 1 prefix keep all three.
    let got = p_kept(&recombine_rares(&amulet_prefixes(3), 2, 1), 3);
    assert!(
        (got - 0.10).abs() <= PERCENT,
        "{GUIDE_329} (3.29) says 10% for all three of a three-prefix pool; the kernel gives {got}"
    );
}

#[test]
fn guide_count_table_rows() {
    // "Table 1: Conditional final affix number distributions", 217 to 424 recombines per row;
    // P(0..=3 kept) by pooled count 1..=6. Rows 3 and 4 sum to 101 as published.
    let published: [&[f64]; 6] = [
        &[41.0, 59.0],
        &[0.0, 67.0, 33.0],
        &[0.0, 39.0, 52.0, 10.0],
        &[0.0, 11.0, 59.0, 31.0],
        &[0.0, 0.0, 43.0, 57.0],
        &[0.0, 0.0, 28.0, 72.0],
    ];
    let t = amulet_prefixes(6);
    for (n, row) in (1..=6usize).zip(published) {
        let total: f64 = row.iter().sum();
        let outs = recombine_rares(&t, n.div_ceil(2), n / 2);
        for (k, count) in row.iter().enumerate() {
            let want = count / total;
            let got = p_kept(&outs, k);
            assert!(
                (got - want).abs() < 1e-12,
                "{GUIDE_325} (3.25), pool of {n}: P({k} kept) is {want}, the kernel gives {got}"
            );
        }
    }
}

#[test]
fn guide_one_prefix_one_suffix_three_outcomes() {
    // "only 3 outcomes": a magic 1-prefix item and a magic 1-suffix item give 1p, 1s or 1p1s.
    let mut reqs = distinct_prefixes("Lapis Amulet", 1);
    reqs.push(req(&["IncreasedCastSpeed2"]));
    let t = target("Lapis Amulet", 84, Rarity::Magic, reqs);
    let a = state(&t, Rarity::Magic, &[0]);
    let c = state(&t, Rarity::Magic, &[1]);
    let outs = kernels::recombine(b(), &t, &a, &c, &RecombineRules::plan());
    let mut shapes: Vec<((usize, usize), f64)> = Vec::new();
    for (o, p) in &outs {
        let shape = (o.prefixes.wanted.len(), o.suffixes.wanted.len());
        match shapes.iter_mut().find(|(s, _)| *s == shape) {
            Some((_, q)) => *q += p,
            None => shapes.push((shape, *p)),
        }
    }
    shapes.sort_by_key(|(s, _)| *s);
    let got: Vec<(usize, usize)> = shapes.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        got,
        vec![(0, 1), (1, 0), (1, 1)],
        "{GUIDE_325} (3.25): only 1p, 1s or 1p1s, never an empty item"
    );
    for (shape, p) in shapes {
        assert!(
            (p - 1.0 / 3.0).abs() <= PERCENT,
            "{GUIDE_325} (3.25): {shape:?} has {p}"
        );
    }
}

#[test]
fn coe_magic_orbs_one_or_two_mods() {
    // "Will turn a normal item into a magic item (with one or two random explicit mods)."
    let t = target(
        "Lapis Amulet",
        84,
        Rarity::Magic,
        vec![req(&["GlobalLightningGemLevel1"])],
    );
    let normal = ItemState::new(t.base, 84, Rarity::Normal, 0);
    let mut by_count = [0.0; 7];
    for (o, p) in kernels::outcomes(b(), &t, &[], &normal, &Action::Transmute) {
        by_count[o.mod_count() as usize] += p;
    }
    assert_eq!(by_count[0], 0.0, "{COE_BASICS} (3.29): never 0 mods");
    for (n, p) in by_count.iter().enumerate().skip(1) {
        let want = if n <= 2 { 0.5 } else { 0.0 };
        assert!(
            (p - want).abs() <= PERCENT,
            "{COE_BASICS} (3.29): one or two mods, even odds; {n} mods has {p}"
        );
    }
}

#[test]
fn low_weight_mod_kept_below_equal_chance() {
    // "For low weights it will be below this number": a rare T1 mod is kept less often than an
    // equal-chance draw would keep it. Pool: {T1 %phys, T1 hybrid} + {flat, fire} on a Jewelled
    // Foil, spawn weights 25, 25, 200 and 700.
    let ids = [
        "LocalIncreasedPhysicalDamagePercent8",
        "LocalIncreasedPhysicalDamagePercentAndAccuracyRating8",
        "LocalAddedPhysicalDamage9",
        "LocalAddedFireDamage9",
    ];
    let t = target(
        "Jewelled Foil",
        84,
        Rarity::Rare,
        ids.iter().map(|id| req(&[id])).collect(),
    );
    let a = state(&t, Rarity::Rare, &[0, 1]);
    let c = state(&t, Rarity::Rare, &[2, 3]);
    // At equal chance a pool of four keeps any one mod with Σ P(k kept)·k/4, from the 3.25
    // table's row 4 (11/59/31 of 101): (11·1 + 59·2 + 31·3) / (101·4) = 222/404.
    let equal_chance = 222.0 / 404.0;
    let keep_rate = |rules: &RecombineRules| -> f64 {
        kernels::recombine(b(), &t, &a, &c, rules)
            .iter()
            .filter(|(o, _)| o.prefixes.wanted.iter().any(|h| h.req == 0))
            .map(|(_, p)| p)
            .sum()
    };
    let plan = RecombineRules::plan();
    let weighted = keep_rate(&plan);
    println!("T1 %phys kept {weighted:.6} (equal chance {equal_chance:.6})");
    assert!(
        weighted < equal_chance,
        "{GUIDE_326} (3.26): a weight-25 mod must be kept below equal chance ({equal_chance}); the kernel gives {weighted}"
    );
    // The old uniform rule is exactly the equal-chance figure, so this oracle rejects it.
    let uniform = keep_rate(&RecombineRules { alpha: 0.0, ..plan });
    assert!(
        (uniform - equal_chance).abs() < 1e-12,
        "uniform selection keeps it {uniform}"
    );
    assert!(uniform >= equal_chance - 1e-12);
}
