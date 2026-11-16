//! The draw: which modifiers an item can roll, and at what exact chance.
//!
//! A modifier is in a side's pool only when every one of these holds:
//!
//! - it is an explicit modifier on that side (`ModKind::side`), not an implicit;
//! - its required level is at most the **item's** level (`item.item_level`), not the base's;
//! - its weight on the base's tags **and** the item's influence tag is positive
//!   ([`Data::weight_on`]) — this is why a Shaper shield rolls `MaximumTotemUber1` and a plain one
//!   does not;
//! - none of its groups is already held on the item, whatever side that other modifier sits on.
//!
//! Two further rules hold the arithmetic together.
//!
//! **A family cannot repeat.** The group rule above is the whole of it: a held group removes a
//! modifier from both sides' pools.
//!
//! **The side is drawn first**, in equal shares among the sides that can take a modifier, and then
//! the modifier on that side by weight. That is why [`one_add`] scales each side's weights by the
//! *other* sides' totals: it puts every outcome of one add over one denominator
//! (`k × Π totals` for `k` sides with room), so the shares stay integers and the total stays exact.

use crate::data::Data;
use crate::fraction::Fraction;
use crate::item::{Affix, Item, Rarity, Side};

/// One result of a draw and the integer weight that produced it. Every [`Weighted`] of one add
/// shares one denominator, so the weights can be summed and divided by [`crate::currency::ItemOutputSet::total`].
pub type Weighted = (Item, u128, Option<(Side, u16)>);

/// One result of a draw and its exact chance. Used where the results came from branches with
/// different denominators, and by [`adds`], which multiplies chances rather than weights.
pub type Mass = (Item, Fraction, Option<(Side, u16)>);

/// How many modifiers a side of an item of this rarity may hold.
pub fn rarity_cap(rarity: Rarity) -> usize {
    match rarity {
        Rarity::Normal => 0,
        Rarity::Magic => 1,
        Rarity::Rare => 3,
    }
}

/// How many more modifiers this side may take.
pub fn room(item: &Item, side: Side) -> usize {
    rarity_cap(item.rarity).saturating_sub(item.affixes(side).len())
}

/// Every group held on the item, both sides together, sorted. A modifier whose groups touch this
/// list is out of both pools.
///
/// A list rather than a set: an item holds at most six modifiers, so its groups number a handful and
/// a scan of a few `u16`s beats hashing — and a draw makes this call once per state it draws from.
pub fn held_groups(data: &Data, item: &Item) -> Vec<u16> {
    let mut held: Vec<u16> = Side::BOTH
        .into_iter()
        .flat_map(|side| item.affixes(side).iter())
        .flat_map(|affix| data.mod_at(affix.m).groups.iter().copied())
        .collect();
    held.sort_unstable();
    held.dedup();
    held
}

/// The pool of one side: `(modifier, weight)` for every modifier that side could roll on this item,
/// in the data file's order.
///
/// Empty when the side has no room, or when nothing is left to roll — an item holding every group
/// the base can roll has an empty pool on both sides.
pub fn draw_weights(data: &Data, item: &Item, side: Side) -> Vec<(u16, u128)> {
    if room(item, side) == 0 {
        return Vec::new();
    }
    let held = held_groups(data, item);
    data.mods
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind.side() == Some(side))
        .filter(|(_, m)| m.level <= u32::from(item.item_level))
        .filter(|(_, m)| !m.groups.iter().any(|group| held.contains(group)))
        .map(|(m, _)| {
            let m = m as u16;
            (m, data.weight_on(m, item.base, item.influence))
        })
        .filter(|&(_, weight)| weight > 0)
        .collect()
}

/// One random add, exactly: the side in equal shares among the sides that can take a modifier, then
/// the modifier by weight on that side.
///
/// `None` when a total or a scaled weight would overflow `u128`. An empty vector means no modifier
/// can be added at all — a dead draw, which a caller of one add must treat as a refusal and a caller
/// in the middle of [`adds`] must treat as a no-op.
pub fn one_add(data: &Data, item: &Item) -> Option<Vec<Weighted>> {
    let sides: Vec<(Side, Vec<(u16, u128)>)> = Side::BOTH
        .into_iter()
        .map(|side| (side, draw_weights(data, item, side)))
        .filter(|(_, pool)| !pool.is_empty())
        .collect();
    let totals: Vec<u128> = sides
        .iter()
        .map(|(_, pool)| {
            pool.iter()
                .try_fold(0u128, |sum, &(_, weight)| sum.checked_add(weight))
        })
        .collect::<Option<_>>()?;
    let mut out = Vec::new();
    for (i, (side, pool)) in sides.iter().enumerate() {
        // The product of the *other* sides' totals; 1 when this is the only side, so the single
        // side's weights are not scaled at all.
        let scale = totals
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .try_fold(1u128, |acc, (_, total)| acc.checked_mul(*total))?;
        for &(m, weight) in pool {
            let mut result = item.clone();
            result.affixes_mut(*side).push(Affix::at_range(data, m));
            out.push((result, weight.checked_mul(scale)?, Some((*side, m))));
        }
    }
    Some(out)
}

