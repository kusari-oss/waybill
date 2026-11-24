#!/bin/bash
# Spec 1652 FR-001: the six synthetic trees, written under $1.
set -euo pipefail
OUT=${1:?usage: make-fixtures.sh <dir>}
hp() { mkdir -p "$1"; printf 'name: %s\nversion: 0.1.0\nlibrary:\n  source-dirs: src\n  dependencies: [%s]\n' "$2" "$3" > "$1/package.yaml"; }
js='{"name":"waybill-fixture-root-js","version":"1.0.0"}'
# f1: root package.json; nested hpack pair, b -> a. The root neither depends on nor declares them.
F=$OUT/f1-json-root; mkdir -p "$F"; echo "$js" > "$F/package.json"
hp "$F/local-packages/a" waybill-fixture-a "base, text"; hp "$F/local-packages/b" waybill-fixture-b "base, waybill-fixture-a"
# f2: root package.yaml -> a -> c; d is independent.
F=$OUT/f2-hpack-root-chain; hp "$F" waybill-fixture-root "base, waybill-fixture-a"
hp "$F/local-packages/a" waybill-fixture-a "base, waybill-fixture-c"; hp "$F/local-packages/c" waybill-fixture-c "base"; hp "$F/local-packages/d" waybill-fixture-d "base"
# f3: root cabal.project listing local-packages/*, no root package; tools/e is unlisted.
F=$OUT/f3-cabal-project; mkdir -p "$F"; printf 'packages: local-packages/*\n' > "$F/cabal.project"
hp "$F/local-packages/a" waybill-fixture-a "base, text"; hp "$F/local-packages/b" waybill-fixture-b "base, waybill-fixture-a"; hp "$F/tools/e" waybill-fixture-e "base"
# f4: f3 plus a root package.json.
cp -R "$OUT/f3-cabal-project" "$OUT/f4-cabal-project-json"; echo "$js" > "$OUT/f4-cabal-project-json/package.json"
# f5: root stack.yaml listing two packages; tools/e is unlisted.
F=$OUT/f5-stack; mkdir -p "$F"; printf 'resolver: lts-22.0\npackages:\n- local-packages/a\n- local-packages/b\n' > "$F/stack.yaml"
hp "$F/local-packages/a" waybill-fixture-a "base, text"; hp "$F/local-packages/b" waybill-fixture-b "base, waybill-fixture-a"; hp "$F/tools/e" waybill-fixture-e "base"
# f6: npm root depending on a nested npm package by path.
F=$OUT/f6-npm-path; mkdir -p "$F/packages/lib"
echo '{"name":"waybill-fixture-root-js","version":"1.0.0","dependencies":{"waybill-fixture-lib":"file:packages/lib"}}' > "$F/package.json"
echo '{"name":"waybill-fixture-lib","version":"0.2.0"}' > "$F/packages/lib/package.json"
