//! The engine as an API: the catalog a page builds from, a base's modifiers, and one craft run.
//!
//! Three entry points, all JSON out and JSON in:
//!
//! - [`catalog`] lists the six bases, the ten methods, the currencies a craft can pay with, the
//!   essences a method can use, the grades a filter is written in, and the built-in presets.
//! - [`mods`] lists one base's explicit modifiers, with their plain weight and their weight under
//!   the base's offered influence.
//! - [`run`] takes the mod filters, a price list, a craft and the library the craft nests, and
//!   reports what that craft costs per finished item: every step, the whole total, the ways out,
//!   the share of finished items that meet the craft's goal, a diagram of each method, and every
//!   other move priced.
//!
//! A mod filter is a target written out: a base, the grade floors per side, and the modifier ids
//! each grade is made of. A craft is a [`CraftSpec`] of blocks wired exit to block, and each method
//! block reads one filter — its own, or the craft's goal. This module compiles the craft through
//! [`crate::craft::spec`], classes the base's modifiers by every filter the steps read
//! ([`Classes`]), and builds the [`Kit`] it needs, adding the extras the compiled methods ask for
//! (the bench, the Shaper's Exalted Orb, an essence).
//!
//! Nothing here caches a result: a catalog is small, and a run is cheap next to the page's wait.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bases::{self, BaseFile};
use crate::classes::{Class, Classes};
use crate::cost::{Cost, CurrencyId, Spend};
use crate::craft::spec::{self, BlockSpec, CraftSpec, Extra, MethodSpec, WireSpec};
use crate::craft::{Craft, Place, Report, Step, Wire};
use crate::currency::{Bench, Currency, ESSENCE_OF_HORROR, ON_CLASSED, ShapersExalted};
use crate::data::Data;
use crate::diagram::{self, BoxKind, Diagram};
use crate::filter::{ItemSetFilter, Tier};
use crate::item::{Influence, Item, Rarity, Side};
use crate::law::{Law, LawError};
use crate::method::{
    AltAugRegal, AltPair, AltToMod, BenchLife, DesiredPair, EssenceSpam, ExaltFull, ExaltTo, Kit,
    Method, RegalTo, ShaperSlam, Through,
};
use crate::node::Node;
use crate::target::Target;
use crate::weights::Weights;

/// The price list the built-in crafts are costed at.
const PRICES_330: &str = "builtin:prices:330";

/// The Redeemer boots filter that asks for Onslaught alone.
const ONSLAUGHT: &str = "builtin:filter:boots-redeemer-onslaught";

/// The Redeemer boots filter that asks for Onslaught and movement speed.
const ONSLAUGHT_SPEED: &str = "builtin:filter:boots-redeemer-speed";

/// A mod filter: a base, the grade floors each side has to clear, and the modifiers each grade is
/// made of. `id` and `name` are the preset's. A method block reads one, and a craft's goal is one.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterSpec {
    pub id: String,
    pub name: String,
    pub base: String,
    /// Grade names, best first: the prefix side's floors.
    pub prefixes: Vec<String>,
    /// Grade names, best first: the suffix side's floors.
    pub suffixes: Vec<String>,
    /// The modifier ids the filter calls desired.
    pub desired: Vec<String>,
    /// The modifier ids the filter calls good.
    pub good: Vec<String>,
}

/// What one orb is worth: the unit the numbers are in, and the weight of every id it names. A key
/// with no weight leaves the orb counted rather than priced.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceSpec {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub weights: BTreeMap<String, f64>,
}

/// One run: the mod filters its craft and library name, the prices its orbs are weighed at, the
/// craft to cost, and the library that craft's nested blocks name.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub filters: Vec<FilterSpec>,
    pub prices: PriceSpec,
    pub craft: CraftSpec,
    pub library: Vec<CraftSpec>,
}

/// Everything the page needs to draw itself: the bases it offers, the methods it can put in a
/// block, the currencies it prices, the essences a method can use, the grades a filter is written
/// in, and the presets it starts from.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub bases: Vec<BaseView>,
    pub methods: Vec<MethodView>,
    pub currencies: Vec<CurrencyView>,
    pub essences: Vec<EssenceView>,
    pub grades: Vec<&'static str>,
    pub builtins: Builtins,
}

/// One base, its caps, and the goal its data file states.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseView {
    pub key: &'static str,
    pub label: &'static str,
    pub item_level: u8,
    pub item_class: String,
    pub influences: Vec<&'static str>,
    pub caps: CapsView,
    pub goal: FilterSpec,
}

/// How many modifiers a side of the base may hold at each cap.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapsView {
    pub magic: usize,
    pub rare: usize,
}

/// One method, as the page offers it: what it takes, what it leaves through, the parameters a
/// block sets, and the extra currency it needs.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodView {
    pub kind: &'static str,
    pub label: &'static str,
    pub takes: Vec<&'static str>,
    pub exits: Vec<ExitSpecView>,
    pub params: Vec<ParamView>,
    /// The extra currency's id, or `null` when the method needs none.
    pub needs: Option<&'static str>,
}

/// One way out of a method, and the rarity an item leaves with.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitSpecView {
    pub name: &'static str,
    pub rarity: &'static str,
}

