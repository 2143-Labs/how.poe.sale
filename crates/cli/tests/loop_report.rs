//! `cli loop-report` runs the owner's loop on the exact lumped state and prints one section per
//! depth. The assertions pin the header, the first depth's goal share (exactly 1/108, which prints
//! as 0.925926%), the number of depths, and the conservation line.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The report, captured from the built binary into a temporary file.
fn report() -> String {
    let root = root();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let out = dir.path().join("loop-report.txt");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
        .current_dir(&root)
        .args([
            "loop-report",
            "--bundle",
            "data/bundle.json",
            "--pattern",
            "totem-shield",
            "--item",
            "Supreme Spiked Shield",
            "--item-level",
            "87",
            "--influence",
            "shaper",
            "--depth",
            "3",
        ])
        .output()
        .expect("running cli");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(&out, &output.stdout).expect("writing the report");
    std::fs::read_to_string(&out).expect("the written report")
}

/// A `num/den` word, for checking the conservation line's arithmetic.
fn fraction(word: &str) -> Option<(u128, u128)> {
    let word = word.trim_end_matches(['.', ',', '—']);
    let (num, den) = word.split_once('/')?;
    Some((num.parse().ok()?, den.parse().ok()?))
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

    let depths = text
        .lines()
        .filter(|line| line.starts_with("## Depth "))
        .count();
    assert!(depths >= 3, "{depths} depth sections:\n{text}");
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
    let numbers: Vec<(u128, u128)> = conservation
        .split_whitespace()
        .filter_map(fraction)
        .collect();
    let [(ln_, ld), (an, ad), (sn, sd)] = numbers.as_slice() else {
        panic!("three fractions on the conservation line: {conservation}");
    };
    // The exact fractions can carry numerators too wide to cross-multiply in `u128`; at these
    // magnitudes `f64` resolves the identity far finer than the six decimals printed.
    let value = |num: &u128, den: &u128| *num as f64 / *den as f64;
    assert!(
        (value(ln_, ld) + value(an, ad) - value(sn, sd)).abs() < 1e-12,
        "live plus absorbed is the printed total: {conservation}"
    );
}
