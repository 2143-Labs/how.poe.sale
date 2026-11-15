# Context: crown-bidirectional-audit

> Saved 2026-11-11 from branch `main` (repo `how.poe.sale`) by the agent session that shipped PR #18 and then audited the Archdemon Crown craft in both directions.

## Goal

Two things happened in this session, and the second one is where the work stands:

1. **Shipped and deployed** the item-reader fix: a real PoE tooltip's `Synthesised <Base>` type line must set the synthesised flag, a paste with no `Item Level:` line must read its modifiers and warn, and Heist enchantments must survive the bundle build. Merged as PR #18 (`cebc014`), deployed, verified on prod.
2. **Audited the Archdemon Crown craft in both directions** — 14 item states × one forward probe (`cli solve`) and one backward probe (`cli sources`) — to find what the model is missing. That audit is complete and produced 8 confirmed defects plus several modelling gaps. **Nothing has been fixed yet.**

## Current State

- **Branch**: `main`
- **Modified files**: 0 tracked (`git status --short` shows only `?? docs/`, which is the owner's untracked directory — never add, edit or commit it)
- **Last commit**: `cebc014 Merge pull request #18 from 2143-Labs/synth-paste-reader`
- **Worktree**: none outstanding (`git worktree list` shows only `/home/john/repos/how.poe.sale`)
- **Prod**: `ghcr.io/2143-labs/how-poe-sale:sha-cebc01419e637365ee862234b260fa74dcff795f`, rolled out and verified (`/`, `/simple/`, `/poe1/` = 200; `/data/bundle.json` 9,143,126 B with 15,630 mods, including 256 Heist enchantments as `kind: enchant`)
- **CI**: the pipeline's own `deploy` job bumps `apps/how-poe-sale.yaml` in `2143-Labs/argo`; nothing manual is needed to deploy a push to `main`

## What shipped this session (PR #18, three commits)

| Commit | Change |
|---|---|
| `1b04592` `fix(text)` | `Synthesised ` type-line prefix sets `item.synthesised` (`crates/poe1/src/text.rs`, header scan beside the `Rarity:` loop); the item-level section is now located in a **pre-pass** (`sections.iter().enumerate().find_map(...)` over `classify`), so a paste with no `Item Level:` line reads its modifier sections instead of silently dropping all of them, and warns `no "Item Level:" line; read as item level 1` |
| `6614795` `fix(bundle)` | `WeaponEnchantmentHeist*` / `ArmourEnchantmentHeist*` (256 mods) keep `ModKind::Enchant` in `crates/cli/src/bundle.rs` `mod_kind`; they enter no affix pool, only the enchantment pool |
| `17305a4` `test(text)` | `fixtures/pastes/synthesised-bow.txt` (the owner's real tooltip, verbatim) plus 3 tests: `parse_synthesised_bow_from_a_real_tooltip`, `real_tooltip_over_fills_with_the_magnified_accuracy`, `parse_without_item_level_warns` |

Verified: `cargo test --workspace --release` (19 text tests), clippy `-D warnings`, fmt, both bundles rebuilt, and the shipped binary reading the deployed bundle through a port-forward.

### Two implementation lessons worth keeping
- The naive fix for the missing-level case (`level_section.is_none_or` → `is_some_and` in the section loop) **broke `parse_jade_flask`**: the old forward scan skipped every section until it *saw* the level line, which is what kept a utility flask's base effect (printed above the level line) out of the pools. The pre-pass is required; the forward-scan change alone is a regression.
- The plan's Step 3 (a synthesised tie-break clause in the single-block heuristic) was **dropped after measurement**: it never changes an outcome in any reachable composition, because the pre-existing `as_implicit > as_explicit` clause already decides them. Shipping it would have been idle code.

## The audit: what the model is missing

Method: for each of 14 states, one forward probe (`cli solve`) and one backward probe (`cli sources`), each checked against `apply`/`invert`/`solve`/`pattern` semantics and against the game rules. Reproduce any row with:

```sh
cd ~/repos/how.poe.sale
./target/release/cli solve   --bundle data/bundle.json --fixtures fixtures --pattern crown-shaper --item <state>
./target/release/cli sources --bundle data/bundle.json --fixtures fixtures --pattern crown-shaper --item <state>
# dual-influence goal: --pattern crown-shaper-elder --item crown-shaper-elder-ideal
```

### States probed
`crown-a`, `crown-a-mana`, `crown-actual`, `crown-c1`, `crown-c2`, `crown-c2-blocked`, `crown-d2-scbc`, `crown-d2-scbc-fractured`, `crown-ideal`, `crown-l1`, `crown-l2`, `crown-shaper-elder-ideal` (under `crown-shaper-elder`), plus negative controls `crown-a-corrupted` + `crown-actual-corrupted` + `crown-d2-scbc-corrupted`, and `crown-uninfluenced-fracture`.

### Confirmed defects (none fixed)

| # | Defect | Evidence | Severity for this craft |
|---|---|---|---|
| D1 | **Beast "Shaper slam" is modelled as a guaranteed Shaper mod.** `beast/influence-slam-shaper` draws `Pool::Influence{shaper}` — identical odds to `currency/shapers-exalted` (0.1429 vs 0.1429 in every backward group). Maxroll — the page the method cites as `beast.list` — says an influence slam "is functionally the same as an Exalted Orb. It does not guarantee an influenced modifier." | `rules/methods.json` (beast arm), `catalog/methods.md`, https://maxroll.gg/poe/crafting/beastcrafting | Highest: ~9× optimistic (0.1429 modelled vs 0.0165 for a plain Exalted Orb) on the step feeding the real crown. **Owner must rule** — the model's claim `add.side-first` is owner-sourced. |
| D2 | **Ranking tie-break: `rank()` compares raw floats while `tied()`/`stands()` use a 1e-9 tolerance,** so float noise in Goal outranks a full 1.000 difference in Blocked. On `crown-l1` the Farric slam (Blocked 1.000, fills the last prefix slot) is ranked **above** Divine (Blocked 0.000) and is named "Best exact step". | `crates/poe1/src/solve.rs:402-410` vs `:34`, `:388-397` | High: the advisory's top recommendation on a live state is the wrong move |
| D3 | **The backward pass never considers 2,086 mods as the removed mod.** `eligible()` skips any mod whose domain isn't the base's `item` domain (except Ducat), dropping 1,601 crafted, 258 unveiled, 177 delve, 22 veiled, 28 mercenary — while its own doc comment says they are included. Consequence: no preimage can start from an item that *had* the veiled gem-level prefix and lost it. | `crates/poe1/src/invert.rs:442-471` (filter), doc comment at `:441` | High: severs the backward view exactly at the veiled prefix |
| D4 | **Impossible preimages: magic items with 5 affixes before a Regal.** `undo_rarity` only flips the rarity and `check` re-runs the method without validating the item, so `crown-actual`, `crown-l2`, `crown-d2-scbc`, `crown-d2-scbc-fractured` all list a magic preimage with 5 explicits (magic holds ≤2). | `crates/poe1/src/invert.rs:558-578`; poedb Regal Orb ("a magic item only has space for two affixes") | Medium: wrong history shown |
| D5 | **Mirror of Kalandra offered on corrupted items.** Requires only `notMirrored` + `notUnique`. Poedb's wiki: mirroring needs "non-unique equipment, non-corrupted item, or map". | `rules/methods.json` `currency/mirror`; https://poedb.tw/us/Mirror_of_Kalandra | Medium: offers an illegal step on every corrupted state |
| D6 | **`bench/remove-crafted` removes a fractured crafted mod.** The op retains only `kind != Crafted`, ignoring `fractured`. 3.11.1 notes "Fixed a bug where using the crafting bench could overwrite Fractured modifiers"; GGG staff: "A Fractured mod cannot be modified (this includes removing it)". | `crates/poe1/src/apply.rs:1431-1441`; https://www.pathofexile.com/forum/view-thread/2893980 ; thread 2894041 | Low now (no crown fixture exercises it) but a real correctness bug |
| D7 | **`blocked()` ignores the one-crafted-mod limit.** It tests only side fullness, so on `crown-c2` — where the SCBC craft consumes the only crafted slot — removing that craft is scored `sets back` (Goal 0.500, Worse 1.000, **Blocked 0.000**) instead of `unblocks`. | `crates/poe1/src/solve.rs:100-106`; row 6 of `solve ... --item crown-c2` | Medium: the tool cannot see the crafted slot as blocked |
| D8 | **Test gap: `pairs()` only keeps fixtures the pattern accepts,** so corrupted and uninfluenced fixtures never get their backward sources forward-verified. | `crates/poe1/tests/common/mod.rs:47-70` | Low-medium: silent coverage hole |

### Modelling gaps (by design or not, they cost the owner visibility)

- **Un-invertible ops** (`crates/poe1/src/invert.rs:279-300`): `Clear`, `Scour`, `Unveil`, `RemoveCrafted`, `SetEnchant*`, `ClearEnchants`, `AddInfluences`, `TakeInput`, `VaalImplicit`, `EldritchImplicit`, `Dominance`, `Conflict`, `ChangeResistance`, `Imprint`, `Restore`, `LinkAll`, `Mirror`, `Destroy`, `RemoveIntangibility`, `Fill`, `Sockets`, `Unmodelled` (298 of 1,084 methods cannot be walked back). Consequences for this craft: the **essence reforge** (clear+fill), **veiled chaos** (fill), **Orb of Dominance**, and the **unveil** are all invisible backwards — so `crown-actual` and `crown-ideal` can never show "veiled-exalt → unveil → …". Once a veiled mod is revealed, the tool cannot see that it was unveiled.
- **Two-input methods are excluded from both directions** (`solve.rs:458`, `invert.rs:71` filter `inputs == 1`), so `currency/awakeners-orb` never appears — the only route to the dual-influence `crown-shaper-elder-ideal` is invisible. `harvest/randomise-influence` is `unmodelled: multiDraw` and likewise skipped. (This is card #60 territory.)
- **Display caps hide rows without saying so** (`crates/poe1/src/catalog.rs`): step tables ≤6 rows (`:1666`, `:1682`), backward `take(4)` groups (`~:1604`) and `take(2)`/`take(3)` methods per group (`:1588`, `:1606`), "does not apply" shows 3 reasons of hundreds, "not computed" shows 8 names (`:1692`), and `sources_md` dedupes identical text (`:1588`) so the removed mod is not named. **A missing row in the CLI output is not evidence of absence** — absence claims must come from the code.
- **Vaal's backward odds are understated**: 0.25 is the surviving "nothing else" branch; branches 2–4 are impossible for these targets, so the true value is ≥0.5. Note reads "3 of 4 branches cannot be undone".
- **Bench preimages report P(added), not P(this value)**: `bench/add/EinharMasterIncreasedLife4` is 1.0000 although 65 is one of 56–70 (note says so). `bench/add/EinharMasterIncreasedMana2` is the same shape (40 of 35–44).
- **Unrevealed `VeiledPrefix` scores 0** in the pattern (matchers are `JunMasterVeiled*` ids only), so a pending veil counts as a missing slot until unveil — correct, but it means `blocked` reads 1.000 while the item is one unveil from the goal.

### Plan-level finding (the craft itself)
- **A Shaper crown with a fractured modifier cannot be made in 3.29.** The Fracturing Orb refuses influenced items ("Cannot be used on Influenced or Fractured items", poedb; 3.29.0 notes), and all six influence orbs refuse fractured items (poedb wiki line: "cannot be used on … fractured items"; Craft of Exile `cannot.fractured`). So `crown-d2-scbc-fractured` is a **legacy-only** state (3.24 Necropolis grave crafting, removed from core in 3.25) and the "fracture life on an uninfluenced crown, then add Shaper" branch is a dead end in game. The model's `noInfluence`/`noFractured` gates are correct. **Recommendation: treat Shaper+fractured as unreachable for new crafts, but keep the paste reader accepting legacy items.**
- **The pattern's Maven tiers can never roll**: `IncreasedChillEffectSupportedUberMaven` (L25 Hypothermia), `AreaOfEffectSupportedUberMaven`, `SocketedSpellCriticalStrikeChanceUberMaven`, `AreaDamageSupportedUberMaven` all have spawn weight 0 and levels 90–94. Only `currency/orb-of-dominance` (or an Awakener's Orb carrying one over) can supply them — and Dominance is un-invertible. The owner's "Level 22 [elevated] Hypothermia" therefore has no modelled route; the data's elevated tier is Level 25. **Open question for the owner.**

### Correct, verified working (do not "fix" these)
- Annulment odds everywhere (1/removable with locks respected): crown-a 0.25, crown-l1 1/3, crown-c2 0.5, crown-actual 1/6, crown-d2-scbc 1/3, fractured 0.5.
- `currency/veiled-exalted` at 0.2 (5 removable) and 0.5 (2 removable, SCBC locking the suffix side); it is invertible and does appear backwards from an unrevealed placeholder.
- `beast/suffixes-cannot-be-changed` and its bench twin at p = 1.0.
- Shaper-pool odds: suffix slam 0.0404 = ½ × 800/9900; prefix slam 0.1429 = 800/5600 (11 prefixes: 3 at 800 + 8 at 400) — the pool arithmetic is right *for the pool the model uses* (D1 is about whether that pool is right).
- Unveil: 56 outcomes on `crown-c2-blocked` (C(8,3)) and 84 on `crown-c2` (C(9,3)); the crown-c1 "0.838 goal" decomposition reproduces (0.685 × 11/12 + 0.315 × 8/12 = 0.838).
- The two-line Zombies/Skeletons bench craft is counted as **one** prefix (`crates/poe1/src/apply.rs:1063-1065`, `item.rs:771-774`).
- Fractured life is protected from annul, scour and divine; the fractured item's steps are consistent.
- Corrupted items refuse all but `tainted-*` and socket/colour bench steps; scores are 0 because the pattern has `corrupted: false`.
- Uninfluenced crown: `beast/influence-slam-shaper` is correctly refused (needs `hasInfluence`); `currency/shapers-exalted` is correctly offered; no Shaper-only credit is given to an uninfluenced item.
- Farric (`beast/suffix-to-prefix`) under SCBC: removes nothing (suffixes locked) and adds one **natural** prefix — matches maxroll ("no Suffix is removed but a Prefix is still added"), and it cannot produce a veiled mod.

## Key Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Missing `Item Level:` → **read the modifiers anyway at level 1 and warn**, not refuse | The old behaviour dropped every modifier silently; refusing would have made the tool useless for a common copy-paste. Item level does not gate reforge fills the way it gates `apply` (`apply.rs:1777`) — the reader's pools never consult it, so reading at 1 changes nothing except the recorded level |
| 2 | Locate the level section in a **pre-pass**, not during the loop | Keeps the "everything before the level line is base, not modifiers" rule that `parse_jade_flask` depends on |
| 3 | Warning text is `no "Item Level:" line; read as item level 1`, with no claim about matching | Item level gates nothing in the reader, so any stronger wording would be false |
| 4 | Unreadable-but-faithful pastes live in `fixtures/pastes/`, not `fixtures/text/` | `fixtures/text/*.txt` is the web example set: `crates/wasm/src/lib.rs` `every_text_fixture_is_an_example` requires every file there to be a readable `EXAMPLES` entry |
| 5 | Ship the bundle change (Heist enchantments) with the reader change | Without it the enchantment line can never be read, so the "Synthesised item reads" claim would be false |
| 6 | Fixture + tests keep the *real* tooltip, and the residual is **pinned** by a test that asserts the refusal | The enchant-boosted lines are a separate feature; pinning the current behaviour is a tripwire for when it lands |
| 7 | Audit shape: 14 states × 2 directions, one agent each | The product's whole premise is two directions; verifying only forward would miss the invert-side defects (D3–D5) |

## Ruled Out

| Approach | Why rejected |
|----------|-------------|
| Making the real paste parse fully | `278% increased Physical Damage`, `Adds 50 to 88`, `+207 to Accuracy Rating` are the **Heist-enchant-boosted** prints of `179 + 78`, `46.3–81.5`, `191.7`; no tier matches the first two, and `+207` matches the natural bow accuracy suffix (`LocalIncreasedAccuracyNew2`, 131–215), giving a 4th suffix → `too many suffixes for a rare Bow`. Fixing it needs a magnitude-boost model, explicitly out of the plan's scope |
| Modelling the enchant's explicit-magnitude boost now | Separate feature; would need per-stat "is physical" knowledge and the game's display rounding |
| Reading the Gilded Fossil line (`Item sells for much more to vendors`) | `mod_kind` drops the `delve`/`unique` family and `implicit_pool` accepts only synthesis implicits; it stays a visible warning on purpose |
| Adding a synthesised tie-break clause to the single-block heuristic (plan Step 3) | Measured unreachable: probed 4 single-block compositions, the existing `as_implicit > as_explicit` clause decides all of them; would have been untested idle code |
| Changing `level_section` handling in the loop only (`is_none_or` → `is_some_and`) | Broke `parse_jade_flask` — the old forward scan is what excluded a flask's base effect from the pools |
| Keeping the bow paste in `fixtures/text/` | Breaks `every_text_fixture_is_an_example` (the item cannot be read, so it cannot be a web example) |
| A whole-route planner for this craft | The engine is one step of lookahead by design (`solve.rs:21-24`); the audit assumed per-state advice, not a route |

## Open Questions

- [ ] **D1: is the beast influence slam a Shaper-pool draw or an Exalted-like roll?** The model guarantees a Shaper mod (0.1429); maxroll says it "does not guarantee an influenced modifier" (≈0.0165). The claim `add.side-first` is owner-sourced, so this needs the owner's ruling before any change.
- [ ] **Hypothermia level:** the owner says "Supported by Level 22 [elevated]"; the data's elevated tier `IncreasedChillEffectSupportedUberMaven` is **Level 25** (and level 90, weight 0). Which is right? (Also recorded as a `notes` string in `fixtures/patterns/crown-shaper.json`.)
- [ ] Which defects to fix, in what order? Suggested: D2 (ranking), D3 (eligible domains), D4 (rarity undo), D5 (mirror), D7 (crafted cap), D6, D8 — D1 after the owner rules.
- [ ] Should `blocked()` also treat "the crafted slot is taken by a metamod" as blocked (D7), or should the crafted class be dropped when a metamod occupies the slot?
- [ ] Should the pattern's Maven matchers stay, given they can only arrive via Dominance (un-invertible) and the Awakener's Orb (unsupported)?
- [ ] Do you want Mirror refused on corrupted items (D5) even though the in-game text only says "non-unique"?

## Recent Artifacts

| Path | Description | Last Modified |
|------|-------------|---------------|
| `fixtures/pastes/synthesised-bow.txt` | The owner's real Archdemon-style Synthesised bow tooltip, verbatim (new dir: pastes that must not become web examples) | 2026-11-11 |
| `crates/poe1/src/text.rs` | Reader: type-line synthesised flag, level-section pre-pass, missing-level warning | 2026-11-11 |
| `crates/cli/src/bundle.rs` | `mod_kind`: Heist weapon/armour enchantments → `ModKind::Enchant` | 2026-11-11 |
| `crates/poe1/tests/text.rs` | 3 new tests, all green | 2026-11-11 |
| `catalog/next-steps.md` | Generated per-state forward/backward report — the primary audit source (line numbers cited above) | 2026-11-11 |
| `catalog/methods.md`, `catalog/questions.md`, `catalog/items.md`, `catalog/bench.md` | Generated method/mod/fixture documentation | 2026-11-11 |
| `data/bundle.json` | Main bundle, 15,630 mods / 1,084 methods (gitignored, regenerate) | 2026-11-11 |
| `data/simple.json` | Simple game bundle, 88,443 B, unchanged by this session's work | 2026-11-11 |
| `fixtures/patterns/crown-shaper.json`, `crown-shaper-elder.json` | The crown goals (classes, slot groups, the Hypothermia note) | unchanged |

## Constraints

- Repo: `~/repos/how.poe.sale`. Start every shell with `set +e`; cargo only through `nix develop -c cargo ...`; never set `CARGO_TARGET_DIR`.
- Work in a worktree under `~/repos/worktrees/how.poe.sale-<slug>` (never inside the checkout, never `~/worktrees`), based on `origin/main`.
- **`docs/` is the owner's untracked directory** (including `docs/decisions.md`): never add, edit, commit or delete it.
- One conventional commit per step; no attribution lines; never `--no-verify`.
- Gate before push: rebuild both bundles, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -q --workspace --release`, `cd web && npm run build && npm test` (the web build needs `npm ci`; no web source changed this session), forbidden-token grep over tracked files.
- **Forbidden-token grep over tracked files must print nothing.** The five tokens are deliberately not restated here: this file is tracked, and writing them down would itself trip the check. Read the list from the project guardrails, not from this document. **One rule check this session cited a GitHub URL from a forbidden repo; that citation was discarded and the conclusion re-derived from poedb / patch notes / Craft of Exile.** Cite public sources only: poedb, pathofexile.com, craftofexile, maxroll, vhpg, repoe-fork, 2143-Labs repos.
- Never bypass Anubis (poewiki.net, how.poe.sale, how.john2143.com); `pathofexile.fandom.com` 403s to curl (use web_search snippets).
- Read prod through `kubectl --context closet-as-admin -n how-poe-sale port-forward svc/how-poe-sale <port>:80`.
- **No prices anywhere** — valuation is expected tries, not currency.
- The owner is the game-mechanics authority; the AI proofreads. Forward first, one shared chart, and never refer to an item state by label alone (show the full item, the step and the full result).
- Board (optional): ai_tasks project 24, `format: "markdown"`, read `res["details"]["structuredContent"]`. No card currently tracks this audit.
- The `deploy` job in `.github/workflows/build.yml` bumps Argo on every push to `main`; merging is deploying.

## Next Steps

1. [ ] **Pick up here**: fix **D2** — make `Assessment::rank` (`crates/poe1/src/solve.rs:402-410`) compare with the same `EPS` tolerance `tied()` uses, so float noise in Goal cannot outrank a 1.000 Blocked difference. Add a test that reproduces the `crown-l1` case (Farric must not be "Best exact step" above Divine). Regenerate `catalog/` and expect `catalog/next-steps.md` row order to change.
2. [ ] Fix **D3**: `eligible()` (`crates/poe1/src/invert.rs:442-471`) should admit the domains it documents (`crafted`, `unveiled`, `veiled`, `delve`, `mercenary`) the way `item.rs` validate does (source-specific domains carry no item domain of their own), then check the crown-c2 preimages show a `desired-veiled` input at 0.5 alongside the existing ones.
3. [ ] Fix **D4**: validate the item in `undo_rarity` (or in `undo`'s output filter) so a magic preimage with >1 prefix or >1 suffix is dropped; the four affected states must stop listing a 5-affix magic item.
4. [ ] Fix **D5**: add `notCorrupted` to `currency/mirror` (poedb wiki) and re-check the corrupted negative controls.
5. [ ] Fix **D7**: decide and implement the crafted-slot rule in `blocked()`; on `crown-c2`, removing SCBC should become `unblocks` (or the crafted class should stop being counted when a metamod holds the slot).
6. [ ] Fix **D6**: `Op::RemoveCrafted` must not remove a `fractured` affix; add a test with a fractured crafted mod.
7. [ ] Fix **D8**: extend the backward-verification test beyond `pairs()` so corrupted and uninfluenced fixtures are covered.
8. [ ] Take **D1** to the owner (quote both sources); then either keep the model or switch the beast slam to the natural pool.
9. [ ] Resolve the Hypothermia level question (L22 owner vs L25 data) and update `fixtures/patterns/crown-shaper.json` accordingly.
10. [ ] Afterwards, return to the queued cards: #58 (unveil in the forward chart), #57 (tier on the picker; family filter), #46 (examples menu), README intro/Limits (stale), #42 (route solver), #59 (fossils), #60 (two-input).

## Verification

- **This session's shipped work**: `git log --oneline -1` shows `cebc014`; prod serves `sha-cebc014…`; the deployed `/data/bundle.json` contains 256 Heist enchantments with `kind: enchant`; the shipped binary reading that bundle parses the bow paste to 3 synthesis implicits + 1 enchantment + 3 suffixes, with exactly three `not understood` warnings.
- **The audit**: every defect above is reproducible with the `cli solve` / `cli sources` commands in this file; the "correct" list is the set that must not change.
- **Future fixes**: each defect gets a test that fails before the change and passes after; the full gate (both bundles, fmt, clippy, workspace tests) must be green before any commit; `catalog/` regenerated so the served reports match the code.

## Missing

- The 28 probe subagents had no shell tool, so they could not run the CLI themselves. I ran all 28 commands and fed each agent its real output; every defect claim above was then re-verified against the code or a CLI run by me. Four agent claims were checked and discarded ("0.1429 does not reproduce" — it does, 800/5600; "crown-l2 shows an unchanged annul leaf" — the three leaves are the three removals; "crown-l1 has 58 outcomes summing to 0.89" — 57 summing to 1.0; "corrupted states list bench/essence producers" — they do not).
- No web build was run this session (no `node_modules`); no web or wasm source changed.
- No board card tracks this audit.
