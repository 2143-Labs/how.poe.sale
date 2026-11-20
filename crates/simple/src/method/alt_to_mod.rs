//! Alter until the desired modifier (`alt_to_mod`).
//!
//! The goal is a magic item holding the one desired modifier. A normal item is transmuted; a magic
//! item that holds it leaves; anything else is rerolled. The rule is:
//!
//! | node | move |
//! |---|---|
//! | normal | [`CurrencyId::TRANSMUTATION`] |
//! | magic, holding the desired modifier | exit `magic` |
//! | magic, `augment`, a side empty whose floors ask for a desired modifier | [`CurrencyId::AUGMENTATION`] |
//! | magic, otherwise | [`CurrencyId::ALTERATION`] |
//! | rare | [`CurrencyId::SCOURING`] |
//!
//! With `augment` the augmentation is part of the cheapest rule, not a second chance bolted on. On
//! the Hunter boots the desired modifier is a suffix, and a draw picks a side first in equal shares
//! among the sides with room; a magic item holding only a prefix therefore has one suffix slot open
//! and an augmentation can put a modifier there at a full suffix draw's odds, for less than an
//! alteration costs (at the 3.30 prices the spike is costed at: 0.067 for 1/617 against 0.091 for
//! 3/2468). The arithmetic is in `ai_research/spike-boots/model.py`: with the augmentation the run
//! spends 616 alterations and 617/4 augmentations; without it, 2465/3 alterations (821.67, the
//! figure in the research's catalogue).
//!
//! The rare arm is not reachable by the method's own moves. It exists because the craft's flat
//! chain tries every allowed currency at every place, and a regal there would land on a rare node;
//! scouring it restarts the method, which is what a player would do.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

/// Alter until the desired modifier; `augment` fills an empty side the goal wants a desired
/// modifier on with an augmentation instead of an alteration.
pub struct AltToMod {
    pub augment: bool,
}

impl Method for AltToMod {
    fn name(&self) -> String {
        "alt_to_mod".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic, Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "magic",
            rarity: Rarity::Magic,
        }]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        let open_for_desired =
            |side: Side| node.side(side).is_empty() && target.floors(side).contains(&Tier::Desired);
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic if node.holds(Tier::Desired) => Move::Exit("magic"),
            Rarity::Magic if self.augment && Side::BOTH.into_iter().any(open_for_desired) => {
                Move::Use(CurrencyId::AUGMENTATION)
            }
            Rarity::Magic => Move::Use(CurrencyId::ALTERATION),
            Rarity::Rare => Move::Use(CurrencyId::SCOURING),
        }
    }
}
