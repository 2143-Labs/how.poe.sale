//! The committed `catalog/` is exactly what `render_all` produces from the bundle and fixtures.

mod common;

use poe1_v2::catalog::render_all;
use poe1_v2::test_data;

const STALE: &str = "catalog/ is stale; run: nix develop -c cargo run -p datagen-v2 --release -- catalog --bundle data/v2/bundle.json --fixtures fixtures/v2 --out catalog";

#[test]
fn catalog_is_current() {
    let root = common::root();
    let want = render_all(test_data(), &root.join("fixtures/v2"));
    let mut committed: Vec<String> = std::fs::read_dir(root.join("catalog"))
        .expect(STALE)
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    committed.sort();
    let produced: Vec<String> = want.keys().cloned().collect();
    assert_eq!(committed, produced, "{STALE}");
    for (name, content) in &want {
        let have = std::fs::read_to_string(root.join("catalog").join(name)).expect(STALE);
        assert!(have == *content, "{name}: {STALE}");
    }
}
