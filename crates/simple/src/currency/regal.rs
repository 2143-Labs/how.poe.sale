//! Orb of Regal: a magic item becomes rare and gains one modifier (`currency/regal`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::data::Data;
use crate::item::{Item, Rarity};
use crate::pool::add_required;

/// The Orb of Regal, one step.
pub struct Regal<'d> {
    data: &'d Data,
}

impl<'d> Regal<'d> {
    pub fn new(data: &'d Data) -> Regal<'d> {
        Regal { data }
    }
}

impl Currency for Regal<'_> {
    fn id(&self) -> &'static str {
        "currency/regal"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be regaled");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be regaled");
        }
        if item.rarity != Rarity::Magic {
            return ItemOutputSet::refused("an Orb of Regal needs a magic item");
        }
        // The draw is taken on the *rare* item, which is what the orb hands to the next draw: a
        // magic item with one prefix may gain a second prefix, because a rare side has room for
        // three, and a modifier whose group is already held is still out of the pool.
        let rare = item.with_rarity(Rarity::Rare);
        let Some(weighted) = add_required(self.data, &rare) else {
            return ItemOutputSet::refused("no modifier can roll on this item");
        };
        let set = ItemOutputSet::new(weighted);
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
