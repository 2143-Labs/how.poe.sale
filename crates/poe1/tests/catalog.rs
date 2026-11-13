//! The method catalog: every currency scoped, method ids unique and sorted, claims resolve, the
//! methods the Crown route relies on exist, a bad method stops the bundle loading, the
//! protection policies follow their sources, and the reforge tables and resonators are the
//! data's.

use std::collections::BTreeSet;

mod common;

use poe1::data::{Data, Scope};
use poe1::method::{CostSpec, Family, Method, Op, Policy, Protection, Require};
use poe1::test_data;

const REASONS: [&str; 7] = [
    "placeholder",
    "shard",
    "maps-atlas",
    "gems",
    "other-item-class",
    "cost-only",
    "not-used-on-items",
];

#[test]
fn every_currency_scoped() {
    let d = test_data();
    assert_eq!(d.scope.len(), d.currencies.len());
    let scoped: BTreeSet<&str> = d.scope.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(scoped.len(), d.currencies.len());
    for c in &d.currencies {
        assert!(scoped.contains(c.id.as_str()), "{} unscoped", c.id);
    }
    for (id, s) in &d.scope {
        match s {
            Scope::Methods { ids } => {
                assert!(!ids.is_empty(), "{id}");
                for m in ids {
                    assert!(d.method(m).is_some(), "{id}: unknown method {m}");
                }
            }
            Scope::Excluded { reason } => {
                assert!(REASONS.contains(&reason.as_str()), "{id}: {reason}")
            }
        }
    }
}

