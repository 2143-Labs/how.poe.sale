//! Exalt a rare item until it holds a number of modifiers (`exalt_to(n)`).
//!
//! One exalt at a time, for as long as the item can still become the target. It leaves through
//! `slammed` holding the modifiers asked for. An exalt that rules the target out is scoured, and
//! the normal item leaves through `scrap`: what to do with it is the craft's decision, not this
//! method's.

use super::{ExitSpec, Kit, Method, Move};
use crate::cost::CurrencyId;
use crate::item::Rarity;
use crate::node::Node;

pub struct ExaltTo {
    /// How many modifiers the item holds when it leaves through `slammed`.
    pub modifiers: u8,
}

impl Method for ExaltTo {
    fn name(&self) -> String {
        format!("exalt_to({})", self.modifiers)
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "slammed",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "scrap",
                rarity: Rarity::Normal,
            },
        ]
    }

    fn rule(&self, kit: &Kit, node: &Node) -> Move {
        if node.rarity != Rarity::Rare {
            Move::Exit("scrap")
        } else if !node.reachable(kit.target) {
            Move::Use(CurrencyId::SCOURING)
        } else if node.modifiers() < usize::from(self.modifiers) {
            Move::Use(CurrencyId::EXALTED)
        } else {
            Move::Exit("slammed")
        }
    }
}
