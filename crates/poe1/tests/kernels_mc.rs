//! Monte Carlo check of the kernel distributions against concrete simulation.
//!
//! The kernels aggregate untracked mods by (side, influence class) and approximate the weight a
//! draw removes from that class; this test bounds that approximation for the four V6 scenarios.

use std::collections::HashMap;

use poe1::data::Bundle;
use poe1::item::{Blocked, Hit, INFLUENCE_NAMES, ItemState, Rarity, Side};
use poe1::kernels::{self, Action};
use poe1::target::{self, Target};
use poe1::test_bundle;
use poe1::weights::{PoolFilter, TagSet, pool};
use rand::{RngExt, SeedableRng};
use rand_pcg::Pcg64Mcg;

const RUNS: u32 = 200_000;
const SEED: u64 = 1;

fn zbits(names: &[&str]) -> u8 {
    let mut out = 0;
    for n in names {
        let i = INFLUENCE_NAMES
            .iter()
            .position(|x| x == n)
            .expect("influence");
        out |= 1 << i;
    }
    out
}

fn simplex_target(b: &Bundle) -> Target {
    let base = b.base_by_name("Simplex Amulet").expect("base");
    let cat = b.catalyst_by_label("Attribute").expect("catalyst");
    let req = |group: &str, side: Side, id: &str, min_value: f64, label: &str| {
        poe1::target::Requirement {
            group: b.groups.iter().position(|g| g == group).unwrap() as u16,
            side,
            accepted: vec![b.mod_by_id(id).unwrap()],
            min_value,
            label: label.into(),
        }
    };
    Target {
        base,
        ilvl_min: 84,
        influences: zbits(&["Shaper", "Warlord"]),
        rarity: Rarity::Rare,
        catalyst: Some((cat, 20)),
        requirements: vec![
            req(
                "DamagePer15Attributes",
                Side::Prefix,
                "DamagePer15StrengthInfluence1",
                2.0,
                "2% increased Damage per 15 Strength",
            ),
            req(
                "PercentageStrength",
                Side::Suffix,
                "PercentageStrengthBodyInfluence2_",
                26.0,
                "26% increased Strength",
            ),
            req(
                "PercentageAllAttributes",
                Side::Suffix,
                "PercentageAllAttributesUberShaper1",
                19.0,
                "19% increased Attributes",
            ),
        ],
        min_hits: 3,
    }
}

struct Sim<'a> {
    b: &'a Bundle,
    t: &'a Target,
    tags: TagSet,
    caps: (u8, u8),
}

impl<'a> Sim<'a> {
    fn new(b: &'a Bundle, t: &'a Target) -> Sim<'a> {
        let caps = ItemState::new(t.base, t.ilvl_min, t.rarity, t.influences).caps(b);
        Sim {
            b,
            t,
            tags: b.item_tags(t.base, t.influences),
            caps,
        }
    }

    /// One weighted draw across every side with a free slot, excluding already-used groups.
    fn draw(
        &self,
        rng: &mut Pcg64Mch,
        free: &mut [u8; 2],
        used: &[u16],
        influence_only: bool,
    ) -> Option<(Side, u32, i64)> {
        let mut candidates: Vec<(Side, u32, f64)> = Vec::new();
        for side in Side::ALL {
            if free[side.index()] == 0 {
                continue;
            }
            let entries = pool(
                self.b,
                &PoolFilter {
                    side,
                    tags: &self.tags,
                    ilvl: self.t.ilvl_min,
                    excluded_groups: used,
                    influence_only,
                    catalyst: None,
                },
            );
            for e in entries {
                candidates.push((side, e.mod_index, e.weight));
            }
        }
        let total: f64 = candidates.iter().map(|c| c.2).sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = rng.random::<f64>() * total;
        let mut chosen = *candidates.last().expect("candidates");
        for (side, m, w) in candidates {
            r -= w;
            if r <= 0.0 {
                chosen = (side, m, w);
                break;
            }
        }
        free[chosen.0.index()] -= 1;
        let md = self.b.mod_at(chosen.1);
        let (min, max) = md
            .stats
            .first()
            .map(|s| (s.min.floor() as i64, s.max.floor() as i64))
            .unwrap_or((0, 0));
        let roll = if max >= min {
            rng.random_range(min..=max)
        } else {
            0
        };
        Some((chosen.0, chosen.1, roll))
    }

    fn to_state(&self, influences: u8, lock: u8, mods: &[Vec<(u32, i64)>; 2]) -> ItemState {
        let b = self.b;
        let t = self.t;
        let mut s = ItemState::new(t.base, t.ilvl_min, t.rarity, influences);
        s.lock = lock;
        for side in Side::ALL {
            for (m, roll) in &mods[side.index()] {
                let md = b.mod_at(*m);
                let req = t
                    .requirements
                    .iter()
                    .position(|r| r.side == side && r.group == md.group);
                match req {
                    Some(req) => match t.requirements[req].accepted.iter().position(|x| x == m) {
                        Some(idx) => {
                            let eff = target::effect(b, t.base, t.catalyst, *m);
                            let value_ok =
                                target::display(*roll as f64, eff) >= t.requirements[req].min_value;
                            s.side_mut(side).wanted.push(Hit {
                                req: req as u16,
                                idx: idx as u8,
                                value_ok,
                            });
                        }
                        None => s.side_mut(side).blocked.push(Blocked {
                            group: md.group,
                            influence: md.influence,
                        }),
                    },
                    None => {
                        s.side_mut(side).untracked += 1;
                        if md.influence {
                            s.side_mut(side).untracked_influenced += 1;
                        }
                    }
                }
            }
            s.side_mut(side).wanted.sort();
            s.side_mut(side).blocked.sort();
        }
        s
    }
}

