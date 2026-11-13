# poe1

The v2 model of Path of Exile 1 (3.29) item crafting: a concrete item, abstract item patterns, a catalog of every in-scope crafting method, and `apply(item, method) → outcome`.

## What v2 is

**`Item`** (`src/item.rs`) is one real item and everything crafting touches on it: base, item level, rarity, influences, quality and catalyst, implicits, enchantments, prefixes and suffixes, sockets and every flag (corrupted, mirrored, split, synthesised, foreseen, Memory Strands, intangibility, imprint). It has two layers. `ItemSpec` is the serde form with string ids, used by the fixtures and the wire; `Item` is the index form the engine uses. `Item::from_spec` validates every field and returns an `ItemError` whose text names the problem, and `Item::to_spec` inverts it. The engine reads an item through `limits`, `effect` and `display` (magnitudes), `tags`, `open`, `veiled`, `eldritch` and `dominant`, `locked` and `crafted_limit`. `Item::validate` checks a whole item (`from_spec` runs it, and the tests run it on every result a method gives); `Item::validate_over_limit` is the same check for a result of a method that adds past the affix limits. The game data behind those calls lives in `src/data.rs`: `Bundle` is the serialised shape, `Data` wraps it with indexes and is built by `Data::from_json`, which validates every method in the bundle, and `test_data()` loads `$HOW_BUNDLE` or `data/bundle.json`.

**`Pattern`** (`src/pattern.rs`) is the abstract item. A `PatternSpec` JSON file names an item filter (`bases`, `itemLevelMin`, `rarity`, `influences`, `corrupted`) and lists classes such as "Desired Veiled Prefix", "Good Prefix", "Any Crafted Prefix" and "Any Other Affix", each with a side, a grade and matchers. A matcher is one mod id with optional per-stat minimums, "any crafted modifier", or "anything". The slots of the pattern say which classes fill them. `Pattern::compile` turns the JSON into a `Pattern` and rejects unknown mods, unknown classes, a class on the wrong side and a side with no catch-all class. Patterns are data, never code: a new kind of class is one new matcher variant plus its `classify` arm. `classify` reads one affix as `(class, probability)` pairs, splitting the mass by value when the affix is unrolled; `evaluate` reads a whole item as `Vec<(f64, PatternState)>`, assigns affixes to slots, and reports `missing`, `unused` and `matches`; `matches_probability` sums the matching states. A `PatternState` carries what the catalog's methods read besides the classes, so two items with equal states take the same next steps: rarity, influences, corruption and mirroring, the per-side locks, the metamods, the crafted count and limit, the veiled placeholders' sides, and per affix its class, whether it is fractured or crafted and, for an affix a catch-all class took, its mod groups (`item_ok` stays as the conjunction of the filter). What a class leaves open (which tier of a listed mod, values, item level) is not in the state; the classes are written to make it irrelevant.

**`Method`** (`src/method.rs`) is one crafting step as data: id, name, family, how many items it consumes, its cost, its `Require` and `PairRequire` checks, its protection policy, its `Op` list, the claim ids it rests on, its game text and the fixture the catalog demonstrates it on. The protection policy (`Protection`) says how the method treats Prefixes/Suffixes Cannot Be Changed and Cannot Roll Attack/Caster Modifiers: respect, ignore, or forbid the method while the metamod is on the item. Every method whose ops remove or change existing modifiers states it, and the values follow Craft of Exile's metamod table, maxroll and vhpg. A reforge is written as primitives: `SetRarity`, `Clear` (what can change goes), an optional guaranteed placement (`AddEssence`, `AddVeiled`, or an `AddRandom` from a tag or influence pool), then `Fill`, which carries the typed mod-count table (claim `chaos.count`), the weighting, the side and an essence's modifier-level cap. Which modifiers count toward that total is claim `reforge.counts`, an open owner question. `Method::validate` resolves every id a method names; `Data::from_bundle` runs it, so a bundle with a bad method fails to load. Hand-authored methods live in `rules/methods.json`. The bench, essence, catalyst and fossil families are generated from the bundle by `crates/cli/src/methods.rs` (`attach`), so a new bench option or essence appears in the catalog without hand-editing.