/// One add that cannot be skipped — the currencies whose "add a modifier" is not inside a coin flip
/// (`currency/augmentation`, `currency/exalted`, `currency/regal`). An item with room but nothing
/// left to draw yields `None`, which the currency refuses with the reason.
pub fn add_required(data: &Data, item: &Item) -> Option<Vec<Weighted>> {
    let drawn = one_add(data, item)?;
    (!drawn.is_empty()).then_some(drawn)
}

/// `n` adds in a row, as exact chances.
///
/// Each draw is normalised against the state it is drawn on: the result of a draw keeps
/// `weight / that state's total`, because the next draw's pool depends on what the draw just added,
/// so different states have different totals. Multiplying raw weights across draws would only be
/// right when every first-draw result left the same second-draw total, which is not true.
///
/// The first draw must happen: an empty result means nothing could be added, and [`crate::currency`]
/// turns that into a refusal. A later draw with nothing left to take leaves the item as it is and
/// the draws after it still run — an alteration on a shield whose free side has no modifier left
/// keeps what the first draw gave it rather than producing nothing.
///
/// The reported modifier is the *first* one added.
pub fn adds(data: &Data, item: &Item, n: usize) -> Option<Vec<Mass>> {
    let mut state: Vec<Mass> = vec![(item.clone(), Fraction::ONE, None)];
    for draw in 0..n {
        let mut next: Vec<Mass> = Vec::new();
        for (item, chance, first) in state {
            let branches = one_add(data, &item)?;
            if branches.is_empty() {
                if draw == 0 {
                    return Some(Vec::new());
                }
                next.push((item, chance, first));
                continue;
            }
            let total = branches
                .iter()
                .try_fold(0u128, |sum, &(_, weight, _)| sum.checked_add(weight))?;
            for (result, weight, added) in branches {
                let share = Fraction::new(weight, total)?;
                next.push((result, chance.checked_mul(share)?, first.or(added)));
            }
        }
        state = next;
    }
    Some(state)
}

