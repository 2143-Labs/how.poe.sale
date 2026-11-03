//! The header's example items: each parses with nothing left unrecognised.

mod common;

use common::*;
use poe1::item::Rarity;
use poe1::text::{TargetDraft, parse_target};

fn parse(text: &str) -> TargetDraft {
    let d = parse_target(b(), text);
    assert!(d.unmatched.is_empty(), "unmatched {:?}", d.unmatched);
    assert!(d.buy_note.is_none(), "an example sets no buy price");
    d
}

/// The first accepted mod id of each requirement, in order.
fn ids(d: &TargetDraft) -> Vec<String> {
    d.requirements
        .iter()
        .map(|r| b().mod_at(r.accepted[0]).id.clone())
        .collect()
}

fn expect(d: &TargetDraft, want: &[&str]) {
    assert_eq!(
        ids(d),
        want.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
}

const PHYS: [&str; 3] = [
    "LocalIncreasedPhysicalDamagePercent8",
    "LocalIncreasedPhysicalDamagePercentAndAccuracyRating8",
    "LocalAddedPhysicalDamageTwoHand9",
];

#[test]
fn simplex() {
    let d = parse(include_str!("../../../web/src/demo/simplex-amulet.txt"));
    expect(
        &d,
        &[
            "PercentageAllAttributesUberShaper1",
            "PercentageStrengthBodyInfluence2_",
            "DamagePer15StrengthInfluence1",
        ],
    );
}

#[test]
fn crown() {
    let d = parse(include_str!("../../../web/src/demo/archdemon-crown.txt"));
    let mut got = ids(&d);
    got.sort();
    let mut want = vec![
        "JunMasterVeiledSkillAreaOfEffectPercentAndAreaOfEffectGemLevel",
        "AreaOfEffectSupportedUber1_",
        "IncreasedChillEffectSupportedUber2",
        "SocketedGemsDealMoreElementalDamageEssence1",
        "SocketedSpellCriticalStrikeChanceUber3",
        "LocalIncreasedArmourAndEnergyShield6",
    ];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn bow_and_staff() {
    expect(
        &parse(include_str!("../../../web/src/demo/thicket-bow.txt")),
        &PHYS,
    );
    expect(
        &parse(include_str!("../../../web/src/demo/ezomyte-staff.txt")),
        &PHYS,
    );
}

#[test]
fn amulet() {
    let d = parse(include_str!("../../../web/src/demo/lapis-amulet.txt"));
    assert_eq!(d.rarity, Rarity::Magic);
    expect(&d, &["GlobalSkillGemLevel1"]);
}

#[test]
fn boots() {
    let d = parse(include_str!("../../../web/src/demo/two-toned-boots.txt"));
    expect(
        &d,
        &[
            "MovementVelocity5",
            "IncreasedLife8",
            "FireResist8",
            "ColdResist8",
            "LightningResist8",
        ],
    );
}
