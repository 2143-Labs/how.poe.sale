//! What the browser shows, as JSON: the start view, a base's modifiers, the priced craft loop and
//! one currency's outcomes. Every number and word is chosen here; the page lays them out.
//!
//! Exact masses cross the wire as strings on purpose: [`Fraction`] is a numerator and a
//! denominator, and [`ItemOutputSet::total`] is a `u128`, both of which JavaScript would round if
//! they arrived as numbers. A [`Mass`] therefore carries `exact` (`"num/den"`) and `percent` (six
//! decimals, no sign) as text, and `apply_currency`'s `total` is a decimal string.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::apply;
use crate::craft::{self, Priced, Prices, Report, Stage};
use crate::currency::{self, Accept, Fraction, ItemOutputSet, ItemSetFilter, ModSelector};
use crate::data::{Data, ModKind, Side};
use crate::item::{Influence, Item, ItemError, ItemSpec, Rarity};
use crate::method::{Pool, SideReq};
use crate::text;

/// A mass as the page reads it: the percentage is scale, the fraction beside it is the number.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mass {
    /// `"num/den"`.
    pub exact: String,
    /// Six decimals, without a `%` sign.
    pub percent: String,
}

impl Mass {
    fn of(mass: Fraction) -> Mass {
        Mass {
            exact: mass.to_string(),
            percent: mass.percent_text(6),
        }
    }
}

/// One line item of a currency's cost, keyed by the currency metadata id `Prices::prices` uses.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostView {
    pub key: String,
    pub name: String,
    pub amount: f64,
}

/// A modelled currency: its id, its display name and what it costs.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyView {
    pub id: String,
    pub name: String,
    pub costs: Vec<CostView>,
}

/// A currency named only: `apply_currency`'s answer says which currency produced the outcomes.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyNameView {
    pub id: String,
    pub name: String,
}

/// What the page needs before its first request.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Start {
    pub version: String,
    /// The bundle's patch, so the page can say which game data it is running.
    pub patch: String,
    pub bases: Vec<String>,
    pub influences: Vec<String>,
    /// The stage names a depth row can carry, in the order the page shows its columns.
    pub stages: Vec<String>,
    pub accepts: Vec<String>,
    /// The modelled currencies, in [`currency::MODELLED`] order.
    pub currencies: Vec<CurrencyView>,
    /// Chaos per cost key, so the price panel opens with usable numbers.
    pub price_defaults: BTreeMap<String, f64>,
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

/// One stage's mass in a depth row or in the totals.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StageView {
    pub stage: String,
    pub mass: Mass,
}

/// One depth of the loop: what a step left live, and what mass it recorded per stage.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DepthView {
    pub depth: usize,
    pub classes: usize,
    pub live: Mass,
    /// Only the stages that took mass at this depth, as the CLI's report prints them.
    pub stages: Vec<StageView>,
}

/// One currency's expected consumption on the way to the goal.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpendView {
    pub key: String,
    pub name: String,
    pub count: f64,
}

/// What the solved priced graph says.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricedView {
    pub expected_chaos: f64,
    pub sd_chaos: f64,
    /// Class nodes in the reachable closure.
    pub closure: usize,
    /// Outcome classes the node budget folded into the frontier.
    pub truncated: usize,
    /// Expected line items consumed, largest count first.
    pub spend: Vec<SpendView>,
}

/// The whole loop report: the start item, one row per depth, the mass that left the loop, and the
/// priced graph when the caller asked for it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoopView {
    pub start: ItemLine,
    pub depths: Vec<DepthView>,
    /// Only `desired`, `reset` and `stop`: the stages whose mass left the loop.
    pub totals: Vec<StageView>,
    pub live: Mass,
    pub stopped: Option<String>,
    pub priced: Option<PricedView>,
    /// Everything the request could not give a number for, old to new.
    pub warnings: Vec<String>,
}

/// The item the loop starts from, as the engine reads it.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemLine {
    pub text: String,
    pub stage: String,
}

/// One outcome of one currency.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeView {
    pub chance: Mass,
    pub text: String,
    /// The modifier the step added, as the game shows it.
    pub added: Option<String>,
}

/// One currency's exact outcomes on one item.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyView {
    pub currency: CurrencyNameView,
    /// The weights' shared total, as a decimal string (`u128` outruns a JS number).
    pub total: String,
    /// Largest weight first, at most [`ROW_LIMIT`] rows.
    pub outcomes: Vec<OutcomeView>,
    /// How many outcomes the row limit left out.
    pub more: usize,
    /// The hidden outcomes' combined exact chance, taken from their weights over `total` rather
    /// than as `1 - shown`, so rounding in the shown rows cannot hide in it. `0/1` when `more` is
    /// zero.
    pub rest: Mass,
}

