//! Recipes: named, data-defined templates that instantiate market processes
//! (`recipes/recipes.json`, compiled in).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::data::Bundle;
use crate::item::{Rarity, Side};
use crate::market::{self, CraftPlan, Good, Market, Process, ProcessKind, ProcessStatus};
use crate::registry::{self, DonorSpec};
use crate::target::Target;
use crate::weights::{ModSource, SourceCtx};

/// Most splits `recombine-split` offers for one good.
pub const MAX_SPLITS: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecipeDef {
    pub id: String,
    pub name: String,
    /// `"craft"` or `"recombine"`.
    pub kind: String,
    /// Craft only: `"normal-base"` or `"same-base"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// The output rarity the recipe applies to: `"Normal"`, `"Magic"` or `"Rare"`.
    pub rarity: String,
    /// Craft only: contract ids the process may use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<String>,
}

static RECIPES: LazyLock<Vec<RecipeDef>> = LazyLock::new(|| {
    let list: Vec<RecipeDef> = serde_json::from_str(include_str!("../../../recipes/recipes.json"))
        .expect("recipes/recipes.json parses");
    for r in &list {
        if let Err(why) = validate(r) {
            panic!("recipes/recipes.json: {why}");
        }
    }
    list
});

/// The compiled-in recipes, in file order.
pub fn recipes() -> &'static [RecipeDef] {
    &RECIPES
}

/// A recipe is well formed: known kind, input and rarity, and every allowed id is a craft.
pub fn validate(r: &RecipeDef) -> Result<(), String> {
    match r.kind.as_str() {
        "craft" => {
            if !matches!(r.input.as_deref(), Some("normal-base" | "same-base")) {
                return Err(format!("recipe {} has an unknown input", r.id));
            }
            for a in &r.allowed {
                if registry::contract_by_id(a).is_none() || a == "recombine" {
                    return Err(format!("recipe {} allows the unknown craft {a}", r.id));
                }
            }
        }
        "recombine" => {}
        other => return Err(format!("recipe {} has the unknown kind {other}", r.id)),
    }
    if rarity(&r.rarity).is_none() {
        return Err(format!(
            "recipe {} has the unknown rarity {}",
            r.id, r.rarity
        ));
    }
    Ok(())
}

fn rarity(name: &str) -> Option<Rarity> {
    [Rarity::Normal, Rarity::Magic, Rarity::Rare]
        .into_iter()
        .find(|r| r.name() == name)
}

/// The source rank a craft delivers ([`ModSource::rank`]); plain rolling is `Native`.
fn craft_rank(id: &str) -> u8 {
    match id {
        "essence" => 0,
        "influence-exalt" | "awakener" | "harvest-influenced" => 1,
        "fossil" => 2,
        _ => 5,
    }
}

/// (source ranks the recipe can deliver, the rank its core needs some requirement to have).
fn delivers(r: &RecipeDef) -> (BTreeSet<u8>, Option<u8>) {
    if r.kind == "recombine" {
        return ((0..=5).collect(), None);
    }
    let mut set: BTreeSet<u8> = r.allowed.iter().map(|a| craft_rank(a)).collect();
    if r.allowed.is_empty() {
        set.extend([0, 1, 2, 4, 5]);
    }
    set.insert(5);
    let core = set.iter().next().copied();
    (set, core)
}

/// Source ranks available to each requirement of `spec`.
pub fn requirement_ranks(b: &Bundle, spec: &Target) -> Vec<BTreeSet<u8>> {
    let ctx = SourceCtx::new(b, spec.base, spec.influences);
    spec.requirements
        .iter()
        .map(|r| {
            r.accepted
                .iter()
                .flat_map(|m| ctx.sources(*m))
                .map(ModSource::rank)
                .collect()
        })
        .collect()
}

