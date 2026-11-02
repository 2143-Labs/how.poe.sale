# how.poe.sale

A crafting solver for Path of Exile 1 (3.29, Curse of the Allflame). Paste the item you want, and it works backward from it one craft at a time: for each step it shows which crafts can produce it, with exact odds, expected cost, spread, time and quantiles. Every item state can also be bought or sold, so the tree tells you when buying beats crafting.

Live at <https://how.john2143.com>. Everything runs in your browser: the solver is Rust compiled to WebAssembly, running in a Web Worker. Your tree is kept in `localStorage` and in the share link; the server only serves files.

## How it works

| Path | What |
|---|---|
| `crates/solver` | Game-agnostic: the state graph, policy iteration (stochastic shortest path), moments, Monte Carlo quantiles, and backward proposals. |
| `crates/poe1` | PoE1 3.29 rules: data bundle, spawn weights, item and target state, tooltip parser, craft kernels, the `Engine` domain and the JSON API. |
| `crates/datagen` | Builds `data/bundle.json` from the pinned repoe export. |
| `crates/wasm-api` | The `Bridge` the worker calls. |
| `crates/server` | axum static server for the built front end, plus `/health`. |
| `web` | React + React Flow front end. |

## Develop

Everything comes from the flake; there is no global Rust toolchain to install.

```sh
nix develop
bash scripts/fetch-data.sh                                    # pinned repoe files into data/raw/, hash-checked
cargo run -p datagen --release -- --raw data/raw --out data/bundle.json
cd web && npm ci && npm run dev                               # builds the wasm, copies the bundle, starts Vite
```

Tests (CI runs the same):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release                              # includes a 200,000-run Monte Carlo check of the kernels
cd web && npm run build && npm test
```

Container image: `nix build .#container` writes a gzipped image tarball to `./result` (`podman load -i result`). The server reads `PORT` (default 8080) and `STATIC_DIR` (default `./web/dist`).

## Refreshing game data

The data is pinned to the repoe-fork export of 3.29.3.3 (commit `d8635430f4005faccfed69c7957ef2fb9f8fb1fb`). To move to a new export, change the commit and the six SRI hashes in both `flake.nix` and `scripts/fetch-data.sh` (compute them with `nix hash file --sri <file>`), regenerate the bundle, and rerun the tests. Several tests assert exact pool weights, so expect to update them on a new patch.

## Deploy

- Every push to `main` builds `ghcr.io/2143-labs/how-poe-sale:sha-<commit>` and commits that tag to `apps/how-poe-sale.yaml` in [2143-Labs/argo](https://github.com/2143-Labs/argo); Argo CD rolls it out to <https://how.john2143.com>.
- Roll back by reverting that bump commit in the argo repo.
- Every open pull request from this repository gets a preview at `https://how-<PR number>.john2143.com`, built from the PR's head commit. Previews scale to zero, so the first request after a while takes about 30 seconds. Pull requests from forks get no image and no working preview.

## Limits of this version

- Prices are entered by hand. The defaults are a poe.ninja snapshot from 2026-11-02.
- The crafting state is one item. An Awakener's Orb donor is something you declare and price, not something the tree crafts.
- Not modelled yet: imprints, magic-item routes, "Cannot roll Attack/Caster Modifiers", essences, fossils, eldritch currency, veiled mods, fracturing, and the 3.29 bench "reroll" crafts.
- Some mechanics are not documented by GGG. Each one sits behind a single constant, so it can be flipped when evidence turns up:
  - Catalysts: quality raises matching mod weights, and any other currency use removes it (`CATALYST_AFFECTS_WEIGHTS`, `CATALYST_CONSUMED_BY_CURRENCY`). Catalyst magnitude adds to the Simplex's +100% (`CATALYST_STACKING`). 20 catalysts reach full quality (`CATALYSTS_FOR_FULL_QUALITY`).
  - The harvest influenced reforge respects "Cannot Be Changed" locks, and its first new mod comes from the influenced pool (`HARVEST_INFLUENCED_RESPECTS_LOCKS`, `HARVEST_FIRST_DRAW_INFLUENCED`).
  - An Orb of Annulment can remove the lock mod itself, and an Awakener's Orb keeps either mod with equal odds when the two kept mods collide.
- Junk mods are tracked as counts, not individually. Removing their groups from the pool uses a mean-field correction, which is the only approximation in the craft odds; a Monte Carlo test keeps it within 4 standard errors.

## License

MIT, see `LICENSE`. The game data is Grinding Gear Games' and comes from [repoe-fork](https://github.com/repoe-fork/repoe-fork.github.io).
