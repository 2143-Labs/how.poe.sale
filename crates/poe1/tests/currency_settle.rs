//! A currency result must not keep what the step invalidated.
//!
//! `apply::settle` is the rule the op path already uses: the display-only figures (`defences`) are a
//! function of the item's modifiers, quality and catalyst, so they go once any of those change, and
//! foresight is lost by modifying the item in any way. The per-currency model builds its results
//! itself, so it has to apply the same rule — otherwise a step changes an item's modifiers while
//! leaving numbers that describe a different item, and a `foreseen` flag that the game would clear.

use poe1::currency::{Annulment, Currency, Scouring};
use poe1::data::{Data, Rarity};
use poe1::item::{Item, ItemSpec};

/// Every paste in `fixtures/text` that carries display figures, as a built item.
fn items_with_defences(data: &Data) -> Vec<Item> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/text");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("the text fixtures") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a paste");
        let Ok(parsed) = poe1::text::parse(data, &text) else {
            continue;
        };
        if let Ok(item) = Item::from_spec(data, &parsed.spec)
            && item.defences.is_some()
        {
            out.push(item);
        }
    }
    out
}

#[test]
fn a_step_drops_the_display_figures_it_invalidated() {
    let data = poe1::test_data();
    let items = items_with_defences(data);
    assert!(
        !items.is_empty(),
        "at least one fixture paste must carry defences for this test to mean anything"
    );
    let mut checked = 0;
    for item in items {
        // A scouring changes the modifiers of a magic or rare item; a transmutation changes a
        // normal one's rarity and modifiers.
        let set = if matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            Scouring::new(data).apply(&item, &None)
        } else {
            poe1::currency::Transmutation::new(data).apply(&item, &None)
        };
        for outcome in set.outcomes() {
            assert!(
                outcome.item.defences.is_none(),
                "a step that changed the item kept stale display figures"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no outcome was checked");
}

#[test]
fn a_step_clears_foresight() {
    let data = poe1::test_data();
    let spec: ItemSpec = serde_json::from_str(
        r#"{
            "base": "Supreme Spiked Shield",
            "itemLevel": 87,
            "rarity": "rare",
            "influences": ["shaper"],
            "prefixes": [{"id": "IncreasedLife10"}],
            "suffixes": [{"id": "FireResist8"}],
            "foreseen": true
        }"#,
    )
    .expect("spec");
    let item = Item::from_spec(data, &spec).expect("the item");
    assert!(item.foreseen, "the fixture starts foreseen");
    let set = Annulment::new(data).apply(&item, &None);
    assert!(!set.outcomes().is_empty());
    for outcome in set.outcomes() {
        assert!(
            !outcome.item.foreseen,
            "modifying the item removes the ability to foresee"
        );
    }
}

/// `empty.refused`: using a currency on an item it has nothing to change is refused, and the
/// currency is not consumed. The Simplex amulet allows no explicit modifiers at all
/// (`limits [0, 0]`), so an alteration's first draw has nothing to take: the step is a refusal, not
/// a zero-total distribution whose mass vanishes.
///
/// This is the other side of the dead-end rule in `currency_chain`: a *later* draw that cannot
/// happen inside a `chance` branch keeps what the earlier draw gave the item, but the first draw is
/// what makes the step a use of the currency.
#[test]
fn a_step_that_produces_nothing_is_refused_not_an_empty_set() {
    let data = poe1::test_data();
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/text/simplex-amulet.txt"
    ))
    .expect("the paste");
    let parsed = poe1::text::parse(data, &text).expect("parse");
    let mut item = Item::from_spec(data, &parsed.spec).expect("item");
    item.rarity = Rarity::Magic;
    assert_eq!(
        item.limits(data),
        [0, 0],
        "the fixture must be the item with no explicit room"
    );
    match poe1::currency::Alteration::new(data).apply(&item, &None) {
        poe1::currency::ItemOutputSet::Refused(why) => {
            assert!(!why.is_empty(), "a refusal says why");
            assert!(
                why.contains("no outcome"),
                "the refusal must say the step produced nothing: {why}"
            );
        }
        other => panic!(
            "a step with no outcome must be refused, got {} outcomes / total {}",
            other.outcomes().len(),
            other.total()
        ),
    }
}
