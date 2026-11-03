//! V-A7: the triple T1 physical Ezomyte Staff from r/pathofexile 1x9400l, computed by the engine.
//!
//! Alt magic staves until each holds one T1 mod (three physical prefixes, two crit suffixes),
//! recombine 1p + 1s into 1p1s, two 1p1s into 2p2s, and two 2p2s sharing the flat mod into three
//! physical prefixes. Every step's odds except `two_with_flat` are independent of α: each step keeps
//! every distinct mod whenever enough mods are kept.

mod common;

use common::*;
use poe1::item::{Rarity, Side};
use poe1::kernels::{self, RecombineRules};
use poe1::market::{self, Market};
use poe1::target::Target;
use poe1::weights::{PoolFilter, pool};

const STAFF: &str = "Ezomyte Staff";
const PHYS: &str = "LocalIncreasedPhysicalDamagePercent8";
const HYBRID: &str = "LocalIncreasedPhysicalDamagePercentAndAccuracyRating8";
const FLAT: &str = "LocalAddedPhysicalDamageTwoHand9";
const CRIT: &str = "LocalCriticalStrikeChance6";
const MULTI: &str = "LocalCriticalMultiplier6";

fn staff(rarity: Rarity, ids: &[&str]) -> Target {
    target(STAFF, 84, rarity, ids.iter().map(|id| req(&[id])).collect())
}

/// The chain as the post describes it, plus `crit`: the same top recombine wanting both crits too.
fn chain() -> (Market, std::collections::BTreeMap<String, f64>) {
    use Rarity::*;
    let mut goods = vec![
        good("main", staff(Rare, &[PHYS, HYBRID, FLAT]), None, None),
        good(
            "crit",
            staff(Rare, &[PHYS, HYBRID, FLAT, CRIT, MULTI]),
            None,
            None,
        ),
        good("a2", staff(Rare, &[PHYS, FLAT, CRIT, MULTI]), None, None),
        good("b2", staff(Rare, &[HYBRID, FLAT, CRIT, MULTI]), None, None),
        good("pc", staff(Magic, &[PHYS, CRIT]), None, None),
        good("hc", staff(Magic, &[HYBRID, CRIT]), None, None),
        good("fm", staff(Magic, &[FLAT, MULTI]), None, None),
        good("normal", staff(Normal, &[]), Some(1.0), None),
    ];
    let mut processes = vec![
        recombine("r-main", "main", "a2", "b2"),
        recombine("r-crit", "crit", "a2", "b2"),
        recombine("r-a2", "a2", "pc", "fm"),
        recombine("r-b2", "b2", "hc", "fm"),
        recombine("r-pc", "pc", "phys", "crit1"),
        recombine("r-hc", "hc", "hybrid", "crit1"),
        recombine("r-fm", "fm", "flat", "multi"),
    ];
    for (id, m) in [
        ("phys", PHYS),
        ("hybrid", HYBRID),
        ("flat", FLAT),
        ("crit1", CRIT),
        ("multi", MULTI),
    ] {
        goods.push(good(id, staff(Magic, &[m]), None, None));
        processes.push(craft(
            &format!("alt-{id}"),
            id,
            "normal",
            &["transmute", "alteration"],
        ));
    }
    (Market::new(goods, processes, "main").unwrap(), prices())
}

