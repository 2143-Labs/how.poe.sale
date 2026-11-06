//! Shared test helpers: the workspace root and the v2 fixtures.
#![allow(dead_code)]

use std::path::PathBuf;

use poe1_v2::item::{Item, ItemSpec};
use poe1_v2::test_data;

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join("fixtures/v2/items"))
        .expect("fixtures/v2/items")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

pub fn spec(name: &str) -> ItemSpec {
    let path = root().join(format!("fixtures/v2/items/{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn item(name: &str) -> Item {
    Item::from_spec(test_data(), &spec(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

pub fn pattern_spec(name: &str) -> poe1_v2::pattern::PatternSpec {
    let path = root().join(format!("fixtures/v2/patterns/{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn pattern(name: &str) -> poe1_v2::pattern::Pattern {
    poe1_v2::pattern::Pattern::compile(test_data(), &pattern_spec(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every (pattern, fixture) the pattern accepts: the pairs the catalog reports.
pub fn pairs() -> Vec<(String, String)> {
    let data = test_data();
    let mut patterns: Vec<String> = std::fs::read_dir(root().join("fixtures/v2/patterns"))
        .expect("fixtures/v2/patterns")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    patterns.sort();
    let mut out = Vec::new();
    for pname in patterns {
        let pat = pattern(&pname);
        for name in fixture_names() {
            let it = item(&name);
            if poe1_v2::pattern::evaluate(data, &pat, &it)
                .iter()
                .any(|(p, s)| *p > 0.0 && s.item_ok)
            {
                out.push((pname.clone(), name));
            }
        }
    }
    out
}
