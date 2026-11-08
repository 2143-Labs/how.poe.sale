//! Forward steps: applying a method to a node's items, grouped by the modifiers a chart tracks.
//! The base is an Archdemon Crown at item level 87 with no influence, as in `preimages.rs`.

use std::collections::{BTreeMap, BTreeSet};

use poe1::apply::{self};
use poe1::data::Data;
use poe1::forward::{self, Kind, Node};
use poe1::item::{AbsSpec, Rarity, SlotSpec};
use poe1::method::{Pool, SideReq};
use poe1::preimages::{self, AbsItem, Slot};
use poe1::test_data;

const EXALT: &str = "currency/exalted";
const ANNUL: &str = "currency/annulment";
const ESSENCE: &str = "essence/essence-of-horror";
const VEILED_CHAOS: &str = "currency/veiled-chaos";
/// Essence of Horror's helmet modifier, a suffix.
const HORROR: &str = "SocketedGemsDealMoreElementalDamageEssence1";
/// ColdResist6: the crown can roll tiers 6, 7 and 8 of it at item level 87.
const RESIST_FAMILY: &str = "ColdResist6";

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

/// A crown whose named slots stand for their family at that tier or higher.
fn crown_family(prefixes: &[&str], suffixes: &[&str], any: [u8; 2], family: &[&str]) -> AbsItem {
    let mut s = spec(prefixes, suffixes, any);
    for slot in s.prefixes.iter_mut().chain(s.suffixes.iter_mut()) {
        if family.contains(&slot.id.as_str()) {
            slot.or_better = true;
        }
    }
    let item = AbsItem::from_spec(test_data(), &s).expect("a crown");
    item.validate(test_data()).expect("a valid crown");
    item
}

fn node_of(item: &AbsItem) -> Node {
    Node {
        members: vec![(item.item.clone(), 1.0)],
        approx: false,
    }
}

fn method(id: &str) -> &'static poe1::method::Method {
    test_data().method(id).expect("a method")
}

/// The `any` counts of every item group, with its chance.
fn spacings(applied: &forward::Applied) -> Vec<([u8; 2], f64)> {
    let mut out: Vec<([u8; 2], f64)> = applied
        .groups
        .iter()
        .filter_map(|g| match &g.kind {
            Kind::Item(a) => Some((a.any, g.p)),
            _ => None,
        })
        .collect();
    out.sort_by_key(|a| a.0);
    out
}

fn slot(data: &Data, id: &str, or_better: bool) -> Slot {
    Slot::from_spec(
        data,
        &SlotSpec {
            id: id.into(),
            or_better,
        },
    )
    .expect("a slot")
}

/// Both sides have room, so the orb picks the side evenly: one item group each way.
#[test]
fn an_exalt_splits_by_side() {
    let start = crown(&["IncreasedLife9"], &["FireResist8"], [0, 0]);
    let applied = forward::apply_node(
        test_data(),
        &node_of(&start),
        method(EXALT),
        &[],
        forward::seed_for("start", EXALT),
    );
    let groups = spacings(&applied);
    assert_eq!(groups.len(), 2, "{:?}", applied.groups);
    assert_eq!(groups[0].0, [1, 2]);
    assert_eq!(groups[1].0, [2, 1]);
    for (any, p) in groups {
        assert!((p - 0.5).abs() < 1e-12, "{any:?}: {p}");
    }
}

/// A family slot counts every tier at or above it: the group's chance is the sum of those
/// candidates' chances.
#[test]
fn a_tracked_family_counts_every_better_tier() {
    let data = test_data();
    let start = crown(&[], &["FireResist8"], [0, 0]);
    let tracked = [slot(data, RESIST_FAMILY, true)];
    let applied = forward::apply_node(
        data,
        &node_of(&start),
        method(EXALT),
        &tracked,
        forward::seed_for("start", EXALT),
    );
    let met = applied
        .groups
        .iter()
        .find(|g| match &g.kind {
            Kind::Item(a) => a
                .item
                .suffixes
                .iter()
                .any(|x| x.roll.m == data.mod_by_id(RESIST_FAMILY).expect("the tier")),
            _ => false,
        })
        .expect("the family group");
    let want = data.mod_at(data.mod_by_id(RESIST_FAMILY).unwrap());
    let expected: f64 =
        apply::add_candidates(data, &start.item, &Pool::Natural, SideReq::Any, false)
            .iter()
            .filter(|(m, _)| {
                let md = data.mod_at(*m);
                md.side == want.side && md.mod_type == want.mod_type && md.level >= want.level
            })
            .map(|(_, p)| *p)
            .sum();
    assert!(expected > 0.0);
    assert!((met.p - expected).abs() < 1e-12, "{} != {expected}", met.p);
}

