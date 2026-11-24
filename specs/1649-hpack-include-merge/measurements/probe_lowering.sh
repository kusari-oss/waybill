#!/usr/bin/env bash
# Spec 1649 T018 checkpoint (FR-009, FR-010): how hpack 0.38.3 renders a
# single-mapping `when:`, names differing only in case, and
# `package:qualifier` entries. Run: bash probe_lowering.sh <scratch-dir>
set -euo pipefail
P=${1:?scratch dir}
rm -rf "$P"; mkdir -p "$P/a" "$P/b"
cat > "$P/a/package.yaml" <<'YAML'
name: waybill-fixture-probe-a
version: 0.1.0
library:
  source-dirs: src
  when:
    condition: os(linux)
    dependencies: [unix]
  dependencies:
    - QuickCheck >= 2
    - quickcheck < 3
    - waybill-fixture-x:sub
    - waybill-fixture-x:other
  build-tools:
    - hspec-discover:hspec-discover
YAML
cat > "$P/b/package.yaml" <<'YAML'
name: waybill-fixture-probe-b
version: 0.1.0
library:
  source-dirs: src
  dependencies:
    - text >= 1
    - text < 3
    - Text
YAML
for d in a b; do
  (cd "$P/$d" && nix run --quiet nixpkgs#hpack -- --force >/dev/null)
  echo "== $d"; sed -n '/^library/,$p' "$P/$d"/*.cabal
done
