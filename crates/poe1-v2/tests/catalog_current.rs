//! The committed `catalog/` is exactly what `render_all` produces from the bundle and fixtures,
//! and every unconfirmed rule that a method cites has a question example.

mod common;

use std::fs;
use std::path::Path;

use poe1_v2::catalog::{Fixtures, render_all, validate_examples};
use poe1_v2::test_data;

const STALE: &str = "catalog/ is stale; run: nix develop -c cargo run -p datagen-v2 --release -- catalog --bundle data/v2/bundle.json --fixtures fixtures/v2 --out catalog";

fn fixtures() -> Fixtures {
    Fixtures::load(test_data(), &common::root().join("fixtures/v2"))
}

#[test]
fn catalog_is_current() {
    let root = common::root();
    let want = render_all(test_data(), &root.join("fixtures/v2")).expect(STALE);
    let mut committed: Vec<String> = fs::read_dir(root.join("catalog"))
        .expect(STALE)
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    committed.sort();
    let produced: Vec<String> = want.keys().cloned().collect();
    assert_eq!(committed, produced, "{STALE}");
    for (name, content) in &want {
        let have = fs::read_to_string(root.join("catalog").join(name)).expect(STALE);
        assert!(have == *content, "{name}: {STALE}");
    }
}

#[test]
fn examples_cover_every_unconfirmed_rule() {
    let data = test_data();
    let fx = fixtures();
    validate_examples(data, &fx).expect("every unconfirmed rule a method cites has an example");
}

/// The owner deferred four rules: each is listed once, in the section at the end of
/// `questions.md`, and never as a question.
#[test]
fn deferred_rules_are_listed_once() {
    let root = common::root();
    let md = render_all(test_data(), &root.join("fixtures/v2")).expect(STALE);
    let questions = md.get("questions.md").expect("questions.md");
    let at = questions
        .find("## Deferred by the owner")
        .expect("the deferred section is missing");
    let (asked, deferred) = questions.split_at(at);
    for id in [
        "bench.reroll-unknown",
        "tainted.odds",
        "generation.multiplier",
        "metamod.level-28",
    ] {
        let quoted = format!("`{id}`");
        assert!(
            !asked.contains(&quoted),
            "{id} is still listed as a question"
        );
        assert_eq!(
            deferred.matches(&quoted).count(),
            1,
            "{id} is not listed exactly once as deferred"
        );
    }
}

#[test]
fn a_missing_example_is_an_error() {
    let data = test_data();
    let mut fx = fixtures();
    fx.examples.remove("reforge.guarantee-first");
    let err = validate_examples(data, &fx).unwrap_err().to_string();
    assert!(
        err.contains("no example for the unconfirmed rule `reforge.guarantee-first`"),
        "{err}"
    );
}

#[test]
fn an_example_must_cite_its_rule() {
    let data = test_data();
    let mut fx = fixtures();
    fx.examples
        .get_mut("reforge.guarantee-first")
        .unwrap()
        .first_mut()
        .unwrap()
        .method = "currency/fracturing".into();
    let err = validate_examples(data, &fx).unwrap_err().to_string();
    assert!(err.contains("does not cite it"), "{err}");
}

#[test]
fn an_example_fixture_must_load() {
    let data = test_data();
    let mut fx = fixtures();
    fx.examples
        .get_mut("reforge.guarantee-first")
        .unwrap()
        .first_mut()
        .unwrap()
        .fixtures = vec!["not-a-fixture".into()];
    let err = validate_examples(data, &fx).unwrap_err().to_string();
    assert!(err.contains("unknown fixture `not-a-fixture`"), "{err}");
}

#[test]
fn render_all_rejects_a_missing_example() {
    let root = common::root();
    let dir = tempfile::tempdir().expect("temp dir");
    copy_dir(&root.join("fixtures/v2/items"), &dir.path().join("items"));
    copy_dir(
        &root.join("fixtures/v2/patterns"),
        &dir.path().join("patterns"),
    );
    let questions: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("fixtures/v2/questions.json")).expect("questions"),
    )
    .expect("questions json");
    let mut questions = questions;
    questions["examples"]
        .as_object_mut()
        .expect("examples")
        .remove("reforge.guarantee-first");
    fs::write(
        dir.path().join("questions.json"),
        serde_json::to_vec(&questions).expect("encode"),
    )
    .expect("write questions");
    let err = render_all(test_data(), dir.path()).unwrap_err().to_string();
    assert!(
        err.contains("no example for the unconfirmed rule `reforge.guarantee-first`"),
        "{err}"
    );
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        fs::copy(entry.path(), to.join(entry.file_name())).expect("copy");
    }
}
