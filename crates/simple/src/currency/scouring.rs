//! Orb of Scouring: strips a magic or rare item back to normal (`currency/scouring`).

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
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("an Orb of Scouring needs a magic or rare item");
        }
        if item.explicits() == 0 {
            return ItemOutputSet::refused("the item has no explicit modifiers to scour");
        }
        let scoured = Item::new(Rarity::Normal);
        let set = ItemOutputSet::new(vec![(scoured, 1, None)]);
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
