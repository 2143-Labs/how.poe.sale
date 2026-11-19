//! A craft: methods wired exit to entry, and what it costs to get past each of them.
//!
//! A craft is a list of steps. Each step is a method and, for every exit the method declares, a
//! wire saying where an item leaving through it goes: on to the next step, back to an earlier
//! one, or out of the craft. [`Craft::check`] reads the wiring before anything is solved, so a
//! step that would be handed an item of a rarity its method does not take is refused by name.
//!
//! The report is forward: the strategy is written down, and the report says what it costs.
//!
//! - A chance is used one step deep. Each method is solved on its own, and only its exits, their
//!   chances and what was spent reaching them cross a step boundary.
//! - What is carried through the steps is cost: [`Row::reach`] is the spend to get past a step,
//!   counting every failed attempt before it, with its spread. With `run` the spend of one run of
//!   the step and `chance` the share of runs that go on, `reach_k = (reach_(k-1) + run_k) / chance_k`.
//! - [`Other`] lists, for every place the craft passes through, what each other allowed move
//!   would cost from there, so a place where the written strategy is not the cheapest is visible.

pub mod shield;
pub mod spec;

use std::collections::HashMap;
use std::fmt;

use crate::chain::{Arm, Next, SolveError, Solved, solve};
use crate::cost::{Cost, CurrencyId, Spend};
use crate::item::Rarity;
use crate::method::{self, Kit, Method, Move};
use crate::node::Node;
use crate::weights::Weights;

/// Where an item goes when it leaves a step through an exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    /// On to the step after this one.
    Next,
    /// To the step with this index.
    To(usize),
    /// Out of the craft: the item is finished.
    Done,
}

pub struct Step {
    pub method: Box<dyn Method>,
    /// One wire per exit the method declares.
    pub wires: Vec<(&'static str, Wire)>,
}

pub struct Craft {
    pub name: String,
    /// The rarity of the item the craft starts from, which holds no modifiers.
    pub start: Rarity,
    pub steps: Vec<Step>,
}

/// Where an item is in a craft.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    At { step: usize, node: Node },
    Done(Node),
}

/// Why a craft cannot be reported.
#[derive(Clone, Debug, PartialEq)]
pub enum CraftError {
    /// The craft has no steps.
    Empty,
    /// The first step's method does not take the rarity the craft starts from.
    StartNotTaken {
        rarity: Rarity,
    },
    /// A declared exit with no wire.
    Unwired {
        step: usize,
        exit: &'static str,
    },
    /// A declared exit with more than one wire.
    WiredTwice {
        step: usize,
        exit: &'static str,
    },
    /// A wire for an exit the method does not declare.
    UnknownExit {
        step: usize,
        exit: &'static str,
    },
    /// A wire to a step the craft does not have.
    NoSuchStep {
        step: usize,
        exit: &'static str,
    },
    /// An exit that hands the step it is wired to a rarity that step's method does not take.
    WrongRarity {
        step: usize,
        exit: &'static str,
        rarity: Rarity,
        to: usize,
    },
    Solve(SolveError),
}

impl fmt::Display for CraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CraftError::Empty => write!(f, "the craft has no steps"),
            CraftError::StartNotTaken { rarity } => {
                write!(f, "step 0 does not take a {} item", rarity.name())
            }
            CraftError::Unwired { step, exit } => {
                write!(f, "step {step}'s exit `{exit}` is not wired")
            }
            CraftError::WiredTwice { step, exit } => {
                write!(f, "step {step}'s exit `{exit}` is wired more than once")
            }
            CraftError::UnknownExit { step, exit } => {
                write!(
                    f,
                    "step {step} has a wire for `{exit}`, which its method does not declare"
                )
            }
            CraftError::NoSuchStep { step, exit } => {
                write!(
                    f,
                    "step {step}'s exit `{exit}` is wired to a step the craft does not have"
                )
            }
            CraftError::WrongRarity {
                step,
                exit,
                rarity,
                to,
            } => write!(
                f,
                "step {to} does not take a {} item, and step {step}'s exit `{exit}` hands it one",
                rarity.name()
            ),
            CraftError::Solve(why) => write!(f, "{why}"),
        }
    }
}

impl From<SolveError> for CraftError {
    fn from(why: SolveError) -> CraftError {
        CraftError::Solve(why)
    }
}

