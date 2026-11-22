//! The lumped class graph as a `solver::Domain`: the reachable closure is measured, every priced
//! action's chance distribution is conserved both exactly and in `f64`, and a missing price or an
//! unpriced action is an error rather than a zero.

use std::collections::BTreeMap;

use poe1::craft::CurrencyId;
use poe1::craft::priced::{ACTIONS, GOAL_KEY, Move, Next, Node, Priced, PricedError, Prices};
use poe1::currency::{Accept, Fraction, ItemSetFilter};
use poe1::data::Data;
use poe1::item::{Item, ItemSpec};
use poe1::pattern::{Pattern, PatternSpec};
use solver::{Domain, SolveError};

/// The currency metadata ids `apply::cost` resolves the loop's methods to. These are the keys a
/// `prices.json` must hold, not the method ids.
const TRANSMUTATION: &str = "Metadata/Items/Currency/CurrencyUpgradeToMagic";
const ALTERATION: &str = "Metadata/Items/Currency/CurrencyRerollMagic";
const AUGMENTATION: &str = "Metadata/Items/Currency/CurrencyAddModToMagic";
const REGAL: &str = "Metadata/Items/Currency/CurrencyUpgradeMagicToRare";
const EXALTED: &str = "Metadata/Items/Currency/CurrencyAddModToRare";
const SCOURING: &str = "Metadata/Items/Currency/CurrencyConvertToNormal";

/// A Shaper Archdemon Crown, item level 86, normal to begin with: the base the goal pattern names.
fn crown(data: &Data) -> Item {
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Archdemon Crown",
            "itemLevel": 86,
            "rarity": "normal",
            "influences": ["shaper"]
        }"#,
    )
    .expect("spec");
    Item::from_spec(data, &spec).expect("the crown")
}

/// One of the checked-in item fixtures, as the solver would be handed it.
fn fixture(data: &Data, name: &str) -> Item {
    let path = format!(
        "{}/../../fixtures/items/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let spec: ItemSpec = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path)),
    )
    .expect("item spec");
    Item::from_spec(data, &spec).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn pattern(data: &Data) -> Pattern {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/patterns/crown-shaper.json"
    );
    let spec: PatternSpec =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the pattern fixture"))
            .expect("pattern spec");
    Pattern::compile(data, &spec).expect("compiles")
}

fn filter(pattern: &Pattern) -> ItemSetFilter {
    ItemSetFilter::from_pattern(pattern, Accept::DesiredOrGoodBothSides)
}

/// A synthetic but plausible price list, so the numbers are about the graph and not a market: the
/// six currencies the loop uses, the base a few chaos, the finished crown worth far more than one
/// attempt.
fn prices(data: &Data, start: &Item, filter: &ItemSetFilter) -> Prices {
    let mut line_items = BTreeMap::new();
    for (key, chaos) in [
        (TRANSMUTATION, 0.1),
        (ALTERATION, 0.07),
        (AUGMENTATION, 0.5),
        (REGAL, 1.5),
        (EXALTED, 12.0),
        (SCOURING, 0.4),
    ] {
        line_items.insert(key.to_owned(), chaos);
    }
    Prices {
        prices: line_items,
        buy: BTreeMap::from([
            (GOAL_KEY.to_owned(), 40.0),
            (Priced::start_key(data, filter, start), 0.05),
        ]),
        sell: BTreeMap::new(),
        trade_penalty: 0.02,
    }
}

/// The exact sum of a list of chances.
fn sum<'a>(chances: impl IntoIterator<Item = &'a Fraction>) -> Fraction {
    chances
        .into_iter()
        .fold(Fraction::ZERO, |acc, &p| acc.checked_add(p).expect("exact"))
}

fn build(data: &Data) -> Priced {
    let filter = filter(&pattern(data));
    Priced::build(data, filter, &crown(data), Prices::default()).expect("the graph builds")
}