/// The most outcomes `apply_currency` lists.
const ROW_LIMIT: usize = 200;

/// The most modifiers `mods` lists.
const MOD_LIMIT: usize = 2000;

/// Every stage a depth row or a total can carry, in enum order: the seven classifiers, then the
/// loop's two terminals `reset` and `stop`, which no item classifies into but the loop's summary
/// fills.
const STAGES: [Stage; 9] = [
    Stage::Desired,
    Stage::GoodPair,
    Stage::GoodPrefix,
    Stage::GoodSuffix,
    Stage::Other,
    Stage::Spare,
    Stage::SpentRare,
    Stage::Reset,
    Stage::Stop,
];

/// The `accept` values the request may name, in the order the page lists them.
const ACCEPTS: [&str; 3] = ["has-desired", "desired-or-good-both-sides", "any"];

/// The 3.30 chaos prices each modelled currency is listed at, so the price panel opens with the
/// numbers the owner quoted rather than zeroes.
const PRICE_DEFAULTS: [(&str, f64); 7] = [
    ("currency/transmutation", 0.021),
    ("currency/alteration", 0.091),
    ("currency/augmentation", 0.067),
    ("currency/regal", 0.217),
    ("currency/exalted", 0.83),
    ("currency/scouring", 0.625),
    ("currency/annulment", 8.6),
];

/// What `buy` must say about the finished item when the caller gives no price for it.
///
/// `Priced::report` refuses a graph whose goal is not for sale, and the page has no market for the
/// finished item — it prices a craft, not a trade. A large finite number keeps "buy the goal" out
/// of the optimum without putting an infinity into the solve; the craft is still free to reach the
/// goal through its own outcomes.
const GOAL_NOT_FOR_SALE: f64 = 1e12;

/// The warning the priced panel shows when the solved plan buys the finished item instead of
/// crafting it: an empty or unreachable goal has no craft to price.
const GOAL_UNREACHABLE: &str =
    "the goal cannot be reached by crafting from this base, so the loop has no price";

/// The whole start view.
pub fn start(data: &Data) -> Start {
    let mut currencies = Vec::with_capacity(currency::MODELLED.len());
    let mut price_defaults = BTreeMap::new();
    for &id in &currency::MODELLED {
        let Some(method) = data.method(id) else {
            continue;
        };
        let costs = apply::cost(data, method, &apply::Params::default()).unwrap_or_default();
        if let Some(&(_, chaos)) = PRICE_DEFAULTS.iter().find(|(other, _)| *other == id) {
            // A key shared by two methods keeps the first method's price, which is the order the
            // table and `MODELLED` both have.
            for cost in &costs {
                price_defaults.entry(cost.key.clone()).or_insert(chaos);
            }
        }
        currencies.push(CurrencyView {
            id: id.to_string(),
            name: method.name.clone(),
            costs: costs
                .into_iter()
                .map(|cost| CostView {
                    name: currency_name(data, &cost.key),
                    key: cost.key,
                    amount: cost.amount,
                })
                .collect(),
        });
    }
    Start {
        version: "0.1.0".to_string(),
        patch: data.bundle.patch.clone(),
        bases: bases(data),
        influences: Influence::ALL.into_iter().map(influence_wire).collect(),
        stages: STAGES
            .iter()
            .map(|stage| stage.name().to_string())
            .collect(),
        accepts: ACCEPTS.iter().map(|accept| (*accept).to_string()).collect(),
        currencies,
        price_defaults,
    }
}

/// An influence as `ItemSpec`'s `influences` spell it, so a checkbox's value can go straight back
/// into a request. The enum serialises `kebab-case`; [`Influence::name`] is the display form.
fn influence_wire(influence: Influence) -> String {
    serde_json::to_value(influence)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| influence.name().to_string())
}

/// The currency metadata id's display name, or the key itself when the bundle names no currency
/// under it (a beast tag, say): the page still has something to print.
fn currency_name(data: &Data, key: &str) -> String {
    data.currency_by_id(key)
        .map_or_else(|| key.to_string(), |currency| currency.name.clone())
}

