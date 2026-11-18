//! Alt-Aug-Regal for the desired modifier (`alt_aug_regal`).
//!
//! The goal is a rare item that holds the desired modifier; every other modifier on it is a
//! bonus, and nothing is exalted to fill the item.
//!
//! Transmute a normal item. On a magic item augment while a side is still empty — the next
//! modifier can be the desired one, and an augmentation costs a fraction of an alteration — and
//! alter while the desired modifier is missing and no side has room. Regal once the desired
//! modifier is held, which makes it rare and leaves through `rare`. A rare that does not hold the
//! desired modifier is scoured and the method starts again, so every item that leaves satisfies
//! the target.

use super::{ExitSpec, Kit, Method, Move};
use crate::cost::CurrencyId;
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::node::Node;

pub struct AltAugRegal;

impl Method for AltAugRegal {
    fn name(&self) -> String {
        "alt_aug_regal".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "rare",
            rarity: Rarity::Rare,
        }]
    }

    fn rule(&self, kit: &Kit, node: &Node) -> Move {
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic => {
                let open: Vec<Side> = Side::BOTH
                    .into_iter()
                    .filter(|side| node.side(*side).is_empty())
                    .collect();
                let hunt = open.iter().any(|side| rolls_desired(kit, *side, node));
                if !open.is_empty() && (node.holds(Tier::Desired) || hunt) {
                    Move::Use(CurrencyId::AUGMENTATION)
                } else if node.holds(Tier::Desired) {
                    Move::Use(CurrencyId::REGAL)
                } else {
                    Move::Use(CurrencyId::ALTERATION)
                }
            }
            Rarity::Rare if node.reached(kit.target) => Move::Exit("rare"),
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}

/// Whether an empty side of this node can still roll the desired grade. On this shield only the
/// prefixes can (`MaximumTotemUber1` is a prefix), so augmenting a magic item whose only empty
/// side is the suffixes is a modifier bought for an item that is about to be altered away.
fn rolls_desired(kit: &Kit, side: Side, node: &Node) -> bool {
    kit.law
        .next(side, node.side(side))
        .iter()
        .any(|(grade, _)| *grade == Tier::Desired)
}
