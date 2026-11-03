//! V-M: the market of goods — chains, recombines, cycles, money loops, vocabulary, remap.

mod common;

use common::*;
use poe1::item::{Rarity, Side};
use poe1::kernels::{self, Action};
use poe1::market::{self, Market};
use poe1::target::{self, Target};

fn lightning(rarity: Rarity) -> Target {
    target(
        "Lapis Amulet",
        84,
        rarity,
        vec![req(&["GlobalLightningGemLevel1"])],
    )
}

/// P(the wanted mod) after one transmute of a normal item, and after one alteration of a magic
/// item holding one other mod, straight from the kernels.
fn alt_odds(t: &Target) -> (f64, f64) {
    let b = b();
    let hit = |outs: Vec<(poe1::item::ItemState, f64)>| -> f64 {
        outs.iter()
            .filter(|(o, _)| target::satisfied(b, t, o))
            .map(|(_, p)| p)
            .sum()
    };
    let normal = state(t, Rarity::Normal, &[]);
    let q_t = hit(kernels::outcomes(b, t, &[], &normal, &Action::Transmute));
    let mut other = state(t, Rarity::Magic, &[]);
    other.prefixes.untracked = 1;
    let q_a = hit(kernels::outcomes(b, t, &[], &other, &Action::Alteration));
    (q_t, q_a)
}

#[test]
fn alt_spam_chain_matches_the_closed_form() {
    let b = b();
    let prices = prices();
    let main = lightning(Rarity::Magic);
    let base = target("Lapis Amulet", 84, Rarity::Normal, Vec::new());
    let m = Market::new(
        vec![
            good("main", main.clone(), None, None),
            good("base", base, Some(1.0), None),
        ],
        vec![craft("p1", "main", "base", &["transmute", "alteration"])],
        "main",
    )
    .unwrap();
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    let v = sol.goods[0].value.expect("main has a value");
    let (q_t, q_a) = alt_odds(&main);
    let (t, a) = (prices["transmute"], prices["alt"]);
    // Buying the input carries the graph's per-trade penalty.
    let closed = 1.0 + market::TRADE_PENALTY + t + (1.0 - q_t) * a / q_a;
    println!("q_t {q_t} q_a {q_a} V(main) {v} closed form {closed}");
    assert_eq!(sol.goods[0].via.as_deref(), Some("p1"));
    assert!(close(v, closed, 1e-9), "V(main) {v} vs {closed}");
    assert_eq!(sol.processes[0].status, market::ProcessStatus::Ready);
}

/// Goods for a recombine of two single-prefix rares into one with both.
fn pair(brick_sell: Option<f64>, va: f64, vb: f64) -> Market {
    let reqs = distinct_prefixes("Lapis Amulet", 2);
    let both = target("Lapis Amulet", 84, Rarity::Rare, reqs.clone());
    let a = target("Lapis Amulet", 84, Rarity::Rare, vec![reqs[0].clone()]);
    let c = target("Lapis Amulet", 84, Rarity::Rare, vec![reqs[1].clone()]);
    // One of the two kept is a magic item.
    let mut one = both.clone();
    one.min_hits = 1;
    one.rarity = Rarity::Magic;
    let mut goods = vec![
        good("main", both, None, None),
        good("a", a, Some(va), None),
        good("b", c, Some(vb), None),
    ];
    if let Some(s) = brick_sell {
        goods.push(good("one", one, None, Some(s)));
    }
    Market::new(goods, vec![recombine("r", "main", "a", "b")], "main").unwrap()
}

#[test]
fn recombine_closed_form() {
    let b = b();
    let mut prices = prices();
    // One combine: 100k Thaumaturgic Dust and 12k Gold, priced per 1k.
    let r = 100.0 * prices["thaumaturgic-dust-1k"] + 12.0 * prices["gold-1k"];
    let sol = market::solve(b, &pair(None, 10.0, 20.0), &prices, &[]).unwrap();
    let rp = &sol.processes[0];
    // Both of two pooled prefixes kept: the count table's 33/100; one kept: 67/100.
    let (both, one) = (33.0 / 100.0, 67.0 / 100.0);
    assert!(close(rp.p_success.unwrap(), both, 1e-12));
    assert!(
        close(rp.cost.unwrap(), (30.0 + r) / both, 1e-12),
        "cost {:?}",
        rp.cost
    );
    assert_eq!(sol.goods[0].via.as_deref(), Some("r"));
    // A magic "1 of 2 kept" priced 30 is credited on every one-kept outcome.
    let sol = market::solve(b, &pair(Some(30.0), 10.0, 20.0), &prices, &[]).unwrap();
    let with_brick = (30.0 + r - 30.0 * one) / both;
    assert!(close(sol.processes[0].cost.unwrap(), with_brick, 1e-12));
    // Dearer Gold raises V(main).
    prices.insert("gold-1k".into(), 5.0);
    let up = market::solve(b, &pair(Some(30.0), 10.0, 20.0), &prices, &[]).unwrap();
    assert!(up.goods[0].value.unwrap() > with_brick);
}