/// One parameter a block sets, with the values it accepts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamView {
    pub name: &'static str,
    /// `int`, `bool` or `essence`.
    #[serde(rename = "type")]
    pub type_name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<ParamDefault>,
}

/// A parameter's default value, in the parameter's own type.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(untagged)]
pub enum ParamDefault {
    Int(u8),
    Bool(bool),
    Text(&'static str),
}

/// One currency a craft can pay with.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyView {
    pub id: &'static str,
    pub name: &'static str,
    /// Whether the price editor shows it. The bench option is bought by option, not by orb.
    pub priced: bool,
}

/// One essence a method can be told to use.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EssenceView {
    /// The `essence` parameter's value.
    pub id: &'static str,
    /// The id the use is costed and priced under.
    pub currency: &'static str,
    pub label: &'static str,
    /// The export id of the modifier the essence places.
    pub modifier: &'static str,
    /// The base key the essence's modifier is for.
    pub base: &'static str,
}

/// One explicit modifier of a base's pool.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    pub id: String,
    /// `prefix` or `suffix`.
    pub side: &'static str,
    pub name: String,
    pub text: String,
    pub level: u32,
    /// The modifier's first group, or empty when it lists none.
    pub group: String,
    /// Its weight on the base with no influence.
    pub weight_plain: u128,
    /// Its weight under the base's offered influence, or `null` when the base offers none.
    pub weight_influenced: Option<u128>,
    /// Whether the modifier rolls nowhere and an essence places it instead.
    pub essence_only: bool,
}

/// The presets a page starts from: one filter per base and three more, two price lists, nine
/// crafts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Builtins {
    pub filters: Vec<FilterSpec>,
    pub prices: Vec<PriceSpec>,
    pub crafts: Vec<CraftSpec>,
}

/// One run's report: what the craft costs, step by step, with every way out and every other move.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// The craft's name.
    pub craft: String,
    /// The unit the weighted numbers are in.
    pub unit: String,
    /// The law's warning, then the compile's.
    pub warnings: Vec<String>,
    pub total: SpendView,
    /// How many finished items meet the craft's goal.
    pub goal: GoalView,
    pub steps: Vec<StepView>,
    /// Every exit wired out of the craft.
    pub outputs: Vec<OutputView>,
    /// One diagram per step.
    pub diagrams: Vec<DiagramView>,
    /// Every other move the craft could make at a place it passes through.
    pub others: Vec<OtherView>,
    pub checked: CheckedView,
}

/// The craft's goal, and the share of finished items whose sides clear its floors, whatever their
/// rarity.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalView {
    /// The goal filter's id.
    pub filter: String,
    pub name: String,
    pub share: f64,
}

/// A spend as one weighted number and the orb counts behind it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendView {
    pub value: f64,
    pub sd: f64,
    /// Every orb paid for, in id order.
    pub orbs: Vec<OrbView>,
    /// The orb counts with no weight, so they can only be counted.
    pub unweighted: Vec<UnweightedView>,
}

/// One orb's mean count and spread, and its weight when the price list names it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbView {
    pub id: String,
    pub name: String,
    pub mean: f64,
    pub sd: f64,
    pub price: Option<f64>,
}

/// One thing paid for that has no weight.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnweightedView {
    pub id: String,
    pub name: String,
    pub count: f64,
}

/// One step of a craft, costed.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub step: usize,
    /// The block's path in the spec, e.g. `b2/b1` for a step inside a nested craft.
    pub block: String,
    pub name: String,
    /// The name of the mod filter the step reads.
    pub filter: String,
    pub takes: Vec<&'static str>,
    /// The share of this step's runs that leave through a wire.
    pub chance: f64,
    /// `1 / chance`, or `null` when nothing leaves: JSON has no infinity.
    pub tries: Option<f64>,
    pub exits: Vec<ExitView>,
    /// What one run of the step spends.
    pub run: SpendView,
    /// What it takes to get past the step, failed attempts included.
    pub reach: SpendView,
}

/// One exit of a step, and where it goes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitView {
    pub exit: &'static str,
    /// `"step N"` or `"done"`.
    pub to: String,
    pub share: f64,
}

/// One way out of the craft.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputView {
    pub step: usize,
    pub exit: &'static str,
    pub share: f64,
}

/// One step's method as a graph.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagramView {
    pub step: usize,
    pub boxes: Vec<BoxView>,
    pub edges: Vec<EdgeView>,
}

/// One box of a method's diagram.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxView {
    pub id: usize,
    /// `forgets`, `keeps` or `exit`.
    pub kind: &'static str,
    /// The box's rarity, or `null` for an exit.
    pub rarity: Option<&'static str>,
    /// The prefixes held, best first. `null` means any grades; a normal item has `[]`.
    pub prefixes: Option<Vec<String>>,
    /// The suffixes held, best first. `null` means any grades; a normal item has `[]`.
    pub suffixes: Option<Vec<String>>,
    /// The currency the strategy uses here.
    #[serde(rename = "move")]
    pub move_id: Option<String>,
    /// What the move costs to finish from here, when it is priced.
    pub cost: Option<f64>,
    /// Every other move the item allows, cheapest first.
    pub options: Vec<BoxOptionView>,
    /// The exit's name, for an exit box.
    pub exit: Option<&'static str>,
    /// Where the exit goes: `"step N"` or `"done"`.
    pub to: Option<String>,
    pub visits: f64,
}

