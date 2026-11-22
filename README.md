# how.poe.sale

A crafting calculator for Path of Exile 1 (3.29, Curse of the Allflame). Pick a base and a goal from the modifiers it can roll, and the page runs the owner's craft loop on an exact ladder of item classes: one row per step with the chance of each outcome, the mass still live, and — on request — the loop priced in chaos. A second panel applies one currency to the item and lists every outcome with its exact chance and the game text it produces.

Live at <https://how.john2143.com>. Everything runs in your browser: the engine is Rust compiled to WebAssembly, running in a Web Worker. Your setup is kept in `localStorage` and in the share link; the server only serves files.

## What the page shows

1. **The loop.** A white base, a goal written as modifiers, and the currencies the loop may use. The depth table is exact: every stage's mass is a `Fraction`, not a sample.
2. **The price.** The same loop as a Markov decision process over a bounded class graph, costed from a price per currency. The closure of a real craft is far past the graph's budget, so the report says where it was cut and the cost is an upper bound.
3. **One currency.** Every outcome of one currency on the item, with its exact chance and the text the game would show.

The reverse direction — what could have made a given item — is not built on this engine yet.

## How it works

| Path | What |
|---|---|
| `crates/poe1` | The item model, the per-currency crafting rules, the goal filter, and the lumped craft loop with its priced graph. `cargo run -p cli -- loop-report --bundle data/bundle.json --fixtures fixtures --pattern totem-shield --base "Supreme Spiked Shield" --item-level 87 --influence shaper --depth 6` prints the loop. |
| `crates/cli` | Builds `data/bundle.json` from the pinned repoe export and `rules/`, and `data/simple.json` from `games/simple/`; prints the loop and pool reports. |
| `crates/simple` | The crafting engine behind the Craft planner page: items, currencies with chances and a cost, methods, crafts wired exit to step with each block reading its own mod filter, and the solver that carries cost and spread through loops. Six bases are compiled in from `crates/simple/data/*.json`; add one by writing a `<name>.spec.json` and running `python3 crates/simple/data/gen_base.py data/raw crates/simple/data/<name>.spec.json > crates/simple/data/<name>.json`, then listing it in `src/bases.rs`. `cargo run -p simple` prints the shield's report (`crates/simple/craft.md`). |
| `crates/wasm` | The `Bridge` the worker calls, and the `CraftBridge` the Craft planner's worker calls. |
| `crates/server` | axum static server for the built front end, plus `/health`. |
| `crates/solver` | Game-agnostic graph solver, kept for the route solver; nothing uses it yet. |
| `web` | React front end. |

## Develop

Everything comes from the flake; there is no global Rust toolchain to install.

```sh
nix develop
bash scripts/fetch-data.sh                                    # pinned repoe files into data/raw/, hash-checked
cargo run -p cli --release -- bundle --raw data/raw --rules rules --out data/bundle.json
cargo run -p cli --release -- bundle --raw games/simple/raw --rules games/simple/rules --claims rules/claims.json --out data/simple.json   # the Simple game's data, from games/simple/
cd web && npm ci && npm run dev                               # builds the wasm, copies both bundles, starts Vite
```

Tests (CI runs the same):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
cd web && npm run build && npm test
```

Container image: `nix build .#container` writes a gzipped image tarball to `./result` (`podman load -i result`). The server reads `PORT` (default 8080) and `STATIC_DIR` (default `./web/dist`).

## Refreshing game data

The data is pinned to the repoe-fork export of 3.29.3.3 (commit `d8635430f4005faccfed69c7957ef2fb9f8fb1fb`). To move to a new export, change the commit and the nine SRI hashes in both `flake.nix` and `scripts/fetch-data.sh` (compute them with `nix hash file --sri <file>`), regenerate the bundle, and rerun the tests. Several tests assert exact pool weights, so expect to update them on a new patch.

## Deploy

- Every push to `main` builds `ghcr.io/2143-labs/how-poe-sale:sha-<commit>` and commits that tag to `apps/how-poe-sale.yaml` in [2143-Labs/argo](https://github.com/2143-Labs/argo); Argo CD rolls it out to <https://how.john2143.com>.
- Roll back by reverting that bump commit in the argo repo.
- Every open pull request from this repository gets a preview at `https://how-<PR number>.john2143.com`, built from the PR's head commit. Previews scale to zero, so the first request after a while takes about 30 seconds. Pull requests from forks get no image and no working preview.

## Limits of this version

- The app shows the priced loop and single-currency outcomes. The reverse direction (what could
  have made an item) is not built on this engine yet.
- Seven currencies are modelled exactly: transmutation, alteration, augmentation, regal, exalted,
  scouring and annulment. Essences, eldritch embers and fracture are not in the loop.
- The priced loop is solved on a *bounded* class graph: the reachable closure of a real craft is
  over 500 000 classes, so the graph cuts at 1 000 and says how many classes it folded away. A
  priced report is therefore an upper bound on the truncated graph's optimum, not a market answer;
  it still needs a price from the owner (the page starts from the 3.30 currency prices).

## License

MIT, see `LICENSE`. The game data is Grinding Gear Games' and comes from [repoe-fork](https://github.com/repoe-fork/repoe-fork.github.io).
