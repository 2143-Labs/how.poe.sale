//! What the browser shows, as JSON: a pasted item as the engine reads it, the steps that could
//! have produced an abstract item (`preimages::preimages`), the modifiers a target can name, and
//! the bases. Every number and word is chosen here; the page lays them out.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::apply;
use crate::catalog::paid_cost;
use crate::data::{Data, ModKind, Side};
use crate::forward::{self, Kind};
use crate::item::{AbsSpec, Influence, Item, ItemError, ItemSpec, Rarity, SlotSpec};
use crate::method::{Pool, SideReq};
use crate::preimages::{self, AbsItem, Preimages, Slot};
use crate::text;

/// A pasted item as the engine read it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub item: ItemSpec,
    /// The item printed back as game text.
    pub text: String,
    /// Every line the reader could not explain.
    pub warnings: Vec<String>,
}

/// A method that shows no input, and why.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnmetView {
    pub method: String,
    pub name: String,
    pub reason: String,
}

/// A class as the page shows it: names and modifier texts, not ids.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClassView {
    pub base: String,
    pub item_level: u8,
    pub rarity: String,
    pub corrupted: bool,
    pub influences: Vec<String>,
    /// The tracked modifiers, each `"… or better"` when it stands for its family.
    pub prefixes: Vec<String>,
    pub suffixes: Vec<String>,
    pub any_prefixes: u8,
    pub any_suffixes: u8,
}

/// A chart node: its class, and what it stands for.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    pub id: String,
    pub spec: AbsSpec,
    pub view: ClassView,
    /// The items the node stands for, when it has any of its own.
    pub members: Option<usize>,
    pub approx: bool,
    /// The step whose outcome gave the node its items, when a forward step did.
    pub source: Option<SourceView>,
}

/// The forward step a node's items came from: the node it was used on, and the method.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub from: String,
    pub method: String,
}

/// One outcome of a method applied to a node.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeView {
    pub key: String,
    /// `item`, `destroyed`, `refused` or `notModelled`.
    pub kind: String,
    pub p: f64,
    pub interval: Option<[f64; 2]>,
    pub view: Option<ClassView>,
    pub reason: Option<String>,
}

/// The chance a step's result holds a watched modifier.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchView {
    /// The watched modifier's text, or "at least one of these".
    pub label: String,
    pub p: f64,
    pub interval: Option<[f64; 2]>,
}

/// One method's outcomes from a node.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppliedView {
    pub node: String,
    pub method: String,
    pub name: String,
    pub cost: String,
    /// The reforges run, when the numbers are simulated.
    pub simulated: Option<u32>,
    /// The method can be used again on each result with the same odds, so a count of tries
    /// means something.
    pub repeatable: bool,
    pub outcomes: Vec<OutcomeView>,
    /// For each watched modifier the chance the result holds it, then, with two or more
    /// watched, the chance it holds at least one: the outcomes split by every tracked slot and
    /// untracked count, so these are their sums.
    pub watched: Vec<WatchView>,
}

/// One input a method could have turned into a node.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InputView {
    pub key: String,
    pub method: String,
    pub name: String,
    pub p: f64,
    pub interval: Option<[f64; 2]>,
    pub cost: String,
    pub view: ClassView,
    pub note: Option<String>,
    /// The method used on this input can be used again on the node with the same odds.
    pub repeatable: bool,
}

/// The inputs that could have produced a node, and the methods that show none.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MadeByView {
    pub node: String,
    pub inputs: Vec<InputView>,
    pub skipped: Vec<UnmetView>,
}

/// A method a node can be pushed through.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MethodOptionView {
    pub id: String,
    pub name: String,
    pub family: String,
    pub cost: String,
}

/// A pinned step. Edges always run in crafting order.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EdgeView {
    pub from: String,
    pub to: String,
    pub method: String,
    pub name: String,
    pub p: f64,
    pub interval: Option<[f64; 2]>,
    /// The method can be used again on the step's result with the same odds.
    pub repeatable: bool,
}

/// A node added to the chart, and the edge that added it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PinnedView {
    pub node: NodeView,
    pub edge: EdgeView,
}

