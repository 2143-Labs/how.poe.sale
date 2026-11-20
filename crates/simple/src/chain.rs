//! The chain solver: from where a craft starts, the chance of leaving through each exit, and what
//! was spent on the way, with its spread.
//!
//! A chain is a set of states. Each state either is an exit or has arms: where it goes next, how
//! often, and what that costs. A loop is allowed (an alteration that rolls nothing useful comes
//! back to the same state), so the answer is not a walk but a solve: for an inside state `i`, an
//! arm to `j` with chance `p`, mean spend `μ` and second moment `Σ`, and each exit `e`,
//!
//! ```text
//! a_i(e) = Σ_j p · a_j(e)                                  a_j(e) = [j = e] when j is an exit
//! m_i(e) = Σ_j p · ( μ·a_j(e) + m_j(e) )                   m and w are zero at an exit
//! w_i(e) = Σ_j p · ( Σ·a_j(e) + μ·m_j(e)ᵀ + m_j(e)·μᵀ + w_j(e) )
//! ```
//!
//! `a` is the chance of ending at `e`, `m` is `E[X · 1_e]` and `w` is `E[X Xᵀ · 1_e]`, `X` being
//! the orb counts spent. Each line is `x = b + Q·x` with `Q` the inside-to-inside chances, so
//! `I − Q` is factored once and every right-hand side is back-substituted.
//!
//! Chances here are `f64`: a state deep in a rare item has a chance whose exact form needs
//! hundreds of bits. A single orb's own chances stay exact where they are stated (`currency`).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fmt::{self, Debug};
use std::hash::Hash;

use crate::cost::{Cost, CurrencyId, Spend};
use crate::stock::Refusal;

/// The most states one chain may hold.
const MOST: usize = 5_000;

/// How far a state's arms may be from a whole chance.
const WHOLE: f64 = 1e-9;

/// A pivot below this means some state never reaches an exit.
const PIVOT: f64 = 1e-13;

/// A chance below this is rounding, not an arm.
const NOISE: f64 = 1e-15;

/// One way out of a state: where it leads, how often, and what is spent given that it is taken.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm<S> {
    pub to: S,
    pub chance: f64,
    pub spend: Spend,
}

/// What a state does next.
#[derive(Clone, Debug, PartialEq)]
pub enum Next<S> {
    Arms(Vec<Arm<S>>),
    Exit,
}

