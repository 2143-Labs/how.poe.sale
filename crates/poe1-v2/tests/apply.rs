//! `apply`: exact single-draw outcomes against independent numbers, refusals, Pending results
//! and the property tests over every method and fixture.
//!
//! Expected numbers come from closed forms (1/n removals, 1/2 branches) or from an independent
//! recount over the pinned RePoE files, written in Python against `data/raw` (it reads no v2
//! code or bundle). The recount's helpers, which every recount below uses:
//!
//! ```python
//! import json
//! R = 'data/raw/'
//! mods = json.load(open(R + 'mods.json')); bases = json.load(open(R + 'base_items.json'))
//! classes = json.load(open(R + 'item_classes.json'))
//! SUFFIX = {'_shaper': 'shaper', '_elder': 'elder', '_crusader': 'crusader',
//!           '_basilisk': 'hunter', '_eyrie': 'redeemer', '_adjudicator': 'warlord'}
//! def base(name): return next(v for v in bases.values()
//!                             if v['name'] == name and v.get('release_state') == 'released')
//! def first(ws, tags, d): return next((w['weight'] for w in ws if w['tag'] in tags), d)
//! def weight(m, tags):   # first matching spawn entry × first matching generation multiplier
//!     return first(m['spawn_weights'], tags, 0) * first(m['generation_weights'], tags, 100) / 100
//! def influence(m): return next((i for w in m['spawn_weights'] if w['weight'] > 0
//!                                for s, i in SUFFIX.items() if w['tag'].endswith(s)), None)
//! def tags_of(b, infl=(), on=()):  # base tags, influence tags, every held mod's adds_tags
//!     order = ['crusader', 'elder', 'hunter', 'redeemer', 'shaper', 'warlord']
//!     t = set(b['tags']) | {classes[b['item_class']]['influence_tags'][order.index(i)] for i in infl}
//!     return t | {x for k in on for x in mods[k]['adds_tags']}
//! def groups(ks): return {g for k in ks for g in mods[k]['groups']}
//! def pool(domain, tags, ilvl, sides, taken=(), barred=(), infl=None, tag=None, cap=None):
//!     return {k: weight(m, tags) for k, m in mods.items()
//!             if m['domain'] == domain and m['generation_type'] in sides and 'Royale' not in k
//!             and not m['is_essence_only'] and m['required_level'] <= (cap or ilvl)
//!             and not set(m['groups']) & set(taken)
//!             and not any(t in m['implicit_tags'] for t in barred)
//!             and (infl is None or influence(m) == infl)
//!             and (tag is None or tag in m['implicit_tags']) and weight(m, tags) > 0}
//! crown = base('Archdemon Crown'); imp = ['SocketedActiveGemLevelSupportGemPenaltyImplicitR2']
//! suf = ['IncreasedChillEffectSupportedUber2', 'SocketedGemsDealMoreElementalDamageEssence1',
//!        'SocketedSpellCriticalStrikeChanceUber3']   # crown-a's suffixes
//! ```

mod common;

use common::{item, pattern, spec};
use poe1_v2::apply::{Leaf, Outcome, Params, Pending, apply, check, cost};
use poe1_v2::data::{Data, Influence, ModKind, Rarity};
use poe1_v2::item::{AffixSpec, Item, ItemSpec, ModSpec};
use poe1_v2::method::{Method, Op, PendingReason, Pool, Require};
use poe1_v2::pattern::{LiftedLeaf, lift};
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

fn reason_of(m: &Method, inputs: &[Item], params: &Params) -> String {
    match apply(test_data(), m, inputs, params) {
        Ok(o) => panic!("{} applied: {o:?}", m.id),
        Err(e) => {
            // check refuses with the same reason.
            assert_eq!(
                check(test_data(), m, inputs, params).unwrap_err(),
                e,
                "{}",
                m.id
            );
            e.reason
        }
    }
}

fn reason(id: &str, it: &Item) -> String {
    reason_of(method(id), std::slice::from_ref(it), &Params::default())
}

/// A method written in the test, as JSON in the rules' own shape.
fn custom(json: serde_json::Value) -> Method {
    let mut m = serde_json::json!({"id": "test/custom", "name": "test", "family": "currency"});
    m.as_object_mut()
        .unwrap()
        .extend(json.as_object().unwrap().clone());
    serde_json::from_value(m).expect("a valid method")
}

