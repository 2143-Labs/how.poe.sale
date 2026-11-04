//! Concrete items: fixture round trips, counts, tags, eldritch dominance, base magnitudes and
//! every validation error.

mod common;

use common::{fixture_names, item, spec};
use poe1_v2::data::{Data, Influence, Rarity, Side};
use poe1_v2::item::{AffixSpec, Eldritch, Item, ItemSpec, ModSpec, Roll};
use poe1_v2::test_data;

#[test]
fn fixtures_round_trip() {
    let d = test_data();
    let names = fixture_names();
    assert!(names.len() >= 19, "{names:?}");
    for name in names {
        let a = item(&name);
        let b = Item::from_spec(d, &a.to_spec(d)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn crown_counts() {
    let d = test_data();
    let c2 = item("crown-c2");
    assert_eq!(c2.open(d, Side::Prefix), 1);
    assert_eq!(c2.open(d, Side::Suffix), 0);
    assert!(c2.locked(d, Side::Suffix));
    assert!(!c2.locked(d, Side::Prefix));
    assert_eq!(c2.crafted_count(d), 1);
    assert!(matches!(c2.veiled(d), Some((Side::Prefix, _))));
}

#[test]
fn tags_include_influence() {
    let d = test_data();
    let shaper = d.tag_by_name("helmet_shaper").unwrap();
    assert!(item("crown-a").tags(d).contains(&shaper));
}

#[test]
fn dominance() {
    let d = test_data();
    assert_eq!(
        item("slink-boots-eldritch").dominant(d),
        Some(Eldritch::Exarch)
    );
}

#[test]
fn simplex_base() {
    let d = test_data();
    let simplex = item("simplex-normal");
    let base = d.base_at(simplex.base);
    assert_eq!(base.explicit_effect, 100);
    assert!(base.implicits_locked);
    let mut rare = simplex.clone();
    rare.rarity = Rarity::Rare;
    assert_eq!(rare.limits(d), [1, 2]);
    assert_eq!(Item::display(50, 100), 100);
    let crown = d.base_at(item("crown-a").base);
    assert_eq!(crown.explicit_effect, 0);
    assert!(!crown.implicits_locked);
}

fn affix(id: &str, values: Option<Vec<i32>>) -> AffixSpec {
    AffixSpec {
        id: id.into(),
        values,
        fractured: false,
    }
}

fn err(d: &Data, s: &ItemSpec) -> String {
    match Item::from_spec(d, s) {
        Ok(_) => panic!("{s:?} was accepted"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn rejects() {
    let d = test_data();
    let check = |s: ItemSpec, want: &str| assert_eq!(err(d, &s), want);

    let mut s = spec("crown-a");
    s.base = "No Such Base".into();
    check(s, "unknown base \"No Such Base\"");

    let mut s = spec("slink-boots-normal");
    s.base = "Two-Toned Boots".into();
    check(
        s,
        "base name \"Two-Toned Boots\" matches several bases; use one of: \
         Metadata/Items/Armours/Boots/BootsAtlas1, Metadata/Items/Armours/Boots/BootsAtlas2, \
         Metadata/Items/Armours/Boots/BootsAtlas3",
    );

    // Every base in the bundle belongs to a covered class; drop the last class (Tincture) from a
    // copy of the bundle to reach the check.
    let mut bundle = d.bundle.clone();
    let dropped = bundle.classes.pop().unwrap();
    assert_eq!(dropped.id, "Tincture");
    let narrowed = Data::from_bundle(bundle).unwrap();
    let mut s = spec("quicksilver-normal");
    s.base = "Metadata/Items/Tinctures/TinctureStun".into();
    assert_eq!(
        err(&narrowed, &s),
        format!("item class {} is not covered by v2", d.classes.len() - 1)
    );

    let mut s = spec("crown-a");
    s.implicits.push(ModSpec {
        id: "NoSuchMod".into(),
        values: None,
    });
    check(s, "unknown modifier \"NoSuchMod\"");

    let mut s = spec("crown-a");
    s.prefixes
        .push(affix("IncreasedChillEffectSupportedUber1", None));
    check(
        s,
        "modifier \"IncreasedChillEffectSupportedUber1\" cannot be a prefix",
    );

    let mut s = spec("crown-a");
    s.implicits.push(ModSpec {
        id: "IncreasedLife9".into(),
        values: None,
    });
    check(s, "modifier \"IncreasedLife9\" cannot be a implicit");

    let mut s = spec("crown-a");
    s.prefixes.push(affix("IncreasedLife9", Some(vec![200])));
    check(
        s,
        "modifier \"IncreasedLife9\" needs 1 values within its ranges",
    );

    let mut s = spec("cobalt-jewel-normal");
    s.prefixes.push(affix("WandDamageJewel", Some(vec![15])));
    check(s, "a normal item cannot have explicit modifiers");

    let mut s = spec("cobalt-jewel-magic");
    s.prefixes.push(affix("IncreasedLife0", None));
    check(s, "too many prefixes for a magic Jewel");

    let mut s = spec("crown-l1");
    s.prefixes.push(affix("IncreasedLife8", None));
    check(s, "two modifiers share the group IncreasedLife");

    let mut s = spec("crown-l1");
    s.prefixes.push(affix("EinharMasterIncreasedMana2", None));
    check(s, "more crafted modifiers than the item allows");

    let mut s = spec("crown-a");
    s.catalyst = Some("Fertile Catalyst".into());
    check(
        s,
        "catalyst quality only exists on rings, amulets and belts",
    );

    let mut s = spec("lapis-amulet-normal");
    s.catalyst = Some("Shiny Catalyst".into());
    check(s, "unknown catalyst \"Shiny Catalyst\"");

    let mut s = spec("crown-a");
    s.sockets = "B=B".into();
    check(s, "cannot read sockets \"B=B\"");

    let mut s = spec("crown-a");
    s.suffixes.pop();
    s.suffixes
        .push(affix("FlaskIgniteImmunityDuringEffect_", None));
    check(
        s,
        "modifier \"FlaskIgniteImmunityDuringEffect_\" cannot be on a Helmet",
    );

    let mut s = spec("cobalt-jewel-normal");
    s.influences = vec![Influence::Shaper];
    check(s, "this item class cannot be influenced");

    let mut s = spec("crown-a");
    s.sockets = "B-B-B-B-B".into();
    check(s, "a Helmet holds at most 4 sockets");
}

/// A rare holds two veiled modifiers since 3.26 (Catarina drops them).
#[test]
fn double_veiled() {
    let d = test_data();
    let mut s = spec("crown-a");
    s.prefixes.push(affix("VeiledPrefix", None));
    s.suffixes.pop();
    s.suffixes.push(affix("VeiledSuffix", None));
    let item = Item::from_spec(d, &s).expect("a double-veiled rare");
    assert_eq!(item.veiled(d).map(|(side, _)| side), Some(Side::Prefix));
}

/// A mod whose stats are all fixed has one stored form, whoever wrote it.
#[test]
fn fixed_values_are_canonical() {
    let d = test_data();
    let mut s = spec("crown-a");
    s.prefixes.push(affix(
        "DexMasterItemGenerationCannotChangeSuffixes",
        Some(vec![1]),
    ));
    let drafted = Item::from_spec(d, &s).expect("the drafted item");
    let mut written = spec("crown-a");
    written
        .prefixes
        .push(affix("DexMasterItemGenerationCannotChangeSuffixes", None));
    let read: Item = Item::from_spec(d, &written).expect("the item without values");
    assert_eq!(drafted, read);
    assert_eq!(drafted.prefixes[0].roll.values, Some(vec![1]));
    assert!(
        drafted.prefixes.iter().all(|a| a.roll.values.is_some()),
        "a fixed mod stores its roll"
    );
}

/// Base magnitudes: the base's own effect plus the one for the mod's side; a catalyst enhances an
/// implicit it tags.
#[test]
fn base_magnitudes() {
    let d = test_data();
    let effect = |name: &str, id: &str, implicit: bool| {
        let item = item(name);
        let m = d.mod_by_id(id).expect("a known mod");
        item.effect(d, &Roll { m, values: None }, implicit)
    };
    // Cogwork Ring: +25% to suffix modifier magnitudes, nothing for prefixes.
    assert_eq!(effect("cogwork-ring-normal", "AllAttributes4", false), 25);
    assert_eq!(
        effect("lapis-amulet-normal", "IntelligenceImplicitAmulet1", true),
        0
    );
    // Simplex Amulet: +100% to explicit magnitudes, no side magnitude.
    assert_eq!(effect("simplex-normal", "IncreasedLife9", false), 100);
    // Manifold Ring: +50% to prefix magnitudes; Geodesic Ring: +25% to prefix magnitudes.
    for (ring, prefix) in [("Manifold Ring", 50), ("Geodesic Ring", 25)] {
        let base = d.bases_named(ring)[0];
        assert_eq!(d.base_at(base).prefix_effect, prefix, "{ring}");
        assert_eq!(d.base_at(base).suffix_effect, 0, "{ring}");
    }
    assert_eq!(
        d.base_at(d.bases_named("Archdemon Crown")[0]).prefix_effect,
        0
    );

    // Intrinsic Catalyst quality enhances the attribute implicit of a Lapis Amulet.
    let mut s = spec("lapis-amulet-normal");
    s.quality = 20;
    s.catalyst = Some("Intrinsic Catalyst".into());
    let item = Item::from_spec(d, &s).expect("the catalysed amulet");
    let implicit = item.implicits[0].clone();
    assert_eq!(item.effect(d, &implicit, true), 20);
    assert_eq!(Item::display(20, 20), 24);

    // Astrolabe Amulet can hold every influence, so it spawns every influence's mods.
    let astrolabe = d
        .bases_named("Astrolabe Amulet")
        .first()
        .expect("the Astrolabe Amulet");
    let mut s = spec("lapis-amulet-normal");
    s.base = d.base_at(*astrolabe).id.clone();
    let bare = Item::from_spec(d, &s).expect("a bare Astrolabe Amulet");
    let tags = bare.tags(d);
    let class = bare.class(d);
    let inf = class.influence_tags.expect("amulets are influenced");
    for t in inf {
        assert!(tags.contains(&t), "{} missing", d.tag(t));
    }
}
