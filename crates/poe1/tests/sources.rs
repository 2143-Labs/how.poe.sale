//! V-A3: where each mod can come from, and which recipes that offers.

mod common;

use common::*;
use poe1::item::Rarity;
use poe1::market::Market;
use poe1::recipe;
use poe1::weights::{ModSource, sources};

/// Mods bucketed by their first (scarcest-last) source on a plain amulet; crafted mods excluded.
fn buckets(base_name: &str) -> [usize; 6] {
    let b = b();
    let base = base(base_name);
    let ctx = poe1::weights::SourceCtx::new(b, base, 0);
    let mut out = [0; 6];
    for (i, m) in b.mods.iter().enumerate() {
        if m.crafted {
            continue;
        }
        let s = ctx.sources(i as u32);
        let Some(first) = s.first() else { continue };
        let k = match first {
            ModSource::Native => 0,
            ModSource::Influence(_) => 1,
            ModSource::Essence(_) => 2,
            ModSource::Fossil(_) => 3,
            ModSource::Bench => 4,
            ModSource::Foreign(_) => 5,
        };
        out[k] += 1;
    }
    out
}

#[test]
fn amulet_source_counts() {
    let got = buckets("Lapis Amulet");
    println!("Lapis Amulet native/influence/essence/fossil/bench/foreign = {got:?}");
    // 264 native; the one fossil-only mod is Tangled Fossil's DelveAmuletVaalSkillDuration1.
    assert_eq!(got, [264, 135, 45, 1, 0, 0]);
    let wand = buckets("Omen Wand");
    println!("Omen Wand = {wand:?}");
    assert!(wand[3] > 0, "a caster weapon has fossil-only mods");
    assert!(wand[5] > 0, "a caster weapon has foreign (same-class) mods");
}

#[test]
fn single_mod_sources() {
    let b = b();
    let simplex = base("Simplex Amulet");
    assert_eq!(
        sources(
            b,
            simplex,
            0,
            mod_index("PercentageAllAttributesUberShaper1")
        ),
        vec![ModSource::Influence(0)]
    );
    let ess = sources(b, simplex, 0, mod_index("StrengthEssence7_"));
    assert!(
        matches!(ess.as_slice(), [ModSource::Essence(_)]),
        "StrengthEssence7_ on a Simplex: {ess:?}"
    );
    assert_eq!(
        sources(
            b,
            base("Lapis Amulet"),
            0,
            mod_index("GlobalSpellGemsLevel1")
        ),
        Vec::<ModSource>::new()
    );
}

fn offered(req_id: &str, rarity: Rarity) -> Vec<String> {
    let t = target("Lapis Amulet", 84, rarity, vec![req(&[req_id])]);
    let m = Market::new(vec![good("main", t, None, None)], Vec::new(), "main").unwrap();
    recipe::propose(b(), &m, 0, &prices(), &[])
        .into_iter()
        .filter_map(|p| p.process.recipe)
        .collect()
}

#[test]
fn recipes_follow_the_source() {
    let essence = offered("StrengthEssence7_", Rarity::Rare);
    println!("StrengthEssence7_: {essence:?}");
    assert!(essence.iter().any(|r| r == "essence"));
    assert!(!essence.iter().any(|r| r == "alt-spam" || r == "reforge"));
    let gem = offered("GlobalLightningGemLevel1", Rarity::Magic);
    println!("GlobalLightningGemLevel1: {gem:?}");
    assert!(gem.iter().any(|r| r == "alt-spam"));
    assert!(!gem.iter().any(|r| r == "essence"));
}
