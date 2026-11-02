//! Craft kernels: what each currency does to an item, as an outcome distribution.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::data::Bundle;
use crate::item::{Blocked, Hit, ItemState, Side, SideState};
use crate::registry::{self, DonorSpec};
use crate::target::{self, Target};
use crate::weights::{PoolFilter, PoolMod, pool_with};

/// Whether any currency use consumes a catalyst's quality (user-stated 3.29 behaviour).
pub const CATALYST_CONSUMED_BY_CURRENCY: bool = true;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Action {
    Chaos,
    Exalted,
    /// `influence` is a single influence bit (1 Shaper, 2 Elder, 4 Crusader, 8 Redeemer, 16 Hunter,
    /// 32 Warlord).
    InfluenceExalt {
        influence: u8,
    },
    Annul,
    Divine,
    HarvestInfluenced,
    HarvestRandomiseInfluence,
    BenchLock {
        side: Side,
    },
    ApplyCatalyst {
        catalyst: u8,
    },
    Combine {
        donor: u8,
    },
    /// Normal -> magic with one mod.
    Transmute,
    /// Magic -> magic, its mods re-rolled; the mod count is kept.
    Alteration,
    /// Magic with a free slot -> one more mod.
    Augmentation,
    /// Magic -> rare with one more mod.
    Regal,
    /// Removes every mod on unlocked sides and the lock.
    Scour,
    /// Craicic Chimeral: stores the magic item's mods.
    Imprint,
    /// The item becomes its imprint again; the imprint is spent.
    Restore,
    /// Reforge as a rare with the essence's mod placed first (`essence` indexes `Bundle::essences`).
    Essence {
        essence: u16,
    },
    /// Reforge as a rare with the fossils' weights (`fossils` index `Bundle::fossils`, sorted).
    Fossil {
        fossils: Vec<u16>,
    },
}

fn consume_catalyst(a: &Action) -> bool {
    CATALYST_CONSUMED_BY_CURRENCY && !matches!(a, Action::ApplyCatalyst { .. })
}

/// Canonical form: both sides sorted.
fn sorted(mut s: ItemState) -> ItemState {
    s.prefixes.wanted.sort();
    s.prefixes.blocked.sort();
    s.suffixes.wanted.sort();
    s.suffixes.blocked.sort();
    s
}

/// Apply the catalyst consumption rule, merge identical states and normalise the distribution.
fn finish(consume: bool, outs: Vec<(ItemState, f64)>) -> Vec<(ItemState, f64)> {
    let mut map: BTreeMap<ItemState, f64> = BTreeMap::new();
    for (mut state, p) in outs {
        if consume {
            state.catalyst = None;
        }
        if p <= 0.0 {
            continue;
        }
        *map.entry(sorted(state)).or_insert(0.0) += p;
    }
    let total: f64 = map.values().sum();
    if total > 0.0 {
        for p in map.values_mut() {
            *p /= total;
        }
    }
    map.into_iter().collect()
}

// ------------------------------------------------------------------ fill: drawing new mods

#[derive(Clone)]
struct Entry {
    mod_index: u32,
    group: u16,
    req: Option<u16>,
    weight: f64,
    influence: bool,
    side: usize,
    used: bool,
}

#[derive(Clone, Copy)]
struct SideMass {
    free: u8,
}

/// Mean-field model of the untracked mods. Classes are indexed `side * 2 + influenced`.
/// `classes[k]` is the remaining weight of class `k`; `m[k][j]` is the sum over untracked groups
/// of `w_g[k] * w_g[j]`, so drawing from class `k` removes `m[k][j] / classes[k]` from class `j`
/// in expectation — including the weight the same group holds in the other classes and sides.
struct Model {
    entries: Vec<Entry>,
    sides: [SideMass; 2],
    classes: [f64; 4],
    m: [[f64; 4]; 4],
    buckets: Vec<[f64; 4]>,
    bucket_of_group: BTreeMap<u16, usize>,
}

fn accepted_position(t: &Target, req: u16, mod_index: u32) -> Option<u8> {
    t.requirements
        .get(req as usize)?
        .accepted
        .iter()
        .position(|m| *m == mod_index)
        .map(|i| i as u8)
}

fn present_groups(t: &Target, s: &ItemState) -> Vec<u16> {
    let mut groups: Vec<u16> = Vec::new();
    for side in Side::ALL {
        let ss = s.side(side);
        for hit in &ss.wanted {
            if let Some(r) = t.requirements.get(hit.req as usize) {
                groups.push(r.group);
            }
        }
        for b in &ss.blocked {
            groups.push(b.group);
        }
    }
    groups.sort_unstable();
    groups.dedup();
    groups
}