/// One other move a keeps box allows, and what it costs to finish from there.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxOptionView {
    pub id: String,
    pub name: String,
    pub cost: f64,
}

/// One edge of a method's diagram: the share of `from`'s visits that go to `to`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeView {
    pub from: usize,
    pub to: usize,
    pub share: f64,
}

/// One other move the craft could make at a place it passes through.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherView {
    pub step: usize,
    /// The place, as a node's label.
    pub item: String,
    pub visits: f64,
    /// The move the craft makes there.
    pub planned: String,
    pub planned_cost: f64,
    /// The other move.
    pub other: String,
    pub other_cost: f64,
    /// Whether the other move is cheaper by the weights.
    pub better: bool,
}

/// What the other-moves pass checked, and what it had to leave out.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckedView {
    pub moves: usize,
    pub places: usize,
    pub skipped: usize,
}

/// The catalog: six bases, ten methods, eleven currencies, one essence, three grades, and the
/// built-in presets.
pub fn catalog() -> Result<Catalog, String> {
    let bases = bases::BASES
        .iter()
        .map(base_view)
        .collect::<Result<Vec<BaseView>, String>>()?;
    Ok(Catalog {
        bases,
        methods: method_views(),
        currencies: CURRENCIES
            .iter()
            .map(|&(id, name, priced)| CurrencyView { id, name, priced })
            .collect(),
        essences: vec![EssenceView {
            id: "essence-of-horror",
            currency: CurrencyId::ESSENCE_OF_HORROR.0,
            label: "Essence of Horror",
            modifier: ESSENCE_OF_HORROR.modifier,
            base: "crown",
        }],
        grades: vec!["desired", "good", "any"],
        builtins: Builtins {
            filters: builtin_filters()?,
            prices: builtin_prices()?,
            crafts: builtin_crafts(),
        },
    })
}

/// One base's explicit modifiers, prefixes first, then by group and item level.
pub fn mods(base: &str) -> Result<Vec<ModView>, String> {
    let file = bases::find(base).ok_or_else(|| format!("unknown base \"{base}\""))?;
    let data = bases::data(file.key)?;
    let influence = file.influences.first().copied();
    let mut views: Vec<ModView> = Vec::new();
    for (m, entry) in data.mods.iter().enumerate() {
        let Some(side) = entry.kind.side() else {
            continue;
        };
        let m = m as u16;
        views.push(ModView {
            id: entry.id.clone(),
            side: side.name(),
            name: entry.name.clone(),
            text: entry.text.clone(),
            level: entry.level,
            group: entry
                .groups
                .first()
                .map(|&group| data.group_name(group).to_string())
                .unwrap_or_default(),
            weight_plain: data.weight_on(m, 0, None),
            weight_influenced: influence.map(|influence| data.weight_on(m, 0, Some(influence))),
            essence_only: entry.essence_only,
        });
    }
    views.sort_by_cached_key(|view| {
        let side = if view.side == Side::Prefix.name() {
            0u8
        } else {
            1u8
        };
        (side, view.group.clone(), view.level)
    });
    Ok(views)
}

