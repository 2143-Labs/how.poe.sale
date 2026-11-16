//! Beastcraft fracture: one explicit modifier on a rare Talisman becomes fractured, each
//! modifier that can still be fractured in the same share (`beast/fracture-talisman`).
//!
//! `Op::Fracture` is uniform over the affixes on both sides that are not already fractured and
//! whose kind is not `Veiled`: an unrevealed veiled modifier is not a modifier the item has yet,
//! so it is not one the beast can fracture. A fractured modifier cannot be fractured again, which
//! is also why the method declares `noFractured` — an item that already carries one is refused
//! before the op runs.
//!
//! The method's declared `requires` are evaluated by `apply::requirements` rather than restated
//! here, so a rule that changes in `rules/methods.json` changes this step too. Only the refusal
//! text is written here, because `ItemOutputSet::Refused` carries a `&'static str` and the
//! requirement check builds its reason at run time.

use super::{Currency, ItemOutputSet, ItemSetFilter, Weighted};
use crate::apply;
use crate::data::Data;
use crate::item::Item;
use crate::method::Method;

/// The beastcraft fracture, one step: one modifier becomes fractured, each fracturable one in the
/// same share.
pub struct Fracture<'d> {
    data: &'d Data,
    /// `beast/fracture-talisman`: the method whose requirements this step refuses on and whose
    /// single `fracture` op this file mirrors.
    method: &'d Method,
}

impl<'d> Fracture<'d> {
    /// The bundle's `beast/fracture-talisman`, or `None` when the bundle declares no such method.
    pub fn new(data: &'d Data) -> Option<Fracture<'d>> {
        let method = data.method("beast/fracture-talisman")?;
        Some(Fracture { data, method })
    }
}

impl Currency for Fracture<'_> {
    fn id(&self) -> &str {
        &self.method.id
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        let data = self.data;
        // The op path's own requirement check: a rare, uncorrupted, unmirrored, uninfluenced,
        // unfractured Talisman with at least four explicit modifiers.
        let params = apply::Params::default();
        if apply::requirements(data, self.method, std::slice::from_ref(item), &params).is_err() {
            return ItemOutputSet::refused(
                "a beastcraft fracture needs a rare, uncorrupted, unmirrored, uninfluenced, \
                 unfractured Talisman with at least four modifiers",
            );
        }
        // `Op::Fracture`, step for step: every affix on either side that is not already fractured
        // and is not an unrevealed veiled modifier, in equal shares. `apply::fracturable` is the
        // op arm's own predicate, so the two cannot disagree about what is on offer.
        let mut weighted: Vec<Weighted> = Vec::new();
        for (side, index) in apply::fracturable(data, item) {
            let mut fractured = item.clone();
            fractured.affixes_mut(side)[index].fractured = true;
            weighted.push((fractured, 1, None));
        }
        if weighted.is_empty() {
            // The op's own refusal, for an item the declared requirements let through.
            return ItemOutputSet::refused("no modifier can be fractured");
        }
        let set = ItemOutputSet::new(data, item, weighted);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
