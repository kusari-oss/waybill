#!/usr/bin/env bash
# Spec 1667 T002: --split source_dir for a Haskell tree.
# usage: split-fixture.sh <waybill-binary> <out-dir>
set -euo pipefail
bin=$1; out=$2; rm -rf "$out"; mkdir -p "$out"
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
r=$t/waybill-fixture-tree
pkg() { mkdir -p "$r/$1"; printf 'name: %s\nversion: 0.1.0\nlibrary:\n  dependencies: [base]\n' "$2" > "$r/$1/package.yaml"; }
pkg . waybill-fixture-root
pkg local-packages/a waybill-fixture-a
pkg local-packages/c waybill-fixture-c
pkg local-packages/d waybill-fixture-d
printf 'cabal-version: 2.4\nname: waybill-fixture-d2\nversion: 0.1.0\nlibrary\n  build-depends: base\n' > "$r/local-packages/d/waybill-fixture-d2.cabal"
for mode in workspace directory; do
  HOME=$t/home WAYBILL_FIXED_TIMESTAMP=2026-01-01T00:00:00Z "$bin" --offline sbom scan --path "$r" --no-deep-hash \
    --format cyclonedx-json --split="$mode" --output-dir "$t/$mode" >"$out/$mode.log" 2>&1 || { echo "scan failed: $mode"; tail -5 "$out/$mode.log"; exit 1; }
  cp "$t/$mode/split-manifest.json" "$out/$mode-split-manifest.json"
  ls "$t/$mode" > "$out/$mode-files.txt"
done
