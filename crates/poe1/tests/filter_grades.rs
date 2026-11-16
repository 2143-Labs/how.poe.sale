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

/// A value floor is read in the units the game shows, exactly as the pattern reads it — the one
/// comparison both sides now call (`pattern::floors_met`).
///
/// The item is a magic Blue Pearl Amulet carrying `IncreasedLife6` (stored 85..99) with a Fertile
/// Catalyst at 20% quality, so the stored 85 is shown as 102 (`Item::display` at effect 20). A
/// floor of 100 is met by the shown value but not by the stored roll: the pattern puts the affix in
/// the floored class, and the filter must put it in `Good`. A floor of 103 fails both. Before the
/// delegation the filter compared the raw roll, so it rejected at 100 and the two disagreed.
#[test]
fn a_value_floor_is_read_in_the_units_the_game_shows() {
    use poe1::item::ItemSpec;
    use poe1::pattern::{Pattern, PatternSpec, evaluate};

    let data = test_data();
    let item_spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Blue Pearl Amulet",
            "itemLevel": 84,
            "rarity": "magic",
            "quality": 20,
            "catalyst": "Fertile Catalyst",
            "prefixes": [{ "id": "IncreasedLife6", "values": [85] }]
        }"#,
    )
    .expect("the jewel's spec");
    let item = poe1::item::Item::from_spec(data, &item_spec).expect("the jewel");
    let affix = &item.prefixes[0];
    assert_eq!(
        item.effect(data, &affix.roll, false),
        20,
        "the catalyst's quality is the effect the floor must be read with"
    );
    assert_eq!(
        item.prefixes[0].roll.values.as_deref(),
        Some([85].as_slice()),
        "the stored roll is what the old comparison used"
    );
    assert_eq!(
        poe1::item::Item::display(85, 20),
        102,
        "the value the game shows, which the floor is read in"
    );
    assert!(
        85 < 100 && 102 >= 100,
        "the raw comparison and the shown one must disagree at this floor"
    );

    // The pattern, built here so the floor is the only thing that moves between the two runs.
    let probe = |floor: i32| -> Pattern {
        let spec: PatternSpec = serde_json::from_value(serde_json::json!({
            "name": "floor-probe",
            "item": {
                "bases": ["Blue Pearl Amulet"],
                "itemLevelMin": 84,
                "rarity": "magic",
                "influences": []
            },
            "classes": [
                {
                    "id": "life-at-floor",
                    "label": "Jewel life at the floor",
                    "side": "prefix",
                    "grade": "good",
                    "matchers": [{ "mod": "IncreasedLife6", "min": [floor] }]
                },
                {
                    "id": "any-other-prefix",
                    "label": "Any other prefix",
                    "side": "prefix",
                    "grade": "bad",
                    "matchers": [{ "any": true }]
                },
                {
                    "id": "any-other-suffix",
                    "label": "Any other suffix",
                    "side": "suffix",
                    "grade": "bad",
                    "matchers": [{ "any": true }]
                }
            ],
            "prefixes": [["life-at-floor", "any-other-prefix"]],
            "suffixes": [["any-other-suffix"]]
        }))
        .expect("the probe's spec");
        Pattern::compile(data, &spec).expect("the probe compiles")
    };
    // The class the pattern's own reading puts the affix in.
    let pattern_class = |pattern: &Pattern| -> String {
        let states = evaluate(data, pattern, &item);
        assert_eq!(states.len(), 1, "the item's values are concrete");
        let state = &states[0].1;
        assert!(state.item_ok, "the jewel fits the probe's item filter");
        pattern.classes[state.prefixes[0].class as usize].id.clone()
    };

    // 85 stored, 102 shown: a floor of 100 is met only by the shown value.
    let accepted = probe(100);
    let filter = ItemSetFilter::from_pattern(&accepted, Accept::DesiredOrGoodBothSides);
    assert_eq!(pattern_class(&accepted), "life-at-floor");
    assert_eq!(
        filter.tier_of_item(data, &item, affix),
        Tier::Good,
        "the filter reads the shown value, like the pattern"
    );

    // A floor above the shown value fails both readings.
    let rejected = probe(103);
    let filter = ItemSetFilter::from_pattern(&rejected, Accept::DesiredOrGoodBothSides);
    assert_eq!(pattern_class(&rejected), "any-other-prefix");
    assert_eq!(filter.tier_of_item(data, &item, affix), Tier::Any);
}
