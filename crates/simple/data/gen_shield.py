#!/usr/bin/env python3
"""Generate crates/simple/data/shield.json from the pinned Path of Exile export.

Usage, from the repository root:

    python3 crates/simple/data/gen_shield.py games/simple/raw > crates/simple/data/shield.json

The raw copy on `main` holds only the toy bases; the Shaper Supreme Spiked Shield and its
204 modifiers were added to `games/simple/raw` on the `v3-item-system` branch, which is
where this script was run from. The generated file is self-contained, so the crate never
needs the export at build time.

There is exactly one base in the file. Its influence is not a property of the base: the
Shaper version of the shield is the same base with an influence on the item, and an
influence contributes one extra tag (`shield_shaper` for the Shaper). So the pool rule is:
a modifier can roll on the shield when it is a prefix or a suffix, its required level is at
most the base's item level, and its spawn weight on the item is positive.

A modifier's spawn weights are an ordered list, and the first entry whose tag the item
carries decides: its weight is the modifier's weight, and a zero there means the modifier
cannot roll, whatever a later entry says. (RePoE documents this in `RePoE/docs/mods.md`:
"The array elements are evaluated in order, the first element with a tag the item in
question has is taken.") Summing the positive weights instead lets in 19 modifiers the
game never rolls on this base, such as the suppression family, whose `shield: 0` comes
before `dex_int_armour: 500`. The file keeps each modifier's entries on the item's tags
in the export's order, zeros included, so the crate applies the same rule.

The generator pins both pools: with Shaper influence, 73 prefixes over 64 800 and 131
suffixes over 96 475; with no influence, 72 prefixes over 64 000 and 121 suffixes over
88 475. The difference is the 11 modifiers whose first matching tag is `shield_shaper`.
The tag table still carries `shield_shaper`, because an item's influence finds its tag by
name.

The goal the file states is the totem prefix and nothing else: any rare shield that holds it
is a valid finish, and every other modifier on the item is a bonus. The `desired` and `good`
id lists say which modifiers count as which grade; the `prefixes` and `suffixes` lists are the
grade floors, best first, and a side with no floors is free.
"""

import json
import sys

ITEM_LEVEL = 87
BASE_ID = "Metadata/Items/Armours/Shields/ShieldDexInt13"
SHAPER_TAG = "shield_shaper"
BASE_TAGS = [
    "dex_int_armour",
    "dex_int_shield",
    "top_tier_base_item_type",
    "shield",
    "armour",
    "default",
]
EXPECT_COUNTS = {"prefix": 73, "suffix": 131}
EXPECT_WEIGHTS = {"prefix": 64800, "suffix": 96475}
EXPECT_PLAIN_COUNTS = {"prefix": 72, "suffix": 121}
EXPECT_PLAIN_WEIGHTS = {"prefix": 64000, "suffix": 88475}
SHAPER_ONLY = [
    "ChanceToChillAttackersOnBlockUber1",
    "ChanceToChillAttackersOnBlockUber2",
    "ChanceToShockAttackersOnBlockUber1",
    "ChanceToShockAttackersOnBlockUber2",
    "DisplaySocketedGemsGetReducedReservationUber1",
    "DisplaySocketedGemsGetReducedReservationUber2",
    "GainRandomChargeOnBlockUber1__",
    "MaximumTotemUber1",
    "RecoverEnergyShieldPercentOnBlockUber1",
    "RecoverLifePercentOnBlockUber1_",
    "RecoverManaPercentOnBlockUber1",
]
IMPLICIT = "ChanceToDodgeSpellsImplicitShield2"
IMPLICIT_STATS = [{"id": "base_spell_suppression_chance_%", "min": 5, "max": 5}]
DESIRED = ["MaximumTotemUber1"]
GOOD = [
    "IncreasedLife10",
    "IncreasedLife9",
    "IncreasedMana12",
    "LocalIncreasedBlockPercentage7",
    "LocalIncreasedBlockPercentage6",
    "AllResistances6",
    "AllResistances5",
    "FireResist8",
    "FireResist7",
    "ColdResist8",
    "ColdResist7",
    "LightningResist8",
    "LightningResist7",
    "SpellCriticalStrikeChance6_",
    "SpellCriticalStrikeChance5",
    "RecoverEnergyShieldPercentOnBlockUber1",
    "RecoverLifePercentOnBlockUber1_",
    "MaximumFireResist3",
    "MaximumColdResist3",
    "MaximumLightningResist3",
    "MaximumChaosResist3",
    "MaximumAllResist2",
    "ChanceToAvoidElementalStatusAilments4",
    "ChanceToSuppressSpellsHigh5___",
    "ChanceToSuppressSpellsHigh4_",
]


def fail(msg):
    sys.exit("gen_shield: " + msg)


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


