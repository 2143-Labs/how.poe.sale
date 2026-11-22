# Where the per-currency model stops

A record of the boundary found while covering `rules/methods.json` against the per-currency model.
It is written to be argued with, not obeyed: each item names what is missing, the evidence, and the
smallest change that would close it. Nothing here is a promise to build it.

`rules/methods.json` declares 199 hand-authored methods over 30 op kinds (296 op occurrences); 76
methods are a single `unmodelled` op. Of those op kinds, **26 are expressible today** by a
`Currency` whose signature is `apply(item, filter) -> ItemOutputSet` — the parameters they need
(pool, side, mod id, essence, eldritch tier, value scope, protection) are constructor fields, which
is where they belong. Four are not.

## 1. A choice leaf — `unveil`

`Op::Unveil` emits `Leaf::Choice(Vec<Outcome>)` (`apply.rs:2019`, `:2083`): the vehicle rolls three
options and the player picks. `Outcome.item` is a single `Item`, so `ItemOutputSet` cannot say "the
result is one of these three, and the player chooses".

Smallest change: an outcome variant `Choice(Vec<Item>)`, or a separate `ItemOutputSet::Choice` case.
The mass model is unchanged — the three options are still weighted — so this is a shape change, not a
probability change. Whether a *policy* can act on it is a second question: the loop would have to
decide by grade.

## 2. A destroyed leaf — `destroy`

`Leaf::Destroyed` (`apply.rs:1347`) means the item ceases to exist. Today an empty
`ItemOutputSet::Outcomes { total: 0 }` is indistinguishable from `Refused`, and `Refused` means "this
currency does not apply". A currency whose result is *nothing* needs its own case, or a policy will
read a destroyed item as an inapplicable step and try again forever.

## 3. A multi-item leaf — `mirror`

`Leaf::Items(item, copy)` (`apply.rs:1341`) is one input becoming two. `Outcome.item` cannot hold it.
Mirror of Kalandra is also `currency/mirror`, which the machine's loop never names, so this is not
urgent — but it is the same shape as forging two halves of a split item, and it is the reason
`ItemOutputSet` is not a general outcome set yet.

## 4. A second input — `takeInput`, `addInfluences`

`Op::TakeInput` (`apply.rs:1377`) and `Op::AddInfluences` (`apply.rs:1704`) read another method
input. The `Machine`'s state is one item's distribution, and it has no notion of a second input:
`Awakener's Orb` and every other two-item method cannot be a step at all. This is a property of the
machine, not of `ItemOutputSet` — and it is the one boundary I would keep, because a two-input craft
is a different kind of decision (you are choosing a pair, not rolling one item).

## 5. `TooLarge`, which the old model already has

`PendingReason::TooLarge => "too many outcomes"` (`method.rs:616`) exists because a multi-draw
reforge — `Op::Fill` (`apply.rs:1348`), which is a pure `Pending` today — has a combinatorial outcome
set: a chaos reroll of a rare shield is four to six draws from 204 modifiers. The per-currency model
*can* express this exactly (`adds(data, item, n, pool)` composes draws and folds identical items),
but it cannot enumerate it.

So the honest shape is a bound: compute exactly under a threshold, and report `TooLarge` with the
number of outcomes above it, rather than refusing vaguely or approximating silently. That keeps the
repository's existing vocabulary — the report already groups by `PendingReason::kind()` — and it makes
`currency/chaos` implementable for the first time, since the op path has never computed it.

## What this means for the machine

The `Machine`'s action set is currently four currencies; `Exalted` and `Scouring` were implemented but
unreachable from a policy, which is a dispatch defect rather than a modelling one. Coverage is
therefore a question of three things in order: dispatch (name a currency by id), shape (the four
leaves above), and scale (`TooLarge` plus whatever the multi-step analysis concludes). Dispatch is
the cheapest and the one that unblocks the most, so it comes first.

## Correction: the state *is* exactly lumpable