/// One step of a craft, costed.
#[derive(Clone, Debug)]
pub struct Row {
    pub step: usize,
    pub name: String,
    pub takes: &'static [Rarity],
    /// The share of this step's runs that go on (leave through an exit wired `Next` or `Done`).
    pub chance: f64,
    /// `1 / chance`: how many times the step is run, on average, for one that goes on.
    pub tries: f64,
    /// Every exit the method declares, with its wire and the share of runs that leave through it.
    pub exits: Vec<(&'static str, Wire, f64)>,
    /// What one run of this step spends, from the mix of items it is handed.
    pub run: Spend,
    /// What it takes to get past this step, from the start of the craft, failed attempts included.
    pub reach: Spend,
}

/// One place the craft passes through, and one move it could make there instead of its own.
#[derive(Clone, Debug)]
pub struct Other {
    pub step: usize,
    pub node: Node,
    /// How often the craft is at this place, per finished item.
    pub visits: f64,
    pub planned: CurrencyId,
    /// The mean spend from here to the finished item, as written.
    pub planned_to_go: Cost,
    pub other: CurrencyId,
    /// The mean spend from here if the other move is made once and the craft followed after it.
    pub other_to_go: Cost,
    /// Whether the other move is cheaper, by the weights.
    pub better: bool,
}

pub struct Report {
    pub rows: Vec<Row>,
    /// The spend of the whole craft: the last row's `reach`.
    pub total: Spend,
    pub others: Vec<Other>,
    /// How many other moves were left out of [`Report::others`] because the craft has no rule for
    /// where they land (a step handed an item its method cannot move on).
    pub skipped: usize,
    /// The craft as one chain of single orb uses, solved from the start. It also holds every
    /// place an other move can land on, at weight zero.
    pub flat: Solved<Place>,
}

/// A move the craft could make at a place instead of its own: what it costs, and where it lands.
struct Alternative {
    place: Place,
    planned: CurrencyId,
    other: CurrencyId,
    cost: Cost,
    lands: Vec<(Place, f64)>,
}

/// How far a weighted cost must be below another to count as cheaper.
const CHEAPER: f64 = 1e-9;

/// Fewer visits than this is rounding: the craft does not pass through the place.
const VISITED: f64 = 1e-12;

impl Craft {
    /// Reads the wiring: every exit wired once, to a step that exists and takes what it is handed.
    pub fn check(&self) -> Result<(), CraftError> {
        let Some(first) = self.steps.first() else {
            return Err(CraftError::Empty);
        };
        if !first.method.takes().contains(&self.start) {
            return Err(CraftError::StartNotTaken { rarity: self.start });
        }
        for (step, this) in self.steps.iter().enumerate() {
            for spec in this.method.exits() {
                let exit = spec.name;
                match this.wires.iter().filter(|(name, _)| *name == exit).count() {
                    0 => return Err(CraftError::Unwired { step, exit }),
                    1 => {}
                    _ => return Err(CraftError::WiredTwice { step, exit }),
                }
            }
            for &(exit, wire) in &this.wires {
                let Some(spec) = this.method.exits().iter().find(|spec| spec.name == exit) else {
                    return Err(CraftError::UnknownExit { step, exit });
                };
                let to = match wire {
                    Wire::Next => step + 1,
                    Wire::To(to) => to,
                    Wire::Done => continue,
                };
                let Some(target) = self.steps.get(to) else {
                    return Err(CraftError::NoSuchStep { step, exit });
                };
                if !target.method.takes().contains(&spec.rarity) {
                    return Err(CraftError::WrongRarity {
                        step,
                        exit,
                        rarity: spec.rarity,
                        to,
                    });
                }
            }
        }
        Ok(())
    }

    /// The wire of the exit a node leaves a step through. `None` when the step's rule does not
    /// exit at this node.
    pub fn wire(&self, kit: &Kit, step: usize, node: &Node) -> Option<(&'static str, Wire)> {
        let this = &self.steps[step];
        let Move::Exit(exit) = this.method.rule(kit.target, node) else {
            return None;
        };
        this.wires.iter().copied().find(|(name, _)| *name == exit)
    }

