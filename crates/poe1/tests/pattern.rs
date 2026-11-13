//! Patterns: the owner's two Archdemon Crown patterns against the Crown fixtures, value
//! thresholds on unrolled values, slot progress, the solver-grade state and its flat lift, and
//! compile errors.

mod common;

use common::{item, pattern, pattern_spec, spec};
use poe1::apply::{Leaf, Outcome, Params, Pending, apply};
use poe1::data::{Data, Side};
use poe1::item::{AffixSpec, Item, MetaMod};
use poe1::method::PendingReason;
use poe1::pattern::{
    AffixState, ClassSpec, Grade, LiftedLeaf, MatcherSpec, Pattern, PatternSpec, classify,
    evaluate, lift, matches_probability,
};
use poe1::test_data;

/// `crown-shaper` with one prefix class `big-life` (T1 life at 137 or more) before the catch-all.
fn big_life_pattern(d: &Data) -> Pattern {
    let mut s = pattern_spec("crown-shaper");
    s.name = "big-life".into();
    let other = s
        .classes
        .iter()
        .position(|c| c.id == "any-other-prefix")
        .unwrap();
    s.classes.insert(
        other,
        ClassSpec {
            id: "big-life".into(),
            label: "137+ life".into(),
            side: Side::Prefix,
            grade: Grade::Good,
            matchers: vec![MatcherSpec::Mod {
                r#mod: "IncreasedLife9".into(),
                min: Some(vec![Some(137)]),
                or_better: false,
            }],
        },
    );
    // Only the new class and the catch-all can take the life prefix.
    s.classes.retain(|c| c.id != "good-prefix");
    s.prefixes = vec![vec!["big-life".into()]];
    Pattern::compile(d, &s).unwrap()
}

/// `crown-a` plus one unrolled `IncreasedLife9` prefix.
fn crown_a_life(d: &Data, values: Option<Vec<i32>>) -> Item {
    let mut s = spec("crown-a");
    s.prefixes.push(AffixSpec {
        id: "IncreasedLife9".into(),
        values,
        fractured: false,
    });
    Item::from_spec(d, &s).unwrap()
}

fn affix_index(d: &Data, item: &Item, side: Side, id: &str) -> usize {
    item.affixes(side)
        .iter()
        .position(|a| d.mod_at(a.roll.m).id == id)
        .unwrap_or_else(|| panic!("no {side:?} affix {id}"))
}

fn class_ids(p: &Pattern, affixes: &[AffixState]) -> Vec<String> {
    let mut ids: Vec<String> = affixes
        .iter()
        .map(|a| p.classes[a.class as usize].id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn crown_actual_vs_shaper() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let states = evaluate(d, &p, &item("crown-actual"));
    assert_eq!(states.len(), 1);
    let (prob, s) = &states[0];
    assert_eq!(*prob, 1.0);
    assert!(s.item_ok);
    assert_eq!(
        class_ids(&p, &s.prefixes),
        ["any-other-prefix", "desired-veiled", "ok-prefix"]
    );
    assert_eq!(
        class_ids(&p, &s.suffixes),
        ["essence-elemental", "hypothermia", "spell-crit"]
    );
    // The third prefix (88% Armour and Energy Shield) is not crafted.
    assert!(!s.matches);
}

#[test]
fn crown_ideal_matches_shaper() {
    let d = test_data();
    assert_eq!(
        matches_probability(d, &pattern("crown-shaper"), &item("crown-ideal")),
        1.0
    );
}

#[test]
fn shaper_elder() {
    let d = test_data();
    let se = pattern("crown-shaper-elder");
    let shaper = pattern("crown-shaper");
    let ideal = item("crown-shaper-elder-ideal");
    assert_eq!(matches_probability(d, &se, &ideal), 1.0);
    let states = evaluate(d, &shaper, &ideal);
    assert!(states.iter().all(|(_, s)| !s.item_ok));
    let states = evaluate(d, &se, &item("crown-ideal"));
    assert!(states.iter().all(|(_, s)| !s.item_ok));
}

#[test]
fn value_threshold_splits() {
    let d = test_data();
    let mut s = pattern_spec("crown-shaper");
    s.name = "big-life".into();
    let other = s
        .classes
        .iter()
        .position(|c| c.id == "any-other-prefix")
        .unwrap();
    s.classes.insert(
        other,
        ClassSpec {
            id: "big-life".into(),
            label: "137+ life".into(),
            side: Side::Prefix,
            grade: Grade::Good,
            matchers: vec![MatcherSpec::Mod {
                r#mod: "IncreasedLife9".into(),
                min: Some(vec![Some(137)]),
                or_better: false,
            }],
        },
    );
    // Only the new class and the catch-all can take the life prefix.
    s.classes.retain(|c| c.id != "good-prefix");
    s.prefixes = vec![vec!["big-life".into()]];
    let p = Pattern::compile(d, &s).unwrap();
    let mut spec = spec("crown-a");
    spec.prefixes.push(AffixSpec {
        id: "IncreasedLife9".into(),
        values: None,
        fractured: false,
    });
    let it = Item::from_spec(d, &spec).unwrap();
    let got = classify(d, &p, &it, Side::Prefix, &it.prefixes[0]);
    let big = p.class_index("big-life").unwrap();
    let any = p.class_index("any-other-prefix").unwrap();
    // 137..=144 is 8 of the 15 values 130..=144.
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0, big);
    assert!((got[0].1 - 8.0 / 15.0).abs() < 1e-12);
    assert_eq!(got[1].0, any);
    assert!((got[1].1 - 7.0 / 15.0).abs() < 1e-12);
}

