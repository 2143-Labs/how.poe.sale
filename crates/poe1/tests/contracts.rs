//! V-A3: every craft's declared contract against its kernel, and the recombine kernel.

mod common;

use common::*;
use poe1::item::{ItemState, Rarity, Side};
use poe1::kernels::{self, Action, RecombineRules};
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
fn magic_rolls_hold_one_or_two_mods_half_each() {
    let b = b();
    let t = amulet(Rarity::Magic);
    let by_count = |outs: &[(ItemState, f64)]| {
        let mut p = [0.0; 3];
        for (o, q) in outs {
            assert_eq!(o.rarity, Rarity::Magic);
            p[o.mod_count() as usize] += q;
        }
        p
    };
    let normal = state(&t, Rarity::Normal, &[]);
    let one = state(&t, Rarity::Magic, &[0]);
    let mut two = one.clone();
    two.suffixes.untracked = 1;
    let from_one = kernels::outcomes(b, &t, &[], &one, &Action::Alteration);
    let from_two = kernels::outcomes(b, &t, &[], &two, &Action::Alteration);
    for outs in [
        kernels::outcomes(b, &t, &[], &normal, &Action::Transmute),
        from_one.clone(),
        from_two.clone(),
    ] {
        let p = by_count(&outs);
        assert_eq!(p[0], 0.0);
        assert!(close(p[1], 0.5, 1e-12), "{p:?}");
        assert!(close(p[2], 0.5, 1e-12), "{p:?}");
    }
    // An Alteration rerolls everything: the old mod count does not matter.
    assert_eq!(from_one.len(), from_two.len());
    for ((o1, p1), (o2, p2)) in from_one.iter().zip(&from_two) {
        assert_eq!(o1, o2);
        assert!(close(*p1, *p2, 1e-12));
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
fn augmentation_adds_one_mod() {
    let b = b();
    let t = amulet(Rarity::Magic);
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
fn recombine_prefixes(
    t: &Target,
    na: usize,
    nb: usize,
    rules: &RecombineRules,
) -> Vec<(ItemState, f64)> {
    let a = state(t, Rarity::Rare, &(0..na).collect::<Vec<_>>());
    let c = state(t, Rarity::Rare, &(na..na + nb).collect::<Vec<_>>());
    kernels::recombine(b(), t, &a, &c, rules)
}

/// Total probability of the outcomes holding exactly these prefix requirements.
fn p_prefixes(outs: &[(ItemState, f64)], want: &[u16]) -> f64 {
    outs.iter()
        .filter(|(o, _)| {
            let reqs: Vec<u16> = o.prefixes.wanted.iter().map(|h| h.req).collect();
            reqs == want
        })
        .map(|(_, p)| p)
        .sum()
}

#[test]
fn recombine_keeps_the_drawn_subset() {
    let t = target(
        "Lapis Amulet",
        84,
        Rarity::Rare,
        distinct_prefixes("Lapis Amulet", 4),
    );
    let weights: Vec<f64> = t
        .requirements
        .iter()
        .map(|r| weight_on("Lapis Amulet", &b().mod_at(r.accepted[0]).id))
        .collect();
    let plan = RecombineRules::plan();
    // Both of two kept: the table's 33/100, whatever the weights.
    let both = p_prefixes(&recombine_prefixes(&t, 1, 1, &plan), &[0, 1]);
    assert!(close(both, 33.0 / 100.0, 1e-12), "both of two = {both}");
    // A specific 3 of 4: P(3 kept) = 31/101, times the chance the draw picks exactly that set.
    let uniform = RecombineRules { alpha: 0.0, ..plan };
    let three = p_prefixes(&recombine_prefixes(&t, 2, 2, &uniform), &[0, 1, 2]);
    assert!(
        close(three, 31.0 / 101.0 / 4.0, 1e-12),
        "uniform specific 3 of 4 = {three}"
    );
    let three = p_prefixes(&recombine_prefixes(&t, 2, 2, &plan), &[0, 1, 2]);
    let want = 31.0 / 101.0 * keep_set_prob(&weights, &[0, 1, 2], 3, plan.alpha);
    assert!(
        close(three, want, 1e-12),
        "specific 3 of 4 = {three}, want {want}"
    );
    // The wiki's rounded table is read only when asked for: its P(3 of 4 kept) is 30/100.
    let alt = RecombineRules::alt_table();
    let three = p_prefixes(&recombine_prefixes(&t, 2, 2, &alt), &[0, 1, 2]);
    let want = 30.0 / 100.0 * keep_set_prob(&weights, &[0, 1, 2], 3, alt.alpha);
    assert!(
        close(three, want, 1e-12),
        "wiki specific 3 of 4 = {three}, want {want}"
    );
}

#[test]
fn recombine_base_and_item_level() {
    let b = b();
    let rules = RecombineRules::plan();
    let t = target("Lapis Amulet", 1, Rarity::Rare, Vec::new());
    let onyx = base("Onyx Amulet");
    for ((x, y), want) in [((84, 86), 86), ((80, 84), 84), ((80, 80), 80)] {
        let mut a = ItemState::new(t.base, x, Rarity::Rare, 0);
        a.suffixes.wanted.clear();
        let c = ItemState::new(onyx, y, Rarity::Rare, 0);
        let outs = kernels::recombine(b, &t, &a, &c, &rules);
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
    // The chosen base carries its influence and its catalyst; the other base's are lost.
    let shaper = ItemState::new(t.base, 84, Rarity::Rare, 1);
    let plain = ItemState::new(t.base, 84, Rarity::Rare, 0);
    let outs = kernels::recombine(b, &t, &shaper, &plain, &rules);
    let p_shaper: f64 = outs
        .iter()
        .filter(|(o, _)| o.influences == 1)
        .map(|(_, p)| p)
        .sum();
    assert!(close(p_shaper, 0.5, 1e-12), "{p_shaper}");
    assert!(outs.iter().all(|(o, _)| o.influences <= 1));
    let mut a = ItemState::new(t.base, 84, Rarity::Rare, 0);
    a.catalyst = Some((0, 20));
    let c = ItemState::new(onyx, 84, Rarity::Rare, 0);
    for (o, _) in kernels::recombine(b, &t, &a, &c, &rules) {
        let want = if o.base == t.base { a.catalyst } else { None };
        assert_eq!(o.catalyst, want);
    }
    // A Shaper-only suffix travels only on the Shaper base: kept 59/100 of the time there, never
    // on the plain base, where it still counts as one pooled mod.
    let t = target(
        "Lapis Amulet",
        84,
        Rarity::Rare,
        vec![req(&["PercentageAllAttributesUberShaper1"])],
    );
    let mut a = ItemState::new(t.base, 84, Rarity::Rare, 1);
    a.suffixes.wanted.push(poe1::item::Hit {
        req: 0,
        idx: 0,
        value_ok: true,
    });
    let c = ItemState::new(t.base, 84, Rarity::Rare, 0);
    let outs = kernels::recombine(b, &t, &a, &c, &rules);
    let holding: f64 = outs
        .iter()
        .filter(|(o, _)| !o.suffixes.wanted.is_empty())
        .map(|(_, p)| p)
        .sum();
    assert!(close(holding, 0.5 * 59.0 / 100.0, 1e-12), "{holding}");
    for (o, _) in &outs {
        if !o.suffixes.wanted.is_empty() {
            assert_eq!(o.influences, 1);
        }
    }
    // Different classes and untracked mods are refused.
    let t = target("Lapis Amulet", 1, Rarity::Rare, Vec::new());
    let wand = ItemState::new(base("Omen Wand"), 84, Rarity::Rare, 0);
    let a = ItemState::new(t.base, 84, Rarity::Rare, 0);
    assert!(kernels::recombine(b, &t, &a, &wand, &rules).is_empty());
    let mut junk = a.clone();
    junk.prefixes.untracked = 1;
    assert!(kernels::recombine(b, &t, &junk, &a, &rules).is_empty());
}

#[test]
fn recombine_non_native_mod_never_transfers() {
    let b = b();
    let t = target(
        "Convoking Wand",
        84,
        Rarity::Rare,
        vec![
            req(&["MinionAttackAndCastSpeed7"]),
            req(&["IncreasedCastSpeed2"]),
        ],
    );
    let hit = |r: u16| poe1::item::Hit {
        req: r,
        idx: 0,
        value_ok: true,
    };
    let mut a = ItemState::new(t.base, 84, Rarity::Rare, 0);
    a.suffixes.wanted.push(hit(0));
    let omen = base("Omen Wand");
    let mut c = ItemState::new(omen, 84, Rarity::Rare, 0);
    c.suffixes.wanted.push(hit(1));
    let outs = kernels::recombine(b, &t, &a, &c, &RecombineRules::plan());
    let on_omen = |f: &dyn Fn(&ItemState) -> bool| -> f64 {
        outs.iter()
            .filter(|(o, _)| o.base == omen && f(o))
            .map(|(_, p)| p)
            .sum()
    };
    // The minion mod counts in the pool of two but never lands on the Omen Wand, so the cast
    // speed mod is all a kept count of 1 or 2 can take there.
    let with_minion = on_omen(&|o| o.suffixes.wanted.iter().any(|h| h.req == 0));
    assert_eq!(with_minion, 0.0);
    let only_cast = on_omen(&|o| o.suffixes.wanted.len() == 1 && o.suffixes.wanted[0].req == 1);
    assert!(close(only_cast, 0.5, 1e-12), "{only_cast}");
}

#[test]
fn recombine_output_rarity_follows_kept_mods() {
    let b = b();
    let rules = RecombineRules::plan();
    let t = target(
        "Lapis Amulet",
        84,
        Rarity::Rare,
        distinct_prefixes("Lapis Amulet", 2),
    );
    let by_rarity = |outs: Vec<(ItemState, f64)>| {
        let mut p = [0.0; 3];
        for (o, q) in outs {
            let n = o.prefixes.wanted.len();
            let i = match o.rarity {
                Rarity::Normal => 0,
                Rarity::Magic => 1,
                Rarity::Rare => 2,
            };
            assert_eq!(
                (o.rarity, n),
                ([Rarity::Normal, Rarity::Magic, Rarity::Rare][n.min(2)], n)
            );
            p[i] += q;
        }
        p
    };
    // Two rares: one prefix kept is a magic item, both a rare.
    let a = state(&t, Rarity::Rare, &[0]);
    let c = state(&t, Rarity::Rare, &[1]);
    let p = by_rarity(kernels::recombine(b, &t, &a, &c, &rules));
    assert_eq!(p[0], 0.0);
    assert!(close(p[1], 67.0 / 100.0, 1e-12), "{p:?}");
    assert!(close(p[2], 33.0 / 100.0, 1e-12), "{p:?}");
    // A magic prefix and an empty magic item: nothing kept is a normal item.
    let a = state(&t, Rarity::Magic, &[0]);
    let c = state(&t, Rarity::Magic, &[]);
    let p = by_rarity(kernels::recombine(b, &t, &a, &c, &rules));
    assert!(close(p[0], 41.0 / 100.0, 1e-12), "{p:?}");
    assert!(close(p[1], 59.0 / 100.0, 1e-12), "{p:?}");
    assert_eq!(p[2], 0.0);
}