#[test]
fn va7_triple_t1_phys_warstaff() {
    let b = b();
    let (m, prices) = chain();
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    let p = |id: &str| sol.processes.iter().find(|x| x.id == id).unwrap();
    let v = |id: &str| {
        sol.goods
            .iter()
            .find(|x| x.id == id)
            .unwrap()
            .value
            .unwrap()
    };
    for x in &sol.processes {
        println!(
            "{:>10}  p {:.6}  cost {:>12.2}",
            x.id,
            x.p_success.unwrap_or(f64::NAN),
            x.cost.unwrap_or(f64::NAN)
        );
    }

    // Alt-spam: every transmute and alteration rolls 1 or 2 mods from the whole magic pool.
    let tags = b.item_tags(base(STAFF), 0);
    let side = |side| {
        pool(
            b,
            &PoolFilter {
                side,
                tags: &tags,
                ilvl: 84,
                excluded_groups: &[],
                influence_only: false,
                catalyst: None,
            },
        )
    };
    let (pre, suf) = (side(Side::Prefix), side(Side::Suffix));
    let total: f64 = pre.iter().chain(&suf).map(|e| e.weight).sum();
    let weight = |id: &str| {
        pre.iter()
            .chain(&suf)
            .find(|e| e.mod_index == mod_index(id))
            .unwrap()
            .weight
    };
    let p_total: f64 = pre.iter().map(|e| e.weight).sum();
    let s_total: f64 = suf.iter().map(|e| e.weight).sum();
    println!("magic pool total weight {total} ({p_total} prefix, {s_total} suffix)");
    for (good, id, w) in [
        ("phys", PHYS, 25.0),
        ("hybrid", HYBRID, 25.0),
        ("flat", FLAT, 200.0),
        ("crit1", CRIT, 1000.0),
        ("multi", MULTI, 1000.0),
    ] {
        assert_eq!(weight(id), w, "{id}");
        // 1 or 2 mods, 1/2 each: a hit on the first draw, or on the second once the other side's
        // only slot is full.
        let prefix = pre.iter().any(|e| e.mod_index == mod_index(id));
        let other = if prefix { s_total } else { p_total };
        let own = if prefix { p_total } else { s_total };
        let q = 0.5 * w / total + 0.5 * (w / total + (other / total) * (w / own));
        let closed =
            1.0 + market::TRADE_PENALTY + prices["transmute"] + (1.0 - q) * prices["alt"] / q;
        println!("{good}: 1 in {:.0} alterations, V {:.2}", 1.0 / q, v(good));
        assert!(close(v(good), closed, 1e-9), "{good}");
    }

    // 1p + 1s: prefix only, suffix only or both, 1/3 each.
    let one_one = 1.0 / 3.0;
    for id in ["r-pc", "r-hc", "r-fm"] {
        assert!(close(p(id).p_success.unwrap(), one_one, 1e-9), "{id}");
    }
    // 1p1s + 1p1s: both of two on each side, 33/100 each.
    let two_two = (33.0 / 100.0) * (33.0 / 100.0);
    for id in ["r-a2", "r-b2"] {
        assert!(close(p(id).p_success.unwrap(), two_two, 1e-9), "{id}");
    }
    // 2p + 2p sharing the flat mod: a pool of 4, and 3 kept (31/101) is always the three distinct
    // groups, since a picked mod makes its duplicate ineligible.
    let three = 31.0 / 101.0;
    assert!(close(p("r-main").p_success.unwrap(), three, 1e-9));
    // Both crit suffixes from {crit, multi} x2: 2 or 3 kept (90/101) always holds both.
    assert!(close(
        p("r-crit").p_success.unwrap(),
        three * 90.0 / 101.0,
        1e-9
    ));

    // Costs compose as (inputs + one combine's Dust and Gold) / p with no brick credit.
    let r = 100.0 * prices["thaumaturgic-dust-1k"] + 12.0 * prices["gold-1k"];
    for (out, a, c, q) in [
        ("pc", "phys", "crit1", one_one),
        ("hc", "hybrid", "crit1", one_one),
        ("fm", "flat", "multi", one_one),
        ("a2", "pc", "fm", two_two),
        ("b2", "hc", "fm", two_two),
        ("main", "a2", "b2", three),
    ] {
        assert!(close(v(out), (v(a) + v(c) + r) / q, 1e-9), "{out}");
    }
    println!(
        "V(main) {:.2}, V(crit) {:.2}, expected magic staves per main {:.1}",
        v("main"),
        v("crit"),
        // Two inputs per attempt at every level: 2p2s per main, 1p1s per 2p2s, magic per 1p1s.
        (2.0 / three) * (2.0 / two_two) * (2.0 / one_one)
    );

    // Dearer Gold raises every recombined good.
    let (m, mut dear) = chain();
    dear.insert("gold-1k".into(), 1.0);
    let up = market::solve(b, &m, &dear, &[]).unwrap();
    assert!(up.goods[0].value.unwrap() > v("main"));
}

