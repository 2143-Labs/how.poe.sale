//! The one-step reverse solver: what each step does to a pattern, and how the numbers are read.

mod common;

use common::{item, pattern};
use poe1::apply::{self, Leaf, Params};
use poe1::pattern::{self, Lifted, LiftedLeaf, PatternState};
use poe1::solve::{self, Model};
use poe1::test_data;

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
    assert!(
        close(report.now.progress, 3.0 / 8.0),
        "{}",
        report.now.progress
    );
    let best = report.exact.first().expect("an exact step");
    assert!(close(best.weighed.finish, 0.0));
    // The crafted slot is worth 1 of the prefixes' 4 and each suffix is worth 1 (4 of 7).
    assert!(close(best.weighed.goal, 4.0 / 8.0), "{}", best.weighed.goal);
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
        close(report.now.progress, 6.0 / 8.0),
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

/// The score is the share of the goal's slot value the item already holds: of `crown-shaper`'s
/// eight eighths (2 + 1 + 1 on the prefix side, 2 + 1 + 1 on the suffix side), read off each
/// fixture's modifiers, not off the catalog. A metamod is a tool, not the modifier `any-crafted-
/// prefix` is looking for, so the slot it sits in earns nothing: a Crown whose only crafted
/// prefix is Suffixes Cannot Be Changed stays at its natural modifiers' share.
#[test]
fn progress_is_the_grade_weighted_share() {
    let data = test_data();
    let shaper = pattern("crown-shaper");
    // A crafted modifier fills `any-crafted-prefix` (1) unless it is a metamod; a Veiled
    // placeholder fills `desired-veiled` (2) while it stands and is worth nothing once unveiled
    // into a modifier the pattern does not list; a natural modifier the pattern does not list
    // fills no slot.
    let expected = [
        ("crown-a", 3.0 / 8.0),          // three good suffixes
        ("crown-a-mana", 4.0 / 8.0),     // crafted mana + three good suffixes
        ("crown-actual", 6.0 / 8.0),     // veiled AoE (2) + its natural tier (1) + suffixes
        ("crown-c1", 4.0 / 8.0),         // T1 life (1) + three suffixes
        ("crown-c2", 3.0 / 8.0),         // Suffixes Cannot Be Changed is a tool: nothing + suffixes
        ("crown-c2-blocked", 4.0 / 8.0), // a real crafted prefix (1) + three suffixes
        ("crown-d2-scbc", 4.0 / 8.0),    // T1 life (1) + a metamod (0) + suffixes
        ("crown-d2-scbc-fractured", 4.0 / 8.0),
        // It matches, so it is finished however much of the goal its modifiers happen to hold.
        ("crown-ideal", 1.0),
        ("crown-l1", 4.0 / 8.0),
        ("crown-l2", 4.0 / 8.0),
    ];
    for (name, want) in expected {
        let got = solve::item_progress(data, &shaper, &item(name));
        assert!((got - want).abs() < 1e-12, "{name}: {got}, expected {want}");
    }
    // A corrupted Crown fails the pattern's filter, so nothing it holds counts.
    assert_eq!(
        solve::item_progress(data, &shaper, &item("crown-a-corrupted")),
        0.0
    );
}

/// Some items are stuck: a side is full while a slot the pattern wants is empty, or the pattern's
/// crafted slot is held by a metamod, which its crafted class does not count.
#[test]
fn blocked_is_a_full_side_with_an_empty_slot() {
    let data = test_data();
    let shaper = pattern("crown-shaper");
    let read = |name: &str| {
        pattern::evaluate(data, &shaper, &item(name))
            .into_iter()
            .find(|(p, s)| *p > 0.0 && s.item_ok)
            .map(|(_, s)| s)
            .unwrap_or_else(|| panic!("{name}: no accepted state"))
    };
    for name in [
        "crown-actual",
        "crown-c2",
        "crown-d2-scbc",
        "crown-d2-scbc-fractured",
        "crown-l1",
        "crown-l2",
    ] {
        assert!(
            solve::blocked(&shaper, &read(name)),
            "{name} should be stuck"
        );
    }
    for name in [
        "crown-a",
        "crown-a-mana",
        "crown-c1",
        "crown-c2-blocked",
        "crown-ideal",
    ] {
        assert!(
            !solve::blocked(&shaper, &read(name)),
            "{name} should be open"
        );
    }
}

