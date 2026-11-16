//! Orb of Transmutation: a normal item becomes magic holding one or two modifiers, half the time
//! each way (`currency/transmutation`).

use super::{Currency, ItemOutputSet, ItemSetFilter, combine, half};
use crate::data::Data;
use crate::item::{Item, Rarity};
use crate::pool::adds;

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
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be transmuted");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be transmuted");
        }
        if item.rarity != Rarity::Normal {
            return ItemOutputSet::refused("an Orb of Transmutation needs a normal item");
        }
        let magic = item.with_rarity(Rarity::Magic);
        let (Some(one), Some(two)) = (adds(self.data, &magic, 1), adds(self.data, &magic, 2))
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
