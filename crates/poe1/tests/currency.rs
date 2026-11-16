//! The per-currency model: one step at a time, with exact chances.

use poe1::apply::draw_weights;
use poe1::currency::{
    Accept, Currency, Fraction, ItemOutputSet, ItemSetFilter, REMOVAL_PROTECTION, Tier,
    Transmutation, side_chance,
};
use poe1::data::{Data, Rarity, Side};
use poe1::item::{Item, ItemSpec};
use poe1::method::{Pool, Protection, SideReq};

/// The owner's shield: a Shaper Supreme Spiked Shield, item level 87, normal to begin with.
fn shield(data: &Data) -> Item {
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "normal",
            "influences": ["shaper"],
            "sockets": "G-W W"
        }"#,
    )
    .expect("spec");
    Item::from_spec(data, &spec).expect("the shield")
}

/// The owner's totem goal, compiled from its fixture: the one pattern the filter tests read.
fn totem_pattern(data: &Data) -> poe1::pattern::Pattern {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/patterns/totem-shield.json"
    );
    let spec: poe1::pattern::PatternSpec =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the pattern fixture"))
            .expect("pattern spec");
    poe1::pattern::Pattern::compile(data, &spec).expect("compiles")
}

/// A modifier's printed text with every number blanked, so a family can be named by what it says
/// rather than by an id: `+(145-159) to maximum Life` and `+(3-9) to maximum Life` both become
/// `+# to maximum Life`.
fn key(text: &str) -> String {
    let mut out = String::new();
    let mut last_hash = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !last_hash {
                out.push('#');
                last_hash = true;
            }
        } else {
            last_hash = false;
            out.push(c);
        }
    }
    out.replace("#-#", "#")
}

/// The modifiers of one side that a single draw on this item can reach, by weight.
fn pool(data: &Data, item: &Item, side: Side) -> Vec<(u32, u128)> {
    draw_weights(data, item, &Pool::Natural, SideReq::Any, false, false)
        .into_iter()
        .filter(|&(m, w)| data.mod_at(m).side == Some(side) && w > 0)
        .collect()
}

fn family_weight(data: &Data, pool: &[(u32, u128)], text: &str) -> u128 {
    pool.iter()
        .filter(|&&(m, _)| key(&data.mod_at(m).text) == key(text))
        .map(|&(_, w)| w)
        .sum()
}

#[test]
fn a_step_is_an_exact_distribution() {
    let data = poe1::test_data();
    let item = shield(data);
    let set = Transmutation::new(data).apply(&item, &None);
    assert!(set.exact(), "weights must sum to the total");
    let one = set.chance(|i| i.explicits().count() == 1).expect("mass");
    let two = set.chance(|i| i.explicits().count() == 2).expect("mass");
    assert_eq!(
        one.checked_add(two).unwrap(),
        Fraction::ONE,
        "the set is whole"
    );
    assert_eq!(one, Fraction::new(1, 2).unwrap(), "the branches are 50/50");
}

#[test]
fn a_currency_refuses_what_it_cannot_do() {
    let data = poe1::test_data();
    let item = shield(data);
    let set = poe1::currency::Alteration::new(data).apply(&item, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(_)),
        "alteration needs a magic item"
    );
}

