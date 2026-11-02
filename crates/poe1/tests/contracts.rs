//! V-A3: every craft's declared contract against its kernel, and the recombine kernel.

mod common;

use common::*;
use poe1::item::{ItemState, Rarity, Side};
use poe1::kernels::{self, Action};
use poe1::registry::{self, Effect, SideEffect};
use poe1::target::Target;

fn amulet(rarity: Rarity) -> Target {
    target(
        "Lapis Amulet",
        84,
        rarity,
        vec![
            req(&["GlobalLightningGemLevel1"]),
            req(&["GlobalSkillGemLevel1"]),
        ],
    )
}

fn single_actions() -> Vec<Action> {
    vec![
        Action::Chaos,
        Action::Exalted,
        Action::Annul,
        Action::Divine,
        Action::Transmute,
        Action::Alteration,
        Action::Augmentation,
        Action::Regal,
        Action::Scour,
        Action::Imprint,
        Action::Restore,
    ]
}

/// One outcome against the declared effect.
fn obeys(e: &Effect, before: &ItemState, after: &ItemState) -> Result<(), String> {
    if let Some(r) = e.rarity
        && after.rarity != r
    {
        return Err(format!("rarity {:?}, declared {:?}", after.rarity, r));
    }
    let mut added = 0i32;
    let mut removed = 0i32;
    let mut add_decl = None;
    let mut rem_decl = None;
    for (side, se) in [(Side::Prefix, e.prefixes), (Side::Suffix, e.suffixes)] {
        let (b, a) = (before.side(side), after.side(side));
        let locked = registry::side_locked(before, side);
        match se {
            SideEffect::Keep => {
                if a != b {
                    return Err(format!("{side:?} changed under Keep"));
                }
            }
            SideEffect::Clear => {
                if locked {
                    if a != b {
                        return Err(format!("locked {side:?} changed under Clear"));
                    }
                } else if a.slots_used() != 0 {
                    return Err(format!("{side:?} not cleared"));
                }
            }
            SideEffect::Reroll(n) => {
                // A locked side keeps every mod it had; a reforge may still fill its free slots.
                if locked
                    && (!b.wanted.iter().all(|h| a.wanted.contains(h))
                        || !b.blocked.iter().all(|x| a.blocked.contains(x))
                        || a.untracked < b.untracked)
                {
                    return Err(format!("locked {side:?} lost a mod"));
                }
                if a.slots_used() > n.max(b.slots_used()) {
                    return Err(format!("{side:?} holds {} > {n}", a.slots_used()));
                }
            }
            SideEffect::Add(n) => {
                added += a.slots_used() as i32 - b.slots_used() as i32;
                add_decl = Some(n as i32);
            }
            SideEffect::Remove(n) => {
                removed += b.slots_used() as i32 - a.slots_used() as i32;
                rem_decl = Some(n as i32);
            }
        }
    }
    if let Some(n) = add_decl
        && added != n
    {
        return Err(format!("added {added}, declared {n}"));
    }
    if let Some(n) = rem_decl {
        let lock_gone = i32::from(before.lock != 0 && after.lock == 0);
        if removed + lock_gone != n {
            return Err(format!("removed {removed}+lock {lock_gone}, declared {n}"));
        }
    }
    Ok(())
}

fn fixtures() -> Vec<(Target, ItemState)> {
    let rare = amulet(Rarity::Rare);
    let magic = amulet(Rarity::Magic);
    let normal = amulet(Rarity::Normal);
    let mut locked = state(&rare, Rarity::Rare, &[0]);
    locked.suffixes.untracked = 2;
    locked.lock = registry::lock_bit_for(Side::Prefix);
    let mut imprinted = state(&magic, Rarity::Magic, &[0]);
    imprinted.imprint = Some(Box::new((
        imprinted.prefixes.clone(),
        imprinted.suffixes.clone(),
    )));
    imprinted.prefixes = Default::default();
    imprinted.suffixes.untracked = 1;
    vec![
        (rare.clone(), state(&rare, Rarity::Rare, &[0])),
        (rare.clone(), locked),
        (magic.clone(), state(&magic, Rarity::Magic, &[0])),
        (magic.clone(), imprinted),
        (normal.clone(), state(&normal, Rarity::Normal, &[])),
    ]
}

