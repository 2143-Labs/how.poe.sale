//! The game-data bundle produced by `datagen`.

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("bundle JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bundle {
    pub format: u32,
    pub patch: String,
}

impl Bundle {
    pub fn from_json(json: &str) -> Result<Bundle, DataError> {
        Ok(serde_json::from_str(json)?)
    }
}
