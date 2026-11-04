//! `apply`: exact single-draw outcome tables against independent numbers (the owner's Craft of
//! Exile unveil table, closed forms, and the plan's Python recount over `data/raw/mods.json`),
//! requirement texts, Pending reforges and the post-processing rules.

mod common;

use common::{item, pattern, spec};
use poe1_v2::apply::{Outcome, Params, apply};
use poe1_v2::data::{Data, ModKind, Rarity};
use poe1_v2::item::{AffixSpec, Item};
use poe1_v2::method::{Method, Op, PendingReason};
use poe1_v2::pattern::{Lifted, lift};
use poe1_v2::test_data;

fn method(id: &str) -> &'static Method {
    test_data()
        .method(id)
        .unwrap_or_else(|| panic!("{id} missing"))
}

fn run(id: &str, it: &Item) -> Outcome {
    apply(
        test_data(),
        method(id),
        std::slice::from_ref(it),
        &Params::default(),
    )
    .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn reason(id: &str, it: &Item) -> String {
    match apply(
        test_data(),
        method(id),
        std::slice::from_ref(it),
        &Params::default(),
    ) {
        Ok(o) => panic!("{id} applied: {o:?}"),
        Err(e) => e.reason,
    }
}

/// Every (probability, leaf) of an outcome, nested Chances multiplied out.
fn leaves(o: &Outcome) -> Vec<(f64, &Outcome)> {
    match o {
        Outcome::Chance(c) => c
            .iter()
            .flat_map(|(p, o)| leaves(o).into_iter().map(move |(q, l)| (p * q, l)))
            .collect(),
        other => vec![(1.0, other)],
    }
}

fn leaf_item(o: &Outcome) -> &Item {
    match o {
        Outcome::Item(i) | Outcome::Pending { item: i, .. } => i,
        other => panic!("not an item: {other:?}"),
    }
}

fn ids(d: &Data, side: &[poe1_v2::item::Affix]) -> Vec<String> {
    side.iter().map(|a| d.mod_at(a.roll.m).id.clone()).collect()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn chance(o: &Outcome) -> &Vec<(f64, Outcome)> {
    match o {
        Outcome::Chance(c) => c,
        other => panic!("not a Chance: {other:?}"),
    }
}

#[test]
fn veiled_exalt_halves() {
    let out = run("currency/veiled-exalted", &item("crown-l1"));
    let c = chance(&out);
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|(p, _)| *p == 0.5));
    let mut got: Vec<&Item> = c.iter().map(|(_, o)| leaf_item(o)).collect();
    got.sort();
    let mut want = [item("crown-c1"), item("crown-c2")];
    want.sort();
    assert_eq!(got, want.iter().collect::<Vec<_>>());
}

#[test]
fn veiled_exalt_thirds() {
    let d = test_data();
    let input = item("crown-l2");
    let out = run("currency/veiled-exalted", &input);
    let c = chance(&out);
    assert_eq!(c.len(), 3);
    let mut removed = Vec::new();
    for (p, o) in c {
        assert!(close(*p, 1.0 / 3.0, 1e-12));
        let it = leaf_item(o);
        let prefixes = ids(d, &it.prefixes);
        assert!(prefixes.contains(&"VeiledPrefix".to_string()));
        for id in ids(d, &input.prefixes) {
            if !prefixes.contains(&id) {
                removed.push(id);
            }
        }
    }
    removed.sort();
    assert_eq!(
        removed,
        [
            "DexMasterItemGenerationCannotChangeSuffixes",
            "IncreasedLife9",
            "LocalIncreasedArmourAndEnergyShield2"
        ]
    );
}

#[test]
fn annul_thirds_and_fracture() {
    let d = test_data();
    let input = item("crown-d2-scbc");
    let out = run("currency/annulment", &input);
    let c = chance(&out);
    assert_eq!(c.len(), 3);
    for (p, o) in c {
        assert!(close(*p, 1.0 / 3.0, 1e-12));
        assert_eq!(leaf_item(o).suffixes, input.suffixes);
    }
    let out = run("currency/annulment", &item("crown-d2-scbc-fractured"));
    let c = chance(&out);
    assert_eq!(c.len(), 2);
    for (p, o) in c {
        assert_eq!(*p, 0.5);
        assert!(ids(d, &leaf_item(o).prefixes).contains(&"IncreasedLife9".to_string()));
    }
}

/// Σ p of the offer sets that include the mod.
fn inclusion(d: &Data, out: &Outcome, id: &str) -> f64 {
    chance(out)
        .iter()
        .filter(|(_, set)| match set {
            Outcome::Choice(opts) => opts
                .iter()
                .any(|o| ids(d, &leaf_item(o).prefixes).contains(&id.to_string())),
            other => panic!("not a Choice: {other:?}"),
        })
        .map(|(p, _)| p)
        .sum()
}

