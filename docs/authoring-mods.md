# Authoring modifiers by hand

`solution` for a mod list that has to be written by a person: what each field in `raw/mods.json`
means, which ones the engine actually reads, and the four places a plausible entry is silently not
what you meant. Everything here is read off `crates/cli/src/bundle.rs` and `crates/cli/src/raw.rs`;
if this document and those disagree, they are right.

## One entry

```json
"MaximumTotemUber1": {
  "adds_tags": [],
  "domain": "item",
  "generation_type": "prefix",
  "generation_weights": [],
  "grants_effects": [],
  "groups": ["AdditionalTotems"],
  "implicit_tags": ["influence_mod"],
  "is_essence_only": false,
  "name": "The Shaper's",
  "required_level": 70,
  "spawn_weights": [{"tag": "shield_shaper", "weight": 800}, {"tag": "default", "weight": 0}],
  "stats": [{"id": "number_of_additional_totems_allowed", "max": 1, "min": 1}],
  "text": "+1 to maximum number of Summoned Totems",
  "type": "AdditionalTotems",
  "gold_value": null
}
```

## What each field means to the engine

| field | what it decides |
|---|---|
| `generation_type` | `prefix` / `suffix` sets the side. Anything else is not an affix and cannot be rolled (implicits, enchants, corrupted implicits and eldritch implicits have their own strings). |
| `domain` + `generation_type` | together they decide the *kind*: `item`+affix = natural, `crafted` = bench, `delve`, `unveiled`, `veiled`, `mercenary`, `ducat_crafted`, `item`+`corrupted` = a Vaal implicit, `searing_exarch_implicit` / `eater_of_worlds_implicit` = eldritch. |
| `type` | the **family**: every tier of one modifier shares it. Two tiers of life are one `type` and a higher `required_level`. This is what "T1/T2 life" means and what `or_better` compares. |
| `required_level` | the tier's item-level requirement; the pool drops a modifier above the item's level. It is also the tier order — the highest level in a family is T1. |
| `spawn_weights` | the weights. **The first entry whose tag the item carries wins**, so order matters and a later entry cannot rescue an earlier match. `{"tag": "default", "weight": 0}` is the usual "nothing else matched" terminator. |
| `generation_weights` | a multiplier in percent, per tag, first match wins. Empty means 100, which is why the model's integer weights are the published weight × 100. |
| `implicit_tags` | the tags the Cannot Roll metamods and catalysts test (attack/caster). Not what marks an influence — see below. |
| `adds_tags` | tags the modifier grants to the item that holds it. |
| `groups` | one group per modifier may be on an item, so a group is also how two families are made mutually exclusive. |
| `stats` | `id`, `min`, `max` — the roll. A stat with min == max has a fixed value and is stored as one. |
| `text` | what a report prints. Tier tables and the pool report show this. |
| `is_essence_only` | an `item`-domain affix with this set is an essence modifier, not a natural one. |

## Four ways a correct-looking entry goes wrong

1. **Influence comes from the spawn tag, not from `implicit_tags`.** `influence_of_tag` reads the
   *last underscore word* of a spawning tag: `_shaper`, `_elder`, `_crusader`, `_eyrie` (Redeemer),
   `_basilisk` (Hunter), `_adjudicator` (Warlord). So `shield_shaper` at weight 800 is what makes
   the totem modifier a Shaper modifier; writing `"influence": "shaper"` does nothing, because there
   is no such field.
2. **An elevated modifier is recognised by its id containing `Maven`.** There is no `elevated` field
   in the raw data; it is derived from the name.
3. **Eldritch tiers are recognised from the id's trailing digits** (`…1` to `…6`). A renamed eldritch
   modifier fails the build rather than losing its tier quietly.
4. **Modifiers with `Royale` in the id are dropped**, because they are Battle Royale copies a league
   item cannot roll.

## Tags are names, never numbers

Every tag in `spawn_weights`, `generation_weights`, `implicit_tags`, `adds_tags` and every group,
stat and mod-family name is written as its **name** and interned by the bundler. A name that appears
nowhere else in the dataset is still fine — it simply never matches — which is the failure mode to
watch for: a typo in a tag name does not error, it makes a modifier unrollable.

The two tag sets that matter for a pool:

- the **base**'s tags (`base_items.json`): what the base is, e.g. `dex_int_armour`, `dex_int_shield`,
  `shield`, `armour`, `default`;
- the **item class**'s influence tags (`item_classes.json`): added to the item's tag set when it is
  influenced, which is what makes `shield_shaper` reachable at all.

## The check that catches almost everything

After bundling, a pool's total is the sum of its modifiers' weights. For the owner's Shaper Supreme
Spiked Shield at item level 87 that must be 64 800 for prefixes and 96 475 for suffixes (after
dividing out the ×100). If a family's total is wrong, a tag name is wrong. If the *count* is right
but the total is wrong, a `spawn_weights` entry is out of order.
