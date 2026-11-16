//! Orb of Annulment: removes one explicit modifier from a magic or rare item, in equal shares
//! among the modifiers a removal may take (`currency/annulment`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use super::REMOVAL_PROTECTION;
use crate::apply::removable;
use crate::data::{Data, Rarity};
use crate::item::Item;

/// The Orb of Annulment, one step: one modifier goes, each removable one in the same share.
pub struct Annulment<'d> {
    data: &'d Data,
}

impl<'d> Annulment<'d> {
    pub fn new(data: &'d Data) -> Annulment<'d> {
        Annulment { data }
    }
}

impl Currency for Annulment<'_> {
    fn id(&self) -> &'static str {
        "currency/annulment"
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        let data = self.data;
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be annulled");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be annulled");
        }
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("an Orb of Annulment needs a magic or rare item");
        }
        // `currency/annulment` requires one explicit modifier, and a fractured one counts: the
        // case where none of them may go is the removable set's, below.
        if item.explicits().count() == 0 {
            return ItemOutputSet::refused("the item has no explicit modifiers to annul");
        }
        // `currency/annulment` declares the same protection as a scouring — a lock is respected,
        // the Cannot Roll metamods are ignored — so this is the op path's own removable set.
        let positions = removable(data, REMOVAL_PROTECTION, item);
        if positions.is_empty() {
            return ItemOutputSet::refused("no modifier can be removed");
        }
        let weighted = positions
            .into_iter()
            .map(|(side, index)| {
                let mut it = item.clone();
                it.affixes_mut(side).remove(index);
                // Rarity follows the affixes only where an op recomputes it; `Op::RemoveRandom`
                // does not, so a rare whose last modifier goes stays rare.
                (it, 1, None)
            })
            .collect();
        let set = ItemOutputSet::new(data, item, weighted);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
