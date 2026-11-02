//! Path of Exile 1 (3.29) crafting rules: game data, item state, craft kernels and the wire API.

use std::path::PathBuf;
use std::sync::LazyLock;

pub mod api;
pub mod data;
pub mod engine;
pub mod item;
pub mod kernels;
pub mod market;
pub mod recipe;
pub mod registry;
pub mod target;
pub mod text;
pub mod weights;

static BUNDLE: LazyLock<data::Bundle> = LazyLock::new(load_bundle);

/// The workspace's generated bundle, for tests and tools. Always compiled so integration tests
/// can call it; cached for the process lifetime.
///
/// Reads `$HOW_BUNDLE`, else `<workspace>/data/bundle.json`.
#[doc(hidden)]
pub fn test_bundle() -> &'static data::Bundle {
    &BUNDLE
}

fn load_bundle() -> data::Bundle {
    let path = std::env::var_os("HOW_BUNDLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/bundle.json")
        });
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read bundle {}: {err}\n\
             regenerate it with:\n  \
             nix develop -c cargo run -p datagen --release -- --raw data/raw --out data/bundle.json",
            path.display()
        )
    });
    data::Bundle::from_json(&text).unwrap_or_else(|err| {
        panic!(
            "{} is not a valid bundle ({err})\n\
             regenerate it with:\n  \
             nix develop -c cargo run -p datagen --release -- --raw data/raw --out data/bundle.json",
            path.display()
        )
    })
}
