//! Patterns: the owner's two Archdemon Crown patterns against the Crown fixtures, value
//! thresholds on unrolled values, slot progress and compile errors.

mod common;

use common::{item, pattern, pattern_spec, spec};
use poe1_v2::data::Side;
use poe1_v2::item::{AffixSpec, Item};
use poe1_v2::pattern::{
    ClassSpec, Grade, MatcherSpec, Pattern, classify, evaluate, matches_probability,
};
use poe1_v2::test_data;

fn class_ids(p: &Pattern, classes: &[u16]) -> Vec<String> {
    let mut ids: Vec<String> = classes
        .iter()
        .map(|&c| p.classes[c as usize].id.clone())
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