    /// Where a node goes when it leaves a step.
    fn wired(&self, kit: &Kit, step: usize, node: &Node) -> Place {
        match self.wire(kit, step, node) {
            Some((_, Wire::Next)) => Place::At {
                step: step + 1,
                node: node.clone(),
            },
            Some((_, Wire::To(to))) => Place::At {
                step: to,
                node: node.clone(),
            },
            Some((_, Wire::Done)) => Place::Done(node.clone()),
            None => unreachable!("only a node the step's rule exits at is wired on"),
        }
    }

    /// One place of the craft as a chain of single orb uses: a use's outcomes stay in the step, an
    /// exit is a free move along its wire.
    pub fn flat_next(&self, kit: &Kit, place: &Place) -> Result<Next<Place>, SolveError> {
        let Place::At { step, node } = place else {
            return Ok(Next::Exit);
        };
        let method = &*self.steps[*step].method;
        Ok(Next::Arms(match method::next(method, kit, node)? {
            Next::Exit => vec![Arm {
                to: self.wired(kit, *step, node),
                chance: 1.0,
                spend: Spend::none(),
            }],
            Next::Arms(arms) => arms
                .into_iter()
                .map(|arm| Arm {
                    to: Place::At {
                        step: *step,
                        node: arm.to,
                    },
                    chance: arm.chance,
                    spend: arm.spend,
                })
                .collect(),
        }))
    }