/// P1: a class takes the union of its matchers, so a first threshold does not swallow the rest.
#[test]
fn union_of_threshold_matchers() {
    let d = test_data();
    let mut s = pattern_spec("crown-shaper");
    let other = s
        .classes
        .iter()
        .position(|c| c.id == "any-other-prefix")
        .unwrap();
    s.classes.insert(
        other,
        ClassSpec {
            id: "life-union".into(),
            label: "T1 life".into(),
            side: Side::Prefix,
            grade: Grade::Good,
            matchers: vec![
                // Only 144 meets this one; the second matcher widens it to 137..=144.
                MatcherSpec::Mod {
                    r#mod: "IncreasedLife9".into(),
                    min: Some(vec![Some(144)]),
                    or_better: false,
                },
                MatcherSpec::Mod {
                    r#mod: "IncreasedLife9".into(),
                    min: Some(vec![Some(137)]),
                    or_better: false,
                },
            ],
        },
    );
    s.classes.retain(|c| c.id != "good-prefix");
    s.prefixes = vec![vec!["life-union".into()]];
    let p = Pattern::compile(d, &s).unwrap();
    let it = crown_a_life(d, None);
    let got = classify(d, &p, &it, Side::Prefix, &it.prefixes[0]);
    let life = p.class_index("life-union").unwrap();
    let any = p.class_index("any-other-prefix").unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0, life);
    // The union of 144-only and 137..=144 is 137..=144: 8 of the 15 values.
    assert!((got[0].1 - 8.0 / 15.0).abs() < 1e-12);
    assert_eq!(got[1].0, any);
    assert!((got[1].1 - 7.0 / 15.0).abs() < 1e-12);
}

/// P1: a matcher that accepts everything makes its class certain, even behind a threshold.
#[test]
fn catch_all_matcher_wins_over_threshold() {
    let d = test_data();
    let mut s = pattern_spec("crown-shaper");
    let other = s
        .classes
        .iter()
        .position(|c| c.id == "any-other-prefix")
        .unwrap();
    s.classes.insert(
        other,
        ClassSpec {
            id: "life-or-any".into(),
            label: "T1 life or anything".into(),
            side: Side::Prefix,
            grade: Grade::Good,
            matchers: vec![
                MatcherSpec::Mod {
                    r#mod: "IncreasedLife9".into(),
                    min: Some(vec![Some(144)]),
                    or_better: false,
                },
                MatcherSpec::Any { any: true },
            ],
        },
    );
    s.classes
        .retain(|c| c.id != "good-prefix" && c.id != "any-other-prefix");
    s.prefixes = vec![vec!["life-or-any".into()]];
    let p = Pattern::compile(d, &s).unwrap();
    let it = crown_a_life(d, Some(vec![137]));
    let got = classify(d, &p, &it, Side::Prefix, &it.prefixes[0]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, p.class_index("life-or-any").unwrap());
    assert_eq!(got[0].1, 1.0);
    // Evaluating with the catch-all on the class itself must not panic.
    let states = evaluate(d, &p, &it);
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].0, 1.0);
}