/// Runs one request: validates the goal, compiles the craft, classes the modifiers by every filter
/// its steps read, builds the law and the kit, and reports it.
pub fn run(request: &str) -> Result<RunView, String> {
    let request: RunRequest =
        serde_json::from_str(request).map_err(|why| format!("the request is not valid: {why}"))?;

    let goal = request
        .filters
        .iter()
        .find(|filter| filter.id == request.craft.goal)
        .ok_or_else(|| format!("filter \"{}\" is not in the request", request.craft.goal))?;
    let file = bases::find(&goal.base).ok_or_else(|| format!("unknown base \"{}\"", goal.base))?;
    let data = bases::data(file.key)?;
    let mut graded = vec![filter_of(data, goal, file)?];
    let influence = influence_of(&request.craft.influence, file)?;

    let start = Item::of_base(data, 0, file.item_level, Rarity::Normal);
    let start = match influence {
        Some(influence) => start.with_influence(influence),
        None => start,
    };

    let compiled = spec::compile(&request.craft, &request.library)?;

    // Every filter the steps read, the goal first: each step's place in that list is its lens.
    let mut used: Vec<&FilterSpec> = vec![goal];
    let mut lens_of: Vec<usize> = Vec::with_capacity(compiled.filters.len());
    for (path, id) in compiled.blocks.iter().zip(&compiled.filters) {
        let filter = request
            .filters
            .iter()
            .find(|filter| &filter.id == id)
            .ok_or_else(|| format!("block \"{path}\" uses an unknown filter \"{id}\""))?;
        if filter.base != goal.base {
            let label = bases::find(&filter.base).map_or(filter.base.as_str(), |found| found.label);
            return Err(format!(
                "filter \"{}\" is for {label}, and the craft is for {}",
                filter.name, file.label
            ));
        }
        let at = match used.iter().position(|seen| seen.id == filter.id) {
            Some(at) => at,
            None => {
                graded.push(filter_of(data, filter, file)?);
                used.push(filter);
                used.len() - 1
            }
        };
        lens_of.push(at);
    }
    let filters: Vec<&ItemSetFilter> = graded.iter().map(|(filter, _)| filter).collect();
    let classes = Classes::new(data, &filters)?;

    let mut warnings: Vec<String> = Vec::new();
    let law = match Law::classed(data, &classes, &start) {
        Ok(law) => law,
        Err(LawError::SharedGroup(_)) => {
            let law = Law::classed_apart(data, &classes, &start).map_err(|why| why.to_string())?;
            warnings.push(format!(
                "the {} family sits on both sides; chances read each side apart (up to 0.2% off \
                 per item)",
                law.shared().join(", ")
            ));
            law
        }
        Err(why) => return Err(why.to_string()),
    };
    warnings.extend(compiled.warnings.iter().cloned());

    // The tools the compiled craft names: the orbs every craft may use, then the extras.
    let mut statics: Vec<&'static dyn Currency<Node<Class>>> = Vec::new();
    let mut owned: Vec<Box<dyn Currency<Node<Class>>>> = Vec::new();
    for extra in &compiled.extras {
        match extra {
            Extra::Bench => statics.push(&Bench),
            Extra::Essence(essence) => statics.push(*essence),
            Extra::ShapersExalted => {
                let orb = ShapersExalted::classed(data, &classes, &start).map_err(|why| {
                    format!("the Shaper's Exalted Orb cannot be used here: {why}")
                })?;
                owned.push(Box::new(orb));
            }
        }
    }
    let mut tools: Vec<&dyn Currency<Node<Class>>> = Vec::new();
    for tool in ON_CLASSED {
        tools.push(tool);
    }
    for tool in statics {
        tools.push(tool);
    }
    for orb in &owned {
        tools.push(&**orb);
    }

    // Each step reads its own filter's view of the classes, and works toward its target.
    let craft: Craft<Class> = Craft {
        name: compiled.craft.name,
        start: compiled.craft.start,
        steps: compiled
            .craft
            .steps
            .into_iter()
            .zip(&lens_of)
            .map(|(step, &at)| Step {
                method: Box::new(Through {
                    method: step.method,
                    lens: classes.lens(at),
                    target: graded[at].1.clone(),
                }),
                wires: step.wires,
            })
            .collect(),
    };
    let kit = Kit {
        law: &law,
        target: &graded[0].1,
        tools: &tools,
    };

    let weights = weights_of(&request.prices)?;
    let report = craft
        .report(&kit, &weights)
        .map_err(|why| why.to_string())?;

    let steps = report
        .rows
        .iter()
        .enumerate()
        .map(|(step, row)| StepView {
            step,
            block: compiled.blocks.get(step).cloned().unwrap_or_default(),
            name: row.name.clone(),
            filter: used[lens_of[step]].name.clone(),
            takes: row.takes.iter().map(|rarity| rarity.name()).collect(),
            chance: row.chance,
            tries: (row.chance != 0.0).then_some(row.tries),
            exits: row
                .exits
                .iter()
                .map(|&(exit, wire, share)| ExitView {
                    exit,
                    to: wired_to(step, wire),
                    share,
                })
                .collect(),
            run: spend_view(&row.run, &weights),
            reach: spend_view(&row.reach, &weights),
        })
        .collect::<Vec<StepView>>();

    let mut outputs: Vec<OutputView> = Vec::new();
    for row in &report.rows {
        for &(exit, wire, share) in &row.exits {
            if matches!(wire, Wire::Done) {
                outputs.push(OutputView {
                    step: row.step,
                    exit,
                    share,
                });
            }
        }
    }

    let diagrams = (0..report.rows.len())
        .map(|step| {
            let source = diagram::diagram(&craft, &kit, &report, step, &weights);
            diagram_view(step, &source)
        })
        .collect::<Vec<DiagramView>>();

    let lens = classes.lens(0);
    let (mut met, mut finished) = (0.0, 0.0);
    for (place, chance) in report.flat.exits.iter().zip(report.flat.exit_chances()) {
        if let Place::Done(node) = place {
            finished += chance;
            if lens.view(node).meets(&graded[0].1) {
                met += chance;
            }
        }
    }

    Ok(RunView {
        craft: craft.name.clone(),
        unit: weights.unit.clone(),
        warnings,
        total: spend_view(&report.total, &weights),
        goal: GoalView {
            filter: goal.id.clone(),
            name: goal.name.clone(),
            share: if finished > 0.0 { met / finished } else { 0.0 },
        },
        steps,
        outputs,
        diagrams,
        others: others_view(&craft, &report, &weights),
        checked: checked_view(&report),
    })
}

/// Validates a mod filter and builds it: the floors' grade names and the two modifier lists, each
/// id resolved to its modifier.
fn filter_of(
    data: &Data,
    filter: &FilterSpec,
    file: &BaseFile,
) -> Result<(ItemSetFilter, Target), String> {
    let caps = data.base_at(0).caps;
    let mut floors: [Vec<Tier>; 2] = [Vec::new(), Vec::new()];
    for side in Side::BOTH {
        let names = match side {
            Side::Prefix => &filter.prefixes,
            Side::Suffix => &filter.suffixes,
        };
        for name in names {
            let tier = Tier::from_name(name).ok_or_else(|| format!("unknown grade \"{name}\""))?;
            floors[side.index()].push(tier);
        }
    }
    for side in Side::BOTH {
        let asked = floors[side.index()].len();
        if asked > caps.rare {
            return Err(format!(
                "the {} floors ask for {asked} modifiers, and a rare {} holds {}",
                side.name(),
                data.base_at(0).item_class,
                caps.rare
            ));
        }
    }
    let desired = resolve(data, &filter.desired, file)?;
    let good = resolve(data, &filter.good, file)?;
    for id in &filter.desired {
        if filter.good.contains(id) {
            return Err(format!("\"{id}\" is both desired and good"));
        }
    }
    let [prefixes, suffixes] = floors;
    Ok((
        ItemSetFilter::new(data, &desired, &good),
        Target {
            prefixes,
            suffixes,
            caps,
        },
    ))
}

