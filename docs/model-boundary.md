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
3. **Metamod bits**: the `Level28` metamod alone cuts the pool from 204 modifiers to 58, and the locks
   decide what a clear can remove.
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
