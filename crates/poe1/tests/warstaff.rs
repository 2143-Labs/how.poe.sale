//! V-A7: the triple T1 physical Ezomyte Staff from r/pathofexile 1x9400l, computed by the engine.
//!
//! Alt magic staves until each holds one T1 mod (three physical prefixes, two crit suffixes),
//! recombine 1p + 1s into 1p1s, two 1p1s into 2p2s, and two 2p2s sharing the flat mod into three
//! physical prefixes. Every step's odds here are independent of how the recombinator weights its
//! picks: each step keeps every distinct mod whenever enough mods are kept.

mod common;

use common::*;
use poe1::item::{Rarity, Side};
use poe1::kernels;
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
fn chain(recombiner: f64) -> (Market, std::collections::BTreeMap<String, f64>) {
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
    let mut prices = prices();
    prices.insert("recombiner".into(), recombiner);
    (Market::new(goods, processes, "main").unwrap(), prices)
}

#[test]
fn va7_triple_t1_phys_warstaff() {
    let b = b();
    let (m, prices) = chain(1.0);
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

    // Alt-spam: one draw from the whole magic pool per alteration.
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
    println!("magic pool total weight {total}");
    for (good, id, w) in [
        ("phys", PHYS, 25.0),
        ("hybrid", HYBRID, 25.0),
        ("flat", FLAT, 200.0),
        ("crit1", CRIT, 1000.0),
        ("multi", MULTI, 1000.0),
    ] {
        assert_eq!(weight(id), w, "{id}");
        let q = w / total;
        let closed =
            1.0 + market::TRADE_PENALTY + prices["transmute"] + (1.0 - q) * prices["alt"] / q;
        println!("{good}: 1 in {:.0} alterations, V {:.2}", 1.0 / q, v(good));
        assert!(close(v(good), closed, 1e-9), "{good}");
    }

    // 1p + 1s: each side keeps its one mod with 59%.
    for id in ["r-pc", "r-hc", "r-fm"] {
        assert!(close(p(id).p_success.unwrap(), 0.59 * 0.59, 1e-9), "{id}");
    }
    // 1p1s + 1p1s: both of two on each side, 1/3 each.
    for id in ["r-a2", "r-b2"] {
        assert!(close(p(id).p_success.unwrap(), 1.0 / 9.0, 1e-9), "{id}");
    }
    // 2p + 2p sharing the flat mod: a pool of 4, and 3 kept (30%) is always the three distinct
    // groups, since a picked mod makes its duplicate ineligible.
    assert!(close(p("r-main").p_success.unwrap(), 0.30, 1e-9));
    // Both crit suffixes from {crit, multi} x2: 2 or 3 kept (90%) always holds both.
    assert!(close(p("r-crit").p_success.unwrap(), 0.30 * 0.90, 1e-9));

    // Costs compose as (inputs + recombiner) / p with no brick credit.
    let r = prices["recombiner"];
    for (out, a, c, q) in [
        ("pc", "phys", "crit1", 0.59 * 0.59),
        ("hc", "hybrid", "crit1", 0.59 * 0.59),
        ("fm", "flat", "multi", 0.59 * 0.59),
        ("a2", "pc", "fm", 1.0 / 9.0),
        ("b2", "hc", "fm", 1.0 / 9.0),
        ("main", "a2", "b2", 0.30),
    ] {
        assert!(close(v(out), (v(a) + v(c) + r) / q, 1e-9), "{out}");
    }
    println!(
        "V(main) {:.2}, V(crit) {:.2}, expected magic staves per main {:.1}",
        v("main"),
        v("crit"),
        // Two inputs per attempt at every level: 2p2s per main, 1p1s per 2p2s, magic per 1p1s.
        (2.0 / 0.30) * (2.0 * 9.0) * (2.0 / (0.59 * 0.59))
    );

    // A dearer recombination raises every recombined good.
    let (m, dear) = chain(10.0);
    let up = market::solve(b, &m, &dear, &[]).unwrap();
    assert!(up.goods[0].value.unwrap() > v("main"));
}

/// The two rules the post's last step depends on, on the kernel directly.
#[test]
fn recombine_duplicates_and_magic_inputs() {
    let b = b();
    let t = staff(Rarity::Rare, &[PHYS, HYBRID, FLAT, CRIT, MULTI]);
    let (phys, hybrid, flat, crit) = (0, 1, 2, 3);

    // {phys, flat} + {hybrid, flat}: 3 of 4 kept is always all three groups.
    let a = state(&t, Rarity::Rare, &[phys, flat]);
    let c = state(&t, Rarity::Rare, &[hybrid, flat]);
    let outs = kernels::recombine(b, &t, &a, &c);
    let three: f64 = outs
        .iter()
        .filter(|(o, _)| o.prefixes.wanted.len() == 3)
        .map(|(_, p)| p)
        .sum();
    assert!(close(three, 0.30, 1e-12), "{three}");
    // Never the same mod twice.
    for (o, _) in &outs {
        let mut reqs: Vec<u16> = o.prefixes.wanted.iter().map(|h| h.req).collect();
        reqs.dedup();
        assert_eq!(reqs.len(), o.prefixes.wanted.len());
    }
    // With 2 kept, flat is missed only by drawing phys then hybrid or the reverse:
    // 2 * (1/4 * 1/3) = 1/6, so P(flat kept | 2 kept) = 5/6.
    let two_with_flat: f64 = outs
        .iter()
        .filter(|(o, _)| {
            o.prefixes.wanted.len() == 2 && o.prefixes.wanted.iter().any(|h| h.req == 2)
        })
        .map(|(_, p)| p)
        .sum();
    assert!(
        close(two_with_flat, 0.60 * 5.0 / 6.0, 1e-12),
        "{two_with_flat}"
    );

    // Magic 1p + magic 1s: magic 1p1s with 0.59^2, magic with one mod, or normal with none.
    let a = state(&t, Rarity::Magic, &[phys]);
    let c = state(&t, Rarity::Magic, &[crit]);
    let mut by_rarity = [0.0; 3];
    for (o, p) in kernels::recombine(b, &t, &a, &c) {
        let n = o.prefixes.wanted.len() + o.suffixes.wanted.len();
        let want = match n {
            0 => Rarity::Normal,
            _ => Rarity::Magic,
        };
        assert_eq!(o.rarity, want, "{n} mods");
        by_rarity[n] += p;
    }
    assert!(close(by_rarity[2], 0.59 * 0.59, 1e-12));
    assert!(close(by_rarity[0], 0.41 * 0.41, 1e-12));
    // Two magic 1p items: both kept is a rare.
    let c = state(&t, Rarity::Magic, &[hybrid]);
    for (o, _) in kernels::recombine(b, &t, &a, &c) {
        let want = if o.prefixes.wanted.len() == 2 {
            Rarity::Rare
        } else {
            Rarity::Magic
        };
        assert_eq!(o.rarity, want);
    }
    // With a rare input the result is rare, whatever it keeps.
    let rare = state(&t, Rarity::Rare, &[crit]);
    for (o, _) in kernels::recombine(b, &t, &a, &rare) {
        assert_eq!(o.rarity, Rarity::Rare);
    }
    // A normal input is still refused.
    let normal = state(&t, Rarity::Normal, &[]);
    assert!(kernels::recombine(b, &t, &normal, &a).is_empty());
}
