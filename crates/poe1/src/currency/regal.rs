//! Orb of Regal: a magic item becomes rare and gains one modifier (`currency/regal`).

use super::{Currency, ItemOutputSet, ItemSetFilter, adds, with_rarity};
use crate::data::{Category, Data, Rarity};
use crate::item::Item;
use crate::method::{Pool, SideReq};

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
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be regaled");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be regaled");
        }
        if item.rarity != Rarity::Magic {
            return ItemOutputSet::refused("an Orb of Regal needs a magic item");
        }
        // `currency/regal` in the rules excludes flasks and tinctures: they carry categories the
        // orb does not work on.
        if !matches!(
            item.class(data).category,
            Category::Weapon
                | Category::Armour
                | Category::Jewellery
                | Category::Quiver
                | Category::Jewel
                | Category::AbyssJewel
        ) {
            return ItemOutputSet::refused("an Orb of Regal cannot be used on a flask or tincture");
        }
        let rare = with_rarity(item, Rarity::Rare);
        let Some(weighted) = adds(data, &rare, &Pool::Natural, 1, SideReq::Any) else {
            return ItemOutputSet::refused("the chances overflow exact arithmetic");
        };
        let set = ItemOutputSet::new(data, weighted);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
