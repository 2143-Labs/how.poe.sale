//! The step filter is a projection of the pattern's classes, so it must read them exactly the way
//! the score does: desired is the best bucket, good/middling/neutral are good enough, and a class
//! written by category still selects the modifiers it names.

mod common;

use common::pattern;
use poe1::currency::{Accept, ItemSetFilter, ModSelector, Tier};
use poe1::data::{Data, Side};
use poe1::test_data;

/// The bucket the goal read from `crown-shaper` puts the modifier with this id in.
fn tier(data: &Data, filter: &ItemSetFilter, id: &str) -> Tier {
    let m = data
        .mod_by_id(id)
        .unwrap_or_else(|| panic!("the bundle has no modifier {id}"));
    filter.tier_of_mod(data, m)
}

fn crown_filter() -> ItemSetFilter {
    ItemSetFilter::from_pattern(&pattern("crown-shaper"), Accept::DesiredOrGoodBothSides)
}

/// `ok-prefix` is `middling`, which the score values as good enough (1), so its four modifiers are
/// good — while a `desired` class keeps its own bucket.
#[test]
fn middling_classes_are_good_enough() {
    let data = test_data();
    let filter = crown_filter();
    for id in [
        "AreaOfEffectSupportedUber1_",
        "AreaOfEffectSupportedUber2",
        "AreaOfEffectSupportedUber3",
        "AreaOfEffectSupportedUberMaven",
    ] {
        assert_eq!(tier(data, &filter, id), Tier::Good, "{id}");
    }
    assert_eq!(
        tier(
            data,
            &filter,
            "JunMasterVeiledSkillAreaOfEffectPercentAndAreaOfEffectGemLevel"
        ),
        Tier::Desired,
        "a desired class does not collapse into good"
    );
}

/// `any-crafted-prefix` is `neutral` and names no ids, only `{"crafted": true}`; the filter must
/// carry that category, and a bench craft is exactly what it means.
#[test]
fn a_crafted_class_takes_a_bench_craft() {
    let data = test_data();
    let filter = crown_filter();
    assert!(
        filter.good.contains(&ModSelector::Crafted {
            side: Side::Prefix,
            crafted: true,
        }),
        "the category matcher is projected, not dropped"
    );
    assert_eq!(
        tier(data, &filter, "EinharMasterIncreasedMana2"),
        Tier::Good,
        "a bench-crafted prefix is good enough"
    );
}

/// A metamod is crafted too, but it is a tool in a slot rather than a modifier the goal wants, so
/// a crafted class never takes one and it stays in the fallback bucket.
#[test]
fn a_crafted_class_never_takes_a_metamod() {
    let data = test_data();
    let filter = crown_filter();
    assert_eq!(
        data.mod_at(
            data.mod_by_id("DexMasterItemGenerationCannotChangeSuffixes")
                .expect("the metamod")
        )
        .side,
        Some(Side::Prefix),
        "the metamod must share the crafted class's side for this to test anything"
    );
    assert_eq!(
        tier(data, &filter, "DexMasterItemGenerationCannotChangeSuffixes"),
        Tier::Any
    );
}

/// A plain `good` class is unaffected by the grade mapping and the category work.
#[test]
fn a_good_class_is_good() {
    let data = test_data();
    let filter = crown_filter();
    assert_eq!(
        tier(data, &filter, "SocketedSpellCriticalStrikeChanceUber1_"),
        Tier::Good
    );
}