/// The headline numbers, recomputed from the pools rather than read back from the engine: the
/// Exalted Orb's accepted weight, the annulment's average loss, and the unveil's chance of a
/// modifier the pattern accepts.
#[test]
fn numbers_match_an_independent_recount() {
    let data = test_data();
    let shaper = pattern("crown-shaper");
    let weigh = |fixture: &str, method: &str| {
        solve::assess(
            data,
            &shaper,
            &item(fixture),
            data.method(method).unwrap_or_else(|| panic!("{method}")),
            &Params::default(),
        )
        .unwrap_or_else(|e| panic!("{fixture} + {method}: {e}"))
        .weighed
    };

    // The Crown's Shaper prefix pool at item level 87 is 67 modifiers weighing 55375 in all.
    // `IncreasedLife8` and `IncreasedLife9` weigh 1000 each and fill `good-prefix` (2), the three
    // `AreaOfEffectSupportedUber` tiers weigh 800 each and fill `ok-prefix` (1); every other
    // prefix fills no slot, so the step leaves the item at 6/12.
    let exalt = weigh("crown-a", "currency/exalted");
    assert!(
        (exalt.better - 4400.0 / 55375.0).abs() < 1e-12,
        "{}",
        exalt.better
    );
    // Both prefix classes are good enough in the same slot, so each hit is worth one eighth.
    let want = 3.0 / 8.0 + (2000.0 + 2400.0) / 55375.0 * (1.0 / 8.0);
    assert!((exalt.goal - want).abs() < 1e-12, "{}", exalt.goal);
    assert_eq!(exalt.finish, 0.0);
    assert_eq!(exalt.worse, 0.0);

    // Six explicit modifiers, one removal each: five of the six lose ground (the armour prefix
    // leaves the item where it was but frees the side). Summing the six results in eighths,
    // (4 + 5 + 6 + 5 + 5 + 5) / 6 / 8 = 5/8; half the removals leave the prefix side full with
    // the crafted slot still missing.
    let annul = weigh("crown-actual", "currency/annulment");
    assert!((annul.worse - 5.0 / 6.0).abs() < 1e-12, "{}", annul.worse);
    assert_eq!(annul.better, 0.0);
    assert!((annul.blocked - 0.5).abs() < 1e-12, "{}", annul.blocked);
    assert!((annul.goal - 5.0 / 8.0).abs() < 1e-12, "{}", annul.goal);

    // The unveil's pool is 9 veiled prefixes weighing 12000; the two the `desired-veiled` class
    // accepts weigh 1500 each. Enumerating every three-set as ordered draws without replacement
    // gives 0.637176497393216 of the draws at least one of them. The player keeps the best
    // option, so that is `better`, and `goal` is 3/8 without the veiled prefix and 5/8 with
    // one: the Suffixes Cannot Be Changed the item holds fills no slot.
    let unveil = weigh("crown-c2", "unveil");
    let p = 0.637_176_497_393_216;
    assert!((unveil.better - p).abs() < 1e-12, "{}", unveil.better);
    let want = 3.0 / 8.0 * (1.0 - p) + 5.0 / 8.0 * p;
    assert!((unveil.goal - want).abs() < 1e-12, "{}", unveil.goal);
}

/// A hand sum of one method's leaves, in the same units `Weighed` reports: mass, goal, and the
/// better/worse split around the item's current progress.
#[derive(Default)]
struct Hand {
    mass: f64,
    uncomputed: f64,
    finish: f64,
    goal: f64,
    better: f64,
    worse: f64,
    blocked: f64,
}