/// A fixture changed by `f`, read back through `from_spec`.
fn edited(name: &str, f: impl FnOnce(&mut ItemSpec)) -> Item {
    let mut s = spec(name);
    f(&mut s);
    Item::from_spec(test_data(), &s).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn affix(id: &str, values: Option<Vec<i32>>) -> AffixSpec {
    AffixSpec {
        id: id.into(),
        values,
        fractured: false,
    }
}

fn fractured(id: &str) -> AffixSpec {
    AffixSpec {
        id: id.into(),
        values: None,
        fractured: true,
    }
}

/// A rare item level 84 Slink Boots with these prefixes and suffixes.
fn boots(prefixes: Vec<AffixSpec>, suffixes: Vec<AffixSpec>) -> Item {
    edited("slink-boots-normal", |s| {
        s.rarity = Rarity::Rare;
        s.prefixes = prefixes;
        s.suffixes = suffixes;
    })
}

fn leaf_item(l: &Leaf) -> &Item {
    match l {
        Leaf::Item(i) => i,
        Leaf::Pending(p) => &p.item,
        other => panic!("not an item: {other:?}"),
    }
}

fn pending(l: &Leaf) -> &Pending {
    match l {
        Leaf::Pending(p) => p,
        other => panic!("not Pending: {other:?}"),
    }
}

fn ids(d: &Data, side: &[poe1_v2::item::Affix]) -> Vec<String> {
    side.iter().map(|a| d.mod_at(a.roll.m).id.clone()).collect()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn mass(o: &Outcome, f: impl Fn(&Leaf) -> bool) -> f64 {
    o.leaves.iter().filter(|(_, l)| f(l)).map(|(p, _)| p).sum()
}

/// The leaf whose item holds exactly these explicits (prefixes then suffixes).
fn find<'a>(d: &Data, o: &'a Outcome, explicits: &[&str]) -> (f64, &'a Item) {
    let mut want: Vec<&str> = explicits.to_vec();
    want.sort_unstable();
    o.leaves
        .iter()
        .filter_map(|(p, l)| match l {
            Leaf::Item(i) => Some((*p, i)),
            _ => None,
        })
        .find(|(_, i)| {
            let mut got: Vec<&str> = i
                .explicits()
                .map(|a| d.mod_at(a.roll.m).id.as_str())
                .collect();
            got.sort_unstable();
            got == want
        })
        .unwrap_or_else(|| panic!("no leaf with {explicits:?}"))
}

#[test]
fn veiled_exalt_halves() {
    let out = run("currency/veiled-exalted", &item("crown-l1"));
    assert_eq!(out.leaves.len(), 2);
    assert!(out.leaves.iter().all(|(p, _)| *p == 0.5));
    let mut got: Vec<&Item> = out.leaves.iter().map(|(_, l)| leaf_item(l)).collect();
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
    assert_eq!(out.leaves.len(), 3);
    let mut removed = Vec::new();
    for (p, l) in &out.leaves {
        assert!(close(*p, 1.0 / 3.0, 1e-12));
        let prefixes = ids(d, &leaf_item(l).prefixes);
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
    assert_eq!(out.leaves.len(), 3);
    for (p, l) in &out.leaves {
        assert!(close(*p, 1.0 / 3.0, 1e-12));
        assert_eq!(leaf_item(l).suffixes, input.suffixes);
    }
    let out = run("currency/annulment", &item("crown-d2-scbc-fractured"));
    assert_eq!(out.leaves.len(), 2);
    for (p, l) in &out.leaves {
        assert_eq!(*p, 0.5);
        assert!(ids(d, &leaf_item(l).prefixes).contains(&"IncreasedLife9".to_string()));
    }
}

/// Σ p of the offer sets that include the mod.
fn inclusion(d: &Data, out: &Outcome, id: &str) -> f64 {
    out.leaves
        .iter()
        .filter(|(_, l)| match l {
            Leaf::Choice(options) => options.iter().any(|o| {
                o.items()
                    .iter()
                    .any(|i| ids(d, &i.prefixes).contains(&id.to_string()))
            }),
            other => panic!("not a Choice: {other:?}"),
        })
        .map(|(p, _)| p)
        .sum()
}

#[test]
fn unveil_matches_owner_table() {
    let d = test_data();
    let out = run("unveil", &item("crown-c2"));
    assert_eq!(out.leaves.len(), 84);
    assert!(
        out.leaves
            .iter()
            .all(|(_, l)| matches!(l, Leaf::Choice(o) if o.len() == 3))
    );
    assert!(close(out.mass(), 1.0, 1e-12));
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
    assert_eq!(out.leaves.len(), 56);
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

/// E9: one offer never holds two options of one group. A rare Imperial Bow offers 7 veiled
/// prefixes, 4 of them in `LocalPhysicalDamagePercent`. Recount (sequential draws, each leaving
/// out the options that share a group with one already drawn):
///
/// ```python
/// bow = base('Imperial Bow'); t = tags_of(bow)
/// opts = {k: (first(m['spawn_weights'], t, 0), set(m['groups'])) for k, m in mods.items()
///         if m['domain'] == 'unveiled' and m['generation_type'] == 'prefix'
///         and m['required_level'] <= 84}
/// opts = {k: v for k, v in opts.items() if v[0] > 0}
/// def draws(drawn, p, sets):
///     elig = [k for k in opts if k not in drawn and not any(opts[k][1] & opts[d][1] for d in drawn)]
///     if len(drawn) == 3 or not elig:
///         key = tuple(sorted(drawn)); sets[key] = sets.get(key, 0) + p; return
///     W = sum(opts[k][0] for k in elig)
///     for k in elig: draws(drawn + [k], p * opts[k][0] / W, sets)
/// # 13 sets; inclusion: ChaosPenetration 0.8020172, each physical variant 0.2464857,
/// # minion damage 0.4100228
/// ```
#[test]
fn unveil_never_offers_one_group_twice() {
    let d = test_data();
    let bow = edited("slink-boots-normal", |s| {
        s.base = "Imperial Bow".into();
        s.rarity = Rarity::Rare;
        s.prefixes = vec![affix("VeiledPrefix", None)];
    });
    let out = run("unveil", &bow);
    assert_eq!(out.leaves.len(), 13);
    assert!(close(out.mass(), 1.0, 1e-12));
    for (_, l) in &out.leaves {
        let Leaf::Choice(options) = l else {
            panic!("not a Choice: {l:?}")
        };
        let mut seen = std::collections::BTreeSet::new();
        for o in options {
            for g in &d.mod_at(o.item().unwrap().prefixes[0].roll.m).groups {
                assert!(seen.insert(*g), "two options of one group");
            }
        }
    }
    for (id, want) in [
        ("JunMasterVeiledChaosPenetrationWithAttacks__", 0.8020172),
        (
            "JunMasterVeiledLocalIncreasedPhysicalDamageAndImpaleCrafted",
            0.2464857,
        ),
        (
            "JunMasterVeiledMinionDamageAndMinionMaximumLife2h__",
            0.4100228,
        ),
    ] {
        let got = inclusion(d, &out, id);
        assert!(close(got, want, 1e-6), "{id}: {got}");
    }
}

/// Σ p of lifted states whose prefixes hold the class.
fn lifted_mass(l: &poe1_v2::pattern::Lifted, class: u16) -> f64 {
    l.leaves
        .iter()
        .map(|(p, leaf)| match leaf {
            LiftedLeaf::State(s) if s.prefixes.iter().any(|a| a.class == class) => *p,
            LiftedLeaf::State(_) => 0.0,
            other => panic!("unexpected {other:?}"),
        })
        .sum()
}

#[test]
fn exalt_lifts_onto_pattern() {
    let d = test_data();
    let input = item("crown-a-mana");
    let out = run("currency/exalted", &input);
    for (_, l) in &out.leaves {
        let it = leaf_item(l);
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

fn mana_craft() -> &'static Method {
    bench_by_ops(&[Op::AddMod {
        r#mod: "EinharMasterIncreasedMana2".into(),
    }])
}

#[test]
fn bench_and_limits() {
    let d = test_data();
    let out = apply(d, mana_craft(), &[item("crown-a")], &Params::default()).unwrap();
    let mut want = spec("crown-a-mana");
    want.prefixes[0].values = None;
    assert_eq!(out.item(), Some(&Item::from_spec(d, &want).unwrap()));
    assert_eq!(
        reason_of(mana_craft(), &[item("crown-a-mana")], &Params::default()),
        "the item already has as many crafted modifiers as it can hold"
    );
}

/// Q3: a bench craft on a normal item turns it magic.
#[test]
fn bench_on_normal_turns_magic() {
    let d = test_data();
    let normal = edited("crown-a", |s| {
        s.rarity = Rarity::Normal;
        s.suffixes.clear();
    });
    let out = apply(d, mana_craft(), &[normal], &Params::default()).unwrap();
    let it = out.item().unwrap();
    assert_eq!(it.rarity, Rarity::Magic);
    assert_eq!(ids(d, &it.prefixes), ["EinharMasterIncreasedMana2"]);
    it.validate(d).unwrap();
}

#[test]
fn scour_with_lock() {
    assert_eq!(
        run("currency/scouring", &item("crown-l1")).item(),
        Some(&item("crown-a"))
    );
}

/// R7 (vhpg, fractured items): one fractured modifier left makes the item magic, two or more
/// rare, none normal; Tainted Chaos's remove-all branch keeps fractured modifiers too.
#[test]
fn scour_rarity_follows_fractured() {
    let d = test_data();
    let one = boots(
        vec![fractured("MovementVelocity5")],
        vec![affix("ColdResist5", None)],
    );
    let out = run("currency/scouring", &one);
    let it = out.item().unwrap();
    assert_eq!(it.rarity, Rarity::Magic);
    assert_eq!(ids(d, &it.prefixes), ["MovementVelocity5"]);
    assert!(it.suffixes.is_empty());

    let two = boots(
        vec![fractured("MovementVelocity5")],
        vec![fractured("ColdResist5"), affix("FireResist5", None)],
    );
    let it = run("currency/scouring", &two);
    assert_eq!(it.item().unwrap().rarity, Rarity::Rare);
    assert_eq!(it.item().unwrap().explicits().count(), 2);

    let none = boots(Vec::new(), vec![affix("ColdResist5", None)]);
    assert_eq!(
        run("currency/scouring", &none).item().unwrap().rarity,
        Rarity::Normal
    );
    // Q1: nothing it can remove refuses the orb.
    let only = boots(vec![fractured("MovementVelocity5")], Vec::new());
    assert_eq!(
        reason("currency/scouring", &only),
        "no modifier can be removed"
    );

    let mut corrupted = one.clone();
    corrupted.corrupted = true;
    let out = run("currency/tainted-chaos-orb", &corrupted);
    let cleared = out
        .leaves
        .iter()
        .find_map(|(p, l)| match l {
            Leaf::Item(i) => Some((*p, i)),
            _ => None,
        })
        .expect("the remove-all branch");
    assert_eq!(cleared.0, 0.5);
    assert_eq!(ids(d, &cleared.1.prefixes), ["MovementVelocity5"]);
    assert_eq!(cleared.1.rarity, Rarity::Magic);
}

#[test]
fn remove_crafted() {
    let d = test_data();
    let m = bench_by_ops(&[Op::RemoveCrafted]);
    let out = apply(d, m, &[item("crown-c2-blocked")], &Params::default()).unwrap();
    assert_eq!(ids(d, &out.item().unwrap().prefixes), ["VeiledPrefix"]);
}

/// A1/E3: one item reached through different draw orders is one leaf. Recount:
///
/// ```python
/// jewel = base('Cobalt Jewel'); t = tags_of(jewel)
/// P = pool('misc', t, 84, ['prefix']); S = pool('misc', t, 84, ['suffix'])   # 47, 61
/// pairs = [(a, s) for a in P for s in S if not groups([a]) & groups([s])]     # 2858
/// # distinct items 47 + 61 + 2858 = 2966; a one-mod item: 0.5 · w / W, W = ΣP + ΣS
/// W = sum(P.values()) + sum(S.values())
/// print(0.5 * P['BlockDualWieldingJewel'] / W)                                # 0.004714439655172414
/// a, s = 'WandDamageJewel', 'ProjectileDamageJewel'                           # prefix then suffix, or back
/// Sa = sum(pool('misc', tags_of(jewel, on=[a]), 84, ['suffix'], taken=groups([a])).values())
/// Pb = sum(pool('misc', tags_of(jewel, on=[s]), 84, ['prefix'], taken=groups([s])).values())
/// print(0.5 * (P[a] / W * S[s] / Sa + S[s] / W * P[a] / Pb))                  # 0.0003035982305456436
/// ```
#[test]
fn transmute_distinct_items() {
    let d = test_data();
    let out = run("currency/transmutation", &item("cobalt-jewel-normal"));
    assert_eq!(out.leaves.len(), 2966);
    assert!(close(out.mass(), 1.0, 1e-9));
    let one = mass(&out, |l| leaf_item(l).explicits().count() == 1);
    assert!(close(one, 0.5, 1e-12), "{one}");
    let (p, it) = find(d, &out, &["BlockDualWieldingJewel"]);
    assert!(close(p, 0.004714439655172414, 1e-15), "{p}");
    assert_eq!(it.rarity, Rarity::Magic);
    let (p, _) = find(d, &out, &["WandDamageJewel", "ProjectileDamageJewel"]);
    assert!(close(p, 0.0003035982305456436, 1e-15), "{p}");
}

/// R1: an Orb of Alteration keeps the side a metamod locks. A magic Shaper Crown with Suffixes
/// Cannot Be Changed (a prefix) and Hypothermia (a suffix): the clear removes the metamod and
/// keeps the suffix, and the one or two adds both end in one new prefix (a magic item holds one).
/// Recount: `pool('item', tags_of(crown, ['shaper'], imp + [hyp]), 87, ['prefix'],
/// taken=groups([hyp]))` with `hyp = 'IncreasedChillEffectSupportedUber2'`: 67 prefixes, weight
/// 55375.
#[test]
fn alteration_keeps_locked_side() {
    let d = test_data();
    let magic = edited("crown-a", |s| {
        s.rarity = Rarity::Magic;
        s.suffixes.truncate(1);
        s.prefixes = vec![affix("DexMasterItemGenerationCannotChangeSuffixes", None)];
    });
    let out = run("currency/alteration", &magic);
    assert_eq!(out.leaves.len(), 67);
    let life = d.mod_by_id("IncreasedLife9").unwrap();
    for (p, l) in &out.leaves {
        let it = leaf_item(l);
        assert_eq!(it.suffixes, magic.suffixes);
        assert_eq!(it.prefixes.len(), 1);
        if it.prefixes[0].roll.m == life {
            assert!(close(*p, 1000.0 / 55375.0, 1e-12), "{p}");
        }
    }
}

#[test]
fn pending_and_vaal() {
    let d = test_data();
    // E2: the certain part of a Chaos Orb is the rare item with what it keeps.
    let crown_a = item("crown-a");
    let out = run("currency/chaos", &crown_a);
    let [(certain, Leaf::Pending(p))] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*certain, 1.0);
    assert_eq!(p.reason, PendingReason::MultiDraw);
    assert!(p.certain);
    assert_eq!(p.inputs, std::slice::from_ref(&crown_a));
    assert!(matches!(p.ops[0], Op::Fill { .. }));
    let mut cleared = crown_a.clone();
    cleared.suffixes.clear();
    assert_eq!(p.item, cleared);
    // R1: Suffixes Cannot Be Changed keeps the three suffixes through a Chaos Orb.
    let l1 = item("crown-l1");
    let out = run("currency/chaos", &l1);
    let p = pending(&out.leaves[0].1);
    assert_eq!(p.item.suffixes, l1.suffixes);
    assert!(p.item.prefixes.is_empty());

    let slink = item("slink-boots-normal");
    let essence = d
        .essences
        .iter()
        .find(|e| e.name == "Deafening Essence of Hatred")
        .unwrap();
    let boots_mod = essence
        .mods
        .iter()
        .find(|(c, _)| *c == d.base_at(slink.base).class)
        .unwrap()
        .1;
    let out = run("essence/deafening-essence-of-hatred", &slink);
    let p = pending(&out.leaves[0].1);
    assert_eq!(out.leaves.len(), 1);
    assert_eq!(p.reason, PendingReason::MultiDraw);
    assert_eq!(p.item.rarity, Rarity::Rare);
    let explicits: Vec<u32> = p.item.explicits().map(|a| a.roll.m).collect();
    assert_eq!(explicits, [boots_mod]);
    assert!(matches!(p.ops[0], Op::Fill { .. }));

    let out = run("currency/veiled-chaos", &crown_a);
    assert_eq!(out.leaves.len(), 2);
    let mut placeholders = Vec::new();
    for (p, l) in &out.leaves {
        assert_eq!(*p, 0.5);
        let it = &pending(l).item;
        assert_eq!(it.rarity, Rarity::Rare);
        let explicits: Vec<&str> = it
            .explicits()
            .map(|a| d.mod_at(a.roll.m).id.as_str())
            .collect();
        assert_eq!(explicits.len(), 1);
        placeholders.push(explicits[0].to_string());
    }
    placeholders.sort();
    assert_eq!(placeholders, ["VeiledPrefix", "VeiledSuffix"]);
    let p = pattern("crown-shaper");
    let mut open: Vec<[u8; 2]> = lift(d, &p, &out)
        .leaves
        .iter()
        .map(|(_, l)| match l {
            LiftedLeaf::Pending { known: Some(s), .. } => s.open,
            other => panic!("{other:?}"),
        })
        .collect();
    open.sort();
    assert_eq!(open, [[2, 3], [3, 2]]);
    // R1 example: on crown-l1 the three locked suffixes stay, so the veiled mod is a prefix.
    let out = run("currency/veiled-chaos", &l1);
    let [(one, Leaf::Pending(p))] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*one, 1.0);
    assert_eq!(ids(d, &p.item.prefixes), ["VeiledPrefix"]);
    assert_eq!(p.item.suffixes, l1.suffixes);

    // Q7: the tag reforge clears what can change (SCBC itself is a prefix) and fills until at
    // least one life modifier is drawn, so no prefix is placed before the Fill.
    let life = d.tag_by_name("life").unwrap();
    let out = run("harvest/reforge-life", &l1);
    assert!(close(out.mass(), 1.0, 1e-9));
    for (_, l) in &out.leaves {
        let pend = pending(l);
        assert_eq!(pend.item.suffixes, l1.suffixes);
        assert!(pend.item.prefixes.is_empty());
        let Op::Fill { fill } = &pend.ops[0] else {
            panic!("{:?}", pend.ops);
        };
        assert_eq!(fill.include, Some(Pool::Tag { tag: "life".into() }));
        assert_eq!(d.tag_by_name("life"), Some(life));
    }

    // Vaal Orb: nothing, one implicit replaced, sockets, or a reforge as rare, a quarter each.
    let actual = item("crown-actual");
    let out = run("currency/vaal", &actual);
    assert!(close(out.mass(), 1.0, 1e-12));
    let unchanged = mass(
        &out,
        |l| matches!(l, Leaf::Item(i) if i.corrupted && i.implicits == actual.implicits),
    );
    let implicit = mass(
        &out,
        |l| matches!(l, Leaf::Item(i) if i.corrupted && i.implicits != actual.implicits),
    );
    let sockets = mass(
        &out,
        |l| matches!(l, Leaf::Pending(p) if p.reason == PendingReason::Sockets && !p.certain),
    );
    let reforge = mass(&out, |l| {
        matches!(l, Leaf::Pending(p) if p.reason == PendingReason::MultiDraw && p.certain
            && p.item.explicits().count() == 0 && p.item.corrupted)
    });
    for got in [unchanged, implicit, sockets, reforge] {
        assert!(close(got, 0.25, 1e-12), "{got}");
    }
}

/// E2: a step the model does not run stops at the item before it, and nothing about the result
/// is known.
#[test]
fn unmodelled_is_before_step() {
    let d = test_data();
    let slink = item("slink-boots-normal");
    let out = run("currency/chance", &slink);
    let p = pending(&out.leaves[0].1);
    assert!(!p.certain);
    assert_eq!(p.item, slink);
    let lifted = lift(d, &pattern("crown-shaper"), &out);
    assert!(matches!(
        lifted.leaves.as_slice(),
        [(_, LiftedLeaf::Pending { known: None, .. })]
    ));
    // Q6: Hinekora's Lock is not modelled yet.
    for id in ["currency/hinekoras-lock", "beast/hinekoras-lock"] {
        let it = item("cobalt-jewel-magic");
        let out = run(id, &it);
        let p = pending(&out.leaves[0].1);
        assert!(
            matches!(p.reason, PendingReason::NotModelled { .. }),
            "{id}"
        );
        assert_eq!(p.item, it);
    }
}

/// E4: Pending keeps every input and the params; the Awakener's Orb reforges its second input.
#[test]
fn pending_holds_inputs_and_params() {
    let d = test_data();
    let crown_a = item("crown-a");
    let elder = edited("crown-a", |s| s.influences = vec![Influence::Elder]);
    let out = apply(
        d,
        method("currency/awakeners-orb"),
        &[elder.clone(), crown_a.clone()],
        &Params::default(),
    )
    .unwrap();
    let [(one, Leaf::Pending(p))] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*one, 1.0);
    assert_eq!(p.inputs, [elder.clone(), crown_a.clone()]);
    assert!(p.certain);
    assert_eq!(
        p.item.influences,
        Influence::Shaper.bit() | Influence::Elder.bit()
    );
    assert_eq!(p.item.base, crown_a.base);
    assert_eq!(p.item.explicits().count(), 0);
    let l1 = item("crown-l1");
    let out = apply(
        d,
        method("currency/armour-recombinator"),
        &[crown_a.clone(), l1.clone()],
        &Params::default(),
    )
    .unwrap();
    let p = pending(&out.leaves[0].1);
    assert_eq!(p.inputs, [crown_a.clone(), l1]);
    assert!(!p.certain, "a recombine keeps nothing certain");
    let params = Params {
        fossils: vec![0, 1],
    };
    let out = apply(
        d,
        method("fossil/chaotic-resonator"),
        std::slice::from_ref(&crown_a),
        &params,
    )
    .unwrap();
    let p = pending(&out.leaves[0].1);
    assert_eq!(p.params, params);
    assert!(p.certain);
}

/// Resonators come from the method's data: the stackable currency for the fossil count.
#[test]
fn fossil_cost() {
    let d = test_data();
    let m = method("fossil/chaotic-resonator");
    let params = Params {
        fossils: vec![0, 1],
    };
    let c = cost(d, m, &params).unwrap();
    assert_eq!(
        c[0].key,
        "Metadata/Items/Delve/DelveStackableSocketableCurrencyReroll2"
    );
    assert_eq!(c[1].key, d.fossils[0].currency);
    assert_eq!(c[2].key, d.fossils[1].currency);
    let twice = Params {
        fossils: vec![3, 3],
    };
    assert_eq!(
        cost(d, m, &twice).unwrap_err().reason,
        format!("{} is given twice", d.fossils[3].name)
    );
    assert_eq!(
        reason_of(m, &[item("crown-a")], &twice),
        format!("{} is given twice", d.fossils[3].name)
    );
    assert_eq!(
        reason_of(
            method("currency/chaos"),
            &[item("crown-a")],
            &Params { fossils: vec![0] }
        ),
        "this method takes no fossils"
    );
    // R1: fossils cannot be used on an item with a lock or a Cannot Roll metamod.
    assert_eq!(
        reason_of(m, &[item("crown-l1")], &Params { fossils: vec![0] }),
        "cannot be used on an item with \"Suffixes Cannot Be Changed\""
    );
    let chaos = cost(d, method("currency/chaos"), &Params::default()).unwrap();
    assert_eq!(chaos[0].key, "Metadata/Items/Currency/CurrencyRerollRare");
}

/// R9: embers have no item level requirement. Recount: tier 1 Searing Exarch implicits with
/// spawn on Slink Boots: 27, weight 15400, every one of required level 75.
#[test]
fn eldritch_ember() {
    let d = test_data();
    for level in [84, 60] {
        let slink = edited("slink-boots-normal", |s| s.item_level = level);
        let out = run("currency/lesser-eldritch-ember", &slink);
        assert_eq!(out.leaves.len(), 27, "item level {level}");
        assert!(close(out.mass(), 1.0, 1e-9));
        for (_, l) in &out.leaves {
            let it = leaf_item(l);
            assert_eq!(it.implicits.len(), 1);
            let m = d.mod_at(it.implicits[0].m);
            assert_eq!((m.kind, m.eldritch_tier), (ModKind::Exarch, Some(1)));
        }
    }
    assert_eq!(
        reason("currency/lesser-eldritch-ember", &item("crown-a")),
        "the item is influenced"
    );
}

/// R5: a Vaal Orb replaces one random implicit. Recount: corrupted implicits (domain item,
/// level ≤ 84) with spawn on the eldritch boots' tags: 25, weight 21500; each of the two
/// implicits is replaced with 1/2, so 50 results share the quarter.
#[test]
fn vaal_replaces_one_implicit() {
    let eldritch = item("slink-boots-eldritch");
    let out = run("currency/vaal", &eldritch);
    let replaced: Vec<(f64, &Item)> = out
        .leaves
        .iter()
        .filter_map(|(p, l)| match l {
            Leaf::Item(i) if i.implicits != eldritch.implicits => Some((*p, i)),
            _ => None,
        })
        .collect();
    assert_eq!(replaced.len(), 50);
    let total: f64 = replaced.iter().map(|(p, _)| p).sum();
    assert!(close(total, 0.25, 1e-12), "{total}");
    for (_, it) in &replaced {
        assert_eq!(it.implicits.len(), 2);
        let kept = it
            .implicits
            .iter()
            .filter(|r| eldritch.implicits.contains(r))
            .count();
        assert_eq!(kept, 1, "exactly one implicit replaced");
    }
}

#[test]
fn foresight_ends_with_any_change() {
    let mut foreseen = item("crown-a-mana");
    foreseen.foreseen = true;
    for (_, l) in &run("currency/exalted", &foreseen).leaves {
        assert!(!leaf_item(l).foreseen);
    }
}

#[test]
fn imprint_restore() {
    let d = test_data();
    let jewel = item("cobalt-jewel-magic");
    let imprinted = run("beast/imprint", &jewel).item().unwrap().clone();
    for (_, l) in &run("currency/augmentation", &imprinted).leaves {
        let restored = run("beast/restore-imprint", leaf_item(l));
        assert_eq!(restored.item(), Some(&jewel));
    }
    // R10: the imprint comes back with its own defence figures; a corrupted or mirrored item
    // cannot be restored, and a mirrored copy carries no imprint.
    let magic = edited("crown-a", |s| {
        s.rarity = Rarity::Magic;
        s.suffixes.truncate(1);
        s.defences = Some([440, 0, 90, 0]);
    });
    let imprinted = run("beast/imprint", &magic).item().unwrap().clone();
    let changed = run("currency/augmentation", &imprinted);
    let after = leaf_item(&changed.leaves[0].1);
    assert_eq!(after.defences, None, "a new prefix makes the figures stale");
    assert_eq!(
        run("beast/restore-imprint", after).item(),
        Some(&magic),
        "restored with its defence figures"
    );
    let mut corrupted = after.clone();
    corrupted.corrupted = true;
    assert_eq!(
        reason("beast/restore-imprint", &corrupted),
        "the item is corrupted"
    );
    let mut mirrored = after.clone();
    mirrored.mirrored = true;
    assert_eq!(
        reason("beast/restore-imprint", &mirrored),
        "the item is mirrored"
    );
    let out = apply(
        d,
        method("currency/mirror"),
        &[edited("crown-a", |s| {
            s.imprint = Some(Box::new(spec("crown-a")))
        })],
        &Params::default(),
    )
    .unwrap();
    let [(_, Leaf::Items(items))] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert!(items[0].imprint.is_some());
    assert!(items[1].imprint.is_none());
}

#[test]
fn mirror() {
    let crown = item("crown-actual");
    let out = run("currency/mirror", &crown);
    let [(one, Leaf::Items(items))] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*one, 1.0);
    assert_eq!(items.len(), 2);
    assert!(!items[0].mirrored);
    assert!(items[1].mirrored);
    assert_eq!(reason("currency/mirror", &items[1]), "the item is mirrored");
}

