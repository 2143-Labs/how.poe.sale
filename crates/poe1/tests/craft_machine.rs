//! The craft loop as a machine: the stage an item is in, the owner's policy, and one exact
//! distribution step.

use poe1::apply::draw_weights;
use poe1::craft::{Action, CurrencyId, LoopPolicy, Machine, MachineError, Policy, Stage, Step};
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

/// The mass one stage holds in a summary.
fn summary_of(chances: &[(Stage, Fraction)], stage: Stage) -> Fraction {
    sum(chances.iter().filter(|(s, _)| *s == stage).map(|(_, p)| p))
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
    assert_eq!(step.action, Action::Apply(CurrencyId::TRANSMUTATION));
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
    assert_eq!(step.action, Action::Apply(CurrencyId::AUGMENTATION));
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
    //
    // With absorb-on-reach a desired outcome leaves the live state, and the only desired modifier is
    // a prefix, so the prefix draws are the live prefix-bearing items *plus* the absorbed mass.
    // This test is where the old totals move: `state` alone no longer holds the whole 3/4.
    let absorbed = snapshot
        .chances
        .iter()
        .find(|(stage, _)| *stage == Stage::Desired)
        .map(|(_, p)| *p)
        .expect("the goal is reachable in one step");
    let live_prefix = sum(snapshot
        .state
        .iter()
        .filter(|(item, _)| !item.affixes(Side::Prefix).is_empty())
        .map(|(_, p)| p));
    let prefix_draws = live_prefix.checked_add(absorbed).expect("exact");
    assert_eq!(
        prefix_draws,
        Fraction::new(3, 4).unwrap(),
        "one branch draws a prefix half the time, the other always"
    );
    assert!(
        snapshot
            .state
            .iter()
            .all(|(item, _)| Stage::classify(data, &filter, item) != Stage::Desired),
        "a desired outcome is absorbed into the summary, not left live"
    );

    // Conditional on a prefix draw, the totem modifier is 800 of the published 64 800 weight, so
    // the unconditional chance is (3/4)·(800/64 800) = 1/108. The goal's only desired modifier is
    // that prefix, so this is exactly the Desired mass.
    assert_eq!(
        absorbed,
        prefix_draws
            .checked_mul(Fraction::new(800, 64_800).unwrap())
            .unwrap(),
        "the goal is 800/64 800 of the prefix draws"
    );
    assert_eq!(absorbed, Fraction::new(1, 108).unwrap());

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
    assert_eq!(step.action, Action::Reset);

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
        step.action,
        Action::Stop,
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
    // The summary splits into the live mass and the mass absorbed at the goal; a transmutation has
    // no reset and no stop, so those are the only two parts.
    let absorbed = summary_of(machine.chances(), Stage::Desired);
    assert_eq!(
        live.checked_add(absorbed).expect("exact"),
        Fraction::ONE,
        "live plus absorbed is the whole mass"
    );
    // Each step that was taken is in the history, with its depth and its own summary.
    assert_eq!(machine.history().len(), 1);
    assert_eq!(machine.history()[0].0, 1);
    assert_eq!(machine.history()[0].1.as_slice(), machine.chances());
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
                action: Action::Done,
                reason: "the item holds a desired modifier",
            };
        }
        match item.rarity {
            Rarity::Normal => Step {
                action: Action::Apply(CurrencyId::TRANSMUTATION),
                reason: "a normal item: start it as magic",
            },
            Rarity::Magic => Step {
                action: Action::Apply(CurrencyId::ALTERATION),
                reason: "reroll, whatever the item holds",
            },
            _ => Step {
                action: Action::Reset,
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
    let magic: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "magic",
            "influences": ["shaper"],
            "prefixes": [{"id": "IncreasedLife10"}]
        }"#,
    )
    .expect("spec");
    let item = Item::from_spec(data, &magic).expect("the item");

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

/// A policy that says `Done` for an item without the goal is a defect, not a terminal: `Done` is
/// read from the item, so the mass would be recorded under a live stage and then dropped from the
/// state. The step asserts the two agree; a release build keeps the old reading, so this is a
/// debug-only guard, checked in the default (debug) test run.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "a policy's `Done` must name an item that holds the goal")]
fn a_done_that_is_not_the_goal_is_caught() {
    struct Premature;

    impl Policy for Premature {
        fn step(&self, _data: &Data, _item: &Item) -> Step {
            Step {
                action: Action::Done,
                reason: "the policy gives up without the goal",
            }
        }
    }

    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let mut machine = Machine::new(data, filter, shield(data));
    let _ = machine.step(&Premature);
}

/// An id the registry does not hold is an error, not a refusal. When the vocabulary was a closed
/// enum a policy could not even name a missing currency; with ids, a misspelt or unmodelled one is
/// reported as such instead of silently doing nothing.
#[test]
fn an_unregistered_id_is_an_unknown_currency() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));

    struct Unregistered;

    impl Policy for Unregistered {
        fn step(&self, _data: &Data, _item: &Item) -> Step {
            Step {
                action: Action::Apply(CurrencyId("currency/does-not-exist")),
                reason: "an id this crate does not model",
            }
        }
    }

    let mut machine = Machine::new(data, filter, shield(data));
    assert_eq!(
        machine.step(&Unregistered).unwrap_err(),
        MachineError::UnknownCurrency("currency/does-not-exist")
    );
}