/// The reachable class closure is a measured number, not an assumption: it is printed here and
/// asserted against the budget the graph was built with.
///
/// It is also the finding: the closure of this craft is not one depth's worth of classes. Expansion
/// reaches 314 classes at one lumped step; the budget keeps the states nearest the start, and the
/// classes it cut are counted.
#[test]
fn closure_size_is_measured() {
    let data = poe1::test_data();
    let priced = build(data);

    let closure = priced.closure_size();
    println!(
        "reachable class closure: {closure} class nodes expanded, {} outcome classes folded into \
         the frontier",
        priced.truncated()
    );
    assert!(closure > 0, "the start class is in its own closure");
    assert!(
        closure <= poe1::craft::priced::NODE_BUDGET,
        "closure {closure} exceeds the budget {}",
        poe1::craft::priced::NODE_BUDGET
    );
    assert_eq!(
        Node::Class(priced.start().clone()).key(),
        Node::Class(priced.closure()[0].clone()).key(),
        "the start class is the first node"
    );
    assert!(
        priced.truncated() > 0,
        "the reachable closure is far larger than the budget, so the cut must have taken something"
    );
}

/// Conservation: every action's outcomes sum to one, exactly and within 1e-9 in `f64`.
///
/// The exact sum is the real check — a lumped step that lost or invented mass would fail it — and
/// the `f64` sum is the one the solver actually solves with.
#[test]
fn every_actions_outcomes_are_conserved() {
    let data = poe1::test_data();
    let priced = build(data);

    let mut checked = 0usize;
    let mut transitions = 0usize;
    for (_class, action, outcomes) in priced.steps() {
        let exact = sum(outcomes.iter().map(|t| &t.chance));
        assert!(exact.is_one(), "{action:?} outcomes sum to {exact}, not 1");
        let float: f64 = outcomes.iter().map(|t| t.probability).sum();
        assert!(
            (float - 1.0).abs() < 1e-9,
            "{action:?} outcomes sum to {float} in f64, not 1"
        );
        // The solver's number must be the exact one, rounded once.
        for t in outcomes {
            let from_exact = t.chance.num() as f64 / t.chance.den() as f64;
            assert!((t.probability - from_exact).abs() < 1e-15);
        }
        checked += 1;
        transitions += outcomes.len();
    }
    println!("checked {checked} (class, action) outcome lists, {transitions} transitions");
    assert!(checked > 0, "the closure has at least one action");
    assert_eq!(
        checked,
        priced.steps().count(),
        "every expanded action was checked"
    );

    // The same check through the trait, which is what the graph actually consumes: `actions` must
    // name exactly the actions whose outcomes sum to one.
    let mut served = 0usize;
    for class in priced.closure() {
        let state = Node::Class(class.clone());
        for action in Domain::actions(&priced, &state) {
            let outcomes = Domain::outcomes(&priced, &state, &action);
            let total: f64 = outcomes.iter().map(|(_, p)| p).sum();
            assert!(
                (total - 1.0).abs() < 1e-9,
                "Domain::outcomes sums to {total} for {action:?}"
            );
            served += 1;
        }
    }
    assert_eq!(served, checked, "the trait serves every row in the table");
}

/// A priced graph solves: the expected chaos is finite and positive, and every craft the policy
/// names is a registered currency with a declared cost.
#[test]
fn a_priced_graph_solves() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let start = crown(data);
    let prices = prices(data, &start, &filter);
    let priced = Priced::build(data, filter, &start, prices).expect("the graph builds");

    let report = priced.report().expect("the graph solves");
    println!(
        "closure {} ({} cut), expected chaos {:.4} (sd {:.4}), {} rounds, policy over {} nodes",
        report.closure,
        report.truncated,
        report.expected_chaos,
        report.sd_chaos,
        report.iterations,
        report.policy.len()
    );
    println!("expected spend: {:?}", report.expected_spend);

    assert!(
        report.expected_chaos.is_finite(),
        "expected chaos is finite"
    );
    assert!(report.expected_chaos > 0.0, "expected chaos is positive");
    assert_eq!(report.closure, priced.closure_size());
    assert_eq!(report.truncated, priced.truncated());

    let start_key = Node::Class(priced.start().clone()).key();
    assert_eq!(
        report.policy.get(&start_key),
        Some(&Move::Craft(CurrencyId::TRANSMUTATION)),
        "a normal crown is started with a transmutation"
    );

    let mut crafts = 0usize;
    for (node, step) in &report.policy {
        let Move::Craft(id) = step else { continue };
        crafts += 1;
        assert!(
            ACTIONS.contains(id),
            "{node} is priced to apply {}, which is not a lumpable registered currency",
            id.0
        );
        let method = data.method(id.0).expect("a registered method");
        assert!(
            !poe1::apply::cost(data, method, &poe1::apply::Params::default())
                .expect("resolvable")
                .is_empty(),
            "{} must declare a cost",
            id.0
        );
    }
    println!(
        "{crafts} of {} priced nodes take a craft",
        report.policy.len()
    );
    assert!(crafts > 1, "the policy crafts from more than the start");
    assert!(
        report.expected_spend.contains_key(TRANSMUTATION),
        "the expected spend names the currency the start applies"
    );
}

