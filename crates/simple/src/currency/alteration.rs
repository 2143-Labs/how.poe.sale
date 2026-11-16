//! Orb of Alteration: rerolls a magic item into one or two fresh modifiers, half the time each way
//! (`currency/alteration`).

use super::{Currency, ItemOutputSet, ItemSetFilter, combine, half};
use crate::data::Data;
use crate::item::{Item, Rarity};
use crate::pool::adds;

/// The Orb of Alteration, one step.
pub struct Alteration<'d> {
    data: &'d Data,
}

impl<'d> Alteration<'d> {
    pub fn new(data: &'d Data) -> Alteration<'d> {
        Alteration { data }
    }
}

impl Currency for Alteration<'_> {
    fn id(&self) -> &'static str {
        "currency/alteration"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        if item.rarity != Rarity::Magic {
            return ItemOutputSet::refused("an Orb of Alteration needs a magic item");
        }
        // Every modifier goes before the new ones are drawn, so the pool the draws see is the pool
        // of an empty magic item, not of the item that was just emptied.
        let fresh = item.cleared();
        let (Some(one), Some(two)) = (adds(self.data, &fresh, 1), adds(self.data, &fresh, 2))
        else {
            return ItemOutputSet::refused("the chances do not fit in 128 bits");
        };
        let Some(set) = combine(vec![(half(), one), (half(), two)]) else {
            return ItemOutputSet::refused("the chances do not fit in 128 bits");
        };
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
