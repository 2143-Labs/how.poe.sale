//! `datagen-v2`: builds `data/v2/bundle.json` from the pinned RePoE export (3.29.3.3) and
//! `rules/v2/`.

mod bundle;
mod methods;
mod raw;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, bail};

const USAGE: &str = "usage: datagen-v2 bundle --raw <dir> --rules <dir> --out <file>\n       datagen-v2 catalog --bundle <file> --fixtures <dir> --out <dir>";

/// Parses `--flag value` pairs and returns the named ones in order; any other flag is an error.
fn flags(args: impl Iterator<Item = String>, names: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
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

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().context(USAGE)?;
    match command.as_str() {
        "bundle" => {
            let [raw, rules, out] =
                <[PathBuf; 3]>::try_from(flags(args, &["--raw", "--rules", "--out"])?)
                    .expect("three flags");
            if !raw.is_dir() {
                bail!("--raw {} is not a directory", raw.display());
            }
            let mut bundle = bundle::build(&raw, &rules)?;
            methods::attach(&mut bundle, &rules)?;
            let text = serde_json::to_string(&bundle).context("serialising bundle")?;
            // The written bundle must load through the same code the library uses.
            poe1_v2::data::Data::from_json(&text).context("the built bundle does not load")?;
            write(&out, text.as_bytes())
        }
        "catalog" => {
            let [bundle, fixtures, out] =
                <[PathBuf; 3]>::try_from(flags(args, &["--bundle", "--fixtures", "--out"])?)
                    .expect("three flags");
            let text = std::fs::read_to_string(&bundle)
                .with_context(|| format!("reading {}", bundle.display()))?;
            let data = poe1_v2::data::Data::from_json(&text)
                .with_context(|| format!("loading {}", bundle.display()))?;
            let files = poe1_v2::catalog::render_all(&data, &fixtures)?;
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
        other => bail!("unknown command {other}; {USAGE}"),
    }
}
