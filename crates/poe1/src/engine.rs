//! The PoE 1 ruleset as a `solver::domain::Domain`.

use crate::data::Bundle;
use crate::item::{INFLUENCE_NAMES, ItemState, Side};
use crate::kernels::{self, Action};
use crate::registry::{self, DonorSpec};
use crate::target::{self, Target};
use solver::domain::Domain;

pub struct Engine<'a> {
    pub b: &'a Bundle,
    pub t: &'a Target,
    pub donors: &'a [DonorSpec],
}

impl Engine<'_> {
    /// Catalysts the proposer may use to steer a craft (the target's plus any that touch a
    /// requirement mod).
    pub fn offered_catalysts(&self) -> Vec<u8> {
        registry::offered_catalysts(self.b, self.t)
    }

    /// Influence bits the item's class supports, ascending.
    fn class_influences(&self, s: &ItemState) -> Vec<u8> {
        let class = self.b.base_at(s.base).class.clone();
        let Some(c) = self.b.class_by_name(&class) else {
            return Vec::new();
        };
        (0..6u8)
            .filter(|bit| {
                registry::influence_name(1 << bit)
                    .is_some_and(|name| c.influence_tags.contains_key(name))
            })
            .map(|bit| 1 << bit)
            .collect()
    }
}

impl Domain for Engine<'_> {
    type State = ItemState;
    type Action = Action;

    fn key(&self, s: &ItemState) -> String {
        s.key()
    }

    fn is_goal(&self, s: &ItemState) -> bool {
        target::satisfied(self.b, self.t, s)
    }

    fn actions(&self, s: &ItemState) -> Vec<Action> {
        let mut out = Vec::new();
        let mut push = |a: Action| {
            if registry::applicable(self.b, self.t, s, &a, self.donors) {
                out.push(a);
            }
        };
        push(Action::Chaos);
        push(Action::Exalted);
        for bit in self.class_influences(s) {
            push(Action::InfluenceExalt { influence: bit });
        }
        push(Action::Annul);
        push(Action::Divine);
        push(Action::HarvestInfluenced);
        push(Action::HarvestRandomiseInfluence);
        push(Action::BenchLock { side: Side::Prefix });
        push(Action::BenchLock { side: Side::Suffix });
        for c in self.offered_catalysts() {
            push(Action::ApplyCatalyst { catalyst: c });
        }
        for d in 0..self.donors.len() {
            push(Action::Combine { donor: d as u8 });
        }
        out
    }

    fn outcomes(&self, s: &ItemState, a: &Action) -> Vec<(ItemState, f64)> {
        kernels::outcomes(self.b, self.t, self.donors, s, a)
    }

    fn cost(&self, _s: &ItemState, a: &Action) -> Vec<(String, f64)> {
        registry::cost(self.b, a, self.donors)
    }

    fn seconds(&self, _s: &ItemState, a: &Action) -> f64 {
        registry::seconds(a)
    }

    fn predecessors(&self, s: &ItemState) -> Vec<(Action, ItemState)> {
        kernels::predecessors(self.b, self.t, self.donors, s)
    }

    fn label(&self, a: &Action) -> String {
        match a {
            Action::Chaos => "Chaos Orb".to_string(),
            Action::Exalted => "Exalted Orb".to_string(),
            Action::InfluenceExalt { influence } => {
                let name = registry::influence_name(*influence).unwrap_or("Influence");
                format!("{name}'s Exalted Orb")
            }
            Action::Annul => "Orb of Annulment".to_string(),
            Action::Divine => "Divine Orb".to_string(),
            Action::HarvestInfluenced => "Harvest: reforge influenced rare".to_string(),
            Action::HarvestRandomiseInfluence => "Harvest: randomise influence".to_string(),
            Action::BenchLock { side } => match side {
                Side::Prefix => "Bench: Prefixes Cannot Be Changed".to_string(),
                Side::Suffix => "Bench: Suffixes Cannot Be Changed".to_string(),
            },
            Action::ApplyCatalyst { catalyst } => self.b.catalysts[*catalyst as usize].name.clone(),
            Action::Combine { donor } => format!(
                "Awakener's Orb ({})",
                self.donors
                    .get(*donor as usize)
                    .map(|d| d.label.clone())
                    .unwrap_or_else(|| "donor".to_string())
            ),
        }
    }

    /// Every `value_ok` flag set to true, hits re-sorted.
    fn ignore_values(&self, s: &ItemState) -> ItemState {
        let mut out = s.clone();
        for side in Side::ALL {
            for hit in &mut out.side_mut(side).wanted {
                hit.value_ok = true;
            }
            out.side_mut(side).wanted.sort();
        }
        out
    }
}

/// Human name of an influence bit, for the wire API.
pub fn influence_name(bit: u8) -> &'static str {
    registry::influence_name(bit).unwrap_or("Unknown")
}

/// All influence names, in bit order.
pub fn influence_names() -> &'static [&'static str] {
    &INFLUENCE_NAMES
}
