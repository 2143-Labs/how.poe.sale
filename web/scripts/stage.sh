#!/usr/bin/env bash
# Puts the wasm-bindgen package in src/wasm/pkg and the data bundles in public/data.
# Nix sets WASM_PKG, BUNDLE and SIMPLE_BUNDLE; locally the wasm is built here and the bundles come from ../data.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p src/wasm/pkg public/data
chmod -R u+w src/wasm/pkg 2>/dev/null || true
rm -rf src/wasm/pkg/*
if [ -n "${WASM_PKG:-}" ]; then
  cp -r "$WASM_PKG"/. src/wasm/pkg/ && chmod -R u+w src/wasm/pkg
else
  (cd .. && cargo build -p wasm --release --target wasm32-unknown-unknown \
    && wasm-bindgen --target web --out-name solver --out-dir web/src/wasm/pkg "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/wasm.wasm")
fi
if [ -n "${BUNDLE:-}" ]; then cp "$BUNDLE" public/data/bundle.json
elif [ -f ../data/bundle.json ]; then cp ../data/bundle.json public/data/bundle.json
else echo "missing data/bundle.json: run scripts/fetch-data.sh, then cargo run -p cli --release -- bundle --raw data/raw --rules rules --out data/bundle.json" >&2; exit 1; fi
if [ -n "${SIMPLE_BUNDLE:-}" ]; then cp "$SIMPLE_BUNDLE" public/data/simple.json
elif [ -f ../data/simple.json ]; then cp ../data/simple.json public/data/simple.json
else echo "missing data/simple.json: run cargo run -p cli --release -- bundle --raw games/simple/raw --rules games/simple/rules --claims rules/claims.json --out data/simple.json" >&2; exit 1; fi
