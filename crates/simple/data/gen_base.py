#!/usr/bin/env python3
"""Generate one base's data file from the pinned Path of Exile export.

Usage, from the repository root:

    python3 crates/simple/data/gen_base.py <raw-dir> <spec.json> > <out.json>

The spec names the base, the item level the pool is read at, and any influences the pool
should include:

    {
      "base": "Metadata/Items/Armours/Shields/ShieldDexInt13",
      "itemLevel": 87,
      "influences": ["shaper"],
      "essenceMods": [...optional: essence-only modifier ids an essence can place...],
      "target": {"prefixes": ["desired"], "suffixes": [], "desired": [...], "good": [...]},
      "expect": {...optional pins the generator checks and fails on...}
    }

There is exactly one base in the output. Its influence is not a property of the base but of
the item: the Shaper version of a base is that base with an influence on the item, and an
influence contributes one extra tag (`shield_shaper` for a Shaper shield). The pool rule is:
a modifier is in the file when

  * it is a prefix or a suffix,
  * its `domain` equals the base's `domain`,
  * its required level is at most the spec's `itemLevel`,
  * its spawn weight on the item is positive (the base's tags plus, for each influence the
    spec lists, that influence's tag),
  * its id does not contain `Royale` (claim `pool.royale-excluded`: those four item-domain
    mods are Battle Royale copies of real tiers that a league item cannot roll; keeping them
    would put two of them in the shield's suffix pool, making it 133 modifiers over 97 975
    instead of 131 over 96 475),
  * it has at least one stat.

A modifier's spawn weights are an ordered list, and the first entry whose tag the item
carries decides: its weight is the modifier's weight, and a zero there means the modifier
cannot roll, whatever a later entry says. (RePoE documents this in `RePoE/docs/mods.md`:
"The array elements are evaluated in order, the first element with a tag the item in
question has is taken.") Summing the positive weights instead lets in modifiers the game
never rolls on the base. The file keeps each modifier's entries on the item's tags in the
export's order, zeros included, so the crate applies the same rule.

Influence tags are a property of the item class: the class tag `t` for which `t_shaper` is a
spawn-weight tag somewhere in the export also names the other five, `t_elder`,
`t_crusader`, `t_eyrie` (Redeemer), `t_basilisk` (Hunter) and `t_adjudicator` (Warlord). The
base object records all six in `influenceTags` when the class has them and `{}` otherwise. A
base with no such tag (a flask, a jewel) cannot be influenced, so listing an influence for it
is an error. Only the influence tags the spec lists go into the tag table and the pool; the
rest are interned by the crate when the file is read.

The implicit modifiers are whatever the base lists, in the base's order, each with its own
stats; nothing assumes a generation type. A base without `requirements` or without defence
properties in the export gets zeros and nulls.

An essence-only modifier has a spawn weight of zero everywhere, so the pool rule leaves it out;
an essence places it instead of drawing it. Each id the spec lists in `essenceMods` is written
after the explicit pool, with its zero weights on the item's tags, so the crate knows it and
never draws it. It must be an essence-only prefix or suffix of the base's domain whose weight
on the item is zero; it does not count toward any `expect` pin.

The `expect` object pins what the generator must see; every key is optional and a mismatch is
an error, not a correction:

    "mods"          the explicit pool's size
    "counts"        {"prefix": N, "suffix": N}
    "weights"       {"prefix": N, "suffix": N}
    "plainMods"     the pool's size with no influence
    "plainCounts"   per-side counts with no influence
    "plainWeights"  per-side weight totals with no influence
    "influenceOnly" the sorted ids in the pool that are not in the plain pool
    "implicits"     [{"id": ..., "stats": [{"id": ..., "min": ..., "max": ...}]}]

The shield's spec pins, with Shaper influence, 73 prefixes over 64 800 and 131 suffixes over
96 475; with no influence, 72 prefixes over 64 000 and 121 suffixes over 88 475. The
difference is the 11 modifiers whose first matching tag is `shield_shaper`.
"""

import json
import sys

# The six influences, in the game's order, and the export's suffix for each class tag.
INFLUENCES = [
    ("shaper", "_shaper"),
    ("elder", "_elder"),
    ("crusader", "_crusader"),
    ("redeemer", "_eyrie"),
    ("hunter", "_basilisk"),
    ("warlord", "_adjudicator"),
]


def fail(msg):
    sys.exit("gen_base: " + msg)


