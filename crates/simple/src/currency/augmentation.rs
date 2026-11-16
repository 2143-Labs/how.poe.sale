//! Orb of Augmentation: adds one modifier to a magic item that has room (`currency/augmentation`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::data::Data;
use crate::item::{Item, Rarity, Side};
use crate::pool::{add_required, room};

/// The Orb of Augmentation, one step.
pub struct Augmentation<'d> {
    data: &'d Data,
}

impl<'d> Augmentation<'d> {
    pub fn new(data: &'d Data) -> Augmentation<'d> {
        Augmentation { data }
    }
}

impl Currency for Augmentation<'_> {
    fn id(&self) -> &'static str {
        "currency/augmentation"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        if item.rarity != Rarity::Magic {
            return ItemOutputSet::refused("an Orb of Augmentation needs a magic item");
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
