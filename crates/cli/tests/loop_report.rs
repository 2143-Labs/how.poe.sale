//! `cli loop-report` runs the owner's loop on the exact lumped state and prints one section per
//! depth. The assertions pin the header, the first depth's goal share (exactly 1/108, which prints
//! as 0.925926%), the number of depths, and the conservation line.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The report flags both tests need, with absolute paths so the command does not depend on the
/// directory it runs from.
fn report_args() -> Vec<String> {
    let root = root();
    [
        "loop-report",
        "--bundle",
        &root.join("data/bundle.json").display().to_string(),
        "--fixtures",
        &root.join("fixtures").display().to_string(),
        "--pattern",
        "totem-shield",
        "--base",
        "Supreme Spiked Shield",
        "--item-level",
        "87",
        "--influence",
        "shaper",
        "--depth",
        "3",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// The report, captured from the built binary.
fn report_from(dir: &std::path::Path) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
        .current_dir(dir)
        .args(report_args())
        .output()
        .expect("running cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 report")
}

fn report() -> String {
    report_from(&root())
}

#[test]
fn reports_the_lumped_loop_depth_by_depth() {
    let text = report();
    assert!(text.contains("totem-shield"), "the pattern name:\n{text}");
    assert!(
        text.contains("Supreme Spiked Shield"),
        "the base name:\n{text}"
    );

    // Depth 1 is one transmutation of the normal shield. The goal absorbs exactly 1/108:
    // 100/108 = 0.925925…, printed to six decimals as 0.925926%.
    let first = text
        .split("## Depth 2")
        .next()
        .and_then(|head| head.split("## Depth 1").nth(1))
        .expect("a depth-1 section");
    let goal = first
        .lines()
        .find(|line| line.contains("| `desired` |"))
        .unwrap_or_else(|| panic!("the depth-1 goal row:\n{first}"));
    assert!(goal.contains("1/108"), "the exact goal mass: {goal}");
    assert!(goal.contains("0.925926%"), "1/108 to six decimals: {goal}");

    // The document says the count is pinned to the `--depth` asked for, so a section lost or
    // gained is a finding.
    let depths = text
        .lines()
        .filter(|line| line.starts_with("## Depth "))
        .count();
    assert_eq!(depths, 3, "{depths} depth sections:\n{text}");
    assert!(
        text.contains("Live after depth 3:"),
        "the last depth's live mass:\n{text}"
    );

    let conservation = text
        .lines()
        .find(|line| line.starts_with("Conservation:"))
        .expect("the conservation line");
    assert!(
        conservation.ends_with("holds exactly."),
        "the mass account: {conservation}"
    );
    // Each term is named: the goal's absorption is `desired`, and the total that left the loop is
    // desired plus the two policy terminals — not `absorbed` alone.
    for term in ["live", "left the loop", "desired", "reset", "stop"] {
        assert!(
            conservation.contains(term),
            "the conservation line names {term}: {conservation}"
        );
    }
}

/// The fixtures live behind `--fixtures`, so the report runs from a directory that has neither the
/// fixture tree nor the bundle: only the absolute flags point at them.
#[test]
fn runs_from_another_directory() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let text = report_from(dir.path());
    assert!(text.contains("## Depth 3"), "{text}");
    assert!(
        text.contains("Conservation:"),
        "the account is printed away from the repository:\n{text}"
    );
}

/// A depth deeper than the exact state can hold kept its earlier depths: they are printed, the
/// document says which depth stopped it and why, and the command still exits non-zero.
#[test]
fn a_depth_too_deep_keeps_the_depths_that_fit() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut args = report_args();
    let depth = args
        .iter()
        .position(|arg| arg == "--depth")
        .expect("the depth flag");
    args[depth + 1] = "4".to_string();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
        .current_dir(dir.path())
        .args(&args)
        .output()
        .expect("running cli");
    assert!(!output.status.success(), "a depth that did not fit fails");
    let text = String::from_utf8(output.stdout).expect("utf-8 report");
    assert!(text.contains("## Depth 3"), "the depths that fit:\n{text}");
    assert!(
        text.contains("## Stopped at depth 4"),
        "the depth that stopped it:\n{text}"
    );
    assert!(
        text.contains("The 3 depths above ran to completion"),
        "how many depths fit:\n{text}"
    );
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("depth 4 of 4 did not fit"), "{err}");
    assert!(
        err.contains("the distribution cannot be held exactly in u128"),
        "why it stopped: {err}"
    );
    assert!(err.contains("the 3 depths that ran"), "{err}");
}

/// A missing bundle points at the README's build section and at the command's shape — with the
/// path asked for as `--out`, so the advice fits that dataset — and both report commands say the
/// same thing.
#[test]
fn a_missing_bundle_says_how_to_build_one() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let bundle = dir.path().join("nope.json");
    let run = |command: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
            .current_dir(dir.path())
            .args(command)
            .output()
            .expect("running cli");
        assert!(!output.status.success(), "{command:?} needs a bundle");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let bundle_text = bundle.display().to_string();
    let loop_err = run(&[
        "loop-report",
        "--bundle",
        &bundle_text,
        "--pattern",
        "totem-shield",
        "--base",
        "Supreme Spiked Shield",
        "--item-level",
        "87",
        "--depth",
        "1",
    ]);
    let pool_err = run(&[
        "pool-report",
        "--bundle",
        &bundle_text,
        "--base",
        "Supreme Spiked Shield",
        "--item-level",
        "87",
        "--out",
        &dir.path().join("pool.md").display().to_string(),
    ]);
    for err in [&loop_err, &pool_err] {
        assert!(err.contains("\"Develop\" section of README.md"), "{err}");
        assert!(
            err.contains("bundle --raw <raw-dir> --rules <rules-dir>"),
            "the command's shape, not a hard-coded dataset: {err}"
        );
        assert!(err.contains(&bundle_text), "the bundle path: {err}");
    }
}