#[test]
fn pool_counts() {
    let d = test_data();
    let out = run("currency/exalted", &item("crown-a"));
    assert_eq!(out.leaves.len(), 67);
    let mut life = Vec::new();
    for (p, l) in &out.leaves {
        let it = leaf_item(l);
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
        run("currency/exalted", &item("crown-a-mana")).leaves.len(),
        55
    );
}

/// The add filters. Recounts:
/// - Cannot Roll Caster: crown-a with its spell crit suffix replaced by the metamod:
///   `pool(..., taken=groups(two + [metamod]), barred=['caster'])`: 61 prefixes, weight 52975.
/// - Level 28: the same with `PvPMasterLevel28Crafting` and `cap=28`: 19, weight 18250.
/// - Cannot Roll Attack: rare Titan Gauntlets with ColdResist1, FireResist1 and the metamod:
///   33 prefixes, weight 33000 (55 and 46000 without the filter).
#[test]
fn metamod_filters_on_adds() {
    let d = test_data();
    for (suffix, n, w) in [
        (
            "StrDexMasterItemGenerationCannotRollCasterAffixes",
            61,
            52975.0,
        ),
        ("PvPMasterLevel28Crafting", 19, 18250.0),
    ] {
        let it = edited("crown-a", |s| s.suffixes[2] = affix(suffix, None));
        let out = run("currency/exalted", &it);
        assert_eq!(out.leaves.len(), n, "{suffix}");
        // IncreasedLife1 (level 5) weighs 1000 on the Crown through its `default` entry.
        let life = d.mod_by_id("IncreasedLife1").unwrap();
        let (p, _) = out
            .leaves
            .iter()
            .find(|(_, l)| leaf_item(l).prefixes[0].roll.m == life)
            .expect("T10 life is in both pools");
        assert!(close(*p, 1000.0 / w, 1e-12), "{suffix}: {p}");
    }
    let gloves = edited("slink-boots-normal", |s| {
        s.base = "Titan Gauntlets".into();
        s.rarity = Rarity::Rare;
        s.suffixes = vec![
            affix("ColdResist1", None),
            affix("FireResist1", None),
            affix("IntMasterItemGenerationCannotRollAttackAffixes", None),
        ];
    });
    let out = run("currency/exalted", &gloves);
    assert_eq!(out.leaves.len(), 33);
    let attack = d.tag_by_name("attack").unwrap();
    for (p, l) in &out.leaves {
        let m = d.mod_at(leaf_item(l).prefixes[0].roll.m);
        assert!(!m.tags.contains(&attack), "{}", m.id);
        assert!(close(*p, 1000.0 / 33000.0, 1e-12));
    }
}

