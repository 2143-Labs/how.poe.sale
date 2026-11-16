//! The baseline table, pinned: `cargo run -p simple` has to print what it printed before.
//!
//! Re-pin with `BLESS=1 cargo test -p simple --test depth` when a change to the model is intended —
//! that is the only way the file changes.
//!
//! The table is six depths of the loop, which is the whole loop's exact state space (it closes at
//! 952 518 classes); an unoptimized test build computes it in about a minute, an optimized one in a
//! few seconds, so `cargo test --release` is the cheap way to run this.

use std::path::PathBuf;
use std::process::Command;

/// The depths the pinned table covers.
const DEPTH: &str = "6";

#[test]
fn the_baseline_table_is_what_it_was() {
    let output = Command::new(env!("CARGO_BIN_EXE_simple"))
        .args(["--depth", DEPTH])
        .output()
        .expect("the baseline binary runs");
    assert!(
        output.status.success(),
        "the baseline binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).expect("the table is text");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/expected-table.txt");

    if std::env::var_os("BLESS").is_some() {
        std::fs::write(&path, &actual).expect("the table is writable");
        return;
    }

    let expected = std::fs::read_to_string(&path).expect("tests/expected-table.txt exists");
    if actual == expected {
        return;
    }
    for (line, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
        if want != got {
            panic!(
                "line {}: the table is\n  {want}\nand the model prints\n  {got}\n(BLESS=1 re-pins it)",
                line + 1
            );
        }
    }
    panic!(
        "the table has {} lines and the model prints {} (BLESS=1 re-pins it)",
        expected.lines().count(),
        actual.lines().count()
    );
}