/// P1: with a catch-all class every side's classification carries all its mass.
#[test]
fn classify_mass_sums_to_one() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let it = item("crown-actual");
    for side in Side::BOTH {
        for affix in it.affixes(side) {
            let sum: f64 = classify(d, &p, &it, side, affix)
                .iter()
                .map(|(_, p)| p)
                .sum();
            assert!((sum - 1.0).abs() < 1e-12, "{side:?} {sum}");
        }
    }
}

/// P2: an implicit threshold on an unrolled value splits the state into branches.
#[test]
fn implicit_threshold_splits() {
    let d = test_data();
    let mut s = pattern_spec("crown-shaper");
    s.item.bases = vec!["Lapis Amulet".into()];
    s.item.item_level_min = 84;
    s.item.rarity = poe1::data::Rarity::Normal;
    s.item.influences.clear();
    s.implicits = vec![MatcherSpec::Mod {
        r#mod: "IntelligenceImplicitAmulet1".into(),
        min: Some(vec![Some(26)]),
        or_better: false,
    }];
    let p = Pattern::compile(d, &s).unwrap();
    let states = evaluate(d, &p, &item("lapis-amulet-normal"));
    assert_eq!(states.len(), 2);
    let ok: f64 = states
        .iter()
        .filter(|(_, s)| s.item_ok)
        .map(|(p, _)| p)
        .sum();
    let not: f64 = states
        .iter()
        .filter(|(_, s)| !s.item_ok)
        .map(|(p, _)| p)
        .sum();
    // The implicit rolls 20..=30; 26..=30 is 5 of the 11 values.
    assert!((ok - 5.0 / 11.0).abs() < 1e-12, "{ok}");
    assert!((not - 6.0 / 11.0).abs() < 1e-12, "{not}");
}

/// P3: thresholds are in the units the game shows, so the stat's translation factor counts.
#[test]
fn threshold_uses_shown_value() {
    let d = test_data();
    let mut s = pattern_spec("crown-shaper");
    let other = s
        .classes
        .iter()
        .position(|c| c.id == "any-other-suffix")
        .unwrap();
    s.classes.insert(
        other,
        ClassSpec {
            id: "crit-shown".into(),
            label: "1% spell crit".into(),
            side: Side::Suffix,
            grade: Grade::Good,
            matchers: vec![MatcherSpec::Mod {
                r#mod: "AdditionalCritWithAttacksInfluence1_".into(),
                min: Some(vec![Some(1)]),
                or_better: false,
            }],
        },
    );
    s.suffixes = vec![vec!["crit-shown".into()]];
    let p = Pattern::compile(d, &s).unwrap();
    let mut spec = spec("slink-boots-eldritch");
    spec.suffixes.push(AffixSpec {
        id: "AdditionalCritWithAttacksInfluence1_".into(),
        values: None,
        fractured: false,
    });
    let it = Item::from_spec(d, &spec).unwrap();
    let i = affix_index(d, &it, Side::Suffix, "AdditionalCritWithAttacksInfluence1_");
    let got = classify(d, &p, &it, Side::Suffix, &it.suffixes[i]);
    let crit = p.class_index("crit-shown").unwrap();
    let any = p.class_index("any-other-suffix").unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0, crit);
    // The stat is 50..=100 stored, shown at 0.01x: only 100 shows +1%.
    assert!((got[0].1 - 1.0 / 51.0).abs() < 1e-12, "{:?}", got[0].1);
    assert_eq!(got[1].0, any);
    assert!((got[1].1 - 50.0 / 51.0).abs() < 1e-12);
}

