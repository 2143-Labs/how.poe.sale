//! The wasm-bindgen bridge the Web Worker talks to: it holds the bundle and answers with the
//! JSON views of `poe1::api`. The built-in patterns and the example pastes are the repository's
//! own fixtures, compiled in.

use poe1::api;
use poe1::data::Data;
use wasm_bindgen::prelude::*;

/// The built-in targets, in menu order.
const PATTERNS: [&str; 2] = [
    include_str!("../../../fixtures/patterns/crown-shaper.json"),
    include_str!("../../../fixtures/patterns/crown-shaper-elder.json"),
];

/// Every paste in `fixtures/text`, by file stem, sorted.
const EXAMPLES: [(&str, &str); 13] = [
    (
        "archdemon-crown",
        include_str!("../../../fixtures/text/archdemon-crown.txt"),
    ),
    (
        "archdemon-crown-blocked",
        include_str!("../../../fixtures/text/archdemon-crown-blocked.txt"),
    ),
    (
        "cobalt-jewel",
        include_str!("../../../fixtures/text/cobalt-jewel.txt"),
    ),
    (
        "ducat-belt",
        include_str!("../../../fixtures/text/ducat-belt.txt"),
    ),
    (
        "helmet-fire-resistance",
        include_str!("../../../fixtures/text/helmet-fire-resistance.txt"),
    ),
    (
        "jade-flask",
        include_str!("../../../fixtures/text/jade-flask.txt"),
    ),
    (
        "life-flask",
        include_str!("../../../fixtures/text/life-flask.txt"),
    ),
    (
        "mercenary-gloves",
        include_str!("../../../fixtures/text/mercenary-gloves.txt"),
    ),
    (
        "quicksilver-flask",
        include_str!("../../../fixtures/text/quicksilver-flask.txt"),
    ),
    (
        "simplex-amulet",
        include_str!("../../../fixtures/text/simplex-amulet.txt"),
    ),
    (
        "slink-boots-magic",
        include_str!("../../../fixtures/text/slink-boots-magic.txt"),
    ),
    (
        "synthesised-paua-ring",
        include_str!("../../../fixtures/text/synthesised-paua-ring.txt"),
    ),
    (
        "two-stone-ring",
        include_str!("../../../fixtures/text/two-stone-ring.txt"),
    ),
];

/// A file stem as a menu label: `archdemon-crown-blocked` → "Archdemon Crown Blocked".
pub fn label(stem: &str) -> String {
    stem.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn js_error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen]
pub struct Bridge {
    data: Data,
}

#[wasm_bindgen]
impl Bridge {
    #[wasm_bindgen(constructor)]
    pub fn new(bundle_json: &str) -> Result<Bridge, JsValue> {
        console_error_panic_hook::set_once();
        let data = Data::from_json(bundle_json).map_err(js_error)?;
        Ok(Bridge { data })
    }

    pub fn version(&self) -> String {
        format!(
            "{} / data {}",
            env!("CARGO_PKG_VERSION"),
            self.data.bundle.patch
        )
    }

    /// The built-in patterns, as a JSON array of pattern documents.
    pub fn patterns(&self) -> String {
        format!("[{}]", PATTERNS.join(","))
    }

    /// The example pastes: JSON `[{"id", "label", "text"}]`.
    pub fn examples(&self) -> String {
        let list: Vec<serde_json::Value> = EXAMPLES
            .iter()
            .map(|(id, text)| serde_json::json!({ "id": id, "label": label(id), "text": text }))
            .collect();
        serde_json::Value::Array(list).to_string()
    }

    /// A pasted item as the engine reads it: `ItemView` JSON.
    pub fn parse(&self, text: &str) -> Result<String, JsValue> {
        let view = api::read_item(&self.data, text).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// The pattern that asks for a pasted item exactly: `PatternSpec` JSON.
    pub fn target(&self, text: &str) -> Result<String, JsValue> {
        let spec = api::target(&self.data, text).map_err(js_error)?;
        serde_json::to_string(&spec).map_err(js_error)
    }

    /// The one-step report for a pattern (JSON) and a pasted item: `SolveView` JSON.
    pub fn solve(&self, pattern_json: &str, item_text: &str) -> Result<String, JsValue> {
        let view = api::solve(&self.data, pattern_json, item_text).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use poe1::pattern::{Pattern, PatternSpec};

    fn dir(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"))
    }

    fn stems(name: &str, ext: &str) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir(name))
            .unwrap_or_else(|e| panic!("fixtures/{name}: {e}"))
            .filter_map(|e| {
                let file = e.ok()?.file_name().into_string().ok()?;
                file.strip_suffix(ext).map(str::to_string)
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn every_text_fixture_is_an_example() {
        let built_in: Vec<String> = EXAMPLES.iter().map(|(id, _)| id.to_string()).collect();
        assert_eq!(stems("text", ".txt"), built_in);
        for (id, text) in EXAMPLES {
            api::read_item(poe1::test_data(), text).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
        assert_eq!(label("archdemon-crown-blocked"), "Archdemon Crown Blocked");
    }

    #[test]
    fn every_pattern_fixture_is_built_in() {
        assert_eq!(stems("patterns", ".json").len(), PATTERNS.len());
        for text in PATTERNS {
            let spec: PatternSpec = serde_json::from_str(text).expect("a pattern");
            Pattern::compile(poe1::test_data(), &spec).expect("compiles");
        }
        let all: Vec<serde_json::Value> =
            serde_json::from_str(&format!("[{}]", PATTERNS.join(","))).expect("one JSON array");
        assert_eq!(all[0]["name"], "crown-shaper");
    }
}