**`Outcome`** (`src/apply.rs`) is what a method returns: one flat distribution of distinct leaves, most likely first, whose masses sum to 1. A leaf is an `Item`, the `Items` a Mirror leaves, `Destroyed`, a `Choice` the player makes (unveiling; each option is itself an outcome of the ops after it), or `Pending`. A `Pending` holds every input, the params, the ops still to run and a `PendingReason` (`MultiDraw`, `Sockets`, `Unverified`, `NeedsData { what }`, `NotModelled { why }`, `TooLarge`), and its item is either certain — a reforge's kept and guaranteed part, which its `Fill` only adds to — or the item before a step the model does not run, which says nothing about the result.

**`apply(items, method) → outcome`** (`src/apply.rs`) checks the method first: `Require` on every input, `PairRequire` on the two inputs, the params (fossils known and distinct), and the metamods the method's protection forbids; the first failure becomes an `Unmet` with a reason such as `needs a rare item` or `the item is corrupted`. It then runs the ops over a distribution of item states, starting from the first input, and merges equal states after every op, so one item reached through different draw orders is one leaf. Ops can read every input (`TakeInput`, `AddInfluences` for the Awakener's Orb). Random ops weigh their candidates with `src/pool.rs`: `spawn` is the weight of the first spawn entry whose tag the item has (claim `spawn.first-match`), `generation` is the matching generation multiplier, `weight` is their product, and `normalise` turns the candidate weights into probabilities. An item holding Memory Strands draws from a cut pool: `src/strands.rs` keeps the top `K` of the `T` tiers that pass the draw's level gate, `K = floor(100 (T - 1) / (2.5 s + 147.5)) + 1`, computed in integers (claim `memory-strands.rollable-tiers`); the cut is made before the item's own modifiers block their families, and on a draw only, so a requirement asking whether a modifier can roll at all is not cut. Whether an affix can change — removed, rerolled, cleared or upgraded — is decided in one place from the method's protection: fractured modifiers never change, a respected lock keeps its side, and a respected Cannot Roll metamod keeps the attack or caster modifiers it names; every add leaves those tags out. An op with nothing to do outside a `Chance` branch refuses the method with its own reason (owner, Q1): an empty pool, nothing that can be removed, no room for a guaranteed mod. Inside a branch it leaves that branch's item unchanged. `check` runs the same program, so it is `Ok` exactly when `apply` is. Unveiling gives one `Choice` leaf per offer set; an offer never holds two mods of one group. A reforge applies its certain part and stops at its `Fill`. After every op the pasted defence figures go once anything that feeds them changes, and foresight goes once the item changes; past 100000 states the result is `Pending { reason: TooLarge }`. `cost` resolves the method's prices to currency ids; a fossil method's resonator comes from its own `resonators` list.

**`evaluate(item, pattern)`** and **`lift`** (`src/pattern.rs`) close the loop from a concrete item to the abstract one. `evaluate` gives the probability of each pattern state. `lift` maps an `Outcome` onto a flat distribution of the same states: an item leaf becomes one `State` leaf per state it can be in, a Mirror's items one `Many`, a choice its lifted options in canonical order, a reforge's certain part one `Pending` leaf per state with that state as `known`, and a Pending that stopped before its step a `Pending` leaf with no known state. Equal leaves merge.

**Game text** (`src/text.rs`) prints and reads items the way the game shows them. `render` writes the item block, and `parse` reads a pasted copy back into an `ItemSpec` with a list of warnings; both use `stat_lines`, which walks the bundle's stat translations in file order — the order the game prints them.

