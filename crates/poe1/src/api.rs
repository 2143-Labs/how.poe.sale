//! What the browser shows, as JSON: a pasted item as the engine reads it, the steps that could
//! have produced an abstract item (`preimages::preimages`), the modifiers a target can name, and
//! the bases. Every number and word is chosen here; the page lays them out.

use serde::Serialize;

use crate::apply;
use crate::catalog::paid_cost;
use crate::data::{Data, Side};
use crate::item::{AbsSpec, Influence, Item, ItemError, ItemSpec, Rarity};
use crate::method::{Pool, SideReq};
use crate::preimages::{self, AbsItem};
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

/// An abstract item as the page shows it: names and modifier texts, not ids.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AbsView {
    pub base: String,
    pub item_level: u8,
    pub rarity: String,
    pub influences: Vec<String>,
    /// Tracked modifier texts, in item order.
    pub prefixes: Vec<String>,
    pub suffixes: Vec<String>,
    pub any_prefixes: u8,
    pub any_suffixes: u8,
}

/// One input that could have produced the item. `input_spec` lets the page ask about the input
/// in turn without rebuilding it from display text.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreimageView {
    pub method: String,
    pub name: String,
    pub p: f64,
    pub cost: String,
    pub input: AbsView,
    pub input_spec: AbsSpec,
    pub note: Option<String>,
}

/// The steps that could have produced the target, and the methods that show none.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreimagesView {
    pub target: AbsView,
    pub steps: Vec<PreimageView>,
    pub skipped: Vec<UnmetView>,
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

/// A modifier's text on one line.
fn mod_text(data: &Data, m: u32) -> String {
    data.mod_at(m).text.replace('\n', " / ")
}

fn abs_view(data: &Data, item: &AbsItem) -> AbsView {
    let texts = |side: Side| -> Vec<String> {
        item.item
            .affixes(side)
            .iter()
            .map(|a| mod_text(data, a.roll.m))
            .collect()
    };
    AbsView {
        base: data.base_at(item.item.base).name.clone(),
        item_level: item.item.item_level,
        rarity: item.item.rarity.name().to_string(),
        influences: Influence::ALL
            .into_iter()
            .filter(|i| item.item.has_influence(*i))
            .map(|i| i.name().to_string())
            .collect(),
        prefixes: texts(Side::Prefix),
        suffixes: texts(Side::Suffix),
        any_prefixes: item.any[0],
        any_suffixes: item.any[1],
    }
}

/// The steps that could have produced a target (`AbsSpec` JSON), over the methods named.
pub fn preimages(
    data: &Data,
    target_json: &str,
    methods: &[String],
) -> Result<PreimagesView, String> {
    let spec: AbsSpec = serde_json::from_str(target_json).map_err(|e| format!("target: {e}"))?;
    let target = AbsItem::from_spec(data, &spec).map_err(|e| format!("target: the item {e}"))?;
    target
        .validate(data)
        .map_err(|e| format!("target: the item {e}"))?;
    let ids: Vec<&str> = methods.iter().map(String::as_str).collect();
    let slots = preimages::tracked(data, &spec, &[])?;
    let found = preimages::preimages(data, &target, &slots, &ids);
    Ok(PreimagesView {
        target: abs_view(data, &target),
        steps: found
            .steps
            .iter()
            .map(|s| PreimageView {
                method: s.method.clone(),
                name: s.name.clone(),
                p: s.p,
                cost: paid_cost(data, &s.cost),
                input: abs_view(data, &s.input),
                input_spec: s.input.to_spec(data),
                note: s.note.clone(),
            })
            .collect(),
        skipped: unmet(data, &found.skipped),
    })
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