/// Why a chain could not be solved.
#[derive(Clone, Debug, PartialEq)]
pub enum SolveError {
    /// A currency refused the state it was used on.
    Refused {
        at: String,
        currency: CurrencyId,
        why: Refusal,
    },
    /// A move names a currency that is not among the tools.
    UnknownCurrency(CurrencyId),
    /// A method left through an exit it does not declare, or with the wrong rarity.
    BadExit {
        at: String,
        exit: &'static str,
        why: &'static str,
    },
    /// A state's arms do not add up to a whole chance.
    NotWhole { at: String, sum: f64 },
    /// A state from which no exit can be reached.
    NeverEnds { at: String },
    /// More states than a chain may hold.
    TooBig(usize),
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolveError::Refused { at, currency, why } => {
                write!(f, "{at}: {} refused: {why}", currency.name())
            }
            SolveError::UnknownCurrency(id) => write!(f, "{} is not among the tools", id.0),
            SolveError::BadExit { at, exit, why } => write!(f, "{at}: exit `{exit}`: {why}"),
            SolveError::NotWhole { at, sum } => {
                write!(f, "{at}: the chances add up to {sum}, not to one")
            }
            SolveError::NeverEnds { at } => write!(f, "{at}: no exit can be reached from here"),
            SolveError::TooBig(most) => write!(f, "the chain has more than {most} states"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Inside(usize),
    Exit(usize),
}

/// An arm with its state resolved and its spend laid out over the chain's keys.
struct Link {
    to: Slot,
    chance: f64,
    mean: Vec<f64>,
    second: Vec<f64>,
}

/// A solved chain.
#[derive(Debug)]
pub struct Solved<S> {
    /// Every currency spent anywhere in the chain, sorted.
    pub keys: Vec<CurrencyId>,
    /// The states that are not exits, breadth-first from the entry, arms in order.
    pub inside: Vec<S>,
    pub exits: Vec<S>,
    /// The expected number of visits to `inside[i]`, from the entry mix.
    pub visits: Vec<f64>,
    slots: HashMap<S, Slot>,
    entry: Vec<(Slot, f64)>,
    /// `chance[e][i]`: the chance of leaving through `exits[e]` from `inside[i]`.
    chance: Vec<Vec<f64>>,
    /// `first[e][k][i]`: `E[X_k · 1_e]` from `inside[i]`.
    first: Vec<Vec<Vec<f64>>>,
    /// `second[e][k·K + l][i]`: `E[X_k · X_l · 1_e]` from `inside[i]`.
    second: Vec<Vec<Vec<f64>>>,
}

/// Solves the chain that `next` describes, started from `entry`.
///
/// `entry` is a mix: its weights are shares of one whole start. A weight of zero is allowed and
/// only makes sure the state is part of the solve; an entry state may itself be an exit.
pub fn solve<S: Clone + Eq + Hash + Debug>(
    entry: &[(S, f64)],
    next: &mut dyn FnMut(&S) -> Result<Next<S>, SolveError>,
) -> Result<Solved<S>, SolveError> {
    solve_named(entry, next, &|state: &S| format!("{state:?}"))
}

/// [`solve`], with the states an error names written by `name` rather than by their `Debug`.
pub fn solve_named<S: Clone + Eq + Hash + Debug>(
    entry: &[(S, f64)],
    next: &mut dyn FnMut(&S) -> Result<Next<S>, SolveError>,
    name: &dyn Fn(&S) -> String,
) -> Result<Solved<S>, SolveError> {
    let mut slots: HashMap<S, Slot> = HashMap::new();
    let mut inside: Vec<S> = Vec::new();
    let mut exits: Vec<S> = Vec::new();
    let mut arms: Vec<Vec<Arm<S>>> = Vec::new();
    let mut queue: VecDeque<S> = entry.iter().map(|(state, _)| state.clone()).collect();
    while let Some(state) = queue.pop_front() {
        if slots.contains_key(&state) {
            continue;
        }
        if slots.len() >= MOST {
            return Err(SolveError::TooBig(MOST));
        }
        match next(&state)? {
            Next::Exit => {
                slots.insert(state.clone(), Slot::Exit(exits.len()));
                exits.push(state);
            }
            Next::Arms(list) => {
                let list: Vec<Arm<S>> = list.into_iter().filter(|arm| arm.chance > 0.0).collect();
                let sum: f64 = list.iter().map(|arm| arm.chance).sum();
                if (sum - 1.0).abs() > WHOLE {
                    return Err(SolveError::NotWhole {
                        at: name(&state),
                        sum,
                    });
                }
                slots.insert(state.clone(), Slot::Inside(inside.len()));
                queue.extend(list.iter().map(|arm| arm.to.clone()));
                inside.push(state);
                arms.push(list);
            }
        }
    }

    let keys: Vec<CurrencyId> = arms
        .iter()
        .flatten()
        .flat_map(|arm| arm.spend.mean.iter().map(|(id, _)| id))
        .collect::<BTreeSet<CurrencyId>>()
        .into_iter()
        .collect();
    let width = keys.len();
    let links: Vec<Vec<Link>> = arms
        .iter()
        .map(|list| {
            list.iter()
                .map(|arm| Link {
                    to: slots[&arm.to],
                    chance: arm.chance,
                    mean: keys.iter().map(|&id| arm.spend.mean.get(id)).collect(),
                    second: keys
                        .iter()
                        .flat_map(|&a| keys.iter().map(move |&b| (a, b)))
                        .map(|(a, b)| arm.spend.moment(a, b))
                        .collect(),
                })
                .collect()
        })
        .collect();
    let states = inside.len();

    // Every inside state has to be able to reach an exit, or the solve has no answer.
    let mut back: Vec<Vec<usize>> = vec![Vec::new(); states];
    let mut ends = vec![false; states];
    let mut stack: Vec<usize> = Vec::new();
    for (from, list) in links.iter().enumerate() {
        for link in list {
            match link.to {
                Slot::Exit(_) => {
                    if !ends[from] {
                        ends[from] = true;
                        stack.push(from);
                    }
                }
                Slot::Inside(to) => back[to].push(from),
            }
        }
    }
    while let Some(to) = stack.pop() {
        for &from in &back[to] {
            if !ends[from] {
                ends[from] = true;
                stack.push(from);
            }
        }
    }
    let never_ends = |state: usize| SolveError::NeverEnds {
        at: name(&inside[state]),
    };
    if let Some(stuck) = ends.iter().position(|&ends| !ends) {
        return Err(never_ends(stuck));
    }

    // I − Q, and its transpose for the visits.
    let mut matrix = vec![0.0; states * states];
    let mut transposed = vec![0.0; states * states];
    for (from, list) in links.iter().enumerate() {
        matrix[from * states + from] += 1.0;
        transposed[from * states + from] += 1.0;
        for link in list {
            if let Slot::Inside(to) = link.to {
                matrix[from * states + to] -= link.chance;
                transposed[to * states + from] -= link.chance;
            }
        }
    }
    let forward = Lu::factor(states, matrix).map_err(never_ends)?;
    let backward = Lu::factor(states, transposed).map_err(never_ends)?;

    let mut starts = vec![0.0; states];
    let entry: Vec<(Slot, f64)> = entry
        .iter()
        .map(|(state, weight)| (slots[state], *weight))
        .collect();
    for &(slot, weight) in &entry {
        if let Slot::Inside(state) = slot {
            starts[state] += weight;
        }
    }
    let visits = backward.solve(&starts);

    let mut chance: Vec<Vec<f64>> = Vec::with_capacity(exits.len());
    let mut first: Vec<Vec<Vec<f64>>> = Vec::with_capacity(exits.len());
    let mut second: Vec<Vec<Vec<f64>>> = Vec::with_capacity(exits.len());
    for exit in 0..exits.len() {
        // a: the chance of ending here.
        let direct: Vec<f64> = links
            .iter()
            .map(|list| {
                list.iter()
                    .filter(|link| link.to == Slot::Exit(exit))
                    .map(|link| link.chance)
                    .sum()
            })
            .collect();
        let a = forward.solve(&direct);
        let ends_here = |to: Slot| match to {
            Slot::Exit(other) => f64::from(u8::from(other == exit)),
            Slot::Inside(state) => a[state],
        };

        // m: the spend on the way here.
        let m: Vec<Vec<f64>> = (0..width)
            .map(|k| {
                let rhs: Vec<f64> = links
                    .iter()
                    .map(|list| {
                        list.iter()
                            .map(|link| link.chance * link.mean[k] * ends_here(link.to))
                            .sum()
                    })
                    .collect();
                forward.solve(&rhs)
            })
            .collect();
        let spent = |to: Slot, k: usize| match to {
            Slot::Exit(_) => 0.0,
            Slot::Inside(state) => m[k][state],
        };

        // w: its second moment.
        let w: Vec<Vec<f64>> = (0..width * width)
            .map(|kl| {
                let (k, l) = (kl / width, kl % width);
                let rhs: Vec<f64> = links
                    .iter()
                    .map(|list| {
                        list.iter()
                            .map(|link| {
                                link.chance
                                    * (link.second[kl] * ends_here(link.to)
                                        + link.mean[k] * spent(link.to, l)
                                        + spent(link.to, k) * link.mean[l])
                            })
                            .sum()
                    })
                    .collect();
                forward.solve(&rhs)
            })
            .collect();

        chance.push(a);
        first.push(m);
        second.push(w);
    }

    Ok(Solved {
        keys,
        inside,
        exits,
        visits,
        slots,
        entry,
        chance,
        first,
        second,
    })
}

impl<S: Clone + Eq + Hash + Debug> Solved<S> {
    /// The chance of leaving through each of `exits`, from the entry mix.
    pub fn exit_chances(&self) -> Vec<f64> {
        (0..self.exits.len())
            .map(|exit| {
                self.entry
                    .iter()
                    .map(|&(slot, weight)| match slot {
                        Slot::Exit(at) => weight * f64::from(u8::from(at == exit)),
                        Slot::Inside(state) => weight * self.chance[exit][state],
                    })
                    .sum()
            })
            .collect()
    }

