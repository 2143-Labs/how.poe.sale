# how.poe.sale

A crafting solver for Path of Exile 1 (3.29, Curse of the Allflame). Paste an item and choose the item you want (a built-in pattern, a pasted item, or a pattern written as JSON), and it ranks every single craft the item allows by what that craft does for the goal, with exact odds where every outcome is computed. It also lists the inputs the item could have been made from one step earlier.

Live at <https://how.john2143.com>. Everything runs in your browser: the engine is Rust compiled to WebAssembly, running in a Web Worker. Your pattern and item are kept in `localStorage` and in the share link; the server only serves files.

## Two directions

The most important feature of how.poe.sale is that it works in both directions on one chart.

1. **Forward: what can this make?** Given an item, or the inputs to a node, apply a currency or other crafting method and show every possible output in the flowchart, each with its chance.
2. **Backward: what could have made this?** Given an item, show the steps that could have produced it.

Forward comes first. Backward work waits until forward is mastered. Both directions grow the same chart: a node is a class of items, so a forward outcome and a backward input that land on the same class are the same node.

## How it works

| Path | What |
|---|---|
| `crates/poe1` | The PoE1 3.29 model: the data bundle, concrete items, patterns, every crafting method as data, `apply`, the one-step solver and its inverse, game text, the generated catalog and the JSON views the browser shows. See `crates/poe1/README.md`. |
| `crates/cli` | Builds `data/bundle.json` from the pinned repoe export and `rules/`, and `data/simple.json` from `games/simple/`; writes `catalog/`, and runs the solver from the command line. |
| `crates/wasm` | The `Bridge` the worker calls. |
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

A chart page can be kept as a test: press Share, save the copied link as `web/src/__tests__/charts/<name>.link`, and `npm test` replays it — the root, watch, pins and merges are all in the link — comparing the cards, the pinned steps and the chain's totals with its snapshot. `npm test -- -u` accepts a new snapshot once the diff has been read.

Container image: `nix build .#container` writes a gzipped image tarball to `./result` (`podman load -i result`). The server reads `PORT` (default 8080) and `STATIC_DIR` (default `./web/dist`).

## Refreshing game data

The data is pinned to the repoe-fork export of 3.29.3.3 (commit `d8635430f4005faccfed69c7957ef2fb9f8fb1fb`). To move to a new export, change the commit and the nine SRI hashes in both `flake.nix` and `scripts/fetch-data.sh` (compute them with `nix hash file --sri <file>`), regenerate the bundle and the catalog, and rerun the tests. Several tests assert exact pool weights, so expect to update them on a new patch.

## Deploy

- Every push to `main` builds `ghcr.io/2143-labs/how-poe-sale:sha-<commit>` and commits that tag to `apps/how-poe-sale.yaml` in [2143-Labs/argo](https://github.com/2143-Labs/argo); Argo CD rolls it out to <https://how.john2143.com>.
- Roll back by reverting that bump commit in the argo repo.
- Every open pull request from this repository gets a preview at `https://how-<PR number>.john2143.com`, built from the PR's head commit. Previews scale to zero, so the first request after a while takes about 30 seconds. Pull requests from forks get no image and no working preview.

## Limits of this version

- One step of lookahead only: the solver ranks the next craft, not a whole route.
- Reforges (Chaos Orb, essences, fossils and the like) are ranked from their certain part until the reforge fill lands.
- Costs are listed, not priced.

## License

MIT, see `LICENSE`. The game data is Grinding Gear Games' and comes from [repoe-fork](https://github.com/repoe-fork/repoe-fork.github.io).
