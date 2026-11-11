//! `cli`: builds `data/bundle.json` from the pinned RePoE export (3.29.3.3) and
//! `rules/`.

mod bundle;
mod methods;
mod raw;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, bail};

const USAGE: &str = "usage: cli bundle --raw <dir> --rules <dir> [--claims <file>] --out <file>\n       cli catalog --bundle <file> --fixtures <dir> --out <dir>\n       cli solve --bundle <file> --fixtures <dir> --pattern <pattern> (--item <fixture> | --paste <file>) [--all]\n       cli sources --bundle <file> --fixtures <dir> --pattern <pattern> (--item <fixture> | --paste <file>) [--all]";

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

/// Parses `--flag value` pairs and returns the named ones in order, every one required.
fn flags(args: impl Iterator<Item = String>, names: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    let mut given = flag_map(args, names)?;
    names
        .iter()
        .map(|n| {
            given
                .remove(*n)
                .with_context(|| format!("missing {n}; {USAGE}"))
        })
        .collect()
}

fn write(out: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))
}

/// Everything both report subcommands need: the bundle, the fixtures, the pattern and the item,
/// the item either named as a fixture or read from a copied item block.
struct Loaded {
    data: poe1::data::Data,
    fixtures: poe1::catalog::Fixtures,
    pattern_name: String,
    item_name: String,
    item: poe1::item::Item,
    /// `--all`: print every row and name rather than the report's usual share.
    caps: poe1::catalog::Caps,
}

fn load(args: impl Iterator<Item = String>) -> anyhow::Result<Loaded> {
    // `--all` is the one flag without a value.
    let (all, args): (Vec<String>, Vec<String>) = args.partition(|a| a == "--all");
    let caps = if all.is_empty() {
        poe1::catalog::Caps::default()
    } else {
        poe1::catalog::Caps::all()
    };
    let mut given = flag_map(
        args.into_iter(),
        &["--bundle", "--fixtures", "--pattern", "--item", "--paste"],
    )?;
    let mut required = |name: &str| {
        given
            .remove(name)
            .with_context(|| format!("missing {name}; {USAGE}"))
    };
    let bundle = required("--bundle")?;
    let fixtures = required("--fixtures")?;
    let pattern = required("--pattern")?;
    let item = given.remove("--item");
    let paste = given.remove("--paste");
    let text = std::fs::read_to_string(&bundle)
        .with_context(|| format!("reading {}", bundle.display()))?;
    let data = poe1::data::Data::from_json(&text)
        .with_context(|| format!("loading {}", bundle.display()))?;
    let fx = poe1::catalog::Fixtures::load(&data, &fixtures);
    let pattern_name = pattern
        .to_str()
        .map(str::to_string)
        .with_context(|| format!("{} is not a name", pattern.display()))?;
    if !fx.patterns.contains_key(&pattern_name) {
        bail!("no pattern {pattern_name}");
    }
    let (item_name, item) = match (item, paste) {
        (Some(name), None) => {
            let item_name = name
                .file_stem()
                .and_then(|n| n.to_str())
                .with_context(|| format!("{} is not a fixture name", name.display()))?
                .to_string();
            let item = fx
                .items
                .get(&item_name)
                .map(|(_, item)| item.clone())
                .with_context(|| format!("no item fixture {item_name}"))?;
            (item_name, item)
        }
        (None, Some(path)) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let parsed = poe1::text::parse(&data, &text)
                .with_context(|| format!("reading the paste {}", path.display()))?;
            for warning in &parsed.warnings {
                eprintln!("note: {warning}");
            }
            let item = poe1::item::Item::from_spec(&data, &parsed.spec)
                .with_context(|| format!("the paste {}", path.display()))?;
            let item_name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("pasted item")
                .to_string();
            (item_name, item)
        }
        (Some(_), Some(_)) => bail!("give --item or --paste, not both; {USAGE}"),
        (None, None) => bail!("missing --item or --paste; {USAGE}"),
    };
    Ok(Loaded {
        data,
        fixtures: fx,
        pattern_name,
        item_name,
        item,
        caps,
    })
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
        "catalog" => {
            let [bundle, fixtures, out] =
                <[PathBuf; 3]>::try_from(flags(args, &["--bundle", "--fixtures", "--out"])?)
                    .expect("three flags");
            let text = std::fs::read_to_string(&bundle)
                .with_context(|| format!("reading {}", bundle.display()))?;
            let data = poe1::data::Data::from_json(&text)
                .with_context(|| format!("loading {}", bundle.display()))?;
            let files = poe1::catalog::render_all(&data, &fixtures)?;
            std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
            // A file the catalog no longer produces is stale: remove it.
            for entry in std::fs::read_dir(&out)? {
                let path = entry?.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if name.ends_with(".md") && !files.contains_key(name) {
                    std::fs::remove_file(&path)
                        .with_context(|| format!("removing {}", path.display()))?;
                }
            }
            for (name, content) in files {
                write(&out.join(name), content.as_bytes())?;
            }
            Ok(())
        }
        "solve" => {
            let loaded = load(args)?;
            let pattern = &loaded.fixtures.patterns[&loaded.pattern_name];
            let lines = poe1::catalog::next_steps_section(
                &loaded.data,
                &loaded.item_name,
                &loaded.item,
                &loaded.pattern_name,
                pattern,
                loaded.caps,
            );
            println!("{}", lines.join("\n"));
            Ok(())
        }
        "sources" => {
            let loaded = load(args)?;
            let pattern = &loaded.fixtures.patterns[&loaded.pattern_name];
            let inversion = poe1::invert::sources(&loaded.data, pattern, &loaded.item);
            print!(
                "{}",
                poe1::catalog::sources_report(
                    pattern,
                    &loaded.item_name,
                    &loaded.pattern_name,
                    &inversion,
                    loaded.caps,
                )
            );
            Ok(())
        }
        other => bail!("unknown command {other}; {USAGE}"),
    }
}
