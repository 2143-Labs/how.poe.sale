//! V-A1 (alt-spam amulet) and V-A2 (3x t1 physical recombine), computed by the engine.

mod common;

use std::time::Instant;

use common::*;
use poe1::item::{Rarity, Side};
use poe1::kernels::{self, Action};
use poe1::market::{self, Market, ProcessStatus};
use poe1::registry;
use poe1::target::{self, Target};
use poe1::weights::{PoolFilter, pool};

const LIGHTNING: &str = "GlobalLightningGemLevel1";
const FIRE: &str = "GlobalFireGemLevel1_";
const COLD: &str = "GlobalColdGemLevel1__";
const ALL: &str = "GlobalSkillGemLevel1";

fn amulet(rarity: Rarity, ids: &[&str]) -> Target {
    target("Lapis Amulet", 84, rarity, vec![req(ids)])
}

fn normal_amulet() -> Target {
    target("Lapis Amulet", 84, Rarity::Normal, Vec::new())
}

#[test]
fn va1_alt_spam_amulet() {
    let b = b();
    let prices = prices();
    let lapis = base("Lapis Amulet");

    // The magic pool: 110 prefixes, 149 suffixes, total weight 210,750.
    let tags = b.item_tags(lapis, 0);
    let side_pool = |side| {
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
    let (pre, suf) = (side_pool(Side::Prefix), side_pool(Side::Suffix));
    let total: f64 = pre.iter().chain(&suf).map(|e| e.weight).sum();
    println!(
        "pool: {} prefixes, {} suffixes, total weight {total}",
        pre.len(),
        suf.len()
    );
    assert_eq!((pre.len(), suf.len()), (110, 149));
    assert_eq!(total, 210_750.0);
    let weight = |id: &str| {
        pre.iter()
            .find(|e| e.mod_index == mod_index(id))
            .unwrap()
            .weight
    };
    for (id, w) in [
        (LIGHTNING, 200.0),
        (FIRE, 200.0),
        (COLD, 200.0),
        (ALL, 50.0),
    ] {
        assert_eq!(weight(id), w, "{id}");
    }

    // Per alteration from a magic item with one other mod: the engine's single-draw share.
    let main = amulet(Rarity::Magic, &[LIGHTNING]);
    let mut other = state(&main, Rarity::Magic, &[]);
    other.suffixes.untracked = 1;
    let share: f64 = kernels::outcomes(b, &main, &[], &other, &Action::Alteration)
        .iter()
        .filter(|(o, _)| target::satisfied(b, &main, o))
        .map(|(_, p)| p)
        .sum();
    println!(
        "per-alteration share of {LIGHTNING}: {share:.6e} ({:.4}%)",
        share * 100.0
    );
    assert!(close(share, 200.0 / 210_750.0, 1e-12));
    assert!((share * 100.0 - 0.0949).abs() < 0.0001);
    assert!(((weight(ALL) / total) * 100.0 - 0.0237).abs() < 0.0001);

    // The market: normal base bought at 1, alt-spam into the magic amulet.
    let goods = vec![
        good("main", main.clone(), None, None),
        good("base", normal_amulet(), Some(1.0), None),
    ];
    let p1 = craft("p1", "main", "base", &["transmute", "alteration"]);
    let m = Market::new(goods.clone(), vec![p1.clone()], "main").unwrap();
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    let v = sol.goods[0].value.unwrap();
    let pv = &sol.processes[0];
    let closed =
        1.0 + market::TRADE_PENALTY + prices["transmute"] + (1.0 - share) * prices["alt"] / share;
    println!(
        "V(main) {v:.4} (closed form {closed:.4}), p_success {:?}, attempts {:?}",
        pv.p_success, pv.attempts
    );
    assert!(close(v, closed, 1e-9));
    assert_eq!(pv.status, ProcessStatus::Ready);
    // No sell exits: every run ends in the target.
    assert!(close(pv.p_success.unwrap(), 1.0, 1e-12));
    let core_goal: f64 = pv
        .core
        .iter()
        .filter(|e| e.good == Some(0))
        .map(|e| e.p)
        .sum();
    assert!(
        close(core_goal, share, 1e-12),
        "core step success {core_goal}"
    );
    let labels: Vec<&str> = pv.steps.iter().map(|s| s.label.as_str()).collect();
    println!("steps {labels:?}");
    assert_eq!(pv.steps.len(), 2);

    // Three named but unpriced bricks are counted at 0, with a warning.
    let mut named = goods.clone();
    named.push(good("fire", amulet(Rarity::Magic, &[FIRE]), None, None));
    named.push(good("cold", amulet(Rarity::Magic, &[COLD]), None, None));
    named.push(good("all", amulet(Rarity::Magic, &[ALL]), None, None));
    let m = Market::new(named.clone(), vec![p1.clone()], "main").unwrap();
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    println!("warnings {:?}", sol.warnings);
    assert_eq!(
        sol.warnings,
        vec!["p1: 3 outcomes are unpriced bricks, counted at 0".to_string()]
    );
    assert!(close(sol.goods[0].value.unwrap(), v, 1e-9));

    // Pricing the Fire brick lowers V(main). (250 would make alt-spamming pay for itself: a
    // money loop, since a Fire amulet then outearns the alterations spent finding it.)
    named[2].sell = Some(50.0);
    let m = Market::new(named, vec![p1], "main").unwrap();
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    let cheaper = sol.goods[0].value.unwrap();
    println!("with Fire brick at 50: V(main) {cheaper:.4}");
    assert!(cheaper < v && cheaper > 0.0);

    // The crafts the panel lists: Transmute on a normal amulet, Augmentation on a 1-mod magic.
    let normal = state(&main, Rarity::Normal, &[]);
    assert!(registry::applicable(
        b,
        &main,
        &normal,
        &Action::Transmute,
        &[]
    ));
    assert!(registry::applicable(
        b,
        &main,
        &other,
        &Action::Augmentation,
        &[]
    ));
}

const PHYS: &str = "LocalIncreasedPhysicalDamagePercent8";
const HYBRID: &str = "LocalIncreasedPhysicalDamagePercentAndAccuracyRating8";
const FLAT: &str = "LocalAddedPhysicalDamage9";
const FOURTH: &str = "LocalAddedFireDamage9";

fn foil(ids: &[&str], min_hits: u16) -> Target {
    let mut t = target(
        "Jewelled Foil",
        84,
        Rarity::Rare,
        ids.iter().map(|id| req(&[id])).collect(),
    );
    t.min_hits = min_hits;
    t
}

fn va2_market(
    brick: Option<f64>,
    recombiner: f64,
) -> (Market, std::collections::BTreeMap<String, f64>) {
    let mut goods = vec![
        good("main", foil(&[PHYS, HYBRID, FLAT], 3), None, None),
        good("a2", foil(&[PHYS, HYBRID], 2), None, None),
        good("b2", foil(&[FLAT, FOURTH], 2), None, None),
        good("phys", foil(&[PHYS], 1), Some(10.0), None),
        good("hybrid", foil(&[HYBRID], 1), Some(10.0), None),
        good("flat", foil(&[FLAT], 1), Some(10.0), None),
        good("fourth", foil(&[FOURTH], 1), Some(10.0), None),
    ];
    if let Some(sell) = brick {
        goods.push(good("a1", foil(&[PHYS, HYBRID], 1), None, Some(sell)));
    }
    let processes = vec![
        recombine("ra", "a2", "phys", "hybrid"),
        recombine("rb", "b2", "flat", "fourth"),
        recombine("rm", "main", "a2", "b2"),
    ];
    let mut prices = prices();
    prices.insert("recombiner".into(), recombiner);
    (Market::new(goods, processes, "main").unwrap(), prices)
}

#[test]
fn va2_three_t1_recombine() {
    let b = b();
    let (m, prices) = va2_market(None, 15.0);
    let start = Instant::now();
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    let ms = start.elapsed().as_secs_f64() * 1e3;
    println!("market::solve {ms:.1} ms");
    assert!(ms < 2000.0, "V-A5: {ms} ms");

    let p = |id: &str| sol.processes.iter().find(|x| x.id == id).unwrap();
    let g = |id: &str| sol.goods.iter().find(|x| x.id == id).unwrap();
    for id in ["ra", "rb"] {
        assert!(close(p(id).p_success.unwrap(), 1.0 / 3.0, 1e-9), "{id}");
    }
    assert!(close(p("rm").p_success.unwrap(), 0.075, 1e-9));
    for id in ["main", "a2", "b2"] {
        assert_eq!(
            g(id).via.as_deref(),
            Some(match id {
                "main" => "rm",
                "a2" => "ra",
                _ => "rb",
            })
        );
    }
    for id in ["phys", "hybrid", "flat", "fourth"] {
        assert_eq!(g(id).via.as_deref(), Some("buy"));
    }
    let a2 = g("a2").value.unwrap();
    assert!(close(a2, 35.0 * 3.0, 1e-9));
    let main = g("main").value.unwrap();
    println!("V(a2) {a2}, V(b2) {:?}, V(main) {main}", g("b2").value);
    assert!(close(main, (2.0 * a2 + 15.0) / 0.075, 1e-9));

    // A dearer recombinator raises V(main).
    let (m, dear) = va2_market(None, 30.0);
    let up = market::solve(b, &m, &dear, &[]).unwrap();
    assert!(up.goods[0].value.unwrap() > main);

    // "1 of 2 kept" priced 30 lowers V(a2) by exactly 30 * P(1 kept) / p_success.
    let (m, prices) = va2_market(Some(30.0), 15.0);
    let sol = market::solve(b, &m, &prices, &[]).unwrap();
    let a2b = sol
        .goods
        .iter()
        .find(|x| x.id == "a2")
        .unwrap()
        .value
        .unwrap();
    let drop = 30.0 * (2.0 / 3.0) / (1.0 / 3.0);
    println!("V(a2) with brick {a2b} (drop {})", a2 - a2b);
    assert!(close(a2 - a2b, drop, 1e-9));

    // Result base and item level of the top recombine.
    let x = m.ideal(b, m.good("a2").unwrap());
    let y = m.ideal(b, m.good("b2").unwrap());
    for (o, _) in kernels::recombine(b, &m.universe, &x, &y) {
        assert_eq!(o.base, base("Jewelled Foil"));
        assert_eq!(o.ilvl, 84);
    }
}
