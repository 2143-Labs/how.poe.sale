//! Methods: small state machines over an abstract item, one file each.
//!
//! A method is a rule — holding this, use that orb, or leave through this exit — and nothing else.
//! It does not know its own odds: [`run`] hands the rule to the chain solver, which follows every
//! outcome of every orb the rule names and reports the chance of each exit and what was spent
//! getting there.
//!
//! A method declares the rarities it can be handed ([`Method::takes`]) and, for each exit, the
//! rarity an item leaves with ([`Method::exits`]). A craft reads those two lists when it wires
//! methods together, so a magic item can never be handed to a method that exalts.

pub mod alt_aug_regal;
pub mod alt_pair;
pub mod desired_pair;
pub mod exalt_to;

pub use alt_aug_regal::AltAugRegal;
pub use alt_pair::AltPair;
pub use desired_pair::DesiredPair;
pub use exalt_to::ExaltTo;

use crate::chain::{Arm, Next, SolveError, Solved, solve};
use crate::cost::{CurrencyId, Spend};
use crate::currency::{Currency, find};
use crate::item::Rarity;
use crate::law::Law;
use crate::node::Node;
use crate::target::Target;

/// What a method does with one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    /// Use this currency on it.
    Use(CurrencyId),
    /// Leave the method through this exit.
    Exit(&'static str),
}

/// One way out of a method, and the rarity an item has when it takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitSpec {
    pub name: &'static str,
    pub rarity: Rarity,
}

/// What a method is run with: the law its draws follow, the target it works toward, and the
/// currencies it may name.
#[derive(Clone, Copy)]
pub struct Kit<'a> {
    pub law: &'a Law,
    pub target: &'a Target,
    pub tools: &'a [&'a dyn Currency<Node>],
}

pub trait Method {
    fn name(&self) -> String;

    /// The rarities it can be handed.
    fn takes(&self) -> &'static [Rarity];

    /// Every exit, and the rarity an item leaves with.
    fn exits(&self) -> &'static [ExitSpec];

    fn rule(&self, target: &Target, node: &Node) -> Move;
}

/// One node's move as arms: `Use` applies the currency, whose cost is certain; `Exit` ends the
/// method.
pub fn next(method: &dyn Method, kit: &Kit, node: &Node) -> Result<Next<Node>, SolveError> {
    let at = || format!("{} at {}", method.name(), node.label());
    match method.rule(kit.target, node) {
        Move::Exit(exit) => {
            let Some(spec) = method.exits().iter().find(|spec| spec.name == exit) else {
                return Err(SolveError::BadExit {
                    at: at(),
                    exit,
                    why: "the method does not declare this exit",
                });
            };
            if spec.rarity != node.rarity {
                return Err(SolveError::BadExit {
                    at: at(),
                    exit,
                    why: "the item leaves with a rarity the exit does not declare",
                });
            }
            Ok(Next::Exit)
        }
        Move::Use(id) => {
            let currency = find(kit.tools, id).ok_or(SolveError::UnknownCurrency(id))?;
            let applied = currency
                .apply(kit.law, node)
                .map_err(|why| SolveError::Refused {
                    at: at(),
                    currency: id,
                    why,
                })?;
            let spend = Spend::certain(&applied.cost);
            Ok(Next::Arms(
                applied
                    .outcomes
                    .parts()
                    .iter()
                    .map(|(to, chance)| Arm {
                        to: to.clone(),
                        chance: *chance,
                        spend: spend.clone(),
                    })
                    .collect(),
            ))
        }
    }
}

/// The method solved from an entry mix: every node it can pass through, the chance of each exit
/// node, and the spend.
pub fn run(
    method: &dyn Method,
    kit: &Kit,
    entry: &[(Node, f64)],
) -> Result<Solved<Node>, SolveError> {
    solve(entry, &mut |node| next(method, kit, node))
}
