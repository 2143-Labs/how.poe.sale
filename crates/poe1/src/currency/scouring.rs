//! Orb of Scouring: strips a magic or rare item back to normal (`currency/scouring`).

use super::{Currency, ItemOutputSet, ItemSetFilter, with_rarity};
use crate::data::{Data, Rarity};
use crate::item::Item;

/// The Orb of Scouring, one step. It is deterministic, so the set is a single item at the whole
/// chance.
pub struct Scouring<'d> {
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
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be scoured");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be scoured");
        }
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("an Orb of Scouring needs a magic or rare item");
        }
        if item.explicits().count() == 0 {
            return ItemOutputSet::refused("the item has no explicit modifiers to scour");
        }
        let mut normal = with_rarity(item, Rarity::Normal);
        normal.prefixes.clear();
        normal.suffixes.clear();
        let set = ItemOutputSet::new(vec![(normal, 1, None)]);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
