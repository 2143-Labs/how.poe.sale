//! The wasm-bindgen bridge the Web Worker talks to: it holds the bundle and answers with the
//! JSON views of `poe1::api`. The example pastes are the repository's own fixtures, compiled in.

use std::cell::RefCell;

use poe1::api;
use poe1::data::Data;
use poe1::item::Influence;
use wasm_bindgen::prelude::*;

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

fn no_chart() -> &'static str {
    "chart: start a chart first"
}

/// A comma-joined list, without empty entries.
fn list(joined: &str) -> impl Iterator<Item = &str> {
    joined.split(',').map(str::trim).filter(|s| !s.is_empty())
}

/// Influence names, either case: `shaper,elder`.
fn influences(joined: &str) -> Result<Vec<Influence>, String> {
    list(joined)
        .map(|name| {
            Influence::ALL
                .into_iter()
                .find(|i| i.name().eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("mods: unknown influence \"{name}\""))
        })
        .collect()
}

#[wasm_bindgen]
pub struct Bridge {
    data: Data,
    chart: RefCell<Option<api::Chart>>,
}

#[wasm_bindgen]
impl Bridge {
    #[wasm_bindgen(constructor)]
    pub fn new(bundle_json: &str) -> Result<Bridge, JsValue> {
        console_error_panic_hook::set_once();
        let data = Data::from_json(bundle_json).map_err(js_error)?;
        Ok(Bridge {
            data,
            chart: RefCell::new(None),
        })
    }

    pub fn version(&self) -> String {
        format!(
            "{} / data {}",
            env!("CARGO_PKG_VERSION"),
            self.data.bundle.patch
        )
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

    /// Every modifier a rare `base` at `item_level` can name: `ModOptionView[]` JSON.
    /// `influences` is a comma-joined list of influence names, empty for none.
    pub fn mods(&self, base: &str, item_level: u8, influences: &str) -> Result<String, JsValue> {
        let influences = self::influences(influences).map_err(js_error)?;
        let view = api::mods(&self.data, base, item_level, &influences).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// Starts a chart from a root class (`AbsSpec` JSON) and the comma-separated slots to watch:
    /// `NodeView` JSON.
    pub fn chart_start(&self, root: &str, watch: &str) -> Result<String, JsValue> {
        let watch = if watch.trim().is_empty() { "[]" } else { watch };
        let (chart, view) = api::Chart::start(&self.data, root, watch).map_err(js_error)?;
        *self.chart.borrow_mut() = Some(chart);
        serde_json::to_string(&view).map_err(js_error)
    }

    /// The methods a node can be pushed through: `MethodOptionView[]` JSON.
    pub fn chart_methods(&self, node: &str) -> Result<String, JsValue> {
        let mut chart = self.chart.borrow_mut();
        let chart = chart.as_mut().ok_or_else(no_chart).map_err(js_error)?;
        let view = chart.methods(&self.data, node).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// One method's outcomes from a node: `AppliedView` JSON.
    pub fn chart_forward(&self, node: &str, method: &str) -> Result<String, JsValue> {
        let mut chart = self.chart.borrow_mut();
        let chart = chart.as_mut().ok_or_else(no_chart).map_err(js_error)?;
        let view = chart.forward(&self.data, node, method).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// The inputs that could have produced a node, over the comma-joined methods (empty for the
    /// chart's own set): `MadeByView` JSON.
    pub fn chart_made_by(&self, node: &str, methods: &str) -> Result<String, JsValue> {
        let methods: Vec<String> = list(methods).map(str::to_string).collect();
        let mut chart = self.chart.borrow_mut();
        let chart = chart.as_mut().ok_or_else(no_chart).map_err(js_error)?;
        let view = chart
            .made_by(&self.data, node, &methods)
            .map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// Adds a forward outcome to the chart: `PinnedView` JSON.
    pub fn chart_pin_forward(
        &self,
        node: &str,
        method: &str,
        key: &str,
    ) -> Result<String, JsValue> {
        let mut chart = self.chart.borrow_mut();
        let chart = chart.as_mut().ok_or_else(no_chart).map_err(js_error)?;
        let view = chart
            .pin_forward(&self.data, node, method, key)
            .map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// Adds a backward input to the chart: `PinnedView` JSON.
    pub fn chart_pin_backward(
        &self,
        node: &str,
        method: &str,
        key: &str,
    ) -> Result<String, JsValue> {
        let mut chart = self.chart.borrow_mut();
        let chart = chart.as_mut().ok_or_else(no_chart).map_err(js_error)?;
        let view = chart
            .pin_backward(&self.data, node, method, key)
            .map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// Every base a target can name: a JSON array of strings.
    pub fn bases(&self) -> String {
        serde_json::Value::from(api::bases(&self.data)).to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

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
}
