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
pub mod alt_to_mod;
pub mod bench_life;
pub mod desired_pair;
pub mod essence_spam;
pub mod exalt_full;
pub mod exalt_to;
pub mod regal_to;
pub mod shaper_slam;

pub use alt_aug_regal::AltAugRegal;
pub use alt_pair::AltPair;
pub use alt_to_mod::AltToMod;
pub use bench_life::BenchLife;
pub use desired_pair::DesiredPair;
pub use essence_spam::EssenceSpam;
pub use exalt_full::ExaltFull;
pub use exalt_to::ExaltTo;
pub use regal_to::RegalTo;
pub use shaper_slam::ShaperSlam;

use crate::chain::{Arm, Next, SolveError, Solved, solve_named};
use crate::classes::{Class, Grade, Lens};
use crate::cost::{CurrencyId, Spend};
use crate::currency::{Currency, find};
use crate::filter::Tier;
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
///
/// A step that reads its own filter ([`Through`]) works toward that filter's target and ignores
/// this one.
pub struct Kit<'a, G: Grade = Tier> {
    pub law: &'a Law<G>,
    pub target: &'a Target,
    pub tools: &'a [&'a dyn Currency<Node<G>>],
}

impl<G: Grade> Clone for Kit<'_, G> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<G: Grade> Copy for Kit<'_, G> {}

pub trait Method {
    fn name(&self) -> String;

    /// The rarities it can be handed.
    fn takes(&self) -> &'static [Rarity];

    /// Every exit, and the rarity an item leaves with.
    fn exits(&self) -> &'static [ExitSpec];

    fn rule(&self, target: &Target, node: &Node) -> Move;
}

/// A method as a craft step runs it: over nodes of one kind of grade, and with a way of seeing a
/// node as the method's filter grades it.
///
/// Every [`Method`] is one over a filter's tiers. A step of a run with several filters is a
/// [`Through`]: the method, the lens of the filter its block reads, and that filter's target.
pub trait StepMethod<G: Grade> {
    fn name(&self) -> String;

    /// The rarities it can be handed.
    fn takes(&self) -> &'static [Rarity];

    /// Every exit, and the rarity an item leaves with.
    fn exits(&self) -> &'static [ExitSpec];

    fn rule(&self, target: &Target, node: &Node<G>) -> Move;

    /// The node as the method's filter grades it: what its rule reads, and what a report prints.
    fn view(&self, node: &Node<G>) -> Node<Tier>;
}

impl<M: Method> StepMethod<Tier> for M {
    fn name(&self) -> String {
        Method::name(self)
    }

    fn takes(&self) -> &'static [Rarity] {
        Method::takes(self)
    }

    fn exits(&self) -> &'static [ExitSpec] {
        Method::exits(self)
    }

    fn rule(&self, target: &Target, node: &Node) -> Move {
        Method::rule(self, target, node)
    }

    fn view(&self, node: &Node) -> Node {
        node.clone()
    }
}

/// A method run over a run's classes, reading them through one filter: its rule sees the node as
/// that filter grades it and works toward that filter's target.
pub struct Through {
    pub method: Box<dyn StepMethod<Tier>>,
    pub lens: Lens,
    pub target: Target,
}

impl StepMethod<Class> for Through {
    fn name(&self) -> String {
        self.method.name()
    }

    fn takes(&self) -> &'static [Rarity] {
        self.method.takes()
    }

    fn exits(&self) -> &'static [ExitSpec] {
        self.method.exits()
    }

    fn rule(&self, _target: &Target, node: &Node<Class>) -> Move {
        self.method.rule(&self.target, &self.lens.view(node))
    }

    fn view(&self, node: &Node<Class>) -> Node {
        self.lens.view(node)
    }
}

/// One node's move as arms: `Use` applies the currency, whose cost is certain; `Exit` ends the
/// method.
pub fn next<G: Grade>(
    method: &dyn StepMethod<G>,
    kit: &Kit<G>,
    node: &Node<G>,
) -> Result<Next<Node<G>>, SolveError> {
    let at = || format!("{} at {}", method.name(), method.view(node).label());
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
pub fn run<G: Grade>(
    method: &dyn StepMethod<G>,
    kit: &Kit<G>,
    entry: &[(Node<G>, f64)],
) -> Result<Solved<Node<G>>, SolveError> {
    solve_named(entry, &mut |node| next(method, kit, node), &|node| {
        format!("{} at {}", method.name(), method.view(node).label())
    })
}