/// P4: `missing` prefers leaving the slot that accepts the most classes unfilled.
#[test]
fn missing_prefers_flexible_slot() {
    let d = test_data();
    let mut s = pattern_spec("crown-shaper");
    s.classes.retain(|c| c.side != Side::Prefix);
    s.classes.insert(
        0,
        ClassSpec {
            id: "a".into(),
            label: "life".into(),
            side: Side::Prefix,
            grade: Grade::Good,
            matchers: vec![MatcherSpec::Mod {
                r#mod: "IncreasedLife9".into(),
                min: None,
                or_better: false,
            }],
        },
    );
    s.classes.insert(
        1,
        ClassSpec {
            id: "b".into(),
            label: "area".into(),
            side: Side::Prefix,
            grade: Grade::Middling,
            matchers: vec![MatcherSpec::Mod {
                r#mod: "AreaOfEffectSupportedUber1_".into(),
                min: None,
                or_better: false,
            }],
        },
    );
    s.classes.insert(
        2,
        ClassSpec {
            id: "any-other-prefix".into(),
            label: "Any Other Prefix".into(),
            side: Side::Prefix,
            grade: Grade::Bad,
            matchers: vec![MatcherSpec::Any { any: true }],
        },
    );
    // Slot 0 takes only life, slot 1 takes both, slot 2 takes only area: filling slots 0 and 2
    // and leaving the flexible slot 1 empty is preferred.
    s.prefixes = vec![
        vec!["a".into()],
        vec!["a".into(), "b".into()],
        vec!["b".into()],
    ];
    let p = Pattern::compile(d, &s).unwrap();
    let mut spec = spec("crown-a");
    spec.prefixes.push(AffixSpec {
        id: "IncreasedLife9".into(),
        values: Some(vec![137]),
        fractured: false,
    });
    spec.prefixes.push(AffixSpec {
        id: "AreaOfEffectSupportedUber1_".into(),
        values: Some(vec![8, 16]),
        fractured: false,
    });
    let it = Item::from_spec(d, &spec).unwrap();
    let states = evaluate(d, &p, &it);
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].1.missing, [vec![1], vec![]]);
    assert_eq!(states[0].1.unused, [0, 0]);
}

/// One certain leaf.
fn single(leaf: Leaf) -> Outcome {
    Outcome {
        leaves: vec![(1.0, leaf)],
    }
}

/// P4: `lift` keeps the whole distribution of every item in a multi-item outcome.
#[test]
fn lift_many_keeps_distribution() {
    let d = test_data();
    let p = big_life_pattern(d);
    let it = crown_a_life(d, None);
    let lifted = lift(d, &p, &single(Leaf::Items(vec![it])));
    let [(mass, LiftedLeaf::Many(items))] = lifted.leaves.as_slice() else {
        panic!("not one Many: {lifted:?}")
    };
    assert_eq!(*mass, 1.0);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].len(), 2);
    assert!((items[0][0].0 - 8.0 / 15.0).abs() < 1e-12);
    assert!((items[0][1].0 - 7.0 / 15.0).abs() < 1e-12);
}

/// P4: a reforge's certain part lifts to one Pending leaf per state it can be in; a result
/// stopped before its step lifts to no known state at all.
#[test]
fn lift_pending_keeps_distribution() {
    let d = test_data();
    let p = big_life_pattern(d);
    let it = crown_a_life(d, None);
    let pending = |certain: bool| {
        single(Leaf::Pending(Pending {
            inputs: vec![it.clone()],
            params: Params::default(),
            item: it.clone(),
            certain,
            ops: Vec::new(),
            reason: PendingReason::MultiDraw,
        }))
    };
    let lifted = lift(d, &p, &pending(true));
    assert_eq!(lifted.leaves.len(), 2);
    assert!((lifted.mass() - 1.0).abs() < 1e-12);
    assert!(
        lifted
            .leaves
            .iter()
            .all(|(_, l)| matches!(l, LiftedLeaf::Pending { known: Some(_), .. }))
    );
    let before = lift(d, &p, &pending(false));
    assert_eq!(
        before.leaves,
        [(
            1.0,
            LiftedLeaf::Pending {
                reason: PendingReason::MultiDraw,
                known: None
            }
        )]
    );
}

