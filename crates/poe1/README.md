# poe1

The Path of Exile 1 item model and the v3 crafting engine: a concrete item, the per-currency
crafting rules with exact integer outcomes, the goal filter, the lumped craft loop and its priced
graph. **Rules follow patch 3.30 where the notes are authoritative** — the Cannot Roll metamods no
longer shield a removal, an Imprint, a Restore and a Mirror do not end Hinekora's foresight, and the
Memory Strands tier cut is no longer applied — while **the game data stays pinned at 3.29.3.3**, so
base implicits, modifier weights and essences (the new essence is absent) are 3.29 data.

## The item

**`Item`** (`src/item.rs`) is one real item and everything crafting touches on it: base, item level,
rarity, influences, quality and catalyst, implicits, enchantments, prefixes and suffixes, sockets and
every flag (corrupted, mirrored, split, synthesised, foreseen, Memory Strands, intangibility,
imprint). It has two layers. `ItemSpec` is the serde form with string ids, used by the fixtures and
the wire; `Item` is the index form the engine uses. `Item::from_spec` validates every field and
returns an `ItemError` whose text names the problem, and `Item::to_spec` inverts it. The engine reads
an item through `limits`, `effect` and `display` (magnitudes), `tags`, `open`, `veiled`, `eldritch`
and `dominant`, `locked` and `crafted_limit`. `Item::validate` checks the rarity and side limits the
bundle states.

**Game text** (`src/text.rs`) prints and reads items the way the game shows them. `render` writes the
item block, and `parse` reads a pasted copy back into an `ItemSpec` with a list of warnings; both use
`stat_lines`, which walks the bundle's stat translations in file order — the order the game prints
them.

## Patterns and the goal

**`Pattern`** (`src/pattern.rs`) is the abstract item: a `PatternSpec` JSON file names an item filter
(`bases`, `itemLevelMin`, `rarity`, `influences`, `corrupted`) and lists classes such as "Desired
Veiled Prefix", "Good Prefix", "Any Crafted Prefix" and "Any Other Affix", each with a side, a grade
and matchers. A matcher is one mod id with optional per-stat minimums, "any crafted modifier", or
"anything". `Pattern::compile` turns the JSON into a `Pattern` and rejects unknown mods, unknown
classes, a class on the wrong side and a side with no catch-all class. `pattern::grade_value` is the
one collapse of the five authoring grades onto the three every reader uses: desired, good enough,
not good enough.

The loop does not read a pattern directly. `ItemSetFilter::from_pattern` (`src/currency/filter.rs`)
turns the pattern's classes into the goal's two buckets — its `desired` classes and its `good`,
`middling` and `neutral` ones — and an `Accept` rule (`has-desired`,
`desired-or-good-both-sides`, `any`). `ItemSetFilter` answers the only questions the engine asks of a
goal: which bucket one modifier falls into (`tier_of`, `tier_of_item`), the best bucket on a side
(`best`), and whether the item meets the goal (`holds_desired`, `good_both_sides`, `accepts`).
`evaluate` gives the probability of each pattern state for a concrete item, which is still how a
written pattern is checked against a fixture.

## Methods and `apply`

