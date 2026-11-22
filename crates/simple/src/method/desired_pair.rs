//! A desired modifier on each side of a magic item, then regal and exalt to the target
//! (`desired_pair`).
//!
//! The jewel's craft (catalogue row 13). A magic item is rolled until it holds the desired prefix
//! and the desired suffix, then regaled and exalted until it is the target. On a base whose rare
//! side holds two modifiers that is exactly one regal and one exalt: the regal fills one side and
//! the exalt is forced into the other.
//!
//! |item|move|
//! |---|---|
//! |normal|transmutation|
//! |magic, the desired modifier on both sides|regal|
//! |magic, the desired modifier on one side and nothing on the other|augmentation|
//! |magic, anything else|alteration|
//! |rare, the target|exit `rare`|
//! |rare, can still become the target|exalted|
//! |rare, cannot|scouring (the normal item is transmuted again: the method retries itself)|

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

pub struct DesiredPair;

impl Method for DesiredPair {
    fn name(&self) -> String {
        "desired_pair".to_string()
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
            Rarity::Magic if Side::BOTH.into_iter().all(|side| holds_desired(node, side)) => {
                Move::Use(CurrencyId::REGAL)
            }
            Rarity::Magic
                if Side::BOTH.into_iter().any(|side| {
                    holds_desired(node, side) && node.side(side.other()).is_empty()
                }) =>
            {
                Move::Use(CurrencyId::AUGMENTATION)
            }
            Rarity::Magic => Move::Use(CurrencyId::ALTERATION),
            Rarity::Rare if node.reached(target) => Move::Exit("rare"),
            Rarity::Rare if node.reachable(target) => Move::Use(CurrencyId::EXALTED),
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}

/// Whether the side holds the desired modifier.
fn holds_desired(node: &Node, side: Side) -> bool {
    node.side(side).contains(&Tier::Desired)
}