#[test]
fn annul_cannot_roll() {
    let d = test_data();
    let it = edited("crown-a", |s| {
        s.prefixes = vec![affix("IncreasedLife9", Some(vec![137]))];
        s.suffixes = vec![
            affix("IncreasedAccuracyNew6", Some(vec![500])),
            affix("IntMasterItemGenerationCannotRollAttackAffixes", None),
        ];
    });
    let accuracy = d.mod_by_id("IncreasedAccuracyNew6").unwrap();
    let has_accuracy = |i: &Item| i.suffixes.iter().any(|a| a.roll.m == accuracy);
    let out = run("currency/annulment", &it);
    assert_eq!(out.leaves.len(), 2);
    for (p, l) in &out.leaves {
        assert_eq!(*p, 0.5);
        assert!(has_accuracy(leaf_item(l)));
    }
    // Claim remove.cannot-roll-scope: the Veiled Exalted Orb's removal ignores the metamod.
    let out = run("currency/veiled-exalted", &it);
    let without = mass(&out, |l| !has_accuracy(leaf_item(l)));
    assert!(close(without, 1.0 / 3.0, 1e-12), "{without}");
}

/// R2: crafting has not removed jewellery quality since 3.15.
#[test]
fn catalyst_kept() {
    let it = edited("lapis-amulet-normal", |s| {
        s.quality = 20;
        s.catalyst = Some("Fertile Catalyst".into());
    });
    for (_, l) in &run("currency/transmutation", &it).leaves {
        let x = leaf_item(l);
        assert_eq!((x.catalyst, x.quality), (it.catalyst, 20));
    }
    assert_eq!(
        reason("catalyst/fertile-catalyst", &it),
        "the item already has 20% Life and Mana quality"
    );
    let out = run("currency/mirror", &it);
    let [(_, Leaf::Items(items))] = out.leaves.as_slice() else {
        panic!("not Items")
    };
    assert!(
        items
            .iter()
            .all(|i| i.catalyst == it.catalyst && i.quality == 20)
    );
}

#[test]
fn defences_cleared() {
    let mut crown = item("crown-actual");
    crown.defences = Some([440, 0, 90, 0]);
    for (_, l) in &run("currency/annulment", &crown).leaves {
        assert_eq!(leaf_item(l).defences, None);
    }
    let out = run("currency/mirror", &crown);
    let [(_, Leaf::Items(items))] = out.leaves.as_slice() else {
        panic!("not Items")
    };
    assert!(items.iter().all(|i| i.defences == Some([440, 0, 90, 0])));
    // Corruption alone feeds no defence: the figures stay.
    let out = run("currency/vaal", &crown);
    assert!(out.leaves.iter().any(
        |(_, l)| matches!(l, Leaf::Item(i) if i.implicits == crown.implicits && i.defences.is_some())
    ));
}

#[test]
fn simplex_locks() {
    let simplex = item("simplex-normal");
    assert_eq!(
        reason("currency/blessed", &simplex),
        "the item's implicit modifiers cannot be changed"
    );
    // The implicit outcome changes nothing and merges with the no-change outcome (2 of 3).
    let out = run("currency/vaal-socketless", &simplex);
    let unchanged = mass(
        &out,
        |l| matches!(l, Leaf::Item(i) if i.corrupted && i.implicits == simplex.implicits),
    );
    assert!(close(unchanged, 2.0 / 3.0, 1e-12));
}

#[test]
fn dominance_elevates_top_tier() {
    let d = test_data();
    // crown-actual's influenced explicits: AoE support T1, Hypothermia T2, spell crit T3 (the top
    // tier below elevated): six ordered (removed, upgraded) pairs at 1/6.
    let out = run("currency/orb-of-dominance", &item("crown-actual"));
    assert_eq!(out.leaves.len(), 6);
    assert!(out.leaves.iter().all(|(p, _)| close(*p, 1.0 / 6.0, 1e-12)));
    let elevated = d
        .mod_by_id("SocketedSpellCriticalStrikeChanceUberMaven")
        .unwrap();
    let hits = out
        .leaves
        .iter()
        .filter(|(_, l)| leaf_item(l).suffixes.iter().any(|a| a.roll.m == elevated))
        .count();
    // Two of the six pairs upgrade the spell crit suffix.
    assert_eq!(hits, 2);
}

/// R1/E7: the Orb of Dominance leaves a locked side alone. Two influenced prefixes beside
/// Suffixes Cannot Be Changed: the pairs are among the prefixes only (1/2 each), and the three
/// locked Shaper suffixes stay.
#[test]
fn dominance_skips_locked_sides() {
    let d = test_data();
    let it = edited("crown-a", |s| {
        s.prefixes = vec![
            affix("SpellAddedColdDamageUber1", None),
            affix("ChanceToGainPowerChargeOnKillUber1", None),
            affix("DexMasterItemGenerationCannotChangeSuffixes", None),
        ];
    });
    let out = run("currency/orb-of-dominance", &it);
    assert_eq!(out.leaves.len(), 2);
    let mut upgraded = Vec::new();
    for (p, l) in &out.leaves {
        assert_eq!(*p, 0.5);
        let x = leaf_item(l);
        assert_eq!(x.suffixes, it.suffixes);
        upgraded.extend(
            ids(d, &x.prefixes)
                .into_iter()
                .filter(|id| id.ends_with("Uber2")),
        );
    }
    upgraded.sort();
    assert_eq!(
        upgraded,
        [
            "ChanceToGainPowerChargeOnKillUber2",
            "SpellAddedColdDamageUber2"
        ]
    );
    assert_eq!(
        reason("currency/orb-of-dominance", &item("crown-d2-scbc")),
        "needs at least 2 influenced modifiers that can change"
    );
}

/// E1: an op with nothing to do outside a Chance branch refuses the method, and `check` says
/// so too.
#[test]
fn empty_candidates_refuse() {
    // RemoveRandom: only a fractured prefix.
    let jewel = edited("cobalt-jewel-magic", |s| s.prefixes[0].fractured = true);
    assert_eq!(
        reason("currency/annulment", &jewel),
        "no modifier can be removed"
    );
    // Eldritch Annulment with nothing on the dominant (prefix) side.
    let eldritch = edited("slink-boots-eldritch", |s| {
        s.suffixes = vec![affix("ColdResist5", None)];
    });
    assert_eq!(
        reason("currency/eldritch-annulment", &eldritch),
        "no prefix can be removed"
    );
    // Fracture with only a veiled modifier (a method written here: the orb's own requirements
    // stop earlier).
    let fracture = custom(serde_json::json!({"ops": [{"op": "fracture"}]}));
    let veiled = edited("crown-a", |s| {
        s.suffixes.clear();
        s.prefixes = vec![affix("VeiledPrefix", None)];
    });
    assert_eq!(
        reason_of(&fracture, &[veiled], &Params::default()),
        "no modifier can be fractured"
    );
    // Dominance: no influenced modifier can change.
    assert_eq!(
        reason("currency/orb-of-dominance", &item("crown-d2-scbc")),
        "needs at least 2 influenced modifiers that can change"
    );
    // The Influence pool: every Shaper modifier is level 68 or more.
    let low = edited("crown-a", |s| {
        s.influences.clear();
        s.item_level = 67;
    });
    assert_eq!(
        reason("currency/shapers-exalted", &low),
        "no Shaper modifier can roll on this item"
    );
    // The eldritch pool.
    let tier7 = custom(serde_json::json!({
        "ops": [{"op": "eldritchImplicit", "eldritch": "exarch", "tier": 7}]
    }));
    assert_eq!(
        reason_of(&tier7, &[item("slink-boots-normal")], &Params::default()),
        "no tier 7 Searing Exarch implicit can roll on this item"
    );
    // An unplaceable guaranteed mod: the essence's cold resistance meets a fractured one.
    let cold = boots(Vec::new(), vec![fractured("ColdResist5")]);
    assert_eq!(
        reason("essence/deafening-essence-of-hatred", &cold),
        "the item already has a ColdResistance modifier"
    );
    // A tag pool with nothing in it, and the removal after an add with nothing else to remove.
    assert_eq!(
        reason(
            "harvest/add-attack-remove-random",
            &item("slink-boots-eldritch")
        ),
        "no attack modifier can roll on this item"
    );
    assert_eq!(
        reason(
            "harvest/add-life-remove-random",
            &item("slink-boots-eldritch")
        ),
        "no modifier can be removed"
    );
    // Blessed Orb on fixed implicits (R12) and the Sacred Orb's op on a base with no defences.
    assert_eq!(
        reason("currency/blessed", &item("crown-a")),
        "no implicit modifier has values to reroll"
    );
    let sacred = custom(serde_json::json!({
        "ops": [{"op": "rerollValues", "scope": "defences"}]
    }));
    assert_eq!(
        reason_of(&sacred, &[item("cobalt-jewel-normal")], &Params::default()),
        "the base has no defence values to reroll"
    );
    // Inside a Chance branch an empty op leaves its item: the Tainted Exalted Orb on a full
    // item adds nothing half the time.
    let mut full = item("crown-actual");
    full.corrupted = true;
    let out = run("currency/tainted-exalted-orb", &full);
    assert!(close(mass(&out, |l| leaf_item(l) == &full), 0.5, 1e-12));
}

/// beast.farric: with the suffixes locked nothing is removed and a prefix is still added.
/// Recounts: on crown-l1, `pool(..., taken=groups(l1))`: 57 prefixes, weight 45375; on
/// crown-a-mana each of the three suffixes goes with 1/3 and 55 prefixes (weight 43375) can
/// follow each, 165 results.
#[test]
fn farric_respects_locks_and_adds() {
    let d = test_data();
    let l1 = item("crown-l1");
    let out = run("beast/suffix-to-prefix", &l1);
    assert_eq!(out.leaves.len(), 57);
    assert!(close(out.mass(), 1.0, 1e-12));
    for (_, l) in &out.leaves {
        let it = leaf_item(l);
        assert_eq!(it.suffixes, l1.suffixes);
        assert_eq!(it.prefixes.len(), 3);
    }
    let mana = item("crown-a-mana");
    let out = run("beast/suffix-to-prefix", &mana);
    assert_eq!(out.leaves.len(), 165);
    let life9 = d.mod_by_id("IncreasedLife9").unwrap();
    let with_life: f64 = mass(&out, |l| {
        leaf_item(l).prefixes.iter().any(|a| a.roll.m == life9)
    });
    assert!(close(with_life, 1000.0 / 43375.0, 1e-12), "{with_life}");
    for (_, l) in &out.leaves {
        assert_eq!(leaf_item(l).suffixes.len(), 2);
    }
}

/// keepLastAdded: the add-remove craft never removes the mod it added. Rare Slink Boots with
/// MovementVelocity5 and ColdResist5; recount `pool('item', tags_of(boots, on=have), 84,
/// ['prefix', 'suffix'], taken=groups(have), tag='life')`: 19 mods, weight 19000, each then
/// followed by removing one of the two old mods with 1/2.
#[test]
fn add_remove_keeps_added() {
    let d = test_data();
    let it = boots(
        vec![affix("MovementVelocity5", None)],
        vec![affix("ColdResist5", None)],
    );
    let out = run("harvest/add-life-remove-random", &it);
    assert_eq!(out.leaves.len(), 38);
    let life = d.tag_by_name("life").unwrap();
    let old: Vec<u32> = it.explicits().map(|a| a.roll.m).collect();
    for (p, l) in &out.leaves {
        let x = leaf_item(l);
        assert_eq!(x.explicits().count(), 2);
        let kept_old = x.explicits().filter(|a| old.contains(&a.roll.m)).count();
        assert_eq!(kept_old, 1);
        let added = x.explicits().find(|a| !old.contains(&a.roll.m)).unwrap();
        assert!(d.mod_at(added.roll.m).tags.contains(&life));
        assert!(close(*p, 1000.0 / 19000.0 / 2.0, 1e-12), "{p}");
    }
}

