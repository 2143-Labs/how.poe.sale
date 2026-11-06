//! The inverse of one step: which input and method could have produced an item, and with what
//! chance.

mod common;

use common::{item, pattern, spec};
use poe1_v2::apply::{Leaf, Params, apply};
use poe1_v2::invert;
use poe1_v2::item::{AffixSpec, Item, ItemSpec};
use poe1_v2::test_data;

fn edited(name: &str, f: impl FnOnce(&mut ItemSpec)) -> Item {
    let mut s = spec(name);
    f(&mut s);
    Item::from_spec(test_data(), &s).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn affix(id: &str) -> AffixSpec {
    AffixSpec {
        id: id.into(),
        values: None,
        fractured: false,
    }
}

/// Whether two items hold the same modifiers on the same sides, whatever their values.
fn same_mods(a: &Item, b: &Item) -> bool {
    let ids = |i: &Item| -> Vec<(u32, bool, bool)> {
        i.prefixes
            .iter()
            .chain(&i.suffixes)
            .map(|x| (x.roll.m, x.fractured, x.roll.values.is_some()))
            .collect()
    };
    let strip = |v: &[(u32, bool, bool)]| -> Vec<(u32, bool)> {
        v.iter().map(|(m, f, _)| (*m, *f)).collect()
    };
    a.base == b.base
        && a.rarity == b.rarity
        && a.influences == b.influences
        && a.corrupted == b.corrupted
        && a.quality == b.quality
        && a.catalyst == b.catalyst
        && a.implicits
            .iter()
            .map(|r| r.m)
            .eq(b.implicits.iter().map(|r| r.m))
        && strip(&ids(a)) == strip(&ids(b))
}

/// Every source, flattened, with the method and chance.
fn flat(inversion: &invert::Inversion) -> Vec<(String, f64)> {
    inversion
        .groups
        .iter()
        .flat_map(|g| g.sources.iter().map(|s| (s.method.clone(), s.p)))
        .collect()
}

fn chance(inversion: &invert::Inversion, method: &str) -> Vec<f64> {
    flat(inversion)
        .into_iter()
        .filter(|(m, _)| m == method)
        .map(|(_, p)| p)
        .collect()
}

/// A bench craft's input is the item without the modifier it added, for certain.
#[test]
fn a_bench_craft_is_its_own_input_plus_the_modifier() {
    let d = test_data();
    let target = item("crown-a-mana");
    let inversion = invert::sources(d, &pattern("crown-shaper"), &target);
    let source = inversion
        .groups
        .iter()
        .flat_map(|g| &g.sources)
        .find(|s| s.method == "bench/add/EinharMasterIncreasedMana2")
        .expect("the mana bench craft");
    assert_eq!(source.p, 1.0);
    assert_eq!(
        source.input.prefixes.len() + source.input.suffixes.len(),
        target.prefixes.len() + target.suffixes.len() - 1
    );
    assert_eq!(source.input.prefixes, item("crown-a").prefixes);
}

/// An Exalted Orb's input is the item without the modifier it added, at the chance the pool draws
/// that modifier: T1 life is 1000 of the Crown's 55375.
#[test]
fn an_exalted_orb_is_priced_by_its_pool() {
    let d = test_data();
    let target = edited("crown-a", |s| s.prefixes.push(affix("IncreasedLife9")));
    let inversion = invert::sources(d, &pattern("crown-shaper"), &target);
    let chances = chance(&inversion, "currency/exalted");
    assert!(!chances.is_empty(), "no Exalted Orb source");
    assert!(
        chances.iter().any(|p| (p - 1000.0 / 55375.0).abs() < 1e-12),
        "{chances:?}"
    );
}

/// An annulment's input has one more modifier than its result, at one over the number of
/// modifiers the removal could have taken from that input: here five, so a fifth.
#[test]
fn an_annulment_is_one_over_the_removable_count() {
    let d = test_data();
    let target = edited("crown-a", |s| s.prefixes.push(affix("IncreasedLife9")));
    let inversion = invert::sources(d, &pattern("crown-shaper"), &target);
    let chances = chance(&inversion, "currency/annulment");
    assert!(
        chances.iter().any(|p| (p - 1.0 / 5.0).abs() < 1e-12),
        "{chances:?}"
    );
}

/// A metamod narrows the removal: with Suffixes Cannot Be Changed, an annulment can only take one
/// of the prefixes, so each candidate input is a half rather than a fifth.
#[test]
fn a_lock_narrows_the_removal() {
    let d = test_data();
    let target = edited("crown-l1", |s| {
        s.prefixes.retain(|a| a.id != "IncreasedLife9");
    });
    let inversion = invert::sources(d, &pattern("crown-shaper"), &target);
    let chances = chance(&inversion, "currency/annulment");
    assert!(
        !chances.is_empty() && chances.iter().all(|p| (p - 0.5).abs() < 1e-12),
        "{chances:?}"
    );
}

/// The Vaal Orb's corruption is one of its four branches, so the input is the uncorrupted item at
/// one chance in four.
#[test]
fn corruption_is_one_branch_of_the_vaal_orb() {
    let d = test_data();
    let target = edited("crown-a", |s| s.corrupted = true);
    let inversion = invert::sources(d, &pattern("crown-shaper"), &target);
    let chances = chance(&inversion, "currency/vaal");
    assert!(
        chances.iter().any(|p| (p - 0.25).abs() < 1e-12),
        "{chances:?}"
    );
}

/// Everything the inverse says is true forwards: running the method on the input it found gives
/// the item back, with the chance it claimed, for every item and pattern the catalog covers.
#[test]
fn every_source_is_a_real_source() {
    let d = test_data();
    let mut checked = 0;
    for (pname, name) in common::pairs() {
        let pat = pattern(&pname);
        let target = item(&name);
        let inversion = invert::sources(d, &pat, &target);
        assert!(inversion.found > 0, "{pname} / {name}: nothing found");
        for group in &inversion.groups {
            for source in &group.sources {
                let at = format!("{pname} / {name} / {}", source.method);
                let method = d
                    .method(&source.method)
                    .unwrap_or_else(|| panic!("{at}: missing"));
                let outcome = apply(
                    d,
                    method,
                    std::slice::from_ref(&source.input),
                    &Params::default(),
                )
                .unwrap_or_else(|e| panic!("{at} on its own input: {e}"));
                // The model's outcomes leave values unrolled, so a source is compared on the
                // modifiers it holds, not on the numbers the target happens to show.
                let found = outcome
                    .leaves
                    .iter()
                    .filter(|(_, leaf)| matches!(leaf, Leaf::Item(i) if same_mods(i, &target)))
                    .map(|(p, _)| *p)
                    .sum::<f64>();
                assert!(
                    (found - source.p).abs() < 1e-12,
                    "{at} claims {:.6}, the forward run gives {:.6}",
                    source.p,
                    found
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} sources checked");
}

/// Every claimed source is a probability of a method that really applies, the group's best is
/// the best in it, and the sources are ordered so the report reads best first.
#[test]
fn sources_are_probabilities_of_methods_that_apply() {
    let d = test_data();
    for (pname, name) in common::pairs() {
        let inversion = invert::sources(d, &pattern(&pname), &item(&name));
        for group in &inversion.groups {
            let at = format!("{pname} / {name}");
            let best = group.sources.iter().map(|s| s.p).fold(0.0f64, f64::max);
            assert!((group.best - best).abs() < 1e-12, "{at}: group best");
            for pair in group.sources.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                assert!(
                    a.p > b.p || (a.p == b.p && a.method <= b.method),
                    "{at}: {} at {} before {} at {}",
                    a.method,
                    a.p,
                    b.method,
                    b.p
                );
            }
            for source in &group.sources {
                let at = format!("{at} / {}", source.method);
                assert!(source.p > 0.0 && source.p <= 1.0, "{at}: p {}", source.p);
                let method = d
                    .method(&source.method)
                    .unwrap_or_else(|| panic!("{at}: missing"));
                assert_eq!(method.inputs, 1, "{at}: not a single-input method");
                assert!(
                    !inversion.skipped.iter().any(|(id, _)| id == &source.method),
                    "{at}: claimed and skipped at once"
                );
            }
        }
    }
}

/// A method the model does not compute cannot be inverted, and says so rather than guessing.
#[test]
fn a_reforge_is_not_invertible() {
    let d = test_data();
    let inversion = invert::sources(d, &pattern("crown-shaper"), &item("crown-a"));
    let chaos = inversion
        .skipped
        .iter()
        .find(|(id, _)| id == "currency/chaos")
        .map(|(_, why)| why.clone());
    assert_eq!(chaos, Some("cannot undo `fill`".to_string()));
}

/// The sources are grouped by what the input looked like, so hundreds of concrete inputs collapse
/// into a few states, and a group's chance is the best chance in it.
#[test]
fn sources_group_by_pattern_state() {
    let d = test_data();
    let inversion = invert::sources(d, &pattern("crown-shaper"), &item("crown-a-mana"));
    assert!(inversion.groups.len() < inversion.found, "{inversion:?}");
    for group in &inversion.groups {
        let best = group.sources.iter().map(|s| s.p).fold(0.0f64, f64::max);
        assert!((group.best - best).abs() < 1e-12);
    }
    // The groups are ordered by that chance.
    let bests: Vec<f64> = inversion.groups.iter().map(|g| g.best).collect();
    let mut sorted = bests.clone();
    sorted.sort_by(|a, b| b.total_cmp(a));
    assert_eq!(bests, sorted);
}