/// P4: Choice options are sorted canonically, so order of production does not matter.
#[test]
fn choice_options_sorted() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let a = single(Leaf::Item(item("crown-a")));
    let b = single(Leaf::Item(item("crown-a-mana")));
    let one = lift(d, &p, &single(Leaf::Choice(vec![a.clone(), b.clone()])));
    let two = lift(d, &p, &single(Leaf::Choice(vec![b, a])));
    assert_eq!(one, two);
}

/// A2: the lifted state keys what the next step reads. The two Crowns hold the same classes,
/// but one's life prefix is fractured: Annulment gives 3 outcomes at 1/3 on the first and 2 at
/// 1/2 on the second, so their states must differ.
#[test]
fn fractured_and_locks_split_states() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let plain = evaluate(d, &p, &item("crown-d2-scbc"));
    let fractured = evaluate(d, &p, &item("crown-d2-scbc-fractured"));
    assert_eq!((plain.len(), fractured.len()), (1, 1));
    let (a, b) = (&plain[0].1, &fractured[0].1);
    assert_ne!(a, b);
    assert_eq!(class_ids(&p, &a.prefixes), class_ids(&p, &b.prefixes));
    assert_eq!(
        a.prefixes.iter().filter(|x| x.fractured).count() + 1,
        b.prefixes.iter().filter(|x| x.fractured).count()
    );
    // The lifts of the two fixtures as outcomes differ the same way.
    let lift_of = |name: &str| lift(d, &p, &single(Leaf::Item(item(name))));
    assert_ne!(lift_of("crown-d2-scbc"), lift_of("crown-d2-scbc-fractured"));
    // Suffixes Cannot Be Changed shows as a lock and a metamod; a crafted mana prefix in its
    // place has neither, and fills the crafted class the metamod does not.
    assert_eq!(a.locks, [false, true]);
    assert_eq!(a.metamods, [MetaMod::LockSuffixes]);
    let l1 = &evaluate(d, &p, &item("crown-l1"))[0].1;
    let mut s = spec("crown-l1");
    s.prefixes
        .retain(|x| x.id != "DexMasterItemGenerationCannotChangeSuffixes");
    s.prefixes.push(AffixSpec {
        id: "EinharMasterIncreasedMana2".into(),
        values: Some(vec![40]),
        fractured: false,
    });
    let mana = &evaluate(d, &p, &Item::from_spec(d, &s).unwrap())[0].1;
    assert_ne!(l1, mana);
    assert_eq!(mana.locks, [false, false]);
    // Both classes are catch-alls and carry the mod groups; the metamod lands in the plain
    // catch-all, the bench craft in the crafted class.
    let crafted = p.class_index("any-crafted-prefix").unwrap();
    let other = p.class_index("any-other-prefix").unwrap();
    let held = |s: &poe1::pattern::PatternState, class: u16| {
        s.prefixes
            .iter()
            .find(|a| a.class == class)
            .map(|a| (a.crafted, a.groups.clone()))
    };
    assert!(
        held(l1, crafted).is_none(),
        "the metamod filled the crafted class"
    );
    assert_eq!(held(l1, other).map(|(crafted, _)| crafted), Some(true));
    let (is_crafted, mana_groups) = held(mana, crafted).expect("the crafted class");
    assert!(is_crafted);
    let (_, metamod_groups) = held(l1, other).expect("the catch-all");
    assert_ne!(mana_groups, metamod_groups);
}