/// One recipe instantiation for a good, evaluated against the current market.
#[derive(Clone, Debug)]
pub struct Proposal {
    pub process: Process,
    pub new_goods: Vec<Good>,
    pub name: String,
    pub cost: Option<f64>,
    pub p_success: Option<f64>,
    pub status: ProcessStatus,
    pub reason: Option<String>,
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn input_good(b: &Bundle, out: &Good, input: &str) -> Good {
    let base = &b.base_at(out.spec.base).name;
    let (rarity, id, name) = match input {
        "normal-base" => (
            Rarity::Normal,
            format!("{}-{}-normal", slug(base), out.spec.ilvl_min),
            format!("{base} ilvl {} normal", out.spec.ilvl_min),
        ),
        _ => (
            out.spec.rarity,
            format!(
                "{}-{}-{}-base",
                slug(base),
                out.spec.ilvl_min,
                out.spec.rarity.name().to_ascii_lowercase()
            ),
            format!(
                "{base} ilvl {} {}",
                out.spec.ilvl_min,
                out.spec.rarity.name().to_ascii_lowercase()
            ),
        ),
    };
    Good {
        id,
        name,
        spec: Target {
            base: out.spec.base,
            ilvl_min: out.spec.ilvl_min,
            influences: if rarity == Rarity::Normal {
                0
            } else {
                out.spec.influences
            },
            rarity,
            catalyst: None,
            requirements: Vec::new(),
            min_hits: 0,
        },
        buy: None,
        sell: None,
    }
}

/// Every k-subset of `items`, lexicographic.
fn choose(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (i, x) in items.iter().enumerate() {
        for mut rest in choose(&items[i + 1..], k - 1) {
            rest.insert(0, *x);
            out.push(rest);
        }
    }
    out
}

/// Splits of a good's requirements into two non-empty halves: per side, ⌈k/2⌉ to A.
fn splits(out: &Good) -> Vec<(Vec<usize>, Vec<usize>)> {
    let t = &out.spec;
    let mut per_side: Vec<Vec<(Vec<usize>, Vec<usize>)>> = Vec::new();
    for side in Side::ALL {
        let reqs = market::side_requirements(t, side);
        let k = reqs.len();
        let take = k.div_ceil(2);
        per_side.push(
            choose(&reqs, take)
                .into_iter()
                .map(|a| {
                    let b: Vec<usize> = reqs.iter().copied().filter(|r| !a.contains(r)).collect();
                    (a, b)
                })
                .collect(),
        );
    }
    let mut out_splits = Vec::new();
    for (pa, pb) in &per_side[0] {
        for (sa, sb) in &per_side[1] {
            let a: Vec<usize> = pa.iter().chain(sa).copied().collect();
            let b: Vec<usize> = pb.iter().chain(sb).copied().collect();
            if a.is_empty() || b.is_empty() {
                continue;
            }
            out_splits.push((a, b));
            if out_splits.len() == MAX_SPLITS {
                return out_splits;
            }
        }
    }
    out_splits
}

fn half(out: &Good, reqs: &[usize], id: String, name: String) -> Good {
    let mut spec = out.spec.clone();
    spec.requirements = reqs
        .iter()
        .map(|i| out.spec.requirements[*i].clone())
        .collect();
    spec.min_hits = spec.requirements.len() as u16;
    spec.influences = 0;
    spec.catalyst = None;
    Good {
        id,
        name,
        spec,
        buy: None,
        sell: None,
    }
}

fn next_process_id(m: &Market, taken: &BTreeSet<String>) -> String {
    let mut n = m.processes.len() + 1;
    loop {
        let id = format!("p{n}");
        if m.process(&id).is_none() && !taken.contains(&id) {
            return id;
        }
        n += 1;
    }
}

/// Every recipe instantiation for good `g`, scarcest source first, each evaluated with the current
/// market values. Nothing is added to the market.
pub fn propose(
    b: &Bundle,
    m: &Market,
    g: usize,
    prices: &BTreeMap<String, f64>,
    donors: &[DonorSpec],
) -> Vec<Proposal> {
    let out = &m.goods[g];
    let ranks = requirement_ranks(b, &out.spec);
    let mut drafts: Vec<(u8, Process, Vec<Good>, String)> = Vec::new();
    let mut taken = BTreeSet::new();
    for r in recipes() {
        if rarity(&r.rarity) != Some(out.spec.rarity) {
            continue;
        }
        let (set, core) = delivers(r);
        if !ranks.iter().all(|rs| rs.iter().any(|x| set.contains(x))) {
            continue;
        }
        if let Some(core) = core
            && !ranks.is_empty()
            && !ranks.iter().any(|rs| rs.contains(&core))
        {
            continue;
        }
        let rank = core.unwrap_or(3);
        match r.kind.as_str() {
            "craft" => {
                let input = input_good(b, out, r.input.as_deref().unwrap_or("same-base"));
                let mut new_goods = Vec::new();
                if m.good(&input.id).is_none() {
                    new_goods.push(input.clone());
                }
                let id = next_process_id(m, &taken);
                taken.insert(id.clone());
                drafts.push((
                    rank,
                    Process {
                        id,
                        output: out.id.clone(),
                        kind: ProcessKind::Craft {
                            from: Some(input.id),
                            allowed: r.allowed.clone(),
                            plan: CraftPlan::default(),
                        },
                        recipe: Some(r.id.clone()),
                    },
                    new_goods,
                    r.name.clone(),
                ));
            }
            _ => {
                let all = splits(out);
                let single = all.len() == 1;
                for (i, (a, bh)) in all.iter().enumerate() {
                    let tag = if single {
                        String::new()
                    } else {
                        (i + 1).to_string()
                    };
                    let ga = half(
                        out,
                        a,
                        format!("{}-a{tag}", out.id),
                        format!("{} / A{tag}", out.name),
                    );
                    let gb = half(
                        out,
                        bh,
                        format!("{}-b{tag}", out.id),
                        format!("{} / B{tag}", out.name),
                    );
                    let mut new_goods = Vec::new();
                    for h in [&ga, &gb] {
                        if m.good(&h.id).is_none() {
                            new_goods.push(h.clone());
                        }
                    }
                    let id = next_process_id(m, &taken);
                    taken.insert(id.clone());
                    drafts.push((
                        rank,
                        Process {
                            id,
                            output: out.id.clone(),
                            kind: ProcessKind::Recombine { a: ga.id, b: gb.id },
                            recipe: Some(r.id.clone()),
                        },
                        new_goods,
                        r.name.clone(),
                    ));
                }
            }
        }
    }
    drafts.sort_by_key(|d| d.0);

    drafts
        .into_iter()
        .map(|(_, process, new_goods, name)| {
            let mut goods = m.goods.clone();
            goods.extend(new_goods.iter().cloned());
            let mut processes = m.processes.clone();
            processes.push(process.clone());
            let root = m.goods[m.root].id.clone();
            let evaluated = Market::new(goods, processes, &root)
                .map_err(|e| e.to_string())
                .and_then(|tm| {
                    market::solve(b, &tm, prices, donors)
                        .map_err(|e| e.to_string())
                        .map(|s| s.processes.last().cloned())
                });
            let (cost, p_success, status, reason) = match evaluated {
                Ok(Some(v)) => {
                    let status = match v.status {
                        ProcessStatus::Error => ProcessStatus::NeedsInput,
                        other => other,
                    };
                    (v.cost, v.p_success, status, v.error)
                }
                Ok(None) => (None, None, ProcessStatus::NeedsInput, None),
                Err(e) => (None, None, ProcessStatus::NeedsInput, Some(e)),
            };
            Proposal {
                process,
                new_goods,
                name,
                cost,
                p_success,
                status,
                reason,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_recipes_validate() {
        let ids: Vec<&str> = recipes().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "alt-spam",
                "reforge",
                "essence",
                "fossil",
                "recombine-split"
            ]
        );
    }

    #[test]
    fn rejects_unknown_crafts() {
        let r = RecipeDef {
            id: "x".into(),
            name: "x".into(),
            kind: "craft".into(),
            input: Some("normal-base".into()),
            rarity: "Magic".into(),
            allowed: vec!["teleport".into()],
        };
        assert!(validate(&r).is_err());
    }
}
