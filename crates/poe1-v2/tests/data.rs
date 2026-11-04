//! The generated v2 bundle: mod kinds, bases, catalysts, essences and stat translations.

use poe1_v2::data::{Category, Decimals, Influence, ModKind, Side};
use poe1_v2::test_data;

#[test]
fn bundle_loads() {
    let d = test_data();
    assert_eq!(d.format, 1);
    let kind = |id: &str| {
        let m = d.mod_at(d.mod_by_id(id).unwrap_or_else(|| panic!("{id} missing")));
        (m.kind, m.side)
    };
    assert_eq!(
        kind("JunMasterVeiledSkillAreaOfEffectPercentAndAreaOfEffectGemLevel"),
        (ModKind::Unveiled, Some(Side::Prefix))
    );
    assert_eq!(kind("VeiledPrefix").0, ModKind::Veiled);
    assert_eq!(
        kind("DexMasterItemGenerationCannotChangeSuffixes"),
        (ModKind::Crafted, Some(Side::Prefix))
    );
    let ember = d.mod_at(d.mod_by_id("MovementVelocityEldritchImplicit6").unwrap());
    assert_eq!(
        (ember.kind, ember.eldritch_tier),
        (ModKind::Exarch, Some(6))
    );
    let maven = d.mod_at(
        d.mod_by_id("IncreasedChillEffectSupportedUberMaven")
            .unwrap(),
    );
    assert!(maven.elevated);
    assert_eq!(maven.influence, Some(Influence::Shaper));
    assert_eq!(
        kind("SocketedActiveGemLevelSupportGemPenaltyImplicitR2").0,
        ModKind::BaseImplicit
    );
    assert!(d.mods.iter().all(|m| !m.id.contains("Royale")));
}

#[test]
fn bases_by_name() {
    let d = test_data();
    assert_eq!(d.bases_named("Two-Toned Boots").len(), 3);
    let crown = d.bases_named("Archdemon Crown");
    assert_eq!(crown.len(), 1);
    assert_eq!(
        d.base_at(crown[0]).id,
        "Metadata/Items/Armours/Helmets/HelmetStrIntRitual3"
    );
    let class = d.class_of(crown[0]);
    assert_eq!(
        (class.id.as_str(), class.category),
        ("Helmet", Category::Armour)
    );
    assert_eq!(class.limits.rare, [3, 3]);
}

#[test]
fn catalysts_and_essences() {
    let d = test_data();
    assert_eq!(d.catalysts.len(), 12);
    let essence = |name: &str| {
        d.essences
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    let wailing = essence("Wailing Essence of Hatred");
    assert!(!wailing.allows_rare);
    assert_eq!(wailing.item_level_max, Some(75));
    assert!(essence("Deafening Essence of Hatred").allows_rare);
}

#[test]
fn translations() {
    let d = test_data();
    let entry = |stat: &str| {
        let s = d
            .stat_by_name(stat)
            .unwrap_or_else(|| panic!("{stat} missing"));
        &d.translations[d.translation_of_stat(s).unwrap() as usize]
    };
    let life = entry("base_maximum_life");
    assert_eq!(life.variants.len(), 1);
    assert_eq!(life.variants[0].string, "{0} to maximum Life");
    assert_eq!(life.variants[0].slots.len(), 1);
    assert!(life.variants[0].slots[0].plus);
    assert_eq!(life.variants[0].slots[0].factor, 1.0);
    assert_eq!(life.variants[0].slots[0].decimals, Decimals::UpTo(2));

    let crit = entry("local_display_socketed_spells_additional_critical_strike_chance");
    assert_eq!(crit.variants[0].slots[0].factor, 0.01);

    let aoe = entry("base_skill_area_of_effect_+%");
    assert_eq!(aoe.variants.len(), 2);
    assert_eq!(aoe.variants[0].conditions[0].min, Some(1));
    assert_eq!(aoe.variants[1].conditions[0].max, Some(-1));
    assert_eq!(aoe.variants[1].slots[0].factor, -1.0);

    let locked = entry("local_implicit_mod_cannot_be_changed");
    assert!(
        locked
            .variants
            .iter()
            .any(|v| v.slots.is_empty() && v.string == "Implicit Modifiers Cannot Be Changed")
    );
}