/// A modifier a target can name.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModOptionView {
    pub id: String,
    pub text: String,
    pub side: String,
    pub level: u8,
    pub influence: Option<String>,
    /// `"natural"`, or the name of the essence that adds it.
    pub source: String,
}

/// The most modifiers `mods` lists.
const MOD_LIMIT: usize = 2000;

/// Reads a pasted item: the item, with the warnings of the reader.
fn read(data: &Data, text: &str) -> Result<(Item, Vec<String>), String> {
    let parsed = text::parse(data, text).map_err(|e| format!("item: {e}"))?;
    let item = Item::from_spec(data, &parsed.spec).map_err(|e| format!("item: {e}"))?;
    Ok((item, parsed.warnings))
}

/// A pasted item as the engine reads it.
pub fn read_item(data: &Data, text: &str) -> Result<ItemView, String> {
    let (item, warnings) = read(data, text)?;
    Ok(ItemView {
        item: item.to_spec(data),
        text: text::render(data, &item),
        warnings,
    })
}

fn name_of(data: &Data, method: &str) -> String {
    data.method(method)
        .map_or_else(|| method.to_string(), |m| m.name.clone())
}

fn unmet(data: &Data, list: &[(String, String)]) -> Vec<UnmetView> {
    list.iter()
        .map(|(method, reason)| UnmetView {
            method: method.clone(),
            name: name_of(data, method),
            reason: reason.clone(),
        })
        .collect()
}

/// A modifier's text on one line, the way the game shows it: the unrevealed veiled placeholder
/// (which has no text of its own) as "Veiled Prefix" or "Veiled Suffix", and a crafted modifier
/// marked "(crafted)".
fn mod_text(data: &Data, m: u32) -> String {
    let md = data.mod_at(m);
    match md.kind {
        ModKind::Veiled => match md.side {
            Some(Side::Suffix) => "Veiled Suffix".into(),
            _ => "Veiled Prefix".into(),
        },
        ModKind::Crafted => format!("{} (crafted)", md.text.replace('\n', " / ")),
        _ => md.text.replace('\n', " / "),
    }
}

/// The backward methods a chart offers when the page names none.
const BACKWARD: [&str; 4] = [
    "currency/exalted",
    "currency/annulment",
    "bench/add/StrMasterItemGenerationCannotChangePrefixes",
    "bench/add/DexMasterItemGenerationCannotChangeSuffixes",
];

/// The order the page lists method families in: the ones a crafter reaches for first, then the
/// long tables of bench crafts and unveils.
fn family_rank(family: &str) -> usize {
    const ORDER: [&str; 8] = [
        "currency", "essence", "harvest", "fossil", "catalyst", "beast", "bench", "unveil",
    ];
    ORDER
        .iter()
        .position(|f| *f == family)
        .unwrap_or(ORDER.len())
}

/// One chart: a root class, the modifiers it tracks, and every class reachable from the root in
/// either direction. A node's id is its class, so a forward step and a backward step that reach
/// the same class are one node.
pub struct Chart {
    tracked: Vec<Slot>,
    /// The modifiers the page watches for ("Also track"), in its order, less the root's own.
    watched: Vec<u32>,
    nodes: BTreeMap<String, (AbsItem, Option<forward::Node>)>,
    /// For each node a forward step gave its items: that step's node and method.
    sources: BTreeMap<String, (String, String)>,
    last: Option<(String, String, forward::Applied)>,
    made_by: Option<(String, Preimages)>,
}

