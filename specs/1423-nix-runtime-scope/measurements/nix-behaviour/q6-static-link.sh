#!/usr/bin/env bash
# Q6: which derivation fields lead from a runtime member to the Haskell libraries linked
# into a static executable. Inputs per target:
#   - runtime closure     : nix path-info -r --store https://cache.nixos.org <outPath>
#   - derivation closure  : nix derivation show -r <drvPath>
#   - truth (cabal)       : cabal-closure.nix over getCabalDeps (library deps, transitively)
# then q6-rule.py scores candidate field rules against the truth.
set -u
cd "$(dirname "$0")"
REV=a799d3e3886da994fa307f817a6bc705ae538eeb
R=github:NixOS/nixpkgs/$REV
o=q6-out; mkdir -p "$o"
# top-level attr : haskellPackages attr holding its cabal metadata
# hadolint and ormolu added because their executables have test-only Haskell deps (hspec, HUnit, ...)
for pair in shellcheck:ShellCheck pandoc:pandoc-cli haskellPackages.hlint:hlint hadolint:hadolint ormolu:ormolu; do
  top=${pair%%:*}; hp=${pair##*:}
  out=$(nix eval --raw "$R#$top.outPath"); drv=$(nix eval --raw "$R#$top.drvPath")
  nix path-info -r --store https://cache.nixos.org "$out" > "$o/$top.rt"
  [ -s "$o/$top.drv-closure.json" ] || nix derivation show -r "$drv" > "$o/$top.drv-closure.json"
  [ -s "$o/cabal-closure.$hp.json" ] || nix eval --json --impure --expr "import ./cabal-closure.nix { rev = \"$REV\"; attr = [ \"haskellPackages\" \"$hp\" ]; }" > "$o/cabal-closure.$hp.json"
  python3 -I -B q6-rule.py "$o/$top.drv-closure.json" "$o/$top.rt" "$o/cabal-closure.$hp.json" "$drv" > "$o/$top.rule-score.txt"
done