**One step back** (`src/solve.rs`) asks the question the model exists for: given this item and this pattern, which single craft moves it closest to the goal, and what does that craft risk? See [One step back](#one-step-back).

**The catalog** (`src/render.rs`, `src/catalog.rs`) renders `catalog/*.md` from the bundle and `fixtures`: `render::outcome` prints an outcome, `render::diff` prints a line diff between two items, and `render_all` returns the whole file map that the `catalog` subcommand writes. `catalog/next-steps.md` is the reverse solver's report: every fixture against every pattern it fits.

## Data flow

```
scripts/fetch-data.sh          -> data/raw            (pinned RePoE export, 3.29.3.3)
data/raw + rules/*.json     -> cli bundle   -> data/bundle.json
data/bundle.json + fixtures -> cli catalog -> catalog/*.md
```

`rules/` holds the hand-written side of the data: `item-classes.json` (every in-scope class, its category, socket count and affix limits), `excluded-bases.json` (the current-league bases no data signal marks), `claims.json` (the cited game rules), `methods.json` (the hand-authored methods) and `currency-scope.json` (which currency maps to which methods, or why it is out of scope). `cli bundle` reads them, builds the bundle, and validates every reference in it. The two commands:

```
nix develop -c cargo run -q -p cli --release -- bundle --raw data/raw --rules rules --out data/bundle.json
nix develop -c cargo run -q -p cli --release -- catalog --bundle data/bundle.json --fixtures fixtures --out catalog
```

The solver runs on the generated bundle and takes a pattern by name plus either a fixture or a
pasted item:

```
nix develop -c cargo run -q -p cli --release -- solve --bundle data/bundle.json --fixtures fixtures --pattern crown-shaper --item crown-actual
nix develop -c cargo run -q -p cli --release -- solve --bundle data/bundle.json --fixtures fixtures --pattern crown-shaper --paste my-crown.txt
nix develop -c cargo run -q -p cli --release -- sources --bundle data/bundle.json --fixtures fixtures --pattern crown-shaper --item crown-a-mana
```

`solve` lists the steps that could come next; `sources` lists the inputs that could have produced the
item. Both take `--item` (a fixture name) or `--paste`. 

`--paste` reads a copied item the way the game copies it (`text::parse`) and prints its reading
warnings first, so an item on the clipboard needs no fixture.

`data/` is gitignored, so `data/bundle.json` is generated rather than committed: the command above, and the same command in the CI test job before the tests run. `catalog/` is committed and a test fails when it is stale, so a change to a method, a claim or a fixture shows up as a reviewable diff. Tests read the bundle from `$HOW_BUNDLE` when it is set, else `data/bundle.json`; the tests' own inputs live in `fixtures/items` (`ItemSpec` files), `fixtures/patterns` (`PatternSpec` files) and `fixtures/text` (pasted item text).

## Exact and Pending

Every method has a contract. Exact outcome tables in this pass cover the single-draw methods: Transmutation, Alteration, Augmentation, Regal (its mod add), Exalted and the influence exalts, Annulment, Scouring, Divine, Blessed and Sacred (values unrolled), the Veiled Exalted Orb, unveil, every bench craft, catalysts and the quality currencies, eldritch embers, ichors, exalt and annul, Dominance, Fracturing, the resistance changes, Harvest add-remove, the Farric and metamod beastcrafts, imprint and restore, and the Mirror.

Every reforge applies its certain part exactly before it stops: the rarity, the clear under its protection policy (what a respected lock keeps and fractured modifiers stay), and its guaranteed modifier — the essence mod, the Veiled Chaos placeholder, or the Harvest reforge's tag or influence mod. Its remaining random mods stay `Pending { reason: MultiDraw }` with that certain part as the item. A recombine keeps nothing certain, so its Pending holds both inputs and the item before the step. Socket outcomes and the rules not modelled yet (Hinekora's Lock among them, owner Q6) stop at the item before their step. The rules behind the rest are recorded as claims. The rare-reforge distribution (a mod count of 4, 5 or 6 at 8:3:1, then weighted group-exclusive draws) and the recombine distribution are the next pass.

## One step back

`src/solve.rs` and `src/invert.rs` are one step, forwards and backwards: not a route planner. Given a concrete item and a compiled
pattern it applies every single-input method the item allows, reads each outcome at the pattern's
level of detail (`lift`), and ranks the steps. It answers "what should I do next", not "what is the
cheapest path to a finished item".

**The score.** A pattern state is a number in `[0, 1]`: `1` when it matches (every finished item is
equally finished), otherwise the share of the pattern's slot value the item holds. A slot's value is
the grade of the class that fills it — a desired class is worth three, good two, middling and neutral
one, bad nothing — so landing the veiled prefix counts for more than landing the crafted slot the
pattern asks for, and a bad class in the third prefix slot is not counted at all. An item that fails
the pattern's filter (base, item level, rarity, influences, corruption, implicits) scores `0`,
which is why a Vaal Orb on a Crown the pattern wants uncorrupted scores nothing however much of the
item it keeps. The assignment of affixes to slots is the one `pattern::placement` already uses, so
the score agrees with `matches`.