/// The Orb of Annulment takes each removable modifier equally: two of three are prefixes.
#[test]
fn an_annul_forward_is_uniform() {
    let start = crown(
        &["IncreasedLife9", "LocalIncreasedArmourAndEnergyShield5"],
        &["FireResist8"],
        [0, 0],
    );
    let applied = forward::apply_node(
        test_data(),
        &node_of(&start),
        method(ANNUL),
        &[],
        forward::seed_for("start", ANNUL),
    );
    let groups = spacings(&applied);
    assert_eq!(groups.len(), 2, "{:?}", applied.groups);
    assert_eq!(groups[0].0, [1, 1]);
    assert_eq!(groups[1].0, [2, 0]);
    assert!((groups[0].1 - 2.0 / 3.0).abs() < 1e-12, "{}", groups[0].1);
    assert!((groups[1].1 - 1.0 / 3.0).abs() < 1e-12, "{}", groups[1].1);
}

/// The essence's reforge adds only untracked modifiers around the tracked essence modifier, so
/// it is counted exactly, and its six compositions are the chances `compositions` computes.
#[test]
fn the_essence_reforge_matches_the_compositions() {
    let data = test_data();
    let start = crown(&[], &[], [0, 0]);
    let mut normal = start.clone();
    normal.item.rarity = Rarity::Normal;
    let tracked = [slot(data, HORROR, false)];
    let applied = forward::apply_node(
        data,
        &node_of(&normal),
        method(ESSENCE),
        &tracked,
        forward::seed_for("start", ESSENCE),
    );
    assert_eq!(applied.simulated, None);
    let cases: [([u8; 2], f64); 6] = [
        ([3, 0], 1.0 / 12.0),
        ([2, 1], 1.0 / 4.0),
        ([1, 2], 1.0 / 3.0),
        ([3, 1], 5.0 / 64.0),
        ([2, 2], 11.0 / 64.0),
        ([3, 2], 1.0 / 12.0),
    ];
    let groups = spacings(&applied);
    for (any, want) in cases {
        let (_, p) = groups
            .iter()
            .find(|(a, _)| *a == any)
            .unwrap_or_else(|| panic!("{any:?} missing from {groups:?}"));
        assert!((p - want).abs() < 1e-12, "{any:?}: {p} != {want}");
        assert!(applied.groups.iter().all(|g| g.interval.is_none()));
    }
}

/// The same reforge with life also tracked can land a tracked modifier, so it is simulated. Its
/// totals per side still land on the six compositions within the simulation's error: this checks
/// `fill_once`'s side choice against the exact count.
#[test]
fn a_simulated_essence_matches_the_compositions() {
    let data = test_data();
    let mut normal = crown(&[], &[], [0, 0]);
    normal.item.rarity = Rarity::Normal;
    let tracked = [
        slot(data, HORROR, false),
        slot(data, "IncreasedLife9", true),
    ];
    let applied = forward::apply_node(
        data,
        &node_of(&normal),
        method(ESSENCE),
        &tracked,
        forward::seed_for("start", ESSENCE),
    );
    assert_eq!(applied.simulated, Some(forward::SIMULATIONS));
    let mut totals: BTreeMap<[usize; 2], f64> = BTreeMap::new();
    for group in &applied.groups {
        let Kind::Item(class) = &group.kind else {
            panic!("not an item: {:?}", group.kind);
        };
        let sides = [
            class.item.prefixes.len() + usize::from(class.any[0]),
            class.item.suffixes.len() + usize::from(class.any[1]),
        ];
        *totals.entry(sides).or_default() += group.p;
    }
    let n = f64::from(forward::SIMULATIONS);
    let cases: [([usize; 2], f64); 6] = [
        ([3, 1], 1.0 / 12.0),
        ([2, 2], 1.0 / 4.0),
        ([1, 3], 1.0 / 3.0),
        ([3, 2], 5.0 / 64.0),
        ([2, 3], 11.0 / 64.0),
        ([3, 3], 1.0 / 12.0),
    ];
    assert_eq!(totals.len(), cases.len(), "{totals:?}");
    for (sides, want) in cases {
        let got = totals.get(&sides).copied().unwrap_or(0.0);
        let tolerance = 4.0 * (want * (1.0 - want) / n).sqrt();
        assert!(
            (got - want).abs() < tolerance,
            "{sides:?}: {got} != {want} in {totals:?}"
        );
    }
}

