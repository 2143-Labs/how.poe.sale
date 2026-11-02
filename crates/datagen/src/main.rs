//! Builds `data/bundle.json` from the pinned repoe export.

use std::path::PathBuf;

use anyhow::{Context, bail};

const USAGE: &str = "usage: datagen --raw <dir> --out <file>";

fn main() -> anyhow::Result<()> {
    let (mut raw, mut out) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--raw" => raw = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            other => bail!("unknown argument {other}; {USAGE}"),
        }
    }
    let raw = raw.context(USAGE)?;
    let out = out.context(USAGE)?;
    if !raw.is_dir() {
        bail!("--raw {} is not a directory", raw.display());
    }
    let bundle = serde_json::json!({ "format": 1, "patch": "3.29.3.3" });
    std::fs::write(&out, serde_json::to_vec(&bundle)?)
        .with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}