/// The lift of a real outcome is flat and carries all its mass.
#[test]
fn lift_of_exalt_is_flat() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let m = d.method("currency/exalted").unwrap();
    let out = apply(d, m, &[item("crown-a-mana")], &Params::default()).unwrap();
    let lifted = lift(d, &p, &out);
    assert!((lifted.mass() - 1.0).abs() < 1e-12);
    assert!(
        lifted
            .leaves
            .iter()
            .all(|(_, l)| matches!(l, LiftedLeaf::State(_)))
    );
    // Distinct states only.
    for (i, (_, a)) in lifted.leaves.iter().enumerate() {
        for (_, b) in &lifted.leaves[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn progress() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let states = evaluate(d, &p, &item("crown-a"));
    assert_eq!(states.len(), 1);
    let s = &states[0].1;
    assert_eq!(s.missing, [vec![0, 1, 2], vec![]]);
    assert_eq!(s.unused, [0, 0]);
    assert_eq!(s.open, [3, 0]);
    let s = &evaluate(d, &p, &item("crown-actual"))[0].1;
    assert_eq!(s.missing, [vec![2], vec![]]);
    assert_eq!(s.unused, [1, 0]);
}

#[test]
fn compile_errors() {
    let d = test_data();
    let err = |s| Pattern::compile(d, &s).unwrap_err().to_string();

    let mut s = pattern_spec("crown-shaper");
    s.classes[0].matchers.push(MatcherSpec::Mod {
        r#mod: "NoSuchMod".into(),
        min: None,
        or_better: false,
    });
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": unknown modifier \"NoSuchMod\""
    );

    let mut s = pattern_spec("crown-shaper");
    s.prefixes[0].push("no-such-class".into());
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": slot names unknown class \"no-such-class\""
    );

    let mut s = pattern_spec("crown-shaper");
    s.suffixes[0].push("desired-veiled".into());
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": class \"desired-veiled\" is a prefix class but sits in a suffix slot"
    );

    let mut s = pattern_spec("crown-shaper");
    s.classes.retain(|c| c.id != "any-other-suffix");
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": the suffix classes need a catch-all class ({\"any\": true})"
    );
}

/// P4: the remaining compile checks: bases, mod side, empty slots, duplicate ids and the rare
/// affix limit.
#[test]
fn compile_rejects_more() {
    let d = test_data();
    let err = |s| Pattern::compile(d, &s).unwrap_err().to_string();

    let mut s = pattern_spec("crown-shaper");
    s.item.bases = vec!["No Such Base".into()];
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": unknown base \"No Such Base\""
    );

    let mut s = pattern_spec("crown-shaper");
    s.classes.push(ClassSpec {
        id: "bad-side".into(),
        label: "Suffix mod in a prefix class".into(),
        side: Side::Prefix,
        grade: Grade::Good,
        matchers: vec![MatcherSpec::Mod {
            r#mod: "IncreasedChillEffectSupportedUber2".into(),
            min: None,
            or_better: false,
        }],
    });
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": modifier \"IncreasedChillEffectSupportedUber2\" is a suffix modifier but sits in a prefix class"
    );

    let mut s = pattern_spec("crown-shaper");
    s.prefixes.push(Vec::new());
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": prefix slot 4 names no class"
    );

    let mut s = pattern_spec("crown-shaper");
    let first = s.classes[0].clone();
    s.classes.push(first);
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": duplicate class \"desired-veiled\""
    );

    let mut s = pattern_spec("crown-shaper");
    s.prefixes.push(vec!["any-crafted-prefix".into()]);
    assert_eq!(
        err(s),
        "pattern \"crown-shaper\": prefix slots exceed what Archdemon Crown allows (3)"
    );
}

/// A pattern matches the one placeholder a new Talisman carries, and a Talisman without it does
/// not pass the pattern's filter.
#[test]
fn talisman_placeholder_is_a_mod_matcher() {
    let d = test_data();
    let talisman_spec: PatternSpec = serde_json::from_str(
        r#"{
            "name": "talisman",
            "item": {
                "bases": ["Chieftain Talisman"],
                "itemLevelMin": 80,
                "rarity": "rare",
                "influences": []
            },
            "implicits": [{"mod": "TalismanImplicitPlaceholder"}],
            "classes": [
                {"id": "any-other-prefix", "label": "Any Other Prefix", "side": "prefix", "grade": "bad", "matchers": [{"any": true}]},
                {"id": "any-other-suffix", "label": "Any Other Suffix", "side": "suffix", "grade": "bad", "matchers": [{"any": true}]}
            ],
            "prefixes": [["any-other-prefix"]],
            "suffixes": [["any-other-suffix"]]
        }"#,
    )
    .unwrap();
    let pattern = Pattern::compile(d, &talisman_spec).unwrap();
    let states = evaluate(d, &pattern, &item("talisman-rare-four"));
    assert_eq!(states.len(), 1);
    assert!(states[0].1.item_ok);
    assert_eq!(states[0].0, 1.0);

    let mut bare = spec("talisman-rare-four");
    bare.implicits.clear();
    let bare = Item::from_spec(d, &bare).unwrap();
    let states = evaluate(d, &pattern, &bare);
    assert!(states.iter().all(|(_, s)| !s.item_ok));
}

