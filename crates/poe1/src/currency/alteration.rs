//! Orb of Alteration: rerolls a magic item into one or two fresh modifiers, half the time each way
//! (`currency/alteration` in `rules/methods.json`).

use super::scouring::SCOURING_PROTECTION;
use super::{Currency, ItemOutputSet, ItemSetFilter, adds, combine};
use crate::apply::clear_changeable;
use crate::data::{Data, Rarity};
use crate::item::Item;
use crate::method::SideReq;

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
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be altered");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be altered");
        }
        if item.rarity != Rarity::Magic {
            return ItemOutputSet::refused("an Orb of Alteration needs a magic item");
        }
        // `currency/alteration` declares the same protection as a scouring, so it clears through
        // the same predicate: a fractured modifier stays.
        let mut fresh = item.clone();
        clear_changeable(data, SCOURING_PROTECTION, &mut fresh);
        let (Some(one), Some(two)) = (
            adds(data, &fresh, 1, SideReq::Any),
            adds(data, &fresh, 2, SideReq::Any),
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