/// The same reforge, read backwards and forwards with the same seed, is the same number.
#[test]
fn backward_and_forward_agree() {
    let data = test_data();
    let target = crown_family(&[], &[HORROR, RESIST_FAMILY], [1, 1], &[RESIST_FAMILY]);
    let tracked = [slot(data, HORROR, false), slot(data, RESIST_FAMILY, true)];
    let back = preimages::preimages(data, &target, &tracked, &[ESSENCE]);
    assert_eq!(back.steps.len(), 1, "{:?}", back.skipped);
    let step = &back.steps[0];
    assert!(step.interval.is_some());
    let applied = forward::apply_node(
        data,
        &node_of(&step.input),
        method(ESSENCE),
        &tracked,
        forward::seed_for(&preimages::abs_key(data, &target), ESSENCE),
    );
    let want = preimages::abs_key(data, &target);
    let group = applied
        .groups
        .iter()
        .find(|g| match &g.kind {
            Kind::Item(a) => preimages::abs_key(data, a) == want,
            _ => false,
        })
        .expect("the target's class");
    assert_eq!(step.p, group.p);
}

#[test]
fn a_simulation_is_deterministic() {
    let data = test_data();
    let mut normal = crown(&[], &[], [0, 0]);
    normal.item.rarity = Rarity::Normal;
    let tracked = [slot(data, HORROR, false)];
    let seed = forward::seed_for("start", ESSENCE);
    let first = forward::apply_node(data, &node_of(&normal), method(ESSENCE), &tracked, seed);
    let second = forward::apply_node(data, &node_of(&normal), method(ESSENCE), &tracked, seed);
    let p = |a: &forward::Applied| -> Vec<f64> { a.groups.iter().map(|g| g.p).collect() };
    assert_eq!(p(&first), p(&second));
}

/// A node reached backward has no members. Realising it draws items that belong to the same
/// class, so pushing one forward answers about the same items.
#[test]
fn realise_keeps_the_class() {
    let data = test_data();
    let target = crown_family(&[], &[HORROR, RESIST_FAMILY], [1, 1], &[RESIST_FAMILY]);
    let tracked = [slot(data, HORROR, false), slot(data, RESIST_FAMILY, true)];
    let node = forward::realise(data, &target, &tracked, forward::seed_for("x", ""));
    assert!(node.approx);
    assert!(node.members.len() > 1, "{}", node.members.len());
    for (item, _) in &node.members {
        assert_eq!(forward::classify(data, &tracked, item), target);
    }
    let total: f64 = node.members.iter().map(|(_, w)| *w).sum();
    assert!((total - 1.0).abs() < 1e-9, "{total}");
}

/// A hit on every tracked slot is what the page calls the goal.
#[test]
fn a_class_keeps_the_tiers_it_was_tracked_with() {
    let data = test_data();
    let target = crown_family(&[], &[HORROR, RESIST_FAMILY], [1, 1], &[RESIST_FAMILY]);
    let tracked = [slot(data, HORROR, false), slot(data, RESIST_FAMILY, true)];
    let better: BTreeSet<u32> = tracked
        .iter()
        .filter(|s| s.or_better)
        .map(|s| s.m)
        .collect();
    assert_eq!(target.better, better);
    let key = preimages::abs_key(data, &target);
    let again = AbsItem::from_spec(data, &target.to_spec(data)).expect("the spec resolves");
    assert_eq!(preimages::abs_key(data, &again), key);
}