/// The registry is what makes a currency reachable from the machine: the owner's loop never exalts,
/// so under the old closed vocabulary no policy could name `currency/exalted` at all. A rare item
/// with an empty side is a legal Exalted target, and the step must give that currency's exact
/// one-modifier distribution.
#[test]
fn a_registered_currency_the_loop_never_uses_is_reachable() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let mut rare = shield(data);
    rare.rarity = Rarity::Rare;
    let prefix = mod_of(data, &rare, &filter, Side::Prefix, Tier::Any);
    rare.prefixes.push(Affix::new(prefix));
    assert!(rare.affixes(Side::Suffix).is_empty());

    struct ExaltOnly;

    impl Policy for ExaltOnly {
        fn step(&self, _data: &Data, _item: &Item) -> Step {
            Step {
                action: Action::Apply(CurrencyId::EXALTED),
                reason: "a currency the loop itself never reaches for",
            }
        }
    }

    let before = rare.affixes(Side::Prefix).len() + rare.affixes(Side::Suffix).len();
    assert_eq!(before, 1, "the item starts with one modifier");
    let mut machine = Machine::new(data, filter.clone(), rare);
    let snapshot = machine
        .step(&ExaltOnly)
        .expect("a rare item with room exalts");

    assert!(!snapshot.state.is_empty(), "the exalt distribution is live");
    for (item, _) in snapshot.state {
        assert_eq!(item.rarity, Rarity::Rare, "an Exalted Orb keeps the rarity");
        assert_eq!(
            item.affixes(Side::Prefix).len() + item.affixes(Side::Suffix).len(),
            before + 1,
            "an Exalted Orb adds exactly one modifier"
        );
    }
    // A prefix draw can be the goal prefix, and the prefix side is open beside the empty suffix
    // side, so part of the mass is absorbed on reach. The live items all moved up by one modifier;
    // the absorbed ones did too, and left. Live plus absorbed is the whole mass.
    let absorbed = summary_of(snapshot.chances, Stage::Desired);
    assert_eq!(
        sum(snapshot.state.iter().map(|(_, p)| p))
            .checked_add(absorbed)
            .expect("exact"),
        Fraction::ONE,
        "the whole mass moved up by one modifier: nothing stayed where it was"
    );
    assert!(
        !absorbed.is_zero(),
        "the open prefix side can draw the goal prefix"
    );
    assert_eq!(sum(snapshot.chances.iter().map(|(_, p)| p)), Fraction::ONE);
}

/// Absorption fixes the double count: a desired item is recorded in the step that produced it and
/// leaves the live state. A second step over an empty state records nothing, instead of recording
/// the same `Desired` mass again as the old code did while the item sat live.
#[test]
fn a_desired_item_is_recorded_once_not_every_step() {
    let data = poe1::test_data();
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "magic",
            "influences": ["shaper"],
            "prefixes": [{"id": "MaximumTotemUber1"}]
        }"#,
    )
    .expect("spec");
    let item = Item::from_spec(data, &spec).expect("the item");
    let filter = filter(&pattern(data));
    let policy = LoopPolicy::new(&filter);
    let mut machine = Machine::new(data, filter.clone(), item);

    let first = machine
        .step(&policy)
        .expect("the policy is already done with this item");
    assert_eq!(
        first.chances,
        &[(Stage::Desired, Fraction::ONE)],
        "the item holds the goal, so all of it is absorbed at once"
    );
    assert!(first.state.is_empty(), "a desired item is not live");

    let second = machine.step(&policy).expect("nothing is left to move");
    assert!(second.state.is_empty());
    assert!(
        second.chances.is_empty(),
        "the absorbed mass is not recorded a second time"
    );
    assert_eq!(machine.history().len(), 1, "only one step did anything");
}

/// A currency can produce nothing at all. The old `step` looped over an empty outcome list, so the
/// loop body never ran and the item's mass was recorded nowhere: the step "succeeded" while
/// silently shrinking the distribution. This is the real item that reproduced it — an alteration
/// clears the paste's modifiers, and the natural pool it then draws from is empty, so the step's
/// set has no outcome and its total is zero. The error must name the failure instead.
#[test]
fn an_empty_pool_is_an_error_not_a_vanished_item() {
    let data = poe1::test_data();
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/text/simplex-amulet.txt"
    ))
    .expect("the fixture");
    let parsed = poe1::text::parse(data, &text).expect("parse");
    let mut item = Item::from_spec(data, &parsed.spec).expect("the item");
    // The paste is rare; the step that hits the hole is an alteration, which needs a magic item.
    item.rarity = Rarity::Magic;

    struct AlterOnly;

    impl Policy for AlterOnly {
        fn step(&self, _data: &Data, _item: &Item) -> Step {
            Step {
                action: Action::Apply(CurrencyId::ALTERATION),
                reason: "reroll the item",
            }
        }
    }

    let filter = filter(&pattern(data));
    let mut machine = Machine::new(data, filter, item);
    let before = machine.state().to_vec();
    let err = machine
        .step(&AlterOnly)
        .expect_err("an empty pool is an error, not a vanished item");
    // The currency layer refuses the empty list itself: `ItemOutputSet::new` gives a step that
    // produced nothing the reason "the step produced no outcome", so the machine reports the
    // currency's own refusal rather than its hand-built-set guard (`MachineError::Empty`).
    assert_eq!(
        err,
        MachineError::Refused("the step produced no outcome"),
        "the empty draw is refused with the currency layer's own reason"
    );
    assert_eq!(
        machine.state(),
        before.as_slice(),
        "a failed step leaves the distribution untouched"
    );
    assert!(
        machine.history().is_empty(),
        "a failed step is not a depth in the history"
    );
}