impl Chart {
    pub fn start(
        data: &Data,
        root_json: &str,
        watch_json: &str,
    ) -> Result<(Chart, NodeView), String> {
        let spec: AbsSpec = serde_json::from_str(root_json).map_err(|e| format!("root: {e}"))?;
        let watch: Vec<SlotSpec> =
            serde_json::from_str(watch_json).map_err(|e| format!("watch: {e}"))?;
        let tracked = preimages::tracked(data, &spec, &watch).map_err(|e| format!("track: {e}"))?;
        let in_root: BTreeSet<&str> = spec
            .prefixes
            .iter()
            .chain(&spec.suffixes)
            .map(|s| s.id.as_str())
            .collect();
        let mut watched = Vec::new();
        for w in watch.iter().filter(|w| !in_root.contains(w.id.as_str())) {
            if let Some(slot) = tracked.iter().find(|s| data.mod_at(s.m).id == w.id)
                && !watched.contains(&slot.m)
            {
                watched.push(slot.m);
            }
        }
        let target = AbsItem::from_spec(data, &spec).map_err(|e| format!("root: the item {e}"))?;
        target
            .validate(data)
            .map_err(|e| format!("root: the item {e}"))?;
        let id = preimages::abs_key(data, &target);
        let members = concrete(&target).then(|| forward::Node {
            members: vec![(target.item.clone(), 1.0)],
            approx: false,
        });
        let mut nodes = BTreeMap::new();
        nodes.insert(id.clone(), (target, members));
        let chart = Chart {
            tracked,
            watched,
            nodes,
            sources: BTreeMap::new(),
            last: None,
            made_by: None,
        };
        let view = chart.node_view(data, &id)?;
        Ok((chart, view))
    }

    /// The methods a node can be pushed through: every one-input method `apply` accepts for at
    /// least one of the node's heaviest items.
    pub fn methods(&mut self, data: &Data, node: &str) -> Result<Vec<MethodOptionView>, String> {
        let state = self.members(data, node)?;
        let mut heaviest: Vec<&Item> = state.members.iter().map(|(item, _)| item).collect();
        heaviest.sort_by(|a, b| {
            let weight = |item: &Item| {
                state
                    .members
                    .iter()
                    .find(|(other, _)| other == item)
                    .map_or(0.0, |(_, w)| *w)
            };
            weight(b).total_cmp(&weight(a))
        });
        let probe = &heaviest[..heaviest.len().min(20)];
        let mut out: Vec<MethodOptionView> = data
            .methods
            .iter()
            .filter(|m| m.inputs == 1)
            .filter(|m| {
                probe.iter().any(|item| {
                    apply::check(
                        data,
                        m,
                        std::slice::from_ref(*item),
                        &apply::Params::default(),
                    )
                    .is_ok()
                })
            })
            .map(|m| MethodOptionView {
                id: m.id.clone(),
                name: m.name.clone(),
                family: serde_json::to_value(m.family)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default(),
                cost: paid_cost(
                    data,
                    &apply::cost(data, m, &apply::Params::default()).unwrap_or_default(),
                ),
            })
            .collect();
        out.sort_by_cached_key(|m| (family_rank(&m.family), m.name.clone(), m.id.clone()));
        Ok(out)
    }

    /// Every class `method` reaches from `node`.
    pub fn forward(
        &mut self,
        data: &Data,
        node: &str,
        method: &str,
    ) -> Result<AppliedView, String> {
        let applied = self.applied(data, node, method)?;
        self.applied_view(data, node, &applied)
    }

    /// Every input that could have produced `node`, over the methods named (`[]` for the
    /// chart's own set: the two orbs, the two lock crafts and every essence).
    pub fn made_by(
        &mut self,
        data: &Data,
        node: &str,
        methods: &[String],
    ) -> Result<MadeByView, String> {
        let found = self.backward(data, node, methods)?;
        self.made_by_view(data, node, &found)
    }

    /// Adds the outcome `key` of `method` on `node` to the chart.
    pub fn pin_forward(
        &mut self,
        data: &Data,
        node: &str,
        method: &str,
        key: &str,
    ) -> Result<PinnedView, String> {
        let applied = self.applied(data, node, method)?;
        let group = applied
            .groups
            .iter()
            .find(|g| g.kind.key(data) == key)
            .ok_or_else(|| format!("chart: the step has no outcome \"{key}\""))?;
        let Kind::Item(class) = &group.kind else {
            return Err("chart: only an item outcome can be pinned".into());
        };
        let id = preimages::abs_key(data, class);
        let fresh = if members_of_forward(group) {
            Some(forward::child(group, forward::seed_for(&id, "")))
        } else {
            None
        };
        let gives = fresh.is_some();
        let gave = match self.nodes.get_mut(&id) {
            Some((_, slot @ None)) => {
                if gives {
                    *slot = fresh;
                }
                gives
            }
            Some(_) => false,
            None => {
                self.nodes.insert(id.clone(), (class.clone(), fresh));
                gives
            }
        };
        if gave {
            self.sources
                .insert(id.clone(), (node.to_string(), method.to_string()));
        }
        let name = name_of(data, method);
        Ok(PinnedView {
            node: self.node_view(data, &id)?,
            edge: EdgeView {
                from: node.to_string(),
                to: id,
                method: method.to_string(),
                name,
                p: group.p,
                interval: group.interval,
                repeatable: applied.repeatable,
            },
        })
    }