/// A Veiled Chaos Orb is a Chaos Orb whose result has one of its added modifiers veiled, on that
/// modifier's own side (claim `veiled-chaos.replaces-one`): 4, 5 or 6 modifiers at 8:3:1
/// (claim `chaos.count`), exactly one of them the placeholder, which a class names rather than
/// counts. With room on both sides the veil is a prefix half the time. Nothing is tracked, so
/// the counts are exact.
#[test]
fn a_veiled_chaos_veils_one_added_modifier() {
    let data = test_data();
    let start = crown(&["IncreasedLife9"], &["FireResist8"], [0, 0]);
    let applied = forward::apply_node(
        data,
        &node_of(&start),
        method(VEILED_CHAOS),
        &[],
        forward::seed_for("start", VEILED_CHAOS),
    );
    assert_eq!(applied.simulated, None);
    let placeholders = apply::VEILED.map(|id| data.mod_by_id(id).expect("a placeholder"));
    let mut totals = [0.0f64; 3];
    let mut prefix = 0.0;
    for group in &applied.groups {
        let Kind::Item(class) = &group.kind else {
            panic!("not an item: {:?}", group.kind);
        };
        let named: Vec<u32> = class.item.explicits().map(|a| a.roll.m).collect();
        assert_eq!(named.len(), 1, "only the veil is named: {named:?}");
        assert!(placeholders.contains(&named[0]), "{named:?}");
        if named[0] == placeholders[0] {
            prefix += group.p;
        }
        let total = 1 + usize::from(class.any[0]) + usize::from(class.any[1]);
        totals[total - 4] += group.p;
    }
    let near = |got: f64, want: f64| (got - want).abs() < 1e-12;
    for (got, want) in totals.into_iter().zip([8.0 / 12.0, 3.0 / 12.0, 1.0 / 12.0]) {
        assert!(near(got, want), "{totals:?}");
    }
    assert!(near(prefix, 0.5), "veiled prefix {prefix}");
}

/// The same Veiled Chaos Orb with life tracked is simulated. Its totals and the veil's side still
/// land on the exact counts within the simulation's error: this checks `veil` and the side choice.
#[test]
fn a_simulated_veiled_chaos_matches_the_exact_counts() {
    let data = test_data();
    let start = crown(&["IncreasedLife9"], &["FireResist8"], [0, 0]);
    let tracked = [slot(data, "IncreasedLife9", true)];
    let applied = forward::apply_node(
        data,
        &node_of(&start),
        method(VEILED_CHAOS),
        &tracked,
        forward::seed_for("start", VEILED_CHAOS),
    );
    assert_eq!(applied.simulated, Some(forward::SIMULATIONS));
    let placeholders = apply::VEILED.map(|id| data.mod_by_id(id).expect("a placeholder"));
    let mut totals = [0.0f64; 3];
    let mut prefix = 0.0;
    for group in &applied.groups {
        let Kind::Item(class) = &group.kind else {
            panic!("not an item: {:?}", group.kind);
        };
        let named: Vec<u32> = class.item.explicits().map(|a| a.roll.m).collect();
        let veils: Vec<u32> = named
            .iter()
            .copied()
            .filter(|m| placeholders.contains(m))
            .collect();
        assert_eq!(veils.len(), 1, "exactly one veil: {named:?}");
        if veils[0] == placeholders[0] {
            prefix += group.p;
        }
        let total = named.len() + usize::from(class.any[0]) + usize::from(class.any[1]);
        totals[total - 4] += group.p;
    }
    let n = f64::from(forward::SIMULATIONS);
    let near = |got: f64, want: f64| (got - want).abs() < 4.0 * (want * (1.0 - want) / n).sqrt();
    for (got, want) in totals.into_iter().zip([8.0 / 12.0, 3.0 / 12.0, 1.0 / 12.0]) {
        assert!(near(got, want), "{totals:?}");
    }
    assert!(near(prefix, 0.5), "veiled prefix {prefix}");
}

