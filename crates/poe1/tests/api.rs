//! The browser's views: a pasted item, the steps that could have produced an abstract item, and
//! the modifiers a target can name, as JSON.

mod common;

use common::root;
use poe1::api;
use poe1::test_data;

const METHODS: [&str; 5] = [
    "currency/exalted",
    "currency/annulment",
    "bench/add/StrMasterItemGenerationCannotChangePrefixes",
    "bench/add/DexMasterItemGenerationCannotChangeSuffixes",
    "essence/essence-of-horror",
];

fn text(name: &str) -> String {
    let path = root().join(format!("fixtures/text/{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn methods() -> Vec<String> {
    METHODS.iter().map(|m| m.to_string()).collect()
}

/// The essence outcome with two untracked prefixes and two untracked suffixes beside the
/// essence's suffix: every method answers, with the input's spec ready to ask about in turn.
#[test]
fn preimages_answer_in_json() {
    let d = test_data();
    let target = serde_json::json!({
        "base": "Archdemon Crown",
        "itemLevel": 87,
        "rarity": "rare",
        "suffixes": [{ "id": "SocketedGemsDealMoreElementalDamageEssence1" }],
        "anyPrefixes": 2,
        "anySuffixes": 2,
    });
    let view = api::preimages(d, &target.to_string(), &methods()).expect("a view");
    let json = serde_json::to_value(&view).unwrap();
    assert!(
        json["steps"][0]["input"].get("anyPrefixes").is_some(),
        "{json}"
    );
    assert_eq!(json["target"]["anySuffixes"], 2);
    assert_eq!(
        json["target"]["suffixes"][0],
        "Socketed Gems deal 30% more Elemental Damage"
    );
    let essence = view
        .steps
        .iter()
        .find(|s| s.method == "essence/essence-of-horror")
        .expect("the essence step");
    assert!((essence.p - 11.0 / 64.0).abs() < 1e-12, "{}", essence.p);
    assert_eq!(essence.input_spec.rarity, poe1::item::Rarity::Normal);
    assert_eq!(essence.cost, "1 × Essence of Horror");
    for m in METHODS {
        let shown =
            view.steps.iter().any(|s| s.method == m) || view.skipped.iter().any(|s| s.method == m);
        assert!(shown, "{m}");
    }

    let err = api::preimages(d, "{", &methods()).unwrap_err();
    assert!(err.starts_with("target: "), "{err}");
    let mut wide = target.clone();
    wide["anyPrefixes"] = 4.into();
    assert_eq!(
        api::preimages(d, &wide.to_string(), &methods()).unwrap_err(),
        "target: the item prefixes: 4 modifiers, the rarity holds 3"
    );
}

#[test]
fn mods_list_the_natural_and_essence_modifiers() {
    let d = test_data();
    let list = api::mods(d, "Archdemon Crown", 87, &[]).expect("a list");
    let life = list
        .iter()
        .find(|m| m.id == "IncreasedLife9")
        .expect("IncreasedLife9");
    assert_eq!(
        (life.side.as_str(), life.source.as_str()),
        ("prefix", "natural")
    );
    let horror = list
        .iter()
        .find(|m| m.id == "SocketedGemsDealMoreElementalDamageEssence1")
        .expect("Essence of Horror's modifier");
    assert_eq!(horror.side, "suffix");
    assert_eq!(horror.source, "Essence of Horror");
    let first_suffix = list
        .iter()
        .position(|m| m.side == "suffix")
        .expect("suffixes");
    assert!(list[..first_suffix].iter().all(|m| m.side == "prefix"));
    assert!(list[first_suffix..].iter().all(|m| m.side == "suffix"));
    assert_eq!(
        api::mods(d, "Nope", 87, &[]).unwrap_err(),
        "mods: unknown base \"Nope\""
    );
    assert!(api::bases(d).iter().any(|b| b == "Archdemon Crown"));
}

#[test]
fn read_item_reports_its_warnings() {
    let d = test_data();
    let view = api::read_item(d, &text("archdemon-crown")).expect("an item");
    assert!(
        view.warnings
            .iter()
            .any(|w| w.contains("is the sum of two modifiers")),
        "{:?}",
        view.warnings
    );
}