    /// What the whole run spends, from the entry mix to whichever exit it leaves through.
    pub fn spend(&self) -> Spend {
        self.spend_into(&|_| true)
            .map(|(_, spend)| spend)
            .unwrap_or_else(Spend::none)
    }

    /// The chance of leaving through an exit `keep` accepts, and what is spent given that.
    /// `None` when that never happens.
    pub fn spend_into(&self, keep: &dyn Fn(&S) -> bool) -> Option<(f64, Spend)> {
        let width = self.keys.len();
        let mut chance = 0.0;
        let mut mean = vec![0.0; width];
        let mut second = vec![0.0; width * width];
        for (exit, state) in self.exits.iter().enumerate() {
            if !keep(state) {
                continue;
            }
            for &(slot, weight) in &self.entry {
                match slot {
                    Slot::Exit(at) => {
                        if at == exit {
                            chance += weight;
                        }
                    }
                    Slot::Inside(from) => {
                        chance += weight * self.chance[exit][from];
                        for (sum, column) in mean.iter_mut().zip(&self.first[exit]) {
                            *sum += weight * column[from];
                        }
                        for (sum, column) in second.iter_mut().zip(&self.second[exit]) {
                            *sum += weight * column[from];
                        }
                    }
                }
            }
        }
        if chance <= NOISE {
            return None;
        }
        for value in mean.iter_mut().chain(second.iter_mut()) {
            *value /= chance;
        }
        Some((chance, Spend::from_moments(&self.keys, &mean, &second)))
    }

