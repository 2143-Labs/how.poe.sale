//! The shield's craft: alt-aug-regal until the item holds the totem prefix.
//!
//! One step. The goal is a rare shield with the totem prefix on it; every other modifier the
//! alterations, augmentations and the regal happen to leave is a bonus. Nothing is exalted: the
//! item is finished as soon as it holds the desired modifier, and it can be a component of
//! another craft from there.

use super::{Craft, Step, Wire};
use crate::item::Rarity;
use crate::method::AltAugRegal;

pub fn shield() -> Craft {
    Craft {
        name: "shield",
        start: Rarity::Normal,
        steps: vec![Step {
            method: Box::new(AltAugRegal),
            wires: vec![("rare", Wire::Done)],
        }],
    }
}
