//! The shield's craft: alt-aug-regal until the item holds the totem prefix.
//!
//! One step. The craft stops the moment the totem appears, so it ends at one of three places: a
//! magic item whose only modifier is the totem, a magic item that holds the totem and one more
//! modifier, or a rare item whose extra modifier is the totem. Every one of them is a finished
//! shield for this goal and can be a component of another craft. Nothing is exalted.

use super::{Craft, Step, Wire};
use crate::item::Rarity;
use crate::method::AltAugRegal;

pub fn shield() -> Craft {
    Craft {
        name: "shield".to_string(),
        start: Rarity::Normal,
        steps: vec![Step {
            method: Box::new(AltAugRegal),
            wires: vec![
                ("magic", Wire::Done),
                ("magic_pair", Wire::Done),
                ("rare", Wire::Done),
            ],
        }],
    }
}
