//! The shield's craft: alt-aug-regal for the desired modifier, then three exalts, one per step.
//!
//! Each exalt is its own step so the report can say what it costs to get past each one. A rare
//! that an exalt has ruled out is scoured and goes back to the first step.

use super::{Craft, Step, Wire};
use crate::item::Rarity;
use crate::method::{AltAugRegal, ExaltTo};

pub fn shield() -> Craft {
    let exalt = |modifiers: u8, slammed: Wire| Step {
        method: Box::new(ExaltTo { modifiers }),
        wires: vec![("slammed", slammed), ("scrap", Wire::To(0))],
    };
    Craft {
        name: "shield",
        start: Rarity::Normal,
        steps: vec![
            Step {
                method: Box::new(AltAugRegal),
                wires: vec![("rare", Wire::Next)],
            },
            exalt(4, Wire::Next),
            exalt(5, Wire::Next),
            exalt(6, Wire::Done),
        ],
    }
}
