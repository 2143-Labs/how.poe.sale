//! The craft loop as a machine: the stage an item is in, the owner's policy, and one exact
//! distribution step.

use poe1::apply::draw_weights;
use poe1::craft::{LoopPolicy, Machine, Policy, Stage, Step, StepKind};
use poe1::currency::{Accept, Fraction, ItemSetFilter, Tier};
use poe1::data::{Data, Rarity, Side};
use poe1::item::{Affix, Item, ItemSpec};
use poe1::method::{Pool, SideReq};
use poe1::pattern::{Pattern, PatternSpec};

/// The owner's shield: a Shaper Supreme Spiked Shield, item level 87, normal to begin with.
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

/// The modifiers one draw on `item` can reach, in the data's own order.
fn draw(data: &Data, item: &Item) -> Vec<u32> {
    draw_weights(data, item, &Pool::Natural, SideReq::Any, false, false)
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

/// The first reachable modifier on `side` that `filter` calls `tier`.
fn mod_of(data: &Data, item: &Item, filter: &ItemSetFilter, side: Side, tier: Tier) -> u32 {
    draw(data, item)
        .into_iter()
        .find(|&m| data.mod_at(m).side == Some(side) && filter.tier_of_mod(data, m) == tier)
        .unwrap_or_else(|| panic!("no {tier:?} modifier on the {} side", side.name()))
}

/// The exact sum of a list of chances.
fn sum<'a>(chances: impl IntoIterator<Item = &'a Fraction>) -> Fraction {
    chances
        .into_iter()
        .fold(Fraction::ZERO, |acc, &p| acc.checked_add(p).expect("exact"))
}

#[test]
fn a_normal_shield_is_spare_and_is_transmuted() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let item = shield(data);
    assert_eq!(
        Stage::classify(data, &filter, &item),
        Stage::Spare,
        "a normal shield has no explicit modifiers"
    );
    let step = LoopPolicy::new(&filter).step(data, &item);
    assert_eq!(step.currency, StepKind::Transmutation);
    assert!(!step.reason.is_empty());
}

#[test]
fn a_magic_item_with_a_good_prefix_is_augmented() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let mut magic = shield(data);
    magic.rarity = Rarity::Magic;
    let good = mod_of(data, &magic, &filter, Side::Prefix, Tier::Good);
    magic.prefixes.push(Affix::new(good));
    assert_eq!(Stage::classify(data, &filter, &magic), Stage::GoodPrefix);
    let step = LoopPolicy::new(&filter).step(data, &magic);
    assert_eq!(step.currency, StepKind::Augmentation);
}

#[test]
fn one_transmutation_step_puts_the_goal_in_the_distribution() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let mut machine = Machine::new(data, filter.clone(), shield(data));
    let snapshot = machine
        .step(&LoopPolicy::new(&filter))
        .expect("a normal item is transmutable");

    // A transmutation is 1/2 one modifier and 1/2 two. A one-modifier magic item takes its single
    // modifier on the prefix side half the time; a two-modifier one always has one prefix and one
    // suffix, because the prefix side is full, so the second draw has only the suffix open. The
    // share of the mass that drew a prefix is therefore 1/2·1/2 + 1/2 = 3/4.
    let prefix_draws = sum(snapshot
        .state
        .iter()
        .filter(|(item, _)| !item.affixes(Side::Prefix).is_empty())
        .map(|(_, p)| p));
    assert_eq!(
        prefix_draws,
        Fraction::new(3, 4).unwrap(),
        "one branch draws a prefix half the time, the other always"
    );

    // Conditional on a prefix draw, the totem modifier is 800 of the published 64 800 weight, so
    // the unconditional chance is (3/4)·(800/64 800) = 1/108. The goal's only desired modifier is
    // that prefix, so this is exactly the Desired mass.
    let desired = snapshot
        .chances
        .iter()
        .find(|(stage, _)| *stage == Stage::Desired)
        .map(|(_, p)| *p)
        .expect("the goal is reachable in one step");
    assert_eq!(
        desired,
        prefix_draws
            .checked_mul(Fraction::new(800, 64_800).unwrap())
            .unwrap(),
        "the goal is 800/64 800 of the prefix draws"
    );
    assert_eq!(desired, Fraction::new(1, 108).unwrap());

    // The summary is a whole distribution: every item landed in exactly one stage.
    assert_eq!(sum(snapshot.chances.iter().map(|(_, p)| p)), Fraction::ONE);
    assert!(
        snapshot
            .chances
            .iter()
            .all(|(stage, p)| stage.is_terminal() || !p.is_zero())
    );
}

#[test]
fn a_rare_without_the_goal_is_reset() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let mut rare = shield(data);
    rare.rarity = Rarity::Rare;
    let any = mod_of(data, &rare, &filter, Side::Prefix, Tier::Any);
    rare.prefixes.push(Affix::new(any));
    assert_eq!(Stage::classify(data, &filter, &rare), Stage::SpentRare);
    let step = LoopPolicy::new(&filter).step(data, &rare);
    assert_eq!(step.currency, StepKind::Reset);

    // The machine records the reset as terminal mass and keeps nothing live: the caller starts a
    // fresh item rather than the machine silently chaining a scour into a transmutation.
    let mut machine = Machine::new(data, filter.clone(), rare);
    let snapshot = machine
        .step(&LoopPolicy::new(&filter))
        .expect("a reset applies no currency");
    assert!(snapshot.state.is_empty(), "a reset item leaves the loop");
    assert_eq!(snapshot.chances, &[(Stage::Reset, Fraction::ONE)]);
}

