//! A method's diagram as data: one box per kind of item a step passes through, and how often the
//! craft goes from one box to another.
//!
//! The one thing decided here is how places merge into boxes: a place whose move forgets what the
//! item holds (transmutation, alteration, scouring) is one box per rarity and move, and a place
//! whose move keeps the item is one box per rarity, grades held and move. [`crate::report`] prints
//! a diagram as Mermaid; the web page draws it.

use std::collections::HashMap;

use crate::chain::Next;
use crate::classes::Grade;
use crate::cost::{Cost, CurrencyId, Spend};
use crate::craft::{Craft, Place, Report, Wire};
use crate::filter::Tier;
use crate::item::{Rarity, Side};
use crate::method::{Kit, Move};
use crate::node::Node;
use crate::weights::Weights;

/// The moves that forget what the item holds.
const FORGETS: [CurrencyId; 3] = [
    CurrencyId::TRANSMUTATION,
    CurrencyId::ALTERATION,
    CurrencyId::SCOURING,
];

/// Fewer visits than this is rounding: the craft does not pass through the place.
const VISITED: f64 = 1e-12;

/// One step's diagram: its boxes in the order the craft first reaches them, and the edges between
/// them in the order they were first taken.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagram {
    pub boxes: Vec<DiagramBox>,
    pub edges: Vec<DiagramEdge>,
}

/// One box, and how often the craft is in it per finished item.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramBox {
    pub kind: BoxKind,
    pub visits: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoxKind {
    /// A move that forgets what the item holds: one box per (rarity, move).
    Forgets {
        rarity: Rarity,
        currency: CurrencyId,
    },
    /// A move that keeps the item: grades per side, best first; the planned move with its cost
    /// to finish (if priced); every other move with its cost, cheapest first.
    Keeps {
        rarity: Rarity,
        sides: [Vec<Tier>; 2],
        currency: CurrencyId,
        cost: Option<f64>,
        options: Vec<(CurrencyId, f64)>,
    },
    /// Leaving the method; `to` is "step N" or "done".
    Exit { exit: &'static str, to: String },
}

/// The share of a box's visits that go on to another box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiagramEdge {
    pub from: usize,
    pub to: usize,
    pub share: f64,
}

/// What a box is keyed by while the places are merged.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Cell {
    Forgets(Rarity, CurrencyId),
    Keeps(Rarity, [Vec<Tier>; 2], CurrencyId),
    Exit(&'static str),
}

impl Cell {
    /// The box a node is in: its move, and for a move that keeps the item, the grades the step's
    /// filter sees.
    fn of<G: Grade>(craft: &Craft<G>, kit: &Kit<G>, step: usize, node: &Node<G>) -> Cell {
        let method = &craft.steps[step].method;
        match method.rule(kit.target, node) {
            Move::Exit(exit) => Cell::Exit(exit),
            Move::Use(id) if FORGETS.contains(&id) => Cell::Forgets(node.rarity, id),
            Move::Use(id) => Cell::Keeps(node.rarity, best_first(&method.view(node)), id),
        }
    }
}

/// A node's grades per side, best first.
fn best_first(node: &Node) -> [Vec<Tier>; 2] {
    Side::BOTH.map(|side| {
        let mut held = node.side(side).to_vec();
        held.sort_by_key(|tier| std::cmp::Reverse(tier.rank()));
        held
    })
}

/// The moves an item allows: its own, then every other currency that works on it, each with what
/// it would cost to finish the craft from there.
type Options = Vec<(CurrencyId, f64)>;

