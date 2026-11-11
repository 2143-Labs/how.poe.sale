//! The two report subcommands, end to end, on the fixture tree and on a pasted item.

use std::path::PathBuf;
use std::process::Output;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[String]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_cli"))
        .args(args)
        .output()
        .expect("running cli")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The subcommand, then the bundle and fixture flags every report needs.
fn args(rest: &[&str]) -> Vec<String> {
    let root = root();
    let mut out: Vec<String> = rest.iter().map(|s| s.to_string()).collect();
    out.extend(
        ["--bundle", "--fixtures"]
            .iter()
            .zip([root.join("data/bundle.json"), root.join("fixtures")])
            .flat_map(|(flag, path)| [flag.to_string(), path.display().to_string()]),
    );
    out
}

/// The forward report on a fixture: where the item is, what its exact steps do, and what could
/// have produced it.
#[test]
fn solve_reports_a_fixture() {
    let out = run(&args(&[
        "solve",
        "--pattern",
        "crown-shaper",
        "--item",
        "crown-a",
    ]));
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    for want in [
        "## `crown-a` → `crown-shaper`",
        "**Now:** 0.500 of the goal",
        "**Exact steps:**",
        "**Could have come from:**",
        "|1|advances|",
    ] {
        assert!(
            text.contains(want),
            "missing {want:?} in:\n{}",
            &text[..text.len().min(500)]
        );
    }
}

/// The backward report on a fixture: the best input is the bench craft that added its modifier.
#[test]
fn sources_reports_a_fixture() {
    let out = run(&args(&[
        "sources",
        "--pattern",
        "crown-shaper",
        "--item",
        "crown-a-mana",
    ]));
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("## `crown-a-mana` → `crown-shaper`"),
        "{text}"
    );
    assert!(
        text.contains("`bench/add/EinharMasterIncreasedMana2` at 1.0000"),
        "{text}"
    );
}

/// A pasted item is parsed and reported like a fixture, and the parser's warning is passed on.
#[test]
fn solve_reads_a_paste() {
    let paste = root().join("fixtures/text/archdemon-crown.txt");
    let out = run(&args(&[
        "solve",
        "--pattern",
        "crown-shaper",
        "--paste",
        &paste.display().to_string(),
    ]));
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("**Now:** 0.833 of the goal"),
        "{}",
        stdout(&out)
    );
    assert!(
        stderr(&out).contains("is the sum of two modifiers"),
        "{}",
        stderr(&out)
    );
}

/// Without a pattern there is nothing to weigh the item against, and the command says so.
#[test]
fn solve_needs_a_pattern() {
    let out = run(&args(&["solve", "--item", "crown-a"]));
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("missing --pattern"),
        "{}",
        stderr(&out)
    );
}

/// A report says what it left out, and `--all` leaves nothing out: on `crown-a` the exact table
/// stops at six rows with a count of the rest, and with `--all` it shows every row and names
/// every member of a tied row.
#[test]
fn all_prints_what_the_caps_hide() {
    let report = |extra: &[&str]| {
        let mut a = vec!["solve", "--pattern", "crown-shaper", "--item", "crown-a"];
        a.extend(extra);
        let out = run(&args(&a));
        assert!(out.status.success(), "{}", stderr(&out));
        stdout(&out)
    };
    let exact = |text: &str| {
        let table = text
            .split("**Exact steps:**")
            .nth(1)
            .and_then(|t| t.split("What those steps do").next())
            .expect("an exact table")
            .to_string();
        let rows = table
            .lines()
            .filter(|l| l.starts_with("|") && l[1..].starts_with(char::is_numeric))
            .count();
        (table, rows)
    };
    let capped = report(&[]);
    let (table, rows) = exact(&capped);
    assert_eq!(rows, 6, "{table}");
    assert!(table.contains("more steps, in further outcomes"), "{table}");
    assert!(
        capped.contains("more)"),
        "a capped list says how many it hid"
    );
    let all = report(&["--all"]);
    let (table, rows) = exact(&all);
    assert!(rows > 6, "{table}");
    assert!(
        !table.contains("more steps, in further outcomes"),
        "{table}"
    );
    assert!(!all.contains("more)"), "nothing is hidden with --all");
}