/// A resistance change keeps the rank: FireResist5 is the fourth highest fire tier on Slink
/// Boots, and ColdResist5 the fourth cold one (recount: FireResistance / ColdResistance mods
/// with spawn on the boots, by required level descending). A locked side cannot be changed.
#[test]
fn change_resistance() {
    let d = test_data();
    let it = boots(Vec::new(), vec![affix("FireResist5", None)]);
    let out = run("harvest/fire-to-cold", &it);
    assert_eq!(ids(d, &out.item().unwrap().suffixes), ["ColdResist5"]);
    let locked = boots(
        vec![affix("DexMasterItemGenerationCannotChangeSuffixes", None)],
        vec![affix("FireResist5", None)],
    );
    assert_eq!(
        reason("harvest/fire-to-cold", &locked),
        "no FireResistance modifier can be changed"
    );
}

/// Side Dominant: Exarch tier 2 outranks Eater tier 1, so prefixes. Recount: natural prefixes on
/// the eldritch boots' tags at item level 84: 38, weight 38000.
#[test]
fn eldritch_exalt_dominant_side() {
    let out = run("currency/eldritch-exalted", &item("slink-boots-eldritch"));
    assert_eq!(out.leaves.len(), 38);
    for (p, l) in &out.leaves {
        let it = leaf_item(l);
        assert_eq!((it.prefixes.len(), it.suffixes.len()), (1, 0));
        assert!(close(*p, 1.0 / 38.0, 1e-12));
    }
}

/// R16: every ducat is an Allflame crafting ingredient, so its preview table is not modelled.
#[test]
fn ducat_pool() {
    let boots = item("slink-boots-eldritch");
    let out = run("currency/brinehooks-ducat", &boots);
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    let pend = pending(l);
    assert!(matches!(pend.reason, PendingReason::NeedsData { .. }));
    assert_eq!(pend.item, boots);
    assert!(!pend.certain);
}

/// R16: Merrick's Ducat is an Allflame ingredient too, so nothing adds past the limits in this
/// pass and the method no longer claims to.
#[test]
fn merricks_ducat_ignores_limits() {
    let jewel = edited("slink-boots-normal", |s| {
        s.base = "Murderous Eye Jewel".into();
        s.rarity = Rarity::Rare;
        s.prefixes = vec![
            affix("AbyssAddedChaosDamageWithClawsJewel1", None),
            affix("AbyssAddedColdDamageWithAxesJewel1", None),
        ];
        s.suffixes = vec![
            affix("AbyssAccuracyRatingJewel2", None),
            affix("AbyssAddedChaosSuffixJewel1", None),
        ];
    });
    let m = method("currency/merricks-ducat");
    assert!(!m.ignores_limits());
    let out = run("currency/merricks-ducat", &jewel);
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    assert!(matches!(pending(l).reason, PendingReason::NeedsData { .. }));
    assert_eq!(pending(l).item.explicits().count(), 4);
}

/// AddInfluence and the Influence pool. Recount on an uninfluenced rare Archdemon Crown with
/// crown-a's suffixes: Shaper prefixes `pool(..., taken=groups(suf), infl='shaper')`: 11, weight
/// 5600 (the Increased Area of Effect support tiers 800 each).
#[test]
fn influence_exalt() {
    let d = test_data();
    let plain = edited("crown-a", |s| s.influences.clear());
    let out = run("currency/shapers-exalted", &plain);
    assert_eq!(out.leaves.len(), 11);
    let aoe = d.mod_by_id("AreaOfEffectSupportedUber1_").unwrap();
    for (p, l) in &out.leaves {
        let it = leaf_item(l);
        assert!(it.has_influence(Influence::Shaper));
        assert_eq!(it.prefixes.len(), 1);
        assert_eq!(
            d.mod_at(it.prefixes[0].roll.m).influence,
            Some(Influence::Shaper)
        );
        if it.prefixes[0].roll.m == aoe {
            assert!(close(*p, 800.0 / 5600.0, 1e-12));
        }
    }
}

/// Q7: the influenced reforge clears what can change and then fills until at least one
/// influenced modifier is drawn; it does not place a guaranteed first one.
#[test]
fn influenced_reforge_places_one_influenced_mod() {
    let out = run("harvest/reforge-influenced", &item("crown-a"));
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    let pend = pending(l);
    assert_eq!(pend.reason, PendingReason::MultiDraw);
    assert!(pend.certain);
    assert_eq!(pend.item.explicits().count(), 0);
    assert_eq!(pend.item.rarity, Rarity::Rare);
    let Op::Fill { fill } = &pend.ops[0] else {
        panic!("{:?}", pend.ops);
    };
    assert_eq!(fill.include, Some(Pool::Influenced));
}

#[test]
fn reroll_values() {
    let d = test_data();
    let actual = item("crown-actual");
    let it = run("currency/divine", &actual).item().unwrap().clone();
    for a in it.explicits() {
        let ranged = d.mod_at(a.roll.m).stats.iter().any(|s| s.min != s.max);
        assert_eq!(a.roll.values.is_none(), ranged, "{}", d.mod_at(a.roll.m).id);
    }
    // R1: a locked side keeps its values.
    let l1 = item("crown-l1");
    let it = run("currency/divine", &l1).item().unwrap().clone();
    assert_eq!(it.suffixes, l1.suffixes);
    let life = d.mod_by_id("IncreasedLife9").unwrap();
    let rolled = it.prefixes.iter().find(|a| a.roll.m == life).unwrap();
    assert_eq!(rolled.roll.values, None);
    let lapis = edited("lapis-amulet-normal", |s| {
        s.implicits = vec![ModSpec {
            id: "IntelligenceImplicitAmulet1".into(),
            values: Some(vec![25]),
        }]
    });
    let it = run("currency/blessed", &lapis).item().unwrap().clone();
    assert_eq!(it.implicits[0].values, None);
    let mut crown = item("crown-a");
    crown.defences = Some([200, 0, 40, 0]);
    assert_eq!(
        run("currency/sacred", &crown).item().unwrap().defences,
        None
    );
}

/// R3: since 3.25 the quality a scrap adds depends on item level, so the op is not modelled; a
/// unique item is a legal target too, and only the amount it receives is unknown (claim
/// `quality.unique`).
#[test]
fn add_quality() {
    for rarity in [Rarity::Normal, Rarity::Magic, Rarity::Rare, Rarity::Unique] {
        let it = edited("slink-boots-normal", |s| s.rarity = rarity);
        let out = run("currency/armourers-scrap", &it);
        let [(p, l)] = out.leaves.as_slice() else {
            panic!("{out:?}")
        };
        assert_eq!(*p, 1.0);
        let pend = pending(l);
        assert!(matches!(pend.reason, PendingReason::NeedsData { .. }));
        assert_eq!(pend.item, it);
    }
    // A 20%-quality unique is still refused: the cap is refused by `qualityBelow`.
    let unique = edited("slink-boots-unique", |s| s.quality = 20);
    assert_eq!(
        reason("currency/armourers-scrap", &unique),
        "quality is already 20%"
    );
}

/// R3: the catalyst amount depends on item level since 3.25, so the op is NeedsData; the
/// catalyst-room refusal still applies at 20% of the same catalyst.
#[test]
fn catalyst_quality() {
    let d = test_data();
    let lapis = item("lapis-amulet-normal");
    let out = run("catalyst/fertile-catalyst", &lapis);
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    let pend = pending(l);
    assert!(matches!(pend.reason, PendingReason::NeedsData { .. }));
    assert_eq!(pend.item, lapis);
    let c = d.catalyst_by_name("Fertile Catalyst").unwrap();
    let label = d.catalysts[c as usize].label.clone();
    let full = edited("lapis-amulet-normal", |s| {
        s.quality = 20;
        s.catalyst = Some("Fertile Catalyst".into());
    });
    assert_eq!(
        reason("catalyst/fertile-catalyst", &full),
        format!("the item already has 20% {label} quality")
    );
}

