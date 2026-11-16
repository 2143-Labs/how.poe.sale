//! Exalted Orb: adds one modifier to a rare item that has room (`currency/exalted`).

use super::{Currency, ItemOutputSet, ItemSetFilter, adds, has_room};
use crate::data::{Data, Rarity, Side};
use crate::item::Item;
use crate::method::{Pool, SideReq};

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
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be exalted");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be exalted");
        }
        if item.rarity != Rarity::Rare {
            return ItemOutputSet::refused("an Exalted Orb needs a rare item");
        }
        if !Side::BOTH.into_iter().any(|s| has_room(data, item, s)) {
            return ItemOutputSet::refused("the item has no room for another modifier");
        }
        let Some(weighted) = adds(data, item, &Pool::Natural, 1, SideReq::Any) else {
            return ItemOutputSet::refused("the chances overflow exact arithmetic");
        };
        let set = ItemOutputSet::new(data, item, weighted);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
