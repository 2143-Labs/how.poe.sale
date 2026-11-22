//! The wasm-bindgen bridge the Web Workers talk to. `Bridge` holds a game's bundle and answers with
//! the JSON views of `poe1::api`: the start view, a base's modifiers, the priced craft loop and one
//! currency's outcomes. `CraftBridge` answers the Craft planner from `simple::api`.

use poe1::api;
use poe1::data::Data;
use poe1::item::Influence;
use wasm_bindgen::prelude::*;

fn js_error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
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

    /// What the page needs before its first request: `Start` JSON.
    pub fn start(&self) -> Result<String, JsValue> {
        serde_json::to_string(&api::start(&self.data)).map_err(js_error)
    }

    /// Every modifier a rare `base` at `item_level` can name: `ModOptionView[]` JSON.
    /// `influences` is a comma-joined list of influence names, empty for none.
    pub fn mods(&self, base: &str, item_level: u8, influences: &str) -> Result<String, JsValue> {
        let influences = self::influences(influences).map_err(js_error)?;
        let view = api::mods(&self.data, base, item_level, &influences).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// The craft loop from a white base toward a goal: `LoopRequest` JSON in, `LoopView` JSON out.
    pub fn loop_report(&self, request: &str) -> Result<String, JsValue> {
        let view = api::loop_report(&self.data, request).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// One currency on one item, exactly: `ApplyRequest` JSON in, `ApplyView` JSON out.
    pub fn apply_currency(&self, request: &str) -> Result<String, JsValue> {
        let view = api::apply_currency(&self.data, request).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }
}

/// The Craft planner's engine: the JSON views of `simple::api`. It holds nothing; every base is
/// compiled into `simple`.
#[wasm_bindgen]
pub struct CraftBridge;

#[wasm_bindgen]
impl CraftBridge {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> CraftBridge {
        console_error_panic_hook::set_once();
        CraftBridge
    }

    /// The bases, methods, currencies and built-in presets.
    pub fn catalog(&self) -> Result<String, JsValue> {
        let view = simple::api::catalog().map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// Every explicit modifier of a base.
    pub fn mods(&self, base: &str) -> Result<String, JsValue> {
        let view = simple::api::mods(base).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }

    /// A craft, costed: `request` is a JSON `RunRequest`.
    pub fn run(&self, request: &str) -> Result<String, JsValue> {
        let view = simple::api::run(request).map_err(js_error)?;
        serde_json::to_string(&view).map_err(js_error)
    }
}
