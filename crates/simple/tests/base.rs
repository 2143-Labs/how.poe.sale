//! The data loader on a second, unrelated base: Two-Toned Boots (`BootsAtlas1`) with a Hunter
//! influence.
//!
//! The counts and weight totals are a separate Python count over the raw export, not the
//! generator's own `expect` (which this spec has none of): the generator could agree with itself.

use simple::data::{Data, ModKind};
use simple::item::Influence;

const BOOTS: &str = include_str!("../data/boots.json");

/// Per-side counts and weight totals of the explicit pool, with or without an influence.
fn split(data: &Data, influence: Option<Influence>) -> ((usize, usize), (u128, u128)) {
    let mut counts = (0, 0);
    let mut totals = (0, 0);
    for (m, entry) in data.mods.iter().enumerate() {
        let Some(side) = entry.kind.side() else {
            continue;
        };
        let weight = data.weight_on(m as u16, 0, influence);
        if weight == 0 {
            continue;
        }
        if side.index() == 0 {
            counts.0 += 1;
            totals.0 += weight;
        } else {
            counts.1 += 1;
            totals.1 += weight;
        }
    }
    (counts, totals)
}

#[test]
fn the_boots_file_loads_and_counts_what_the_export_says() {
    let data = Data::from_json(BOOTS).expect("the boots data loads");

    assert_eq!(data.bases.len(), 1);
    let base = data.base_at(0);
    assert_eq!(base.name, "Two-Toned Boots");
    assert_eq!(base.item_class, "Boots");
    assert_eq!(base.drop_level, 70);
    assert_eq!(base.defences, [None, Some((126, 145)), Some((26, 30))]);
    assert_eq!(
        data.mods.len(),
        143,
        "56 prefixes, 86 suffixes, one implicit"
    );
    assert_eq!(
        data.mod_at(base.implicits[0]).kind,
        ModKind::Implicit,
        "the base's implicit"
    );

    // The class's influence tags are recorded on the base; the file's tag table only names the
    // one its pool weighs on, so this also proves the loader interns the rest.
    assert_eq!(base.influence_tags.len(), 6);
    for (influence, tag) in &base.influence_tags {
        assert_eq!(data.influence_tag(*influence), Some(*tag));
    }
    let tag = |influence| data.tag_name(data.influence_tag(influence).unwrap());
    assert_eq!(tag(Influence::Hunter), "boots_basilisk");
    assert_eq!(tag(Influence::Shaper), "boots_shaper");
    assert_eq!(tag(Influence::Warlord), "boots_adjudicator");

    // Hunter adds four prefixes and ten suffixes; the plain pool is the rest.
    assert_eq!(split(&data, None), ((52, 76), (49_000, 56_600)));
    assert_eq!(
        split(&data, Some(Influence::Hunter)),
        ((56, 86), (51_000, 61_700))
    );
}
