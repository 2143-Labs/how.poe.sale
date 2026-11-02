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

    /// `Problem` JSON (v1 or v3) in, `SolutionV3` JSON out. Only malformed JSON throws; solver
    /// errors are reported inside the solution.
    pub fn solve(&self, problem_json: &str) -> Result<String, JsValue> {
        let solution = poe1::api::solve_json(&self.bundle, problem_json).map_err(js_error)?;
        serde_json::to_string(&solution).map_err(js_error)
    }

    /// `ProcessProposalJson[]` for one good of a v1/v3 problem.
    pub fn propose_processes(&self, problem_json: &str, good_id: &str) -> Result<String, JsValue> {
        let list =
            poe1::api::propose_processes(&self.bundle, problem_json, good_id).map_err(js_error)?;
        serde_json::to_string(&list).map_err(js_error)
    }

    /// `ContractJson[]`: every craft's declared behaviour.
    pub fn contracts(&self) -> Result<String, JsValue> {
        serde_json::to_string(&poe1::api::contracts_json(&self.bundle)).map_err(js_error)
    }

    /// `RecipeDef[]`: the compiled-in recipes.
    pub fn recipes(&self) -> Result<String, JsValue> {
        serde_json::to_string(poe1::recipe::recipes()).map_err(js_error)
    }

    /// `TierJson[]` for the tier picker.
    pub fn mod_tiers(&self, base: &str, group: &str, side: &str) -> Result<String, JsValue> {
        let tiers = poe1::api::mod_tiers(&self.bundle, base, group, side).map_err(js_error)?;
        serde_json::to_string(&tiers).map_err(js_error)
    }

    /// Pasted tooltip text -> `TargetDraft` JSON.
    pub fn parse_target(&self, text: &str) -> Result<String, JsValue> {
        let draft = poe1::api::parse_target_api(&self.bundle, text);
        serde_json::to_string(&draft).map_err(js_error)
    }

    /// JSON `[{"id","text","side"}]` of the influence mods that spawn on `base` with exactly
    /// `influence`.
    pub fn influence_mods(&self, base: &str, influence: &str) -> Result<String, JsValue> {
        let mods = poe1::api::influence_mods(&self.bundle, base, influence).map_err(js_error)?;
        serde_json::to_string(&mods).map_err(js_error)
    }
}

fn js_error(err: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&err.to_string())
}
