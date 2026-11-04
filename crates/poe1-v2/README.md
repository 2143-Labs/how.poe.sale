# poe1-v2

The v2 model of Path of Exile 1 (3.29) item crafting: a concrete item, abstract item patterns, a catalog of every in-scope crafting method, and `apply(item, method) → outcome`.

## What v2 is

**`Item`** (`src/item.rs`) is one real item and everything crafting touches on it: base, item level, rarity, influences, quality and catalyst, implicits, enchantments, prefixes and suffixes, sockets and every flag (corrupted, mirrored, split, synthesised, foreseen, Memory Strands, intangibility, imprint). It has two layers. `ItemSpec` is the serde form with string ids, used by the fixtures and the wire; `Item` is the index form the engine uses. `Item::from_spec` validates every field and returns an `ItemError` whose text names the problem, and `Item::to_spec` inverts it. The engine reads an item through `limits`, `effect` and `display` (magnitudes), `tags`, `open`, `veiled`, `eldritch` and `dominant`, `locked` and `crafted_limit`. The game data behind those calls lives in `src/data.rs`: `Bundle` is the serialised shape, `Data` wraps it with indexes and is built by `Data::from_json`, and `test_data()` loads `$HOW_V2_BUNDLE` or `data/v2/bundle.json`.

**`Pattern`** (`src/pattern.rs`) is the abstract item. A `PatternSpec` JSON file names an item filter (`bases`, `itemLevelMin`, `rarity`, `influences`, `corrupted`) and lists classes such as "Desired Veiled Prefix", "Good Prefix", "Any Crafted Prefix" and "Any Other Affix", each with a side, a grade and matchers. A matcher is one mod id with optional per-stat minimums, "any crafted modifier", or "anything". The slots of the pattern say which classes fill them. `Pattern::compile` turns the JSON into a `Pattern` and rejects unknown mods, unknown classes, a class on the wrong side and a side with no catch-all class. Patterns are data, never code: a new kind of class is one new matcher variant plus its `classify` arm. `classify` reads one affix as `(class, probability)` pairs, splitting the mass by value when the affix is unrolled; `evaluate` reads a whole item as `Vec<(f64, PatternState)>`, assigns affixes to slots, and reports `missing`, `unused` and `matches`; `matches_probability` sums the matching states.

**`Method`** (`src/method.rs`) is one crafting step as data: id, name, family, how many items it consumes, its cost, its `Require` and `PairRequire` checks, its `Op` list, the claim ids it rests on, its game text and the fixture the catalog demonstrates it on. Hand-authored methods live in `rules/v2/methods.json`. The bench, essence, catalyst and fossil families are generated from the bundle by `crates/datagen-v2/src/methods.rs` (`attach`), so a new bench option or essence appears in the catalog without hand-editing.

**`Outcome`** (`src/apply.rs`) is what a method returns: one `Item`, several `Items`, `Destroyed`, a `Chance` of weighted branches, a `Choice` of options the player picks from, or `Pending` — the partial item with the ops that are still to run and a `PendingReason` (`MultiDraw`, `Sockets`, `Unverified`, `NeedsData { what }`, `TooLarge`).

**`apply(item, method) → outcome`** (`src/apply.rs`) checks the method first. `check` walks `Require` on every input and then `PairRequire` on the two inputs, and the first failure becomes an `Unmet` with a reason such as `needs a rare item` or `the item is corrupted`. `apply` then runs the ops on a clone of the first input. Random ops become `Chance` branches whose probabilities come from `src/pool.rs`: `spawn` is the weight of the first spawn entry whose tag the item has (claim `spawn.first-match`), `generation` is the matching generation multiplier, `weight` is their product, and `normalise` turns the candidate weights into probabilities. Unveiling is a `Chance` over offer sets, each set a `Choice` of three options. A reforge with a guarantee places the guaranteed mod exactly and hands the rest on as `Pending`. After the ops, `apply` clears foresight unless the method set it, consumes catalyst quality when a currency, essence or fossil touched the item, drops stale defence figures, canonicalises the item and merges equal branches; past 100000 branches the result is `Pending { reason: TooLarge }`. `cost` resolves the method's prices to currency ids.

**`evaluate(item, pattern)`** and **`lift`** (`src/pattern.rs`) close the loop from a concrete item to the abstract one. `evaluate` gives the probability of each pattern state. `lift` maps an `Outcome` onto the same states: `Item` becomes a `State` or a `Chance`, `Items` becomes `Many`, `Pending` keeps its reason and reports what the partial item already holds as `known`, and `Chance` and `Choice` recurse.

