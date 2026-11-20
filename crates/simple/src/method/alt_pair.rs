//! Alt-Pair: alteration and augmentation until a magic item holds the desired modifier on both
//! sides (`alt_pair`).
//!
//! The goal is a magic item — a flask has no rare form — with one floor per side. There is one
//! way out: a magic item holding the desired modifier on each side. A magic item holding the
//! desired modifier on one side with the other side open is augmented, which is the second chance
//! at the pair; any other magic item is rerolled. Nothing is regaled or exalted, because a flask
//! cannot be rare.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

pub struct AltPair;

impl Method for AltPair {
    fn name(&self) -> String {
        "alt_pair".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "magic",
            rarity: Rarity::Magic,
        }]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        let empty = |side: Side| node.side(side).is_empty();
        let reached = |side: Side| target.side_reached(side, node.counts(side));
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic if reached(Side::Prefix) && reached(Side::Suffix) => Move::Exit("magic"),
            Rarity::Magic
                if (empty(Side::Prefix) && reached(Side::Suffix))
                    || (empty(Side::Suffix) && reached(Side::Prefix)) =>
            {
                Move::Use(CurrencyId::AUGMENTATION)
            }
            Rarity::Magic => Move::Use(CurrencyId::ALTERATION),
            // The method never makes a rare item; one handed in by mistake is scoured back.
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}
