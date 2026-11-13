//! Memory Strands: one strand or more cuts the tiers a random draw can reach.
//!
//! Claim `memory-strands.rollable-tiers`. On an item holding `s >= 1` strands, a random draw sees
//! only the highest tiers of a modifier's tier table: of the `T` tiers that pass the draw's level
//! gate it keeps the `K` best, with
//!
//! `K = floor(100 (T - 1) / (2.5 s + 147.5)) + 1`
//!
//! computed in integers, because at eight tiers 81 strands is exactly 2.0 before the floor: 81
//! strands keep 3 tiers and 82 keep 2. The tiers are the distinct required levels of the table,
//! counted from the highest, so two modifiers that tie on their level share a tier.
//!
//! The Foulborn orb's +75 to modifier tier rating is left out: no Foulborn orb is in the model, so
//! a Foulborn draw is not a state this code can be in. A draw on an item with no strands is not cut
//! at all, and neither is a fixed modifier (an essence's guaranteed mod, a crafted or a fractured
//! modifier, a bench craft): only a random draw is.

/// Of a tier table's `T` rollable tiers, how many a strand count keeps.
pub fn rollable_tiers(t: usize, strands: u8) -> usize {
    if strands == 0 || t <= 1 {
        return t;
    }
    // `200 (T - 1) / (295 + 5 s) + 1`, the integer form of the claim's formula.
    let k = 200 * (t as u32 - 1) / (295 + 5 * u32::from(strands)) + 1;
    (k as usize).min(t)
}

/// The lowest required level a strand count still rolls, given one tier table's levels. `None` for
/// an empty table.
pub fn kept_floor(levels: impl IntoIterator<Item = u8>, strands: u8) -> Option<u8> {
    let mut levels: Vec<u8> = levels.into_iter().collect();
    levels.sort_unstable_by(|a, b| b.cmp(a));
    levels.dedup();
    if levels.is_empty() {
        return None;
    }
    Some(levels[rollable_tiers(levels.len(), strands) - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boundary_is_exact_integers() {
        // Eight tiers at 81 strands is exactly 2.0 before the floor, so 81 keeps three and 82 two.
        assert_eq!(rollable_tiers(8, 81), 3);
        assert_eq!(rollable_tiers(8, 82), 2);
    }

    #[test]
    fn no_strands_is_no_cut() {
        for t in 0..=40 {
            assert_eq!(rollable_tiers(t, 0), t, "{t} tiers");
        }
    }

    #[test]
    fn more_strands_never_keep_more_tiers() {
        for t in 1..=40 {
            let mut last = t;
            for s in 0..=100u8 {
                let k = rollable_tiers(t, s);
                assert!(k <= last, "{t} tiers at {s} strands: {k} > {last}");
                assert!(k >= 1 && k <= t, "{t} tiers at {s} strands: {k}");
                last = k;
            }
        }
    }

    #[test]
    fn the_floor_is_the_lowest_kept_level() {
        // Levels eight down to one, so the top three are 8, 7, 6.
        assert_eq!(kept_floor(1..=8, 81), Some(6));
        assert_eq!(kept_floor(1..=8, 82), Some(7));
        assert_eq!(kept_floor(1..=8, 0), Some(1));
        // Ties on a level share a tier: 8, 8, 7 is two tiers.
        assert_eq!(kept_floor([8, 8, 7], 100), Some(8));
        assert_eq!(kept_floor([], 10), None);
    }
}