The `craft` module originally claimed the machine's state cannot be lumped by `Stage` because "one
good prefix item's augmentation pool is not another's". For this craft that is **false**: on the
Shaper Supreme Spiked Shield the prefix and suffix modifier groups are disjoint in the data, so an
augmentation excludes nothing, and no shield candidate carries a spawn weight on the `adds_tags` the
pool tests. The conclusion still holds — `Stage` is not a Markov state — but for four other reasons:

1. **Regal's group exclusion**: a rare draw may not repeat a group the item already holds, so the pool
   depends on the item's held groups (an augmentation's does not).
2. **Rarity and room**: a magic `GoodPair` and a rare `GoodPair` have different affix limits, so the
   same stage means different pools.
3. **Metamod bits**: on this shield the `Level28` metamod alone cuts its own pool from 204 modifiers
   (73 prefixes, 131 suffixes at item level 87) to 58 (23 and 35 at level 28 or below), and the locks
   decide what a clear can remove. The all-bases natural pool is 3 765 at item level 87 and 633 at
   level ≤ 28 — a different set, and the source of a disagreement worth recording so nobody re-derives
   it: both numbers are right, about different things.
4. **Fractured and locked affixes** survive a clear, so they persist into the next state.

The audit established that the next pool depends on the item only through: rarity, per-side count,
per-side held group set, per-side held count by tier bucket, the union of held `adds_tags`, the
metamod bits, Memory Strands, corrupted, mirrored, and which affixes are fractured or on a locked
side — the complete dependency list of `apply::candidates`. That is a finite, exact state. On this
shield it is 360 pool-exact classes (629 once tier buckets are counted) and about 46 under the
owner's loop, against 9 767 concrete magic items after one transmutation.

Sampling was considered and rejected: with replacement it is unbiased and easy (integer weights), but
a 4 940-modifier pool scan dominates a draw, so ten thousand sampled paths cost about one exact step
and carry 1–3 % error. Exact lumping is cheaper *and* exact.

Two defects the audit also found, both in the machine's accounting: a currency whose pool is empty
returns `Outcomes { outcomes: [], total: 0 }`, and the step loop then drops that item's mass without a
word (`ItemOutputSet::exact()` already reports false and nothing checked it); and an outcome that
reaches the goal is left in the live distribution and recorded again as `Done` on the next step, so
summing `Desired` across depths double counts. Both are being fixed by absorbing on reach and by
refusing an inexact set.

## What the per-currency model covers now

Implemented as code, each agreeing with its op-declared twin item-for-item and chance-for-chance:
transmutation, alteration, augmentation, regal, exalted, scouring, annulment, essence (parameterised
by which essence), with fracture and the eight eldritch embers/ichors following.

Still out of reach, and why — the four leaf shapes above, plus the multi-draw:

- **chaos, and vaal's fourth quarter.** Both need `Op::Fill`, which the op path has never computed
  (it returns a `Pending` leaf). A rare reroll is four to six draws from ~204 modifiers, so the
  concrete outcome set is tens of millions of items. The per-currency model can *express* this
  (`adds` composes draws), but only a lumped state can make it tractable — which is exactly what the
  multi-step work is building.
- **vaal's second and third quarters** need `Op::Sockets` (the "dewhiten" outcome), which is a
  `Pending` leaf too, so a Vaal Orb cannot be exact today even though two of its four quarters are.
- **mirror, unveil, destroy** are the leaf shapes: multi-item, choice, destroyed.
- **Awakener's Orb** and every other two-input method: the machine's state is one item.

Two facts the essence work settled, worth keeping:

- The generated essence methods want a **normal or rare** item, not magic — the op path refuses a
  magic one, so a currency must too. A currency may never be more permissive than its method.
- `item_level_max` on an essence is **not** a refusal: claim `essence.ilvl` says an essence works on
  an item of any level and the cap belongs to the fill that follows it. The op path accepts such
  items, so refusing would have invented a rule.