    /// The craft, costed: one row per step, the total, and every other move.
    pub fn report(&self, kit: &Kit, weights: &Weights) -> Result<Report, CraftError> {
        self.check()?;
        let start = Place::At {
            step: 0,
            node: Node::fresh(self.start),
        };
        let step_of = |place: &Place| match place {
            Place::At { step, .. } => Some(*step),
            Place::Done(_) => None,
        };

        // The nested chain: one place's arms are its whole method, solved. Solved once per place.
        let mut solved: HashMap<Place, Vec<Arm<Place>>> = HashMap::new();
        let mut nested = |place: &Place| -> Result<Next<Place>, SolveError> {
            let Place::At { step, node } = place else {
                return Ok(Next::Exit);
            };
            if let Some(arms) = solved.get(place) {
                return Ok(Next::Arms(arms.clone()));
            }
            let method = &*self.steps[*step].method;
            let arms: Vec<Arm<Place>> = match method::next(method, kit, node)? {
                Next::Exit => vec![Arm {
                    to: self.wired(kit, *step, node),
                    chance: 1.0,
                    spend: Spend::none(),
                }],
                Next::Arms(_) => method::run(method, kit, &[(node.clone(), 1.0)])?
                    .arms(node)
                    .expect("a node with arms is inside its own run")
                    .into_iter()
                    .map(|arm| Arm {
                        to: self.wired(kit, *step, &arm.to),
                        chance: arm.chance,
                        spend: arm.spend,
                    })
                    .collect(),
            };
            solved.insert(place.clone(), arms.clone());
            Ok(Next::Arms(arms))
        };

        let mut rows: Vec<Row> = Vec::with_capacity(self.steps.len());
        for (k, this) in self.steps.iter().enumerate() {
            // Past step k: stopped at the first place after it, or out of the craft.
            let past = solve(
                std::slice::from_ref(&(start.clone(), 1.0)),
                &mut |place| match step_of(place) {
                    Some(step) if step <= k => nested(place),
                    _ => Ok(Next::Exit),
                },
            )?;
            // What step k is handed: where the craft first lands on it.
            let lands =
                solve(
                    std::slice::from_ref(&(start.clone(), 1.0)),
                    &mut |place| match step_of(place) {
                        Some(step) if step < k => nested(place),
                        _ => Ok(Next::Exit),
                    },
                )?;
            let handed: Vec<(Node, f64)> = lands
                .exits
                .iter()
                .zip(lands.exit_chances())
                .filter_map(|(place, chance)| match place {
                    Place::At { step, node } if *step == k => Some((node.clone(), chance)),
                    _ => None,
                })
                .collect();
            let whole: f64 = handed.iter().map(|(_, chance)| chance).sum();
            let handed: Vec<(Node, f64)> = handed
                .into_iter()
                .map(|(node, chance)| (node, chance / whole))
                .collect();
            let method = &*this.method;
            let run = method::run(method, kit, &handed)?;
            let exits: Vec<(&'static str, Wire, f64)> = this
                .wires
                .iter()
                .map(|&(exit, wire)| {
                    let share = run
                        .spend_into(&|node| {
                            self.wire(kit, k, node).map(|(name, _)| name) == Some(exit)
                        })
                        .map(|(share, _)| share)
                        .unwrap_or(0.0);
                    (exit, wire, share)
                })
                .collect();
            let chance = exits
                .iter()
                .filter(|(_, wire, _)| matches!(wire, Wire::Next | Wire::Done))
                .map(|&(_, _, share)| share)
                .sum();
            rows.push(Row {
                step: k,
                name: method.name(),
                takes: method.takes(),
                chance,
                tries: 1.0 / chance,
                exits,
                run: run.spend(),
                reach: past.spend(),
            });
        }
        let total = rows
            .last()
            .expect("a checked craft has a step")
            .reach
            .clone();

        // The flat chain, and every other move from every place it passes through.
        let mut flat_next = |place: &Place| self.flat_next(kit, place);
        let walked = solve(std::slice::from_ref(&(start.clone(), 1.0)), &mut flat_next)?;
        let mut moves: Vec<Alternative> = Vec::new();
        for place in &walked.inside {
            let Place::At { step, node } = place else {
                continue;
            };
            let Move::Use(planned) = self.steps[*step].method.rule(kit.target, node) else {
                continue;
            };
            for tool in kit.tools {
                if tool.id() == planned {
                    continue;
                }
                let Ok(applied) = tool.apply(kit.law, node) else {
                    continue;
                };
                let lands = applied
                    .outcomes
                    .parts()
                    .iter()
                    .map(|(node, chance)| {
                        (
                            Place::At {
                                step: *step,
                                node: node.clone(),
                            },
                            *chance,
                        )
                    })
                    .collect();
                moves.push(Alternative {
                    place: place.clone(),
                    planned,
                    other: tool.id(),
                    cost: applied.cost,
                    lands,
                });
            }
        }
        // Every landing at weight zero, so the chain knows what to finish from each. When one
        // landing has no rule, the moves that land somewhere unruled are left out, one by one.
        let entry_of = |moves: &[Alternative]| -> Vec<(Place, f64)> {
            let mut entry: Vec<(Place, f64)> = vec![(start.clone(), 1.0)];
            entry.extend(
                moves
                    .iter()
                    .flat_map(|alternative| alternative.lands.iter())
                    .map(|(place, _)| (place.clone(), 0.0)),
            );
            entry
        };
        let (flat, moves, skipped) = match solve(&entry_of(&moves), &mut flat_next) {
            Ok(flat) => (flat, moves, 0),
            Err(_) => {
                let before = moves.len();
                let kept: Vec<Alternative> = moves
                    .into_iter()
                    .filter(|alternative| {
                        solve(&entry_of(std::slice::from_ref(alternative)), &mut flat_next).is_ok()
                    })
                    .collect();
                let skipped = before - kept.len();
                (solve(&entry_of(&kept), &mut flat_next)?, kept, skipped)
            }
        };
        let visits: HashMap<&Place, f64> = flat
            .inside
            .iter()
            .zip(flat.visits.iter().copied())
            .collect();

        let weighed = |cost: &Cost| weights.value(&Spend::certain(cost));
        let mut others: Vec<Other> = Vec::new();
        for Alternative {
            place,
            planned,
            other,
            cost,
            lands,
        } in &moves
        {
            let Place::At { step, node } = place else {
                continue;
            };
            let visits = visits.get(place).copied().unwrap_or(0.0);
            if visits <= VISITED {
                continue;
            }
            let planned_to_go = flat.to_go(place).expect("a walked place is in the chain");
            let mut other_to_go = cost.clone();
            for (land, chance) in lands {
                let onward = flat.to_go(land).expect("every landing is an entry");
                other_to_go = other_to_go.plus(&onward.times(*chance));
            }
            let (was, now) = (weighed(&planned_to_go), weighed(&other_to_go));
            let better = was.unweighted == Cost::none()
                && now.unweighted == Cost::none()
                && now.value < was.value * (1.0 - CHEAPER);
            others.push(Other {
                step: *step,
                node: node.clone(),
                visits,
                planned: *planned,
                planned_to_go,
                other: *other,
                other_to_go,
                better,
            });
        }

        Ok(Report {
            rows,
            total,
            others,
            skipped,
            flat,
        })
    }
}
