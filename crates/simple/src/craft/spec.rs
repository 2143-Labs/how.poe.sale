//! A craft as data: blocks and wires, compiled into the steps a report walks.
//!
//! [`CraftSpec`] is the shape the page edits and stores: a list of blocks, each either a method
//! with its parameters or a saved craft used as one block, and a list of wires that take a block's
//! exit to another block or out of the craft.
//!
//! [`compile`] checks the shape before it builds anything, so a mistake in the wiring is named by
//! block — [`Craft::check`] could only say `step N` — and then flattens nested crafts into one
//! list of steps. [`outputs`] names the exits a craft hands out, which is what a craft that uses
//! it as a block wires.
//!
//! A method block is one step. A craft block is that craft's own steps, placed in order where the
//! block is, so its internal wires are offset to where its steps landed, and the parent's wire on
//! one of its outputs replaces the `done` wire that output ends on. A block the entry cannot reach
//! is left out with a warning; its wires are ignored.
//!
//! The method registry reads the one table of kinds the plan names; every other kind is an error.
//! [`Extra`] lists the currencies the compiled methods name that are not among the six orbs usable
//! on an abstract item, so a caller can build the tools a report needs.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{Craft, Step, Wire};
use crate::cost::CurrencyId;
use crate::currency::{ESSENCE_OF_HORROR, Essence};
use crate::item::Rarity;
use crate::method::{
    AltAugRegal, AltPair, AltToMod, BenchLife, DesiredPair, EssenceSpam, ExaltFull, ExaltTo,
    Method, RegalTo, ShaperSlam,
};

/// A craft as the page stores it: blocks wired exit to block, with the goal and prices it runs
/// under.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftSpec {
    pub id: String,
    pub name: String,
    /// The id of the goal the craft is built for.
    pub goal: String,
    /// The id of the price list the craft is costed at.
    pub prices: String,
    /// The rarity the craft starts from: `normal`, `magic` or `rare`.
    pub start: String,
    /// The id of the block the craft starts at.
    pub entry: String,
    pub blocks: Vec<BlockSpec>,
    pub wires: Vec<WireSpec>,
}

/// One block of a craft: a method with its parameters, or a saved craft used as one block.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockSpec {
    pub id: String,
    /// The method this block runs, when it is a method block.
    pub method: Option<MethodSpec>,
    /// The id of the craft this block inlines, when it is a craft block.
    pub craft: Option<String>,
    /// Where the page drew the block, kept for the next visit.
    pub position: Option<(f64, f64)>,
}

/// The method a block names, with only the parameters that method reads.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodSpec {
    pub kind: String,
    /// `exalt_to`: how many modifiers the item holds when it leaves.
    pub modifiers: Option<u8>,
    /// `alt_to_mod`: whether an augmentation fills an empty side the goal wants a desired
    /// modifier on.
    pub augment: Option<bool>,
    /// `essence_spam`: the essence to use, by its short id (`essence-of-horror`).
    pub essence: Option<String>,
}

/// One wire: the exit of a block, and where an item leaving through it goes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSpec {
    pub from: String,
    pub exit: String,
    /// A block id, or `done` for the finished item.
    pub to: String,
}

/// A currency a compiled craft's methods name that is not one of the six orbs usable on an
/// abstract item, so a caller must supply it.
pub enum Extra {
    /// The crafting bench.
    Bench,
    /// The Shaper's Exalted Orb.
    ShapersExalted,
    /// An essence, the one the data file holds.
    Essence(&'static Essence),
}

/// A craft compiled to steps, with what a caller must know about it.
pub struct Compiled {
    pub craft: Craft,
    /// The block path of each step, e.g. `b1/b1` for the first block of the craft inlined at
    /// `b1`, or `b2` for a top-level block.
    pub blocks: Vec<String>,
    /// The extra currencies the compiled methods need, in the order they are first used.
    pub extras: Vec<Extra>,
    /// A block the entry cannot reach, which was left out.
    pub warnings: Vec<String>,
}

/// One output of a compiled craft: where the wire that leaves the craft lives.
#[derive(Clone)]
struct Slot {
    name: String,
    step: usize,
    wire: usize,
}

/// What compiling one craft produced for the craft that inlined it.
struct Built {
    /// The start rarity, on the spec compiled from the request; a nested craft's is ignored.
    start: Option<Rarity>,
    outputs: Vec<Slot>,
}

/// The compiler, holding the whole craft being built so nested crafts share its steps.
struct Flattener<'a> {
    library: &'a [CraftSpec],
    /// The ids of the crafts being compiled, outermost first, which is what refuses a cycle.
    stack: Vec<String>,
    steps: Vec<Step>,
    blocks: Vec<String>,
    extras: Vec<Extra>,
    warnings: Vec<String>,
}

