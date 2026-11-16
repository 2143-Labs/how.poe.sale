//! Orb of Annulment: removes one explicit modifier from a magic or rare item, in equal shares among
//! the modifiers it may take (`currency/annulment`).

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::data::Data;
use crate::item::{Item, Rarity, Side};

/// The Orb of Annulment, one step: one modifier goes, each in the same share.
pub struct Annulment<'d> {
    // Every orb takes the pool, so the registry builds them alike; a removal reads no weight.
    #[allow(dead_code)]
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
        if !matches!(item.rarity, Rarity::Magic | Rarity::Rare) {
            return ItemOutputSet::refused("an Orb of Annulment needs a magic or rare item");
        }
        if item.explicits() == 0 {
            return ItemOutputSet::refused("the item has no explicit modifiers to annul");
        }
        // One outcome per held modifier, each at weight 1: the orb picks a modifier, not a side, so
        // a side holding two of the item's four modifiers loses one twice as often. The rarity is
        // left as it was — an annulment does not recompute it.
        let mut weighted = Vec::with_capacity(item.explicits());
        for side in Side::BOTH {
            for index in 0..item.affixes(side).len() {
                let mut result = item.clone();
                result.affixes_mut(side).remove(index);
                weighted.push((result, 1, None));
            }
        }
        let set = ItemOutputSet::new(weighted);
        match filter {
            Some(filter) => set.filtered(filter),
            None => set,
        }
    }
}