#[test]
fn ids_unique_and_sorted() {
    let d = test_data();
    let ids: Vec<&str> = d.methods.iter().map(|m| m.id.as_str()).collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn claims_resolve() {
    let d = test_data();
    for m in &d.methods {
        for c in &m.claims {
            assert!(d.claim(c).is_some(), "{}: unknown claim {c}", m.id);
        }
    }
    for c in &d.claims {
        if c.checked {
            assert!(!c.quote.is_empty(), "{} is checked without a quote", c.id);
        }
    }
}

#[test]
fn known_methods() {
    let d = test_data();
    let scbc = d
        .mod_by_id("DexMasterItemGenerationCannotChangeSuffixes")
        .unwrap();
    for id in [
        "currency/veiled-exalted".to_string(),
        "unveil".to_string(),
        format!("bench/add/{}", d.mod_at(scbc).id),
        "essence/deafening-essence-of-hatred".to_string(),
    ] {
        assert!(d.method(&id).is_some(), "{id} missing");
    }
}

/// A4/E6: `Data::from_bundle` validates every method, so a bad one stops the bundle loading.
#[test]
fn bad_methods_do_not_load() {
    let d = test_data();
    type Edit = fn(&mut Method);
    let cases: [(Edit, &str); 6] = [
        (
            |m| {
                m.ops = vec![Op::AddMod {
                    r#mod: "NoSuchMod".into(),
                }]
            },
            "unknown mod NoSuchMod",
        ),
        (
            |m| {
                m.cost = vec![CostSpec {
                    name: "No Such Orb".into(),
                    amount: 1.0,
                }]
            },
            "cost names unknown currency \"No Such Orb\"",
        ),
        (
            |m| {
                m.cost = vec![CostSpec {
                    name: "Primitive Chaotic Resonator".into(),
                    amount: 1.0,
                }]
            },
            "cost \"Primitive Chaotic Resonator\" names 2 currencies; use one with a unique name",
        ),
        (
            |m| m.claims.push("no.such-claim".into()),
            "unknown claim no.such-claim",
        ),
        (
            |m| {
                m.requires.push(Require::CatalystRoom {
                    catalyst: "Shiny Catalyst".into(),
                })
            },
            "unknown catalyst Shiny Catalyst",
        ),
        (
            |m| m.ops.push(Op::Scour),
            "changes existing modifiers but states no protection policy",
        ),
    ];
    for (edit, want) in cases {
        let mut bundle = d.bundle.clone();
        let m = bundle
            .methods
            .iter_mut()
            .find(|m| m.id == "currency/exalted")
            .unwrap();
        edit(m);
        let err = Data::from_bundle(bundle).unwrap_err().to_string();
        assert_eq!(err, format!("method currency/exalted: {want}"));
    }
}

/// R1: each method's metamod policy, from Craft of Exile's metamod table (checked against
/// maxroll's metacrafting guide and vhpg's Prefixes Cannot Be Changed table).
#[test]
fn protection_policies_follow_the_sources() {
    use Policy::{Forbid, Ignore, Respect};
    let d = test_data();
    for (id, locks, cannot_roll) in [
        ("currency/chaos", Respect, Ignore),
        ("currency/veiled-chaos", Respect, Ignore),
        ("currency/divine", Respect, Ignore),
        ("currency/orb-of-dominance", Respect, Ignore),
        ("currency/alteration", Respect, Ignore),
        ("harvest/fire-to-cold", Respect, Ignore),
        ("currency/eldritch-chaos", Ignore, Ignore),
        ("currency/annulment", Respect, Respect),
        ("currency/eldritch-annulment", Respect, Respect),
        ("currency/scouring", Respect, Ignore),
        ("currency/veiled-exalted", Respect, Ignore),
        ("harvest/add-life-remove-random", Respect, Ignore),
        ("beast/prefix-to-suffix", Respect, Ignore),
        ("beast/suffix-to-prefix", Respect, Ignore),
        ("fossil/chaotic-resonator", Forbid, Forbid),
        ("fossil/alchemical-resonator", Forbid, Forbid),
    ] {
        assert_eq!(
            d.method(id).unwrap().protect,
            Some(Protection { locks, cannot_roll }),
            "{id}"
        );
    }
    for m in d.methods.iter().filter(|m| m.family == Family::Essence) {
        assert_eq!(
            m.protect,
            Some(Protection::IGNORE_ALL),
            "{} does not ignore every metamod",
            m.id
        );
    }
}

/// A3: every reforge's count table is claim `chaos.count`'s, and no essence limits the item
/// level any more (R4): its cap is the Fill's modifier level.
#[test]
fn fill_tables_are_the_claims() {
    let d = test_data();
    let claim: std::collections::BTreeMap<u8, u32> =
        serde_json::from_value(d.claim("chaos.count").unwrap().value.clone().unwrap()).unwrap();
    let table: Vec<(u8, u32)> = claim.into_iter().collect();
    let mut fills = 0;
    for m in &d.methods {
        for op in m.all_ops() {
            if let Op::Fill { fill } = op
                && !fill.counts.is_empty()
            {
                assert_eq!(fill.counts, table, "{}", m.id);
                fills += 1;
            }
        }
    }
    assert!(fills > 100, "{fills}");
    let wailing = d.method("essence/wailing-essence-of-hatred").unwrap();
    let caps: Vec<Option<u8>> = wailing
        .all_ops()
        .into_iter()
        .filter_map(|op| match op {
            Op::Fill { fill } => Some(fill.mod_level_max),
            _ => None,
        })
        .collect();
    assert_eq!(caps, [Some(75)]);
}

/// E6: the resonators are the stackable currencies, one per fossil count.
#[test]
fn resonators_are_data() {
    let d = test_data();
    for (id, kind) in [
        ("fossil/chaotic-resonator", "Chaotic"),
        ("fossil/alchemical-resonator", "Alchemical"),
    ] {
        let m = d.method(id).unwrap();
        assert_eq!(m.resonators.len(), 4);
        for (r, size) in m
            .resonators
            .iter()
            .zip(["Primitive", "Potent", "Powerful", "Prime"])
        {
            let c = d.currency_by_id(r).unwrap();
            assert_eq!(c.name, format!("{size} {kind} Resonator"));
            assert_eq!(c.class, "DelveStackableSocketableCurrency");
        }
    }
}

/// The directions check `cli` runs, restated here so the bundle itself is guarded: a
/// method's costing currency directions may state a limit the method must repeat, never looser.
#[test]
fn method_requirements_follow_the_currency_directions() {
    let d = test_data();
    for m in &d.methods {
        if m.family != Family::Currency {
            continue;
        }
        for c in &m.cost {
            if c.name.starts_with("beast:") {
                continue;
            }
            let Some(cur) = d.currencies.iter().find(|x| x.name == c.name) else {
                continue;
            };
            let dirs = cur.directions.as_str();
            let has = |p: &str| dirs.contains(p);
            // The three quality currencies say "non-unique" because the 3.25 item-level scaling
            // applies only to non-unique items (claim `quality.per-level`); a unique item is still
            // a legal target (claim `quality.unique`), so their methods do not repeat the limit.
            let non_unique_exempt = matches!(
                m.id.as_str(),
                "currency/armourers-scrap"
                    | "currency/blacksmiths-whetstone"
                    | "currency/glassblowers-bauble"
            );
            let mut wants: Vec<(&str, Require)> = Vec::new();
            if (has("non-unique") || has("non-Unique")) && !non_unique_exempt {
                wants.push(("non-unique", Require::NotUnique));
            }
            let cannot_influence = has("Cannot be used on") && has("Influenced");
            if cannot_influence || has("with no influence") {
                wants.push(("no-influence", Require::NoInfluence));
            }
            if has("Influenced") && !cannot_influence && !has("with no influence") {
                wants.push(("influenced", Require::MinInfluencedMods { n: 2 }));
            }
            if has("Fractured") {
                wants.push(("no-fractured", Require::NoFractured));
            }
            if has("Cannot be used on Veiled") {
                wants.push(("no-veiled", Require::NoVeiled));
            }
            let rarities = if has("normal, magic or rare") {
                Some(vec![
                    poe1::data::Rarity::Normal,
                    poe1::data::Rarity::Magic,
                    poe1::data::Rarity::Rare,
                ])
            } else if has("magic or rare item") {
                Some(vec![poe1::data::Rarity::Magic, poe1::data::Rarity::Rare])
            } else if has("magic, rare or unique") {
                Some(vec![
                    poe1::data::Rarity::Magic,
                    poe1::data::Rarity::Rare,
                    poe1::data::Rarity::Unique,
                ])
            } else if has("rare item") {
                Some(vec![poe1::data::Rarity::Rare])
            } else {
                None
            };
            if let Some(any_of) = rarities {
                wants.push(("rarity", Require::Rarity { any_of }));
            }
            if has("normal item") {
                wants.push((
                    "normal",
                    Require::Rarity {
                        any_of: vec![poe1::data::Rarity::Normal],
                    },
                ));
            }
            if has("corrupted") {
                wants.push(("corrupted", Require::Corrupted));
            }
            for (key, want) in wants {
                assert!(
                    implies(m, &want),
                    "{} costs {} whose directions state {key}",
                    m.id,
                    c.name
                );
            }
        }
    }
}

/// Whether the method's requirements imply `want`.
fn implies(m: &Method, want: &Require) -> bool {
    m.requires.iter().any(|have| match (have, want) {
        (Require::NotUnique, Require::NotUnique)
        | (Require::NoInfluence, Require::NoInfluence)
        | (Require::NoFractured, Require::NoFractured)
        | (Require::NoVeiled, Require::NoVeiled)
        | (Require::NotCorrupted, Require::NotCorrupted)
        | (Require::Corrupted, Require::Corrupted) => true,
        (Require::Rarity { any_of: a }, Require::Rarity { any_of: b }) => {
            b.iter().all(|r| a.contains(r))
        }
        (Require::HasInfluence { .. } | Require::SingleInfluence, Require::HasInfluence { .. }) => {
            true
        }
        (Require::MinInfluencedMods { n }, Require::MinInfluencedMods { n: want }) => n >= want,
        _ => false,
    })
}

/// The owner decisions are recorded as owner claims with the owner's words.
#[test]
fn owner_decisions_are_claims() {
    let d = test_data();
    for id in [
        "empty.refused",
        "metamod.crafted-room",
        "bench.normal-to-magic",
        "fracture.crafted",
        "multicraft.annul",
        "hinekoras.unmodelled",
        "reforge.counts",
        "pattern.crown-ideal-finished",
        "metamod.multicraft-does-not-block",
        "enchant.font-removed",
        "fracture.eligible",
        "quality.unique",
        "tainted-exalt.locks",
        "vaal.locked-implicits",
        "bench.corrupted-sockets",
        "magic.count-weights",
        "add.side-first",
        "essence.ignores-metamods",
        "reforge.counts-scope",
        "harvest.damage-conversion",
        "beast.aspect-level",
        "beast.talisman-fracture-cost",
    ] {
        let c = d.claim(id).unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!(c.source_kind, poe1::data::SourceKind::Owner, "{id}");
        assert!(c.checked, "{id} is not checked");
        assert!(!c.quote.is_empty(), "{id} has no quote");
    }
    assert_eq!(
        d.claim("bench.normal-to-magic").unwrap().quote,
        "Turns it magic"
    );
}

/// The owner confirmed all twelve recombine rules, and each recombinator still cites them.
#[test]
fn recombine_rules_are_owner_claims() {
    let d = test_data();
    let mut ids: Vec<&str> = d
        .claims
        .iter()
        .map(|c| c.id.as_str())
        .filter(|id| id.starts_with("recombine."))
        .collect();
    ids.sort_unstable();
    assert_eq!(ids.len(), 12, "the recombine family is twelve rules");
    for id in &ids {
        let c = d.claim(id).unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!(c.source_kind, poe1::data::SourceKind::Owner, "{id}");
        assert!(c.checked, "{id} is not checked");
        assert_eq!(c.quote, "All twelve are right", "{id}");
    }
    let cited: BTreeSet<&str> = ids.iter().copied().collect();
    for id in [
        "currency/armour-recombinator",
        "currency/jewellery-recombinator",
        "currency/weapon-recombinator",
    ] {
        let m = d.method(id).unwrap_or_else(|| panic!("{id} missing"));
        for claim in &cited {
            assert!(
                m.claims.iter().any(|c| c.as_str() == *claim),
                "{id} no longer cites {claim}"
            );
        }
    }
}

/// The 3.29 removals and the claim they displace are gone, and the new family is scoped.
#[test]
fn removed_and_new_methods() {
    let d = test_data();
    for id in [
        "harvest/synthesise",
        "beast/split",
        "beast/split-three",
        "currency/enchant",
        "currency/tainted-blessing",
    ] {
        assert!(d.method(id).is_none(), "{id} still exists");
    }
    for id in [
        "harvest.first-draw-influenced",
        "ducat.odds",
        "enchant.font-shape",
        "enchant.lab-tiers",
    ] {
        assert!(d.claim(id).is_none(), "{id} still exists");
    }
    let vaal = d.currencies.iter().find(|c| c.name == "Vaal Orb").unwrap();
    let Scope::Methods { ids } = &d.scope.iter().find(|(k, _)| *k == vaal.id).unwrap().1 else {
        panic!("Vaal Orb is not scoped to methods")
    };
    for m in [
        "currency/vaal",
        "currency/vaal-socketless",
        "currency/vaal-jewel",
        "currency/vaal-flask",
        "currency/vaal-unique",
    ] {
        assert!(ids.iter().any(|x| x == m), "Vaal Orb does not scope {m}");
    }
    for name in [
        "Oil Extractor",
        "Omen of Fortune",
        "Omen of Trichromatism",
        "Omen of Connections",
        "Omen of the Jeweller",
    ] {
        let c = d.currencies.iter().find(|c| c.name == name).unwrap();
        assert!(
            matches!(
                d.scope.iter().find(|(k, _)| *k == c.id).unwrap().1,
                Scope::Methods { .. }
            ),
            "{name} is not scoped to a method"
        );
    }
    // The Divine Font is gone (claim enchant.font-removed): its two currencies carry no method.
    for name in ["Enchant", "Tainted Blessing"] {
        let c = d.currencies.iter().find(|c| c.name == name).unwrap();
        assert!(
            matches!(
                d.scope.iter().find(|(k, _)| *k == c.id).unwrap().1,
                Scope::Excluded { ref reason } if reason == "not-used-on-items"
            ),
            "{name} is not excluded as not used on items"
        );
    }
}

/// Every requirement the review's findings require, present on the method they name.
#[test]
fn finding_requirements_are_present() {
    let d = test_data();
    let has = |id: &str, f: &dyn Fn(&Require) -> bool| {
        let m = d.method(id).unwrap_or_else(|| panic!("{id} missing"));
        assert!(
            m.requires.iter().any(f),
            "{id} lacks the requirement the finding asks for: {:?}",
            m.requires
        );
    };
    // R8: influence Exalted Orbs refuse fractured items and eldritch implicits.
    for id in [
        "currency/shapers-exalted",
        "currency/elders-exalted",
        "currency/crusaders-exalted",
        "currency/redeemers-exalted",
        "currency/hunters-exalted",
        "currency/warlords-exalted",
    ] {
        has(id, &|r| matches!(r, Require::NoFractured));
        has(id, &|r| matches!(r, Require::NoEldritch));
    }
    // R9: embers and ichors refuse synthesised items; the Eldritch Annulment takes magic or rare.
    for id in [
        "currency/lesser-eldritch-ember",
        "currency/greater-eldritch-ember",
        "currency/grand-eldritch-ember",
        "currency/exceptional-eldritch-ember",
        "currency/lesser-eldritch-ichor",
        "currency/greater-eldritch-ichor",
        "currency/grand-eldritch-ichor",
        "currency/exceptional-eldritch-ichor",
    ] {
        has(id, &|r| matches!(r, Require::NotSynthesised));
    }
    has(
        "currency/eldritch-annulment",
        &|r| matches!(r, Require::Rarity { any_of } if any_of == &[poe1::data::Rarity::Magic, poe1::data::Rarity::Rare]),
    );
    has("currency/orb-of-conflict", &|r| {
        matches!(r, Require::NotUnique)
    });
    // R11: each recombinator takes its own item classes.
    has(
        "currency/armour-recombinator",
        &|r| matches!(r, Require::Category { any_of } if any_of == &[poe1::data::Category::Armour]),
    );
    has(
        "currency/weapon-recombinator",
        &|r| matches!(r, Require::Category { any_of } if any_of == &[poe1::data::Category::Weapon, poe1::data::Category::Quiver]),
    );
    has(
        "currency/jewellery-recombinator",
        &|r| matches!(r, Require::Category { any_of } if any_of == &[poe1::data::Category::Jewellery]),
    );
    // R13: the Mirror takes a non-unique item; the Mists refuse veiled, influenced and synthesised.
    has("currency/mirror", &|r| matches!(r, Require::NotUnique));
    for id in [
        "currency/reflecting-mist",
        "currency/fading-reflecting-mist",
    ] {
        has(id, &|r| matches!(r, Require::NoVeiled));
        has(id, &|r| matches!(r, Require::NoInfluence));
        has(id, &|r| matches!(r, Require::NotSynthesised));
    }
    // R18: aspect crafts and the fossil family's own limits.
    for id in [
        "beast/aspect-avian",
        "beast/aspect-cat",
        "beast/aspect-crab",
        "beast/aspect-spider",
    ] {
        has(
            id,
            &|r| matches!(r, Require::Rarity { any_of } if any_of.contains(&poe1::data::Rarity::Magic)),
        );
    }
    has("beast/maximum-links", &|r| {
        matches!(r, Require::NotCorrupted)
    });
    has("beast/maximum-links", &|r| {
        matches!(r, Require::NotMirrored)
    });
    for id in ["fossil/chaotic-resonator", "fossil/alchemical-resonator"] {
        has(id, &|r| matches!(r, Require::Category { .. }));
        has(id, &|r| matches!(r, Require::FossilLimits));
    }
    // R21: the Level 28 reading is recorded, unchecked.
    let level28 = d.claim("metamod.level-28").expect("metamod.level-28");
    assert!(!level28.checked);
    // R25: the presence checks come first on every method.
    for m in &d.methods {
        let mut seen_other = false;
        for r in &m.requires {
            match r {
                Require::NotCorrupted | Require::Corrupted => {
                    assert!(!seen_other, "{}: presence check not first", m.id)
                }
                Require::NotMirrored => assert!(!seen_other, "{}: mirrored check not early", m.id),
                _ => seen_other = true,
            }
        }
    }
}

/// A strand count is a fact the report must state: the odds it prints are the cut ones
/// (claim `memory-strands.rollable-tiers`), and a reader who cannot see the count would read them
/// as the whole pool's.
#[test]
fn a_strand_count_is_stated_in_the_report() {
    let d = test_data();
    let pattern = common::pattern("crown-shaper");
    let mut spec = common::spec("crown-a");
    spec.memory_strands = 30;
    let stranded = poe1::item::Item::from_spec(d, &spec).expect("an item");
    let lines = poe1::catalog::next_steps_section(
        d,
        "crown-a",
        &stranded,
        "crown-shaper",
        &pattern,
        poe1::catalog::Caps::default(),
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("**Memory Strands:** 30")),
        "no strand line in:\n{}",
        lines.join("\n")
    );
    spec.memory_strands = 0;
    let plain = poe1::item::Item::from_spec(d, &spec).expect("an item");
    let lines = poe1::catalog::next_steps_section(
        d,
        "crown-a",
        &plain,
        "crown-shaper",
        &pattern,
        poe1::catalog::Caps::default(),
    );
    assert!(!lines.iter().any(|l| l.contains("Memory Strands")));
}