def main(argv):
    raw = argv[1] if len(argv) > 1 else "games/simple/raw"
    bases = json.load(open(f"{raw}/base_items.json"))
    if BASE_ID not in bases:
        fail(
            f"{raw}/base_items.json has no {BASE_ID}: the export copy on main holds only the toy"
            " bases, and the shield was added on the v3-item-system branch. Point this script at"
            " that branch's games/simple/raw."
        )
    base = bases[BASE_ID]
    mods = json.load(open(f"{raw}/mods.json"))

    if base["tags"] != BASE_TAGS:
        fail(f"base tags are {base['tags']}, expected {BASE_TAGS}")
    if base["item_class"] != "Shield":
        fail(f"base class is {base['item_class']}, expected Shield")
    if base["implicits"] != [IMPLICIT]:
        fail(f"base implicits are {base['implicits']}, expected [{IMPLICIT}]")

    pool_tags = BASE_TAGS + [SHAPER_TAG]
    explicit = []
    for key, mod in mods.items():
        if mod["generation_type"] not in ("prefix", "suffix"):
            continue
        if mod["required_level"] > ITEM_LEVEL:
            continue
        if spawn_weight(mod, pool_tags) == 0:
            continue
        explicit.append((key, mod, weights_on(mod, pool_tags)))
    explicit.sort(key=lambda e: (e[1]["generation_type"] != "prefix", e[1]["required_level"], e[0]))

    for key, mod, _ in explicit:
        if not mod.get("stats"):
            fail(f"{key} has no stats")

    def pool(entries, tags):
        counts = {"prefix": 0, "suffix": 0}
        totals = {"prefix": 0, "suffix": 0}
        for _, mod, _ in entries:
            side = mod["generation_type"]
            counts[side] += 1
            totals[side] += spawn_weight(mod, tags)
        return counts, totals

    if len(explicit) != 204:
        fail(f"the pool has {len(explicit)} modifiers, expected 204")
    shaper_counts, shaper_totals = pool(explicit, pool_tags)
    if shaper_counts != EXPECT_COUNTS:
        fail(f"shaper counts are {shaper_counts}, expected {EXPECT_COUNTS}")
    if shaper_totals != EXPECT_WEIGHTS:
        fail(f"shaper weight totals are {shaper_totals}, expected {EXPECT_WEIGHTS}")

    plain = [entry for entry in explicit if spawn_weight(entry[1], BASE_TAGS) > 0]
    shaper_only = sorted(key for key, _, _ in explicit if key not in {p[0] for p in plain})
    if shaper_only != SHAPER_ONLY:
        fail(f"the shaper-only set is {shaper_only}, expected {SHAPER_ONLY}")

    plain_counts, plain_totals = pool(plain, BASE_TAGS)
    if plain_counts != EXPECT_PLAIN_COUNTS:
        fail(f"plain counts are {plain_counts}, expected {EXPECT_PLAIN_COUNTS}")
    if plain_totals != EXPECT_PLAIN_WEIGHTS:
        fail(f"plain weight totals are {plain_totals}, expected {EXPECT_PLAIN_WEIGHTS}")
    if len(explicit) - len(plain) != len(SHAPER_ONLY):
        fail("the plain pool does not drop exactly the shaper-only modifiers")

    ids = {key for key, _, _ in explicit}
    for group in (DESIRED, GOOD):
        for key in group:
            if key not in ids:
                fail(f"{key} is not in the shield's pool")

    # The global tag table: the base's tags first, then the influence's tag (an item finds it by
    # name even though no base carries it), then anything a weight, implicit tag or added tag
    # names. Every name is interned exactly once, in first-seen order.
    tag_names = []
    tag_index = {}

    def intern_tag(name):
        if name not in tag_index:
            tag_index[name] = len(tag_names)
            tag_names.append(name)

    for tag in BASE_TAGS:
        intern_tag(tag)
    intern_tag(SHAPER_TAG)
    for _, mod, weights in explicit:
        for tag, _ in weights:
            intern_tag(tag)
        for tag in mod.get("implicit_tags", []) + mod.get("adds_tags", []):
            intern_tag(tag)

    implicit_mod = mods.get(IMPLICIT)
    if implicit_mod is None:
        fail(f"{IMPLICIT} is not in the export")
    if implicit_mod["generation_type"] != "unique":
        fail(f"{IMPLICIT} is {implicit_mod['generation_type']}, expected unique")
    implicit_entry = mod_json(IMPLICIT, implicit_mod, "implicit", [])
    if implicit_entry["stats"] != IMPLICIT_STATS:
        fail(f"{IMPLICIT} has stats {implicit_entry['stats']}, expected {IMPLICIT_STATS}")
    for tag in implicit_mod.get("implicit_tags", []) + implicit_mod.get("adds_tags", []):
        intern_tag(tag)

    properties = base["properties"]
    out_mods = [mod_json(key, mod, mod["generation_type"], weights) for key, mod, weights in explicit]
    out_mods.append(implicit_entry)

    json.dump(
        {
            "tags": tag_names,
            "bases": [
                {
                    "id": BASE_ID,
                    "name": base["name"],
                    "itemClass": base["item_class"],
                    "dropLevel": base["drop_level"],
                    "tags": BASE_TAGS,
                    "requirements": {
                        "level": base["requirements"]["level"],
                        "strength": base["requirements"]["strength"],
                        "dexterity": base["requirements"]["dexterity"],
                        "intelligence": base["requirements"]["intelligence"],
                    },
                    "defences": {
                        "armour": defence(properties["armour"]),
                        "evasion": defence(properties["evasion"]),
                        "energyShield": defence(properties["energy_shield"]),
                        "ward": defence(properties["ward"]),
                    },
                    "implicits": list(base["implicits"]),
                }
            ],
            "mods": out_mods,
            "target": {
                "prefixes": ["desired"],
                "suffixes": [],
                "desired": DESIRED,
                "good": GOOD,
            },
        },
        sys.stdout,
        indent=1,
        sort_keys=False,
    )
    sys.stdout.write("\n")


if __name__ == "__main__":
    main(sys.argv)