type Pcg64Mch = Pcg64Mcg;

enum Scenario {
    /// Chaos Orb on a fresh Simplex: three mods drawn from the combined pool.
    Chaos,
    /// Harvest influenced reforge on the two-suffix state with a Suffix lock.
    HarvestTwoSuffixLock,
    /// Exalted Orb on a state holding only the Shaper attribute suffix.
    ExaltedOneSuffix,
    /// Orb of Annulment on the three-mod ideal state.
    AnnulThreeMods,
}

fn req_index(t: &Target, label_part: &str) -> (u16, u32) {
    let i = t
        .requirements
        .iter()
        .position(|r| r.label.contains(label_part))
        .expect("requirement");
    (i as u16, t.requirements[i].accepted[0])
}

fn simulate(scenario: &Scenario, runs: u32) -> HashMap<ItemState, u64> {
    let b = test_bundle();
    let t = simplex_target(b);
    let sim = Sim::new(b, &t);
    let mut rng = Pcg64Mch::seed_from_u64(SEED);
    let mut counts: HashMap<ItemState, u64> = HashMap::new();

    for _ in 0..runs {
        let mods: [Vec<(u32, i64)>; 2] = [Vec::new(), Vec::new()];
        let state = match scenario {
            Scenario::Chaos => {
                let mut mods = mods;
                let mut free = [sim.caps.0, sim.caps.1];
                let mut used: Vec<u16> = Vec::new();
                let n = sim.caps.0 + sim.caps.1;
                for _ in 0..n {
                    if let Some((side, m, roll)) = sim.draw(&mut rng, &mut free, &used, false) {
                        mods[side.index()].push((m, roll));
                        used.push(b.mod_at(m).group);
                    }
                }
                sim.to_state(t.influences, 0, &mods)
            }
            Scenario::HarvestTwoSuffixLock => {
                let mut mods = mods;
                // Keep both target suffixes, then reforge the (locked-out) prefix side only.
                for part in ["increased Attributes", "increased Strength"] {
                    let (_, m) = req_index(&t, part);
                    let roll = if part.contains("Strength") { 12 } else { 9 };
                    mods[Side::Suffix.index()].push((m, roll));
                }
                let mut used: Vec<u16> = mods[Side::Suffix.index()]
                    .iter()
                    .map(|(m, _)| b.mod_at(*m).group)
                    .collect();
                let mut free = [1u8, 0u8];
                if let Some((side, m, roll)) = sim.draw(&mut rng, &mut free, &used, true) {
                    mods[side.index()].push((m, roll));
                    used.push(b.mod_at(m).group);
                }
                sim.to_state(t.influences, 0, &mods)
            }
            Scenario::ExaltedOneSuffix => {
                let mut mods = mods;
                let (_, m) = req_index(&t, "increased Attributes");
                mods[Side::Suffix.index()].push((m, 9));
                let used: Vec<u16> = vec![b.mod_at(m).group];
                let mut free = [1u8, 1u8];
                if let Some((side, m2, roll)) = sim.draw(&mut rng, &mut free, &used, false) {
                    mods[side.index()].push((m2, roll));
                }
                sim.to_state(t.influences, 0, &mods)
            }
            Scenario::AnnulThreeMods => {
                let mut mods = mods;
                for (part, roll) in [
                    ("increased Attributes", 9),
                    ("increased Strength", 12),
                    ("Damage per 15 Strength", 1),
                ] {
                    let (req, m) = req_index(&t, part);
                    let side = t.requirements[req as usize].side;
                    mods[side.index()].push((m, roll));
                }
                let pick = rng.random_range(0..3);
                let mut flat: Vec<(Side, usize)> = Vec::new();
                for side in Side::ALL {
                    for i in 0..mods[side.index()].len() {
                        flat.push((side, i));
                    }
                }
                let (side, i) = flat[pick];
                mods[side.index()].remove(i);
                sim.to_state(t.influences, 0, &mods)
            }
        };
        *counts.entry(state).or_insert(0) += 1;
    }
    counts
}