**Game text** (`src/text.rs`) prints and reads items the way the game shows them. `render` writes the item block, and `parse` reads a pasted copy back into an `ItemSpec` with a list of warnings; both use `stat_lines`, which walks the bundle's stat translations in file order — the order the game prints them.

**The catalog** (`src/render.rs`, `src/catalog.rs`) renders `catalog/*.md` from the bundle and `fixtures/v2`: `render::outcome` prints an outcome, `render::diff` prints a line diff between two items, and `render_all` returns the whole file map that the `catalog` subcommand writes.

## Data flow

```
scripts/fetch-data.sh          -> data/raw            (pinned RePoE export, 3.29.3.3)
data/raw + rules/v2/*.json     -> datagen-v2 bundle   -> data/v2/bundle.json
data/v2/bundle.json + fixtures/v2 -> datagen-v2 catalog -> catalog/*.md
```

`rules/v2/` holds the hand-written side of the data: `item-classes.json` (every in-scope class, its category, socket count and affix limits), `claims.json` (the cited game rules), `methods.json` (the hand-authored methods) and `currency-scope.json` (which currency maps to which methods, or why it is out of scope). `datagen-v2 bundle` reads them, builds the bundle, and validates every reference in it. The two commands:

```
nix develop -c cargo run -q -p datagen-v2 --release -- bundle --raw data/raw --rules rules/v2 --out data/v2/bundle.json
nix develop -c cargo run -q -p datagen-v2 --release -- catalog --bundle data/v2/bundle.json --fixtures fixtures/v2 --out catalog
```

`data/` is gitignored, so `data/v2/bundle.json` is generated rather than committed: the command above, and the same command in the CI test job before the tests run. `catalog/` is committed and a test fails when it is stale, so a change to a method, a claim or a fixture shows up as a reviewable diff. Tests read the bundle from `$HOW_V2_BUNDLE` when it is set, else `data/v2/bundle.json`; the tests' own inputs live in `fixtures/v2/items` (`ItemSpec` files), `fixtures/v2/patterns` (`PatternSpec` files) and `fixtures/v2/text` (pasted item text).

## Exact and Pending

Every method has a contract. Exact outcome tables in this pass cover the single-draw methods: Transmutation, Alteration, Augmentation, Regal (its mod add), Exalted and the influence exalts, Annulment, Scouring, Divine, Blessed and Sacred (values unrolled), the Veiled Exalted Orb, unveil, every bench craft, catalysts and the quality currencies, eldritch embers, ichors, exalt and annul, Dominance, Fracturing, the resistance changes, Harvest add-remove, the Farric and metamod beastcrafts, Hinekora's Lock, imprint and restore, and the Mirror.

Reforges with a guarantee get their guaranteed part exactly: the essence mod, the Veiled Chaos placeholder, or the Harvest reforge's tag or influence mod. Their remaining random mods stay `Pending { reason: MultiDraw }`. Reforges without a guarantee stay wholly Pending: Chaos, Alchemy, Binding, fossils, Harvest more-likely and less-likely, Eldritch Chaos, the Awakener's Orb and the Recombinators. Their rules are recorded as claims. The rare-reforge distribution (a mod count of 4, 5 or 6 at 8:3:1, then weighted group-exclusive draws) and the recombine distribution are the next pass.

## From v1 to v2