impl Hand {
    fn state(&mut self, pattern: &poe1::pattern::Pattern, p: f64, s: &PatternState, base: f64) {
        let v = solve::state_progress(pattern, s);
        self.mass += p;
        self.goal += p * v;
        if solve::blocked(pattern, s) {
            self.blocked += p;
        }
        if s.matches {
            self.finish += p;
        }
        // `solve`'s own epsilon; a step that leaves the item where it was is neither better nor
        // worse.
        if v > base + 1e-9 {
            self.better += p;
        } else if v < base - 1e-9 {
            self.worse += p;
        }
    }

    fn item(
        &mut self,
        data: &poe1::data::Data,
        pattern: &poe1::pattern::Pattern,
        p: f64,
        it: &poe1::item::Item,
        base: f64,
    ) {
        for (q, s) in pattern::evaluate(data, pattern, it) {
            self.state(pattern, p * q, &s, base);
        }
    }
}

/// The engine's weighing agrees with a sum taken straight from the method's own results: every
/// item result is re-read with `evaluate`, independent of how `lift` merges states.
#[test]
fn weighing_matches_a_brute_force_over_the_outcome() {
    let data = test_data();
    let mut compared = 0;
    for (pname, name) in common::pairs() {
        let pat = pattern(&pname);
        let target = item(&name);
        let now = solve::Now::of(data, &pat, &target);
        for method in data.methods.iter().filter(|m| m.inputs == 1) {
            let outcome = match apply::apply(
                data,
                method,
                std::slice::from_ref(&target),
                &Params::default(),
            ) {
                Ok(outcome) => outcome,
                Err(_) => continue,
            };
            // The choice rule (the player keeps the best option) is pinned by
            // `choice_is_read_by_its_best_option` and by the unveil recount.
            if outcome
                .leaves
                .iter()
                .any(|(_, leaf)| matches!(leaf, Leaf::Choice(_)))
            {
                continue;
            }
            let mut hand = Hand::default();
            for (p, leaf) in &outcome.leaves {
                match leaf {
                    Leaf::Item(it) => hand.item(data, &pat, *p, it, now.progress),
                    // A Mirror keeps both items and the goal is the crafted one.
                    Leaf::Items(items) => match items.first() {
                        Some(first) => hand.item(data, &pat, *p, first, now.progress),
                        None => hand.mass += p,
                    },
                    // An item that no longer exists scores nothing.
                    Leaf::Destroyed => {
                        hand.mass += p;
                        hand.worse += p;
                    }
                    Leaf::Pending(pending) if pending.certain => {
                        hand.item(data, &pat, *p, &pending.item, now.progress)
                    }
                    Leaf::Pending(_) => hand.uncomputed += p,
                    Leaf::Choice(_) => unreachable!("choices were skipped"),
                }
            }
            let weighed = solve::weigh(&pat, &pattern::lift(data, &pat, &outcome), &now);
            let goal = if hand.mass > 0.0 {
                hand.goal / hand.mass
            } else {
                0.0
            };
            let at = |a: f64, b: f64| (a - b).abs() < 1e-12;
            assert!(
                at(weighed.computed, hand.mass)
                    && at(weighed.uncomputed, hand.uncomputed)
                    && at(weighed.finish, hand.finish)
                    && at(weighed.goal, goal)
                    && at(weighed.better, hand.better)
                    && at(weighed.worse, hand.worse)
                    && at(weighed.blocked, hand.blocked),
                "{pname} / {name} / {}: engine {weighed:?}, hand {:.9}/{:.9}/{:.9}/{}",
                method.id,
                hand.mass,
                hand.uncomputed,
                hand.finish,
                goal
            );
            compared += 1;
        }
    }
    assert!(compared > 100, "only {compared} comparisons");
}

