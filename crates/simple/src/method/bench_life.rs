//! Bench the life prefix on a rare item, and leave (`bench_life`).
//!
//! | node | move |
//! |---|---|
//! | rare, nothing crafted, a prefix slot open | [`CurrencyId::BENCH`] |
//! | rare, otherwise | exit `done` |
//! | not rare | exit `scrap` |
//!
//! The crafted flag is what says the bench has already run: a second use is refused by the
//! currency ([`crate::currency::Bench`]), so the rule has to exit instead of naming it.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

/// Bench the life prefix, and leave.
pub struct BenchLife;

impl Method for BenchLife {
    fn name(&self) -> String {
        "bench_life".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[
            ExitSpec {
                name: "done",
                rarity: Rarity::Rare,
            },
            ExitSpec {
                name: "scrap",
                rarity: Rarity::Normal,
            },
        ]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        if node.rarity != Rarity::Rare {
            return Move::Exit("scrap");
        }
        let room = node.side(Side::Prefix).len() < target.caps.rare;
        if !node.crafted() && room {
            Move::Use(CurrencyId::BENCH)
        } else {
            Move::Exit("done")
        }
    }
}
