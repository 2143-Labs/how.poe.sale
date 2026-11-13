//! The crown audit, pinned: `cli solve` and `cli sources` on every state of the owner's Archdemon
//! Crown craft, compared with the reports checked in under `fixtures/crown-audit/`. A changed
//! number is a finding to read before it is blessed.
//!
//! `BLESS=1 cargo test -p cli --release --test crown_audit` rewrites the files, on a branch: on
//! `main` it refuses, because the checked-in report is what every other branch is compared with.

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

/// Lines of this file's step table, without the step and cost cells that differ for reasons
/// nobody reads a diff for. The last five cells are `Finish Goal Better Worse Blocked`.
const TABLE_CELLS: usize = 9;

/// The five numbers of a step-table row, when the line is one.
fn row_numbers(line: &str) -> Option<String> {
    let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
    if cells.len() < TABLE_CELLS {
        return None;
    }
    let tail = &cells[cells.len() - 5..];
    tail.iter()
        .all(|c| c.trim().parse::<f64>().is_ok())
        .then(|| tail.iter().map(|c| c.trim()).collect::<Vec<_>>().join(" "))
}

/// At most this many differing lines, because a golden diff is read, not scrolled.
const DIFF_LINES: usize = 6;

/// Where two reports differ, reduced to the numbers when the lines are table rows.
fn differences(want: &str, have: &str) -> String {
    let mut out = Vec::new();
    let mut more = 0usize;
    for (n, (a, b)) in want.lines().zip(have.lines()).enumerate() {
        if a == b {
            continue;
        }
        if out.len() == DIFF_LINES {
            more += 1;
            continue;
        }
        match (row_numbers(a), row_numbers(b)) {
            (Some(wa), Some(wb)) => out.push(format!(
                "line {}: Finish Goal Better Worse Blocked\n  checked in: {wa}\n  now:        {wb}",
                n + 1
            )),
            _ => out.push(format!(
                "line {}:\n  checked in: {a}\n  now:        {b}",
                n + 1
            )),
        }
    }
    let (wl, hl) = (want.lines().count(), have.lines().count());
    if wl != hl {
        more += wl.abs_diff(hl);
    }
    if more > 0 {
        out.push(format!("… and {more} more differing lines"));
    }
    if out.is_empty() {
        "the texts differ only in line endings".into()
    } else {
        out.join("\n")
    }
}

/// Whether `BLESS=1` may rewrite the goldens. On `main` it may not: the checked-in report is what
/// every other branch is compared with. A detached head or a missing git cannot be `main`.
fn bless_allowed(branch: Option<&str>) -> bool {
    branch != Some("main")
}

/// The branch `HEAD` is on, when git can be asked. `None` for a detached head or no repository.
fn head_branch(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!name.is_empty() && name != "HEAD").then_some(name)
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
    if bless {
        // A blessed file on the branch everyone builds from rewrites the reference itself, so the
        // next reader has nothing to compare against. Bless on a branch, then read the diff.
        assert!(
            bless_allowed(head_branch(&root).as_deref()),
            "BLESS=1 refuses to rewrite the goldens on main: the checked-in report is what every \
             other branch is compared with. Bless on a branch and read the diff."
        );
    }
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
                    "{}:\n{}",
                    path.display(),
                    differences(&want, &have)
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

#[test]
fn bless_refuses_main_and_its_aliases() {
    assert!(!bless_allowed(Some("main")), "main must not be blessed");
    assert!(bless_allowed(Some("post-audit-fixes")));
    assert!(bless_allowed(None), "a detached head or no git is not main");
}