/// The diagram of one step of a reported craft.
///
/// Places are boxed by what the step's filter sees, so in a craft of several filters two places
/// whose grades differ only under another block's filter share a box. A keeps box's costs are
/// those of the first place the other-moves pass reached with its grades, as with one filter, where
/// draw orders that sort alike share a box too.
pub fn diagram<G: Grade>(
    craft: &Craft<G>,
    kit: &Kit<G>,
    report: &Report<G>,
    step: usize,
    weights: &Weights,
) -> Diagram {
    let flat = &report.flat;
    let mut cells: Vec<Cell> = Vec::new();
    let mut index: HashMap<Cell, usize> = HashMap::new();
    let mut weight: Vec<f64> = Vec::new();
    let mut edges: Vec<((usize, usize), f64)> = Vec::new();
    let mut edge_at: HashMap<(usize, usize), usize> = HashMap::new();
    let mut slot = |found: Cell, cells: &mut Vec<Cell>, weight: &mut Vec<f64>| {
        *index.entry(found.clone()).or_insert_with(|| {
            cells.push(found);
            weight.push(0.0);
            cells.len() - 1
        })
    };
    // What every move at every place the craft passes would cost to finish, keyed by the item's
    // grades so it lines up with the boxes.
    let valued = |cost: &Cost| weights.value(&Spend::certain(cost)).value;
    let mut options: HashMap<(Rarity, [Vec<Tier>; 2]), Options> = HashMap::new();
    for other in &report.others {
        if other.step != step {
            continue;
        }
        let key = best_first(&craft.steps[step].method.view(&other.node));
        let entry = options.entry((other.node.rarity, key)).or_default();
        for (id, cost) in [
            (other.planned, &other.planned_to_go),
            (other.other, &other.other_to_go),
        ] {
            if !entry.iter().any(|(at, _)| *at == id) {
                entry.push((id, valued(cost)));
            }
        }
    }

    for (place, &visits) in flat.inside.iter().zip(&flat.visits) {
        let Place::At { step: at, node } = place else {
            continue;
        };
        if *at != step || visits <= VISITED {
            continue;
        }
        let from = slot(Cell::of(craft, kit, step, node), &mut cells, &mut weight);
        weight[from] += visits;
        let Ok(Next::Arms(arms)) = craft.flat_next(kit, place) else {
            continue;
        };
        for arm in arms {
            let Place::At {
                step: to_step,
                node: to,
            } = &arm.to
            else {
                continue;
            };
            if *to_step != step {
                continue;
            }
            let to = slot(Cell::of(craft, kit, step, to), &mut cells, &mut weight);
            let at = *edge_at.entry((from, to)).or_insert_with(|| {
                edges.push(((from, to), 0.0));
                edges.len() - 1
            });
            edges[at].1 += visits * arm.chance;
        }
    }

    let boxes = cells
        .into_iter()
        .zip(weight.iter().copied())
        .map(|(cell, visits)| {
            let kind = match cell {
                Cell::Forgets(rarity, currency) => BoxKind::Forgets { rarity, currency },
                Cell::Keeps(rarity, sides, currency) => {
                    let found = options.get(&(rarity, sides.clone()));
                    let cost = found
                        .and_then(|o| o.iter().find(|(at, _)| *at == currency))
                        .map(|&(_, cost)| cost);
                    let mut others: Vec<(CurrencyId, f64)> = found
                        .map(|o| {
                            o.iter()
                                .filter(|(at, _)| *at != currency)
                                .copied()
                                .collect()
                        })
                        .unwrap_or_default();
                    others.sort_by(|a, b| a.1.total_cmp(&b.1));
                    BoxKind::Keeps {
                        rarity,
                        sides,
                        currency,
                        cost,
                        options: others,
                    }
                }
                Cell::Exit(exit) => {
                    let to = craft.steps[step]
                        .wires
                        .iter()
                        .find(|(name, _)| *name == exit)
                        .map(|&(_, wire)| match wire {
                            Wire::Next => format!("step {}", step + 1),
                            Wire::To(to) => format!("step {to}"),
                            Wire::Done => "done".to_string(),
                        })
                        .unwrap_or_default();
                    BoxKind::Exit { exit, to }
                }
            };
            DiagramBox { kind, visits }
        })
        .collect();
    let edges = edges
        .into_iter()
        .map(|((from, to), mass)| DiagramEdge {
            from,
            to,
            share: mass / weight[from],
        })
        .collect();
    Diagram { boxes, edges }
}