/// A filter's modifier ids, resolved against the pool by name.
fn resolve(data: &Data, ids: &[String], file: &BaseFile) -> Result<Vec<u16>, String> {
    ids.iter()
        .map(|id| {
            data.id_of(id)
                .ok_or_else(|| format!("\"{id}\" is not a modifier of {}", file.label))
        })
        .collect()
}

/// The craft's influence, which must be one the base is offered.
fn influence_of(influence: &str, file: &BaseFile) -> Result<Option<Influence>, String> {
    if influence == "none" {
        return Ok(None);
    }
    match Influence::ALL
        .into_iter()
        .find(|offered| offered.name() == influence)
    {
        Some(found) if file.influences.contains(&found) => Ok(Some(found)),
        _ => Err(format!(
            "{} has no {influence} pool in this file",
            file.label
        )),
    }
}

/// The weight table a run costs at.
fn weights_of(prices: &PriceSpec) -> Result<Weights, String> {
    /// The `Weights` source, as the weight table's own file writes it.
    #[derive(Serialize)]
    struct Source<'a> {
        unit: &'a str,
        weights: &'a BTreeMap<String, f64>,
    }
    let source = serde_json::to_string(&Source {
        unit: &prices.unit,
        weights: &prices.weights,
    })
    .map_err(|why| why.to_string())?;
    Weights::from_json(&source)
}

/// The wire's destination, as the page names it.
fn wired_to(step: usize, wire: Wire) -> String {
    match wire {
        Wire::Next => format!("step {}", step + 1),
        Wire::To(to) => format!("step {to}"),
        Wire::Done => "done".to_string(),
    }
}

/// One spend as the view's numbers.
fn spend_view(spend: &Spend, weights: &Weights) -> SpendView {
    let valued = weights.value(spend);
    SpendView {
        value: valued.value,
        sd: valued.sd,
        orbs: spend
            .mean
            .iter()
            .map(|(id, mean)| OrbView {
                id: id.0.to_string(),
                name: currency_name(id),
                mean,
                sd: spend.sd(id),
                price: weights.of(id),
            })
            .collect(),
        unweighted: valued
            .unweighted
            .iter()
            .map(|(id, count)| UnweightedView {
                id: id.0.to_string(),
                name: currency_name(id),
                count,
            })
            .collect(),
    }
}

/// One step's diagram as the view's numbers.
fn diagram_view(step: usize, source: &Diagram) -> DiagramView {
    DiagramView {
        step,
        boxes: source
            .boxes
            .iter()
            .enumerate()
            .map(|(id, found)| {
                let mut view = BoxView {
                    id,
                    kind: "exit",
                    rarity: None,
                    prefixes: None,
                    suffixes: None,
                    move_id: None,
                    cost: None,
                    options: Vec::new(),
                    exit: None,
                    to: None,
                    visits: found.visits,
                };
                match &found.kind {
                    BoxKind::Forgets { rarity, currency } => {
                        view.kind = "forgets";
                        view.rarity = Some(rarity.name());
                        let normal = *rarity == Rarity::Normal;
                        view.prefixes = normal.then(Vec::<String>::new);
                        view.suffixes = normal.then(Vec::<String>::new);
                        view.move_id = Some(currency.0.to_string());
                    }
                    BoxKind::Keeps {
                        rarity,
                        sides,
                        currency,
                        cost,
                        options,
                    } => {
                        view.kind = "keeps";
                        view.rarity = Some(rarity.name());
                        view.prefixes = Some(tiers(&sides[0]));
                        view.suffixes = Some(tiers(&sides[1]));
                        view.move_id = Some(currency.0.to_string());
                        view.cost = *cost;
                        view.options = options
                            .iter()
                            .map(|&(id, cost)| BoxOptionView {
                                id: id.0.to_string(),
                                name: currency_name(id),
                                cost,
                            })
                            .collect();
                    }
                    BoxKind::Exit { exit, to } => {
                        view.exit = Some(*exit);
                        view.to = Some(to.clone());
                    }
                }
                view
            })
            .collect(),
        edges: source
            .edges
            .iter()
            .map(|edge| EdgeView {
                from: edge.from,
                to: edge.to,
                share: edge.share,
            })
            .collect(),
    }
}

/// A side's grades, best first, as names.
fn tiers(side: &[Tier]) -> Vec<String> {
    side.iter().map(|tier| tier.name().to_string()).collect()
}

