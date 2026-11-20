//! Regal, then exalt until both sides are full (`exalt_full`).
//!
//! A magic item is regaled, and the rare item exalted until neither side has room, whether or not
//! it can still become the target: a miss is judged on the full item. A full item that is the
//! target leaves through `done`. One that is not leaves through `miss` as it is, so the craft says
//! what a miss is worth: scoured and started again, or annulled. A normal item, which only a move
//! other than the craft's own can make here, leaves through `scrap`.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

pub struct ExaltFull;

impl Method for ExaltFull {
    fn name(&self) -> String {
        "exalt_full".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Magic, Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "done",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "miss",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "scrap",
                rarity: Rarity::Normal,
            },
        ]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        let full = Side::BOTH
            .into_iter()
            .all(|side| node.side(side).len() >= target.caps.at(node.rarity));
        match node.rarity {
            Rarity::Normal => Move::Exit("scrap"),
            Rarity::Magic => Move::Use(CurrencyId::REGAL),
            Rarity::Rare if !full => Move::Use(CurrencyId::EXALTED),
            Rarity::Rare if node.reached(target) => Move::Exit("done"),
            Rarity::Rare => Move::Exit("miss"),
        }
    }
}
