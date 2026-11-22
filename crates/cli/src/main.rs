//! `cli`: builds `data/bundle.json` from the pinned RePoE export (3.29.3.3) and
//! `rules/`.

mod bundle;
mod loop_report;
mod methods;
mod pool_report;
mod raw;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, bail};

const USAGE: &str = "usage: cli bundle --raw <dir> --rules <dir> [--claims <file>] --out <file>\n       cli pool-report --bundle <file> --base <name> --item-level <n> [--influence <name>] --out <file>\n       cli loop-report --bundle <file> [--fixtures <dir>] --pattern <name> --base <name> --item-level <n> [--influence <name>] --depth <n>";

/// Parses `--flag value` pairs; any flag not in `names` is an error.
fn flag_map(
    args: impl Iterator<Item = String>,
    names: &[&str],
) -> anyhow::Result<BTreeMap<String, PathBuf>> {
    let mut given = BTreeMap::new();
    let mut args = args;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("{flag} needs a value; {USAGE}"))?;
        if !names.contains(&flag.as_str()) {
            bail!("unknown argument {flag}; {USAGE}");
        }
        given.insert(flag, PathBuf::from(value));
    }
    Ok(given)
}

fn write(out: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))
}

/// The bundle, or an error that says how to build one: the README's "Develop" section and the
/// command's shape, with the path the caller asked for as `--out`. The hints name no dataset, so
/// `--bundle data/simple.json` is told to write `data/simple.json`, not the game's own bundle.
fn read_bundle(bundle: &std::path::Path) -> anyhow::Result<poe1::data::Data> {
    let text = std::fs::read_to_string(bundle).with_context(|| {
        format!(
            "reading {} (build a bundle first — see the \"Develop\" section of README.md: \
             `cargo run -p cli --release -- bundle --raw <raw-dir> --rules <rules-dir> --out {}`)",
            bundle.display(),
            bundle.display(),
        )
    })?;
    poe1::data::Data::from_json(&text).with_context(|| format!("loading {}", bundle.display()))
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().context(USAGE)?;
    match command.as_str() {
        "bundle" => {
            let mut given = flag_map(args, &["--raw", "--rules", "--claims", "--out"])?;
            let mut required = |name: &str| {
                given
                    .remove(name)
                    .with_context(|| format!("missing {name}; {USAGE}"))
            };
            let raw = required("--raw")?;
            let rules = required("--rules")?;
            let out = required("--out")?;
            let claims = given
                .remove("--claims")
                .unwrap_or_else(|| rules.join("claims.json"));
            if !raw.is_dir() {
                bail!("--raw {} is not a directory", raw.display());
            }
            let mut bundle = bundle::build(&raw, &rules, &claims)?;
            methods::attach(&mut bundle, &rules)?;
            let text = serde_json::to_string(&bundle).context("serialising bundle")?;
            // The written bundle must load through the same code the library uses.
            poe1::data::Data::from_json(&text).context("the built bundle does not load")?;
            write(&out, text.as_bytes())
        }
        "pool-report" => {
            let mut given = flag_map(
                args,
                &["--bundle", "--base", "--item-level", "--influence", "--out"],
            )?;
            let mut required = |name: &str| {
                given
                    .remove(name)
                    .with_context(|| format!("missing {name}; {USAGE}"))
            };
            let bundle = required("--bundle")?;
            let base = required("--base")?;
            let level = required("--item-level")?;
            let out = required("--out")?;
            let influence = given.remove("--influence");
            let base = base
                .to_str()
                .with_context(|| format!("{} is not a base name", base.display()))?;
            let level: u8 = level
                .to_str()
                .and_then(|s| s.parse().ok())
                .with_context(|| format!("--item-level {} is not a number", level.display()))?;
            let influence = influence
                .map(|name| {
                    let text = name
                        .to_str()
                        .with_context(|| format!("{} is not an influence name", name.display()))?;
                    pool_report::influence(text).with_context(|| format!("no influence {text:?}"))
                })
                .transpose()?;
            let data = read_bundle(&bundle)?;
            let item = pool_report::item(&data, base, level, influence)?;
            write(&out, pool_report::render(&data, &item).as_bytes())
        }
        "loop-report" => {
            let mut given = flag_map(
                args,
                &[
                    "--bundle",
                    "--fixtures",
                    "--pattern",
                    "--base",
                    "--item-level",
                    "--influence",
                    "--depth",
                ],
            )?;
            let mut required = |name: &str| {
                given
                    .remove(name)
                    .with_context(|| format!("missing {name}; {USAGE}"))
            };
            let bundle = required("--bundle")?;
            let pattern = required("--pattern")?;
            let base = required("--base")?;
            let level = required("--item-level")?;
            let depth = required("--depth")?;
            // The pattern fixtures; the default is the repository's `fixtures/` tree.
            let fixtures = given
                .remove("--fixtures")
                .unwrap_or_else(|| PathBuf::from("fixtures"));
            let influence = given.remove("--influence");
            let pattern = pattern
                .to_str()
                .with_context(|| format!("{} is not a pattern name", pattern.display()))?;
            let base = base
                .to_str()
                .with_context(|| format!("{} is not a base name", base.display()))?;
            let level: u8 = level
                .to_str()
                .and_then(|s| s.parse().ok())
                .with_context(|| format!("--item-level {} is not a number", level.display()))?;
            let depth: usize = depth
                .to_str()
                .and_then(|s| s.parse().ok())
                .with_context(|| format!("--depth {} is not a number", depth.display()))?;
            if depth == 0 {
                bail!("--depth must be at least 1; {USAGE}");
            }
            let influence = influence
                .map(|name| {
                    let text = name
                        .to_str()
                        .with_context(|| format!("{} is not an influence name", name.display()))?;
                    pool_report::influence(text).with_context(|| format!("no influence {text:?}"))
                })
                .transpose()?;
            let data = read_bundle(&bundle)?;
            let item = pool_report::item(&data, base, level, influence)?;
            let pattern = loop_report::pattern(&data, &fixtures, pattern)?;
            let report = loop_report::report(&data, &pattern, &item, depth)?;
            print!("{}", report.text);
            // The document is on stdout either way; a depth that did not fit still fails the
            // command, so a script cannot read a partial account as a result.
            match report.stopped {
                Some(stop) => bail!(
                    "depth {} of {} did not fit: {}; the report above covers the {} depth{} that \
                     ran",
                    stop.depth,
                    depth,
                    stop.reason,
                    stop.depth - 1,
                    if stop.depth == 2 { "" } else { "s" },
                ),
                None => Ok(()),
            }
        }
        other => bail!("unknown command {other}; {USAGE}"),
    }
}
