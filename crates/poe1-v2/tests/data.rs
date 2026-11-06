//! The generated v2 bundle: mod kinds, bases, catalysts, essences and stat translations.

use std::collections::BTreeSet;

use poe1_v2::data::{BenchAction, Category, Decimals, Influence, ModKind, Side};
use poe1_v2::test_data;

#[test]
fn bundle_loads() {
    let d = test_data();
    assert_eq!(d.format, poe1_v2::data::FORMAT);
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
    // Elevated mods with their own `…Maven` mod type take their chain's influence by id.
    for (id, inf) in [
        ("AdditionalCritWithAttacksInfluenceMaven", Influence::Hunter),
        ("AvoidStunInfluenceMaven__", Influence::Warlord),
    ] {
        assert_eq!(
            d.mod_at(d.mod_by_id(id).unwrap()).influence,
            Some(inf),
            "{id}"
        );
    }
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

#[test]
fn repeated_placeholder_reuses_its_slot() {
    let d = test_data();
    let entry = |stat: &str| {
        let s = d
            .stat_by_name(stat)
            .unwrap_or_else(|| panic!("{stat} missing"));
        &d.translations[d.translation_of_stat(s).unwrap() as usize]
    };
    // "of Sealing" and "of Thawing" show one roll twice; the repeated `{0}` keeps the variant.
    for stat in [
        "local_flask_bleeding_immunity_if_bleeding_and_remove_corrupted_blood_s",
        "local_flask_chill_or_freeze_immunity_if_chilled_or_frozen_s",
    ] {
        let e = entry(stat);
        let slots = &e.variants[0].slots;
        assert_eq!(e.variants.len(), 1, "{stat}");
        assert_eq!(slots.len(), 2, "{stat}");
        assert_eq!(slots[0].k, slots[1].k, "{stat}");
        assert_eq!(slots[0].factor, slots[1].factor, "{stat}");
        assert_eq!(slots[0].plus, slots[1].plus, "{stat}");
    }
}

#[test]
fn essence_columns_cover_rune_daggers_and_warstaves() {
    let d = test_data();
    let class = |id: &str| {
        d.class_by_id(id)
            .unwrap_or_else(|| panic!("class {id} missing"))
    };
    let (dagger, rune, staff, war, rod) = (
        class("Dagger"),
        class("Rune Dagger"),
        class("Staff"),
        class("Warstaff"),
        class("FishingRod"),
    );
    let (mut daggers, mut staffs) = (0, 0);
    for e in &d.essences {
        let mod_for = |c: u16| e.mods.iter().find(|&&(cc, _)| cc == c).map(|&(_, m)| m);
        if let Some(m) = mod_for(dagger) {
            daggers += 1;
            assert_eq!(
                mod_for(rune),
                Some(m),
                "{} needs the Dagger column on Rune Daggers",
                e.name
            );
        }
        if let Some(m) = mod_for(staff) {
            staffs += 1;
            assert_eq!(
                mod_for(war),
                Some(m),
                "{} needs the Staff column on Warstaves",
                e.name
            );
        }
        assert_eq!(
            mod_for(rod),
            None,
            "{} must have no Fishing Rod mod",
            e.name
        );
    }
    assert!(
        daggers > 0 && staffs > 0,
        "no essence has a Dagger or Staff mod"
    );
}

#[test]
fn currencies_carry_directions() {
    let d = test_data();
    // The game's usage line, distinct from the description. Placeholders and itemised things
    // (wisps, tattoos, nets) have none, so only the count is asserted wholesale.
    let with_directions = d
        .currencies
        .iter()
        .filter(|c| !c.directions.is_empty())
        .count();
    assert!(
        with_directions > 600,
        "only {with_directions} currencies carry directions"
    );
    let fracturing = d
        .currency_by_name("Fracturing Orb")
        .expect("Fracturing Orb missing");
    assert!(
        fracturing
            .directions
            .contains("Cannot be used on Influenced or Fractured items"),
        "{}",
        fracturing.directions
    );
    assert_ne!(fracturing.directions, fracturing.description);
    let chaos = d.currency_by_name("Chaos Orb").expect("Chaos Orb missing");
    assert_eq!(
        chaos.directions,
        "Right click this item then left click a rare item to apply it."
    );
}

/// Stats the pinned data cannot translate, with why.
const UNTRANSLATED: &[(&str, &str)] = &[
    (
        "essence_buff_elemental_damage_taken_+%",
        "a hidden essence-buff stat: the game shows the buff, not this value, on the item",
    ),
    (
        "local_skills_granted_by_item_are_supported_by_level_x_unique_trigger_support",
        "the hidden support level of the trigger supports; the game shows it only on the unique item that grants them",
    ),
    (
        "local_use_skill_on_melee_hit_%",
        "the hidden trigger chance of the on-hit experimented weapons; the game shows no line for it",
    ),
    (
        "dummy_stat_display_nothing",
        "a marker the game hides: its translation entry is empty",
    ),
    (
        "grant_focus_skill",
        "grants the Focus skill; the game shows no line for it",
    ),
    (
        "local_influence_mod_requires_unique_monster_presence",
        "the pinned data has no translation: presence mods cannot show their condition (D3)",
    ),
    (
        "local_influence_mod_requires_celestial_boss_presence",
        "the pinned data has no translation: presence mods cannot show their condition (D3)",
    ),
];

#[test]
fn every_spawnable_mod_stat_is_translated() {
    let d = test_data();
    // A tag on a kept base, or on the influence variant of its class: a mod with a positive spawn
    // weight on any of these can appear on an in-scope item.
    let item_tags: BTreeSet<u16> = d
        .bases
        .iter()
        .flat_map(|b| {
            b.tags.iter().copied().chain(
                d.classes[b.class as usize]
                    .influence_tags
                    .iter()
                    .flatten()
                    .copied(),
            )
        })
        .collect();
    for (stat, _) in UNTRANSLATED {
        let s = d
            .stat_by_name(stat)
            .unwrap_or_else(|| panic!("the allow-list names unknown stat {stat}"));
        assert!(
            d.translation_of_stat(s).is_none(),
            "the allow-list entry {stat} is stale: the pinned data now translates it"
        );
    }
    // Every mod the model can reach: a base implicit, a bench action, an essence, a fossil, or a
    // positive spawn weight on some in-scope item's tags.
    let mut reachable: BTreeSet<u32> = BTreeSet::new();
    for b in &d.bases {
        reachable.extend(b.implicits.iter().copied());
    }
    for option in &d.bench {
        match option.action {
            BenchAction::AddMod { m } | BenchAction::AddEnchant { m } => {
                reachable.insert(m);
            }
            _ => {}
        }
    }
    for e in &d.essences {
        reachable.extend(e.mods.iter().map(|&(_, m)| m));
    }
    for f in &d.fossils {
        reachable.extend(f.added_mods.iter().copied());
        reachable.extend(f.forced_mods.iter().copied());
    }
    for (i, m) in d.mods.iter().enumerate() {
        if m.spawn
            .iter()
            .any(|&(t, weight)| weight > 0 && item_tags.contains(&t))
        {
            reachable.insert(i as u32);
        }
    }
    let mut missing = BTreeSet::new();
    for &i in &reachable {
        let m = d.mod_at(i);
        for s in &m.stats {
            let name = d.stat(s.stat);
            if d.translation_of_stat(s.stat).is_none()
                && !UNTRANSLATED.iter().any(|&(n, _)| n == name)
            {
                missing.insert(format!("{name} (on {})", m.id));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "spawnable stats with no translation: {missing:?}"
    );
}

/// Tincture lines live in `stat_translations/tincture.json`, which `scripts/fetch-data.sh` pins
/// beside the main file and datagen merges. A rolled tincture affix therefore prints the game's
/// wording; without the file the merged line would come out empty.
#[test]
fn tincture_mods_render_their_lines() {
    let d = test_data();
    let spec: poe1_v2::item::ItemSpec = serde_json::from_str(
        r#"{
            "base": "Metadata/Items/Tinctures/Tincture4",
            "itemLevel": 84,
            "rarity": "magic",
            "prefixes": [{ "id": "TinctureEffect2", "values": [16] }]
        }"#,
    )
    .expect("tincture spec");
    let item = poe1_v2::item::Item::from_spec(d, &spec).expect("tincture item");
    let text = poe1_v2::text::render(d, &item);
    assert!(
        text.lines().any(|line| line == "16% increased effect"),
        "{text}"
    );
}

#[test]
fn presence_mods_cannot_show_their_condition() {
    // D3: every `…Presence` eldritch tier carries a presence-requirement stat the game shows on
    // its own line, but the pinned RePoE export has no translation for either stat, so the tiers
    // print exactly like their base versions. Do not invent the line; refresh the data first.
    let d = test_data();
    let presence = d
        .mod_by_id("IncreasedAttackSpeedEldritchImplicitUniquePresence1")
        .expect("presence mod missing");
    let stat = |name: &str| {
        d.stat_by_name(name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    let unique = stat("local_influence_mod_requires_unique_monster_presence");
    let celestial = stat("local_influence_mod_requires_celestial_boss_presence");
    assert!(
        d.mod_at(presence).stats.iter().any(|s| s.stat == unique),
        "the presence mod no longer carries its condition stat"
    );
    for s in [unique, celestial] {
        assert!(
            d.translation_of_stat(s).is_none(),
            "the pinned data now translates {}; give presence mods their own line",
            d.stat(s)
        );
    }
}

/// The data fixes the coverage audit asked for: no Remnant of Corruption among the essences, no
/// Energy Blade skill buffs among the bases, every elevated mod with an influence, and the 14
/// Horticrafting quality enchants kept as enchant mods that no random pool can draw.
#[test]
fn audit_data_fixes_hold() {
    let d = test_data();
    assert_eq!(d.essences.len(), 105);
    assert!(d.essences.iter().all(|e| e.name != "Remnant of Corruption"));
    assert!(d.bases.iter().all(|b| !b.id.contains("StormBlade")));
    let mut elevated = 0;
    for m in &d.mods {
        if m.elevated {
            elevated += 1;
            assert!(
                m.influence.is_some(),
                "elevated mod {} has no influence",
                m.id
            );
        }
    }
    assert_eq!(elevated, 247);
    // The 21 whose group carries several influences read their own zero-weight spawn tag.
    for (id, influence) in [
        ("AdditionalChanceToEvadeInfluenceMaven", Influence::Redeemer),
        ("AdditionalMinesPlacedSupportedUberMaven", Influence::Shaper),
        ("FortifyEffectInfluenceMaven_", Influence::Crusader),
    ] {
        assert_eq!(
            d.mod_at(d.mod_by_id(id).unwrap()).influence,
            Some(influence),
            "{id}"
        );
    }
    let harvest_enchants: Vec<_> = d
        .mods
        .iter()
        .filter(|m| m.id.starts_with("HarvestAlternate"))
        .collect();
    assert_eq!(harvest_enchants.len(), 14);
    for m in harvest_enchants {
        assert_eq!(m.kind, ModKind::Enchant, "{}", m.id);
        assert!(
            m.spawn.iter().all(|&(_, weight)| weight == 0) || m.spawn.is_empty(),
            "{} can be drawn by a random enchant pool",
            m.id
        );
    }
}

/// The current-league base scope (claim `bases.current-league`): every Standard-only base the
/// owner listed is gone, the duplicate names they caused are unique, and the surviving
/// Talisman bases all carry the one placeholder implicit.
#[test]
fn only_current_league_bases() {
    let d = test_data();
    let ids: BTreeSet<&str> = d.bases.iter().map(|b| b.id.as_str()).collect();
    let quivers = [
        "Metadata/Items/Quivers/Quiver6",
        "Metadata/Items/Quivers/Quiver7",
        "Metadata/Items/Quivers/Quiver8",
        "Metadata/Items/Quivers/Quiver9",
        "Metadata/Items/Quivers/Quiver10",
        "Metadata/Items/Quivers/Quiver11",
        "Metadata/Items/Quivers/Quiver12",
        "Metadata/Items/Quivers/Quiver13",
    ];
    let tinctures = [
        "Metadata/Items/Tinctures/TinctureStun",
        "Metadata/Items/Tinctures/TinctureIgnite",
        "Metadata/Items/Tinctures/TinctureShock",
        "Metadata/Items/Tinctures/TincturePoison",
        "Metadata/Items/Tinctures/TinctureCriticalStrike",
        "Metadata/Items/Tinctures/TinctureCullingStrike",
        "Metadata/Items/Tinctures/TinctureFreeze",
        "Metadata/Items/Tinctures/TinctureBleed",
    ];
    let talismans = [
        "Metadata/Items/Amulets/Talismans/Talisman1_1",
        "Metadata/Items/Amulets/Talismans/Talisman1_2",
        "Metadata/Items/Amulets/Talismans/Talisman1_3",
        "Metadata/Items/Amulets/Talismans/Talisman1_4",
        "Metadata/Items/Amulets/Talismans/Talisman1_5",
        "Metadata/Items/Amulets/Talismans/Talisman1_6",
        "Metadata/Items/Amulets/Talismans/Talisman1_7",
        "Metadata/Items/Amulets/Talismans/Talisman1_8",
        "Metadata/Items/Amulets/Talismans/Talisman1_9",
        "Metadata/Items/Amulets/Talismans/Talisman1_10",
        "Metadata/Items/Amulets/Talismans/Talisman1_11",
        "Metadata/Items/Amulets/Talismans/Talisman1_12",
        "Metadata/Items/Amulets/Talismans/Talisman2_1",
        "Metadata/Items/Amulets/Talismans/Talisman2_2",
        "Metadata/Items/Amulets/Talismans/Talisman2_3",
        "Metadata/Items/Amulets/Talismans/Talisman2_4",
        "Metadata/Items/Amulets/Talismans/Talisman2_5",
        "Metadata/Items/Amulets/Talismans/Talisman2_6_1",
        "Metadata/Items/Amulets/Talismans/Talisman2_6_2",
        "Metadata/Items/Amulets/Talismans/Talisman2_6_3",
        "Metadata/Items/Amulets/Talismans/Talisman2_6_4",
        "Metadata/Items/Amulets/Talismans/Talisman2_6_5",
        "Metadata/Items/Amulets/Talismans/Talisman2_6_6",
        "Metadata/Items/Amulets/Talismans/Talisman2_7",
        "Metadata/Items/Amulets/Talismans/Talisman2_8",
        "Metadata/Items/Amulets/Talismans/Talisman3_1",
        "Metadata/Items/Amulets/Talismans/Talisman3_2",
        "Metadata/Items/Amulets/Talismans/Talisman3_3",
        "Metadata/Items/Amulets/Talismans/Talisman3_4",
        "Metadata/Items/Amulets/Talismans/Talisman3_5",
        "Metadata/Items/Amulets/Talismans/Talisman3_6_1",
        "Metadata/Items/Amulets/Talismans/Talisman3_6_2",
        "Metadata/Items/Amulets/Talismans/Talisman3_6_3",
        "Metadata/Items/Amulets/Talismans/Talisman3_7",
    ];
    let heist = [
        "Metadata/Items/Weapons/TwoHandWeapons/Bows/BowE1",
        "Metadata/Items/Weapons/OneHandWeapons/Claws/ClawE1",
        "Metadata/Items/Weapons/OneHandWeapons/Daggers/DaggerE1",
        "Metadata/Items/Weapons/OneHandWeapons/Daggers/RuneDaggerE1",
        "Metadata/Items/Weapons/OneHandWeapons/OneHandAxes/OneHandAxeE1",
        "Metadata/Items/Weapons/OneHandWeapons/OneHandMaces/OneHandMaceE1",
        "Metadata/Items/Weapons/OneHandWeapons/OneHandSwords/OneHandSwordE1",
        "Metadata/Items/Weapons/TwoHandWeapons/Staves/StaffE1",
        "Metadata/Items/Weapons/TwoHandWeapons/Staves/WarstaffE1",
        "Metadata/Items/Weapons/TwoHandWeapons/TwoHandAxes/TwoHandAxeE1",
        "Metadata/Items/Weapons/TwoHandWeapons/TwoHandMaces/TwoHandMaceE1",
        "Metadata/Items/Weapons/TwoHandWeapons/TwoHandSwords/TwoHandSwordE1",
        "Metadata/Items/Weapons/OneHandWeapons/Wands/WandE1",
        "Metadata/Items/Armours/Shields/ShieldDexE1",
        "Metadata/Items/Armours/Shields/ShieldIntE1",
        "Metadata/Items/Armours/Shields/ShieldStrE1",
        "Metadata/Items/Belts/BeltE2",
    ];
    let dropped: Vec<&str> = quivers
        .into_iter()
        .chain(tinctures)
        .chain(talismans)
        .chain(heist)
        .chain(["Metadata/Items/Amulets/Talismans/Talisman4"])
        .collect();
    assert_eq!(dropped.len(), 68);
    for id in &dropped {
        assert!(!ids.contains(id), "{id} is still in the bundle");
    }
    // The names the dropped bases duplicated are now unique.
    for name in [
        "Serrated Arrow Quiver",
        "Two-Point Arrow Quiver",
        "Sharktooth Arrow Quiver",
        "Blunt Arrow Quiver",
        "Fire Arrow Quiver",
        "Broadhead Arrow Quiver",
        "Penetrating Arrow Quiver",
        "Spike-Point Arrow Quiver",
        "Ironwood Tincture",
        "Ashbark Tincture",
        "Fulgurite Tincture",
        "Poisonberry Tincture",
        "Rosethorn Tincture",
        "Oakbranch Tincture",
        "Borealwood Tincture",
        "Blood Sap Tincture",
        "Black Maw Talisman",
        "Greatwolf Talisman",
    ] {
        assert_eq!(d.bases_named(name).len(), 1, "{name}");
    }
    // The data signals hold: no kept Tincture lacks its 3.25 implicit, and every kept Talisman
    // base carries the one placeholder.
    for (i, b) in d.bases.iter().enumerate() {
        let class = d.class_of(i as u16);
        if class.id == "Tincture" {
            assert!(!b.implicits.is_empty(), "{} lost its implicit", b.id);
        }
        if b.tags.iter().any(|&t| d.tag(t) == "talisman") {
            assert!(
                b.implicits
                    .iter()
                    .any(|&m| d.mod_at(m).id == "TalismanImplicitPlaceholder"),
                "{} lacks the talisman placeholder",
                b.id
            );
        }
    }
}