/// Compiles a craft spec into the steps a report walks.
///
/// The blocks and wires are checked first, in the order the plan lists them, so the error names
/// the block rather than a step; only then is the craft built and [`Craft::check`] run.
pub fn compile(spec: &CraftSpec, library: &[CraftSpec]) -> Result<Compiled, String> {
    let mut flattener = Flattener {
        library,
        stack: vec![spec.id.clone()],
        steps: Vec::new(),
        blocks: Vec::new(),
        extras: Vec::new(),
        warnings: Vec::new(),
    };
    let built = flattener.build(spec, "", Some(&spec.start))?;
    let craft = Craft {
        name: spec.name.clone(),
        start: built.start.expect("the compiled spec carries its start"),
        steps: flattener.steps,
    };
    craft.check().map_err(|why| why.to_string())?;
    Ok(Compiled {
        craft,
        blocks: flattener.blocks,
        extras: flattener.extras,
        warnings: flattener.warnings,
    })
}

/// The names of a craft's outputs: one per wire to `done`, in the craft's wire order, with the
/// second and later uses of a name suffixed `#2`, `#3` and so on.
pub fn outputs(spec: &CraftSpec, library: &[CraftSpec]) -> Result<Vec<String>, String> {
    if !spec.blocks.iter().any(|block| block.id == spec.entry) {
        return Err(format!("the entry block \"{}\" does not exist", spec.entry));
    }
    let mut stack = vec![spec.id.clone()];
    check_references(spec, library, &mut stack)?;
    Ok(named_outputs(spec)
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

impl Flattener<'_> {
    /// Checks one spec and compiles it, with the block paths prefixed by where the craft was
    /// inlined.
    fn build(
        &mut self,
        spec: &CraftSpec,
        prefix: &str,
        start: Option<&str>,
    ) -> Result<Built, String> {
        let (order, start) = self.validate(spec, prefix, start)?;
        self.warn(spec, &order, prefix);
        let outputs = self.flatten(spec, &order, prefix)?;
        Ok(Built { start, outputs })
    }

    /// The plan's checks, in its order: the entry, the start rarity, every wire's source, every
    /// reachable block's shape, and finally each reachable block's exits and wires.
    fn validate(
        &self,
        spec: &CraftSpec,
        prefix: &str,
        start: Option<&str>,
    ) -> Result<(Vec<usize>, Option<Rarity>), String> {
        let Some(entry) = spec.blocks.iter().position(|block| block.id == spec.entry) else {
            return Err(format!("the entry block \"{}\" does not exist", spec.entry));
        };
        let start = start.map(parse_start).transpose()?;
        for wire in &spec.wires {
            if !spec.blocks.iter().any(|block| block.id == wire.from) {
                return Err(format!(
                    "wire from \"{prefix}{}\" starts at an unknown block",
                    wire.from
                ));
            }
        }
        let reach = reachable(spec);
        let walk: Vec<usize> = spec
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| reach.contains(block.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        let mut order = vec![entry];
        order.extend(walk.iter().copied().filter(|i| *i != entry));
        for &i in &walk {
            let block = &spec.blocks[i];
            let path = format!("{prefix}{}", block.id);
            match (&block.method, &block.craft) {
                (Some(method), None) => {
                    method_of(method, &path)?;
                }
                (None, Some(id)) => {
                    if !self.library.iter().any(|craft| &craft.id == id) {
                        return Err(format!("craft \"{id}\" is not in the library"));
                    }
                    if self.stack.iter().any(|seen| seen == id) {
                        return Err(format!("craft \"{id}\" contains itself"));
                    }
                }
                _ => {
                    return Err(format!(
                        "block \"{path}\" needs exactly one of method or craft"
                    ));
                }
            }
        }
        for &i in &walk {
            let block = &spec.blocks[i];
            let path = format!("{prefix}{}", block.id);
            let exits = match (&block.method, &block.craft) {
                (Some(method), None) => method_of(method, &path)?
                    .exits()
                    .iter()
                    .map(|exit| exit.name.to_string())
                    .collect::<Vec<_>>(),
                (None, Some(id)) => {
                    let child = self
                        .library
                        .iter()
                        .find(|craft| &craft.id == id)
                        .expect("the block's craft is carried by the library");
                    outputs(child, self.library)?
                }
                _ => unreachable!("the block's shape is checked above"),
            };
            let mut wired: Vec<&str> = Vec::new();
            for wire in spec.wires.iter().filter(|wire| wire.from == block.id) {
                if !exits.iter().any(|exit| exit == &wire.exit) {
                    return Err(format!("block \"{path}\" has no exit \"{}\"", wire.exit));
                }
                if wired.contains(&wire.exit.as_str()) {
                    return Err(format!(
                        "block \"{path}\": exit \"{}\" is wired twice",
                        wire.exit
                    ));
                }
                wired.push(wire.exit.as_str());
                if wire.to != "done" && !spec.blocks.iter().any(|other| other.id == wire.to) {
                    return Err(format!(
                        "wire from \"{path}\" goes to an unknown block \"{}\"",
                        wire.to
                    ));
                }
            }
            for exit in &exits {
                if !wired.contains(&exit.as_str()) {
                    return Err(format!("block \"{path}\": exit \"{exit}\" is not wired"));
                }
            }
        }
        Ok((order, start))
    }

    /// Warns about every block the entry cannot reach, which is left out.
    fn warn(&mut self, spec: &CraftSpec, order: &[usize], prefix: &str) {
        for (i, block) in spec.blocks.iter().enumerate() {
            if !order.contains(&i) {
                self.warnings.push(format!(
                    "block \"{prefix}{}\" is not reachable from the start and was left out",
                    block.id
                ));
            }
        }
    }

    /// Emits the reachable blocks in order — a method block as one step, a craft block as its own
    /// craft compiled in place — then wires them and returns this craft's outputs.
    fn flatten(
        &mut self,
        spec: &CraftSpec,
        order: &[usize],
        prefix: &str,
    ) -> Result<Vec<Slot>, String> {
        let library = self.library;
        let mut first: HashMap<&str, usize> = HashMap::new();
        let mut nested: HashMap<&str, Vec<Slot>> = HashMap::new();
        for &i in order {
            let block = &spec.blocks[i];
            let path = format!("{prefix}{}", block.id);
            let at = self.steps.len();
            first.insert(block.id.as_str(), at);
            if let Some(spec) = &block.method {
                let method = method_of(spec, &path)?;
                let wires = method
                    .exits()
                    .iter()
                    .map(|exit| (exit.name, Wire::Done))
                    .collect();
                self.steps.push(Step { method, wires });
                self.blocks.push(path);
                if let Some(extra) = extra_for(&spec.kind) {
                    self.need(extra);
                }
            } else {
                let id = block
                    .craft
                    .as_deref()
                    .expect("the block's shape is checked before it is flattened");
                let child = library
                    .iter()
                    .find(|craft| craft.id == id)
                    .expect("the block's craft is carried by the library");
                self.stack.push(child.id.clone());
                let built = self.build(child, &format!("{path}/"), None)?;
                self.stack.pop();
                nested.insert(block.id.as_str(), built.outputs);
            }
        }
        for &i in order {
            let block = &spec.blocks[i];
            for wire in spec.wires.iter().filter(|wire| wire.from == block.id) {
                let slot = self.slot_of(&nested, &first, block, &wire.exit);
                let target = wire_of(&wire.to, &first, slot.step);
                self.steps[slot.step].wires[slot.wire].1 = target;
            }
        }
        let mut outputs = Vec::new();
        for (name, wire) in named_outputs(spec) {
            let block = spec
                .blocks
                .iter()
                .find(|block| block.id == wire.from)
                .expect("an output's block is reachable");
            let slot = self.slot_of(&nested, &first, block, &wire.exit);
            outputs.push(Slot {
                name,
                step: slot.step,
                wire: slot.wire,
            });
        }
        Ok(outputs)
    }

    /// The step and wire a block's exit lives on: the method's one step, or the nested craft's
    /// step that ends on that output.
    fn slot_of(
        &self,
        nested: &HashMap<&str, Vec<Slot>>,
        first: &HashMap<&str, usize>,
        block: &BlockSpec,
        exit: &str,
    ) -> Slot {
        if block.method.is_some() {
            let step = first[block.id.as_str()];
            let wire = self.steps[step]
                .wires
                .iter()
                .position(|(name, _)| *name == exit)
                .expect("a checked method declares the exit");
            Slot {
                name: exit.to_string(),
                step,
                wire,
            }
        } else {
            let slot = nested[block.id.as_str()]
                .iter()
                .find(|slot| slot.name == exit)
                .expect("a checked craft block declares the output");
            Slot {
                name: exit.to_string(),
                step: slot.step,
                wire: slot.wire,
            }
        }
    }

    /// Adds an extra currency once.
    fn need(&mut self, extra: Extra) {
        if self.extras.iter().any(|held| same(held, &extra)) {
            return;
        }
        self.extras.push(extra);
    }
}

/// The method a block names, with its parameters checked.
fn method_of(spec: &MethodSpec, path: &str) -> Result<Box<dyn Method>, String> {
    let method: Box<dyn Method> = match spec.kind.as_str() {
        "alt_aug_regal" => Box::new(AltAugRegal),
        "exalt_to" => match spec.modifiers {
            Some(modifiers @ 1..=6) => Box::new(ExaltTo { modifiers }),
            _ => {
                return Err(format!(
                    "block \"{path}\": exalt_to needs modifiers from 1 to 6"
                ));
            }
        },
        "alt_pair" => Box::new(AltPair),
        "alt_to_mod" => Box::new(AltToMod {
            augment: spec.augment.unwrap_or(false),
        }),
        "regal_to" => Box::new(RegalTo),
        "bench_life" => Box::new(BenchLife),
        "desired_pair" => Box::new(DesiredPair),
        "exalt_full" => Box::new(ExaltFull),
        "shaper_slam" => Box::new(ShaperSlam),
        "essence_spam" => {
            let essence = spec.essence.as_deref().unwrap_or("");
            if essence != "essence-of-horror" {
                return Err(format!("block \"{path}\": unknown essence \"{essence}\""));
            }
            Box::new(EssenceSpam {
                essence: CurrencyId::ESSENCE_OF_HORROR,
            })
        }
        kind => return Err(format!("block \"{path}\" has an unknown method \"{kind}\"")),
    };
    Ok(method)
}

/// The extra currency a method kind needs, if it needs one.
fn extra_for(kind: &str) -> Option<Extra> {
    match kind {
        "bench_life" => Some(Extra::Bench),
        "shaper_slam" => Some(Extra::ShapersExalted),
        "essence_spam" => Some(Extra::Essence(&ESSENCE_OF_HORROR)),
        _ => None,
    }
}

/// Whether two extras are the same currency: the variants match, and an essence is the same
/// essence only when its cost id matches.
fn same(a: &Extra, b: &Extra) -> bool {
    match (a, b) {
        (Extra::Bench, Extra::Bench) | (Extra::ShapersExalted, Extra::ShapersExalted) => true,
        (Extra::Essence(a), Extra::Essence(b)) => a.id == b.id,
        _ => false,
    }
}

/// The rarity a craft start names.
fn parse_start(name: &str) -> Result<Rarity, String> {
    match name {
        "normal" => Ok(Rarity::Normal),
        "magic" => Ok(Rarity::Magic),
        "rare" => Ok(Rarity::Rare),
        unknown => Err(format!("unknown start rarity \"{unknown}\"")),
    }
}

/// The wire a target makes: the next step when that is where the target is, otherwise an index.
fn wire_of(to: &str, first: &HashMap<&str, usize>, from: usize) -> Wire {
    if to == "done" {
        return Wire::Done;
    }
    let target = first[to];
    if target == from + 1 {
        Wire::Next
    } else {
        Wire::To(target)
    }
}

/// The blocks the entry can reach, following wires to blocks that exist.
fn reachable(spec: &CraftSpec) -> HashSet<&str> {
    let mut seen = HashSet::new();
    let mut queue = vec![spec.entry.as_str()];
    while let Some(id) = queue.pop() {
        if !spec.blocks.iter().any(|block| block.id == id) || !seen.insert(id) {
            continue;
        }
        for wire in spec.wires.iter().filter(|wire| wire.from == id) {
            queue.push(wire.to.as_str());
        }
    }
    seen
}

/// Every wire to `done` from a reachable block, in wire order, named by its exit and suffixed
/// `#2`, `#3` and so on when the name repeats.
fn named_outputs(spec: &CraftSpec) -> Vec<(String, &WireSpec)> {
    let reach = reachable(spec);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut outs = Vec::new();
    for wire in &spec.wires {
        if wire.to != "done" || !reach.contains(wire.from.as_str()) {
            continue;
        }
        let count = counts.entry(wire.exit.as_str()).or_insert(0);
        *count += 1;
        let name = if *count == 1 {
            wire.exit.clone()
        } else {
            format!("{}#{}", wire.exit, *count)
        };
        outs.push((name, wire));
    }
    outs
}

/// Checks that every craft a reachable block inlines is in the library and does not contain the
/// craft being compiled.
fn check_references(
    spec: &CraftSpec,
    library: &[CraftSpec],
    stack: &mut Vec<String>,
) -> Result<(), String> {
    let reach = reachable(spec);
    for block in &spec.blocks {
        if !reach.contains(block.id.as_str()) {
            continue;
        }
        let Some(id) = &block.craft else {
            continue;
        };
        let child = library
            .iter()
            .find(|craft| &craft.id == id)
            .ok_or_else(|| format!("craft \"{id}\" is not in the library"))?;
        if stack.iter().any(|seen| seen == id) {
            return Err(format!("craft \"{id}\" contains itself"));
        }
        stack.push(id.clone());
        check_references(child, library, stack)?;
        stack.pop();
    }
    Ok(())
}
