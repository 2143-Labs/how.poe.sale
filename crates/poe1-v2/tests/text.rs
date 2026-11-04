//! Game text: the demo Crown and Simplex pastes read back into items, a rendered item prints the
//! paste's implicit and explicit lines, and every fixture whose modifiers are rolled or fixed
//! survives a print-and-read round trip.

mod common;

use std::collections::HashSet;

use poe1_v2::data::{Data, ModKind};
use poe1_v2::item::{Influence, Item, Roll};
use poe1_v2::test_data;
use poe1_v2::text::{parse, render};

fn demo(name: &str) -> String {
    let path = common::root().join(format!("fixtures/v2/text/{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn sections(text: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![Vec::new()];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "--------" {
            out.push(Vec::new());
            continue;
        }
        out.last_mut().expect("seeded above").push(line.to_string());
    }
    out.retain(|s| !s.is_empty());
    out
}

fn section_starting(text: &str, first: &str) -> Vec<String> {
    sections(text)
        .into_iter()
        .find(|s| s.first().map(String::as_str) == Some(first))
        .unwrap_or_else(|| panic!("no section starting with {first}"))
}

#[test]
fn parse_demo_crown() {
    let data = test_data();
    let parsed = parse(data, &demo("archdemon-crown")).expect("the demo Crown parses");
    assert!(
        parsed.warnings.iter().any(|w| w
            == "\"17% increased Area of Effect\" is the sum of two modifiers; their rolls are unknown"),
        "{:?}",
        parsed.warnings
    );

    let mut expected = common::spec("crown-actual");
    expected.defences = Some([440, 0, 90, 0]);
    for affix in &mut expected.prefixes {
        if affix.id == "JunMasterVeiledSkillAreaOfEffectPercentAndAreaOfEffectGemLevel"
            || affix.id == "AreaOfEffectSupportedUber1_"
        {
            affix.values = None;
        }
    }
    let expected = Item::from_spec(data, &expected).expect("the expected item");
    let got = Item::from_spec(data, &parsed.spec).expect("the parsed item");
    assert_eq!(got, expected);
}

#[test]
fn render_demo_crown() {
    let data = test_data();
    let mut spec = common::spec("crown-actual");
    spec.defences = Some([440, 0, 90, 0]);
    let item = Item::from_spec(data, &spec).expect("crown-actual");
    let rendered = render(data, &item);
    let paste = demo("archdemon-crown");

    for first in [
        "Quality: +20% (augmented)",
        "-2 to Level of Socketed Support Gems",
        "+2 to Level of Socketed AoE Gems",
    ] {
        assert_eq!(
            section_starting(&rendered, first),
            section_starting(&paste, first),
            "section starting with {first}"
        );
    }
}

#[test]
fn parse_demo_simplex() {
    let data = test_data();
    let parsed = parse(data, &demo("simplex-amulet")).expect("the demo Simplex parses");
    assert_eq!(
        parsed.warnings,
        vec!["not understood: \"Allocates Force of Darkness\"".to_string()],
        "every explicit line reads; only the anointment is unexplained"
    );

    let item = Item::from_spec(data, &parsed.spec).expect("the parsed item");
    assert_eq!(data.base_at(item.base).name, "Simplex Amulet");
    assert!(
        item.implicits
            .iter()
            .any(|r| data.mod_at(r.m).id == "MaxPrefixMaxSuffixModEffectImplicitE2"),
        "the base implicit"
    );
    assert!(item.has_influence(Influence::Shaper));
    assert!(item.has_influence(Influence::Warlord));
    assert_eq!(item.intangibility, 6);
    assert_eq!(item.quality, 20);
    assert_eq!(
        data.catalysts[item.catalyst.expect("the quality names a catalyst") as usize].label,
        "Attribute"
    );

    let mut prefixes: Vec<&str> = item
        .prefixes
        .iter()
        .map(|a| data.mod_at(a.roll.m).id.as_str())
        .collect();
    prefixes.sort_unstable();
    assert_eq!(prefixes, ["DamagePer15StrengthInfluence1"]);
    let mut suffixes: Vec<&str> = item
        .suffixes
        .iter()
        .map(|a| data.mod_at(a.roll.m).id.as_str())
        .collect();
    suffixes.sort_unstable();
    assert_eq!(
        suffixes,
        [
            "PercentageAllAttributesUberShaper1",
            "PercentageStrengthBodyInfluence2_"
        ]
    );
}

/// Every modifier is rolled, or all its stats are fixed, or it is the veiled placeholder.
fn printable(data: &Data, item: &Item) -> bool {
    let mods = item
        .implicits
        .iter()
        .chain(&item.enchants)
        .map(|r| (r.m, r.values.is_some()))
        .chain(
            item.explicits()
                .map(|a| (a.roll.m, a.roll.values.is_some())),
        );
    for (m, rolled) in mods {
        let md = data.mod_at(m);
        let fixed = !md.stats.is_empty() && md.stats.iter().all(|s| s.min == s.max);
        if !rolled && !fixed && md.kind != ModKind::Veiled {
            return false;
        }
    }
    // A line two explicits share prints as their sum, so its rolls are not recoverable.
    let mut seen: HashSet<u16> = HashSet::new();
    for affix in item.explicits() {
        for stat in &data.mod_at(affix.roll.m).stats {
            if data.translation_of_stat(stat.stat).is_some() && !seen.insert(stat.stat) {
                return false;
            }
        }
    }
    true
}

/// The item with a fixed roll filled in wherever it was left unrolled.
fn with_fixed_values(data: &Data, item: &Item) -> Item {
    fn fill(data: &Data, roll: &mut Roll) {
        if roll.values.is_some() {
            return;
        }
        let md = data.mod_at(roll.m);
        if !md.stats.is_empty() && md.stats.iter().all(|s| s.min == s.max) {
            roll.values = Some(md.stats.iter().map(|s| s.min).collect());
        }
    }
    let mut out = item.clone();
    for roll in out.implicits.iter_mut().chain(out.enchants.iter_mut()) {
        fill(data, roll);
    }
    for affix in out.prefixes.iter_mut().chain(out.suffixes.iter_mut()) {
        fill(data, &mut affix.roll);
    }
    out
}

#[test]
fn round_trip() {
    let data = test_data();
    let mut checked: Vec<String> = Vec::new();
    let mut lossy: Vec<(String, String)> = Vec::new();
    for name in common::fixture_names() {
        let item = common::item(&name);
        if !printable(data, &item) {
            continue;
        }
        let text = render(data, &item);
        let parsed = match parse(data, &text) {
            Ok(parsed) => parsed,
            Err(err) => panic!("{name}: {err}\n{text}"),
        };
        let got = Item::from_spec(data, &parsed.spec).expect("the parsed item");
        let expected = with_fixed_values(data, &item);
        if got == expected {
            checked.push(name);
        } else {
            lossy.push((name, parsed.warnings.join("; ")));
        }
    }
    for name in [
        "crown-a",
        "crown-a-mana",
        "crown-l1",
        "crown-l2",
        "crown-c1",
        "crown-c2",
        "crown-d2-scbc",
        "crown-d2-scbc-fractured",
        "cobalt-jewel-magic",
        "slink-boots-eldritch",
        "simplex-normal",
    ] {
        assert!(
            checked.iter().any(|c| c == name),
            "{name} no longer round-trips"
        );
    }
    assert!(lossy.is_empty(), "{lossy:?}");
}
