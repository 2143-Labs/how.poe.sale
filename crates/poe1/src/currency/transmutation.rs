//! Orb of Transmutation: a normal item becomes magic holding one or two modifiers, half the time
//! each way (`currency/transmutation` in `rules/methods.json`).

use super::{Currency, ItemOutputSet, ItemSetFilter, adds, combine, with_rarity};
use crate::data::{Data, Rarity};
use crate::item::Item;
use crate::method::SideReq;

/// The Orb of Transmutation, one step.
pub struct Transmutation<'d> {
    data: &'d Data,
}

impl<'d> Transmutation<'d> {
    pub fn new(data: &'d Data) -> Transmutation<'d> {
        Transmutation { data }
    }
}

impl Currency for Transmutation<'_> {
    fn id(&self) -> &'static str {
        "currency/transmutation"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be transmuted");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be transmuted");
        }
        if item.rarity != Rarity::Normal {
            return ItemOutputSet::refused("an Orb of Transmutation needs a normal item");
        }
        let magic = with_rarity(item, Rarity::Magic);
        let (Some(one), Some(two)) = (
            adds(data, &magic, 1, SideReq::Any),
            adds(data, &magic, 2, SideReq::Any),
        ) else {
            return ItemOutputSet::refused("the chances overflow exact arithmetic");
        };
        let Some(set) = combine(data, vec![(1, one), (1, two)]) else {
            return ItemOutputSet::refused("the chances overflow exact arithmetic");
        };
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
