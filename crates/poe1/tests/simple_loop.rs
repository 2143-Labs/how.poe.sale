//! The toy dataset is a dataset, not a second model: the same currency structs, craft machine and
//! lumped state must produce the same numbers from `data/simple.json` as from the pinned export.
//!
//! This is the regression that keeps it that way. If a figure ever differs between the two bundles,
//! the dataset is what is wrong — the code cannot tell them apart, because it never sees which
//! bundle it is running on.

mod common;

use poe1::craft::{LoopPolicy, Lumped, Stage};
use poe1::currency::{Accept, Fraction, ItemSetFilter};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::pattern::{Pattern, PatternSpec};

fn shield(data: &Data) -> Item {
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "normal",
            "influences": ["shaper"],
            "sockets": "G-W W"
        }"#,
    )
    .expect("spec");
    Item::from_spec(data, &spec).expect("the shield")
}

fn filter(data: &Data) -> ItemSetFilter {
    let path = common::root().join("fixtures/patterns/totem-shield.json");
    let spec: PatternSpec =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the pattern fixture"))
            .expect("pattern spec");
    let pattern = Pattern::compile(data, &spec).expect("the pattern compiles against this bundle");
    ItemSetFilter::from_pattern(&pattern, Accept::DesiredOrGoodBothSides)
}

/// One depth of the owner's loop, from normal, over one dataset.
fn first_depth(data: &Data) -> Vec<(Stage, Fraction)> {
    let filter = filter(data);
    let policy = LoopPolicy::new(&filter);
    let mut machine = Lumped::new(data, filter.clone(), shield(data));
    let depth = machine.step(&policy).expect("one lumped step");
    [
        Stage::Desired,
        Stage::GoodPair,
        Stage::GoodPrefix,
        Stage::GoodSuffix,
        Stage::Other,
        Stage::Spare,
        Stage::SpentRare,
        Stage::Reset,
        Stage::Stop,
    ]
    .into_iter()
    .map(|stage| (stage, depth.mass(stage)))
    .filter(|(_, mass)| !mass.is_zero())
    .collect()
}

#[test]
fn the_toy_dataset_reaches_the_same_numbers_as_the_export() {
    let export = poe1::test_data();
    let simple = common::simple_data();
    let from_export = first_depth(export);
    let from_simple = first_depth(simple);
    assert!(
        !from_export.is_empty(),
        "the export must reach some mass for this to mean anything"
    );
    assert_eq!(
        from_simple, from_export,
        "the toy dataset must produce the export's distribution, stage by stage"
    );
    // The figures the export is pinned to, restated so a change in either dataset is visible here.
    let desired = from_simple
        .iter()
        .find(|(stage, _)| *stage == Stage::Desired)
        .map(|(_, mass)| *mass);
    assert_eq!(desired, Some(Fraction::new(1, 108).expect("1/108")));
}
