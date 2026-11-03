//! Fixtures shared by the M2 tests: targets built from mod ids, default prices.
#![allow(dead_code)]

use std::collections::BTreeMap;

use poe1::data::Bundle;
use poe1::item::{Hit, ItemState, Rarity, Side};
use poe1::market::{CraftPlan, Good, Process, ProcessKind};
use poe1::target::{Requirement, Target};

const PRICES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../web/src/defaultPrices.json"
));

pub fn b() -> &'static Bundle {
    poe1::test_bundle()
}

pub fn prices() -> BTreeMap<String, f64> {
    let v: serde_json::Value = serde_json::from_str(PRICES).expect("defaultPrices.json");
    serde_json::from_value(v["prices"].clone()).expect("prices map")
}

pub fn mod_index(id: &str) -> u32 {
    b().mod_by_id(id).unwrap_or_else(|| panic!("no mod {id}"))
}

pub fn base(name: &str) -> u16 {
    b().base_by_name(name)
        .unwrap_or_else(|| panic!("no base {name}"))
}

/// A requirement accepting exactly these mods (one group), any roll.
pub fn req(ids: &[&str]) -> Requirement {
    let b = b();
    let accepted: Vec<u32> = ids.iter().map(|id| mod_index(id)).collect();
    let m = b.mod_at(accepted[0]);
    Requirement {
        group: m.group,
        side: m.generation,
        accepted,
        min_value: 0.0,
        label: m.text.clone(),
    }
}

pub fn target(base_name: &str, ilvl: u8, rarity: Rarity, reqs: Vec<Requirement>) -> Target {
    let min_hits = reqs.len() as u16;
    Target {
        base: base(base_name),
        ilvl_min: ilvl,
        influences: 0,
        rarity,
        catalyst: None,
        requirements: reqs,
        min_hits,
    }
}

/// A state of `t`'s base holding hits for the listed requirement indices.
pub fn state(t: &Target, rarity: Rarity, hits: &[usize]) -> ItemState {
    let mut s = ItemState::new(t.base, t.ilvl_min, rarity, 0);
    for &i in hits {
        let side = t.requirements[i].side;
        s.side_mut(side).wanted.push(Hit {
            req: i as u16,
            idx: 0,
            value_ok: true,
        });
    }
    s.prefixes.wanted.sort();
    s.suffixes.wanted.sort();
    s
}

pub fn good(id: &str, spec: Target, buy: Option<f64>, sell: Option<f64>) -> Good {
    Good {
        id: id.into(),
        name: id.into(),
        spec,
        buy,
        sell,
    }
}

pub fn craft(id: &str, output: &str, from: &str, allowed: &[&str]) -> Process {
    Process {
        id: id.into(),
        output: output.into(),
        kind: ProcessKind::Craft {
            from: Some(from.into()),
            allowed: allowed.iter().map(|s| s.to_string()).collect(),
            plan: CraftPlan::default(),
        },
        recipe: None,
    }
}

pub fn recombine(id: &str, output: &str, a: &str, b: &str) -> Process {
    Process {
        id: id.into(),
        output: output.into(),
        kind: ProcessKind::Recombine {
            a: a.into(),
            b: b.into(),
        },
        recipe: Some("recombine-split".into()),
    }
}

/// `n` prefix mods of distinct groups that roll on `base_name` (bundle order).
pub fn distinct_prefixes(base_name: &str, n: usize) -> Vec<Requirement> {
    let b = b();
    let tags = b.item_tags(base(base_name), 0);
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for m in &b.mods {
        if out.len() == n {
            break;
        }
        if m.generation != Side::Prefix
            || m.fossil_only
            || seen.contains(&m.group)
            || poe1::weights::spawn_weight(m, &tags) == 0
        {
            continue;
        }
        seen.push(m.group);
        out.push(req(&[m.id.as_str()]));
    }
    assert_eq!(out.len(), n);
    out
}

pub fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

/// Every ordered draw of up to `k` entries without replacement, each pick ∝ `weights[i]^alpha`
/// among the entries whose group is not drawn yet: (picked indices in order, probability). A draw
/// stops early when no entry is left. An independent enumeration, not the kernel.
pub fn ordered_draws(
    weights: &[f64],
    groups: &[usize],
    k: usize,
    alpha: f64,
) -> Vec<(Vec<usize>, f64)> {
    fn go(
        w: &[f64],
        g: &[usize],
        k: usize,
        alpha: f64,
        picked: &mut Vec<usize>,
        p: f64,
        out: &mut Vec<(Vec<usize>, f64)>,
    ) {
        let eligible: Vec<usize> = (0..w.len())
            .filter(|i| picked.iter().all(|j| g[*j] != g[*i]))
            .collect();
        if picked.len() == k || eligible.is_empty() {
            out.push((picked.clone(), p));
            return;
        }
        let total: f64 = eligible.iter().map(|&i| w[i].powf(alpha)).sum();
        for &i in &eligible {
            picked.push(i);
            go(w, g, k, alpha, picked, p * w[i].powf(alpha) / total, out);
            picked.pop();
        }
    }
    let mut out = Vec::new();
    go(weights, groups, k, alpha, &mut Vec::new(), 1.0, &mut out);
    out
}

/// P(the `k` entries drawn are exactly the set `wanted`), every entry its own group.
pub fn keep_set_prob(weights: &[f64], wanted: &[usize], k: usize, alpha: f64) -> f64 {
    let groups: Vec<usize> = (0..weights.len()).collect();
    let mut want = wanted.to_vec();
    want.sort_unstable();
    ordered_draws(weights, &groups, k, alpha)
        .into_iter()
        .filter(|(seq, _)| {
            let mut s = seq.clone();
            s.sort_unstable();
            s == want
        })
        .map(|(_, p)| p)
        .sum()
}

/// The weight of `id` in the ilvl-84 magic pool of a plain `base_name` (`weights::pool`), as the
/// alt-spam tests read it.
pub fn weight_on(base_name: &str, id: &str) -> f64 {
    let b = b();
    let tags = b.item_tags(base(base_name), 0);
    let m = mod_index(id);
    poe1::weights::pool(
        b,
        &poe1::weights::PoolFilter {
            side: b.mod_at(m).generation,
            tags: &tags,
            ilvl: 84,
            excluded_groups: &[],
            influence_only: false,
            catalyst: None,
        },
    )
    .iter()
    .find(|e| e.mod_index == m)
    .unwrap_or_else(|| panic!("{id} does not roll on {base_name}"))
    .weight
}