/// Every method is reported once, the three groups never overlap, the probabilities of each
/// step are probabilities of the part of it that is computed, and the order is the documented
/// one.
#[test]
fn every_step_conserves_its_mass() {
    let data = test_data();
    let singles = data.methods.iter().filter(|m| m.inputs == 1).count();
    for (pname, name) in common::pairs() {
        let pat = pattern(&pname);
        let target = item(&name);
        let report = solve::next_steps(data, &pat, &target);
        let counted =
            report.exact.len() + report.certain.len() + report.blind.len() + report.unmet.len();
        assert_eq!(counted, singles, "{pname} / {name}");
        for group in [&report.exact, &report.certain, &report.blind] {
            for a in group {
                let w = &a.weighed;
                let at = format!("{pname} / {name} / {}", a.method);
                assert!(
                    (w.computed + w.uncomputed - 1.0).abs() < 1e-9,
                    "{at}: {} + {}",
                    w.computed,
                    w.uncomputed
                );
                assert!(w.finish <= w.computed + 1e-9, "{at}: finish");
                assert!(
                    w.better + w.worse <= w.computed + 1e-9,
                    "{at}: better/worse"
                );
                assert!(w.blocked <= w.computed + 1e-9, "{at}: blocked");
                assert!((0.0..=1.0).contains(&w.goal), "{at}: goal {}", w.goal);
                assert!(w.results >= 1, "{at}: no results");
            }
            for pair in group.windows(2) {
                assert_ne!(
                    pair[0].rank(&pair[1]),
                    std::cmp::Ordering::Greater,
                    "{pname} / {name}: {} before {}",
                    pair[0].method,
                    pair[1].method
                );
            }
        }
        for a in &report.exact {
            assert_eq!(a.weighed.model, Model::Exact, "{}", a.method);
            assert_eq!(a.weighed.uncomputed, 0.0, "{}", a.method);
        }
        for a in &report.certain {
            assert_eq!(a.weighed.model, Model::CertainPart, "{}", a.method);
        }
        for a in &report.blind {
            assert_eq!(a.weighed.model, Model::Blind, "{}", a.method);
            assert_eq!(a.weighed.computed, 0.0, "{}", a.method);
            assert!(a.weighed.uncomputed > 0.0, "{}", a.method);
        }
        for (id, why) in &report.unmet {
            assert!(!why.is_empty(), "{pname} / {name} / {id}: empty reason");
        }
    }
}

/// The ranking reads numbers the way the report prints them: two steps that differ only by
/// rounding noise on finish, goal and worse are ordered by the one real difference, `blocked`.
#[test]
fn rank_ignores_rounding_noise() {
    let at = |method: &str, noise: f64, blocked: f64| solve::Assessment {
        method: method.to_string(),
        name: method.to_string(),
        params: Params::default(),
        cost: Vec::new(),
        weighed: solve::Weighed {
            computed: 1.0,
            finish: noise,
            goal: 0.75 + noise,
            worse: noise,
            blocked,
            model: Model::Exact,
            ..Default::default()
        },
        pending: None,
    };
    // `a` sorts first by id and carries a sliver more goal; only `blocked` really differs.
    let stuck = at("a", 1e-12, 1.0);
    let free = at("b", 0.0, 0.0);
    assert_eq!(free.rank(&stuck), std::cmp::Ordering::Less);
    assert_eq!(stuck.rank(&free), std::cmp::Ordering::Greater);
    let mut steps = [stuck, free];
    steps.sort_by(solve::Assessment::rank);
    assert_eq!(steps[0].method, "b");
}

/// On `crown-l1` the Suffixes Cannot Be Changed craft holds the crafted slot the pattern wants,
/// so the step named best is the one that scours it off: it leaves the item open and the goal
/// where it was, above the slam that fills the last prefix slot and leaves it stuck.
#[test]
fn crown_l1_best_step_is_not_blocked() {
    let data = test_data();
    let report = solve::next_steps(data, &pattern("crown-shaper"), &item("crown-l1"));
    assert!(close(report.now.blocked, 1.0), "{:?}", report.now);
    let best = report.exact.first().expect("an exact step");
    assert_eq!(best.method, "bench/remove-crafted");
    assert_eq!(best.stands(), solve::Stands::Unblocks, "{:?}", best.weighed);
    assert!(close(best.weighed.blocked, 0.0), "{:?}", best.weighed);
    assert!(close(best.weighed.worse, 0.0), "{:?}", best.weighed);
    let slam = step(&report, "beast/suffix-to-prefix");
    assert_eq!(
        best.rank(slam),
        std::cmp::Ordering::Less,
        "the slam ranks above the scour"
    );
}

