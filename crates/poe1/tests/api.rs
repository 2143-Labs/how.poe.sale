//! The browser's views: a target from a paste, and the one-step report as JSON.

mod common;

use common::{item, root};
use poe1::api;
use poe1::pattern::{self, Pattern};
use poe1::solve;
use poe1::test_data;

fn text(name: &str) -> String {
    let path = root().join(format!("fixtures/text/{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn pattern_json(name: &str) -> String {
    let path = root().join(format!("fixtures/patterns/{name}.json"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The Archdemon Crown paste has three prefixes and three suffixes and no veiled one, so its own
/// pattern has six desired classes, two catch-alls, and one slot per modifier; the paste then
/// matches it.
#[test]
fn a_pasted_target_is_its_own_goal() {
    let d = test_data();
    let crown = text("archdemon-crown");
    let spec = api::target(d, &crown).expect("a target");
    assert_eq!(spec.classes.len(), 8);
    assert_eq!(spec.prefixes.len(), 3);
    assert_eq!(spec.suffixes.len(), 3);
    let json = serde_json::to_string(&spec).unwrap();
    let report = api::solve(d, &json, &crown).expect("a report");
    assert_eq!(report.now.progress, 1.0);
    assert_eq!(report.now.matches, 1.0);
}

/// crown-c1's veiled prefix stands for a modifier not chosen yet, so it gets no class or slot.
#[test]
fn a_target_leaves_out_a_veiled_placeholder() {
    let d = test_data();
    let spec = pattern::from_item(d, &item("crown-c1"));
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
    let spec = pattern::from_item(d, &item("crown-l1"));
    assert_eq!(spec.prefixes.len() + spec.suffixes.len(), 5);
    let pat = Pattern::compile(d, &spec).expect("compiles");
    let progress = solve::item_progress(d, &pat, &item("crown-a"));
    assert!((progress - 9.0 / 15.0).abs() < 1e-12, "{progress}");
}

#[test]
fn the_report_accounts_for_every_method() {
    let d = test_data();
    let report =
        api::solve(d, &pattern_json("crown-shaper"), &text("archdemon-crown")).expect("a report");
    assert!(
        (report.now.progress - 10.0 / 12.0).abs() < 1e-12,
        "{}",
        report.now.progress
    );
    let single = d.methods.iter().filter(|m| m.inputs == 1).count();
    assert_eq!(
        report.exact.len() + report.certain.len() + report.blind.len() + report.unmet.len(),
        single
    );
    assert!(!report.exact[0].tied);
    assert!(!report.sources.groups.is_empty());
    let value = serde_json::to_value(&report).unwrap();
    assert!(
        value["exact"][0]["method"].is_string(),
        "{}",
        value["exact"][0]
    );
}

#[test]
fn errors_name_what_failed() {
    let d = test_data();
    let crown = text("archdemon-crown");
    let err = api::solve(d, "{", &crown).unwrap_err();
    assert!(err.starts_with("pattern: "), "{err}");
    let err = api::solve(d, &pattern_json("crown-shaper"), "hello").unwrap_err();
    assert_eq!(err, "item: no known base item in the pasted text");
    let mut spec: serde_json::Value = serde_json::from_str(&pattern_json("crown-shaper")).unwrap();
    spec["item"]["bases"] = serde_json::json!(["Nope"]);
    let err = api::solve(d, &spec.to_string(), &crown).unwrap_err();
    assert!(err.contains("unknown base \"Nope\""), "{err}");
}

#[test]
fn read_item_reports_its_warnings() {
    let d = test_data();
    let view = api::read_item(d, &text("archdemon-crown")).expect("an item");
    assert!(
        view.warnings
            .iter()
            .any(|w| w.contains("is the sum of two modifiers")),
        "{:?}",
        view.warnings
    );
}

#[test]
fn a_target_needs_a_modifier() {
    let d = test_data();
    assert_eq!(
        api::target_of(d, &item("cobalt-jewel-normal")),
        Err("target: the pasted item has no explicit modifier to aim for".to_string())
    );
}

/// The Simplex Amulet is not an Archdemon Crown: no step can move it toward the crown pattern.
#[test]
fn an_item_outside_the_filter_does_not_fit() {
    let d = test_data();
    let crown = pattern_json("crown-shaper");
    let amulet = api::solve(d, &crown, &text("simplex-amulet")).expect("a report");
    assert!(!amulet.now.fits);
    assert_eq!(amulet.now.progress, 0.0);
    let report = api::solve(d, &crown, &text("archdemon-crown")).expect("a report");
    assert!(report.now.fits);
}
