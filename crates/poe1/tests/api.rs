//! The browser's views: the start view, a base's modifiers, the priced craft loop and one
//! currency's outcomes, as the page asks for them.
//!
//! The loop request is built the way the page builds it: the goal selectors are the totem-shield
//! pattern's classes, converted from the one filter both the CLI and the page derive
//! (`ItemSetFilter::from_pattern`), so the depth table here is the table `loop-report` prints.

mod common;

use poe1::api;
use poe1::currency::{Accept, Fraction, ItemSetFilter, ModSelector};
use poe1::data::{Data, Side};
use poe1::item::Influence;
use poe1::test_data;
use serde_json::{Value, json};

/// The loop's base: a white Supreme Spiked Shield, the item `loop-report` starts from.
fn shield() -> Value {
    json!({
        "base": "Supreme Spiked Shield",
        "itemLevel": 87,
        "rarity": "normal",
        "influences": ["shaper"],
    })
}

/// The side as the request spells it.
fn side(side: Side) -> &'static str {
    match side {
        Side::Prefix => "prefix",
        Side::Suffix => "suffix",
    }
}

/// One filter selector as a `SelectorSpec`: the same modifier id, side and family flag.
fn selector(data: &Data, selector: &ModSelector) -> Value {
    match selector {
        ModSelector::Mod {
            side: at,
            m,
            or_better,
            ..
        } => json!({
            "side": side(*at),
            "id": data.mod_at(*m).id,
            "orBetter": or_better,
        }),
        // The totem-shield fixture names no `{"crafted": ...}` class; one would name no id and
        // could not be sent, which is exactly why the request speaks in ids.
        ModSelector::Crafted { .. } => panic!("the totem-shield goal names no crafted class"),
    }
}

/// The totem-shield goal, converted from the filter `ItemSetFilter::from_pattern` derives.
fn goal(data: &Data) -> Value {
    let filter = ItemSetFilter::from_pattern(
        &common::pattern("totem-shield"),
        Accept::DesiredOrGoodBothSides,
    );
    let selectors =
        |list: &[ModSelector]| -> Vec<Value> { list.iter().map(|s| selector(data, s)).collect() };
    json!({
        "desired": selectors(&filter.desired),
        "good": selectors(&filter.good),
        "accept": "desired-or-good-both-sides",
    })
}

/// A whole loop request: the white shield, the totem goal, the start view's price defaults and the
/// depth the report asks for.
fn loop_request(data: &Data, depth: u32, price: bool) -> String {
    json!({
        "item": shield(),
        "goal": goal(data),
        "prices": {
            "prices": api::start(data).price_defaults,
            "tradePenalty": 0.02,
        },
        "depth": depth,
        "price": price,
    })
    .to_string()
}

/// The depth-1 row's mass at `stage`, or a failure naming the stages the row does carry.
fn stage_mass(view: &api::LoopView, depth: usize, stage: &str) -> api::Mass {
    view.depths[depth]
        .stages
        .iter()
        .find(|row| row.stage == stage)
        .unwrap_or_else(|| {
            let seen: Vec<&str> = view.depths[depth]
                .stages
                .iter()
                .map(|row| row.stage.as_str())
                .collect();
            panic!("depth {depth} has no {stage:?} row; it has {seen:?}")
        })
        .mass
        .clone()
}

/// The pinned number: a transmute's chance to roll the totem prefix first is 1 in 108, and the
/// page must read it off the same exact fraction the CLI prints.
#[test]
fn the_loop_reports_the_transmute_chance_at_depth_one() {
    let data = test_data();
    let view = api::loop_report(data, &loop_request(data, 3, true)).expect("a loop report");
    assert_eq!(view.depths.len(), 3, "{:?}", view.depths);
    let desired = stage_mass(&view, 0, "desired");
    assert_eq!(desired.exact, "1/108");
    assert_eq!(desired.percent, "0.925926");
    assert_eq!(view.depths[0].depth, 1);
    assert!(view.depths[0].classes > 0);
    assert_eq!(view.start.stage, "spare", "{}", view.start.text);
    assert!(view.stopped.is_none(), "{:?}", view.stopped);
    // The shield's closure is far past the node budget, so the cut is the one warning allowed.
    assert!(
        view.warnings
            .iter()
            .all(|warning| warning.contains("was cut at")),
        "{:?}",
        view.warnings
    );
}