/// `crown-a` with two suffixes: the Hypothermia it wants and a resistance, crafted or not. Its
/// prefix side is empty and has room.
fn crown_with_resist(crafted: bool) -> poe1::item::Item {
    let mut s = common::spec("crown-a");
    s.suffixes.truncate(1);
    s.suffixes.push(poe1::item::AffixSpec {
        id: if crafted {
            "EinharMasterFireResist3_"
        } else {
            "FireResist4"
        }
        .into(),
        values: None,
        fractured: false,
    });
    poe1::item::Item::from_spec(test_data(), &s).expect("a legal item")
}

/// A crafted suffix the pattern does not want uses the item's one crafted modifier, so the
/// pattern's crafted prefix slot can never be filled: the item is blocked although both sides
/// have room. The same item with a natural resistance is not.
#[test]
fn blocked_counts_a_crafted_slot_the_cap_has_used_up() {
    let data = test_data();
    let shaper = pattern("crown-shaper");
    let read = |it: &poe1::item::Item| {
        pattern::evaluate(data, &shaper, it)
            .into_iter()
            .find(|(p, s)| *p > 0.0 && s.item_ok)
            .map(|(_, s)| s)
            .expect("an accepted state")
    };
    let stuck = read(&crown_with_resist(true));
    assert_eq!(stuck.crafted, [1, 1]);
    assert!(stuck.open.iter().all(|&o| o > 0), "{stuck:?}");
    assert!(solve::blocked(&shaper, &stuck), "{stuck:?}");
    let free = read(&crown_with_resist(false));
    assert_eq!(free.crafted, [0, 1]);
    assert!(!solve::blocked(&shaper, &free), "{free:?}");
}

/// Removing the crafted suffix frees the crafted slot: the bench step unblocks the item.
#[test]
fn removing_the_crafted_suffix_unblocks() {
    let data = test_data();
    let report = solve::next_steps(data, &pattern("crown-shaper"), &crown_with_resist(true));
    assert!(close(report.now.blocked, 1.0), "{:?}", report.now);
    let remove = step(&report, "bench/remove-crafted");
    assert_eq!(
        remove.stands(),
        solve::Stands::Unblocks,
        "{:?}",
        remove.weighed
    );
    assert!(close(remove.weighed.blocked_before, 1.0));
    assert!(close(remove.weighed.blocked, 0.0));
}

/// A blocked item says why, in the report's words: the three kinds cost the player very
/// differently, and the report used to print the same sentence for all of them.
#[test]
fn a_blocker_names_its_kind() {
    let data = test_data();
    let shaper = pattern("crown-shaper");
    let reasons = |name: &str| {
        solve::next_steps(data, &shaper, &item(name))
            .blockers
            .iter()
            .map(|b| b.phrase())
            .collect::<Vec<_>>()
    };
    // One scour frees the crafted slot: the metamod is a tool, not a wall.
    assert_eq!(
        reasons("crown-l1"),
        vec!["a metamod holds the crafted prefix slot; one Orb of Scouring frees it"]
    );
    // A side full of ordinary modifiers is an annulment's gamble, and says so.
    assert_eq!(
        reasons("crown-actual"),
        vec!["the prefix side is full of modifiers a removal could take"]
    );
    // A fractured modifier cannot be removed at all, which is the one permanent blocker here.
    let fractured = reasons("crown-d2-scbc-fractured");
    assert!(
        fractured
            .iter()
            .any(|r| r == "a fractured prefix modifier cannot be removed"),
        "{fractured:?}"
    );
    // A Crown that is not stuck says nothing.
    assert!(reasons("crown-a").is_empty());
    assert!(reasons("crown-ideal").is_empty());
}
