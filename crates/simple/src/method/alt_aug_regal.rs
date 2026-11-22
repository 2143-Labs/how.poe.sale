//! Alt-Aug-Regal for the desired modifier (`alt_aug_regal`).
//!
//! The goal is any item that holds the desired modifier. The craft stops the moment it appears,
//! so there are three things it can hand over, and the method declares all three:
//!
//! - `magic`: a magic item whose only modifier is the desired one — it came on the transmutation
//!   or an alteration.
//! - `magic_pair`: a magic item that holds the desired modifier and one more — it came as the
//!   second modifier of a roll, or an augmentation added it.
//! - `rare`: a rare item that holds the desired modifier — it came on the regal.
//!
//! Nothing is exalted to fill the item. A magic item that does not hold the desired modifier is
//! augmented while a side is empty, which is a second chance at it, and then regaled: the regal's
//! own modifier is the third way the desired one can arrive. A rare that does not hold it is
//! scoured and the craft starts again from a normal item.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

pub struct AltAugRegal;

impl Method for AltAugRegal {
    fn name(&self) -> String {
        "alt_aug_regal".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "magic",
                rarity: Rarity::Magic,
            },
            ExitSpec {
                name: "magic_pair",
                rarity: Rarity::Magic,
            },
            ExitSpec {
                name: "rare",
                rarity: Rarity::Rare,
            },
        ]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic if node.holds(Tier::Desired) => {
                if Side::BOTH
                    .into_iter()
                    .any(|side| node.side(side).is_empty())
                {
                    Move::Exit("magic")
                } else {
                    Move::Exit("magic_pair")
                }
            }
            Rarity::Magic
                if Side::BOTH
                    .into_iter()
                    .any(|side| node.side(side).is_empty()) =>
            {
                Move::Use(CurrencyId::AUGMENTATION)
            }
            Rarity::Magic => Move::Use(CurrencyId::REGAL),
            Rarity::Rare if node.reached(target) => Move::Exit("rare"),
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}