/// Recount for the Labyrinth enchant on crown-a: enchantment mods (domain item) with spawn on
/// its tags, level ≤ 87: 1338, weight 129659.
#[test]
fn enchant_ops() {
    let d = test_data();
    let out = run("currency/enchant", &item("crown-a"));
    assert_eq!(out.leaves.len(), 1338);
    assert!(close(out.mass(), 1.0, 1e-9));
    assert!(
        out.leaves
            .iter()
            .all(|(_, l)| leaf_item(l).enchants.len() == 1)
    );
    let flask = item("quicksilver-normal");
    let injector = d
        .methods
        .iter()
        .find(|m| matches!(m.ops.as_slice(), [Op::SetEnchantMod { .. }]))
        .unwrap();
    let Op::SetEnchantMod { r#mod } = &injector.ops[0] else {
        unreachable!()
    };
    let enchanted = apply(
        d,
        injector,
        std::slice::from_ref(&flask),
        &Params::default(),
    )
    .unwrap()
    .item()
    .unwrap()
    .clone();
    assert_eq!(
        enchanted.enchants.iter().map(|r| r.m).collect::<Vec<_>>(),
        [d.mod_by_id(r#mod).unwrap()]
    );
    let remove = bench_by_ops(&[Op::ClearEnchants]);
    let cleared = apply(d, remove, &[enchanted], &Params::default()).unwrap();
    assert!(cleared.item().unwrap().enchants.is_empty());
    assert_eq!(
        reason_of(remove, &[flask], &Params::default()),
        "needs an enchantment"
    );
}

#[test]
fn link_all() {
    let split = edited("crown-actual", |s| s.sockets = "B-B B".into());
    let out = run("beast/maximum-links", &split);
    assert_eq!(out.item().unwrap().sockets.len(), 1);
    assert_eq!(out.item().unwrap().sockets[0].len(), 3);
    assert_eq!(
        reason("beast/maximum-links", &item("crown-actual")),
        "every socket is already linked"
    );
}

/// R16: Tzamoto's Ducat is an Allflame ingredient, so its 50/50 destroy-or-clear is not
/// modelled; the item is unchanged in the Pending result.
#[test]
fn destroy_and_intangibility() {
    let crown = item("crown-actual");
    assert_eq!(crown.intangibility, 97);
    let out = run("currency/tzamotos-ducat", &crown);
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    let pend = pending(l);
    assert!(matches!(pend.reason, PendingReason::NeedsData { .. }));
    assert_eq!(pend.item, crown);
}

/// Q4: crafted modifiers can be fractured; the orb refuses influenced items (its game text).
#[test]
fn fracture_takes_crafted() {
    let d = test_data();
    let it = boots(
        vec![
            affix("MovementVelocity5", None),
            affix("EinharMasterIncreasedMana2", None),
        ],
        vec![affix("ColdResist5", None), affix("FireResist5", None)],
    );
    let out = run("currency/fracturing", &it);
    assert_eq!(out.leaves.len(), 4);
    let mana = d.mod_by_id("EinharMasterIncreasedMana2").unwrap();
    for (p, l) in &out.leaves {
        assert_eq!(*p, 0.25);
        assert_eq!(leaf_item(l).explicits().filter(|a| a.fractured).count(), 1);
    }
    assert!(out.leaves.iter().any(|(_, l)| {
        leaf_item(l)
            .prefixes
            .iter()
            .any(|a| a.fractured && a.roll.m == mana)
    }));
    assert_eq!(
        reason("currency/fracturing", &item("crown-actual")),
        "the item is influenced"
    );
}

/// R1: essences and fossils are forbidden by the locks and the Cannot Roll metamods, not by
/// the multi-craft metamod alone (owner, Q9).
#[test]
fn essences_forbidden_by_metamods() {
    let hatred = "essence/deafening-essence-of-hatred";
    assert_eq!(
        reason(hatred, &item("crown-l1")),
        "cannot be used on an item with \"Suffixes Cannot Be Changed\""
    );
    let caster = edited("crown-a", |s| {
        s.suffixes[2] = affix("StrDexMasterItemGenerationCannotRollCasterAffixes", None)
    });
    assert_eq!(
        reason(hatred, &caster),
        "cannot be used on an item with \"Cannot roll Caster Modifiers\""
    );
    let multi = edited("crown-a", |s| {
        s.suffixes.pop();
        s.suffixes.push(affix(
            "StrIntMasterItemGenerationCanHaveMultipleCraftedMods",
            None,
        ));
    });
    assert!(apply(test_data(), method(hatred), &[multi], &Params::default()).is_ok());
}

/// R4: an essence's level restriction caps its modifiers, not the item.
#[test]
fn essence_level_caps_mods() {
    let out = run(
        "essence/wailing-essence-of-hatred",
        &item("slink-boots-normal"),
    );
    let p = pending(&out.leaves[0].1);
    let Some(Op::Fill { fill }) = p.ops.first() else {
        panic!("{:?}", p.ops)
    };
    assert_eq!(fill.mod_level_max, Some(75));
}

/// Every Require reason, each from an item that fails it, through a method that has only that
/// requirement.
#[test]
fn requirement_texts() {
    let d = test_data();
    let crown_a = item("crown-a");
    let lapis_full = edited("lapis-amulet-normal", |s| {
        s.quality = 20;
        s.catalyst = Some("Fertile Catalyst".into());
    });
    let mut corrupted = crown_a.clone();
    corrupted.corrupted = true;
    let mut mirrored = crown_a.clone();
    mirrored.mirrored = true;
    let mut split = crown_a.clone();
    split.split = true;
    let mut synthesised = crown_a.clone();
    synthesised.synthesised = true;
    let mut unique = crown_a.clone();
    unique.rarity = Rarity::Unique;
    let mut foreseen = crown_a.clone();
    foreseen.foreseen = true;
    let enchanted = edited("crown-a", |s| {
        s.enchants = vec![ModSpec {
            id: d.mod_at(d.mods_of_kind(ModKind::Enchant)[0]).id.clone(),
            values: None,
        }];
    });
    let jewel = item("cobalt-jewel-normal");
    let slink = item("slink-boots-normal");
    let jewel_essence = d
        .essences
        .iter()
        .find(|e| !e.mods.is_empty())
        .unwrap()
        .name
        .clone();
    let cases: Vec<(serde_json::Value, Item, String)> = vec![
        (
            serde_json::json!({"kind": "rarity", "anyOf": ["rare"]}),
            slink.clone(),
            "needs a rare item".into(),
        ),
        (
            serde_json::json!({"kind": "rarity", "anyOf": ["magic", "rare"]}),
            slink.clone(),
            "needs a magic or rare item".into(),
        ),
        (
            serde_json::json!({"kind": "category", "anyOf": ["weapon"]}),
            crown_a.clone(),
            "only works on: weapons".into(),
        ),
        (
            serde_json::json!({"kind": "class", "anyOf": ["Ring", "Amulet"]}),
            crown_a.clone(),
            "only works on: Rings, Amulets".into(),
        ),
        (
            serde_json::json!({"kind": "notCorrupted"}),
            corrupted.clone(),
            "the item is corrupted".into(),
        ),
        (
            serde_json::json!({"kind": "corrupted"}),
            crown_a.clone(),
            "needs a corrupted item".into(),
        ),
        (
            serde_json::json!({"kind": "notMirrored"}),
            mirrored.clone(),
            "the item is mirrored".into(),
        ),
        (
            serde_json::json!({"kind": "notSplit"}),
            split.clone(),
            "the item is split".into(),
        ),
        (
            serde_json::json!({"kind": "notSynthesised"}),
            synthesised.clone(),
            "the item is synthesised".into(),
        ),
        (
            serde_json::json!({"kind": "notUnique"}),
            unique.clone(),
            "the item is unique".into(),
        ),
        (
            serde_json::json!({"kind": "noFractured"}),
            item("crown-d2-scbc-fractured"),
            "the item has a fractured modifier".into(),
        ),
        (
            serde_json::json!({"kind": "noEnchant"}),
            enchanted.clone(),
            "the item is enchanted".into(),
        ),
        (
            serde_json::json!({"kind": "noInfluence"}),
            crown_a.clone(),
            "the item is influenced".into(),
        ),
        (
            serde_json::json!({"kind": "hasInfluence"}),
            slink.clone(),
            "needs an influenced item".into(),
        ),
        (
            serde_json::json!({"kind": "hasInfluence", "influence": "elder"}),
            crown_a.clone(),
            "needs a Elder item".into(),
        ),
        (
            serde_json::json!({"kind": "singleInfluence"}),
            item("crown-shaper-elder-ideal"),
            "needs exactly one influence".into(),
        ),
        (
            serde_json::json!({"kind": "influenceable"}),
            jewel.clone(),
            "this item class cannot be influenced".into(),
        ),
        (
            serde_json::json!({"kind": "openAffix", "side": "any"}),
            item("crown-actual"),
            "needs an open affix".into(),
        ),
        (
            serde_json::json!({"kind": "openAffix", "side": "prefix"}),
            item("crown-actual"),
            "needs an open prefix".into(),
        ),
        (
            serde_json::json!({"kind": "openAffix", "side": "suffix"}),
            crown_a.clone(),
            "needs an open suffix".into(),
        ),
        (
            serde_json::json!({"kind": "openAffix", "side": "dominant"}),
            crown_a.clone(),
            "neither eldritch implicit is dominant".into(),
        ),
        (
            serde_json::json!({"kind": "fullSuffixes"}),
            slink.clone(),
            "needs full suffixes".into(),
        ),
        (
            serde_json::json!({"kind": "minExplicits", "n": 4}),
            crown_a.clone(),
            "needs at least 4 modifiers".into(),
        ),
        (
            serde_json::json!({"kind": "minInfluencedMods", "n": 4}),
            crown_a.clone(),
            "needs at least 4 influenced modifiers".into(),
        ),
        (
            serde_json::json!({"kind": "hasImplicit"}),
            slink.clone(),
            "needs an implicit modifier".into(),
        ),
        (
            serde_json::json!({"kind": "hasCrafted"}),
            crown_a.clone(),
            "needs a crafted modifier".into(),
        ),
        (
            serde_json::json!({"kind": "hasEnchant"}),
            crown_a.clone(),
            "needs an enchantment".into(),
        ),
        (
            serde_json::json!({"kind": "craftedRoom"}),
            item("crown-a-mana"),
            "the item already has as many crafted modifiers as it can hold".into(),
        ),
        (
            serde_json::json!({"kind": "groupFree", "mod": "IncreasedLife8"}),
            item("crown-l1"),
            "the item already has a IncreasedLife modifier".into(),
        ),
        (
            serde_json::json!({"kind": "hasVeiled"}),
            crown_a.clone(),
            "needs a veiled modifier".into(),
        ),
        (
            serde_json::json!({"kind": "noVeiled"}),
            item("crown-c1"),
            "the item already has a veiled modifier".into(),
        ),
        (
            serde_json::json!({"kind": "qualityBelow", "max": 20}),
            crown_a.clone(),
            "quality is already 20%".into(),
        ),
        (
            serde_json::json!({"kind": "catalystRoom", "catalyst": "Fertile Catalyst"}),
            lapis_full.clone(),
            "the item already has 20% Life and Mana quality".into(),
        ),
        (
            serde_json::json!({"kind": "hasImprint"}),
            crown_a.clone(),
            "the item has no imprint".into(),
        ),
        (
            serde_json::json!({"kind": "notForeseen"}),
            foreseen.clone(),
            "the item already foresees its next result".into(),
        ),
        (
            serde_json::json!({"kind": "eldritchDominant"}),
            crown_a.clone(),
            "neither eldritch implicit is dominant".into(),
        ),
        (
            serde_json::json!({"kind": "bothEldritch"}),
            slink.clone(),
            "needs both a Searing Exarch and an Eater of Worlds implicit".into(),
        ),
        (
            serde_json::json!({"kind": "noEldritch"}),
            item("slink-boots-eldritch"),
            "the item already has an eldritch implicit".into(),
        ),
        (
            serde_json::json!({"kind": "hasSockets"}),
            crown_a.clone(),
            "the item has no sockets".into(),
        ),
        (
            serde_json::json!({"kind": "canHaveSockets"}),
            jewel.clone(),
            "this item class has no sockets".into(),
        ),
        (
            serde_json::json!({"kind": "intangible"}),
            crown_a.clone(),
            "the item is not intangible".into(),
        ),
        (
            serde_json::json!({"kind": "hasMemoryStrands"}),
            crown_a.clone(),
            "the item has no Memory Strands".into(),
        ),
        (
            serde_json::json!({"kind": "hasModType", "anyOf": ["FireResistance"]}),
            crown_a.clone(),
            "needs a FireResistance modifier".into(),
        ),
        (
            serde_json::json!({"kind": "essenceFits", "essence": jewel_essence}),
            jewel.clone(),
            format!("{jewel_essence} has no modifier for Jewel"),
        ),
        (
            serde_json::json!({"kind": "fossils", "min": 1, "max": 4}),
            crown_a.clone(),
            "needs 1 to 4 fossils".into(),
        ),
        (
            serde_json::json!({"kind": "implicitsChangeable"}),
            item("simplex-normal"),
            "the item's implicit modifiers cannot be changed".into(),
        ),
    ];
    let mut kinds = std::collections::HashSet::new();
    for (r, it, want) in cases {
        let req: Require = serde_json::from_value(r.clone()).unwrap();
        kinds.insert(std::mem::discriminant(&req));
        let m = custom(serde_json::json!({"requires": [r], "ops": [{"op": "corrupt"}]}));
        assert_eq!(
            reason_of(&m, std::slice::from_ref(&it), &Params::default()),
            want
        );
    }
    // One case per Require variant.
    assert_eq!(kinds.len(), 41);
    assert_eq!(
        reason_of(
            method("currency/awakeners-orb"),
            &[crown_a],
            &Params::default()
        ),
        "needs 2 items"
    );
}

/// Both pair rules, on real pairs.
#[test]
fn pair_rules() {
    let crown_a = item("crown-a");
    let boots = item("slink-boots-eldritch");
    assert_eq!(
        reason_of(
            method("currency/armour-recombinator"),
            &[crown_a.clone(), boots],
            &Params::default()
        ),
        "both items must be the same item class"
    );
    assert_eq!(
        reason_of(
            method("currency/awakeners-orb"),
            &[crown_a.clone(), crown_a.clone()],
            &Params::default()
        ),
        "each item needs one influence, and they must differ"
    );
}

/// E9: the size guard stops every loop and reports the whole method as too large to compute,
/// stopped at the input.
#[test]
fn size_guard_stops() {
    let add = serde_json::json!({"op": "addRandom", "pool": {"kind": "natural"}, "side": "any"});
    let m = custom(serde_json::json!({
        "ops": [{"op": "setRarity", "rarity": "rare"}, add, add, add, add]
    }));
    let jewel = item("cobalt-jewel-normal");
    let out = apply(
        test_data(),
        &m,
        std::slice::from_ref(&jewel),
        &Params::default(),
    )
    .unwrap();
    let [(one, Leaf::Pending(p))] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*one, 1.0);
    assert_eq!(p.reason, PendingReason::TooLarge);
    assert!(!p.certain);
    assert_eq!(p.item, jewel);
    assert_eq!(p.ops, m.ops);
    assert!(check(test_data(), &m, &[jewel], &Params::default()).is_ok());
}

/// A method that does not validate is refused with the validation's reason.
#[test]
fn invalid_methods_are_refused() {
    for (ops, want) in [
        (
            serde_json::json!([{"op": "addMod", "mod": "NoSuchMod"}]),
            "unknown mod NoSuchMod",
        ),
        (
            serde_json::json!([{"op": "chance", "branches": [{"p": 0.5, "ops": []}]}]),
            "Chance branches sum to 0.5, not 1",
        ),
        (
            serde_json::json!([{"op": "scour"}]),
            "changes existing modifiers but states no protection policy",
        ),
        (
            serde_json::json!([{"op": "mirror"}, {"op": "corrupt"}]),
            "an op follows a Mirror or a Destroy, which end the method",
        ),
        (
            serde_json::json!([{"op": "fill", "fill": {"counts": [[5, 1], [4, 1]]}}]),
            "Fill counts must rise, each total once",
        ),
    ] {
        let m = custom(serde_json::json!({ "ops": ops }));
        assert_eq!(
            reason_of(&m, &[item("crown-a")], &Params::default()),
            format!("method test/custom is invalid: {want}")
        );
    }
}

/// Every leaf item an outcome holds, choice options included, with whether it may exceed the
/// affix limits.
fn every_item<'a>(o: &'a Outcome, out: &mut Vec<&'a Item>) {
    for (_, l) in &o.leaves {
        match l {
            Leaf::Choice(options) => {
                for opt in options {
                    assert!(close(opt.mass(), 1.0, 1e-9));
                    every_item(opt, out);
                }
            }
            other => out.extend(other.items()),
        }
    }
}