/// The priced graph is the slow half of the view, and it answers: a positive expected cost over a
/// non-empty closure, with the goal itself never the plan.
#[test]
fn the_priced_loop_has_a_cost_and_a_closure() {
    let data = test_data();
    let view = api::loop_report(data, &loop_request(data, 3, true)).expect("a loop report");
    let priced = view.priced.expect("the priced graph");
    assert!(priced.expected_chaos > 0.0, "{priced:?}");
    assert!(priced.sd_chaos >= 0.0, "{priced:?}");
    assert!(priced.closure > 0, "{priced:?}");
    assert!(
        !priced.spend.is_empty(),
        "the plan spends no currency: {priced:?}"
    );
    let counts: Vec<f64> = priced.spend.iter().map(|spend| spend.count).collect();
    assert!(
        counts.windows(2).all(|pair| pair[0] >= pair[1]),
        "spend is not largest first: {counts:?}"
    );
}

/// An empty goal names no modifier, so no craft can ever meet it: the only plan the solver has is
/// to buy the finished item, and the page shows the absence of a price rather than that number.
#[test]
fn an_empty_goal_has_no_craft_price() {
    let data = test_data();
    let request = json!({
        "item": shield(),
        "goal": { "desired": [], "good": [], "accept": "any" },
        "prices": {
            "prices": api::start(data).price_defaults,
            "tradePenalty": 0.02,
        },
        "depth": 3,
        "price": true,
    })
    .to_string();
    let view = api::loop_report(data, &request).expect("an empty goal is legal");
    assert!(view.priced.is_none(), "{:?}", view.priced);
    assert!(
        view.warnings.iter().any(|warning| warning
            == "the goal cannot be reached by crafting from this base, so the loop has no price"),
        "{:?}",
        view.warnings
    );
    assert!(!view.depths.is_empty());
}

/// Answering without the price is a depth table and nothing else: the button that asks for a
/// figure is the only caller that pays for the graph.
#[test]
fn an_unpriced_report_has_no_priced_block() {
    let data = test_data();
    let view = api::loop_report(data, &loop_request(data, 3, false)).expect("a loop report");
    assert!(view.priced.is_none());
    assert_eq!(view.depths.len(), 3);
}

