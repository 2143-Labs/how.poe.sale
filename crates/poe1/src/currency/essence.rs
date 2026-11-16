//! Essence: a reforge whose certain part is its own modifier (`essence/<name>`).
//!
//! The generated method (`crates/cli/src/methods.rs`) is `setRarity rare`, `clear`, `addEssence`,
//! `fill`. Everything but the `fill` is deterministic, so this file is the step before the fill:
//! an essence reforge is ranked from its certain part (README, "Limits of this version"), and the
//! random additions that follow are a multi-draw the item model does not enumerate.

use super::{Currency, ItemOutputSet, ItemSetFilter};
use crate::apply::{cannot_place, clear_changeable, essence_mod};
use crate::data::{Data, Rarity};
use crate::item::{Affix, Item};
use crate::method::{Family, Protection};

/// One essence, as the currency its generated method declares.
pub struct Essence<'d> {
    data: &'d Data,
    /// Position in `Bundle::essences`.
    index: u16,
    /// The method the generator declares for this essence, e.g.
    /// `essence/deafening-essence-of-hatred`. Its id is this currency's id, and its `requires`
    /// are the refusals below, so the two implementations cannot drift.
    method: &'d str,
}

impl<'d> Essence<'d> {
    /// The essence the bundle spells `name`, e.g. `Deafening Essence of Hatred`. `None` when the
    /// bundle has no such essence, or the generator declares no method for it — an essence with no
    /// modifier for any in-scope class has none.
    pub fn named(data: &'d Data, name: &str) -> Option<Essence<'d>> {
        let index = data.essence_by_name(name)?;
        let method = data
            .methods
            .iter()
            .find(|m| m.family == Family::Essence && m.name == name)?;
        Some(Essence {
            data,
            index,
            method: &method.id,
        })
    }
}

impl Currency for Essence<'_> {
    fn id(&self) -> &str {
        self.method
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        let data = self.data;
        let essence = &data.essences[self.index as usize];
        // The method's `notCorrupted` and `notMirrored`.
        if item.corrupted {
            return ItemOutputSet::refused("a corrupted item cannot be reforged with an essence");
        }
        if item.mirrored {
            return ItemOutputSet::refused("a mirrored item cannot be reforged with an essence");
        }
        // The method's `rarity` requirement: `normal`, or `rare` where the essence may reforge a
        // rare item (`allowsRare`). A magic item is outside it, as `currency/alteration` is
        // outside a rare one.
        let allowed = match item.rarity {
            Rarity::Normal => true,
            Rarity::Rare => essence.allows_rare,
            _ => false,
        };
        if !allowed {
            return ItemOutputSet::refused(if essence.allows_rare {
                "an essence needs a normal or rare item"
            } else {
                "this essence needs a normal item"
            });
        }
        // The method's `essenceFits`, which is also what its `addEssence` op resolves. An
        // `item_level_max` is not a refusal (claim `essence.ilvl`): it caps the level of the
        // reforge's random fill, which is not this step.
        let Some(m) = essence_mod(data, item, &essence.name) else {
            return ItemOutputSet::refused("this essence has no modifier for the item's class");
        };
        // `setRarity rare` then `clear` under the method's protection: an essence ignores every
        // metamod (claim `essence.ignores-metamods`), so both sides are cleared as if no lock or
        // Cannot Roll metamod were there. A fractured modifier still stays — it cannot be changed.
        let mut reforged = item.clone();
        reforged.rarity = Rarity::Rare;
        clear_changeable(data, Protection::IGNORE_ALL, &mut reforged);
        let Some(side) = data.mod_at(m).side else {
            return ItemOutputSet::refused("the essence's modifier has no side to go on");
        };
        // `addEssence` asks this before it adds, on the item the clear left behind.
        if cannot_place(data, &reforged, m).is_some() {
            return ItemOutputSet::refused("the essence's modifier cannot go on the item");
        }
        reforged.affixes_mut(side).push(Affix::new(m));
        // One outcome at the whole chance: the certain part of the reforge is deterministic.
        let set = ItemOutputSet::new(data, item, vec![(reforged, 1, Some((side, m)))]);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
