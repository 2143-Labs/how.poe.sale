//! The one-step reverse solver: what each step does to a pattern, and how the numbers are read.

mod common;

use common::{item, pattern};
use poe1_v2::apply::Params;
use poe1_v2::pattern::{self, Lifted, LiftedLeaf};
use poe1_v2::solve::{self, Model};
use poe1_v2::test_data;

fn step<'a>(report: &'a solve::Report, method: &str) -> &'a solve::Assessment {
    report
        .exact
        .iter()
        .chain(&report.certain)
        .chain(&report.blind)
        .find(|a| a.method == method)
        .unwrap_or_else(|| {
            let unmet = report
                .unmet
                .iter()
                .find(|(id, _)| id == method)
                .map(|(_, r)| r.as_str())
                .unwrap_or("not in the report");
            panic!("{method}: {unmet}")
        })
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// A bench craft fills the pattern's crafted prefix slot for certain: it is the best exact step
/// on a Crown with three open prefixes.
#[test]
fn bench_craft_is_the_best_exact_step() {
    let data = test_data();
    let crown = item("crown-a");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    assert_eq!(report.now.matches, 0.0);
    assert!(close(report.now.progress, 0.5), "{}", report.now.progress);
    let best = report.exact.first().expect("an exact step");
    assert!(close(best.weighed.finish, 0.0));
    // The crafted slot is worth 1 of the prefixes' 6 and all three suffixes are good (6 of 12).
    assert!(
        close(best.weighed.goal, 7.0 / 12.0),
        "{}",
        best.weighed.goal
    );
    assert!(close(best.weighed.better, 1.0));
    assert!(close(best.weighed.worse, 0.0));
    assert!(close(best.weighed.blocked, 0.0));
    assert_eq!(best.weighed.model, Model::Exact);
    // Every crafted prefix does exactly as much for this pattern, so the step that wins is one of
    // them and the ranking's only say between them is the id.
    let tied: Vec<&str> = report
        .exact
        .iter()
        .take_while(|a| a.tied(best))
        .map(|a| a.method.as_str())
        .collect();
    assert!(tied.iter().any(|m| m.starts_with("bench/add/")), "{tied:?}");
    assert!(tied.iter().all(|m| *m == best.method
        || *m == "beast/suffixes-cannot-be-changed"
        || m.starts_with("bench/add/")));
    assert!(tied.len() > 1, "the crafted prefixes tie: {tied:?}");
}

/// An Exalted Orb on that Crown lands a slot the pattern wants with the weight of the classes it
/// accepts, out of the whole prefix pool: 4400 of 55375.
#[test]
fn exalt_better_chance_is_the_accepted_weight() {
    let data = test_data();
    let crown = item("crown-a");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    let exalt = step(&report, "currency/exalted");
    assert_eq!(exalt.weighed.model, Model::Exact);
    assert!(
        close(exalt.weighed.better, 4400.0 / 55375.0),
        "{}",
        exalt.weighed.better
    );
    assert!(close(exalt.weighed.finish, 0.0));
    // Every other prefix fills no slot, so it leaves the item where it was.
    assert!(close(
        exalt.weighed.better + exalt.weighed.worse,
        4400.0 / 55375.0
    ));
}

/// An annulment on the finished-looking Crown loses ground five times in six; the one branch that
/// removes the bad prefix leaves the item where it was but frees a prefix slot.
#[test]
fn annulment_trades_progress_for_room() {
    let data = test_data();
    let crown = item("crown-actual");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    assert!(
        close(report.now.progress, 10.0 / 12.0),
        "{}",
        report.now.progress
    );
    assert!(close(report.now.blocked, 1.0));
    let annul = step(&report, "currency/annulment");
    // Six explicit modifiers, one branch each: the veiled prefix and every other modifier the
    // pattern counts are lost in five of them.
    assert!(
        close(annul.weighed.worse, 5.0 / 6.0),
        "{}",
        annul.weighed.worse
    );
    assert!(close(annul.weighed.better, 0.0));
    // Removing the bad prefix is the one way out, and removing a suffix frees nothing on the
    // prefix side, so half the branches leave a full side with the crafted slot still missing.
    assert!(
        close(annul.weighed.blocked, 0.5),
        "{}",
        annul.weighed.blocked
    );
}

/// Corruption fails the pattern's filter, so every Vaal branch loses the goal however much of the
/// item it keeps.
#[test]
fn vaal_loses_the_goal() {
    let data = test_data();
    let crown = item("crown-a");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    let vaal = step(&report, "currency/vaal");
    assert_eq!(vaal.weighed.model, Model::CertainPart);
    assert!(close(vaal.weighed.goal, 0.0));
    assert!(close(vaal.weighed.finish, 0.0));
    // Three of the four outcomes are computed and each loses the goal; the fourth is the socket
    // table, which the model does not compute.
    assert!(close(vaal.weighed.worse, 0.75), "{}", vaal.weighed.worse);
    assert!(
        close(vaal.weighed.uncomputed, 0.25),
        "{}",
        vaal.weighed.uncomputed
    );
}

