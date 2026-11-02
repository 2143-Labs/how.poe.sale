//! wasm-bindgen bridge the Web Worker talks to.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Bridge {
    bundle: poe1::data::Bundle,
}

#[wasm_bindgen]
impl Bridge {
    #[wasm_bindgen(constructor)]
    pub fn new(bundle_json: &str) -> Result<Bridge, JsValue> {
        console_error_panic_hook::set_once();
        let bundle = poe1::data::Bundle::from_json(bundle_json).map_err(js_error)?;
        Ok(Bridge { bundle })
    }

    pub fn version(&self) -> String {
        format!("{} / data {}", env!("CARGO_PKG_VERSION"), self.bundle.patch)
    }

    /// Step 8 replaces this with `poe1::api::solve`.
    pub fn solve(&self, _problem_json: &str) -> Result<String, JsValue> {
        Ok(r#"{"version":1}"#.into())
    }

    /// Step 5 replaces this with the tooltip parser.
    pub fn parse_target(&self, _text: &str) -> Result<String, JsValue> {
        Ok("{}".into())
    }

    /// Step 8 replaces this with the influence-mod list for the donor editor.
    pub fn influence_mods(&self, _base: &str, _influence: &str) -> Result<String, JsValue> {
        Ok("[]".into())
    }
}

fn js_error(err: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&err.to_string())
}
