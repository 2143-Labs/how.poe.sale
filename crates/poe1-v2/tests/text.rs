//! Game text: real pastes read back into items (a Crown, a Simplex Amulet, a life flask, a jewel,
//! a synthesised ring, a magic pair of boots), a rendered item prints the paste's lines, and every
//! fixture whose modifiers are rolled or fixed survives a print-and-read round trip.

mod common;

use std::collections::HashSet;

use poe1_v2::data::{Data, Influence, ModKind};
use poe1_v2::item::{AffixSpec, Item, Roll};
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

fn parsed(name: &str) -> (Item, Vec<String>) {
    let data = test_data();
    let parsed = parse(data, &demo(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let item = Item::from_spec(data, &parsed.spec).unwrap_or_else(|e| panic!("{name}: {e}"));
    (item, parsed.warnings)
}

fn ids(data: &Data, rolls: &[Roll]) -> Vec<String> {
    rolls.iter().map(|r| data.mod_at(r.m).id.clone()).collect()
}

fn affix_ids(data: &Data, affixes: &[poe1_v2::item::Affix]) -> Vec<String> {
    affixes
        .iter()
        .map(|a| data.mod_at(a.roll.m).id.clone())
        .collect()
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
    assert_eq!(
        affix_ids(data, &item.prefixes),
        ["DamagePer15StrengthInfluence1"]
    );
    let mut suffixes = affix_ids(data, &item.suffixes);
    suffixes.sort_unstable();
    assert_eq!(
        suffixes,
        [
            "PercentageAllAttributesUberShaper1",
            "PercentageStrengthBodyInfluence2_"
        ]
    );
}

/// A magic life flask from https://www.pathofexile.com/forum/view-thread/3555230: its property
/// lines and the trailing description are not modifiers, and the three printed mod lines are one
/// hybrid prefix and one suffix.
#[test]
fn parse_life_flask() {
    let data = test_data();
    let (item, warnings) = parsed("life-flask");
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(data.base_at(item.base).name, "Divine Life Flask");
    assert_eq!(item.rarity, poe1_v2::data::Rarity::Magic);
    assert_eq!(item.quality, 35);
    assert!(item.corrupted);
    assert!(item.implicits.is_empty(), "a flask has no implicit");
    assert_eq!(
        affix_ids(data, &item.prefixes),
        ["FlaskExtraLifeCostsMana5_"]
    );
    assert_eq!(
        affix_ids(data, &item.suffixes),
        ["LocalLifeFlaskAdditionalLifeRecovery5_"]
    );
}

/// A rare jewel from https://www.pathofexile.com/forum/view-thread/3131823: the trailing
/// description stays out of the modifier pools.
#[test]
fn parse_cobalt_jewel() {
    let data = test_data();
    let (item, warnings) = parsed("cobalt-jewel");
    assert_eq!(data.base_at(item.base).name, "Cobalt Jewel");
    assert_eq!(item.item_level, 83);
    assert!(item.implicits.is_empty(), "a jewel has no implicit");
    assert_eq!(affix_ids(data, &item.prefixes), ["TotemLifeJewel"]);
    // The block line is a jewel corrupted implicit the export does not carry.
    assert_eq!(
        warnings,
        vec![
            "not understood: \"+1% Chance to Block Attack Damage while Dual Wielding\"".to_string()
        ]
    );
}

/// A synthesised ring from https://www.pathofexile.com/forum/view-thread/3307135: the type line
/// carries the "Synthesised" prefix and the synthesis implicit is reachable; the Attribute catalyst
/// scales it.
#[test]
fn parse_synthesised_ring() {
    let data = test_data();
    let (item, warnings) = parsed("synthesised-paua-ring");
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(data.base_at(item.base).name, "Paua Ring");
    assert!(item.synthesised);
    assert_eq!(item.quality, 20);
    let implicit = item.implicits[0].clone();
    let md = data.mod_at(implicit.m);
    assert_eq!(md.kind, ModKind::Synthesis, "{}", md.id);
    assert_eq!(item.effect(data, &implicit, true), 20);
    assert_eq!(Item::display(6, 20), 7, "the paste shows 7%");
    assert_eq!(implicit.values, Some(vec![6]));
    assert_eq!(affix_ids(data, &item.prefixes).len(), 2);
    assert_eq!(affix_ids(data, &item.suffixes).len(), 3);
}

/// The owner's unveil step: the Zombies blocker reads from its two crafted lines, and the stat the
/// game hides at zero prints nothing. The paste is built from the demo Crown's text.
#[test]
fn parse_blocked_crown() {
    let data = test_data();
    let (item, _warnings) = parsed("archdemon-crown-blocked");
    let mut expected = common::spec("crown-c2-blocked");
    expected.defences = Some([440, 0, 90, 0]);
    expected.sockets = "B-B-B-B".into();
    expected.intangibility = 97;
    let expected = Item::from_spec(data, &expected).expect("the expected item");
    assert_eq!(item, expected);
    let zombies = item
        .prefixes
        .iter()
        .map(|a| data.mod_at(a.roll.m).id.as_str())
        .find(|id| *id != "VeiledPrefix");
    assert_eq!(zombies, Some("JunMaster2MaximumZombieAndSkeleton1_"));
}

/// A flask suffix that prints on two lines reads as its modifier, and the "less Duration" line
/// reads through a negative factor. The paste is built from game text.
#[test]
fn parse_two_line_flask_suffix() {
    let data = test_data();
    let (item, warnings) = parsed("quicksilver-flask");
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(data.base_at(item.base).name, "Quicksilver Flask");
    assert_eq!(
        affix_ids(data, &item.prefixes),
        ["FlaskChargesAddedIncreasePercent2_"]
    );
    assert_eq!(
        affix_ids(data, &item.suffixes),
        ["FlaskIgniteImmunityDuringEffect_"]
    );
    assert_eq!(item.suffixes[0].roll.values, Some(vec![1, -47]));
}

/// A magic pair of boots built from game text: one line reads as an explicit, so the block is an
/// explicit block, not an implicit one.
#[test]
fn parse_magic_boots() {
    let data = test_data();
    let (item, warnings) = parsed("slink-boots-magic");
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(data.base_at(item.base).name, "Slink Boots");
    assert!(item.implicits.is_empty());
    assert_eq!(affix_ids(data, &item.prefixes), ["MovementVelocity5"]);
    assert_eq!(affix_ids(data, &item.suffixes), ["ChaosResist5"]);
}

/// A helmet line reads as the helmet modifier, never as a jewel or flask one. The paste is built
/// from game text.
#[test]
fn parse_helmet_lines_by_domain() {
    let data = test_data();
    let (item, _warnings) = parsed("helmet-fire-resistance");
    let base = data.base_at(item.base);
    assert_eq!(base.domain, poe1_v2::data::Domain::Item);
    let mut fire = None;
    for affix in item.explicits() {
        let md = data.mod_at(affix.roll.m);
        assert_eq!(md.domain, "item", "{} is not an item modifier", md.id);
        if md
            .stats
            .iter()
            .any(|s| data.stat(s.stat) == "base_fire_damage_resistance_%")
        {
            fire = Some(md.id.clone());
        }
    }
    assert_eq!(fire.as_deref(), Some("FireResist2"));
}

/// A synthesised item prints its type line with the prefix and its flag; a fractured one prints
/// "Fractured Item"; a fractured crafted modifier keeps " (fractured)".
#[test]
fn render_synthesised_and_fractured() {
    let data = test_data();
    let (ring, _) = parsed("synthesised-paua-ring");
    let text = render(data, &ring);
    assert!(
        text.starts_with("Item Class: Rings\nRarity: Rare\nSynthesised Paua Ring\n"),
        "{text}"
    );
    assert!(text.contains("Synthesised Item"), "{text}");
    assert!(text.contains("7% increased Dexterity"), "{text}");

    // A stat fixed at 0 prints no line: the Zombies blocker shows two lines, not three.
    let blocked = common::item("crown-c2-blocked");
    let text = render(data, &blocked);
    assert!(
        text.contains("+1 to maximum number of Raised Zombies"),
        "{text}"
    );
    assert!(text.contains("+1 to maximum number of Skeletons"), "{text}");
    assert!(!text.contains("Spectres"), "{text}");

    let fractured = common::item("crown-d2-scbc-fractured");
    let text = render(data, &fractured);
    assert!(
        sections(&text)
            .last()
            .is_some_and(|s| s.iter().any(|line| line == "Fractured Item")),
        "{text}"
    );

    // A fractured bench craft keeps its tag.
    let mut spec = common::spec("crown-a");
    spec.prefixes.push(AffixSpec {
        id: "EinharMasterIncreasedMana2".into(),
        values: Some(vec![40]),
        fractured: true,
    });
    let item = Item::from_spec(data, &spec).expect("a fractured crafted prefix");
    let text = render(data, &item);
    assert!(text.contains("40 to maximum Mana (fractured)"), "{text}");
    assert!(!text.contains("40 to maximum Mana (crafted)"), "{text}");
}

/// Bases that share a name are told apart by the implicit they print. The paste is built from game
/// text.
#[test]
fn parse_shared_base_name() {
    let data = test_data();
    let (item, warnings) = parsed("two-stone-ring");
    assert_eq!(
        warnings,
        vec![
            "base name \"Two-Stone Ring\" matches several bases; using Metadata/Items/Rings/Ring12"
                .to_string()
        ]
    );
    assert_eq!(
        data.base_at(item.base).id,
        "Metadata/Items/Rings/Ring12",
        "the Fire and Lightning Two-Stone Ring"
    );
    assert_eq!(
        ids(data, &item.implicits),
        ["FireAndLightningResistImplicitRing1"]
    );
}

/// The reader reaches the league mods the export keeps in their own domains. The pastes are built
/// from game text.
#[test]
fn parse_ducat_and_mercenary() {
    let data = test_data();
    let (belt, warnings) = parsed("ducat-belt");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        affix_ids(data, &belt.prefixes),
        ["DeepwaterHazardModTrapMineActiveDamage"]
    );
    let (gloves, warnings) = parsed("mercenary-gloves");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        affix_ids(data, &gloves.suffixes),
        ["MercenaryModAshWarcries"]
    );
}