/// A method's display name, or its id when the data has no such method.
fn name_of(data: &Data, method: &str) -> String {
    data.method(method)
        .map_or_else(|| method.to_string(), |m| m.name.clone())
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

/// Every modifier a rare `base` at `item_level` with these influences can roll, and every
/// essence modifier for its class: prefixes first, then by level and text.
pub fn mods(
    data: &Data,
    base: &str,
    item_level: u8,
    influences: &[Influence],
) -> Result<Vec<ModOptionView>, String> {
    let spec = ItemSpec {
        base: base.to_string(),
        item_level,
        rarity: Rarity::Rare,
        influences: influences.to_vec(),
        quality: 0,
        catalyst: None,
        defences: None,
        implicits: Vec::new(),
        enchants: Vec::new(),
        prefixes: Vec::new(),
        suffixes: Vec::new(),
        sockets: String::new(),
        corrupted: false,
        mirrored: false,
        split: false,
        synthesised: false,
        foreseen: false,
        memory_strands: 0,
        intangibility: 0,
        imprint: None,
        note: None,
    };
    let item = match Item::from_spec(data, &spec) {
        Ok(item) => item,
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
fn bases(data: &Data) -> Vec<String> {
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

/// One goal modifier, named by its id.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectorSpec {
    /// `"prefix"` or `"suffix"`.
    pub side: String,
    /// A modifier id, resolved with `Data::mod_by_id`.
    pub id: String,
    #[serde(default)]
    pub or_better: bool,
}

/// The goal, as the two buckets and the acceptance rule the loop reads.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoalSpec {
    pub desired: Vec<SelectorSpec>,
    pub good: Vec<SelectorSpec>,
    pub accept: String,
}

/// One loop request, as the page sends it.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoopRequest {
    pub item: ItemSpec,
    pub goal: GoalSpec,
    #[serde(default)]
    pub prices: Prices,
    pub depth: usize,
    /// `false` skips the priced graph, which is the slow part: every setup change asks for the
    /// depth table only, and the "Price the loop" button asks for both.
    #[serde(default)]
    pub price: bool,
}

/// One currency on one item, as the page sends it.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyRequest {
    pub item: ItemSpec,
    /// A [`CurrencyView::id`], e.g. `currency/transmutation`.
    pub currency: String,
}

/// Parses a request, or says so in the one wording every view uses.
fn parse<T: for<'de> Deserialize<'de>>(request: &str) -> Result<T, String> {
    serde_json::from_str(request).map_err(|e| format!("the request is not valid: {e}"))
}

/// The goal selectors as the filter holds them: an exact modifier, or its family at this level or
/// better.
fn selectors(data: &Data, specs: &[SelectorSpec]) -> Result<Vec<ModSelector>, String> {
    specs
        .iter()
        .map(|spec| {
            let side = match spec.side.as_str() {
                "prefix" => Side::Prefix,
                "suffix" => Side::Suffix,
                other => return Err(format!("unknown side \"{other}\"")),
            };
            let m = data
                .mod_by_id(&spec.id)
                .ok_or_else(|| format!("unknown modifier \"{}\"", spec.id))?;
            Ok(if spec.or_better {
                ModSelector::or_better(side, m)
            } else {
                ModSelector::exact(side, m)
            })
        })
        .collect()
}

/// The acceptance rule, from its wire name.
fn accept(name: &str) -> Result<Accept, String> {
    match name {
        "has-desired" => Ok(Accept::HasDesired),
        "desired-or-good-both-sides" => Ok(Accept::DesiredOrGoodBothSides),
        "any" => Ok(Accept::Any),
        other => Err(format!("unknown accept \"{other}\"")),
    }
}

