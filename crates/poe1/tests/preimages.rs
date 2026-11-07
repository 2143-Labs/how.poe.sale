//! The steps that could have produced an abstract item: an Archdemon Crown at item level 87 with
//! no influence, whose untracked modifiers are counted rather than named.

use poe1::item::{AbsSpec, Rarity, SlotSpec};
use poe1::method::{Pool, SideReq};
use poe1::preimages::{AbsItem, Preimages, Slot, preimages};
use poe1::test_data;

const ESSENCE: &str = "essence/essence-of-horror";
/// ColdResist6: three tiers of the crown's cold resistance family are open at item level 87
/// (ColdResist6 at 60, 7 at 72 and 8 at 84), so a family slot above it is not degenerate.
const RESIST_FAMILY: &str = "ColdResist6";
const EXALT: &str = "currency/exalted";
const ANNUL: &str = "currency/annulment";
const LOCK_SUFFIXES: &str = "bench/add/DexMasterItemGenerationCannotChangeSuffixes";
/// Essence of Horror's helmet modifier, a suffix.
const HORROR: &str = "SocketedGemsDealMoreElementalDamageEssence1";

fn spec(prefixes: &[&str], suffixes: &[&str], any: [u8; 2]) -> AbsSpec {
    let mods = |ids: &[&str]| -> Vec<SlotSpec> {
        ids.iter()
            .map(|id| SlotSpec {
                id: id.to_string(),
                or_better: false,
            })
            .collect()
    };
    AbsSpec {
        base: "Archdemon Crown".into(),
        item_level: 87,
        rarity: Rarity::Rare,
        influences: Vec::new(),
        prefixes: mods(prefixes),
        suffixes: mods(suffixes),
        any_prefixes: any[0],
        any_suffixes: any[1],
        corrupted: false,
    }
}

fn crown(prefixes: &[&str], suffixes: &[&str], any: [u8; 2]) -> AbsItem {
    let item = AbsItem::from_spec(test_data(), &spec(prefixes, suffixes, any)).expect("a crown");
    item.validate(test_data()).expect("a valid crown");
    item
}

/// The target's own modifiers, each as its exact tier.
fn slots_of(target: &AbsItem) -> Vec<Slot> {
    let spec = target.to_spec(test_data());
    spec.prefixes
        .iter()
        .chain(&spec.suffixes)
        .map(|s| Slot::from_spec(test_data(), s).expect("a slot"))
        .collect()
}

fn run(target: &AbsItem, methods: &[&str]) -> Preimages {
    preimages(test_data(), target, &slots_of(target), methods)
}

fn run_with(target: &AbsItem, slots: &[Slot], methods: &[&str]) -> Preimages {
    preimages(test_data(), target, slots, methods)
}

fn reason<'a>(found: &'a Preimages, method: &str) -> Option<&'a str> {
    found
        .skipped
        .iter()
        .find(|(m, _)| m == method)
        .map(|(_, why)| why.as_str())
}

/// Essence of Horror places its suffix, then draws the rest side first: 4, 5 or 6 modifiers at
/// 8:3:1. Recounted by hand in exact fractions.
#[test]
fn the_essence_leaves_the_six_compositions() {
    let cases: [([u8; 2], f64); 6] = [
        ([3, 0], 1.0 / 12.0),
        ([2, 1], 1.0 / 4.0),
        ([1, 2], 1.0 / 3.0),
        ([3, 1], 5.0 / 64.0),
        ([2, 2], 11.0 / 64.0),
        ([3, 2], 1.0 / 12.0),
    ];
    for (any, want) in cases {
        let target = crown(&[], &[HORROR], any);
        let found = run(&target, &[ESSENCE]);
        assert_eq!(found.steps.len(), 1, "{any:?}: {:?}", found.skipped);
        let step = &found.steps[0];
        assert!(
            (step.p - want).abs() < 1e-12,
            "{any:?}: {} != {want}",
            step.p
        );
        let input = &step.input;
        assert_eq!(input.item.rarity, Rarity::Normal);
        assert!(input.item.prefixes.is_empty() && input.item.suffixes.is_empty());
        assert_eq!(input.any, [0, 0]);
        assert_eq!(input.item.base, target.item.base);
        assert_eq!(input.item.item_level, 87);
        assert_eq!(input.item.influences, target.item.influences);
        assert!(step.note.is_some());
    }
}