#[test]
fn unveil_matches_owner_table() {
    let d = test_data();
    let out = run("unveil", &item("crown-c2"));
    let sets = chance(&out);
    assert_eq!(sets.len(), 84);
    assert!(
        sets.iter()
            .all(|(_, s)| matches!(s, Outcome::Choice(o) if o.len() == 3))
    );
    assert!(close(sets.iter().map(|(p, _)| p).sum(), 1.0, 1e-12));
    // Exact values from the plan's recount; the owner's Craft of Exile figures 37.165% / 46.668% /
    // 26.166% are these truncated.
    for (id, want) in [
        (
            "JunMasterVeiledSkillAreaOfEffectPercentAndAreaOfEffectGemLevel",
            0.3716594,
        ),
        ("JunMasterVeiledMaximumZombieAndSkeleton", 0.4666895),
        ("JunMasterVeiledBaseLifeAndManaRegen_", 0.2616682),
    ] {
        let got = inclusion(d, &out, id);
        assert!(close(got, want, 1e-6), "{id}: {got}");
    }
}

#[test]
fn unveil_blocker() {
    let d = test_data();
    let out = run("unveil", &item("crown-c2-blocked"));
    assert_eq!(chance(&out).len(), 56);
    assert_eq!(
        inclusion(d, &out, "JunMasterVeiledMaximumZombieAndSkeleton"),
        0.0
    );
    // From the plan's recount script: 8 options, total weight 10000.
    let aoe = inclusion(
        d,
        &out,
        "JunMasterVeiledSkillAreaOfEffectPercentAndAreaOfEffectGemLevel",
    );
    assert!(close(aoe, 0.4374650, 1e-6), "{aoe}");
}