/// The exact share of one side's next draw that `want` would take: the accepted modifiers' weights
/// over the side's whole pool.
///
/// This is the number a report wants — "the added prefix is the totem modifier one time in 81" —
/// and it is exact, because the pool's weights are integers and the share is their ratio. `None`
/// when the side has nothing to draw, which is not the same as a zero chance.
pub fn side_chance(
    data: &Data,
    item: &Item,
    side: Side,
    mut want: impl FnMut(u16) -> bool,
) -> Option<Fraction> {
    let pool = draw_weights(data, item, side);
    let total = pool
        .iter()
        .try_fold(0u128, |sum, &(_, weight)| sum.checked_add(weight))?;
    let kept = pool
        .iter()
        .filter(|&&(m, _)| want(m))
        .try_fold(0u128, |sum, &(_, weight)| sum.checked_add(weight))?;
    Fraction::new(kept, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data;
    use crate::item::Influence;
    use crate::roll::Roll;

    const BASE: &str = "Supreme Spiked Shield";
    const SHAPER: Option<Influence> = Some(Influence::Shaper);

    fn base(data: &Data) -> u16 {
        data.base_of(BASE)
    }

    /// A fresh magic shield with no influence.
    fn plain_magic(data: &Data) -> Item {
        Item::of_base(data, base(data), 87, Rarity::Magic)
    }

    /// The same shield with the Shaper influence: the pool every chance in this crate is about.
    fn shaper_magic(data: &Data) -> Item {
        plain_magic(data).with_influence(Influence::Shaper)
    }

    fn total(pool: &[(u16, u128)]) -> u128 {
        pool.iter().map(|&(_, weight)| weight).sum()
    }

    fn ids(pool: &[(u16, u128)]) -> Vec<u16> {
        pool.iter().map(|&(m, _)| m).collect()
    }

    #[test]
    fn a_shaper_magic_shield_rolls_the_whole_pool_on_either_side() {
        let data = data();
        let magic = shaper_magic(data);
        let prefix = draw_weights(data, &magic, Side::Prefix);
        let suffix = draw_weights(data, &magic, Side::Suffix);
        assert_eq!(prefix.len(), 73);
        assert_eq!(suffix.len(), 131);
        assert_eq!(total(&prefix), 64800);
        assert_eq!(total(&suffix), 96475);
        // A normal item has no room, so neither side has a pool.
        let normal = Item::of_base(data, base(data), 87, Rarity::Normal);
        assert!(draw_weights(data, &normal, Side::Prefix).is_empty());
        assert!(draw_weights(data, &normal, Side::Suffix).is_empty());
    }

    #[test]
    fn the_shaper_influence_is_the_eleven_modifier_difference() {
        let data = data();
        let plain = plain_magic(data);
        let shaper = shaper_magic(data);
        let plain_prefix = draw_weights(data, &plain, Side::Prefix);
        let plain_suffix = draw_weights(data, &plain, Side::Suffix);
        let shaper_prefix = draw_weights(data, &shaper, Side::Prefix);
        let shaper_suffix = draw_weights(data, &shaper, Side::Suffix);

        // The same base without an influence is the smaller pool.
        assert_eq!(plain_prefix.len(), 72);
        assert_eq!(plain_suffix.len(), 121);
        assert_eq!(total(&plain_prefix), 64000);
        assert_eq!(total(&plain_suffix), 88475);

        // Every modifier that only the influenced pool has weighs nothing without the influence,
        // and there are exactly eleven of them: one prefix and ten suffixes.
        let added: Vec<u16> = ids(&shaper_prefix)
            .into_iter()
            .chain(ids(&shaper_suffix))
            .filter(|&m| data.weight_on(m, base(data), None) == 0)
            .collect();
        assert_eq!(added.len(), 11);
        assert!(
            added
                .iter()
                .all(|&m| data.weight_on(m, base(data), SHAPER) > 0)
        );
        assert!(
            ids(&plain_prefix)
                .iter()
                .chain(&ids(&plain_suffix))
                .all(|&m| { data.weight_on(m, base(data), SHAPER) > 0 }),
            "nothing leaves the pool when the influence is added"
        );
    }

    #[test]
    fn the_totem_prefix_needs_the_shaper_influence() {
        let data = data();
        let totem = data.id_of("MaximumTotemUber1").unwrap();
        assert!(
            draw_weights(data, &plain_magic(data), Side::Prefix)
                .iter()
                .all(|&(m, _)| m != totem)
        );
        assert!(
            draw_weights(data, &shaper_magic(data), Side::Prefix)
                .iter()
                .any(|&(m, _)| m == totem)
        );
    }

    #[test]
    fn item_level_gates_the_pool() {
        let data = data();
        let low =
            Item::of_base(data, base(data), 60, Rarity::Magic).with_influence(Influence::Shaper);
        let prefix = draw_weights(data, &low, Side::Prefix);
        let suffix = draw_weights(data, &low, Side::Suffix);

        assert!(
            prefix
                .iter()
                .chain(&suffix)
                .all(|&(m, _)| data.mod_at(m).level <= 60)
        );
        // Every modifier above level 60 is gone: 53 prefixes over 48000, 76 suffixes over 62600.
        assert_eq!(prefix.len(), 53);
        assert_eq!(total(&prefix), 48000);
        assert_eq!(suffix.len(), 76);
        assert_eq!(total(&suffix), 62600);

        // The high pool is strictly larger on both sides.
        let high = shaper_magic(data);
        assert!(total(&draw_weights(data, &high, Side::Prefix)) > total(&prefix));
        assert!(total(&draw_weights(data, &high, Side::Suffix)) > total(&suffix));
    }

    #[test]
    fn one_add_puts_both_sides_over_one_denominator() {
        let data = data();
        let magic = shaper_magic(data);
        let outcomes = one_add(data, &magic).unwrap();
        // Two sides, so every outcome's weight is scaled by the other side's total: the sum is
        // 64800·96475 twice, and each side contributes its own share of that.
        let prefix_total = 64800u128 * 96475;
        let side_total = |side| -> u128 {
            outcomes
                .iter()
                .filter(|(item, _, _)| item.affixes(side).len() == 1)
                .map(|&(_, weight, _)| weight)
                .sum()
        };
        assert_eq!(side_total(Side::Prefix), prefix_total);
        assert_eq!(side_total(Side::Suffix), 64800u128 * 96475);
        assert_eq!(
            outcomes.iter().map(|&(_, w, _)| w).sum::<u128>(),
            2 * prefix_total
        );
        assert_eq!(outcomes.len(), 204);
        // Every drawn affix starts at its full range and is appended to its side.
        for (item, _, added) in &outcomes {
            let (side, m) = added.unwrap();
            let affix = item.affix(m).unwrap();
            assert_eq!(item.affixes(side).last(), Some(affix));
            let stats = &data.mod_at(m).stats;
            assert_eq!(affix.rolls.len(), stats.len());
            assert!(
                affix
                    .rolls
                    .iter()
                    .zip(stats)
                    .all(|(roll, &(_, min, max))| *roll == Roll::range(min, max).unwrap())
            );
        }
    }

    #[test]
    fn two_adds_are_normalised_against_their_own_state() {
        let data = data();
        let magic = shaper_magic(data);
        let results = adds(data, &magic, 2).unwrap();
        let total = results
            .iter()
            .try_fold(Fraction::ZERO, |sum, &(_, chance, _)| {
                sum.checked_add(chance)
            })
            .unwrap();
        assert_eq!(total, Fraction::ONE, "two adds are a whole chance");
        assert!(results.iter().all(|&(_, _, added)| added.is_some()));
        // Every result holds one modifier per side: a magic item has room for one of each.
        assert!(results.iter().all(|(item, _, _)| {
            item.affixes(Side::Prefix).len() == 1 && item.affixes(Side::Suffix).len() == 1
        }));
        // Every drawn affix holds the mod's stats at their full range.
        assert!(results.iter().all(|(item, _, _)| {
            Side::BOTH.into_iter().all(|side| {
                item.affixes(side).iter().all(|affix| {
                    let stats = &data.mod_at(affix.m).stats;
                    affix.rolls.len() == stats.len()
                        && affix
                            .rolls
                            .iter()
                            .zip(stats)
                            .all(|(roll, &(_, min, max))| *roll == Roll::range(min, max).unwrap())
                })
            })
        }));
    }

    #[test]
    fn a_family_cannot_repeat() {
        let data = data();
        let life = data.id_of("IncreasedLife10").unwrap();
        let lower_life = data.id_of("IncreasedLife9").unwrap();
        assert_eq!(
            data.mod_at(life).groups,
            data.mod_at(lower_life).groups,
            "the two life tiers share a group"
        );
        let mut item =
            Item::of_base(data, base(data), 87, Rarity::Rare).with_influence(Influence::Shaper);
        item.affixes_mut(Side::Prefix)
            .push(Affix::at_range(data, life));
        let pool = draw_weights(data, &item, Side::Prefix);
        assert!(
            !pool.iter().any(|&(m, _)| m == lower_life),
            "a second life roll must be out of the pool"
        );
        assert!(!pool.iter().any(|&(m, _)| m == life));
    }

    #[test]
    fn a_step_with_nothing_to_draw_is_empty_not_a_no_op() {
        let data = data();
        let normal = Item::of_base(data, base(data), 87, Rarity::Normal);
        assert_eq!(adds(data, &normal, 1), Some(Vec::new()));
        assert_eq!(add_required(data, &normal), None);
    }

    #[test]
    fn side_chance_is_the_accepted_weight_over_the_pool() {
        let data = data();
        let magic = shaper_magic(data);
        let totem = data.id_of("MaximumTotemUber1").unwrap();
        assert_eq!(
            side_chance(data, &magic, Side::Prefix, |m| m == totem),
            Fraction::new(800, 64800)
        );
        assert_eq!(
            side_chance(data, &magic, Side::Prefix, |m| m == totem),
            Fraction::new(1, 81),
            "800/64800 reduces to 1/81"
        );
        // No influence, no totem in the pool, so the chance is zero — not absent.
        assert_eq!(
            side_chance(data, &plain_magic(data), Side::Prefix, |m| m == totem),
            Some(Fraction::ZERO)
        );
        // An item with no room has no pool to take a share of.
        assert_eq!(
            side_chance(
                data,
                &Item::of_base(data, base(data), 87, Rarity::Normal),
                Side::Prefix,
                |_| true
            ),
            None
        );
    }
}