fn build_model(b: &Bundle, t: &Target, s: &ItemState, excluded: &[u16], pm: &PoolMod) -> Model {
    let tags = b.item_tags(s.base, s.influences);
    let mut model = Model {
        entries: Vec::new(),
        sides: [SideMass { free: 0 }, SideMass { free: 0 }],
        classes: [0.0; 4],
        m: [[0.0; 4]; 4],
        buckets: Vec::new(),
        bucket_of_group: BTreeMap::new(),
    };
    let catalyst = s.catalyst.map(|(c, q)| {
        let cat = &b.catalysts[c as usize];
        (cat.tags.as_slice(), q)
    });
    let mut bucket_weights: BTreeMap<u16, [f64; 4]> = BTreeMap::new();
    for side in Side::ALL {
        let si = side.index();
        let free = s.free(b, side);
        model.sides[si].free = free;
        if free == 0 {
            continue;
        }
        let entries = pool_with(
            b,
            &PoolFilter {
                side,
                tags: &tags,
                ilvl: s.ilvl,
                excluded_groups: excluded,
                influence_only: false,
                catalyst,
            },
            pm,
        );
        for e in entries {
            let req = registry::requirement_for(t, side, e.group);
            if req.is_some() {
                model.entries.push(Entry {
                    mod_index: e.mod_index,
                    group: e.group,
                    req,
                    weight: e.weight,
                    influence: e.influence,
                    side: si,
                    used: false,
                });
            } else {
                let k = si * 2 + usize::from(e.influence);
                bucket_weights.entry(e.group).or_insert([0.0; 4])[k] += e.weight;
            }
        }
    }
    for (group, weights) in bucket_weights {
        let idx = model.buckets.len();
        model.buckets.push(weights);
        model.bucket_of_group.insert(group, idx);
        for k in 0..4 {
            model.classes[k] += weights[k];
            for j in 0..4 {
                model.m[k][j] += weights[k] * weights[j];
            }
        }
    }
    model
}

impl Model {
    fn total(&self, influence_only: bool) -> f64 {
        let mut total = 0.0;
        for e in &self.entries {
            if e.used || self.sides[e.side].free == 0 || (influence_only && !e.influence) {
                continue;
            }
            total += e.weight;
        }
        for k in 0..4 {
            if self.sides[k / 2].free == 0 {
                continue;
            }
            if influence_only && k % 2 == 0 {
                continue;
            }
            total += self.classes[k];
        }
        total
    }

    fn class_available(&self, k: usize, influence_only: bool) -> bool {
        self.sides[k / 2].free > 0 && (!influence_only || k % 2 == 1)
    }

    /// Mark (or unmark) every mod of `group` as drawn, removing its untracked weight too.
    fn set_group_used(&mut self, group: u16, used: bool) {
        for e in &mut self.entries {
            if e.group == group {
                e.used = used;
            }
        }
        if let Some(idx) = self.bucket_of_group.get(&group).copied() {
            let weights = self.buckets[idx];
            for (k, w) in weights.iter().enumerate() {
                if used {
                    self.classes[k] = (self.classes[k] - w).max(0.0);
                } else {
                    self.classes[k] += w;
                }
            }
        }
    }

