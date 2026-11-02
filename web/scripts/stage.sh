#!/usr/bin/env bash
# Puts the wasm-bindgen package in src/wasm/pkg and the data bundle in public/data.
# Nix sets WASM_PKG and BUNDLE; locally the wasm is built here and the bundle comes from ../data.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p src/wasm/pkg public/data
chmod -R u+w src/wasm/pkg 2>/dev/null || true
rm -rf src/wasm/pkg/*
if [ -n "${WASM_PKG:-}" ]; then
  cp -r "$WASM_PKG"/. src/wasm/pkg/ && chmod -R u+w src/wasm/pkg
else
  (cd .. && cargo build -p wasm-api --release --target wasm32-unknown-unknown)
  wasm-bindgen --target web --out-name solver --out-dir src/wasm/pkg ../target/wasm32-unknown-unknown/release/wasm_api.wasm
fi
if [ -n "${BUNDLE:-}" ]; then cp "$BUNDLE" public/data/bundle.json
elif [ -f ../data/bundle.json ]; then cp ../data/bundle.json public/data/bundle.json
else echo "missing data/bundle.json: run scripts/fetch-data.sh, then cargo run -p datagen --release -- --raw data/raw --out data/bundle.json" >&2; exit 1; fi
