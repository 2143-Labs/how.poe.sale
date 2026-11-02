#!/usr/bin/env bash
# Downloads the pinned repoe-fork export (3.29.3.3) into data/raw/ and checks each file's SRI hash.
# Keep the commit and hashes in sync with flake.nix.
set -euo pipefail
cd "$(dirname "$0")/.."

COMMIT=d8635430f4005faccfed69c7957ef2fb9f8fb1fb
BASE="https://raw.githubusercontent.com/repoe-fork/repoe-fork.github.io/$COMMIT/data"

sri() {
  echo "sha256-$(python3 -c 'import sys,hashlib,base64;print(base64.b64encode(hashlib.sha256(open(sys.argv[1],"rb").read()).digest()).decode())' "$1")"
}

mkdir -p data/raw
while read -r file hash; do
  dest="data/raw/$file"
  if [ -f "$dest" ] && [ "$(sri "$dest")" = "$hash" ]; then
    continue
  fi
  curl -fsSL "$BASE/$file" -o "$dest.tmp"
  got=$(sri "$dest.tmp")
  if [ "$got" != "$hash" ]; then
    rm -f "$dest.tmp"
    echo "hash mismatch for $file: expected $hash, got $got" >&2
    exit 1
  fi
  mv "$dest.tmp" "$dest"
done <<'EOF'
mods.json sha256-3vu6MbYPkZEIDHgZcngFf4NPf5J8GioICnym7T6mhrU=
base_items.json sha256-SK+wel9A61FVL5DBfVMYZGnbKCoudZ2re0bn+dKg9Ys=
item_classes.json sha256-Z4+jumU8J1DDjduk7tLA99bF7AQLK7wHvFAv7WnF5jI=
stat_translations.json sha256-PmIQZ4Lu6e5t1o8Ed65TdSIgCXACCLmgsTQ1ooLTvvo=
crafting_bench_options.json sha256-D+UasWV86mnFM7sGEOOT8QQBFWFA3ojt9oYSBIPNKEU=
essences.json sha256-AcatVYZt9nPdJamwhXfuIoAq07jwZxsBnLTPUjub1wA=
fossils.json sha256-4XHBGfh0tPK+Kpg2avYcWiCRyib1CLL1uda1Ubud8YE=
tags.json sha256-Jd+ROr1Fm+3T3BtknjRLNT4nHtCjIWLxFtZgNKMmm1A=
EOF