    /// Adds the input `key` of `method` to the chart, above `node`.
    pub fn pin_backward(
        &mut self,
        data: &Data,
        node: &str,
        method: &str,
        key: &str,
    ) -> Result<PinnedView, String> {
        let found = self.backward(data, node, &[])?;
        let step = found
            .steps
            .iter()
            .find(|s| s.method == method && preimages::abs_key(data, &s.input) == key)
            .ok_or_else(|| format!("chart: no input \"{key}\" for {method}"))?;
        let class = step.input.clone();
        let id = key.to_string();
        let repeatable = self.repeats_into(data, &step.method, &class, node);
        if !self.nodes.contains_key(&id) {
            let members = concrete(&class).then(|| forward::Node {
                members: vec![(class.item.clone(), 1.0)],
                approx: false,
            });
            self.nodes.insert(id.clone(), (class, members));
        }
        Ok(PinnedView {
            node: self.node_view(data, &id)?,
            edge: EdgeView {
                from: id,
                to: node.to_string(),
                method: step.method.clone(),
                name: step.name.clone(),
                p: step.p,
                interval: step.interval,
                repeatable,
            },
        })
    }

    /// Whether `method`, used on `input` to reach `node`, can be used again on `node`'s class
    /// with the same odds (`forward::repeats`).
    fn repeats_into(&self, data: &Data, method: &str, input: &AbsItem, node: &str) -> bool {
        let Some((class, _)) = self.nodes.get(node) else {
            return false;
        };
        data.method(method)
            .is_some_and(|m| forward::repeats(data, m, &input.item, &class.item))
    }

    fn node_view(&self, data: &Data, id: &str) -> Result<NodeView, String> {
        let (class, members) = self
            .nodes
            .get(id)
            .ok_or_else(|| format!("chart: no node \"{id}\""))?;
        Ok(NodeView {
            id: id.to_string(),
            spec: class.to_spec(data),
            view: class_view(data, &self.tracked, class),
            members: members.as_ref().map(|n| n.members.len()),
            approx: members.as_ref().is_some_and(|n| n.approx),
            source: self.sources.get(id).map(|(from, method)| SourceView {
                from: from.clone(),
                method: method.clone(),
            }),
        })
    }

    /// The node's items, realised from the class when it has none of its own.
    fn members(&mut self, data: &Data, id: &str) -> Result<forward::Node, String> {
        let class = self
            .nodes
            .get(id)
            .map(|(class, _)| class.clone())
            .ok_or_else(|| format!("chart: no node \"{id}\""))?;
        if let Some((_, Some(state))) = self.nodes.get(id) {
            return Ok(state.clone());
        }
        let state = forward::realise(data, &class, &self.tracked, forward::seed_for(id, ""));
        if let Some((_, slot)) = self.nodes.get_mut(id) {
            *slot = Some(state.clone());
        }
        Ok(state)
    }

    fn applied(
        &mut self,
        data: &Data,
        node: &str,
        method: &str,
    ) -> Result<forward::Applied, String> {
        if let Some((at, by, applied)) = &self.last
            && at == node
            && by == method
        {
            return Ok(applied.clone());
        }
        let Some(m) = data.method(method) else {
            return Err(format!("chart: unknown method \"{method}\""));
        };
        if m.inputs != 1 {
            return Err(format!("chart: {method} takes two items"));
        }
        let state = self.members(data, node)?;
        let seed = forward::seed_for(node, method);
        let applied = forward::apply_node(data, &state, m, &self.tracked, seed);
        self.last = Some((node.to_string(), method.to_string(), applied.clone()));
        Ok(applied)
    }