/// Every other move, the most value it would save first. A place is labelled as its step's filter
/// sees it.
fn others_view(craft: &Craft<Class>, report: &Report<Class>, weights: &Weights) -> Vec<OtherView> {
    let valued = |cost: &Cost| weights.value(&Spend::certain(cost)).value;
    let mut views: Vec<OtherView> = report
        .others
        .iter()
        .map(|other| OtherView {
            step: other.step,
            item: craft.steps[other.step].method.view(&other.node).label(),
            visits: other.visits,
            planned: other.planned.0.to_string(),
            planned_cost: valued(&other.planned_to_go),
            other: other.other.0.to_string(),
            other_cost: valued(&other.other_to_go),
            better: other.better,
        })
        .collect();
    views.sort_by(|a, b| {
        let saving = |view: &OtherView| view.visits * (view.planned_cost - view.other_cost);
        saving(b).total_cmp(&saving(a))
    });
    views
}

/// What the other-moves pass checked.
fn checked_view(report: &Report<Class>) -> CheckedView {
    let mut places: Vec<(usize, &Node<Class>)> = report
        .others
        .iter()
        .map(|other| (other.step, &other.node))
        .collect();
    places.dedup();
    CheckedView {
        moves: report.others.len(),
        places: places.len(),
        skipped: report.skipped,
    }
}

/// The display name of a currency, or its raw id when the table does not name it.
fn currency_name(id: CurrencyId) -> String {
    CURRENCIES
        .iter()
        .find(|&&(at, _, _)| at == id.0)
        .map(|&(_, name, _)| name.to_string())
        .unwrap_or_else(|| id.0.to_string())
}

/// One base, its caps, and its data file's goal.
fn base_view(file: &BaseFile) -> Result<BaseView, String> {
    let data = bases::data(file.key)?;
    let caps = data.base_at(0).caps;
    Ok(BaseView {
        key: file.key,
        label: file.label,
        item_level: file.item_level,
        item_class: data.base_at(0).item_class.clone(),
        influences: file
            .influences
            .iter()
            .map(|influence| influence.name())
            .collect(),
        caps: CapsView {
            magic: caps.magic,
            rare: caps.rare,
        },
        goal: goal_of(file, data),
    })
}

/// A base's own goal as a filter: its file's floors, and the ids its lists name.
fn goal_of(file: &BaseFile, data: &Data) -> FilterSpec {
    FilterSpec {
        id: format!("builtin:goal:{}", file.key),
        name: format!("{}: the file's goal", file.label),
        base: file.key.to_string(),
        prefixes: data.target.prefixes.clone(),
        suffixes: data.target.suffixes.clone(),
        desired: ids_of(data, &data.target.desired),
        good: ids_of(data, &data.target.good),
    }
}

/// Modifier indices as their export ids.
fn ids_of(data: &Data, ids: &[u16]) -> Vec<String> {
    ids.iter().map(|&m| data.mod_at(m).id.clone()).collect()
}

/// One filter per base (its file's goal), the shield's Shaper's Exalted Orb route, and the two
/// filters of the Redeemer boots' two-filter craft.
fn builtin_filters() -> Result<Vec<FilterSpec>, String> {
    let names =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| name.to_string()).collect() };
    let mut filters: Vec<FilterSpec> = Vec::with_capacity(bases::BASES.len() + 3);
    for file in bases::BASES.iter() {
        filters.push(goal_of(file, bases::data(file.key)?));
    }
    let file = bases::find("shield").ok_or_else(|| "unknown base \"shield\"".to_string())?;
    let data = bases::data("shield")?;
    filters.push(FilterSpec {
        id: "builtin:goal:shield-slam".to_string(),
        name: format!("{}: the Shaper's Exalted Orb goal", file.label),
        base: "shield".to_string(),
        prefixes: names(&["desired", "good", "any"]),
        suffixes: names(&["good", "good", "any"]),
        desired: ids_of(data, &data.target.desired),
        good: ids_of(data, &data.target.good),
    });
    let onslaught = ["OnslaughtOnKillInfluence1", "OnslaughtOnKillInfluence2_"];
    filters.push(FilterSpec {
        id: ONSLAUGHT.to_string(),
        name: "Two-Toned Boots (Redeemer): Onslaught".to_string(),
        base: "boots-redeemer".to_string(),
        prefixes: Vec::new(),
        suffixes: names(&["desired"]),
        desired: names(&onslaught),
        good: Vec::new(),
    });
    filters.push(FilterSpec {
        id: ONSLAUGHT_SPEED.to_string(),
        name: "Two-Toned Boots (Redeemer): Onslaught and 30% movement speed".to_string(),
        base: "boots-redeemer".to_string(),
        prefixes: names(&["desired"]),
        suffixes: names(&["desired"]),
        desired: names(&[
            onslaught[0],
            onslaught[1],
            "MovementVelocity5",
            "MovementVelocity6",
        ]),
        good: Vec::new(),
    });
    Ok(filters)
}

/// The two price lists: the 3.30 figures, and the owner's placeholders.
fn builtin_prices() -> Result<Vec<PriceSpec>, String> {
    Ok(vec![prices_330(), prices_placeholder()?])
}

