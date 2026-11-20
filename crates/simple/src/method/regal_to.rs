//! Regal a magic item into a rare one, and leave (`regal_to`).
//!
//! | node | move |
//! |---|---|
//! | magic | [`CurrencyId::REGAL`] |
//! | rare | exit `rare` |
//! | normal | exit `normal` |
//!
//! A normal item cannot come from the method's own moves; it reaches this step only as a flat-chain
//! alternative, and is handed back through `normal` so a craft can restart it.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::item::Rarity;
use crate::node::Node;
use crate::target::Target;

/// Regal a magic item into a rare one, and leave.
pub struct RegalTo;

impl Method for RegalTo {
    fn name(&self) -> String {
        "regal_to".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "rare",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "normal",
                rarity: Rarity::Normal,
            },
        ]
    }

    fn rule(&self, _target: &Target, node: &Node) -> Move {
        match node.rarity {
            Rarity::Magic => Move::Use(CurrencyId::REGAL),
            Rarity::Rare => Move::Exit("rare"),
            Rarity::Normal => Move::Exit("normal"),
        }
    }
}
