//! Prepare a rare with no influence, then slam a Shaper's Exalted Orb on it (`shaper_slam`).
//!
//! The route is the cheapest one research report 11 found at 3.30 prices: every draw before the
//! slam is on the shield's pool without an influence, and the totem comes last, from the orb.
//!
//! - A normal item is transmuted.
//! - A magic item with a good prefix and a good suffix is regaled; one holding a single good
//!   modifier is augmented on its empty side; any other is altered.
//! - A rare item is exalted until it holds two prefixes and three suffixes. With the suffixes full
//!   the orb can only add a prefix, and the Shaper's only prefix is the totem, so the slam cannot
//!   miss. A rare that can no longer reach the target ([`Node::reachable`]) is scoured, and the
//!   normal item starts again. `reachable` counts every open prefix slot as desired; on report 11's
//!   floors (`[desired, good, any]` / `[good, good, any]`) that asks exactly what the route needs —
//!   at most two prefixes, one of them good once there are two, and room for two good suffixes —
//!   but a target asking for two desired prefixes would need a sharper test.
//!
//! It leaves through `shaped` once the item clears the target. The kit's law has to be the pool
//! without an influence: the method draws on it before the slam and nothing after it, and a node
//! does not record that the slam made it a Shaper item.

use super::{ExitSpec, Method, Move};
use crate::cost::CurrencyId;
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::node::Node;
use crate::target::Target;

pub struct ShaperSlam;

impl Method for ShaperSlam {
    fn name(&self) -> String {
        "shaper_slam".to_string()
    }

    fn takes(&self) -> &'static [Rarity] {
        &[Rarity::Normal, Rarity::Magic, Rarity::Rare]
    }

    fn exits(&self) -> &'static [ExitSpec] {
        &[ExitSpec {
            name: "shaped",
            rarity: Rarity::Rare,
        }]
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        let good = |side: Side| {
            node.side(side)
                .iter()
                .any(|tier| tier.rank() >= Tier::Good.rank())
        };
        let empty = |side: Side| node.side(side).is_empty();
        match node.rarity {
            Rarity::Normal => Move::Use(CurrencyId::TRANSMUTATION),
            Rarity::Magic if good(Side::Prefix) && good(Side::Suffix) => {
                Move::Use(CurrencyId::REGAL)
            }
            Rarity::Magic
                if (good(Side::Prefix) && empty(Side::Suffix))
                    || (good(Side::Suffix) && empty(Side::Prefix)) =>
            {
                Move::Use(CurrencyId::AUGMENTATION)
            }
            Rarity::Magic => Move::Use(CurrencyId::ALTERATION),
            Rarity::Rare if node.reached(target) => Move::Exit("shaped"),
            Rarity::Rare if !node.reachable(target) => Move::Use(CurrencyId::SCOURING),
            Rarity::Rare
                if node.side(Side::Prefix).len() == 2 && node.side(Side::Suffix).len() == 3 =>
            {
                Move::Use(CurrencyId::SHAPERS_EXALTED)
            }
            Rarity::Rare => Move::Use(CurrencyId::EXALTED),
        }
    }
}