    /// Expected weight removed from every class when class `k` is drawn from.
    fn removal(&self, k: usize) -> [f64; 4] {
        let mut r = [0.0; 4];
        if self.classes[k] <= 0.0 {
            return r;
        }
        for (j, slot) in r.iter_mut().enumerate() {
            *slot = self.m[k][j] / self.classes[k];
        }
        r
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_all(
    b: &Bundle,
    t: &Target,
    state: &ItemState,
    model: &mut Model,
    remaining: u8,
    influence_only: bool,
    p: f64,
    out: &mut Vec<(ItemState, f64)>,
) {
    if remaining == 0 {
        out.push((state.clone(), p));
        return;
    }
    let total = model.total(influence_only);
    if total <= 0.0 {
        out.push((state.clone(), p));
        return;
    }

    // Requirement-group mods branch individually: each tier is its own state.
    let mut entries: Vec<usize> = (0..model.entries.len()).collect();
    entries.retain(|i| {
        let e = &model.entries[*i];
        !e.used
            && model.sides[e.side].free > 0
            && (!influence_only || e.influence)
            && e.weight > 0.0
    });
    for idx in entries {
        let (mod_index, group, req, weight, side) = {
            let e = &model.entries[idx];
            (e.mod_index, e.group, e.req, e.weight, e.side)
        };
        let prob = p * weight / total;
        let Some(req) = req else { continue };
        let mut next = state.clone();
        model.set_group_used(group, true);
        model.sides[side].free -= 1;
        if let Some(hit_idx) = accepted_position(t, req, mod_index) {
            let pok = target::p_value_ok(b, t, req, mod_index);
            let mut push = |value_ok: bool, prob: f64, next: &mut ItemState| {
                next.side_mut(Side::ALL[side]).wanted.push(Hit {
                    req,
                    idx: hit_idx,
                    value_ok,
                });
                draw_all(b, t, next, model, remaining - 1, false, prob, out);
                next.side_mut(Side::ALL[side]).wanted.pop();
            };
            if pok >= 1.0 {
                push(true, prob, &mut next);
            } else if pok <= 0.0 {
                push(false, prob, &mut next);
            } else {
                push(true, prob * pok, &mut next.clone());
                push(false, prob * (1.0 - pok), &mut next);
            }
        } else {
            let influence = b.mod_at(mod_index).influence;
            next.side_mut(Side::ALL[side])
                .blocked
                .push(Blocked { group, influence });
            draw_all(b, t, &next, model, remaining - 1, false, prob, out);
        }
        model.sides[side].free += 1;
        model.set_group_used(group, false);
    }

    // Untracked mods are indistinguishable per (side, influence class): one branch per class. The
    // group drawn is unknown, so every class loses its expected share of that group's weight.
    for k in 0..4 {
        if !model.class_available(k, influence_only) {
            continue;
        }
        let mass = model.classes[k];
        if mass <= 0.0 {
            continue;
        }
        let prob = p * mass / total;
        let removal = model.removal(k);
        let mut next = state.clone();
        let side = k / 2;
        next.side_mut(Side::ALL[side]).untracked += 1;
        if k % 2 == 1 {
            next.side_mut(Side::ALL[side]).untracked_influenced += 1;
        }
        let saved = model.classes;
        for (j, r) in removal.iter().enumerate() {
            model.classes[j] = (model.classes[j] - r).max(0.0);
        }
        model.sides[side].free -= 1;
        draw_all(b, t, &next, model, remaining - 1, false, prob, out);
        model.sides[side].free += 1;
        model.classes = saved;
    }
}

/// Draw `n_new` mods without replacement on every side that has a free slot.
pub fn fill(
    b: &Bundle,
    t: &Target,
    s: &ItemState,
    n_new: u8,
    first_influence_only: bool,
) -> Vec<(ItemState, f64)> {
    fill_with(
        b,
        t,
        s,
        n_new,
        first_influence_only,
        &PoolMod::default(),
        &[],
    )
}

/// [`fill`] over a fossil-modified pool, with extra groups (an untracked forced mod's) excluded.
fn fill_with(
    b: &Bundle,
    t: &Target,
    s: &ItemState,
    n_new: u8,
    first_influence_only: bool,
    pm: &PoolMod,
    extra_excluded: &[u16],
) -> Vec<(ItemState, f64)> {
    let mut excluded = present_groups(t, s);
    excluded.extend_from_slice(extra_excluded);
    let mut model = build_model(b, t, s, &excluded, pm);
    let mut out = Vec::new();
    draw_all(
        b,
        t,
        s,
        &mut model,
        n_new,
        first_influence_only,
        1.0,
        &mut out,
    );
    finish(false, out)
}

// ------------------------------------------------------------------ reforge

fn reforge(
    b: &Bundle,
    t: &Target,
    s: &ItemState,
    respect_locks: bool,
    first_influence_only: bool,
) -> Vec<(ItemState, f64)> {
    reforge_with(
        b,
        t,
        s,
        respect_locks,
        first_influence_only,
        &PoolMod::default(),
        None,
    )
}

/// A rare reforge: unlocked sides cleared, the lock removed, then 4/5/6 mods in total (8:3:1),
/// the `forced` mod (an essence's) placed first, the rest drawn from the `pm`-modified pool.
fn reforge_with(
    b: &Bundle,
    t: &Target,
    s: &ItemState,
    respect_locks: bool,
    first_influence_only: bool,
    pm: &PoolMod,
    forced: Option<u32>,
) -> Vec<(ItemState, f64)> {
    let mut base = s.clone();
    base.lock = 0;
    base.rarity = crate::item::Rarity::Rare;
    let mut kept = 0u8;
    for side in Side::ALL {
        if respect_locks && registry::side_locked(s, side) {
            kept += s.side(side).slots_used();
        } else {
            *base.side_mut(side) = SideState::default();
        }
    }
    let starts: Vec<(ItemState, f64, Vec<u16>)> = match forced {
        Some(m) => {
            kept += 1;
            place_mod(b, t, &base, m)
                .into_iter()
                .map(|(st, p)| (st, p, vec![b.mod_at(m).group]))
                .collect()
        }
        None => vec![(base.clone(), 1.0, Vec::new())],
    };
    let (cap_p, cap_s) = base.caps(b);
    let capacity = (cap_p + cap_s).saturating_sub(kept);
    let mut outs = Vec::new();
    for (start, p_start, excluded) in &starts {
        for (k, weight) in [(4u8, 8.0 / 12.0), (5u8, 3.0 / 12.0), (6u8, 1.0 / 12.0)] {
            let n_new = k.saturating_sub(kept).min(capacity);
            for (state, p) in fill_with(b, t, start, n_new, first_influence_only, pm, excluded) {
                outs.push((state, p * weight * p_start));
            }
        }
    }
    finish(false, outs)
}

/// `s` with one specific mod added on its own side: a hit (its value rolled), a blocked tier, or an
/// untracked mod.
fn place_mod(b: &Bundle, t: &Target, s: &ItemState, mod_index: u32) -> Vec<(ItemState, f64)> {
    let m = b.mod_at(mod_index);
    let side = m.generation;
    let mut out = Vec::new();
    match registry::requirement_for(t, side, m.group) {
        Some(req) => match accepted_position(t, req, mod_index) {
            Some(idx) => {
                let pok = target::p_value_ok(b, t, req, mod_index);
                for value_ok in [true, false] {
                    let p = if value_ok { pok } else { 1.0 - pok };
                    if p <= 0.0 {
                        continue;
                    }
                    let mut st = s.clone();
                    st.side_mut(side).wanted.push(Hit { req, idx, value_ok });
                    out.push((st, p));
                }
            }
            None => {
                let mut st = s.clone();
                st.side_mut(side).blocked.push(Blocked {
                    group: m.group,
                    influence: m.influence,
                });
                out.push((st, 1.0));
            }
        },
        None => {
            let mut st = s.clone();
            st.side_mut(side).untracked += 1;
            if m.influence {
                st.side_mut(side).untracked_influenced += 1;
            }
            out.push((st, 1.0));
        }
    }
    out
}

// ------------------------------------------------------------------ normal and magic crafts

fn cleared(s: &ItemState) -> ItemState {
    let mut st = s.clone();
    st.prefixes = SideState::default();
    st.suffixes = SideState::default();
    st.lock = 0;
    st
}

fn transmute(b: &Bundle, t: &Target, s: &ItemState) -> Vec<(ItemState, f64)> {
    let mut st = cleared(s);
    st.rarity = crate::item::Rarity::Magic;
    fill(b, t, &st, 1, false)
}

/// The mod count is kept: a one-mod item re-rolls to one mod, a two-mod item to two.
fn alteration(b: &Bundle, t: &Target, s: &ItemState) -> Vec<(ItemState, f64)> {
    let n = s.mod_count().clamp(1, 2);
    fill(b, t, &cleared(s), n, false)
}

fn regal(b: &Bundle, t: &Target, s: &ItemState) -> Vec<(ItemState, f64)> {
    let mut st = s.clone();
    st.rarity = crate::item::Rarity::Rare;
    fill(b, t, &st, 1, false)
}

/// Unlocked sides and the lock go; an item left with no mods becomes normal.
fn scour(s: &ItemState) -> Vec<(ItemState, f64)> {
    let mut st = s.clone();
    for side in Side::ALL {
        if !registry::side_locked(s, side) {
            *st.side_mut(side) = SideState::default();
        }
    }
    st.lock = 0;
    if st.mod_count() == 0 {
        st.rarity = crate::item::Rarity::Normal;
    }
    vec![(st, 1.0)]
}

fn imprint(s: &ItemState) -> Vec<(ItemState, f64)> {
    let mut st = s.clone();
    st.imprint = Some(Box::new((s.prefixes.clone(), s.suffixes.clone())));
    vec![(st, 1.0)]
}

fn restore(s: &ItemState) -> Vec<(ItemState, f64)> {
    let Some(imp) = &s.imprint else {
        return vec![(s.clone(), 1.0)];
    };
    let mut st = s.clone();
    st.prefixes = imp.0.clone();
    st.suffixes = imp.1.clone();
    st.rarity = crate::item::Rarity::Magic;
    st.lock = 0;
    st.imprint = None;
    vec![(st, 1.0)]
}

// ------------------------------------------------------------------ recombine (two items)

#[derive(Clone, Copy, PartialEq)]
enum PoolMod2 {
    Hit(Hit),
    Blocked(Blocked),
}

fn entry_group(t: &Target, e: &PoolMod2) -> u16 {
    match e {
        PoolMod2::Hit(h) => t.requirements[h.req as usize].group,
        PoolMod2::Blocked(x) => x.group,
    }
}

/// Every k-subset of `0..n`, in lexicographic order.
fn subsets(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut cur = Vec::with_capacity(k);
    fn rec(start: usize, n: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for i in start..n {
            cur.push(i);
            rec(i + 1, n, k, cur, out);
            cur.pop();
        }
    }
    rec(0, n, k, &mut cur, &mut out);
    out
}

/// Distribution of one side of a recombined item: (side state, probability).
fn recombine_side(b: &Bundle, t: &Target, pool: &[PoolMod2]) -> Vec<(SideState, f64)> {
    let n = pool.len();
    if n == 0 {
        return vec![(SideState::default(), 1.0)];
    }
    let row = registry::RECOMBINE_MOD_COUNT[n.min(6) - 1];
    let mut out: Vec<(SideState, f64)> = Vec::new();
    for (k, pk) in row.iter().enumerate() {
        if *pk <= 0.0 || k > n {
            continue;
        }
        let subs = subsets(n, k);
        let p_sub = pk / subs.len() as f64;
        for sub in subs {
            // Entries of one group collapse to one mod; each duplicate is the survivor equally often.
            let mut groups: Vec<(u16, Vec<PoolMod2>)> = Vec::new();
            for i in sub {
                let e = pool[i];
                let g = entry_group(t, &e);
                match groups.iter_mut().find(|(x, _)| *x == g) {
                    Some((_, v)) => v.push(e),
                    None => groups.push((g, vec![e])),
                }
            }
            let mut partial: Vec<(SideState, f64)> = vec![(SideState::default(), p_sub)];
            for (_, choices) in groups {
                let share = 1.0 / choices.len() as f64;
                let mut next = Vec::new();
                for (ss, p) in &partial {
                    for e in &choices {
                        match e {
                            PoolMod2::Blocked(x) => {
                                let mut s2 = ss.clone();
                                s2.blocked.push(*x);
                                next.push((s2, p * share));
                            }
                            PoolMod2::Hit(h) => {
                                let m = t.requirements[h.req as usize].accepted[h.idx as usize];
                                let pok = target::p_value_ok(b, t, h.req, m);
                                for value_ok in [true, false] {
                                    let pv = if value_ok { pok } else { 1.0 - pok };
                                    if pv <= 0.0 {
                                        continue;
                                    }
                                    let mut s2 = ss.clone();
                                    s2.wanted.push(Hit { value_ok, ..*h });
                                    next.push((s2, p * share * pv));
                                }
                            }
                        }
                    }
                }
                partial = next;
            }
            out.extend(partial);
        }
    }
    out
}

fn side_pool(a: &SideState, c: &SideState) -> Vec<PoolMod2> {
    let mut pool = Vec::new();
    for ss in [a, c] {
        pool.extend(ss.wanted.iter().map(|h| PoolMod2::Hit(*h)));
        pool.extend(ss.blocked.iter().map(|x| PoolMod2::Blocked(*x)));
    }
    pool
}

/// Why two items cannot be recombined, or `None` when they can.
pub fn recombine_blocker(b: &Bundle, a: &ItemState, c: &ItemState) -> Option<&'static str> {
    use crate::item::Rarity;
    if a.rarity != Rarity::Rare || c.rarity != Rarity::Rare {
        return Some("both items must be rare");
    }
    if b.base_at(a.base).class != b.base_at(c.base).class {
        return Some("both items must be the same item class");
    }
    if a.prefixes.untracked + a.suffixes.untracked + c.prefixes.untracked + c.suffixes.untracked > 0
    {
        return Some(
            "this item has mods the target does not track, so recombine cannot be modelled precisely",
        );
    }
    None
}

/// The Recombinator: base 1/2 each, ilvl `min(higher, average + 2)`, influence/catalyst/lock lost,
/// and per side `N` of the pooled mods kept (`RECOMBINE_MOD_COUNT`), chosen uniformly.
pub fn recombine(b: &Bundle, t: &Target, a: &ItemState, c: &ItemState) -> Vec<(ItemState, f64)> {
    if recombine_blocker(b, a, c).is_some() {
        return Vec::new();
    }
    let ilvl = {
        let (x, y) = (a.ilvl as u16, c.ilvl as u16);
        x.max(y).min((x + y) / 2 + 2) as u8
    };
    let prefixes = recombine_side(b, t, &side_pool(&a.prefixes, &c.prefixes));
    let suffixes = recombine_side(b, t, &side_pool(&a.suffixes, &c.suffixes));
    let influences = if registry::RECOMBINE_KEEPS_INFLUENCE {
        a.influences & c.influences
    } else {
        0
    };
    let mut outs = Vec::new();
    for base in [a.base, c.base] {
        for (ps, pp) in &prefixes {
            for (ss, sp) in &suffixes {
                let mut st = ItemState::new(base, ilvl, crate::item::Rarity::Rare, influences);
                st.prefixes = ps.clone();
                st.suffixes = ss.clone();
                outs.push((st, 0.5 * pp * sp));
            }
        }
    }
    finish(false, outs)
}

// ------------------------------------------------------------------ individual actions

fn annul(b: &Bundle, t: &Target, s: &ItemState) -> Vec<(ItemState, f64)> {
    let _ = (b, t);
    let mut count = u32::from(s.lock != 0);
    for side in Side::ALL {
        if registry::side_locked(s, side) {
            continue;
        }
        let ss = s.side(side);
        count += ss.wanted.len() as u32 + ss.blocked.len() as u32 + ss.untracked as u32;
    }
    if count == 0 {
        return vec![(s.clone(), 1.0)];
    }
    let unit = 1.0 / count as f64;
    let mut outs = Vec::new();
    if s.lock != 0 {
        let mut st = s.clone();
        st.lock = 0;
        outs.push((st, unit));
    }
    for side in Side::ALL {
        if registry::side_locked(s, side) {
            continue;
        }
        let ss = s.side(side);
        for i in 0..ss.wanted.len() {
            let mut st = s.clone();
            st.side_mut(side).wanted.remove(i);
            outs.push((st, unit));
        }
        for i in 0..ss.blocked.len() {
            let mut st = s.clone();
            st.side_mut(side).blocked.remove(i);
            outs.push((st, unit));
        }
        let untracked = ss.untracked;
        if untracked > 0 {
            let influenced = ss.untracked_influenced;
            if influenced > 0 {
                let mut st = s.clone();
                st.side_mut(side).untracked -= 1;
                st.side_mut(side).untracked_influenced -= 1;
                outs.push((st, unit * influenced as f64 / untracked as f64));
            }
            let plain = untracked - influenced;
            if plain > 0 {
                let mut st = s.clone();
                st.side_mut(side).untracked -= 1;
                outs.push((st, unit * plain as f64 / untracked as f64));
            }
        }
    }
    outs
}

fn divine(b: &Bundle, t: &Target, s: &ItemState) -> Vec<(ItemState, f64)> {
    let mut outs = vec![(s.clone(), 1.0)];
    for side in Side::ALL {
        if registry::side_locked(s, side) {
            continue;
        }
        let hits = s.side(side).wanted.len();
        for i in 0..hits {
            let hit = s.side(side).wanted[i];
            let Some(mod_index) = t
                .requirements
                .get(hit.req as usize)
                .and_then(|r| r.accepted.get(hit.idx as usize))
                .copied()
            else {
                continue;
            };
            let pok = target::p_value_ok(b, t, hit.req, mod_index);
            if pok >= 1.0 || pok <= 0.0 {
                continue;
            }
            let mut next = Vec::with_capacity(outs.len() * 2);
            for (st, p) in outs {
                let mut ok = st.clone();
                ok.side_mut(side).wanted[i].value_ok = true;
                next.push((ok, p * pok));
                let mut bad = st;
                bad.side_mut(side).wanted[i].value_ok = false;
                next.push((bad, p * (1.0 - pok)));
            }
            outs = next;
        }
    }
    outs
}

#[derive(Clone, Copy)]
enum Kept {
    None,
    Hit(Hit),
    Blocked(Blocked),
    UntrackedInfluenced(usize),
}

/// (state, probability, (group, side) of the mod added), for the donor's mod.
type AddedMod = (ItemState, f64, Option<(u16, Side)>);

fn donor_mod_shape(b: &Bundle, t: &Target, donor: &DonorSpec, base: &ItemState) -> Vec<AddedMod> {
    let m = b.mod_at(donor.mod_index);
    let side = m.generation;
    let mut out = Vec::new();
    match registry::requirement_for(t, side, m.group) {
        Some(req) => match accepted_position(t, req, donor.mod_index) {
            Some(idx) => {
                let pok = target::p_value_ok(b, t, req, donor.mod_index);
                for value_ok in [true, false] {
                    let p = if value_ok { pok } else { 1.0 - pok };
                    if p <= 0.0 {
                        continue;
                    }
                    let mut st = base.clone();
                    st.side_mut(side).wanted.push(Hit { req, idx, value_ok });
                    out.push((st, p, Some((m.group, side))));
                }
            }
            None => {
                let mut st = base.clone();
                st.side_mut(side).blocked.push(Blocked {
                    group: m.group,
                    influence: true,
                });
                out.push((st, 1.0, Some((m.group, side))));
            }
        },
        None => {
            let mut st = base.clone();
            st.side_mut(side).untracked += 1;
            st.side_mut(side).untracked_influenced += 1;
            out.push((st, 1.0, None));
        }
    }
    out
}

fn combine(
    b: &Bundle,
    t: &Target,
    donors: &[DonorSpec],
    s: &ItemState,
    donor: u8,
) -> Vec<(ItemState, f64)> {
    let d = &donors[donor as usize];
    let result_base = || ItemState::new(s.base, s.ilvl, s.rarity, s.influences | d.influence);

    // Choices for the receiver's one kept influenced mod.
    let mut choices: Vec<Kept> = Vec::new();
    for side in Side::ALL {
        let ss = s.side(side);
        for hit in &ss.wanted {
            if registry::hit_is_influence_t(b, t, hit) {
                choices.push(Kept::Hit(*hit));
            }
        }
        for blocked in &ss.blocked {
            if blocked.influence {
                choices.push(Kept::Blocked(*blocked));
            }
        }
        if ss.untracked_influenced > 0 {
            choices.push(Kept::UntrackedInfluenced(side.index()));
        }
    }
    if choices.is_empty() {
        choices.push(Kept::None);
    }

    let choice_prob = 1.0 / choices.len() as f64;
    let mut outs: Vec<(ItemState, f64)> = Vec::new();
    for kept in choices {
        // States after keeping the receiver's mod (before the donor's mod is added).
        let mut kept_states: Vec<AddedMod> = Vec::new();
        match kept {
            Kept::None => kept_states.push((result_base(), 1.0, None)),
            Kept::Hit(hit) => {
                let Some(requirement) = t.requirements.get(hit.req as usize) else {
                    continue;
                };
                let (req, idx) = (hit.req, hit.idx);
                let side = requirement.side;
                let group = requirement.group;
                let pok = requirement
                    .accepted
                    .get(idx as usize)
                    .map(|m| target::p_value_ok(b, t, req, *m))
                    .unwrap_or(1.0);
                for value_ok in [true, false] {
                    let p = if value_ok { pok } else { 1.0 - pok };
                    if p <= 0.0 {
                        continue;
                    }
                    let mut st = result_base();
                    st.side_mut(side).wanted.push(Hit { req, idx, value_ok });
                    kept_states.push((st, p, Some((group, side))));
                }
            }
            Kept::Blocked(blocked) => {
                let side = t
                    .requirements
                    .iter()
                    .find(|r| r.group == blocked.group)
                    .map(|r| r.side)
                    .unwrap_or(Side::Prefix);
                let mut st = result_base();
                st.side_mut(side).blocked.push(blocked);
                kept_states.push((st, 1.0, Some((blocked.group, side))));
            }
            Kept::UntrackedInfluenced(side) => {
                let mut st = result_base();
                st.side_mut(Side::ALL[side]).untracked += 1;
                st.side_mut(Side::ALL[side]).untracked_influenced += 1;
                kept_states.push((st, 1.0, None));
            }
        }

        for (kept_state, p_kept, kept_mod) in kept_states {
            let added = donor_mod_shape(b, t, d, &kept_state);
            for (added_state, p_add, donor_mod) in added {
                let conflicts = match (kept_mod, donor_mod) {
                    (Some((g1, s1)), Some((g2, s2))) => {
                        let (cap_p, cap_s) = kept_state.caps(b);
                        let cap = match s1 {
                            Side::Prefix => cap_p,
                            Side::Suffix => cap_s,
                        };
                        g1 == g2 || (s1 == s2 && cap == 1)
                    }
                    _ => false,
                };
                let mut final_states: Vec<(ItemState, f64)> = Vec::new();
                if conflicts {
                    // Keep either mod with 1/2 each.
                    final_states.push((kept_state.clone(), p_kept * 0.5));
                    for (donor_only, p_only, _) in donor_mod_shape(b, t, d, &result_base()) {
                        final_states.push((donor_only, p_kept * p_add * p_only * 0.5));
                    }
                } else {
                    final_states.push((added_state, p_kept * p_add));
                }

                for (mut state, p) in final_states {
                    state.catalyst = s.catalyst;
                    let kept_count = state.mod_count();
                    let (cap_p, cap_s) = state.caps(b);
                    let capacity = (cap_p + cap_s).saturating_sub(kept_count);
                    for (k, weight) in [(4u8, 8.0 / 12.0), (5u8, 3.0 / 12.0), (6u8, 1.0 / 12.0)] {
                        let n_new = k.saturating_sub(kept_count).min(capacity);
                        for (filled, p_fill) in fill(b, t, &state, n_new, false) {
                            outs.push((filled, choice_prob * p * weight * p_fill));
                        }
                    }
                }
            }
        }
    }
    finish(false, outs)
}

/// Outcome distribution of `a` applied to `s`. Assumes `registry::applicable` holds.
pub fn outcomes(
    b: &Bundle,
    t: &Target,
    donors: &[DonorSpec],
    s: &ItemState,
    a: &Action,
) -> Vec<(ItemState, f64)> {
    let outs = match a {
        Action::Chaos => reforge(b, t, s, true, false),
        Action::Exalted => fill(b, t, s, 1, false),
        Action::InfluenceExalt { influence } => {
            let mut st = s.clone();
            st.influences |= influence;
            fill(b, t, &st, 1, true)
        }
        Action::Annul => annul(b, t, s),
        Action::Divine => divine(b, t, s),
        Action::HarvestInfluenced => reforge(
            b,
            t,
            s,
            registry::HARVEST_INFLUENCED_RESPECTS_LOCKS,
            registry::HARVEST_FIRST_DRAW_INFLUENCED,
        ),
        Action::HarvestRandomiseInfluence => {
            let mut outs = Vec::new();
            for bit in 0..6 {
                let mut st = s.clone();
                st.influences = 1 << bit;
                for (state, p) in reforge(b, t, &st, false, false) {
                    outs.push((state, p / 6.0));
                }
            }
            outs
        }
        Action::BenchLock { side } => {
            let mut st = s.clone();
            st.lock |= registry::lock_bit_for(*side);
            vec![(st, 1.0)]
        }
        Action::ApplyCatalyst { catalyst } => {
            let mut st = s.clone();
            st.catalyst = Some((*catalyst, 20));
            vec![(st, 1.0)]
        }
        Action::Combine { donor } => combine(b, t, donors, s, *donor),
        Action::Transmute => transmute(b, t, s),
        Action::Alteration => alteration(b, t, s),
        Action::Augmentation => fill(b, t, s, 1, false),
        Action::Regal => regal(b, t, s),
        Action::Scour => scour(s),
        Action::Imprint => imprint(s),
        Action::Restore => restore(s),
        Action::Essence { essence } => {
            let class = &b.base_at(s.base).class;
            match b.essence_mod(*essence, class) {
                Some(m) => reforge_with(b, t, s, true, false, &PoolMod::default(), Some(m)),
                None => Vec::new(),
            }
        }
        Action::Fossil { fossils } => {
            reforge_with(b, t, s, true, false, &PoolMod::fossils(b, fossils), None)
        }
    };
    finish(consume_catalyst(a), outs)
}

// ------------------------------------------------------------------ predecessors

fn all_hits(s: &ItemState) -> Vec<(Side, Hit)> {
    let mut out = Vec::new();
    for side in Side::ALL {
        for hit in &s.side(side).wanted {
            out.push((side, *hit));
        }
    }
    out
}

fn with_steering(
    out: &mut Vec<(Action, ItemState)>,
    action: Action,
    state: ItemState,
    catalysts: &[u8],
) {
    out.push((action.clone(), state.clone()));
    for c in catalysts {
        let mut st = state.clone();
        st.catalyst = Some((*c, 20));
        out.push((action.clone(), st));
    }
}

/// Candidate (action, previous state) pairs that can produce `s`.
pub fn predecessors(
    b: &Bundle,
    t: &Target,
    donors: &[DonorSpec],
    s: &ItemState,
) -> Vec<(Action, ItemState)> {
    let mut out: Vec<(Action, ItemState)> = Vec::new();
    if let Some((c, _)) = s.catalyst
        && CATALYST_CONSUMED_BY_CURRENCY
    {
        let mut st = s.clone();
        st.catalyst = None;
        out.push((Action::ApplyCatalyst { catalyst: c }, st));
        return out;
    }
    let catalysts = registry::offered_catalysts(b, t);

    // Divine: re-rolls values on hits that could fail.
    let mut low = s.clone();
    let mut any_low = false;
    for side in Side::ALL {
        if registry::side_locked(s, side) {
            continue;
        }
        for hit in &mut low.side_mut(side).wanted {
            let Some(mod_index) = t
                .requirements
                .get(hit.req as usize)
                .and_then(|r| r.accepted.get(hit.idx as usize))
                .copied()
            else {
                continue;
            };
            if target::p_value_ok(b, t, hit.req, mod_index) < 1.0 {
                hit.value_ok = false;
                any_low = true;
            }
        }
    }
    if any_low {
        out.push((Action::Divine, low));
    }

    // BenchLock: remove the lock mod.
    for side in Side::ALL {
        let bit = registry::lock_bit_for(side);
        if s.lock & bit != 0 {
            let mut st = s.clone();
            st.lock &= !bit;
            out.push((Action::BenchLock { side }, st));
        }
    }

    // HarvestInfluenced: the side it produced was empty and the other side was locked.
    for side in Side::ALL {
        let produced = s
            .side(side)
            .wanted
            .iter()
            .any(|h| registry::hit_is_influence_t(b, t, h));
        if !produced {
            continue;
        }
        let mut st = s.clone();
        *st.side_mut(side) = SideState::default();
        st.lock = registry::lock_bit_for(side.other());
        with_steering(&mut out, Action::HarvestInfluenced, st, &catalysts);
    }

    // Combine: the receiver held one influence mod of the other influence.
    for (di, d) in donors.iter().enumerate() {
        if d.influence & s.influences == 0 {
            continue;
        }
        for (_, hit) in all_hits(s) {
            let accepted = t
                .requirements
                .get(hit.req as usize)
                .and_then(|r| r.accepted.get(hit.idx as usize))
                .copied();
            if accepted != Some(d.mod_index) {
                continue;
            }
            let other_influences = s.influences & !d.influence;
            for (side2, hit2) in all_hits(s) {
                if !registry::hit_is_influence_t(b, t, &hit2) {
                    continue;
                }
                let Some(mod2) = t
                    .requirements
                    .get(hit2.req as usize)
                    .and_then(|r| r.accepted.get(hit2.idx as usize))
                    .copied()
                else {
                    continue;
                };
                if b.mod_influences(s.base, mod2) & other_influences == 0 {
                    continue;
                }
                let mut st = ItemState::new(s.base, s.ilvl, s.rarity, other_influences);
                st.side_mut(side2).wanted.push(hit2);
                with_steering(
                    &mut out,
                    Action::Combine { donor: di as u8 },
                    st,
                    &catalysts,
                );
            }
        }
    }

    // InfluenceExalt: the item had exactly this influence and one fewer influenced mod.
    if s.influences.count_ones() == 1 {
        let bit = s.influences;
        for (side, hit) in all_hits(s) {
            if !registry::hit_is_influence_t(b, t, &hit) {
                continue;
            }
            let mut st = s.clone();
            st.influences &= !bit;
            st.side_mut(side).wanted.retain(|h| *h != hit);
            with_steering(
                &mut out,
                Action::InfluenceExalt { influence: bit },
                st,
                &catalysts,
            );
        }
    }

    // Exalted: one fewer mod.
    for (side, hit) in all_hits(s) {
        let mut st = s.clone();
        st.side_mut(side).wanted.retain(|h| *h != hit);
        with_steering(&mut out, Action::Exalted, st, &catalysts);
    }

    use crate::item::Rarity;
    match s.rarity {
        Rarity::Rare => {
            // Chaos: a fresh rare.
            let fresh = ItemState::new(s.base, s.ilvl, s.rarity, s.influences);
            with_steering(&mut out, Action::Chaos, fresh, &catalysts);

            // Essence: the hit is the essence's mod; the item before was a normal one.
            let class = b.base_at(s.base).class.clone();
            for (_, hit) in all_hits(s) {
                let Some(m) = t
                    .requirements
                    .get(hit.req as usize)
                    .and_then(|r| r.accepted.get(hit.idx as usize))
                    .copied()
                else {
                    continue;
                };
                for e in b.essences_for_mod(m, &class) {
                    let normal = ItemState::new(s.base, s.ilvl, Rarity::Normal, s.influences);
                    out.push((Action::Essence { essence: e }, normal));
                }
                for f in b.fossils_adding(m) {
                    let normal = ItemState::new(s.base, s.ilvl, Rarity::Normal, s.influences);
                    out.push((Action::Fossil { fossils: vec![f] }, normal));
                }
            }

            // Regal: a magic item with one fewer mod.
            if s.mod_count() >= 1 && s.mod_count() <= 3 {
                for (side, hit) in all_hits(s) {
                    let mut st = s.clone();
                    st.rarity = Rarity::Magic;
                    st.side_mut(side).wanted.retain(|h| *h != hit);
                    let (p, q) = (st.prefixes.slots_used(), st.suffixes.slots_used());
                    if p <= 1 && q <= 1 {
                        out.push((Action::Regal, st));
                    }
                }
            }
        }
        Rarity::Magic => {
            // Alteration from a magic item holding the same number of other mods.
            let mut st = ItemState::new(s.base, s.ilvl, Rarity::Magic, s.influences);
            let n = s.mod_count().clamp(1, 2);
            st.prefixes.untracked = n.min(1);
            if n == 2 {
                st.suffixes.untracked = 1;
            }
            if st != *s {
                out.push((Action::Alteration, st));
            }
            // Transmute from the normal base.
            if s.mod_count() == 1 {
                out.push((
                    Action::Transmute,
                    ItemState::new(s.base, s.ilvl, Rarity::Normal, s.influences),
                ));
            }
            // Augmentation: one fewer mod.
            if s.mod_count() == 2 {
                for (side, hit) in all_hits(s) {
                    let mut st = s.clone();
                    st.side_mut(side).wanted.retain(|h| *h != hit);
                    out.push((Action::Augmentation, st));
                }
            }
        }
        Rarity::Normal => {}
    }

    let mut set: BTreeSet<(Action, ItemState)> = BTreeSet::new();
    for pair in out {
        set.insert(pair);
    }
    let mut list: Vec<(Action, ItemState)> = set
        .into_iter()
        .filter(|(a, pre)| {
            registry::contract(a)
                .requires
                .rarity
                .is_none_or(|r| r == pre.rarity)
        })
        .collect();
    // Scarcer mechanics first (essence, influence, fossil, bench, then plain rolling); stable.
    list.sort_by_key(|(a, _)| source_rank(a));
    list
}

/// [`crate::weights::ModSource::rank`] of the mechanic an action delivers.
fn source_rank(a: &Action) -> u8 {
    match a {
        Action::Essence { .. } => 0,
        Action::InfluenceExalt { .. } | Action::Combine { .. } | Action::HarvestInfluenced => 1,
        Action::Fossil { .. } => 2,
        Action::BenchLock { .. } => 4,
        _ => 5,
    }
}