**Three groups, never mixed.** A number computed from part of an outcome is not comparable with a
number computed from all of it, so the report keeps them apart:

- **exact** — every outcome is computed: single draws, unveil, bench crafts, the eldritch and Vaal
  orbs' exact branches;
- **certain part** — the random rest of the step is not modelled yet (a reforge's `Fill`, which is
  the next pass), so the numbers come from what the step certainly does: its rarity, its clear, its
  guaranteed modifier, the empty item the fill would add to. A Chaos Orb therefore reads as "a rare
  item with no modifiers", which is true of every result and says nothing about the 4–6 modifiers
  still to come;
- **blind** — nothing about the result is known (the socket tables, rules still being confirmed).
  These steps are listed with the reason and not ranked.

**What a step can do.** Each step is placed in one bucket, in this order: `finishes` (it can complete
the pattern), `advances` (it can bring the item further along), `unblocks` (it cannot, but it can free
a side that is full while a slot is still missing, or the crafted modifier a crafted-only slot
needs), `holds` (it leaves the goal where it is), and
`sets back` (all it can do is lose ground). Inside a bucket the order is: the chance of finishing,
the expected score, the least chance of losing ground, the least chance of ending blocked, then the
method id, so a report is reproducible; numbers are compared at the tolerance the report prints, so
rounding noise in one never outranks a real difference in the next. A step that does exactly as much
as another (`Assessment::tied`) shares its row, which gives the count and names the members, because
the ranking's only say between them is the id.

**What it reports per step.** The cost as a list of currencies (not compared: an Exalted Orb against
a Chaos Orb needs a price table the bundle does not have), the chance of finishing, the expected
score, the chance of gaining and of losing ground, the chance of ending with a side full and a slot
still missing, and how much of the outcome is not computed. Methods that do not apply are counted
with their reasons.

**The other direction** (`src/invert.rs`) answers "what could have produced this item": it walks a
method's ops backwards one at a time. Undoing an op gives the items it could have started from, each
with the chance the op turns it into the item asked about — for an add, the chance the pool draws the
modifier that was added; for a removal, one over the number of modifiers that removal could have
taken from that input, which a metamod narrows to the side it lets through; for a fracture, one over
the candidates; for a corruption, the branch's chance. Multiplying down the op list gives the
whole step's chance, and `check` rejects an input the method would not accept, so a preimage the
answering engine would refuse never appears. Three filters keep the answers honest: an input `Item::validate`
refuses (a magic item with five affixes) is no input, an item already at its affix limit cannot have
lost a modifier of that side, and a step that leaves the item exactly as
it is (a value reroll on an item whose values are unknown) is not reported, because the input and
the result are the same state.

The answers are grouped by what the *input* looked like to the pattern, so hundreds of concrete
inputs collapse into the handful of states the question is asked in. A method whose ops throw
information away (a clearance could have started from anything) or whose result the model does not
compute (a reforge's fill, a socket table) is reported as not invertible with the op that stopped it,
never guessed at. `catalog/next-steps.md` prints the top few groups under each item, and
`cli sources` prints one item's. Every list a report cuts short says how many it left out, and
`cli solve --all` / `cli sources --all` print everything.

**What it cannot do yet, and why that is next.** A reforge's random rest — the 4/5/6 modifier draw
and its weighted, group-exclusive picks — is the missing piece, and it is what turns "a Chaos Orb
leaves an empty rare" into a real distribution; until then, reforge-heavy paths are ranked only from
their certain part. Prices are the other gap: without them the solver ranks outcomes, not value for
money. Chaining steps (a full route with cycles and cost-optimality) is a later phase.

## Scope notes

The Allflame Crafting Bench aboard The Sovereign is left out entirely (claim `allflame-bench.excluded`): it is treated as a league mechanic that will leave the game.

## Open questions

`catalog/questions.md` is the review list: every claim a method uses that is not yet checked and that the owner did not defer, each with the claim, a worked input, step, result and diff where the method has a fixture, and the proposed rule; a `## Pattern notes` section follows it with each pattern's own notes, and a `## Deferred by the owner` section ends the file with each deferred rule, its statement and the methods that cite it, with no worked example.