    /// An inside state's way to each exit it can reach, as arms: the solved chain seen from
    /// outside. `None` when `from` is not an inside state.
    pub fn arms(&self, from: &S) -> Option<Vec<Arm<S>>> {
        let Some(&Slot::Inside(from)) = self.slots.get(from) else {
            return None;
        };
        Some(
            self.exits
                .iter()
                .enumerate()
                .filter(|&(exit, _)| self.chance[exit][from] > NOISE)
                .map(|(exit, state)| {
                    let chance = self.chance[exit][from];
                    let mean: Vec<f64> = self.first[exit]
                        .iter()
                        .map(|column| column[from] / chance)
                        .collect();
                    let second: Vec<f64> = self.second[exit]
                        .iter()
                        .map(|column| column[from] / chance)
                        .collect();
                    Arm {
                        to: state.clone(),
                        chance,
                        spend: Spend::from_moments(&self.keys, &mean, &second),
                    }
                })
                .collect(),
        )
    }

    /// The mean spend from a state until the chain is left: nothing at an exit. `None` when the
    /// state is not part of the chain.
    pub fn to_go(&self, from: &S) -> Option<Cost> {
        match *self.slots.get(from)? {
            Slot::Exit(_) => Some(Cost::none()),
            Slot::Inside(from) => {
                let mut cost = Cost::none();
                for (k, &id) in self.keys.iter().enumerate() {
                    cost.add(id, self.first.iter().map(|exit| exit[k][from]).sum());
                }
                Some(cost)
            }
        }
    }
}

/// `P·A = L·U`, row-major, with partial pivoting.
struct Lu {
    size: usize,
    cells: Vec<f64>,
    order: Vec<usize>,
}

impl Lu {
    /// Factors a `size × size` matrix. `Err` names the column whose pivot vanished.
    fn factor(size: usize, mut cells: Vec<f64>) -> Result<Lu, usize> {
        let mut order: Vec<usize> = (0..size).collect();
        for col in 0..size {
            let pivot = (col..size)
                .max_by(|&a, &b| {
                    cells[a * size + col]
                        .abs()
                        .total_cmp(&cells[b * size + col].abs())
                })
                .expect("the column has at least its diagonal row");
            if cells[pivot * size + col].abs() < PIVOT {
                return Err(col);
            }
            if pivot != col {
                for k in 0..size {
                    cells.swap(pivot * size + k, col * size + k);
                }
                order.swap(pivot, col);
            }
            for row in col + 1..size {
                let factor = cells[row * size + col] / cells[col * size + col];
                if factor == 0.0 {
                    continue;
                }
                cells[row * size + col] = factor;
                for k in col + 1..size {
                    cells[row * size + k] -= factor * cells[col * size + k];
                }
            }
        }
        Ok(Lu { size, cells, order })
    }

