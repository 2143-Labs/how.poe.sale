//! The PoE 1 ruleset as a `solver::domain::Domain`.

use std::collections::BTreeSet;

use crate::data::Bundle;
use crate::item::{INFLUENCE_NAMES, ItemState, Side};
use crate::kernels::{self, Action};
use crate::registry::{self, DonorSpec};
use crate::target::{self, Target};
use crate::weights::{PoolFilter, PoolMod, pool_with};
use solver::domain::Domain;

pub struct Engine<'a> {
    pub b: &'a Bundle,
    /// The target states are described against (the market universe, or the v1 target).
    pub t: &'a Target,
    pub donors: &'a [DonorSpec],
    /// Contract ids the engine may use; `None` allows every craft.
    pub allowed: Option<&'a BTreeSet<String>>,
    /// (good spec, sell price) of every other good a state may satisfy as a brick.
    pub bricks: &'a [(Target, f64)],
    /// The good being made, when it differs from `t`; `None` means `t` itself is the goal.
    pub goal: Option<&'a Target>,
}

impl<'a> Engine<'a> {
    /// The M1 engine: one target, every craft, no bricks.
    pub fn new(b: &'a Bundle, t: &'a Target, donors: &'a [DonorSpec]) -> Engine<'a> {
        Engine {
            b,
            t,
            donors,
            allowed: None,
            bricks: &[],
            goal: None,
        }
    }

    /// Catalysts the proposer may use to steer a craft (the target's plus any that touch a
    /// requirement mod).
    pub fn offered_catalysts(&self) -> Vec<u8> {
        registry::offered_catalysts(self.b, self.t)
    }

    fn allows(&self, id: &str) -> bool {
        self.allowed.is_none_or(|set| set.contains(id))
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

    /// Every requirement tier the engine tracks.
    fn wanted_mods(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .t
            .requirements
            .iter()
            .flat_map(|r| r.accepted.iter().copied())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Essences whose mod for this class is a tracked requirement tier.
    fn relevant_essences(&self, s: &ItemState) -> Vec<u16> {
        let class = self.b.base_at(s.base).class.clone();
        let mut out = Vec::new();
        for m in self.wanted_mods() {
            for e in self.b.essences_for_mod(m, &class) {
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// The [`registry::FOSSIL_TOP_K`] best fossil sets (one or two fossils) for this item, by the
    /// share of one draw's weight that lands on a tracked requirement tier.
    fn best_fossils(&self, s: &ItemState) -> Vec<Vec<u16>> {
        let b = self.b;
        let wanted = self.wanted_mods();
        if wanted.is_empty() {
            return Vec::new();
        }
        let wanted_tags: Vec<&String> = wanted
            .iter()
            .flat_map(|m| b.mod_at(*m).implicit_tags_sorted.iter())
            .collect();
        let candidates: Vec<u16> = (0..b.fossils.len() as u16)
            .filter(|f| {
                let fossil = &b.fossils[*f as usize];
                fossil.supported
                    && (fossil
                        .weights
                        .iter()
                        .any(|(tag, w)| *w > 100 && wanted_tags.contains(&tag))
                        || fossil
                            .added_mods
                            .iter()
                            .any(|id| b.mod_by_id(id).is_some_and(|i| wanted.contains(&i))))
            })
            .collect();
        let mut sets: Vec<Vec<u16>> = candidates.iter().map(|f| vec![*f]).collect();
        for (i, f) in candidates.iter().enumerate() {
            for g in &candidates[i + 1..] {
                sets.push(vec![*f, *g]);
            }
        }
        let tags = b.item_tags(s.base, s.influences);
        let mut scored: Vec<(f64, Vec<u16>)> = Vec::new();
        for set in sets {
            let action = Action::Fossil {
                fossils: set.clone(),
            };
            if !registry::applicable(b, self.t, s, &action, self.donors) {
                continue;
            }
            let pm = PoolMod::fossils(b, &set);
            let (mut hit, mut total) = (0.0, 0.0);
            for side in Side::ALL {
                for e in pool_with(
                    b,
                    &PoolFilter {
                        side,
                        tags: &tags,
                        ilvl: s.ilvl,
                        excluded_groups: &[],
                        influence_only: false,
                        catalyst: None,
                    },
                    &pm,
                ) {
                    total += e.weight;
                    if wanted.contains(&e.mod_index) {
                        hit += e.weight;
                    }
                }
            }
            if total > 0.0 && hit > 0.0 {
                scored.push((hit / total, set));
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        scored
            .into_iter()
            .take(registry::FOSSIL_TOP_K)
            .map(|(_, set)| set)
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
        match self.goal {
            Some(g) => target::satisfied(self.b, g, &target::remap(self.b, self.t, g, s)),
            None => target::satisfied(self.b, self.t, s),
        }
    }

    fn actions(&self, s: &ItemState) -> Vec<Action> {
        let mut out = Vec::new();
        let mut push = |a: Action| {
            if self.allows(registry::contract(&a).id)
                && registry::applicable(self.b, self.t, s, &a, self.donors)
            {
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
        push(Action::Transmute);
        push(Action::Alteration);
        push(Action::Augmentation);
        push(Action::Regal);
        push(Action::Scour);
        push(Action::Imprint);
        push(Action::Restore);
        if self.allows("essence") {
            for e in self.relevant_essences(s) {
                push(Action::Essence { essence: e });
            }
        }
        if self.allows("fossil") {
            for fossils in self.best_fossils(s) {
                push(Action::Fossil { fossils });
            }
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
        let mut list = kernels::predecessors(self.b, self.t, self.donors, s);
        list.retain(|(a, _)| self.allows(registry::contract(a).id));
        list
    }

    fn label(&self, a: &Action) -> String {
        match a {
            Action::InfluenceExalt { influence } => {
                let name = registry::influence_name(*influence).unwrap_or("Influence");
                format!("{name}'s Exalted Orb")
            }
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
            Action::Essence { essence } => self
                .b
                .essences
                .get(*essence as usize)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Essence".to_string()),
            Action::Fossil { fossils } => {
                let names: Vec<&str> = fossils
                    .iter()
                    .filter_map(|f| self.b.fossils.get(*f as usize))
                    .map(|f| f.name.as_str())
                    .collect();
                format!("Fossils: {}", names.join(" + "))
            }
            other => registry::contract(other).name.to_string(),
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

    /// The best sell price among the brick goods this state satisfies.
    fn sell_value(&self, s: &ItemState) -> Option<f64> {
        let mut best: Option<f64> = None;
        for (spec, price) in self.bricks {
            if target::satisfied(self.b, spec, &target::remap(self.b, self.t, spec, s)) {
                best = Some(best.map_or(*price, |x: f64| x.max(*price)));
            }
        }
        best
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