#[test]
fn kernels_obey_their_contracts() {
    let b = b();
    let mut checked = 0;
    for (t, s) in fixtures() {
        for a in single_actions() {
            let c = registry::contract(&a);
            let ok = registry::applicable(b, &t, &s, &a, &[]);
            if let Some(r) = c.requires.rarity
                && s.rarity != r
            {
                assert!(!ok, "{} applies to a {:?} item", c.id, s.rarity);
            }
            if c.requires.lock_free && s.lock != 0 {
                assert!(!ok, "{} applies to a locked item", c.id);
            }
            if !ok {
                continue;
            }
            let outs = kernels::outcomes(b, &t, &[], &s, &a);
            assert!(!outs.is_empty(), "{} has no outcomes", c.id);
            let total: f64 = outs.iter().map(|(_, p)| p).sum();
            assert!((total - 1.0).abs() < 1e-9, "{} sums to {total}", c.id);
            for (o, _) in &outs {
                if let Err(e) = obeys(&c.effect, &s, o) {
                    panic!("{} on {:?}: {e}\n{o:?}", c.id, s.rarity);
                }
            }
            checked += 1;
        }
    }
    assert!(checked >= 15, "only {checked} (craft, item) pairs applied");
}

#[test]
fn scour_keeps_a_locked_side_and_clears_the_rest() {
    let b = b();
    let t = amulet(Rarity::Rare);
    let mut s = state(&t, Rarity::Rare, &[0, 1]);
    s.suffixes.untracked = 1;
    s.lock = registry::lock_bit_for(Side::Prefix);
    let outs = kernels::outcomes(b, &t, &[], &s, &Action::Scour);
    assert_eq!(outs.len(), 1);
    let o = &outs[0].0;
    assert_eq!(o.prefixes, s.prefixes);
    assert_eq!(o.suffixes.slots_used(), 0);
    assert_eq!(o.lock, 0);
}

#[test]
fn alteration_keeps_the_mod_count() {
    let b = b();
    let t = amulet(Rarity::Magic);
    let one = state(&t, Rarity::Magic, &[0]);
    for (o, _) in kernels::outcomes(b, &t, &[], &one, &Action::Alteration) {
        assert_eq!(o.mod_count(), 1);
        assert_eq!(o.rarity, Rarity::Magic);
    }
    let mut two = one.clone();
    two.suffixes.untracked = 1;
    for (o, _) in kernels::outcomes(b, &t, &[], &two, &Action::Alteration) {
        assert_eq!(o.mod_count(), 2);
    }
}

#[test]
fn regal_adds_exactly_one_mod() {
    let b = b();
    let t = amulet(Rarity::Magic);
    let s = state(&t, Rarity::Magic, &[0]);
    let outs = kernels::outcomes(b, &t, &[], &s, &Action::Regal);
    assert!(!outs.is_empty());
    for (o, _) in outs {
        assert_eq!(o.rarity, Rarity::Rare);
        assert_eq!(o.mod_count(), 2);
    }
}

#[test]
fn transmute_and_augmentation_add_one_mod() {
    let b = b();
    let t = amulet(Rarity::Magic);
    let normal = state(&t, Rarity::Normal, &[]);
    for (o, _) in kernels::outcomes(b, &t, &[], &normal, &Action::Transmute) {
        assert_eq!((o.rarity, o.mod_count()), (Rarity::Magic, 1));
    }
    let magic = state(&t, Rarity::Magic, &[0]);
    assert!(registry::applicable(
        b,
        &t,
        &magic,
        &Action::Augmentation,
        &[]
    ));
    for (o, _) in kernels::outcomes(b, &t, &[], &magic, &Action::Augmentation) {
        assert_eq!(o.mod_count(), 2);
        assert_eq!(o.prefixes, magic.prefixes);
    }
}

#[test]
fn restore_returns_the_imprinted_item() {
    let b = b();
    let t = amulet(Rarity::Magic);
    let s = state(&t, Rarity::Magic, &[0]);
    let imp = kernels::outcomes(b, &t, &[], &s, &Action::Imprint);
    assert_eq!(imp.len(), 1);
    let mut later = imp[0].0.clone();
    assert!(later.imprint.is_some());
    later.prefixes = Default::default();
    later.suffixes.untracked = 1;
    assert!(registry::applicable(b, &t, &later, &Action::Restore, &[]));
    let back = kernels::outcomes(b, &t, &[], &later, &Action::Restore);
    assert_eq!(back.len(), 1);
    assert_eq!(
        back[0].0, s,
        "restore must give back the imprinted item and spend the imprint"
    );
}