/// A chance no run hit still has an upper bound: the Wilson range at 0 of 200,000.
#[test]
fn an_outcome_no_run_hit_has_an_upper_bound() {
    let [low, high] = forward::interval(0.0, 200_000);
    assert_eq!(low, 0.0);
    assert!(
        (high - 3.8416 / (200_000.0 + 3.8416)).abs() < 1e-12,
        "{high}"
    );
}

fn assert_spread(got: &BTreeMap<[u8; 2], f64>, want: &[([u8; 2], f64)]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (added, p) in want {
        let g = got.get(added).copied().unwrap_or(-1.0);
        assert!((g - p).abs() < 1e-12, "{added:?}: {g} != {p} in {got:?}");
    }
}

/// Each draw picks a side evenly among those with room and candidates; a draw with neither adds
/// nothing (claim `reforge.side-first`).
#[test]
fn side_first_splits_each_draw_evenly() {
    assert_spread(
        &forward::side_first([3, 2], [true, true], 3),
        &[([3, 0], 1.0 / 8.0), ([2, 1], 3.0 / 8.0), ([1, 2], 0.5)],
    );
    assert_spread(
        &forward::side_first([3, 3], [false, true], 3),
        &[([0, 3], 1.0)],
    );
    assert_spread(
        &forward::side_first([1, 0], [true, true], 2),
        &[([1, 0], 1.0)],
    );
}

/// A reforge's members are each in the one form `canonical` gives, so equal items merge.
#[test]
fn reforge_members_are_canonical() {
    let data = test_data();
    let start = crown(&[], &[], [0, 0]);
    let tracked = [slot(data, "IncreasedLife9", true)];
    let applied = forward::apply_node(
        data,
        &node_of(&start),
        method("currency/chaos"),
        &tracked,
        forward::seed_for("start", "currency/chaos"),
    );
    assert_eq!(applied.simulated, Some(forward::SIMULATIONS));
    for group in &applied.groups {
        for (item, _) in &group.members {
            let mut again = item.clone();
            again.canonical(data);
            assert_eq!(*item, again);
        }
    }
}

/// "Tries" needs a step that can be used again on its own result with the same odds: a step
/// that clears first and leaves the same protected modifiers.
#[test]
fn only_a_clearing_step_repeats() {
    let data = test_data();
    let mut magic = crown(&["IncreasedLife9"], &[], [0, 0]);
    magic.item.rarity = Rarity::Magic;
    let run = |start: &AbsItem, id: &str| {
        forward::apply_node(
            data,
            &node_of(start),
            method(id),
            &[],
            forward::seed_for("start", id),
        )
    };
    assert!(run(&magic, "currency/alteration").repeatable);
    let rare = crown(&["IncreasedLife9"], &["FireResist8"], [0, 0]);
    assert!(!run(&rare, EXALT).repeatable);

    // The metacraft: the lock is gone after the reforge, so the result is a different start.
    const LOCK: &str = "bench/add/DexMasterItemGenerationCannotChangeSuffixes";
    let mut s = spec(
        &[],
        &["FireResist8", "ColdResist8", "LightningResist8"],
        [0, 0],
    );
    s.base = "Slink Boots".into();
    s.item_level = 86;
    let boots = AbsItem::from_spec(data, &s).expect("boots");
    let lock = run(&boots, LOCK);
    assert_eq!(lock.groups.len(), 1);
    let locked = forward::child(&lock.groups[0], forward::seed_for("locked", ""));
    let veiled = forward::apply_node(
        data,
        &locked,
        method(VEILED_CHAOS),
        &[],
        forward::seed_for("locked", VEILED_CHAOS),
    );
    assert!(!veiled.repeatable);
    assert_eq!(veiled.simulated, None);
    let mut p: Vec<f64> = veiled.groups.iter().map(|g| g.p).collect();
    p.sort_by(|a, b| b.total_cmp(a));
    assert_eq!(p.len(), 3, "{p:?}");
    for (got, want) in p.iter().zip([2.0 / 3.0, 1.0 / 4.0, 1.0 / 12.0]) {
        assert!((got - want).abs() < 1e-12, "{p:?}");
    }
}