And two the currency layer itself settled:

- `ItemOutputSet::new` now settles every result against the pre-step item (`apply::settle`), so the
  display figures and `foreseen` cannot outlive the modification that invalidated them.
- It refuses an empty set. Before, an empty pool produced `Outcomes { outcomes: [], total: 0 }` and
  the machine's loop then dropped that item's mass silently. The Simplex amulet paste forced to
  magic is the reproduction and is now a test.

## Measured: shape legality was never the problem

The earlier diagnosis predicted that the currency layer would produce items `Item::validate` rejects.
It does not. An independent pass ran every single-input method over all 34 fixtures (66 555 result
items), one step further from 2 246 depth-1 states (8 322 053 items), and every currency over every
fixture plus 4 224 two-step checks: **zero** shape failures, no item ever over its rarity's affix
limits. So result validation belongs in tests, and an invariant engine that folds `validate` into the
op path would be churn for no hits.

What the layer did get wrong is the *distribution*, and that is where the value went:

- **A wrong normalisation across draws.** `adds` multiplied raw weights across successive draws
  without dividing by each intermediate item's own total. It is exact only when every first draw
  leaves the same second-draw total — on the shield the prefix-first and suffix-first errors cancel
  to 2.7e-20, but a Viridian Jewel is 5.5 % out on some outcomes, and `exact()` still reported true.
- **A dead end that should be carried forward.** A draw that cannot happen is a no-op in the op path
  (a conditional branch keeps the item), but `adds` dropped the path, so an alteration on a magic
  item with a fractured or lock-protected modifier produced 131 outcomes all at weight zero.
- **Settling once per step instead of once per op.** `settle` now runs on each result, but an
  alteration that rerolls the same unrolled modifier keeps `defences` where the op path clears it.

The lesson is the one worth keeping: exactness that is only checked against items with no display
state, on one base whose pools happen to be symmetric, is not exactness.

## The price ceiling, and where the seam actually is

A policy is `&Item -> Action`, so it cannot weigh cost — which is the real limit on applying this to
other crafts, because the owner's decisions are cost-aware ("annul until it is worth exalting"). The
pieces exist and are not connected:

- **Cost is already data, as line items.** Every method in `rules/methods.json` carries
  `cost: [{name, amount}]` — a currency display name or `beast:<name>`, with counts of the tradeable
  item (Harvest lifeforce is in points, not items). `Method::validate` already requires each currency
  name to resolve to exactly one bundle currency, so a cost is never a dangling reference.
- **Every consumer displays cost and none does arithmetic with it.** `solve`, `invert`, `preimages`
  and the browser all print it; `Assessment::rank` compares only the probability columns. The README
  says so twice ("Costs are listed, not priced"), so this is a known gap rather than a bug.
- **`crates/solver` is a complete priced-MDP kit with no client.** `Domain` already has
  `cost` (line items) and `sell_value` — the "what is a three-good rare worth" hook — and
  `GraphInput` takes `prices` in chaos per line-item key, plus `buy`, `sell` and `trade_penalty`.
  `solve` gives a policy, `moments` gives expected chaos and expected spend per line item, and
  `propose` ranks a predecessor's actions by `cost + Σ p·V`. An unpriced key is a hard error, by
  design.

So the missing piece is *not* a cost model, and not a solver: it is the adapter from the machine's
lumped class graph to `solver::Domain`, plus a `prices.json` mapping each line-item key the craft
touches to chaos. The lumped class graph is the right state space for it — 587 classes where the
concrete distribution has 9 767 items — which is why the lumping work was worth doing before pricing
rather than after.

What a first honest report can say without that adapter: expected currency spent per stage
transition at one step, and the share of mass that reaches each stage. What it must not claim: an
optimal policy, or a total cost of the craft.

## One divergence the harness found is a defect in the *op* path