/// A step whose random part is not modelled is ranked from its certain part and carries the share
/// the model does not compute.
#[test]
fn reforge_is_read_from_its_certain_part() {
    let data = test_data();
    let crown = item("crown-a");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    let chaos = step(&report, "currency/chaos");
    assert_eq!(chaos.weighed.model, Model::CertainPart);
    // The certain part of a Chaos Orb is a rare item with no modifiers.
    assert!(close(chaos.weighed.goal, 0.0));
    assert!(close(chaos.weighed.worse, 1.0));
    assert!(close(chaos.weighed.uncomputed, 0.0));
}

/// A socket outcome is not modelled at all, so the step is listed and not ranked.
#[test]
fn socket_step_is_listed_not_ranked() {
    let data = test_data();
    let crown = item("crown-actual");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    assert!(
        report
            .exact
            .iter()
            .chain(&report.certain)
            .all(|a| a.method != "currency/chromatic-orb")
    );
    let chrom = step(&report, "currency/chromatic-orb");
    assert_eq!(chrom.weighed.model, Model::Blind);
}

/// Methods that do not apply are reported with the reason.
#[test]
fn unmet_methods_carry_their_reason() {
    let data = test_data();
    let jewel = item("cobalt-jewel-normal");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &jewel);
    let reason = report
        .unmet
        .iter()
        .find(|(id, _)| id == "currency/exalted")
        .map(|(_, r)| r.as_str());
    assert_eq!(reason, Some("needs a rare item"));
}

/// A choice is read by its best option, so the numbers of an unveil come from one option, not a
/// mixture.
#[test]
fn choice_is_read_by_its_best_option() {
    let data = test_data();
    let crown = item("crown-c2");
    let shaper = pattern("crown-shaper");
    let best = pattern::evaluate(data, &shaper, &item("crown-ideal"))
        .into_iter()
        .find(|(_, s)| s.matches)
        .expect("the ideal Crown matches");
    let poor = pattern::evaluate(data, &shaper, &item("crown-a"))
        .into_iter()
        .next()
        .expect("a state");
    let lifted = Lifted {
        leaves: vec![(
            1.0,
            LiftedLeaf::Choice(vec![
                Lifted {
                    leaves: vec![(1.0, LiftedLeaf::State(poor.1))],
                },
                Lifted {
                    leaves: vec![(1.0, LiftedLeaf::State(best.1))],
                },
            ]),
        )],
    };
    let now = solve::Now::of(data, &shaper, &crown);
    let weighed = solve::weigh(&shaper, &lifted, &now);
    assert!(close(weighed.finish, 1.0), "{}", weighed.finish);
    assert!(close(weighed.goal, 1.0));
    assert!(close(weighed.better, 1.0));
    assert!(close(weighed.worse, 0.0));
}

/// Two runs give the same order.
#[test]
fn report_is_reproducible() {
    let data = test_data();
    let crown = item("crown-actual");
    let shaper = pattern("crown-shaper");
    let a = solve::next_steps(data, &shaper, &crown);
    let b = solve::next_steps(data, &shaper, &crown);
    assert_eq!(a, b);
    let order: Vec<&str> = a.exact.iter().map(|s| s.method.as_str()).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order.len(), sorted.len());
    assert!(!order.is_empty());
}

/// The bench craft is reproducible on a second item, so the ranking is not an artefact of one
/// fixture: a Crown with one open prefix cannot take a crafted one, and its best exact step is
/// something else.
#[test]
fn a_full_item_has_no_bench_step() {
    let data = test_data();
    let crown = item("crown-l2");
    let shaper = pattern("crown-shaper");
    let report = solve::next_steps(data, &shaper, &crown);
    assert!(
        !report
            .exact
            .iter()
            .chain(&report.certain)
            .chain(&report.blind)
            .any(|a| a.method.starts_with("bench/add/")),
        "a full prefix side takes no bench craft"
    );
}

/// Params are carried into the assessment, so a caller can see what it asked for.
#[test]
fn assessment_keeps_its_params() {
    let data = test_data();
    let crown = item("crown-a");
    let shaper = pattern("crown-shaper");
    let method = data.method("currency/exalted").expect("the Exalted Orb");
    let a = solve::assess(data, &shaper, &crown, method, &Params::default()).expect("applies");
    assert_eq!(a.params, Params::default());
    assert_eq!(a.cost.len(), 1);
    assert_eq!(a.name, "Exalted Orb");
}
