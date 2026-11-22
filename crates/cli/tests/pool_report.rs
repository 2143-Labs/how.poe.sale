//! `cli pool-report` writes the owner's shield as a document: every family and tier of the
//! natural explicit pool, with the exact weights the currency model divides by. The assertions
//! pin the two published side totals and the ids a reviewer checks the export against.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The document, written into a temporary directory.
fn report() -> String {
    let root = root();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let out = dir.path().join("totem-shield.md");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
        .current_dir(&root)
        .args([
            "pool-report",
            "--bundle",
            "data/bundle.json",
            "--base",
            "Supreme Spiked Shield",
            "--item-level",
            "87",
            "--influence",
            "shaper",
            "--out",
            out.to_str().expect("a utf-8 path"),
        ])
        .output()
        .expect("running cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(&out).expect("the written report")
}

/// The cells of a markdown table row, outer empties included.
fn cells(line: &str) -> Vec<&str> {
    line.split('|').map(str::trim).collect()
}

#[test]
fn documents_the_totem_shield_pool() {
    let text = report();
    assert!(text.contains("64,800"), "the prefix total:\n{text}");
    assert!(text.contains("96,475"), "the suffix total:\n{text}");

    let totem = text
        .lines()
        .find(|line| line.contains("`MaximumTotemUber1`"))
        .expect("the totem tier row");
    let totem = cells(totem);
    assert_eq!(totem.len(), 9, "{totem:?}");
    assert_eq!(totem[6], "800", "the totem's exact weight");

    // The life family, best tier first: the two highest tiers the pool can reach.
    let life: Vec<&str> = text
        .lines()
        .filter(|line| {
            let cells = cells(line);
            cells.len() == 9 && cells[3] == "IncreasedLife"
        })
        .collect();
    assert!(life.len() > 2, "the life family by tier:\n{text}");
    assert!(life[0].contains("`IncreasedLife10`"), "{}", life[0]);
    assert!(life[1].contains("`IncreasedLife9`"), "{}", life[1]);
}