The widened comparison (5 736 cases across 18 items and 8 currencies, at one and two steps) came back
5 632 pass / 104 fail. All 104 are the two currency-layer bugs above, with one exception, and that
exception goes the other way:

Transmuting a **Simplex Amulet** scoured to normal: the currency refuses ("the step produced no
outcome") and the op path returns a magic item with **no modifiers at all**. The claim
`empty.refused` settles it — "Using a currency on an item it has nothing to change is refused; the
currency is not consumed" — so the code path is right and the op path is wrong. The mechanism is that
`Op::AddRandom` does error on an empty pool, but a `Chance` branch catches the failure as a
conditional no-op and keeps the item, so a transmutation whose only branch can add nothing still
returns a rarity change.

That is worth fixing in `apply.rs`, where it also affects the old `solve` and `sources` paths — but it
is not a currency-layer change, it touches the 32 goldens, and it needs its own before/after. Recorded
here rather than folded into the currency work.

## The fill kernel: exact in principle, out of reach for a rare reroll

A fill (chaos, veiled chaos, eldritch chaos, the Harvest add/remove reforges) draws several
modifiers in a row without repeating a group. The annulment obstruction does **not** apply: a fill
*adds*, and the exclusion test is membership in the class's group union, which is monotone. The
outcome class is a pure function of (class, drawn modifier) — `count += 1`, the bucket count,
`groups ∪ groups(m)`, `tags ∪ adds_tags(m)`, the metamod bits — so an exact class kernel exists.
That is why adding is lumpable and removing is not.

Two walls stop it rescuing the shield's chaos reroll:

1. **The class graph is not small.** With 18 groups per side, the 4-to-6-draw reroll has of the order
   of 10⁸ outcome classes — about 100× fewer than the 2.4 × 10¹⁰ concrete items, which is not enough
   to make a depth cheap.
2. **The exact rational cannot hold its denominator.** A class mass's denominator divides the product
   of `k·T` over the draws, and folding a layer into per-stage masses takes the LCM across classes,
   which overruns `u128` at three or four draws. `checked_add` returns `None`, and `MachineError::`
   `Overflow` is then the correct answer rather than a rounded one.

So the honest shape is the one the repository already has vocabulary for: implement the kernel
(it is cheap and exact for jewels, magic items and single-side fills), gate it on a class budget
*and* on `Fraction` overflow, and for the shield emit `PendingReason::TooLarge` **with the exact
outcome count** — a knapsack count over families, not an enumeration.

Three limits to record before anyone builds it:

- **`Fill::include`** (the 17 Harvest reforges that guarantee a tag) tests a *mod-id* set. The class
  keeps `adds_tags`, not the held modifiers' own `tags`, so it needs a `held_tags` fact — and that
  fact is only exact when a fill does not also cap the level.
- **`Weighting::MoreLikelySame` / `LessLikelySame`** boost a candidate by whether the item already
  holds that modifier: that is modifier identity, and it is **permanently** un-lumpable.
- **`SideReq::Dominant`** (eldritch chaos) reads the eldritch implicits' tiers, which the class does
  not record yet.

## A lesson from the lumped cache: never restate the engine's predicate

The first version of `Class::cleared` decided a side was locked by testing that *side's* metamod bit.
But a lock sits on the opposite side — `StrMasterItemGenerationCannotChangePrefixes` is a suffix that
locks prefixes — so `cleared()` never saw a lock, and two classes that clear differently shared one
alteration cache entry. Nothing was lost from a total; the mass landed on the wrong items, which is
precisely what a coarse state's exactness argument forbids.

The engine's own answer was already there: `Item::locked` searches every explicit. A coarse state is a
second implementation of the same facts, so every predicate it needs must be *called* from the engine,
not restated beside it. That is the same rule the currency layer learned when `settle`,
`clear_changeable`, `removable` and `cannot_place` were shared rather than copied — and it is worth
stating here because the lumped state is the third place the temptation appears.

The review also found a grading error in the goal fixture itself: `totem-shield`'s catch-all classes
were graded `neutral`, which `solve::grade_value` scores the same as `good`, so the score path could
not tell a junk prefix from a good one while the step filter could. A goal written in one vocabulary
and scored in another is the same class of mistake at the data layer.

## The pricing adapter, specified

The adapter from the lumped class graph to `solver::Domain` has been designed down to signatures. The
one thing `crates/poe1` is missing is small and specific: `Class` derives only `Clone, Debug`, its
`Key` is private, and its `witness()` is explicitly *not* identity (two equal classes keep whichever
witness was inserted first), while `Domain::State` requires `Clone + Eq + Hash + Ord` and
`Domain::key` needs a stable unique string. So `Class` needs `Hash` by key and a `Display` that writes
the key — nothing else — before it can be a solver state.

Everything else falls out of what exists:

- **State.** A wrapper over `Class` plus one `Done` symbol, because the mass the lumped run absorbs at
  `Stage::Desired` has no class, and `Domain::outcomes` must return a state for it.
- **Actions.** The six registered currencies that are lumpable. Annulment is refused by
  `step_action`; parameterised methods (essence, fossil) and unregistered ones (chaos, divine, bench,
  beast) cannot be named at all, because the registry is private and `CurrencyId` is a bare string.
- **Cost.** `apply::cost`'s output verbatim: a currency metadata id and a count of the tradeable
  item. A missing price is already a hard `SolveError::MissingPrice`; an empty cost list is silent and
  must be rejected at build time instead.
- **Exactness.** Exact through `Fraction`, lost at one boundary: the `num/den` to `f64` conversion the
  solver's DP needs. Keep the `Fraction`s beside the `f64`s so a reported number can be traced.
- **Value.** The goal never needs a price (it is absorbing at zero); the finished item's price enters
  as `buy["goal"]`. `sell_value` — what junk is worth — is an owner input.

Owner decisions it cannot default: the base's buy price, `trade_penalty` (zero diverges), the seven
currency prices, junk sell values, the finished item's price, and the goal predicate (`Desired` is one
totem prefix, *not* a finished shield).