    fn backward(
        &mut self,
        data: &Data,
        node: &str,
        methods: &[String],
    ) -> Result<Preimages, String> {
        let class = self
            .nodes
            .get(node)
            .map(|(class, _)| class.clone())
            .ok_or_else(|| format!("chart: no node \"{node}\""))?;
        let ids: Vec<String> = if methods.is_empty() {
            let mut ids: Vec<String> = BACKWARD.iter().map(|m| m.to_string()).collect();
            ids.extend(
                data.methods
                    .iter()
                    .filter(|m| m.id.starts_with("essence/"))
                    .map(|m| m.id.clone()),
            );
            ids
        } else {
            methods.to_vec()
        };
        if let Some((at, found)) = &self.made_by
            && at == node
        {
            return Ok(found.clone());
        }
        let names: Vec<&str> = ids.iter().map(String::as_str).collect();
        let found = preimages::preimages(data, &class, &self.tracked, &names);
        self.made_by = Some((node.to_string(), found.clone()));
        Ok(found)
    }

    fn applied_view(
        &self,
        data: &Data,
        node: &str,
        applied: &forward::Applied,
    ) -> Result<AppliedView, String> {
        Ok(AppliedView {
            node: node.to_string(),
            method: applied.method.clone(),
            name: name_of(data, &applied.method),
            cost: paid_cost(
                data,
                &data
                    .method(&applied.method)
                    .and_then(|m| apply::cost(data, m, &apply::Params::default()).ok())
                    .unwrap_or_default(),
            ),
            simulated: applied.simulated,
            repeatable: applied.repeatable,
            outcomes: applied
                .groups
                .iter()
                .map(|group| {
                    let (kind, view, reason) = match &group.kind {
                        Kind::Item(class) => {
                            ("item", Some(class_view(data, &self.tracked, class)), None)
                        }
                        Kind::Destroyed => ("destroyed", None, None),
                        Kind::Refused(why) => ("refused", None, Some(why.clone())),
                        Kind::NotModelled(why) => ("notModelled", None, Some(why.clone())),
                    };
                    OutcomeView {
                        key: group.kind.key(data),
                        kind: kind.to_string(),
                        p: group.p,
                        interval: group.interval,
                        view,
                        reason,
                    }
                })
                .collect(),
            watched: self.watched_view(data, applied),
        })
    }

    /// The chance the step's result holds each watched modifier, and with two or more watched,
    /// the chance it holds at least one of them.
    fn watched_view(&self, data: &Data, applied: &forward::Applied) -> Vec<WatchView> {
        let chance = |hit: &dyn Fn(&AbsItem) -> bool| -> f64 {
            applied
                .groups
                .iter()
                .filter_map(|g| match &g.kind {
                    Kind::Item(class) if hit(class) => Some(g.p),
                    _ => None,
                })
                .sum()
        };
        let holds = |class: &AbsItem, m: u32| class.item.explicits().any(|a| a.roll.m == m);
        let view = |label: String, p: f64| WatchView {
            label,
            p,
            interval: applied.simulated.map(|n| forward::interval(p, n)),
        };
        let mut out: Vec<WatchView> = self
            .watched
            .iter()
            .filter_map(|&m| {
                let slot = self.tracked.iter().find(|s| s.m == m)?;
                Some(view(slot.text(data), chance(&|c| holds(c, m))))
            })
            .collect();
        if self.watched.len() > 1 {
            let any = chance(&|c| self.watched.iter().any(|&m| holds(c, m)));
            out.push(view("at least one of these".into(), any));
        }
        out
    }

    fn made_by_view(
        &self,
        data: &Data,
        node: &str,
        found: &Preimages,
    ) -> Result<MadeByView, String> {
        Ok(MadeByView {
            node: node.to_string(),
            inputs: found
                .steps
                .iter()
                .map(|s| InputView {
                    key: preimages::abs_key(data, &s.input),
                    method: s.method.clone(),
                    name: s.name.clone(),
                    p: s.p,
                    interval: s.interval,
                    cost: paid_cost(data, &s.cost),
                    view: class_view(data, &self.tracked, &s.input),
                    note: s.note.clone(),
                    repeatable: self.repeats_into(data, &s.method, &s.input, node),
                })
                .collect(),
            skipped: unmet(data, &found.skipped),
        })
    }
}

