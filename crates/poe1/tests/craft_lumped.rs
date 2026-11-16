//! The lumped state: the coarse class is exact, one lumped step reproduces the concrete step's
//! per-stage masses as the same `Fraction`s, and three depths run in bounded time with every
//! depth's mass accounted for.

use std::collections::BTreeMap;

use poe1::craft::lumped::{Class, Lumped};
use poe1::craft::{LoopPolicy, Machine, Stage};
use poe1::currency::{Accept, Currency, Fraction, ItemSetFilter, Transmutation};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::pattern::{Pattern, PatternSpec};

/// The owner's shield: a Shaper Supreme Spiked Shield, item level 87, normal to begin with.
fn shield(data: &Data) -> Item {
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "normal",
            "influences": ["shaper"]
        }"#,
    )
    .expect("spec");
    Item::from_spec(data, &spec).expect("the shield")
}

fn pattern(data: &Data) -> Pattern {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/patterns/totem-shield.json"
    );
    let spec: PatternSpec =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the pattern fixture"))
            .expect("pattern spec");
    Pattern::compile(data, &spec).expect("compiles")
}

fn filter(pattern: &Pattern) -> ItemSetFilter {
    ItemSetFilter::from_pattern(pattern, Accept::DesiredOrGoodBothSides)
}

/// The exact sum of a list of chances.
fn sum<'a>(chances: impl IntoIterator<Item = &'a Fraction>) -> Fraction {
    chances
        .into_iter()
        .fold(Fraction::ZERO, |acc, &p| acc.checked_add(p).expect("exact"))
}

/// `a >= b`, exactly. `Fraction` has no ordering — a derived `Ord` would compare the numerator
/// and then the denominator, not the values (1/3 would sort after 1/2) — so a curve has to
/// cross-multiply to be read at all. The products reach about 1.6 × 10³⁸ here, so they are checked
/// rather than left to wrap.
fn at_least(a: Fraction, b: Fraction) -> bool {
    let left = a
        .num()
        .checked_mul(b.den())
        .expect("the cross-product of two loop masses fits in u128");
    let right = b
        .num()
        .checked_mul(a.den())
        .expect("the cross-product of two loop masses fits in u128");
    left >= right
}

/// One lumped step from the normal shield is the concrete step folded by class, exactly.
///
/// The stages compared are every stage a transmutation of a normal item can record: `Desired` (the
/// goal prefix, absorbed on reach), `GoodPair`, `GoodPrefix`, `GoodSuffix` and `Other`. `Spare` is
/// impossible from a magic item with a modifier, and `Reset` and `Stop` are policy terminals this
/// step never produces.
#[test]
fn one_lumped_step_equals_the_concrete_step() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let policy = LoopPolicy::new(&filter);

    let mut concrete = Machine::new(data, filter.clone(), shield(data));
    let step = concrete
        .step(&policy)
        .expect("a normal item is transmutable");

    let mut lumped = Lumped::new(data, filter.clone(), shield(data));
    let depth = lumped.step(&policy).expect("the coarse step");

    assert_eq!(
        depth.chances.as_slice(),
        step.chances,
        "every per-stage mass must be the same exact Fraction"
    );
    assert_eq!(
        depth.live(),
        sum(step.state.iter().map(|(_, p)| p)),
        "the coarse live mass is the concrete live mass"
    );

    // Stronger than the stage totals: fold the concrete items by class and compare the mass of
    // each class. This is the claim the lumping rests on.
    let mut folded: BTreeMap<Class, Fraction> = BTreeMap::new();
    for (item, p) in step.state {
        let class = Class::of(data, &filter, item);
        let slot = folded.entry(class).or_insert(Fraction::ZERO);
        *slot = slot.checked_add(*p).expect("exact");
    }
    let concrete_classes: Vec<(Class, Fraction)> = folded.into_iter().collect();
    assert_eq!(
        depth.classes, concrete_classes,
        "the concrete distribution folded by class is the lumped distribution"
    );
}

