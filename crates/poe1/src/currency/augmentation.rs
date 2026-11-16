//! Orb of Augmentation: adds one modifier to a magic item that has room
//! (`currency/augmentation`).

use super::{Currency, ItemOutputSet, ItemSetFilter, adds, has_room};
use crate::data::{Data, Rarity, Side};
use crate::item::Item;
use crate::method::SideReq;

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
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be augmented");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be augmented");
        }
        if item.rarity != Rarity::Magic {
            return ItemOutputSet::refused("an Orb of Augmentation needs a magic item");
        }
        if !Side::BOTH.into_iter().any(|s| has_room(data, item, s)) {
            return ItemOutputSet::refused("the item has no room for another modifier");
        }
        let Some(weighted) = adds(data, item, 1, SideReq::Any) else {
            return ItemOutputSet::refused("the chances overflow exact arithmetic");
        };
        let set = ItemOutputSet::new(weighted);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
