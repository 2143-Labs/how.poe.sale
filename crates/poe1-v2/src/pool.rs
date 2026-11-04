//! Modifier weights and pools: a mod's spawn weight on an item's tags (first matching spawn
//! entry, claim `spawn.first-match`), its generation multiplier, and the weighted candidate lists
//! the random ops draw from.

use crate::data::{Data, ModData};

/// The weight of the first spawn entry whose tag is in `tags` (sorted); 0 if none.
pub fn spawn(m: &ModData, tags: &[u16]) -> u32 {
    m.spawn
        .iter()
        .find(|(t, _)| tags.binary_search(t).is_ok())
        .map_or(0, |&(_, w)| w)
}

/// The first generation entry whose tag is in `tags`, divided by 100; 1.0 when the list is empty
/// or nothing matches.
pub fn generation(m: &ModData, tags: &[u16]) -> f64 {
    m.generation
        .iter()
        .find(|(t, _)| tags.binary_search(t).is_ok())
        .map_or(1.0, |&(_, w)| f64::from(w) / 100.0)
}

/// Spawn weight × generation multiplier.
pub fn weight(m: &ModData, tags: &[u16]) -> f64 {
    f64::from(spawn(m, tags)) * generation(m, tags)
}

/// The spawn weight of the entry with exactly this tag, ignoring the item's tags (Ducat pools).
pub fn weight_of_tag(m: &ModData, tag: u16) -> f64 {
    m.spawn
        .iter()
        .find(|(t, _)| *t == tag)
        .map_or(0.0, |&(_, w)| f64::from(w))
}

/// Normalises (candidate, weight) pairs to probabilities, dropping zero weights.
pub fn normalise<T>(weighted: Vec<(T, f64)>) -> Vec<(T, f64)> {
    let kept: Vec<(T, f64)> = weighted.into_iter().filter(|(_, w)| *w > 0.0).collect();
    let total: f64 = kept.iter().map(|(_, w)| w).sum();
    kept.into_iter().map(|(c, w)| (c, w / total)).collect()
}

/// Every mod of the given kinds with weight > 0 on `tags`, as (mod, weight).
pub fn weighted<'d>(
    data: &'d Data,
    kinds: &[crate::data::ModKind],
    tags: &'d [u16],
) -> impl Iterator<Item = (u32, &'d ModData, f64)> + 'd {
    let kinds = kinds.to_vec();
    kinds.into_iter().flat_map(move |k| {
        data.mods_of_kind(k).iter().filter_map(move |&i| {
            let m = data.mod_at(i);
            let w = weight(m, tags);
            (w > 0.0).then_some((i, m, w))
        })
    })
}