/// A utility flask from https://www.pathofexile.com/forum/view-thread/3199842 prints its base
/// effect among the properties; it is not a modifier.
#[test]
fn parse_jade_flask() {
    let data = test_data();
    let (item, warnings) = parsed("jade-flask");
    // The export holds the movement-speed suffix only from level 81; the paste is level 63.
    assert_eq!(
        warnings,
        vec!["not understood: \"7% increased Movement Speed during Flask effect\"".to_string()]
    );
    assert_eq!(data.base_at(item.base).name, "Jade Flask");
    assert!(item.implicits.is_empty(), "the base effect is a property");
    assert_eq!(affix_ids(data, &item.prefixes).len(), 1);
    assert!(item.suffixes.is_empty());
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
        if got == item {
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
        "cogwork-ring-normal",
    ] {
        assert!(
            checked.iter().any(|c| c == name),
            "{name} no longer round-trips: {lossy:?}"
        );
    }
    assert!(lossy.is_empty(), "{lossy:?}");
}

/// A new Talisman's special modifier is the one placeholder implicit: it prints one clear line,
/// a rendered Talisman reads back unchanged, and an enchantment line the pinned data cannot
/// explain reads as the placeholder instead of "not understood" (claim `talisman.pool-missing`).
#[test]
fn talisman_placeholder_prints_and_reads() {
    let data = test_data();
    let talisman = common::item("talisman-rare-four");
    let text = render(data, &talisman);
    assert!(
        text.lines()
            .any(|line| line == "Talisman Implicit Modifier"),
        "{text}"
    );
    let parsed = parse(data, &text).expect("the rendered Talisman parses");
    assert!(
        !parsed
            .warnings
            .iter()
            .any(|w| w.starts_with("not understood")),
        "{:?}",
        parsed.warnings
    );
    let back = Item::from_spec(data, &parsed.spec).expect("the parsed Talisman builds");
    assert_eq!(back, talisman);

    // The game's real line is unknown, so any unexplained enchantment line on a Talisman base is
    // read as the placeholder.
    let paste = "Item Class: Amulets\n\
Rarity: Rare\n\
Chieftain Talisman\n\
--------\n\
Item Level: 84\n\
--------\n\
15% increased maximum Life (enchant)\n\
--------\n\
+52 to maximum Mana\n\
--------\n\
+46 to Strength\n";
    let parsed = parse(data, paste).expect("the Talisman paste parses");
    assert!(
        parsed
            .spec
            .implicits
            .iter()
            .any(|m| m.id == "TalismanImplicitPlaceholder"),
        "{:?}",
        parsed.spec.implicits
    );
    assert!(
        !parsed.warnings.iter().any(|w| w.contains("not understood")),
        "{:?}",
        parsed.warnings
    );
    Item::from_spec(data, &parsed.spec).expect("the placeholder paste builds");
}