/// The two rules the post's last step depends on, on the kernel directly.
#[test]
fn recombine_duplicates_and_magic_inputs() {
    let b = b();
    let rules = RecombineRules::plan();
    let t = staff(Rarity::Rare, &[PHYS, HYBRID, FLAT, CRIT, MULTI]);
    let (phys, hybrid, flat, crit) = (0, 1, 2, 3);

    // {phys, flat} + {hybrid, flat}: 3 of 4 kept (31/101) is always all three groups.
    let a = state(&t, Rarity::Rare, &[phys, flat]);
    let c = state(&t, Rarity::Rare, &[hybrid, flat]);
    let outs = kernels::recombine(b, &t, &a, &c, &rules);
    let three: f64 = outs
        .iter()
        .filter(|(o, _)| o.prefixes.wanted.len() == 3)
        .map(|(_, p)| p)
        .sum();
    assert!(close(three, 31.0 / 101.0, 1e-12), "{three}");
    // Never the same mod twice.
    for (o, _) in &outs {
        let mut reqs: Vec<u16> = o.prefixes.wanted.iter().map(|h| h.req).collect();
        reqs.dedup();
        assert_eq!(reqs.len(), o.prefixes.wanted.len());
    }
    // With 2 kept (59/101), flat is missed only by drawing phys and hybrid. The pool is phys,
    // flat, hybrid, flat in input order, and a drawn flat makes the other one ineligible.
    let w = |id: &str| weight_on(STAFF, id);
    let pool_w = [w(PHYS), w(FLAT), w(HYBRID), w(FLAT)];
    let groups = [0, 1, 2, 1];
    let flat_in_two = |alpha: f64| -> f64 {
        ordered_draws(&pool_w, &groups, 2, alpha)
            .iter()
            .filter(|(seq, _)| seq.iter().any(|&i| groups[i] == 1))
            .map(|(_, p)| p)
            .sum()
    };
    assert!(close(flat_in_two(0.0), 5.0 / 6.0, 1e-12));
    let two_with_flat = |outs: &[(poe1::item::ItemState, f64)]| -> f64 {
        outs.iter()
            .filter(|(o, _)| {
                o.prefixes.wanted.len() == 2 && o.prefixes.wanted.iter().any(|h| h.req == 2)
            })
            .map(|(_, p)| p)
            .sum()
    };
    let got = two_with_flat(&outs);
    let want = 59.0 / 101.0 * flat_in_two(rules.alpha);
    assert!(close(got, want, 1e-12), "{got} vs {want}");
    let uniform = RecombineRules {
        alpha: 0.0,
        ..rules
    };
    let got = two_with_flat(&kernels::recombine(b, &t, &a, &c, &uniform));
    assert!(close(got, 59.0 / 101.0 * 5.0 / 6.0, 1e-12), "{got}");

    // One prefix against one suffix: prefix only, suffix only or both, 1/3 each, all magic, never
    // an empty item, whatever the inputs' rarities.
    let a = state(&t, Rarity::Magic, &[phys]);
    for c in [
        state(&t, Rarity::Magic, &[crit]),
        state(&t, Rarity::Rare, &[crit]),
    ] {
        let mut shapes: std::collections::BTreeMap<(usize, usize), f64> = Default::default();
        for (o, p) in kernels::recombine(b, &t, &a, &c, &rules) {
            assert_eq!(o.rarity, Rarity::Magic);
            *shapes
                .entry((o.prefixes.wanted.len(), o.suffixes.wanted.len()))
                .or_default() += p;
        }
        assert_eq!(
            shapes.keys().copied().collect::<Vec<_>>(),
            vec![(0, 1), (1, 0), (1, 1)]
        );
        for p in shapes.values() {
            assert!(close(*p, 1.0 / 3.0, 1e-12), "{shapes:?}");
        }
    }
    // Two magic 1p items: both kept is a rare.
    let c = state(&t, Rarity::Magic, &[hybrid]);
    for (o, _) in kernels::recombine(b, &t, &a, &c, &rules) {
        let want = if o.prefixes.wanted.len() == 2 {
            Rarity::Rare
        } else {
            Rarity::Magic
        };
        assert_eq!(o.rarity, want);
    }
    // A normal input is still refused.
    let normal = state(&t, Rarity::Normal, &[]);
    assert!(kernels::recombine(b, &t, &normal, &a, &rules).is_empty());
}