/// A missing price is the solver's error, not a silent zero.
#[test]
fn a_missing_price_is_an_error() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let start = crown(data);
    let mut prices = prices(data, &start, &filter);
    prices.prices.remove(ALTERATION);

    match Priced::build(data, filter, &start, prices)
        .expect("the closure does not need prices")
        .report()
    {
        Err(PricedError::Solve(SolveError::MissingPrice(key))) => {
            assert_eq!(key, ALTERATION, "the missing key is named");
        }
        other => panic!("expected a missing-price error, got {other:?}"),
    }
}

/// An action whose method declares no cost is rejected when the graph is built, naming the action.
#[test]
fn an_action_without_a_cost_is_rejected() {
    let data = poe1::test_data();
    let mut bundle = data.bundle.clone();
    bundle
        .methods
        .iter_mut()
        .find(|m| m.id == CurrencyId::ALTERATION.0)
        .expect("the alteration method")
        .cost
        .clear();
    let unpriced = Data::from_bundle(bundle).expect("a bundle with an empty cost still loads");

    let filter = filter(&pattern(&unpriced));
    let start = crown(&unpriced);
    match Priced::build(&unpriced, filter, &start, Prices::default()) {
        Err(PricedError::NoCost(id)) => assert_eq!(id, CurrencyId::ALTERATION.0),
        Err(other) => panic!("expected the costless action to be rejected, got {other}"),
        Ok(_) => panic!("a currency with no cost must not be priced as free"),
    }
}

/// A report that cannot begin is a named error, not a silently unfunded graph.
#[test]
fn an_unfunded_graph_is_an_error() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let start = crown(data);
    let start_key = Priced::start_key(data, &filter, &start);
    let mut prices = prices(data, &start, &filter);
    prices.buy.remove(&start_key);

    let priced = Priced::build(data, filter, &start, prices).expect("the graph builds");
    match priced.report() {
        Err(PricedError::Unfunded(key)) => assert_eq!(key, start_key),
        other => panic!("expected an unfunded error, got {other:?}"),
    }
}

/// A start that already holds the goal is a named error, not a graph that keeps crafting it.
#[test]
fn a_finished_start_is_rejected() {
    let data = poe1::test_data();
    let filter = filter(&pattern(data));
    let start = fixture(data, "crown-ideal");
    match Priced::build(data, filter, &start, Prices::default()) {
        Err(PricedError::AlreadyDone) => {}
        Err(other) => panic!("expected the finished start to be refused, got {other}"),
        Ok(_) => panic!("a start that holds a desired modifier has nothing to price"),
    }
}

/// A `prices.json` round-trips, so the documented shape is the shape the type reads.
#[test]
fn prices_read_the_documented_shape() {
    let text = r#"{
        "prices": {
            "Metadata/Items/Currency/CurrencyRerollMagic": 0.07
        },
        "buy": { "goal": 40.0 },
        "sell": {},
        "tradePenalty": 0.02
    }"#;
    let prices: Prices = serde_json::from_str(text).expect("the documented shape");
    assert_eq!(
        prices.prices.get(ALTERATION).copied(),
        Some(0.07),
        "a price is keyed by currency metadata id"
    );
    assert_eq!(prices.buy.get(GOAL_KEY).copied(), Some(40.0));
    assert_eq!(prices.trade_penalty, 0.02);
}

/// `Next` must name every outcome the class graph has: the goal, a closure index, or the frontier.
#[test]
fn every_transition_lands_in_the_closure() {
    let data = poe1::test_data();
    let priced = build(data);
    for (_, action, outcomes) in priced.steps() {
        for t in outcomes {
            match t.to {
                Next::Done | Next::Frontier => {}
                Next::Class(i) => assert!(
                    i < priced.closure_size(),
                    "{action:?} lands outside the closure at {i}"
                ),
            }
        }
    }
}