def weights_on(mod, tags):
    """The mod's spawn weights on `tags`, in the export's order, zeros included."""
    return [
        [w["tag"], w["weight"]]
        for w in mod.get("spawn_weights", [])
        if w["tag"] in tags
    ]


def spawn_weight(mod, tags):
    """The weight of the first spawn-weight entry whose tag is in `tags`; 0 when none is."""
    for tag, weight in weights_on(mod, tags):
        return weight
    return 0


def influence_map(base_tags, weight_tags):
    """The base class's six influence tags, or `{}` when the class cannot be influenced."""
    roots = [tag for tag in base_tags if tag + "_shaper" in weight_tags]
    if not roots:
        return {}
    if len(roots) > 1:
        fail(f"the base tags name more than one influence class: {roots}")
    root = roots[0]
    tags = {}
    for influence, suffix in INFLUENCES:
        tag = root + suffix
        if tag not in weight_tags:
            fail(f"the class tag {root!r} has {tag!r} for {influence} nowhere in the export")
        tags[influence] = tag
    return tags


def defence(prop):
    return None if prop is None else [prop["min"], prop["max"]]


def mod_json(key, mod, kind, weights):
    stats = mod.get("stats", [])
    if not stats:
        fail(f"{key} has no stats")
    return {
        "id": key,
        "name": mod["name"],
        "kind": kind,
        "level": mod["required_level"],
        "groups": mod["groups"],
        "weights": weights,
        # Keep the export's stat order: a held affix's rolls are stored in it.
        "stats": [
            {"id": s["id"], "min": s["min"], "max": s["max"]} for s in stats
        ],
        "implicitTags": mod.get("implicit_tags", []),
        "addsTags": mod.get("adds_tags", []),
        "essenceOnly": mod["is_essence_only"],
        "text": mod["text"],
    }


def pool(entries, tags):
    counts = {"prefix": 0, "suffix": 0}
    totals = {"prefix": 0, "suffix": 0}
    for _, mod, _ in entries:
        side = mod["generation_type"]
        counts[side] += 1
        totals[side] += spawn_weight(mod, tags)
    return counts, totals


def check(expect, key, got):
    if key in expect and expect[key] != got:
        fail(f"{key} is {got}, expected {expect[key]}")


