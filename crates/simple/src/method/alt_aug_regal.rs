//! Alt-Aug-Regal for the desired modifier (`alt_aug_regal`).
//!
//! Alter a magic item until it holds the desired modifier; augment it if a side is still empty;
//! regal it. The rare that comes out leaves through `rare` if it can still become the target. If
//! the regal's modifier ruled that out, the item is scoured and the method starts again, so every
//! item that leaves is worth carrying on with.

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
        &[ExitSpec {
            name: "rare",
            rarity: Rarity::Rare,
        }]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic if !node.holds(Tier::Desired) => Move::Use(CurrencyId::ALTERATION),
            Rarity::Magic
                if Side::BOTH
                    .into_iter()
                    .any(|side| node.side(side).is_empty()) =>
            {
                Move::Use(CurrencyId::AUGMENTATION)
            }
            Rarity::Magic => Move::Use(CurrencyId::REGAL),
            Rarity::Rare if node.reachable(target) => Move::Exit("rare"),
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}