/// crown-c1's veiled prefix stands for a modifier not chosen yet, so it gets no class or slot.
#[test]
fn a_target_leaves_out_a_veiled_placeholder() {
    let d = test_data();
    let spec = poe1::pattern::from_item(d, &item("crown-c1"));
    let ids: Vec<&str> = spec.classes.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "prefix-1",
            "other-prefix",
            "suffix-1",
            "suffix-2",
            "suffix-3",
            "other-suffix"
        ]
    );
    Pattern::compile(d, &spec).expect("compiles");
}

/// crown-l1 has five modifiers that are not veiled placeholders: five desired slots worth three
/// each. crown-a holds the same three suffixes and no prefix, so it holds 9 of the 15.
#[test]
fn a_target_scores_by_its_own_slots() {
    let d = test_data();
    let spec = poe1::pattern::from_item(d, &item("crown-l1"));
    assert_eq!(spec.prefixes.len() + spec.suffixes.len(), 5);
    let pat = Pattern::compile(d, &spec).expect("compiles");
    let progress = poe1::solve::item_progress(d, &pat, &item("crown-a"));
    assert!((progress - 9.0 / 15.0).abs() < 1e-12, "{progress}");
}

/// A metamod is a crafted modifier, but not the modifier `{"crafted": true}` is looking for: it is
/// a tool the bench removes for one scour. So a Crown whose only crafted prefix is Suffixes
/// Cannot Be Changed does not fill `any-crafted-prefix`, while one with a real bench craft does.
#[test]
fn a_metamod_does_not_fill_a_crafted_class() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let crafted = p
        .classes
        .iter()
        .position(|c| c.id == "any-crafted-prefix")
        .expect("the crafted class") as u16;
    let classes = |name: &str| {
        let it = item(name);
        it.prefixes
            .iter()
            .filter(|a| d.mod_at(a.roll.m).kind == poe1::data::ModKind::Crafted)
            .flat_map(|a| classify(d, &p, &it, Side::Prefix, a))
            .map(|(c, _)| c)
            .collect::<Vec<_>>()
    };
    assert!(
        !classes("crown-c2").contains(&crafted),
        "Suffixes Cannot Be Changed filled the crafted class"
    );
    assert!(
        classes("crown-a-mana").contains(&crafted),
        "the crafted mana prefix did not fill it"
    );
}

/// The Hypothermia ladder as the pattern ships it: `orBetter` takes the named tier and every
/// higher tier of the same family and nothing lower, so Level 18 or better is good enough and the
/// elevated Level 25 is best possible. There is no Level 22 tier.
#[test]
fn or_better_takes_the_family_at_this_tier_and_above() {
    let d = test_data();
    let p = pattern("crown-shaper");
    let class = |id: &str| p.classes.iter().position(|c| c.id == id).expect(id) as u16;
    let (good, best) = (class("hypothermia"), class("hypothermia-best"));
    let takes = |id: &str, want: u16| {
        let mut it = spec("crown-a");
        it.suffixes = vec![AffixSpec {
            id: id.into(),
            values: None,
            fractured: false,
        }];
        let it = Item::from_spec(d, &it).expect("an item");
        classify(d, &p, &it, Side::Suffix, &it.suffixes[0])
            .iter()
            .any(|(c, _)| *c == want)
    };
    assert!(
        !takes("IncreasedChillEffectSupportedUber1", good),
        "Level 16"
    );
    assert!(
        !takes("IncreasedChillEffectSupportedUber1", best),
        "Level 16"
    );
    assert!(
        takes("IncreasedChillEffectSupportedUber2", good),
        "Level 18"
    );
    assert!(
        takes("IncreasedChillEffectSupportedUber3", good),
        "Level 20"
    );
    assert!(
        takes("IncreasedChillEffectSupportedUberMaven", best),
        "Level 25"
    );
    assert!(
        !takes("IncreasedChillEffectSupportedUberMaven", good),
        "Level 25 took the good-enough class as well"
    );
}
