//! The crown audit, pinned: `cli solve` and `cli sources` on every state of the owner's Archdemon
//! Crown craft, compared with the reports checked in under `fixtures/crown-audit/`. A changed
//! number is a finding to read before it is blessed.
//!
//! `BLESS=1 cargo test -p cli --release --test crown_audit` rewrites the files.

use std::path::{Path, PathBuf};

const BUNDLE: &str =
    "cargo run -p cli --release -- bundle --raw data/raw --rules rules --out data/bundle.json";

/// (pattern, item fixture): every crown state the audit read, the four the pattern refuses
/// (corrupted, uninfluenced) included as negative controls.
const STATES: &[(&str, &str)] = &[
    ("crown-shaper", "crown-a"),
    ("crown-shaper", "crown-a-mana"),
    ("crown-shaper", "crown-actual"),
    ("crown-shaper", "crown-c1"),
    ("crown-shaper", "crown-c2"),
    ("crown-shaper", "crown-c2-blocked"),
    ("crown-shaper", "crown-d2-scbc"),
    ("crown-shaper", "crown-d2-scbc-fractured"),
    ("crown-shaper", "crown-ideal"),
    ("crown-shaper", "crown-l1"),
    ("crown-shaper", "crown-l2"),
    ("crown-shaper-elder", "crown-shaper-elder-ideal"),
    ("crown-shaper", "crown-a-corrupted"),
    ("crown-shaper", "crown-actual-corrupted"),
    ("crown-shaper", "crown-d2-scbc-corrupted"),
    ("crown-shaper", "crown-uninfluenced-fracture"),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn report(root: &Path, command: &str, pattern: &str, item: &str) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
        .current_dir(root)
        .args([
            command,
            "--bundle",
            "data/bundle.json",
            "--fixtures",
            "fixtures",
            "--pattern",
            pattern,
            "--item",
            item,
        ])
        .output()
        .expect("running cli");
    assert!(
        out.status.success(),
        "cli {command} --pattern {pattern} --item {item}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 report")
}

/// The first line two texts differ on, numbered from one.
fn first_difference(want: &str, have: &str) -> String {
    let (mut w, mut h) = (want.lines(), have.lines());
    for n in 1.. {
        match (w.next(), h.next()) {
            (None, None) => return "the texts differ only in line endings".into(),
            (a, b) if a == b => continue,
            (a, b) => {
                return format!(
                    "line {n}:\n  checked in: {}\n  now:        {}",
                    a.unwrap_or("<end of file>"),
                    b.unwrap_or("<end of file>")
                );
            }
        }
    }
    unreachable!()
}

#[test]
fn crown_audit_reports_are_unchanged() {
    let root = root();
    assert!(
        root.join("data/bundle.json").is_file(),
        "data/bundle.json is missing; build it from the repository root with: {BUNDLE}"
    );
    // `BLESS=0` is off, not on: a value that says "do not" must not rewrite the files.
    let bless = matches!(std::env::var("BLESS").as_deref(), Ok(v) if !v.is_empty() && v != "0");
    let dir = root.join("fixtures/crown-audit");
    if bless {
        std::fs::create_dir_all(&dir).expect("creating fixtures/crown-audit");
    }
    let mut failures = Vec::new();
    for (pattern, item) in STATES {
        for command in ["solve", "sources"] {
            let have = report(&root, command, pattern, item);
            let path = dir.join(format!("{item}.{command}.txt"));
            if bless {
                std::fs::write(&path, &have).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(want) if want == have => {}
                Ok(want) => failures.push(format!(
                    "{}: {}",
                    path.display(),
                    first_difference(&want, &have)
                )),
                Err(e) => failures.push(format!("{}: {e}", path.display())),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} reports changed (BLESS=1 rewrites them once the change is understood):\n{}",
        failures.len(),
        STATES.len() * 2,
        failures.join("\n")
    );
}