fn kernel_distribution(scenario: &Scenario) -> Vec<(ItemState, f64)> {
    let b = test_bundle();
    let t = simplex_target(b);
    let ideal = target::ideal_state(b, &t);
    let mut state = match scenario {
        Scenario::Chaos => ItemState::new(t.base, t.ilvl_min, t.rarity, t.influences),
        Scenario::HarvestTwoSuffixLock => {
            let mut s = ideal.clone();
            s.prefixes = Default::default();
            s.catalyst = None;
            s.lock = 0b10;
            s
        }
        Scenario::ExaltedOneSuffix => {
            let mut s = ItemState::new(t.base, t.ilvl_min, t.rarity, t.influences);
            let req = t
                .requirements
                .iter()
                .position(|r| r.label.contains("Attributes"))
                .unwrap();
            s.suffixes.wanted.push(Hit {
                req: req as u16,
                idx: 0,
                value_ok: true,
            });
            s
        }
        Scenario::AnnulThreeMods => {
            let mut s = ideal;
            s.catalyst = None;
            s
        }
    };
    state.catalyst = None;
    let action = match scenario {
        Scenario::Chaos => Action::Chaos,
        Scenario::HarvestTwoSuffixLock => Action::HarvestInfluenced,
        Scenario::ExaltedOneSuffix => Action::Exalted,
        Scenario::AnnulThreeMods => Action::Annul,
    };
    let outs = kernels::outcomes(b, &t, &[], &state, &action);
    let total: f64 = outs.iter().map(|(_, p)| p).sum();
    assert!((total - 1.0).abs() < 1e-9, "{total} does not sum to 1");
    outs
}

fn describe(s: &ItemState) -> String {
    let tag = |side: &poe1::item::SideState| {
        format!(
            "w{} b{} u{}/{}",
            side.wanted.len(),
            side.blocked.len(),
            side.untracked,
            side.untracked_influenced
        )
    };
    format!("P[{}] S[{}]", tag(&s.prefixes), tag(&s.suffixes))
}

#[test]
fn kernels_match_concrete_simulation() {
    let b = test_bundle();
    let t = simplex_target(b);
    assert_eq!(t.influences, 0b100001);
    let scenarios = [
        ("Chaos on fresh Simplex", Scenario::Chaos),
        (
            "HarvestInfluenced on two-suffix state with Suffix lock",
            Scenario::HarvestTwoSuffixLock,
        ),
        ("Exalted on one-suffix state", Scenario::ExaltedOneSuffix),
        ("Annul on three-mod state", Scenario::AnnulThreeMods),
    ];

    let mut worst_norm = 0.0f64;
    let mut worst_scenario = String::new();
    let mut worst_class = String::new();
    for (name, scenario) in scenarios {
        let kernel = kernel_distribution(&scenario);
        let mut kernel_map: HashMap<ItemState, f64> = kernel.into_iter().collect();
        let counts = simulate(&scenario, RUNS);
        let n = RUNS as f64;
        let mut keys: Vec<ItemState> = kernel_map.keys().cloned().collect();
        for k in counts.keys() {
            if !kernel_map.contains_key(k) {
                keys.push(k.clone());
            }
        }
        let mut scenario_worst = 0.0f64;
        for key in &keys {
            let p = kernel_map.remove(key).unwrap_or(0.0);
            let p_hat = counts.get(key).copied().unwrap_or(0) as f64 / n;
            let diff = (p - p_hat).abs();
            let sigma = (p * (1.0 - p) / n).sqrt();
            let tol = 4.0 * sigma + 1e-4;
            assert!(
                diff <= tol,
                "{name}: {} p={p:.8} p_hat={p_hat:.8} diff={diff:.3e} tol={tol:.3e}",
                describe(key)
            );
            if sigma > 0.0 {
                let norm = diff / sigma;
                if norm > scenario_worst {
                    scenario_worst = norm;
                }
                if norm > worst_norm {
                    worst_norm = norm;
                    worst_scenario = name.to_string();
                    worst_class = describe(key);
                }
            }
        }
        println!(
            "{name}: {} outcome classes, worst normalised deviation {scenario_worst:.3}",
            keys.len()
        );
    }
    println!(
        "worst normalised deviation overall: {worst_norm:.3} ({worst_scenario}; {worst_class})"
    );
}