/// The page's maximum depth answers rather than erroring: the shield's own loop stops early (an
/// exact distribution the lumped state cannot hold past depth 4), and the view keeps the depths
/// that fit and still serialises.
#[test]
fn a_depth_twenty_request_answers_with_the_depths_that_fit() {
    let data = test_data();
    let view = api::loop_report(data, &loop_request(data, 20, false)).expect("depth 20 answers");
    assert!(!view.depths.is_empty(), "no depth survived");
    assert!(view.depths.len() <= 20);
    assert_eq!(view.totals.len(), 3, "desired, reset and stop");
    if let Some(stopped) = &view.stopped {
        assert!(stopped.contains("did not fit"), "{stopped}");
    }
    // The view has to cross the wire even when the loop stopped: every field is a JSON value.
    let json = serde_json::to_value(&view).expect("the view serialises");
    assert!(
        json["depths"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(json["live"]["exact"].is_string());
    assert!(
        json["totals"]
            .as_array()
            .is_some_and(|rows| rows.len() == 3)
    );
}

/// The depth is what the caller asked for, and a request for one depth is one row.
#[test]
fn a_one_depth_request_reports_one_depth() {
    let data = test_data();
    let view = api::loop_report(data, &loop_request(data, 1, false)).expect("a loop report");
    assert_eq!(view.depths.len(), 1);
    assert_eq!(view.depths[0].depth, 1);
}

/// The exact fraction a [`api::Mass`] carries back.
fn fraction(mass: &api::Mass) -> Fraction {
    let (num, den) = mass.exact.split_once('/').expect("num/den");
    Fraction::new(
        num.parse().expect("a numerator"),
        den.parse().expect("a denominator"),
    )
    .expect("a fraction")
}

/// A currency on a white base: every outcome carries its exact chance, and the shown rows plus the
/// hidden rest add up to the whole step — exactly, not just to a rounded percentage.
#[test]
fn applying_transmutation_lists_exact_outcomes() {
    let data = test_data();
    let request = json!({
        "item": shield(),
        "currency": "currency/transmutation",
    })
    .to_string();
    let view = api::apply_currency(data, &request).expect("a transmute");
    assert_eq!(view.currency.id, "currency/transmutation");
    assert_eq!(view.currency.name, "Orb of Transmutation");
    assert_ne!(view.total, "0");
    assert!(!view.outcomes.is_empty());
    // A transmute on this shield has more outcomes than the table shows, so the rest is real mass.
    assert_eq!(view.outcomes.len(), 200);
    assert!(view.more > 0, "the row limit is not reached");
    assert_ne!(view.rest.exact, "0/1");
    let sum = view
        .outcomes
        .iter()
        .map(|outcome| fraction(&outcome.chance))
        .chain([fraction(&view.rest)])
        .try_fold(Fraction::ZERO, |acc, chance| acc.checked_add(chance))
        .expect("the chances add up exactly");
    assert!(sum.is_one(), "shown + rest = {sum}, not 1");
    let percent: f64 = view
        .outcomes
        .iter()
        .map(|outcome| outcome.chance.percent.parse::<f64>().expect("a percentage"))
        .chain([view.rest.percent.parse().expect("a percentage")])
        .sum();
    assert!(
        (percent - 100.0).abs() < 1e-4,
        "the chances sum to {percent}"
    );
    assert!(
        view.outcomes.iter().any(|outcome| outcome.added.is_some()),
        "a transmute adds nothing"
    );
    assert!(view.outcomes[0].text.contains("Spiked Shield"));
}

/// An id no currency is registered under is refused by name, not silently empty.
#[test]
fn an_unknown_currency_is_named() {
    let data = test_data();
    let request = json!({ "item": shield(), "currency": "currency/nope" }).to_string();
    let err = api::apply_currency(data, &request).expect_err("no such currency");
    assert_eq!(err, "unknown currency \"currency/nope\"");
}

/// The start view: the modelled currencies with their costs, the price defaults, and the bases and
/// stages the page builds its controls from.
#[test]
fn the_start_view_names_the_currencies_and_their_costs() {
    let data = test_data();
    let view = api::start(data);
    assert_eq!(view.version, "0.1.0");
    assert!(!view.patch.is_empty());
    assert_eq!(view.currencies.len(), 7);
    assert_eq!(view.price_defaults.len(), 7);
    assert_eq!(
        view.stages,
        [
            "desired",
            "good pair",
            "good prefix",
            "good suffix",
            "other",
            "spare",
            "spent rare",
            "reset",
            "stop"
        ]
    );
    assert_eq!(
        view.accepts,
        ["has-desired", "desired-or-good-both-sides", "any"]
    );
    let transmutation = view
        .currencies
        .iter()
        .find(|currency| currency.id == "currency/transmutation")
        .expect("the transmute");
    assert!(!transmutation.costs.is_empty());
    for cost in &transmutation.costs {
        assert_eq!(view.price_defaults.get(&cost.key), Some(&0.021));
        assert!(!cost.name.is_empty());
    }
    assert!(
        view.bases.contains(&"Supreme Spiked Shield".to_string()),
        "the base select would not offer the shield"
    );
    assert!(view.influences.contains(&"shaper".to_string()));
}

/// The modifier list a base fills the goal picker with: every row names a modifier id the request
/// could send back.
#[test]
fn the_modifier_list_names_ids_the_goal_can_use() {
    let data = test_data();
    let options = api::mods(data, "Supreme Spiked Shield", 87, &[Influence::Shaper])
        .expect("the shield's modifiers");
    assert!(!options.is_empty());
    let totem = options
        .iter()
        .find(|option| option.id == "MaximumTotemUber1")
        .expect("the totem prefix is offered");
    assert_eq!(totem.side, "prefix");
    assert_eq!(totem.influence.as_deref(), Some("Shaper"));
    assert!(options.iter().all(|option| !option.text.is_empty()));
    assert!(
        options
            .iter()
            .all(|option| data.mod_by_id(&option.id).is_some())
    );
}