/// Both sides open: one half for the prefix side, then IncreasedLife9's 1000 of the crown's
/// 49775 prefix weight.
#[test]
fn an_exalt_prices_the_modifier_it_added() {
    let found = run(&crown(&["IncreasedLife9"], &[], [0, 0]), &[EXALT]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert!((step.p - 0.010045203415369162).abs() < 1e-12, "{}", step.p);
    assert!(step.input.item.prefixes.is_empty() && step.input.item.suffixes.is_empty());
    assert_eq!(step.input.any, [0, 0]);
}

#[test]
fn an_exalt_on_an_untracked_slot_costs_half() {
    let found = run(&crown(&[], &[], [1, 0]), &[EXALT]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert!((step.p - 0.5).abs() < 1e-12, "{}", step.p);
    assert!(step.input.item.prefixes.is_empty() && step.input.item.suffixes.is_empty());
    assert_eq!(step.input.any, [0, 0]);
}

/// A side that untracked slots fill is closed to the orb, so the other side is certain.
#[test]
fn an_exalt_skips_a_side_untracked_slots_fill() {
    let found = run(&crown(&[], &[], [1, 3]), &[EXALT]);
    let prefix = found
        .steps
        .iter()
        .find(|s| s.input.any == [0, 3])
        .expect("the prefix bucket");
    assert!((prefix.p - 1.0).abs() < 1e-12, "{}", prefix.p);
}

/// Each removable modifier of the input is equally likely to go (claim `annul.uniform`): with
/// one prefix and one suffix, the orb takes the prefix one time in three.
#[test]
fn an_annul_takes_each_modifier_equally() {
    let target = crown(&["IncreasedLife9"], &["FireResist8"], [0, 0]);
    let found = run(&target, &[ANNUL]);
    assert_eq!(found.steps.len(), 2, "{:?}", found.skipped);
    let mut anys: Vec<[u8; 2]> = found.steps.iter().map(|s| s.input.any).collect();
    anys.sort();
    assert_eq!(anys, [[0, 1], [1, 0]]);
    for step in &found.steps {
        assert_eq!(step.input.item, target.item);
        assert!((step.p - 1.0 / 3.0).abs() < 1e-12, "{}", step.p);
    }
}

/// Untracked slots count toward the chance an untracked modifier was the one the orb took. The
/// simple page's default holds three prefixes (two untracked) and three suffixes (one tracked,
/// two untracked), so the prefix bucket is three of the input's six removable modifiers.
#[test]
fn an_annul_counts_untracked_slots() {
    let target = crown(&[], &[HORROR], [2, 2]);
    let found = run(&target, &[ANNUL]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert_eq!(step.input.any, [3, 2]);
    assert!((step.p - 0.5).abs() < 1e-12, "{}", step.p);
}

/// Both sides open: two untracked prefixes and one untracked suffix beside the tracked one.
#[test]
fn an_annul_splits_by_removable_modifiers() {
    let target = crown(&[], &[HORROR], [2, 1]);
    let found = run(&target, &[ANNUL]);
    assert_eq!(found.steps.len(), 2, "{:?}", found.skipped);
    let prefix = found
        .steps
        .iter()
        .find(|s| s.input.any == [3, 1])
        .expect("prefix");
    let suffix = found
        .steps
        .iter()
        .find(|s| s.input.any == [2, 2])
        .expect("suffix");
    assert!((prefix.p - 0.6).abs() < 1e-12, "{}", prefix.p);
    assert!((suffix.p - 0.4).abs() < 1e-12, "{}", suffix.p);
}

/// A locked side gives nothing up, and its metamod is the only removable prefix.
#[test]
fn a_locked_side_gives_no_annul_row() {
    let target = crown(
        &["DexMasterItemGenerationCannotChangeSuffixes"],
        &["FireResist8"],
        [0, 1],
    );
    let found = run(&target, &[ANNUL]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert_eq!(step.input.any, [1, 1]);
    assert!((step.p - 0.5).abs() < 1e-12, "{}", step.p);
}

/// "Suffixes Cannot Be Changed" is a prefix craft.
#[test]
fn a_bench_lock_costs_nothing() {
    let target = crown(
        &["DexMasterItemGenerationCannotChangeSuffixes"],
        &[],
        [0, 0],
    );
    let found = run(&target, &[LOCK_SUFFIXES]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert_eq!(step.p, 1.0);
    assert!(step.input.item.prefixes.is_empty());
    assert_eq!(step.input.any, [0, 0]);
}

/// A full item cannot have lost a modifier, but an Exalted Orb could have filled its last slot:
/// one step per modifier, each from the side left open.
#[test]
fn a_full_item_shows_why_no_annulment_came_before_it() {
    let target = crown(
        &[
            "IncreasedLife9",
            "IncreasedMana12",
            "LocalIncreasedArmourAndEnergyShield1",
        ],
        &["FireResist8", "ColdResist8", "LightningResist8"],
        [0, 0],
    );
    let found = run(&target, &[EXALT, ANNUL]);
    assert_eq!(
        reason(&found, ANNUL),
        Some(
            "the item already holds every modifier it can, so nothing could have been removed from it"
        )
    );
    assert_eq!(reason(&found, EXALT), None);
    assert_eq!(found.steps.len(), 6);
    assert!(found.steps.iter().all(|s| s.method == EXALT));
}

/// A normal item holds no explicit modifier, so neither orb could have touched it.
#[test]
fn a_normal_item_shows_why_nothing_came_before_it() {
    let mut normal = spec(&[], &[], [0, 0]);
    normal.rarity = Rarity::Normal;
    let target = AbsItem::from_spec(test_data(), &normal).expect("a normal crown");
    let found = run(&target, &[EXALT, ANNUL, LOCK_SUFFIXES, ESSENCE]);
    assert!(found.steps.is_empty());
    for method in [EXALT, ANNUL] {
        assert_eq!(
            reason(&found, method),
            Some("a normal item holds no explicit modifiers")
        );
    }
    assert_eq!(
        reason(&found, LOCK_SUFFIXES),
        Some("the item does not hold Suffixes Cannot Be Changed")
    );
    assert_eq!(
        reason(&found, ESSENCE),
        Some("an essence leaves a rare item")
    );
}

/// A second tracked modifier no longer stops the essence: the whole reforge is simulated, and
/// the step carries the simulation's 95% range.
#[test]
fn a_tracked_fill_modifier_is_simulated() {
    let found = run(&crown(&["IncreasedLife9"], &[HORROR], [1, 1]), &[ESSENCE]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert!(step.p > 0.0, "{}", step.p);
    assert!(step.interval.is_some());
    assert_eq!(step.input.item.rarity, Rarity::Normal);
}

#[test]
fn more_untracked_slots_than_the_rarity_holds_is_refused() {
    let item = AbsItem::from_spec(test_data(), &spec(&[], &[], [4, 0])).expect("the spec resolves");
    assert_eq!(
        item.validate(test_data()),
        Err("prefixes: 4 modifiers, the rarity holds 3".to_string())
    );
}

#[test]
fn a_method_outside_the_page_is_named() {
    let found = run(
        &crown(&["IncreasedLife9"], &[], [0, 0]),
        &["currency/chaos", "currency/nope"],
    );
    assert!(found.steps.is_empty());
    assert_eq!(
        reason(&found, "currency/chaos"),
        Some("this page does not model it yet")
    );
    assert_eq!(
        reason(&found, "currency/nope"),
        Some("this method is not in the bundle")
    );
}

/// The spec comes back sorted, reads as the same item, and keeps which slots are families.
#[test]
fn an_abstract_item_round_trips_its_spec() {
    let d = test_data();
    let item =
        AbsItem::from_spec(d, &spec(&["IncreasedLife9"], &[HORROR], [1, 2])).expect("a crown");
    let back = item.to_spec(d);
    assert_eq!(back.suffixes[0].id, HORROR);
    assert!(!back.suffixes[0].or_better);
    assert_eq!((back.any_prefixes, back.any_suffixes), (1, 2));
    assert_eq!(AbsItem::from_spec(d, &back).expect("again"), item);
    assert_eq!(item.mods(), 5);
}

/// A family slot reads back as one, and two tiers of one family cannot both be tracked.
#[test]
fn a_family_slot_round_trips() {
    let d = test_data();
    let mut family = spec(&["IncreasedLife11"], &[], [0, 0]);
    family.prefixes[0].or_better = true;
    let item = AbsItem::from_spec(d, &family).expect("a crown");
    assert!(
        item.better
            .contains(&d.mod_by_id("IncreasedLife11").unwrap())
    );
    let back = item.to_spec(d);
    assert!(back.prefixes[0].or_better);
    assert_eq!(AbsItem::from_spec(d, &back).expect("again"), item);
    let two = SlotSpec {
        id: "IncreasedLife9".into(),
        or_better: false,
    };
    let err = poe1::preimages::tracked(d, &family, &[two]).unwrap_err();
    assert!(err.contains("track the same family"), "{err}");
}

/// The chance the orb draws any tier of the family at or above the slot's own: the sum of the
/// family's candidates on an item that holds nothing.
fn family_chances(d: &poe1::data::Data) -> f64 {
    let empty = AbsItem::from_spec(d, &spec(&[], &[], [0, 0]))
        .expect("a crown")
        .item;
    let want = d.mod_at(d.mod_by_id(RESIST_FAMILY).expect("the tier"));
    poe1::apply::add_candidates(d, &empty, &Pool::Natural, SideReq::Any, false)
        .iter()
        .filter(|(m, _)| {
            let md = d.mod_at(*m);
            md.side == want.side && md.mod_type == want.mod_type && md.level >= want.level
        })
        .map(|(_, p)| *p)
        .sum()
}

fn resist_family(d: &poe1::data::Data) -> Vec<Slot> {
    let slot = SlotSpec {
        id: RESIST_FAMILY.into(),
        or_better: true,
    };
    vec![Slot::from_spec(d, &slot).expect("a slot")]
}

/// A family slot is filled by any tier at or above it, so the Exalted Orb prices them together.
#[test]
fn an_exalt_prices_a_family() {
    let d = test_data();
    let target = crown(&[], &[RESIST_FAMILY], [0, 0]);
    let found = run_with(&target, &resist_family(d), &[EXALT]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let want = family_chances(d);
    assert!(
        (found.steps[0].p - want).abs() < 1e-12,
        "{} != {want}",
        found.steps[0].p
    );
    assert!(want > 0.0);
}

/// An untracked slot is filled by a modifier no tracked slot names, so a family the chart tracks
/// does not count toward it.
#[test]
fn an_untracked_slot_excludes_tracked_families() {
    let d = test_data();
    let target = crown(&[], &[], [0, 1]);
    let found = run_with(&target, &resist_family(d), &[EXALT]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let want = 0.5 - family_chances(d);
    assert!(
        family_chances(d) > 0.0 && family_chances(d) < 0.5,
        "{}",
        family_chances(d)
    );
    assert!(
        (found.steps[0].p - want).abs() < 1e-12,
        "{} != {want}",
        found.steps[0].p
    );
}

/// A second tracked modifier no longer stops the essence: the whole reforge is simulated.
#[test]
fn an_essence_with_a_tracked_life_is_simulated() {
    let found = run(&crown(&["IncreasedLife9"], &[HORROR], [1, 2]), &[ESSENCE]);
    assert_eq!(found.steps.len(), 1, "{:?}", found.skipped);
    let step = &found.steps[0];
    assert!(step.p > 0.0, "{}", step.p);
    assert!(step.interval.is_some());
}