def main(argv):
    if len(argv) != 3:
        fail("usage: gen_base.py <raw-dir> <spec.json>")
    raw = argv[1]
    spec = json.load(open(argv[2]))

    base_id = spec["base"]
    item_level = spec["itemLevel"]
    influences = spec.get("influences", [])
    essence_ids = spec.get("essenceMods", [])
    target = spec["target"]
    expect = spec.get("expect", {})

    bases = json.load(open(f"{raw}/base_items.json"))
    if base_id not in bases:
        fail(f"{raw}/base_items.json has no {base_id}")
    base = bases[base_id]
    mods = json.load(open(f"{raw}/mods.json"))

    weight_tags = {
        w["tag"] for mod in mods.values() for w in mod.get("spawn_weights", [])
    }
    base_tags = list(base["tags"])
    influence_tags = influence_map(base_tags, weight_tags)
    for influence in influences:
        if influence not in influence_tags:
            fail(f"{base_id} has no {influence} influence tag")
    pool_tags = base_tags + [influence_tags[i] for i in influences]

    explicit = []
    for key, mod in mods.items():
        # Claim `pool.royale-excluded`: the four item-domain `…Royale` mods are Battle Royale
        # copies a league item cannot roll, so keeping them would inflate every pool they touch.
        if "Royale" in key:
            continue
        if mod["domain"] != base["domain"]:
            continue
        if mod["generation_type"] not in ("prefix", "suffix"):
            continue
        if mod["required_level"] > item_level:
            continue
        if spawn_weight(mod, pool_tags) == 0:
            continue
        explicit.append((key, mod, weights_on(mod, pool_tags)))
    explicit.sort(
        key=lambda e: (e[1]["generation_type"] != "prefix", e[1]["required_level"], e[0])
    )

    for key, mod, _ in explicit:
        if not mod.get("stats"):
            fail(f"{key} has no stats")

    counts, totals = pool(explicit, pool_tags)
    check(expect, "mods", len(explicit))
    check(expect, "counts", counts)
    check(expect, "weights", totals)

    plain = [entry for entry in explicit if spawn_weight(entry[1], base_tags) > 0]
    plain_counts, plain_totals = pool(plain, base_tags)
    check(expect, "plainMods", len(plain))
    check(expect, "plainCounts", plain_counts)
    check(expect, "plainWeights", plain_totals)
    plain_ids = {key for key, _, _ in plain}
    influence_only = sorted(key for key, _, _ in explicit if key not in plain_ids)
    check(expect, "influenceOnly", influence_only)

    implicit_entries = []
    for implicit in base.get("implicits") or []:
        if implicit not in mods:
            fail(f"{implicit} is not in the export")
        implicit_entries.append(mod_json(implicit, mods[implicit], "implicit", []))

    essence = []
    for key in essence_ids:
        if key not in mods:
            fail(f"{key} is not in the export")
        mod = mods[key]
        if not mod["is_essence_only"]:
            fail(f"{key} is not an essence-only modifier")
        if mod["domain"] != base["domain"]:
            fail(f"{key} is not of the base's domain")
        if mod["generation_type"] not in ("prefix", "suffix"):
            fail(f"{key} is neither a prefix nor a suffix")
        if spawn_weight(mod, pool_tags) != 0:
            fail(f"{key} can roll on the item, so it is already in the pool")
        essence.append((key, mod, weights_on(mod, pool_tags)))
    explicit_ids = {key for key, _, _ in explicit}
    for entry in implicit_entries:
        if entry["id"] in explicit_ids:
            fail(f"{entry['id']} is both a base implicit and an explicit modifier")
    got_implicits = {entry["id"]: entry for entry in implicit_entries}
    pinned = expect.get("implicits")
    if pinned is not None:
        got = sorted(got_implicits)
        want = sorted(entry["id"] for entry in pinned)
        if got != want:
            fail(f"the base's implicits are {got}, expected {want}")
        for entry in pinned:
            stats = got_implicits[entry["id"]]["stats"]
            if stats != entry["stats"]:
                fail(
                    f"{entry['id']} has stats {stats}, expected {entry['stats']}"
                )

    # The goal names modifiers of the pool; a name that is not there cannot be a grade.
    ids = explicit_ids | {key for key, _, _ in essence}
    for group in ("desired", "good"):
        for key in target[group]:
            if key not in ids:
                fail(f"{key} is not in the base's pool")

    # The global tag table: the base's tags first, then the influences the spec lists (the
    # crate interns any other influence tag when it reads the file), then anything a weight,
    # implicit tag or added tag names. Every name is interned once, in first-seen order.
    tag_names = []
    tag_index = {}

    def intern_tag(name):
        if name not in tag_index:
            tag_index[name] = len(tag_names)
            tag_names.append(name)

    for tag in base_tags:
        intern_tag(tag)
    for tag in pool_tags[len(base_tags):]:
        intern_tag(tag)
    for _, mod, weights in explicit + essence:
        for tag, _ in weights:
            intern_tag(tag)
        for tag in mod.get("implicit_tags", []) + mod.get("adds_tags", []):
            intern_tag(tag)
    for implicit in base.get("implicits") or []:
        mod = mods[implicit]
        for tag in mod.get("implicit_tags", []) + mod.get("adds_tags", []):
            intern_tag(tag)

    properties = base.get("properties") or {}
    requirements = base.get("requirements") or {
        "level": 0,
        "strength": 0,
        "dexterity": 0,
        "intelligence": 0,
    }

    json.dump(
        {
            "tags": tag_names,
            "bases": [
                {
                    "id": base_id,
                    "name": base["name"],
                    "itemClass": base["item_class"],
                    "dropLevel": base["drop_level"],
                    "tags": base_tags,
                    "influenceTags": influence_tags,
                    "requirements": {
                        "level": requirements["level"],
                        "strength": requirements["strength"],
                        "dexterity": requirements["dexterity"],
                        "intelligence": requirements["intelligence"],
                    },
                    "defences": {
                        "armour": defence(properties.get("armour")),
                        "evasion": defence(properties.get("evasion")),
                        "energyShield": defence(properties.get("energy_shield")),
                        "ward": defence(properties.get("ward")),
                    },
                    "implicits": list(base.get("implicits") or []),
                }
            ],
            "mods": [
                mod_json(key, mod, mod["generation_type"], weights)
                for key, mod, weights in explicit + essence
            ]
            + implicit_entries,
            "target": {
                "prefixes": target["prefixes"],
                "suffixes": target["suffixes"],
                "desired": target["desired"],
                "good": target["good"],
            },
        },
        sys.stdout,
        indent=1,
        sort_keys=False,
    )
    sys.stdout.write("\n")


if __name__ == "__main__":
    main(sys.argv)