/// The November 2026 orb prices: report 08's live figures with report 11's 89.6c Shaper's orb.
fn prices_330() -> PriceSpec {
    let weights: BTreeMap<String, f64> = [
        (CurrencyId::TRANSMUTATION, 0.021),
        (CurrencyId::ALTERATION, 0.091),
        (CurrencyId::AUGMENTATION, 0.067),
        (CurrencyId::REGAL, 0.217),
        (CurrencyId::EXALTED, 0.83),
        (CurrencyId::SCOURING, 0.625),
        (CurrencyId::ANNULMENT, 8.6),
        (CurrencyId::SHAPERS_EXALTED, 89.6),
    ]
    .into_iter()
    .map(|(id, weight)| (id.0.to_string(), weight))
    .collect();
    PriceSpec {
        id: PRICES_330.to_string(),
        name: "3.30, November 2026".to_string(),
        unit: "chaos".to_string(),
        weights,
    }
}

/// The weight table's own file, as a price list.
fn prices_placeholder() -> Result<PriceSpec, String> {
    /// `data/weights.json`, read for its unit and its weights.
    #[derive(Deserialize)]
    struct WeightFile {
        unit: String,
        weights: BTreeMap<String, f64>,
    }
    let source = include_str!("../data/weights.json");
    let file: WeightFile = serde_json::from_str(source)
        .map_err(|why| format!("crates/simple/data/weights.json: {why}"))?;
    Ok(PriceSpec {
        id: "builtin:prices:placeholder".to_string(),
        name: "Owner's placeholders".to_string(),
        unit: file.unit,
        weights: file.weights,
    })
}

/// The nine built-in crafts, all at the 3.30 prices and all starting from a normal item. A craft's
/// influence is the one its goal's file was built for, except the Shaper's orb route, which needs
/// an item with none.
fn builtin_crafts() -> Vec<CraftSpec> {
    vec![
        craft_spec(
            "builtin:craft:shield-totem",
            "Shield: alt-aug-regal for +1 totems",
            "builtin:goal:shield",
            "shaper",
            vec![block("b1", method_spec("alt_aug_regal"))],
            vec![
                wire("b1", "magic", "done"),
                wire("b1", "magic_pair", "done"),
                wire("b1", "rare", "done"),
            ],
        ),
        craft_spec(
            "builtin:craft:shield-slam",
            "Shield: Shaper's Exalted Orb",
            "builtin:goal:shield-slam",
            "none",
            vec![block("b1", method_spec("shaper_slam"))],
            vec![wire("b1", "shaped", "done")],
        ),
        craft_spec(
            "builtin:craft:flask",
            "Flask: charges and freeze immunity",
            "builtin:goal:flask",
            "none",
            vec![block("b1", method_spec("alt_pair"))],
            vec![wire("b1", "magic", "done")],
        ),
        craft_spec(
            "builtin:craft:boots-hunter",
            "Hunter boots: Tailwind, regal, bench life",
            "builtin:goal:boots-hunter",
            "hunter",
            vec![
                block(
                    "b1",
                    MethodSpec {
                        augment: Some(true),
                        ..method_spec("alt_to_mod")
                    },
                ),
                block("b2", method_spec("regal_to")),
                block("b3", method_spec("bench_life")),
            ],
            vec![
                wire("b1", "magic", "b2"),
                wire("b2", "rare", "b3"),
                wire("b2", "normal", "b1"),
                wire("b3", "done", "done"),
                wire("b3", "scrap", "b1"),
            ],
        ),
        craft_spec(
            "builtin:craft:alt-until",
            "Alt until the desired modifier",
            "builtin:goal:boots-redeemer",
            "redeemer",
            vec![block(
                "b1",
                MethodSpec {
                    augment: Some(false),
                    ..method_spec("alt_to_mod")
                },
            )],
            vec![wire("b1", "magic", "done")],
        ),
        craft_spec(
            "builtin:craft:boots-redeemer",
            "Redeemer boots: restart on a miss",
            "builtin:goal:boots-redeemer",
            "redeemer",
            vec![
                nested("b1", "builtin:craft:alt-until"),
                block("b2", method_spec("exalt_full")),
            ],
            vec![
                wire("b1", "magic", "b2"),
                wire("b2", "done", "done"),
                wire("b2", "miss", "b1"),
                wire("b2", "scrap", "b1"),
            ],
        ),
        craft_spec(
            "builtin:craft:jewel",
            "Viridian jewel: life and attack speed",
            "builtin:goal:jewel",
            "none",
            vec![block("b1", method_spec("desired_pair"))],
            vec![wire("b1", "rare", "done")],
        ),
        craft_spec(
            "builtin:craft:crown",
            "Crown: Essence of Horror until Hypothermia",
            "builtin:goal:crown",
            "shaper",
            vec![block(
                "b1",
                MethodSpec {
                    essence: Some("essence-of-horror".to_string()),
                    ..method_spec("essence_spam")
                },
            )],
            vec![wire("b1", "hit", "done")],
        ),
        craft_spec(
            "builtin:craft:boots-redeemer-speed",
            "Redeemer boots: alt for Onslaught, exalt for speed",
            ONSLAUGHT_SPEED,
            "redeemer",
            vec![
                BlockSpec {
                    filter: Some(ONSLAUGHT.to_string()),
                    ..block(
                        "b1",
                        MethodSpec {
                            augment: Some(false),
                            ..method_spec("alt_to_mod")
                        },
                    )
                },
                block("b2", method_spec("exalt_full")),
            ],
            vec![
                wire("b1", "magic", "b2"),
                wire("b2", "done", "done"),
                wire("b2", "miss", "b1"),
                wire("b2", "scrap", "b1"),
            ],
        ),
    ]
}

