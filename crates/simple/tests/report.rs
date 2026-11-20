//! The report, pinned: `cargo run -p simple` has to print `craft.md`.
//!
//! Re-pin with `BLESS=1 cargo test -p simple --test report` when a change to the model is
//! intended, and read the diff: that is the only way the file changes.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn the_report_is_craft_md() {
    let output = Command::new(env!("CARGO_BIN_EXE_simple"))
        .output()
        .expect("the binary runs");
    assert!(
        output.status.success(),
        "the binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).expect("the report is text");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("craft.md");

    if std::env::var_os("BLESS").is_some() {
        std::fs::write(&path, &actual).expect("craft.md is writable");
        return;
    }

    let expected = std::fs::read_to_string(&path).expect("craft.md exists");
    if actual == expected {
        return;
    }
    for (line, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
        if want != got {
            panic!(
                "line {}: craft.md has\n  {want}\nand the model prints\n  {got}\n(BLESS=1 re-pins it)",
                line + 1
            );
        }
    }
    panic!(
        "craft.md has {} lines and the model prints {} (BLESS=1 re-pins it)",
        expected.lines().count(),
        actual.lines().count()
    );
}
