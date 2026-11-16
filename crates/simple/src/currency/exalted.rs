//! Exalted Orb: adds one modifier to a rare item that has room (`currency/exalted`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::data::Data;
use crate::item::{Item, Rarity, Side};
use crate::pool::{add_required, room};

/// The Exalted Orb, one step.
pub struct Exalted<'d> {
    data: &'d Data,
}

impl<'d> Exalted<'d> {
    pub fn new(data: &'d Data) -> Exalted<'d> {
        Exalted { data }
    }
}

impl Currency for Exalted<'_> {
    fn id(&self) -> &'static str {
        "currency/exalted"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        if item.rarity != Rarity::Rare {
            return ItemOutputSet::refused("an Exalted Orb needs a rare item");
        }
        if !Side::BOTH.into_iter().any(|side| room(item, side) > 0) {
            return ItemOutputSet::refused("the item has no room for another modifier");
        }
        let Some(weighted) = add_required(self.data, item) else {
            return ItemOutputSet::refused("no modifier can roll on this item");
        };
        let set = ItemOutputSet::new(weighted);
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
