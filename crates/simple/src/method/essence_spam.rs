//! Use one essence until the item holds the desired modifier (`essence_spam(<essence>)`).
//!
//! An essence reforges a normal or a rare item into a rare one, so a miss needs no scouring: the
//! next essence reforges the rare item it left. The method leaves through `hit` with a rare item
//! holding a desired modifier, whatever else it holds.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::filter::Tier;
use crate::item::Rarity;
use crate::node::Node;
use crate::target::Target;

pub struct EssenceSpam {
    /// The essence used, by its id.
    pub essence: CurrencyId,
}

impl Method for EssenceSpam {
    fn name(&self) -> String {
        format!("essence_spam({})", self.essence.name())
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "hit",
            rarity: Rarity::Rare,
        }]
    }

    fn rule(&self, _target: &Target, node: &Node) -> Move {
        if node.rarity == Rarity::Rare && node.holds(Tier::Desired) {
            Move::Exit("hit")
        } else {
            Move::Use(self.essence)
        }
    }
}