/// The loop from a white base toward a goal: `LoopRequest` JSON in, [`LoopView`] JSON out.
///
/// The depth table comes from the lumped state and is cheap, so every setup change can ask for it.
/// The priced graph expands a thousand classes and solves a dense system — seconds on the real
/// bundle — so it is built only when the request asks for it, and its failure is a warning rather
/// than a lost depth table.
pub fn loop_report(data: &Data, request: &str) -> Result<LoopView, String> {
    let request: LoopRequest = parse(request)?;
    let item = Item::from_spec(data, &request.item).map_err(|e| e.to_string())?;
    let filter = ItemSetFilter::new(
        selectors(data, &request.goal.desired)?,
        selectors(data, &request.goal.good)?,
        accept(&request.goal.accept)?,
    );
    let depth = request.depth.clamp(1, 20);
    let run = craft::run_loop(data, &filter, item.clone(), depth)?;

    let depths: Vec<DepthView> = run
        .depths
        .iter()
        .map(|depth| DepthView {
            depth: depth.index,
            classes: depth.classes.len(),
            live: Mass::of(depth.live()),
            stages: depth
                .chances
                .iter()
                .map(|(stage, mass)| StageView {
                    stage: stage.name().to_string(),
                    mass: Mass::of(*mass),
                })
                .collect(),
        })
        .collect();
    let totals: Vec<StageView> = run
        .absorbed
        .iter()
        .map(|(stage, mass)| StageView {
            stage: stage.name().to_string(),
            mass: Mass::of(*mass),
        })
        .collect();

    let start = ItemLine {
        text: text::render(data, &item),
        stage: Stage::classify(data, &filter, &item).name().to_string(),
    };

    let mut warnings = Vec::new();
    let mut priced = None;
    if request.price {
        // The page can author currency prices but not a class key, so the two entries
        // `Priced::report` insists on are supplied here: the base is already held (zero) and the
        // finished item is not for sale. A caller that did price them keeps its numbers.
        let mut prices = request.prices;
        let start_key = Priced::start_key(data, &filter, &item);
        prices.buy.entry(start_key).or_insert(0.0);
        prices
            .buy
            .entry(craft::priced::GOAL_KEY.to_string())
            .or_insert(GOAL_NOT_FOR_SALE);
        match Priced::build(data, filter, &item, prices) {
            Ok(graph) => {
                if graph.truncated() > 0 {
                    warnings.push(format!(
                        "the priced graph was cut at {} classes; the cost is an upper bound",
                        graph.truncated()
                    ));
                }
                match graph.report() {
                    // The only way to "reach" the goal at this price is to buy it, so the loop has
                    // no crafting price to show. That is not a figure — it is the absence of one,
                    // and the page says so rather than printing a huge number.
                    Ok(report) if report.expected_chaos >= GOAL_NOT_FOR_SALE / 2.0 => {
                        warnings.push(GOAL_UNREACHABLE.to_string())
                    }
                    Ok(report) => priced = Some(priced_view(data, &report)),
                    Err(e) => warnings.push(e.to_string()),
                }
            }
            Err(e) => warnings.push(e.to_string()),
        }
    }

    Ok(LoopView {
        start,
        depths,
        totals,
        live: Mass::of(run.live),
        stopped: run
            .stopped
            .as_ref()
            .map(|(depth, why)| format!("depth {depth} did not fit: {why}")),
        priced,
        warnings,
    })
}

/// The solved graph as the page reads it, spend largest first.
fn priced_view(data: &Data, report: &Report) -> PricedView {
    let mut spend: Vec<SpendView> = report
        .expected_spend
        .iter()
        .map(|(key, count)| SpendView {
            key: key.clone(),
            name: currency_name(data, key),
            count: *count,
        })
        .collect();
    spend.sort_by(|a, b| b.count.total_cmp(&a.count));
    PricedView {
        expected_chaos: report.expected_chaos,
        sd_chaos: report.sd_chaos,
        closure: report.closure,
        truncated: report.truncated,
        spend,
    }
}

/// One currency on one item, exactly: `ApplyRequest` JSON in, [`ApplyView`] JSON out.
///
/// A currency the item refuses is an error carrying the engine's reason; the outcomes come back
/// largest weight first, capped at [`ROW_LIMIT`] rows with `more` counting the rest.
pub fn apply_currency(data: &Data, request: &str) -> Result<ApplyView, String> {
    let request: ApplyRequest = parse(request)?;
    let item = Item::from_spec(data, &request.item).map_err(|e| e.to_string())?;
    let currency = currency::modelled(data, &request.currency)
        .ok_or_else(|| format!("unknown currency \"{}\"", request.currency))?;
    let name = name_of(data, &request.currency);
    let (outcomes, total) = match currency.apply(&item, &None) {
        ItemOutputSet::Refused(why) => return Err(why.to_owned()),
        ItemOutputSet::Outcomes { outcomes, total } => (outcomes, total),
    };
    let mut rows: Vec<(u128, OutcomeView)> = outcomes
        .iter()
        .map(|outcome| {
            let chance = Fraction::new(outcome.weight, total)
                .ok_or_else(|| "the outcome's chance does not fit in 128 bits".to_string())?;
            Ok((
                outcome.weight,
                OutcomeView {
                    chance: Mass::of(chance),
                    text: text::render(data, &outcome.item),
                    added: outcome.added.map(|(_, m)| mod_text(data, m)),
                },
            ))
        })
        .collect::<Result<_, String>>()?;
    rows.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
    let more = rows.len().saturating_sub(ROW_LIMIT);
    let hidden = rows[ROW_LIMIT.min(rows.len())..]
        .iter()
        .try_fold(0u128, |sum, &(weight, _)| sum.checked_add(weight))
        .ok_or_else(|| "the hidden outcomes do not fit in 128 bits".to_string())?;
    let rest = Fraction::new(hidden, total)
        .ok_or_else(|| "the hidden chance does not fit in 128 bits".to_string())?;
    rows.truncate(ROW_LIMIT);
    Ok(ApplyView {
        currency: CurrencyNameView {
            id: request.currency,
            name,
        },
        total: total.to_string(),
        outcomes: rows.into_iter().map(|(_, view)| view).collect(),
        more,
        rest: Mass::of(rest),
    })
}