**`Method`** (`src/method.rs`) is one crafting step as data: id, name, family, how many items it
consumes, its cost, its `Require` and `PairRequire` checks, its protection policy, its `Op` list, the
claim ids it rests on and its game text. The protection policy (`Protection`) says how the method
treats Prefixes/Suffixes Cannot Be Changed and Cannot Roll Attack/Caster Modifiers: respect, ignore,
or forbid the method while the metamod is on the item. Every method whose ops remove or change
existing modifiers states it. Since 3.30 no removal respects Cannot Roll Attack/Caster Modifiers
(patch notes: the metamod "no longer prevents some Currencies that remove Modifiers, such as Orbs of
Annulment, from removing Caster Modifiers"), so the three removal currencies share
`currency::REMOVAL_PROTECTION`: locks respected, Cannot Roll ignored.

**`Outcome`** (`src/apply.rs`) is what a method returns: one flat distribution of distinct leaves,
most likely first, whose masses sum to 1. A leaf is an `Item`, the `Items` a Mirror leaves,
`Destroyed`, a `Choice` the player makes (unveiling; each option is itself an outcome of the ops
after it), or `Pending`. A `Pending` holds every input, the params, the ops still to run and a
`PendingReason` (`MultiDraw`, `Sockets`, `Unverified`, `NeedsData { what }`, `NotModelled { why }`,
`TooLarge`), and its item is either certain — a reforge's kept and guaranteed part, which its `Fill`
only adds to — or the item before a step the model does not run, which says nothing about the result.

**`apply(items, method) → outcome`** (`src/apply.rs`) checks the method first: `Require` on every
input, `PairRequire` on the two inputs, the params (fossils known and distinct), and the metamods the
method's protection forbids; the first failure becomes an `Unmet` with a reason such as `needs a rare
item` or `the item is corrupted`. It then runs the ops over a distribution of item states, starting
from the first input, and merges equal states after every op, so one item reached through different
draw orders is one leaf. Ops can read every input (`TakeInput`, `AddInfluences` for the Awakener's
Orb). Random ops weigh their candidates with `src/pool.rs`: `spawn` is the weight of the first spawn
entry whose tag the item has (claim `spawn.first-match`), and the generation percent narrows it
further. `apply::candidates` is the one function that builds a draw's pool, which is why the lumped
class is the set of facts it reads.

`pattern::evaluate` and `pattern::lift` close the loop from a concrete item to the abstract one.
`evaluate` gives the probability of each pattern state. `lift` maps an `Outcome` onto a flat
distribution of the same states: an item leaf becomes one `State` leaf per state it can be in, a
Mirror's items one `Many`, a choice its lifted options in canonical order, a reforge's certain part
one `Pending` leaf per state with that state as `known`, and a Pending that stopped before its step a
`Pending` leaf with no known state. Equal leaves merge.

## The v3 engine

**One file per currency** (`src/currency/`). Each currency is a single struct implementing
`Currency`: `apply(item, filter)` returns the exact distribution of items the step produces. Chances
are never floats on the way in or out: the pool hands out integer weights (`spawn × generation
percent`, straight out of data), a distribution is those integers over one shared total, and every
reported chance is a `Fraction` of them. `ItemOutputSet` is either `Refused(why)` — the currency
cannot be applied, so there is no distribution rather than a set of zeroes — or `Outcomes { outcomes,
total }` whose weights sum to `total` exactly. `ItemOutputSet::new` settles every result against the
item it came from (`apply::settle`) and folds equal items, refusing an empty or overflowing set.
`currency::MODELLED` lists the seven modelled ids and `currency::modelled(data, id)` is the registry
that builds one; the machine and the page both resolve ids through it. Essences, eldritch embers and
fracture are implemented but not registered: they are parameterised by a sub-kind, so an id alone
cannot construct them.

**The loop over a distribution** (`src/craft/machine.rs`). `Machine` keeps a list of `(item, chance)`
pairs because the next draw's pool depends on what the item already holds, so two items in one stage
can draw differently. One step asks `LoopPolicy` for each item's move and applies that item's
currency with its exact weights. `LoopPolicy` is the owner's loop: stop on a desired modifier;
augment from a good single side; regal a good pair; on a rare without the goal, stop once three of
its modifiers are good, otherwise reset; otherwise alter, or transmute a normal item. An outcome
that already meets the goal is absorbed the step it appears, so a stage's mass summed over steps is
the mass that left the loop there.

**The exact coarse state** (`src/craft/lumped.rs`). `Lumped` runs the same steps over `Class`, a key
that holds exactly the facts `apply::candidates` reads: base, item level, rarity, influences,
corruption, memory strands, fixed tags, and per side the counts, bucket splits, group and tag unions,
metamod bits and fractured facts. Two items with equal classes draw the same modifiers with the same
weights, so the class distribution is the concrete distribution folded by class — exact, not
approximate. Each class carries one witness item and every step runs on it. Annulment is the one
operation the class cannot restate (it removes a uniformly chosen modifier, and the class does not
record which modifier carries which group), so a lumped step refuses it. `Depth` records one step:
the live classes, each stage's mass, and the mass absorbed at the goal.

**The shared driver** (`src/craft/mod.rs`). `run_loop(data, filter, item, depth)` runs `Lumped` with
`LoopPolicy` and returns a `Loop`: the depths that ran, the absorbed mass by stage (`desired`,
`reset`, `stop` — the only stages that may be summed across depths), the live mass, and the depth
that did not fit if one did not. The CLI's `loop-report` and the page's report both call it, so they
cannot disagree about what a step did.

**The priced graph** (`src/craft/priced.rs`). `Priced::build` expands the reachable closure of classes
under `NODE_BUDGET` (1000), computes each class's exact transition table from the lumped step, and
maps that table onto the game-agnostic solver as a `Domain`. `Prices` is the owner's input: `prices`
keyed by currency metadata id (a missing key is a hard error, never a zero), `buy` and `sell` keyed
by node key (`"goal"` prices the finished item, `Priced::start_key` the item the craft starts from),
and `tradePenalty` charged on every trade. `Priced::report` solves the graph and returns the expected
chaos and its spread, the expected line items consumed, and the policy. A real craft's closure is
far past the budget, so the report carries `truncated` — how many classes the cut folded into the
frontier — and a cost over a cut graph is an upper bound.

**The JSON views** (`src/api.rs`) are what the browser sees: `start` (bases, influences, stage names,
the modelled currencies and their costs, the 3.30 price defaults), `mods` (every modifier a base can
roll), `loop_report` (the depth table plus, on request, the priced graph) and `apply_currency` (one
currency's exact outcomes). Masses cross the wire as strings — `"num/den"` and a six-decimal
percentage — because a `u128` total and an exact fraction both outrun a JavaScript number.

## What this crate no longer has

The v2 layer is gone: the one-step solver (`solve`), its inverse (`invert`, `preimages`), the forward
chart (`forward`), the catalog renderer (`catalog`, `render`), their CLI subcommands (`solve`,
`sources`, `catalog`) and every test that pinned them. `catalog/questions.md` was the owner's open
rule questions and is kept as `docs/open-questions.md`; nothing generates it. The reverse direction —
what could have made a given item — is not built on the exact engine yet.

## Data flow

```
scripts/fetch-data.sh       -> data/raw        (pinned RePoE export, 3.29.3.3)
data/raw + rules/*.json  -> cli bundle      -> data/bundle.json
games/simple/raw + games/simple/rules + rules/claims.json -> cli bundle -> data/simple.json
```

`rules/` holds the hand-written side of the data: `item-classes.json` (every in-scope class, its
category, socket count and affix limits), `excluded-bases.json` (the current-league bases no data
signal marks), `claims.json` (the cited game rules), `methods.json` (the hand-authored methods) and
`currency-scope.json` (which currency maps to which methods, or why it is out of scope). `cli bundle`
reads them, builds the bundle, and validates every reference in it.

```
nix develop -c cargo run -q -p cli --release -- bundle --raw data/raw --rules rules --out data/bundle.json
nix develop -c cargo run -q -p cli --release -- bundle --raw games/simple/raw --rules games/simple/rules --claims rules/claims.json --out data/simple.json
```

The loop report runs on the generated bundle, from a white base, with a pattern from `fixtures`:

```
nix develop -c cargo run -q -p cli --release -- loop-report --bundle data/bundle.json --fixtures fixtures --pattern totem-shield --base "Supreme Spiked Shield" --item-level 87 --influence shaper --depth 6
```

`data/` is gitignored, so `data/bundle.json` and `data/simple.json` are generated rather than
committed: by the commands above, and by CI before the tests run. Tests read the bundle from
`$HOW_BUNDLE` when it is set, else `data/bundle.json`; their own inputs live in `fixtures/items`
(`ItemSpec` files), `fixtures/patterns` (`PatternSpec` files) and `fixtures/text` (pasted item text).

## Exact and Pending

Every method has a contract. Exact outcome tables cover the single-draw methods: Transmutation,
Alteration, Augmentation, Regal (its mod add), Exalted and the influence exalts, Annulment,
Scouring, Divine, Blessed and Sacred (values unrolled), the Veiled Exalted Orb, unveil, every bench
craft, catalysts and the quality currencies, eldritch embers, ichors, exalt and annul, Dominance,
Fracturing, the resistance changes, Harvest add-remove, the Farric and metamod beastcrafts, imprint
and restore, and the Mirror. The seven currencies the loop uses are computed a second time in
`src/currency/` as exact integer-weighted sets, and the two readings are checked against each other
in `tests/currency_*.rs`.

Every reforge applies its certain part exactly before it stops: the rarity, the clear under its
protection policy (what a respected lock keeps and fractured modifiers stay), and its guaranteed
modifier — the essence mod, the Veiled Chaos placeholder, or the Harvest reforge's tag or influence
mod. Its remaining random mods stay `Pending { reason: MultiDraw }` with that certain part as the
item. A recombine keeps nothing certain, so its Pending holds both inputs and the item before the
step. Socket outcomes and the rules not modelled yet (Hinekora's Lock among them, owner Q6) stop at
the item before their step. The rules behind the rest are recorded as claims.

## Scope notes

The Allflame Crafting Bench aboard The Sovereign is left out entirely (claim
`allflame-bench.excluded`): it is treated as a league mechanic that will leave the game.

## Open questions

`docs/open-questions.md` is the review list the old catalog generated: every claim a method used
that was not yet checked and that the owner did not defer, with the claim, a worked input, step,
result and diff where the method had a fixture, and the proposed rule. The file is static now — the
generator is gone — and is kept as the record of the questions it carried.
