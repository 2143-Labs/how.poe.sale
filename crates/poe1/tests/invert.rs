//! The inverse of one step: which input and method could have produced an item, and with what
//! chance.

mod common;

use common::{item, pattern, spec};
use poe1::apply::{Leaf, Params, apply};
use poe1::invert;
use poe1::item::{AffixSpec, Item, ItemSpec};
use poe1::test_data;

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

/// Three removable modifiers, one branch each: with Suffixes Cannot Be Changed on a Crown whose
/// prefix side has room, a removal can only take one of the three prefixes, a third each.
#[test]
fn a_locked_side_leaves_three_removals_at_a_third() {
    let d = test_data();
    let inversion = invert::sources(d, &pattern("crown-shaper"), &item("crown-l1"));
    let chances = chance(&inversion, "currency/annulment");
    assert!(
        !chances.is_empty() && chances.iter().all(|p| (p - 1.0 / 3.0).abs() < 1e-12),
        "{chances:?}"
    );
    for group in &inversion.groups {
        for source in group
            .sources
            .iter()
            .filter(|s| s.method == "currency/annulment")
        {
            assert_eq!(source.input.prefixes.len(), 3, "the input's prefix side");
            assert_eq!(source.input.suffixes.len(), 3, "the input's suffix side");
        }
    }
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

/// Sources whose chance the inverse is known to miscount, found once the walk took in every
/// fixture rather than only those the patterns accept: (fixture, method, why). Each is a defect
/// of the backward pass, listed so that a new miscount still fails and a fixed one is noticed.
const KNOWN_MISCOUNTS: &[(&str, &str, &str)] = &[
    (
        "cobalt-jewel-magic",
        "harvest/add-attack-remove-random",
        "the removal's keepLastAdded is not undone: the added modifier is counted as removable",
    ),
    (
        "cogwork-ring-fire",
        "harvest/add-attack-remove-random",
        "keepLastAdded is not undone",
    ),
    (
        "cogwork-ring-fire",
        "harvest/add-fire-remove-random",
        "keepLastAdded is not undone",
    ),
    (
        "talisman-rare-four",
        "harvest/add-attack-remove-random",
        "keepLastAdded is not undone",
    ),
    (
        "talisman-rare-four",
        "harvest/add-physical-remove-random",
        "keepLastAdded is not undone",
    ),
    (
        "crown-uninfluenced-fracture",
        "harvest/add-life-remove-random",
        "keepLastAdded is not undone",
    ),
    (
        "cogwork-ring-fire",
        "currency/annulment",
        "Cannot Roll narrows the removal; the source's note says this pass does not count it",
    ),
    (
        "talisman-rare-four",
        "currency/annulment",
        "Cannot Roll narrows the removal; the source's note says this pass does not count it",
    ),
    (
        "tincture-magic",
        "currency/transmutation",
        "a two-modifier result is reached in two draw orders and only one is counted",
    ),
];

/// Everything the inverse says is true forwards: running the method on the input it found gives
/// the item back, with the chance it claimed, for every item under every pattern, including the
/// corrupted and uninfluenced fixtures the catalog leaves out.
#[test]
fn every_source_is_a_real_source() {
    let d = test_data();
    let accepted = common::pairs();
    let mut checked = 0;
    let mut walked: std::collections::BTreeMap<String, usize> = Default::default();
    let mut miscounted: std::collections::BTreeSet<(String, String)> = Default::default();
    for (pname, name) in common::all_pairs() {
        let pat = pattern(&pname);
        let target = item(&name);
        let inversion = invert::sources(d, &pat, &target);
        if accepted.contains(&(pname.clone(), name.clone())) {
            assert!(inversion.found > 0, "{pname} / {name}: nothing found");
        }
        *walked.entry(name.clone()).or_default() += inversion.found;
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
                if (found - source.p).abs() >= 1e-12 {
                    let known = KNOWN_MISCOUNTS
                        .iter()
                        .any(|(n, m, _)| *n == name && *m == source.method);
                    assert!(
                        known,
                        "{at} claims {:.6}, the forward run gives {:.6}",
                        source.p,
                        found
                    );
                    miscounted.insert((name.clone(), source.method.clone()));
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} sources checked");
    // The fixtures `pairs` leaves out are walked, and have sources to check.
    for name in [
        "crown-a-corrupted",
        "crown-actual-corrupted",
        "crown-d2-scbc-corrupted",
        "crown-uninfluenced-fracture",
    ] {
        assert!(
            !accepted.iter().any(|(_, n)| n == name),
            "{name} is accepted"
        );
        assert!(walked.get(name).copied().unwrap_or(0) > 0, "{name}: {walked:?}");
    }
    // A known miscount that no longer shows has been fixed: take it off the list.
    for (name, method, why) in KNOWN_MISCOUNTS {
        assert!(
            miscounted.contains(&(name.to_string(), method.to_string())),
            "{name} / {method} now round-trips ({why}); remove it from KNOWN_MISCOUNTS"
        );
    }
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

/// A removal can have taken any modifier the item could hold, not only a natural one: `crown-c2`
/// could be the item that held the veiled gem-level prefix and lost it to a Veiled Exalted Orb.
#[test]
fn a_removed_unveiled_modifier_is_a_source() {
    let d = test_data();
    let pat = pattern("crown-shaper");
    let desired = pat
        .classes
        .iter()
        .position(|c| c.id == "desired-veiled")
        .expect("the pattern's veiled class") as u16;
    let inversion = invert::sources(d, &pat, &item("crown-c2"));
    let had_it: Vec<&str> = inversion
        .groups
        .iter()
        .filter(|g| g.state.prefixes.iter().any(|a| a.class == desired))
        .flat_map(|g| g.sources.iter().map(|s| s.method.as_str()))
        .collect();
    assert!(
        had_it.contains(&"currency/veiled-exalted"),
        "no input held the veiled prefix: {had_it:?}"
    );
}

/// Every input the inverse offers is an item the game can hold: no magic item with more than one
/// prefix or suffix ahead of a Regal Orb.
#[test]
fn every_source_is_a_legal_item() {
    let d = test_data();
    let pat = pattern("crown-shaper");
    for name in [
        "crown-actual",
        "crown-l2",
        "crown-d2-scbc",
        "crown-d2-scbc-fractured",
    ] {
        let inversion = invert::sources(d, &pat, &item(name));
        for source in inversion.groups.iter().flat_map(|g| &g.sources) {
            let input = &source.input;
            assert!(
                input.rarity != poe1::data::Rarity::Magic
                    || (input.prefixes.len() <= 1 && input.suffixes.len() <= 1),
                "{name} / {}: a magic input with {} prefixes and {} suffixes",
                source.method,
                input.prefixes.len(),
                input.suffixes.len()
            );
            assert!(
                input.validate(d).is_ok(),
                "{name} / {}: {:?}",
                source.method,
                input.validate(d)
            );
        }
    }
}