#[test]
fn recombine_is_paid_in_dust_and_gold() {
    let b = b();
    let prices = prices();
    // At the default prices one combine is 6 chaos of Gold and 0.6667 of Dust.
    let unit: f64 = market::recombine_cost(false)
        .iter()
        .map(|(key, qty)| qty * prices[key])
        .sum();
    assert!((unit - 6.6667).abs() < 1e-4, "{unit}");
    // An influenced input costs half as much again, once per combine.
    let mut m = pair(None, 10.0, 20.0);
    m.goods[1].spec.influences = 1;
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    assert_eq!(
        sol.processes[0].steps[0].cost,
        vec![
            (market::DUST_KEY.to_string(), 150.0),
            (market::GOLD_KEY.to_string(), 18.0)
        ]
    );
    // Without a Gold price the recombine waits for one instead of costing it at 0.
    let mut unpriced = prices.clone();
    unpriced.remove("gold-1k");
    let sol = market::solve(b, &pair(None, 10.0, 20.0), &unpriced, &[]).unwrap();
    let rp = &sol.processes[0];
    assert_eq!(rp.status, market::ProcessStatus::NeedsInput);
    let error = rp.error.as_deref().unwrap_or("");
    assert!(error.contains("gold-1k"), "{error}");
}

#[test]
fn brick_paying_more_than_inputs_is_a_money_loop() {
    let b = b();
    let err =
        market::solve(b, &pair(Some(1000.0), 10.0, 20.0), &prices(), &[]).expect_err("money loop");
    assert!(err.to_string().contains("money loop"), "{err}");
}

#[test]
fn a_cycle_settles_and_reuses_crafts() {
    let b = b();
    let reqs = distinct_prefixes("Lapis Amulet", 2);
    let x = target("Lapis Amulet", 84, Rarity::Rare, vec![reqs[0].clone()]);
    let y = target("Lapis Amulet", 84, Rarity::Rare, vec![reqs[1].clone()]);
    let m = Market::new(
        vec![good("x", x, Some(1.0), None), good("y", y, None, None)],
        vec![
            craft("x-from-y", "x", "y", &["chaos"]),
            craft("y-from-x", "y", "x", &["chaos"]),
        ],
        "x",
    )
    .unwrap();
    let sol = market::solve(b, &m, &prices(), &[]).unwrap();
    // x is bought; y is made from x; making x from y never beats buying it.
    assert_eq!(sol.goods[0].value, Some(1.0));
    assert_eq!(sol.goods[0].via.as_deref(), Some("buy"));
    assert_eq!(sol.goods[1].via.as_deref(), Some("y-from-x"));
    assert!(sol.goods[1].value.unwrap() > 1.0);
    println!("cache hits {}", sol.cache_hits);
    assert!(
        sol.cache_hits > 0,
        "V-A5: an unchanged craft must not be re-solved"
    );
}

#[test]
fn goods_must_share_one_vocabulary() {
    let mut strict = req(&["GlobalLightningGemLevel1"]);
    strict.min_value = 1.0;
    let a = lightning(Rarity::Magic);
    let c = target("Lapis Amulet", 84, Rarity::Magic, vec![strict]);
    let err = Market::new(
        vec![good("a", a, None, None), good("c", c, None, None)],
        Vec::new(),
        "a",
    )
    .unwrap_err();
    assert!(err.starts_with("goods disagree on"), "{err}");
    // Different mods of one group (Lightning vs Fire gems) are one vocabulary.
    let fire = target(
        "Lapis Amulet",
        84,
        Rarity::Magic,
        vec![req(&["GlobalFireGemLevel1_"])],
    );
    let m = Market::new(
        vec![
            good("a", lightning(Rarity::Magic), None, None),
            good("f", fire, None, None),
        ],
        Vec::new(),
        "a",
    )
    .unwrap();
    assert_eq!(m.universe.requirements.len(), 1);
    assert_eq!(m.universe.requirements[0].accepted.len(), 2);
}

#[test]
fn remap_round_trips_and_untracks() {
    let b = b();
    let reqs = distinct_prefixes("Lapis Amulet", 2);
    let both = target("Lapis Amulet", 84, Rarity::Rare, reqs.clone());
    let mut swapped = both.clone();
    swapped.requirements.reverse();
    let s = state(&both, Rarity::Rare, &[0, 1]);
    let there = target::remap(b, &both, &swapped, &s);
    assert_eq!(target::remap(b, &swapped, &both, &there), s);
    let only = target("Lapis Amulet", 84, Rarity::Rare, vec![reqs[0].clone()]);
    let small = target::remap(b, &both, &only, &s);
    assert_eq!(small.prefixes.wanted.len(), 1);
    assert_eq!(small.side(Side::Prefix).untracked, 1);
}