/// The concrete distribution after one transmutation is 9 767 items; the coarse state is far
/// smaller.
///
/// The 9 767 is the currency's own set, which still contains the desired prefixes that the machine
/// then absorbs; the live state is the set minus those. The exact class cannot reach the audit's two
/// orders of magnitude: a pool-exact class has to keep the per-side group sets, and the shield's
/// transmutation pool has only a few dozen distinct per-side signatures, so
/// `prefix_signatures × suffix_signatures` bounds the two-affix classes well above a hundredth of
/// 9 767. The numbers are asserted rather than a ratio so a widening class is a test failure, not a
/// silently weaker bound.
#[test]
fn the_coarse_state_is_far_smaller_than_the_concrete_one() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let policy = LoopPolicy::new(&filter);

    // The whole transmutation distribution, before the machine absorbs the goal.
    let set = Transmutation::new(data).apply(&shield(data), &None);
    assert!(set.exact(), "the transmutation set is a distribution");
    assert_eq!(
        set.outcomes().len(),
        9_767,
        "the concrete distribution after one transmutation"
    );

    let mut concrete = Machine::new(data, filter.clone(), shield(data));
    let step = concrete
        .step(&policy)
        .expect("a normal item is transmutable");
    let mut lumped = Lumped::new(data, filter.clone(), shield(data));
    let depth = lumped.step(&policy).expect("the coarse step");

    let live = step.state.len();
    let classes = depth.classes.len();
    println!(
        "one transmutation: 9767 items in the set, {live} live after absorption, {classes} classes"
    );
    assert!(
        live < 9_767,
        "the desired items left the live state: {live} live of 9 767 outcomes"
    );
    assert!(
        classes * 10 < 9_767,
        "the coarse state must be at least an order of magnitude smaller: {classes} classes \
         against 9 767 items"
    );
    assert!(
        classes < 1_000,
        "the exact class stays bounded by the per-side group signatures: {classes} classes"
    );
}

/// Three lumped depths, bounded in time, with every step's mass accounted for: the per-stage masses
/// cover exactly what the step accounted for, and that mass plus everything already gone is one, so
/// no mass is counted twice — the double-counting fix, since absorbed mass leaves the live state
/// and cannot appear in a later depth.
#[test]
fn three_lumped_steps_account_for_their_mass() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let policy = LoopPolicy::new(&filter);

    let mut lumped = Lumped::new(data, filter.clone(), shield(data));
    let started = std::time::Instant::now();
    let depths = lumped
        .advance(&policy, 3)
        .expect("three lumped steps are exact");
    let elapsed = started.elapsed();
    println!("three lumped steps took {elapsed:?}");

    assert_eq!(depths.len(), 3, "the loop has mass to move at every depth");
    assert!(elapsed.as_secs() < 60, "three depths took {elapsed:?}");

    // A depth is given the mass the previous depth left live, not necessarily all of one: the
    // desired mass absorbed earlier is gone from the loop. So the invariant is conservation, not
    // "every depth is one": what this depth accounts for, plus everything already absorbed or
    // terminal, is the whole one.
    let mut gone = Fraction::ZERO;
    let mut accumulated = Fraction::ZERO;
    for depth in depths {
        let terminals = depth
            .mass(Stage::Reset)
            .checked_add(depth.mass(Stage::Stop))
            .expect("exact");
        let this = depth
            .live()
            .checked_add(depth.absorbed)
            .expect("exact")
            .checked_add(terminals)
            .expect("exact");
        assert_eq!(
            sum(depth.chances.iter().map(|(_, p)| p)),
            this,
            "depth {}: the summary covers exactly the mass the step accounted for",
            depth.index
        );
        assert_eq!(
            this.checked_add(gone).expect("exact"),
            Fraction::ONE,
            "depth {}: live plus absorbed plus terminal, plus everything already gone, is one",
            depth.index
        );
        assert_eq!(
            depth.absorbed,
            depth.mass(Stage::Desired),
            "depth {}: `absorbed` is the Desired mass",
            depth.index
        );
        assert!(
            !depth.classes.is_empty(),
            "depth {} leaves work",
            depth.index
        );

        println!(
            "depth {}: {} live classes, live {}, absorbed {}, terminal {}",
            depth.index,
            depth.classes.len(),
            depth.live(),
            depth.absorbed,
            terminals
        );
        // The goal's mass only ever accumulates: `absorbed` left the live state, so adding it to a
        // running total cannot decrease that total. There is nothing to assert there; the
        // conservation checks above are what catch a double count.
        accumulated = accumulated.checked_add(depth.absorbed).expect("exact");
        gone = gone
            .checked_add(depth.absorbed)
            .expect("exact")
            .checked_add(terminals)
            .expect("exact");
    }
    assert!(
        !accumulated.is_zero(),
        "the goal is reachable within three depths"
    );
    assert!(
        at_least(Fraction::ONE, accumulated),
        "absorbed mass is part of the same one, never more"
    );
}