/// After its clear an Alteration sees only the cleared item, so a class's thousand realised
/// items give exactly what one of them gives.
#[test]
fn an_alteration_from_a_class_matches_one_member() {
    let data = test_data();
    let mut class = crown(&[], &[], [1, 1]);
    class.item.rarity = Rarity::Magic;
    let tracked = [slot(data, "IncreasedLife9", true)];
    let realised = forward::realise(data, &class, &tracked, forward::seed_for("class", ""));
    assert!(realised.members.len() > 1, "{}", realised.members.len());
    let one = Node {
        members: vec![(realised.members[0].0.clone(), 1.0)],
        approx: false,
    };
    let alteration = method("currency/alteration");
    let seed = forward::seed_for("class", "currency/alteration");
    let groups = |node: &Node| -> Vec<(String, f64)> {
        forward::apply_node(data, node, alteration, &tracked, seed)
            .groups
            .iter()
            .map(|g| (g.kind.key(data), g.p))
            .collect()
    };
    let (all, single) = (groups(&realised), groups(&one));
    assert_eq!(all.len(), single.len(), "{all:?} != {single:?}");
    for ((key, p), (other, q)) in all.iter().zip(&single) {
        assert_eq!(key, other);
        assert!((p - q).abs() < 1e-12, "{key}: {p} != {q}");
    }
}

/// A Harvest "reforge with a Physical modifier" is a Chaos Orb's reforge that repeats until the
/// result holds a physical modifier (claim `reforge.guarantee-present`): every result holds one,
/// and the chance of a physical modifier is the Chaos Orb's chance divided by the Chaos Orb's
/// chance of any physical modifier.
#[test]
fn a_tag_reforge_is_a_chaos_orb_that_reached_its_tag() {
    const PERCENT: &str = "LocalIncreasedPhysicalDamagePercent8";
    let data = test_data();
    let mut s = spec(&[], &[], [0, 0]);
    s.base = "Jewelled Foil".into();
    s.item_level = 86;
    let foil = AbsItem::from_spec(data, &s).expect("a foil");
    let tracked = [slot(data, PERCENT, false)];
    let physical = data.tag_by_name("physical").expect("a physical tag");
    let has_physical = |item: &poe1::item::Item| {
        item.explicits()
            .any(|a| data.mod_at(a.roll.m).tags.contains(&physical))
    };
    let holds = |applied: &forward::Applied| -> f64 {
        applied
            .groups
            .iter()
            .filter(|g| matches!(&g.kind, Kind::Item(c) if c.item.explicits().any(|a| a.roll.m == tracked[0].m)))
            .map(|g| g.p)
            .sum()
    };
    let run = |id: &str| {
        forward::apply_node(
            data,
            &node_of(&foil),
            method(id),
            &tracked,
            forward::seed_for("foil", id),
        )
    };
    let chaos = run("currency/chaos");
    let harvest = run("harvest/reforge-physical");
    assert_eq!(harvest.simulated, Some(forward::SIMULATIONS));
    for group in &harvest.groups {
        assert!(matches!(group.kind, Kind::Item(_)), "{:?}", group.kind);
        assert!(group.members.iter().all(|(item, _)| has_physical(item)));
    }
    let chaos_physical: f64 = chaos
        .groups
        .iter()
        .map(|g| {
            g.p * g
                .members
                .iter()
                .filter(|(item, _)| has_physical(item))
                .map(|(_, w)| w)
                .sum::<f64>()
        })
        .sum();
    let want = holds(&chaos) / chaos_physical;
    let got = holds(&harvest);
    let n = f64::from(forward::SIMULATIONS);
    let sd = (got * (1.0 - got) / n).sqrt() + (want * (1.0 - want) / n).sqrt() / chaos_physical;
    assert!(
        chaos_physical > 0.1 && chaos_physical < 0.95,
        "{chaos_physical}"
    );
    assert!((got - want).abs() < 5.0 * sd, "{got} != {want}");
}