#[test]
fn a_rare_with_three_good_modifiers_stops_for_the_owner() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let mut rare = shield(data);
    rare.rarity = Rarity::Rare;
    let good_prefix = mod_of(data, &rare, &filter, Side::Prefix, Tier::Good);
    rare.prefixes.push(Affix::new(good_prefix));
    let second = draw(data, &rare)
        .into_iter()
        .find(|&m| {
            data.mod_at(m).side == Some(Side::Prefix)
                && filter.tier_of_mod(data, m) == Tier::Good
                && m != good_prefix
        })
        .expect("a second good prefix");
    rare.prefixes.push(Affix::new(second));
    let good_suffix = mod_of(data, &rare, &filter, Side::Suffix, Tier::Good);
    rare.suffixes.push(Affix::new(good_suffix));
    // The label is about the filter only: good on both sides is `GoodPair` whatever the rarity.
    // The policy is what knows a rare item cannot be regalled again.
    assert_eq!(Stage::classify(data, &filter, &rare), Stage::GoodPair);
    let step = LoopPolicy::new(&filter).step(data, &rare);
    assert_eq!(
        step.currency,
        StepKind::Stop,
        "three good modifiers without the goal are worth showing to the owner"
    );
}

/// The invariant that makes the summary trustworthy: one step's stages account for exactly the
/// mass that entered the step — live items included, not just terminals.
///
/// Only one step is asserted here, and that is a property of the model rather than of this test: an
/// alteration alone produces about 9 800 outcomes, so a second step over a distribution of any size
/// enumerates millions of items. Deeper runs need the state lumped or per-step reporting, which is
/// why `Machine` has no `expected_steps` (see its module docs).
#[test]
fn one_step_accounts_for_all_its_mass() {
    let data = poe1::test_data();
    let pattern = pattern(data);
    let filter = filter(&pattern);
    let policy = LoopPolicy::new(&filter);
    let mut machine = Machine::new(data, filter.clone(), shield(data));
    machine.step(&policy).expect("the loop takes a step");
    let summary = sum(machine.chances().iter().map(|(_, p)| p));
    assert_eq!(
        summary,
        Fraction::ONE,
        "the summary covers every item that entered the step"
    );
    let live = sum(machine.state().iter().map(|(_, p)| p));
    assert!(!live.is_zero(), "a normal item has somewhere to go");
}

/// A second policy over the same machine: reroll only, never augment and never regal. It exists to
/// show the seam — the machine, the exact weights and the stage vocabulary are the same, and only
/// the decision differs, so a policy is genuinely a decision function rather than part of the engine.
struct RerollOnly<'a> {
    filter: &'a ItemSetFilter,
}

impl Policy for RerollOnly<'_> {
    fn step(&self, data: &Data, item: &Item) -> Step {
        if self.filter.holds_desired(data, item) {
            return Step {
                currency: StepKind::Done,
                reason: "the item holds a desired modifier",
            };
        }
        match item.rarity {
            Rarity::Normal => Step {
                currency: StepKind::Transmutation,
                reason: "a normal item: start it as magic",
            },
            Rarity::Magic => Step {
                currency: StepKind::Alteration,
                reason: "reroll, whatever the item holds",
            },
            _ => Step {
                currency: StepKind::Reset,
                reason: "a rare item without the goal: start again",
            },
        }
    }
}

/// The two policies give the same machine two different answers, and both are exact: augmentation
/// can never add the desired prefix (a prefix draw is not what it does), while a reroll gives it
/// the usual 1/108.
#[test]
fn two_policies_give_the_same_machine_two_answers() {
    let data = poe1::test_data();
    let pattern = pattern(data);
    let filter = filter(&pattern);
    // A magic shield already holding a good prefix: the loop augments it, a reroll starts over.
    let mut magic: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "magic",
            "influences": ["shaper"],
            "prefixes": [{"id": "IncreasedLife10"}]
        }"#,
    )
    .expect("spec");
    let item = Item::from_spec(data, &mut magic).expect("the item");

    let mass = |chances: &[(Stage, Fraction)], stage: Stage| {
        chances
            .iter()
            .filter(|(s, _)| *s == stage)
            .map(|(_, p)| *p)
            .fold(Fraction::ZERO, |a, b| a.checked_add(b).expect("exact"))
    };

    let mut loop_machine = Machine::new(data, filter.clone(), item.clone());
    loop_machine
        .step(&LoopPolicy::new(&filter))
        .expect("the loop augments");
    let augmented = loop_machine.chances();
    assert_eq!(
        mass(augmented, Stage::Desired),
        Fraction::ZERO,
        "a suffix draw cannot be the goal"
    );

    let mut reroll = Machine::new(data, filter.clone(), item.clone());
    reroll
        .step(&RerollOnly { filter: &filter })
        .expect("a reroll");
    let rerolled = reroll.chances();
    assert_eq!(
        mass(rerolled, Stage::Desired),
        Fraction::new(1, 108).expect("1/108"),
        "one alteration puts the goal in the distribution at 1/108"
    );
}
