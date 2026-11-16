//! Eldritch embers and ichors: replace the item's implicit with one drawn from the orb's kind and
//! tier (`currency/{lesser,greater,grand,exceptional}-eldritch-{ember,ichor}`).
//!
//! `Op::EldritchImplicit` draws from the mods of one kind (`Exarch` for an ember, `Eater` for an
//! ichor) that carry the orb's `eldritch_tier` — and deliberately does *not* filter by item level:
//! embers and ichors have no level requirement, so a level-1 item still draws a high-tier implicit.
//! An orb replaces its *own* kind's implicit and leaves the other kind's implicit as it was (claim
//! `eldritch.ember-tier`, owner): the op's `retain` keeps exactly the opposite kind before pushing
//! the draw, so an item holding both kinds keeps one of each.
//!
//! Which method the orb is comes from the bundle rather than a hard-coded name: the generator
//! declares one method per (kind, tier) whose single op is that `eldritchImplicit`, so the pair
//! identifies the row and [`Currency::id`] is that method's id. A list of the eight ids here
//! would be a second list to keep in step with `rules/methods.json`.
//!
//! The method's declared `requires` are evaluated by `apply::requirements`; only the refusal text
//! is written here, because `ItemOutputSet::Refused` carries a `&'static str` and the requirement
//! check builds its reason at run time.

use super::{Currency, ItemOutputSet, ItemSetFilter, Weighted};
use crate::apply;
use crate::data::{Data, Eldritch as Kind, ModKind};
use crate::item::{Item, Roll};
use crate::method::{Method, Op};

/// One eldritch orb: its kind, its tier, and the method the bundle declares for that pair.
pub struct Eldritch<'d> {
    data: &'d Data,
    kind: Kind,
    tier: u8,
    /// The method whose single op is this orb's `eldritchImplicit`, e.g.
    /// `currency/greater-eldritch-ember`; its id is this currency's id.
    method: &'d Method,
}

impl<'d> Eldritch<'d> {
    /// The orb for `kind` at `tier`, or `None` when the bundle declares no method with that one
    /// `eldritchImplicit` op.
    pub fn new(data: &'d Data, kind: Kind, tier: u8) -> Option<Eldritch<'d>> {
        let method = data.methods.iter().find(|m| {
            matches!(
                m.ops.as_slice(),
                [Op::EldritchImplicit { eldritch, tier: t }] if *eldritch == kind && *t == tier
            )
        })?;
        Some(Eldritch {
            data,
            kind,
            tier,
            method,
        })
    }
}

impl Currency for Eldritch<'_> {
    fn id(&self) -> &str {
        &self.method.id
    }

    fn apply(&self, item: &Item, filter: &Option<ItemSetFilter>) -> ItemOutputSet {
        let data = self.data;
        // The op path's own requirement check: an uncorrupted, unmirrored, non-unique,
        // uninfluenced, non-synthesised body armour, boots, gloves or helmet whose implicits can
        // change, at normal, magic or rare rarity.
        let params = apply::Params::default();
        if apply::requirements(data, self.method, std::slice::from_ref(item), &params).is_err() {
            return ItemOutputSet::refused(
                "an eldritch orb needs an uncorrupted, unmirrored, non-unique, uninfluenced, \
                 non-synthesised body armour, boots, gloves or helmet whose implicits can change",
            );
        }
        let (kind, other) = match self.kind {
            Kind::Exarch => (ModKind::Exarch, ModKind::Eater),
            Kind::Eater => (ModKind::Eater, ModKind::Exarch),
        };
        // The op draws with no item-level cap; the base's domain is the pool's one restriction.
        let domain = data.base_at(item.base).domain.repoe();
        let candidates = apply::kind_weights(data, item, kind, false, |m| {
            m.domain == domain && m.eldritch_tier == Some(self.tier)
        });
        if candidates.is_empty() {
            return ItemOutputSet::refused("no implicit of this tier can roll on the item");
        }
        // `Op::EldritchImplicit`, step for step: the opposite kind's implicit survives, then the
        // draw is pushed with the pool's own weight.
        let weighted: Vec<Weighted> = candidates
            .into_iter()
            .map(|(m, weight)| {
                let mut it = item.clone();
                it.implicits.retain(|r| data.mod_at(r.m).kind == other);
                it.implicits.push(Roll { m, values: None });
                (it, weight, None)
            })
            .collect();
        let set = ItemOutputSet::new(data, item, weighted);
        match filter {
            Some(f) => set.filtered(data, f),
            None => set,
        }
    }
}
