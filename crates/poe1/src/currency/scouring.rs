//! Orb of Scouring: strips a magic or rare item back to normal (`currency/scouring`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::apply::clear_changeable;
use crate::data::{Data, Rarity};
use crate::item::Item;
use crate::method::{Policy, Protection};

/// What `rules/methods.json` declares for `currency/scouring`: a lock is respected, and since 3.30
/// no removal respects the Cannot Roll metamods (`remove.cannot-roll-scope`).
pub const SCOURING_PROTECTION: Protection = Protection {
    locks: Policy::Respect,
    cannot_roll: Policy::Ignore,
};

/// The Orb of Scouring, one step. It is deterministic, so the set is a single item at the whole
/// chance.
pub struct Scouring<'d> {
    data: &'d Data,
}

impl<'d> Scouring<'d> {
    pub fn new(data: &'d Data) -> Scouring<'d> {
        Scouring { data }
    }
}

impl Currency for Scouring<'_> {
    fn id(&self) -> &'static str {
        "currency/scouring"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be scoured");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be scoured");
        }
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("an Orb of Scouring needs a magic or rare item");
        }
        if item.explicits().count() == 0 {
            return ItemOutputSet::refused("the item has no explicit modifiers to scour");
        }
        let mut scoured = item.clone();
        if clear_changeable(data, SCOURING_PROTECTION, &mut scoured) == 0 {
            return ItemOutputSet::refused("no modifier can be removed");
        }
        // A fractured modifier cannot be removed, so the rarity follows what survived: nothing at
        // all is normal, one fractured modifier makes a magic item, two or more a rare one. A side
        // a respected lock kept is not fractured, and the rarity then stays as it was.
        let left: Vec<&crate::item::Affix> = scoured.explicits().collect();
        if left.iter().all(|a| a.fractured) {
            scoured.rarity = match left.len() {
                0 => Rarity::Normal,
                1 => Rarity::Magic,
                _ => Rarity::Rare,
            };
        }
        let set = ItemOutputSet::new(data, vec![(scoured, 1, None)]);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