/// Whether a forward group's members are worth keeping: a reforge's simulation samples them,
/// and an exact step's are the items itself.
fn members_of_forward(group: &forward::Group) -> bool {
    !group.members.is_empty()
}

/// Whether a class names every modifier: only then does it have one item of its own.
fn concrete(class: &AbsItem) -> bool {
    class.any == [0, 0] && class.better.is_empty()
}

fn class_view(data: &Data, tracked: &[Slot], class: &AbsItem) -> ClassView {
    let texts = |side: Side| -> Vec<String> {
        class
            .item
            .affixes(side)
            .iter()
            .map(|a| match tracked.iter().find(|slot| slot.m == a.roll.m) {
                Some(slot) => slot.text(data),
                None => mod_text(data, a.roll.m),
            })
            .collect()
    };
    ClassView {
        base: data.base_at(class.item.base).name.clone(),
        item_level: class.item.item_level,
        rarity: class.item.rarity.name().to_string(),
        corrupted: class.item.corrupted,
        influences: Influence::ALL
            .into_iter()
            .filter(|i| class.item.has_influence(*i))
            .map(|i| i.name().to_string())
            .collect(),
        prefixes: texts(Side::Prefix),
        suffixes: texts(Side::Suffix),
        any_prefixes: class.any[0],
        any_suffixes: class.any[1],
    }
}

/// Every modifier a rare `base` at `item_level` with these influences can roll, and every
/// essence modifier for its class: prefixes first, then by level and text.
pub fn mods(
    data: &Data,
    base: &str,
    item_level: u8,
    influences: &[Influence],
) -> Result<Vec<ModOptionView>, String> {
    let spec = AbsSpec {
        base: base.to_string(),
        item_level,
        rarity: Rarity::Rare,
        influences: influences.to_vec(),
        prefixes: Vec::new(),
        suffixes: Vec::new(),
        any_prefixes: 0,
        any_suffixes: 0,
        corrupted: false,
    };
    let item = match AbsItem::from_spec(data, &spec) {
        Ok(abs) => abs.item,
        Err(ItemError::UnknownBase(_)) => return Err(format!("mods: unknown base \"{base}\"")),
        Err(e) => return Err(format!("mods: {e}")),
    };
    let option = |m: u32, source: &str| -> Option<ModOptionView> {
        let md = data.mod_at(m);
        Some(ModOptionView {
            id: md.id.clone(),
            text: mod_text(data, m),
            side: md.side?.name().to_string(),
            level: md.level,
            influence: md.influence.map(|i| i.name().to_string()),
            source: source.to_string(),
        })
    };
    let mut out: Vec<ModOptionView> =
        apply::pool_candidates(data, &item, &Pool::Natural, SideReq::Any, true, true)
            .into_iter()
            .filter_map(|(m, _)| option(m, "natural"))
            .collect();
    let class = data.base_at(item.base).class;
    for essence in &data.essences {
        if let Some(&(_, m)) = essence.mods.iter().find(|(c, _)| *c == class) {
            out.extend(option(m, &essence.name));
        }
    }
    out.sort_by(|a, b| {
        (a.side != "prefix", a.level, &a.text, &a.id).cmp(&(
            b.side != "prefix",
            b.level,
            &b.text,
            &b.id,
        ))
    });
    out.dedup_by(|a, b| a.id == b.id);
    out.truncate(MOD_LIMIT);
    Ok(out)
}

/// Every base a target can name, sorted: its name, or its metadata id when several bases share
/// the name (as `Item::to_spec` writes it).
pub fn bases(data: &Data) -> Vec<String> {
    let mut out: Vec<String> = data
        .bases
        .iter()
        .map(|b| {
            if data.bases_named(&b.name).len() == 1 {
                b.name.clone()
            } else {
                b.id.clone()
            }
        })
        .collect();
    out.sort();
    out.dedup();
    out
}
