# games/simple

A small game for the engine's tests: three bases and the modifiers they roll, in the pinned
export's raw shape and through the same bundler as `data/raw`. It exists so the per-currency
model can be exercised without the 63 MB export.

```
nix develop -c cargo run -q -p cli --release -- bundle --raw games/simple/raw \
  --rules games/simple/rules --claims rules/claims.json --out data/simple.json
```

`data/simple.json` is a build product, not committed; `crates/poe1/tests/simple.rs` and
`crates/poe1/tests/simple_shield.rs` load it by that path and fail with this command when it is
missing.

## What it holds

- `raw/base_items.json` — the two toy bases (Bow, Helmet) and the export's
  `Supreme Spiked Shield` (`Metadata/Items/Armours/Shields/ShieldDexInt13`), with its implicit,
  tags, requirements and defences copied from the export.
- `raw/item_classes.json` — Bow, Helmet and Shield; Shield carries the export's six influence
  tags (`shield_shaper` among them), which is what makes an influenced item's pool reachable.
- `raw/mods.json` — the toy modifiers, the shield's implicit, and every modifier the shield can
  roll: 73 prefixes and 131 suffixes, with the export's ids verbatim (`MaximumTotemUber1`,
  `IncreasedLife10`, …) so `fixtures/patterns/totem-shield.json` compiles unchanged. Within the
  shield block, prefixes come first, then suffixes, each ordered by family (`type`) and level.
- `rules/item-classes.json` — affix limits by category and each class's socket count; Shield is
  an `armour` class with 3 sockets, so a magic shield holds one prefix and one suffix.
- `raw/crafting_bench_options.json`, `essences.json`, `fossils.json` — the toy bench crafts and
  empty essence/fossil tables; the Shield class has neither.

## How the shield's modifiers were selected

They are a filtered copy of the pinned export, not a transcription. From `data/raw/mods.json`,
keep every modifier that

- is `domain: "item"` with `generation_type` prefix or suffix,
- is not essence-only and is not a Battle Royale copy (`Royale` in the id),
- has a positive spawn weight for one of the item's tags: the base's tags
  (`dex_int_armour`, `dex_int_shield`, `shield`, `armour`, `default`, …) plus `shield_shaper`.

No level filter is applied: the bundler tests `required_level` against the item level itself, so
a level-filtered copy would be quietly wrong at another level. This base's top tier is level 86,
so the filter yields exactly the item-level-87 pool.

Weights and `generation_weights` are copied verbatim. Data divides generation weight by 100, so
the model's weights are the published figures ×100; divided out, the pool is

| side | total |
| --- | --- |
| prefix | 64 800 |
| suffix | 96 475 |

with `MaximumTotemUber1` at 800, `IncreasedLife*` at 11 000, `IncreasedMana*` at 12 000 and each
elemental resistance at 8 000. `crates/poe1/tests/simple_shield.rs` pins the totals and the three
chances (1/81, 5/81, 584/3859) for one draw on a magic shield.

## Why the toy bases' tags are namespaced

The toy bases carry `simple_bow`, `simple_helmet` and `simple_default` instead of `bow`,
`helmet`, `armour`, `weapon` and `default`. Real shield modifiers spawn on those real tags (the
helmet's resistances, the bow's weapon modifiers), so a shared bundle would silently widen the
toy pools. The toy modifiers' weights are unchanged, only the tag names; every toy chance is the
same as before.

## Deliberately absent

- **Stat translations for the shield's modifiers.** `ModData.text` carries the printed line, which
  is what the engine prints and what the pattern tests read. Translations are only needed to
  resolve a pasted item's lines back to modifiers, and the simple game is not used for paste
  parsing.
- **Bench crafts, essences and fossils for the Shield class.** Only the two toy classes have them;
  the currency methods the toy rules list are the eleven that need no bench.
- **Every other modifier kind and every other base.** Crafted and veiled modifiers exist only as
  the toy placeholders the bench and the Veiled Chaos Orb need.

## The same code, the same numbers

Simple is not a parallel implementation: the currency structs, the craft machine and the lumped
state in `crates/poe1` run on `data/simple.json` exactly as they run on the pinned export. The check
is that the answers do not move:

```
nix develop -c cargo run -q -p cli --release -- loop-report \
  --bundle data/simple.json --pattern totem-shield \
  --item "Supreme Spiked Shield" --item-level 87 --influence shaper --depth 2
```

prints the same first depth as the export does — 587 live classes, the goal at `1/108`
(0.925926 %), a good prefix at `5/324`, a good suffix at `146/3859`, everything else at
`291575/312579`. `crates/poe1/tests/simple_shield.rs` pins the same three chances and the two pool
totals from simple's data, and compiles `fixtures/patterns/totem-shield.json` against it unchanged,
because the modifier ids are the export's own.

That is the property worth keeping: a dataset can be small and hand-reviewable without becoming a
second model. If a figure ever differs between the two bundles, the dataset is what is wrong.
