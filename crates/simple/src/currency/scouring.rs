//! Orb of Scouring: strips a magic or rare item's explicit modifiers and returns it to normal
//! rarity, keeping the item itself — base, item level, influence, quality, flags and implicits
//! (`currency/scouring`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::data::Data;
use crate::item::{Item, Rarity};

/// The Orb of Scouring, one step. It is deterministic, so its set is one item at the whole chance.
pub struct Scouring<'d> {
    // Every orb takes the pool, so the registry builds them alike; a removal reads no weight.
    #[allow(dead_code)]
    data: &'d Data,
}

impl<'d> Scouring<'d> {
    pub fn new(data: &'d Data) -> Scouring<'d> {
        Scouring { data }
    }
}

impl Currency for Scouring<'_> {
    fn id(&self) -> &'static str {
        "currency/scouring"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be scoured");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be scoured");
        }
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("an Orb of Scouring needs a magic or rare item");
        }
        if item.explicits() == 0 {
            return ItemOutputSet::refused("the item has no explicit modifiers to scour");
        }
        // Everything but the explicits survives: the same base, item level, influence, quality,
        // flags and implicits, back at normal rarity. A scouring is not a fresh item.
        let scoured = item.cleared().with_rarity(Rarity::Normal);
        let set = ItemOutputSet::new(vec![(scoured, 1, None)]);
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