/// Σ p of lifted states whose prefixes hold the class.
fn lifted_mass(l: &Lifted, class: u16) -> f64 {
    match l {
        Lifted::State(s) => {
            if s.prefixes.contains(&class) {
                1.0
            } else {
                0.0
            }
        }
        Lifted::Chance(c) => c.iter().map(|(p, l)| p * lifted_mass(l, class)).sum(),
        Lifted::Many(items) => items.iter().map(|l| lifted_mass(l, class)).sum(),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn exalt_lifts_onto_pattern() {
    let d = test_data();
    let input = item("crown-a-mana");
    let out = run("currency/exalted", &input);
    for (_, o) in leaves(&out) {
        let it = leaf_item(o);
        assert_eq!(it.prefixes.len(), input.prefixes.len() + 1);
        assert_eq!(it.suffixes, input.suffixes);
    }
    let p = pattern("crown-shaper");
    let lifted = lift(d, &p, &out);
    // The plan's recount: 55 prefixes, total weight 43375; T1/T2 life 1000 each, the three
    // Increased Area of Effect support tiers 800 each.
    let good = lifted_mass(&lifted, p.class_index("good-prefix").unwrap());
    let ok = lifted_mass(&lifted, p.class_index("ok-prefix").unwrap());
    assert!(close(good, 2000.0 / 43375.0, 1e-12), "{good}");
    assert!(close(ok, 2400.0 / 43375.0, 1e-12), "{ok}");
}

fn bench_by_ops(ops: &[Op]) -> &'static Method {
    test_data()
        .methods
        .iter()
        .find(|m| m.family == poe1_v2::method::Family::Bench && m.ops == ops)
        .unwrap_or_else(|| panic!("no bench method {ops:?}"))
}

#[test]
fn bench_and_limits() {
    let d = test_data();
    let mana = bench_by_ops(&[Op::AddMod {
        r#mod: "EinharMasterIncreasedMana2".into(),
    }]);
    let out = apply(d, mana, &[item("crown-a")], &Params::default()).unwrap();
    let mut want = spec("crown-a-mana");
    want.prefixes[0].values = None;
    assert_eq!(out, Outcome::Item(Item::from_spec(d, &want).unwrap()));
    let err = apply(d, mana, &[item("crown-a-mana")], &Params::default()).unwrap_err();
    assert_eq!(
        err.reason,
        "the item already has as many crafted modifiers as it can hold"
    );
}

#[test]
fn scour_with_lock() {
    assert_eq!(
        run("currency/scouring", &item("crown-l1")),
        Outcome::Item(item("crown-a"))
    );
}

#[test]
fn remove_crafted() {
    let d = test_data();
    let m = bench_by_ops(&[Op::RemoveCrafted]);
    let out = apply(d, m, &[item("crown-c2-blocked")], &Params::default()).unwrap();
    assert_eq!(ids(d, &leaf_item(&out).prefixes), ["VeiledPrefix"]);
}

#[test]
fn transmute_mass() {
    let out = run("currency/transmutation", &item("cobalt-jewel-normal"));
    let all = leaves(&out);
    assert!(close(all.iter().map(|(p, _)| p).sum(), 1.0, 1e-9));
    let one: f64 = all
        .iter()
        .filter(|(_, o)| {
            let it = leaf_item(o);
            it.prefixes.len() + it.suffixes.len() == 1
        })
        .map(|(p, _)| p)
        .sum();
    assert!(close(one, 0.5, 1e-9), "{one}");
}

#[test]
fn pending_and_vaal() {
    let d = test_data();
    let crown_a = item("crown-a");
    match run("currency/chaos", &crown_a) {
        Outcome::Pending { item, reason, .. } => {
            assert_eq!(reason, PendingReason::MultiDraw);
            assert_eq!(item, crown_a);
        }
        other => panic!("{other:?}"),
    }

    let boots = item("slink-boots-normal");
    let essence = d
        .essences
        .iter()
        .find(|e| e.name == "Deafening Essence of Hatred")
        .unwrap();
    let boots_mod = essence
        .mods
        .iter()
        .find(|(c, _)| *c == d.base_at(boots.base).class)
        .unwrap()
        .1;
    match run("essence/deafening-essence-of-hatred", &boots) {
        Outcome::Pending { item, ops, reason } => {
            assert_eq!(reason, PendingReason::MultiDraw);
            assert_eq!(item.rarity, Rarity::Rare);
            let explicits: Vec<u32> = item.explicits().map(|a| a.roll.m).collect();
            assert_eq!(explicits, [boots_mod]);
            assert!(matches!(ops[0], Op::Fill { .. }));
        }
        other => panic!("{other:?}"),
    }

    let out = run("currency/veiled-chaos", &crown_a);
    let c = chance(&out);
    assert_eq!(c.len(), 2);
    let mut placeholders = Vec::new();
    for (p, o) in c {
        assert_eq!(*p, 0.5);
        let Outcome::Pending { item, .. } = o else {
            panic!("{o:?}")
        };
        assert_eq!(item.rarity, Rarity::Rare);
        let explicits: Vec<&str> = item
            .explicits()
            .map(|a| d.mod_at(a.roll.m).id.as_str())
            .collect();
        assert_eq!(explicits.len(), 1);
        placeholders.push(explicits[0].to_string());
    }
    placeholders.sort();
    assert_eq!(placeholders, ["VeiledPrefix", "VeiledSuffix"]);

    let p = pattern("crown-shaper");
    let Lifted::Chance(lifted) = lift(d, &p, &out) else {
        panic!("not a Chance")
    };
    let mut open: Vec<[u8; 2]> = lifted
        .iter()
        .map(|(_, l)| match l {
            Lifted::Pending { known, .. } => known.first().expect("known states").1.open,
            other => panic!("{other:?}"),
        })
        .collect();
    open.sort();
    assert_eq!(open, [[2, 3], [3, 2]]);

    let l1 = item("crown-l1");
    let life = d.tag_by_name("life").unwrap();
    let out = run("harvest/reforge-life", &l1);
    let all = leaves(&out);
    assert!(close(all.iter().map(|(p, _)| p).sum(), 1.0, 1e-9));
    for (_, o) in all {
        let Outcome::Pending { item, .. } = o else {
            panic!("{o:?}")
        };
        assert_eq!(item.suffixes, l1.suffixes);
        assert_eq!(item.prefixes.len(), 1);
        assert!(d.mod_at(item.prefixes[0].roll.m).tags.contains(&life));
    }

    let out = run("currency/vaal", &item("crown-actual"));
    let c = chance(&out);
    assert_eq!(c.len(), 4);
    assert!(c.iter().all(|(p, _)| *p == 0.25));
    let kinds: Vec<&str> = c
        .iter()
        .map(|(_, o)| match o {
            Outcome::Item(i) => {
                assert!(i.corrupted);
                "item"
            }
            Outcome::Chance(_) => "implicits",
            Outcome::Pending {
                reason: PendingReason::Sockets,
                ..
            } => "sockets",
            Outcome::Pending {
                reason: PendingReason::MultiDraw,
                ..
            } => "reforge",
            other => panic!("{other:?}"),
        })
        .collect();
    let mut sorted = kinds.clone();
    sorted.sort();
    assert_eq!(sorted, ["implicits", "item", "reforge", "sockets"]);
}

#[test]
fn eldritch_ember() {
    let d = test_data();
    let out = run(
        "currency/lesser-eldritch-ember",
        &item("slink-boots-normal"),
    );
    let all = leaves(&out);
    assert!(close(all.iter().map(|(p, _)| p).sum(), 1.0, 1e-9));
    for (_, o) in all {
        let it = leaf_item(o);
        assert_eq!(it.implicits.len(), 1);
        let m = d.mod_at(it.implicits[0].m);
        assert_eq!((m.kind, m.eldritch_tier), (ModKind::Exarch, Some(1)));
    }
    assert_eq!(
        reason("currency/lesser-eldritch-ember", &item("crown-a")),
        "the item is influenced"
    );
}

#[test]
fn requirement_texts() {
    assert_eq!(
        reason("currency/exalted", &item("slink-boots-normal")),
        "needs a rare item"
    );
    let mut corrupted = item("crown-a");
    corrupted.corrupted = true;
    assert_eq!(
        reason("currency/exalted", &corrupted),
        "the item is corrupted"
    );
    assert_eq!(
        reason("unveil", &item("crown-a")),
        "needs a veiled modifier"
    );
    // crown-l2 has three prefixes (crown-c1 still has one open).
    assert_eq!(
        reason("beast/suffixes-cannot-be-changed", &item("crown-l2")),
        "needs an open prefix"
    );
}

#[test]
fn foresee_cleared() {
    let foreseen = leaf_item(&run("currency/hinekoras-lock", &item("crown-a"))).clone();
    assert!(foreseen.foreseen);
    for (_, o) in leaves(&run("currency/exalted", &foreseen)) {
        assert!(!leaf_item(o).foreseen);
    }
}

#[test]
fn imprint_restore() {
    let jewel = item("cobalt-jewel-magic");
    let imprinted = leaf_item(&run("beast/imprint", &jewel)).clone();
    for (_, o) in leaves(&run("currency/augmentation", &imprinted)) {
        let restored = run("beast/restore-imprint", leaf_item(o));
        assert_eq!(restored, Outcome::Item(jewel.clone()));
    }
}

#[test]
fn mirror() {
    let crown = item("crown-actual");
    let Outcome::Items(items) = run("currency/mirror", &crown) else {
        panic!("not Items")
    };
    assert_eq!(items.len(), 2);
    assert!(!items[0].mirrored);
    assert!(items[1].mirrored);
    assert_eq!(reason("currency/mirror", &items[1]), "the item is mirrored");
}

#[test]
fn pool_counts() {
    let d = test_data();
    let out = run("currency/exalted", &item("crown-a"));
    let c = chance(&out);
    assert_eq!(c.len(), 67);
    let mut life = Vec::new();
    for (p, o) in c {
        let it = leaf_item(o);
        assert_eq!(it.prefixes.len(), 1);
        let m = d.mod_at(it.prefixes[0].roll.m);
        assert!(!m.id.starts_with("AreaDamageSupported"), "{}", m.id);
        if m.groups.iter().any(|&g| d.group(g) == "IncreasedLife") {
            life.push(m.id.clone());
        }
        if m.id == "IncreasedLife9" {
            assert!(close(*p, 1000.0 / 55375.0, 1e-12));
            assert_eq!((m.stats[0].min, m.stats[0].max, m.level), (130, 144, 64));
        }
    }
    life.sort();
    let want: Vec<String> = (0..10).map(|i| format!("IncreasedLife{i}")).collect();
    assert_eq!(life, want);
    assert_eq!(
        chance(&run("currency/exalted", &item("crown-a-mana"))).len(),
        55
    );
}

#[test]
fn annul_cannot_roll() {
    let d = test_data();
    let mut s = spec("crown-a");
    s.prefixes = vec![AffixSpec {
        id: "IncreasedLife9".into(),
        values: Some(vec![137]),
        fractured: false,
    }];
    s.suffixes = vec![
        AffixSpec {
            id: "IncreasedAccuracyNew6".into(),
            values: Some(vec![500]),
            fractured: false,
        },
        AffixSpec {
            id: "IntMasterItemGenerationCannotRollAttackAffixes".into(),
            values: None,
            fractured: false,
        },
    ];
    let it = Item::from_spec(d, &s).unwrap();
    let accuracy = d.mod_by_id("IncreasedAccuracyNew6").unwrap();
    let has_accuracy = |i: &Item| i.suffixes.iter().any(|a| a.roll.m == accuracy);
    let out = run("currency/annulment", &it);
    let c = chance(&out);
    assert_eq!(c.len(), 2);
    for (p, o) in c {
        assert_eq!(*p, 0.5);
        assert!(has_accuracy(leaf_item(o)));
    }
    // Claim remove.cannot-roll-scope: the Veiled Exalted Orb's removal ignores the metamod.
    let out = run("currency/veiled-exalted", &it);
    let without: f64 = leaves(&out)
        .iter()
        .filter(|(_, o)| !has_accuracy(leaf_item(o)))
        .map(|(p, _)| p)
        .sum();
    assert!(close(without, 1.0 / 3.0, 1e-12), "{without}");
}

#[test]
fn catalyst_consumed() {
    let d = test_data();
    let mut s = spec("lapis-amulet-normal");
    s.quality = 20;
    s.catalyst = Some("Fertile Catalyst".into());
    let it = Item::from_spec(d, &s).unwrap();
    for (_, o) in leaves(&run("currency/transmutation", &it)) {
        let x = leaf_item(o);
        assert_eq!((x.catalyst, x.quality), (None, 0));
    }
    assert_eq!(
        reason("catalyst/fertile-catalyst", &it),
        "the item already has 20% Life and Mana quality"
    );
}

#[test]
fn defences_cleared() {
    let mut crown = item("crown-actual");
    crown.defences = Some([440, 0, 90, 0]);
    for (_, o) in leaves(&run("currency/annulment", &crown)) {
        assert_eq!(leaf_item(o).defences, None);
    }
    let Outcome::Items(items) = run("currency/mirror", &crown) else {
        panic!("not Items")
    };
    assert!(items.iter().all(|i| i.defences == Some([440, 0, 90, 0])));
}

#[test]
fn simplex_locks() {
    let simplex = item("simplex-normal");
    assert_eq!(
        reason("currency/blessed", &simplex),
        "the item's implicit modifiers cannot be changed"
    );
    let out = run("currency/vaal", &simplex);
    let unchanged: Vec<f64> = chance(&out)
        .iter()
        .filter(|(_, o)| matches!(o, Outcome::Item(i) if i.corrupted && i.implicits == simplex.implicits))
        .map(|(p, _)| *p)
        .collect();
    assert_eq!(unchanged, [0.5]);
}

/// Every Chance level's masses sum to 1.
fn masses_ok(o: &Outcome) -> bool {
    match o {
        Outcome::Chance(c) => close(o.mass(), 1.0, 1e-9) && c.iter().all(|(_, o)| masses_ok(o)),
        Outcome::Choice(c) => c.iter().all(masses_ok),
        _ => true,
    }
}

#[test]
fn every_method_runs() {
    let d = test_data();
    let fixtures: Vec<(String, Item)> = common::fixture_names()
        .into_iter()
        .map(|n| {
            let it = item(&n);
            (n, it)
        })
        .collect();
    let mut applied = 0;
    for m in &d.methods {
        for (name, it) in &fixtures {
            if let Ok(o) = apply(d, m, std::slice::from_ref(it), &Params::default()) {
                applied += 1;
                assert!(masses_ok(&o), "{} on {name}", m.id);
            }
        }
    }
    assert!(applied > 1000, "{applied}");
}

#[test]
fn dominance_elevates_top_tier() {
    let d = test_data();
    // crown-actual's influenced explicits: AoE support T1, Hypothermia T2, spell crit T3 (the top
    // tier below elevated).
    let out = run("currency/orb-of-dominance", &item("crown-actual"));
    let elevated = d
        .mod_by_id("SocketedSpellCriticalStrikeChanceUberMaven")
        .unwrap();
    let hits = leaves(&out)
        .iter()
        .filter(|(_, o)| leaf_item(o).suffixes.iter().any(|a| a.roll.m == elevated))
        .count();
    // Two of the six (removed, upgraded) pairs upgrade the spell crit suffix.
    assert_eq!(hits, 2);
}

#[test]
fn empty_tag_pool_is_unmet() {
    // Slink Boots roll no attack modifier.
    assert_eq!(
        reason(
            "harvest/add-attack-remove-random",
            &item("slink-boots-eldritch")
        ),
        "no attack modifier can roll on this item"
    );
}

#[test]
fn mirror_keeps_catalyst() {
    let d = test_data();
    let mut s = spec("lapis-amulet-normal");
    s.quality = 20;
    s.catalyst = Some("Fertile Catalyst".into());
    let it = Item::from_spec(d, &s).unwrap();
    let Outcome::Items(items) = run("currency/mirror", &it) else {
        panic!("not Items")
    };
    assert!(
        items
            .iter()
            .all(|i| i.catalyst == it.catalyst && i.quality == 20)
    );
}