/// The six currencies must mirror `rules/methods.json`, because the loop is a policy over these
/// exact steps: `currency/regal` names the categories it works on, and a flask is not one.
#[test]
fn regal_refuses_a_flask() {
    let data = poe1::test_data();
    let spec: ItemSpec =
        serde_json::from_str(r#"{"base": "Jade Flask", "itemLevel": 80, "rarity": "magic"}"#)
            .expect("spec");
    let flask = Item::from_spec(data, &spec).expect("the flask");
    let set = poe1::currency::Regal::new(data).apply(&flask, &None);
    assert!(
        matches!(set, ItemOutputSet::Refused(why) if why.contains("flask")),
        "a flask cannot be regaled"
    );
}

#[test]
fn the_pool_is_the_published_pool() {
    let data = poe1::test_data();
    let item = shield(data);
    let prefix = pool(data, &item, Side::Prefix);
    let suffix = pool(data, &item, Side::Suffix);
    let prefix_total: u128 = prefix.iter().map(|&(_, w)| w).sum();
    let suffix_total: u128 = suffix.iter().map(|&(_, w)| w).sum();
    // Every family total below is the row the published table gives for this base at item level
    // 87. Weights carry data's per-100 generation factor, so they are the published figures × 100.
    assert_eq!(
        family_weight(data, &prefix, "+(145-159) to maximum Life") / 100,
        11_000
    );
    assert_eq!(
        family_weight(data, &prefix, "+(69-73) to maximum Mana") / 100,
        12_000
    );
    assert_eq!(
        family_weight(data, &prefix, "+1 to maximum number of Summoned Totems") / 100,
        800
    );
    assert_eq!(
        family_weight(data, &suffix, "+(46-48)% to Fire Resistance") / 100,
        8_000
    );
    assert_eq!(prefix_total / 100, 64_800);
    assert_eq!(suffix_total / 100, 96_475);
}

/// The goal is written once, in the pattern language, and the step filter reads it from there —
/// no second vocabulary for "desired / good / any".
#[test]
fn the_goal_comes_from_the_pattern() {
    let data = poe1::test_data();
    let pattern = totem_pattern(data);
    let filter = ItemSetFilter::from_pattern(&pattern, Accept::DesiredOrGoodBothSides);

    let mut magic = shield(data);
    magic.rarity = Rarity::Magic;
    let share = |side: Side, tier: Tier| {
        side_chance(data, &magic, &Pool::Natural, side, |m, _| {
            filter.tier_of_mod(data, m) == tier
        })
        .expect("a side with a pool")
    };
    assert_eq!(
        share(Side::Prefix, Tier::Desired),
        Fraction::new(800, 64_800).unwrap()
    );
    assert_eq!(
        share(Side::Prefix, Tier::Good),
        Fraction::new(4_000, 64_800).unwrap()
    );
    assert_eq!(share(Side::Suffix, Tier::Desired), Fraction::ZERO);
    assert_eq!(
        share(Side::Suffix, Tier::Good),
        Fraction::new(14_600, 96_475).unwrap()
    );
}

/// The filter parameter of a currency step is a real path, not a decoration: the outcomes the
/// filter accepts keep their weights untouched, the rejected ones are dropped, and the chances are
/// renormalised over exactly the kept mass. The dropped mass is reported only by the *unfiltered*
/// set — see [`ItemOutputSet::filtered`]'s doc.
#[test]
fn a_filtered_step_renormalises_exactly() {
    let data = poe1::test_data();
    let item = shield(data);
    let currency = Transmutation::new(data);
    let unfiltered = currency.apply(&item, &None);
    assert!(
        unfiltered.exact(),
        "the step's own weights are a distribution"
    );

    // The totem pattern's desired bucket, keeping only outcomes that hold the wanted prefix.
    let pattern = totem_pattern(data);
    let filter = ItemSetFilter::from_pattern(&pattern, Accept::HasDesired);

    let filtered = currency.apply(&item, &Some(filter.clone()));
    assert_eq!(
        filtered,
        unfiltered.filtered(data, &filter),
        "every currency's filter arm is the set's filtered"
    );
    assert!(
        filtered.exact(),
        "the renormalised weights still sum to the total"
    );

    let kept: Vec<&poe1::currency::Outcome> = unfiltered
        .outcomes()
        .iter()
        .filter(|o| filter.accepts(data, &o.item))
        .collect();
    assert!(
        !kept.is_empty(),
        "some transmutation outcome holds the totem prefix"
    );
    let kept_mass = kept
        .iter()
        .try_fold(0u128, |sum, o| sum.checked_add(o.weight))
        .expect("the kept weights fit");
    assert_eq!(
        filtered.total(),
        kept_mass,
        "the set's total is exactly the kept mass"
    );
    assert_eq!(
        filtered.outcomes().len(),
        kept.len(),
        "only the rejected outcomes go"
    );
    let mut whole = Fraction::ZERO;
    for (survivor, original) in filtered.outcomes().iter().zip(&kept) {
        assert_eq!(
            survivor, *original,
            "an accepted outcome keeps its weight and its modifier"
        );
        assert!(
            filter.holds_desired(data, &survivor.item),
            "a survivor is what the filter accepted"
        );
        let chance = Fraction::new(survivor.weight, kept_mass).expect("a chance");
        whole = whole.checked_add(chance).expect("exact");
    }
    assert!(whole.is_one(), "the filtered chances are whole: {whole}");

    // Filtering never rescales, so the kept share of the *step* is the filtered total over the
    // unfiltered one — and it complements the dropped mass exactly.
    let accepts = |i: &Item| filter.accepts(data, i);
    let kept_share = unfiltered.chance(|i| accepts(i)).expect("mass");
    let dropped = unfiltered.chance(|i| !accepts(i)).expect("mass");
    assert!(
        kept_share.checked_add(dropped).expect("exact").is_one(),
        "the two shares are the whole step"
    );
    assert_eq!(
        Fraction::new(kept_mass, unfiltered.total()).expect("a chance"),
        kept_share,
        "the filtered total over the step's total is the accepted share"
    );
}

/// A filter that accepts nothing refuses the step rather than returning an empty distribution,
/// which would read as "0 %" instead of "this step cannot reach the goal".
#[test]
fn a_filter_that_accepts_nothing_refuses_the_step() {
    let data = poe1::test_data();
    let item = shield(data);
    let empty = ItemSetFilter::new(Vec::new(), Vec::new(), Accept::DesiredOrGoodBothSides);
    let set = Transmutation::new(data).apply(&item, &Some(empty));
    assert!(
        matches!(set, ItemOutputSet::Refused(why) if why.contains("filter")),
        "{set:?}"
    );
}

/// A fractured modifier cannot be removed, so a scour leaves it and the rarity follows what is
/// left — the same rule `Op::Scour` applies, because both go through `can_change`.
#[test]
fn scouring_keeps_a_fractured_modifier() {
    let data = poe1::test_data();
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "rare",
            "influences": ["shaper"],
            "prefixes": [
                {"id": "IncreasedLife10", "fractured": true},
                {"id": "IncreasedMana12"}
            ]
        }"#,
    )
    .expect("spec");
    let item = Item::from_spec(data, &spec).expect("the item");
    let set = poe1::currency::Scouring::new(data).apply(&item, &None);
    let outcomes = set.outcomes();
    assert_eq!(outcomes.len(), 1, "a scour is deterministic");
    let left = &outcomes[0].item;
    assert_eq!(left.prefixes.len(), 1, "only the plain prefix goes");
    assert!(left.prefixes[0].fractured, "the fractured one stays");
    assert_eq!(
        left.rarity,
        Rarity::Magic,
        "one fractured modifier is magic"
    );
}

/// The code's removal policy must be the one the rules declare, or a currency written as code and
/// the same currency written as ops would drift apart.
///
/// The rule's `protect` object is read through the same [`Protection`] deserialiser the ops path
/// uses and compared with [`REMOVAL_PROTECTION`] itself, so a change to either side fails here.
/// Comparing the JSON against string literals, as an earlier version did, would notice a rule
/// edit but never a change to the constant the code actually applies — and it forgot annulment,
/// the third method that shares it.
#[test]
fn the_code_removal_policy_matches_the_rules() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../rules/methods.json");
    let rules: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the rules")).expect("json");
    for id in [
        "currency/scouring",
        "currency/alteration",
        "currency/annulment",
    ] {
        let method = rules["methods"]
            .as_array()
            .expect("methods")
            .iter()
            .find(|m| m["id"] == id)
            .unwrap_or_else(|| panic!("{id} is in the rules"));
        let declared: Protection = serde_json::from_value(method["protect"].clone())
            .unwrap_or_else(|e| panic!("{id}: the rule's protect does not parse: {e}"));
        assert_eq!(
            declared, REMOVAL_PROTECTION,
            "{id} must declare exactly the code's removal policy"
        );
    }
}