/// The properties every outcome keeps: `check` agrees with `apply`, the masses sum to 1, the
/// leaves are distinct, every item is valid (over the limits only for a method that adds past
/// them), and a Pending holds every input.
fn properties(d: &Data, m: &Method, inputs: &[Item], params: &Params, what: &str) -> bool {
    let applied = apply(d, m, inputs, params);
    let checked = check(d, m, inputs, params);
    assert_eq!(
        applied.as_ref().err(),
        checked.as_ref().err(),
        "{} on {what}",
        m.id
    );
    let Ok(o) = applied else { return false };
    assert!(close(o.mass(), 1.0, 1e-9), "{} on {what}", m.id);
    let mut sorted: Vec<&Leaf> = o.leaves.iter().map(|(_, l)| l).collect();
    sorted.sort_by(|a, b| a.order(b));
    for w in sorted.windows(2) {
        assert!(
            w[0].order(w[1]).is_ne(),
            "{} on {what}: repeated leaf",
            m.id
        );
    }
    let mut items = Vec::new();
    every_item(&o, &mut items);
    for it in items {
        let valid = if m.ignores_limits() {
            it.validate_over_limit(d)
        } else {
            it.validate(d)
        };
        assert!(valid.is_ok(), "{} on {what}: {valid:?}", m.id);
    }
    for (_, l) in &o.leaves {
        if let Leaf::Pending(p) = l {
            assert_eq!(p.inputs, inputs, "{} on {what}", m.id);
        }
    }
    true
}

#[test]
fn every_method_keeps_its_properties() {
    let d = test_data();
    let fixtures: Vec<(String, Item)> = common::fixture_names()
        .into_iter()
        .map(|n| {
            let it = item(&n);
            (n, it)
        })
        .collect();
    // Pairs for the two-input methods: every fixture pair, an Elder Crown beside each fixture
    // (Awakener's Orb), and two magic bows and two magic amulets (the weapon and jewellery
    // recombinators).
    let elder = edited("crown-a", |s| s.influences = vec![Influence::Elder]);
    let bow = edited("slink-boots-normal", |s| {
        s.base = "Imperial Bow".into();
        s.rarity = Rarity::Magic;
    });
    let amulet = edited("lapis-amulet-normal", |s| s.rarity = Rarity::Magic);
    let mut pairs: Vec<(String, [Item; 2])> = vec![
        ("two bows".into(), [bow.clone(), bow]),
        ("two amulets".into(), [amulet.clone(), amulet]),
    ];
    for (a, x) in &fixtures {
        for (b, y) in &fixtures {
            pairs.push((format!("{a} + {b}"), [x.clone(), y.clone()]));
        }
        pairs.push((format!("elder crown + {a}"), [elder.clone(), x.clone()]));
    }
    let mut applied = 0;
    let mut two_input = std::collections::BTreeSet::new();
    for m in &d.methods {
        let params = if m
            .requires
            .iter()
            .any(|r| matches!(r, Require::Fossils { .. }))
        {
            Params {
                fossils: vec![0, 1],
            }
        } else {
            Params::default()
        };
        if m.inputs == 2 {
            for (name, inputs) in &pairs {
                if properties(d, m, inputs, &params, name) {
                    two_input.insert(m.id.clone());
                    applied += 1;
                }
            }
        } else {
            for (name, it) in &fixtures {
                if properties(d, m, std::slice::from_ref(it), &params, name) {
                    applied += 1;
                }
            }
        }
    }
    assert!(applied > 1000, "{applied}");
    // Every two-input method ran on a real pair.
    let wanted: Vec<&str> = d
        .methods
        .iter()
        .filter(|m| m.inputs == 2)
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(
        two_input.iter().map(String::as_str).collect::<Vec<_>>(),
        wanted
    );
    // The fossil methods ran with fossils.
    for id in ["fossil/chaotic-resonator", "fossil/alchemical-resonator"] {
        let m = method(id);
        let it = if id.contains("chaotic") {
            item("crown-a")
        } else {
            item("slink-boots-normal")
        };
        assert!(properties(
            d,
            m,
            &[it],
            &Params {
                fossils: vec![0, 1]
            },
            "fossils"
        ));
    }
}

/// R3: catalysts are not modelled either; since 3.25 the quality they add depends on item level.
#[test]
fn catalyst_quality_is_needs_data() {
    let amulet = edited("lapis-amulet-normal", |s| s.quality = 20);
    let out = run("catalyst/fertile-catalyst", &amulet);
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    let pend = pending(l);
    assert!(matches!(pend.reason, PendingReason::NeedsData { .. }));
    assert_eq!(pend.item, amulet);
}

/// R9: the Eldritch Orb of Annulment takes a magic or rare item (its directions).
#[test]
fn eldritch_annulment_takes_magic() {
    let normal = edited("slink-boots-eldritch", |s| s.rarity = Rarity::Normal);
    assert_eq!(
        reason("currency/eldritch-annulment", &normal),
        "needs a magic or rare item"
    );
    let magic = edited("slink-boots-eldritch", |s| s.rarity = Rarity::Magic);
    assert_ne!(
        reason("currency/eldritch-annulment", &magic),
        "needs a magic or rare item"
    );
}

/// R14: the bench offers socket and link crafts on corrupted items, never above the class
/// maximum, and a link craft needs that many sockets.
#[test]
fn bench_sockets_on_corrupted_and_limits() {
    let corrupted = edited("crown-actual", |s| s.corrupted = true);
    let out = run("bench/sockets/4", &corrupted);
    let [(p, l)] = out.leaves.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(*p, 1.0);
    assert_eq!(pending(l).reason, PendingReason::Sockets);
    // Six sockets is not a helmet option at all; four links needs four sockets.
    assert!(reason("bench/sockets/6", &item("crown-actual")).starts_with("only works on:"));
    let short = edited("crown-actual", |s| s.sockets = "B-B".into());
    assert_eq!(reason("bench/links/4", &short), "needs at least 4 sockets");
    // A corrupted item still refuses the modifier crafts.
    assert_eq!(
        reason("bench/add/EinharMasterIncreasedMana2", &corrupted),
        "the item is corrupted"
    );
}

/// R18/3.29: a Fractured Fossil refuses influenced and fractured items.
#[test]
fn fractured_fossil_limits() {
    let d = test_data();
    let index = d
        .fossils
        .iter()
        .position(|f| f.name == "Fractured Fossil")
        .unwrap() as u16;
    let params = Params {
        fossils: vec![index],
    };
    let boots = edited("slink-boots-normal", |s| s.rarity = Rarity::Rare);
    assert!(
        apply(
            d,
            method("fossil/chaotic-resonator"),
            std::slice::from_ref(&boots),
            &params
        )
        .is_ok()
    );
    for input in [item("crown-a"), item("crown-d2-scbc-fractured")] {
        assert_eq!(
            reason_of(
                method("fossil/chaotic-resonator"),
                std::slice::from_ref(&input),
                &params
            ),
            "Fractured Fossil cannot be used on an influenced or fractured item"
        );
    }
}

/// Q7: a tag reforge refuses when no modifier carrying the tag can roll on the item. On a
/// helmet no speed modifier spawns (recount over `data/raw/mods.json`: 0 of the 11 Harvest tags
/// is speed there).
#[test]
fn harvest_tag_reforge_refuses_when_no_tag_can_roll() {
    assert_eq!(
        reason("harvest/reforge-speed", &item("crown-a")),
        "no speed modifier can roll on this item"
    );
}

/// R5: Vaal Orb splits by item class, and uniques are no longer refused outright.
#[test]
fn vaal_tables() {
    let unique = edited("lapis-amulet-normal", |s| s.rarity = Rarity::Unique);
    assert_eq!(
        reason(
            "currency/vaal",
            &edited("crown-actual", |s| s.rarity = Rarity::Unique)
        ),
        "the item is unique"
    );
    let out = run("currency/vaal-unique", &unique);
    let pend = pending(&out.leaves[0].1);
    assert!(matches!(pend.reason, PendingReason::NeedsData { .. }));
    assert!(pend.item.corrupted);
    assert_eq!(pend.item.rarity, Rarity::Unique);

    for (id, name) in [
        ("currency/vaal-jewel", "cobalt-jewel-magic"),
        ("currency/vaal-flask", "quicksilver-normal"),
    ] {
        let out = run(id, &item(name));
        let [(p, l)] = out.leaves.as_slice() else {
            panic!("{out:?}")
        };
        assert_eq!(*p, 1.0);
        assert!(matches!(pending(l).reason, PendingReason::NeedsData { .. }));
        assert!(pending(l).item.corrupted);
    }

    // Jewellery has no socket outcome, so the other three take a third each, and the implicit
    // outcome of an implicit-less item merges with the no-change one where they coincide.
    let ring = edited("lapis-amulet-normal", |s| s.rarity = Rarity::Rare);
    let out = run("currency/vaal-socketless", &ring);
    assert!(close(out.mass(), 1.0, 1e-12));
    let unchanged = mass(
        &out,
        |l| matches!(l, Leaf::Item(i) if i.corrupted && i.implicits == ring.implicits),
    );
    let implicit = mass(
        &out,
        |l| matches!(l, Leaf::Item(i) if i.corrupted && i.implicits != ring.implicits),
    );
    let reforge = mass(
        &out,
        |l| matches!(l, Leaf::Pending(p) if p.reason == PendingReason::MultiDraw),
    );
    for got in [unchanged, implicit, reforge] {
        assert!(close(got, 1.0 / 3.0, 1e-12), "{got}");
    }
}

/// R15/D7: bench ids name the action, and the 3.29 bench reroll takes a rare item.
#[test]
fn bench_ids_are_stable() {
    let d = test_data();
    for id in [
        "bench/remove-crafted",
        "bench/sockets/6",
        "bench/links/6",
        "bench/colours/NN",
        "bench/reroll-rare-mods/3",
        "bench/add/EinharMasterIncreasedMana2",
    ] {
        assert!(d.method(id).is_some(), "{id} missing");
    }
    assert_eq!(
        reason("bench/reroll-rare-mods/3", &item("cobalt-jewel-magic")),
        "needs a rare item"
    );
}

/// R18: the aspect beastcrafts put a suffix on a magic or rare item only.
#[test]
fn aspect_needs_magic_or_rare() {
    assert_eq!(
        reason("beast/aspect-avian", &item("slink-boots-normal")),
        "needs a magic or rare item"
    );
}

