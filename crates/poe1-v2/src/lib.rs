//! Path of Exile 1 (3.29) crafting model, v2: the game-data bundle, concrete items, abstract item
//! patterns, the crafting-method catalog and `apply(item, method) → outcome`.

use std::path::PathBuf;
use std::sync::LazyLock;

pub mod apply;
pub mod catalog;
pub mod data;
pub mod item;
pub mod method;
pub mod pattern;
pub mod pool;
pub mod render;
pub mod text;

static DATA: LazyLock<data::Data> = LazyLock::new(load_data);

/// The workspace's generated v2 bundle, for tests and tools. Always compiled so integration tests
/// can call it; cached for the process lifetime.
///
/// Reads `$HOW_V2_BUNDLE`, else `<workspace>/data/v2/bundle.json`.
#[doc(hidden)]
pub fn test_data() -> &'static data::Data {
    &DATA
}

fn load_data() -> data::Data {
    let path = std::env::var_os("HOW_V2_BUNDLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/v2/bundle.json")
        });
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read bundle {}: {err}\n\
             regenerate it with:\n  \
             nix develop -c cargo run -p datagen-v2 --release -- bundle --raw data/raw --rules rules/v2 --out data/v2/bundle.json",
            path.display()
        )
    });
    data::Data::from_json(&text).unwrap_or_else(|err| {
        panic!(
            "{} is not a valid v2 bundle ({err})\n\
             regenerate it with:\n  \
             nix develop -c cargo run -p datagen-v2 --release -- bundle --raw data/raw --rules rules/v2 --out data/v2/bundle.json",
            path.display()
        )
    })
}