| v1 | What it does | In v2 | Status |
|---|---|---|---|
| `crates/datagen` | Builds the bundle from the pinned RePoE export: raw structs, influence and price keys, catalyst tags, base prefix and suffix deltas, stat-line slots | `crates/datagen-v2` | Copied and extended. The schema now lives once, in `poe1_v2::data` |
| `poe1::data` | Bundle types and indexes | `poe1_v2::data` | Replaced: 16 mod kinds, item classes, stat translations, methods, claims, currency scope |
| `poe1::claims`, `rules/poe1-claims.json` | Game rules as cited data | `rules/v2/claims.json`, `Bundle.claims` | Kept. v1 claims a method uses are copied by id |
| `poe1::item` (`ItemState`) | Target-relative summary of an item | `poe1_v2::item::Item`, `pattern::PatternState` | Replaced. The item keeps every mod; `evaluate` computes the target-relative view |
| `poe1::target` | Goal: one requirement per mod group plus `min_hits`; magnitude `effect` and `display` | `poe1_v2::pattern`; `Item::effect`, `Item::display` | Replaced by patterns; `effect` and `display` copied |
| `poe1::text` | Reads a paste into a target draft | `poe1_v2::text` (`parse`, `render`) | Copied and extended. It reads into a concrete item, prints game text and reads lines that have no number |
| `poe1::weights` | Spawn weights, mod pools, mod sources | `poe1_v2::pool`; `parse` pass-1 reachability | Re-modelled |
| `poe1::kernels` | Each currency's outcome distribution on target-relative state; recombine; `predecessors` | `rules/v2/methods.json` and the generated families; `poe1_v2::apply`; `pattern::lift` | Re-modelled as data plus one interpreter. Recombine is Pending; `predecessors` belongs to the reverse-solver phase |
| `poe1::registry` | Per-action contract: preconditions, side effects, cost, `seconds` | `Method.requires`, `ops` and `cost`; `apply::check` | Replaced, with exact unmet reasons. `seconds` is not carried in this pass |
| `poe1::recipe` | Data-defined templates for market processes | — | Not ported in this pass. The essence and recombine ideas return as methods |
| `poe1::market` | Goods, processes and the value of a good | — | Not ported in this pass |
| `poe1::engine` | The ruleset as a `solver` domain | — | Not ported in this pass |
| `poe1::api`, `crates/wasm-api`, `web/` | Wire contract, worker bridge, UI | — | Untouched; v2 has no UI in this pass |
| `crates/solver` | Game-agnostic graph, policy iteration, statistics, proposals | — | Untouched |
| `crates/server` | Static file server | — | Untouched |
| `fixtures/*.txt`, `web/src/demo/*.txt` | Paste fixtures | `fixtures/v2/text/` | The demo Crown and Simplex pastes are copied |

## Why v2

Five limits in v1 forced the rebuild.

1. **A target is a list of per-group requirements plus a hit count.** `crates/poe1/src/target.rs:16-35`:

   ```rust
   pub struct Requirement {
       pub group: u16,
       pub side: Side,
       /// Accepted mod indices (tiers), in bundle order.
       pub accepted: Vec<u32>,
       pub min_value: f64,
       pub label: String,
   }

   pub struct Target {
       pub base: u16,
       pub ilvl_min: u8,
       pub influences: u8,
       pub rarity: Rarity,
       pub catalyst: Option<(u8, u8)>,
       pub requirements: Vec<Requirement>,
       pub min_hits: u16,
   }
   ```

   That shape cannot say "a good prefix is an AoE support at T1 to T3 or T1/T2 life", "any crafted prefix" or "a desired veiled prefix". v2 states those as classes inside one slot.

2. **An item is a count of mods outside the target.** `crates/poe1/src/item.rs:83` is `pub untracked: u8`, next to `pub untracked_influenced: u8`. Once a mod is untracked, nothing knows which mod an annulment takes. v2 keeps every mod on the `Item` and computes the target-relative view only in `evaluate`.

3. **A recipe must deliver every requirement.** `crates/poe1/src/recipe.rs:285-288`:

   ```rust
   let (set, core) = delivers(r);
   if !ranks.iter().all(|rs| rs.iter().any(|x| set.contains(x))) {
       continue;
   }
   ```

   A requirement that only a veiled craft can meet therefore gets no route at all. v2 has no such gate: a method returns an outcome, and `lift` reports how far it moved the item.

4. **A normal input silently loses its influences.** `crates/poe1/src/recipe.rs:174-178`:

   ```rust
   influences: if rarity == Rarity::Normal {
       0
   } else {
       out.spec.influences
   },
   ```

   An essence route on a Shaper base then reads as unreachable, because the normal item it starts from has no influence. v2 builds the item spec from the base and keeps the influence set, and `apply` checks it against the method.

5. **The v1 bundle keeps two bench metamods and treats veiled mods as a source only.** `crates/datagen/src/main.rs:840-851` skips every bench option whose mod is not one of `DexMasterItemGenerationCannotChangeSuffixes` or `StrMasterItemGenerationCannotChangePrefixes`, and `crates/datagen/src/main.rs:616` keeps a veiled mod only as a resolvable source (`modelled || referenced || is_veiled(m)`): its spawn weights go to `veiled_weights` and its own weight list stays empty (see the `veiled_weights` comment at `crates/datagen/src/main.rs:248-250`), so no natural pool holds it. The Crown finish — veiled exalt, blocker craft, unveil — cannot be written down in v1. In v2 every bench option is a method, the veiled placeholder is an `Op`, unveiling is a `Chance` over offer sets, and an unveiled mod is a normal modifier.

## Open questions

`catalog/questions.md` is the review list: every claim a method uses that is not yet checked, each with the claim, a worked input, step, result and diff where the method has a fixture, and the proposed rule; a `## Pattern notes` section follows it with each pattern's own notes.