    fn solve(&self, rhs: &[f64]) -> Vec<f64> {
        let size = self.size;
        let mut x: Vec<f64> = self.order.iter().map(|&row| rhs[row]).collect();
        for row in 0..size {
            let below: f64 = self.cells[row * size..row * size + row]
                .iter()
                .zip(&x[..row])
                .map(|(cell, value)| cell * value)
                .sum();
            x[row] -= below;
        }
        for row in (0..size).rev() {
            let above: f64 = self.cells[row * size + row + 1..(row + 1) * size]
                .iter()
                .zip(&x[row + 1..])
                .map(|(cell, value)| cell * value)
                .sum();
            x[row] = (x[row] - above) / self.cells[row * size + row];
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const X: CurrencyId = CurrencyId("test/x");

    fn arm(to: &'static str, chance: f64, count: f64) -> Arm<&'static str> {
        Arm {
            to,
            chance,
            spend: Spend::certain(&Cost::of(X, count)),
        }
    }

    fn close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "{got} is not {want}");
    }

    /// One try in four works and every try costs one: four tries on average, and the spread of a
    /// geometric count, `(1 − p) / p²`.
    #[test]
    fn a_retry_costs_the_reciprocal_of_its_chance() {
        let solved = solve(&[("try", 1.0)], &mut |state| {
            Ok(match *state {
                "try" => Next::Arms(vec![arm("done", 0.25, 1.0), arm("try", 0.75, 1.0)]),
                _ => Next::Exit,
            })
        })
        .unwrap();
        assert_eq!(solved.inside, ["try"]);
        assert_eq!(solved.exits, ["done"]);
        close(solved.visits[0], 4.0);
        close(solved.exit_chances()[0], 1.0);
        let spend = solved.spend();
        close(spend.mean.get(X), 4.0);
        close(spend.covariance(X, X), 12.0);
        close(solved.to_go(&"try").unwrap().get(X), 4.0);
        assert_eq!(solved.to_go(&"done"), Some(Cost::none()));
        assert_eq!(solved.to_go(&"elsewhere"), None);
    }

    #[test]
    fn two_exits_after_one_spend_share_it() {
        let solved = solve(&[("start", 1.0)], &mut |state| {
            Ok(match *state {
                "start" => Next::Arms(vec![arm("left", 0.3, 2.0), arm("right", 0.7, 2.0)]),
                _ => Next::Exit,
            })
        })
        .unwrap();
        for (exit, share) in [("left", 0.3), ("right", 0.7)] {
            let (chance, spend) = solved.spend_into(&|state| *state == exit).unwrap();
            close(chance, share);
            close(spend.mean.get(X), 2.0);
            close(spend.sd(X), 0.0);
        }
        assert!(solved.spend_into(&|_| false).is_none());
    }

    #[test]
    fn a_spend_that_differs_by_arm_is_kept_apart() {
        let solved = solve(&[("start", 1.0)], &mut |state| {
            Ok(match *state {
                "start" => Next::Arms(vec![arm("cheap", 0.3, 1.0), arm("dear", 0.7, 3.0)]),
                _ => Next::Exit,
            })
        })
        .unwrap();
        let (_, cheap) = solved.spend_into(&|state| *state == "cheap").unwrap();
        let (_, dear) = solved.spend_into(&|state| *state == "dear").unwrap();
        close(cheap.mean.get(X), 1.0);
        close(dear.mean.get(X), 3.0);
        close(solved.spend().mean.get(X), 0.3 * 1.0 + 0.7 * 3.0);
        let arms = solved.arms(&"start").unwrap();
        assert_eq!(arms.len(), 2);
        close(arms[1].chance, 0.7);
        close(arms[1].spend.mean.get(X), 3.0);
        assert!(solved.arms(&"cheap").is_none());
    }

    /// A solved chain's arms can stand in for the chain: nesting it gives the same spend.
    #[test]
    fn a_solved_chain_nests() {
        let inner = solve(&[("try", 1.0)], &mut |state| {
            Ok(match *state {
                "try" => Next::Arms(vec![arm("done", 0.25, 1.0), arm("try", 0.75, 1.0)]),
                _ => Next::Exit,
            })
        })
        .unwrap();
        // Twice the inner chain, one after the other.
        let outer = solve(&[(0u8, 1.0)], &mut |step| {
            Ok(if *step == 2 {
                Next::Exit
            } else {
                Next::Arms(
                    inner
                        .arms(&"try")
                        .unwrap()
                        .into_iter()
                        .map(|arm| Arm {
                            to: step + 1,
                            chance: arm.chance,
                            spend: arm.spend,
                        })
                        .collect(),
                )
            })
        })
        .unwrap();
        let spend = outer.spend();
        close(spend.mean.get(X), 8.0);
        close(spend.covariance(X, X), 24.0);
    }

    #[test]
    fn an_entry_of_weight_zero_is_solved_but_not_counted() {
        let solved = solve(&[("start", 1.0), ("aside", 0.0)], &mut |state| {
            Ok(match *state {
                "start" => Next::Arms(vec![arm("done", 1.0, 1.0)]),
                "aside" => Next::Arms(vec![arm("done", 1.0, 5.0)]),
                _ => Next::Exit,
            })
        })
        .unwrap();
        close(solved.spend().mean.get(X), 1.0);
        close(solved.to_go(&"aside").unwrap().get(X), 5.0);
        close(solved.visits[1], 0.0);
    }

    #[test]
    fn a_loop_with_no_way_out_is_an_error() {
        let stuck = solve(&[("ping", 1.0)], &mut |state| {
            Ok(match *state {
                "ping" => Next::Arms(vec![arm("pong", 1.0, 1.0)]),
                _ => Next::Arms(vec![arm("ping", 1.0, 1.0)]),
            })
        });
        assert_eq!(
            stuck.unwrap_err(),
            SolveError::NeverEnds {
                at: "\"ping\"".to_string()
            }
        );
    }

    #[test]
    fn arms_that_are_not_a_whole_chance_are_an_error() {
        let short = solve(&[("start", 1.0)], &mut |state| {
            Ok(match *state {
                "start" => Next::Arms(vec![arm("left", 0.3, 1.0), arm("right", 0.6, 1.0)]),
                _ => Next::Exit,
            })
        });
        assert!(
            matches!(short.unwrap_err(), SolveError::NotWhole { sum, .. } if (sum - 0.9).abs() < 1e-12)
        );
    }
}