#[test]
fn every_cost_key_is_priced_or_typed() {
    let b = b();
    let prices = prices();
    let typed = ["craicic-croaker", "recombiner"];
    let mut actions = single_actions();
    actions.push(Action::Essence { essence: 0 });
    for a in actions {
        for (key, _) in registry::cost(b, &a, &[]) {
            assert!(
                prices.contains_key(&key) || typed.contains(&key.as_str()),
                "{} costs unknown price key {key}",
                registry::contract(&a).id
            );
        }
    }
    assert!(registry::contract_by_id("recombine").is_some());
}

/// Recombine two rares holding `na` and `nb` of the distinct prefixes of `t`.
fn recombine_prefixes(t: &Target, na: usize, nb: usize) -> Vec<(ItemState, f64)> {
    let a = state(t, Rarity::Rare, &(0..na).collect::<Vec<_>>());
    let c = state(t, Rarity::Rare, &(na..na + nb).collect::<Vec<_>>());
    kernels::recombine(b(), t, &a, &c)
}

#[test]
fn recombine_count_table_rows() {
    let t = target(
        "Lapis Amulet",
        84,
        Rarity::Rare,
        distinct_prefixes("Lapis Amulet", 6),
    );
    let rows: [&[f64]; 6] = [
        &[0.41, 0.59],
        &[0.0, 2.0 / 3.0, 1.0 / 3.0],
        &[0.0, 0.40, 0.50, 0.10],
        &[0.0, 0.10, 0.60, 0.30],
        &[0.0, 0.0, 0.43, 0.57],
        &[0.0, 0.0, 0.30, 0.70],
    ];
    for (n, row) in (1..=6usize).zip(rows) {
        let na = n.div_ceil(2);
        let outs = recombine_prefixes(&t, na, n - na);
        let mut by_count = [0.0; 4];
        for (o, p) in &outs {
            by_count[o.prefixes.wanted.len()] += p;
        }
        for (k, want) in row.iter().enumerate() {
            assert!(
                (by_count[k] - want).abs() < 1e-12,
                "pool {n}: P({k} kept) = {}, table {want}",
                by_count[k]
            );
        }
    }
    // Specific subsets: both of two kept 1/3; a specific 3 of 4 = 0.30 * 1/4.
    let both = recombine_prefixes(&t, 1, 1)
        .iter()
        .filter(|(o, _)| o.prefixes.wanted.len() == 2)
        .map(|(_, p)| p)
        .sum::<f64>();
    assert!((both - 1.0 / 3.0).abs() < 1e-12);
    let three = recombine_prefixes(&t, 2, 2)
        .iter()
        .filter(|(o, _)| {
            let reqs: Vec<u16> = o.prefixes.wanted.iter().map(|h| h.req).collect();
            reqs == [0, 1, 2]
        })
        .map(|(_, p)| p)
        .sum::<f64>();
    assert!((three - 0.075).abs() < 1e-12, "specific 3 of 4 = {three}");
}

#[test]
fn recombine_base_and_item_level() {
    let b = b();
    let t = target("Lapis Amulet", 1, Rarity::Rare, Vec::new());
    let onyx = base("Onyx Amulet");
    for ((x, y), want) in [((84, 86), 86), ((80, 84), 84), ((80, 80), 80)] {
        let mut a = ItemState::new(t.base, x, Rarity::Rare, 0);
        a.suffixes.wanted.clear();
        let c = ItemState::new(onyx, y, Rarity::Rare, 0);
        let outs = kernels::recombine(b, &t, &a, &c);
        let mut on_a = 0.0;
        for (o, p) in &outs {
            assert_eq!(o.ilvl, want, "ilvl({x},{y})");
            assert_eq!(o.influences, 0);
            if o.base == t.base {
                on_a += p;
            }
        }
        assert!((on_a - 0.5).abs() < 1e-12);
    }
    // Different classes and untracked mods are refused.
    let wand = ItemState::new(base("Omen Wand"), 84, Rarity::Rare, 0);
    let a = ItemState::new(t.base, 84, Rarity::Rare, 0);
    assert!(kernels::recombine(b, &t, &a, &wand).is_empty());
    let mut junk = a.clone();
    junk.prefixes.untracked = 1;
    assert!(kernels::recombine(b, &t, &junk, &a).is_empty());
}