One number must be measured before anything is claimed: the reachable class closure. `GraphInput` only
expands pre-listed keys, `MAX_NODES` is 5 000, and the solver's dense linear solve is O(n³) per policy
round across four or five rounds — so the practical budget is roughly 1 000–2 000 nodes, about one
lumped depth's worth (587 classes at depth 1). The first thing any implementation should print is that
count.

## What the priced graph actually measured

The adapter exists (`crates/poe1/src/craft/priced.rs`) and the first number it produced changes the
plan: the reachable class closure of the ilvl-86 Shaper Archdemon Crown is **over 500 000 classes**,
not the ~600 one depth suggests — depth 1 is 314, depth 2 is 685, and a 500 000-node probe was still
growing when stopped. That is inherent to the class, which records per-side *unions* of groups and
tags, so three modifiers on a side multiply out.

So a priced graph is **cut**, not solved: the budget is 1 000 nodes and every unexpanded outcome class
folds into one `Frontier` dead end, because a transition must land somewhere and its chance must stay
on the row. The adapter reports `truncated`, and a report over a cut graph is an upper bound on the
closure's optimum — never a claim about the whole space.

What can be claimed is checkable: 2 347 (class, action) rows and 104 609 transitions whose exact
`Fraction` weights sum to 1 and whose `f64`s agree within 1e-9; a 1 000-node solve in 0.68 s; and an
expected cost that stops moving at three decimals from 400 nodes up. On the crown that is 10.0167
chaos for "transmute once, then alter until the desired suffix" — 140.7 alterations — which is *not*
the owner's real craft, because veil, bench, essence and beast are outside the graph and the goal
predicate fires on any desired class.

The owner inputs remain: currency prices keyed by metadata id, the base's buy price, junk sell values,
the finished item's price under `"goal"`, a positive trade penalty, and the goal predicate.