/// A craft spec with no positions, starting from a normal item at the 3.30 prices.
fn craft_spec(
    id: &str,
    name: &str,
    goal: &str,
    influence: &str,
    blocks: Vec<BlockSpec>,
    wires: Vec<WireSpec>,
) -> CraftSpec {
    CraftSpec {
        id: id.to_string(),
        name: name.to_string(),
        goal: goal.to_string(),
        influence: influence.to_string(),
        prices: PRICES_330.to_string(),
        start: "normal".to_string(),
        entry: "b1".to_string(),
        blocks,
        wires,
    }
}

/// A method block that reads its craft's goal.
fn block(id: &str, method: MethodSpec) -> BlockSpec {
    BlockSpec {
        id: id.to_string(),
        method: Some(method),
        craft: None,
        filter: None,
        position: None,
    }
}

/// A block that is a saved craft, run in place.
fn nested(id: &str, craft: &str) -> BlockSpec {
    BlockSpec {
        id: id.to_string(),
        method: None,
        craft: Some(craft.to_string()),
        filter: None,
        position: None,
    }
}

/// A method with its parameters unset.
fn method_spec(kind: &str) -> MethodSpec {
    MethodSpec {
        kind: kind.to_string(),
        modifiers: None,
        augment: None,
        essence: None,
    }
}

/// A wire from one block's exit.
fn wire(from: &str, exit: &str, to: &str) -> WireSpec {
    WireSpec {
        from: from.to_string(),
        exit: exit.to_string(),
        to: to.to_string(),
    }
}

/// The ten methods the page offers, in its order.
fn method_views() -> Vec<MethodView> {
    vec![
        MethodView::of(
            "alt_aug_regal",
            "Alt-aug-regal",
            &AltAugRegal,
            Vec::new(),
            None,
        ),
        MethodView::of(
            "exalt_to",
            "Exalt to a modifier count",
            &ExaltTo { modifiers: 1 },
            vec![ParamView {
                name: "modifiers",
                type_name: "int",
                min: Some(1),
                max: Some(6),
                default: Some(ParamDefault::Int(1)),
            }],
            None,
        ),
        MethodView::of(
            "alt_pair",
            "Alt for a desired pair",
            &AltPair,
            Vec::new(),
            None,
        ),
        MethodView::of(
            "alt_to_mod",
            "Alt until the desired modifier",
            &AltToMod { augment: false },
            vec![ParamView {
                name: "augment",
                type_name: "bool",
                min: None,
                max: None,
                default: Some(ParamDefault::Bool(false)),
            }],
            None,
        ),
        MethodView::of("regal_to", "Regal", &RegalTo, Vec::new(), None),
        MethodView::of(
            "bench_life",
            "Bench: life prefix",
            &BenchLife,
            Vec::new(),
            Some(CurrencyId::BENCH.0),
        ),
        MethodView::of(
            "desired_pair",
            "Alt-aug for two desired, then regal",
            &DesiredPair,
            Vec::new(),
            None,
        ),
        MethodView::of(
            "exalt_full",
            "Exalt until full",
            &ExaltFull,
            Vec::new(),
            None,
        ),
        MethodView::of(
            "shaper_slam",
            "Shaper's Exalted Orb",
            &ShaperSlam,
            Vec::new(),
            Some(CurrencyId::SHAPERS_EXALTED.0),
        ),
        MethodView::of(
            "essence_spam",
            "Essence spam",
            &EssenceSpam {
                essence: CurrencyId::ESSENCE_OF_HORROR,
            },
            vec![ParamView {
                name: "essence",
                type_name: "essence",
                min: None,
                max: None,
                default: Some(ParamDefault::Text("essence-of-horror")),
            }],
            Some(CurrencyId::ESSENCE_OF_HORROR.0),
        ),
    ]
}

impl MethodView {
    /// One method's view, read off a default instance of it.
    fn of(
        kind: &'static str,
        label: &'static str,
        method: &dyn Method,
        params: Vec<ParamView>,
        needs: Option<&'static str>,
    ) -> MethodView {
        MethodView {
            kind,
            label,
            takes: method.takes().iter().map(|rarity| rarity.name()).collect(),
            exits: method
                .exits()
                .iter()
                .map(|exit| ExitSpecView {
                    name: exit.name,
                    rarity: exit.rarity.name(),
                })
                .collect(),
            params,
            needs,
        }
    }
}

/// Every currency a craft can pay with, in the order the page lists it: id, name, and whether the
/// price editor shows it.
const CURRENCIES: [(&str, &str, bool); 11] = [
    ("currency/transmutation", "Orb of Transmutation", true),
    ("currency/alteration", "Orb of Alteration", true),
    ("currency/augmentation", "Orb of Augmentation", true),
    ("currency/regal", "Regal Orb", true),
    ("currency/exalted", "Exalted Orb", true),
    ("currency/scouring", "Orb of Scouring", true),
    ("currency/annulment", "Orb of Annulment", true),
    ("currency/shapers_exalted", "Shaper's Exalted Orb", true),
    (
        "currency/alchemy",
        "Orb of Alchemy (the bench's price)",
        true,
    ),
    ("essence/essence-of-horror", "Essence of Horror", true),
    (
        "bench/EinharMasterIncreasedLife4",
        "Crafting bench: life prefix",
        false,
    ),
];