/// R23: every beast recipe pays its yellow beasts (maxroll's beastcrafting guide).
#[test]
fn beast_costs_include_yellow_beasts() {
    let d = test_data();
    for id in [
        "beast/aspect-avian",
        "beast/imprint",
        "beast/prefix-to-suffix",
        "beast/influence-slam-shaper",
        "beast/suffixes-cannot-be-changed",
        "beast/create-talisman",
    ] {
        let m = d.method(id).unwrap_or_else(|| panic!("{id} missing"));
        assert!(
            m.cost.iter().any(|c| c.name == "beast:Yellow Beast"),
            "{id} pays no yellow beasts"
        );
        assert!(
            !m.cost.iter().any(|c| c.name.contains("Chimeral")),
            "{id} still names Craicic Chimeral"
        );
    }
}

/// A beastcraft that adds "a Shaper modifier" adds a modifier of that influence, not any natural
/// modifier: the pool is the influence's, so a Crown whose prefix pool holds the good and ok
/// classes but whose Shaper pool does not can never gain one from the slam.
#[test]
fn influence_slam_adds_an_influenced_modifier() {
    let d = test_data();
    for (id, influence, input) in [
        (
            "beast/influence-slam-shaper",
            Influence::Shaper,
            item("crown-a-mana"),
        ),
        (
            "beast/influence-slam-hunter",
            Influence::Hunter,
            edited("crown-a-mana", |s| s.influences = vec![Influence::Hunter]),
        ),
    ] {
        let m = d.method(id).unwrap_or_else(|| panic!("{id} missing"));
        let out = apply(d, m, std::slice::from_ref(&input), &Params::default()).expect("applies");
        let mut children = 0;
        for (_, leaf) in &out.leaves {
            for it in leaf.items() {
                children += 1;
                assert_eq!(
                    it.prefixes.len() + it.suffixes.len(),
                    (input.prefixes.len() + input.suffixes.len()) + 1,
                    "{id}"
                );
                let added = it
                    .prefixes
                    .iter()
                    .chain(&it.suffixes)
                    .find(|a| {
                        !input
                            .prefixes
                            .iter()
                            .chain(&input.suffixes)
                            .any(|b| b.roll.m == a.roll.m)
                    })
                    .unwrap_or_else(|| panic!("{id}: no added modifier"));
                assert_eq!(d.mod_at(added.roll.m).influence, Some(influence), "{id}");
            }
        }
        assert!(children > 1, "{id}: one child means the pool was empty");
    }
}

/// Q2: a metamod craft needs the crafted slot free, so the player removes the old craft first.
#[test]
fn metamod_craft_needs_room() {
    let mana = item("crown-a-mana");
    assert_eq!(
        reason(
            "bench/add/DexMasterItemGenerationCannotChangeSuffixes",
            &mana
        ),
        "the item already has as many crafted modifiers as it can hold"
    );
    let out = run("bench/remove-crafted", &mana);
    let bare = out.item().expect("one item").clone();
    assert_eq!(bare.crafted_count(test_data()), 0);
    assert!(
        run(
            "bench/add/DexMasterItemGenerationCannotChangeSuffixes",
            &bare
        )
        .item()
        .is_some()
    );
}

/// R13: the Mirror takes a non-unique item; the Reflecting Mists refuse veiled, influenced and
/// synthesised items (directions and the 3.29 notes).
#[test]
fn mirror_and_mist_limits() {
    assert_eq!(
        reason(
            "currency/mirror",
            &edited("lapis-amulet-normal", |s| s.rarity = Rarity::Unique)
        ),
        "the item is unique"
    );
    for id in [
        "currency/reflecting-mist",
        "currency/fading-reflecting-mist",
    ] {
        let influenced = edited("lapis-amulet-normal", |s| {
            s.rarity = Rarity::Rare;
            s.influences = vec![Influence::Shaper];
        });
        assert_eq!(reason(id, &influenced), "the item is influenced");
        let synthesised = edited("lapis-amulet-normal", |s| {
            s.rarity = Rarity::Rare;
            s.synthesised = true;
        });
        assert_eq!(reason(id, &synthesised), "the item is synthesised");
    }
}

/// R16: every ducat is an Allflame ingredient, so none of them is modelled exactly.
#[test]
fn ducats_are_allflame() {
    let d = test_data();
    let ducats: Vec<&Method> = d
        .methods
        .iter()
        .filter(|m| m.id.ends_with("-ducat"))
        .collect();
    assert_eq!(ducats.len(), 9);
    for m in ducats {
        let unmodelled = m.all_ops().iter().any(|op| {
            matches!(
                op,
                Op::Unmodelled {
                    reason: PendingReason::NeedsData { .. }
                }
            )
        });
        assert!(unmodelled, "{} is modelled", m.id);
        assert!(
            m.claims.iter().any(|c| c == "ducat.allflame"),
            "{} does not cite ducat.allflame",
            m.id
        );
    }
}

/// R15: the Talisman crafts exist and key on the base tag.
#[test]
fn talisman_crafts() {
    let d = test_data();
    let amulet = edited("lapis-amulet-normal", |s| s.rarity = Rarity::Rare);
    assert_eq!(
        reason("beast/imprint-talisman", &amulet),
        "needs a talisman base"
    );
    let talisman = edited("lapis-amulet-normal", |s| {
        s.base = "Metadata/Items/Amulets/Talismans/TalismanChieftain".into();
        s.rarity = Rarity::Rare;
    });
    assert!(
        run("beast/imprint-talisman", &talisman)
            .item()
            .unwrap()
            .imprint
            .is_some()
    );
    for id in ["beast/reroll-talisman-base", "beast/create-talisman"] {
        assert!(d.method(id).is_some(), "{id} missing");
    }
}

/// Q7: a tag reforge only asks whether the tag can roll at all. A modifier the item already has
/// in that tag's groups is cleared first, so it must not make the method refuse.
#[test]
fn harvest_tag_reforge_ignores_current_groups() {
    let with_fire = edited("crown-a", |s| {
        s.suffixes = vec![affix("FireResist5", None)];
    });
    let out = run("harvest/reforge-fire", &with_fire);
    let pend = pending(&out.leaves[0].1);
    assert_eq!(
        pend.item.explicits().count(),
        0,
        "the clear removes the fire mod"
    );
}

/// The 14 Horticrafting quality enchants place their own enchantment, and the anoint methods
/// refuse a Talisman base (claim `anoint.talismans`).
#[test]
fn harvest_quality_enchant_and_talisman_refusal() {
    let d = test_data();
    let body = edited("slink-boots-normal", |s| {
        s.base = "Metadata/Items/Armours/BodyArmours/BodyDex1".into()
    });
    let out = run("harvest/enchant-body-armour-life", &body);
    let it = leaf_item(&out.leaves[0].1);
    assert_eq!(it.enchants.len(), 1);
    assert_eq!(
        d.mod_at(it.enchants[0].m).id,
        "HarvestAlternateArmourQualityIncreasedLife"
    );
    assert_eq!(
        reason(
            "harvest/enchant-body-armour-life",
            &item("cobalt-jewel-normal")
        ),
        "only works on: Body Armours"
    );
    let talisman = edited("lapis-amulet-normal", |s| {
        s.base = "Metadata/Items/Amulets/Talismans/TalismanChieftain".into()
    });
    assert_eq!(
        reason("currency/anoint-clear-oil", &talisman),
        "the item's base has the talisman tag"
    );
    assert_eq!(
        reason(
            "currency/anoint-prismatic-oil",
            &item("cogwork-ring-normal")
        ),
        "only works on: Amulets"
    );
    assert!(
        matches!(run("currency/anoint-clear-oil", &item("lapis-amulet-normal")), out if out.leaves.len() == 1)
    );
}

/// The two 3.29 Talisman fracture beastcrafts replace the old split crafts (claim
/// `beast.split-replaced`): one fractures a single modifier on a rare Talisman with four or more
/// modifiers, the other fractures two on one with six or more. No source states their beast cost
/// (claim `beast.talisman-fracture-cost`), so they carry none.
#[test]
fn talisman_fracture_beastcrafts() {
    let d = test_data();
    let four = item("talisman-rare-four");
    let out = run("beast/fracture-talisman", &four);
    assert!(close(out.mass(), 1.0, 1e-12));
    assert_eq!(out.leaves.len(), 4);
    for (_, l) in &out.leaves {
        let it = leaf_item(l);
        assert_eq!(it.explicits().filter(|a| a.fractured).count(), 1);
    }
    let six = item("talisman-rare-six");
    let out = run("beast/fracture-talisman-two", &six);
    assert!(close(out.mass(), 1.0, 1e-12));
    // Two ordered draws of the same pair merge, so the 30 orders are 15 unordered pairs.
    assert_eq!(out.leaves.len(), 15);
    for (_, l) in &out.leaves {
        let it = leaf_item(l);
        assert_eq!(it.explicits().filter(|a| a.fractured).count(), 2);
    }
    for id in ["beast/fracture-talisman", "beast/fracture-talisman-two"] {
        assert!(
            cost(d, method(id), &Params::default()).unwrap().is_empty(),
            "{id} states a cost no source gives"
        );
    }
    assert_eq!(
        reason("beast/fracture-talisman", &item("lapis-amulet-a")),
        "needs a talisman base"
    );
    assert_eq!(
        reason("beast/fracture-talisman-two", &four),
        "needs at least 6 modifiers"
    );
}

/// The Vaal pool of an amulet holds the 3.29 minimum-power-charges corrupted implicit: the amulet
/// tags match its spawn list, so the implicit outcome can draw it.
#[test]
fn vaal_pool_includes_the_amulet_power_charge_implicit() {
    let d = test_data();
    let amulet = edited("lapis-amulet-normal", |s| s.rarity = Rarity::Rare);
    let out = run("currency/vaal-socketless", &amulet);
    assert!(
        out.leaves.iter().any(|(_, l)| matches!(
            l,
            Leaf::Item(i) if i
                .implicits
                .iter()
                .any(|r| d.mod_at(r.m).id == "V2MinPowerChargesCorruptedNew")
        )),
        "the amulet's Vaal pool lacks V2MinPowerChargesCorruptedNew"
    );
}

/// The anointment, Rancour reforge and rune costs resolve from the rules: a standard oil, one
/// special oil whose contract needs a corrupted item, a Harvest reforge that also spends
/// Crystallised Rancour, and one Kalguuran rune.
#[test]
fn oil_rancour_and_rune_costs() {
    let d = test_data();
    let costs = |id: &str| cost(d, method(id), &Params::default()).unwrap();
    let clear = costs("currency/anoint-clear-oil");
    assert_eq!(clear.len(), 1);
    assert_eq!(d.currency_by_id(&clear[0].key).unwrap().name, "Clear Oil");
    assert_eq!(clear[0].amount, 1.0);

    let tainted = costs("currency/anoint-tainted-oil");
    assert_eq!(
        d.currency_by_id(&tainted[0].key).unwrap().name,
        "Tainted Oil"
    );
    assert_eq!(
        reason("currency/anoint-tainted-oil", &item("lapis-amulet-normal")),
        "needs a corrupted item"
    );

    let rancour = costs("harvest/reforge-minion");
    let names: Vec<(&str, f64)> = rancour
        .iter()
        .map(|c| (d.currency_by_id(&c.key).unwrap().name.as_str(), c.amount))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Primal Crystallised Lifeforce", 200.0),
            ("Crystallised Rancour", 3.0)
        ]
    );

    let rune = costs("currency/rune-bound");
    assert_eq!(d.currency_by_id(&rune[0].key).unwrap().name, "Bound Rune");
    assert_eq!(
        reason("currency/rune-bound", &item("cobalt-jewel-normal")),
        "only works on: weapons"
    );
}
